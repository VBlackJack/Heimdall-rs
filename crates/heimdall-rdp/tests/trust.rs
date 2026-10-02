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

//! The gate in front of the credentials, against a fake server over an in-memory pipe.
//!
//! The server answers the X.224 negotiation with Network Level Authentication, completes a
//! TLS handshake with a fixture certificate, then counts the bytes that reach it: `CredSSP`
//! starts with the client's first message, so nothing after the handshake means nothing
//! was sent that could carry a credential.

#![allow(
    clippy::large_futures,
    reason = "a connection future is large; tests await it once"
)]

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use heimdall_rdp::{
    AskCredentials, Fingerprint, KnownRdpHosts, RdpConfig, RdpError, Security, ServerCertificate,
    Timeouts, connect, connect_over,
};
use ironrdp::pdu::nego::{ConnectionConfirm, ResponseFlags, SecurityProtocol};
use ironrdp::pdu::x224::X224;
use tokio::io::{AsyncRead, AsyncReadExt as _, AsyncWrite, AsyncWriteExt as _};
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;
use tokio_rustls::rustls::crypto::ring::default_provider;
use tokio_rustls::rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use tokio_rustls::rustls::server::{ClientHello, ResolvesServerCert};
use tokio_rustls::rustls::sign::CertifiedKey;
use tokio_rustls::rustls::version::{TLS12, TLS13};
use tokio_rustls::rustls::{ServerConfig, SupportedProtocolVersion};
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;

const HOST: &str = "rdp.test";

/// `requestedProtocols` flags of MS-RDPBCGR 2.2.1.1.1.
const PROTOCOL_SSL: u32 = 0x1;
const PROTOCOL_HYBRID: u32 = 0x2;
const PORT: u16 = 3389;
const USERNAME: &str = "alice-the-user";

const CERT: &[u8] = include_bytes!("fixtures/server-cert.der");
const KEY: &[u8] = include_bytes!("fixtures/server-key.der");
const OTHER_KEY: &[u8] = include_bytes!("fixtures/other-key.der");
/// SHA-256 of the fixture's `SubjectPublicKeyInfo`, computed by openssl, not by this crate.
const EXPECTED_PIN: &str = include_str!("fixtures/server-spki-sha256.txt");

/// Bound on waiting for what the client does next.
const WAIT: Duration = Duration::from_secs(10);

/// Serves one certificate with whatever key it is given: the real one, or another key, as a
/// replayed certificate would be.
#[derive(Debug)]
struct Presents(Arc<CertifiedKey>);

impl ResolvesServerCert for Presents {
    fn resolve(&self, _hello: ClientHello<'_>) -> Option<Arc<CertifiedKey>> {
        Some(Arc::clone(&self.0))
    }
}

fn acceptor(key: &[u8], tls12_only: bool) -> TlsAcceptor {
    let provider = Arc::new(default_provider());
    let signing = provider
        .key_provider
        .load_private_key(PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key.to_vec())))
        .expect("key");
    let certified = CertifiedKey::new(vec![CertificateDer::from(CERT.to_vec())], signing);
    let versions: &[&SupportedProtocolVersion] = if tls12_only {
        &[&TLS12]
    } else {
        &[&TLS12, &TLS13]
    };
    let config = ServerConfig::builder_with_provider(provider)
        .with_protocol_versions(versions)
        .expect("versions")
        .with_no_client_auth()
        .with_cert_resolver(Arc::new(Presents(Arc::new(certified))));
    TlsAcceptor::from(Arc::new(config))
}

/// What the fake server saw.
struct Seen {
    /// Whether the client asked for the credentials.
    asked: bool,
    /// The X.224 Connection Request, as sent.
    request: Vec<u8>,
    /// Whether the TLS handshake completed.
    handshake: bool,
    /// Bytes received after the handshake, until the client closed or the wait ended.
    after_handshake: usize,
}

