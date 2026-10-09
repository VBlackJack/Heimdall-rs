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

//! A VNC session over TCP, against a scripted server on this machine; and, when
//! `HEIMDALL_LIVE_VNC_PORT` and `HEIMDALL_LIVE_VNC_PASSWORD` are set, against a real one.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use heimdall_remote::vnc::{
    AskPassword, Authentication, CloseReason, Quality, Rect, Security, SecurityPolicy,
    SecurityWrapper, VncConfig, VncError, VncEvent, VncSession, connect, given_password, start,
};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::{TcpListener, TcpStream};
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;

/// The Extended Clipboard pseudo-encoding, 0xC0A1E5CE as noVNC's `encodings.js` has it.
const EXTENDED_CLIPBOARD: i32 = 0xC0A1_E5CE_u32.cast_signed();

/// Bound on anything the test waits for.
const WAIT: Duration = Duration::from_secs(10);

const CHALLENGE: [u8; 16] = [
    0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff,
];
/// The response for "Secret12", computed with openssl.
const RESPONSE: [u8; 16] = [
    0xee, 0xe9, 0x81, 0xe2, 0x74, 0x19, 0x66, 0x45, 0xd7, 0x10, 0xe9, 0xd9, 0x1f, 0xf5, 0xf5, 0xe8,
];

/// What the client sends between `ServerInit` and the first update: `SetPixelFormat` (20),
/// `SetEncodings` of 11 (4 + 44) and a `FramebufferUpdateRequest` (10).
const OPENING_REQUESTS: usize = 20 + 48 + 10;
/// An incremental `FramebufferUpdateRequest`.
const UPDATE_REQUEST: usize = 10;

fn config(port: u16, policy: SecurityPolicy) -> VncConfig {
    VncConfig {
        host: "127.0.0.1".to_owned(),
        port,
        policy,
        // Trusting what the system trusts only: no server here starts TLS.
        tls: heimdall_tls::connector(
            Arc::new(|_, _| heimdall_tls::UserVerdict::Untrusted),
            heimdall_tls::PresentedSlot::default(),
        ),
        connect_timeout: WAIT,
        handshake_timeout: WAIT,
    }
}

async fn read_exactly(stream: &mut TcpStream, count: usize) -> Vec<u8> {
    let mut bytes = vec![0; count];
    tokio::time::timeout(WAIT, stream.read_exact(&mut bytes))
        .await
        .expect("in time")
        .expect("read");
    bytes
}

/// `SetEncodings` of `encodings`.
fn set_encodings(encodings: &[i32]) -> Vec<u8> {
    let mut bytes = vec![2, 0];
    bytes.extend_from_slice(&u16::try_from(encodings.len()).expect("few").to_be_bytes());
    for encoding in encodings {
        bytes.extend_from_slice(&encoding.to_be_bytes());
    }
    bytes
}

/// A non-incremental `FramebufferUpdateRequest` for the whole 4 by 2 desktop.
const FULL_UPDATE_REQUEST: [u8; 10] = [3, 0, 0, 0, 0, 0, 0, 4, 0, 2];

/// Plays a TigerVNC-like server with VNC Authentication up to the opened session on a 4 by 2
/// desktop; what the client sent once it opened.
async fn serve_handshake(stream: &mut TcpStream) -> Vec<u8> {
    stream.write_all(b"RFB 003.008\n").await.expect("version");
    assert_eq!(read_exactly(stream, 12).await, b"RFB 003.008\n");
    stream.write_all(&[1, 2]).await.expect("types");
    assert_eq!(read_exactly(stream, 1).await, [2]);
    stream.write_all(&CHALLENGE).await.expect("challenge");
    assert_eq!(read_exactly(stream, 16).await, RESPONSE);
    stream.write_all(&[0, 0, 0, 0]).await.expect("result");
    assert_eq!(read_exactly(stream, 1).await, [1], "ClientInit");
    let mut init = vec![0, 4, 0, 2];
    init.extend_from_slice(&[32, 24, 0, 1, 0, 255, 0, 255, 0, 255, 16, 8, 0, 0, 0, 0]);
    init.extend_from_slice(&[0, 0, 0, 4]);
    init.extend_from_slice(b"desk");
    stream.write_all(&init).await.expect("init");
    read_exactly(stream, OPENING_REQUESTS).await
}

