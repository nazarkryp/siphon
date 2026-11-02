//! Step 4: find video links in the HTML of a page.

use std::sync::LazyLock;

use futures_util::StreamExt;
use regex::Regex;
use reqwest::Client;
use reqwest::header::HeaderMap;

use super::{Choice, direct, hls};
use crate::model::Stream;
use crate::net::DlError;
use crate::util;

const MAX_PAGE_BYTES: usize = 5 * 1024 * 1024;
const MAX_CANDIDATES: usize = 10;

static TAG_SRC: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)<(?:video|source)\b[^>]*?\bsrc\s*=\s*["']([^"']+)["']"#).unwrap()
});
static MEDIA_URL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)https?://[^\s"'<>()\\]+?\.(?:m3u8|mp4|webm|m4v|mov)(?:\?[^\s"'<>()\\]*)?"#)
        .unwrap()
});
static TITLE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?is)<title[^>]*>(.*?)</title>").unwrap());

type Found = Vec<(Stream, Vec<Choice>)>;

pub async fn scan(
    client: &Client,
    url: &str,
    headers: &HeaderMap,
) -> Result<(Found, String), DlError> {
    let (html, final_url) = fetch_page(client, url, headers).await?;
    let title = page_title(&html);
    let links = find_media_links(&html, &final_url);
    if links.is_empty() {
        return Ok((Vec::new(), "The page has no video links.".into()));
    }

    let tasks: Vec<_> = links
        .into_iter()
        .map(|link| check_link(client, link, headers, title.as_deref()))
        .collect();
    let results: Vec<Option<(Stream, Vec<Choice>)>> = futures_util::stream::iter(tasks)
        .buffered(4)
        .collect()
        .await;
    let found: Found = results.into_iter().flatten().collect();
    let summary = match found.len() {
        0 => "The page has video links, but none of them works.".to_string(),
        1 => "Found 1 video on the page.".to_string(),
        n => format!("Found {n} videos on the page."),
    };
    Ok((found, summary))
}

async fn check_link(
    client: &Client,
    link: String,
    headers: &HeaderMap,
    title: Option<&str>,
) -> Option<(Stream, Vec<Choice>)> {
    if util::path_ends_with(&link, ".m3u8") {
        return hls::resolve(client, &link, headers, title)
            .await
            .ok()
            .map(|(s, c, _)| (s, c));
    }
    match direct::check(client, &link, headers).await {
        direct::Outcome::File { stream, choice, .. } => Some((stream, vec![choice])),
        direct::Outcome::Playlist(u) => hls::resolve(client, &u, headers, title)
            .await
            .ok()
            .map(|(s, c, _)| (s, c)),
        _ => None,
    }
}

async fn fetch_page(
    client: &Client,
    url: &str,
    headers: &HeaderMap,
) -> Result<(String, String), DlError> {
    let resp = client
        .get(url)
        .headers(headers.clone())
        .send()
        .await
        .map_err(|e| DlError::from_reqwest(&e))?;
    if !resp.status().is_success() {
        return Err(DlError::from_status(resp.status()));
    }
    let final_url = resp.url().to_string();
    let mut body = resp.bytes_stream();
    let mut bytes = Vec::new();
    while let Some(chunk) = body.next().await {
        let chunk = chunk.map_err(|e| DlError::from_reqwest(&e))?;
        bytes.extend_from_slice(&chunk);
        if bytes.len() >= MAX_PAGE_BYTES {
            break;
        }
    }
    Ok((String::from_utf8_lossy(&bytes).into_owned(), final_url))
}

fn page_title(html: &str) -> Option<String> {
    let raw = TITLE.captures(html)?.get(1)?.as_str();
    let text = unescape_html(raw)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    (!text.is_empty()).then_some(text)
}

fn unescape_html(s: &str) -> String {
    s.replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&#x27;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

/// Links from `<video>`/`<source>` tags first, then media URLs anywhere in the page, including
/// JSON-escaped ones inside scripts. Relative tag links resolve against the page address.
pub fn find_media_links(html: &str, page_url: &str) -> Vec<String> {
    let base = url::Url::parse(page_url).ok();
    let text = html
        .replace("\\/", "/")
        .replace("\\u002F", "/")
        .replace("\\u002f", "/");
    let mut out: Vec<String> = Vec::new();
    let mut push = |raw: &str| {
        let raw = unescape_html(raw.trim());
        if raw.starts_with("blob:") || raw.starts_with("data:") {
            return;
        }
        let resolved = match &base {
            Some(b) => b.join(&raw).ok(),
            None => url::Url::parse(&raw).ok(),
        };
        if let Some(u) = resolved.filter(|u| matches!(u.scheme(), "http" | "https")) {
            let s = u.to_string();
            if !out.contains(&s) {
                out.push(s);
            }
        }
    };
    for c in TAG_SRC.captures_iter(&text) {
        push(&c[1]);
    }
    for m in MEDIA_URL.find_iter(&text) {
        push(m.as_str());
    }
    out.truncate(MAX_CANDIDATES);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_tag_script_and_escaped_links() {
        let html = r#"<html><head><title> Opening &amp; keynote </title></head><body>
            <video controls><source src="/media/talk.mp4" type="video/mp4"></video>
            <video src="blob:https://x"></video>
            <script>var cfg = {"hls":"https:\/\/cdn.test\/v\/master.m3u8?token=a&amp;b=1"};</script>
            <a href="https://cdn.test/v/master.m3u8?token=a&b=1">dup</a>
            <img src="https://cdn.test/poster.jpg">
        </body></html>"#;
        let links = find_media_links(html, "https://www.site.test/talks/opening");
        assert_eq!(
            links,
            vec![
                "https://www.site.test/media/talk.mp4".to_string(),
                "https://cdn.test/v/master.m3u8?token=a&b=1".to_string()
            ]
        );
        assert_eq!(page_title(html).as_deref(), Some("Opening & keynote"));
    }

    #[test]
    fn empty_page_has_no_links() {
        assert!(find_media_links("<html><body>Hello</body></html>", "https://a.test/").is_empty());
    }
}
