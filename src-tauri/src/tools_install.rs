//! Installs yt-dlp and ffmpeg from their official builds and checks the SHA-256 checksums.

use std::io::Read;
use std::path::{Path, PathBuf};

use futures_util::StreamExt;
use reqwest::{Client, RequestBuilder, Response};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ToolId {
    Ffmpeg,
    YtDlp,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Stage {
    Download,
    Verify,
    Extract,
    Done,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub tool: ToolId,
    pub stage: Stage,
    pub done: u64,
    pub total: Option<u64>,
}

/// Where the expected checksum comes from.
#[derive(Clone, Debug)]
pub enum Checksum {
    /// A "SHA2-256SUMS" file with one "<hash>  <name>" line per asset.
    SumsFile { url: String, name: String },
    /// A file that holds only the hash.
    Single { url: String },
}

#[derive(Clone, Debug)]
pub enum Payload {
    /// The download is the program itself.
    Executable { file_name: String },
    /// A zip that holds the programs under some folder; these names are copied out.
    Zip { files: Vec<String> },
}

#[derive(Clone, Debug)]
pub struct Asset {
    pub url: String,
    pub checksum: Checksum,
    pub payload: Payload,
}

pub fn official_asset(tool: ToolId) -> Result<Asset, String> {
    const YT: &str = "https://github.com/yt-dlp/yt-dlp/releases/latest/download";
    match tool {
        ToolId::YtDlp => {
            let (name, file_name) = match std::env::consts::OS {
                "windows" => ("yt-dlp.exe", "yt-dlp.exe"),
                "macos" => ("yt-dlp_macos", "yt-dlp"),
                "linux" => ("yt-dlp_linux", "yt-dlp"),
                os => return Err(format!("Siphon cannot install yt-dlp on {os}.")),
            };
            Ok(Asset {
                url: format!("{YT}/{name}"),
                checksum: Checksum::SumsFile {
                    url: format!("{YT}/SHA2-256SUMS"),
                    name: name.into(),
                },
                payload: Payload::Executable {
                    file_name: file_name.into(),
                },
            })
        }
        ToolId::Ffmpeg => {
            if !(cfg!(windows) && cfg!(target_arch = "x86_64")) {
                return Err("Siphon can install ffmpeg only on 64-bit Windows. Install ffmpeg with your package manager, then set its path in Settings.".into());
            }
            const ZIP: &str = "https://www.gyan.dev/ffmpeg/builds/ffmpeg-release-essentials.zip";
            Ok(Asset {
                url: ZIP.into(),
                checksum: Checksum::Single {
                    url: format!("{ZIP}.sha256"),
                },
                payload: Payload::Zip {
                    files: vec!["ffmpeg.exe".into(), "ffprobe.exe".into()],
                },
            })
        }
    }
}

const YT_DLP_LATEST_URL: &str = "https://github.com/yt-dlp/yt-dlp/releases/latest";
/// gyan.dev publishes its release builds here too. gyan.dev itself blocks an address for
/// 10 minutes after a few requests, and an install needs two of them.
const FFMPEG_LATEST_URL: &str = "https://github.com/GyanD/codexffmpeg/releases/latest";

/// The version of the build that `install` downloads now, for example "9.0.2" or "2026.10.26".
pub async fn latest_version(client: &Client, tool: ToolId) -> Result<String, String> {
    // Siphon checks only the tools it can install on this system.
    official_asset(tool)?;
    let name = tool_slug(tool);
    let url = match tool {
        ToolId::YtDlp => YT_DLP_LATEST_URL,
        ToolId::Ffmpeg => FFMPEG_LATEST_URL,
    };
    let tag = latest_github_tag(client, url)
        .await
        .map_err(|e| format!("Siphon could not check for a newer {name}: {e}."))?;
    parse_release_version(&tag)
        .ok_or_else(|| format!("Siphon could not read the newest {name} version."))
}

/// GitHub redirects ".../releases/latest" to ".../releases/tag/<version>".
/// A HEAD request follows the redirect and reads only the headers.
async fn latest_github_tag(client: &Client, url: &str) -> Result<String, String> {
    let resp = send_ok(client.head(url)).await?;
    Ok(resp
        .url()
        .path()
        .rsplit_once("/tag/")
        .map(|(_, tag)| tag.to_string())
        .unwrap_or_default())
}

async fn send_ok(request: RequestBuilder) -> Result<Response, String> {
    let resp = request.send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status().as_u16()));
    }
    Ok(resp)
}

