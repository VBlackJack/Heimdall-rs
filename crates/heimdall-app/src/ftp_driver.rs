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

//! One FTP connection attempt, reported as [`ConnectionEvent`]s like the others: the
//! password question for an account, the certificate question for an explicit FTPS server
//! nobody trusts yet, then the Files session.
//!
//! An FTPS server is pinned as an RDP one is: by its certificate's public key, in a file of
//! its own, so a renewed certificate on the same key stays trusted. A certificate the system
//! trusts goes through without a pin, as in the C#.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use heimdall_core::profile::{FtpProfile, display_address};
use heimdall_files::ftps_trust::{PresentedSlot, UserTrust, connector};
use heimdall_files::{FtpClient, FtpConnectError, FtpSecurity, FtpTarget, RemoteSession};
use heimdall_rdp::{Fingerprint, KnownRdpHosts, ServerCertificate, Validity, Verdict};
use heimdall_ssh::{AuthMethod, PasswordQuestion};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_util::sync::CancellationToken;

use crate::driver::{AnswerRegistry, ask};
use crate::error::UiError;
use crate::event::{Answer, CertificateDetails, ConnectionEvent, QuestionKind};

/// Events buffered before the attempt waits for the UI to read them.
const EVENT_QUEUE_LENGTH: usize = 64;

/// Longest wait for the server to answer the connection.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);

/// Passwords asked for at most, the first included, before the attempt gives up.
const PASSWORD_TRIES: u32 = 3;

/// Where the certificate the user accepted is kept once the server presents it, so that
/// its subject and issuer are recorded with its key.
type AcceptedSlot = Arc<Mutex<Option<ServerCertificate>>>;

/// What an FTP attempt needs.
#[derive(Debug, Clone)]
pub struct FtpRequest {
    /// Destination.
    pub profile: FtpProfile,
    /// File of the FTPS servers the user trusts.
    pub known_hosts: PathBuf,
    /// A key the user accepted after the certificate question, recorded once the server
    /// presents exactly it.
    pub accepted: Option<Fingerprint>,
    /// Keys the user trusted for this server for this run only.
    pub trusted_for_run: Vec<Fingerprint>,
    /// Cancels the attempt.
    pub cancel: CancellationToken,
}

/// Starts an attempt on the current tokio runtime. The stream ends after
/// [`ConnectionEvent::FilesReady`], [`ConnectionEvent::Failed`] or
/// [`ConnectionEvent::UnknownRdpCertificate`].
#[must_use]
pub fn ftp_events(
    request: FtpRequest,
    registry: AnswerRegistry,
) -> ReceiverStream<ConnectionEvent> {
    let (events, receiver) = mpsc::channel(EVENT_QUEUE_LENGTH);
    tokio::spawn(run(request, registry, events));
    ReceiverStream::new(receiver)
}

async fn run(request: FtpRequest, registry: AnswerRegistry, events: mpsc::Sender<ConnectionEvent>) {
    let profile = &request.profile;
    let target = display_address(&profile.host, profile.port);
    log::info!("connecting to {target} over FTP");
    let outcome = tokio::select! {
        () = request.cancel.cancelled() => Err(UiError::Cancelled),
        outcome = connect(&request, &registry, &events) => outcome,
    };
    let event = match outcome {
        Ok(Some(client)) => {
            log::info!("FTP session open to {target}");
            ConnectionEvent::FilesReady {
                client: RemoteSession::Ftp(client),
                shell: None,
            }
        }
        // The certificate question was sent: the attempt ends there.
        Ok(None) => return,
        Err(error) => {
            if error == UiError::Cancelled {
                log::info!("FTP connection to {target} cancelled");
            } else {
                log::warn!("FTP connection to {target} failed: {error:?}");
            }
            ConnectionEvent::Failed(error)
        }
    };
    let _ = events.send(event).await;
}

/// Connects and logs in, asking for the password as many times as allowed; `None` once
/// the certificate question has been sent.
async fn connect(
    request: &FtpRequest,
    registry: &AnswerRegistry,
    events: &mpsc::Sender<ConnectionEvent>,
) -> Result<Option<FtpClient>, UiError> {
    let profile = &request.profile;
    for attempt in 1..=PASSWORD_TRIES {
        let password = match &profile.username {
            // Anonymous: no password is asked, as the C# with a blank name.
            None => String::new(),
            Some(username) => {
                let question = QuestionKind::Password(PasswordQuestion {
                    host: profile.host.clone(),
                    port: profile.port,
                    username: username.clone(),
                    attempt,
                });
                match ask(registry, events, question).await {
                    Some(Answer::Secret(secret)) => secret.expose().to_owned(),
                    _ => return Err(UiError::Cancelled),
                }
            }
        };
        let presented = PresentedSlot::default();
        let accepted = AcceptedSlot::default();
        let security = if profile.tls {
            FtpSecurity::Explicit {
                connector: connector(user_trust(request, accepted.clone()), presented.clone()),
                domain: profile.host.clone(),
            }
        } else {
            FtpSecurity::Plain
        };
        let target = FtpTarget {
            host: profile.host.clone(),
            port: profile.port,
            username: profile.username.clone(),
            password,
            passive: profile.passive,
            security,
            timeout: CONNECT_TIMEOUT,
        };
        match FtpClient::connect(&target).await {
            Ok(client) => {
                record_accepted(request, &accepted)?;
                return Ok(Some(client));
            }
            Err(FtpConnectError::LoginRefused) if profile.username.is_some() => {}
            Err(FtpConnectError::LoginRefused) => {
                return Err(refused());
            }
            Err(FtpConnectError::Tls(detail)) => {
                return certificate_refused(request, &presented, events, detail)
                    .await
                    .map(|()| None);
            }
            Err(FtpConnectError::Network(source)) => return Err(UiError::network(&source)),
            Err(FtpConnectError::Timeout) => return Err(UiError::Timeout),
            Err(FtpConnectError::Protocol(detail)) => return Err(UiError::Protocol { detail }),
        }
    }
    Err(refused())
}

