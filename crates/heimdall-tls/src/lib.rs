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

//! The certificate check of the TLS clients that must trust a server before any password is
//! sent: explicit FTPS, as the C# one, and the X509 subtypes of VNC's `VeNCrypt`.
//!
//! A certificate the system trusts for the server's name goes through; otherwise one the
//! user trusted for this server, which the caller decides, as it keeps the user's pins;
//! otherwise the handshake stops and the certificate is kept, with why the system did not
//! vouch for it, for the user to be shown it and asked.
//!
//! A pin is no blank cheque, as in the C# (`FtpBrowser.cs`,
//! `EnsurePinnedCertificateRemainsValid`): a pinned certificate outside its validity period
//! is refused, with or without a system store, and so is one the system refuses for what no
//! pin overrides (see [`refuses_pin`]). Only the connection the user decided on just now,
//! after the question, takes the certificate as it is. Revocation is not checked: no CRL
//! nor OCSP is fetched.
//!
//! The decision is taken in the handshake, before any password is sent. Every handshake
//! signature is verified against the certificate, so a server proves it holds its key: a
//! replayed certificate would not pass the signature check.
//!
//! A pin is refused only once the server proved it holds the pinned key. A pin is the hash
//! of a public key, which anyone can read: refused at the certificate, a forged certificate
//! around the pinned key, over or for another purpose, would read as the server's own
//! certificate grown invalid, to be forgotten in one click, instead of a failure. So the
//! refusal waits for the handshake signature, which rustls checks right after the
//! certificate, on the same thread, in every full handshake: TLS 1.3 `CertificateVerify`,
//! and the signed key exchange of TLS 1.2, every suite of which is ephemeral
//! Diffie-Hellman. A resumed handshake checks neither, nor the certificate.

use std::sync::{Arc, Mutex, PoisonError};
use std::thread::ThreadId;

use tokio_rustls::TlsConnector;
use tokio_rustls::rustls::client::WebPkiServerVerifier;
use tokio_rustls::rustls::client::danger::{
    HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier,
};
use tokio_rustls::rustls::crypto::{
    CryptoProvider, WebPkiSupportedAlgorithms, ring::default_provider, verify_tls12_signature,
    verify_tls13_signature,
};
use tokio_rustls::rustls::pki_types::{CertificateDer, ServerName};
use tokio_rustls::rustls::{
    CertificateError, ClientConfig, DigitallySignedStruct, Error, RootCertStore, SignatureScheme,
};

/// The moment of a handshake, as the user's trust is asked about it.
pub use tokio_rustls::rustls::pki_types::UnixTime;

/// Length of a certificate's SHA-256 fingerprint.
pub const FINGERPRINT_LEN: usize = sealvault::hash::SHA256_LEN;

/// The SHA-256 of a whole certificate, as the C# shows and pins it.
pub type CertificateFingerprint = [u8; FINGERPRINT_LEN];

/// The fingerprint of certificate `der`.
#[must_use]
pub fn fingerprint(der: &[u8]) -> CertificateFingerprint {
    sealvault::hash::sha256(der)
}

/// Why the system did not vouch for a certificate, as the C# prompt's "Validation issue"
/// says it: the first problem its check met.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationIssue {
    /// No authority of this computer issued it; [`ValidationIssue::refined`] tells a
    /// certificate that is its own issuer apart.
    UnknownIssuer,
    /// It names itself as its issuer, which no authority of this computer is.
    SelfSigned,
    /// Its validity period is over.
    Expired,
    /// Its validity period has not begun.
    NotYetValid,
    /// It was issued for another name than the server's.
    NameMismatch,
    /// Its extended key usage names other purposes than a TLS server's.
    WrongPurpose,
    /// Its issuer revoked it.
    Revoked,
    /// This computer has no certificate authority to check it against.
    NoSystemStore,
    /// Anything else the check refused: a bad signature or encoding.
    Other,
}

impl ValidationIssue {
    /// The issue the check's `refusal` stands for.
    #[must_use]
    pub fn of(refusal: &Error) -> Self {
        let Error::InvalidCertificate(problem) = refusal else {
            return Self::Other;
        };
        match problem {
            CertificateError::UnknownIssuer => Self::UnknownIssuer,
            CertificateError::Expired | CertificateError::ExpiredContext { .. } => Self::Expired,
            CertificateError::NotValidYet | CertificateError::NotValidYetContext { .. } => {
                Self::NotYetValid
            }
            CertificateError::NotValidForName | CertificateError::NotValidForNameContext { .. } => {
                Self::NameMismatch
            }
            CertificateError::Revoked => Self::Revoked,
            CertificateError::InvalidPurpose | CertificateError::InvalidPurposeContext { .. } => {
                Self::WrongPurpose
            }
            _ => Self::Other,
        }
    }

    /// The issue, told precisely for a certificate whose issuer is its subject
    /// (`self_issued`): an unknown issuer is then the certificate itself.
    #[must_use]
    pub fn refined(self, self_issued: bool) -> Self {
        if self == Self::UnknownIssuer && self_issued {
            Self::SelfSigned
        } else {
            self
        }
    }
}

