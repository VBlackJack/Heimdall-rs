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

//! One VNC connection attempt, reported as [`ConnectionEvent`]s like the others: the password
//! question when the server requires one, then the desktop.

use heimdall_core::profile::{VncProfile, display_address};
use heimdall_remote::vnc::{
    self, AskPassword, CloseReason, DEFAULT_CONNECT_TIMEOUT, DEFAULT_HANDSHAKE_TIMEOUT, RfbError,
    SecurityPolicy, VncConfig, VncError, VncEvent,
};
use heimdall_ssh::AuthMethod;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;

use crate::driver::{AnswerRegistry, ask};
use crate::error::UiError;
use crate::event::{Answer, ConnectionEvent, QuestionKind, ServerPasswordQuestion};

/// Events buffered before the attempt waits for the UI to read them.
const EVENT_QUEUE_LENGTH: usize = 64;

/// What a VNC attempt needs.
#[derive(Debug, Clone)]
pub struct VncRequest {
    /// Destination.
    pub profile: VncProfile,
    /// Cancels the attempt and, once connected, the session.
    pub cancel: CancellationToken,
}

/// Starts an attempt on the current tokio runtime. The stream ends after
/// [`ConnectionEvent::Closed`] or [`ConnectionEvent::Failed`].
#[must_use]
pub fn vnc_events(
    request: VncRequest,
    registry: AnswerRegistry,
) -> ReceiverStream<ConnectionEvent> {
    let (events, receiver) = mpsc::channel(EVENT_QUEUE_LENGTH);
    tokio::spawn(run(request, registry, events));
    ReceiverStream::new(receiver)
}

async fn run(request: VncRequest, registry: AnswerRegistry, events: mpsc::Sender<ConnectionEvent>) {
    let profile = &request.profile;
    let target = display_address(&profile.host, profile.port);
    log::info!("connecting to {target} over VNC");
    let ask_password: AskPassword = {
        let (profile, registry, events) = (profile.clone(), registry.clone(), events.clone());
        Box::new(move || Box::pin(async move { password(&profile, &registry, &events).await }))
    };
    let config = VncConfig {
        host: profile.host.clone(),
        port: profile.port,
        policy: SecurityPolicy {
            allow_no_authentication: profile.allow_no_password,
        },
        connect_timeout: DEFAULT_CONNECT_TIMEOUT,
        handshake_timeout: DEFAULT_HANDSHAKE_TIMEOUT,
    };
    let connection = match vnc::connect(&config, ask_password, &request.cancel).await {
        Ok(connection) => connection,
        Err(error) => {
            let error = ui_error(error);
            if error == UiError::Cancelled {
                log::info!("VNC connection to {target} cancelled");
            } else {
                log::warn!("VNC connection to {target} failed: {error:?}");
            }
            let _ = events.send(ConnectionEvent::Failed(error)).await;
            return;
        }
    };
    let name = connection.name.clone();
    let mut session = vnc::start(connection, request.cancel.clone());
    log::info!("VNC session open to {target}");
    if events
        .send(ConnectionEvent::VncReady {
            name,
            framebuffer: session.framebuffer.clone(),
            input: session.input.clone(),
        })
        .await
        .is_err()
    {
        request.cancel.cancel();
        return;
    }
    while let Some(event) = session.events.recv().await {
        let event = match event {
            VncEvent::Updated(_) | VncEvent::Resized { .. } => ConnectionEvent::DesktopFrame,
            // What the server copied goes to this side's clipboard, as RDP's does.
            VncEvent::CutText(text) => ConnectionEvent::RemoteClipboard(Zeroizing::new(text)),
            // The bell is not used yet.
            VncEvent::Bell => continue,
            VncEvent::Closed(CloseReason::Failed(detail)) => {
                log::warn!("VNC session to {target} failed: {detail:?}");
                ConnectionEvent::Failed(UiError::VncProtocol { detail })
            }
            VncEvent::Closed(_) => {
                log::info!("VNC session to {target} ended");
                ConnectionEvent::Closed { exit_status: None }
            }
        };
        let last = matches!(
            event,
            ConnectionEvent::Closed { .. } | ConnectionEvent::Failed(_)
        );
        if events.send(event).await.is_err() {
            request.cancel.cancel();
            return;
        }
        if last {
            return;
        }
    }
}

/// The server's password, asked each time and never stored.
async fn password(
    profile: &VncProfile,
    registry: &AnswerRegistry,
    events: &mpsc::Sender<ConnectionEvent>,
) -> Option<Zeroizing<String>> {
    let question = QuestionKind::ServerPassword(ServerPasswordQuestion {
        host: profile.host.clone(),
        port: profile.port,
    });
    match ask(registry, events, question).await {
        Some(Answer::Secret(secret)) => Some(Zeroizing::new(secret.expose().to_owned())),
        _ => None,
    }
}

/// How a VNC failure is shown.
fn ui_error(error: VncError) -> UiError {
    match error {
        VncError::Network(error) => UiError::network(&error),
        VncError::Timeout => UiError::Timeout,
        VncError::Cancelled => UiError::Cancelled,
        VncError::Rfb(RfbError::AuthenticationFailed(_)) => UiError::AuthenticationFailed {
            tried: vec![AuthMethod::Password],
            agent_keys: None,
        },
        VncError::Rfb(RfbError::NoAcceptableSecurity(offered)) => UiError::SecurityRefused {
            detail: format!("security types offered: {offered:?}"),
        },
        VncError::Rfb(RfbError::Refused(reason)) => UiError::Disconnected {
            server_message: Some(reason),
        },
        VncError::Rfb(other) => UiError::VncProtocol {
            detail: other.to_string(),
        },
    }
}