async fn serve<S: AsyncRead + AsyncWrite + Unpin>(
    mut stream: S,
    key: &'static [u8],
    tls12_only: bool,
) -> Seen {
    let mut header = [0; 4];
    stream.read_exact(&mut header).await.expect("TPKT header");
    let length = usize::from(u16::from_be_bytes([header[2], header[3]]));
    let mut request = header.to_vec();
    request.resize(length, 0);
    stream
        .read_exact(&mut request[4..])
        .await
        .expect("X.224 request");
    let confirm = ironrdp_core::encode_vec(&X224(ConnectionConfirm::Response {
        flags: ResponseFlags::empty(),
        protocol: SecurityProtocol::HYBRID,
    }))
    .expect("encode");
    stream.write_all(&confirm).await.expect("confirm");
    let Ok(mut tls) = acceptor(key, tls12_only).accept(stream).await else {
        return Seen {
            asked: false,
            request,
            handshake: false,
            after_handshake: 0,
        };
    };
    // The client's first CredSSP message is enough to know the gate was passed.
    let mut buffer = [0; 4096];
    let after_handshake = match tokio::time::timeout(WAIT, tls.read(&mut buffer)).await {
        Ok(Ok(read)) => read,
        _ => 0,
    };
    Seen {
        asked: false,
        request,
        handshake: true,
        after_handshake,
    }
}

fn config(known_hosts: &Path, accepted: Option<Fingerprint>, port: u16) -> RdpConfig {
    RdpConfig {
        host: HOST.to_owned(),
        port,
        domain: None,
        desktop: (1024, 768),
        keyboard_layout: 0,
        security: Security::Nla,
        known_hosts: KnownRdpHosts::new(known_hosts),
        accepted,
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
        // Negotiate: with no KDC to be found, NTLM still sends the first message.
        kerberos: true,
    }
}

/// Connects to a fake server presenting the fixture certificate with `key`.
async fn attempt(config: &RdpConfig, key: &'static [u8]) -> (Result<(), RdpError>, Seen) {
    attempt_with(config, key, false).await
}

async fn attempt_with(
    config: &RdpConfig,
    key: &'static [u8],
    tls12_only: bool,
) -> (Result<(), RdpError>, Seen) {
    let (client, server) = tokio::io::duplex(1 << 16);
    let server = tokio::spawn(serve(server, key, tls12_only));
    let asked = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&asked);
    let credentials: heimdall_rdp::AskCredentials = Box::new(move || {
        flag.store(true, Ordering::SeqCst);
        Box::pin(std::future::ready(Some((
            USERNAME.to_owned(),
            Zeroizing::new("hunter2-password".to_owned()),
        ))))
    });
    let outcome = tokio::time::timeout(
        WAIT * 3,
        connect_over(
            Box::new(client),
            "127.0.0.1:50000".parse().expect("address"),
            config,
            credentials,
            &CancellationToken::new(),
        ),
    )
    .await
    .expect("in time")
    .map(|_| ());
    let mut seen = server.await.expect("server");
    seen.asked = asked.load(Ordering::SeqCst);
    (outcome, seen)
}

fn expected_pin() -> Fingerprint {
    format!("SHA256:{}", EXPECTED_PIN.trim())
        .parse()
        .expect("fixture pin")
}

fn lines(path: &Path) -> Vec<String> {
    std::fs::read_to_string(path)
        .map(|text| text.lines().map(str::to_owned).collect())
        .unwrap_or_default()
}

#[test]
fn the_pin_is_the_spki_hash_openssl_computes() {
    let certificate = ServerCertificate::from_der(CERT).expect("fixture");
    assert_eq!(certificate.fingerprint, expected_pin());
    assert_eq!(certificate.subject, "CN=rdp.test");
}

#[tokio::test]
async fn an_unknown_server_is_asked_about_and_gets_nothing() {
    let dir = tempfile::tempdir().expect("dir");
    let known = dir.path().join("known_rdp_hosts");
    let (outcome, seen) = attempt(&config(&known, None, PORT), KEY).await;
    let Err(RdpError::UnknownCertificate(certificate)) = outcome else {
        panic!("{outcome:?}");
    };
    assert_eq!(certificate.fingerprint, expected_pin());
    // The client may close before the server sees its last handshake message: either way,
    // nothing past the handshake reached it.
    assert_eq!(seen.after_handshake, 0, "nothing sent past the handshake");
    assert!(
        !seen.asked,
        "no password typed for a server not yet trusted"
    );
    assert!(
        lines(&known).is_empty(),
        "nothing recorded without a decision"
    );
    // The user name never travels in clear before TLS.
    let request = String::from_utf8_lossy(&seen.request);
    assert!(!request.contains(USERNAME), "{request}");
    assert!(request.contains("mstshash=heimdall"), "{request}");
    // RDP_NEG_REQ ends the request: its requestedProtocols must not offer plain TLS.
    let tail: [u8; 4] = seen.request[seen.request.len() - 4..]
        .try_into()
        .expect("four bytes");
    let offered = u32::from_le_bytes(tail);
    assert_eq!(offered & PROTOCOL_SSL, 0, "plain TLS offered: {offered:#x}");
    assert_ne!(offered & PROTOCOL_HYBRID, 0, "NLA offered: {offered:#x}");
}

