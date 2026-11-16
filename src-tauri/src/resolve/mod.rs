//! Turns a link into downloadable streams: direct file → HLS playlist → yt-dlp → page scan.

mod direct;
mod hls;
mod page;
mod ytdlp;

use std::collections::HashMap;
use std::path::PathBuf;

use reqwest::Client;

use crate::job::{Convert, JobSpec, SubtitleSource};
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
    /// The stream's subtitle tracks by `SubtitleTrack` id, in the format that suits this
    /// choice's container. Empty when the choice cannot take subtitles (audio only).
    pub subtitles: Vec<(String, SubtitleSource)>,
    /// What ffmpeg changes when the user saves this video in a compatible format. None when
    /// the file is already compatible, and for audio only.
    pub convert: Option<Convert>,
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

/// Video sites whose video page links are safe to fetch before the user asks.
/// Other links can work only once (password reset, login, unsubscribe).
/// `is_video_page` knows the video page paths of each site.
const VIDEO_SITES: &[&str] = &[
    "youtube.com",
    "youtu.be",
    "vimeo.com",
    "x.com",
    "twitter.com",
    "instagram.com",
    "tiktok.com",
    "reddit.com",
    "v.redd.it",
    "twitch.tv",
    "dailymotion.com",
    "dai.ly",
    "facebook.com",
    "fb.watch",
];

/// True when the link opens one video on a listed video site or one of its subdomains.
/// A home page, profile, feed or search gives false.
pub fn is_video_page(url: &str) -> bool {
    let Ok(u) = url::Url::parse(url) else {
        return false;
    };
    let Some(host) = u.host_str().map(str::to_ascii_lowercase) else {
        return false;
    };
    let Some(site) = VIDEO_SITES.iter().copied().find(|site| {
        host == *site
            || host
                .strip_suffix(site)
                .is_some_and(|rest| rest.ends_with('.'))
    }) else {
        return false;
    };
    let path: Vec<&str> = u
        .path_segments()
        .map(|s| s.filter(|p| !p.is_empty()).collect())
        .unwrap_or_default();
    let has_query = |key: &str| u.query_pairs().any(|(k, v)| k == key && !v.is_empty());
    // True when the path has the segment `name` and a segment after it (the id).
    let id_after = |name: &str| {
        path.iter()
            .position(|p| *p == name)
            .is_some_and(|i| i + 1 < path.len())
    };
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    match site {
        "youtube.com" => match path.as_slice() {
            ["watch"] => has_query("v"),
            ["shorts" | "live" | "embed", _] => true,
            _ => false,
        },
        "youtu.be" | "v.redd.it" | "dai.ly" | "fb.watch" => path.len() == 1,
        "vimeo.com" => path.iter().any(|p| digits(p)),
        "x.com" | "twitter.com" => {
            matches!(path.as_slice(), [_, "status", id, ..] if digits(id))
        }
        "instagram.com" => ["p", "reel", "reels", "tv"].into_iter().any(id_after),
        "tiktok.com" if host.starts_with("vm.") || host.starts_with("vt.") => path.len() == 1,
        "tiktok.com" => match path.as_slice() {
            [user, "video", id] => user.starts_with('@') && digits(id),
            ["t", _] => true,
            _ => false,
        },
        "reddit.com" => id_after("comments") || matches!(path.as_slice(), ["r", _, "s", _]),
        "twitch.tv" if host == "clips.twitch.tv" => path.len() == 1,
        "twitch.tv" => matches!(path.as_slice(), ["videos", _] | [_, "clip", _]),
        "dailymotion.com" => matches!(path.as_slice(), ["video", _]),
        "facebook.com" => match path.as_slice() {
            ["watch"] => has_query("v"),
            ["reel", _] | ["share", "v" | "r", _] => true,
            _ => id_after("videos"),
        },
        _ => false,
    }
}

pub async fn resolve(
    client: &Client,
    url: &str,
    referer: Option<&str>,
    user_agent: Option<&str>,
    yt_dlp: Option<PathBuf>,
    english_titles: bool,
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
            Some(path) => {
                match ytdlp::resolve(&path, url, referer, user_agent, english_titles).await {
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
                }
            }
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

/// True when the default players on every system open the file: QuickTime on macOS,
/// Films & TV on Windows, phones and TVs. That means H.264 (or HEVC tagged `hvc1`) video
/// with AAC, MP3 or Dolby audio, in an MP4 or M4A file. VP9, AV1, Opus, WebM and MKV are not:
/// QuickTime does not open them, and only the newest Macs decode AV1.
/// `codecs` holds RFC 6381 strings ("avc1.640028", "mp4a.40.2") or yt-dlp names ("h264", "opus").
pub fn plays_everywhere<'a>(container: &str, codecs: impl IntoIterator<Item = &'a str>) -> bool {
    matches!(
        container.to_ascii_lowercase().as_str(),
        "mp4" | "m4a" | "m4v" | "mov"
    ) && codecs.into_iter().all(|c| match family(c) {
        None => true,
        Some(f) => VIDEO_OK.contains(&f.as_str()) || AUDIO_OK.contains(&f.as_str()),
    })
}

