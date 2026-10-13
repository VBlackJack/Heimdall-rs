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
    AcceptedCertificate, AskCredentials, CertificateHash, CloseReason, Ending, KnownRdpHosts,
    Opening, Progress, RdpConfig, RdpConnection, RdpError, Security, ServerCertificate, Timeouts,
    Transport, connect, connect_through,
};
use heimdall_ssh::{
    ConnectError, ConnectOptions, PasswordQuestion, UsernameQuestion, establish_via,
};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;

use crate::driver::{
    AnswerRegistry, ChannelPrompter, OpenForwards, ask, open_forwards, report_failure,
};
use crate::error::UiError;
use crate::event::{Answer, CertificateDetails, ConnectionEvent, QuestionKind, Renewal};
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
    /// The certificate the user accepted after the certificate question.
    pub accepted: Option<AcceptedCertificate>,
    /// Certificates the user trusted for this server for this run only, by the hash of the
    /// whole of each.
    pub trusted_for_run: Vec<CertificateHash>,
    /// Desktop size asked for.
    pub desktop: (u16, u16),
    /// The desktop scale factor asked for, in percent; see
    /// [`heimdall_rdp::desktop_scale_factor`].
    pub desktop_scale: u32,
    /// How long logging on may take; `None` for no limit.
    pub logon_timeout: Option<std::time::Duration>,
    /// The SSH gateways the server is reached through, nearest first, each as the hop it
    /// is; empty for a direct connection.
    pub route: Vec<SshProfile>,
    /// Settings of the SSH connections to the gateways.
    pub ssh: ConnectOptions,
    /// Cancels the attempt and, once connected, the session.
    pub cancel: CancellationToken,
    /// The Credential Guard detection when the settings require it, `None` when they do
    /// not: the attempt then opens only while it runs. The application's gate asked it
    /// already; asked here too, a reconnection and an auto-reconnection are held to it
    /// whatever opened them.
    pub credential_guard: Option<Arc<crate::credential_guard::Detector>>,
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

/// What a running session's event is to the application; the session to `target`.
fn connection_event(event: RdpEvent, target: &str) -> ConnectionEvent {
    match event {
        RdpEvent::Updated { .. } | RdpEvent::Resized { .. } => ConnectionEvent::DesktopFrame,
        RdpEvent::RemoteClipboard(text) => ConnectionEvent::RemoteClipboard(text),
        RdpEvent::RemoteImage(image) => ConnectionEvent::RemoteImage(image.into()),
        RdpEvent::FilesRefused(refusal) => ConnectionEvent::RdpFilesRefused(refusal),
        RdpEvent::RemoteFiles(available) => ConnectionEvent::RdpRemoteFiles(available),
        RdpEvent::SaveProgress { saved, total } => {
            ConnectionEvent::RdpSaveProgress { saved, total }
        }
        RdpEvent::SaveEnded(end) => {
            log::info!("saving the files of {target} ended: {end:?}");
            ConnectionEvent::RdpSaveEnded(end)
        }
        RdpEvent::ResizeRefused { width, height } => {
            log::info!("RDP session to {target} cannot take {width}x{height} live");
            ConnectionEvent::DesktopResizeRefused { width, height }
        }
        RdpEvent::Closed(CloseReason::Failed(detail)) => {
            log::warn!("RDP session to {target} failed: {detail:?}");
            ConnectionEvent::Failed(UiError::RdpProtocol { detail })
        }
        RdpEvent::Closed(CloseReason::Disconnected(reason)) => {
            log::info!("RDP session to {target} ended: {reason:?}");
            ConnectionEvent::Ended {
                reason: safe(reason),
            }
        }
        RdpEvent::Closed(_) => {
            log::info!("RDP session to {target} ended");
            ConnectionEvent::Closed { exit_status: None }
        }
    }
}

