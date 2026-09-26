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

//! An SFTP version 3 client over any byte stream: an SSH subsystem channel, or a child
//! process's standard input and output in tests.
//!
//! Requests run concurrently: each gets an id, the reader routes each response to its
//! request, so transfers keep many reads or writes in flight.
//!
//! What ends the session, failing every pending request, because the stream can no longer
//! be trusted: a response to an id nobody asked for, a response of the wrong type, a
//! malformed or oversized packet, a handle longer than the protocol allows, and a request
//! unanswered past its timeout. A request's timer starts when its bytes are written, not
//! when it is queued, so a long queue on a slow link does not time out.
//!
//! The reader never awaits anything but the stream: russh delivers every channel of a
//! connection from one loop, and a reader that stalled would stall the terminal beside it.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use thiserror::Error;
use tokio::io::{AsyncRead, AsyncReadExt as _, AsyncWrite, AsyncWriteExt as _};
use tokio::sync::{mpsc, oneshot};
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

use crate::path::RemotePath;
use crate::protocol::{
    Attributes, Extensions, NameEntry, Request, Response, SFTP_VERSION, StatusCode,
};
use crate::wire::{Reader, WireError, Writer};

/// Longest handle version 3 allows.
const MAX_HANDLE_LENGTH: usize = 256;

/// Shortest interval between two deadline checks.
const MIN_DEADLINE_TICK: Duration = Duration::from_millis(10);

/// Extension renaming over an existing target.
const POSIX_RENAME: &[u8] = b"posix-rename@openssh.com";

/// Extension flushing an open file to stable storage.
const FSYNC: &[u8] = b"fsync@openssh.com";

/// Extension reporting the server's size limits.
const LIMITS: &[u8] = b"limits@openssh.com";

/// Settings of a client.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClientConfig {
    /// Longest wait for a response once its request is written.
    pub request_timeout: Duration,
    /// Largest packet accepted from the server.
    pub max_packet: u32,
    /// Most entries a directory listing may hold.
    pub max_listing_entries: usize,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            request_timeout: Duration::from_secs(30),
            max_packet: 256 * 1024,
            max_listing_entries: 1_000_000,
        }
    }
}

/// Why the session ended.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum Closed {
    /// The stream ended or failed.
    #[error("the connection ended")]
    Stream,
    /// The server sent something that breaks the protocol.
    #[error("protocol violation: {0}")]
    Protocol(String),
    /// A request was not answered in time.
    #[error("the server did not answer in time")]
    Timeout,
    /// The client was shut down or dropped.
    #[error("the client was closed")]
    ByClient,
}

/// Why an operation failed.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SftpError {
    /// The server refused the operation.
    #[error("{code:?}")]
    Status {
        /// Status code.
        code: StatusCode,
        /// The server's message, untrusted.
        message: Vec<u8>,
    },
    /// The session is over.
    #[error("session closed: {0}")]
    Closed(Closed),
    /// The server speaks another version.
    #[error("unsupported SFTP version {0}")]
    UnsupportedVersion(u32),
    /// A directory listing went past the configured limit.
    #[error("the listing exceeds {0} entries")]
    ListingTooLarge(usize),
}

/// An open file or directory on the server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Handle(Vec<u8>);

/// Limits the server announced (`limits@openssh.com`); zero means none announced.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Limits {
    /// Largest packet the server accepts.
    pub max_packet: u64,
    /// Largest read the server serves.
    pub max_read: u64,
    /// Largest write the server accepts.
    pub max_write: u64,
    /// Most handles open at once.
    pub max_open_handles: u64,
}

/// One entry of a directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirEntry {
    /// The name, as the server's bytes.
    pub name: Vec<u8>,
    /// Attributes, as far as the server sent them.
    pub attributes: Attributes,
}

/// The response type a request waits for; a status is always acceptable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Expect {
    Status,
    Handle,
    Data,
    Name,
    Attrs,
    Extended,
}

impl Expect {
    fn fits(self, response: &Response) -> bool {
        matches!(
            (self, response),
            (_, Response::Status { .. })
                | (Self::Handle, Response::Handle { .. })
                | (Self::Data, Response::Data { .. })
                | (Self::Name, Response::Name { .. })
                | (Self::Attrs, Response::Attrs { .. })
                | (Self::Extended, Response::ExtendedReply { .. })
        )
    }
}

