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

//! VNC sessions over TCP: the handshake, then a task that keeps the desktop up to date and
//! carries the keyboard and the pointer.

use std::future::Future;
use std::io;
use std::pin::Pin;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncReadExt as _, AsyncWrite, AsyncWriteExt as _};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;

use super::protocol::{Rfb, RfbError, RfbEvent, SecurityPolicy};
use super::screen::{Rect, Screen};

/// Bound on reaching the server, unless the caller sets another.
pub const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

/// Bound on each exchange of the handshake, the time a person takes to type a password
/// apart.
pub const DEFAULT_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(30);

/// Bound on sending to the server: past it, the server has stopped reading.
const WRITE_TIMEOUT: Duration = Duration::from_secs(30);

/// Bytes read from the server at a time.
const READ_BUFFER: usize = 64 * 1024;

/// Events queued before the session waits for the receiver.
const EVENT_QUEUE: usize = 64;

/// Asks for the password, only when the server requires one. `None` cancels.
pub type AskPassword =
    Box<dyn FnOnce() -> Pin<Box<dyn Future<Output = Option<Zeroizing<String>>> + Send>> + Send>;

/// A password already known, for callers that have it.
#[must_use]
pub fn given_password(password: Zeroizing<String>) -> AskPassword {
    Box::new(move || Box::pin(std::future::ready(Some(password))))
}

/// Where to connect.
#[derive(Debug, Clone)]
pub struct VncConfig {
    /// Server.
    pub host: String,
    /// Port.
    pub port: u16,
    /// Which security is accepted.
    pub policy: SecurityPolicy,
    /// Bound on reaching the server.
    pub connect_timeout: Duration,
    /// Bound on each exchange of the handshake.
    pub handshake_timeout: Duration,
}

/// Why a connection did not open.
#[derive(Debug, thiserror::Error)]
pub enum VncError {
    /// The server cannot be reached, or the connection broke.
    #[error("the server cannot be reached: {0}")]
    Network(#[source] io::Error),
    /// The server took longer than the timeout.
    #[error("the server did not answer in time")]
    Timeout,
    /// Cancelled, or no password was given.
    #[error("the connection was cancelled")]
    Cancelled,
    /// The protocol refused to go on.
    #[error(transparent)]
    Rfb(#[from] RfbError),
}

/// The desktop, shared between the session and whoever draws it.
#[derive(Debug, Clone)]
pub struct Framebuffer(Arc<Mutex<Screen>>);

impl Framebuffer {
    fn new(width: u16, height: u16) -> Self {
        Self(Arc::new(Mutex::new(Screen::new(width, height))))
    }

    /// Calls `read` with the width, height and RGBA pixels, rows top to bottom.
    pub fn read<T>(&self, read: impl FnOnce(u16, u16, &[u8]) -> T) -> T {
        let screen = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        read(screen.width(), screen.height(), screen.pixels())
    }

    /// Copies `rect` of `source` in, or all of it when the size changed.
    fn update(&self, source: &Screen, rect: Option<Rect>) {
        let mut screen = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        let resized = (screen.width(), screen.height()) != (source.width(), source.height());
        match rect {
            Some(rect) if !resized => screen.copy_from(source, rect),
            _ => source.clone_into(&mut screen),
        }
    }
}

/// An open connection, handshake done.
pub struct VncConnection {
    stream: Box<dyn Transport>,
    rfb: Rfb,
    /// The desktop's name, as the server gives it: untrusted.
    pub name: String,
}

impl std::fmt::Debug for VncConnection {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("VncConnection")
            .field("rfb", &self.rfb)
            .finish_non_exhaustive()
    }
}

/// A byte stream to a server.
pub trait Transport: AsyncRead + AsyncWrite + Send + Unpin {}

impl<T: AsyncRead + AsyncWrite + Send + Unpin> Transport for T {}

/// Connects to `config.host` and runs the handshake.
///
/// # Errors
///
/// [`VncError`].
pub async fn connect(
    config: &VncConfig,
    password: AskPassword,
    cancel: &CancellationToken,
) -> Result<VncConnection, VncError> {
    let stream = tokio::select! {
        () = cancel.cancelled() => return Err(VncError::Cancelled),
        connected = tokio::time::timeout(
            config.connect_timeout,
            TcpStream::connect((config.host.as_str(), config.port)),
        ) => connected
            .map_err(|_| VncError::Timeout)?
            .map_err(VncError::Network)?,
    };
    // Keys and pointer moves go out as they come, not batched.
    let _ = stream.set_nodelay(true);
    handshake(Box::new(stream), config, password, cancel).await
}

/// Runs the handshake over `stream`, already connected to the server.
///
/// # Errors
///
/// [`VncError`].
pub async fn handshake(
    mut stream: Box<dyn Transport>,
    config: &VncConfig,
    password: AskPassword,
    cancel: &CancellationToken,
) -> Result<VncConnection, VncError> {
    let mut rfb = Rfb::new(config.policy);
    let mut password = Some(password);
    let mut buffer = vec![0; READ_BUFFER];
    loop {
        let read = tokio::select! {
            () = cancel.cancelled() => return Err(VncError::Cancelled),
            read = tokio::time::timeout(config.handshake_timeout, stream.read(&mut buffer)) => {
                read.map_err(|_| VncError::Timeout)?.map_err(VncError::Network)?
            }
        };
        if read == 0 {
            return Err(VncError::Network(io::ErrorKind::UnexpectedEof.into()));
        }
        for event in rfb.receive(&buffer[..read])? {
            match event {
                RfbEvent::PasswordRequired => {
                    let ask = password.take().ok_or(VncError::Cancelled)?;
                    // No timeout: a person is typing.
                    let answer = tokio::select! {
                        () = cancel.cancelled() => return Err(VncError::Cancelled),
                        answer = ask() => answer.ok_or(VncError::Cancelled)?,
                    };
                    rfb.answer_password(answer.as_bytes())?;
                }
                RfbEvent::Connected { name, .. } => {
                    send(&mut stream, &rfb.take_output(), cancel)
                        .await
                        .map_err(|reason| match reason {
                            CloseReason::Failed(detail) => {
                                VncError::Network(io::Error::other(detail))
                            }
                            _ => VncError::Cancelled,
                        })?;
                    return Ok(VncConnection { stream, rfb, name });
                }
                // Nothing else comes before the session opens.
                _ => {}
            }
        }
        send(&mut stream, &rfb.take_output(), cancel)
            .await
            .map_err(|reason| match reason {
                CloseReason::Failed(detail) => VncError::Network(io::Error::other(detail)),
                _ => VncError::Cancelled,
            })?;
    }
}

/// Why a session ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CloseReason {
    /// The server closed the connection.
    Server,
    /// The client ended it.
    Local,
    /// The connection or the protocol failed.
    Failed(String),
}