/// A certificate neither the system nor the user trusts, kept for the question; or one the
/// user trusted that is no longer valid, kept to say why it was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Presented {
    /// The certificate, whole (DER).
    pub der: Vec<u8>,
    /// Why the system did not vouch for it; for a pinned one, why it was refused.
    pub issue: ValidationIssue,
    /// The user trusted it, pinned or for this run, and it is no longer valid: refused,
    /// never asked about again.
    pub pinned: bool,
}

/// Where a certificate neither trusted by the system nor by the user is kept, for the
/// question.
pub type PresentedSlot = Arc<Mutex<Option<Presented>>>;

/// Where a moment falls in a certificate's validity period.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Period {
    /// Between its two dates.
    Current,
    /// After its last moment.
    Expired,
    /// Before its first moment.
    NotYetValid,
}

/// How the user trusts a certificate for the server connected to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserVerdict {
    /// Not at all: the system decides, else the user is asked.
    Untrusted,
    /// Pinned, on file or for this run, with where the handshake's moment falls in its
    /// validity period and whether it may serve a TLS server: checked again at every
    /// connection, as the C# checks its pins.
    Pinned {
        /// Where the moment falls.
        period: Period,
        /// Whether its extended key usage, when it has one, names a TLS server's purpose or
        /// any purpose. The system's check never reaches that question for a certificate
        /// authority serving as its own server's certificate: it stops at its basic
        /// constraints, a refusal that keeps the pin.
        for_servers: bool,
    },
    /// Decided by the user just now, after the certificate question: this connection takes
    /// it as it is, as the C# "Accept" and "Trust once" do.
    Decided,
}

/// How the user trusts a certificate, given whole (DER), for the server connected to, at
/// the moment of the handshake.
pub type UserTrust = Arc<dyn Fn(&[u8], UnixTime) -> UserVerdict + Send + Sync>;

/// Whether the system's `refusal` of a pinned certificate refuses it still: those of the C#
/// `NonOverridableChainErrors` that rustls tells. Over or not begun, revoked, a bad
/// signature, a wrong purpose, a signature algorithm not supported. An unknown issuer, a
/// name mismatch, a bad encoding and anything else keep the pin: among them a certificate
/// authority serving as the server's own certificate (`openssl req -x509`) and the version
/// 1 certificates of appliances.
#[must_use]
#[expect(
    deprecated,
    reason = "rustls still reports an unsupported signature algorithm without its context"
)]
pub fn refuses_pin(refusal: &Error) -> bool {
    let Error::InvalidCertificate(problem) = refusal else {
        return false;
    };
    matches!(
        problem,
        CertificateError::Expired
            | CertificateError::ExpiredContext { .. }
            | CertificateError::NotValidYet
            | CertificateError::NotValidYetContext { .. }
            | CertificateError::Revoked
            | CertificateError::BadSignature
            | CertificateError::InvalidPurpose
            | CertificateError::InvalidPurposeContext { .. }
            | CertificateError::UnsupportedSignatureAlgorithm
            | CertificateError::UnsupportedSignatureAlgorithmContext { .. }
            | CertificateError::UnsupportedSignatureAlgorithmForPublicKeyContext { .. }
    )
}

/// What the system checks of a server's certificate, as the handshake gives it.
struct Checked<'a, 'b> {
    end_entity: &'a CertificateDer<'b>,
    intermediates: &'a [CertificateDer<'b>],
    server_name: &'a ServerName<'b>,
    ocsp_response: &'a [u8],
    now: UnixTime,
}

/// The refusal of a pinned certificate, held until the server proves it holds the
/// certificate's key.
#[derive(Debug)]
struct PendingRefusal {
    /// The handshake's thread: rustls checks the signature right after the certificate, in
    /// the same call.
    thread: ThreadId,
    /// The certificate refused, whole (DER).
    der: Vec<u8>,
    /// What the handshake fails with.
    refusal: Error,
    /// Why, for the caller.
    issue: ValidationIssue,
}

/// The check: the system, then what the user trusted for this server.
struct Trust {
    system: Option<Arc<WebPkiServerVerifier>>,
    trusted: UserTrust,
    presented: PresentedSlot,
    algorithms: WebPkiSupportedAlgorithms,
    /// Refusals of pinned certificates waiting for their handshake's signature: one per
    /// thread at most, the connector being shared by concurrent handshakes.
    pending: Mutex<Vec<PendingRefusal>>,
}

impl std::fmt::Debug for Trust {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Trust")
            .field("system", &self.system.is_some())
            .finish_non_exhaustive()
    }
}

impl Trust {
    /// A check of `trusted` beside `system`, keeping what it refuses in `presented`.
    fn new(
        system: Option<Arc<WebPkiServerVerifier>>,
        trusted: UserTrust,
        presented: PresentedSlot,
        algorithms: WebPkiSupportedAlgorithms,
    ) -> Self {
        Self {
            system,
            trusted,
            presented,
            algorithms,
            pending: Mutex::new(Vec::new()),
        }
    }

    /// Holds `refusal` for this thread's handshake until its signature is checked; `None`
    /// drops what an earlier handshake of this thread left, cut short before its signature.
    fn hold(&self, refusal: Option<PendingRefusal>) {
        let mut pending = self.pending.lock().unwrap_or_else(PoisonError::into_inner);
        let thread = std::thread::current().id();
        pending.retain(|held| held.thread != thread);
        pending.extend(refusal);
    }

