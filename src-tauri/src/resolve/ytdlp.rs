//! Step 3: ask yt-dlp for the formats of a site that Siphon cannot read by itself.

use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use serde::Deserialize;

use super::{Choice, name_from};
use crate::job::{JobSpec, MediaSource, SourceKind};
use crate::model::{DownloadKind, Quality, Stream};
use crate::tools;

const TIMEOUT: Duration = Duration::from_secs(120);
const MAX_QUALITIES: usize = 12;

pub enum Outcome {
    Found {
        stream: Stream,
        choices: Vec<Choice>,
        summary: String,
    },
    NotFound(String),
    Failed(String),
}

#[derive(Deserialize)]
struct Info {
    #[serde(rename = "_type")]
    kind: Option<String>,
    title: Option<String>,
    duration: Option<f64>,
    extractor_key: Option<String>,
    webpage_url: Option<String>,
    url: Option<String>,
    ext: Option<String>,
    protocol: Option<String>,
    http_headers: Option<HashMap<String, String>>,
    formats: Option<Vec<Format>>,
}

#[derive(Deserialize, Clone, Debug)]
struct Format {
    format_id: Option<String>,
    url: Option<String>,
    ext: Option<String>,
    vcodec: Option<String>,
    acodec: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
    fps: Option<f64>,
    tbr: Option<f64>,
    abr: Option<f64>,
    filesize: Option<u64>,
    filesize_approx: Option<f64>,
    protocol: Option<String>,
    http_headers: Option<HashMap<String, String>>,
    has_drm: Option<serde_json::Value>,
}

impl Format {
    fn usable(&self) -> bool {
        let protocol_ok = matches!(
            self.protocol.as_deref(),
            None | Some("http" | "https" | "m3u8" | "m3u8_native")
        );
        let drm = matches!(self.has_drm, Some(serde_json::Value::Bool(true)));
        protocol_ok
            && !drm
            && self.url.is_some()
            && self.ext.as_deref() != Some("mhtml")
            && (self.has_video() || self.has_audio())
    }

    /// An absent codec means yt-dlp does not know. Treat it as present.
    fn has_video(&self) -> bool {
        self.vcodec.as_deref() != Some("none")
    }

    fn has_audio(&self) -> bool {
        self.acodec.as_deref() != Some("none")
    }

    fn size(&self) -> (Option<u64>, bool) {
        match (self.filesize, self.filesize_approx) {
            (Some(s), _) => (Some(s), false),
            (None, Some(a)) => (Some(a as u64), true),
            _ => (None, false),
        }
    }

    fn ext(&self) -> &str {
        self.ext.as_deref().unwrap_or("mp4")
    }

    fn source(
        &self,
        label: String,
        fallback_headers: &Option<HashMap<String, String>>,
    ) -> MediaSource {
        let kind = match self.protocol.as_deref() {
            Some(p) if p.starts_with("m3u8") => SourceKind::Hls,
            _ => SourceKind::Http,
        };
        let headers = self
            .http_headers
            .as_ref()
            .or(fallback_headers.as_ref())
            .map(sorted_pairs)
            .unwrap_or_default();
        MediaSource {
            kind,
            url: self.url.clone().unwrap_or_default(),
            headers,
            label,
            size_hint: self.size().0,
        }
    }
}

fn sorted_pairs(map: &HashMap<String, String>) -> Vec<(String, String)> {
    let mut v: Vec<_> = map.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
    v.sort();
    v
}

fn short_codec(c: Option<&str>) -> Option<&str> {
    c.filter(|c| *c != "none")
        .map(|c| c.split('.').next().unwrap_or(c))
}

