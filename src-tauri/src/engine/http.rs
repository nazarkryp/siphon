//! Chunked HTTP download with resume: parallel `Range` requests into one `.part` file.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use futures_util::StreamExt;
use reqwest::header::{CONTENT_RANGE, ETAG, HeaderMap, LAST_MODIFIED, RANGE};
use reqwest::{Client, StatusCode};
use tokio::io::{AsyncSeekExt, AsyncWriteExt};
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

use super::PartSink;
use crate::job::{Chunk, PartState};
use crate::net::{self, DlError};

/// Smallest chunk worth its own connection.
const MIN_CHUNK: u64 = 1024 * 1024;
/// Bytes collected before one write to disk.
const WRITE_BUFFER: usize = 256 * 1024;

#[derive(Debug, Clone, PartialEq)]
pub struct Probe {
    pub total: Option<u64>,
    pub ranged: bool,
    pub validator: Option<String>,
}

pub async fn download(
    client: &Client,
    url: &str,
    headers: &HeaderMap,
    dest: &Path,
    connections: u32,
    sink: Arc<dyn PartSink>,
    cancel: &CancellationToken,
) -> Result<(), DlError> {
    let probe = probe_with_retry(client, url, headers, cancel).await?;
    let resumable = match sink.snapshot() {
        Some(PartState::Http {
            total,
            ranged: true,
            validator,
            chunks,
        }) if probe.ranged
            && total == probe.total
            && validator == probe.validator
            && dest.exists() =>
        {
            Some(chunks)
        }
        _ => None,
    };

    let chunks = match resumable {
        Some(chunks) => chunks,
        None => {
            let chunks = plan_chunks(probe.total, probe.ranged, connections);
            prepare_file(dest, probe.total).await?;
            let state = PartState::Http {
                total: probe.total,
                ranged: probe.ranged,
                validator: probe.validator.clone(),
                chunks: chunks.clone(),
            };
            sink.update(&mut |p| *p = Some(state.clone()));
            chunks
        }
    };

    let local = cancel.child_token();
    let mut tasks = JoinSet::new();
    for (idx, chunk) in chunks.into_iter().enumerate() {
        if chunk.complete() {
            continue;
        }
        let job = ChunkJob {
            client: client.clone(),
            url: url.to_string(),
            headers: headers.clone(),
            path: dest.to_path_buf(),
            idx,
            chunk,
            ranged: probe.ranged,
            sink: sink.clone(),
            cancel: local.clone(),
        };
        tasks.spawn(job.run());
    }

    let mut first_error: Option<DlError> = None;
    while let Some(res) = tasks.join_next().await {
        let res = res.unwrap_or_else(|e| {
            Err(DlError::fatal(
                format!("A download task failed: {e}."),
                None,
            ))
        });
        if let Err(e) = res {
            local.cancel();
            if first_error.is_none()
                || first_error
                    .as_ref()
                    .is_some_and(|f| f.message == DlError::cancelled().message)
            {
                first_error = Some(e);
            }
        }
    }
    if cancel.is_cancelled() {
        return Err(DlError::cancelled());
    }
    if let Some(e) = first_error {
        if e.restart {
            sink.update(&mut |p| *p = None);
        }
        return Err(e);
    }
    let file = tokio::fs::OpenOptions::new()
        .write(true)
        .open(dest)
        .await
        .map_err(|e| DlError::io(&e))?;
    file.sync_all().await.map_err(|e| DlError::io(&e))?;
    Ok(())
}

/// Asks for the first byte. A 206 answer with a total size means the server accepts ranges.
pub async fn probe(
    client: &Client,
    url: &str,
    headers: &HeaderMap,
) -> Result<(Probe, reqwest::Response), DlError> {
    let resp = client
        .get(url)
        .headers(headers.clone())
        .header(RANGE, "bytes=0-0")
        .send()
        .await
        .map_err(|e| DlError::from_reqwest(&e))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(DlError::from_status(status));
    }
    let validator = resp
        .headers()
        .get(ETAG)
        .or_else(|| resp.headers().get(LAST_MODIFIED))
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let probe = if status == StatusCode::PARTIAL_CONTENT {
        let total = resp
            .headers()
            .get(CONTENT_RANGE)
            .and_then(|v| v.to_str().ok())
            .and_then(total_from_content_range);
        Probe {
            total,
            ranged: total.is_some(),
            validator,
        }
    } else {
        Probe {
            total: resp.content_length(),
            ranged: false,
            validator,
        }
    };
    Ok((probe, resp))
}

