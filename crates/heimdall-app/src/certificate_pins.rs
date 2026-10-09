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
//! VNC, in a file per protocol. A server is pinned as the C# pins it: by its whole
//! certificate (the C# thumbprint), its key recorded beside it. A certificate the system
//! trusts goes through without a pin, as in the C#.
//!
//! Another certificate on the same key, renewed or minted again by whoever holds the key,
//! is no longer taken silently: the user is asked again, told it is a renewal. Another key
//! is the key-changed alarm, as before.
//!
//! The check runs in the TLS handshake ([`heimdall_tls`]); what it could not decide comes
//! back here as the certificate question, or as a refusal when the key changed.
//!
//! A pin, on file or for this run, is checked again at every connection, as the C# checks
//! its pins (`FtpBrowser.cs` `EnsurePinnedCertificateRemainsValid`): a certificate no longer
//! valid is refused, and never asked about again. Only the attempt built after the user's
//! answer to the question takes the certificate as it is, and only the very certificate the
//! user was asked about.
//!
//! A server recorded by an earlier Heimdall, which pinned keys alone, has no certificate on
//! record. The first connection whose certificate carries the recorded key, and passes every
//! check a pin gets, adopts that certificate silently, logged: that connection goes as it
//! would have gone before, and the next ones take that certificate only. Asking instead
//! would ask every user about every server once, for no reason they could weigh.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, UNIX_EPOCH};

use heimdall_core::profile::display_address;
use heimdall_rdp::{
    CertificateHash, CertificateVerdict, KnownRdpHosts, ServerCertificate, Validity,
    ValidityPeriod, serves_tls_servers,
};
use heimdall_tls::{Period, PresentedSlot, UnixTime, UserTrust, UserVerdict};
use tokio::sync::mpsc;

use crate::error::UiError;
use crate::event::{CertificateDetails, ConnectionEvent, Renewal};

/// A certificate as the handshake presented it, read: its key and names, the hash of the
/// whole of it, and when it holds.
#[derive(Debug, Clone)]
pub(crate) struct SeenCertificate {
    /// Its key and names.
    certificate: ServerCertificate,
    /// The hash of the whole certificate.
    hash: CertificateHash,
    /// When it holds, when read.
    validity: Option<Validity>,
}

impl SeenCertificate {
    /// The DER certificate `der` read; `None` when it does not parse.
    fn read(der: &[u8]) -> Option<Self> {
        Some(Self {
            certificate: ServerCertificate::from_der(der).ok()?,
            hash: CertificateHash::of(der),
            validity: Validity::from_der(der).ok(),
        })
    }
}

/// What the file is to record once the connection is through, every check passed.
#[derive(Debug, Clone)]
pub(crate) enum ToRecord {
    /// The certificate the user accepted to trust after the question.
    Accepted(SeenCertificate),
    /// The certificate presented on a key recorded alone, by an earlier Heimdall: adopted.
    Adopted(SeenCertificate),
}

/// Where the handshake leaves what to record once the connection is through.
pub(crate) type RecordSlot = Arc<Mutex<Option<ToRecord>>>;

/// The pins of one server, and what the user decided of it in this run.
#[derive(Debug, Clone)]
pub(crate) struct CertificatePins {
    /// File of the servers of this protocol the user trusts.
    pub known_hosts: PathBuf,
    /// Host.
    pub host: String,
    /// Port.
    pub port: u16,
    /// The certificate the user accepted after the certificate question, by its hash:
    /// this attempt takes it as it is, and records it once the server presented exactly
    /// it.
    pub accepted: Option<CertificateHash>,
    /// The certificate the user trusted for this run just now, after the certificate
    /// question, by its hash: this attempt takes it as it is, as the C# "Trust once" does;
    /// later ones check it as a pin.
    pub trusted_once: Option<CertificateHash>,
    /// Certificates the user trusted for this server for this run only, by their hash.
    pub trusted_for_run: Vec<CertificateHash>,
    /// The protocol, as the log names it.
    pub protocol: &'static str,
}

