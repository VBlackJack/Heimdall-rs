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
use std::time::Duration;

use heimdall_core::profile::{FtpProfile, display_address};
use heimdall_files::{FtpClient, FtpConnectError, FtpSecurity, FtpTarget, RemoteSession};
use heimdall_rdp::CertificateHash;
use heimdall_ssh::{AuthMethod, PasswordQuestion};
use heimdall_tls::{PresentedSlot, connector};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_util::sync::CancellationToken;

use crate::certificate_pins::{CertificatePins, RecordSlot};
use crate::driver::{AnswerRegistry, ask};
use crate::error::UiError;
use crate::event::{Answer, ConnectionEvent, QuestionKind};

/// Events buffered before the attempt waits for the UI to read them.
const EVENT_QUEUE_LENGTH: usize = 64;

/// Longest wait for the server to answer the connection.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);

/// Passwords asked for at most, the first included, before the attempt gives up.
const PASSWORD_TRIES: u32 = 3;

/// The protocol, as the certificate log lines name it.
const PROTOCOL: &str = "FTPS";

impl FtpRequest {
    /// The pins of the profile's server.
    fn pins(&self) -> CertificatePins {
        CertificatePins {
            known_hosts: self.known_hosts.clone(),
            host: self.profile.host.clone(),
            port: self.profile.port,
            accepted: self.accepted,
            trusted_once: self.trusted_once,
            trusted_for_run: self.trusted_for_run.clone(),
            protocol: PROTOCOL,
        }
    }
}

/// What an FTP attempt needs.
#[derive(Debug, Clone)]
pub struct FtpRequest {
    /// Destination.
    pub profile: FtpProfile,
    /// File of the FTPS servers the user trusts.
    pub known_hosts: PathBuf,
    /// The certificate the user accepted after the certificate question, by the hash of the
    /// whole of it: this attempt takes it as it is, and records it once the server presents
    /// exactly it.
    pub accepted: Option<CertificateHash>,
    /// The certificate the user trusted for this run just now, after the certificate
    /// question, by its hash: this attempt takes it as it is, later ones check it as a pin.
    pub trusted_once: Option<CertificateHash>,
    /// Certificates the user trusted for this server for this run only, by their hash.
    pub trusted_for_run: Vec<CertificateHash>,
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
    let pins = request.pins();
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
        let to_record = RecordSlot::default();
        let security = if profile.tls {
            FtpSecurity::Explicit {
                connector: connector(pins.user_trust(to_record.clone()), presented.clone()),
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
                pins.record_trusted(&to_record)?;
                return Ok(Some(client));
            }
            Err(FtpConnectError::LoginRefused) if profile.username.is_some() => {}
            Err(FtpConnectError::LoginRefused) => {
                return Err(refused());
            }
            Err(FtpConnectError::Tls(detail)) => {
                return pins
                    .refused(&presented, events, UiError::Protocol { detail })
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
