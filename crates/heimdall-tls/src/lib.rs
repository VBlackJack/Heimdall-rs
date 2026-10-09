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

use std::sync::{Arc, Mutex};

use ring::digest::{SHA256, digest};
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
pub const FINGERPRINT_LEN: usize = 32;

/// The SHA-256 of a whole certificate, as the C# shows and pins it.
pub type CertificateFingerprint = [u8; FINGERPRINT_LEN];

/// The fingerprint of certificate `der`.
#[must_use]
pub fn fingerprint(der: &[u8]) -> CertificateFingerprint {
    let mut fingerprint = [0; FINGERPRINT_LEN];
    fingerprint.copy_from_slice(digest(&SHA256, der).as_ref());
    fingerprint
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
    /// Its issuer revoked it.
    Revoked,
    /// This computer has no certificate authority to check it against.
    NoSystemStore,
    /// Anything else the check refused: a bad signature or encoding, a wrong purpose.
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
    /// validity period: checked again at every connection, as the C# checks its pins.
    Pinned {
        /// Where the moment falls.
        period: Period,
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

/// The check: the system, then what the user trusted for this server.
struct Trust {
    system: Option<Arc<WebPkiServerVerifier>>,
    trusted: UserTrust,
    presented: PresentedSlot,
    algorithms: WebPkiSupportedAlgorithms,
}

impl std::fmt::Debug for Trust {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Trust")
            .field("system", &self.system.is_some())
            .finish_non_exhaustive()
    }
}

impl Trust {
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

    /// Why a pinned certificate, whose validity period is at `period`, is refused; `None`
    /// when it goes through.
    fn pin_refusal(
        &self,
        period: Period,
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
        match (self.trusted)(end_entity.as_ref(), now) {
            UserVerdict::Decided => return Ok(ServerCertVerified::assertion()),
            UserVerdict::Pinned { period } => {
                let checked = Checked {
                    end_entity,
                    intermediates,
                    server_name,
                    ocsp_response,
                    now,
                };
                return match self.pin_refusal(period, &checked) {
                    None => Ok(ServerCertVerified::assertion()),
                    Some((refusal, issue)) => {
                        self.keep(end_entity.as_ref(), issue, true);
                        Err(refusal)
                    }
                };
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
        verify_tls12_signature(message, cert, dss, &self.algorithms)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        verify_tls13_signature(message, cert, dss, &self.algorithms)
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
    let trust = Trust {
        system,
        trusted,
        presented,
        algorithms: provider.signature_verification_algorithms,
    };
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

    use tokio_rustls::rustls::RootCertStore;
    use tokio_rustls::rustls::client::WebPkiServerVerifier;
    use tokio_rustls::rustls::client::danger::ServerCertVerifier as _;
    use tokio_rustls::rustls::crypto::ring::default_provider;
    use tokio_rustls::rustls::pki_types::{CertificateDer, ServerName, UnixTime};

    use super::{
        Period, Presented, PresentedSlot, Trust, UserTrust, UserVerdict, ValidationIssue,
        fingerprint, refuses_pin,
    };
    use tokio_rustls::rustls::{CertificateError, Error};

    /// A check trusting `system_roots` as the system store and the certificates of
    /// `trusted` as pinned by the user, all of them current.
    fn trust(system_roots: &[&[u8]], trusted: Vec<[u8; 32]>) -> (Trust, PresentedSlot) {
        trust_with(
            system_roots,
            Arc::new(move |der, _| {
                if trusted.contains(&fingerprint(der)) {
                    UserVerdict::Pinned {
                        period: Period::Current,
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
    ) -> (Trust, PresentedSlot) {
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
                UserVerdict::Pinned { period }
            }),
        )
    }

    /// The first second of `year`, in seconds since 1970.
    fn year_start(year: i32) -> i64 {
        rcgen::date_time_ymd(year, 1, 1).unix_timestamp()
    }

    /// A check trusting `system_roots` as the system store and what `trusted` says.
    fn trust_with(system_roots: &[&[u8]], trusted: UserTrust) -> (Trust, PresentedSlot) {
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
        let trust = Trust {
            system,
            trusted,
            presented: presented.clone(),
            algorithms: provider.signature_verification_algorithms,
        };
        (trust, presented)
    }

    fn verdict(trust: &Trust, cert: &[u8]) -> bool {
        verdict_for(trust, cert, "localhost")
    }

    /// Whether `trust` takes `cert` from a server named `name`.
    fn verdict_for(trust: &Trust, cert: &[u8], name: &str) -> bool {
        trust
            .verify_server_cert(
                &CertificateDer::from(cert.to_vec()),
                &[],
                &ServerName::try_from(name.to_owned()).expect("name"),
                &[],
                UnixTime::now(),
            )
            .is_ok()
    }

    /// The issue kept for the question once `trust` refused `cert` from `name`.
    fn issue_kept(
        trust: &Trust,
        presented: &PresentedSlot,
        cert: &[u8],
        name: &str,
    ) -> ValidationIssue {
        assert!(!verdict_for(trust, cert, name), "refused");
        presented
            .lock()
            .expect("slot")
            .take()
            .expect("kept for the question")
            .issue
    }

    /// A distinguished name of common name `common_name`.
    fn named(common_name: &str) -> rcgen::DistinguishedName {
        let mut distinguished_name = rcgen::DistinguishedName::new();
        distinguished_name.push(rcgen::DnType::CommonName, common_name);
        distinguished_name
    }

    /// An authority, and a certificate it issues for `name`, valid from the first of
    /// January of `from` to that of `until`.
    fn issued_by_authority(name: &str, validity: (i32, i32)) -> (Vec<u8>, Vec<u8>) {
        issued_by_authority_for(name, validity, &[])
    }

    /// An authority, and a certificate it issues for `name`, valid from the first of
    /// January of `from` to that of `until`, for the `purposes` given, any when none.
    fn issued_by_authority_for(
        name: &str,
        (from, until): (i32, i32),
        purposes: &[rcgen::ExtendedKeyUsagePurpose],
    ) -> (Vec<u8>, Vec<u8>) {
        use rcgen::{BasicConstraints, CertificateParams, IsCa, Issuer, KeyPair, date_time_ymd};

        let authority_key = KeyPair::generate().expect("key");
        let mut authority = CertificateParams::new(Vec::<String>::new()).expect("params");
        authority.distinguished_name = named("Lab Root CA");
        authority.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        let authority_cert = authority.self_signed(&authority_key).expect("authority");
        let issuer = Issuer::new(authority, authority_key);
        let mut server = CertificateParams::new(vec![name.to_owned()]).expect("params");
        server.distinguished_name = named(name);
        server.not_before = date_time_ymd(from, 1, 1);
        server.not_after = date_time_ymd(until, 1, 1);
        server.extended_key_usages = purposes.to_vec();
        let server_key = KeyPair::generate().expect("key");
        let server_cert = server.signed_by(&server_key, &issuer).expect("server");
        (authority_cert.der().to_vec(), server_cert.der().to_vec())
    }

    /// A certificate for `name` that is its own issuer, valid from the first of January of
    /// `from` to that of `until`; a certificate authority when `authority`, as
    /// `openssl req -x509` makes them.
    fn self_signed(name: &str, (from, until): (i32, i32), authority: bool) -> Vec<u8> {
        use rcgen::{BasicConstraints, CertificateParams, IsCa, KeyPair, date_time_ymd};

        let mut params = CertificateParams::new(vec![name.to_owned()]).expect("params");
        params.distinguished_name = named(name);
        params.not_before = date_time_ymd(from, 1, 1);
        params.not_after = date_time_ymd(until, 1, 1);
        if authority {
            params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        }
        let key = KeyPair::generate().expect("key");
        params.self_signed(&key).expect("cert").der().to_vec()
    }

    /// Why `trust` refused `cert`, a certificate the user trusted, from `name`; the
    /// refusal is kept marked pinned, never for the question.
    fn pin_refused(
        trust: &Trust,
        presented: &PresentedSlot,
        cert: &[u8],
        name: &str,
    ) -> ValidationIssue {
        assert!(!verdict_for(trust, cert, name), "refused");
        let kept = presented
            .lock()
            .expect("slot")
            .take()
            .expect("kept to say why");
        assert!(kept.pinned, "a pin refused, not a question");
        assert_eq!(kept.der, cert);
        kept.issue
    }

    /// A certificate that is no authority's: the system store of the tests that need one.
    fn some_root() -> Vec<u8> {
        issued_by_authority("root.lab", (2020, 2045)).0
    }

    #[test]
    fn a_pinned_certificate_over_is_refused_issued_or_self_signed() {
        let (authority, cert) = issued_by_authority("files.lab", (2000, 2001));
        let (pinned, presented) = trusting_one(&[&authority], &cert, (2000, 2001), true);
        assert_eq!(
            pin_refused(&pinned, &presented, &cert, "files.lab"),
            ValidationIssue::Expired
        );
        let root = some_root();
        let cert = self_signed("files.lab", (2000, 2001), false);
        let (pinned, presented) = trusting_one(&[&root], &cert, (2000, 2001), true);
        assert_eq!(
            pin_refused(&pinned, &presented, &cert, "files.lab"),
            ValidationIssue::Expired
        );
    }

    #[test]
    fn a_pinned_certificate_not_yet_valid_is_refused() {
        let root = some_root();
        let cert = self_signed("files.lab", (2045, 2046), false);
        let (pinned, presented) = trusting_one(&[&root], &cert, (2045, 2046), true);
        assert_eq!(
            pin_refused(&pinned, &presented, &cert, "files.lab"),
            ValidationIssue::NotYetValid
        );
    }

    #[test]
    fn a_pinned_certificate_over_is_refused_without_a_system_store() {
        let cert = self_signed("files.lab", (2000, 2001), false);
        let (pinned, presented) = trusting_one(&[], &cert, (2000, 2001), true);
        assert_eq!(
            pin_refused(&pinned, &presented, &cert, "files.lab"),
            ValidationIssue::Expired
        );
    }

    #[test]
    fn a_current_pinned_certificate_goes_through_what_a_pin_overrides() {
        let root = some_root();
        // Self-signed: no authority of the system issued it.
        let cert = self_signed("files.lab", (2020, 2045), false);
        let (pinned, presented) = trusting_one(&[&root], &cert, (2020, 2045), true);
        assert!(verdict_for(&pinned, &cert, "files.lab"));
        assert!(presented.lock().expect("slot").is_none(), "nothing kept");
        // Issued for another name than the one connected to.
        let (authority, cert) = issued_by_authority("files.lab", (2020, 2045));
        let (pinned, _) = trusting_one(&[&authority], &cert, (2020, 2045), true);
        assert!(verdict_for(&pinned, &cert, "other.lab"));
        // A certificate authority serving as its own server's certificate.
        let cert = self_signed("files.lab", (2020, 2045), true);
        let (pinned, _) = trusting_one(&[&root], &cert, (2020, 2045), true);
        assert!(verdict_for(&pinned, &cert, "files.lab"));
        // No system store: the pin alone.
        let cert = self_signed("files.lab", (2020, 2045), false);
        let (pinned, _) = trusting_one(&[], &cert, (2020, 2045), true);
        assert!(verdict_for(&pinned, &cert, "files.lab"));
    }

    #[test]
    fn a_pinned_certificate_for_another_purpose_is_refused() {
        let (authority, cert) = issued_by_authority_for(
            "files.lab",
            (2020, 2045),
            &[rcgen::ExtendedKeyUsagePurpose::ClientAuth],
        );
        let (pinned, presented) = trusting_one(&[&authority], &cert, (2020, 2045), true);
        assert_eq!(
            pin_refused(&pinned, &presented, &cert, "files.lab"),
            ValidationIssue::Other
        );
    }

    #[test]
    fn a_certificate_the_user_just_decided_on_goes_through_even_over() {
        let root = some_root();
        let cert = self_signed("files.lab", (2000, 2001), false);
        let (decided, presented) = trusting_one(&[&root], &cert, (2000, 2001), false);
        assert!(verdict_for(&decided, &cert, "files.lab"));
        assert!(presented.lock().expect("slot").is_none(), "nothing kept");
        let (decided, _) = trusting_one(&[], &cert, (2000, 2001), false);
        assert!(verdict_for(&decided, &cert, "files.lab"));
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
    fn what_the_system_trusts_goes_through_without_asking() {
        let issued =
            rcgen::generate_simple_self_signed(vec!["localhost".to_owned()]).expect("cert");
        let cert = issued.cert.der().to_vec();
        let (trusted, presented) = trust(&[&cert], Vec::new());
        assert!(verdict(&trusted, &cert));
        assert!(presented.lock().expect("slot").is_none(), "nothing to ask");
    }

    #[test]
    fn what_nobody_trusts_is_refused_and_kept_for_the_question() {
        let issued =
            rcgen::generate_simple_self_signed(vec!["localhost".to_owned()]).expect("cert");
        let cert = issued.cert.der().to_vec();
        // Another authority, under another name: one of the same name would be tried as the
        // issuer, and refused for its signature instead.
        let (other, _) = issued_by_authority("other.lab", (2020, 2045));
        let (refusing, presented) = trust(&[&other], Vec::new());
        assert!(!verdict(&refusing, &cert));
        assert_eq!(
            presented.lock().expect("slot").clone(),
            Some(Presented {
                der: cert.clone(),
                issue: ValidationIssue::UnknownIssuer,
                pinned: false,
            })
        );
        // No system store at all: the user's fingerprints alone.
        let (alone, _) = trust(&[], vec![fingerprint(&cert)]);
        assert!(verdict(&alone, &cert));
    }

    #[test]
    fn why_the_system_refused_is_kept_with_the_certificate() {
        // Issued by an authority the system trusts, for another name.
        let (authority, cert) = issued_by_authority("files.lab", (2020, 2045));
        let (system, presented) = trust(&[&authority], Vec::new());
        assert!(verdict_for(&system, &cert, "files.lab"), "its own name");
        assert_eq!(
            issue_kept(&system, &presented, &cert, "other.lab"),
            ValidationIssue::NameMismatch
        );
        // Over, and not begun yet.
        let (authority, cert) = issued_by_authority("files.lab", (2000, 2001));
        let (system, presented) = trust(&[&authority], Vec::new());
        assert_eq!(
            issue_kept(&system, &presented, &cert, "files.lab"),
            ValidationIssue::Expired
        );
        let (authority, cert) = issued_by_authority("files.lab", (2045, 2046));
        let (system, presented) = trust(&[&authority], Vec::new());
        assert_eq!(
            issue_kept(&system, &presented, &cert, "files.lab"),
            ValidationIssue::NotYetValid
        );
        // No system store at all.
        let (alone, presented) = trust(&[], Vec::new());
        assert_eq!(
            issue_kept(&alone, &presented, &cert, "files.lab"),
            ValidationIssue::NoSystemStore
        );
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
