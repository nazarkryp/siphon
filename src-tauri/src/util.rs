use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::time::{SystemTime, UNIX_EPOCH};

use regex::Regex;

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub fn format_bytes(n: u64) -> String {
    const UNITS: [&str; 4] = ["KB", "MB", "GB", "TB"];
    if n < 1024 {
        return format!("{n} B");
    }
    let mut v = n as f64 / 1024.0;
    let mut unit = 0;
    while v >= 1024.0 && unit < UNITS.len() - 1 {
        v /= 1024.0;
        unit += 1;
    }
    format!("{v:.1} {}", UNITS[unit])
}

pub fn host_of(url: &str) -> String {
    url::Url::parse(url)
        .ok()
        .and_then(|u| {
            u.host_str()
                .map(|h| h.trim_start_matches("www.").to_string())
        })
        .unwrap_or_default()
}

/// Makes a name safe for Windows, macOS and Linux file systems.
pub fn sanitize_file_name(name: &str) -> String {
    const RESERVED: [&str; 22] = [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    let cleaned: String = name
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    let mut cleaned = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    cleaned = cleaned
        .trim_matches(|c: char| c == '.' || c == ' ')
        .to_string();
    if cleaned.chars().count() > 150 {
        let (stem, ext) = split_ext(&cleaned);
        let stem: String = stem.chars().take(140).collect();
        cleaned = format!("{}{}", stem.trim_end(), ext);
    }
    if cleaned.is_empty() {
        return "download".into();
    }
    let (stem, _) = split_ext(&cleaned);
    if RESERVED.iter().any(|r| r.eq_ignore_ascii_case(stem)) {
        cleaned.insert(0, '_');
    }
    cleaned
}

/// Splits "a.b.mp4" into ("a.b", ".mp4"). A name without an extension gives ("name", "").
pub fn split_ext(name: &str) -> (&str, &str) {
    match name.rfind('.') {
        Some(i) if i > 0 && name.len() - i <= 6 => (&name[..i], &name[i..]),
        _ => (name, ""),
    }
}

pub fn with_extension(name: &str, ext: &str) -> String {
    let (stem, _) = split_ext(name);
    format!("{stem}.{ext}")
}

/// Returns `folder/name`, or `folder/name (n)` when the path exists or `taken` says it is in use.
pub fn unique_path(folder: &Path, name: &str, taken: &dyn Fn(&Path) -> bool) -> PathBuf {
    let first = folder.join(name);
    if !first.exists() && !taken(&first) {
        return first;
    }
    let (stem, ext) = split_ext(name);
    for n in 1..10_000 {
        let candidate = folder.join(format!("{stem} ({n}){ext}"));
        if !candidate.exists() && !taken(&candidate) {
            return candidate;
        }
    }
    folder.join(format!("{stem} ({}){ext}", now_ms()))
}

static CD_EXT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?i)filename\*\s*=\s*([\w-]+)'[^']*'([^;]+)"#).unwrap());
static CD_PLAIN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?i)filename\s*=\s*(?:"([^"]*)"|([^;]+))"#).unwrap());

pub fn file_name_from_disposition(header: &str) -> Option<String> {
    if let Some(c) = CD_EXT.captures(header) {
        let decoded = percent_encoding::percent_decode_str(c[2].trim()).decode_utf8_lossy();
        if !decoded.trim().is_empty() {
            return Some(decoded.trim().to_string());
        }
    }
    let c = CD_PLAIN.captures(header)?;
    let raw = c.get(1).or_else(|| c.get(2))?.as_str().trim();
    (!raw.is_empty()).then(|| raw.to_string())
}

pub fn file_name_from_url(url: &str) -> Option<String> {
    let parsed = url::Url::parse(url).ok()?;
    let last = parsed.path_segments()?.rfind(|s| !s.is_empty())?;
    let decoded = percent_encoding::percent_decode_str(last)
        .decode_utf8_lossy()
        .to_string();
    (!decoded.is_empty()).then_some(decoded)
}

pub fn path_ends_with(url: &str, ext: &str) -> bool {
    url::Url::parse(url)
        .map(|u| u.path().to_ascii_lowercase().ends_with(ext))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizes_bad_characters_and_reserved_names() {
        assert_eq!(sanitize_file_name("a/b:c*?.mp4"), "a_b_c__.mp4");
        assert_eq!(sanitize_file_name("  ..name..  "), "name");
        assert_eq!(sanitize_file_name("CON.txt"), "_CON.txt");
        assert_eq!(sanitize_file_name(""), "download");
        let long = format!("{}.mp4", "x".repeat(300));
        let s = sanitize_file_name(&long);
        assert!(s.ends_with(".mp4") && s.chars().count() <= 150);
    }

    #[test]
    fn reads_content_disposition() {
        assert_eq!(
            file_name_from_disposition(r#"attachment; filename="report final.pdf""#).as_deref(),
            Some("report final.pdf")
        );
        assert_eq!(
            file_name_from_disposition("attachment; filename*=UTF-8''%D1%84%D0%B0%D0%B9%D0%BB.zip")
                .as_deref(),
            Some("файл.zip")
        );
        assert_eq!(file_name_from_disposition("inline"), None);
    }

    #[test]
    fn reads_name_from_url() {
        assert_eq!(
            file_name_from_url("https://x.test/a/b/my%20video.mp4?t=1").as_deref(),
            Some("my video.mp4")
        );
        assert_eq!(file_name_from_url("https://x.test/"), None);
    }

    #[test]
    fn finds_free_name() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.mp4"), b"x").unwrap();
        let p = unique_path(dir.path(), "a.mp4", &|_| false);
        assert_eq!(p.file_name().unwrap(), "a (1).mp4");
        let taken = dir.path().join("a (1).mp4");
        let p = unique_path(dir.path(), "a.mp4", &|q| q == taken);
        assert_eq!(p.file_name().unwrap(), "a (2).mp4");
    }
}
