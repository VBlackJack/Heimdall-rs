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

//! One VNC connection attempt, reported as [`ConnectionEvent`]s like the others: the
//! certificate question for a server encrypting with a certificate nobody trusts yet, the
//! password question when the server requires one, then the desktop.
//!
//! A VNC server's certificate is pinned as an FTPS one is: by its public key, in a file of its
//! own; one the system trusts for the server's name goes through without a pin. Once one is
//! pinned for the server, the connection must be in TLS: a server offering it no more is
//! refused, never answered in clear.

use std::path::PathBuf;

use heimdall_core::profile::{VncProfile, display_address};
use heimdall_rdp::Fingerprint;
use heimdall_remote::vnc::{
    self, AskPassword, CloseReason, DEFAULT_CONNECT_TIMEOUT, DEFAULT_HANDSHAKE_TIMEOUT, RfbError,
    SecurityPolicy, SecurityWrapper, VncConfig, VncConnection, VncError, VncEvent,
};
use heimdall_ssh::AuthMethod;
use heimdall_tls::{PresentedSlot, connector};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;

use crate::certificate_pins::{AcceptedSlot, CertificatePins};
use crate::driver::{AnswerRegistry, ask};
use crate::error::UiError;
use crate::event::{Answer, ConnectionEvent, QuestionKind, ServerPasswordQuestion};

/// Events buffered before the attempt waits for the UI to read them.
const EVENT_QUEUE_LENGTH: usize = 64;

/// Between the security codes a refusal lists.
const CODE_SEPARATOR: &str = ", ";

/// The protocol, as the certificate log lines name it.
const PROTOCOL: &str = "VNC";

/// What a VNC attempt needs.
#[derive(Debug, Clone)]
pub struct VncRequest {
    /// Destination.
    pub profile: VncProfile,
    /// File of the VNC servers whose certificate the user trusts.
    pub known_hosts: PathBuf,
    /// A key the user accepted after the certificate question, recorded once the server
    /// presents exactly it.
    pub accepted: Option<Fingerprint>,
    /// Keys the user trusted for this server for this run only.
    pub trusted_for_run: Vec<Fingerprint>,
    /// Cancels the attempt and, once connected, the session.
    pub cancel: CancellationToken,
}

impl VncRequest {
    /// The pins of the profile's server.
    fn pins(&self) -> CertificatePins {
        CertificatePins {
            known_hosts: self.known_hosts.clone(),
            host: self.profile.host.clone(),
            port: self.profile.port,
            accepted: self.accepted,
            trusted_for_run: self.trusted_for_run.clone(),
            protocol: PROTOCOL,
        }
    }
}