    /// The refusal this thread's handshake holds, taken.
    fn take_held(&self) -> Option<PendingRefusal> {
        let mut pending = self.pending.lock().unwrap_or_else(PoisonError::into_inner);
        let thread = std::thread::current().id();
        let index = pending.iter().position(|held| held.thread == thread)?;
        Some(pending.swap_remove(index))
    }

    /// The handshake signature on `cert` was checked, `signed`: a refusal held for it now
    /// stands, the server having proved it holds the certificate's key. A signature that
    /// does not hold fails the handshake as it is, and nothing is kept: the certificate is
    /// not the server's.
    fn proved(
        &self,
        cert: &CertificateDer<'_>,
        signed: Result<HandshakeSignatureValid, Error>,
    ) -> Result<HandshakeSignatureValid, Error> {
        let held = self.take_held();
        let valid = signed?;
        match held {
            None => Ok(valid),
            Some(held) if held.der == cert.as_ref() => {
                self.keep(&held.der, held.issue, true);
                Err(held.refusal)
            }
            // Never another certificate than the one checked: refused all the same.
            Some(held) => Err(held.refusal),
        }
    }

    /// Keeps `der` for the caller: for the question, or, `pinned`, to say why a certificate
    /// the user trusted was refused.
    fn keep(&self, der: &[u8], issue: ValidationIssue, pinned: bool) {
        if let Ok(mut slot) = self.presented.lock() {
            *slot = Some(Presented {
                der: der.to_vec(),
                issue,
                pinned,
            });
        }
    }

    /// Why a pinned certificate, whose validity period is at `period` and that may serve a
    /// TLS server or not, `for_servers`, is refused; `None` when it goes through.
    fn pin_refusal(
        &self,
        (period, for_servers): (Period, bool),
        checked: &Checked<'_, '_>,
    ) -> Option<(Error, ValidationIssue)> {
        // Checked here, with or without a system store.
        match period {
            Period::Expired => {
                return Some((
                    Error::InvalidCertificate(CertificateError::Expired),
                    ValidationIssue::Expired,
                ));
            }
            Period::NotYetValid => {
                return Some((
                    Error::InvalidCertificate(CertificateError::NotValidYet),
                    ValidationIssue::NotYetValid,
                ));
            }
            Period::Current => {}
        }
        if !for_servers {
            return Some((
                Error::InvalidCertificate(CertificateError::InvalidPurpose),
                ValidationIssue::WrongPurpose,
            ));
        }
        let refusal = self
            .system
            .as_ref()?
            .verify_server_cert(
                checked.end_entity,
                checked.intermediates,
                checked.server_name,
                checked.ocsp_response,
                checked.now,
            )
            .err()?;
        refuses_pin(&refusal).then(|| {
            let issue = ValidationIssue::of(&refusal);
            (refusal, issue)
        })
    }
}