pub async fn resolve(
    yt_dlp: &Path,
    url: &str,
    referer: Option<&str>,
    user_agent: Option<&str>,
) -> Outcome {
    let mut cmd = tools::command(yt_dlp);
    cmd.args([
        "-J",
        "--no-playlist",
        "--no-warnings",
        "--no-progress",
        "--no-color",
    ]);
    if let Some(r) = referer.filter(|s| !s.trim().is_empty()) {
        cmd.args(["--referer", r.trim()]);
    }
    if let Some(ua) = user_agent.filter(|s| !s.trim().is_empty()) {
        cmd.args(["--user-agent", ua.trim()]);
    }
    cmd.args(["--", url]);
    let output = match tokio::time::timeout(TIMEOUT, cmd.output()).await {
        Err(_) => return Outcome::Failed("yt-dlp did not answer in 2 minutes.".into()),
        Ok(Err(e)) => return Outcome::Failed(format!("Siphon could not start yt-dlp: {e}.")),
        Ok(Ok(o)) => o,
    };
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("Unsupported URL") {
            return Outcome::NotFound("yt-dlp does not support this site.".into());
        }
        let line = stderr
            .lines()
            .rev()
            .find(|l| l.contains("ERROR"))
            .or_else(|| stderr.lines().rev().find(|l| !l.trim().is_empty()))
            .unwrap_or("yt-dlp failed without a message.");
        let line = line.trim().trim_start_matches("ERROR:").trim();
        return Outcome::Failed(line.chars().take(240).collect());
    }
    match serde_json::from_slice::<Info>(&output.stdout) {
        Ok(info) => from_info(info, url),
        Err(e) => Outcome::Failed(format!("Siphon could not read the yt-dlp answer: {e}.")),
    }
}

struct Candidate {
    video: Format,
    audio: Option<Format>,
    height: u32,
    tbr: f64,
}

