//! `mk-media` — a loopback HTTP server that streams a `VfsBackend` file to
//! VLC (or any Range-aware client) without copying it to disk first.
//!
//! The server binds `127.0.0.1`, requires a per-session token in the URL (so a
//! DNS-rebinding page or another local process can't read the user's files),
//! and serves byte ranges by `open_read` + `seek` on the backend — never by
//! buffering the whole file. See the streaming spike in `docs/spikes.md`.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::body::{Body, Bytes};
use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use futures_util::Stream;
use mk_core::host::Host;
use mk_vfs::{ConnectionPool, ReadStream};

/// Resolves a host id to its `Host` (protocol/address/creds), so the server
/// can route a stream URL to the right backend via the connection pool.
pub type HostResolver = Arc<dyn Fn(&str) -> Option<Host> + Send + Sync>;

/// Read chunk size for the streaming body (independent of the backend's own
/// transfer chunk).
const CHUNK: usize = 64 * 1024;

/// Parse an HTTP `Range: bytes=...` header into an inclusive `(start, end)`
/// clamped to `[0, size-1]`. `Ok(None)` means "no Range header present";
/// `Err(RangeError)` means the range was malformed or unsatisfiable (callers
/// answer `416`).
pub fn parse_range(header: Option<&str>, size: u64) -> Result<Option<(u64, u64)>, RangeError> {
    let Some(h) = header else { return Ok(None) };
    let spec = h.trim().strip_prefix("bytes=").ok_or(RangeError)?;
    // Multiple ranges ("bytes=0-1,5-9") aren't supported; VLC doesn't send them.
    let spec = spec.split(',').next().unwrap_or(spec);
    let (a, b) = spec.split_once('-').ok_or(RangeError)?;
    let (start, end) = match (a.is_empty(), b.is_empty()) {
        // bytes=-N : last N bytes
        (true, false) => {
            let n: u64 = b.parse().map_err(|_| RangeError)?;
            if n == 0 {
                return Err(RangeError);
            }
            (size.saturating_sub(n), size - 1)
        }
        // bytes=N- : from N to EOF
        (false, true) => {
            let s: u64 = a.parse().map_err(|_| RangeError)?;
            (s, size - 1)
        }
        // bytes=N-M : closed range
        (false, false) => {
            let s: u64 = a.parse().map_err(|_| RangeError)?;
            let e: u64 = b.parse().map_err(|_| RangeError)?;
            (s, e.min(size - 1))
        }
        (true, true) => return Err(RangeError),
    };
    if start >= size {
        return Err(RangeError); // unsatisfiable
    }
    Ok(Some((start, end.min(size - 1))))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RangeError;

impl std::fmt::Display for RangeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("invalid or unsatisfiable byte range")
    }
}

impl std::error::Error for RangeError {}

/// The streaming server. Cheap to clone via `router()`; each instance is a
/// router sharing one token + pool + resolver.
pub struct MediaServer {
    pool: Arc<ConnectionPool>,
    resolve: HostResolver,
    token: String,
}

#[derive(Clone)]
struct Inner {
    pool: Arc<ConnectionPool>,
    resolve: HostResolver,
    token: String,
}

impl MediaServer {
    pub fn new(pool: Arc<ConnectionPool>, resolve: HostResolver) -> Self {
        MediaServer {
            pool,
            resolve,
            token: random_token(),
        }
    }

    /// The axum router. `GET /s/{token}/{host_id}?path=<encoded-remote-path>`.
    pub fn router(&self) -> Router {
        let inner = Inner {
            pool: self.pool.clone(),
            resolve: self.resolve.clone(),
            token: self.token.clone(),
        };
        Router::new()
            .route("/s/{token}/{host_id}", get(stream_file))
            .with_state(inner)
    }

    /// Build the VLC URL for a host + remote path, given the bound `port`.
    pub fn stream_url(&self, port: u16, host_id: &str, path: &str) -> String {
        format!(
            "http://127.0.0.1:{port}/s/{}/{host_id}?path={}",
            self.token,
            percent_encode(path)
        )
    }

    /// Bind to an ephemeral loopback port. Returns the bound address and the
    /// serve future (await it, or `tokio::spawn` it and keep the handle).
    pub async fn bind_local(
        &self,
    ) -> std::io::Result<(
        SocketAddr,
        impl std::future::Future<Output = std::io::Result<()>> + Send,
    )> {
        use std::future::IntoFuture;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let addr = listener.local_addr()?;
        let serve = axum::serve(listener, self.router()).into_future();
        Ok((addr, serve))
    }
}

#[derive(serde::Deserialize)]
struct StreamQuery {
    path: String,
}