async fn run(request: RdpRequest, registry: AnswerRegistry, events: mpsc::Sender<ConnectionEvent>) {
    let profile = &request.profile;
    let target = display_address(&profile.host, profile.port);
    // Before anything is asked or sent: the C# `RdpHandler` refuses before its tunnel.
    if let Some(detector) = &request.credential_guard
        && !detector.status().await.is_active()
    {
        log::warn!("Embedded RDP blocked: Credential Guard not enabled for {target}");
        return failed(&events, UiError::CredentialGuardRequired).await;
    }
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
                "{target} presented an unknown certificate {}, key {}",
                certificate.certificate,
                certificate.fingerprint
            );
            return ask_about(profile, &certificate, None, &events).await;
        }
        Err(RdpError::RenewedCertificate {
            presented,
            recorded,
        }) => {
            return ask_about(profile, &presented, Some(Renewal { recorded }), &events).await;
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
        let event = connection_event(event, &target);
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

/// The certificate question about `certificate`, presented by the server of `profile`;
/// `renewal` when it renews a certificate trusted on the same key.
async fn ask_about(
    profile: &RdpProfile,
    certificate: &ServerCertificate,
    renewal: Option<Renewal>,
    events: &mpsc::Sender<ConnectionEvent>,
) {
    // The C# RDP question shows the subject alone; a renewal says so, with both validities,
    // as the FTPS and VNC questions do. No authority of this computer was asked.
    let details = renewal.map(|renewal| CertificateDetails {
        issuer: certificate.issuer.clone(),
        validity: certificate.validity,
        issue: None,
        renewal: Some(renewal),
    });
    let _ = events
        .send(ConnectionEvent::UnknownRdpCertificate {
            host: profile.host.clone(),
            port: profile.port,
            fingerprint: certificate.fingerprint,
            certificate: certificate.certificate,
            subject: Some(certificate.subject.clone()).filter(|subject| !subject.trim().is_empty()),
            details: details.map(Box::new),
        })
        .await;
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
        timeouts: Timeouts {
            // A day stands for no limit: a longer one would overflow the clock.
            logon: request.logon_timeout.unwrap_or(UNBOUNDED_LOGON),
            ..Timeouts::default()
        },
        clipboard: profile.redirect_clipboard,
        drives: if profile.redirect_drives {
            local_drives()
        } else {
            Vec::new()
        },
        trusted_for_run: request.trusted_for_run.clone(),
        options: profile.options,
        several_servers: profile.several_servers,
        // The profile in effect: its own choice, or the application's default when it follows
        // them, as the C# `RdpProfileResolver` resolves it.
        strict_server_authentication: profile.extras.strict_server_authentication,
        kerberos: request.route.is_empty(),
        time_zone: crate::time_zone::local(),
        desktop_scale: request.desktop_scale,
        progress: None,
    }
}

