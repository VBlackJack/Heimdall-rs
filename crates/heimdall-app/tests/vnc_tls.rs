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

//! A VNC tab against a server encrypting with `VeNCrypt` `X509Vnc` and a certificate no system
//! trusts: the certificate question before any password, the certificate pinned once
//! trusted, then refused when it changes and a downgrade to clear refused; and a certificate
//! trusted that is no longer valid refused, never asked about, until the server is
//! forgotten.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use heimdall_app::vnc_driver::{VncRequest, vnc_events};
use heimdall_app::{
    Answer, AnswerRegistry, App, AppConfig, AttemptId, ConnectionEvent, Effect, Message, Phase,
    QuestionKind, SettingsMessage, TabId, TrustedKeysMessage, UiError,
};
use heimdall_core::profile::{ProfileId, VncProfile};
use heimdall_core::store::ProfileStore;
use heimdall_rdp::{CertificateHash, Fingerprint, KnownRdpHosts, ServerCertificate, Validity};
use heimdall_ssh::{AgentSource, Secret};
use heimdall_term::GridSize;
use tokio::io::{AsyncRead, AsyncReadExt as _, AsyncWrite, AsyncWriteExt as _};
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::TlsAcceptor;
use tokio_rustls::rustls::ServerConfig;
use tokio_rustls::rustls::crypto::ring::default_provider;
use tokio_rustls::rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use tokio_rustls::rustls::server::{ClientHello, ResolvesServerCert};
use tokio_rustls::rustls::sign::CertifiedKey;
use tokio_stream::StreamExt as _;

/// Bound on anything the test waits for.
const WAIT: Duration = Duration::from_secs(10);

const CHALLENGE: [u8; 16] = [
    0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff,
];
/// The response for "Secret12", computed with openssl.
const RESPONSE: [u8; 16] = [
    0xee, 0xe9, 0x81, 0xe2, 0x74, 0x19, 0x66, 0x45, 0xd7, 0x10, 0xe9, 0xd9, 0x1f, 0xf5, 0xf5, 0xe8,
];
/// Client messages between the server's init and the session: pixel format, encodings,
/// the first update request.
const OPENING_REQUESTS: usize = 20 + 44 + 10;

/// `VeNCrypt` and its `X509Vnc` subtype.
const VENCRYPT: u8 = 19;
const X509_VNC: u32 = 261;

/// The host the profile names.
const HOST: &str = "127.0.0.1";

/// A TLS server with a fresh self-signed certificate, and the key it is pinned by.
fn server_tls() -> (TlsAcceptor, Fingerprint) {
    let issued = rcgen::generate_simple_self_signed(vec![HOST.to_owned()]).expect("cert");
    acceptor(
        issued.cert.der().to_vec(),
        issued.signing_key.serialize_der(),
    )
}

/// A TLS server with a fresh self-signed certificate over since 2001, and the key it is
/// pinned by.
fn expired_server_tls() -> (TlsAcceptor, Fingerprint) {
    made_server_tls((2000, 2001), Vec::new(), false).0
}

/// The parameters of a certificate for [`HOST`], valid from the first of January of `from`
/// to that of `until`, for `purposes`, any when none; a certificate authority when
/// `authority`.
fn params(
    (from, until): (i32, i32),
    purposes: Vec<rcgen::ExtendedKeyUsagePurpose>,
    authority: bool,
) -> rcgen::CertificateParams {
    let mut params = rcgen::CertificateParams::new(vec![HOST.to_owned()]).expect("params");
    params.not_before = rcgen::date_time_ymd(from, 1, 1);
    params.not_after = rcgen::date_time_ymd(until, 1, 1);
    params.extended_key_usages = purposes;
    if authority {
        params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
    }
    params
}

/// A TLS server with a fresh self-signed certificate made by [`params`], the key it is
/// pinned by, and its private key (PKCS #8).
fn made_server_tls(
    validity: (i32, i32),
    purposes: Vec<rcgen::ExtendedKeyUsagePurpose>,
    authority: bool,
) -> ((TlsAcceptor, Fingerprint), Vec<u8>) {
    let key = rcgen::KeyPair::generate().expect("key");
    let cert = params(validity, purposes, authority)
        .self_signed(&key)
        .expect("cert");
    (
        acceptor(cert.der().to_vec(), key.serialize_der()),
        key.serialize_der(),
    )
}

/// A forger's TLS server: a certificate around the public key of `victim` (PKCS #8), over
/// since 2001, signed by a key of its own, which signs the handshake too.
fn forged_server_tls(victim: &[u8]) -> TlsAcceptor {
    let victim = rcgen::KeyPair::try_from(victim).expect("victim's key");
    let forger = rcgen::KeyPair::generate().expect("key");
    let mut authority = rcgen::CertificateParams::new(Vec::<String>::new()).expect("params");
    authority.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
    let issuer = rcgen::Issuer::new(authority, rcgen::KeyPair::generate().expect("key"));
    let cert = params((2000, 2001), Vec::new(), false)
        .signed_by(&victim, &issuer)
        .expect("forged");
    acceptor(cert.der().to_vec(), forger.serialize_der()).0
}

/// The server's certificate, whatever the client says: no check that the key is the
/// certificate's, for a forger.
#[derive(Debug)]
struct Fixed(Arc<CertifiedKey>);

impl ResolvesServerCert for Fixed {
    fn resolve(&self, _: ClientHello<'_>) -> Option<Arc<CertifiedKey>> {
        Some(Arc::clone(&self.0))
    }
}

