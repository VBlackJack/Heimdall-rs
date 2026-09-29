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
//! goes through; otherwise one the user trusted for this server; otherwise the handshake
//! stops and the certificate is kept, for the user to be shown it and asked.
//!
//! The decision is taken in the handshake, before any password is sent. Every handshake
//! signature is verified against the certificate, so a server proves it holds its key: a
//! replayed certificate would not match a trusted fingerprint.

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
    ClientConfig, DigitallySignedStruct, Error, RootCertStore, SignatureScheme,
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

/// Where a certificate neither trusted by the system nor by the user is kept, for the
/// question.
pub type PresentedSlot = Arc<Mutex<Option<Vec<u8>>>>;

/// The check: the system, then the fingerprints the user trusted for this server.
#[derive(Debug)]
struct Trust {
    system: Option<Arc<WebPkiServerVerifier>>,
    trusted: Vec<CertificateFingerprint>,
    presented: PresentedSlot,
    algorithms: WebPkiSupportedAlgorithms,
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
        if self.trusted.contains(&fingerprint(end_entity)) {
            return Ok(ServerCertVerified::assertion());
        }
        let refusal = match &self.system {
            Some(system) => match system.verify_server_cert(
                end_entity,
                intermediates,
                server_name,
                ocsp_response,
                now,
            ) {
                Ok(verified) => return Ok(verified),
                Err(refusal) => refusal,
            },
            None => Error::General("no system certificate check".to_owned()),
        };
        if let Ok(mut slot) = self.presented.lock() {
            *slot = Some(end_entity.as_ref().to_vec());
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

/// A TLS client for explicit FTPS trusting what the system trusts and the certificates of
/// `trusted`; a certificate trusted by neither lands in `presented` and stops the handshake.
///
/// # Panics
///
/// Never: ring's default protocol versions are always accepted.
#[must_use]
pub fn connector(trusted: Vec<CertificateFingerprint>, presented: PresentedSlot) -> TlsConnector {
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

    use super::{PresentedSlot, Trust, fingerprint};

    /// A check trusting `system_roots` as the system store and `trusted` as the user's.
    fn trust(system_roots: &[&[u8]], trusted: Vec<[u8; 32]>) -> (Trust, PresentedSlot) {
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
        trust
            .verify_server_cert(
                &CertificateDer::from(cert.to_vec()),
                &[],
                &ServerName::try_from("localhost").expect("name"),
                &[],
                UnixTime::now(),
            )
            .is_ok()
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
        let other = rcgen::generate_simple_self_signed(vec!["localhost".to_owned()]).expect("cert");
        let (refusing, presented) = trust(&[other.cert.der()], Vec::new());
        assert!(!verdict(&refusing, &cert));
        assert_eq!(
            presented.lock().expect("slot").as_deref(),
            Some(cert.as_slice())
        );
        // No system store at all: the user's fingerprints alone.
        let (alone, _) = trust(&[], vec![fingerprint(&cert)]);
        assert!(verdict(&alone, &cert));
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
