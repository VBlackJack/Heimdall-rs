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
    AcceptedCertificate, AskCredentials, CertificateHash, Fingerprint, KnownRdpHosts, RdpConfig,
    RdpError, Security, ServerCertificate, Timeouts, Validity, connect, connect_over,
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
/// Another certificate on [`KEY`], as a renewal makes one: `CN=rdp.test`, signed by [`KEY`],
/// made by openssl (`req -x509` on the fixture key, other dates).
const RENEWED: &[u8] = include_bytes!("fixtures/renewed-cert.der");
/// A certificate carrying the public key of [`KEY`] but minted by whoever holds
/// [`OTHER_KEY`], as someone without the server's key could mint one: made by openssl
/// (`x509 -new -force_pubkey`).
const FORGED: &[u8] = include_bytes!("fixtures/forged-cert.der");
/// SHA-256 of the fixture's `SubjectPublicKeyInfo`, computed by openssl, not by this crate.
const EXPECTED_PIN: &str = include_str!("fixtures/server-spki-sha256.txt");

/// Bound on waiting for what the client does next.
const WAIT: Duration = Duration::from_secs(10);

/// Serves one certificate with whatever key it is given: the real one, or another key, as a
/// replayed or forged certificate would be.
#[derive(Debug)]
struct Presents(Arc<CertifiedKey>);

impl ResolvesServerCert for Presents {
    fn resolve(&self, _hello: ClientHello<'_>) -> Option<Arc<CertifiedKey>> {
        Some(Arc::clone(&self.0))
    }
}

