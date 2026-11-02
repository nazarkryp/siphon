//! settings.json and downloads.json in the app data folder.

use std::path::{Path, PathBuf};

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::job::StoredDownload;
use crate::model::{DownloadStatus, Settings};

const SETTINGS: &str = "settings.json";
const DOWNLOADS: &str = "downloads.json";

pub fn default_settings(download_folder: PathBuf) -> Settings {
    Settings {
        download_folder: download_folder.display().to_string(),
        max_downloads: 3,
        connections_per_file: 8,
        ffmpeg_path: None,
        yt_dlp_path: None,
        clipboard_watch: true,
    }
}

pub fn load_settings(dir: &Path, default_folder: PathBuf) -> Settings {
    load::<Settings>(&dir.join(SETTINGS))
        .filter(|s| s.validate().is_ok())
        .unwrap_or_else(|| default_settings(default_folder))
}

/// Loads the queue. A download that was running when the app closed comes back paused.
pub fn load_downloads(dir: &Path) -> Vec<StoredDownload> {
    let mut list = load::<Vec<StoredDownload>>(&dir.join(DOWNLOADS)).unwrap_or_default();
    for d in &mut list {
        if matches!(
            d.view.status,
            DownloadStatus::Downloading | DownloadStatus::Merging
        ) {
            d.view.status = DownloadStatus::Paused;
        }
        d.view.speed = 0;
        d.view.eta_seconds = None;
        d.view.merge_progress = None;
    }
    list
}

pub fn save_settings(dir: &Path, settings: &Settings) -> std::io::Result<()> {
    save(&dir.join(SETTINGS), settings)
}

pub fn save_downloads(dir: &Path, list: &[StoredDownload]) -> std::io::Result<()> {
    save(&dir.join(DOWNLOADS), list)
}

/// A missing file gives None. A file that does not parse is kept as `<name>.broken` so no data is lost.
fn load<T: DeserializeOwned>(path: &Path) -> Option<T> {
    let text = std::fs::read_to_string(path).ok()?;
    match serde_json::from_str(&text) {
        Ok(v) => Some(v),
        Err(e) => {
            let backup = path.with_extension("json.broken");
            eprintln!(
                "siphon: {} is not valid ({e}). It is kept as {}.",
                path.display(),
                backup.display()
            );
            let _ = std::fs::rename(path, backup);
            None
        }
    }
}

fn save<T: Serialize + ?Sized>(path: &Path, value: &T) -> std::io::Result<()> {
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(value)?)?;
    std::fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_broken_file_and_uses_defaults() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(SETTINGS), "{ not json").unwrap();
        let s = load_settings(dir.path(), PathBuf::from("D:/Videos"));
        assert_eq!(s.max_downloads, 3);
        assert!(dir.path().join("settings.json.broken").exists());
    }

    #[test]
    fn settings_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = default_settings(PathBuf::from("D:/Videos"));
        s.connections_per_file = 4;
        save_settings(dir.path(), &s).unwrap();
        assert_eq!(load_settings(dir.path(), PathBuf::from("x")), s);
    }
}
