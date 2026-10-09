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

//! X11 forwarding: asked by a shell alone, each X11 channel checked against the fake cookie
//! the server was given, then carried to the display with the real one. A stand-in X server
//! listens on a loopback port: display `n` is TCP port 6000 plus `n`.

mod common;

use std::net::Ipv4Addr;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use common::{
    LOOPBACK, MIT_MAGIC_COOKIE, PASSWORD, STEP_TIMEOUT, ScriptedPrompter, Spec, TestServer,
    X11_CLIENT_BYTES, X11_LSB_FIRST, X11_MSB_FIRST, X11Open, host_public_key, options_empty,
    profile, start, x11_setup,
};
use heimdall_ssh::{
    ConnectOptions, Connection, KnownHosts, X11Display, establish, establish_via_keeping_gateway,
};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio_util::sync::CancellationToken;

/// What the stand-in X server answers on each connection, as replies and events.
const X11_SERVER_BYTES: &[u8] = b"from-the-x-server";

/// TCP port of display 0.
const DISPLAY_PORT_BASE: u16 = 6000;

/// Longest the tests wait for the server to have opened its channels.
const SETTLE: Duration = Duration::from_secs(10);

/// A real cookie of the display, as the Xauthority file holds it.
const REAL_COOKIE: [u8; 16] = [0xAB; 16];

/// The Xauthority family matching any address.
const FAMILY_WILD: u16 = 65535;

/// X11 channels of a connection that may wait for their setup at once, as the client caps
/// them.
const MAX_PENDING_SETUPS: usize = 32;

/// Silent channels the server opens past that cap.
const PAST_THE_CAP: usize = 8;

/// A stand-in X server: what each connection sent until its end, in order.
struct FakeXServer {
    port: u16,
    received: Arc<Mutex<Vec<Vec<u8>>>>,
}

impl FakeXServer {
    async fn start() -> Self {
        let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .expect("bound");
        let port = listener.local_addr().expect("address").port();
        let received = Arc::new(Mutex::new(Vec::new()));
        let kept = received.clone();
        tokio::spawn(async move {
            while let Ok((mut client, _)) = listener.accept().await {
                let kept = kept.clone();
                tokio::spawn(async move {
                    let mut bytes = Vec::new();
                    let _ = client.read_to_end(&mut bytes).await;
                    kept.lock().expect("received").push(bytes);
                    let _ = client.write_all(X11_SERVER_BYTES).await;
                });
            }
        });
        Self { port, received }
    }

    /// Its display, as `DISPLAY` would name it, with no cookie file.
    fn display(&self) -> X11Display {
        let number = self.port - DISPLAY_PORT_BASE;
        X11Display::parse(&format!("{LOOPBACK}:{number}"))
            .expect("display")
            .with_authority(None)
    }

    fn received(&self) -> Vec<Vec<u8>> {
        self.received.lock().expect("received").clone()
    }
}

fn trusting(dir: &Path, servers: &[&TestServer]) -> ConnectOptions {
    let options = options_empty(dir);
    for server in servers {
        KnownHosts::new(&options.known_hosts)
            .learn(LOOPBACK, server.port, &host_public_key("host-ed25519"))
            .expect("learn");
    }
    options
}

async fn connected(server: &TestServer, options: &ConnectOptions) -> Connection {
    let prompter = Arc::new(ScriptedPrompter::passwords(&[PASSWORD]));
    tokio::time::timeout(
        STEP_TIMEOUT,
        establish(
            &profile(server.port, None),
            options,
            prompter,
            CancellationToken::new(),
        ),
    )
    .await
    .expect("in time")
    .expect("connected")
}