async fn probe_with_retry(
    client: &Client,
    url: &str,
    headers: &HeaderMap,
    cancel: &CancellationToken,
) -> Result<Probe, DlError> {
    let mut attempt = 0;
    loop {
        let result = tokio::select! {
            _ = cancel.cancelled() => return Err(DlError::cancelled()),
            r = probe(client, url, headers) => r,
        };
        match result {
            Ok((probe, _resp)) => return Ok(probe),
            Err(e) if e.retryable && attempt + 1 < net::MAX_ATTEMPTS => {
                attempt += 1;
                if !net::backoff(attempt, cancel).await {
                    return Err(DlError::cancelled());
                }
            }
            Err(e) => return Err(e),
        }
    }
}

/// "bytes 0-0/12345" gives 12345. "bytes 0-0/*" gives None.
pub fn total_from_content_range(value: &str) -> Option<u64> {
    value.rsplit('/').next()?.trim().parse().ok()
}

pub fn plan_chunks(total: Option<u64>, ranged: bool, connections: u32) -> Vec<Chunk> {
    let Some(total) = total.filter(|_| ranged) else {
        return vec![Chunk {
            start: 0,
            end: total.map(|t| t.saturating_sub(1)),
            done: 0,
        }];
    };
    if total == 0 {
        return Vec::new();
    }
    let count = (connections.max(1) as u64)
        .min(total.div_ceil(MIN_CHUNK))
        .max(1);
    let size = total / count;
    (0..count)
        .map(|i| {
            let start = i * size;
            let end = if i == count - 1 {
                total - 1
            } else {
                start + size - 1
            };
            Chunk {
                start,
                end: Some(end),
                done: 0,
            }
        })
        .collect()
}

async fn prepare_file(dest: &Path, total: Option<u64>) -> Result<(), DlError> {
    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| DlError::io(&e))?;
    }
    let file = tokio::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(dest)
        .await
        .map_err(|e| DlError::io(&e))?;
    if let Some(total) = total {
        file.set_len(total).await.map_err(|e| DlError::io(&e))?;
    }
    Ok(())
}

struct ChunkJob {
    client: Client,
    url: String,
    headers: HeaderMap,
    path: PathBuf,
    idx: usize,
    chunk: Chunk,
    ranged: bool,
    sink: Arc<dyn PartSink>,
    cancel: CancellationToken,
}

impl ChunkJob {
    async fn run(mut self) -> Result<(), DlError> {
        let mut attempt = 0;
        loop {
            let before = self.chunk.done;
            match self.fetch().await {
                Ok(()) => return Ok(()),
                Err(e) if e.retryable && !self.cancel.is_cancelled() => {
                    if self.chunk.done > before {
                        attempt = 0;
                    }
                    attempt += 1;
                    if attempt >= net::MAX_ATTEMPTS {
                        return Err(e);
                    }
                    if !net::backoff(attempt, &self.cancel).await {
                        return Err(DlError::cancelled());
                    }
                }
                Err(e) => return Err(e),
            }
        }
    }

