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

//! A Telnet session over TCP, against a fake server on this machine that negotiates as a
//! Linux `telnetd` does: window size and echo, then a login prompt.

use std::time::Duration;

use heimdall_remote::telnet::{
    CloseReason, TelnetConfig, TelnetError, TelnetEvent, TelnetSession, WindowSize, connect,
};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::{TcpListener, TcpStream};
use tokio_util::sync::CancellationToken;

const IAC: u8 = 255;
const DO: u8 = 253;
const WILL: u8 = 251;
const SB: u8 = 250;
const SE: u8 = 240;
const ECHO: u8 = 1;
const NAWS: u8 = 31;

/// Bound on anything the test waits for.
const WAIT: Duration = Duration::from_secs(10);

const SIZE: WindowSize = WindowSize {
    columns: 80,
    rows: 24,
};

fn config(port: u16) -> TelnetConfig {
    TelnetConfig {
        host: "127.0.0.1".to_owned(),
        port,
        size: SIZE,
        connect_timeout: WAIT,
    }
}

/// Reads from `stream` until `expected` bytes arrived.
async fn read_exactly(stream: &mut TcpStream, expected: usize) -> Vec<u8> {
    let mut bytes = vec![0; expected];
    tokio::time::timeout(WAIT, stream.read_exact(&mut bytes))
        .await
        .expect("in time")
        .expect("read");
    bytes
}

/// Everything the session reports until it closes.
async fn drain(session: &mut TelnetSession) -> (Vec<u8>, CloseReason) {
    let mut output = Vec::new();
    loop {
        match tokio::time::timeout(WAIT, session.events.recv())
            .await
            .expect("in time")
            .expect("an event")
        {
            TelnetEvent::Output(bytes) => output.extend(bytes),
            TelnetEvent::Closed(reason) => return (output, reason),
        }
    }
}

#[tokio::test]
async fn a_session_negotiates_logs_in_and_ends_when_the_server_closes() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let port = listener.local_addr().expect("address").port();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("accepted");
        stream
            .write_all(&[IAC, DO, NAWS, IAC, WILL, ECHO])
            .await
            .expect("offers");
        stream.write_all(b"login: ").await.expect("prompt");
        // WILL NAWS, the size, DO ECHO.
        let answer = read_exactly(&mut stream, 3 + 9 + 3).await;
        // "root" and Enter, a lone CR sent as CR NUL.
        let typed = read_exactly(&mut stream, 6).await;
        stream.write_all(b"welcome\r\n").await.expect("welcome");
        (answer, typed)
    });

    let mut session = connect(&config(port), CancellationToken::new())
        .await
        .expect("connected");
    // Typed once the prompt shows, as a person would: the answers to the offers come first.
    let mut output = Vec::new();
    while !output.ends_with(b"login: ") {
        match tokio::time::timeout(WAIT, session.events.recv())
            .await
            .expect("in time")
            .expect("an event")
        {
            TelnetEvent::Output(bytes) => output.extend(bytes),
            TelnetEvent::Closed(reason) => panic!("closed early: {reason:?}"),
        }
    }
    session.input.write(b"root\r".to_vec()).expect("typed");
    let (rest, reason) = drain(&mut session).await;
    output.extend(rest);
    let (answer, typed) = server.await.expect("server");

    assert_eq!(
        answer,
        [
            IAC, WILL, NAWS, IAC, SB, NAWS, 0, 80, 0, 24, IAC, SE, IAC, DO, ECHO
        ]
    );
    assert_eq!(typed, b"root\r\0");
    assert_eq!(output, b"login: welcome\r\n");
    assert_eq!(reason, CloseReason::Server);
}

#[tokio::test]
async fn a_resize_reaches_a_server_that_asked_for_sizes() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let port = listener.local_addr().expect("address").port();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("accepted");
        stream.write_all(&[IAC, DO, NAWS]).await.expect("offer");
        let _ = read_exactly(&mut stream, 3 + 9).await;
        stream.write_all(b"ready").await.expect("ready");
        // Then the new size, and nothing else.
        read_exactly(&mut stream, 9).await
    });

    let mut session = connect(&config(port), CancellationToken::new())
        .await
        .expect("connected");
    // The server has its answer once it says so.
    let ready = tokio::time::timeout(WAIT, session.events.recv())
        .await
        .expect("in time");
    assert_eq!(ready, Some(TelnetEvent::Output(b"ready".to_vec())));
    session
        .input
        .resize(WindowSize {
            columns: 132,
            rows: 43,
        })
        .expect("resized");
    assert_eq!(
        server.await.expect("server"),
        [IAC, SB, NAWS, 0, 132, 0, 43, IAC, SE]
    );
}

#[tokio::test]
async fn closing_ends_the_session_on_both_sides() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let port = listener.local_addr().expect("address").port();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("accepted");
        let mut rest = Vec::new();
        tokio::time::timeout(WAIT, stream.read_to_end(&mut rest))
            .await
            .expect("in time")
            .expect("read");
        rest
    });

    let mut session = connect(&config(port), CancellationToken::new())
        .await
        .expect("connected");
    session.input.close();
    let (_, reason) = drain(&mut session).await;
    assert_eq!(reason, CloseReason::Local);
    // The server sees the connection end, with nothing sent.
    assert!(server.await.expect("server").is_empty());
    assert!(session.input.write(b"late".to_vec()).is_err());
}

#[tokio::test]
async fn a_closed_port_is_a_network_error() {
    // Bound but not listening: the port stays this test's, and refuses. A listener
    // dropped instead frees it, for a test running beside this one to take.
    let socket = tokio::net::TcpSocket::new_v4().expect("socket");
    socket
        .bind("127.0.0.1:0".parse().expect("address"))
        .expect("bind");
    let port = socket.local_addr().expect("address").port();
    let outcome = connect(&config(port), CancellationToken::new()).await;
    assert!(
        matches!(outcome, Err(TelnetError::Network(_))),
        "{outcome:?}"
    );
}

#[tokio::test]
async fn cancelling_ends_a_session_whose_output_nobody_reads() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let port = listener.local_addr().expect("address").port();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("accepted");
        // Output without end, until the client goes away.
        let chunk = vec![b'x'; 4096];
        loop {
            if stream.write_all(&chunk).await.is_err() {
                return;
            }
        }
    });

    let cancel = CancellationToken::new();
    // Nothing reads the events: the queue fills and the session waits on it.
    let _session = connect(&config(port), cancel.clone())
        .await
        .expect("connected");
    tokio::time::sleep(Duration::from_millis(300)).await;
    cancel.cancel();
    // The session lets go of the connection, so the server's writes fail and it returns.
    tokio::time::timeout(WAIT, server)
        .await
        .expect("the session ended although its queue was full")
        .expect("server");
}
