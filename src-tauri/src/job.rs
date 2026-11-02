//! What a download needs to run and to resume. Stored with each download in downloads.json.

use serde::{Deserialize, Serialize};

use crate::model::{ByteProgress, Download, DownloadStatus, ProgressDetail};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SourceKind {
    /// One file over HTTP(S). Downloaded in parallel byte ranges when the server allows it.
    Http,
    /// An HLS media playlist. Segments download in parallel, then ffmpeg builds the file.
    Hls,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaSource {
    pub kind: SourceKind,
    pub url: String,
    /// Headers the source requires, for example from yt-dlp.
    pub headers: Vec<(String, String)>,
    pub label: String,
    pub size_hint: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobSpec {
    pub primary: MediaSource,
    /// Set when video and audio download separately. ffmpeg merges them at the end.
    pub audio: Option<MediaSource>,
    pub duration: Option<f64>,
    pub encrypted: bool,
}

impl JobSpec {
    pub fn needs_ffmpeg(&self) -> bool {
        self.audio.is_some() || self.primary.kind == SourceKind::Hls
    }

    pub fn sources(&self) -> Vec<&MediaSource> {
        std::iter::once(&self.primary)
            .chain(self.audio.as_ref())
            .collect()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Chunk {
    pub start: u64,
    /// Inclusive. None when the size is unknown.
    pub end: Option<u64>,
    pub done: u64,
}

impl Chunk {
    pub fn len(&self) -> Option<u64> {
        self.end.map(|e| e + 1 - self.start)
    }

    pub fn complete(&self) -> bool {
        self.len().is_some_and(|l| self.done >= l)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum PartState {
    Http {
        total: Option<u64>,
        ranged: bool,
        /// ETag or Last-Modified of the file. A change means the file on the server changed.
        validator: Option<String>,
        chunks: Vec<Chunk>,
    },
    Hls {
        /// '0' waiting, '2' done. In-flight segments are not stored.
        states: String,
        bytes: u64,
        #[serde(skip)]
        in_flight: Vec<u32>,
        encrypted: bool,
    },
}

impl PartState {
    pub fn done_bytes(&self) -> u64 {
        match self {
            PartState::Http { chunks, .. } => chunks.iter().map(|c| c.done).sum(),
            PartState::Hls { bytes, .. } => *bytes,
        }
    }

    /// Known total, or an estimate from the finished HLS segments.
    pub fn total_bytes(&self, hint: Option<u64>) -> (Option<u64>, bool) {
        match self {
            PartState::Http { total, .. } => (total.or(hint), total.is_none() && hint.is_some()),
            PartState::Hls { states, bytes, .. } => {
                let done = states.bytes().filter(|&b| b == b'2').count() as u64;
                let total = states.len() as u64;
                if done == total && total > 0 {
                    (Some(*bytes), false)
                } else {
                    match bytes.checked_div(done) {
                        Some(per_segment) => (Some(per_segment * total), true),
                        None => (hint, hint.is_some()),
                    }
                }
            }
        }
    }

    fn view(&self) -> ProgressDetail {
        match self {
            PartState::Http { chunks, .. } => ProgressDetail::Chunks {
                chunks: chunks
                    .iter()
                    .map(|c| ByteProgress {
                        done: c.done,
                        total: c.len(),
                    })
                    .collect(),
            },
            PartState::Hls {
                states,
                in_flight,
                encrypted,
                ..
            } => {
                let mut chars: Vec<u8> = states.bytes().collect();
                for &i in in_flight {
                    if let Some(c) = chars.get_mut(i as usize)
                        && *c == b'0'
                    {
                        *c = b'1';
                    }
                }
                ProgressDetail::Segments {
                    total: chars.len() as u32,
                    done: chars.iter().filter(|&&b| b == b'2').count() as u32,
                    in_flight: chars.iter().filter(|&&b| b == b'1').count() as u32,
                    states: String::from_utf8(chars).unwrap_or_default(),
                    encrypted: *encrypted,
                }
            }
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredDownload {
    pub view: Download,
    pub spec: JobSpec,
    /// Progress for each source in `spec.sources()` order.
    pub parts: Vec<Option<PartState>>,
}

impl StoredDownload {
    /// Recomputes the byte counts and the detail view from the part states.
    pub fn refresh_view(&mut self) {
        let sources = self.spec.sources();
        let mut done = 0;
        let mut total = Some(0u64);
        let mut estimate = false;
        for (i, src) in sources.iter().enumerate() {
            let part = self.parts.get(i).and_then(Option::as_ref);
            done += part.map_or(0, PartState::done_bytes);
            let (t, est) = part.map_or((src.size_hint, src.size_hint.is_some()), |p| {
                p.total_bytes(src.size_hint)
            });
            total = total.zip(t).map(|(a, b)| a + b);
            estimate |= est;
        }
        self.view.done_bytes = done;
        self.view.total_bytes = total.filter(|&t| t > 0);
        self.view.total_is_estimate = estimate;
        if self.view.status == DownloadStatus::Done {
            self.view.total_bytes = Some(done.max(self.view.total_bytes.unwrap_or(0)));
            self.view.total_is_estimate = false;
        }
        self.view.detail = self.detail();
    }

    fn detail(&self) -> ProgressDetail {
        let part_progress = |i: usize| -> ByteProgress {
            let src = self.spec.sources()[i];
            match self.parts.get(i).and_then(Option::as_ref) {
                Some(p) => ByteProgress {
                    done: p.done_bytes(),
                    total: p.total_bytes(src.size_hint).0,
                },
                None => ByteProgress {
                    done: 0,
                    total: src.size_hint,
                },
            }
        };
        match &self.spec.audio {
            Some(audio) => ProgressDetail::Streams {
                video: part_progress(0),
                audio: part_progress(1),
                video_label: self.spec.primary.label.clone(),
                audio_label: audio.label.clone(),
            },
            None => match self.parts.first().and_then(Option::as_ref) {
                Some(p) => p.view(),
                None => ProgressDetail::None,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_length_is_inclusive() {
        let c = Chunk {
            start: 10,
            end: Some(19),
            done: 10,
        };
        assert_eq!(c.len(), Some(10));
        assert!(c.complete());
        assert!(
            !Chunk {
                start: 0,
                end: None,
                done: 5
            }
            .complete()
        );
    }

    #[test]
    fn hls_estimate_grows_from_done_segments() {
        let p = PartState::Hls {
            states: "2200".into(),
            bytes: 200,
            in_flight: vec![2],
            encrypted: false,
        };
        assert_eq!(p.total_bytes(None), (Some(400), true));
        match p.view() {
            ProgressDetail::Segments {
                states,
                done,
                in_flight,
                ..
            } => {
                assert_eq!(states, "2210");
                assert_eq!((done, in_flight), (2, 1));
            }
            other => panic!("unexpected {other:?}"),
        }
    }
}
