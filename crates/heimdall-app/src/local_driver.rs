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

//! One local shell, reported as [`ConnectionEvent`]s like a remote session. There is nothing
//! to reach and nothing to ask: the shell starts at once or fails to.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use heimdall_ssh::{SessionClosed, TerminalSize};
use heimdall_term::local::{self, LocalArguments, LocalConfig, LocalEvent, LocalInput};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_util::sync::CancellationToken;

use crate::error::UiError;
use crate::event::ConnectionEvent;
use crate::sink::InputSink;

/// Events buffered before the session waits for the UI to read them.
const EVENT_QUEUE_LENGTH: usize = 64;

/// A program to run in a local terminal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalShell {
    /// Name shown to the user.
    pub name: String,
    /// The program; `None` for the user's default shell (`$SHELL` on Unix, `PowerShell` on
    /// Windows).
    pub program: Option<String>,
    /// Its arguments.
    pub arguments: LocalArguments,
    /// Folder it starts in; `None` for the one Files tabs start in.
    pub working_directory: Option<PathBuf>,
}

/// What a local attempt needs.
#[derive(Debug, Clone)]
pub struct LocalRequest {
    /// What to run.
    pub shell: LocalShell,
    /// Terminal size.
    pub size: TerminalSize,
    /// Where the shell starts when its folder is not there any more, as the C# Heimdall
    /// falls back: the home folder.
    pub fallback_directory: PathBuf,
    /// Hangs the shell up.
    pub cancel: CancellationToken,
}

/// Starts the shell on the current tokio runtime. The stream ends after
/// [`ConnectionEvent::Closed`] or [`ConnectionEvent::Failed`].
#[must_use]
pub fn local_events(request: LocalRequest) -> ReceiverStream<ConnectionEvent> {
    let (events, receiver) = mpsc::channel(EVENT_QUEUE_LENGTH);
    tokio::spawn(run(request, events));
    ReceiverStream::new(receiver)
}

async fn run(request: LocalRequest, events: mpsc::Sender<ConnectionEvent>) {
    let LocalRequest {
        shell,
        size,
        fallback_directory,
        cancel,
    } = request;
    let (columns, rows) = side_lengths(size);
    let config = LocalConfig {
        program: shell.program,
        arguments: shell.arguments,
        working_directory: Some(starting_folder(shell.working_directory, fallback_directory)),
        columns,
        rows,
    };
    // The program is named, never its arguments: they may carry anything.
    let program = config.program.as_deref().unwrap_or("default shell");
    let mut session = match local::spawn(&config) {
        Ok(session) => session,
        Err(error) => {
            log::warn!("local shell {program} failed to start: {error}");
            let failed = UiError::LocalShell {
                detail: error.to_string(),
            };
            let _ = events.send(ConnectionEvent::Failed(failed)).await;
            return;
        }
    };
    log::info!("local shell {program} started");
    let input: Arc<dyn InputSink> = Arc::new(LocalSink(session.input.clone()));
    if events
        .send(ConnectionEvent::Connected { input })
        .await
        .is_err()
    {
        session.input.close();
        return;
    }
    let mut hung_up = false;
    loop {
        let event = tokio::select! {
            event = session.events.recv() => event,
            () = cancel.cancelled(), if !hung_up => {
                // The exit still comes through the events, once the shell has gone.
                hung_up = true;
                session.input.close();
                continue;
            }
        };
        let (event, last) = match event {
            Some(LocalEvent::Output(bytes)) => (ConnectionEvent::Output(bytes), false),
            Some(LocalEvent::Exited(code)) => {
                log::info!("local shell {program} exited: {code:?}");
                let exit_status = code.and_then(|code| u32::try_from(code).ok());
                (ConnectionEvent::Closed { exit_status }, true)
            }
            // The session ends with its exit: a stream closing without it lost the shell.
            None => (ConnectionEvent::Closed { exit_status: None }, true),
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

/// `folder` when it is one, else `fallback`. Looked at only once the program is on its way:
/// a folder on another machine is not reached before the user has agreed to run it.
fn starting_folder(folder: Option<PathBuf>, fallback: PathBuf) -> PathBuf {
    match folder {
        Some(folder) if Path::is_dir(&folder) => folder,
        Some(folder) => {
            log::warn!(
                "local shell folder {} is not there; starting in {}",
                folder.display(),
                fallback.display()
            );
            fallback
        }
        None => fallback,
    }
}

/// A terminal size as the pseudo-terminal takes it; beyond 65535 is capped.
fn side_lengths(size: TerminalSize) -> (u16, u16) {
    (
        u16::try_from(size.cols).unwrap_or(u16::MAX),
        u16::try_from(size.rows).unwrap_or(u16::MAX),
    )
}

/// The input side of a local shell, as a terminal tab writes to it.
#[derive(Debug)]
struct LocalSink(LocalInput);

impl InputSink for LocalSink {
    fn write(&self, bytes: Vec<u8>) -> Result<(), SessionClosed> {
        self.0.write(bytes).map_err(|_| SessionClosed)
    }

    fn resize(&self, size: TerminalSize) -> Result<(), SessionClosed> {
        let (columns, rows) = side_lengths(size);
        self.0.resize(columns, rows).map_err(|_| SessionClosed)
    }

    fn close(&self) {
        self.0.close();
    }
}