fn refused() -> UiError {
    UiError::AuthenticationFailed {
        tried: vec![AuthMethod::Password],
        agent_keys: None,
    }
}

/// Whether the user trusts a certificate for the profile's server: pinned by its public
/// key, trusted for this run, or just accepted, and then kept in `slot`.
fn user_trust(request: &FtpRequest, slot: AcceptedSlot) -> UserTrust {
    let known = KnownRdpHosts::new(&request.known_hosts);
    let (host, port) = (request.profile.host.clone(), request.profile.port);
    let run = request.trusted_for_run.clone();
    let accepted = request.accepted;
    Arc::new(move |der| {
        let Ok(certificate) = ServerCertificate::from_der(der) else {
            return false;
        };
        let presented = certificate.fingerprint;
        match known.verdict(&host, port, &presented) {
            Ok(Verdict::Known) => true,
            Ok(Verdict::Unknown) if accepted == Some(presented) => {
                if let Ok(mut slot) = slot.lock() {
                    *slot = Some(certificate);
                }
                true
            }
            Ok(Verdict::Unknown) => run.contains(&presented),
            Ok(Verdict::Changed { .. }) | Err(_) => false,
        }
    })
}

/// Records the key the user accepted, once the server presented exactly it: with the
/// subject and issuer of its certificate when `slot` holds it.
fn record_accepted(request: &FtpRequest, slot: &AcceptedSlot) -> Result<(), UiError> {
    let Some(accepted) = request.accepted else {
        return Ok(());
    };
    let known = KnownRdpHosts::new(&request.known_hosts);
    let (host, port) = (&request.profile.host, request.profile.port);
    let certificate = slot.lock().ok().and_then(|mut slot| slot.take());
    match certificate.filter(|certificate| certificate.fingerprint == accepted) {
        Some(certificate) => known.record_certificate(host, port, &certificate),
        None => known.record(host, port, &accepted),
    }
    .map_err(|error| UiError::KnownHosts {
        detail: error.to_string(),
    })?;
    log::info!(
        "the FTPS certificate of {} accepted by the user is recorded: {accepted}",
        display_address(host, port)
    );
    Ok(())
}

/// The handshake stopped: the certificate question for a certificate nobody trusts yet, a
/// warning for one that changed, else the TLS failure as it is.
async fn certificate_refused(
    request: &FtpRequest,
    presented: &PresentedSlot,
    events: &mpsc::Sender<ConnectionEvent>,
    detail: String,
) -> Result<(), UiError> {
    let kept = presented.lock().ok().and_then(|mut slot| slot.take());
    let Some((certificate, details)) = kept.and_then(|kept| {
        let certificate = ServerCertificate::from_der(&kept.der).ok()?;
        let details = Validity::from_der(&kept.der)
            .ok()
            .map(|validity| CertificateDetails {
                issuer: certificate.issuer.clone(),
                validity,
                issue: kept
                    .issue
                    .refined(certificate.subject == certificate.issuer),
            });
        Some((certificate, details))
    }) else {
        return Err(UiError::Protocol { detail });
    };
    let profile = &request.profile;
    let presented = certificate.fingerprint;
    match KnownRdpHosts::new(&request.known_hosts).verdict(&profile.host, profile.port, &presented)
    {
        Ok(Verdict::Changed { recorded }) => {
            // As the C# "FTPS certificate rejected" line.
            log::warn!(
                "the FTPS certificate of {} changed: trusted {recorded}, presented {presented}: refused",
                display_address(&profile.host, profile.port)
            );
            Err(UiError::HostKeyChanged {
                target: None,
                recorded: recorded.to_string(),
                offered: presented.to_string(),
            })
        }
        Ok(_) => {
            log::info!(
                "{} presented an unknown certificate {presented}",
                display_address(&profile.host, profile.port)
            );
            let _ = events
                .send(ConnectionEvent::UnknownRdpCertificate {
                    host: profile.host.clone(),
                    port: profile.port,
                    fingerprint: presented,
                    // Shown in the question, as the C# FTPS one shows them.
                    subject: Some(certificate.subject.clone()),
                    details: details.map(Box::new),
                })
                .await;
            Ok(())
        }
        Err(error) => Err(UiError::KnownHosts {
            detail: error.to_string(),
        }),
    }
}
