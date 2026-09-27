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

//! One Telnet connection attempt, reported as [`ConnectionEvent`]s like an SSH one. It asks
//! nothing: a Telnet server asks for an account itself, as text in the terminal.

use std::sync::Arc;

use heimdall_core::profile::{TelnetProfile, display_address};
use heimdall_remote::telnet::{
    self, CloseReason, DEFAULT_CONNECT_TIMEOUT, TelnetConfig, TelnetError, TelnetEvent,
    TelnetInput, WindowSize,
};
use heimdall_ssh::{SessionClosed, TerminalSize};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_util::sync::CancellationToken;

use crate::error::{NetworkFailure, UiError};
use crate::event::ConnectionEvent;
use crate::sink::InputSink;

/// Events buffered before the attempt waits for the UI to read them.
const EVENT_QUEUE_LENGTH: usize = 64;

/// What a Telnet attempt needs.
#[derive(Debug, Clone)]
pub struct TelnetRequest {
    /// Destination.
    pub profile: TelnetProfile,
    /// Terminal size, reported if the server asks.
    pub size: TerminalSize,
    /// Cancels the attempt and, once connected, the session.
    pub cancel: CancellationToken,
}

/// Starts an attempt on the current tokio runtime. The stream ends after
/// [`ConnectionEvent::Closed`] or [`ConnectionEvent::Failed`].
#[must_use]
pub fn telnet_events(request: TelnetRequest) -> ReceiverStream<ConnectionEvent> {
    let (events, receiver) = mpsc::channel(EVENT_QUEUE_LENGTH);
    tokio::spawn(run(request, events));
    ReceiverStream::new(receiver)
}

async fn run(request: TelnetRequest, events: mpsc::Sender<ConnectionEvent>) {
    let TelnetRequest {
        profile,
        size,
        cancel,
    } = request;
    let target = display_address(&profile.host, profile.port);
    log::info!("connecting to {target} over Telnet");
    let config = TelnetConfig {
        host: profile.host,
        port: profile.port,
        size: window_size(size),
        connect_timeout: DEFAULT_CONNECT_TIMEOUT,
    };
    let mut session = match telnet::connect(&config, cancel).await {
        Ok(session) => session,
        Err(error) => {
            log::warn!("Telnet connection to {target} failed: {error}");
            let _ = events.send(ConnectionEvent::Failed(ui_error(error))).await;
            return;
        }
    };
    log::info!("Telnet session open to {target}");
    let input: Arc<dyn InputSink> = Arc::new(TelnetSink(session.input.clone()));
    if events
        .send(ConnectionEvent::Connected { input })
        .await
        .is_err()
    {
        session.input.close();
        return;
    }
    while let Some(event) = session.events.recv().await {
        let (event, last) = match event {
            TelnetEvent::Output(bytes) => (ConnectionEvent::Output(bytes), false),
            TelnetEvent::Closed(CloseReason::Server | CloseReason::Local) => {
                (ConnectionEvent::Closed { exit_status: None }, true)
            }
            TelnetEvent::Closed(CloseReason::Failed(detail)) => {
                log::warn!("Telnet session to {target} failed: {detail}");
                let failure = NetworkFailure::Other;
                (
                    ConnectionEvent::Failed(UiError::Network { failure, detail }),
                    true,
                )
            }
        };
        if events.send(event).await.is_err() {
            // Nobody shows this tab any more.
            session.input.close();
            return;
        }
        if last {
            return;
        }
    }
}

fn ui_error(error: TelnetError) -> UiError {
    match error {
        TelnetError::Network(error) => UiError::network(&error),
        TelnetError::Timeout => UiError::Timeout,
        TelnetError::Cancelled => UiError::Cancelled,
    }
}

/// A terminal size in the form Telnet reports it; beyond 65535 columns or rows is capped.
fn window_size(size: TerminalSize) -> WindowSize {
    WindowSize {
        columns: u16::try_from(size.cols).unwrap_or(u16::MAX),
        rows: u16::try_from(size.rows).unwrap_or(u16::MAX),
    }
}

/// The input side of a Telnet session, as a terminal tab writes to it.
#[derive(Debug)]
struct TelnetSink(TelnetInput);

impl InputSink for TelnetSink {
    fn write(&self, bytes: Vec<u8>) -> Result<(), SessionClosed> {
        self.0.write(bytes).map_err(|_| SessionClosed)
    }

    fn resize(&self, size: TerminalSize) -> Result<(), SessionClosed> {
        self.0.resize(window_size(size)).map_err(|_| SessionClosed)
    }

    fn close(&self) {
        self.0.close();
    }
}
