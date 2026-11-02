//! Turns a link into downloadable streams: direct file → HLS playlist → yt-dlp → page scan.

mod direct;
mod hls;
mod page;
mod ytdlp;

use std::collections::HashMap;
use std::path::PathBuf;

use reqwest::Client;

use crate::job::JobSpec;
use crate::model::{
    DownloadKind, ResolveResult, ResolveStep, ResolveStepId, ResolveStepState, Stream,
};
use crate::net;

/// One quality of one stream, ready to become a download.
#[derive(Clone, Debug)]
pub struct Choice {
    pub kind: DownloadKind,
    pub spec: JobSpec,
    pub quality_label: Option<String>,
    /// Extension of the file that this choice produces, without the dot.
    pub output_ext: Option<String>,
}

pub struct Resolved {
    pub result: ResolveResult,
    pub choices: HashMap<(String, String), Choice>,
}

/// Streams found by one step, with the choices behind their qualities.
#[derive(Default)]
struct Found {
    streams: Vec<Stream>,
    choices: Vec<Vec<Choice>>,
}

impl Found {
    fn push(&mut self, stream: Stream, choices: Vec<Choice>) {
        self.streams.push(stream);
        self.choices.push(choices);
    }

    fn is_empty(&self) -> bool {
        self.streams.is_empty()
    }
}

pub fn validate_url(url: &str) -> Result<String, String> {
    let trimmed = url.trim();
    match url::Url::parse(trimmed) {
        Ok(u) if matches!(u.scheme(), "http" | "https") && u.host_str().is_some() => {
            Ok(u.to_string())
        }
        _ => Err("Enter a link that starts with http:// or https://.".into()),
    }
}

pub async fn resolve(
    client: &Client,
    url: &str,
    referer: Option<&str>,
    user_agent: Option<&str>,
    yt_dlp: Option<PathBuf>,
) -> Resolved {
    let headers = net::request_headers(&[], referer, user_agent);
    let mut steps = Vec::new();
    let mut found = Found::default();
    let step = |id, state, message: String| ResolveStep { id, state, message };
    let not_needed = || "Not needed.".to_string();

    let outcome = direct::check(client, url, &headers).await;
    let mut playlist_url = None;
    let mut is_page = false;
    match outcome {
        direct::Outcome::File {
            stream,
            choice,
            summary,
        } => {
            steps.push(step(
                ResolveStepId::Direct,
                ResolveStepState::Found,
                summary,
            ));
            found.push(stream, vec![choice]);
        }
        direct::Outcome::Playlist(u) => {
            steps.push(step(
                ResolveStepId::Direct,
                ResolveStepState::NotFound,
                "The link is an HLS playlist.".into(),
            ));
            playlist_url = Some(u);
        }
        direct::Outcome::Page => {
            steps.push(step(
                ResolveStepId::Direct,
                ResolveStepState::NotFound,
                "The link is a web page (text/html).".into(),
            ));
            is_page = true;
        }
        direct::Outcome::Failed(message) => {
            steps.push(step(
                ResolveStepId::Direct,
                ResolveStepState::Error,
                message,
            ));
            if crate::util::path_ends_with(url, ".m3u8") {
                playlist_url = Some(url.to_string());
            }
        }
    }

    if !found.is_empty() {
        steps.push(step(
            ResolveStepId::Hls,
            ResolveStepState::Skipped,
            not_needed(),
        ));
    } else if let Some(pl) = playlist_url {
        match hls::resolve(client, &pl, &headers, None).await {
            Ok((stream, choices, summary)) => {
                steps.push(step(ResolveStepId::Hls, ResolveStepState::Found, summary));
                found.push(stream, choices);
            }
            Err(e) => steps.push(step(ResolveStepId::Hls, ResolveStepState::Error, e.message)),
        }
    } else {
        steps.push(step(
            ResolveStepId::Hls,
            ResolveStepState::NotFound,
            "The link is not a playlist.".into(),
        ));
    }

    if !found.is_empty() {
        steps.push(step(
            ResolveStepId::Ytdlp,
            ResolveStepState::Skipped,
            not_needed(),
        ));
    } else {
        match yt_dlp {
            None => steps.push(step(
                ResolveStepId::Ytdlp,
                ResolveStepState::Skipped,
                "yt-dlp is not installed.".into(),
            )),
            Some(path) => match ytdlp::resolve(&path, url, referer, user_agent).await {
                ytdlp::Outcome::Found {
                    stream,
                    choices,
                    summary,
                } => {
                    steps.push(step(ResolveStepId::Ytdlp, ResolveStepState::Found, summary));
                    found.push(stream, choices);
                }
                ytdlp::Outcome::NotFound(m) => {
                    steps.push(step(ResolveStepId::Ytdlp, ResolveStepState::NotFound, m))
                }
                ytdlp::Outcome::Failed(m) => {
                    steps.push(step(ResolveStepId::Ytdlp, ResolveStepState::Error, m))
                }
            },
        }
    }

    if !found.is_empty() {
        steps.push(step(
            ResolveStepId::Page,
            ResolveStepState::Skipped,
            not_needed(),
        ));
    } else if is_page {
        match page::scan(client, url, &headers).await {
            Ok((streams, summary)) if !streams.is_empty() => {
                steps.push(step(ResolveStepId::Page, ResolveStepState::Found, summary));
                for (stream, choices) in streams {
                    found.push(stream, choices);
                }
            }
            Ok((_, summary)) => steps.push(step(
                ResolveStepId::Page,
                ResolveStepState::NotFound,
                summary,
            )),
            Err(e) => steps.push(step(
                ResolveStepId::Page,
                ResolveStepState::Error,
                e.message,
            )),
        }
    } else {
        steps.push(step(
            ResolveStepId::Page,
            ResolveStepState::Skipped,
            "The link is not a web page.".into(),
        ));
    }

    let mut choices = HashMap::new();
    let mut streams = Vec::new();
    for (n, (mut stream, stream_choices)) in
        found.streams.into_iter().zip(found.choices).enumerate()
    {
        stream.id = format!("s{}", n + 1);
        for (q, choice) in stream.qualities.iter_mut().zip(stream_choices) {
            choices.insert((stream.id.clone(), q.id.clone()), choice);
        }
        streams.push(stream);
    }

    Resolved {
        result: ResolveResult {
            resolve_id: uuid::Uuid::new_v4().to_string(),
            url: url.to_string(),
            steps,
            streams,
            free_space_bytes: None,
        },
        choices,
    }
}

