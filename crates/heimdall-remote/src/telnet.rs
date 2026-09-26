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

//! Telnet sessions over TCP: a terminal's bytes to and from a server, options negotiated by
//! [`Telnet`].
//!
//! Telnet has no encryption: whatever is typed, passwords included, crosses the network in
//! clear. It is here for equipment that speaks nothing else.

mod protocol;

use std::io;
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncReadExt as _, AsyncWrite, AsyncWriteExt as _};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

pub use protocol::{Received, Telnet, WindowSize};

/// Port a Telnet server listens on unless told otherwise.
pub const DEFAULT_TELNET_PORT: u16 = 23;

/// Bound on reaching the server, unless the caller sets another.
pub const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

/// Bytes read from the server at a time.
const READ_BUFFER: usize = 16 * 1024;

/// Events queued before the session waits for the receiver: output is not read from the
/// server faster than it is shown.
const EVENT_QUEUE: usize = 64;

/// Where to connect.
#[derive(Debug, Clone)]
pub struct TelnetConfig {
    /// Server.
    pub host: String,
    /// Port.
    pub port: u16,
    /// Terminal size, reported if the server asks.
    pub size: WindowSize,
    /// Bound on reaching the server.
    pub connect_timeout: Duration,
}

/// Why a connection did not open.
#[derive(Debug, thiserror::Error)]
pub enum TelnetError {
    /// The server cannot be reached.
    #[error("the server cannot be reached: {0}")]
    Network(#[source] io::Error),
    /// Reaching the server took longer than the timeout.
    #[error("the server did not answer in time")]
    Timeout,
    /// Cancelled before the connection opened.
    #[error("the connection was cancelled")]
    Cancelled,
}

/// Why a session ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CloseReason {
    /// The server closed the connection.
    Server,
    /// The client ended it: closed, cancelled, or nobody reads its output any more.
    Local,
    /// The connection failed.
    Failed(String),
}

/// What a session reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TelnetEvent {
    /// Data for the terminal.
    Output(Vec<u8>),
    /// The session ended. Last event.
    Closed(CloseReason),
}

/// The session ended; nothing more can be sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the Telnet session has ended")]
pub struct SessionEnded;

enum Command {
    Write(Vec<u8>),
    Resize(WindowSize),
    Close,
}

/// The input side of a session. Calls only queue: none waits on the network.
#[derive(Debug, Clone)]
pub struct TelnetInput {
    commands: mpsc::UnboundedSender<Command>,
}

impl std::fmt::Debug for Command {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // What is typed is never shown: it may be a password.
        match self {
            Self::Write(bytes) => write!(formatter, "Write({} bytes)", bytes.len()),
            Self::Resize(size) => write!(formatter, "Resize({size:?})"),
            Self::Close => formatter.write_str("Close"),
        }
    }
}

impl TelnetInput {
    /// Sends what was typed.
    ///
    /// # Errors
    ///
    /// [`SessionEnded`].
    pub fn write(&self, bytes: Vec<u8>) -> Result<(), SessionEnded> {
        self.commands
            .send(Command::Write(bytes))
            .map_err(|_| SessionEnded)
    }

    /// Reports a new terminal size.
    ///
    /// # Errors
    ///
    /// [`SessionEnded`].
    pub fn resize(&self, size: WindowSize) -> Result<(), SessionEnded> {
        self.commands
            .send(Command::Resize(size))
            .map_err(|_| SessionEnded)
    }

    /// Ends the session.
    pub fn close(&self) {
        let _ = self.commands.send(Command::Close);
    }
}

/// An open session.
#[derive(Debug)]
pub struct TelnetSession {
    /// Where input goes.
    pub input: TelnetInput,
    /// What the session reports, ending with [`TelnetEvent::Closed`].
    pub events: mpsc::Receiver<TelnetEvent>,
}

/// Connects to `config.host` and starts the session.
///
/// # Errors
///
/// [`TelnetError`].
pub async fn connect(
    config: &TelnetConfig,
    cancel: CancellationToken,
) -> Result<TelnetSession, TelnetError> {
    let stream = tokio::select! {
        () = cancel.cancelled() => return Err(TelnetError::Cancelled),
        connected = tokio::time::timeout(
            config.connect_timeout,
            TcpStream::connect((config.host.as_str(), config.port)),
        ) => connected
            .map_err(|_| TelnetError::Timeout)?
            .map_err(TelnetError::Network)?,
    };
    // Keystrokes go out as typed, not batched.
    let _ = stream.set_nodelay(true);
    Ok(start(stream, config.size, cancel))
}

/// Starts a session over `stream`, already connected to the server.
pub fn start<S>(stream: S, size: WindowSize, cancel: CancellationToken) -> TelnetSession
where
    S: AsyncRead + AsyncWrite + Send + Unpin + 'static,
{
    let (commands, commands_received) = mpsc::unbounded_channel();
    let (events_sent, events) = mpsc::channel(EVENT_QUEUE);
    tokio::spawn(run(
        stream,
        Telnet::new(size),
        commands_received,
        events_sent,
        cancel,
    ));
    TelnetSession {
        input: TelnetInput { commands },
        events,
    }
}

async fn run<S>(
    stream: S,
    mut telnet: Telnet,
    mut commands: mpsc::UnboundedReceiver<Command>,
    events: mpsc::Sender<TelnetEvent>,
    cancel: CancellationToken,
) where
    S: AsyncRead + AsyncWrite + Send + Unpin,
{
    let (mut reader, mut writer) = tokio::io::split(stream);
    let mut buffer = vec![0; READ_BUFFER];
    let reason = loop {
        tokio::select! {
            () = cancel.cancelled() => break CloseReason::Local,
            read = reader.read(&mut buffer) => {
                let read = match read {
                    Ok(0) => break CloseReason::Server,
                    Ok(read) => read,
                    Err(error) => break CloseReason::Failed(error.to_string()),
                };
                let received = telnet.receive(&buffer[..read]);
                if let Err(reason) = send(&mut writer, &received.reply).await {
                    break reason;
                }
                if !received.data.is_empty()
                    && events.send(TelnetEvent::Output(received.data)).await.is_err()
                {
                    break CloseReason::Local;
                }
            }
            command = commands.recv() => {
                let bytes = match command {
                    Some(Command::Write(bytes)) => telnet.encode_input(&bytes),
                    Some(Command::Resize(size)) => telnet.resize(size),
                    Some(Command::Close) | None => break CloseReason::Local,
                };
                if let Err(reason) = send(&mut writer, &bytes).await {
                    break reason;
                }
            }
        }
    };
    let _ = writer.shutdown().await;
    let _ = events.send(TelnetEvent::Closed(reason)).await;
}

async fn send<W: AsyncWrite + Unpin>(writer: &mut W, bytes: &[u8]) -> Result<(), CloseReason> {
    if bytes.is_empty() {
        return Ok(());
    }
    writer
        .write_all(bytes)
        .await
        .map_err(|error| CloseReason::Failed(error.to_string()))?;
    writer
        .flush()
        .await
        .map_err(|error| CloseReason::Failed(error.to_string()))
}