/// What a session reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VncEvent {
    /// Pixels changed in this rectangle of the framebuffer.
    Updated(Rect),
    /// The desktop changed size.
    Resized {
        /// Width.
        width: u16,
        /// Height.
        height: u16,
    },
    /// The server rang the bell.
    Bell,
    /// The server's clipboard: untrusted text.
    CutText(String),
    /// The session ended. Last event.
    Closed(CloseReason),
}

enum Command {
    Key { keysym: u32, down: bool },
    Pointer { buttons: u8, x: u16, y: u16 },
    CutText(String),
    Close,
}

/// The session ended; nothing more can be sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the VNC session has ended")]
pub struct SessionEnded;

/// The input side of a session. Calls only queue: none waits on the network. Pointer moves
/// queued faster than they are sent are merged, so the pointer does not lag behind.
#[derive(Clone)]
pub struct VncInput {
    commands: mpsc::UnboundedSender<Command>,
}

impl std::fmt::Debug for VncInput {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("VncInput")
    }
}

impl VncInput {
    /// Presses or releases the key `keysym`, an X11 keysym.
    ///
    /// # Errors
    ///
    /// [`SessionEnded`].
    pub fn key(&self, keysym: u32, down: bool) -> Result<(), SessionEnded> {
        self.send(Command::Key { keysym, down })
    }

    /// Moves the pointer with `buttons` held: buttons 1 to 8 as bits 0 to 7.
    ///
    /// # Errors
    ///
    /// [`SessionEnded`].
    pub fn pointer(&self, buttons: u8, x: u16, y: u16) -> Result<(), SessionEnded> {
        self.send(Command::Pointer { buttons, x, y })
    }

    /// Sends the clipboard.
    ///
    /// # Errors
    ///
    /// [`SessionEnded`].
    pub fn cut_text(&self, text: String) -> Result<(), SessionEnded> {
        self.send(Command::CutText(text))
    }

    /// Ends the session.
    pub fn close(&self) {
        let _ = self.commands.send(Command::Close);
    }

    fn send(&self, command: Command) -> Result<(), SessionEnded> {
        self.commands.send(command).map_err(|_| SessionEnded)
    }
}

/// A running session.
#[derive(Debug)]
pub struct VncSession {
    /// The desktop.
    pub framebuffer: Framebuffer,
    /// Where input goes.
    pub input: VncInput,
    /// What the session reports, ending with [`VncEvent::Closed`].
    pub events: mpsc::Receiver<VncEvent>,
}

/// Starts the session on an open connection.
#[must_use]
pub fn start(connection: VncConnection, cancel: CancellationToken) -> VncSession {
    let screen = connection.rfb.screen();
    let framebuffer = Framebuffer::new(screen.width(), screen.height());
    framebuffer.update(screen, None);
    let (commands, commands_received) = mpsc::unbounded_channel();
    let (events_sent, events) = mpsc::channel(EVENT_QUEUE);
    tokio::spawn(run(
        connection,
        framebuffer.clone(),
        commands_received,
        events_sent,
        cancel,
    ));
    VncSession {
        framebuffer,
        input: VncInput { commands },
        events,
    }
}

