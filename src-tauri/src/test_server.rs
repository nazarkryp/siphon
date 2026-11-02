//! A small HTTP/1.1 server for engine tests. Supports `Range` requests.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

#[derive(Clone)]
pub struct TestServerOptions {
    pub ranges: bool,
    pub status: Option<u16>,
    pub delay_per_chunk_ms: u64,
}

impl Default for TestServerOptions {
    fn default() -> Self {
        Self {
            ranges: true,
            status: None,
            delay_per_chunk_ms: 0,
        }
    }
}

#[derive(Clone)]
pub struct Route {
    pub content_type: String,
    pub body: Vec<u8>,
}

struct Shared {
    routes: HashMap<String, Route>,
    fallback: Option<Route>,
    opts: TestServerOptions,
    active: AtomicUsize,
    max_active: AtomicUsize,
    served: AtomicU64,
}

pub struct TestServer {
    port: u16,
    shared: Arc<Shared>,
}

impl TestServer {
    /// Serves `body` on every path.
    pub async fn start(body: Vec<u8>, opts: TestServerOptions) -> Self {
        let route = Route {
            content_type: "application/octet-stream".into(),
            body,
        };
        Self::start_inner(HashMap::new(), Some(route), opts).await
    }

    pub async fn start_routes(routes: Vec<(&str, &str, Vec<u8>)>, opts: TestServerOptions) -> Self {
        let routes = routes
            .into_iter()
            .map(|(p, ct, b)| {
                (
                    p.to_string(),
                    Route {
                        content_type: ct.to_string(),
                        body: b,
                    },
                )
            })
            .collect();
        Self::start_inner(routes, None, opts).await
    }

    async fn start_inner(
        routes: HashMap<String, Route>,
        fallback: Option<Route>,
        opts: TestServerOptions,
    ) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let shared = Arc::new(Shared {
            routes,
            fallback,
            opts,
            active: AtomicUsize::new(0),
            max_active: AtomicUsize::new(0),
            served: AtomicU64::new(0),
        });
        let s = shared.clone();
        tokio::spawn(async move {
            while let Ok((sock, _)) = listener.accept().await {
                let s = s.clone();
                tokio::spawn(async move {
                    let n = s.active.fetch_add(1, Ordering::SeqCst) + 1;
                    s.max_active.fetch_max(n, Ordering::SeqCst);
                    let _ = handle(sock, &s).await;
                    s.active.fetch_sub(1, Ordering::SeqCst);
                });
            }
        });
        Self { port, shared }
    }

    pub fn url(&self, path: &str) -> String {
        format!("http://127.0.0.1:{}{}", self.port, path)
    }

    pub fn max_parallel(&self) -> usize {
        self.shared.max_active.load(Ordering::SeqCst)
    }

    pub fn bytes_served(&self) -> u64 {
        self.shared.served.load(Ordering::SeqCst)
    }
}

async fn handle(mut sock: TcpStream, s: &Shared) -> std::io::Result<()> {
    let mut head = Vec::new();
    let mut byte = [0u8; 1];
    while !head.ends_with(b"\r\n\r\n") {
        if sock.read(&mut byte).await? == 0 {
            return Ok(());
        }
        head.push(byte[0]);
    }
    let head = String::from_utf8_lossy(&head);
    let path = head
        .split_whitespace()
        .nth(1)
        .unwrap_or("/")
        .split('?')
        .next()
        .unwrap_or("/")
        .to_string();
    let range = head
        .lines()
        .find(|l| l.to_ascii_lowercase().starts_with("range:"))
        .and_then(|l| l.split_once('=').map(|(_, r)| r.trim().to_string()));

    if let Some(code) = s.opts.status {
        let resp =
            format!("HTTP/1.1 {code} Error\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
        return sock.write_all(resp.as_bytes()).await;
    }
    let Some(route) = s.routes.get(&path).or(s.fallback.as_ref()) else {
        return sock
            .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
            .await;
    };
    let total = route.body.len() as u64;
    let (status, start, end) = match range.filter(|_| s.opts.ranges && total > 0) {
        Some(r) => {
            let (a, b) = r.split_once('-').unwrap_or((r.as_str(), ""));
            let a: u64 = a.parse().unwrap_or(0);
            let b: u64 = b.parse().unwrap_or(total - 1).min(total - 1);
            ("206 Partial Content", a, b)
        }
        None => ("200 OK", 0, total.saturating_sub(1)),
    };
    let body: &[u8] = if total == 0 {
        &[]
    } else {
        &route.body[start as usize..=end as usize]
    };
    let mut resp = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\nETag: \"v1\"\r\n",
        route.content_type,
        body.len()
    );
    if s.opts.ranges {
        resp.push_str("Accept-Ranges: bytes\r\n");
    }
    if status.starts_with("206") {
        resp.push_str(&format!("Content-Range: bytes {start}-{end}/{total}\r\n"));
    }
    resp.push_str("\r\n");
    sock.write_all(resp.as_bytes()).await?;
    for piece in body.chunks(16 * 1024) {
        sock.write_all(piece).await?;
        s.served.fetch_add(piece.len() as u64, Ordering::SeqCst);
        if s.opts.delay_per_chunk_ms > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(s.opts.delay_per_chunk_ms)).await;
        }
    }
    sock.shutdown().await
}