/// A TLS server presenting certificate `der` and signing with private key `key` (PKCS #8),
/// and the key the certificate is pinned by.
fn acceptor(der: Vec<u8>, key: Vec<u8>) -> (TlsAcceptor, Fingerprint) {
    let pin = ServerCertificate::from_der(&der)
        .expect("readable")
        .fingerprint;
    let provider = Arc::new(default_provider());
    let key = provider
        .key_provider
        .load_private_key(PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key)))
        .expect("signing key");
    let certified = CertifiedKey::new(vec![CertificateDer::from(der)], key);
    let config = ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .expect("versions")
        .with_no_client_auth()
        .with_cert_resolver(Arc::new(Fixed(Arc::new(certified))));
    (TlsAcceptor::from(Arc::new(config)), pin)
}

fn app(dir: &Path, port: u16) -> App {
    app_with(dir, port, false)
}

/// The app with the profile asking for TLS, or not.
fn app_with(dir: &Path, port: u16, require_tls: bool) -> App {
    let profiles_file = dir.join("profiles.toml");
    let mut store = ProfileStore::open(&profiles_file).expect("store");
    store.merge_vnc([VncProfile {
        id: ProfileId::new("kiosk"),
        name: "Kiosk".to_owned(),
        group: None,
        host: HOST.to_owned(),
        port,
        view_only: false,
        allow_no_password: false,
        require_tls,
        username: None,
        vault_entry: None,
    }]);
    store.save().expect("save");
    App::new(AppConfig {
        profiles_file,
        known_hosts: dir.join("known_hosts"),
        legacy_dir: None,
        agent: AgentSource::Disabled,
        initial_grid: GridSize { cols: 80, rows: 24 },
        files_start: dir.to_owned(),
        system_credentials: heimdall_app::SystemCredentials::memory(),
    })
}

/// The whole certificate the single question of `events` asks about.
fn asked_whole(events: &[ConnectionEvent]) -> CertificateHash {
    match events {
        [
            ConnectionEvent::UnknownRdpCertificate {
                details: Some(details),
                ..
            },
        ] => details.certificate,
        other => panic!("one question, with its certificate: {other:?}"),
    }
}

/// The renewal the single question of `events` says, if any.
fn asked_renewal(events: &[ConnectionEvent]) -> Option<heimdall_app::Renewal> {
    match events {
        [
            ConnectionEvent::UnknownRdpCertificate {
                details: Some(details),
                ..
            },
        ] => details.renewal,
        other => panic!("one question, with its certificate: {other:?}"),
    }
}

/// The VNC attempt `effects` start: its tab, its attempt and what it needs.
fn attempt_of(effects: &[Effect]) -> (TabId, AttemptId, VncRequest) {
    let [
        Effect::ConnectVnc {
            tab,
            attempt,
            request,
        },
    ] = effects
    else {
        panic!("{effects:?}");
    };
    (*tab, *attempt, (**request).clone())
}

async fn read_exactly<S: AsyncRead + Unpin>(stream: &mut S, count: usize) -> Vec<u8> {
    let mut bytes = vec![0; count];
    tokio::time::timeout(WAIT, stream.read_exact(&mut bytes))
        .await
        .expect("in time")
        .expect("read");
    bytes
}

/// Plays the server up to its security types, offering `types`.
async fn offer_types(stream: &mut TcpStream, types: &[u8]) {
    stream.write_all(b"RFB 003.008\n").await.expect("version");
    let _ = read_exactly(stream, 12).await;
    let mut offer = vec![u8::try_from(types.len()).expect("few")];
    offer.extend_from_slice(types);
    stream.write_all(&offer).await.expect("types");
}

/// Plays a `VeNCrypt` server offering `X509Vnc` up to the start of TLS.
async fn serve_to_tls(stream: &mut TcpStream) {
    offer_types(stream, &[VENCRYPT]).await;
    assert_eq!(read_exactly(stream, 1).await, [VENCRYPT]);
    stream.write_all(&[0, 2]).await.expect("VeNCrypt version");
    let _ = read_exactly(stream, 2).await;
    let mut subtypes = vec![0, 1];
    subtypes.extend_from_slice(&X509_VNC.to_be_bytes());
    stream.write_all(&subtypes).await.expect("subtypes");
    assert_eq!(read_exactly(stream, 4).await, X509_VNC.to_be_bytes());
    stream.write_all(&[1]).await.expect("TLS starts");
}

/// The rest of the session inside TLS: VNC Authentication, then a 4 by 2 desktop.
async fn serve_desktop<S: AsyncRead + AsyncWrite + Unpin>(stream: &mut S) {
    stream.write_all(&CHALLENGE).await.expect("challenge");
    assert_eq!(read_exactly(stream, 16).await, RESPONSE, "the password");
    stream.write_all(&[0, 0, 0, 0]).await.expect("result");
    let _ = read_exactly(stream, 1).await;
    let mut init = vec![0, 4, 0, 2];
    init.extend_from_slice(&[32, 24, 0, 1, 0, 255, 0, 255, 0, 255, 16, 8, 0, 0, 0, 0]);
    init.extend_from_slice(&[0, 0, 0, 0]);
    stream.write_all(&init).await.expect("init");
    let _ = read_exactly(stream, OPENING_REQUESTS).await;
}

/// Runs the attempt of `request` through the application, answering a password question
/// with "Secret12", until the server ends it; the events it sent, in order.
async fn run(
    app: &mut App,
    (tab, attempt, request): (TabId, AttemptId, VncRequest),
) -> Vec<ConnectionEvent> {
    let registry = AnswerRegistry::default();
    let mut events = vnc_events(request, registry.clone());
    let mut seen = Vec::new();
    while let Some(event) = tokio::time::timeout(WAIT, events.next())
        .await
        .expect("in time")
    {
        let question = match &event {
            ConnectionEvent::Question {
                question,
                kind: QuestionKind::ServerPassword(_),
            } => Some(*question),
            _ => None,
        };
        seen.push(event.clone());
        let _ = app.update(Message::Connection {
            tab,
            attempt,
            event,
        });
        if let Some(question) = question {
            assert!(registry.answer(
                question,
                Some(Answer::Secret(Secret::new("Secret12".to_owned())))
            ));
        }
    }
    seen
}