struct Pending {
    reply: oneshot::Sender<Result<Response, SftpError>>,
    expects: Expect,
    /// Set when the request's bytes are written.
    deadline: Option<Instant>,
}

#[derive(Default)]
struct State {
    pending: HashMap<u32, Pending>,
    next_id: u32,
    closed: Option<Closed>,
}

/// What the background tasks share with the client.
struct Core {
    state: Mutex<State>,
    stop: CancellationToken,
    timeout: Duration,
}

impl Core {
    fn lock(&self) -> MutexGuard<'_, State> {
        // A panic while holding the lock leaves plain data behind: keep using it.
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Ends the session: every pending request fails with `reason`, the tasks stop.
    fn close(&self, reason: &Closed) {
        let pending = {
            let mut state = self.lock();
            if state.closed.is_some() {
                return;
            }
            state.closed = Some(reason.clone());
            std::mem::take(&mut state.pending)
        };
        for (_, request) in pending {
            let _ = request.reply.send(Err(SftpError::Closed(reason.clone())));
        }
        self.stop.cancel();
    }
}

struct Outgoing {
    id: u32,
    frame: Vec<u8>,
}

struct Inner {
    core: Arc<Core>,
    outgoing: mpsc::UnboundedSender<Outgoing>,
    extensions: Extensions,
    limits: Limits,
    max_listing_entries: usize,
}

/// An SFTP session. Cheap to clone; the last clone dropped closes it: the request queue's
/// sender goes with it, the writer task sees the queue end and closes the session.
#[derive(Clone)]
pub struct SftpClient {
    inner: Arc<Inner>,
}

impl std::fmt::Debug for SftpClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SftpClient")
            .field("limits", &self.inner.limits)
            .finish_non_exhaustive()
    }
}

async fn read_frame<R: AsyncRead + Unpin>(reader: &mut R, max: u32) -> Result<Vec<u8>, Closed> {
    let mut length = [0; 4];
    reader
        .read_exact(&mut length)
        .await
        .map_err(|_| Closed::Stream)?;
    let length = u32::from_be_bytes(length);
    if length > max {
        return Err(Closed::Protocol(WireError::TooLong(length).to_string()));
    }
    let length = usize::try_from(length).map_err(|_| Closed::Stream)?;
    let mut body = vec![0; length];
    reader
        .read_exact(&mut body)
        .await
        .map_err(|_| Closed::Stream)?;
    Ok(body)
}

fn decode(body: &[u8]) -> Result<Response, Closed> {
    Response::decode(body).map_err(|error| Closed::Protocol(error.to_string()))
}

/// Routes responses to their requests; returns why the session must end.
async fn read_responses<R: AsyncRead + Unpin>(mut reader: R, core: &Core, max: u32) -> Closed {
    loop {
        let body = match read_frame(&mut reader, max).await {
            Ok(body) => body,
            Err(reason) => return reason,
        };
        let response = match decode(&body) {
            Ok(response) => response,
            Err(reason) => return reason,
        };
        let Some(id) = response.id() else {
            return Closed::Protocol("a second version packet".to_owned());
        };
        if let Response::Handle { handle, .. } = &response
            && handle.len() > MAX_HANDLE_LENGTH
        {
            return Closed::Protocol(format!("a handle of {} bytes", handle.len()));
        }
        let Some(pending) = core.lock().pending.remove(&id) else {
            return Closed::Protocol(format!("a response to unknown request {id}"));
        };
        if !pending.expects.fits(&response) {
            // Taken out of the pending set already: it hears the reason here, not from
            // the close that follows.
            let reason = Closed::Protocol(format!("a response of the wrong type to request {id}"));
            let _ = pending.reply.send(Err(SftpError::Closed(reason.clone())));
            return reason;
        }
        let _ = pending.reply.send(Ok(response));
    }
}

/// Writes requests in order and starts each one's timer once written.
async fn write_requests<W: AsyncWrite + Unpin>(
    mut writer: W,
    mut outgoing: mpsc::UnboundedReceiver<Outgoing>,
    core: &Core,
) -> Closed {
    while let Some(request) = outgoing.recv().await {
        if writer.write_all(&request.frame).await.is_err() || writer.flush().await.is_err() {
            return Closed::Stream;
        }
        if let Some(pending) = core.lock().pending.get_mut(&request.id) {
            pending.deadline = Some(Instant::now() + core.timeout);
        }
    }
    Closed::ByClient
}

