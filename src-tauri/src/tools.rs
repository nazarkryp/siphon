//! External programs: ffmpeg (required for HLS and for merging) and yt-dlp (optional resolver).

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

use crate::model::{Settings, ToolStatus, ToolsStatus};
use crate::net::DlError;

/// A command that does not open a console window on Windows.
pub fn command(program: &Path) -> Command {
    let mut cmd = Command::new(program);
    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd.stdin(Stdio::null()).kill_on_drop(true);
    cmd
}

pub async fn detect(settings: &Settings) -> ToolsStatus {
    let (ffmpeg, yt_dlp) = tokio::join!(
        detect_one(settings.ffmpeg_path.as_deref(), "ffmpeg", &["-version"]),
        detect_one(settings.yt_dlp_path.as_deref(), "yt-dlp", &["--version"]),
    );
    ToolsStatus { ffmpeg, yt_dlp }
}

async fn detect_one(configured: Option<&str>, name: &str, version_args: &[&str]) -> ToolStatus {
    let path = match configured.map(str::trim).filter(|s| !s.is_empty()) {
        Some(p) => Some(PathBuf::from(p)).filter(|p| p.is_file()),
        None => which::which(name).ok(),
    };
    let Some(path) = path else {
        return ToolStatus::default();
    };
    let output = tokio::time::timeout(
        Duration::from_secs(15),
        command(&path)
            .args(version_args)
            .stderr(Stdio::null())
            .output(),
    )
    .await;
    let (version, dev_build) = match output {
        Ok(Ok(out)) if out.status.success() => parse_version(&String::from_utf8_lossy(&out.stdout)),
        _ => {
            return ToolStatus {
                found: false,
                path: Some(path.display().to_string()),
                ..ToolStatus::default()
            };
        }
    };
    ToolStatus {
        found: true,
        path: Some(path.display().to_string()),
        version,
        dev_build,
    }
}

/// Returns the short version and whether it is a development build.
/// - "ffmpeg version 8.1.2-full_build-www.gyan.dev ..." gives ("8.1.2", false).
/// - "ffmpeg version N-124294-g739fc9249c-20260501 ..." gives ("N-124294", true).
/// - "ffmpeg version 2026-05-01-git-1a2b3c-full_build ..." gives ("2026-05-01-git", true).
/// - yt-dlp prints only the version, for example "2026.09.30".
fn parse_version(stdout: &str) -> (Option<String>, bool) {
    let Some(first) = stdout.lines().next().map(str::trim) else {
        return (None, false);
    };
    let raw = match first.find("version ") {
        Some(i) => first[i + 8..].split_whitespace().next(),
        None => first.split_whitespace().next(),
    };
    let Some(raw) = raw else {
        return (None, false);
    };
    if raw.starts_with("N-") {
        return (
            Some(raw.split('-').take(2).collect::<Vec<_>>().join("-")),
            true,
        );
    }
    if let Some(i) = raw.find("-git") {
        return (Some(raw[..i + 4].to_string()), true);
    }
    let short = raw.split(['-', '_']).next().unwrap_or(raw);
    ((!short.is_empty()).then(|| short.to_string()), false)
}

/// Most error lines that a healthy video may print in the checked seconds. Some streams print a
/// few decoder warnings at the start. A broken track prints many lines for every frame.
const MAX_DECODE_ERRORS: usize = 20;
const CHECK_SECONDS: &str = "10";

/// Decodes the first seconds of the video track in `file`. A file without video passes.
/// Finds broken output, for example from an ffmpeg build whose MP4 writer has a fault.
pub async fn check_output(
    ffmpeg: &Path,
    file: &Path,
    cancel: &CancellationToken,
) -> Result<(), DlError> {
    let run = command(ffmpeg)
        .args(["-hide_banner", "-nostdin", "-v", "error", "-i"])
        .arg(file)
        .args(["-map", "0:v:0?", "-t", CHECK_SECONDS, "-f", "null", "-"])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output();
    let out = tokio::select! {
        _ = cancel.cancelled() => return Err(DlError::cancelled()),
        r = run => r.map_err(|e| DlError::fatal(format!("Siphon could not start ffmpeg to check the file: {e}."), None))?,
    };
    let errors = String::from_utf8_lossy(&out.stderr)
        .lines()
        .filter(|l| !l.trim().is_empty())
        .count();
    if out.status.success() && errors <= MAX_DECODE_ERRORS {
        return Ok(());
    }
    Err(DlError::fatal(
        "ffmpeg made a broken video file.",
        Some(
            "Your ffmpeg build may have a fault. Open Settings, click Update in the ffmpeg row to install the release build, then retry.",
        ),
    ))
}