#[tokio::test]
async fn an_unknown_certificate_is_asked_about_before_any_password_then_pinned_once_trusted() {
    let (acceptor, pin) = server_tls();
    let listener = TcpListener::bind((HOST, 0)).await.expect("listener");
    let port = listener.local_addr().expect("address").port();
    let server = tokio::spawn(async move {
        // First: the client refuses the certificate and goes away.
        let (mut stream, _) = listener.accept().await.expect("accepted");
        serve_to_tls(&mut stream).await;
        assert!(acceptor.accept(stream).await.is_err(), "refused");
        // Then, trusted: the session.
        let (mut stream, _) = listener.accept().await.expect("accepted");
        serve_to_tls(&mut stream).await;
        let mut tls = acceptor.accept(stream).await.expect("TLS");
        serve_desktop(&mut tls).await;
    });
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path(), port);

    let first = attempt_of(&app.update(Message::OpenVnc(ProfileId::new("kiosk"))));
    let tab = first.0;
    let events = run(&mut app, first).await;
    assert!(
        matches!(
            events.as_slice(),
            [ConnectionEvent::UnknownRdpCertificate { fingerprint, .. }] if *fingerprint == pin
        ),
        "the certificate question alone, no password asked: {events:?}"
    );
    let whole = asked_whole(&events);
    let found = app.tab(tab).expect("tab");
    assert!(matches!(found.phase, Phase::HostKey { .. }));
    assert!(found.asks_about_certificate());

    // Trusted: the next attempt goes through, encrypted, and records it.
    let second = attempt_of(&app.update(Message::HostKeyDecision { tab, accept: true }));
    assert_eq!(
        second.2.accepted,
        Some(whole),
        "the very certificate asked about"
    );
    let events = run(&mut app, second).await;
    assert!(
        events.iter().any(|event| matches!(
            event,
            ConnectionEvent::VncReady {
                tls: Some("TLS 1.3"),
                ..
            }
        )),
        "{events:?}"
    );
    server.await.expect("server");
    let known = KnownRdpHosts::new(dir.path().join("known_vnc_hosts"));
    assert_eq!(
        known.verdict(HOST, port, &pin).expect("read"),
        heimdall_rdp::Verdict::Known
    );
    // Listed with the others, in a list of its own.
    let _ = app.update(Message::Settings(SettingsMessage::TrustedKeys(
        TrustedKeysMessage::Refresh,
    )));
    let keys = app.trusted_keys();
    assert!(keys.ftps.is_empty() && keys.rdp.is_empty());
    assert_eq!(keys.vnc.len(), 1);
    assert_eq!(keys.vnc[0].fingerprint, pin);
}

/// Pins another key than the server's for the profile's server.
fn pin_another(dir: &Path, port: u16) {
    let (_, other) = server_tls();
    KnownRdpHosts::new(dir.join("known_vnc_hosts"))
        .record(HOST, port, &other)
        .expect("pinned");
}

#[tokio::test]
async fn a_certificate_other_than_the_pinned_one_is_refused_as_a_changed_key() {
    let (acceptor, pin) = server_tls();
    let listener = TcpListener::bind((HOST, 0)).await.expect("listener");
    let port = listener.local_addr().expect("address").port();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("accepted");
        serve_to_tls(&mut stream).await;
        let _ = acceptor.accept(stream).await;
    });
    let dir = tempfile::tempdir().expect("dir");
    pin_another(dir.path(), port);
    let mut app = app(dir.path(), port);
    let attempt = attempt_of(&app.update(Message::OpenVnc(ProfileId::new("kiosk"))));
    let tab = attempt.0;
    let events = run(&mut app, attempt).await;
    let [ConnectionEvent::Failed(UiError::HostKeyChanged { offered, .. })] = events.as_slice()
    else {
        panic!("{events:?}");
    };
    assert_eq!(*offered, pin.to_string());
    server.await.expect("server");
    // Forgotten from the failure, the question comes back on the next attempt.
    let effects = app.update(Message::ForgetServer(tab));
    assert!(
        matches!(effects.as_slice(), [Effect::ConnectVnc { .. }]),
        "{effects:?}"
    );
    assert!(
        !KnownRdpHosts::new(dir.path().join("known_vnc_hosts"))
            .knows(HOST, port)
            .expect("read")
    );
}

#[tokio::test]
async fn a_pinned_server_offering_no_tls_is_refused_never_answered_in_clear() {
    let listener = TcpListener::bind((HOST, 0)).await.expect("listener");
    let port = listener.local_addr().expect("address").port();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("accepted");
        // VNC Authentication alone, as an impostor would offer it.
        offer_types(&mut stream, &[2]).await;
        let mut rest = Vec::new();
        let _ = tokio::time::timeout(WAIT, stream.read_to_end(&mut rest)).await;
        rest
    });
    let dir = tempfile::tempdir().expect("dir");
    pin_another(dir.path(), port);
    let mut app = app(dir.path(), port);
    let attempt = attempt_of(&app.update(Message::OpenVnc(ProfileId::new("kiosk"))));
    let events = run(&mut app, attempt).await;
    assert!(
        matches!(
            events.as_slice(),
            [ConnectionEvent::Failed(UiError::VncTlsRequired { offered })] if offered == "2"
        ),
        "no password asked: {events:?}"
    );
    assert!(
        server.await.expect("server").is_empty(),
        "no security type answered"
    );
}