async fn listener() -> (TcpListener, u16) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let port = listener.local_addr().expect("address").port();
    (listener, port)
}

async fn next_event(session: &mut VncSession) -> VncEvent {
    tokio::time::timeout(WAIT, session.events.recv())
        .await
        .expect("in time")
        .expect("an event")
}

#[tokio::test]
async fn a_session_opens_draws_an_update_and_carries_a_key() {
    let (listener, port) = listener().await;
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("accepted");
        serve_handshake(&mut stream).await;
        // One raw green pixel at 3,1.
        let mut update = vec![0, 0, 0, 1, 0, 3, 0, 1, 0, 1, 0, 1, 0, 0, 0, 0];
        update.extend_from_slice(&[0, 255, 0, 0]);
        stream.write_all(&update).await.expect("update");
        let request = read_exactly(&mut stream, UPDATE_REQUEST).await;
        let key = read_exactly(&mut stream, 8).await;
        (request, key)
    });

    let cancel = CancellationToken::new();
    let connection = connect(
        &config(port, SecurityPolicy::default()),
        given_password(Zeroizing::new("Secret12".to_owned())),
        &cancel,
    )
    .await
    .expect("connected");
    assert_eq!(connection.name, "desk");
    let mut session = start(connection, cancel);
    let area = Rect {
        x: 3,
        y: 1,
        width: 1,
        height: 1,
    };
    assert_eq!(next_event(&mut session).await, VncEvent::Updated(area));
    let green = session.framebuffer.read(|width, _, pixels| {
        let at = (usize::from(width) + 3) * 4;
        <[u8; 4]>::try_from(&pixels[at..at + 4]).expect("pixel")
    });
    assert_eq!(green, [0, 255, 0, 255]);
    session.input.key(0x61, true).expect("key");
    let (request, key) = server.await.expect("server");
    assert_eq!(
        request,
        [3, 1, 0, 0, 0, 0, 0, 4, 0, 2],
        "incremental, whole desktop"
    );
    assert_eq!(key, [4, 1, 0, 0, 0, 0, 0, 0x61]);
}

#[tokio::test]
async fn tight_is_asked_first_and_a_new_quality_asks_its_levels_then_the_whole_desktop() {
    let (listener, port) = listener().await;
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("accepted");
        let opening = serve_handshake(&mut stream).await;
        // Best: 10 encodings, then the request; Balanced: 11, then the request.
        let best = read_exactly(&mut stream, 4 + 40 + 10).await;
        let balanced = read_exactly(&mut stream, 4 + 44 + 10).await;
        (opening, best, balanced)
    });
    let cancel = CancellationToken::new();
    let connection = connect(
        &config(port, SecurityPolicy::default()),
        given_password(Zeroizing::new("Secret12".to_owned())),
        &cancel,
    )
    .await
    .expect("connected");
    let session = start(connection, cancel);
    session.input.set_quality(Quality::Best).expect("best");
    // The quality asked already sends nothing: the next bytes are Balanced's.
    session.input.set_quality(Quality::Best).expect("again");
    session
        .input
        .set_quality(Quality::Balanced)
        .expect("balanced");
    let (opening, best, balanced) = server.await.expect("server");
    // Tight, ZRLE, CopyRect, Raw, the pseudo-encodings, compression 6 and JPEG quality 6:
    // the C# default "Performance".
    assert_eq!(
        opening[20..68],
        set_encodings(&[
            7,
            16,
            1,
            0,
            -223,
            -224,
            -308,
            -307,
            EXTENDED_CLIPBOARD,
            -250,
            -26
        ])
    );
    // Best: compression 0 and no JPEG quality level, so a Tight server sends no JPEG.
    let mut expected = set_encodings(&[
        7,
        16,
        1,
        0,
        -223,
        -224,
        -308,
        -307,
        EXTENDED_CLIPBOARD,
        -256,
    ]);
    expected.extend_from_slice(&FULL_UPDATE_REQUEST);
    assert_eq!(best, expected);
    // Balanced: compression 3, JPEG quality 7.
    let mut expected = set_encodings(&[
        7,
        16,
        1,
        0,
        -223,
        -224,
        -308,
        -307,
        EXTENDED_CLIPBOARD,
        -253,
        -25,
    ]);
    expected.extend_from_slice(&FULL_UPDATE_REQUEST);
    assert_eq!(balanced, expected);
}