impl ServerCertVerifier for Trust {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        server_name: &ServerName<'_>,
        ocsp_response: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, Error> {
        self.hold(None);
        match (self.trusted)(end_entity.as_ref(), now) {
            UserVerdict::Decided => return Ok(ServerCertVerified::assertion()),
            UserVerdict::Pinned {
                period,
                for_servers,
            } => {
                let checked = Checked {
                    end_entity,
                    intermediates,
                    server_name,
                    ocsp_response,
                    now,
                };
                // Refused once the server proves it holds the key, at its signature.
                if let Some((refusal, issue)) = self.pin_refusal((period, for_servers), &checked) {
                    self.hold(Some(PendingRefusal {
                        thread: std::thread::current().id(),
                        der: end_entity.as_ref().to_vec(),
                        refusal,
                        issue,
                    }));
                }
                return Ok(ServerCertVerified::assertion());
            }
            UserVerdict::Untrusted => {}
        }
        let (refusal, issue) = match &self.system {
            Some(system) => match system.verify_server_cert(
                end_entity,
                intermediates,
                server_name,
                ocsp_response,
                now,
            ) {
                Ok(verified) => return Ok(verified),
                Err(refusal) => {
                    let issue = ValidationIssue::of(&refusal);
                    (refusal, issue)
                }
            },
            None => (
                Error::General("no system certificate check".to_owned()),
                ValidationIssue::NoSystemStore,
            ),
        };
        self.keep(end_entity.as_ref(), issue, false);
        Err(refusal)
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        self.proved(
            cert,
            verify_tls12_signature(message, cert, dss, &self.algorithms),
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        self.proved(
            cert,
            verify_tls13_signature(message, cert, dss, &self.algorithms),
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.algorithms.supported_schemes()
    }
}

/// The check of the system's certificate store; `None` when it has nothing to read, and
/// then only what the user trusted goes through.
fn system_verifier(provider: &Arc<CryptoProvider>) -> Option<Arc<WebPkiServerVerifier>> {
    let mut roots = RootCertStore::empty();
    let loaded = rustls_native_certs::load_native_certs();
    for error in &loaded.errors {
        log::debug!("a system certificate could not be read: {error}");
    }
    let (added, ignored) = roots.add_parsable_certificates(loaded.certs);
    log::debug!("system certificates: {added} added, {ignored} ignored");
    if roots.is_empty() {
        return None;
    }
    WebPkiServerVerifier::builder_with_provider(Arc::new(roots), Arc::clone(provider))
        .build()
        .ok()
}

/// A TLS client, 1.2 or 1.3, trusting what the system trusts and what `trusted` accepts; a
/// certificate trusted by neither lands in `presented` and stops the handshake, and so does
/// one `trusted` pinned that is no longer valid, kept there marked pinned.
///
/// # Panics
///
/// Never: ring's default protocol versions are always accepted.
#[must_use]
pub fn connector(trusted: UserTrust, presented: PresentedSlot) -> TlsConnector {
    let provider = Arc::new(default_provider());
    let system = system_verifier(&provider);
    let trust = Trust::new(
        system,
        trusted,
        presented,
        provider.signature_verification_algorithms,
    );
    let config = ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .expect("ring's default protocol versions")
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(trust))
        .with_no_client_auth();
    TlsConnector::from(Arc::new(config))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use rcgen::{
        BasicConstraints, CertificateParams, DistinguishedName, DnType, ExtendedKeyUsagePurpose,
        IsCa, Issuer, KeyPair, date_time_ymd,
    };
    use tokio_rustls::rustls::client::WebPkiServerVerifier;
    use tokio_rustls::rustls::crypto::ring::default_provider;
    use tokio_rustls::rustls::pki_types::{
        CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName,
    };
    use tokio_rustls::rustls::server::{ClientHello, ResolvesServerCert};
    use tokio_rustls::rustls::sign::CertifiedKey;
    use tokio_rustls::rustls::version::{TLS12, TLS13};
    use tokio_rustls::rustls::{
        CertificateError, ClientConfig, ClientConnection, Error, RootCertStore, ServerConfig,
        ServerConnection, SupportedProtocolVersion,
    };

    use super::{
        Period, Presented, PresentedSlot, Trust, UserTrust, UserVerdict, ValidationIssue,
        fingerprint, refuses_pin,
    };

    /// Flights exchanged at most before a test handshake is said stuck.
    const MAX_FLIGHTS: usize = 10;

    /// What a test server presents: its certificate, and the key it signs the handshake
    /// with (PKCS #8), its certificate's own key unless forged.
    struct Served {
        der: Vec<u8>,
        key: Vec<u8>,
    }

    /// A check of `trusted` beside `system_roots` as the system store, and where it keeps
    /// what it refuses.
    fn trust_with(system_roots: &[&[u8]], trusted: UserTrust) -> (Arc<Trust>, PresentedSlot) {
        let provider = Arc::new(default_provider());
        let system = (!system_roots.is_empty()).then(|| {
            let mut roots = RootCertStore::empty();
            for root in system_roots {
                roots
                    .add(CertificateDer::from(root.to_vec()))
                    .expect("root");
            }
            WebPkiServerVerifier::builder_with_provider(Arc::new(roots), Arc::clone(&provider))
                .build()
                .expect("verifier")
        });
        let presented = PresentedSlot::default();
        let trust = Trust::new(
            system,
            trusted,
            presented.clone(),
            provider.signature_verification_algorithms,
        );
        (Arc::new(trust), presented)
    }

    /// A check trusting `system_roots` as the system store and the certificates of
    /// `trusted` as pinned by the user, all of them current and for servers.
    fn trust(system_roots: &[&[u8]], trusted: Vec<[u8; 32]>) -> (Arc<Trust>, PresentedSlot) {
        trust_with(
            system_roots,
            Arc::new(move |der, _| {
                if trusted.contains(&fingerprint(der)) {
                    UserVerdict::Pinned {
                        period: Period::Current,
                        for_servers: true,
                    }
                } else {
                    UserVerdict::Untrusted
                }
            }),
        )
    }

    /// A check trusting `system_roots` as the system store, and `cert`, valid from the
    /// first of January of `from` to that of `until`, as `pinned` (pinned, else decided
    /// just now) by the user, its period read at the handshake's moment as the caller does.
    fn trusting_one(
        system_roots: &[&[u8]],
        cert: &[u8],
        (from, until): (i32, i32),
        pinned: bool,
    ) -> (Arc<Trust>, PresentedSlot) {
        let wanted = fingerprint(cert);
        let (not_before, not_after) = (year_start(from), year_start(until));
        trust_with(
            system_roots,
            Arc::new(move |der, now| {
                if fingerprint(der) != wanted {
                    return UserVerdict::Untrusted;
                }
                if !pinned {
                    return UserVerdict::Decided;
                }
                let now = i64::try_from(now.as_secs()).expect("seconds");
                let period = if now < not_before {
                    Period::NotYetValid
                } else if now > not_after {
                    Period::Expired
                } else {
                    Period::Current
                };
                UserVerdict::Pinned {
                    period,
                    for_servers: true,
                }
            }),
        )
    }

    /// A check taking any certificate as pinned with `verdict`, as the caller does for one
    /// carrying the pinned public key, whoever signed it.
    fn pinning_any(system_roots: &[&[u8]], verdict: UserVerdict) -> (Arc<Trust>, PresentedSlot) {
        trust_with(system_roots, Arc::new(move |_, _| verdict))
    }

    /// The first second of `year`, in seconds since 1970.
    fn year_start(year: i32) -> i64 {
        date_time_ymd(year, 1, 1).unix_timestamp()
    }

    /// The test server's certificate, whatever it presents.
    #[derive(Debug)]
    struct Fixed(Arc<CertifiedKey>);

    impl ResolvesServerCert for Fixed {
        fn resolve(&self, _: ClientHello<'_>) -> Option<Arc<CertifiedKey>> {
            Some(Arc::clone(&self.0))
        }
    }

    /// A full handshake in `version` between a client checking with `trust` the server
    /// named `name` and a server presenting `served`; the client's error when it fails.
    fn handshake(
        trust: &Arc<Trust>,
        served: &Served,
        name: &str,
        version: &'static SupportedProtocolVersion,
    ) -> Result<(), Error> {
        let provider = Arc::new(default_provider());
        let key = provider
            .key_provider
            .load_private_key(PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(
                served.key.clone(),
            )))
            .expect("signing key");
        // No consistency check: a forger presents a key that is not its certificate's.
        let certified = CertifiedKey::new(vec![CertificateDer::from(served.der.clone())], key);
        let server_config = ServerConfig::builder_with_provider(Arc::clone(&provider))
            .with_protocol_versions(&[version])
            .expect("version")
            .with_no_client_auth()
            .with_cert_resolver(Arc::new(Fixed(Arc::new(certified))));
        let client_config = ClientConfig::builder_with_provider(provider)
            .with_protocol_versions(&[version])
            .expect("version")
            .dangerous()
            .with_custom_certificate_verifier(Arc::clone(trust) as _)
            .with_no_client_auth();
        let mut client = ClientConnection::new(
            Arc::new(client_config),
            ServerName::try_from(name.to_owned()).expect("name"),
        )
        .expect("client");
        let mut remote = ServerConnection::new(Arc::new(server_config)).expect("server");
        for _ in 0..MAX_FLIGHTS {
            let mut wire = Vec::new();
            while client.wants_write() {
                client.write_tls(&mut wire).expect("client writes");
            }
            let mut flight = wire.as_slice();
            while !flight.is_empty() {
                remote.read_tls(&mut flight).expect("server reads");
                if remote.process_new_packets().is_err() {
                    break;
                }
            }
            let mut wire = Vec::new();
            while remote.wants_write() {
                remote.write_tls(&mut wire).expect("server writes");
            }
            let mut flight = wire.as_slice();
            while !flight.is_empty() {
                client.read_tls(&mut flight).expect("client reads");
                client.process_new_packets()?;
            }
            if !client.is_handshaking() && !remote.is_handshaking() {
                return Ok(());
            }
        }
        panic!("the handshake did not end");
    }

