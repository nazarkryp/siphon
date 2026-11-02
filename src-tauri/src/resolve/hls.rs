//! Step 2: read an HLS playlist and list its qualities.

use futures_util::StreamExt;
use m3u8_rs::{AlternativeMediaType, MasterPlaylist, VariantStream};
use reqwest::Client;
use reqwest::header::HeaderMap;
use tokio_util::sync::CancellationToken;

use super::{Choice, name_from};
use crate::engine::hls::{build_plan, fetch_text, parse_media_playlist};
use crate::job::{JobSpec, MediaSource, SourceKind};
use crate::model::{DownloadKind, Quality, Stream};
use crate::net::DlError;

/// Variants loaded to read the duration. More are rare and slow the step down.
const MAX_VARIANTS: usize = 12;

pub async fn resolve(
    client: &Client,
    url: &str,
    headers: &HeaderMap,
    name_hint: Option<&str>,
) -> Result<(Stream, Vec<Choice>, String), DlError> {
    let cancel = CancellationToken::new();
    let text = fetch_text(client, url, headers, &cancel).await?;
    if !String::from_utf8_lossy(&text)
        .trim_start()
        .starts_with("#EXTM3U")
    {
        return Err(DlError::fatal(
            "The link does not return an HLS playlist.",
            None,
        ));
    }
    let name = name_from(name_hint, url, "mp4");
    if m3u8_rs::is_master_playlist(&text) {
        let master = m3u8_rs::parse_master_playlist_res(&text)
            .map_err(|_| DlError::fatal("Siphon could not read the HLS master playlist.", None))?;
        from_master(client, url, headers, &master, name).await
    } else {
        let pl = parse_media_playlist(&text)?;
        let plan = build_plan(url, &pl)?;
        let quality = Quality {
            id: "q1".into(),
            label: "Original".into(),
            resolution: None,
            codecs: None,
            bitrate: None,
            size_bytes: None,
            size_is_estimate: false,
            separate_audio: false,
            note: None,
        };
        let choice = Choice {
            kind: DownloadKind::Hls,
            spec: JobSpec {
                primary: MediaSource {
                    kind: SourceKind::Hls,
                    url: url.into(),
                    headers: Vec::new(),
                    label: "Video".into(),
                    size_hint: None,
                },
                audio: None,
                duration: Some(plan.duration),
                encrypted: plan.encrypted,
            },
            quality_label: None,
            output_ext: Some("mp4".into()),
        };
        let summary = format!(
            "A media playlist: {} segments, {}.",
            plan.segments.len(),
            duration_text(plan.duration)
        );
        let stream = Stream {
            id: String::new(),
            kind: DownloadKind::Hls,
            title: "HLS stream".into(),
            url: url.into(),
            duration_seconds: Some(plan.duration),
            encrypted: plan.encrypted,
            qualities: vec![quality],
            suggested_file_name: name,
        };
        Ok((stream, vec![choice], summary))
    }
}

struct Loaded {
    variant: VariantStream,
    media_url: String,
    audio_url: Option<(String, String)>,
    duration: f64,
    encrypted: bool,
}

async fn from_master(
    client: &Client,
    url: &str,
    headers: &HeaderMap,
    master: &MasterPlaylist,
    name: String,
) -> Result<(Stream, Vec<Choice>, String), DlError> {
    let base = url::Url::parse(url)
        .map_err(|_| DlError::fatal("The playlist address is not valid.", None))?;
    let mut variants: Vec<&VariantStream> =
        master.variants.iter().filter(|v| !v.is_i_frame).collect();
    variants.sort_by_key(|v| std::cmp::Reverse(v.bandwidth));
    variants.dedup_by(|a, b| a.uri == b.uri);
    variants.truncate(MAX_VARIANTS);

    let tasks: Vec<_> = variants
        .into_iter()
        .map(|v| load_variant(client, &base, headers, master, v))
        .collect();
    let loaded: Vec<Result<Loaded, DlError>> = futures_util::stream::iter(tasks)
        .buffered(6)
        .collect()
        .await;

    let first_error = loaded.iter().find_map(|r| r.as_ref().err().cloned());
    let mut ok: Vec<Loaded> = loaded.into_iter().filter_map(Result::ok).collect();
    if ok.is_empty() {
        return Err(first_error.unwrap_or_else(|| {
            DlError::fatal("The master playlist has no video variants.", None)
        }));
    }
    ok.sort_by_key(|l| {
        (
            std::cmp::Reverse(l.variant.resolution.as_ref().map_or(0, |r| r.height)),
            std::cmp::Reverse(l.variant.bandwidth),
        )
    });

    let mut qualities = Vec::new();
    let mut choices = Vec::new();
    for (i, l) in ok.iter().enumerate() {
        let v = &l.variant;
        let bitrate = v.average_bandwidth.unwrap_or(v.bandwidth);
        let label = quality_label(v);
        let separate_audio = l.audio_url.is_some();
        qualities.push(Quality {
            id: format!("q{}", i + 1),
            label: label.clone(),
            resolution: v
                .resolution
                .as_ref()
                .map(|r| format!("{}×{}", r.width, r.height)),
            codecs: v.codecs.clone(),
            bitrate: Some(bitrate),
            size_bytes: Some((bitrate as f64 / 8.0 * l.duration) as u64),
            size_is_estimate: true,
            separate_audio,
            note: None,
        });
        let audio = l.audio_url.as_ref().map(|(u, n)| MediaSource {
            kind: SourceKind::Hls,
            url: u.clone(),
            headers: Vec::new(),
            label: format!("Audio · {n}"),
            size_hint: None,
        });
        choices.push(Choice {
            kind: DownloadKind::Hls,
            spec: JobSpec {
                primary: MediaSource {
                    kind: SourceKind::Hls,
                    url: l.media_url.clone(),
                    headers: Vec::new(),
                    label: format!("Video · {label}"),
                    size_hint: Some((bitrate as f64 / 8.0 * l.duration) as u64),
                },
                audio,
                duration: Some(l.duration),
                encrypted: l.encrypted,
            },
            quality_label: Some(label),
            output_ext: Some("mp4".into()),
        });
    }

    let duration = ok.iter().map(|l| l.duration).fold(0.0, f64::max);
    let encrypted = ok.iter().any(|l| l.encrypted);
    let summary = format!(
        "A master playlist with {} qualities, {}.",
        qualities.len(),
        duration_text(duration)
    );
    let stream = Stream {
        id: String::new(),
        kind: DownloadKind::Hls,
        title: "HLS stream".into(),
        url: url.into(),
        duration_seconds: Some(duration),
        encrypted,
        qualities,
        suggested_file_name: name,
    };
    Ok((stream, choices, summary))
}