/// Ends the session when a written request outlives its deadline.
async fn watch_deadlines(core: &Core) -> Closed {
    let mut tick = tokio::time::interval((core.timeout / 4).max(MIN_DEADLINE_TICK));
    loop {
        tick.tick().await;
        let now = Instant::now();
        let expired = core
            .lock()
            .pending
            .values()
            .any(|pending| pending.deadline.is_some_and(|deadline| deadline <= now));
        if expired {
            return Closed::Timeout;
        }
    }
}

fn parse_limits(data: &[u8]) -> Result<Limits, WireError> {
    let mut reader = Reader::new(data);
    Ok(Limits {
        max_packet: reader.u64()?,
        max_read: reader.u64()?,
        max_write: reader.u64()?,
        max_open_handles: reader.u64()?,
    })
}

fn status_error(code: StatusCode, message: Vec<u8>) -> SftpError {
    SftpError::Status { code, message }
}

fn unexpected() -> SftpError {
    // Unreachable: the reader checked the type against the request.
    SftpError::Closed(Closed::Protocol("unexpected response".to_owned()))
}

impl SftpClient {
    /// Opens a session on `stream`: version negotiation, then the server's limits when it
    /// announces them.
    ///
    /// # Errors
    ///
    /// [`SftpError::UnsupportedVersion`], or [`SftpError::Closed`] when the stream fails,
    /// the server says something else, or does not answer within the request timeout.
    pub async fn start<S>(stream: S, config: ClientConfig) -> Result<Self, SftpError>
    where
        S: AsyncRead + AsyncWrite + Send + 'static,
    {
        let (mut reader, mut writer) = tokio::io::split(stream);
        let init = Request::Init {
            version: SFTP_VERSION,
            extensions: Vec::new(),
        };
        let handshake = async {
            writer
                .write_all(&init.encode())
                .await
                .map_err(|_| Closed::Stream)?;
            writer.flush().await.map_err(|_| Closed::Stream)?;
            decode(&read_frame(&mut reader, config.max_packet).await?)
        };
        let version = tokio::time::timeout(config.request_timeout, handshake)
            .await
            .map_err(|_| SftpError::Closed(Closed::Timeout))?
            .map_err(SftpError::Closed)?;
        let Response::Version {
            version,
            extensions,
        } = version
        else {
            return Err(SftpError::Closed(Closed::Protocol(
                "no version packet".to_owned(),
            )));
        };
        if version != SFTP_VERSION {
            return Err(SftpError::UnsupportedVersion(version));
        }

        let core = Arc::new(Core {
            state: Mutex::new(State::default()),
            stop: CancellationToken::new(),
            timeout: config.request_timeout,
        });
        let (outgoing, outgoing_rx) = mpsc::unbounded_channel();
        spawn_tasks(reader, writer, outgoing_rx, core.clone(), config.max_packet);

        let mut client = Self {
            inner: Arc::new(Inner {
                core,
                outgoing,
                extensions,
                limits: Limits::default(),
                max_listing_entries: config.max_listing_entries,
            }),
        };
        if client.has_extension(LIMITS) {
            let limits = client.fetch_limits().await?;
            if let Some(inner) = Arc::get_mut(&mut client.inner) {
                inner.limits = limits;
            }
        }
        Ok(client)
    }

    /// Whether the server announced extension `name`.
    #[must_use]
    pub fn has_extension(&self, name: &[u8]) -> bool {
        self.inner
            .extensions
            .iter()
            .any(|(announced, _)| announced == name)
    }

    /// The server's announced limits.
    #[must_use]
    pub fn limits(&self) -> Limits {
        self.inner.limits
    }

    /// Why the session ended, once it has.
    #[must_use]
    pub fn closed(&self) -> Option<Closed> {
        self.inner.core.lock().closed.clone()
    }

    /// Ends the session; pending requests fail.
    pub fn shutdown(&self) {
        self.inner.core.close(&Closed::ByClient);
    }

