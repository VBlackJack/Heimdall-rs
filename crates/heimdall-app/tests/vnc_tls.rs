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
//! trusted, then refused when it changes and a downgrade to clear refused.

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
use heimdall_rdp::{Fingerprint, KnownRdpHosts, ServerCertificate};
use heimdall_ssh::{AgentSource, Secret};
use heimdall_term::GridSize;
use tokio::io::{AsyncRead, AsyncReadExt as _, AsyncWrite, AsyncWriteExt as _};
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::TlsAcceptor;
use tokio_rustls::rustls::ServerConfig;
use tokio_rustls::rustls::crypto::ring::default_provider;
use tokio_rustls::rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
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
    let der = issued.cert.der().to_vec();
    let pin = ServerCertificate::from_der(&der)
        .expect("readable")
        .fingerprint;
    let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(issued.signing_key.serialize_der()));
    let config = ServerConfig::builder_with_provider(Arc::new(default_provider()))
        .with_safe_default_protocol_versions()
        .expect("versions")
        .with_no_client_auth()
        .with_single_cert(vec![CertificateDer::from(der)], key)
        .expect("server config");
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
    let found = app.tab(tab).expect("tab");
    assert!(matches!(found.phase, Phase::HostKey { .. }));
    assert!(found.asks_about_certificate());

    // Trusted: the next attempt goes through, encrypted, and records it.
    let second = attempt_of(&app.update(Message::HostKeyDecision { tab, accept: true }));
    assert_eq!(second.2.accepted, Some(pin));
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