#[tokio::test]
async fn a_profile_requiring_tls_refuses_a_server_offering_none_with_no_certificate_pinned() {
    let listener = TcpListener::bind((HOST, 0)).await.expect("listener");
    let port = listener.local_addr().expect("address").port();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("accepted");
        offer_types(&mut stream, &[2]).await;
        let mut rest = Vec::new();
        let _ = tokio::time::timeout(WAIT, stream.read_to_end(&mut rest)).await;
        rest
    });
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app_with(dir.path(), port, true);
    let attempt = attempt_of(&app.update(Message::OpenVnc(ProfileId::new("kiosk"))));
    let events = run(&mut app, attempt).await;
    assert!(
        matches!(
            events.as_slice(),
            [ConnectionEvent::Failed(UiError::VncTlsRequiredByProfile { offered })]
                if offered == "2"
        ),
        "the profile's reason, no password asked: {events:?}"
    );
    assert!(
        server.await.expect("server").is_empty(),
        "no security type answered"
    );
}

/// `TLSVnc`: anonymous TLS, never taken.
const TLS_VNC: u32 = 258;
/// VNC Authentication.
const VNC_AUTH: u8 = 2;

/// Plays a server offering `types`, `VeNCrypt` taken, then only `TLSVnc` inside it; it waits
/// for the client to go away and says whether it sent anything more.
async fn serve_nothing_inside(listener: &TcpListener, types: &[u8]) -> Vec<u8> {
    let (mut stream, _) = listener.accept().await.expect("accepted");
    offer_types(&mut stream, types).await;
    assert_eq!(read_exactly(&mut stream, 1).await, [VENCRYPT]);
    stream.write_all(&[0, 2]).await.expect("VeNCrypt version");
    let _ = read_exactly(&mut stream, 2).await;
    let mut subtypes = vec![0, 1];
    subtypes.extend_from_slice(&TLS_VNC.to_be_bytes());
    stream.write_all(&subtypes).await.expect("subtypes");
    let mut rest = Vec::new();
    let _ = tokio::time::timeout(WAIT, stream.read_to_end(&mut rest)).await;
    rest
}

/// Whether the client connects again within a short while.
async fn connects_again(listener: &TcpListener) -> bool {
    tokio::time::timeout(Duration::from_millis(500), listener.accept())
        .await
        .is_ok()
}

#[tokio::test]
async fn a_vencrypt_with_nothing_accepted_inside_is_left_once_for_the_other_type_offered() {
    let listener = TcpListener::bind((HOST, 0)).await.expect("listener");
    let port = listener.local_addr().expect("address").port();
    let server = tokio::spawn(async move {
        let types = [VENCRYPT, VNC_AUTH];
        let first = serve_nothing_inside(&listener, &types).await;
        // Again, VeNCrypt left out: VNC Authentication.
        let (mut stream, _) = listener.accept().await.expect("accepted again");
        offer_types(&mut stream, &types).await;
        assert_eq!(read_exactly(&mut stream, 1).await, [VNC_AUTH]);
        serve_desktop(&mut stream).await;
        drop(stream);
        (first, connects_again(&listener).await)
    });
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path(), port);
    let attempt = attempt_of(&app.update(Message::OpenVnc(ProfileId::new("kiosk"))));
    let events = run(&mut app, attempt).await;
    assert!(
        events
            .iter()
            .any(|event| matches!(event, ConnectionEvent::VncReady { tls: None, .. })),
        "{events:?}"
    );
    let (first, third) = server.await.expect("server");
    assert!(first.is_empty(), "no subtype answered");
    assert!(!third, "one attempt again, no more");
}

#[tokio::test]
async fn a_pinned_server_with_nothing_accepted_inside_vencrypt_is_refused_not_tried_again() {
    let listener = TcpListener::bind((HOST, 0)).await.expect("listener");
    let port = listener.local_addr().expect("address").port();
    let server = tokio::spawn(async move {
        let first = serve_nothing_inside(&listener, &[VENCRYPT, VNC_AUTH]).await;
        (first, connects_again(&listener).await)
    });
    let dir = tempfile::tempdir().expect("dir");
    pin_another(dir.path(), port);
    let mut app = app(dir.path(), port);
    let attempt = attempt_of(&app.update(Message::OpenVnc(ProfileId::new("kiosk"))));
    let events = run(&mut app, attempt).await;
    assert!(
        matches!(
            events.as_slice(),
            [ConnectionEvent::Failed(UiError::VncTlsRequired { offered })] if offered == "258"
        ),
        "{events:?}"
    );
    let (first, again) = server.await.expect("server");
    assert!(first.is_empty());
    assert!(!again, "never in clear once pinned");
}

#[tokio::test]
async fn a_profile_requiring_tls_is_not_tried_again_without_vencrypt() {
    let listener = TcpListener::bind((HOST, 0)).await.expect("listener");
    let port = listener.local_addr().expect("address").port();
    let server = tokio::spawn(async move {
        let first = serve_nothing_inside(&listener, &[VENCRYPT, VNC_AUTH]).await;
        (first, connects_again(&listener).await)
    });
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app_with(dir.path(), port, true);
    let attempt = attempt_of(&app.update(Message::OpenVnc(ProfileId::new("kiosk"))));
    let events = run(&mut app, attempt).await;
    assert!(
        matches!(
            events.as_slice(),
            [ConnectionEvent::Failed(UiError::VncTlsRequiredByProfile { offered })]
                if offered == "258"
        ),
        "{events:?}"
    );
    let (first, again) = server.await.expect("server");
    assert!(first.is_empty());
    assert!(!again, "never in clear when the profile requires TLS");
}