#[tokio::test]
async fn pointer_moves_queued_together_are_merged_around_other_input() {
    let (listener, port) = listener().await;
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("accepted");
        serve_handshake(&mut stream).await;
        // A move, the key, a move: 6 + 8 + 6 bytes.
        read_exactly(&mut stream, 20).await
    });
    let cancel = CancellationToken::new();
    let connection = connect(
        &config(port, SecurityPolicy::default()),
        given_password(Zeroizing::new("Secret12".to_owned())),
        &cancel,
    )
    .await
    .expect("connected");
    let session = start(connection, cancel);
    // All queued before the session task runs again: this runtime has one thread.
    for x in 0..50 {
        session.input.pointer(0, x, 1).expect("move");
    }
    session.input.key(0x61, true).expect("key");
    for x in 50..100 {
        session.input.pointer(0, x, 2).expect("move");
    }
    let sent = server.await.expect("server");
    assert_eq!(
        sent,
        [
            5, 0, 0, 49, 0, 1, // the last of the first 50 moves
            4, 1, 0, 0, 0, 0, 0, 0x61, // the key, in its place
            5, 0, 0, 99, 0, 2, // the last of the next 50
        ]
    );
}

#[tokio::test]
async fn the_password_is_asked_only_when_the_server_requires_one() {
    let (listener, port) = listener().await;
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("accepted");
        stream.write_all(b"RFB 003.008\n").await.expect("version");
        let _ = read_exactly(&mut stream, 12).await;
        stream.write_all(&[1, 1]).await.expect("types");
        let _ = read_exactly(&mut stream, 1).await;
        stream.write_all(&[0, 0, 0, 0]).await.expect("result");
        let _ = read_exactly(&mut stream, 1).await;
        let mut init = vec![0, 1, 0, 1];
        init.extend_from_slice(&[32, 24, 0, 1, 0, 255, 0, 255, 0, 255, 16, 8, 0, 0, 0, 0]);
        init.extend_from_slice(&[0, 0, 0, 0]);
        stream.write_all(&init).await.expect("init");
        let _ = read_exactly(&mut stream, OPENING_REQUESTS).await;
    });
    let asked = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&asked);
    let password: AskPassword = Box::new(move || {
        flag.store(true, Ordering::SeqCst);
        Box::pin(std::future::ready(None))
    });
    let policy = SecurityPolicy {
        allow_no_authentication: true,
        ..SecurityPolicy::default()
    };
    connect(&config(port, policy), password, &CancellationToken::new())
        .await
        .expect("connected");
    server.await.expect("server");
    assert!(!asked.load(Ordering::SeqCst));
}

#[tokio::test]
async fn declining_the_password_cancels_the_connection() {
    let (listener, port) = listener().await;
    let _server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("accepted");
        stream.write_all(b"RFB 003.008\n").await.expect("version");
        let _ = read_exactly(&mut stream, 12).await;
        stream.write_all(&[1, 2]).await.expect("types");
        let _ = read_exactly(&mut stream, 1).await;
        stream.write_all(&CHALLENGE).await.expect("challenge");
        tokio::time::sleep(WAIT).await;
    });
    let password: AskPassword = Box::new(|| Box::pin(std::future::ready(None)));
    let outcome = connect(
        &config(port, SecurityPolicy::default()),
        password,
        &CancellationToken::new(),
    )
    .await;
    assert!(matches!(outcome, Err(VncError::Cancelled)), "{outcome:?}");
}

