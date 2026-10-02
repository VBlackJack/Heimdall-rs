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

use std::io;
use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use heimdall_core::profile::{RdpProfile, SshProfile, display_address};
use heimdall_rdp::drives::local_drives;
use heimdall_rdp::session::{self, RdpEvent};
use heimdall_rdp::{
    AskCredentials, CloseReason, Fingerprint, KnownRdpHosts, Opening, RdpConfig, RdpConnection,
    RdpError, Security, Timeouts, Transport, connect, connect_through,
};
use heimdall_ssh::{
    AuthMethod, ConnectError, ConnectOptions, PasswordQuestion, UsernameQuestion, establish_via,
};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;

use crate::driver::{
    AnswerRegistry, ChannelPrompter, OpenForwards, ask, open_forwards, report_failure,
};
use crate::error::UiError;
use crate::event::{Answer, ConnectionEvent, QuestionKind};
use crate::text::server_text;

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
    /// Keys the user trusted for this server for this run only.
    pub trusted_for_run: Vec<Fingerprint>,
    /// Desktop size asked for.
    pub desktop: (u16, u16),
    /// The SSH gateways the server is reached through, nearest first, each as the hop it
    /// is; empty for a direct connection.
    pub route: Vec<SshProfile>,
    /// Settings of the SSH connections to the gateways.
    pub ssh: ConnectOptions,
    /// Cancels the attempt and, once connected, the session.
    pub cancel: CancellationToken,
}

/// Address this side reports to the server through a tunnel: it has none of its own there.
const TUNNEL_CLIENT_ADDRESS: SocketAddr =
    SocketAddr::new(std::net::IpAddr::V4(Ipv4Addr::LOCALHOST), 0);

/// Where an SSH failure on the way to the server waits to be reported as itself: the RDP
/// connection sees only that its stream could not be opened.
type TunnelFailure = Arc<Mutex<Option<ConnectError>>>;

/// Where the ports the profile opens through the gateway, opened with the tunnel, wait for
/// the session to end.
type ForwardsSlot = Arc<Mutex<Option<OpenForwards>>>;