/// Accepts only numbers with dots, so an error page never counts as a version.
fn parse_release_version(raw: &str) -> Option<String> {
    let v = raw.trim();
    v.split('.')
        .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
        .then(|| v.to_string())
}

/// Downloads, checks and unpacks the asset into `dir`. Returns the path of the first program.
/// The old program stays in place until the new one has passed the check.
pub async fn install(
    client: &Client,
    tool: ToolId,
    asset: &Asset,
    dir: &Path,
    progress: &(dyn Fn(Progress) + Send + Sync),
) -> Result<PathBuf, String> {
    let report = |stage, done, total| {
        progress(Progress {
            tool,
            stage,
            done,
            total,
        })
    };
    tokio::fs::create_dir_all(dir)
        .await
        .map_err(|e| format!("Siphon could not create {}: {e}.", dir.display()))?;
    let expected = expected_hash(client, &asset.checksum).await?;

    let download = dir.join(format!("{}.download", tool_slug(tool)));
    let actual = download_hashing(client, &asset.url, &download, &|done, total| {
        report(Stage::Download, done, total)
    })
    .await?;
    report(Stage::Verify, 0, None);
    if !actual.eq_ignore_ascii_case(&expected) {
        let _ = tokio::fs::remove_file(&download).await;
        return Err(
            "The downloaded file is damaged (its SHA-256 checksum does not match). Try again."
                .into(),
        );
    }

    let result = match &asset.payload {
        Payload::Executable { file_name } => {
            let target = dir.join(file_name);
            replace(&download, &target).await?;
            make_executable(&target)?;
            target
        }
        Payload::Zip { files } => {
            report(Stage::Extract, 0, None);
            let (zip, out, names) = (download.clone(), dir.to_path_buf(), files.clone());
            let extracted = tokio::task::spawn_blocking(move || extract(&zip, &out, &names))
                .await
                .map_err(|e| format!("Unpacking failed: {e}."))?;
            let _ = tokio::fs::remove_file(&download).await;
            extracted?
        }
    };
    report(Stage::Done, 0, None);
    Ok(result)
}

fn tool_slug(tool: ToolId) -> &'static str {
    match tool {
        ToolId::Ffmpeg => "ffmpeg",
        ToolId::YtDlp => "yt-dlp",
    }
}

async fn get_text(client: &Client, url: &str) -> Result<String, String> {
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("Siphon could not download the checksum: {e}."))?;
    if !resp.status().is_success() {
        return Err(format!(
            "Siphon could not download the checksum (HTTP {}).",
            resp.status().as_u16()
        ));
    }
    resp.text()
        .await
        .map_err(|e| format!("Siphon could not read the checksum: {e}."))
}

async fn expected_hash(client: &Client, checksum: &Checksum) -> Result<String, String> {
    let text = match checksum {
        Checksum::SumsFile { url, .. } | Checksum::Single { url } => get_text(client, url).await?,
    };
    let hash = match checksum {
        Checksum::Single { .. } => text.split_whitespace().next().map(str::to_string),
        Checksum::SumsFile { name, .. } => text.lines().find_map(|line| {
            let mut parts = line.split_whitespace();
            let hash = parts.next()?;
            let file = parts.next()?.trim_start_matches('*');
            (file == name).then(|| hash.to_string())
        }),
    };
    hash.filter(|h| h.len() == 64 && h.chars().all(|c| c.is_ascii_hexdigit()))
        .ok_or_else(|| "The checksum file has no valid SHA-256 value for this file.".into())
}

