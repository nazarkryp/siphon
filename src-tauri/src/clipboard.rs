//! Clipboard watch: tells the UI when the user copies a link.

use std::sync::Arc;
use std::time::Duration;

use tauri::Emitter;
use tauri_plugin_clipboard_manager::ClipboardExt;

use crate::core::Core;

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
            }
        }
    });
}

/// The copied text counts only when it is one http(s) link and nothing else.
pub fn link_in(text: &str) -> Option<String> {
    let t = text.trim();
    if t.len() > 4096 || t.contains(char::is_whitespace) {
        return None;
    }
    crate::resolve::validate_url(t).ok()
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
}