#[tokio::test]
async fn cancelling_ends_a_session_whose_events_nobody_reads() {
    let (listener, port) = listener().await;
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("accepted");
        serve_handshake(&mut stream).await;
        // Bells without end, until the client goes away.
        let bells = vec![2; 4096];
        loop {
            if stream.write_all(&bells).await.is_err() {
                return;
            }
        }
    });
    let cancel = CancellationToken::new();
    let connection = connect(
        &config(port, SecurityPolicy::default()),
        given_password(Zeroizing::new("Secret12".to_owned())),
        &cancel,
    )
    .await
    .expect("connected");
    let _session = start(connection, cancel.clone());
    tokio::time::sleep(Duration::from_millis(300)).await;
    cancel.cancel();
    tokio::time::timeout(WAIT, server)
        .await
        .expect("the session ended although its queue was full")
        .expect("server");
}

/// A cut text message of type `kind` carrying the extended clipboard `message`.
fn extended(kind: u8, message: &[u8]) -> Vec<u8> {
    let size = -i32::try_from(message.len()).expect("short");
    let mut bytes = vec![kind, 0, 0, 0];
    bytes.extend_from_slice(&size.to_be_bytes());
    bytes.extend_from_slice(message);
    bytes
}

#[tokio::test]
async fn the_clipboard_goes_both_ways_in_utf_8_through_the_extended_clipboard() {
    use std::io::Read as _;

    let (listener, port) = listener().await;
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("accepted");
        serve_handshake(&mut stream).await;
        // Caps: every action, text up to 10 MiB.
        let caps = [0x1F, 0, 0, 0x01, 0x00, 0xA0, 0x00, 0x00];
        stream.write_all(&extended(3, &caps)).await.expect("caps");
        let client_caps = read_exactly(&mut stream, 8 + 8).await;
        // The server's clipboard changed: notify, then provide what is asked.
        stream
            .write_all(&extended(3, &[0x08, 0, 0, 0x01]))
            .await
            .expect("notify");
        let request = read_exactly(&mut stream, 8 + 4).await;
        // One stored zlib block, by hand from RFC 1950 and 1951: the length 6, then e acute,
        // the euro sign and the NUL in UTF-8, then their Adler-32 (computed with Python).
        let mut provide = vec![0x10, 0, 0, 0x01, 0x78, 0x01, 0x01, 10, 0, 0xF5, 0xFF];
        provide.extend_from_slice(&[0, 0, 0, 6, 0xC3, 0xA9, 0xE2, 0x82, 0xAC, 0]);
        provide.extend_from_slice(&0x0E79_0383_u32.to_be_bytes());
        stream
            .write_all(&extended(3, &provide))
            .await
            .expect("provide");
        // The client's clipboard: notified, then provided once asked.
        let notify = read_exactly(&mut stream, 8 + 4).await;
        stream
            .write_all(&extended(3, &[0x02, 0, 0, 0x01]))
            .await
            .expect("request");
        let header = read_exactly(&mut stream, 8).await;
        let size = i32::from_be_bytes([header[4], header[5], header[6], header[7]]);
        let message = read_exactly(&mut stream, usize::try_from(-size).expect("negative")).await;
        (client_caps, request, notify, header, message)
    });
    let cancel = CancellationToken::new();
    let connection = connect(
        &config(port, SecurityPolicy::default()),
        given_password(Zeroizing::new("Secret12".to_owned())),
        &cancel,
    )
    .await
    .expect("connected");
    let mut session = start(connection, cancel.clone());
    assert_eq!(
        next_event(&mut session).await,
        VncEvent::CutText("\u{e9}\u{20ac}".to_owned())
    );
    session
        .input
        .cut_text("\u{fc}\u{1f600}".to_owned())
        .expect("cut text");
    let (client_caps, request, notify, header, message) = tokio::time::timeout(WAIT, server)
        .await
        .expect("in time")
        .expect("server");
    cancel.cancel();
    assert_eq!(
        client_caps,
        extended(6, &[0x1F, 0, 0, 0x01, 0x00, 0x10, 0x00, 0x00])
    );
    assert_eq!(request, extended(6, &[0x02, 0, 0, 0x01]));
    assert_eq!(notify, extended(6, &[0x08, 0, 0, 0x01]));
    assert_eq!(header[..4], [6, 0, 0, 0]);
    assert_eq!(message[..4], [0x10, 0, 0, 0x01]);
    let mut inflated = Vec::new();
    flate2::read::ZlibDecoder::new(&message[4..])
        .read_to_end(&mut inflated)
        .expect("a zlib stream");
    assert_eq!(
        inflated,
        [0, 0, 0, 7, 0xC3, 0xBC, 0xF0, 0x9F, 0x98, 0x80, 0]
    );
}

