//! Clipboard watch: tells the UI when the user copies a link.

use std::sync::Arc;
use std::time::Duration;

use tauri::Emitter;
use tauri_plugin_clipboard_manager::ClipboardExt;

use crate::core::Core;
use crate::{resolve, util};

pub fn start_watcher(core: Arc<Core>) {
    tauri::async_runtime::spawn(async move {
        // The text on the clipboard at start is old. Only new copies count.
        let mut last = core.app().clipboard().read_text().ok();
        loop {
            tokio::time::sleep(Duration::from_secs(1)).await;
            let text = core.app().clipboard().read_text().ok();
            if text == last {
                continue;
            }
            last = text.clone();
            if !core.settings().clipboard_watch {
                continue;
            }
            if let Some(url) = text.as_deref().and_then(link_in)
                && !core.has_source(&url)
            {
                let _ = core
                    .app()
                    .emit("clipboard-link", serde_json::json!({ "url": url }));
                core.prefetch(&url);
            }
        }
    });
}

/// File extensions that make a copied link on any site worth a prompt: video and
/// playlists, audio, archives and disk images, installers and documents.
const FILE_EXTENSIONS: &[&str] = &[
    "mp4", "m4v", "mov", "mkv", "webm", "avi", "wmv", "flv", "m3u8", "mp3", "m4a", "aac", "flac",
    "wav", "ogg", "opus", "zip", "rar", "7z", "tar", "gz", "tgz", "bz2", "xz", "iso", "dmg", "img",
    "exe", "msi", "pkg", "deb", "rpm", "appimage", "apk", "pdf",
];

/// The copied text counts only when it is one http(s) link and nothing else, and the
/// link opens a video page on a known video site or a file with a known extension.
/// Home pages, articles and other pages give no prompt.
pub fn link_in(text: &str) -> Option<String> {
    let t = text.trim();
    if t.len() > 4096 || t.contains(char::is_whitespace) {
        return None;
    }
    let url = resolve::validate_url(t).ok()?;
    let is_file = FILE_EXTENSIONS
        .iter()
        .any(|ext| util::path_ends_with(&url, &format!(".{ext}")));
    (is_file || resolve::is_video_page(&url)).then_some(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_only_a_single_link() {
        assert_eq!(
            link_in("  https://a.test/v.m3u8 \n").as_deref(),
            Some("https://a.test/v.m3u8")
        );
        assert!(link_in("see https://a.test/v").is_none());
        assert!(link_in("hello").is_none());
        assert!(link_in("ftp://a.test/f").is_none());
    }

    #[test]
    fn accepts_only_video_pages_and_files() {
        for url in [
            "https://cdn.test/v/master.m3u8?token=1",
            "https://a.test/files/Movie.MP4",
            "https://a.test/setup.exe",
            "https://a.test/archive.tar.gz",
            "https://www.youtube.com/watch?v=abc",
            "https://www.instagram.com/reel/abc/",
        ] {
            assert!(link_in(url).is_some(), "{url}");
        }
        for url in [
            "https://instagram.com",
            "https://www.youtube.com/",
            "https://github.com/a/b",
            "https://github.com/a/b/blob/main/src/app.ts",
            "https://a.test/article.html",
            "https://a.test/mp4",
        ] {
            assert!(link_in(url).is_none(), "{url}");
        }
    }
}
