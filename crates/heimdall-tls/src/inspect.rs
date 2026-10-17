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

//! A handshake made only to read the certificates a server presents, as the C# certificate
//! inspector's `SslStream` whose callback takes every certificate
//! (`CertInspectorEngine.RetrieveCertificateAsync`).
//!
//! Nothing but the handshake goes over it, so no trust decision is taken: an expired
//! certificate, one for another name or one no authority issued is read all the same, as
//! the inspector must show it. Every handshake signature is still checked against the
//! certificate, so the chain read is the one of a server that holds its key, in TLS 1.2 and
//! TLS 1.3 alike.

use std::sync::Arc;

use tokio_rustls::TlsConnector;
use tokio_rustls::rustls::client::danger::{
    HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier,
};
use tokio_rustls::rustls::crypto::{
    WebPkiSupportedAlgorithms, ring::default_provider, verify_tls12_signature,
    verify_tls13_signature,
};
use tokio_rustls::rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use tokio_rustls::rustls::{
    AlertDescription, ClientConfig, ClientConnection, DigitallySignedStruct, Error,
    PeerIncompatible, ProtocolVersion, SignatureScheme,
};

/// The TLS version a handshake settled on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TlsVersion {
    /// TLS 1.2.
    Tls12,
    /// TLS 1.3.
    Tls13,
}

impl TlsVersion {
    /// The version as the C# `TlsAuditEngine.FormatTlsProtocol` names it: a protocol name,
    /// the same in every language.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Tls12 => "TLS 1.2",
            Self::Tls13 => "TLS 1.3",
        }
    }
}

/// What a server presented in a handshake.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PresentedChain {
    /// The certificates in the order the server sent them, its own first (DER).
    pub certificates: Vec<Vec<u8>>,
    /// The version the handshake settled on.
    pub version: TlsVersion,
}

/// The check of a handshake made to read certificates: any certificate goes through, every
/// handshake signature is verified.
#[derive(Debug)]
struct ReadOnly {
    algorithms: WebPkiSupportedAlgorithms,
}

