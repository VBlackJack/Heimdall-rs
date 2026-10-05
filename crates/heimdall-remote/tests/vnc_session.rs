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
    AskPassword, CloseReason, Rect, SecurityPolicy, VncConfig, VncError, VncEvent, VncSession,
    connect, given_password, start,
};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::{TcpListener, TcpStream};
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;

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
/// `SetEncodings` of 6 (4 + 24) and a `FramebufferUpdateRequest` (10).
const OPENING_REQUESTS: usize = 20 + 28 + 10;
/// An incremental `FramebufferUpdateRequest`.
const UPDATE_REQUEST: usize = 10;

fn config(port: u16, policy: SecurityPolicy) -> VncConfig {
    VncConfig {
        host: "127.0.0.1".to_owned(),
        port,
        policy,
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

/// Plays a TigerVNC-like server up to the opened session on a 4 by 2 desktop.
async fn serve_handshake(stream: &mut TcpStream) {
    stream.write_all(b"RFB 003.008\n").await.expect("version");
    assert_eq!(read_exactly(stream, 12).await, b"RFB 003.008\n");
    stream.write_all(&[2, 19, 2]).await.expect("types");
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
    let _ = read_exactly(stream, OPENING_REQUESTS).await;
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