/// Where a connection tells its steps: to the tab, as [`ConnectionEvent::RdpStep`]s. A step
/// that finds the queue full is dropped: the next one, or the session, says more.
fn progress(events: &mpsc::Sender<ConnectionEvent>) -> Progress {
    let events = events.clone();
    Progress::new(move |step| {
        let _ = events.try_send(ConnectionEvent::RdpStep(step));
    })
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
    let config = RdpConfig {
        progress: Some(progress(events)),
        ..rdp_config(request)
    };
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

/// `ending`, the server's own words in it made safe to show.
fn safe(ending: Ending) -> Ending {
    match ending {
        Ending::Other(words) => Ending::Other(server_text(&words)),
        known => known,
    }
}

/// The logon's limit when the settings give none.
const UNBOUNDED_LOGON: std::time::Duration = std::time::Duration::from_hours(24);

/// How an RDP failure is shown.
fn ui_error(error: RdpError) -> UiError {
    match error {
        RdpError::Network(error) => UiError::network(&error),
        RdpError::Timeout => UiError::Timeout,
        RdpError::Cancelled => UiError::Cancelled,
        RdpError::Negotiation { detail, code } => UiError::SecurityRefused { detail, code },
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
        RdpError::Authentication { refusal, status } => UiError::RdpRefused { refusal, status },
        RdpError::ServerNotAuthenticated => UiError::RdpServerNotAuthenticated,
        RdpError::Ended { ending, code } => UiError::RdpEnded {
            ending: safe(ending),
            code,
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
            desktop_scale: 100,
            logon_timeout: None,
            route: Vec::new(),
            ssh: ConnectOptions::new(PathBuf::from("known_hosts")),
            cancel: CancellationToken::new(),
            credential_guard: None,
        }
    }

    #[tokio::test]
    async fn a_renewed_certificate_is_asked_about_with_both_validities_an_unknown_one_with_its_subject()
     {
        let der = rcgen::generate_simple_self_signed(vec!["dc.lab".to_owned()])
            .expect("certificate")
            .cert
            .der()
            .to_vec();
        let presented = ServerCertificate::from_der(&der).expect("read");
        let recorded = heimdall_rdp::Validity {
            not_before: std::time::UNIX_EPOCH,
            not_after: std::time::UNIX_EPOCH + std::time::Duration::from_hours(24),
        };
        let (events, mut said) = mpsc::channel(2);
        ask_about(
            &dc(),
            &presented,
            Some(Renewal {
                recorded: Some(recorded),
            }),
            &events,
        )
        .await;
        ask_about(&dc(), &presented, None, &events).await;

        // Renewed: said so, with when the certificate on record and the one presented hold;
        // no authority of this computer was asked, so no issue is said.
        let Some(ConnectionEvent::UnknownRdpCertificate {
            fingerprint,
            certificate,
            details: Some(details),
            ..
        }) = said.recv().await
        else {
            panic!("the question about a renewal");
        };
        assert_eq!(
            (fingerprint, certificate),
            (presented.fingerprint, CertificateHash::of(&der)),
            "the answer trusts this very certificate"
        );
        assert_eq!(
            *details,
            CertificateDetails {
                issuer: presented.issuer.clone(),
                validity: presented.validity,
                issue: None,
                renewal: Some(Renewal {
                    recorded: Some(recorded),
                }),
            }
        );

        // Never seen: the subject alone, as the C# RDP question.
        let Some(ConnectionEvent::UnknownRdpCertificate {
            certificate,
            subject,
            details,
            ..
        }) = said.recv().await
        else {
            panic!("the question");
        };
        assert_eq!(certificate, CertificateHash::of(&der));
        assert_eq!(subject, Some(presented.subject.clone()));
        assert!(details.is_none());
    }

    /// A probe answering `status`.
    struct Answer(crate::credential_guard::Status);

    impl crate::credential_guard::Probe for Answer {
        fn detect(&self) -> crate::credential_guard::Status {
            self.0.clone()
        }
    }

    fn dc() -> RdpProfile {
        RdpProfile {
            extras: heimdall_core::profile::RdpExtras::default(),
            id: ProfileId::new("dc"),
            name: "dc".to_owned(),
            group: None,
            // Never reached: the gate refuses first.
            host: "dc.invalid".to_owned(),
            port: 3389,
            username: None,
            domain: None,
            allow_tls_only: false,
            gateway: None,
            local_tunnel_port: None,
            redirect_clipboard: true,
            redirect_drives: false,
            options: RdpOptions::default(),
            vault_entry: None,
            forwards: heimdall_core::profile::Forwards::default(),
            follow_defaults: false,
            several_servers: false,
            anti_idle: false,
            auto_reconnect: true,
        }
    }

    #[tokio::test]
    async fn an_attempt_requiring_credential_guard_is_refused_before_anything_without_it() {
        use crate::credential_guard::{Detector, Failure, Status};
        use tokio_stream::StreamExt as _;

        for status in [Status::Inactive, Status::Indeterminate(Failure::TimedOut)] {
            let mut request = request(dc());
            request.credential_guard = Some(Arc::new(Detector::with_probe(Arc::new(Answer(
                status.clone(),
            )))));
            let mut events = rdp_events(request, AnswerRegistry::default());
            let first = events.next().await;
            assert!(
                matches!(
                    first,
                    Some(ConnectionEvent::Failed(UiError::CredentialGuardRequired))
                ),
                "{status:?}: fail closed, nothing asked first"
            );
            assert!(events.next().await.is_none(), "{status:?}: nothing after");
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
            extras: heimdall_core::profile::RdpExtras::default(),
            id: ProfileId::new("dc"),
            name: "dc".to_owned(),
            group: None,
            host: "dc.lab".to_owned(),
            port: 3389,
            username: None,
            domain: None,
            allow_tls_only: false,
            gateway: None,
            local_tunnel_port: None,
            redirect_clipboard: true,
            redirect_drives: false,
            options,
            vault_entry: None,
            forwards: heimdall_core::profile::Forwards::default(),
            follow_defaults: false,
            several_servers: false,
            anti_idle: false,
            auto_reconnect: true,
        }));
        assert_eq!(config.options, options);
        assert!(
            config.kerberos,
            "a server reached directly may log on with Kerberos"
        );
    }
}