    /// Whether `trust` takes `served` from a server named `name`, in TLS 1.3 and in TLS 1.2
    /// alike.
    fn verdict_for(trust: &Arc<Trust>, served: &Served, name: &str) -> bool {
        let tls13 = handshake(trust, served, name, &TLS13).is_ok();
        let tls12 = handshake(trust, served, name, &TLS12).is_ok();
        assert_eq!(tls13, tls12, "both versions decide alike");
        tls13
    }

    fn verdict(trust: &Arc<Trust>, served: &Served) -> bool {
        verdict_for(trust, served, "localhost")
    }

    /// The issue kept for the question once `trust` refused `served` from `name`.
    fn issue_kept(
        trust: &Arc<Trust>,
        presented: &PresentedSlot,
        served: &Served,
        name: &str,
    ) -> ValidationIssue {
        assert!(!verdict_for(trust, served, name), "refused");
        let kept = presented
            .lock()
            .expect("slot")
            .take()
            .expect("kept for the question");
        assert!(!kept.pinned, "a question, not a pin refused");
        kept.issue
    }

    /// Why `trust` refused `served`, a certificate the user trusted, from `name`, once the
    /// server proved it holds its key; the refusal is kept marked pinned, never for the
    /// question.
    fn pin_refused(
        trust: &Arc<Trust>,
        presented: &PresentedSlot,
        served: &Served,
        name: &str,
    ) -> ValidationIssue {
        for version in [&TLS13, &TLS12] {
            assert!(
                handshake(trust, served, name, version).is_err(),
                "refused in {version:?}"
            );
        }
        let kept = presented
            .lock()
            .expect("slot")
            .take()
            .expect("kept to say why");
        assert!(kept.pinned, "a pin refused, not a question");
        assert_eq!(kept.der, served.der);
        kept.issue
    }

    /// A distinguished name of common name `common_name`.
    fn named(common_name: &str) -> DistinguishedName {
        let mut distinguished_name = DistinguishedName::new();
        distinguished_name.push(DnType::CommonName, common_name);
        distinguished_name
    }

    /// The parameters of a certificate for `name`, valid from the first of January of
    /// `from` to that of `until`, for `purposes`, any when none.
    fn params(
        name: &str,
        (from, until): (i32, i32),
        purposes: &[ExtendedKeyUsagePurpose],
    ) -> CertificateParams {
        let mut params = CertificateParams::new(vec![name.to_owned()]).expect("params");
        params.distinguished_name = named(name);
        params.not_before = date_time_ymd(from, 1, 1);
        params.not_after = date_time_ymd(until, 1, 1);
        params.extended_key_usages = purposes.to_vec();
        params
    }