#[tokio::test]
async fn an_accepted_key_is_recorded_once_and_then_known() {
    let dir = tempfile::tempdir().expect("dir");
    let known = dir.path().join("sub").join("known_rdp_hosts");
    let (outcome, seen) = attempt(&config(&known, Some(expected_pin()), PORT), KEY).await;
    assert!(
        !matches!(
            outcome,
            Err(RdpError::UnknownCertificate(_) | RdpError::CertificateChanged { .. })
        ),
        "{outcome:?}"
    );
    assert!(seen.after_handshake > 0, "CredSSP started");
    assert!(
        seen.asked,
        "the credentials come once the server is trusted"
    );
    assert_eq!(lines(&known), [format!("{HOST}:{PORT} {}", expected_pin())]);

    // Known now: no decision needed, and nothing is recorded twice.
    let (outcome, seen) = attempt(&config(&known, None, PORT), KEY).await;
    assert!(
        !matches!(outcome, Err(RdpError::UnknownCertificate(_))),
        "{outcome:?}"
    );
    assert!(seen.after_handshake > 0);
    assert_eq!(lines(&known).len(), 1);
}

#[tokio::test]
async fn a_key_trusted_for_the_run_is_let_through_and_never_recorded() {
    let dir = tempfile::tempdir().expect("dir");
    let known = dir.path().join("known_rdp_hosts");
    let mut trusting = config(&known, None, PORT);
    trusting.trusted_for_run = vec![expected_pin()];
    let (outcome, seen) = attempt(&trusting, KEY).await;
    assert!(
        !matches!(
            outcome,
            Err(RdpError::UnknownCertificate(_) | RdpError::CertificateChanged { .. })
        ),
        "{outcome:?}"
    );
    assert!(seen.after_handshake > 0, "CredSSP started");
    assert!(lines(&known).is_empty(), "nothing recorded");

    // Another key trusted for the run is no trust in this one.
    trusting.trusted_for_run = vec![
        "SHA256:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"
            .parse()
            .expect("fingerprint"),
    ];
    let (outcome, seen) = attempt(&trusting, KEY).await;
    assert!(
        matches!(outcome, Err(RdpError::UnknownCertificate(_))),
        "{outcome:?}"
    );
    assert_eq!(seen.after_handshake, 0);
}

#[tokio::test]
async fn a_changed_key_is_refused_without_a_question() {
    let dir = tempfile::tempdir().expect("dir");
    let known = dir.path().join("known_rdp_hosts");
    let other: Fingerprint = "SHA256:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"
        .parse()
        .expect("fingerprint");
    std::fs::write(&known, format!("{HOST}:{PORT} {other}\n")).expect("known");
    // Even an acceptance in hand does not override a recorded key.
    let (outcome, seen) = attempt(&config(&known, Some(expected_pin()), PORT), KEY).await;
    let Err(RdpError::CertificateChanged {
        recorded,
        presented,
    }) = outcome
    else {
        panic!("{outcome:?}");
    };
    assert_eq!(recorded, other);
    assert_eq!(presented.fingerprint, expected_pin());
    assert_eq!(seen.after_handshake, 0);
    assert!(!seen.asked);
    assert_eq!(lines(&known).len(), 1, "the recorded key stays");
}

