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

//! The X509 subtypes of `VeNCrypt` against a TLS server on this machine, with a certificate
//! no system trusts: trusted by its pin, or refused before any password goes out.

use std::io;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::Duration;

use heimdall_remote::vnc::{
    AskPassword, Authentication, RfbError, Security, SecurityPolicy, SecurityWrapper, VncConfig,
    VncConnection, VncError, connect, given_password,
};
use heimdall_tls::{Period, PresentedSlot, UserTrust, UserVerdict, fingerprint};
use tokio::io::{AsyncRead, AsyncReadExt as _, AsyncWrite, AsyncWriteExt as _, ReadBuf};
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::TlsAcceptor;
use tokio_rustls::rustls::ServerConfig;
use tokio_rustls::rustls::crypto::ring::default_provider;
use tokio_rustls::rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;

/// Bound on anything the test waits for.
const WAIT: Duration = Duration::from_secs(10);

const PASSWORD: &str = "Secret12";

const CHALLENGE: [u8; 16] = [
    0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff,
];
/// The response for "Secret12", computed with openssl.
const RESPONSE: [u8; 16] = [
    0xee, 0xe9, 0x81, 0xe2, 0x74, 0x19, 0x66, 0x45, 0xd7, 0x10, 0xe9, 0xd9, 0x1f, 0xf5, 0xf5, 0xe8,
];

/// What the client sends between `ServerInit` and the first update: `SetPixelFormat` (20),
/// `SetEncodings` of 11 (4 + 44) and a `FramebufferUpdateRequest` (10).
const OPENING_REQUESTS: usize = 20 + 4 + 16 * 4 + 10;

/// The X509 subtypes.
const X509_NONE: u32 = 260;
const X509_VNC: u32 = 261;
const X509_PLAIN: u32 = 262;

/// A TLS server with a fresh self-signed certificate, and the certificate.
fn server_tls() -> (TlsAcceptor, Vec<u8>) {
    let issued = rcgen::generate_simple_self_signed(vec!["127.0.0.1".to_owned()]).expect("cert");
    let der = issued.cert.der().to_vec();
    let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(issued.signing_key.serialize_der()));
    let config = ServerConfig::builder_with_provider(Arc::new(default_provider()))
        .with_safe_default_protocol_versions()
        .expect("versions")
        .with_no_client_auth()
        .with_single_cert(vec![CertificateDer::from(der.clone())], key)
        .expect("server config");
    (TlsAcceptor::from(Arc::new(config)), der)
}

/// A client trusting the certificates of `pinned` beside what the system trusts; what it was
/// shown when it trusted neither lands in `presented`.
fn config(port: u16, policy: SecurityPolicy, pinned: &[Vec<u8>]) -> (VncConfig, PresentedSlot) {
    let pins: Vec<[u8; 32]> = pinned.iter().map(|der| fingerprint(der)).collect();
    // Pinned and current: the certificates the test servers make are.
    let trusted: UserTrust = Arc::new(move |der, _| {
        if pins.contains(&fingerprint(der)) {
            UserVerdict::Pinned {
                period: Period::Current,
                for_servers: true,
            }
        } else {
            UserVerdict::Untrusted
        }
    });
    let presented = PresentedSlot::default();
    let config = VncConfig {
        host: "127.0.0.1".to_owned(),
        port,
        policy,
        tls: heimdall_tls::connector(trusted, presented.clone()),
        connect_timeout: WAIT,
        handshake_timeout: WAIT,
    };
    (config, presented)
}

async fn listener() -> (TcpListener, u16) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let port = listener.local_addr().expect("address").port();
    (listener, port)
}

async fn read_exactly<S: AsyncRead + Unpin>(stream: &mut S, count: usize) -> Vec<u8> {
    let mut bytes = vec![0; count];
    tokio::time::timeout(WAIT, stream.read_exact(&mut bytes))
        .await
        .expect("in time")
        .expect("read");
    bytes
}

/// Plays a `VeNCrypt` server offering `subtypes` up to the start of TLS; the subtype the
/// client took, the server answering 1 to it. Nothing is answered past the subtype when it
/// is not an X509 one.
async fn serve_to_tls(stream: &mut TcpStream, subtypes: &[u32]) -> u32 {
    stream.write_all(b"RFB 003.008\n").await.expect("version");
    let _ = read_exactly(stream, 12).await;
    stream.write_all(&[1, 19]).await.expect("types");
    assert_eq!(read_exactly(stream, 1).await, [19]);
    stream.write_all(&[0, 2]).await.expect("VeNCrypt version");
    assert_eq!(read_exactly(stream, 2).await, [0, 2]);
    let mut offer = vec![0, u8::try_from(subtypes.len()).expect("few")];
    for subtype in subtypes {
        offer.extend_from_slice(&subtype.to_be_bytes());
    }
    stream.write_all(&offer).await.expect("subtypes");
    let taken = read_exactly(stream, 4).await;
    let taken = u32::from_be_bytes(taken.try_into().expect("four bytes"));
    if (X509_NONE..=X509_PLAIN).contains(&taken) {
        stream.write_all(&[1]).await.expect("TLS starts");
    }
    taken
}

