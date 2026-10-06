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

//! The TLS client of an RDP connection.
//!
//! RDP servers mostly present self-signed certificates, so no certificate authority is
//! asked: trust comes from the pin checked after the handshake. The handshake signature is
//! always verified, so the server proves it holds the private key of the certificate it
//! shows; without that, a replayed certificate would match the pin.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tokio_rustls::TlsConnector;
use tokio_rustls::rustls::client::Resumption;
use tokio_rustls::rustls::client::WebPkiServerVerifier;
use tokio_rustls::rustls::client::danger::{
    HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier,
};
use tokio_rustls::rustls::crypto::{
    CryptoProvider, WebPkiSupportedAlgorithms, verify_tls12_signature, verify_tls13_signature,
};
use tokio_rustls::rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use tokio_rustls::rustls::{
    ClientConfig, DigitallySignedStruct, Error, RootCertStore, SignatureScheme,
};

/// Whether the server's chain validated, for its name, against the certificates this computer
/// trusts, as Windows checks a server for mstsc's "Do not connect if authentication fails".
/// Asked for under strict server authentication only.
#[derive(Debug, Clone, Default)]
pub struct SystemTrust(Arc<AtomicBool>);

impl SystemTrust {
    /// Whether the chain validated.
    #[must_use]
    pub fn validated(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

/// The check of the system's certificate store; `None` when it has nothing to read.
fn system_verifier(provider: &Arc<CryptoProvider>) -> Option<Arc<WebPkiServerVerifier>> {
    let mut roots = RootCertStore::empty();
    let loaded = rustls_native_certs::load_native_certs();
    for error in &loaded.errors {
        log::debug!("a system certificate could not be read: {error}");
    }
    let _ = roots.add_parsable_certificates(loaded.certs);
    if roots.is_empty() {
        return None;
    }
    WebPkiServerVerifier::builder_with_provider(Arc::new(roots), Arc::clone(provider))
        .build()
        .ok()
}

/// Accepts any certificate, verifies every handshake signature against it; and, asked,
/// notes whether the system's store validates its chain.
#[derive(Debug)]
struct PinnedLater {
    algorithms: WebPkiSupportedAlgorithms,
    system: Option<(Arc<WebPkiServerVerifier>, SystemTrust)>,
}

impl ServerCertVerifier for PinnedLater {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        server_name: &ServerName<'_>,
        ocsp_response: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, Error> {
        if let Some((verifier, trust)) = &self.system {
            let validated = verifier
                .verify_server_cert(end_entity, intermediates, server_name, ocsp_response, now)
                .is_ok();
            trust.0.store(validated, Ordering::Release);
        }
        // Decided by the pin once the handshake is done, before any credential is sent.
        Ok(ServerCertVerified::assertion())
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

/// The TLS connector: ring, TLS 1.2 and 1.3, no session resumption (`CredSSP` does not
/// support it), no key log whatever the environment says.
///
/// # Panics
///
/// Never: the ring provider supports the default protocol versions.
#[must_use]
pub fn connector(system: Option<&SystemTrust>) -> TlsConnector {
    let provider = Arc::new(tokio_rustls::rustls::crypto::ring::default_provider());
    let algorithms = provider.signature_verification_algorithms;
    // A store with nothing to read validates nothing: the flag stays down.
    let system = system
        .and_then(|trust| system_verifier(&provider).map(|verifier| (verifier, trust.clone())));
    let mut config = ClientConfig::builder_with_provider(Arc::<CryptoProvider>::clone(&provider))
        .with_safe_default_protocol_versions()
        .expect("ring supports the default protocol versions")
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(PinnedLater { algorithms, system }))
        .with_no_client_auth();
    config.resumption = Resumption::disabled();
    TlsConnector::from(Arc::new(config))
}