#[tokio::test]
async fn a_vencrypt_only_server_with_nothing_accepted_inside_is_refused_not_tried_again() {
    let listener = TcpListener::bind((HOST, 0)).await.expect("listener");
    let port = listener.local_addr().expect("address").port();
    let server = tokio::spawn(async move {
        let first = serve_nothing_inside(&listener, &[VENCRYPT]).await;
        (first, connects_again(&listener).await)
    });
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path(), port);
    let attempt = attempt_of(&app.update(Message::OpenVnc(ProfileId::new("kiosk"))));
    let events = run(&mut app, attempt).await;
    assert!(
        matches!(
            events.as_slice(),
            [ConnectionEvent::Failed(UiError::VncSecurityRefused { .. })]
        ),
        "{events:?}"
    );
    let (first, again) = server.await.expect("server");
    assert!(first.is_empty());
    assert!(!again, "nothing else to try");
}

/// Whether `events` are the refusal of the certificate pinned by `pin` as no longer valid,
/// over, and nothing else: no question.
fn refused_as_over(events: &[ConnectionEvent], port: u16, pin: Fingerprint) -> bool {
    refused_for(events, port, pin, heimdall_tls::ValidationIssue::Expired)
}

/// Whether `events` are the refusal of the certificate pinned by `pin` as no longer valid,
/// for `why`, and nothing else: no question.
fn refused_for(
    events: &[ConnectionEvent],
    port: u16,
    pin: Fingerprint,
    why: heimdall_tls::ValidationIssue,
) -> bool {
    matches!(
        events,
        [ConnectionEvent::Failed(UiError::PinnedCertificateInvalid {
            target,
            fingerprint,
            issue,
            ..
        })] if *target == format!("{HOST}:{port}") && *fingerprint == pin.to_string()
            && *issue == why
    )
}

#[tokio::test]
async fn a_pinned_certificate_over_is_refused_never_asked_about_and_stays_pinned() {
    let (acceptor, pin) = expired_server_tls();
    let listener = TcpListener::bind((HOST, 0)).await.expect("listener");
    let port = listener.local_addr().expect("address").port();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("accepted");
        serve_to_tls(&mut stream).await;
        assert!(acceptor.accept(stream).await.is_err(), "refused");
    });
    let dir = tempfile::tempdir().expect("dir");
    let file = dir.path().join("known_vnc_hosts");
    KnownRdpHosts::new(&file)
        .record(HOST, port, &pin)
        .expect("pinned");
    let pinned = std::fs::read(&file).expect("pins");
    let mut app = app(dir.path(), port);
    let attempt = attempt_of(&app.update(Message::OpenVnc(ProfileId::new("kiosk"))));
    let tab = attempt.0;
    let events = run(&mut app, attempt).await;
    assert!(refused_as_over(&events, port, pin), "{events:?}");
    server.await.expect("server");
    let found = app.tab(tab).expect("tab");
    assert!(matches!(
        found.phase,
        Phase::Failed(UiError::PinnedCertificateInvalid { .. })
    ));
    assert!(!found.asks_about_certificate());
    assert_eq!(std::fs::read(&file).expect("pins"), pinned, "the pin stays");
}

#[tokio::test]
async fn a_certificate_trusted_once_goes_through_on_the_answer_then_is_checked_as_a_pin() {
    let (acceptor, pin) = expired_server_tls();
    let listener = TcpListener::bind((HOST, 0)).await.expect("listener");
    let port = listener.local_addr().expect("address").port();
    let server = tokio::spawn(async move {
        // The question: the client goes away.
        let (mut stream, _) = listener.accept().await.expect("accepted");
        serve_to_tls(&mut stream).await;
        assert!(acceptor.accept(stream).await.is_err(), "asked about");
        // Trusted once: the session, over as the certificate is.
        let (mut stream, _) = listener.accept().await.expect("accepted");
        serve_to_tls(&mut stream).await;
        let mut tls = acceptor.accept(stream).await.expect("TLS");
        serve_desktop(&mut tls).await;
        drop(tls);
        // Connected again: refused as a pin no longer valid.
        let (mut stream, _) = listener.accept().await.expect("accepted");
        serve_to_tls(&mut stream).await;
        assert!(acceptor.accept(stream).await.is_err(), "refused");
        // Forgotten: asked about again.
        let (mut stream, _) = listener.accept().await.expect("accepted");
        serve_to_tls(&mut stream).await;
        assert!(acceptor.accept(stream).await.is_err(), "asked about again");
    });
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path(), port);

    let first = attempt_of(&app.update(Message::OpenVnc(ProfileId::new("kiosk"))));
    let tab = first.0;
    let events = run(&mut app, first).await;
    assert!(
        matches!(
            events.as_slice(),
            [ConnectionEvent::UnknownRdpCertificate { fingerprint, .. }] if *fingerprint == pin
        ),
        "{events:?}"
    );
    let whole = asked_whole(&events);

    // Trusted once: the attempt built on the answer takes it as it is.
    let second = attempt_of(&app.update(Message::HostKeyTrustOnce(tab)));
    assert_eq!(second.2.accepted, None, "nothing to record");
    assert_eq!(second.2.trusted_once, Some(whole));
    assert_eq!(second.2.trusted_for_run, [whole]);
    let events = run(&mut app, second).await;
    assert!(
        events
            .iter()
            .any(|event| matches!(event, ConnectionEvent::VncReady { .. })),
        "{events:?}"
    );

    // The next connection checks it as a pin: over, refused, not asked about.
    let third = attempt_of(&app.update(Message::ReconnectTab(tab)));
    let tab = third.0;
    assert_eq!(third.2.trusted_once, None, "decided once only");
    assert_eq!(third.2.trusted_for_run, [whole]);
    let events = run(&mut app, third).await;
    assert!(refused_as_over(&events, port, pin), "{events:?}");

    // Forgotten from the failure: the key trusted for this run with it, the question
    // comes back.
    let fourth = attempt_of(&app.update(Message::ForgetServer(tab)));
    assert_eq!(fourth.2.trusted_once, None);
    assert!(fourth.2.trusted_for_run.is_empty(), "no longer trusted");
    let events = run(&mut app, fourth).await;
    assert!(
        matches!(
            events.as_slice(),
            [ConnectionEvent::UnknownRdpCertificate { fingerprint, .. }] if *fingerprint == pin
        ),
        "{events:?}"
    );
    server.await.expect("server");
    assert!(
        !dir.path().join("known_vnc_hosts").exists(),
        "never written"
    );
}