impl ServerCertVerifier for ReadOnly {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, Error> {
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

/// The check of [`inspection_connector`], with ring's signature algorithms.
fn read_only() -> Arc<ReadOnly> {
    Arc::new(ReadOnly {
        algorithms: default_provider().signature_verification_algorithms,
    })
}

/// The client configuration of [`inspection_connector`].
///
/// # Panics
///
/// Never: ring's default protocol versions are always accepted.
fn inspection_config() -> ClientConfig {
    ClientConfig::builder_with_provider(Arc::new(default_provider()))
        .with_safe_default_protocol_versions()
        .expect("ring's default protocol versions")
        .dangerous()
        .with_custom_certificate_verifier(read_only())
        .with_no_client_auth()
}

/// A TLS client, 1.2 or 1.3, made only to read what a server presents: it takes any
/// certificate and checks every handshake signature. Nothing may be sent over it once the
/// handshake is over: no certificate was checked.
#[must_use]
pub fn inspection_connector() -> TlsConnector {
    TlsConnector::from(Arc::new(inspection_config()))
}

/// The name `host` is reached by in the handshake: its address when it is one, sent
/// without a server name as an address may not be one, else its DNS name. `None` when it
/// is neither.
#[must_use]
pub fn inspection_server_name(host: &str) -> Option<ServerName<'static>> {
    ServerName::try_from(host.to_owned()).ok()
}

/// Whether `refusal`, a handshake's, says the server speaks only a TLS version older than
/// 1.2, which Heimdall refuses (owner decision of 2026-10-16: no TLS 1.0 nor 1.1, a clear
/// message instead): its server hello chose one (rustls then alerts `protocol_version`
/// itself), or it answered the client hello with a `protocol_version` alert.
#[must_use]
pub fn is_obsolete_version(refusal: &Error) -> bool {
    matches!(
        refusal,
        Error::PeerIncompatible(PeerIncompatible::ServerDoesNotSupportTls12Or13)
            | Error::AlertReceived(AlertDescription::ProtocolVersion)
    )
}

/// Whether the failed handshake `failure` of an [`inspection_connector`] was refused for
/// [`is_obsolete_version`]: tokio-rustls carries rustls' error inside the I/O error.
#[must_use]
pub fn failed_on_obsolete_version(failure: &std::io::Error) -> bool {
    failure
        .get_ref()
        .and_then(|inner| inner.downcast_ref::<Error>())
        .is_some_and(is_obsolete_version)
}

/// What the server of `connection`, its handshake over, presented; `None` when it presented
/// no certificate or the version is neither TLS 1.2 nor TLS 1.3.
#[must_use]
pub fn presented_chain(connection: &ClientConnection) -> Option<PresentedChain> {
    let version = match connection.protocol_version()? {
        ProtocolVersion::TLSv1_2 => TlsVersion::Tls12,
        ProtocolVersion::TLSv1_3 => TlsVersion::Tls13,
        _ => return None,
    };
    let certificates: Vec<Vec<u8>> = connection
        .peer_certificates()?
        .iter()
        .map(|certificate| certificate.as_ref().to_vec())
        .collect();
    (!certificates.is_empty()).then_some(PresentedChain {
        certificates,
        version,
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use rcgen::{
        BasicConstraints, CertificateParams, DistinguishedName, DnType, IsCa, Issuer, KeyPair,
        date_time_ymd,
    };
    use tokio_rustls::rustls::crypto::ring::default_provider;
    use tokio_rustls::rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
    use tokio_rustls::rustls::server::{ClientHello, ResolvesServerCert};
    use tokio_rustls::rustls::sign::CertifiedKey;
    use tokio_rustls::rustls::version::{TLS12, TLS13};
    use tokio_rustls::rustls::{
        ClientConfig, ClientConnection, Error, ServerConfig, ServerConnection,
        SupportedProtocolVersion,
    };

    use super::{
        PresentedChain, TlsVersion, failed_on_obsolete_version, inspection_config,
        inspection_connector, is_obsolete_version, presented_chain, read_only,
    };
    use crate::inspection_server_name;

    /// Flights exchanged at most before a test handshake is said stuck.
    const MAX_FLIGHTS: usize = 10;

    /// The test server's chain, whatever the client asks.
    #[derive(Debug)]
    struct Fixed(Arc<CertifiedKey>);

    impl ResolvesServerCert for Fixed {
        fn resolve(&self, _: ClientHello<'_>) -> Option<Arc<CertifiedKey>> {
            Some(Arc::clone(&self.0))
        }
    }

    /// A distinguished name of common name `common_name`.
    fn named(common_name: &str) -> DistinguishedName {
        let mut name = DistinguishedName::new();
        name.push(DnType::CommonName, common_name);
        name
    }

    /// A chain for `names`, valid from the first of January of `from` to that of `until`,
    /// issued by an authority sent after it, and the key the server signs with (PKCS #8).
    fn chain(names: &[&str], (from, until): (i32, i32)) -> (Vec<Vec<u8>>, Vec<u8>) {
        let authority_key = KeyPair::generate().expect("key");
        let mut authority = CertificateParams::new(Vec::<String>::new()).expect("params");
        authority.distinguished_name = named("Lab Root CA");
        authority.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        let authority_der = authority
            .self_signed(&authority_key)
            .expect("authority")
            .der()
            .to_vec();
        let issuer = Issuer::new(authority, authority_key);
        let key = KeyPair::generate().expect("key");
        let mut leaf = CertificateParams::new(
            names
                .iter()
                .map(|name| (*name).to_owned())
                .collect::<Vec<_>>(),
        )
        .expect("params");
        leaf.distinguished_name = named(names.first().copied().unwrap_or("leaf"));
        leaf.not_before = date_time_ymd(from, 1, 1);
        leaf.not_after = date_time_ymd(until, 1, 1);
        let leaf_der = leaf.signed_by(&key, &issuer).expect("leaf").der().to_vec();
        (vec![leaf_der, authority_der], key.serialize_der())
    }

    /// A handshake in `version` with a server presenting `certificates` and signing with
    /// `key`, reached as `host`; what the inspection read, or the client's error.
    fn inspect(
        certificates: &[Vec<u8>],
        key: &[u8],
        host: &str,
        version: &'static SupportedProtocolVersion,
    ) -> Result<PresentedChain, Error> {
        let provider = Arc::new(default_provider());
        let signing = provider
            .key_provider
            .load_private_key(PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key.to_vec())))
            .expect("signing key");
        let certified = CertifiedKey::new(
            certificates
                .iter()
                .map(|der| CertificateDer::from(der.clone()))
                .collect(),
            signing,
        );
        let server_config = ServerConfig::builder_with_provider(provider)
            .with_protocol_versions(&[version])
            .expect("version")
            .with_no_client_auth()
            .with_cert_resolver(Arc::new(Fixed(Arc::new(certified))));
        // The inspection's own check, on one version at a time.
        let client_config = ClientConfig::builder_with_provider(Arc::new(default_provider()))
            .with_protocol_versions(&[version])
            .expect("version")
            .dangerous()
            .with_custom_certificate_verifier(read_only())
            .with_no_client_auth();
        let mut client = ClientConnection::new(
            Arc::new(client_config),
            inspection_server_name(host).expect("name"),
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
                return Ok(presented_chain(&client).expect("presented"));
            }
        }
        panic!("the handshake did not end");
    }

    #[test]
    fn the_whole_chain_and_the_version_are_read_in_tls_1_3_and_1_2() {
        let (certificates, key) = chain(&["files.lab"], (2020, 2090));
        for (version, expected) in [(&TLS13, TlsVersion::Tls13), (&TLS12, TlsVersion::Tls12)] {
            let read = inspect(&certificates, &key, "files.lab", version).expect("read");
            assert_eq!(read.version, expected);
            assert_eq!(
                read.certificates, certificates,
                "leaf first, then its issuer"
            );
        }
        assert_eq!(TlsVersion::Tls13.label(), "TLS 1.3");
        assert_eq!(TlsVersion::Tls12.label(), "TLS 1.2");
        // The connector the inspector uses is made without a system store to read.
        let _ = inspection_connector();
    }

