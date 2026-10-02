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

//! The `WinRM` preflight, as the C# `WinRmPreflight`: before `PowerShell` starts, the server's
//! name is resolved, its port reached, and over HTTPS its TLS handshake made, each bounded,
//! so a server that cannot answer is said plainly rather than after `PowerShell`'s own long
//! wait. Through an SSH gateway there is no preflight: it would only reach the local forward.

use std::sync::Arc;
use std::time::Duration;

use heimdall_files::ftps_trust;
use tokio::net::TcpStream;
use tokio_rustls::rustls::pki_types::ServerName;

use crate::error::UiError;

/// How long each probe may take, as the C# `DefaultProbeTimeoutMs`.
pub const PROBE_TIMEOUT: Duration = Duration::from_secs(5);

/// How the TLS handshake is checked, over HTTPS.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TlsCheck {
    /// HTTP: no handshake.
    None,
    /// A certificate the system trusts.
    System,
    /// Any certificate, as the profile's "Skip certificate validation (insecure)" asks; the
    /// server still proves it holds the certificate's key.
    Skipped,
}

/// Resolves `host`, reaches `port` on it, then makes the TLS handshake `tls` asks for.
///
/// # Errors
///
/// [`UiError::WinRmHostUnresolved`], [`UiError::WinRmUnreachable`] or
/// [`UiError::WinRmTlsFailed`], with the C# preflight's meaning.
pub async fn ensure_reachable(host: &str, port: u16, tls: TlsCheck) -> Result<(), UiError> {
    let unresolved = || UiError::WinRmHostUnresolved {
        host: host.to_owned(),
    };
    let unreachable = || UiError::WinRmUnreachable {
        host: host.to_owned(),
        port,
    };
    let addresses: Vec<_> =
        tokio::time::timeout(PROBE_TIMEOUT, tokio::net::lookup_host((host, port)))
            .await
            .map_err(|_| unresolved())?
            .map_err(|error| {
                log::warn!("WinRM preflight: {host} does not resolve: {error}");
                unresolved()
            })?
            .collect();
    if addresses.is_empty() {
        return Err(unresolved());
    }
    let stream = tokio::time::timeout(PROBE_TIMEOUT, TcpStream::connect(addresses.as_slice()))
        .await
        .map_err(|_| unreachable())?
        .map_err(|error| {
            log::warn!("WinRM preflight: {host}:{port} unreachable: {error}");
            unreachable()
        })?;
    let skipped = match tls {
        TlsCheck::None => return Ok(()),
        TlsCheck::System => false,
        TlsCheck::Skipped => true,
    };
    let tls_failed = || UiError::WinRmTlsFailed {
        host: host.to_owned(),
        port,
    };
    let name = ServerName::try_from(host.to_owned()).map_err(|_| tls_failed())?;
    let connector = ftps_trust::connector(
        Arc::new(move |_: &[u8]| skipped),
        ftps_trust::PresentedSlot::default(),
    );
    match tokio::time::timeout(PROBE_TIMEOUT, connector.connect(name, stream)).await {
        Ok(Ok(_)) => Ok(()),
        Ok(Err(error)) => {
            log::warn!("WinRM preflight: TLS to {host}:{port} failed: {error}");
            Err(tls_failed())
        }
        Err(_) => Err(unreachable()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;

    #[tokio::test]
    async fn a_port_that_answers_passes_over_http() {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let port = listener.local_addr().expect("address").port();
        assert_eq!(
            ensure_reachable("127.0.0.1", port, TlsCheck::None).await,
            Ok(())
        );
    }

    #[tokio::test]
    async fn a_closed_port_is_unreachable() {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let port = listener.local_addr().expect("address").port();
        drop(listener);
        assert_eq!(
            ensure_reachable("127.0.0.1", port, TlsCheck::None).await,
            Err(UiError::WinRmUnreachable {
                host: "127.0.0.1".to_owned(),
                port
            })
        );
    }

    #[tokio::test]
    async fn a_name_that_does_not_resolve_is_said_as_such() {
        // `.invalid` never resolves (RFC 2606).
        assert_eq!(
            ensure_reachable("winrm.invalid", 5985, TlsCheck::None).await,
            Err(UiError::WinRmHostUnresolved {
                host: "winrm.invalid".to_owned()
            })
        );
    }

    /// A TLS server on the loopback with a certificate for `localhost` no system trusts.
    async fn self_signed_server() -> u16 {
        use tokio_rustls::TlsAcceptor;
        use tokio_rustls::rustls::ServerConfig;
        use tokio_rustls::rustls::crypto::ring::default_provider;
        use tokio_rustls::rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};

        let issued =
            rcgen::generate_simple_self_signed(vec!["localhost".to_owned()]).expect("cert");
        let certificate = CertificateDer::from(issued.cert.der().to_vec());
        let key =
            PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(issued.signing_key.serialize_der()));
        let config = ServerConfig::builder_with_provider(Arc::new(default_provider()))
            .with_safe_default_protocol_versions()
            .expect("versions")
            .with_no_client_auth()
            .with_single_cert(vec![certificate], key)
            .expect("server config");
        let acceptor = TlsAcceptor::from(Arc::new(config));
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let port = listener.local_addr().expect("address").port();
        tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let _ = acceptor.accept(stream).await;
            }
        });
        port
    }

    #[tokio::test]
    async fn a_certificate_no_system_trusts_passes_only_when_its_check_is_skipped() {
        let port = self_signed_server().await;
        assert_eq!(
            ensure_reachable("localhost", port, TlsCheck::Skipped).await,
            Ok(())
        );
        assert_eq!(
            ensure_reachable("localhost", port, TlsCheck::System).await,
            Err(UiError::WinRmTlsFailed {
                host: "localhost".to_owned(),
                port
            })
        );
    }

    #[tokio::test]
    async fn a_port_that_speaks_no_tls_fails_the_handshake() {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let port = listener.local_addr().expect("address").port();
        // Answers the handshake with plain HTTP, as a WinRM listener on 5985 does.
        tokio::spawn(async move {
            use tokio::io::AsyncWriteExt as _;
            if let Ok((mut stream, _)) = listener.accept().await {
                let _ = stream.write_all(b"HTTP/1.1 400 Bad Request\r\n\r\n").await;
            }
        });
        assert_eq!(
            ensure_reachable("127.0.0.1", port, TlsCheck::Skipped).await,
            Err(UiError::WinRmTlsFailed {
                host: "127.0.0.1".to_owned(),
                port
            })
        );
    }
}