/// How the user trusts a certificate pinned, on file or for this run, at `now`: where `now`
/// falls in its validity period, and whether it may serve a TLS server, read here because
/// the system's check never reaches its purpose for a certificate authority serving as its
/// own server's certificate; untrusted when its validity cannot be read, not for servers
/// when its purposes cannot.
fn pinned_at(der: &[u8], now: UnixTime) -> UserVerdict {
    let Ok(validity) = Validity::from_der(der) else {
        return UserVerdict::Untrusted;
    };
    let period = match validity.period(UNIX_EPOCH + Duration::from_secs(now.as_secs())) {
        ValidityPeriod::Current => Period::Current,
        ValidityPeriod::Expired => Period::Expired,
        ValidityPeriod::NotYetValid => Period::NotYetValid,
    };
    UserVerdict::Pinned {
        period,
        for_servers: serves_tls_servers(der).unwrap_or(false),
    }
}

/// Leaves `record` in `slot`, for once the connection is through.
fn leave(slot: &RecordSlot, record: ToRecord) {
    if let Ok(mut slot) = slot.lock() {
        *slot = Some(record);
    }
}

impl CertificatePins {
    /// How the user trusts a certificate for the server: pinned by the whole of it, or
    /// trusted for this run, checked again; or decided on just now, after the question,
    /// this very certificate. What to record once the connection is through is left in
    /// `slot`: the certificate accepted, or one adopted for a key recorded alone.
    pub(crate) fn user_trust(&self, slot: RecordSlot) -> UserTrust {
        let known = KnownRdpHosts::new(&self.known_hosts);
        let (host, port) = (self.host.clone(), self.port);
        let run = self.trusted_for_run.clone();
        let (accepted, once) = (self.accepted, self.trusted_once);
        Arc::new(move |der, now| {
            // What an earlier handshake of this connector left is not this one's.
            if let Ok(mut left) = slot.lock() {
                *left = None;
            }
            let Some(seen) = SeenCertificate::read(der) else {
                return UserVerdict::Untrusted;
            };
            let hash = seen.hash;
            match known.certificate_verdict(&host, port, &seen.certificate.fingerprint, &hash) {
                Ok(CertificateVerdict::Known) => pinned_at(der, now),
                // Adopted once the connection is through: only if every check passes.
                Ok(CertificateVerdict::KeyOnly) => {
                    leave(&slot, ToRecord::Adopted(seen));
                    pinned_at(der, now)
                }
                Ok(CertificateVerdict::Unknown | CertificateVerdict::Renewed { .. })
                    if accepted == Some(hash) =>
                {
                    leave(&slot, ToRecord::Accepted(seen));
                    UserVerdict::Decided
                }
                Ok(CertificateVerdict::Unknown | CertificateVerdict::Renewed { .. })
                    if once == Some(hash) =>
                {
                    UserVerdict::Decided
                }
                Ok(CertificateVerdict::Unknown | CertificateVerdict::Renewed { .. })
                    if run.contains(&hash) =>
                {
                    pinned_at(der, now)
                }
                Ok(
                    CertificateVerdict::Unknown
                    | CertificateVerdict::Renewed { .. }
                    | CertificateVerdict::Changed { .. },
                )
                | Err(_) => UserVerdict::Untrusted,
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
        if self.accepted.is_some()
            || self.trusted_once.is_some()
            || !self.trusted_for_run.is_empty()
        {
            return Ok(true);
        }
        KnownRdpHosts::new(&self.known_hosts)
            .knows(&self.host, self.port)
            .map_err(|error| UiError::KnownHosts {
                detail: error.to_string(),
            })
    }

    /// The connection went through, every check passed: records what the handshake left in
    /// `slot`. The certificate the user accepted, when the server presented exactly it,
    /// whole, with its key, names and validity; or the certificate adopted for a key an
    /// earlier Heimdall recorded alone.
    ///
    /// The certificate recorded is the one of the connection just completed, though the
    /// connection is not asked for it (neither the FTP client nor the VNC session exposes
    /// its peer's certificate): the slot belongs to one attempt and one connector, every
    /// handshake on it first empties it, and the drivers call this right after the login,
    /// before any other handshake on that connector, so the slot holds what the one
    /// completed handshake presented. FTPS opens its data channels after this call only
    /// ([`heimdall_files::FtpClient::connect`] secures the control channel, logs in, sets
    /// the transfer type and asks FEAT, none of which opens a data channel), and a VNC
    /// attempt makes a single TLS handshake (connecting again without `VeNCrypt` makes
    /// none).
    pub(crate) fn record_trusted(&self, slot: &RecordSlot) -> Result<(), UiError> {
        let Some(to_record) = slot.lock().ok().and_then(|mut slot| slot.take()) else {
            return Ok(());
        };
        let known = KnownRdpHosts::new(&self.known_hosts);
        let (host, port) = (&self.host, self.port);
        let target = display_address(host, port);
        let failed = |error: std::io::Error| UiError::KnownHosts {
            detail: error.to_string(),
        };
        match to_record {
            ToRecord::Accepted(seen) if self.accepted == Some(seen.hash) => {
                known
                    .record_whole_certificate(
                        host,
                        port,
                        &seen.certificate,
                        (&seen.hash, seen.validity.as_ref()),
                    )
                    .map_err(failed)?;
                log::info!(
                    "the {} certificate of {target} accepted by the user is recorded: {}, key {}",
                    self.protocol,
                    seen.hash,
                    seen.certificate.fingerprint
                );
            }
            ToRecord::Accepted(_) => {}
            ToRecord::Adopted(seen) => {
                let adopted = known
                    .adopt_certificate(
                        host,
                        port,
                        &seen.certificate.fingerprint,
                        (&seen.hash, seen.validity.as_ref()),
                    )
                    .map_err(failed)?;
                if adopted {
                    log::info!(
                        "the {} server {target}, trusted by its key {} alone, is now pinned by its whole certificate {}",
                        self.protocol,
                        seen.certificate.fingerprint,
                        seen.hash
                    );
                }
            }
        }
        Ok(())
    }

    /// The handshake stopped: the certificate question for a certificate nobody trusts yet
    /// or renewed on a trusted key, a refusal for one whose key changed or that the user
    /// trusted and is no longer valid, else `failed`, the TLS failure as it is.
    pub(crate) async fn refused(
        &self,
        presented: &PresentedSlot,
        events: &mpsc::Sender<ConnectionEvent>,
        failed: UiError,
    ) -> Result<(), UiError> {
        let kept = presented.lock().ok().and_then(|mut slot| slot.take());
        let Some((seen, issue, pinned)) = kept.and_then(|kept| {
            let seen = SeenCertificate::read(&kept.der)?;
            Some((seen, kept.issue, kept.pinned))
        }) else {
            return Err(failed);
        };
        let certificate = &seen.certificate;
        let issue = issue.refined(certificate.subject == certificate.issuer);
        let target = display_address(&self.host, self.port);
        let presented = certificate.fingerprint;
        // Never the question again: the user trusted it already.
        if pinned {
            let Some(validity) = seen.validity else {
                return Err(failed);
            };
            // As the C# "FTPS certificate rejected" line.
            log::warn!(
                "{} certificate rejected: the pinned certificate of {target} failed non-overridable validity checks: {issue:?}, presented {presented}",
                self.protocol
            );
            return Err(UiError::PinnedCertificateInvalid {
                target,
                fingerprint: presented.to_string(),
                issue,
                not_after: validity.not_after,
            });
        }
        let verdict = KnownRdpHosts::new(&self.known_hosts)
            .certificate_verdict(&self.host, self.port, &presented, &seen.hash)
            .map_err(|error| UiError::KnownHosts {
                detail: error.to_string(),
            })?;
        let renewal = match verdict {
            CertificateVerdict::Changed { recorded } => {
                // As the C# "FTPS certificate rejected" line.
                log::warn!(
                    "the {} certificate of {target} changed: trusted {recorded}, presented {presented}: refused",
                    self.protocol
                );
                return Err(UiError::HostKeyChanged {
                    target: None,
                    recorded: recorded.to_string(),
                    offered: presented.to_string(),
                });
            }
            CertificateVerdict::Renewed { recorded } => {
                log::info!(
                    "{target} presented a renewed certificate {}: the same key {presented}, another certificate than the one trusted",
                    seen.hash
                );
                Some(Renewal { recorded })
            }
            CertificateVerdict::Unknown
            | CertificateVerdict::Known
            | CertificateVerdict::KeyOnly => {
                log::info!("{target} presented an unknown certificate {presented}");
                None
            }
        };
        let details = seen.validity.map(|validity| CertificateDetails {
            issuer: certificate.issuer.clone(),
            validity,
            issue,
            certificate: seen.hash,
            renewal,
        });
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
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::time::Duration;

    use heimdall_rdp::{CertificateHash, KnownRdpHosts, ServerCertificate, serves_tls_servers};
    use heimdall_tls::{Period, UnixTime, UserVerdict};
    use rcgen::{
        BasicConstraints, CertificateParams, ExtendedKeyUsagePurpose as Purpose, IsCa, KeyPair,
        date_time_ymd,
    };

    use super::{CertificatePins, RecordSlot, ToRecord};

    /// The server the pins are about.
    const HOST: &str = "files.lab";
    const PORT: u16 = 990;

    /// A self-signed certificate for [`HOST`] on `key`, valid from the first of January of
    /// `from` to that of `until`, for `purposes`, any when none; a certificate authority
    /// when `authority`.
    fn certificate_on(
        key: &KeyPair,
        (from, until): (i32, i32),
        purposes: Vec<Purpose>,
        authority: bool,
    ) -> Vec<u8> {
        let mut params = CertificateParams::new(vec![HOST.to_owned()]).expect("params");
        params.not_before = date_time_ymd(from, 1, 1);
        params.not_after = date_time_ymd(until, 1, 1);
        params.extended_key_usages = purposes;
        if authority {
            params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        }
        params.self_signed(key).expect("cert").der().to_vec()
    }

    /// The same, on a key of its own.
    fn certificate(validity: (i32, i32), purposes: Vec<Purpose>, authority: bool) -> Vec<u8> {
        certificate_on(
            &KeyPair::generate().expect("key"),
            validity,
            purposes,
            authority,
        )
    }

    /// The key `der` is pinned by.
    fn pin(der: &[u8]) -> heimdall_rdp::Fingerprint {
        ServerCertificate::from_der(der)
            .expect("readable")
            .fingerprint
    }

    /// Pins `der` whole for [`HOST`] in `file`, as accepting it after the question does.
    fn pin_whole(file: &Path, der: &[u8]) {
        let certificate = ServerCertificate::from_der(der).expect("readable");
        KnownRdpHosts::new(file)
            .record_whole_certificate(HOST, PORT, &certificate, (&CertificateHash::of(der), None))
            .expect("pinned");
    }

    /// The pins of [`HOST`] in `file`, nothing decided.
    fn pins(file: &Path) -> CertificatePins {
        CertificatePins {
            known_hosts: file.to_owned(),
            host: HOST.to_owned(),
            port: PORT,
            accepted: None,
            trusted_once: None,
            trusted_for_run: Vec::new(),
            protocol: "FTPS",
        }
    }

    /// What `pins` say of `der` now.
    fn verdict(pins: &CertificatePins, der: &[u8]) -> UserVerdict {
        verdict_at(pins, der, UnixTime::now())
    }

    fn verdict_at(pins: &CertificatePins, der: &[u8], now: UnixTime) -> UserVerdict {
        (pins.user_trust(RecordSlot::default()))(der, now)
    }

    fn pinned(period: Period, for_servers: bool) -> UserVerdict {
        UserVerdict::Pinned {
            period,
            for_servers,
        }
    }

    #[test]
    fn a_pin_on_file_is_checked_at_the_handshake_moment() {
        let dir = tempfile::tempdir().expect("dir");
        let file = dir.path().join("known_ftps_hosts");
        let current = certificate((2020, 2045), Vec::new(), false);
        pin_whole(&file, &current);
        let on_file = pins(&file);
        assert_eq!(verdict(&on_file, &current), pinned(Period::Current, true));
        // The same certificate, at both ends of its validity and past them.
        let at = |year| {
            let seconds = date_time_ymd(year, 1, 1).unix_timestamp();
            UnixTime::since_unix_epoch(Duration::from_secs(
                u64::try_from(seconds).expect("after 1970"),
            ))
        };
        assert_eq!(
            verdict_at(&on_file, &current, at(2020)),
            pinned(Period::Current, true),
            "its first moment"
        );
        assert_eq!(
            verdict_at(&on_file, &current, at(2045)),
            pinned(Period::Current, true),
            "its last moment"
        );
        assert_eq!(
            verdict_at(&on_file, &current, at(2019)),
            pinned(Period::NotYetValid, true)
        );
        assert_eq!(
            verdict_at(&on_file, &current, at(2046)),
            pinned(Period::Expired, true)
        );
        // A certificate on file never decided: another key.
        let other = certificate((2020, 2045), Vec::new(), false);
        assert_eq!(verdict(&on_file, &other), UserVerdict::Untrusted);
    }

    #[test]
    fn another_certificate_on_the_pinned_key_is_not_trusted() {
        let dir = tempfile::tempdir().expect("dir");
        let file = dir.path().join("known_ftps_hosts");
        let key = KeyPair::generate().expect("key");
        let first = certificate_on(&key, (2020, 2045), Vec::new(), false);
        pin_whole(&file, &first);
        // Renewed, or minted again by whoever holds the key, with current dates.
        let renewed = certificate_on(&key, (2021, 2046), Vec::new(), false);
        assert_eq!(pin(&renewed), pin(&first), "the same key");
        assert_eq!(verdict(&pins(&file), &renewed), UserVerdict::Untrusted);
        assert_eq!(verdict(&pins(&file), &first), pinned(Period::Current, true));
    }

    #[test]
    fn a_key_recorded_alone_is_a_pin_whose_certificate_is_adopted_once_through() {
        let dir = tempfile::tempdir().expect("dir");
        let file = dir.path().join("known_ftps_hosts");
        let legacy = certificate((2020, 2045), Vec::new(), false);
        KnownRdpHosts::new(&file)
            .record(HOST, PORT, &pin(&legacy))
            .expect("pinned by its key alone");
        let pins = pins(&file);
        let slot = RecordSlot::default();
        assert_eq!(
            (pins.user_trust(slot.clone()))(&legacy, UnixTime::now()),
            pinned(Period::Current, true),
            "checked as a pin, as before"
        );
        assert!(
            matches!(
                slot.lock().expect("slot").as_ref(),
                Some(ToRecord::Adopted(seen)) if seen.hash == CertificateHash::of(&legacy)
            ),
            "adopted once the connection is through"
        );
        pins.record_trusted(&slot).expect("adopted");
        let [entry] = KnownRdpHosts::new(&file)
            .entries()
            .expect("read")
            .try_into()
            .expect("one entry");
        assert_eq!(entry.certificate, Some(CertificateHash::of(&legacy)));
    }

    #[test]
    fn a_pinned_certificate_for_other_purposes_is_said_not_for_servers() {
        let dir = tempfile::tempdir().expect("dir");
        let file = dir.path().join("known_ftps_hosts");
        // A certificate authority as its own server's certificate, for clients only.
        let client_only = certificate((2020, 2045), vec![Purpose::ClientAuth], true);
        pin_whole(&file, &client_only);
        assert_eq!(
            verdict(&pins(&file), &client_only),
            pinned(Period::Current, false)
        );
    }

    #[test]
    fn a_certificate_trusted_for_this_run_is_a_pin_and_decided_once_only_on_the_answer() {
        let dir = tempfile::tempdir().expect("dir");
        let file = dir.path().join("known_ftps_hosts");
        let key = KeyPair::generate().expect("key");
        let expired = certificate_on(&key, (2000, 2001), Vec::new(), false);
        let hash = CertificateHash::of(&expired);
        let mut once = pins(&file);
        assert_eq!(verdict(&once, &expired), UserVerdict::Untrusted);
        once.trusted_for_run = vec![hash];
        assert_eq!(verdict(&once, &expired), pinned(Period::Expired, true));
        once.trusted_once = Some(hash);
        assert_eq!(verdict(&once, &expired), UserVerdict::Decided);
        // Accepted to be recorded: decided too, and left to be recorded.
        let mut accepted = pins(&file);
        accepted.accepted = Some(hash);
        let slot = RecordSlot::default();
        assert_eq!(
            (accepted.user_trust(slot.clone()))(&expired, UnixTime::now()),
            UserVerdict::Decided
        );
        assert!(matches!(
            slot.lock().expect("slot").as_ref(),
            Some(ToRecord::Accepted(seen)) if seen.hash == hash
        ));
        assert!(!file.exists(), "nothing written by the check");
        // Decided for this very certificate: another on the same key is not.
        let other = certificate_on(&key, (2000, 2001), Vec::new(), false);
        for decided in [&once, &accepted] {
            assert_eq!(verdict(decided, &other), UserVerdict::Untrusted);
        }
    }

    #[test]
    fn a_certificate_serves_a_tls_server_unless_its_key_usage_names_other_purposes() {
        let made = |purposes| certificate((2020, 2045), purposes, false);
        assert_eq!(serves_tls_servers(&made(Vec::new())), Ok(true), "none said");
        assert_eq!(
            serves_tls_servers(&made(vec![Purpose::ClientAuth, Purpose::ServerAuth])),
            Ok(true)
        );
        assert_eq!(serves_tls_servers(&made(vec![Purpose::Any])), Ok(true));
        assert_eq!(
            serves_tls_servers(&made(vec![Purpose::ClientAuth, Purpose::CodeSigning])),
            Ok(false)
        );
        assert!(serves_tls_servers(b"not a certificate").is_err());
    }
}
