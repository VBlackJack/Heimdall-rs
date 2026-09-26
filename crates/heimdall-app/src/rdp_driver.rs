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

//! One RDP connection attempt, reported as [`ConnectionEvent`]s like an SSH one: questions
//! for the user name and the password, the certificate question, then the session.

use std::path::PathBuf;

use heimdall_core::profile::{RdpProfile, display_address};
use heimdall_rdp::session::{self, RdpEvent};
use heimdall_rdp::{
    AskCredentials, CloseReason, Fingerprint, KnownRdpHosts, RdpConfig, RdpError, Security,
    Timeouts, connect,
};
use heimdall_ssh::{AuthMethod, PasswordQuestion, UsernameQuestion};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;

use crate::driver::{AnswerRegistry, ask};
use crate::error::UiError;
use crate::event::{Answer, ConnectionEvent, QuestionKind};

/// Events buffered before the attempt waits for the UI to read them.
const EVENT_QUEUE_LENGTH: usize = 64;

/// Desktop size asked for until the tab tells its own.
pub const DEFAULT_DESKTOP: (u16, u16) = (1280, 800);

/// What an RDP attempt needs.
#[derive(Debug, Clone)]
pub struct RdpRequest {
    /// Destination.
    pub profile: RdpProfile,
    /// File of trusted RDP servers.
    pub known_hosts: PathBuf,
    /// A key the user accepted after the certificate question.
    pub accepted: Option<Fingerprint>,
    /// Desktop size asked for.
    pub desktop: (u16, u16),
    /// Cancels the attempt and, once connected, the session.
    pub cancel: CancellationToken,
}

/// Starts an attempt on the current tokio runtime. The stream ends after
/// [`ConnectionEvent::Closed`], [`ConnectionEvent::Failed`] or
/// [`ConnectionEvent::UnknownRdpCertificate`].
#[must_use]
pub fn rdp_events(
    request: RdpRequest,
    registry: AnswerRegistry,
) -> ReceiverStream<ConnectionEvent> {
    let (events, receiver) = mpsc::channel(EVENT_QUEUE_LENGTH);
    tokio::spawn(run(request, registry, events));
    ReceiverStream::new(receiver)
}

async fn run(request: RdpRequest, registry: AnswerRegistry, events: mpsc::Sender<ConnectionEvent>) {
    let profile = &request.profile;
    let target = display_address(&profile.host, profile.port);
    log::info!("connecting to {target} over RDP");
    let ask_credentials: AskCredentials = {
        let (profile, registry, events) = (profile.clone(), registry.clone(), events.clone());
        Box::new(move || Box::pin(async move { credentials(&profile, &registry, &events).await }))
    };
    let config = RdpConfig {
        host: profile.host.clone(),
        port: profile.port,
        domain: profile.domain.clone(),
        desktop: request.desktop,
        keyboard_layout: 0,
        security: if profile.allow_tls_only {
            Security::NlaOrTls
        } else {
            Security::Nla
        },
        known_hosts: KnownRdpHosts::new(&request.known_hosts),
        accepted: request.accepted,
        timeouts: Timeouts::default(),
    };
    let connection = match connect(config, ask_credentials, request.cancel.clone()).await {
        Ok(connection) => connection,
        Err(RdpError::UnknownCertificate(certificate)) => {
            log::info!(
                "{target} presented an unknown certificate {}",
                certificate.fingerprint
            );
            let _ = events
                .send(ConnectionEvent::UnknownRdpCertificate {
                    host: profile.host.clone(),
                    port: profile.port,
                    fingerprint: certificate.fingerprint,
                })
                .await;
            return;
        }
        Err(error) => {
            let error = ui_error(error);
            if error == UiError::Cancelled {
                log::info!("RDP connection to {target} cancelled");
            } else {
                log::warn!("RDP connection to {target} failed: {error:?}");
            }
            return failed(&events, error).await;
        }
    };
    let mut session = session::start(connection, request.cancel.clone());
    log::info!("RDP session open to {target}");
    if events
        .send(ConnectionEvent::RdpReady {
            framebuffer: session.framebuffer.clone(),
            input: session.input.clone(),
            size: session.size.clone(),
        })
        .await
        .is_err()
    {
        request.cancel.cancel();
        return;
    }
    while let Some(event) = session.events.recv().await {
        let event = match event {
            RdpEvent::Updated { .. } | RdpEvent::Resized { .. } => ConnectionEvent::DesktopFrame,
            RdpEvent::Closed(CloseReason::Failed(detail)) => {
                log::warn!("RDP session to {target} failed: {detail:?}");
                ConnectionEvent::Failed(UiError::RdpProtocol { detail })
            }
            RdpEvent::Closed(_) => {
                log::info!("RDP session to {target} ended");
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

/// The account and its password: the profile's user name, or the user's answer, then the
/// password, asked each time and never stored.
async fn credentials(
    profile: &RdpProfile,
    registry: &AnswerRegistry,
    events: &mpsc::Sender<ConnectionEvent>,
) -> Option<(String, Zeroizing<String>)> {
    let username = if let Some(name) = profile.username.clone().filter(|name| !name.is_empty()) {
        name
    } else {
        let question = QuestionKind::Username(UsernameQuestion {
            host: profile.host.clone(),
            port: profile.port,
        });
        match ask(registry, events, question).await {
            Some(Answer::Text(name)) if !name.is_empty() => name,
            _ => return None,
        }
    };
    let question = QuestionKind::Password(PasswordQuestion {
        host: profile.host.clone(),
        port: profile.port,
        username: username.clone(),
        attempt: 1,
    });
    match ask(registry, events, question).await {
        Some(Answer::Secret(secret)) => {
            Some((username, Zeroizing::new(secret.expose().to_owned())))
        }
        _ => None,
    }
}

async fn failed(events: &mpsc::Sender<ConnectionEvent>, error: UiError) {
    let _ = events.send(ConnectionEvent::Failed(error)).await;
}

/// How an RDP failure is shown.
fn ui_error(error: RdpError) -> UiError {
    match error {
        RdpError::Network(error) => UiError::Network {
            detail: error.to_string(),
        },
        RdpError::Timeout => UiError::Timeout,
        RdpError::Cancelled => UiError::Cancelled,
        RdpError::Negotiation(detail) => UiError::SecurityRefused { detail },
        RdpError::CertificateChanged {
            recorded,
            presented,
        } => UiError::HostKeyChanged {
            recorded: recorded.to_string(),
            offered: presented.fingerprint.to_string(),
        },
        RdpError::KnownHosts(error) => UiError::KnownHosts {
            detail: error.to_string(),
        },
        RdpError::Authentication => UiError::AuthenticationFailed {
            tried: vec![AuthMethod::Password],
        },
        // An unknown certificate is a question, handled by the caller; the rest is a protocol
        // failure described in plain words.
        other => UiError::RdpProtocol {
            detail: other.to_string(),
        },
    }
}