/// A Tight capability: a code, a vendor and a signature.
fn capability(code: u32, vendor: [u8; 4], signature: [u8; 8]) -> Vec<u8> {
    let mut bytes = code.to_be_bytes().to_vec();
    bytes.extend_from_slice(&vendor);
    bytes.extend_from_slice(&signature);
    bytes
}

/// Plays the security of a server wrapping VNC Authentication in `wrapper`, then the rest
/// of a session on a 4 by 2 desktop up to one raw green pixel at 3,1; what the client sent
/// for the security.
async fn serve_wrapped(stream: &mut TcpStream, wrapper: SecurityWrapper) -> Vec<u8> {
    stream.write_all(b"RFB 003.008\n").await.expect("version");
    assert_eq!(read_exactly(stream, 12).await, b"RFB 003.008\n");
    stream.write_all(&[1, wrapper.code()]).await.expect("types");
    let sent = match wrapper {
        SecurityWrapper::Tight => {
            // One tunnel, no tunnel; then no authentication and VNC Authentication.
            let mut tunnels = vec![0, 0, 0, 1];
            tunnels.extend(capability(0, *b"TGHT", *b"NOTUNNEL"));
            stream.write_all(&tunnels).await.expect("tunnels");
            let mut types = vec![0, 0, 0, 2];
            types.extend(capability(1, *b"STDV", *b"NOAUTH__"));
            types.extend(capability(2, *b"STDV", *b"VNCAUTH_"));
            stream
                .write_all(&types)
                .await
                .expect("authentication types");
            read_exactly(stream, 1 + 4 + 4).await
        }
        SecurityWrapper::VeNCrypt => {
            stream.write_all(&[0, 2]).await.expect("version");
            let mut sent = read_exactly(stream, 1 + 2).await;
            // Accepted; then Plain, X509Plain and VNC Authentication.
            stream
                .write_all(&[0, 3, 0, 0, 1, 0, 0, 0, 1, 6, 0, 0, 0, 2])
                .await
                .expect("subtypes");
            sent.extend(read_exactly(stream, 4).await);
            sent
        }
    };
    stream.write_all(&CHALLENGE).await.expect("challenge");
    assert_eq!(read_exactly(stream, 16).await, RESPONSE);
    stream.write_all(&[0, 0, 0, 0]).await.expect("result");
    assert_eq!(read_exactly(stream, 1).await, [1], "ClientInit");
    let mut init = vec![0, 4, 0, 2];
    init.extend_from_slice(&[32, 24, 0, 1, 0, 255, 0, 255, 0, 255, 16, 8, 0, 0, 0, 0]);
    init.extend_from_slice(&[0, 0, 0, 4]);
    init.extend_from_slice(b"desk");
    if wrapper == SecurityWrapper::Tight {
        // One encoding capability.
        init.extend_from_slice(&[0, 0, 0, 0, 0, 1, 0, 0]);
        init.extend(capability(7, *b"TGHT", *b"TIGHT___"));
    }
    stream.write_all(&init).await.expect("init");
    let _ = read_exactly(stream, OPENING_REQUESTS).await;
    let mut update = vec![0, 0, 0, 1, 0, 3, 0, 1, 0, 1, 0, 1, 0, 0, 0, 0];
    update.extend_from_slice(&[0, 255, 0, 0]);
    stream.write_all(&update).await.expect("update");
    sent
}

