/*
 * Copyright 2026 Julien Bombled
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

//! The "Share folder" server, as the C# `EphemeralFileServer` (`EphemeralFileServer.cs`) and
//! `FileShareService`, stricter than it: a read-only HTTP/1.1 server over a folder, written
//! on tokio because it needs no more than `GET` and `HEAD`.
//!
//! - **Access**: a random 256-bit token per share, compared in constant time, taken from an
//!   `Authorization: Bearer` header or a `token` query parameter, as the C#. It is never
//!   logged: the log and the events carry the path, never the query.
//! - **Reach**: it listens on one address the user chose, the loopback by default; the
//!   unspecified address (every interface) is refused. HTTP is cleartext: the caller says so
//!   before it starts.
//! - **Files**: only `GET` and `HEAD`; a path is checked segment by segment and on disk, and
//!   a link or junction anywhere under the root is refused (see [`paths`]).
//! - **Limits**: the request line, the head, the number of headers, the time to send a head
//!   (slowloris), the time to take a chunk, and the connections at once are all bounded.
//! - **Life**: it stops when told to or when its lifetime is over, and drops its connections.
//!
//! TFTP, which the C# also served, is not ported: it has no authentication.

pub mod listing;
pub mod paths;
pub mod request;

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Semaphore;
use tokio::sync::mpsc::Sender;
use tokio_util::sync::CancellationToken;

pub use self::listing::ListingLabels;
use self::paths::{PathError, Resolved};
use self::request::{
    HEAD_END, MAX_HEAD_BYTES, MAX_REQUEST_LINE_BYTES, Method, Request, RequestError,
};

/// Bytes of a share's access token.
pub const TOKEN_BYTES: usize = 32;

/// Connections served at once: another is closed as soon as it is accepted.
pub const MAX_CONNECTIONS: usize = 16;

/// Time a client has to send its whole head: a slower one is dropped (slowloris).
pub const HEAD_TIMEOUT: Duration = Duration::from_secs(10);

/// Time a client has to take one chunk of a response.
pub const WRITE_TIMEOUT: Duration = Duration::from_secs(30);

/// Bytes of a file read and sent at a time.
const CHUNK_BYTES: usize = 64 * 1024;

/// Bytes read from a connection at a time while its head is awaited.
const READ_BYTES: usize = 1024;

/// Pause after a failed `accept`, so a persistent failure does not spin.
const ACCEPT_RETRY: Duration = Duration::from_millis(100);

/// Events queued for the owner of a share: when it is full, request reports are dropped and
/// counted, so a flood of requests (refused ones too) cannot grow memory.
pub const EVENT_QUEUE: usize = 256;

/// Time given to hand the last events over when the share ends.
const FINAL_EVENTS_TIMEOUT: Duration = Duration::from_secs(1);

/// Bytes of the path kept in the log and the events.
const LOGGED_PATH_CHARS: usize = 256;

/// Why a share could not start.
#[derive(Debug, thiserror::Error)]
pub enum ShareError {
    /// The folder cannot be read.
    #[error("the shared folder cannot be read: {0}")]
    Root(std::io::Error),
    /// The path is not a folder.
    #[error("the shared path is not a folder")]
    NotAFolder,
    /// The address is the unspecified one, which means every interface.
    #[error("sharing on every interface is refused")]
    EveryInterface,
    /// The address could not be bound.
    #[error("the address could not be used: {0}")]
    Bind(std::io::Error),
    /// The system gave no random bytes for the token.
    #[error("no random token could be made")]
    Random,
}

/// Where a share listens: the loopback unless the user chose a local address.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BindChoice {
    /// This computer only.
    #[default]
    Loopback,
    /// One address of this computer, chosen from its interfaces.
    Interface(IpAddr),
}

impl BindChoice {
    /// The address to listen on.
    ///
    /// # Errors
    ///
    /// The address is the unspecified one, or a multicast one.
    pub fn address(self) -> Result<IpAddr, ShareError> {
        match self {
            Self::Loopback => Ok(IpAddr::V4(Ipv4Addr::LOCALHOST)),
            Self::Interface(address) if address.is_unspecified() || address.is_multicast() => {
                Err(ShareError::EveryInterface)
            }
            Self::Interface(address) => Ok(address),
        }
    }
}

/// What a share is asked for.
#[derive(Debug, Clone)]
pub struct ShareConfig {
    /// The folder shared.
    pub root: PathBuf,
    /// Where it listens.
    pub bind: BindChoice,
    /// The port; 0 asks the system for a free one.
    pub port: u16,
    /// How long it lives.
    pub lifetime: Duration,
    /// The words of a listing.
    pub labels: ListingLabels,
    /// The time a client has to send its head; [`HEAD_TIMEOUT`] unless a test shortens it.
    pub head_timeout: Duration,
}

impl ShareConfig {
    /// A share of `root` on `bind` and `port` for `lifetime`.
    #[must_use]
    pub fn new(
        root: PathBuf,
        bind: BindChoice,
        port: u16,
        lifetime: Duration,
        labels: ListingLabels,
    ) -> Self {
        Self {
            root,
            bind,
            port,
            lifetime,
            labels,
            head_timeout: HEAD_TIMEOUT,
        }
    }
}

/// A request served, as it is logged: the client and the path, never the token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestRecord {
    /// The client's address.
    pub client: IpAddr,
    /// The method.
    pub method: String,
    /// The path as written, without its query, control characters replaced.
    pub path: String,
    /// The status answered.
    pub status: u16,
}

/// Why a share ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopReason {
    /// It was told to stop.
    Stopped,
    /// Its lifetime was over.
    Expired,
}

/// What a running share reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShareEvent {
    /// A request was answered.
    Request(RequestRecord),
    /// This many request reports were dropped because the queue was full (sent before the
    /// end, with the total).
    Dropped(u64),
    /// The share ended; nothing follows.
    Stopped(StopReason),
}

/// Whether `presented` is the share's token, in constant time.
#[must_use]
pub fn token_matches(expected: &str, presented: Option<&str>) -> bool {
    presented.is_some_and(|presented| {
        sealvault::compare::equal(expected.as_bytes(), presented.as_bytes())
    })
}

/// The token a request presents: the bearer header first, else the query, as the C#.
fn presented_token(request: &Request, query: Option<&str>) -> Option<String> {
    let bearer = request.authorization.as_deref().and_then(|value| {
        let (scheme, token) = value.split_once(' ')?;
        scheme
            .eq_ignore_ascii_case("bearer")
            .then(|| token.trim().to_owned())
    });
    bearer.or_else(|| query.and_then(paths::query_token))
}

/// What a running share needs to answer.
struct Context {
    root: PathBuf,
    token: String,
    labels: ListingLabels,
    head_timeout: Duration,
    events: Sender<ShareEvent>,
    /// Request reports that did not fit in the queue.
    dropped: AtomicU64,
}

/// A share that listens and has not served yet.
pub struct Share {
    listener: TcpListener,
    addr: SocketAddr,
    root: PathBuf,
    token: String,
    labels: ListingLabels,
    lifetime: Duration,
    head_timeout: Duration,
}

impl std::fmt::Debug for Share {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The token is a secret.
        f.debug_struct("Share")
            .field("addr", &self.addr)
            .field("root", &self.root)
            .finish_non_exhaustive()
    }
}

impl Share {
    /// Makes the token and binds the address.
    ///
    /// # Errors
    ///
    /// The folder cannot be read or is not one, the address is refused or cannot be bound,
    /// or no random token could be made.
    pub async fn bind(config: ShareConfig) -> Result<Self, ShareError> {
        let address = config.bind.address()?;
        let root = std::fs::canonicalize(&config.root).map_err(ShareError::Root)?;
        if !root.is_dir() {
            return Err(ShareError::NotAFolder);
        }
        let bytes = sealvault::random::array::<TOKEN_BYTES>().map_err(|_| ShareError::Random)?;
        let token = hex(&bytes);
        let listener = TcpListener::bind((address, config.port))
            .await
            .map_err(ShareError::Bind)?;
        let addr = listener.local_addr().map_err(ShareError::Bind)?;
        Ok(Self {
            listener,
            addr,
            root,
            token,
            labels: config.labels,
            lifetime: config.lifetime,
            head_timeout: config.head_timeout,
        })
    }

    /// The address it listens on.
    #[must_use]
    pub fn local_addr(&self) -> SocketAddr {
        self.addr
    }

    /// The access token: to be shown to the user, never logged.
    #[must_use]
    pub fn token(&self) -> &str {
        &self.token
    }

    /// The canonical folder it shares.
    #[must_use]
    pub fn root(&self) -> &std::path::Path {
        &self.root
    }

    /// Serves until `cancel` is cancelled or the lifetime is over, then drops the
    /// connections and reports [`ShareEvent::Stopped`].
    pub async fn run(self, cancel: CancellationToken, events: Sender<ShareEvent>) -> StopReason {
        let stop = cancel.child_token();
        let context = Arc::new(Context {
            root: self.root,
            token: self.token,
            labels: self.labels,
            head_timeout: self.head_timeout,
            events: events.clone(),
            dropped: AtomicU64::new(0),
        });
        let permits = Arc::new(Semaphore::new(MAX_CONNECTIONS));
        let deadline = tokio::time::sleep(self.lifetime);
        tokio::pin!(deadline);
        let reason = loop {
            tokio::select! {
                () = cancel.cancelled() => break StopReason::Stopped,
                () = &mut deadline => break StopReason::Expired,
                accepted = self.listener.accept() => match accepted {
                    Ok((stream, peer)) => {
                        let Ok(permit) = Arc::clone(&permits).try_acquire_owned() else {
                            log::warn!("share: {} refused, too many connections", peer.ip());
                            continue;
                        };
                        let context = Arc::clone(&context);
                        let stop = stop.clone();
                        tokio::spawn(async move {
                            tokio::select! {
                                () = stop.cancelled() => {}
                                () = serve(stream, peer, &context) => {}
                            }
                            drop(permit);
                        });
                    }
                    Err(error) => {
                        log::warn!("share: accept failed: {error}");
                        tokio::time::sleep(ACCEPT_RETRY).await;
                    }
                },
            }
        };
        stop.cancel();
        let dropped = context.dropped.load(Ordering::Relaxed);
        let _ = tokio::time::timeout(FINAL_EVENTS_TIMEOUT, async {
            if dropped > 0 {
                let _ = events.send(ShareEvent::Dropped(dropped)).await;
            }
            let _ = events.send(ShareEvent::Stopped(reason)).await;
        })
        .await;
        reason
    }
}

/// `bytes` as lowercase hexadecimal.
fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::new(), |mut out, byte| {
        let _ = write!(out, "{byte:02x}");
        out
    })
}

/// What an answer holds besides its head.
enum Body {
    Bytes(Vec<u8>),
    File(tokio::fs::File, u64),
}

/// An answer.
struct Reply {
    status: u16,
    headers: Vec<(&'static str, String)>,
    body: Body,
}

impl Reply {
    /// A bare answer to `status`.
    fn status(status: u16) -> Self {
        let text = format!("{status} {}\n", reason(status));
        let mut reply = Self::bytes(status, "text/plain; charset=utf-8", text.into_bytes());
        match status {
            401 => reply
                .headers
                .push(("WWW-Authenticate", "Bearer".to_owned())),
            405 => reply.headers.push(("Allow", "GET, HEAD".to_owned())),
            _ => {}
        }
        reply
    }

    fn bytes(status: u16, content_type: &str, bytes: Vec<u8>) -> Self {
        Self {
            status,
            headers: vec![
                ("Content-Type", content_type.to_owned()),
                ("Content-Length", bytes.len().to_string()),
            ],
            body: Body::Bytes(bytes),
        }
    }
}

/// The reason phrase of a status the server sends.
fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        408 => "Request Timeout",
        414 => "URI Too Long",
        431 => "Request Header Fields Too Large",
        505 => "HTTP Version Not Supported",
        _ => "Internal Server Error",
    }
}

/// The path of `target` as it is logged: no query, no control characters, bounded.
fn logged_path(target: &str) -> String {
    let (path, _query) = paths::split_target(target);
    path.chars()
        .take(LOGGED_PATH_CHARS)
        .map(|c| if c.is_control() { '?' } else { c })
        .collect()
}

/// Why a head was not read.
enum HeadFailure {
    /// The client was too slow.
    Timeout,
    /// The client left, or the connection failed.
    Closed,
    /// The head is too big.
    Refused(RequestError),
}

/// Reads a request's head: up to the blank line, within the limits and the time.
async fn read_head(stream: &mut TcpStream, timeout: Duration) -> Result<Vec<u8>, HeadFailure> {
    let read = async {
        let mut buffer: Vec<u8> = Vec::new();
        let mut chunk = [0_u8; READ_BYTES];
        loop {
            let count = stream
                .read(&mut chunk)
                .await
                .map_err(|_| HeadFailure::Closed)?;
            if count == 0 {
                return Err(HeadFailure::Closed);
            }
            let from = buffer.len().saturating_sub(HEAD_END.len() - 1);
            buffer.extend_from_slice(&chunk[..count]);
            if let Some(found) = buffer[from..]
                .windows(HEAD_END.len())
                .position(|window| window == HEAD_END)
            {
                buffer.truncate(from + found);
                return Ok(buffer);
            }
            if buffer.len() > MAX_REQUEST_LINE_BYTES && !buffer.windows(2).any(|w| w == b"\r\n") {
                return Err(HeadFailure::Refused(RequestError::LineTooLong));
            }
            if buffer.len() > MAX_HEAD_BYTES {
                return Err(HeadFailure::Refused(RequestError::HeadTooLarge));
            }
        }
    };
    tokio::time::timeout(timeout, read)
        .await
        .unwrap_or(Err(HeadFailure::Timeout))
}

/// Answers one connection: one request, then it closes.
async fn serve(mut stream: TcpStream, peer: SocketAddr, context: &Context) {
    let (method, path, reply) = match read_head(&mut stream, context.head_timeout).await {
        Err(HeadFailure::Closed) => return,
        Err(HeadFailure::Timeout) => ("-".to_owned(), "-".to_owned(), Reply::status(408)),
        Err(HeadFailure::Refused(error)) => (
            "-".to_owned(),
            "-".to_owned(),
            Reply::status(error.status()),
        ),
        Ok(head) => match request::parse_head(&head) {
            Err(error) => (
                "-".to_owned(),
                "-".to_owned(),
                Reply::status(error.status()),
            ),
            Ok(request) => {
                let reply = answer(&request, context).await;
                (
                    request.method.name().to_owned(),
                    logged_path(&request.target),
                    reply,
                )
            }
        },
    };
    let status = reply.status;
    let head_only = method == "HEAD";
    log::info!("share: {} {method} {path} -> {status}", peer.ip());
    let record = ShareEvent::Request(RequestRecord {
        client: peer.ip(),
        method,
        path,
        status,
    });
    // Never waits: a full queue costs the report, counted, not the answer.
    if context.events.try_send(record).is_err() {
        context.dropped.fetch_add(1, Ordering::Relaxed);
    }
    let _ = write_reply(&mut stream, reply, head_only).await;
    let _ = stream.shutdown().await;
}

/// The answer to a parsed request.
async fn answer(request: &Request, context: &Context) -> Reply {
    let (path, query) = paths::split_target(&request.target);
    // Who asks is checked before anything about the request is acted on or revealed.
    let presented = presented_token(request, query);
    if !token_matches(&context.token, presented.as_deref()) {
        return Reply::status(401);
    }
    if !matches!(request.method, Method::Get | Method::Head) {
        return Reply::status(405);
    }
    let segments = match paths::segments(path) {
        Ok(segments) => segments,
        Err(error) => return Reply::status(error.status()),
    };
    let root = context.root.clone();
    let wanted = segments.clone();
    let resolved = tokio::task::spawn_blocking(move || paths::resolve(&root, &wanted)).await;
    let resolved: Resolved = match resolved {
        Ok(Ok(resolved)) => resolved,
        Ok(Err(error)) => return Reply::status(error.status()),
        Err(_) => return Reply::status(500),
    };
    if resolved.metadata.is_dir() {
        let root = context.root.clone();
        // Listed, then looked at again: a folder swapped for a link meanwhile is refused.
        let entries = tokio::task::spawn_blocking(move || {
            let entries = listing::read_entries(&resolved.path).map_err(|_| PathError::NotFound)?;
            paths::recheck_dir(&root, &resolved)?;
            Ok::<_, PathError>(entries)
        })
        .await;
        let entries = match entries {
            Ok(Ok(entries)) => entries,
            Ok(Err(error)) => return Reply::status(error.status()),
            Err(_) => return Reply::status(500),
        };
        let page = listing::render(&segments, &entries, &context.token, &context.labels);
        return Reply::bytes(200, "text/html; charset=utf-8", page.into_bytes());
    }
    if !resolved.metadata.is_file() {
        return Reply::status(PathError::NotFound.status());
    }
    let root = context.root.clone();
    let opened = tokio::task::spawn_blocking(move || paths::open_file(&root, &resolved)).await;
    let file = match opened {
        Ok(Ok(file)) => tokio::fs::File::from_std(file),
        Ok(Err(error)) => return Reply::status(error.status()),
        Err(_) => return Reply::status(500),
    };
    let Ok(metadata) = file.metadata().await else {
        return Reply::status(PathError::NotFound.status());
    };
    if !metadata.is_file() {
        return Reply::status(PathError::NotFound.status());
    }
    let name = segments.last().map_or("file", String::as_str);
    Reply {
        status: 200,
        headers: vec![
            ("Content-Type", "application/octet-stream".to_owned()),
            ("Content-Length", metadata.len().to_string()),
            ("Content-Disposition", content_disposition(name)),
        ],
        body: Body::File(file, metadata.len()),
    }
}

/// `attachment` with `name` safe in a header: an ASCII fallback and the UTF-8 name encoded.
fn content_disposition(name: &str) -> String {
    let fallback: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect();
    format!(
        "attachment; filename=\"{fallback}\"; filename*=UTF-8''{}",
        listing::encode_segment(name)
    )
}

/// Headers of every answer: nothing cached, nothing sniffed, nothing run, and no referrer,
/// since a listing's links carry the token.
fn with_security_headers(mut reply: Reply) -> Reply {
    reply.headers.extend([
        ("Cache-Control", "no-store".to_owned()),
        ("X-Content-Type-Options", "nosniff".to_owned()),
        ("Content-Security-Policy", "default-src 'none'".to_owned()),
        ("Referrer-Policy", "no-referrer".to_owned()),
    ]);
    reply
}

/// Writes `bytes` within the write time.
async fn write_timed(stream: &mut TcpStream, bytes: &[u8]) -> std::io::Result<()> {
    tokio::time::timeout(WRITE_TIMEOUT, stream.write_all(bytes))
        .await
        .unwrap_or_else(|_| Err(std::io::ErrorKind::TimedOut.into()))
}

/// Sends `reply`; for `HEAD`, its head alone.
async fn write_reply(stream: &mut TcpStream, reply: Reply, head_only: bool) -> std::io::Result<()> {
    let reply = with_security_headers(reply);
    let mut head = format!("HTTP/1.1 {} {}\r\n", reply.status, reason(reply.status));
    for (name, value) in &reply.headers {
        head.push_str(name);
        head.push_str(": ");
        head.push_str(value);
        head.push_str("\r\n");
    }
    head.push_str("Connection: close\r\n\r\n");
    write_timed(stream, head.as_bytes()).await?;
    if head_only {
        return Ok(());
    }
    match reply.body {
        Body::Bytes(bytes) => write_timed(stream, &bytes).await,
        Body::File(file, length) => {
            let mut file = file.take(length);
            let mut chunk = vec![0_u8; CHUNK_BYTES];
            loop {
                let count = file.read(&mut chunk).await?;
                if count == 0 {
                    return Ok(());
                }
                write_timed(stream, &chunk[..count]).await?;
            }
        }
    }
}

#[cfg(test)]
mod tests;
