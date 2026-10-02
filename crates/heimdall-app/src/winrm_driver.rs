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

//! A `WinRM` session, as the C# `WinRmHandler` opens one, then the local `PowerShell` entering
//! it. Reached directly, the server is probed first, as the C# preflight does. Through an SSH
//! gateway, the route comes first, its questions asked in the tab, then a forward on this
//! computer's loopback address to the server, which `PowerShell` enters through.
//!
//! The forward lives as long as the attempt: it closes when `PowerShell` exits, when the tab
//! closes, or when the attempt is cancelled.

use std::path::PathBuf;
use std::sync::Arc;

use heimdall_core::profile::{SshProfile, WinRmProfile};
use heimdall_core::winrm::{self, POWERSHELL_ARGUMENTS};
use heimdall_ssh::{ConnectOptions, TerminalSize, establish_via, local_forward};
use heimdall_term::local::LocalArguments;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_util::sync::CancellationToken;

use crate::driver::{AnswerRegistry, ChannelPrompter, report_failure};
use crate::error::UiError;
use crate::event::ConnectionEvent;
use crate::local_driver::{self, LocalRequest, LocalShell};
use crate::winrm_preflight::{TlsCheck, ensure_reachable};

/// Events buffered before the attempt waits for the UI to read them.
const EVENT_QUEUE_LENGTH: usize = 64;

/// What a `WinRM` attempt needs.
#[derive(Debug, Clone)]
pub struct WinRmRequest {
    /// The profile; its gateway is the last hop of `route`.
    pub profile: WinRmProfile,
    /// The `PowerShell` that enters the session.
    pub program: String,
    /// SSH hops to the server's network, outermost first; none for a server reached
    /// directly.
    pub route: Vec<SshProfile>,
    /// SSH options, known hosts included.
    pub ssh: ConnectOptions,
    /// Terminal size.
    pub size: TerminalSize,
    /// Where `PowerShell` starts, as a local shell does.
    pub fallback_directory: PathBuf,
    /// Stops the attempt and the session.
    pub cancel: CancellationToken,
}

/// Starts an attempt on the current tokio runtime. The stream ends after
/// [`ConnectionEvent::Closed`], [`ConnectionEvent::Failed`] or
/// [`ConnectionEvent::UnknownHostKey`].
#[must_use]
pub fn winrm_events(
    request: WinRmRequest,
    registry: AnswerRegistry,
) -> ReceiverStream<ConnectionEvent> {
    let (events, receiver) = mpsc::channel(EVENT_QUEUE_LENGTH);
    tokio::spawn(run(request, registry, events));
    ReceiverStream::new(receiver)
}

async fn run(
    request: WinRmRequest,
    registry: AnswerRegistry,
    events: mpsc::Sender<ConnectionEvent>,
) {
    let WinRmRequest {
        profile,
        program,
        route,
        ssh,
        size,
        fallback_directory,
        cancel,
    } = request;
    let target = format!("{}:{}", profile.host, profile.port);
    let launch = Launch {
        name: profile.name.clone(),
        program,
        size,
        fallback_directory,
        cancel: cancel.clone(),
    };
    let Some((last, before)) = route.split_last() else {
        return run_direct(&profile, launch, &events).await;
    };
    let prompter = Arc::new(ChannelPrompter {
        events: events.clone(),
        registry,
    });
    let gateway = match establish_via(before, last, &ssh, prompter, cancel.clone()).await {
        Ok(gateway) => gateway,
        Err(error) => return report_failure(error, &events, &target).await,
    };
    let forward =
        match local_forward::start(Arc::new(gateway), profile.host.clone(), profile.port).await {
            Ok(forward) => forward,
            Err(error) => {
                log::warn!("the WinRM forward to {target} could not listen: {error}");
                let failed = UiError::LocalShell {
                    detail: error.to_string(),
                };
                let _ = events.send(ConnectionEvent::Failed(failed)).await;
                return;
            }
        };
    log::info!(
        "WinRM to {target} through its gateway, forwarded at {}",
        forward.address()
    );
    // No preflight through a gateway, as the C#: it would only reach the forward.
    let command = winrm::session_command_through(&profile, forward.address());
    launch.start(command, events).await;
    // Held until PowerShell has gone: the session's connections ride on it.
    drop(forward);
}

/// A session reached directly: the C# preflight, then `PowerShell`.
async fn run_direct(
    profile: &WinRmProfile,
    launch: Launch,
    events: &mpsc::Sender<ConnectionEvent>,
) {
    let tls = match (profile.use_ssl, profile.skip_certificate_check) {
        (false, _) => TlsCheck::None,
        (true, false) => TlsCheck::System,
        (true, true) => TlsCheck::Skipped,
    };
    let reached = tokio::select! {
        () = launch.cancel.cancelled() => return,
        reached = ensure_reachable(&profile.host, profile.port, tls) => reached,
    };
    if let Err(failed) = reached {
        let _ = events.send(ConnectionEvent::Failed(failed)).await;
        return;
    }
    launch
        .start(winrm::session_command(profile), events.clone())
        .await;
}

/// What starting `PowerShell` needs, once the server is known to be there.
struct Launch {
    name: String,
    program: String,
    size: TerminalSize,
    fallback_directory: PathBuf,
    cancel: CancellationToken,
}

impl Launch {
    /// Runs `PowerShell` with `command`, until it ends; a command that cannot be written
    /// fails the attempt.
    async fn start(
        self,
        command: Result<String, winrm::CommandError>,
        events: mpsc::Sender<ConnectionEvent>,
    ) {
        let command = match command {
            Ok(command) => command,
            Err(error) => {
                // Checked before the attempt started: only a profile changed meanwhile lands
                // here.
                let _ = events
                    .send(ConnectionEvent::Failed(UiError::from(&error)))
                    .await;
                return;
            }
        };
        if self.cancel.is_cancelled() {
            return;
        }
        let shell = LocalRequest {
            shell: shell(self.name, self.program, command),
            size: self.size,
            fallback_directory: self.fallback_directory,
            cancel: self.cancel,
        };
        local_driver::run(shell, events).await;
    }
}

/// The local `PowerShell` running `command`, left open as [`POWERSHELL_ARGUMENTS`] ask.
#[must_use]
pub fn shell(name: String, program: String, command: String) -> LocalShell {
    let mut arguments: Vec<String> = POWERSHELL_ARGUMENTS
        .iter()
        .map(|argument| (*argument).to_owned())
        .collect();
    arguments.push(command);
    LocalShell {
        name,
        program: Some(program),
        arguments: LocalArguments::List(arguments),
        working_directory: None,
    }
}