/// The events of one attempt to the profile's server on `port`, its key pinned on file by
/// `pin` beforehand, the server presenting what `acceptor` holds; and the tab.
async fn pinned_attempt(
    dir: &Path,
    acceptor: TlsAcceptor,
    pin: Fingerprint,
) -> (App, TabId, Vec<ConnectionEvent>, u16) {
    let listener = TcpListener::bind((HOST, 0)).await.expect("listener");
    let port = listener.local_addr().expect("address").port();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("accepted");
        serve_to_tls(&mut stream).await;
        assert!(acceptor.accept(stream).await.is_err(), "refused");
    });
    KnownRdpHosts::new(dir.join("known_vnc_hosts"))
        .record(HOST, port, &pin)
        .expect("pinned");
    let mut app = app(dir, port);
    let attempt = attempt_of(&app.update(Message::OpenVnc(ProfileId::new("kiosk"))));
    let tab = attempt.0;
    let events = run(&mut app, attempt).await;
    server.await.expect("server");
    (app, tab, events, port)
}

#[tokio::test]
async fn a_pinned_certificate_not_yet_valid_is_refused_never_asked_about() {
    let ((acceptor, pin), _) = made_server_tls((2045, 2046), Vec::new(), false);
    let dir = tempfile::tempdir().expect("dir");
    let (_, _, events, port) = pinned_attempt(dir.path(), acceptor, pin).await;
    assert!(
        refused_for(
            &events,
            port,
            pin,
            heimdall_tls::ValidationIssue::NotYetValid
        ),
        "{events:?}"
    );
}

#[tokio::test]
async fn a_pinned_authority_serving_as_its_own_server_for_clients_only_is_refused() {
    let ((acceptor, pin), _) = made_server_tls(
        (2020, 2045),
        vec![rcgen::ExtendedKeyUsagePurpose::ClientAuth],
        true,
    );
    let dir = tempfile::tempdir().expect("dir");
    let (_, _, events, port) = pinned_attempt(dir.path(), acceptor, pin).await;
    assert!(
        refused_for(
            &events,
            port,
            pin,
            heimdall_tls::ValidationIssue::WrongPurpose
        ),
        "{events:?}"
    );
}

#[tokio::test]
async fn a_forged_certificate_around_the_pinned_key_fails_as_tls_never_as_a_pin_to_forget() {
    // The server's own certificate, current, its key pinned; the forger has its public key
    // only.
    let ((_, pin), victim_key) = made_server_tls((2020, 2045), Vec::new(), false);
    let dir = tempfile::tempdir().expect("dir");
    let file = dir.path().join("known_vnc_hosts");
    let (mut app, tab, events, _) =
        pinned_attempt(dir.path(), forged_server_tls(&victim_key), pin).await;
    assert!(
        matches!(
            events.as_slice(),
            [ConnectionEvent::Failed(UiError::VncProtocol { .. })]
        ),
        "a TLS failure, no refusal of the pin: {events:?}"
    );
    assert!(
        app.update(Message::ForgetServer(tab)).is_empty(),
        "nothing to forget"
    );
    let [entry] = KnownRdpHosts::new(&file)
        .entries()
        .expect("read")
        .try_into()
        .expect("one entry");
    assert_eq!(entry.fingerprint, pin, "the pin stays");

    // The server itself, over, holding the key: refused as the pin no longer valid.
    let victim = rcgen::KeyPair::try_from(victim_key.as_slice()).expect("key");
    let cert = params((2000, 2001), Vec::new(), false)
        .self_signed(&victim)
        .expect("cert");
    let (genuine, same_pin) = acceptor(cert.der().to_vec(), victim_key);
    assert_eq!(same_pin, pin, "the same key");
    let dir = tempfile::tempdir().expect("dir");
    let (_, _, events, port) = pinned_attempt(dir.path(), genuine, pin).await;
    assert!(refused_as_over(&events, port, pin), "{events:?}");
}

/// A certificate minted for [`HOST`] on a key: the whole of it, its hash and its validity.
#[derive(Clone)]
struct Minted {
    der: Vec<u8>,
    hash: CertificateHash,
    held: Validity,
}

/// A self-signed certificate for [`HOST`] minted on `key` (PKCS #8), valid from the first
/// of January of `from` to that of `until`. Each one minted is another certificate, even
/// on the same key and dates.
fn mint(key: &[u8], validity: (i32, i32)) -> Minted {
    let pair = rcgen::KeyPair::try_from(key).expect("key");
    let der = params(validity, Vec::new(), false)
        .self_signed(&pair)
        .expect("cert")
        .der()
        .to_vec();
    Minted {
        hash: CertificateHash::of(&der),
        held: Validity::from_der(&der).expect("validity"),
        der,
    }
}

