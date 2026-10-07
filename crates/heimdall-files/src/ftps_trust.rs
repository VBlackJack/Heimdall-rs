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

//! The certificate check of explicit FTPS, as the C# one: a certificate the system trusts
//! goes through; otherwise one the user trusted for this server, which the caller decides,
//! as it keeps the user's pins; otherwise the handshake stops and the certificate is kept,
//! with why the system did not vouch for it, for the user to be shown it and asked.
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
use tokio_rustls::rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use tokio_rustls::rustls::{
    CertificateError, ClientConfig, DigitallySignedStruct, Error, RootCertStore, SignatureScheme,
};

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

/// A certificate neither the system nor the user trusts, kept for the question.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Presented {
    /// The certificate, whole (DER).
    pub der: Vec<u8>,
    /// Why the system did not vouch for it.
    pub issue: ValidationIssue,
}

/// Where a certificate neither trusted by the system nor by the user is kept, for the
/// question.
pub type PresentedSlot = Arc<Mutex<Option<Presented>>>;

/// Whether the user trusted a certificate, given whole (DER), for the server connected to.
pub type UserTrust = Arc<dyn Fn(&[u8]) -> bool + Send + Sync>;

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

impl ServerCertVerifier for Trust {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        server_name: &ServerName<'_>,
        ocsp_response: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, Error> {
        if (self.trusted)(end_entity.as_ref()) {
            return Ok(ServerCertVerified::assertion());
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
        if let Ok(mut slot) = self.presented.lock() {
            *slot = Some(Presented {
                der: end_entity.as_ref().to_vec(),
                issue,
            });
        }
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

/// A TLS client for explicit FTPS trusting what the system trusts and what `trusted`
/// accepts; a certificate trusted by neither lands in `presented` and stops the handshake.
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

    use super::{Presented, PresentedSlot, Trust, UserTrust, ValidationIssue, fingerprint};

    /// A check trusting `system_roots` as the system store and the certificates of
    /// `trusted` as the user's.
    fn trust(system_roots: &[&[u8]], trusted: Vec<[u8; 32]>) -> (Trust, PresentedSlot) {
        let trusted: UserTrust = Arc::new(move |der| trusted.contains(&fingerprint(der)));
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

    /// An authority, and a certificate it issues for `name`, valid from the first of
    /// January of `from` to that of `until`.
    fn issued_by_authority(name: &str, (from, until): (i32, i32)) -> (Vec<u8>, Vec<u8>) {
        use rcgen::{
            BasicConstraints, CertificateParams, DistinguishedName, DnType, IsCa, Issuer, KeyPair,
            date_time_ymd,
        };

        let named = |common_name: &str| {
            let mut distinguished_name = DistinguishedName::new();
            distinguished_name.push(DnType::CommonName, common_name);
            distinguished_name
        };
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
        let server_key = KeyPair::generate().expect("key");
        let server_cert = server.signed_by(&server_key, &issuer).expect("server");
        (authority_cert.der().to_vec(), server_cert.der().to_vec())
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