fn from_info(info: Info, input_url: &str) -> Outcome {
    if info.kind.as_deref() == Some("playlist") {
        return Outcome::NotFound("The link is a playlist. Add a link to one video.".into());
    }
    let mut formats: Vec<Format> = info.formats.clone().unwrap_or_default();
    if formats.is_empty()
        && let Some(u) = &info.url
    {
        formats.push(Format {
            format_id: Some("0".into()),
            url: Some(u.clone()),
            ext: info.ext.clone(),
            vcodec: None,
            acodec: None,
            width: None,
            height: None,
            fps: None,
            tbr: None,
            abr: None,
            filesize: None,
            filesize_approx: None,
            protocol: info.protocol.clone(),
            http_headers: info.http_headers.clone(),
            has_drm: None,
        });
    }
    let usable: Vec<Format> = formats.into_iter().filter(Format::usable).collect();
    if usable.is_empty() {
        return Outcome::NotFound("yt-dlp found no format that Siphon can download.".into());
    }

    let audio_only: Vec<&Format> = usable
        .iter()
        .filter(|f| !f.has_video() && f.has_audio())
        .collect();
    let best_audio_for = |video_ext: &str| -> Option<Format> {
        let rate = |f: &&Format| f.abr.or(f.tbr).unwrap_or(0.0);
        let preferred = if video_ext == "webm" { "webm" } else { "m4a" };
        audio_only
            .iter()
            .filter(|f| f.ext() == preferred)
            .max_by(|a, b| rate(a).total_cmp(&rate(b)))
            .or_else(|| audio_only.iter().max_by(|a, b| rate(a).total_cmp(&rate(b))))
            .map(|f| (*f).clone())
    };

    let mut candidates: Vec<Candidate> = Vec::new();
    for f in usable.iter().filter(|f| f.has_video()) {
        let video_only = f.acodec.as_deref() == Some("none");
        let audio = if video_only {
            match best_audio_for(f.ext()) {
                Some(a) => Some(a),
                None => continue,
            }
        } else {
            None
        };
        let tbr =
            f.tbr.unwrap_or(0.0) + audio.as_ref().and_then(|a| a.tbr.or(a.abr)).unwrap_or(0.0);
        candidates.push(Candidate {
            video: f.clone(),
            audio,
            height: f.height.unwrap_or(0),
            tbr,
        });
    }
    candidates.sort_by(|a, b| b.height.cmp(&a.height).then(b.tbr.total_cmp(&a.tbr)));
    // One row for each height and container family keeps the list short.
    let mut seen = std::collections::HashSet::new();
    candidates.retain(|c| seen.insert((c.height, c.video.ext() == "webm")));
    candidates.truncate(MAX_QUALITIES);

    let title = info.title.clone().unwrap_or_else(|| "video".into());
    let mut qualities = Vec::new();
    let mut choices = Vec::new();
    for (i, c) in candidates.iter().enumerate() {
        let container = container_for(c.video.ext(), c.audio.as_ref().map(Format::ext));
        let label = video_label(&c.video);
        let (vs, va) = c.video.size();
        let (asz, aa) = c
            .audio
            .as_ref()
            .map(Format::size)
            .unwrap_or((Some(0), false));
        let size = vs.zip(asz).map(|(a, b)| a + b);
        let codecs = [
            short_codec(c.video.vcodec.as_deref()),
            short_codec(
                c.audio
                    .as_ref()
                    .map_or(c.video.acodec.as_deref(), |a| a.acodec.as_deref()),
            ),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" + ");
        qualities.push(Quality {
            id: format!("q{}", i + 1),
            label: label.clone(),
            resolution: c
                .video
                .width
                .zip(c.video.height)
                .map(|(w, h)| format!("{w}×{h}")),
            codecs: (!codecs.is_empty()).then_some(codecs),
            bitrate: (c.tbr > 0.0).then_some((c.tbr * 1000.0) as u64),
            size_bytes: size,
            size_is_estimate: va || aa,
            separate_audio: c.audio.is_some(),
            note: Some(container.to_uppercase()),
        });
        let audio = c
            .audio
            .as_ref()
            .map(|a| a.source(format!("Audio · {}", a.ext()), &info.http_headers));
        choices.push(Choice {
            kind: DownloadKind::Ytdlp,
            spec: JobSpec {
                primary: c
                    .video
                    .source(format!("Video · {label}"), &info.http_headers),
                audio,
                duration: info.duration,
                encrypted: false,
            },
            quality_label: Some(label),
            output_ext: Some(container.into()),
        });
    }

    let best_audio = audio_only.iter().max_by(|a, b| {
        a.abr
            .or(a.tbr)
            .unwrap_or(0.0)
            .total_cmp(&b.abr.or(b.tbr).unwrap_or(0.0))
    });
    if let Some(a) = best_audio {
        let (size, approx) = a.size();
        // Audio-only downloads are not remuxed, so the file keeps the source container.
        let ext = a.ext().to_string();
        qualities.push(Quality {
            id: format!("q{}", qualities.len() + 1),
            label: "Audio only".into(),
            resolution: None,
            codecs: short_codec(a.acodec.as_deref()).map(str::to_string),
            bitrate: a.abr.or(a.tbr).map(|r| (r * 1000.0) as u64),
            size_bytes: size,
            size_is_estimate: approx,
            separate_audio: false,
            note: Some(ext.to_uppercase()),
        });
        choices.push(Choice {
            kind: DownloadKind::Ytdlp,
            spec: JobSpec {
                primary: a.source(format!("Audio · {}", a.ext()), &info.http_headers),
                audio: None,
                duration: info.duration,
                encrypted: false,
            },
            quality_label: Some("Audio only".into()),
            output_ext: Some(ext),
        });
    }
    if qualities.is_empty() {
        return Outcome::NotFound("yt-dlp found no format that Siphon can download.".into());
    }

    let first_ext = choices[0]
        .output_ext
        .clone()
        .unwrap_or_else(|| "mp4".into());
    let site = info
        .extractor_key
        .clone()
        .unwrap_or_else(|| "yt-dlp".into());
    let summary = format!("{site}: {} formats.", qualities.len());
    let stream = Stream {
        id: String::new(),
        kind: DownloadKind::Ytdlp,
        title,
        url: info.webpage_url.clone().unwrap_or_else(|| input_url.into()),
        duration_seconds: info.duration,
        encrypted: false,
        qualities,
        suggested_file_name: name_from(info.title.as_deref(), input_url, &first_ext),
    };
    Outcome::Found {
        stream,
        choices,
        summary,
    }
}

fn video_label(f: &Format) -> String {
    match (f.height, f.fps) {
        (Some(h), Some(fps)) if fps > 31.0 => format!("{h}p{}", fps.round() as u32),
        (Some(h), _) => format!("{h}p"),
        _ => f.format_id.clone().unwrap_or_else(|| "Best".into()),
    }
}