/// Streams the file to disk and returns its SHA-256 as lowercase hex.
async fn download_hashing(
    client: &Client,
    url: &str,
    dest: &Path,
    progress: &(dyn Fn(u64, Option<u64>) + Send + Sync),
) -> Result<String, String> {
    let failed = |e: &dyn std::fmt::Display| format!("The download failed: {e}.");
    let resp = client.get(url).send().await.map_err(|e| failed(&e))?;
    if !resp.status().is_success() {
        return Err(format!(
            "The download failed (HTTP {}).",
            resp.status().as_u16()
        ));
    }
    let total = resp.content_length();
    let mut file = tokio::fs::File::create(dest)
        .await
        .map_err(|e| failed(&e))?;
    let mut hasher = Sha256::new();
    let mut done = 0u64;
    let mut last_report = 0u64;
    let mut body = resp.bytes_stream();
    while let Some(chunk) = body.next().await {
        let chunk = chunk.map_err(|e| failed(&e))?;
        hasher.update(&chunk);
        file.write_all(&chunk).await.map_err(|e| failed(&e))?;
        done += chunk.len() as u64;
        if done - last_report >= 512 * 1024 || Some(done) == total {
            last_report = done;
            progress(done, total);
        }
    }
    file.flush().await.map_err(|e| failed(&e))?;
    Ok(hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

/// Replaces `target` with `new`. Fails clearly when the old program is in use.
async fn replace(new: &Path, target: &Path) -> Result<(), String> {
    if target.exists() {
        tokio::fs::remove_file(target).await.map_err(|e| {
            format!("Siphon could not replace {}: {e}. Wait until running downloads finish, then try again.", target.display())
        })?;
    }
    tokio::fs::rename(new, target)
        .await
        .map_err(|e| format!("Siphon could not save {}: {e}.", target.display()))
}

#[cfg(unix)]
fn make_executable(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
        .map_err(|e| format!("Siphon could not make the file executable: {e}."))
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> Result<(), String> {
    Ok(())
}

/// Copies the named files out of the zip, from whatever folder they are in, into `out`.
fn extract(zip_path: &Path, out: &Path, names: &[String]) -> Result<PathBuf, String> {
    let bad = |e: &dyn std::fmt::Display| format!("Siphon could not unpack the archive: {e}.");
    let file = std::fs::File::open(zip_path).map_err(|e| bad(&e))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| bad(&e))?;
    let mut found: Vec<Option<Vec<u8>>> = vec![None; names.len()];
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| bad(&e))?;
        if entry.is_dir() {
            continue;
        }
        let full = entry.name().map_err(|e| bad(&e))?.to_string();
        let base = full.rsplit(['/', '\\']).next().unwrap_or("").to_string();
        if let Some(slot) = names.iter().position(|n| n.eq_ignore_ascii_case(&base)) {
            let mut data = Vec::with_capacity(entry.size() as usize);
            entry.read_to_end(&mut data).map_err(|e| bad(&e))?;
            found[slot] = Some(data);
        }
    }
    let mut first = None;
    for (name, data) in names.iter().zip(found) {
        let data = data.ok_or_else(|| format!("The archive has no {name}."))?;
        let target = out.join(name);
        let tmp = out.join(format!("{name}.new"));
        std::fs::write(&tmp, data).map_err(|e| bad(&e))?;
        if target.exists() {
            std::fs::remove_file(&target).map_err(|e| {
                format!("Siphon could not replace {}: {e}. Wait until running downloads finish, then try again.", target.display())
            })?;
        }
        std::fs::rename(&tmp, &target).map_err(|e| bad(&e))?;
        first.get_or_insert(target);
    }
    first.ok_or_else(|| "The archive list is empty.".into())
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::sync::Mutex;

    use super::*;
    use crate::net;
    use crate::test_server::{TestServer, TestServerOptions};

    fn sha256_hex(data: &[u8]) -> String {
        Sha256::digest(data)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }

    fn zip_with(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut buf = std::io::Cursor::new(Vec::new());
        let mut w = zip::ZipWriter::new(&mut buf);
        for (name, data) in files {
            w.start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            w.write_all(data).unwrap();
        }
        w.finish().unwrap();
        buf.into_inner()
    }

    #[tokio::test]
    async fn installs_executable_after_checksum() {
        let exe = b"fake yt-dlp".repeat(1000);
        let sums = format!(
            "{}  other\n{}  yt-dlp.exe\n",
            "0".repeat(64),
            sha256_hex(&exe)
        );
        let server = TestServer::start_routes(
            vec![
                ("/yt-dlp.exe", "application/octet-stream", exe.clone()),
                ("/SUMS", "text/plain", sums.into_bytes()),
            ],
            TestServerOptions::default(),
        )
        .await;
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("yt-dlp.exe"), b"old").unwrap();
        let asset = Asset {
            url: server.url("/yt-dlp.exe"),
            checksum: Checksum::SumsFile {
                url: server.url("/SUMS"),
                name: "yt-dlp.exe".into(),
            },
            payload: Payload::Executable {
                file_name: "yt-dlp.exe".into(),
            },
        };
        let stages = Mutex::new(Vec::new());
        let path = install(
            &net::build_client(),
            ToolId::YtDlp,
            &asset,
            dir.path(),
            &|p| stages.lock().unwrap().push(p.stage),
        )
        .await
        .unwrap();
        assert_eq!(std::fs::read(path).unwrap(), exe);
        let stages = stages.into_inner().unwrap();
        assert_eq!(stages.first(), Some(&Stage::Download));
        assert_eq!(stages.last(), Some(&Stage::Done));
        assert!(!dir.path().join("yt-dlp.download").exists());
    }

    #[tokio::test]
    async fn wrong_checksum_keeps_the_old_program() {
        let server = TestServer::start_routes(
            vec![
                (
                    "/tool.exe",
                    "application/octet-stream",
                    b"tampered".to_vec(),
                ),
                (
                    "/tool.sha256",
                    "text/plain",
                    format!("{}\n", "a".repeat(64)).into_bytes(),
                ),
            ],
            TestServerOptions::default(),
        )
        .await;
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("tool.exe"), b"old").unwrap();
        let asset = Asset {
            url: server.url("/tool.exe"),
            checksum: Checksum::Single {
                url: server.url("/tool.sha256"),
            },
            payload: Payload::Executable {
                file_name: "tool.exe".into(),
            },
        };
        let err = install(
            &net::build_client(),
            ToolId::YtDlp,
            &asset,
            dir.path(),
            &|_| {},
        )
        .await
        .unwrap_err();
        assert!(err.contains("checksum"));
        assert_eq!(std::fs::read(dir.path().join("tool.exe")).unwrap(), b"old");
        assert!(!dir.path().join("yt-dlp.download").exists());
    }

    #[tokio::test]
    async fn extracts_programs_from_nested_zip_folder() {
        let zip = zip_with(&[
            ("ffmpeg-9.0.2-essentials_build/README.txt", b"readme"),
            ("ffmpeg-9.0.2-essentials_build/bin/ffmpeg.exe", b"FFMPEG"),
            ("ffmpeg-9.0.2-essentials_build/bin/ffprobe.exe", b"FFPROBE"),
        ]);
        let server = TestServer::start_routes(
            vec![
                ("/f.zip", "application/zip", zip.clone()),
                ("/f.zip.sha256", "text/plain", sha256_hex(&zip).into_bytes()),
            ],
            TestServerOptions::default(),
        )
        .await;
        let dir = tempfile::tempdir().unwrap();
        let asset = Asset {
            url: server.url("/f.zip"),
            checksum: Checksum::Single {
                url: server.url("/f.zip.sha256"),
            },
            payload: Payload::Zip {
                files: vec!["ffmpeg.exe".into(), "ffprobe.exe".into()],
            },
        };
        let path = install(
            &net::build_client(),
            ToolId::Ffmpeg,
            &asset,
            dir.path(),
            &|_| {},
        )
        .await
        .unwrap();
        assert_eq!(path, dir.path().join("ffmpeg.exe"));
        assert_eq!(
            std::fs::read(dir.path().join("ffprobe.exe")).unwrap(),
            b"FFPROBE"
        );
        assert!(!dir.path().join("ffmpeg.download").exists());
    }

    #[tokio::test]
    async fn missing_file_in_zip_is_an_error() {
        let zip = zip_with(&[("x/bin/ffmpeg.exe", b"F")]);
        let server = TestServer::start_routes(
            vec![
                ("/f.zip", "application/zip", zip.clone()),
                ("/f.sha256", "text/plain", sha256_hex(&zip).into_bytes()),
            ],
            TestServerOptions::default(),
        )
        .await;
        let dir = tempfile::tempdir().unwrap();
        let asset = Asset {
            url: server.url("/f.zip"),
            checksum: Checksum::Single {
                url: server.url("/f.sha256"),
            },
            payload: Payload::Zip {
                files: vec!["ffmpeg.exe".into(), "ffprobe.exe".into()],
            },
        };
        let err = install(
            &net::build_client(),
            ToolId::Ffmpeg,
            &asset,
            dir.path(),
            &|_| {},
        )
        .await
        .unwrap_err();
        assert!(err.contains("ffprobe.exe"));
    }

    #[test]
    fn official_assets_point_to_checksummed_files() {
        let yt = official_asset(ToolId::YtDlp).unwrap();
        assert!(
            yt.url
                .starts_with("https://github.com/yt-dlp/yt-dlp/releases/latest/download/")
        );
        assert!(matches!(yt.checksum, Checksum::SumsFile { .. }));
        if cfg!(all(windows, target_arch = "x86_64")) {
            let ff = official_asset(ToolId::Ffmpeg).unwrap();
            assert!(ff.url.starts_with("https://www.gyan.dev/"));
        }
    }

    #[test]
    fn release_version_is_numbers_with_dots() {
        assert_eq!(
            parse_release_version(
                "9.0.2
"
            )
            .as_deref(),
            Some("9.0.2")
        );
        assert_eq!(
            parse_release_version("2026.10.26").as_deref(),
            Some("2026.10.26")
        );
        assert_eq!(
            parse_release_version("2026.10.26.1").as_deref(),
            Some("2026.10.26.1")
        );
        assert_eq!(parse_release_version(""), None);
        assert_eq!(parse_release_version("9..2"), None);
        assert_eq!(parse_release_version("2026-10-25-git-46f23be8b1"), None);
        assert_eq!(parse_release_version("<html>Not found</html>"), None);
    }

    /// Reads the newest versions from GitHub. Run with `cargo test -- --ignored`.
    #[tokio::test]
    #[ignore]
    async fn reads_real_latest_versions() {
        let client = net::build_client();
        let yt = latest_version(&client, ToolId::YtDlp).await.unwrap();
        assert!(yt.starts_with("20"), "{yt}");
        if cfg!(all(windows, target_arch = "x86_64")) {
            latest_version(&client, ToolId::Ffmpeg).await.unwrap();
        }
    }

    /// Downloads the real yt-dlp release (about 18 MB). Run with `cargo test -- --ignored`.
    #[tokio::test]
    #[ignore = "uses the network"]
    async fn installs_real_yt_dlp() {
        let dir = tempfile::tempdir().unwrap();
        let asset = official_asset(ToolId::YtDlp).unwrap();
        let path = install(
            &net::build_client(),
            ToolId::YtDlp,
            &asset,
            dir.path(),
            &|_| {},
        )
        .await
        .unwrap();
        let out = crate::tools::command(&path)
            .arg("--version")
            .output()
            .await
            .unwrap();
        assert!(out.status.success());
        assert!(!String::from_utf8_lossy(&out.stdout).trim().is_empty());
    }
}