/// The stream to the server through `route`, opened once the RDP connection asks for it, so
/// that credentials asked before the connection are asked before the tunnel too.
fn tunnel(
    request: &RdpRequest,
    prompter: Arc<ChannelPrompter>,
    failure: TunnelFailure,
    opened: ForwardsSlot,
) -> Opening {
    let route = request.route.clone();
    let forwards = request.profile.forwards;
    let options = request.ssh.clone();
    let cancel = request.cancel.clone();
    let host = request.profile.host.clone();
    let port = request.profile.port;
    Box::pin(async move {
        let opened = match route.split_last() {
            Some((last, before)) => {
                match establish_via(before, last, &options, prompter, cancel).await {
                    Ok(gateway) => match open_forwards(forwards, Some(&gateway)).await {
                        Ok(forwards) => {
                            if let Ok(mut slot) = opened.lock() {
                                *slot = Some(forwards);
                            }
                            gateway.open_tunnel(&host, port).await
                        }
                        Err(error) => Err(error),
                    },
                    Err(error) => Err(error),
                }
            }
            None => Err(ConnectError::Cancelled),
        };
        match opened {
            Ok(stream) => Ok((
                Box::new(stream) as Box<dyn Transport>,
                TUNNEL_CLIENT_ADDRESS,
            )),
            Err(error) => {
                let reason = error.to_string();
                if let Ok(mut slot) = failure.lock() {
                    *slot = Some(error);
                }
                Err(io::Error::other(reason))
            }
        }
    })
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
    let failure = TunnelFailure::default();
    // The forwards run as long as the session: they go when this attempt returns.
    let forwards = ForwardsSlot::default();
    let connecting = open(
        &request,
        &registry,
        &events,
        ask_credentials,
        &failure,
        &forwards,
    )
    .await;
    // A failure on the way to the server is reported as itself: an unknown gateway key asks
    // its question, a refusal names the host.
    if connecting.is_err()
        && let Some(error) = failure.lock().ok().and_then(|mut slot| slot.take())
    {
        return report_failure(error, &events, &target).await;
    }
    let connection = match connecting {
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
            clipboard: session.clipboard.clone(),
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
            RdpEvent::RemoteClipboard(text) => ConnectionEvent::RemoteClipboard(text),
            RdpEvent::Closed(CloseReason::Failed(detail)) => {
                log::warn!("RDP session to {target} failed: {detail:?}");
                ConnectionEvent::Failed(UiError::RdpProtocol { detail })
            }
            RdpEvent::Closed(CloseReason::Disconnected(reason)) => {
                log::info!("RDP session to {target} ended: {reason}");
                ConnectionEvent::Ended {
                    reason: server_text(&reason),
                }
            }
            RdpEvent::Closed(_) => {
                log::info!("RDP session to {target} ended");
                ConnectionEvent::Closed { exit_status: None }
            }
        };
        let last = matches!(
            event,
            ConnectionEvent::Closed { .. }
                | ConnectionEvent::Ended { .. }
                | ConnectionEvent::Failed(_)
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

/// What `request` asks of the RDP connection.
fn rdp_config(request: &RdpRequest) -> RdpConfig {
    let profile = &request.profile;
    RdpConfig {
        host: profile.host.clone(),
        port: profile.port,
        domain: profile.domain.clone(),
        desktop: request.desktop,
        keyboard_layout: crate::keyboard_layout::local(),
        security: if profile.allow_tls_only {
            Security::NlaOrTls
        } else {
            Security::Nla
        },
        known_hosts: KnownRdpHosts::new(&request.known_hosts),
        accepted: request.accepted,
        timeouts: Timeouts::default(),
        clipboard: profile.redirect_clipboard,
        drives: if profile.redirect_drives {
            local_drives()
        } else {
            Vec::new()
        },
        trusted_for_run: request.trusted_for_run.clone(),
        options: profile.options,
    }
}

/// Opens the RDP connection, over TCP or through the tunnel `request.route` leads to. An
/// SSH failure on the way lands in `failure`.
async fn open(
    request: &RdpRequest,
    registry: &AnswerRegistry,
    events: &mpsc::Sender<ConnectionEvent>,
    ask_credentials: AskCredentials,
    failure: &TunnelFailure,
    forwards: &ForwardsSlot,
) -> Result<RdpConnection, RdpError> {
    let config = rdp_config(request);
    if request.route.is_empty() {
        connect(config, ask_credentials, request.cancel.clone()).await
    } else {
        let prompter = Arc::new(ChannelPrompter {
            events: events.clone(),
            registry: registry.clone(),
        });
        let opening = tunnel(request, prompter, failure.clone(), forwards.clone());
        // No limit of its own: each SSH hop bounds its steps, and a gateway may ask for a
        // password, which a person types.
        connect_through(
            config,
            ask_credentials,
            request.cancel.clone(),
            opening,
            None,
        )
        .await
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
        RdpError::Network(error) => UiError::network(&error),
        RdpError::Timeout => UiError::Timeout,
        RdpError::Cancelled => UiError::Cancelled,
        RdpError::Negotiation(detail) => UiError::SecurityRefused { detail },
        RdpError::CertificateChanged {
            recorded,
            presented,
        } => UiError::HostKeyChanged {
            // The tab's own server: RDP goes through no gateway.
            target: None,
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

#[cfg(test)]
mod tests {
    use heimdall_core::profile::{AudioPlayback, ColorDepth, ProfileId, RdpOptions};

    use super::*;

    fn request(profile: RdpProfile) -> RdpRequest {
        RdpRequest {
            profile,
            known_hosts: PathBuf::from("known_rdp_hosts"),
            accepted: None,
            trusted_for_run: Vec::new(),
            desktop: (1024, 768),
            route: Vec::new(),
            ssh: ConnectOptions::new(PathBuf::from("known_hosts")),
            cancel: CancellationToken::new(),
        }
    }

    #[test]
    fn the_connection_is_given_the_profile_s_display_and_session_options() {
        let options = RdpOptions {
            color_depth: ColorDepth::Bpp24,
            audio: AudioPlayback::OnServer,
            admin_session: true,
            ..RdpOptions::default()
        };
        let config = rdp_config(&request(RdpProfile {
            id: ProfileId::new("dc"),
            name: "dc".to_owned(),
            group: None,
            host: "dc.lab".to_owned(),
            port: 3389,
            username: None,
            domain: None,
            allow_tls_only: false,
            gateway: None,
            redirect_clipboard: true,
            redirect_drives: false,
            options,
            vault_entry: None,
            forwards: heimdall_core::profile::Forwards::default(),
            follow_defaults: false,
        }));
        assert_eq!(config.options, options);
    }
}
