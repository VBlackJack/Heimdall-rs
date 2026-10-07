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

//! The certificates the user trusted for the servers of the TLS clients that ask: FTPS and
//! VNC. A server is pinned as an RDP one is: by its certificate's public key, in a file per
//! protocol, so a renewed certificate on the same key stays trusted. A certificate the
//! system trusts goes through without a pin, as in the C#.
//!
//! The check runs in the TLS handshake ([`heimdall_tls`]); what it could not decide comes
//! back here as the certificate question, or as a refusal when the key changed.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use heimdall_core::profile::display_address;
use heimdall_rdp::{Fingerprint, KnownRdpHosts, ServerCertificate, Validity, Verdict};
use heimdall_tls::{PresentedSlot, UserTrust};
use tokio::sync::mpsc;

use crate::error::UiError;
use crate::event::{CertificateDetails, ConnectionEvent};

/// Where the certificate the user accepted is kept once the server presents it, so that
/// its subject and issuer are recorded with its key.
pub(crate) type AcceptedSlot = Arc<Mutex<Option<ServerCertificate>>>;

/// The pins of one server, and what the user decided of it in this run.
#[derive(Debug, Clone)]
pub(crate) struct CertificatePins {
    /// File of the servers of this protocol the user trusts.
    pub known_hosts: PathBuf,
    /// Host.
    pub host: String,
    /// Port.
    pub port: u16,
    /// A key the user accepted after the certificate question, recorded once the server
    /// presents exactly it.
    pub accepted: Option<Fingerprint>,
    /// Keys the user trusted for this server for this run only.
    pub trusted_for_run: Vec<Fingerprint>,
    /// The protocol, as the log names it.
    pub protocol: &'static str,
}

impl CertificatePins {
    /// Whether the user trusts a certificate for the server: pinned by its public key,
    /// trusted for this run, or just accepted, and then kept in `slot`.
    pub(crate) fn user_trust(&self, slot: AcceptedSlot) -> UserTrust {
        let known = KnownRdpHosts::new(&self.known_hosts);
        let (host, port) = (self.host.clone(), self.port);
        let run = self.trusted_for_run.clone();
        let accepted = self.accepted;
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

    /// Whether a certificate is pinned for the server, or was accepted for it now: its
    /// connection must then be in TLS, never in clear.
    ///
    /// # Errors
    ///
    /// [`UiError::KnownHosts`] when the file cannot be read.
    pub(crate) fn pinned(&self) -> Result<bool, UiError> {
        if self.accepted.is_some() || !self.trusted_for_run.is_empty() {
            return Ok(true);
        }
        KnownRdpHosts::new(&self.known_hosts)
            .knows(&self.host, self.port)
            .map_err(|error| UiError::KnownHosts {
                detail: error.to_string(),
            })
    }

    /// Records the key the user accepted, once the server presented exactly it: with the
    /// subject and issuer of its certificate when `slot` holds it.
    pub(crate) fn record_accepted(&self, slot: &AcceptedSlot) -> Result<(), UiError> {
        let Some(accepted) = self.accepted else {
            return Ok(());
        };
        let known = KnownRdpHosts::new(&self.known_hosts);
        let (host, port) = (&self.host, self.port);
        let certificate = slot.lock().ok().and_then(|mut slot| slot.take());
        match certificate.filter(|certificate| certificate.fingerprint == accepted) {
            Some(certificate) => known.record_certificate(host, port, &certificate),
            None => known.record(host, port, &accepted),
        }
        .map_err(|error| UiError::KnownHosts {
            detail: error.to_string(),
        })?;
        log::info!(
            "the {} certificate of {} accepted by the user is recorded: {accepted}",
            self.protocol,
            display_address(host, port)
        );
        Ok(())
    }

    /// The handshake stopped: the certificate question for a certificate nobody trusts yet,
    /// a refusal for one that changed, else `failed`, the TLS failure as it is.
    pub(crate) async fn refused(
        &self,
        presented: &PresentedSlot,
        events: &mpsc::Sender<ConnectionEvent>,
        failed: UiError,
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
            return Err(failed);
        };
        let target = display_address(&self.host, self.port);
        let presented = certificate.fingerprint;
        match KnownRdpHosts::new(&self.known_hosts).verdict(&self.host, self.port, &presented) {
            Ok(Verdict::Changed { recorded }) => {
                // As the C# "FTPS certificate rejected" line.
                log::warn!(
                    "the {} certificate of {target} changed: trusted {recorded}, presented {presented}: refused",
                    self.protocol
                );
                Err(UiError::HostKeyChanged {
                    target: None,
                    recorded: recorded.to_string(),
                    offered: presented.to_string(),
                })
            }
            Ok(_) => {
                log::info!("{target} presented an unknown certificate {presented}");
                let _ = events
                    .send(ConnectionEvent::UnknownRdpCertificate {
                        host: self.host.clone(),
                        port: self.port,
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
}