    async fn request(
        &self,
        expects: Expect,
        build: impl FnOnce(u32) -> Request,
    ) -> Result<Response, SftpError> {
        let (reply, answer) = oneshot::channel();
        let id = {
            let mut state = self.inner.core.lock();
            if let Some(reason) = &state.closed {
                return Err(SftpError::Closed(reason.clone()));
            }
            let mut id = state.next_id;
            while state.pending.contains_key(&id) {
                id = id.wrapping_add(1);
            }
            state.next_id = id.wrapping_add(1);
            state.pending.insert(
                id,
                Pending {
                    reply,
                    expects,
                    deadline: None,
                },
            );
            id
        };
        let frame = build(id).encode();
        if self.inner.outgoing.send(Outgoing { id, frame }).is_err() {
            self.inner.core.lock().pending.remove(&id);
            return Err(SftpError::Closed(Closed::ByClient));
        }
        answer
            .await
            .unwrap_or(Err(SftpError::Closed(Closed::ByClient)))
    }

    async fn expect_ok(&self, build: impl FnOnce(u32) -> Request) -> Result<(), SftpError> {
        match self.request(Expect::Status, build).await? {
            Response::Status {
                code: StatusCode::Ok,
                ..
            } => Ok(()),
            Response::Status { code, message, .. } => Err(status_error(code, message)),
            _ => Err(unexpected()),
        }
    }

    async fn expect_attrs(
        &self,
        build: impl FnOnce(u32) -> Request,
    ) -> Result<Attributes, SftpError> {
        match self.request(Expect::Attrs, build).await? {
            Response::Attrs { attributes, .. } => Ok(attributes),
            Response::Status { code, message, .. } => Err(status_error(code, message)),
            _ => Err(unexpected()),
        }
    }

    async fn expect_handle(&self, build: impl FnOnce(u32) -> Request) -> Result<Handle, SftpError> {
        match self.request(Expect::Handle, build).await? {
            Response::Handle { handle, .. } => Ok(Handle(handle)),
            Response::Status { code, message, .. } => Err(status_error(code, message)),
            _ => Err(unexpected()),
        }
    }

    async fn expect_one_name(
        &self,
        build: impl FnOnce(u32) -> Request,
    ) -> Result<RemotePath, SftpError> {
        match self.request(Expect::Name, build).await? {
            Response::Name { mut entries, .. } if entries.len() == 1 => {
                Ok(RemotePath::from_bytes(entries.remove(0).filename))
            }
            Response::Name { .. } => Err(SftpError::Closed(Closed::Protocol(
                "a name reply without exactly one name".to_owned(),
            ))),
            Response::Status { code, message, .. } => Err(status_error(code, message)),
            _ => Err(unexpected()),
        }
    }

    async fn fetch_limits(&self) -> Result<Limits, SftpError> {
        let reply = self
            .request(Expect::Extended, |id| Request::Extended {
                id,
                name: LIMITS.to_vec(),
                data: Vec::new(),
            })
            .await?;
        let Response::ExtendedReply { data, .. } = reply else {
            return Ok(Limits::default());
        };
        parse_limits(&data).map_err(|error| SftpError::Closed(Closed::Protocol(error.to_string())))
    }

    /// The canonical absolute form of `path`; `.` gives the starting directory.
    ///
    /// # Errors
    ///
    /// [`SftpError`].
    pub async fn realpath(&self, path: &RemotePath) -> Result<RemotePath, SftpError> {
        let path = path.clone();
        self.expect_one_name(|id| Request::Realpath { id, path })
            .await
    }

    /// Attributes of `path`, following symbolic links.
    ///
    /// # Errors
    ///
    /// [`SftpError`].
    pub async fn stat(&self, path: &RemotePath) -> Result<Attributes, SftpError> {
        let path = path.clone();
        self.expect_attrs(|id| Request::Stat { id, path }).await
    }

    /// Attributes of `path` itself, a symbolic link included.
    ///
    /// # Errors
    ///
    /// [`SftpError`].
    pub async fn lstat(&self, path: &RemotePath) -> Result<Attributes, SftpError> {
        let path = path.clone();
        self.expect_attrs(|id| Request::Lstat { id, path }).await
    }

    /// Attributes of an open file.
    ///
    /// # Errors
    ///
    /// [`SftpError`].
    pub async fn fstat(&self, handle: &Handle) -> Result<Attributes, SftpError> {
        let handle = handle.0.clone();
        self.expect_attrs(|id| Request::Fstat { id, handle }).await
    }

    /// Changes attributes of `path`.
    ///
    /// # Errors
    ///
    /// [`SftpError`].
    pub async fn setstat(
        &self,
        path: &RemotePath,
        attributes: Attributes,
    ) -> Result<(), SftpError> {
        let path = path.clone();
        self.expect_ok(|id| Request::Setstat {
            id,
            path,
            attributes,
        })
        .await
    }

