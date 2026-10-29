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

/// Bound on sending to the server: past it, the server has stopped reading.
const WRITE_TIMEOUT: Duration = Duration::from_secs(30);

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
///
/// The queue has no bound, on purpose: a bounded one would have to drop keystrokes or make
/// the caller wait, and what is queued is what the user typed or pasted, already in memory.
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
    // Keepalive: a session idle on a path that died silently ends with an error, instead of
    // showing connected until something is typed.
    let stream = open(
        heimdall_net::connect_kept_alive(&config.host, config.port),
        config.connect_timeout,
        &cancel,
    )
    .await?;
    // Keystrokes go out as typed, not batched.
    let _ = stream.set_nodelay(true);
    Ok(start(stream, config.size, cancel))
}

/// `dial` within `timeout`: the time running out is [`TelnetError::Timeout`], and only
/// `cancel` is [`TelnetError::Cancelled`], so a timeout is never told as the user's cancel.
async fn open<S>(
    dial: impl Future<Output = io::Result<S>>,
    timeout: Duration,
    cancel: &CancellationToken,
) -> Result<S, TelnetError> {
    tokio::select! {
        () = cancel.cancelled() => Err(TelnetError::Cancelled),
        connected = tokio::time::timeout(timeout, dial) => connected
            .map_err(|_| TelnetError::Timeout)?
            .map_err(TelnetError::Network),
    }
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
                if let Err(reason) = send(&mut writer, &received.reply, &cancel).await {
                    break reason;
                }
                if !received.data.is_empty()
                    && !emit(&events, TelnetEvent::Output(received.data), &cancel).await
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
                if let Err(reason) = send(&mut writer, &bytes, &cancel).await {
                    break reason;
                }
            }
        }
    };
    let _ = tokio::time::timeout(WRITE_TIMEOUT, writer.shutdown()).await;
    let closed = TelnetEvent::Closed(reason);
    if cancel.is_cancelled() {
        // Whoever cancelled knows; a receiver that stopped reading is not waited for.
        let _ = events.try_send(closed);
    } else {
        let _ = emit(&events, closed, &cancel).await;
    }
}

/// Queues `event`, unless the session is cancelled while the queue is full. Whether it was
/// queued.
async fn emit(
    events: &mpsc::Sender<TelnetEvent>,
    event: TelnetEvent,
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A time limit short enough for a test, long against a dial that never ends.
    const SHORT: Duration = Duration::from_millis(50);

    /// A time limit no test reaches.
    const NEVER: Duration = Duration::from_secs(3600);

    /// A dial to an address that drops every packet: it never ends.
    fn blackholed() -> impl Future<Output = io::Result<()>> {
        std::future::pending()
    }

    #[tokio::test]
    async fn a_connection_that_does_not_answer_in_time_is_a_timeout_not_a_cancel() {
        let cancel = CancellationToken::new();
        let outcome = open(blackholed(), SHORT, &cancel).await;
        assert!(matches!(outcome, Err(TelnetError::Timeout)), "{outcome:?}");
        assert!(!cancel.is_cancelled());
    }

    #[tokio::test]
    async fn only_the_users_cancel_is_a_cancel() {
        let cancel = CancellationToken::new();
        let dial = open(blackholed(), NEVER, &cancel);
        cancel.cancel();
        let outcome = dial.await;
        assert!(
            matches!(outcome, Err(TelnetError::Cancelled)),
            "{outcome:?}"
        );
    }

    #[tokio::test]
    async fn a_refusal_is_a_network_error() {
        let refused = async { Err::<(), _>(io::Error::from(io::ErrorKind::ConnectionRefused)) };
        let outcome = open(refused, NEVER, &CancellationToken::new()).await;
        assert!(
            matches!(
                &outcome,
                Err(TelnetError::Network(error)) if error.kind() == io::ErrorKind::ConnectionRefused
            ),
            "{outcome:?}"
        );
    }
}
