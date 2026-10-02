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

//! A logon the server refuses, against a fake server over an in-memory pipe.
//!
//! The server answers the X.224 negotiation with Network Level Authentication, completes a
//! TLS handshake with the fixture certificate, reads the client's first `CredSSP` request,
//! then refuses it with an NTSTATUS, as Windows does from `CredSSP` version 3, or closes.

#![allow(
    clippy::large_futures,
    reason = "a connection future is large; tests await it once"
)]

use std::sync::Arc;
use std::time::Duration;

use heimdall_rdp::{
    Fingerprint, KnownRdpHosts, RdpConfig, RdpError, Refusal, Security, Timeouts, connect_over,
};
use ironrdp::connector::sspi::credssp::{NStatusCode, TsRequest};
use ironrdp::pdu::nego::{ConnectionConfirm, ResponseFlags, SecurityProtocol};
use ironrdp::pdu::x224::X224;
use tokio::io::{AsyncRead, AsyncReadExt as _, AsyncWrite, AsyncWriteExt as _};
use tokio_rustls::TlsAcceptor;
use tokio_rustls::rustls::ServerConfig;
use tokio_rustls::rustls::crypto::ring::default_provider;
use tokio_rustls::rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;

const HOST: &str = "rdp.test";
const PORT: u16 = 3389;

const CERT: &[u8] = include_bytes!("fixtures/server-cert.der");
const KEY: &[u8] = include_bytes!("fixtures/server-key.der");
/// SHA-256 of the fixture's `SubjectPublicKeyInfo`, computed by openssl.
const EXPECTED_PIN: &str = include_str!("fixtures/server-spki-sha256.txt");

/// NTSTATUS of an expired password.
const STATUS_PASSWORD_EXPIRED: u32 = 0xC000_0071;
/// The `CredSSP` version Windows Server 2016 and later speak.
const CREDSSP_VERSION: u32 = 6;

/// Bound on waiting for what the client does next.
const WAIT: Duration = Duration::from_secs(10);

/// What the fake server does with the client's first `CredSSP` request.
#[derive(Clone, Copy)]
enum Answer {
    /// Refuses it with this NTSTATUS.
    Status(u32),
    /// Closes the connection.
    Close,
}

fn acceptor() -> TlsAcceptor {
    let config = ServerConfig::builder_with_provider(Arc::new(default_provider()))
        .with_safe_default_protocol_versions()
        .expect("versions")
        .with_no_client_auth()
        .with_single_cert(
            vec![CertificateDer::from(CERT.to_vec())],
            PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(KEY.to_vec())),
        )
        .expect("certificate");
    TlsAcceptor::from(Arc::new(config))
}

async fn serve<S: AsyncRead + AsyncWrite + Unpin>(mut stream: S, answer: Answer) {
    let mut header = [0; 4];
    stream.read_exact(&mut header).await.expect("TPKT header");
    let length = usize::from(u16::from_be_bytes([header[2], header[3]]));
    let mut request = vec![0; length - header.len()];
    stream
        .read_exact(&mut request)
        .await
        .expect("X.224 request");
    let confirm = ironrdp_core::encode_vec(&X224(ConnectionConfirm::Response {
        flags: ResponseFlags::empty(),
        protocol: SecurityProtocol::HYBRID,
    }))
    .expect("encode");
    stream.write_all(&confirm).await.expect("confirm");
    let mut tls = acceptor().accept(stream).await.expect("TLS");
    let mut buffer = [0; 4096];
    let read = tokio::time::timeout(WAIT, tls.read(&mut buffer))
        .await
        .expect("in time")
        .expect("first CredSSP request");
    assert!(read > 0, "the client sent its first CredSSP request");
    match answer {
        Answer::Status(status) => {
            let refusal = TsRequest {
                version: CREDSSP_VERSION,
                error_code: Some(NStatusCode(status)),
                ..TsRequest::default()
            };
            let mut encoded = Vec::new();
            refusal.encode_ts_request(&mut encoded).expect("encode");
            tls.write_all(&encoded).await.expect("refusal");
            tls.flush().await.expect("flush");
            // Kept open until the client is gone: its reading the refusal, not a close, ends it.
            let _ = tokio::time::timeout(WAIT, tls.read(&mut buffer)).await;
        }
        Answer::Close => {
            let _ = tls.shutdown().await;
        }
    }
}

fn config(known_hosts: &std::path::Path) -> RdpConfig {
    let pin: Fingerprint = format!("SHA256:{}", EXPECTED_PIN.trim())
        .parse()
        .expect("fixture pin");
    RdpConfig {
        host: HOST.to_owned(),
        port: PORT,
        domain: None,
        desktop: (1024, 768),
        keyboard_layout: 0,
        security: Security::Nla,
        known_hosts: KnownRdpHosts::new(known_hosts),
        accepted: Some(pin),
        timeouts: Timeouts {
            connect: WAIT,
            handshake: WAIT,
            logon: WAIT,
        },
        clipboard: false,
        drives: Vec::new(),
        trusted_for_run: Vec::new(),
        options: heimdall_core::profile::RdpOptions::default(),
        several_servers: false,
        kerberos: false,
        time_zone: None,
        desktop_scale: 100,
    }
}

async fn attempt(answer: Answer) -> RdpError {
    let dir = tempfile::tempdir().expect("dir");
    let config = config(&dir.path().join("known_rdp_hosts"));
    let (client, server) = tokio::io::duplex(1 << 16);
    let server = tokio::spawn(serve(server, answer));
    let credentials: heimdall_rdp::AskCredentials = Box::new(|| {
        Box::pin(std::future::ready(Some((
            "alice".to_owned(),
            Zeroizing::new("hunter2-password".to_owned()),
        ))))
    });
    let outcome = tokio::time::timeout(
        WAIT * 3,
        connect_over(
            Box::new(client),
            "127.0.0.1:50000".parse().expect("address"),
            &config,
            credentials,
            &CancellationToken::new(),
        ),
    )
    .await
    .expect("in time");
    server.await.expect("server");
    match outcome {
        Ok(_) => panic!("the connection opened"),
        Err(error) => error,
    }
}

#[tokio::test]
async fn a_logon_refused_with_an_expired_password_says_so() {
    let error = attempt(Answer::Status(STATUS_PASSWORD_EXPIRED)).await;
    assert!(
        matches!(error, RdpError::Authentication(Refusal::PasswordExpired)),
        "{error:?}"
    );
}

#[tokio::test]
async fn a_server_closing_before_the_account_was_sent_did_not_refuse_it() {
    // Only the negotiate message went out: nothing the server could have refused.
    let error = attempt(Answer::Close).await;
    assert!(!matches!(error, RdpError::Authentication(_)), "{error:?}");
}