async fn stream_file(
    State(inner): State<Inner>,
    Path((token, host_id)): Path<(String, String)>,
    Query(q): Query<StreamQuery>,
    headers: HeaderMap,
) -> Response {
    if token != inner.token {
        return (StatusCode::FORBIDDEN, "forbidden").into_response();
    }
    let Some(host) = (inner.resolve)(&host_id) else {
        return (StatusCode::NOT_FOUND, "unknown host").into_response();
    };
    let backend = inner.pool.get(&host).await;

    let size = match backend.stat(&q.path).await {
        Ok(e) => e.size_bytes,
        Err(e) => return (StatusCode::NOT_FOUND, e.to_string()).into_response(),
    };

    let range = headers
        .get(header::RANGE)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);

    match parse_range(range.as_deref(), size) {
        Err(_) => {
            let mut resp = (StatusCode::RANGE_NOT_SATISFIABLE, "").into_response();
            resp.headers_mut().insert(
                header::CONTENT_RANGE,
                HeaderValue::from_str(&format!("bytes */{size}")).unwrap(),
            );
            resp
        }
        Ok(None) => {
            let reader = match backend.open_read(&q.path).await {
                Ok(r) => r,
                Err(e) => {
                    return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
                }
            };
            stream_response(StatusCode::OK, reader, size, None, &q.path)
        }
        Ok(Some((start, end))) => {
            let len = end - start + 1;
            let mut reader = match backend.open_read(&q.path).await {
                Ok(r) => r,
                Err(e) => {
                    return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
                }
            };
            if let Err(e) = reader.seek(start).await {
                return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
            }
            let content_range = format!("bytes {start}-{end}/{size}");
            stream_response(
                StatusCode::PARTIAL_CONTENT,
                reader,
                len,
                Some(content_range),
                &q.path,
            )
        }
    }
}

fn stream_response(
    status: StatusCode,
    reader: Box<dyn ReadStream>,
    len: u64,
    content_range: Option<String>,
    path: &str,
) -> Response {
    let body = Body::from_stream(reader_stream(reader, len));
    let mut resp = Response::new(body);
    *resp.status_mut() = status;
    let h = resp.headers_mut();
    h.insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));
    h.insert(
        header::CONTENT_LENGTH,
        HeaderValue::from_str(&len.to_string()).unwrap(),
    );
    h.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static(content_type(path)),
    );
    if let Some(cr) = content_range {
        h.insert(header::CONTENT_RANGE, HeaderValue::from_str(&cr).unwrap());
    }
    resp
}

/// Stream exactly `remaining` bytes from `reader` as a body, yielding
/// `CHUNK`-sized frames. Returns `Ok(None)` early on EOF so a truncated file
/// ends the body rather than hanging the client.
fn reader_stream(
    reader: Box<dyn ReadStream>,
    remaining: u64,
) -> impl Stream<Item = Result<Bytes, std::io::Error>> + Send {
    futures_util::stream::try_unfold((reader, remaining), |(mut r, mut rem)| async move {
        if rem == 0 {
            return Ok(None);
        }
        let mut buf = vec![0u8; CHUNK];
        let want = buf.len().min(rem as usize);
        match r.read(&mut buf[..want]).await {
            Ok(0) => Ok(None),
            Ok(n) => {
                rem -= n as u64;
                buf.truncate(n);
                Ok(Some((Bytes::from(buf), (r, rem))))
            }
            Err(e) => Err(std::io::Error::other(e.to_string())),
        }
    })
}

fn content_type(path: &str) -> &'static str {
    let ext = path.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "mp4" | "m4v" => "video/mp4",
        "mkv" => "video/x-matroska",
        "mov" => "video/quicktime",
        "webm" => "video/webm",
        "mp3" => "audio/mpeg",
        "m4a" => "audio/mp4",
        "flac" => "audio/flac",
        "wav" => "audio/wav",
        "ogg" | "opus" => "audio/ogg",
        _ => "application/octet-stream",
    }
}

/// Percent-encode a remote path for a query value, preserving `/`.
fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for &b in s.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn random_token() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_range_forms() {
        let size = 100;
        assert_eq!(parse_range(None, size), Ok(None));
        assert_eq!(parse_range(Some("bytes=0-99"), size), Ok(Some((0, 99))));
        assert_eq!(parse_range(Some("bytes=10-"), size), Ok(Some((10, 99))));
        assert_eq!(parse_range(Some("bytes=-5"), size), Ok(Some((95, 99))));
        // Closed range past EOF clamps to size-1.
        assert_eq!(parse_range(Some("bytes=90-999"), size), Ok(Some((90, 99))));
    }

    #[test]
    fn parse_range_invalid_and_unsatisfiable() {
        assert_eq!(parse_range(Some("bytes=100-"), 100), Err(RangeError));
        assert_eq!(parse_range(Some("bytes=-0"), 100), Err(RangeError));
        assert_eq!(parse_range(Some("nonsense"), 100), Err(RangeError));
        assert_eq!(parse_range(Some("bytes=-"), 100), Err(RangeError));
    }

    #[test]
    fn percent_encode_preserves_slashes() {
        assert_eq!(percent_encode("/mnt/raid6/anime"), "/mnt/raid6/anime");
        assert_eq!(percent_encode("/a b/c.d"), "/a%20b/c.d");
        assert_eq!(percent_encode("/x?y#z"), "/x%3Fy%23z");
    }
}
