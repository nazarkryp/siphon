//! Step 1: does the link return a file?

use reqwest::Client;
use reqwest::header::{CONTENT_DISPOSITION, CONTENT_TYPE, HeaderMap};

use super::Choice;
use crate::engine::http::probe;
use crate::job::{JobSpec, MediaSource, SourceKind};
use crate::model::{DownloadKind, Quality, Stream};
use crate::util;

// The value lives only while one link resolves, so the size of the File variant does not matter.
#[allow(clippy::large_enum_variant)]
pub enum Outcome {
    File {
        stream: Stream,
        choice: Choice,
        summary: String,
    },
    Playlist(String),
    Page,
    Failed(String),
}

pub async fn check(client: &Client, url: &str, headers: &HeaderMap) -> Outcome {
    match probe(client, url, headers).await {
        Ok((p, resp)) => {
            let final_url = resp.url().to_string();
            let content_type = header(&resp, CONTENT_TYPE)
                .unwrap_or_default()
                .to_ascii_lowercase();
            let disposition = header(&resp, CONTENT_DISPOSITION);
            drop(resp);
            classify(
                &final_url,
                &content_type,
                disposition.as_deref(),
                p.total,
                p.ranged,
            )
        }
        Err(e) => Outcome::Failed(e.message),
    }
}

fn header(resp: &reqwest::Response, name: reqwest::header::HeaderName) -> Option<String> {
    resp.headers()
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
}

/// Decides from the response headers. Also used by the page scan for found links.
pub fn classify(
    final_url: &str,
    content_type: &str,
    disposition: Option<&str>,
    total: Option<u64>,
    ranged: bool,
) -> Outcome {
    let attachment = disposition.is_some_and(|d| d.to_ascii_lowercase().contains("filename"));
    if content_type.contains("mpegurl") || (util::path_ends_with(final_url, ".m3u8") && !attachment)
    {
        return Outcome::Playlist(final_url.to_string());
    }
    if (content_type.starts_with("text/html") || content_type.starts_with("application/xhtml"))
        && !attachment
    {
        return Outcome::Page;
    }
    let name = disposition
        .and_then(util::file_name_from_disposition)
        .or_else(|| util::file_name_from_url(final_url))
        .map(|n| util::sanitize_file_name(&n))
        .unwrap_or_else(|| "download".into());
    let kind_text = content_type
        .split(';')
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or("unknown type");
    let size_text = total
        .map(util::format_bytes)
        .unwrap_or_else(|| "unknown size".into());
    let stream = Stream {
        id: String::new(),
        kind: DownloadKind::File,
        title: name.clone(),
        url: final_url.to_string(),
        duration_seconds: None,
        encrypted: false,
        qualities: vec![Quality {
            id: "q1".into(),
            label: "Original file".into(),
            resolution: None,
            codecs: Some(kind_text.to_string()),
            bitrate: None,
            size_bytes: total,
            size_is_estimate: false,
            separate_audio: false,
            note: (!ranged).then(|| "The server allows only one connection.".to_string()),
            // The file is saved as the server sends it; Siphon has no other version to offer.
            plays_everywhere: true,
        }],
        suggested_file_name: name,
    };
    let choice = Choice {
        kind: DownloadKind::File,
        spec: JobSpec {
            primary: MediaSource {
                kind: SourceKind::Http,
                url: final_url.to_string(),
                headers: Vec::new(),
                label: "File".into(),
                size_hint: total,
            },
            audio: None,
            duration: None,
            encrypted: false,
        },
        quality_label: None,
        output_ext: None,
    };
    Outcome::File {
        stream,
        choice,
        summary: format!("A file: {kind_text}, {size_text}."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_by_type_and_name() {
        assert!(matches!(
            classify(
                "https://a.test/x.m3u8",
                "application/octet-stream",
                None,
                None,
                false
            ),
            Outcome::Playlist(_)
        ));
        assert!(matches!(
            classify(
                "https://a.test/p",
                "application/vnd.apple.mpegurl",
                None,
                None,
                false
            ),
            Outcome::Playlist(_)
        ));
        assert!(matches!(
            classify(
                "https://a.test/p",
                "text/html; charset=utf-8",
                None,
                None,
                false
            ),
            Outcome::Page
        ));
        match classify(
            "https://a.test/dl?id=1",
            "text/html",
            Some("attachment; filename=\"r.zip\""),
            Some(2048),
            true,
        ) {
            Outcome::File { stream, .. } => {
                assert_eq!(stream.suggested_file_name, "r.zip");
                assert_eq!(stream.qualities[0].size_bytes, Some(2048));
                assert!(stream.qualities[0].note.is_none());
            }
            _ => panic!("expected a file"),
        }
        match classify("https://a.test/v/clip.mp4", "video/mp4", None, None, false) {
            Outcome::File {
                stream, summary, ..
            } => {
                assert_eq!(stream.title, "clip.mp4");
                assert!(summary.contains("video/mp4"));
                assert!(stream.qualities[0].note.is_some());
            }
            _ => panic!("expected a file"),
        }
    }
}
