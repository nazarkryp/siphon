use std::time::Duration;

use reqwest::header::{HeaderMap, HeaderName, HeaderValue, REFERER, USER_AGENT};
use reqwest::{Client, StatusCode};
use tokio_util::sync::CancellationToken;

use crate::model::DownloadError;

pub const DEFAULT_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0.0.0 Safari/537.36";

/// Attempts for one chunk or one segment before the download fails.
pub const MAX_ATTEMPTS: u32 = 6;

#[derive(Debug, Clone)]
pub struct DlError {
    pub message: String,
    pub hint: Option<String>,
    pub retryable: bool,
    /// The saved progress is not valid anymore. The next attempt starts from zero.
    pub restart: bool,
}

impl DlError {
    pub fn fatal(message: impl Into<String>, hint: Option<&str>) -> Self {
        Self {
            message: message.into(),
            hint: hint.map(Into::into),
            retryable: false,
            restart: false,
        }
    }

    pub fn retryable(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            hint: None,
            retryable: true,
            restart: false,
        }
    }

    pub fn restart(message: impl Into<String>) -> Self {
        Self {
            restart: true,
            ..Self::fatal(
                message,
                Some("Retry. Siphon then downloads the file from the start."),
            )
        }
    }

    pub fn cancelled() -> Self {
        Self::fatal("Stopped.", None)
    }

    pub fn from_status(status: StatusCode) -> Self {
        let code = status.as_u16();
        match code {
            401 | 403 => Self::fatal(
                format!("The server refused the request (HTTP {code})."),
                Some(
                    "The server may need the page address as Referer, or a login. Add the Referer in the link options, then retry.",
                ),
            ),
            404 | 410 => Self::fatal(
                format!("The file is not on the server (HTTP {code})."),
                Some("The link may have expired. Add the page link again."),
            ),
            416 => Self::restart("The server rejected the byte range (HTTP 416)."),
            429 | 500..=599 => Self::retryable(format!("The server is busy (HTTP {code}).")),
            _ => Self::fatal(format!("The server answered HTTP {code}."), None),
        }
    }

    pub fn from_reqwest(err: &reqwest::Error) -> Self {
        if let Some(status) = err.status() {
            return Self::from_status(status);
        }
        let what = if err.is_timeout() {
            "The connection timed out."
        } else if err.is_connect() {
            "Siphon could not connect to the server."
        } else {
            "The connection failed."
        };
        Self::retryable(what)
    }

    pub fn io(err: &std::io::Error) -> Self {
        Self::fatal(
            format!("Siphon could not write the file: {err}."),
            Some(
                "Check that the folder exists, that you can write to it, and that the disk has free space.",
            ),
        )
    }

    pub fn into_view(self) -> DownloadError {
        DownloadError {
            message: self.message,
            hint: self.hint,
        }
    }
}

pub fn build_client() -> Client {
    Client::builder()
        .connect_timeout(Duration::from_secs(20))
        .read_timeout(Duration::from_secs(60))
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()
        .expect("HTTP client configuration is static and valid")
}

/// Request headers for one download: the source headers (from yt-dlp), then the user's overrides.
pub fn request_headers(
    base: &[(String, String)],
    referer: Option<&str>,
    user_agent: Option<&str>,
) -> HeaderMap {
    let mut map = HeaderMap::new();
    for (k, v) in base {
        if let (Ok(name), Ok(value)) = (HeaderName::try_from(k.as_str()), HeaderValue::from_str(v))
        {
            map.insert(name, value);
        }
    }
    if !map.contains_key(USER_AGENT) {
        map.insert(USER_AGENT, HeaderValue::from_static(DEFAULT_USER_AGENT));
    }
    if let Some(ua) = user_agent.filter(|s| !s.trim().is_empty())
        && let Ok(v) = HeaderValue::from_str(ua.trim())
    {
        map.insert(USER_AGENT, v);
    }
    if let Some(r) = referer.filter(|s| !s.trim().is_empty())
        && let Ok(v) = HeaderValue::from_str(r.trim())
    {
        map.insert(REFERER, v);
    }
    map
}

/// Waits before the next attempt: 1, 2, 4, 8, 16 seconds. Returns false when the job was stopped.
pub async fn backoff(attempt: u32, cancel: &CancellationToken) -> bool {
    let secs = 1u64 << attempt.saturating_sub(1).min(4);
    tokio::select! {
        _ = cancel.cancelled() => false,
        _ = tokio::time::sleep(Duration::from_secs(secs)) => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_overrides_win_over_source_headers() {
        let base = vec![
            ("User-Agent".to_string(), "yt".to_string()),
            ("Accept".to_string(), "*/*".to_string()),
        ];
        let h = request_headers(&base, Some("https://page.test/"), Some("mine"));
        assert_eq!(h[USER_AGENT], "mine");
        assert_eq!(h[REFERER], "https://page.test/");
        assert_eq!(h["accept"], "*/*");
        let h = request_headers(&[], None, Some("  "));
        assert_eq!(h[USER_AGENT], DEFAULT_USER_AGENT);
    }

    #[test]
    fn maps_status_codes() {
        assert!(!DlError::from_status(StatusCode::FORBIDDEN).retryable);
        assert!(
            DlError::from_status(StatusCode::FORBIDDEN)
                .hint
                .unwrap()
                .contains("Referer")
        );
        assert!(DlError::from_status(StatusCode::SERVICE_UNAVAILABLE).retryable);
        assert!(DlError::from_status(StatusCode::TOO_MANY_REQUESTS).retryable);
    }
}