/// A file name from a page title or a URL: "Opening keynote" or "keynote" for ".../keynote/master.m3u8".
pub fn name_from(hint: Option<&str>, url: &str, ext: &str) -> String {
    const GENERIC: [&str; 7] = [
        "master",
        "index",
        "playlist",
        "chunklist",
        "prog_index",
        "manifest",
        "video",
    ];
    let from_url = || {
        let parsed = url::Url::parse(url).ok()?;
        let segments: Vec<String> = parsed
            .path_segments()?
            .filter(|s| !s.is_empty())
            .map(|s| {
                percent_encoding::percent_decode_str(s)
                    .decode_utf8_lossy()
                    .to_string()
            })
            .collect();
        let last = segments.last()?;
        let (stem, _) = crate::util::split_ext(last);
        if GENERIC.iter().any(|g| stem.eq_ignore_ascii_case(g)) {
            segments
                .iter()
                .rev()
                .nth(1)
                .cloned()
                .or_else(|| parsed.host_str().map(str::to_string))
        } else {
            Some(stem.to_string())
        }
    };
    let base = hint
        .map(str::trim)
        .filter(|h| !h.is_empty())
        .map(str::to_string)
        .or_else(from_url)
        .unwrap_or_else(|| "video".into());
    crate::util::sanitize_file_name(&format!("{base}.{ext}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_links() {
        assert!(validate_url(" https://a.test/x ").is_ok());
        assert!(validate_url("ftp://a.test/x").is_err());
        assert!(validate_url("not a link").is_err());
    }

    #[test]
    fn names_skip_generic_playlist_names() {
        assert_eq!(
            name_from(None, "https://cdn.test/talks/keynote/master.m3u8", "mp4"),
            "keynote.mp4"
        );
        assert_eq!(
            name_from(None, "https://cdn.test/v/clip-04.m3u8?x=1", "mp4"),
            "clip-04.mp4"
        );
        assert_eq!(
            name_from(Some("A: title?"), "https://cdn.test/a.m3u8", "mp4"),
            "A_ title_.mp4"
        );
    }
}
