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

//! An open interactive shell.
//!
//! Nothing the caller does waits on the SSH side: [`SessionInput`] only drops requests into
//! queues. This matters because russh carries data, resizes and its own bookkeeping through
//! one command queue per connection. A caller that awaited a resize while not reading output
//! deadlocked at exactly 2 MiB, the SSH window: the resize waited for the russh loop, the
//! loop waited for its output buffer to drain, and the buffer waited for the caller
//! (measured on 2026-09-26, 2 hangs in 3 runs).
//!
//! Four tasks:
//!
//! - the reader forwards output; when the caller reads slowly it stops reading, russh stops
//!   reading TCP, and the server slows down: that is the only backpressure there is, because
//!   russh reopens the SSH window as soon as a packet arrives;
//! - the writer sends input, waiting for the server's window when it must;
//! - the resizer applies the latest requested size; sizes requested meanwhile collapse into
//!   the last one;
//! - the keeper holds a use of the connection; once the session is cancelled it closes
//!   this channel and lets the connection go, which disconnects when nothing else uses it.

use std::sync::Arc;

use russh::client::Msg;
use russh::{Channel, ChannelMsg, ChannelReadHalf, ChannelWriteHalf};
use thiserror::Error;
use tokio::sync::{mpsc, watch};
use tokio_util::sync::CancellationToken;

use crate::connection::Connection;
use crate::error::ConnectError;
use crate::options::{ConnectOptions, TerminalSize};

/// Output messages buffered before the reader waits for the UI.
const OUTPUT_QUEUE_LENGTH: usize = 256;

/// Something that happened on the session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionEvent {
    /// Bytes from the remote shell, standard output and error interleaved as received.
    Output(Vec<u8>),
    /// The session ended. Always the last event.
    Closed {
        /// Exit status of the remote shell, when the server reported one.
        exit_status: Option<u32>,
    },
}

/// The session is closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("session closed")]
pub struct SessionClosed;

/// Sends input to the session. Cheap to clone.
#[derive(Clone)]
pub struct SessionInput {
    data: mpsc::UnboundedSender<Vec<u8>>,
    size: Arc<watch::Sender<TerminalSize>>,
    cancel: CancellationToken,
}

impl std::fmt::Debug for SessionInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionInput")
            .field("closed", &self.cancel.is_cancelled())
            .finish_non_exhaustive()
    }
}

impl SessionInput {
    /// Queues bytes for the remote shell. Never blocks.
    ///
    /// # Errors
    ///
    /// [`SessionClosed`] once the session has ended.
    pub fn write(&self, bytes: Vec<u8>) -> Result<(), SessionClosed> {
        if self.cancel.is_cancelled() {
            return Err(SessionClosed);
        }
        self.data.send(bytes).map_err(|_| SessionClosed)
    }

    /// Tells the server the terminal changed size. Never blocks; when sizes arrive faster
    /// than they can be sent, only the last one is.
    ///
    /// # Errors
    ///
    /// [`SessionClosed`] once the session has ended.
    pub fn resize(&self, size: TerminalSize) -> Result<(), SessionClosed> {
        if self.cancel.is_cancelled() {
            return Err(SessionClosed);
        }
        self.size.send(size).map_err(|_| SessionClosed)
    }

    /// Ends the session. The event stream then delivers [`SessionEvent::Closed`].
    pub fn close(&self) {
        self.cancel.cancel();
    }
}

/// An open shell: input on one side, events on the other.
#[derive(Debug)]
pub struct ShellSession {
    /// Sends input, resizes, closes.
    pub input: SessionInput,
    /// Output and the final close.
    pub events: mpsc::Receiver<SessionEvent>,
}

async fn expect_success(
    channel: &mut Channel<Msg>,
    refusal: ConnectError,
) -> Result<(), ConnectError> {
    loop {
        match channel.wait().await {
            Some(ChannelMsg::Success) => return Ok(()),
            Some(ChannelMsg::Failure) => return Err(refusal),
            Some(_) => {}
            None => {
                return Err(ConnectError::Disconnected {
                    server_message: None,
                });
            }
        }
    }
}