    async fn fetch(&mut self) -> Result<(), DlError> {
        if !self.ranged && self.chunk.done > 0 {
            // Without ranges the server can only send the file from the start.
            self.chunk.done = 0;
            self.report();
        }
        let start = self.chunk.start + self.chunk.done;
        let mut req = self.client.get(&self.url).headers(self.headers.clone());
        if self.ranged {
            let end = self.chunk.end.expect("a ranged chunk has an end");
            req = req.header(RANGE, format!("bytes={start}-{end}"));
        }
        let resp = tokio::select! {
            _ = self.cancel.cancelled() => return Err(DlError::cancelled()),
            r = req.send() => r.map_err(|e| DlError::from_reqwest(&e))?,
        };
        let status = resp.status();
        if !status.is_success() {
            return Err(DlError::from_status(status));
        }
        if self.ranged && status != StatusCode::PARTIAL_CONTENT {
            return Err(DlError::restart(
                "The server stopped accepting byte ranges.",
            ));
        }

        let mut file = tokio::fs::OpenOptions::new()
            .write(true)
            .open(&self.path)
            .await
            .map_err(|e| DlError::io(&e))?;
        file.seek(std::io::SeekFrom::Start(start))
            .await
            .map_err(|e| DlError::io(&e))?;

        let remaining = |c: &Chunk| c.len().map(|l| l - c.done);
        let mut body = resp.bytes_stream();
        let mut buf: Vec<u8> = Vec::with_capacity(WRITE_BUFFER);
        loop {
            let next = tokio::select! {
                _ = self.cancel.cancelled() => {
                    self.flush(&mut file, &mut buf).await?;
                    return Err(DlError::cancelled());
                }
                n = body.next() => n,
            };
            match next {
                Some(Ok(bytes)) => {
                    let take = match remaining(&self.chunk) {
                        Some(r) => (bytes.len() as u64).min(r - buf.len() as u64) as usize,
                        None => bytes.len(),
                    };
                    buf.extend_from_slice(&bytes[..take]);
                    if buf.len() >= WRITE_BUFFER {
                        self.flush(&mut file, &mut buf).await?;
                    }
                    if remaining(&self.chunk).is_some_and(|r| r == buf.len() as u64) {
                        break;
                    }
                }
                Some(Err(e)) => {
                    self.flush(&mut file, &mut buf).await?;
                    return Err(DlError::from_reqwest(&e));
                }
                None => break,
            }
        }
        self.flush(&mut file, &mut buf).await?;
        if remaining(&self.chunk).is_some_and(|r| r > 0) {
            return Err(DlError::retryable(
                "The connection closed before the end of the data.",
            ));
        }
        Ok(())
    }

    async fn flush(
        &mut self,
        file: &mut tokio::fs::File,
        buf: &mut Vec<u8>,
    ) -> Result<(), DlError> {
        if buf.is_empty() {
            return Ok(());
        }
        file.write_all(buf).await.map_err(|e| DlError::io(&e))?;
        file.flush().await.map_err(|e| DlError::io(&e))?;
        self.chunk.done += buf.len() as u64;
        buf.clear();
        self.report();
        Ok(())
    }