/// Video codecs that the default players open. QuickTime opens HEVC only when it is tagged `hvc1`.
const VIDEO_OK: [&str; 4] = ["avc1", "avc3", "h264", "hvc1"];
const AUDIO_OK: [&str; 5] = ["mp4a", "aac", "mp3", "ac-3", "ec-3"];
const VIDEO_FAMILIES: [&str; 12] = [
    "avc1", "avc3", "h264", "hvc1", "hev1", "hevc", "h265", "vp8", "vp09", "vp9", "av01", "av1",
];

/// "vp09.00.40.08" → "vp09". None for an empty or "none" codec.
fn family(codec: &str) -> Option<String> {
    let c = codec.trim().to_ascii_lowercase();
    if c.is_empty() || c == "none" {
        return None;
    }
    Some(c.split('.').next().unwrap_or("").to_string())
}

/// Splits an HLS `CODECS` list ("avc1.64001f,mp4a.40.2") into the video and the audio codec.
pub fn split_codecs(codecs: &str) -> (Option<&str>, Option<&str>) {
    let mut video = None;
    let mut audio = None;
    for c in codecs.split(',').map(str::trim).filter(|c| !c.is_empty()) {
        let is_video = family(c).is_some_and(|f| VIDEO_FAMILIES.contains(&f.as_str()));
        match (is_video, video, audio) {
            (true, None, _) => video = Some(c),
            (false, _, None) => audio = Some(c),
            _ => {}
        }
    }
    (video, audio)
}

/// What ffmpeg must change so that the default player opens a video file, and the format
/// that the player cannot open ("VP9", "AV1 and Opus", "WebM"). None when the file is already
/// compatible. An unknown codec counts as compatible, as in `plays_everywhere`.
pub fn compatible_convert(
    container: &str,
    vcodec: Option<&str>,
    acodec: Option<&str>,
    source_bitrate: Option<u64>,
    height: Option<u32>,
) -> Option<(Convert, String)> {
    let v = vcodec.and_then(family);
    let a = acodec.and_then(family);
    let video = v.as_deref().is_some_and(|f| !VIDEO_OK.contains(&f));
    let audio = a.as_deref().is_some_and(|f| !AUDIO_OK.contains(&f));
    let mut names: Vec<String> = Vec::new();
    if let Some(f) = v.as_deref().filter(|_| video) {
        names.push(format_name(f));
    }
    if let Some(f) = a.as_deref().filter(|_| audio) {
        names.push(format_name(f));
    }
    if names.is_empty() {
        let container = container.to_ascii_lowercase();
        if matches!(container.as_str(), "mp4" | "m4v" | "mov") {
            return None;
        }
        names.push(format_name(&container));
    }
    Some((
        Convert {
            video,
            audio,
            source_bitrate,
            height,
        },
        names.join(" and "),
    ))
}