    /// A certificate authority of common name `common_name`.
    fn authority(common_name: &str) -> (Vec<u8>, Issuer<'static, KeyPair>) {
        let key = KeyPair::generate().expect("key");
        let mut params = CertificateParams::new(Vec::<String>::new()).expect("params");
        params.distinguished_name = named(common_name);
        params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        let der = params.self_signed(&key).expect("authority").der().to_vec();
        (der, Issuer::new(params, key))
    }

    /// An authority, and a certificate it issues for `name`, valid from the first of
    /// January of `from` to that of `until`.
    fn issued_by_authority(name: &str, validity: (i32, i32)) -> (Vec<u8>, Served) {
        issued_by_authority_for(name, validity, &[])
    }

    /// An authority, and a certificate it issues for `name`, valid from the first of
    /// January of `from` to that of `until`, for the `purposes` given, any when none.
    fn issued_by_authority_for(
        name: &str,
        validity: (i32, i32),
        purposes: &[ExtendedKeyUsagePurpose],
    ) -> (Vec<u8>, Served) {
        let (authority, issuer) = authority("Lab Root CA");
        let key = KeyPair::generate().expect("key");
        let der = params(name, validity, purposes)
            .signed_by(&key, &issuer)
            .expect("server")
            .der()
            .to_vec();
        (
            authority,
            Served {
                der,
                key: key.serialize_der(),
            },
        )
    }

    /// A certificate for `name` that is its own issuer, valid from the first of January of
    /// `from` to that of `until`; a certificate authority when `authority`, as
    /// `openssl req -x509` makes them.
    fn self_signed(name: &str, validity: (i32, i32), authority: bool) -> Served {
        let mut params = params(name, validity, &[]);
        if authority {
            params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        }
        let key = KeyPair::generate().expect("key");
        Served {
            der: params.self_signed(&key).expect("cert").der().to_vec(),
            key: key.serialize_der(),
        }
    }

    /// What a forger presents: a certificate around the public key of `victim`, valid from
    /// the first of January of `from` to that of `until`, signed by a key of its own under
    /// the authority name `issuer_name`, and that key signing the handshake, the forger not
    /// holding the victim's.
    fn forged(victim: &Served, validity: (i32, i32), issuer_name: &str) -> Served {
        let victim_key = KeyPair::try_from(victim.key.as_slice()).expect("victim's key");
        let (_, issuer) = authority(issuer_name);
        let der = params("files.lab", validity, &[])
            .signed_by(&victim_key, &issuer)
            .expect("forged")
            .der()
            .to_vec();
        Served {
            der,
            key: KeyPair::generate().expect("key").serialize_der(),
        }
    }

    /// A certificate that is no authority's: the system store of the tests that need one.
    fn some_root() -> Vec<u8> {
        authority("Other Root CA").0
    }

    fn simple(name: &str) -> Served {
        let issued = rcgen::generate_simple_self_signed(vec![name.to_owned()]).expect("cert");
        Served {
            der: issued.cert.der().to_vec(),
            key: issued.signing_key.serialize_der(),
        }
    }

    #[test]
    fn what_the_system_trusts_goes_through_without_asking() {
        let served = simple("localhost");
        let (trusted, presented) = trust(&[&served.der], Vec::new());
        assert!(verdict(&trusted, &served));
        assert!(presented.lock().expect("slot").is_none(), "nothing to ask");
    }

    #[test]
    fn what_nobody_trusts_is_refused_and_kept_for_the_question() {
        let served = simple("localhost");
        // Another authority, under another name: one of the same name would be tried as the
        // issuer, and refused for its signature instead.
        let (other, _) = issued_by_authority("other.lab", (2020, 2045));
        let (refusing, presented) = trust(&[&other], Vec::new());
        assert!(!verdict(&refusing, &served));
        assert_eq!(
            presented.lock().expect("slot").clone(),
            Some(Presented {
                der: served.der.clone(),
                issue: ValidationIssue::UnknownIssuer,
                pinned: false,
            })
        );
        // No system store at all: the user's fingerprints alone.
        let (alone, _) = trust(&[], vec![fingerprint(&served.der)]);
        assert!(verdict(&alone, &served));
    }

    #[test]
    fn why_the_system_refused_is_kept_with_the_certificate() {
        // Issued by an authority the system trusts, for another name.
        let (authority, served) = issued_by_authority("files.lab", (2020, 2045));
        let (system, presented) = trust(&[&authority], Vec::new());
        assert!(verdict_for(&system, &served, "files.lab"), "its own name");
        assert_eq!(
            issue_kept(&system, &presented, &served, "other.lab"),
            ValidationIssue::NameMismatch
        );
        // Over, and not begun yet.
        let (authority, served) = issued_by_authority("files.lab", (2000, 2001));
        let (system, presented) = trust(&[&authority], Vec::new());
        assert_eq!(
            issue_kept(&system, &presented, &served, "files.lab"),
            ValidationIssue::Expired
        );
        let (authority, served) = issued_by_authority("files.lab", (2045, 2046));
        let (system, presented) = trust(&[&authority], Vec::new());
        assert_eq!(
            issue_kept(&system, &presented, &served, "files.lab"),
            ValidationIssue::NotYetValid
        );
        // For another purpose than a server's.
        let (authority, served) = issued_by_authority_for(
            "files.lab",
            (2020, 2045),
            &[ExtendedKeyUsagePurpose::ClientAuth],
        );
        let (system, presented) = trust(&[&authority], Vec::new());
        assert_eq!(
            issue_kept(&system, &presented, &served, "files.lab"),
            ValidationIssue::WrongPurpose
        );
        // No system store at all.
        let (alone, presented) = trust(&[], Vec::new());
        assert_eq!(
            issue_kept(&alone, &presented, &served, "files.lab"),
            ValidationIssue::NoSystemStore
        );
    }

