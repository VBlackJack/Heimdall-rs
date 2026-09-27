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

//! The TLS client of an FTPS session.
//!
//! FTP servers often present self-signed certificates, so no certificate authority is asked:
//! trust comes from the pin, checked once the handshake is done and before the user name is
//! sent. Each handshake signature is still verified, so the server proves it holds the key of
//! the certificate it shows.
//!
//! Session resumption stays on: many servers (vsftpd, `FileZilla` Server) accept a data
//! connection only when it resumes the control connection's TLS session, proof that both come
//! from the same client.

use std::sync::Arc;

use tokio_rustls::TlsConnector;
use tokio_rustls::rustls::client::danger::{
    HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier,
};
use tokio_rustls::rustls::crypto::{
    CryptoProvider, WebPkiSupportedAlgorithms, verify_tls12_signature, verify_tls13_signature,
};
use tokio_rustls::rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use tokio_rustls::rustls::{ClientConfig, DigitallySignedStruct, Error, SignatureScheme};

/// Accepts any certificate, verifies every handshake signature against it.
#[derive(Debug)]
struct PinnedLater {
    algorithms: WebPkiSupportedAlgorithms,
}

impl ServerCertVerifier for PinnedLater {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, Error> {
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

/// The TLS connector of one session: ring, TLS 1.2 and 1.3, resumption kept in memory for the
/// data connections, no key log whatever the environment says.
///
/// # Panics
///
/// Never: the ring provider supports the default protocol versions.
#[must_use]
pub fn connector() -> TlsConnector {
    let provider = Arc::new(tokio_rustls::rustls::crypto::ring::default_provider());
    let algorithms = provider.signature_verification_algorithms;
    let config = ClientConfig::builder_with_provider(Arc::<CryptoProvider>::clone(&provider))
        .with_safe_default_protocol_versions()
        .expect("ring supports the default protocol versions")
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(PinnedLater { algorithms }))
        .with_no_client_auth();
    TlsConnector::from(Arc::new(config))
}