    /// Opens a file with [`crate::protocol::open_flags`].
    ///
    /// # Errors
    ///
    /// [`SftpError`].
    pub async fn open(
        &self,
        path: &RemotePath,
        flags: u32,
        attributes: Attributes,
    ) -> Result<Handle, SftpError> {
        let path = path.clone();
        self.expect_handle(|id| Request::Open {
            id,
            path,
            flags,
            attributes,
        })
        .await
    }

    /// Closes a handle. Errors matter: a server may report a failed write only here.
    ///
    /// # Errors
    ///
    /// [`SftpError`].
    pub async fn close(&self, handle: &Handle) -> Result<(), SftpError> {
        let handle = handle.0.clone();
        self.expect_ok(|id| Request::Close { id, handle }).await
    }

    /// Reads up to `length` bytes at `offset`; `None` at the end of the file. A server may
    /// return fewer bytes than asked without being at the end.
    ///
    /// # Errors
    ///
    /// [`SftpError`]; more bytes than asked ends the session.
    pub async fn read(
        &self,
        handle: &Handle,
        offset: u64,
        length: u32,
    ) -> Result<Option<Vec<u8>>, SftpError> {
        let request_handle = handle.0.clone();
        let reply = self
            .request(Expect::Data, |id| Request::Read {
                id,
                handle: request_handle,
                offset,
                length,
            })
            .await?;
        match reply {
            Response::Data { data, .. }
                if data.len() <= usize::try_from(length).unwrap_or(usize::MAX) =>
            {
                Ok(Some(data))
            }
            Response::Data { .. } => {
                let reason = Closed::Protocol("more data than asked".to_owned());
                self.inner.core.close(&reason);
                Err(SftpError::Closed(reason))
            }
            Response::Status {
                code: StatusCode::Eof,
                ..
            } => Ok(None),
            Response::Status { code, message, .. } => Err(status_error(code, message)),
            _ => Err(unexpected()),
        }
    }

    /// Writes `data` at `offset`.
    ///
    /// # Errors
    ///
    /// [`SftpError`].
    pub async fn write(
        &self,
        handle: &Handle,
        offset: u64,
        data: Vec<u8>,
    ) -> Result<(), SftpError> {
        let handle = handle.0.clone();
        self.expect_ok(|id| Request::Write {
            id,
            handle,
            offset,
            data,
        })
        .await
    }

    /// Every entry of directory `path`, without `.` and `..`. The listing is closed on the
    /// server whatever happens.
    ///
    /// # Errors
    ///
    /// [`SftpError`], [`SftpError::ListingTooLarge`] past the configured limit.
    pub async fn read_dir(&self, path: &RemotePath) -> Result<Vec<DirEntry>, SftpError> {
        let request_path = path.clone();
        let handle = self
            .expect_handle(|id| Request::Opendir {
                id,
                path: request_path,
            })
            .await?;
        let listed = self.read_dir_pages(&handle).await;
        let closed = self.close(&handle).await;
        let entries = listed?;
        closed?;
        Ok(entries)
    }

    async fn read_dir_pages(&self, handle: &Handle) -> Result<Vec<DirEntry>, SftpError> {
        let mut entries = Vec::new();
        loop {
            let request_handle = handle.0.clone();
            let page = self
                .request(Expect::Name, |id| Request::Readdir {
                    id,
                    handle: request_handle,
                })
                .await?;
            match page {
                Response::Name { entries: page, .. } => {
                    for NameEntry {
                        filename,
                        attributes,
                        ..
                    } in page
                    {
                        if filename == b"." || filename == b".." {
                            continue;
                        }
                        if entries.len() == self.inner.max_listing_entries {
                            return Err(SftpError::ListingTooLarge(entries.len()));
                        }
                        entries.push(DirEntry {
                            name: filename,
                            attributes,
                        });
                    }
                }
                Response::Status {
                    code: StatusCode::Eof,
                    ..
                } => return Ok(entries),
                Response::Status { code, message, .. } => return Err(status_error(code, message)),
                _ => return Err(unexpected()),
            }
        }
    }

    /// Creates directory `path`.
    ///
    /// # Errors
    ///
    /// [`SftpError`].
    pub async fn mkdir(&self, path: &RemotePath, attributes: Attributes) -> Result<(), SftpError> {
        let path = path.clone();
        self.expect_ok(|id| Request::Mkdir {
            id,
            path,
            attributes,
        })
        .await
    }