/// A fresh key pair (PKCS #8), and the key the certificates on it are pinned by.
fn fresh_key() -> (Vec<u8>, Fingerprint) {
    let key = rcgen::KeyPair::generate().expect("key").serialize_der();
    let fingerprint = ServerCertificate::from_der(&mint(&key, (2020, 2045)).der)
        .expect("readable")
        .fingerprint;
    (key, fingerprint)
}

/// A server on `listener` presenting, at each connection in turn, the certificate of a step
/// signing with `key`; the session goes on when the step says so, else the client is
/// expected to go away at the certificate.
fn serve_steps(
    listener: TcpListener,
    key: &[u8],
    steps: Vec<(&Minted, bool)>,
) -> tokio::task::JoinHandle<()> {
    let acceptors: Vec<(TlsAcceptor, bool)> = steps
        .into_iter()
        .map(|(minted, session)| (acceptor(minted.der.clone(), key.to_vec()).0, session))
        .collect();
    tokio::spawn(async move {
        for (acceptor, session) in acceptors {
            let (mut stream, _) = listener.accept().await.expect("accepted");
            serve_to_tls(&mut stream).await;
            if session {
                let mut tls = acceptor.accept(stream).await.expect("TLS");
                serve_desktop(&mut tls).await;
            } else {
                assert!(acceptor.accept(stream).await.is_err(), "stopped at it");
            }
        }
    })
}

fn ready(events: &[ConnectionEvent]) -> bool {
    events
        .iter()
        .any(|event| matches!(event, ConnectionEvent::VncReady { .. }))
}

#[tokio::test]
async fn a_certificate_minted_again_on_the_pinned_key_is_asked_about_as_a_renewal() {
    let (key, pin) = fresh_key();
    let listener = TcpListener::bind((HOST, 0)).await.expect("listener");
    let port = listener.local_addr().expect("address").port();
    // Pinned whole.
    let first = mint(&key, (2020, 2045));
    let dir = tempfile::tempdir().expect("dir");
    let file = KnownRdpHosts::new(dir.path().join("known_vnc_hosts"));
    file.record_whole_certificate(
        HOST,
        port,
        &ServerCertificate::from_der(&first.der).expect("readable"),
        (&first.hash, Some(&first.held)),
    )
    .expect("pinned");
    // Whoever holds the key mints another, current: asked about, never taken silently;
    // trusted, the session; then the one it replaced is asked about in turn.
    let minted_again = mint(&key, (2021, 2046));
    let server = serve_steps(
        listener,
        &key,
        vec![
            (&minted_again, false),
            (&minted_again, true),
            (&first, false),
        ],
    );
    let mut app = app(dir.path(), port);
    let attempt = attempt_of(&app.update(Message::OpenVnc(ProfileId::new("kiosk"))));
    let tab = attempt.0;
    let events = run(&mut app, attempt).await;
    assert!(
        matches!(
            events.as_slice(),
            [ConnectionEvent::UnknownRdpCertificate { fingerprint, .. }] if *fingerprint == pin
        ),
        "the question, not a silent connection: {events:?}"
    );
    assert_eq!(
        asked_renewal(&events),
        Some(heimdall_app::Renewal {
            recorded: Some(first.held)
        }),
        "said renewed, with the certificate on record"
    );
    assert_eq!(asked_whole(&events), minted_again.hash);

    let second = attempt_of(&app.update(Message::HostKeyDecision { tab, accept: true }));
    assert_eq!(second.2.accepted, Some(minted_again.hash));
    assert!(ready(&run(&mut app, second).await));
    let [entry] = file
        .entries()
        .expect("read")
        .try_into()
        .expect("one entry: the renewed one replaced the old");
    assert_eq!(
        (entry.fingerprint, entry.certificate),
        (pin, Some(minted_again.hash))
    );

    let third = attempt_of(&app.update(Message::ReconnectTab(tab)));
    let events = run(&mut app, third).await;
    assert_eq!(asked_whole(&events), first.hash, "no longer trusted");
    assert!(asked_renewal(&events).is_some());
    server.await.expect("server");
}

#[tokio::test]
async fn a_server_pinned_by_its_key_alone_adopts_its_certificate_then_asks_about_another() {
    let (key, pin) = fresh_key();
    let listener = TcpListener::bind((HOST, 0)).await.expect("listener");
    let port = listener.local_addr().expect("address").port();
    let dir = tempfile::tempdir().expect("dir");
    let file = KnownRdpHosts::new(dir.path().join("known_vnc_hosts"));
    // As an earlier Heimdall pinned it: its key alone.
    file.record(HOST, port, &pin).expect("pinned");
    let (presented, another) = (mint(&key, (2020, 2045)), mint(&key, (2021, 2046)));
    let server = serve_steps(
        listener,
        &key,
        vec![(&presented, true), (&presented, true), (&another, false)],
    );
    let mut app = app(dir.path(), port);

    // The first connection goes as before, and adopts its certificate.
    let attempt = attempt_of(&app.update(Message::OpenVnc(ProfileId::new("kiosk"))));
    let tab = attempt.0;
    assert!(ready(&run(&mut app, attempt).await));
    let [entry] = file.entries().expect("read").try_into().expect("one entry");
    assert_eq!(
        (entry.certificate, entry.validity),
        (Some(presented.hash), Some(presented.held))
    );

    // The same certificate: known.
    let again = attempt_of(&app.update(Message::ReconnectTab(tab)));
    let tab = again.0;
    assert!(ready(&run(&mut app, again).await));

    // Another on the same key: asked about, as a renewal.
    let third = attempt_of(&app.update(Message::ReconnectTab(tab)));
    let events = run(&mut app, third).await;
    assert_eq!(asked_whole(&events), another.hash);
    assert_eq!(
        asked_renewal(&events),
        Some(heimdall_app::Renewal {
            recorded: Some(presented.held)
        })
    );
    server.await.expect("server");
}