/// Starts an attempt on the current tokio runtime. The stream ends after
/// [`ConnectionEvent::Closed`], [`ConnectionEvent::Failed`] or
/// [`ConnectionEvent::UnknownRdpCertificate`].
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
    let connection = match open(&request, &registry, &events).await {
        Ok(Some(connection)) => connection,
        // The certificate question was sent: the attempt ends there.
        Ok(None) => return,
        Err(error) => {
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
    let tls = connection.tls_version();
    // Which security and TLS version, never a secret.
    match (connection.security(), tls) {
        (Some(security), Some(version)) => {
            log::info!("VNC session open to {target} with {security}, encrypted with {version}");
        }
        (Some(security), None) => {
            log::info!("VNC session open to {target} with {security}, not encrypted");
        }
        (None, _) => log::info!("VNC session open to {target}"),
    }
    let mut session = vnc::start(connection, request.cancel.clone());
    if events
        .send(ConnectionEvent::VncReady {
            name,
            framebuffer: session.framebuffer.clone(),
            input: session.input.clone(),
            tls,
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
            VncEvent::Renamed(name) => ConnectionEvent::DesktopRenamed(name),
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

/// Connects and runs the handshake, in TLS when the server offers it with a certificate;
/// `None` once the certificate question has been sent.
///
/// A server whose `VeNCrypt` offers nothing accepted inside, though it offered another type,
/// is connected to once more with `VeNCrypt` left out: `VeNCrypt` is chosen first, where TLS
/// is, and a wrapper chosen cannot be left on the same connection. Never when TLS is
/// required: the protocol refuses that server instead.
async fn open(
    request: &VncRequest,
    registry: &AnswerRegistry,
    events: &mpsc::Sender<ConnectionEvent>,
) -> Result<Option<VncConnection>, UiError> {
    let profile = &request.profile;
    let pins = request.pins();
    let ask_password = || -> AskPassword {
        let (profile, registry, events) = (profile.clone(), registry.clone(), events.clone());
        Box::new(move || Box::pin(async move { password(&profile, &registry, &events).await }))
    };
    let presented = PresentedSlot::default();
    let accepted = AcceptedSlot::default();
    let mut config = VncConfig {
        host: profile.host.clone(),
        port: profile.port,
        policy: SecurityPolicy {
            allow_no_authentication: profile.allow_no_password,
            require_tls: pins.pinned()?,
            exclude_vencrypt: false,
            username: profile.username.clone(),
        },
        tls: connector(pins.user_trust(accepted.clone()), presented.clone()),
        connect_timeout: DEFAULT_CONNECT_TIMEOUT,
        handshake_timeout: DEFAULT_HANDSHAKE_TIMEOUT,
    };
    let outcome = match vnc::connect(&config, ask_password(), &request.cancel).await {
        // Once only: left out, VeNCrypt cannot be chosen again.
        Err(VncError::Rfb(RfbError::RetryWithoutVencrypt(offered))) => {
            log::info!(
                "{} offers nothing accepted inside VeNCrypt (subtypes {}): connecting again once without VeNCrypt",
                display_address(&profile.host, profile.port),
                codes(offered)
            );
            config.policy.exclude_vencrypt = true;
            vnc::connect(&config, ask_password(), &request.cancel).await
        }
        outcome => outcome,
    };
    match outcome {
        Ok(connection) => {
            pins.record_accepted(&accepted)?;
            Ok(Some(connection))
        }
        Err(VncError::Tls(detail)) => pins
            .refused(&presented, events, UiError::VncProtocol { detail })
            .await
            .map(|()| None),
        Err(error @ VncError::Rfb(RfbError::AuthenticationFailed(_))) => {
            // The certificate went through: trusted, whatever the password.
            pins.record_accepted(&accepted)?;
            Err(ui_error(error))
        }
        Err(error) => Err(ui_error(error)),
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
        VncError::Rfb(RfbError::NoAcceptableSecurity(offered)) => UiError::VncSecurityRefused {
            offered: codes(offered.into_iter().map(u32::from)),
        },
        VncError::Rfb(RfbError::RetryWithoutVencrypt(offered)) => UiError::VncSecurityRefused {
            offered: format!("{} ({})", SecurityWrapper::VeNCrypt, codes(offered)),
        },
        VncError::Rfb(RfbError::NoAcceptableInnerSecurity { wrapper, offered }) => {
            UiError::VncSecurityRefused {
                offered: format!("{wrapper} ({})", codes(offered)),
            }
        }
        VncError::Rfb(RfbError::Refused(reason)) => UiError::Disconnected {
            server_message: Some(reason),
        },
        VncError::Rfb(RfbError::TlsRequired(offered)) => UiError::VncTlsRequired {
            offered: codes(offered),
        },
        VncError::Tls(detail) => UiError::VncProtocol { detail },
        VncError::Rfb(other) => UiError::VncProtocol {
            detail: other.to_string(),
        },
    }
}

/// Security codes as the server sent them, in its order.
fn codes(codes: impl IntoIterator<Item = u32>) -> String {
    codes
        .into_iter()
        .map(|code| code.to_string())
        .collect::<Vec<_>>()
        .join(CODE_SEPARATOR)
}