/// The rest of a session from the security result on, on a 2 by 1 desktop.
async fn serve_desktop<S: AsyncRead + AsyncWrite + Unpin>(stream: &mut S) {
    stream.write_all(&[0, 0, 0, 0]).await.expect("result");
    assert_eq!(read_exactly(stream, 1).await, [1], "ClientInit");
    let mut init = vec![0, 2, 0, 1];
    init.extend_from_slice(&[32, 24, 0, 1, 0, 255, 0, 255, 0, 255, 16, 8, 0, 0, 0, 0]);
    init.extend_from_slice(&[0, 0, 0, 3]);
    init.extend_from_slice(b"tls");
    stream.write_all(&init).await.expect("init");
    let _ = read_exactly(stream, OPENING_REQUESTS).await;
}

/// A password that records whether it was asked for.
fn watched_password() -> (AskPassword, Arc<AtomicBool>) {
    let asked = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&asked);
    let password: AskPassword = Box::new(move || {
        flag.store(true, Ordering::SeqCst);
        Box::pin(std::future::ready(Some(Zeroizing::new(
            PASSWORD.to_owned(),
        ))))
    });
    (password, asked)
}

/// Connects with `config` and `password`.
async fn client(config: VncConfig, password: AskPassword) -> Result<VncConnection, VncError> {
    connect(&config, password, &CancellationToken::new()).await
}

#[tokio::test]
async fn x509_vnc_opens_encrypted_once_the_certificate_is_trusted_by_its_pin() {
    let (acceptor, certificate) = server_tls();
    let (listener, port) = listener().await;
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("accepted");
        assert_eq!(serve_to_tls(&mut stream, &[2, X509_VNC]).await, X509_VNC);
        let mut tls = acceptor.accept(stream).await.expect("TLS");
        tls.write_all(&CHALLENGE).await.expect("challenge");
        assert_eq!(read_exactly(&mut tls, 16).await, RESPONSE, "inside TLS");
        serve_desktop(&mut tls).await;
    });
    let (config, presented) = config(
        port,
        SecurityPolicy::default(),
        std::slice::from_ref(&certificate),
    );
    let connection = client(config, given_password(Zeroizing::new(PASSWORD.to_owned())))
        .await
        .expect("connected");
    assert_eq!(connection.name, "tls");
    assert_eq!(
        connection.security(),
        Some(Security {
            wrapper: Some(SecurityWrapper::VeNCrypt),
            authentication: Authentication::VncAuth,
            tls: true,
        })
    );
    assert_eq!(connection.tls_version(), Some("TLS 1.3"));
    assert!(presented.lock().expect("slot").is_none(), "nothing to ask");
    server.await.expect("server");
}

/// A stream that keeps every byte read from it.
struct Recording {
    stream: TcpStream,
    read: Arc<Mutex<Vec<u8>>>,
}

impl AsyncRead for Recording {
    fn poll_read(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let before = buffer.filled().len();
        let polled = Pin::new(&mut self.stream).poll_read(context, buffer);
        if let Ok(mut read) = self.read.lock() {
            read.extend_from_slice(&buffer.filled()[before..]);
        }
        polled
    }
}

impl AsyncWrite for Recording {
    fn poll_write(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.stream).poll_write(context, bytes)
    }

    fn poll_flush(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stream).poll_flush(context)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stream).poll_shutdown(context)
    }
}

/// Plays an X509 server whose certificate the client does not trust: every byte the client
/// sent after the subtype, until it went away.
async fn serve_untrusted(listener: TcpListener, acceptor: TlsAcceptor, subtype: u32) -> Vec<u8> {
    let (mut stream, _) = listener.accept().await.expect("accepted");
    assert_eq!(serve_to_tls(&mut stream, &[2, subtype]).await, subtype);
    let read = Arc::new(Mutex::new(Vec::new()));
    let recording = Recording {
        stream,
        read: Arc::clone(&read),
    };
    let accepted = tokio::time::timeout(WAIT, acceptor.accept(recording))
        .await
        .expect("the client went on or went away");
    if let Ok(mut tls) = accepted {
        // Not reached: the client refuses. Whatever it sends then is read all the same.
        let mut rest = Vec::new();
        let _ = tokio::time::timeout(WAIT, tls.read_to_end(&mut rest)).await;
    }
    read.lock().expect("recorded").clone()
}

