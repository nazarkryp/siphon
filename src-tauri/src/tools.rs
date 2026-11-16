//! External programs: ffmpeg (required for HLS and for merging) and yt-dlp (optional resolver).

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

use crate::model::{Settings, ToolStatus, ToolsStatus};
use crate::net::DlError;
use crate::tools_install::{self, ToolId};

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
    ToolsStatus {
        ffmpeg: ToolStatus {
            installable: tools_install::can_install(ToolId::Ffmpeg),
            ..ffmpeg
        },
        yt_dlp: ToolStatus {
            installable: tools_install::can_install(ToolId::YtDlp),
            ..yt_dlp
        },
    }
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
        installable: false,
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

/// How ffmpeg changes the tracks when the user saves a video in a compatible format.
#[derive(Clone, Debug, PartialEq)]
pub struct Recode {
    /// None copies the video track.
    pub video: Option<VideoEncode>,
    /// True encodes the audio to AAC. False copies it.
    pub audio: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct VideoEncode {
    pub encoder: &'static str,
    /// Bits per second.
    pub bitrate: u64,
}

const AAC_BITRATE: &str = "192k";

/// Output arguments that write `out`. Without `recode`, ffmpeg copies the video and the audio.
/// With `subtitles`, the text subtitle track is converted to the format that the container
/// accepts.
pub fn output_args(out: &Path, subtitles: bool, recode: Option<&Recode>) -> Vec<String> {
    let ext = out
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let mut args: Vec<String> = Vec::new();
    match recode.and_then(|r| r.video.as_ref()) {
        Some(v) => {
            let rate = |bits: u64| bits.to_string();
            args.extend(
                [
                    "-c:v",
                    v.encoder,
                    "-b:v",
                    &rate(v.bitrate),
                    "-maxrate",
                    &rate(v.bitrate * 3 / 2),
                    "-bufsize",
                    &rate(v.bitrate * 2),
                    "-pix_fmt",
                    pixel_format(v.encoder),
                    "-profile:v",
                    "high",
                    "-tag:v",
                    "avc1",
                ]
                .map(String::from),
            );
            if v.encoder == "libx264" {
                args.extend(["-preset".into(), "fast".into()]);
            }
        }
        None => args.extend(["-c:v".into(), "copy".into()]),
    }
    if recode.is_some_and(|r| r.audio) {
        args.extend(["-c:a", "aac", "-b:a", AAC_BITRATE].map(String::from));
    } else {
        args.extend(["-c:a".into(), "copy".into()]);
    }
    if subtitles {
        let subtitle_codec = match ext.as_str() {
            "mp4" | "m4v" | "mov" => "mov_text",
            "webm" => "webvtt",
            _ => "srt",
        };
        args.extend(["-c:s".into(), subtitle_codec.into()]);
    }
    if matches!(ext.as_str(), "mp4" | "m4a" | "mov" | "m4v") {
        args.extend(["-movflags".to_string(), "+faststart".to_string()]);
    }
    args.push(out.display().to_string());
    args
}

/// 8-bit 4:2:0, which every H.264 player decodes. Hardware encoders take it as `nv12`.
fn pixel_format(encoder: &str) -> &'static str {
    if encoder == "libx264" {
        "yuv420p"
    } else {
        "nv12"
    }
}

/// H.264 encoders in the order Siphon tries them. A hardware encoder is fast, but it works only
/// when the computer has the hardware and the ffmpeg build includes the encoder.
fn h264_encoders() -> &'static [&'static str] {
    if cfg!(target_os = "macos") {
        &["h264_videotoolbox", "libx264"]
    } else if cfg!(windows) {
        &["h264_nvenc", "h264_qsv", "h264_amf", "libx264"]
    } else {
        &["libx264"]
    }
}

type EncoderKey = (PathBuf, Option<std::time::SystemTime>);

/// The encoder found for each ffmpeg file. An update of the file changes its time and so the key.
fn encoder_cache() -> &'static std::sync::Mutex<std::collections::HashMap<EncoderKey, &'static str>>
{
    static CACHE: std::sync::OnceLock<
        std::sync::Mutex<std::collections::HashMap<EncoderKey, &'static str>>,
    > = std::sync::OnceLock::new();
    CACHE.get_or_init(Default::default)
}

