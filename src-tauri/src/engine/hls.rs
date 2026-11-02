//! HLS download: parallel segments, AES-128 decryption, then one joined file for ffmpeg.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use aes::cipher::{BlockDecryptMut, KeyIvInit, block_padding::Pkcs7};
use futures_util::StreamExt;
use m3u8_rs::{KeyMethod, MediaPlaylist};
use reqwest::Client;
use reqwest::header::{HeaderMap, RANGE};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use super::PartSink;
use crate::job::PartState;
use crate::net::{self, DlError};

type Aes128CbcDec = cbc::Decryptor<aes::Aes128>;

#[derive(Debug, Clone, PartialEq)]
pub struct SegmentRef {
    pub url: String,
    /// (offset, length)
    pub range: Option<(u64, u64)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct KeyRef {
    pub uri: String,
    pub iv: [u8; 16],
}

#[derive(Debug, Clone, PartialEq)]
pub struct Segment {
    pub media: SegmentRef,
    pub key: Option<KeyRef>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    pub segments: Vec<Segment>,
    pub init: Option<SegmentRef>,
    pub duration: f64,
    pub encrypted: bool,
}

pub fn parse_media_playlist(bytes: &[u8]) -> Result<MediaPlaylist, DlError> {
    let text = add_iv_to_method_none(&String::from_utf8_lossy(bytes));
    m3u8_rs::parse_media_playlist_res(text.as_bytes())
        .map_err(|_| DlError::fatal("Siphon could not read the HLS playlist.", None))
}

/// m3u8-rs 6.0.1 rejects `#EXT-X-KEY:METHOD=NONE` without an IV (its check is inverted) and then
/// drops the tag, so encryption would never end. The spec ignores the IV when METHOD is NONE.
fn add_iv_to_method_none(text: &str) -> String {
    text.lines()
        .map(|line| {
            let t = line.trim_end();
            if t.starts_with("#EXT-X-KEY:") && t.contains("METHOD=NONE") && !t.contains("IV=") {
                format!("{t},IV=0x0")
            } else {
                t.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Builds the download plan. Rejects live streams and encryption that Siphon cannot decrypt.
pub fn build_plan(playlist_url: &str, pl: &MediaPlaylist) -> Result<Plan, DlError> {
    if !pl.end_list {
        return Err(DlError::fatal(
            "This is a live stream. Siphon downloads only finished videos.",
            Some("Try again when the broadcast has ended and a recording is available."),
        ));
    }
    let base = url::Url::parse(playlist_url)
        .map_err(|_| DlError::fatal("The playlist address is not valid.", None))?;
    let join = |u: &str| {
        base.join(u)
            .map(|u| u.to_string())
            .map_err(|_| DlError::fatal("A segment address is not valid.", None))
    };

    let mut segments = Vec::with_capacity(pl.segments.len());
    let mut init = None;
    let mut key: Option<(String, Option<[u8; 16]>)> = None;
    let mut encrypted = false;
    let mut next_offset: HashMap<String, u64> = HashMap::new();
    let mut duration = 0.0;

    for (i, seg) in pl.segments.iter().enumerate() {
        if let Some(k) = &seg.key {
            key = match &k.method {
                KeyMethod::None => None,
                KeyMethod::AES128 => {
                    let identity = k.keyformat.as_deref().is_none_or(|f| f == "identity");
                    let uri = k
                        .uri
                        .as_deref()
                        .filter(|_| identity)
                        .ok_or_else(drm_error)?;
                    let iv = match &k.iv {
                        Some(iv) => Some(parse_iv(iv).ok_or_else(|| {
                            DlError::fatal("The playlist has a bad encryption IV.", None)
                        })?),
                        None => None,
                    };
                    encrypted = true;
                    Some((join(uri)?, iv))
                }
                _ => return Err(drm_error()),
            };
        }
        if let Some(map) = &seg.map
            && init.is_none()
        {
            let range = map
                .byte_range
                .as_ref()
                .map(|r| (r.offset.unwrap_or(0), r.length));
            init = Some(SegmentRef {
                url: join(&map.uri)?,
                range,
            });
        }
        let url = join(&seg.uri)?;
        let range = seg.byte_range.as_ref().map(|r| {
            let offset = r
                .offset
                .unwrap_or_else(|| *next_offset.get(&url).unwrap_or(&0));
            (offset, r.length)
        });
        if let Some((offset, len)) = range {
            next_offset.insert(url.clone(), offset + len);
        }
        let seq = pl.media_sequence + i as u64;
        let key = key.as_ref().map(|(uri, iv)| KeyRef {
            uri: uri.clone(),
            iv: iv.unwrap_or_else(|| iv_from_sequence(seq)),
        });
        duration += seg.duration as f64;
        segments.push(Segment {
            media: SegmentRef { url, range },
            key,
        });
    }
    if segments.is_empty() {
        return Err(DlError::fatal("The playlist has no video segments.", None));
    }
    Ok(Plan {
        segments,
        init,
        duration,
        encrypted,
    })
}

fn drm_error() -> DlError {
    DlError::fatal(
        "This stream is protected with DRM. Siphon cannot download it.",
        None,
    )
}

pub fn parse_iv(text: &str) -> Option<[u8; 16]> {
    let hex = text
        .trim()
        .trim_start_matches("0x")
        .trim_start_matches("0X");
    if hex.is_empty() || hex.len() > 32 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let padded = format!("{hex:0>32}");
    let mut out = [0u8; 16];
    for (i, b) in out.iter_mut().enumerate() {
        *b = u8::from_str_radix(&padded[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(out)
}

pub fn iv_from_sequence(seq: u64) -> [u8; 16] {
    (seq as u128).to_be_bytes()
}

pub fn decrypt(data: &[u8], key: &[u8; 16], iv: &[u8; 16]) -> Result<Vec<u8>, DlError> {
    Aes128CbcDec::new(key.into(), iv.into())
        .decrypt_padded_vec_mut::<Pkcs7>(data)
        .map_err(|_| {
            DlError::fatal(
                "Siphon could not decrypt a segment.",
                Some("The key may have changed. Retry, or add the link again."),
            )
        })
}

pub async fn fetch_text(
    client: &Client,
    url: &str,
    headers: &HeaderMap,
    cancel: &CancellationToken,
) -> Result<Vec<u8>, DlError> {
    let mut attempt = 0;
    loop {
        let res = tokio::select! {
            _ = cancel.cancelled() => return Err(DlError::cancelled()),
            r = get_bytes(client, url, None, headers) => r,
        };
        match res {
            Err(e) if e.retryable && attempt + 1 < net::MAX_ATTEMPTS => {
                attempt += 1;
                if !net::backoff(attempt, cancel).await {
                    return Err(DlError::cancelled());
                }
            }
            other => return other,
        }
    }
}

async fn get_bytes(
    client: &Client,
    url: &str,
    range: Option<(u64, u64)>,
    headers: &HeaderMap,
) -> Result<Vec<u8>, DlError> {
    let mut req = client.get(url).headers(headers.clone());
    if let Some((offset, len)) = range {
        req = req.header(RANGE, format!("bytes={}-{}", offset, offset + len - 1));
    }
    let resp = req.send().await.map_err(|e| DlError::from_reqwest(&e))?;
    if !resp.status().is_success() {
        return Err(DlError::from_status(resp.status()));
    }
    resp.bytes()
        .await
        .map(|b| b.to_vec())
        .map_err(|e| DlError::from_reqwest(&e))
}

pub struct HlsOutput {
    /// All segments joined in order. ffmpeg reads this file.
    pub joined: PathBuf,
}

/// Downloads every segment into `work_dir`, then joins them. Finished segments survive a restart.
pub async fn download(
    client: &Client,
    playlist_url: &str,
    headers: &HeaderMap,
    work_dir: &Path,
    connections: u32,
    sink: Arc<dyn PartSink>,
    cancel: &CancellationToken,
) -> Result<HlsOutput, DlError> {
    let text = fetch_text(client, playlist_url, headers, cancel).await?;
    let playlist = parse_media_playlist(&text)?;
    let plan = build_plan(playlist_url, &playlist)?;
    tokio::fs::create_dir_all(work_dir)
        .await
        .map_err(|e| DlError::io(&e))?;

    let count = plan.segments.len();
    let mut states: Vec<u8> = match sink.snapshot() {
        Some(PartState::Hls { states, .. }) if states.len() == count => states.into_bytes(),
        _ => vec![b'0'; count],
    };
    let mut bytes = 0u64;
    for (i, s) in states.iter_mut().enumerate() {
        match std::fs::metadata(segment_path(work_dir, i)) {
            Ok(m) if *s == b'2' => bytes += m.len(),
            _ => *s = b'0',
        }
    }
    let encrypted = plan.encrypted;
    let initial = PartState::Hls {
        states: String::from_utf8(states.clone()).unwrap_or_default(),
        bytes,
        in_flight: Vec::new(),
        encrypted,
    };
    sink.update(&mut |p| *p = Some(initial.clone()));

    let init_path = work_dir.join("init.bin");
    if let Some(init) = &plan.init
        && !init_path.exists()
    {
        let data = fetch_segment(
            client,
            init,
            None,
            headers,
            &Mutex::new(HashMap::new()),
            cancel,
        )
        .await?;
        write_atomic(&init_path, &data).await?;
    }

    let keys: Arc<Mutex<HashMap<String, [u8; 16]>>> = Arc::new(Mutex::new(HashMap::new()));
    let local = cancel.child_token();
    let tasks: Vec<_> = (0..count)
        .filter(|&i| states[i] != b'2')
        .map(|i| {
            run_segment(
                client,
                &plan.segments[i],
                headers,
                &keys,
                &local,
                work_dir,
                &*sink,
                i,
            )
        })
        .collect();
    let results = futures_util::stream::iter(tasks)
        .buffer_unordered(connections.clamp(1, 16) as usize)
        .collect::<Vec<_>>()
        .await;

    if cancel.is_cancelled() {
        return Err(DlError::cancelled());
    }
    let cancelled = DlError::cancelled().message;
    if let Some(err) = results
        .iter()
        .filter_map(|r| r.as_ref().err())
        .find(|e| e.message != cancelled)
        .or_else(|| results.iter().find_map(|r| r.as_ref().err()))
    {
        return Err(err.clone());
    }

    let joined = work_dir.join(if plan.init.is_some() {
        "joined.mp4"
    } else {
        "joined.ts"
    });
    join_segments(
        work_dir,
        count,
        plan.init.as_ref().map(|_| init_path.as_path()),
        &joined,
    )
    .await?;
    Ok(HlsOutput { joined })
}

#[allow(clippy::too_many_arguments)]
async fn run_segment(
    client: &Client,
    seg: &Segment,
    headers: &HeaderMap,
    keys: &Mutex<HashMap<String, [u8; 16]>>,
    local: &CancellationToken,
    work_dir: &Path,
    sink: &dyn PartSink,
    i: usize,
) -> Result<(), DlError> {
    set_in_flight(sink, i as u32, true);
    let res = download_segment(
        client,
        seg,
        headers,
        keys,
        local,
        &segment_path(work_dir, i),
    )
    .await;
    set_in_flight(sink, i as u32, false);
    match res {
        Ok(len) => {
            sink.update(&mut |p| {
                if let Some(PartState::Hls { states, bytes, .. }) = p {
                    states.replace_range(i..=i, "2");
                    *bytes += len;
                }
            });
            Ok(())
        }
        Err(e) => {
            local.cancel();
            Err(e)
        }
    }
}

fn set_in_flight(sink: &dyn PartSink, i: u32, on: bool) {
    sink.update(&mut |p| {
        if let Some(PartState::Hls { in_flight, .. }) = p {
            if on {
                in_flight.push(i);
            } else {
                in_flight.retain(|&x| x != i);
            }
        }
    });
}

pub fn segment_path(dir: &Path, i: usize) -> PathBuf {
    dir.join(format!("seg_{i:06}.bin"))
}

async fn download_segment(
    client: &Client,
    seg: &Segment,
    headers: &HeaderMap,
    keys: &Mutex<HashMap<String, [u8; 16]>>,
    cancel: &CancellationToken,
    path: &Path,
) -> Result<u64, DlError> {
    let mut attempt = 0;
    loop {
        let res = fetch_segment(client, &seg.media, seg.key.as_ref(), headers, keys, cancel).await;
        match res {
            Ok(data) => {
                write_atomic(path, &data).await?;
                return Ok(data.len() as u64);
            }
            Err(e) if e.retryable && attempt + 1 < net::MAX_ATTEMPTS && !cancel.is_cancelled() => {
                attempt += 1;
                if !net::backoff(attempt, cancel).await {
                    return Err(DlError::cancelled());
                }
            }
            Err(e) => return Err(e),
        }
    }
}

async fn fetch_segment(
    client: &Client,
    media: &SegmentRef,
    key: Option<&KeyRef>,
    headers: &HeaderMap,
    keys: &Mutex<HashMap<String, [u8; 16]>>,
    cancel: &CancellationToken,
) -> Result<Vec<u8>, DlError> {
    let data = tokio::select! {
        _ = cancel.cancelled() => return Err(DlError::cancelled()),
        r = get_bytes(client, &media.url, media.range, headers) => r?,
    };
    let Some(key) = key else {
        return Ok(data);
    };
    let key_bytes = {
        let mut cache = keys.lock().await;
        match cache.get(&key.uri) {
            Some(k) => *k,
            None => {
                let raw = get_bytes(client, &key.uri, None, headers)
                    .await
                    .map_err(|e| DlError {
                        message: format!("Siphon could not get the decryption key. {}", e.message),
                        ..e
                    })?;
                let k: [u8; 16] = raw.as_slice().try_into().map_err(|_| drm_error())?;
                cache.insert(key.uri.clone(), k);
                k
            }
        }
    };
    decrypt(&data, &key_bytes, &key.iv)
}

async fn write_atomic(path: &Path, data: &[u8]) -> Result<(), DlError> {
    let tmp = path.with_extension("tmp");
    tokio::fs::write(&tmp, data)
        .await
        .map_err(|e| DlError::io(&e))?;
    tokio::fs::rename(&tmp, path)
        .await
        .map_err(|e| DlError::io(&e))
}

async fn join_segments(
    dir: &Path,
    count: usize,
    init: Option<&Path>,
    out: &Path,
) -> Result<(), DlError> {
    let (dir, init, out) = (
        dir.to_path_buf(),
        init.map(Path::to_path_buf),
        out.to_path_buf(),
    );
    tokio::task::spawn_blocking(move || -> std::io::Result<()> {
        let mut w = std::io::BufWriter::with_capacity(1 << 20, std::fs::File::create(&out)?);
        if let Some(init) = init {
            std::io::copy(&mut std::fs::File::open(init)?, &mut w)?;
        }
        for i in 0..count {
            std::io::copy(&mut std::fs::File::open(segment_path(&dir, i))?, &mut w)?;
        }
        std::io::Write::flush(&mut w)
    })
    .await
    .map_err(|e| DlError::fatal(format!("Joining the segments failed: {e}."), None))?
    .map_err(|e| DlError::io(&e))
}

#[cfg(test)]
mod tests {
    use aes::cipher::BlockEncryptMut;

    use super::*;
    use crate::engine::testing::MemorySink;
    use crate::test_server::{TestServer, TestServerOptions};

    const VOD: &str = "#EXTM3U\n#EXT-X-VERSION:3\n#EXT-X-TARGETDURATION:4\n#EXT-X-MEDIA-SEQUENCE:7\n\
#EXT-X-KEY:METHOD=AES-128,URI=\"key.bin\"\n#EXTINF:4.0,\nseg0.ts\n#EXTINF:4.0,\nseg1.ts\n\
#EXT-X-KEY:METHOD=NONE\n#EXTINF:2.5,\nhttps://cdn.test/abs/seg2.ts\n#EXT-X-ENDLIST\n";

    #[test]
    fn plans_keys_ivs_and_urls() {
        let pl = parse_media_playlist(VOD.as_bytes()).unwrap();
        let plan = build_plan("https://media.test/v/index.m3u8", &pl).unwrap();
        assert_eq!(plan.segments.len(), 3);
        assert!(plan.encrypted);
        assert_eq!(plan.duration, 10.5);
        assert_eq!(plan.segments[0].media.url, "https://media.test/v/seg0.ts");
        let k = plan.segments[1].key.as_ref().unwrap();
        assert_eq!(k.uri, "https://media.test/v/key.bin");
        assert_eq!(k.iv, iv_from_sequence(8));
        assert!(plan.segments[2].key.is_none());
        assert_eq!(plan.segments[2].media.url, "https://cdn.test/abs/seg2.ts");
    }

    #[test]
    fn rejects_live_and_drm() {
        let live = "#EXTM3U\n#EXT-X-TARGETDURATION:4\n#EXTINF:4.0,\na.ts\n";
        let pl = parse_media_playlist(live.as_bytes()).unwrap();
        assert!(
            build_plan("https://x.test/a.m3u8", &pl)
                .unwrap_err()
                .message
                .contains("live")
        );
        let drm = "#EXTM3U\n#EXT-X-TARGETDURATION:4\n#EXT-X-KEY:METHOD=SAMPLE-AES,URI=\"skd://x\",KEYFORMAT=\"com.apple.streamingkeydelivery\"\n#EXTINF:4.0,\na.ts\n#EXT-X-ENDLIST\n";
        let pl = parse_media_playlist(drm.as_bytes()).unwrap();
        assert!(
            build_plan("https://x.test/a.m3u8", &pl)
                .unwrap_err()
                .message
                .contains("DRM")
        );
    }

    #[test]
    fn byte_ranges_follow_each_other() {
        let text = "#EXTM3U\n#EXT-X-TARGETDURATION:4\n#EXTINF:4.0,\n#EXT-X-BYTERANGE:100@0\nall.ts\n#EXTINF:4.0,\n#EXT-X-BYTERANGE:50\nall.ts\n#EXT-X-ENDLIST\n";
        let pl = parse_media_playlist(text.as_bytes()).unwrap();
        let plan = build_plan("https://x.test/a.m3u8", &pl).unwrap();
        assert_eq!(plan.segments[0].media.range, Some((0, 100)));
        assert_eq!(plan.segments[1].media.range, Some((100, 50)));
    }

    #[test]
    fn parses_iv() {
        assert_eq!(
            parse_iv("0x00000000000000000000000000000001").unwrap()[15],
            1
        );
        assert_eq!(parse_iv("0x1F").unwrap()[15], 0x1f);
        assert!(parse_iv("zz").is_none());
    }

    fn encrypt(data: &[u8], key: &[u8; 16], iv: &[u8; 16]) -> Vec<u8> {
        cbc::Encryptor::<aes::Aes128>::new(key.into(), iv.into())
            .encrypt_padded_vec_mut::<Pkcs7>(data)
    }

    #[tokio::test]
    async fn downloads_decrypts_and_joins() {
        let key = [7u8; 16];
        let seg0 = vec![1u8; 5000];
        let seg1 = vec![2u8; 3000];
        let seg2 = vec![3u8; 100];
        let playlist = "#EXTM3U\n#EXT-X-TARGETDURATION:4\n#EXT-X-MEDIA-SEQUENCE:0\n\
#EXT-X-KEY:METHOD=AES-128,URI=\"/key.bin\"\n#EXTINF:4.0,\n/s0.ts\n#EXTINF:4.0,\n/s1.ts\n\
#EXT-X-KEY:METHOD=NONE\n#EXTINF:1.0,\n/s2.ts\n#EXT-X-ENDLIST\n";
        let server = TestServer::start_routes(
            vec![
                (
                    "/index.m3u8",
                    "application/vnd.apple.mpegurl",
                    playlist.as_bytes().to_vec(),
                ),
                ("/key.bin", "application/octet-stream", key.to_vec()),
                (
                    "/s0.ts",
                    "video/mp2t",
                    encrypt(&seg0, &key, &iv_from_sequence(0)),
                ),
                (
                    "/s1.ts",
                    "video/mp2t",
                    encrypt(&seg1, &key, &iv_from_sequence(1)),
                ),
                ("/s2.ts", "video/mp2t", seg2.clone()),
            ],
            TestServerOptions::default(),
        )
        .await;
        let dir = tempfile::tempdir().unwrap();
        let sink = Arc::new(MemorySink::default());
        let out = download(
            &net::build_client(),
            &server.url("/index.m3u8"),
            &HeaderMap::new(),
            dir.path(),
            3,
            sink.clone(),
            &CancellationToken::new(),
        )
        .await
        .unwrap();
        let joined = std::fs::read(&out.joined).unwrap();
        assert_eq!(joined, [seg0, seg1, seg2].concat());
        match sink.snapshot().unwrap() {
            PartState::Hls {
                states,
                encrypted,
                in_flight,
                ..
            } => {
                assert_eq!(states, "222");
                assert!(encrypted);
                assert!(in_flight.is_empty());
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[tokio::test]
    async fn skips_segments_done_before_restart() {
        let playlist = "#EXTM3U\n#EXT-X-TARGETDURATION:4\n#EXTINF:4.0,\n/s0.ts\n#EXTINF:4.0,\n/s1.ts\n#EXT-X-ENDLIST\n";
        let server = TestServer::start_routes(
            vec![
                (
                    "/index.m3u8",
                    "application/vnd.apple.mpegurl",
                    playlist.as_bytes().to_vec(),
                ),
                ("/s1.ts", "video/mp2t", vec![9u8; 10]),
            ],
            TestServerOptions::default(),
        )
        .await;
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(segment_path(dir.path(), 0), [8u8; 4]).unwrap();
        let sink = Arc::new(MemorySink::default());
        *sink.0.lock().unwrap() = Some(PartState::Hls {
            states: "20".into(),
            bytes: 4,
            in_flight: vec![],
            encrypted: false,
        });
        let out = download(
            &net::build_client(),
            &server.url("/index.m3u8"),
            &HeaderMap::new(),
            dir.path(),
            2,
            sink.clone(),
            &CancellationToken::new(),
        )
        .await
        .unwrap();
        assert_eq!(
            std::fs::read(out.joined).unwrap(),
            [vec![8u8; 4], vec![9u8; 10]].concat()
        );
        assert_eq!(sink.snapshot().unwrap().done_bytes(), 14);
    }
}