#[tokio::test]
async fn an_untrusted_certificate_stops_the_connection_before_any_password_is_asked_or_sent() {
    for subtype in [X509_VNC, X509_PLAIN] {
        let (acceptor, certificate) = server_tls();
        let (listener, port) = listener().await;
        let server = tokio::spawn(serve_untrusted(listener, acceptor, subtype));
        let policy = SecurityPolicy {
            username: Some("admin".to_owned()),
            ..SecurityPolicy::default()
        };
        // A pin, but another certificate's.
        let (_, other) = server_tls();
        let (config, presented) = config(port, policy, std::slice::from_ref(&other));
        let (password, asked) = watched_password();
        let outcome = client(config, password).await;
        assert!(matches!(outcome, Err(VncError::Tls(_))), "{outcome:?}");
        assert!(!asked.load(Ordering::SeqCst), "no password asked for");
        let shown = presented
            .lock()
            .expect("slot")
            .take()
            .expect("kept for the question");
        assert_eq!(
            shown.der, certificate,
            "the certificate the server presented"
        );
        let received = server.await.expect("server");
        assert!(!received.is_empty(), "the TLS handshake began");
        for secret in [PASSWORD.as_bytes(), RESPONSE.as_slice()] {
            assert!(
                !received
                    .windows(secret.len())
                    .any(|window| window == secret),
                "no password byte reached the server"
            );
        }
    }
}

#[tokio::test]
async fn x509_plain_sends_the_user_name_and_password_inside_tls() {
    let (acceptor, certificate) = server_tls();
    let (listener, port) = listener().await;
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("accepted");
        assert_eq!(
            serve_to_tls(&mut stream, &[X509_PLAIN, 2]).await,
            X509_PLAIN
        );
        let mut tls = acceptor.accept(stream).await.expect("TLS");
        let lengths = read_exactly(&mut tls, 8).await;
        assert_eq!(lengths, [0, 0, 0, 5, 0, 0, 0, 8]);
        assert_eq!(read_exactly(&mut tls, 13).await, b"adminSecret12");
        serve_desktop(&mut tls).await;
    });
    let policy = SecurityPolicy {
        username: Some("admin".to_owned()),
        ..SecurityPolicy::default()
    };
    let (config, _) = config(port, policy, std::slice::from_ref(&certificate));
    let connection = client(config, given_password(Zeroizing::new(PASSWORD.to_owned())))
        .await
        .expect("connected");
    assert_eq!(
        connection
            .security()
            .map(|security| security.authentication),
        Some(Authentication::Plain)
    );
    assert!(connection.tls_version().is_some());
    server.await.expect("server");
}

#[tokio::test]
async fn x509_none_opens_without_a_password_when_the_profile_allows_none() {
    let (acceptor, certificate) = server_tls();
    let (listener, port) = listener().await;
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("accepted");
        assert_eq!(serve_to_tls(&mut stream, &[1, X509_NONE]).await, X509_NONE);
        let mut tls = acceptor.accept(stream).await.expect("TLS");
        serve_desktop(&mut tls).await;
    });
    let policy = SecurityPolicy {
        allow_no_authentication: true,
        ..SecurityPolicy::default()
    };
    let (config, _) = config(port, policy, std::slice::from_ref(&certificate));
    let (password, asked) = watched_password();
    let connection = client(config, password).await.expect("connected");
    assert!(!asked.load(Ordering::SeqCst));
    assert_eq!(
        connection.security(),
        Some(Security {
            wrapper: Some(SecurityWrapper::VeNCrypt),
            authentication: Authentication::NoAuthentication,
            tls: true,
        })
    );
    server.await.expect("server");
}

#[tokio::test]
async fn a_profile_requiring_tls_is_refused_by_a_server_offering_it_no_more() {
    let (listener, port) = listener().await;
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("accepted");
        stream.write_all(b"RFB 003.008\n").await.expect("version");
        let _ = read_exactly(&mut stream, 12).await;
        stream.write_all(&[1, 2]).await.expect("types");
        // The client goes away without answering a type.
        let mut rest = Vec::new();
        let _ = tokio::time::timeout(WAIT, stream.read_to_end(&mut rest)).await;
        rest
    });
    let policy = SecurityPolicy {
        require_tls: true,
        ..SecurityPolicy::default()
    };
    let (config, _) = config(port, policy, &[]);
    let (password, asked) = watched_password();
    let outcome = client(config, password).await;
    assert!(
        matches!(outcome, Err(VncError::Rfb(RfbError::TlsRequired(ref offered))) if *offered == [2]),
        "{outcome:?}"
    );
    assert!(!asked.load(Ordering::SeqCst));
    assert!(server.await.expect("server").is_empty(), "nothing answered");
}