/// The first H.264 encoder that works with this ffmpeg on this computer. A short test encode
/// checks each one, because ffmpeg lists hardware encoders that the computer cannot use.
pub async fn h264_encoder(
    ffmpeg: &Path,
    cancel: &CancellationToken,
) -> Result<&'static str, DlError> {
    let modified = std::fs::metadata(ffmpeg).and_then(|m| m.modified()).ok();
    let key = (ffmpeg.to_path_buf(), modified);
    let cached = encoder_cache()
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .get(&key)
        .copied();
    if let Some(encoder) = cached {
        return Ok(encoder);
    }
    for &encoder in h264_encoders() {
        let test = command(ffmpeg)
            .args([
                "-hide_banner",
                "-nostdin",
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
            ])
            .arg("color=c=black:s=256x256:r=30:d=1")
            .args(["-frames:v", "5", "-c:v", encoder, "-pix_fmt"])
            .arg(pixel_format(encoder))
            .args(["-f", "null", "-"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        let works = tokio::select! {
            _ = cancel.cancelled() => return Err(DlError::cancelled()),
            r = tokio::time::timeout(Duration::from_secs(20), test) => {
                matches!(r, Ok(Ok(status)) if status.success())
            }
        };
        if works {
            encoder_cache()
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .insert(key, encoder);
            return Ok(encoder);
        }
    }
    Err(DlError::fatal(
        "Your ffmpeg cannot make H.264 video.",
        Some(
            "Open Settings and click Update in the ffmpeg row to install the release build. Or turn off \"Save in a compatible format\" for this video.",
        ),
    ))
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
        assert!(output_args(Path::new("a.mp4"), false, None).contains(&"+faststart".to_string()));
        assert!(!output_args(Path::new("a.mkv"), false, None).contains(&"+faststart".to_string()));
    }

    #[test]
    fn subtitles_get_the_container_format() {
        let codec = |name: &str| {
            let args = output_args(Path::new(name), true, None);
            let i = args.iter().position(|a| a == "-c:s").unwrap();
            assert!(
                !args.contains(&"-c".to_string()),
                "-c copy would override -c:s"
            );
            args[i + 1].clone()
        };
        assert_eq!(codec("a.mp4"), "mov_text");
        assert_eq!(codec("a.webm"), "webvtt");
        assert_eq!(codec("a.mkv"), "srt");
    }

    fn value_after<'a>(args: &'a [String], flag: &str) -> &'a str {
        let i = args.iter().position(|a| a == flag).unwrap();
        &args[i + 1]
    }

    #[test]
    fn without_recode_copies_both_tracks() {
        let args = output_args(Path::new("a.mp4"), false, None);
        assert_eq!(value_after(&args, "-c:v"), "copy");
        assert_eq!(value_after(&args, "-c:a"), "copy");
    }

    #[test]
    fn recode_changes_only_the_named_tracks() {
        let video_only = Recode {
            video: Some(VideoEncode {
                encoder: "h264_videotoolbox",
                bitrate: 2_000_000,
            }),
            audio: false,
        };
        let args = output_args(Path::new("a.mp4"), false, Some(&video_only));
        assert_eq!(value_after(&args, "-c:v"), "h264_videotoolbox");
        assert_eq!(value_after(&args, "-b:v"), "2000000");
        assert_eq!(value_after(&args, "-maxrate"), "3000000");
        assert_eq!(value_after(&args, "-pix_fmt"), "nv12");
        assert_eq!(value_after(&args, "-tag:v"), "avc1");
        assert_eq!(value_after(&args, "-c:a"), "copy");
        assert!(!args.contains(&"-preset".to_string()));

        let audio_only = Recode {
            video: None,
            audio: true,
        };
        let args = output_args(Path::new("a.mp4"), true, Some(&audio_only));
        assert_eq!(value_after(&args, "-c:v"), "copy");
        assert_eq!(value_after(&args, "-c:a"), "aac");
        assert_eq!(value_after(&args, "-c:s"), "mov_text");
        assert_eq!(args.last().unwrap(), "a.mp4");
    }

    #[test]
    fn libx264_uses_yuv420p_and_a_preset() {
        let recode = Recode {
            video: Some(VideoEncode {
                encoder: "libx264",
                bitrate: 1_000_000,
            }),
            audio: true,
        };
        let args = output_args(Path::new("a.mp4"), false, Some(&recode));
        assert_eq!(value_after(&args, "-pix_fmt"), "yuv420p");
        assert_eq!(value_after(&args, "-preset"), "fast");
    }

    #[test]
    fn software_encoder_is_the_last_choice() {
        assert_eq!(h264_encoders().last(), Some(&"libx264"));
    }
}