/// Opens a session through `wrapper` and checks it draws; `expected` is what the client
/// sends for the security.
async fn opens_through(wrapper: SecurityWrapper, expected: &[u8]) {
    let (listener, port) = listener().await;
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("accepted");
        serve_wrapped(&mut stream, wrapper).await
    });
    let cancel = CancellationToken::new();
    let connection = connect(
        &config(port, SecurityPolicy::default()),
        given_password(Zeroizing::new("Secret12".to_owned())),
        &cancel,
    )
    .await
    .expect("connected");
    assert_eq!(connection.name, "desk");
    assert_eq!(
        connection.security(),
        Some(Security {
            wrapper: Some(wrapper),
            authentication: Authentication::VncAuth,
            tls: false,
        })
    );
    let mut session = start(connection, cancel.clone());
    let area = Rect {
        x: 3,
        y: 1,
        width: 1,
        height: 1,
    };
    assert_eq!(next_event(&mut session).await, VncEvent::Updated(area));
    assert_eq!(server.await.expect("server"), expected);
    cancel.cancel();
}

#[tokio::test]
async fn a_session_opens_through_tight_with_no_tunnel_and_vnc_authentication() {
    // Tight, no tunnel, VNC Authentication.
    opens_through(SecurityWrapper::Tight, &[16, 0, 0, 0, 0, 0, 0, 0, 2]).await;
}

#[tokio::test]
async fn a_session_opens_through_vencrypt_0_2_and_vnc_authentication() {
    // VeNCrypt, version 0.2, VNC Authentication.
    opens_through(SecurityWrapper::VeNCrypt, &[19, 0, 2, 0, 0, 0, 2]).await;
}

/// The lab's `TigerVNC` (Heimdall-TestEnv): the session opens on its 1280 by 800 desktop and
/// the first update covers all of it.
#[tokio::test]
async fn a_real_server_opens_and_draws_its_desktop() {
    let (Some(port), Some(password)) = (
        std::env::var("HEIMDALL_LIVE_VNC_PORT")
            .ok()
            .and_then(|port| port.parse::<u16>().ok()),
        std::env::var("HEIMDALL_LIVE_VNC_PASSWORD").ok(),
    ) else {
        eprintln!("HEIMDALL_LIVE_VNC_PORT or HEIMDALL_LIVE_VNC_PASSWORD not set; skipped");
        return;
    };
    let cancel = CancellationToken::new();
    let connection = connect(
        &config(port, SecurityPolicy::default()),
        given_password(Zeroizing::new(password)),
        &cancel,
    )
    .await
    .expect("connected");
    let mut session = start(connection, cancel);
    let size = session.framebuffer.read(|width, height, _| (width, height));
    assert_eq!(size, (1280, 800));
    // The first full update covers the whole desktop, every rectangle of it decoded. (Whether
    // the pixels are right is for the decoder tests; against this server, the ZRLE result was
    // also compared pixel for pixel with a Raw-only client on 2026-09-27: identical.)
    let covered = tokio::time::timeout(Duration::from_secs(20), async {
        let mut covered = 0_usize;
        while covered < 1280 * 800 {
            match session.events.recv().await {
                Some(VncEvent::Updated(rect)) => covered += rect.area(),
                Some(VncEvent::Closed(reason)) => panic!("closed: {reason:?}"),
                Some(_) => {}
                None => panic!("no more events"),
            }
        }
        covered
    })
    .await
    .expect("the whole desktop in time");
    assert!(covered >= 1280 * 800);
    session.input.close();
    loop {
        if let VncEvent::Closed(reason) = next_event(&mut session).await {
            assert_eq!(reason, CloseReason::Local);
            break;
        }
    }
}