/// The name that users know for a codec family or a container.
fn format_name(family: &str) -> String {
    match family {
        "vp09" | "vp9" => "VP9".into(),
        "vp8" => "VP8".into(),
        "av01" | "av1" => "AV1".into(),
        "hev1" | "hevc" | "h265" => "HEVC".into(),
        "opus" => "Opus".into(),
        "vorbis" => "Vorbis".into(),
        "webm" => "WebM".into(),
        other => other.to_ascii_uppercase(),
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
    fn knows_video_pages() {
        let pages = [
            "https://www.youtube.com/watch?v=abc",
            "https://music.youtube.com/watch?v=abc&list=x",
            "https://youtube.com/shorts/abc",
            "https://www.youtube.com/live/abc",
            "https://youtu.be/abc",
            "https://vimeo.com/123456",
            "https://vimeo.com/channels/staffpicks/123456",
            "https://player.vimeo.com/video/123456",
            "https://X.com/a/status/1",
            "https://twitter.com/a/status/1/video/1",
            "https://www.instagram.com/p/abc/",
            "https://www.instagram.com/reel/abc/?igsh=x",
            "https://instagram.com/user/reel/abc",
            "https://www.tiktok.com/@user/video/123",
            "https://vm.tiktok.com/abc/",
            "https://www.tiktok.com/t/abc/",
            "https://www.reddit.com/r/videos/comments/abc/title/",
            "https://www.reddit.com/r/videos/s/abc",
            "https://v.redd.it/abc",
            "https://www.twitch.tv/videos/123",
            "https://www.twitch.tv/user/clip/abc",
            "https://clips.twitch.tv/abc",
            "https://www.dailymotion.com/video/abc",
            "https://dai.ly/abc",
            "https://www.facebook.com/watch?v=123",
            "https://www.facebook.com/page/videos/123/",
            "https://www.facebook.com/reel/123",
            "https://www.facebook.com/share/v/abc/",
            "https://fb.watch/abc/",
        ];
        for url in pages {
            assert!(is_video_page(url), "{url}");
        }
        let others = [
            "https://instagram.com",
            "https://www.instagram.com/user/",
            "https://www.instagram.com/reels/",
            "https://www.youtube.com/",
            "https://www.youtube.com/watch",
            "https://www.youtube.com/@channel",
            "https://www.youtube.com/results?search_query=a",
            "https://youtu.be/",
            "https://vimeo.com/user",
            "https://x.com/user",
            "https://x.com/user/status/abc",
            "https://www.tiktok.com/@user",
            "https://www.reddit.com/r/videos/",
            "https://www.twitch.tv/user",
            "https://www.facebook.com/page/videos/",
            "https://notyoutube.com/watch?v=abc",
            "https://youtube.com.evil.test/watch?v=abc",
            "https://example.com/reset?token=1",
            "not a link",
        ];
        for url in others {
            assert!(!is_video_page(url), "{url}");
        }
    }

    #[test]
    fn knows_what_plays_everywhere() {
        assert!(plays_everywhere("mp4", ["avc1.640028", "mp4a.40.2"]));
        assert!(plays_everywhere("MP4", ["h264", "aac"]));
        assert!(plays_everywhere("m4a", ["mp4a.40.2"]));
        assert!(!plays_everywhere("mp4", ["av01.0.08M.08", "mp4a.40.2"]));
        assert!(!plays_everywhere("mp4", ["avc1.640028", "opus"]));
        assert!(!plays_everywhere("mp4", ["hev1.1.6.L93.B0"]));
        assert!(!plays_everywhere("webm", ["vp9", "opus"]));
        assert!(!plays_everywhere("mkv", ["avc1", "mp4a"]));
        // Unknown codecs count as fine; the container decides.
        assert!(plays_everywhere("mp4", []));
    }

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

    #[test]
    fn compatible_convert_names_what_the_player_cannot_open() {
        let name =
            |container, v, a| compatible_convert(container, v, a, None, None).map(|(_, n)| n);
        assert_eq!(
            name("mp4", Some("vp09.00.40.08"), Some("mp4a.40.5")),
            Some("VP9".into())
        );
        assert_eq!(
            name("webm", Some("av01.0.08M.08"), Some("opus")),
            Some("AV1 and Opus".into())
        );
        assert_eq!(
            name("mkv", Some("avc1.640028"), Some("mp4a.40.2")),
            Some("MKV".into())
        );
        assert_eq!(name("webm", Some("avc1.640028"), None), Some("WebM".into()));
        assert_eq!(name("mp4", Some("avc1.640028"), Some("mp4a.40.2")), None);
        // An unknown codec counts as compatible.
        assert_eq!(name("mp4", None, None), None);
    }

    #[test]
    fn compatible_convert_recodes_only_the_incompatible_track() {
        let (c, _) = compatible_convert(
            "mp4",
            Some("vp09"),
            Some("mp4a.40.2"),
            Some(900_000),
            Some(1078),
        )
        .unwrap();
        assert_eq!(
            c,
            Convert {
                video: true,
                audio: false,
                source_bitrate: Some(900_000),
                height: Some(1078),
            }
        );
        let (c, _) = compatible_convert("webm", Some("avc1"), Some("opus"), None, None).unwrap();
        assert!(!c.video && c.audio);
        let (c, _) = compatible_convert("mkv", Some("h264"), Some("aac"), None, None).unwrap();
        assert!(!c.video && !c.audio);
    }

    #[test]
    fn split_codecs_finds_video_and_audio() {
        assert_eq!(
            split_codecs("avc1.64001f,mp4a.40.2"),
            (Some("avc1.64001f"), Some("mp4a.40.2"))
        );
        assert_eq!(
            split_codecs("mp4a.40.2, vp09.00.40.08"),
            (Some("vp09.00.40.08"), Some("mp4a.40.2"))
        );
        assert_eq!(split_codecs(""), (None, None));
    }
}