    #[test]
    fn a_pinned_certificate_over_is_refused_issued_or_self_signed() {
        let (authority, served) = issued_by_authority("files.lab", (2000, 2001));
        let (pinned, presented) = trusting_one(&[&authority], &served.der, (2000, 2001), true);
        assert_eq!(
            pin_refused(&pinned, &presented, &served, "files.lab"),
            ValidationIssue::Expired
        );
        let root = some_root();
        let served = self_signed("files.lab", (2000, 2001), false);
        let (pinned, presented) = trusting_one(&[&root], &served.der, (2000, 2001), true);
        assert_eq!(
            pin_refused(&pinned, &presented, &served, "files.lab"),
            ValidationIssue::Expired
        );
    }

    #[test]
    fn a_pinned_certificate_not_yet_valid_is_refused() {
        let root = some_root();
        let served = self_signed("files.lab", (2045, 2046), false);
        let (pinned, presented) = trusting_one(&[&root], &served.der, (2045, 2046), true);
        assert_eq!(
            pin_refused(&pinned, &presented, &served, "files.lab"),
            ValidationIssue::NotYetValid
        );
    }

    #[test]
    fn a_pinned_certificate_over_is_refused_without_a_system_store() {
        let served = self_signed("files.lab", (2000, 2001), false);
        let (pinned, presented) = trusting_one(&[], &served.der, (2000, 2001), true);
        assert_eq!(
            pin_refused(&pinned, &presented, &served, "files.lab"),
            ValidationIssue::Expired
        );
    }

    #[test]
    fn a_current_pinned_certificate_goes_through_what_a_pin_overrides() {
        let root = some_root();
        // Self-signed: no authority of the system issued it.
        let served = self_signed("files.lab", (2020, 2045), false);
        let (pinned, presented) = trusting_one(&[&root], &served.der, (2020, 2045), true);
        assert!(verdict_for(&pinned, &served, "files.lab"));
        assert!(presented.lock().expect("slot").is_none(), "nothing kept");
        // Issued for another name than the one connected to.
        let (authority, served) = issued_by_authority("files.lab", (2020, 2045));
        let (pinned, _) = trusting_one(&[&authority], &served.der, (2020, 2045), true);
        assert!(verdict_for(&pinned, &served, "other.lab"));
        // A certificate authority serving as its own server's certificate.
        let served = self_signed("files.lab", (2020, 2045), true);
        let (pinned, _) = trusting_one(&[&root], &served.der, (2020, 2045), true);
        assert!(verdict_for(&pinned, &served, "files.lab"));
        // No system store: the pin alone.
        let served = self_signed("files.lab", (2020, 2045), false);
        let (pinned, _) = trusting_one(&[], &served.der, (2020, 2045), true);
        assert!(verdict_for(&pinned, &served, "files.lab"));
    }

    #[test]
    fn a_pinned_certificate_for_another_purpose_is_refused() {
        // Said so by the system's check.
        let (authority, served) = issued_by_authority_for(
            "files.lab",
            (2020, 2045),
            &[ExtendedKeyUsagePurpose::ClientAuth],
        );
        let (pinned, presented) = trusting_one(&[&authority], &served.der, (2020, 2045), true);
        assert_eq!(
            pin_refused(&pinned, &presented, &served, "files.lab"),
            ValidationIssue::WrongPurpose
        );
        // Said so by the caller, with or without a system store, as for a certificate
        // authority serving as its own server's certificate, which the system's check
        // refuses for its basic constraints before its purpose.
        let served = self_signed("files.lab", (2020, 2045), true);
        let other_purpose = UserVerdict::Pinned {
            period: Period::Current,
            for_servers: false,
        };
        for roots in [vec![some_root()], Vec::new()] {
            let roots: Vec<&[u8]> = roots.iter().map(Vec::as_slice).collect();
            let (pinned, presented) = pinning_any(&roots, other_purpose);
            assert_eq!(
                pin_refused(&pinned, &presented, &served, "files.lab"),
                ValidationIssue::WrongPurpose
            );
        }
    }

    #[test]
    fn a_certificate_the_user_just_decided_on_goes_through_even_over() {
        let root = some_root();
        let served = self_signed("files.lab", (2000, 2001), false);
        let (decided, presented) = trusting_one(&[&root], &served.der, (2000, 2001), false);
        assert!(verdict_for(&decided, &served, "files.lab"));
        assert!(presented.lock().expect("slot").is_none(), "nothing kept");
        let (decided, _) = trusting_one(&[], &served.der, (2000, 2001), false);
        assert!(verdict_for(&decided, &served, "files.lab"));
    }