#[tokio::test]
async fn an_answer_trusts_the_very_certificate_asked_about_not_its_key() {
    let (key, pin) = fresh_key();
    let listener = TcpListener::bind((HOST, 0)).await.expect("listener");
    let port = listener.local_addr().expect("address").port();
    // Asked about one certificate; the attempt built on the answer meets another on the
    // same key.
    let (asked, other) = (mint(&key, (2020, 2045)), mint(&key, (2021, 2046)));
    let server = serve_steps(listener, &key, vec![(&asked, false), (&other, false)]);
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path(), port);
    let attempt = attempt_of(&app.update(Message::OpenVnc(ProfileId::new("kiosk"))));
    let tab = attempt.0;
    let events = run(&mut app, attempt).await;
    assert_eq!(asked_whole(&events), asked.hash);
    let second = attempt_of(&app.update(Message::HostKeyTrustOnce(tab)));
    assert_eq!(second.2.trusted_once, Some(asked.hash));
    let events = run(&mut app, second).await;
    assert!(
        matches!(
            events.as_slice(),
            [ConnectionEvent::UnknownRdpCertificate { fingerprint, .. }] if *fingerprint == pin
        ),
        "another certificate on the key is asked about: {events:?}"
    );
    assert_eq!(asked_whole(&events), other.hash);
    server.await.expect("server");
    assert!(
        !dir.path().join("known_vnc_hosts").exists(),
        "never written"
    );
}

/// What a forger presents around the public key of `victim` (PKCS #8): a certificate valid
/// from the first of January of `from` to that of `until`, signed by an authority of its
/// own; and the key it signs the handshake with, its own, not holding the victim's.
fn forge(victim: &[u8], validity: (i32, i32)) -> (Minted, Vec<u8>) {
    let victim = rcgen::KeyPair::try_from(victim).expect("victim's key");
    let mut authority = rcgen::CertificateParams::new(Vec::<String>::new()).expect("params");
    authority.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
    let issuer = rcgen::Issuer::new(authority, rcgen::KeyPair::generate().expect("key"));
    let der = params(validity, Vec::new(), false)
        .signed_by(&victim, &issuer)
        .expect("forged")
        .der()
        .to_vec();
    let forged = Minted {
        hash: CertificateHash::of(&der),
        held: Validity::from_der(&der).expect("validity"),
        der,
    };
    (
        forged,
        rcgen::KeyPair::generate().expect("key").serialize_der(),
    )
}

/// Whether `events` are a TLS failure for the handshake signature, and nothing else.
fn failed_at_the_signature(events: &[ConnectionEvent]) -> bool {
    matches!(
        events,
        [ConnectionEvent::Failed(UiError::VncProtocol { detail })] if detail.contains("BadSignature")
    )
}

#[tokio::test]
async fn a_forged_renewal_on_the_pinned_key_is_asked_about_and_once_trusted_fails_recording_nothing()
 {
    let (key, pin) = fresh_key();
    let listener = TcpListener::bind((HOST, 0)).await.expect("listener");
    let port = listener.local_addr().expect("address").port();
    let pinned = mint(&key, (2020, 2045));
    let dir = tempfile::tempdir().expect("dir");
    let file = KnownRdpHosts::new(dir.path().join("known_vnc_hosts"));
    file.record_whole_certificate(
        HOST,
        port,
        &ServerCertificate::from_der(&pinned.der).expect("readable"),
        (&pinned.hash, Some(&pinned.held)),
    )
    .expect("pinned");
    let before = std::fs::read(file.path()).expect("pins");
    // Around the pinned public key, which anyone can read, without its private key.
    let (forged, forger_key) = forge(&key, (2021, 2046));
    let server = serve_steps(listener, &forger_key, vec![(&forged, false); 4]);
    let mut app = app(dir.path(), port);

    // Asked about, as a renewal.
    let attempt = attempt_of(&app.update(Message::OpenVnc(ProfileId::new("kiosk"))));
    let tab = attempt.0;
    let events = run(&mut app, attempt).await;
    assert!(
        matches!(
            events.as_slice(),
            [ConnectionEvent::UnknownRdpCertificate { fingerprint, .. }] if *fingerprint == pin
        ),
        "{events:?}"
    );
    assert_eq!(
        asked_renewal(&events),
        Some(heimdall_app::Renewal {
            recorded: Some(pinned.held)
        })
    );
    assert_eq!(asked_whole(&events), forged.hash);

    // Trusted for good: the forger cannot sign the handshake, nothing is recorded.
    let second = attempt_of(&app.update(Message::HostKeyDecision { tab, accept: true }));
    assert_eq!(second.2.accepted, Some(forged.hash));
    let events = run(&mut app, second).await;
    assert!(failed_at_the_signature(&events), "{events:?}");
    assert_eq!(std::fs::read(file.path()).expect("pins"), before);

    // Asked again, trusted once: the same.
    let third = attempt_of(&app.update(Message::ReconnectTab(tab)));
    let tab = third.0;
    let events = run(&mut app, third).await;
    assert_eq!(asked_whole(&events), forged.hash);
    let fourth = attempt_of(&app.update(Message::HostKeyTrustOnce(tab)));
    assert_eq!(fourth.2.trusted_once, Some(forged.hash));
    let events = run(&mut app, fourth).await;
    assert!(failed_at_the_signature(&events), "{events:?}");
    server.await.expect("server");
    assert_eq!(
        std::fs::read(file.path()).expect("pins"),
        before,
        "the file unchanged"
    );
}