/// Runs ffmpeg with `-progress pipe:1` and reports 0..1 when the media duration is known.
pub async fn run_ffmpeg(
    ffmpeg: &Path,
    args: &[String],
    duration_secs: Option<f64>,
    cancel: &CancellationToken,
    mut on_progress: impl FnMut(f64),
) -> Result<(), DlError> {
    let mut child = command(ffmpeg)
        .args([
            "-hide_banner",
            "-nostdin",
            "-y",
            "-loglevel",
            "error",
            "-progress",
            "pipe:1",
            "-nostats",
        ])
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            DlError::fatal(
                format!("Siphon could not start ffmpeg: {e}."),
                Some("Check the ffmpeg path in Settings."),
            )
        })?;

    let stdout = child.stdout.take().expect("stdout is piped");
    let mut stderr = child.stderr.take().expect("stderr is piped");
    let stderr_task = tokio::spawn(async move {
        let mut text = String::new();
        let _ = stderr.read_to_string(&mut text).await;
        text
    });

    let mut lines = BufReader::new(stdout).lines();
    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                let _ = child.kill().await;
                return Err(DlError::cancelled());
            }
            line = lines.next_line() => match line {
                Ok(Some(line)) => {
                    if let (Some(us), Some(total)) = (line.strip_prefix("out_time_us="), duration_secs)
                        && let Ok(us) = us.trim().parse::<f64>()
                            && total > 0.0 {
                                on_progress((us / 1_000_000.0 / total).clamp(0.0, 1.0));
                            }
                }
                _ => break,
            }
        }
    }

    let status = child
        .wait()
        .await
        .map_err(|e| DlError::fatal(format!("ffmpeg stopped unexpectedly: {e}."), None))?;
    let stderr_text = stderr_task.await.unwrap_or_default();
    if status.success() {
        return Ok(());
    }
    let last = stderr_text
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("no error text");
    Err(DlError::fatal(
        format!("ffmpeg could not build the file: {}", last.trim()),
        Some("Retry. If it fails again, choose another quality."),
    ))
}

/// Output arguments that write `out` with copied streams (no re-encode).
pub fn copy_output_args(out: &Path) -> Vec<String> {
    let mut args = vec!["-c".to_string(), "copy".to_string()];
    let ext = out
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if matches!(ext.as_str(), "mp4" | "m4a" | "mov" | "m4v") {
        args.extend(["-movflags".to_string(), "+faststart".to_string()]);
    }
    args.push(out.display().to_string());
    args
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_versions() {
        let v = |s: &str| {
            let (v, dev) = parse_version(s);
            (v.unwrap_or_default(), dev)
        };
        assert_eq!(
            v("ffmpeg version 8.1.2-full_build-www.gyan.dev Copyright (c) 2000-2026\n"),
            ("8.1.2".into(), false)
        );
        assert_eq!(
            v("ffmpeg version N-124294-g739fc9249c-20260501 Copyright\n"),
            ("N-124294".into(), true)
        );
        assert_eq!(
            v("ffmpeg version 2026-05-01-git-1a2b3c4d5e-full_build-www.gyan.dev\n"),
            ("2026-05-01-git".into(), true)
        );
        assert_eq!(v("2026.09.30\n"), ("2026.09.30".into(), false));
        assert_eq!(parse_version(""), (None, false));
    }

    #[test]
    fn faststart_only_for_mp4_family() {
        assert!(copy_output_args(Path::new("a.mp4")).contains(&"+faststart".to_string()));
        assert!(!copy_output_args(Path::new("a.mkv")).contains(&"+faststart".to_string()));
    }
}