    #[test]
    fn a_forged_certificate_around_a_pinned_key_is_no_pin_refused() {
        // The server's own certificate, whose public key is pinned and public.
        let genuine = self_signed("files.lab", (2020, 2045), false);
        let root = some_root();
        let refusals = [
            (
                UserVerdict::Pinned {
                    period: Period::Expired,
                    for_servers: true,
                },
                (2000, 2001),
            ),
            (
                UserVerdict::Pinned {
                    period: Period::NotYetValid,
                    for_servers: true,
                },
                (2045, 2046),
            ),
            (
                UserVerdict::Pinned {
                    period: Period::Current,
                    for_servers: false,
                },
                (2020, 2045),
            ),
        ];
        for (verdict, validity) in refusals {
            let forgery = forged(&genuine, validity, "Forger CA");
            for roots in [vec![root.clone()], Vec::new()] {
                let roots: Vec<&[u8]> = roots.iter().map(Vec::as_slice).collect();
                let (pinned, presented) = pinning_any(&roots, verdict);
                for version in [&TLS13, &TLS12] {
                    let failed = handshake(&pinned, &forgery, "files.lab", version)
                        .expect_err("the forger cannot sign as the key's holder");
                    assert!(
                        matches!(
                            failed,
                            Error::InvalidCertificate(CertificateError::BadSignature)
                        ),
                        "the handshake signature fails, not the pin: {failed:?}"
                    );
                    assert!(
                        presented.lock().expect("slot").is_none(),
                        "nothing kept: no refusal of the user's pin to forget"
                    );
                }
                assert!(pinned.pending.lock().expect("pending").is_empty());
            }
        }
        // Current, under the name of an authority of the system that did not sign it: refused
        // by the system for its signature, a refusal no pin overrides; forged: the same.
        let forgery = forged(&genuine, (2020, 2045), "Other Root CA");
        let (pinned, presented) = pinning_any(
            &[&root],
            UserVerdict::Pinned {
                period: Period::Current,
                for_servers: true,
            },
        );
        assert!(!verdict_for(&pinned, &forgery, "files.lab"));
        assert!(presented.lock().expect("slot").is_none());
    }

    #[test]
    fn a_pin_refusal_held_by_one_handshake_never_reaches_another() {
        let refused = self_signed("files.lab", (2000, 2001), false);
        let current = self_signed("files.lab", (2020, 2045), false);
        let (refused_pin, current_pin) = (fingerprint(&refused.der), fingerprint(&current.der));
        let (pinned, presented) = trust_with(
            &[],
            Arc::new(move |der, _| {
                let print = fingerprint(der);
                UserVerdict::Pinned {
                    period: if print == refused_pin {
                        Period::Expired
                    } else {
                        Period::Current
                    },
                    for_servers: print == refused_pin || print == current_pin,
                }
            }),
        );
        // A forged handshake held a refusal it never proved: the next one is not refused for
        // it.
        let forgery = forged(&refused, (2000, 2001), "Forger CA");
        assert!(!verdict_for(&pinned, &forgery, "files.lab"));
        assert!(verdict_for(&pinned, &current, "files.lab"));
        assert!(presented.lock().expect("slot").is_none());
        assert_eq!(
            pin_refused(&pinned, &presented, &refused, "files.lab"),
            ValidationIssue::Expired
        );
        assert!(verdict_for(&pinned, &current, "files.lab"));
    }

    #[test]
    fn only_what_no_pin_overrides_refuses_a_pin() {
        let refusing = [
            CertificateError::Expired,
            CertificateError::NotValidYet,
            CertificateError::Revoked,
            CertificateError::BadSignature,
            CertificateError::InvalidPurpose,
        ];
        for problem in refusing {
            assert!(
                refuses_pin(&Error::InvalidCertificate(problem.clone())),
                "{problem:?}"
            );
        }
        let kept = [
            CertificateError::UnknownIssuer,
            CertificateError::NotValidForName,
            CertificateError::BadEncoding,
        ];
        for problem in kept {
            assert!(
                !refuses_pin(&Error::InvalidCertificate(problem.clone())),
                "{problem:?}"
            );
        }
        assert!(!refuses_pin(&Error::General("no system check".to_owned())));
    }

    #[test]
    fn an_unknown_issuer_that_is_the_certificate_itself_is_said_self_signed() {
        assert_eq!(
            ValidationIssue::UnknownIssuer.refined(true),
            ValidationIssue::SelfSigned
        );
        assert_eq!(
            ValidationIssue::UnknownIssuer.refined(false),
            ValidationIssue::UnknownIssuer
        );
        assert_eq!(
            ValidationIssue::Expired.refined(true),
            ValidationIssue::Expired,
            "the first problem met stays the one said"
        );
    }

    #[test]
    fn a_fingerprint_is_the_sha256_of_the_whole_certificate() {
        // SHA-256 of "abc", from FIPS 180-2.
        let expected = [
            0xba, 0x78, 0x16, 0xbf, 0x8f, 0x01, 0xcf, 0xea, 0x41, 0x41, 0x40, 0xde, 0x5d, 0xae,
            0x22, 0x23, 0xb0, 0x03, 0x61, 0xa3, 0x96, 0x17, 0x7a, 0x9c, 0xb4, 0x10, 0xff, 0x61,
            0xf2, 0x00, 0x15, 0xad,
        ];
        assert_eq!(fingerprint(b"abc"), expected);
    }
}