/// mp4 video with m4a audio stays MP4. webm with webm stays WebM. Anything else goes into MKV.
fn container_for(video_ext: &str, audio_ext: Option<&str>) -> &'static str {
    match (video_ext, audio_ext) {
        ("mp4", None | Some("m4a" | "mp4")) => "mp4",
        ("webm", None | Some("webm")) => "webm",
        (_, None) if video_ext == "mkv" => "mkv",
        ("flv", None) => "flv",
        (_, None) => "mp4",
        _ => "mkv",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
      "title": "Demo: part 1", "duration": 120.0, "extractor_key": "Example", "webpage_url": "https://site.test/watch/1",
      "http_headers": {"User-Agent": "UA-X"},
      "formats": [
        {"format_id": "sb0", "ext": "mhtml", "url": "https://s/sb", "vcodec": "none", "acodec": "none", "protocol": "mhtml"},
        {"format_id": "140", "ext": "m4a", "url": "https://s/a140", "vcodec": "none", "acodec": "mp4a.40.2", "abr": 129.5, "filesize": 1000, "protocol": "https"},
        {"format_id": "251", "ext": "webm", "url": "https://s/a251", "vcodec": "none", "acodec": "opus", "abr": 140.0, "filesize": 900, "protocol": "https"},
        {"format_id": "18", "ext": "mp4", "url": "https://s/v18", "vcodec": "avc1.42001E", "acodec": "mp4a.40.2", "width": 640, "height": 360, "tbr": 500.0, "filesize_approx": 7500000.0, "protocol": "https"},
        {"format_id": "137", "ext": "mp4", "url": "https://s/v137", "vcodec": "avc1.640028", "acodec": "none", "width": 1920, "height": 1080, "tbr": 4000.0, "filesize": 60000000, "protocol": "https"},
        {"format_id": "248", "ext": "webm", "url": "https://s/v248", "vcodec": "vp9", "acodec": "none", "width": 1920, "height": 1080, "tbr": 3000.0, "protocol": "https"},
        {"format_id": "dash", "ext": "mp4", "url": "https://s/d", "vcodec": "avc1", "acodec": "none", "height": 2160, "protocol": "http_dash_segments"},
        {"format_id": "hls-720", "ext": "mp4", "url": "https://s/720.m3u8", "vcodec": "avc1", "acodec": "mp4a", "height": 720, "tbr": 2000.0, "protocol": "m3u8_native"}
      ]
    }"#;

    #[test]
    fn maps_formats_to_qualities() {
        let info: Info = serde_json::from_str(SAMPLE).unwrap();
        let Outcome::Found {
            stream,
            choices,
            summary,
        } = from_info(info, "https://site.test/watch/1")
        else {
            panic!("expected formats");
        };
        assert_eq!(summary, "Example: 5 formats.");
        let labels: Vec<_> = stream.qualities.iter().map(|q| q.label.as_str()).collect();
        assert_eq!(labels, ["1080p", "1080p", "720p", "360p", "Audio only"]);
        // 1080p MP4 pairs with m4a audio and stays MP4.
        assert!(stream.qualities[0].separate_audio);
        assert_eq!(stream.qualities[0].size_bytes, Some(60_001_000));
        assert_eq!(choices[0].output_ext.as_deref(), Some("mp4"));
        assert!(
            choices[0]
                .spec
                .audio
                .as_ref()
                .unwrap()
                .url
                .ends_with("a140")
        );
        assert_eq!(
            choices[0].spec.primary.headers,
            vec![("User-Agent".to_string(), "UA-X".to_string())]
        );
        // 1080p WebM pairs with opus audio and stays WebM; its size is unknown.
        assert_eq!(choices[1].output_ext.as_deref(), Some("webm"));
        assert_eq!(stream.qualities[1].size_bytes, None);
        // HLS formats use the HLS engine.
        assert_eq!(choices[2].spec.primary.kind, SourceKind::Hls);
        assert!(stream.qualities[3].size_is_estimate);
        assert_eq!(choices[4].output_ext.as_deref(), Some("webm"));
        assert_eq!(stream.suggested_file_name, "Demo_ part 1.mp4");
    }

    #[test]
    fn playlists_and_drm_are_not_offered() {
        let info: Info = serde_json::from_str(r#"{"_type": "playlist", "title": "x"}"#).unwrap();
        assert!(matches!(
            from_info(info, "https://a.test"),
            Outcome::NotFound(_)
        ));
        let info: Info = serde_json::from_str(
            r#"{"title": "x", "formats": [{"format_id": "1", "ext": "mp4", "url": "https://s/1", "has_drm": true, "height": 720}]}"#,
        )
        .unwrap();
        assert!(matches!(
            from_info(info, "https://a.test"),
            Outcome::NotFound(_)
        ));
    }

    #[test]
    fn chooses_containers() {
        assert_eq!(container_for("mp4", Some("m4a")), "mp4");
        assert_eq!(container_for("webm", Some("webm")), "webm");
        assert_eq!(container_for("mp4", Some("webm")), "mkv");
        assert_eq!(container_for("mp4", None), "mp4");
    }
}