async fn load_variant(
    client: &Client,
    base: &url::Url,
    headers: &HeaderMap,
    master: &MasterPlaylist,
    v: &VariantStream,
) -> Result<Loaded, DlError> {
    let media_url = base
        .join(&v.uri)
        .map_err(|_| DlError::fatal("A variant address is not valid.", None))?
        .to_string();
    let text = fetch_text(client, &media_url, headers, &CancellationToken::new()).await?;
    let plan = build_plan(&media_url, &parse_media_playlist(&text)?)?;
    let audio_url = audio_rendition(master, v)
        .and_then(|(uri, name)| base.join(&uri).ok().map(|u| (u.to_string(), name)));
    Ok(Loaded {
        variant: v.clone(),
        media_url,
        audio_url,
        duration: plan.duration,
        encrypted: plan.encrypted,
    })
}

/// The audio rendition a variant points to, when the audio is a separate playlist.
fn audio_rendition(master: &MasterPlaylist, v: &VariantStream) -> Option<(String, String)> {
    let group = v.audio.as_deref()?;
    let mut options: Vec<_> = master
        .alternatives
        .iter()
        .filter(|a| {
            a.media_type == AlternativeMediaType::Audio && a.group_id == group && a.uri.is_some()
        })
        .collect();
    options.sort_by_key(|a| std::cmp::Reverse((a.default, a.autoselect)));
    options
        .first()
        .map(|a| (a.uri.clone().unwrap_or_default(), a.name.clone()))
}

fn quality_label(v: &VariantStream) -> String {
    match &v.resolution {
        Some(r) => match v.frame_rate {
            Some(fps) if fps > 31.0 => format!("{}p{}", r.height, fps.round() as u32),
            _ => format!("{}p", r.height),
        },
        None if v.codecs.as_deref().is_some_and(|c| {
            !c.contains("avc")
                && !c.contains("hvc")
                && !c.contains("hev")
                && !c.contains("vp")
                && !c.contains("av01")
        }) =>
        {
            "Audio only".into()
        }
        None => format!("{} kbps", v.bandwidth / 1000),
    }
}

pub fn duration_text(secs: f64) -> String {
    let s = secs.round() as u64;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, s % 3600 / 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net;
    use crate::test_server::{TestServer, TestServerOptions};

    #[tokio::test]
    async fn lists_master_qualities_best_first() {
        let master = "#EXTM3U\n\
#EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID=\"aud\",NAME=\"English\",DEFAULT=YES,URI=\"a/en.m3u8\"\n\
#EXT-X-STREAM-INF:BANDWIDTH=800000,RESOLUTION=640x360,CODECS=\"avc1.4d401e,mp4a.40.2\"\nlow.m3u8\n\
#EXT-X-STREAM-INF:BANDWIDTH=5000000,AVERAGE-BANDWIDTH=4000000,RESOLUTION=1920x1080,FRAME-RATE=60,CODECS=\"avc1.640028\",AUDIO=\"aud\"\nhigh.m3u8\n";
        let media = "#EXTM3U\n#EXT-X-TARGETDURATION:10\n#EXTINF:10.0,\ns0.ts\n#EXTINF:10.0,\ns1.ts\n#EXT-X-ENDLIST\n";
        let server = TestServer::start_routes(
            vec![
                (
                    "/v/master.m3u8",
                    "application/vnd.apple.mpegurl",
                    master.into(),
                ),
                ("/v/low.m3u8", "application/vnd.apple.mpegurl", media.into()),
                (
                    "/v/high.m3u8",
                    "application/vnd.apple.mpegurl",
                    media.into(),
                ),
                (
                    "/v/a/en.m3u8",
                    "application/vnd.apple.mpegurl",
                    media.into(),
                ),
            ],
            TestServerOptions::default(),
        )
        .await;
        let (stream, choices, summary) = resolve(
            &net::build_client(),
            &server.url("/v/master.m3u8"),
            &HeaderMap::new(),
            None,
        )
        .await
        .unwrap();
        assert!(summary.contains("2 qualities"));
        assert_eq!(stream.suggested_file_name, "v.mp4");
        assert_eq!(stream.qualities[0].label, "1080p60");
        assert_eq!(stream.qualities[0].size_bytes, Some(10_000_000));
        assert!(stream.qualities[0].separate_audio);
        assert_eq!(stream.qualities[1].label, "360p");
        assert!(
            choices[0]
                .spec
                .audio
                .as_ref()
                .unwrap()
                .url
                .ends_with("/v/a/en.m3u8")
        );
        assert!(choices[1].spec.audio.is_none());
        assert_eq!(choices[0].spec.duration, Some(20.0));
    }

    #[test]
    fn formats_duration() {
        assert_eq!(duration_text(834.0), "13:54");
        assert_eq!(duration_text(3725.0), "1:02:05");
    }
}