    #[test]
    fn an_expired_certificate_or_one_for_another_name_is_read_all_the_same() {
        let (expired, key) = chain(&["files.lab"], (2000, 2001));
        let read = inspect(&expired, &key, "files.lab", &TLS13).expect("expired, read");
        assert_eq!(read.certificates[0], expired[0]);
        let (other, key) = chain(&["other.lab"], (2020, 2090));
        let read = inspect(&other, &key, "files.lab", &TLS12).expect("other name, read");
        assert_eq!(read.certificates[0], other[0]);
        let read = inspect(&other, &key, "10.0.0.1", &TLS13).expect("by address, read");
        assert_eq!(read.certificates[0], other[0]);
    }

    #[test]
    fn a_server_that_does_not_hold_the_key_of_its_certificate_is_refused() {
        let (certificates, _) = chain(&["files.lab"], (2020, 2090));
        let other_key = KeyPair::generate().expect("key").serialize_der();
        for version in [&TLS13, &TLS12] {
            assert!(
                inspect(&certificates, &other_key, "files.lab", version).is_err(),
                "the handshake signature is checked in {version:?}"
            );
        }
    }

    /// The record a server speaking TLS 1.0 alone answers a client hello with: a server
    /// hello choosing TLS 1.0 (`0x0301`), `TLS_RSA_WITH_AES_128_CBC_SHA`, no extension.
    fn tls10_server_hello() -> Vec<u8> {
        const TLS10: [u8; 2] = [0x03, 0x01];
        const HANDSHAKE_RECORD: u8 = 0x16;
        const SERVER_HELLO: u8 = 0x02;
        const RSA_AES_128_CBC_SHA: [u8; 2] = [0x00, 0x2F];
        const RANDOM_LEN: usize = 32;
        let mut hello = TLS10.to_vec();
        hello.extend([0x5A; RANDOM_LEN]);
        // No session identifier, the suite, no compression.
        hello.push(0);
        hello.extend(RSA_AES_128_CBC_SHA);
        hello.push(0);
        let mut message = vec![
            SERVER_HELLO,
            0,
            0,
            u8::try_from(hello.len()).expect("short"),
        ];
        message.extend(hello);
        let mut record = vec![HANDSHAKE_RECORD];
        record.extend(TLS10);
        record.extend(u16::try_from(message.len()).expect("short").to_be_bytes());
        record.extend(message);
        record
    }

    /// The fatal `protocol_version` alert a server refusing every version offered answers.
    fn protocol_version_alert() -> Vec<u8> {
        const ALERT_RECORD: u8 = 0x15;
        const FATAL: u8 = 2;
        const PROTOCOL_VERSION: u8 = 70;
        vec![ALERT_RECORD, 0x03, 0x01, 0, 2, FATAL, PROTOCOL_VERSION]
    }

    /// What the inspection's client makes of `answer`, the whole of a server's reply to its
    /// client hello.
    fn answered(answer: &[u8]) -> Error {
        let mut client = ClientConnection::new(
            Arc::new(inspection_config()),
            inspection_server_name("old.lab").expect("name"),
        )
        .expect("client");
        let mut hello = Vec::new();
        while client.wants_write() {
            client.write_tls(&mut hello).expect("client hello");
        }
        let mut flight = answer;
        client.read_tls(&mut flight).expect("client reads");
        client.process_new_packets().expect_err("refused")
    }

    #[test]
    fn a_server_speaking_only_tls_1_0_or_1_1_is_told_apart_from_other_failures() {
        for answer in [tls10_server_hello(), protocol_version_alert()] {
            let refusal = answered(&answer);
            assert!(is_obsolete_version(&refusal), "{refusal:?}");
            // As tokio-rustls hands it over.
            let failure = std::io::Error::new(std::io::ErrorKind::InvalidData, refusal);
            assert!(failed_on_obsolete_version(&failure));
        }
        let handshake_failure = answered(&[0x15, 0x03, 0x03, 0, 2, 2, 40]);
        assert!(
            !is_obsolete_version(&handshake_failure),
            "{handshake_failure:?}"
        );
        assert!(!failed_on_obsolete_version(&std::io::Error::other(
            "refused"
        )));
        assert!(!failed_on_obsolete_version(&std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            handshake_failure
        )));
    }

    #[test]
    fn a_host_that_is_no_name_nor_address_has_no_server_name() {
        assert!(inspection_server_name("files.lab").is_some());
        assert!(inspection_server_name("2001:db8::1").is_some());
        assert!(inspection_server_name("not a name").is_none());
        assert!(inspection_server_name("").is_none());
    }
}