    /// Deletes the empty directory `path`.
    ///
    /// # Errors
    ///
    /// [`SftpError`].
    pub async fn rmdir(&self, path: &RemotePath) -> Result<(), SftpError> {
        let path = path.clone();
        self.expect_ok(|id| Request::Rmdir { id, path }).await
    }

    /// Deletes file `path`; a symbolic link itself, never its target.
    ///
    /// # Errors
    ///
    /// [`SftpError`].
    pub async fn remove(&self, path: &RemotePath) -> Result<(), SftpError> {
        let path = path.clone();
        self.expect_ok(|id| Request::Remove { id, path }).await
    }

    /// Renames `from` to `to`. With `replace`, an existing `to` is replaced atomically,
    /// which needs the server's `posix-rename@openssh.com`; without it, an existing `to`
    /// makes the rename fail.
    ///
    /// # Errors
    ///
    /// [`SftpError`]; [`StatusCode::OpUnsupported`] when `replace` is asked of a server
    /// without the extension.
    pub async fn rename(
        &self,
        from: &RemotePath,
        to: &RemotePath,
        replace: bool,
    ) -> Result<(), SftpError> {
        if replace {
            if !self.has_extension(POSIX_RENAME) {
                return Err(status_error(StatusCode::OpUnsupported, Vec::new()));
            }
            let mut data = Writer::new();
            data.string(from.as_bytes()).string(to.as_bytes());
            let data = data.into_bytes();
            return match self
                .request(Expect::Extended, |id| Request::Extended {
                    id,
                    name: POSIX_RENAME.to_vec(),
                    data,
                })
                .await?
            {
                Response::Status {
                    code: StatusCode::Ok,
                    ..
                } => Ok(()),
                Response::Status { code, message, .. } => Err(status_error(code, message)),
                _ => Err(SftpError::Closed(Closed::Protocol(
                    "posix-rename answered with data".to_owned(),
                ))),
            };
        }
        let (from, to) = (from.clone(), to.clone());
        self.expect_ok(|id| Request::Rename { id, from, to }).await
    }

    /// Flushes an open file to the server's storage, when the server offers
    /// `fsync@openssh.com`; returns whether it did.
    ///
    /// # Errors
    ///
    /// [`SftpError`].
    pub async fn fsync(&self, handle: &Handle) -> Result<bool, SftpError> {
        if !self.has_extension(FSYNC) {
            return Ok(false);
        }
        let mut data = Writer::new();
        data.string(&handle.0);
        let data = data.into_bytes();
        match self
            .request(Expect::Extended, |id| Request::Extended {
                id,
                name: FSYNC.to_vec(),
                data,
            })
            .await?
        {
            Response::Status {
                code: StatusCode::Ok,
                ..
            } => Ok(true),
            Response::Status { code, message, .. } => Err(status_error(code, message)),
            _ => Err(SftpError::Closed(Closed::Protocol(
                "fsync answered with data".to_owned(),
            ))),
        }
    }

    /// The target of symbolic link `path`.
    ///
    /// # Errors
    ///
    /// [`SftpError`].
    pub async fn readlink(&self, path: &RemotePath) -> Result<RemotePath, SftpError> {
        let path = path.clone();
        self.expect_one_name(|id| Request::Readlink { id, path })
            .await
    }
}

fn spawn_tasks<R, W>(
    reader: R,
    writer: W,
    outgoing: mpsc::UnboundedReceiver<Outgoing>,
    core: Arc<Core>,
    max_packet: u32,
) where
    R: AsyncRead + Unpin + Send + 'static,
    W: AsyncWrite + Unpin + Send + 'static,
{
    let reading = core.clone();
    tokio::spawn(async move {
        let reason = tokio::select! {
            () = reading.stop.cancelled() => return,
            reason = read_responses(reader, &reading, max_packet) => reason,
        };
        reading.close(&reason);
    });
    let writing = core.clone();
    tokio::spawn(async move {
        let reason = tokio::select! {
            () = writing.stop.cancelled() => return,
            reason = write_requests(writer, outgoing, &writing) => reason,
        };
        writing.close(&reason);
    });
    tokio::spawn(async move {
        let reason = tokio::select! {
            () = core.stop.cancelled() => return,
            reason = watch_deadlines(&core) => reason,
        };
        core.close(&reason);
    });
}