    fn report(&self) {
        let (idx, done) = (self.idx, self.chunk.done);
        self.sink.update(&mut |p| {
            if let Some(PartState::Http { chunks, .. }) = p
                && let Some(c) = chunks.get_mut(idx)
            {
                c.done = done;
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::testing::MemorySink;
    use crate::test_server::{TestServer, TestServerOptions};

    #[test]
    fn plans_equal_chunks() {
        let chunks = plan_chunks(Some(10 * MIN_CHUNK + 5), true, 4);
        assert_eq!(chunks.len(), 4);
        assert_eq!(chunks[0].start, 0);
        assert_eq!(chunks.last().unwrap().end, Some(10 * MIN_CHUNK + 4));
        let covered: u64 = chunks.iter().map(|c| c.len().unwrap()).sum();
        assert_eq!(covered, 10 * MIN_CHUNK + 5);
    }

    #[test]
    fn small_or_unranged_files_use_one_chunk() {
        assert_eq!(plan_chunks(Some(1000), true, 8).len(), 1);
        let c = plan_chunks(Some(5000), false, 8);
        assert_eq!(
            c,
            vec![Chunk {
                start: 0,
                end: Some(4999),
                done: 0
            }]
        );
        assert_eq!(plan_chunks(None, false, 8)[0].end, None);
        assert!(plan_chunks(Some(0), true, 8).is_empty());
    }

    #[test]
    fn reads_content_range_total() {
        assert_eq!(total_from_content_range("bytes 0-0/12345"), Some(12345));
        assert_eq!(total_from_content_range("bytes 0-0/*"), None);
    }

    fn body(len: usize) -> Vec<u8> {
        (0..len).map(|i| (i * 31 % 251) as u8).collect()
    }

    #[tokio::test]
    async fn downloads_in_parallel_ranges() {
        let data = body(5 * MIN_CHUNK as usize + 123);
        let server = TestServer::start(data.clone(), TestServerOptions::default()).await;
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("f.part");
        let sink = Arc::new(MemorySink::default());
        download(
            &net::build_client(),
            &server.url("/f.bin"),
            &HeaderMap::new(),
            &dest,
            4,
            sink.clone(),
            &CancellationToken::new(),
        )
        .await
        .unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), data);
        match sink.snapshot().unwrap() {
            PartState::Http { chunks, ranged, .. } => {
                assert!(ranged);
                assert_eq!(chunks.len(), 4);
                assert!(chunks.iter().all(Chunk::complete));
            }
            other => panic!("unexpected {other:?}"),
        }
        assert!(server.max_parallel() >= 2, "expected parallel requests");
    }

    #[tokio::test]
    async fn resumes_after_stop() {
        let data = body(3 * MIN_CHUNK as usize);
        let server = TestServer::start(
            data.clone(),
            TestServerOptions {
                delay_per_chunk_ms: 2,
                ..Default::default()
            },
        )
        .await;
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("f.part");
        let sink = Arc::new(MemorySink::default());
        let client = net::build_client();
        let url = server.url("/f.bin");

        let cancel = CancellationToken::new();
        let stopper = cancel.clone();
        let sink_watch = sink.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
                if sink_watch
                    .snapshot()
                    .is_some_and(|p| p.done_bytes() > 300_000)
                {
                    stopper.cancel();
                    break;
                }
            }
        });
        let err = download(
            &client,
            &url,
            &HeaderMap::new(),
            &dest,
            3,
            sink.clone(),
            &cancel,
        )
        .await
        .unwrap_err();
        assert_eq!(err.message, DlError::cancelled().message);
        let partial = sink.snapshot().unwrap().done_bytes();
        assert!(partial > 0 && partial < data.len() as u64);

        let served_before = server.bytes_served();
        download(
            &client,
            &url,
            &HeaderMap::new(),
            &dest,
            3,
            sink.clone(),
            &CancellationToken::new(),
        )
        .await
        .unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), data);
        let served_after = server.bytes_served() - served_before;
        assert!(
            served_after < data.len() as u64,
            "resume must not download everything again"
        );
    }

    #[tokio::test]
    async fn single_connection_when_server_ignores_ranges() {
        let data = body(2 * MIN_CHUNK as usize);
        let server = TestServer::start(
            data.clone(),
            TestServerOptions {
                ranges: false,
                ..Default::default()
            },
        )
        .await;
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("f.part");
        let sink = Arc::new(MemorySink::default());
        download(
            &net::build_client(),
            &server.url("/f.bin"),
            &HeaderMap::new(),
            &dest,
            8,
            sink.clone(),
            &CancellationToken::new(),
        )
        .await
        .unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), data);
        assert!(matches!(
            sink.snapshot().unwrap(),
            PartState::Http { ranged: false, .. }
        ));
    }

    #[tokio::test]
    async fn forbidden_gives_referer_hint() {
        let server = TestServer::start(
            body(10),
            TestServerOptions {
                status: Some(403),
                ..Default::default()
            },
        )
        .await;
        let dir = tempfile::tempdir().unwrap();
        let err = download(
            &net::build_client(),
            &server.url("/f.bin"),
            &HeaderMap::new(),
            &dir.path().join("f.part"),
            2,
            Arc::new(MemorySink::default()),
            &CancellationToken::new(),
        )
        .await
        .unwrap_err();
        assert!(err.hint.unwrap().contains("Referer"));
    }
}