#[tokio::test]
async fn at_an_address_several_servers_answer_a_new_key_is_asked_about_and_trusted_beside() {
    let dir = tempfile::tempdir().expect("dir");
    let known = dir.path().join("known_rdp_hosts");
    let other: Fingerprint = "SHA256:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"
        .parse()
        .expect("fingerprint");
    std::fs::write(&known, format!("{HOST}:{PORT} {other}\n")).expect("known");
    let pool = |accepted| RdpConfig {
        several_servers: true,
        ..config(&known, accepted, PORT)
    };

    // Asked about as a first contact, nothing sent and nothing recorded.
    let (outcome, seen) = attempt(&pool(None), KEY).await;
    let Err(RdpError::UnknownCertificate(certificate)) = outcome else {
        panic!("{outcome:?}");
    };
    assert_eq!(certificate.fingerprint, expected_pin());
    assert_eq!(seen.after_handshake, 0);
    assert!(!seen.asked);
    assert_eq!(lines(&known).len(), 1);

    // Accepted: trusted beside the other machine's key, which stays.
    let (outcome, seen) = attempt(&pool(Some(expected_pin())), KEY).await;
    assert!(
        !matches!(
            outcome,
            Err(RdpError::UnknownCertificate(_) | RdpError::CertificateChanged { .. })
        ),
        "{outcome:?}"
    );
    assert!(
        seen.asked,
        "the credentials come once the server is trusted"
    );
    assert_eq!(
        lines(&known),
        [
            format!("{HOST}:{PORT} {other}"),
            format!("{HOST}:{PORT} {}", expected_pin())
        ]
    );
}

#[tokio::test]
async fn a_key_accepted_for_another_certificate_is_refused() {
    let dir = tempfile::tempdir().expect("dir");
    let known = dir.path().join("known_rdp_hosts");
    let accepted: Fingerprint = "SHA256:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"
        .parse()
        .expect("fingerprint");
    let (outcome, seen) = attempt(&config(&known, Some(accepted), PORT), KEY).await;
    assert!(
        matches!(outcome, Err(RdpError::CertificateChanged { .. })),
        "{outcome:?}"
    );
    assert_eq!(seen.after_handshake, 0);
    assert!(lines(&known).is_empty());
}

#[tokio::test]
async fn a_replayed_certificate_without_its_key_fails_the_handshake() {
    let dir = tempfile::tempdir().expect("dir");
    let known = dir.path().join("known_rdp_hosts");
    std::fs::write(&known, format!("{HOST}:{PORT} {}\n", expected_pin())).expect("known");
    // The pinned certificate, served with another key: the pin would match.
    let (outcome, seen) = attempt(&config(&known, None, PORT), OTHER_KEY).await;
    assert!(matches!(outcome, Err(RdpError::Tls(_))), "{outcome:?}");
    assert!(!seen.handshake);
    assert_eq!(seen.after_handshake, 0);
}

#[tokio::test]
async fn a_pin_belongs_to_one_port() {
    let dir = tempfile::tempdir().expect("dir");
    let known = dir.path().join("known_rdp_hosts");
    std::fs::write(&known, format!("{HOST}:{PORT} {}\n", expected_pin())).expect("known");
    let (outcome, seen) = attempt(&config(&known, None, PORT + 1), KEY).await;
    assert!(
        matches!(outcome, Err(RdpError::UnknownCertificate(_))),
        "{outcome:?}"
    );
    assert_eq!(seen.after_handshake, 0);
}

#[tokio::test]
async fn a_replayed_certificate_fails_over_tls_1_2_too() {
    let dir = tempfile::tempdir().expect("dir");
    let known = dir.path().join("known_rdp_hosts");
    std::fs::write(&known, format!("{HOST}:{PORT} {}\n", expected_pin())).expect("known");
    let (outcome, seen) = attempt_with(&config(&known, None, PORT), OTHER_KEY, true).await;
    assert!(matches!(outcome, Err(RdpError::Tls(_))), "{outcome:?}");
    assert_eq!(seen.after_handshake, 0);
}

#[tokio::test]
async fn tls_1_2_with_the_real_key_passes() {
    let dir = tempfile::tempdir().expect("dir");
    let known = dir.path().join("known_rdp_hosts");
    std::fs::write(&known, format!("{HOST}:{PORT} {}\n", expected_pin())).expect("known");
    let (outcome, seen) = attempt_with(&config(&known, None, PORT), KEY, true).await;
    assert!(!matches!(outcome, Err(RdpError::Tls(_))), "{outcome:?}");
    assert!(seen.after_handshake > 0, "CredSSP started over TLS 1.2");
}

/// Compiles only if a connection can run in a spawned task, as the application runs it.
#[allow(dead_code, reason = "a compile-time check, never called")]
fn connecting_can_be_spawned(config: RdpConfig, password: Zeroizing<String>) {
    drop(tokio::spawn(heimdall_rdp::connect(
        config,
        heimdall_rdp::given(USERNAME.to_owned(), password),
        CancellationToken::new(),
    )));
}