fn acceptor(certificate: &[u8], key: &[u8], tls12_only: bool) -> TlsAcceptor {
    let provider = Arc::new(default_provider());
    let signing = provider
        .key_provider
        .load_private_key(PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key.to_vec())))
        .expect("key");
    let certified = CertifiedKey::new(vec![CertificateDer::from(certificate.to_vec())], signing);
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
    (certificate, key): (&'static [u8], &'static [u8]),
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
    let Ok(mut tls) = acceptor(certificate, key, tls12_only).accept(stream).await else {
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

fn config(known_hosts: &Path, accepted: Option<AcceptedCertificate>, port: u16) -> RdpConfig {
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
        strict_server_authentication: false,
        // Negotiate: with no KDC to be found, NTLM still sends the first message.
        kerberos: true,
        time_zone: None,
        desktop_scale: 100,
        progress: None,
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
    attempt_presenting(config, (CERT, key), tls12_only).await
}

/// Connects to a fake server presenting `certificate` with `key`.
async fn attempt_presenting(
    config: &RdpConfig,
    (certificate, key): (&'static [u8], &'static [u8]),
    tls12_only: bool,
) -> (Result<(), RdpError>, Seen) {
    let (client, server) = tokio::io::duplex(1 << 16);
    let server = tokio::spawn(serve(server, (certificate, key), tls12_only));
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

/// The DER certificate `der`, read.
fn read(der: &[u8]) -> ServerCertificate {
    ServerCertificate::from_der(der).expect("fixture")
}

/// The answer "Trust this certificate" to the question about `der`.
fn accepting(der: &[u8]) -> AcceptedCertificate {
    AcceptedCertificate::from(&read(der))
}

/// The fixture certificate, accepted.
fn accepted() -> AcceptedCertificate {
    accepting(CERT)
}

/// What `path` records of the whole certificates trusted, in the order of the file.
fn whole_certificates(path: &Path) -> Vec<Option<CertificateHash>> {
    KnownRdpHosts::new(path)
        .entries()
        .expect("read")
        .into_iter()
        .map(|entry| entry.certificate)
        .collect()
}

/// The address and key of each line, the attributes after them left out.
fn pins(path: &Path) -> Vec<String> {
    lines(path)
        .iter()
        .map(|line| {
            line.split_whitespace()
                .take(2)
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect()
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
    assert_eq!(certificate.issuer, "CN=rdp.test", "signed by itself");
}

#[test]
fn the_issuer_is_read_apart_from_the_subject() {
    // Issued by `O=Heimdall Lab, CN=Lab Root CA` to `CN=dc.lab`, made by openssl.
    let issued =
        ServerCertificate::from_der(include_bytes!("fixtures/issued-cert.der")).expect("fixture");
    assert_eq!(issued.subject, "CN=dc.lab");
    assert_eq!(issued.issuer, "CN=Lab Root CA,O=Heimdall Lab");
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
    let (outcome, seen) = attempt(&config(&known, Some(accepted()), PORT), KEY).await;
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
    assert_eq!(pins(&known), [format!("{HOST}:{PORT} {}", expected_pin())]);
    let recorded = KnownRdpHosts::new(&known).entries().expect("read");
    assert_eq!(
        (recorded[0].certificate, recorded[0].validity),
        (
            Some(CertificateHash::of(CERT)),
            Some(Validity::from_der(CERT).expect("fixture"))
        ),
        "pinned by the whole certificate, its validity beside it"
    );
    assert_eq!(
        (
            recorded[0].subject.as_deref(),
            recorded[0].issuer.as_deref()
        ),
        (Some("CN=rdp.test"), Some("CN=rdp.test")),
        "recorded with the names of its certificate"
    );
    assert!(recorded[0].trusted.is_some(), "and the time");

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
async fn a_certificate_trusted_for_the_run_is_let_through_and_never_recorded() {
    let dir = tempfile::tempdir().expect("dir");
    let known = dir.path().join("known_rdp_hosts");
    let mut trusting = config(&known, None, PORT);
    trusting.trusted_for_run = vec![CertificateHash::of(CERT)];
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

    // Another certificate trusted for the run, on the same key, is no trust in this one.
    trusting.trusted_for_run = vec![CertificateHash::of(RENEWED)];
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
    let (outcome, seen) = attempt(&config(&known, Some(accepted()), PORT), KEY).await;
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
        strict_server_authentication: false,
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
    let (outcome, seen) = attempt(&pool(Some(accepted())), KEY).await;
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
        pins(&known),
        [
            format!("{HOST}:{PORT} {other}"),
            format!("{HOST}:{PORT} {}", expected_pin())
        ]
    );
}

#[tokio::test]
async fn a_certificate_accepted_on_another_key_is_refused() {
    let dir = tempfile::tempdir().expect("dir");
    let known = dir.path().join("known_rdp_hosts");
    let accepted = AcceptedCertificate {
        key: "SHA256:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"
            .parse()
            .expect("fingerprint"),
        certificate: CertificateHash::of(b"another certificate"),
    };
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
/// recorded for it first when `recorded`, and `trusted_for_run` as the certificates trusted
/// for this run. The outcome, and when the credentials were asked:
/// `Some(true)` once the server had accepted the connection, `Some(false)` before, `None`
/// never.
async fn attempt_over_tcp(
    known: &Path,
    accepted: Option<AcceptedCertificate>,
    recorded: bool,
    trusted_for_run: Vec<CertificateHash>,
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
        serve(stream, (CERT, KEY), false).await
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
        Some(accepted()),
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
    let (outcome, asked) = attempt_over_tcp(
        &dir.path().join("known"),
        None,
        false,
        vec![CertificateHash::of(CERT)],
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

#[tokio::test]
async fn under_strict_authentication_a_server_no_authority_vouches_for_is_refused_unasked() {
    let dir = tempfile::tempdir().expect("dir");
    let known = dir.path().join("known_rdp_hosts");
    let strict = RdpConfig {
        strict_server_authentication: true,
        ..config(&known, None, PORT)
    };
    // A self-signed certificate: no certificate authority of this computer validates it.
    let (outcome, seen) = attempt(&strict, KEY).await;
    assert!(
        matches!(outcome, Err(RdpError::ServerNotAuthenticated)),
        "{outcome:?}"
    );
    assert!(!seen.asked, "no password for a server not authenticated");
    assert_eq!(seen.after_handshake, 0, "nothing sent past the handshake");
    assert!(lines(&known).is_empty(), "never recorded");

    // Trusted before: pinned, it goes through, as the C# keeps a profile's trust.
    let accepted = config(&known, Some(accepted()), PORT);
    let _ = attempt(&accepted, KEY).await;
    let strict = RdpConfig {
        strict_server_authentication: true,
        ..config(&known, None, PORT)
    };
    let (outcome, seen) = attempt(&strict, KEY).await;
    assert!(
        !matches!(
            outcome,
            Err(RdpError::ServerNotAuthenticated | RdpError::UnknownCertificate(_))
        ),
        "{outcome:?}"
    );
    assert!(seen.asked, "the credentials come once the pin matches");
}

/// Every line logged by this test binary, as `LEVEL message`.
static LOGGED: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Keeps every log line in [`LOGGED`].
struct Capture;

impl log::Log for Capture {
    fn enabled(&self, _metadata: &log::Metadata<'_>) -> bool {
        true
    }

    fn log(&self, record: &log::Record<'_>) {
        if let Ok(mut lines) = LOGGED.lock() {
            lines.push(format!("{} {}", record.level(), record.args()));
        }
    }

    fn flush(&self) {}
}

static CAPTURE: Capture = Capture;

/// Starts keeping the log lines, once for the binary.
fn capture_log() {
    static INSTALLED: std::sync::Once = std::sync::Once::new();
    INSTALLED.call_once(|| {
        let _ = log::set_logger(&CAPTURE);
        log::set_max_level(log::LevelFilter::Info);
    });
}

/// The lines logged so far that name `needle`.
fn logged_with(needle: &str) -> Vec<String> {
    LOGGED
        .lock()
        .map(|lines| {
            lines
                .iter()
                .filter(|line| line.contains(needle))
                .cloned()
                .collect()
        })
        .unwrap_or_default()
}

#[tokio::test]
async fn each_certificate_decision_is_in_the_diagnostics_log_and_never_the_password() {
    /// A port no other test of this file uses, to find this test's lines.
    const LOG_PORT: u16 = 4389;
    capture_log();
    let target = format!("{HOST}:{LOG_PORT}");
    let dir = tempfile::tempdir().expect("dir");
    let known = dir.path().join("known_rdp_hosts");

    let (outcome, _) = attempt(&config(&known, Some(accepted()), LOG_PORT), KEY).await;
    assert!(
        !matches!(outcome, Err(RdpError::UnknownCertificate(_))),
        "{outcome:?}"
    );
    let (outcome, _) = attempt(&config(&known, None, LOG_PORT), KEY).await;
    assert!(
        !matches!(outcome, Err(RdpError::UnknownCertificate(_))),
        "{outcome:?}"
    );
    let other: Fingerprint = "SHA256:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"
        .parse()
        .expect("fingerprint");
    std::fs::write(&known, format!("{target} {other}\n")).expect("known");
    let (outcome, _) = attempt(&config(&known, None, LOG_PORT), KEY).await;
    assert!(
        matches!(outcome, Err(RdpError::CertificateChanged { .. })),
        "{outcome:?}"
    );

    let lines = logged_with(&target);
    let pin = expected_pin().to_string();
    assert!(
        lines.iter().any(|line| line.starts_with("INFO")
            && line.contains("is recorded")
            && line.contains(&pin)),
        "{lines:?}"
    );
    assert!(
        lines.iter().any(|line| line.starts_with("INFO")
            && line.contains("the one trusted")
            && line.contains(&pin)),
        "{lines:?}"
    );
    assert!(
        lines.iter().any(|line| line.starts_with("WARN")
            && line.contains("changed")
            && line.contains(&other.to_string())
            && line.contains(&pin)),
        "{lines:?}"
    );
    assert!(
        logged_with("hunter2-password").is_empty(),
        "the password is never logged"
    );
}

/// Pins `der` whole for [`HOST`]:[`PORT`] in `known`, as accepting it after the question does.
fn pin_whole(known: &Path, der: &[u8]) {
    let certificate = read(der);
    KnownRdpHosts::new(known)
        .record_whole_certificate(
            HOST,
            PORT,
            &certificate,
            (&certificate.certificate, Some(&certificate.validity)),
        )
        .expect("pinned");
}

/// Whether `outcome` let the credentials go: the pin passed.
fn passed(outcome: &Result<(), RdpError>) -> bool {
    !matches!(
        outcome,
        Err(RdpError::UnknownCertificate(_)
            | RdpError::RenewedCertificate { .. }
            | RdpError::CertificateChanged { .. }
            | RdpError::Tls(_))
    )
}

#[tokio::test]
async fn the_whole_certificate_trusted_is_known_and_another_on_its_key_is_a_renewal_asked_about() {
    let dir = tempfile::tempdir().expect("dir");
    let known = dir.path().join("known_rdp_hosts");
    pin_whole(&known, CERT);
    let before = std::fs::read(&known).expect("pinned");

    // The very certificate: through, as before.
    let (outcome, seen) = attempt(&config(&known, None, PORT), KEY).await;
    assert!(passed(&outcome), "{outcome:?}");
    assert!(seen.asked && seen.after_handshake > 0);

    // Renewed on the same key, signed by it: asked about, told it is a renewal, with when the
    // certificate on record holds; never taken silently, nothing sent, nothing written.
    let (outcome, seen) =
        attempt_presenting(&config(&known, None, PORT), (RENEWED, KEY), false).await;
    let Err(RdpError::RenewedCertificate {
        presented,
        recorded,
    }) = outcome
    else {
        panic!("{outcome:?}");
    };
    assert_eq!(
        (presented.fingerprint, presented.certificate),
        (expected_pin(), CertificateHash::of(RENEWED))
    );
    assert_eq!(
        presented.validity,
        Validity::from_der(RENEWED).expect("fixture")
    );
    assert_eq!(recorded, Some(Validity::from_der(CERT).expect("fixture")));
    assert_ne!(
        presented.validity,
        Validity::from_der(CERT).expect("fixture")
    );
    assert!(!seen.asked, "no password before the user decides");
    assert_eq!(seen.after_handshake, 0, "nothing sent past the handshake");
    assert_eq!(
        std::fs::read(&known).expect("file"),
        before,
        "nothing written"
    );
}

#[tokio::test]
async fn a_renewal_accepted_replaces_the_certificate_on_record_on_that_key() {
    let dir = tempfile::tempdir().expect("dir");
    let known = dir.path().join("known_rdp_hosts");
    pin_whole(&known, CERT);
    let (outcome, seen) = attempt_presenting(
        &config(&known, Some(accepting(RENEWED)), PORT),
        (RENEWED, KEY),
        false,
    )
    .await;
    assert!(passed(&outcome), "{outcome:?}");
    assert!(seen.asked, "the credentials come once the user accepted");
    assert_eq!(
        whole_certificates(&known),
        [Some(CertificateHash::of(RENEWED))],
        "one certificate on the key: the renewed one"
    );

    // The certificate it replaced is another certificate on the key now.
    let (outcome, _) = attempt(&config(&known, None, PORT), KEY).await;
    assert!(
        matches!(outcome, Err(RdpError::RenewedCertificate { .. })),
        "{outcome:?}"
    );
}

#[tokio::test]
async fn a_renewal_trusted_for_the_run_is_let_through_and_never_recorded() {
    let dir = tempfile::tempdir().expect("dir");
    let known = dir.path().join("known_rdp_hosts");
    pin_whole(&known, CERT);
    let before = std::fs::read(&known).expect("pinned");
    let mut once = config(&known, None, PORT);
    once.trusted_for_run = vec![CertificateHash::of(RENEWED)];
    let (outcome, seen) = attempt_presenting(&once, (RENEWED, KEY), false).await;
    assert!(passed(&outcome), "{outcome:?}");
    assert!(seen.after_handshake > 0, "CredSSP started");
    assert_eq!(
        std::fs::read(&known).expect("file"),
        before,
        "nothing written"
    );
}

#[tokio::test]
async fn a_certificate_accepted_trusts_that_certificate_and_no_other_on_its_key() {
    let dir = tempfile::tempdir().expect("dir");
    let known = dir.path().join("known_rdp_hosts");
    // Accepted after the question about the renewed certificate; the server now presents
    // another one on the same key: the question again, nothing recorded, nothing sent.
    let (outcome, seen) = attempt(&config(&known, Some(accepting(RENEWED)), PORT), KEY).await;
    assert!(
        matches!(&outcome, Err(RdpError::UnknownCertificate(certificate)) if certificate.certificate == CertificateHash::of(CERT)),
        "{outcome:?}"
    );
    assert!(!seen.asked);
    assert_eq!(seen.after_handshake, 0);
    assert!(lines(&known).is_empty(), "nothing recorded");
}

#[tokio::test]
async fn a_key_recorded_alone_lets_its_certificate_through_and_adopts_nothing_before_the_connection_is_up()
 {
    let dir = tempfile::tempdir().expect("dir");
    let known = dir.path().join("known_rdp_hosts");
    // As a Heimdall that pinned keys alone wrote it.
    let legacy = format!("{HOST}:{PORT} {} trusted=1767225600\r\n", expected_pin());
    std::fs::write(&known, &legacy).expect("known");
    let (outcome, seen) = attempt(&config(&known, None, PORT), KEY).await;
    assert!(passed(&outcome), "{outcome:?}");
    assert!(
        seen.asked && seen.after_handshake > 0,
        "checked on its key, as before"
    );
    // The fake server never finishes the connection sequence: nothing adopted.
    assert!(outcome.is_err(), "the connection never came up");
    assert_eq!(
        std::fs::read_to_string(&known).expect("file"),
        legacy,
        "the line kept as it was, byte for byte"
    );
}

#[tokio::test]
async fn a_certificate_minted_on_the_key_by_someone_without_it_is_never_accepted_nor_adopted() {
    let dir = tempfile::tempdir().expect("dir");
    let known = dir.path().join("known_rdp_hosts");
    assert_eq!(read(FORGED).fingerprint, expected_pin(), "the server's key");
    // Pinned by the key alone, adoptable; pinned whole; or accepted after the question: the
    // handshake, signed with another key than the certificate's, fails first every time.
    let legacy = format!("{HOST}:{PORT} {}\n", expected_pin());
    std::fs::write(&known, &legacy).expect("known");
    for tls12_only in [false, true] {
        let (outcome, seen) =
            attempt_presenting(&config(&known, None, PORT), (FORGED, OTHER_KEY), tls12_only).await;
        assert!(matches!(outcome, Err(RdpError::Tls(_))), "{outcome:?}");
        assert!(!seen.handshake && !seen.asked);
        assert_eq!(seen.after_handshake, 0);
    }
    assert_eq!(std::fs::read_to_string(&known).expect("file"), legacy);

    std::fs::remove_file(&known).expect("removed");
    pin_whole(&known, CERT);
    let before = std::fs::read(&known).expect("pinned");
    let (outcome, _) = attempt_presenting(
        &config(&known, Some(accepting(FORGED)), PORT),
        (FORGED, OTHER_KEY),
        false,
    )
    .await;
    assert!(matches!(outcome, Err(RdpError::Tls(_))), "{outcome:?}");
    assert_eq!(
        std::fs::read(&known).expect("file"),
        before,
        "never recorded"
    );
}