/// Forwards output until the channel closes or the session is cancelled, then reports the
/// close and cancels the other tasks.
async fn read_output(
    mut reader: ChannelReadHalf,
    events: mpsc::Sender<SessionEvent>,
    cancel: CancellationToken,
) {
    let mut exit_status = None;
    loop {
        let message = tokio::select! {
            () = cancel.cancelled() => break,
            message = reader.wait() => message,
        };
        match message {
            Some(ChannelMsg::Data { data } | ChannelMsg::ExtendedData { data, .. }) => {
                if events
                    .send(SessionEvent::Output(data.to_vec()))
                    .await
                    .is_err()
                {
                    break;
                }
            }
            Some(ChannelMsg::ExitStatus {
                exit_status: status,
            }) => exit_status = Some(status),
            Some(ChannelMsg::Close) | None => break,
            Some(_) => {}
        }
    }
    cancel.cancel();
    let _ = events.send(SessionEvent::Closed { exit_status }).await;
}

/// Sends queued input; a write waiting for the server's window never blocks a resize.
async fn write_input(
    writer: Arc<ChannelWriteHalf<Msg>>,
    mut input: mpsc::UnboundedReceiver<Vec<u8>>,
    cancel: CancellationToken,
) {
    loop {
        let bytes = tokio::select! {
            () = cancel.cancelled() => break,
            bytes = input.recv() => match bytes {
                Some(bytes) => bytes,
                None => break,
            },
        };
        tokio::select! {
            () = cancel.cancelled() => break,
            written = writer.data_bytes(bytes) => if written.is_err() { break },
        }
    }
}

/// Applies the latest requested size.
async fn apply_resizes(
    writer: Arc<ChannelWriteHalf<Msg>>,
    mut size: watch::Receiver<TerminalSize>,
    cancel: CancellationToken,
) {
    loop {
        tokio::select! {
            () = cancel.cancelled() => break,
            changed = size.changed() => if changed.is_err() { break },
        }
        let latest = *size.borrow_and_update();
        tokio::select! {
            () = cancel.cancelled() => break,
            sent = writer.window_change(
                latest.cols,
                latest.rows,
                latest.pixel_width,
                latest.pixel_height,
            ) => if sent.is_err() { break },
        }
    }
}

/// Holds the connection for the life of the session; once it is cancelled, closes this
/// channel and releases the connection.
async fn keep_connection(
    connection: Connection,
    writer: Arc<ChannelWriteHalf<Msg>>,
    cancel: CancellationToken,
) {
    cancel.cancelled().await;
    let _ = writer.close().await;
    drop(connection);
}

pub(crate) async fn open(
    connection: Connection,
    options: &ConnectOptions,
    cancel: CancellationToken,
) -> Result<ShellSession, ConnectError> {
    let mut channel = connection
        .handle()
        .channel_open_session()
        .await
        .map_err(ConnectError::Protocol)?;
    let size = options.initial_size;
    channel
        .request_pty(
            true,
            &options.terminal_type,
            size.cols,
            size.rows,
            size.pixel_width,
            size.pixel_height,
            &[],
        )
        .await
        .map_err(ConnectError::Protocol)?;
    expect_success(&mut channel, ConnectError::PtyRefused).await?;
    channel
        .request_shell(true)
        .await
        .map_err(ConnectError::Protocol)?;
    expect_success(&mut channel, ConnectError::ShellRefused).await?;

    let (reader, writer) = channel.split();
    let writer = Arc::new(writer);
    let (events_tx, events_rx) = mpsc::channel(OUTPUT_QUEUE_LENGTH);
    let (data_tx, data_rx) = mpsc::unbounded_channel::<Vec<u8>>();

    let (size_tx, size_rx) = watch::channel(options.initial_size);

    tokio::spawn(read_output(reader, events_tx, cancel.clone()));
    tokio::spawn(write_input(writer.clone(), data_rx, cancel.clone()));
    tokio::spawn(apply_resizes(writer.clone(), size_rx, cancel.clone()));
    tokio::spawn(keep_connection(connection, writer, cancel.clone()));

    Ok(ShellSession {
        input: SessionInput {
            data: data_tx,
            size: Arc::new(size_tx),
            cancel,
        },
        events: events_rx,
    })
}