/// Connects through [`connect`], over TCP to a fake server on this machine, with the key
/// recorded for it first when `recorded`, and `trusted_for_run` as the keys trusted for this
/// run. The outcome, and when the credentials were asked:
/// `Some(true)` once the server had accepted the connection, `Some(false)` before, `None`
/// never.
async fn attempt_over_tcp(
    known: &Path,
    accepted: Option<Fingerprint>,
    recorded: bool,
    trusted_for_run: Vec<Fingerprint>,
) -> (Result<(), RdpError>, Option<bool>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let port = listener.local_addr().expect("address").port();
    if recorded {
        std::fs::write(known, format!("127.0.0.1:{port} {}\n", expected_pin())).expect("known");
    }
    let connected = Arc::new(AtomicBool::new(false));
    let server_side = Arc::clone(&connected);
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accepted");
        server_side.store(true, Ordering::SeqCst);
        serve(stream, KEY, false).await
    });
    let asked = Arc::new(Mutex::new(None));
    let record = Arc::clone(&asked);
    let credentials: AskCredentials = Box::new(move || {
        *record.lock().expect("lock") = Some(connected.load(Ordering::SeqCst));
        Box::pin(std::future::ready(Some((
            USERNAME.to_owned(),
            Zeroizing::new("hunter2-password".to_owned()),
        ))))
    });
    let mut config = config(known, accepted, port);
    "127.0.0.1".clone_into(&mut config.host);
    config.trusted_for_run = trusted_for_run;
    let outcome = tokio::time::timeout(
        WAIT * 3,
        connect(config, credentials, CancellationToken::new()),
    )
    .await
    .expect("in time")
    .map(|_| ());
    server.abort();
    let asked = *asked.lock().expect("lock");
    (outcome, asked)
}

#[tokio::test]
async fn a_recorded_server_gets_the_credentials_before_the_connection_opens() {
    let dir = tempfile::tempdir().expect("dir");
    let (outcome, asked) =
        attempt_over_tcp(&dir.path().join("known"), None, true, Vec::new()).await;
    assert!(
        !matches!(
            outcome,
            Err(RdpError::UnknownCertificate(_) | RdpError::CertificateChanged { .. })
        ),
        "{outcome:?}"
    );
    // A server waits for them only so long; a person typing may take longer.
    assert_eq!(
        asked,
        Some(false),
        "asked before the server accepted anything"
    );
}

#[tokio::test]
async fn a_just_accepted_server_gets_the_credentials_before_the_connection_opens() {
    let dir = tempfile::tempdir().expect("dir");
    let (outcome, asked) = attempt_over_tcp(
        &dir.path().join("known"),
        Some(expected_pin()),
        false,
        Vec::new(),
    )
    .await;
    assert!(
        !matches!(
            outcome,
            Err(RdpError::UnknownCertificate(_) | RdpError::CertificateChanged { .. })
        ),
        "{outcome:?}"
    );
    assert_eq!(
        asked,
        Some(false),
        "asked before the server accepted anything"
    );
}

#[tokio::test]
async fn a_server_trusted_for_the_run_gets_the_credentials_before_the_connection_opens() {
    let dir = tempfile::tempdir().expect("dir");
    let (outcome, asked) =
        attempt_over_tcp(&dir.path().join("known"), None, false, vec![expected_pin()]).await;
    assert!(
        !matches!(
            outcome,
            Err(RdpError::UnknownCertificate(_) | RdpError::CertificateChanged { .. })
        ),
        "{outcome:?}"
    );
    assert_eq!(
        asked,
        Some(false),
        "asked before the server accepted anything"
    );
}

#[tokio::test]
async fn an_unknown_server_over_tcp_is_asked_about_and_asks_nothing() {
    let dir = tempfile::tempdir().expect("dir");
    let (outcome, asked) =
        attempt_over_tcp(&dir.path().join("known"), None, false, Vec::new()).await;
    assert!(
        matches!(outcome, Err(RdpError::UnknownCertificate(_))),
        "{outcome:?}"
    );
    assert_eq!(
        asked, None,
        "no credentials for a server whose identity is open"
    );
}
