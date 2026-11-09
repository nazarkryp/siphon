//! Shapes shared with the frontend (src/api/types.ts). Keep both files in sync.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DownloadKind {
    File,
    Hls,
    Ytdlp,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DownloadStatus {
    Queued,
    Downloading,
    Paused,
    Merging,
    Done,
    Failed,
    NeedsFfmpeg,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ByteProgress {
    pub done: u64,
    pub total: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ProgressDetail {
    None,
    Chunks {
        chunks: Vec<ByteProgress>,
    },
    Segments {
        total: u32,
        done: u32,
        in_flight: u32,
        /// One character per segment: '0' waiting, '1' in flight, '2' done.
        states: String,
        encrypted: bool,
    },
    Streams {
        video: ByteProgress,
        audio: ByteProgress,
        video_label: String,
        audio_label: String,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadError {
    pub message: String,
    pub hint: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Download {
    pub id: String,
    pub source_url: String,
    pub host: String,
    pub file_name: String,
    pub folder: String,
    pub kind: DownloadKind,
    pub quality_label: Option<String>,
    pub status: DownloadStatus,
    pub total_bytes: Option<u64>,
    pub total_is_estimate: bool,
    pub done_bytes: u64,
    pub speed: u64,
    pub eta_seconds: Option<u64>,
    pub merge_progress: Option<f64>,
    pub detail: ProgressDetail,
    pub error: Option<DownloadError>,
    pub referer: Option<String>,
    pub user_agent: Option<String>,
    pub created_at: u64,
    pub finished_at: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ResolveStepId {
    Direct,
    Hls,
    Ytdlp,
    Page,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ResolveStepState {
    Found,
    NotFound,
    Skipped,
    Error,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolveStep {
    pub id: ResolveStepId,
    pub state: ResolveStepState,
    pub message: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Quality {
    pub id: String,
    pub label: String,
    pub resolution: Option<String>,
    pub codecs: Option<String>,
    pub bitrate: Option<u64>,
    pub size_bytes: Option<u64>,
    pub size_is_estimate: bool,
    pub separate_audio: bool,
    pub note: Option<String>,
    /// The saved file opens in the default players everywhere (QuickTime, Films & TV).
    pub plays_everywhere: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Stream {
    pub id: String,
    pub kind: DownloadKind,
    pub title: String,
    pub url: String,
    pub duration_seconds: Option<f64>,
    pub encrypted: bool,
    pub qualities: Vec<Quality>,
    pub suggested_file_name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolveResult {
    pub resolve_id: String,
    pub url: String,
    pub steps: Vec<ResolveStep>,
    pub streams: Vec<Stream>,
    pub free_space_bytes: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddRequest {
    pub resolve_id: String,
    pub stream_id: String,
    pub quality_id: String,
    pub file_name: String,
    pub folder: String,
    pub referer: Option<String>,
    pub user_agent: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FailedLink {
    pub url: String,
    pub message: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddLinksResult {
    pub added: Vec<Download>,
    pub failed: Vec<FailedLink>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub download_folder: String,
    pub max_downloads: u32,
    pub connections_per_file: u32,
    pub ffmpeg_path: Option<String>,
    pub yt_dlp_path: Option<String>,
    pub clipboard_watch: bool,
}

impl Settings {
    pub const MAX_DOWNLOADS: std::ops::RangeInclusive<u32> = 1..=10;
    pub const CONNECTIONS: std::ops::RangeInclusive<u32> = 1..=16;

    pub fn validate(&self) -> Result<(), String> {
        if self.download_folder.trim().is_empty() {
            return Err("Choose a download folder.".into());
        }
        if !Self::MAX_DOWNLOADS.contains(&self.max_downloads) {
            return Err("Downloads at the same time must be from 1 to 10.".into());
        }
        if !Self::CONNECTIONS.contains(&self.connections_per_file) {
            return Err("Connections per file must be from 1 to 16.".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolStatus {
    pub found: bool,
    pub path: Option<String>,
    pub version: Option<String>,
    /// A development (nightly or git) build, not a release.
    pub dev_build: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolsStatus {
    pub ffmpeg: ToolStatus,
    pub yt_dlp: ToolStatus,
}

/// The newest release of a tool, or why Siphon could not read it.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolUpdate {
    pub latest: Option<String>,
    pub error: Option<String>,
}

impl From<Result<String, String>> for ToolUpdate {
    fn from(r: Result<String, String>) -> Self {
        match r {
            Ok(latest) => Self {
                latest: Some(latest),
                error: None,
            },
            Err(error) => Self {
                latest: None,
                error: Some(error),
            },
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolUpdates {
    pub ffmpeg: ToolUpdate,
    pub yt_dlp: ToolUpdate,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppState {
    pub downloads: Vec<Download>,
    pub settings: Settings,
    /// None until the first tool detection ends.
    pub tools: Option<ToolsStatus>,
}