/// Waits until the server has opened `count` X11 channels and heard back from each.
async fn opened(server: &TestServer, count: usize) -> Vec<(bool, Vec<u8>)> {
    let deadline = tokio::time::Instant::now() + SETTLE;
    while tokio::time::Instant::now() < deadline {
        let opens = server.observed.lock().expect("observed").x11_opens.clone();
        if opens.len() >= count {
            return opens;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("the server opened fewer than {count} X11 channels in time");
}

/// What the X server receives for a channel whose setup was in `order`: the setup with the
/// real authorization, then the program's bytes.
fn reaching(order: u8, name: &[u8], data: &[u8]) -> Vec<u8> {
    let mut expected = x11_setup(order, name, data);
    expected.extend(X11_CLIENT_BYTES);
    expected
}

#[tokio::test]
async fn a_shell_that_did_not_ask_has_the_server_s_x11_channel_refused() {
    let server = start(Spec {
        x11_opens: vec![X11Open::Given {
            order: X11_MSB_FIRST,
            name: MIT_MAGIC_COOKIE,
        }],
        ..Spec::default()
    })
    .await;
    let dir = tempfile::tempdir().expect("dir");
    let options = trusting(dir.path(), &[&server]);
    let connection = connected(&server, &options).await;
    let _shell = connection
        .open_shell(&options, CancellationToken::new())
        .await
        .expect("shell");
    assert_eq!(opened(&server, 1).await, [(false, Vec::new())]);
    assert!(
        server
            .observed
            .lock()
            .expect("observed")
            .x11_requests
            .is_empty()
    );
}

#[tokio::test]
async fn the_fake_cookie_in_either_byte_order_reaches_the_display_with_the_real_one() {
    let display = FakeXServer::start().await;
    let given = |order| X11Open::Given {
        order,
        name: MIT_MAGIC_COOKIE,
    };
    let server = start(Spec {
        x11_opens: vec![given(X11_MSB_FIRST), given(X11_LSB_FIRST)],
        ..Spec::default()
    })
    .await;
    let dir = tempfile::tempdir().expect("dir");
    let mut options = trusting(dir.path(), &[&server]);
    options.x11 = Some(display.display());
    let connection = connected(&server, &options).await;
    let _shell = connection
        .open_shell(&options, CancellationToken::new())
        .await
        .expect("shell");

    assert_eq!(
        opened(&server, 2).await,
        [
            (true, X11_SERVER_BYTES.to_vec()),
            (true, X11_SERVER_BYTES.to_vec())
        ],
        "both taken, the X server's answer back through each"
    );
    let requests = server
        .observed
        .lock()
        .expect("observed")
        .x11_requests
        .clone();
    let [(single, protocol, cookie, screen)] = requests.as_slice() else {
        panic!("{requests:?}");
    };
    assert!(!single, "every program of the shell");
    assert_eq!(protocol.as_bytes(), MIT_MAGIC_COOKIE);
    assert_eq!(*screen, 0);
    assert_eq!(cookie.len(), 32, "16 bytes, in hexadecimal");
    assert!(
        cookie
            .bytes()
            .all(|digit| digit.is_ascii_digit() || (b'a'..=b'f').contains(&digit))
    );
    // No cookie file: the X server gets no authorization, never the fake cookie.
    assert_eq!(
        display.received(),
        [
            reaching(X11_MSB_FIRST, b"", b""),
            reaching(X11_LSB_FIRST, b"", b"")
        ]
    );
}

#[tokio::test]
async fn another_cookie_protocol_or_a_setup_oversize_or_cut_short_never_reaches_the_display() {
    let display = FakeXServer::start().await;
    let refused = vec![
        X11Open::Other { data: vec![0; 16] },
        // The fake cookie but its last byte, then with one byte more.
        X11Open::Tampered { longer: false },
        X11Open::Tampered { longer: true },
        X11Open::Given {
            order: X11_MSB_FIRST,
            name: b"XDM-AUTHORIZATION-1",
        },
        X11Open::Oversize {
            name_length: 1000,
            data_length: 0,
        },
        X11Open::Oversize {
            name_length: 18,
            data_length: 1000,
        },
        X11Open::Cut { keep: 20 },
        X11Open::Cut { keep: 5 },
    ];
    let count = refused.len();
    let server = start(Spec {
        x11_opens: refused,
        ..Spec::default()
    })
    .await;
    let dir = tempfile::tempdir().expect("dir");
    let mut options = trusting(dir.path(), &[&server]);
    options.x11 = Some(display.display());
    let connection = connected(&server, &options).await;
    let _shell = connection
        .open_shell(&options, CancellationToken::new())
        .await
        .expect("shell");

    // Taken, then closed with nothing back: the setup is read before anything is sent on.
    assert_eq!(
        opened(&server, count).await,
        vec![(true, Vec::new()); count]
    );
    assert!(display.received().is_empty(), "{:?}", display.received());
}

/// An Xauthority file holding `cookie` for display `number`, any address.
fn xauthority(dir: &Path, number: u16, cookie: &[u8]) -> std::path::PathBuf {
    let mut bytes = FAMILY_WILD.to_be_bytes().to_vec();
    let number = number.to_string();
    for field in [&b""[..], number.as_bytes(), MIT_MAGIC_COOKIE, cookie] {
        bytes.extend(u16::try_from(field.len()).expect("short").to_be_bytes());
        bytes.extend(field);
    }
    let path = dir.join("Xauthority");
    std::fs::write(&path, bytes).expect("written");
    path
}

#[tokio::test]
async fn the_display_s_cookie_from_its_xauthority_file_replaces_the_fake_one() {
    let display = FakeXServer::start().await;
    let server = start(Spec {
        x11_opens: vec![X11Open::Given {
            order: X11_LSB_FIRST,
            name: MIT_MAGIC_COOKIE,
        }],
        ..Spec::default()
    })
    .await;
    let dir = tempfile::tempdir().expect("dir");
    let file = xauthority(dir.path(), display.port - DISPLAY_PORT_BASE, &REAL_COOKIE);
    let mut options = trusting(dir.path(), &[&server]);
    options.x11 = Some(display.display().with_authority(Some(file)));
    let connection = connected(&server, &options).await;
    let _shell = connection
        .open_shell(&options, CancellationToken::new())
        .await
        .expect("shell");

    assert_eq!(
        opened(&server, 1).await,
        [(true, X11_SERVER_BYTES.to_vec())]
    );
    assert_eq!(
        display.received(),
        [reaching(X11_LSB_FIRST, MIT_MAGIC_COOKIE, &REAL_COOKIE)]
    );
}

#[tokio::test]
async fn a_gateway_s_x11_channel_is_refused_while_the_server_s_shell_forwards_x11() {
    let display = FakeXServer::start().await;
    let given = || X11Open::Given {
        order: X11_MSB_FIRST,
        name: MIT_MAGIC_COOKIE,
    };
    let gateway = start(Spec {
        forwarding: true,
        x11_opens: vec![given()],
        ..Spec::default()
    })
    .await;
    let server = start(Spec {
        x11_opens: vec![given()],
        ..Spec::default()
    })
    .await;
    let dir = tempfile::tempdir().expect("dir");
    let mut options = trusting(dir.path(), &[&gateway, &server]);
    options.x11 = Some(display.display());
    let prompter = Arc::new(ScriptedPrompter::passwords(&[PASSWORD, PASSWORD]));
    let routed = tokio::time::timeout(
        STEP_TIMEOUT,
        establish_via_keeping_gateway(
            &[profile(gateway.port, None)],
            &profile(server.port, None),
            &options,
            prompter,
            CancellationToken::new(),
        ),
    )
    .await
    .expect("in time")
    .expect("connected");
    let _shell = routed
        .server
        .open_shell(&options, CancellationToken::new())
        .await
        .expect("shell");
    assert_eq!(
        opened(&server, 1).await,
        [(true, X11_SERVER_BYTES.to_vec())]
    );

    // A shell on the gateway, which never asked: its server's X11 channel is refused.
    options.x11 = None;
    let _gateway_shell = routed
        .gateway
        .as_ref()
        .expect("gateway")
        .open_shell(&options, CancellationToken::new())
        .await
        .expect("gateway shell");
    assert_eq!(opened(&gateway, 1).await, [(false, Vec::new())]);
    assert!(
        gateway
            .observed
            .lock()
            .expect("observed")
            .x11_requests
            .is_empty()
    );
    assert_eq!(display.received().len(), 1, "the server's channel alone");
}

#[tokio::test]
async fn bytes_sent_with_the_setup_in_one_packet_reach_the_display_intact() {
    let display = FakeXServer::start().await;
    let server = start(Spec {
        x11_opens: vec![X11Open::Joined {
            order: X11_LSB_FIRST,
        }],
        ..Spec::default()
    })
    .await;
    let dir = tempfile::tempdir().expect("dir");
    let mut options = trusting(dir.path(), &[&server]);
    options.x11 = Some(display.display());
    let connection = connected(&server, &options).await;
    let _shell = connection
        .open_shell(&options, CancellationToken::new())
        .await
        .expect("shell");

    assert_eq!(
        opened(&server, 1).await,
        [(true, X11_SERVER_BYTES.to_vec())]
    );
    assert_eq!(display.received(), [reaching(X11_LSB_FIRST, b"", b"")]);
}

#[tokio::test]
async fn an_x11_channel_opened_once_the_asking_shell_closed_is_refused() {
    let display = FakeXServer::start().await;
    let server = start(Spec {
        x11_opens: vec![X11Open::Given {
            order: X11_MSB_FIRST,
            name: MIT_MAGIC_COOKIE,
        }],
        x11_on_close: true,
        ..Spec::default()
    })
    .await;
    let dir = tempfile::tempdir().expect("dir");
    let mut options = trusting(dir.path(), &[&server]);
    options.x11 = Some(display.display());
    let connection = connected(&server, &options).await;
    let shell = connection
        .open_shell(&options, CancellationToken::new())
        .await
        .expect("shell");
    assert_eq!(
        server.observed.lock().expect("observed").x11_requests.len(),
        1
    );
    // The connection stays, its shell gone: the server opens its X11 channel then.
    shell.input.close();
    assert_eq!(opened(&server, 1).await, [(false, Vec::new())]);
    assert!(!connection.is_closed());
    assert!(display.received().is_empty());
}

#[tokio::test]
async fn x11_channels_waiting_for_their_setup_are_capped() {
    let display = FakeXServer::start().await;
    let server = start(Spec {
        x11_opens: vec![X11Open::Silent; MAX_PENDING_SETUPS + PAST_THE_CAP],
        ..Spec::default()
    })
    .await;
    let dir = tempfile::tempdir().expect("dir");
    let mut options = trusting(dir.path(), &[&server]);
    options.x11 = Some(display.display());
    let connection = connected(&server, &options).await;
    let _shell = connection
        .open_shell(&options, CancellationToken::new())
        .await
        .expect("shell");

    let opens = opened(&server, MAX_PENDING_SETUPS + PAST_THE_CAP).await;
    let taken: Vec<bool> = opens.iter().map(|(took, _)| *took).collect();
    let mut expected = vec![true; MAX_PENDING_SETUPS];
    expected.extend([false; PAST_THE_CAP]);
    assert_eq!(taken, expected, "those past the cap refused");
    assert!(display.received().is_empty());
}