async fn run(
    connection: VncConnection,
    framebuffer: Framebuffer,
    mut commands: mpsc::UnboundedReceiver<Command>,
    events: mpsc::Sender<VncEvent>,
    cancel: CancellationToken,
) {
    let VncConnection {
        stream, mut rfb, ..
    } = connection;
    let (mut reader, mut writer) = tokio::io::split(stream);
    let mut buffer = vec![0; READ_BUFFER];
    // A command read while merging pointer moves, handled next.
    let mut held: Option<Command> = None;
    let reason = loop {
        let command = if let Some(command) = held.take() {
            Some(command)
        } else {
            tokio::select! {
                () = cancel.cancelled() => break CloseReason::Local,
                read = reader.read(&mut buffer) => {
                    match read {
                        Ok(0) => break CloseReason::Server,
                        Ok(read) => {
                            if let Err(reason) =
                                receive(&mut rfb, &buffer[..read], &framebuffer, &events, &cancel).await
                            {
                                break reason;
                            }
                        }
                        Err(error) => break CloseReason::Failed(error.to_string()),
                    }
                    None
                }
                command = commands.recv() => Some(command.unwrap_or(Command::Close)),
            }
        };
        if let Some(command) = command {
            match command {
                Command::Key { keysym, down } => rfb.key(keysym, down),
                Command::Pointer {
                    buttons,
                    mut x,
                    mut y,
                } => {
                    // Later moves with the same buttons replace this one.
                    while let Ok(next) = commands.try_recv() {
                        match next {
                            Command::Pointer {
                                buttons: next_buttons,
                                x: next_x,
                                y: next_y,
                            } if next_buttons == buttons => {
                                (x, y) = (next_x, next_y);
                            }
                            other => {
                                held = Some(other);
                                break;
                            }
                        }
                    }
                    rfb.pointer(buttons, x, y);
                }
                Command::CutText(text) => rfb.cut_text(&text),
                Command::Close => break CloseReason::Local,
            }
        }
        if let Err(reason) = send(&mut writer, &rfb.take_output(), &cancel).await {
            break reason;
        }
    };
    let _ = tokio::time::timeout(WRITE_TIMEOUT, writer.shutdown()).await;
    let closed = VncEvent::Closed(reason);
    if cancel.is_cancelled() {
        let _ = events.try_send(closed);
    } else {
        let _ = emit(&events, closed, &cancel).await;
    }
}

/// Feeds the server's bytes to the protocol and reports what they did.
async fn receive(
    rfb: &mut Rfb,
    bytes: &[u8],
    framebuffer: &Framebuffer,
    events: &mpsc::Sender<VncEvent>,
    cancel: &CancellationToken,
) -> Result<(), CloseReason> {
    let happened = rfb
        .receive(bytes)
        .map_err(|error| CloseReason::Failed(error.to_string()))?;
    for event in happened {
        let event = match event {
            RfbEvent::Updated(rect) => {
                framebuffer.update(rfb.screen(), Some(rect));
                VncEvent::Updated(rect)
            }
            RfbEvent::Resized { width, height } => {
                framebuffer.update(rfb.screen(), None);
                VncEvent::Resized { width, height }
            }
            RfbEvent::Bell => VncEvent::Bell,
            RfbEvent::ServerCutText(text) => VncEvent::CutText(text),
            // Only during the handshake.
            RfbEvent::PasswordRequired | RfbEvent::Connected { .. } => continue,
        };
        if !emit(events, event, cancel).await {
            return Err(CloseReason::Local);
        }
    }
    Ok(())
}

/// Queues `event`, unless the session is cancelled while the queue is full.
async fn emit(
    events: &mpsc::Sender<VncEvent>,
    event: VncEvent,
    cancel: &CancellationToken,
) -> bool {
    tokio::select! {
        () = cancel.cancelled() => false,
        sent = events.send(event) => sent.is_ok(),
    }
}

/// Sends `bytes`, unless the session is cancelled or the server stops reading.
async fn send<W: AsyncWrite + Unpin>(
    writer: &mut W,
    bytes: &[u8],
    cancel: &CancellationToken,
) -> Result<(), CloseReason> {
    if bytes.is_empty() {
        return Ok(());
    }
    let write = async {
        writer.write_all(bytes).await?;
        writer.flush().await
    };
    tokio::select! {
        () = cancel.cancelled() => Err(CloseReason::Local),
        written = tokio::time::timeout(WRITE_TIMEOUT, write) => match written {
            Ok(Ok(())) => Ok(()),
            Ok(Err(error)) => Err(CloseReason::Failed(error.to_string())),
            Err(_) => Err(CloseReason::Failed("the server stopped reading".to_owned())),
        },
    }
}
