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

//! One connection, several channels: a shell and subsystems share it, each closes only
//! itself, and the connection ends with its last use.

mod common;

use std::sync::Arc;
use std::time::Duration;

use common::{
    DROP_COMMAND, FORWARDED_GREETING, PASSWORD, STEP_TIMEOUT, SUBSYSTEM_ACCEPTED, SUBSYSTEM_SILENT,
    ScriptedPrompter, Spec, TestServer, options_trusting, profile, start,
};
use heimdall_ssh::{ConnectError, Connection, SessionEvent, establish};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio_util::sync::CancellationToken;

/// Longest the tests wait for the server to notice a closed connection.
const SETTLE: Duration = Duration::from_secs(5);

/// Bound on a subsystem answer in these tests.
const ANSWER_TIMEOUT: Duration = Duration::from_millis(500);

async fn connected(server: &TestServer, dir: &std::path::Path) -> Connection {
    let options = options_trusting(dir, server.port, "host-ed25519");
    let prompter = Arc::new(ScriptedPrompter::passwords(&[PASSWORD]));
    tokio::time::timeout(
        STEP_TIMEOUT,
        establish(
            &profile(server.port, None),
            &options,
            prompter,
            CancellationToken::new(),
        ),
    )
    .await
    .expect("in time")
    .expect("connected")
}

async fn echoes(stream: &mut heimdall_ssh::SubsystemStream, text: &[u8]) {
    stream.write_all(text).await.expect("written");
    stream.flush().await.expect("flushed");
    let mut echoed = vec![0; text.len()];
    tokio::time::timeout(STEP_TIMEOUT, stream.read_exact(&mut echoed))
        .await
        .expect("in time")
        .expect("read");
    assert_eq!(echoed, text);
}

async fn connections_ended(server: &TestServer, expected: usize) -> bool {
    let deadline = tokio::time::Instant::now() + SETTLE;
    while tokio::time::Instant::now() < deadline {
        if server.observed.lock().expect("observed").connections_ended == expected {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    false
}

#[tokio::test]
async fn a_subsystem_carries_bytes_both_ways() {
    let server = start(Spec::default()).await;
    let dir = tempfile::tempdir().expect("dir");
    let connection = connected(&server, dir.path()).await;
    let mut stream = connection
        .open_subsystem(SUBSYSTEM_ACCEPTED, ANSWER_TIMEOUT)
        .await
        .expect("accepted");
    echoes(&mut stream, b"ping").await;
}

#[tokio::test]
async fn a_refused_or_unanswered_subsystem_fails_instead_of_hanging() {
    let server = start(Spec::default()).await;
    let dir = tempfile::tempdir().expect("dir");
    let connection = connected(&server, dir.path()).await;
    let refused = connection
        .open_subsystem("nope", ANSWER_TIMEOUT)
        .await
        .expect_err("refused");
    assert!(
        matches!(refused, ConnectError::SubsystemRefused { ref name } if name == "nope"),
        "{refused:?}"
    );
    // Bounded here too: without the library's own timeout this must fail, not hang.
    let silent = tokio::time::timeout(
        STEP_TIMEOUT,
        connection.open_subsystem(SUBSYSTEM_SILENT, ANSWER_TIMEOUT),
    )
    .await
    .expect("the library bounds the wait")
    .expect_err("no answer");
    assert!(matches!(silent, ConnectError::Timeout), "{silent:?}");
    assert!(
        !connection.is_closed(),
        "a refusal does not end the connection"
    );
}

#[tokio::test]
async fn closing_the_shell_leaves_a_subsystem_on_the_same_connection_working() {
    let server = start(Spec::default()).await;
    let dir = tempfile::tempdir().expect("dir");
    let options = options_trusting(dir.path(), server.port, "host-ed25519");
    let connection = connected(&server, dir.path()).await;
    let mut shell = connection
        .open_shell(&options, CancellationToken::new())
        .await
        .expect("shell");
    let mut stream = connection
        .open_subsystem(SUBSYSTEM_ACCEPTED, ANSWER_TIMEOUT)
        .await
        .expect("subsystem");
    drop(connection);

    shell.input.close();
    while let Some(event) = shell.events.recv().await {
        if matches!(event, SessionEvent::Closed { .. }) {
            break;
        }
    }
    echoes(&mut stream, b"still here").await;
    assert_eq!(
        server.observed.lock().expect("observed").connections_ended,
        0,
        "the subsystem keeps the connection"
    );

    drop(stream);
    assert!(
        connections_ended(&server, 1).await,
        "the last use gone, the connection ends"
    );
    // An SSH disconnect, not a dropped socket: russh on the server reports `early eof`
    // for the latter (measured on 2026-09-26). On Windows the server may instead see the
    // connection aborted: a socket closed with unread data (the channel close replies) is
    // reset, and the reset discards the disconnect already received (Windows CI,
    // 2026-09-26). The discriminating check runs where the platform allows it.
    let endings = server.observed.lock().expect("observed").endings.clone();
    if cfg!(windows) {
        assert_eq!(endings.len(), 1, "{endings:?}");
    } else {
        assert_eq!(endings, vec!["Ok(())".to_owned()]);
    }
}

#[tokio::test]
async fn a_connection_the_server_dropped_says_it_ended_without_being_asked() {
    let server = start(Spec::default()).await;
    let dir = tempfile::tempdir().expect("dir");
    let options = options_trusting(dir.path(), server.port, "host-ed25519");
    let connection = connected(&server, dir.path()).await;
    let closed = connection.closed();
    let shell = connection
        .open_shell(&options, CancellationToken::new())
        .await
        .expect("shell");
    // Still open: nothing has ended it.
    assert!(
        tokio::time::timeout(ANSWER_TIMEOUT, connection.closed())
            .await
            .is_err(),
        "an open connection is not ended"
    );
    shell.input.write(DROP_COMMAND.to_vec()).expect("write");
    tokio::time::timeout(STEP_TIMEOUT, closed)
        .await
        .expect("the end is told, not polled for");
    assert!(connection.is_closed());
}

/// Waits until `check` holds on what the server observed.
async fn observed_until(server: &TestServer, check: impl Fn(&common::Observed) -> bool) -> bool {
    let deadline = tokio::time::Instant::now() + SETTLE;
    while tokio::time::Instant::now() < deadline {
        if check(&server.observed.lock().expect("observed")) {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    false
}

/// The server's port of the remote forward in these tests; the test server listens nowhere.
const REMOTE_PORT: u16 = 9000;

#[tokio::test]
async fn a_remote_forward_brings_the_server_s_connections_to_the_local_port_and_no_other() {
    let server = start(Spec {
        forwarding: true,
        ..Spec::default()
    })
    .await;
    let dir = tempfile::tempdir().expect("dir");
    let connection = connected(&server, dir.path()).await;
    let local = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .await
        .expect("local");
    let local_port = local.local_addr().expect("address").port();

    let forward = connection
        .forward_remote(REMOTE_PORT, local_port)
        .await
        .expect("listening");
    assert_eq!(
        (forward.port(), forward.local_port()),
        (REMOTE_PORT, local_port)
    );
    let (mut taken, _) = tokio::time::timeout(STEP_TIMEOUT, local.accept())
        .await
        .expect("in time")
        .expect("a connection came back");
    let mut greeting = vec![0; FORWARDED_GREETING.len()];
    taken.read_exact(&mut greeting).await.expect("read");
    assert_eq!(greeting, FORWARDED_GREETING);
    taken.write_all(b"from-here").await.expect("answered");
    drop(taken);
    assert!(
        observed_until(&server, |o| o.forwarded_reply == b"from-here").await,
        "the answer went back"
    );
    // The port never asked for is refused: the server cannot reach this computer on its own.
    assert!(
        observed_until(&server, |o| o.forwarded_opens.len() == 2).await,
        "both opens answered"
    );
    assert_eq!(
        server.observed.lock().expect("observed").forwarded_opens,
        [
            (u32::from(REMOTE_PORT), true),
            (u32::from(REMOTE_PORT) + 1, false)
        ]
    );

    drop(forward);
    assert!(
        observed_until(&server, |o| o.cancelled == [u32::from(REMOTE_PORT)]).await,
        "the server stops listening with the forward"
    );
}

#[tokio::test]
async fn a_server_that_will_not_listen_refuses_the_remote_forward() {
    let server = start(Spec::default()).await;
    let dir = tempfile::tempdir().expect("dir");
    let connection = connected(&server, dir.path()).await;
    let refused = connection.forward_remote(REMOTE_PORT, 1).await;
    assert!(
        matches!(refused, Err(ConnectError::RemoteForwardRefused { port }) if port == REMOTE_PORT),
        "{refused:?}"
    );
    assert_eq!(
        server.observed.lock().expect("observed").listened,
        [u32::from(REMOTE_PORT)]
    );
}

/// A stand-in for the user's agent: answers any request with an empty list of identities.
#[cfg(unix)]
fn fake_agent(dir: &std::path::Path) -> std::path::PathBuf {
    let path = dir.join("agent.sock");
    let listener = tokio::net::UnixListener::bind(&path).expect("agent socket");
    tokio::spawn(async move {
        while let Ok((mut client, _)) = listener.accept().await {
            tokio::spawn(async move {
                let mut request = [0; 5];
                if client.read_exact(&mut request).await.is_ok() {
                    let _ = client.write_all(&EMPTY_IDENTITIES).await;
                }
            });
        }
    });
    path
}

/// `SSH_AGENT_IDENTITIES_ANSWER` with no identity.
#[cfg(unix)]
const EMPTY_IDENTITIES: [u8; 9] = [0, 0, 0, 5, 12, 0, 0, 0, 0];

#[cfg(unix)]
#[tokio::test]
async fn the_agent_is_forwarded_only_to_a_shell_that_asked() {
    use heimdall_ssh::AgentSource;

    let server = start(Spec::default()).await;
    let dir = tempfile::tempdir().expect("dir");
    let agent = fake_agent(dir.path());
    let connection = connected(&server, dir.path()).await;
    let mut options = options_trusting(dir.path(), server.port, "host-ed25519");
    options.agent = AgentSource::Path(agent);
    options.forward_agent = true;
    let _shell = connection
        .open_shell(&options, CancellationToken::new())
        .await
        .expect("shell");
    let deadline = tokio::time::Instant::now() + SETTLE;
    while server
        .observed
        .lock()
        .expect("observed")
        .agent_reply
        .is_empty()
        && tokio::time::Instant::now() < deadline
    {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let observed = server.observed.lock().expect("observed").clone();
    assert_eq!(observed.agent_asked, 1);
    assert_eq!(observed.agent_opens, [true]);
    assert_eq!(
        observed.agent_reply, EMPTY_IDENTITIES,
        "the agent answered through it"
    );
}

#[tokio::test]
async fn a_server_cannot_reach_the_agent_of_a_shell_that_did_not_ask() {
    let server = start(Spec {
        agent_unasked: true,
        ..Spec::default()
    })
    .await;
    let dir = tempfile::tempdir().expect("dir");
    let connection = connected(&server, dir.path()).await;
    // Forwarding off, an agent named: nothing is asked, and what the server opens anyway is
    // refused.
    let mut options = options_trusting(dir.path(), server.port, "host-ed25519");
    options.agent = heimdall_ssh::AgentSource::Path(dir.path().join("agent.sock"));
    let _shell = connection
        .open_shell(&options, CancellationToken::new())
        .await
        .expect("shell");
    let deadline = tokio::time::Instant::now() + SETTLE;
    while server
        .observed
        .lock()
        .expect("observed")
        .agent_opens
        .is_empty()
        && tokio::time::Instant::now() < deadline
    {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let observed = server.observed.lock().expect("observed").clone();
    assert_eq!(observed.agent_asked, 0);
    assert_eq!(observed.agent_opens, [false]);
}

/// A relay in front of `port` counting the bytes it carries from the server: what the wire
/// holds, compressed or not.
async fn counting_relay(port: u16) -> (u16, Arc<std::sync::atomic::AtomicUsize>) {
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .await
        .expect("relay");
    let relay_port = listener.local_addr().expect("address").port();
    let counted = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let count = counted.clone();
    tokio::spawn(async move {
        let Ok((client, _)) = listener.accept().await else {
            return;
        };
        let server = tokio::net::TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, port))
            .await
            .expect("server");
        let (mut client_read, mut client_write) = client.into_split();
        let (mut server_read, mut server_write) = server.into_split();
        tokio::spawn(async move {
            let _ = tokio::io::copy(&mut client_read, &mut server_write).await;
        });
        let mut buffer = vec![0; 16 * 1024];
        while let Ok(read) = server_read.read(&mut buffer).await {
            if read == 0 || client_write.write_all(&buffer[..read]).await.is_err() {
                break;
            }
            count.fetch_add(read, std::sync::atomic::Ordering::SeqCst);
        }
    });
    (relay_port, counted)
}

/// Bytes the server sent to echo `text` once connected, with compression `on` or off.
async fn wire_bytes_of_an_echo(text: &[u8], on: bool) -> usize {
    let server = start(Spec::default()).await;
    let dir = tempfile::tempdir().expect("dir");
    let (relay, counted) = counting_relay(server.port).await;
    // The relay's port, the server's host key.
    let mut options = options_trusting(dir.path(), relay, "host-ed25519");
    options.compression = on;
    let connection = tokio::time::timeout(
        STEP_TIMEOUT,
        establish(
            &profile(relay, None),
            &options,
            Arc::new(ScriptedPrompter::passwords(&[PASSWORD])),
            CancellationToken::new(),
        ),
    )
    .await
    .expect("in time")
    .expect("connected");
    let mut stream = connection
        .open_subsystem(SUBSYSTEM_ACCEPTED, ANSWER_TIMEOUT)
        .await
        .expect("accepted");
    let before = counted.load(std::sync::atomic::Ordering::SeqCst);
    echoes(&mut stream, text).await;
    counted.load(std::sync::atomic::Ordering::SeqCst) - before
}

#[tokio::test]
async fn compression_asked_for_shrinks_what_crosses_the_wire_and_keeps_the_bytes() {
    // Repetitive, as a listing or a log is.
    let text = "compressed ".repeat(2048);
    let plain = wire_bytes_of_an_echo(text.as_bytes(), false).await;
    let compressed = wire_bytes_of_an_echo(text.as_bytes(), true).await;
    assert!(plain >= text.len(), "uncompressed: {plain} bytes");
    assert!(
        compressed * 4 < plain,
        "compressed {compressed} bytes against {plain}"
    );
}

#[tokio::test]
async fn a_local_forward_carries_its_clients_through_the_gateway_to_the_one_destination() {
    let server = start(Spec {
        forwarding: true,
        ..Spec::default()
    })
    .await;
    let dir = tempfile::tempdir().expect("dir");
    let connection = connected(&server, dir.path()).await;
    let destination = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .await
        .expect("destination");
    let port = destination.local_addr().expect("address").port();
    let forward =
        heimdall_ssh::local_forward::start(Arc::new(connection), "127.0.0.1".to_owned(), port)
            .await
            .expect("forward");
    for round in [b"first", b"again"] {
        let mut client = tokio::net::TcpStream::connect(forward.address())
            .await
            .expect("client");
        client.write_all(round).await.expect("sent");
        let (mut far, _) = tokio::time::timeout(STEP_TIMEOUT, destination.accept())
            .await
            .expect("in time")
            .expect("reached through the gateway");
        let mut received = [0; 5];
        far.read_exact(&mut received).await.expect("read");
        assert_eq!(&received, round);
        far.write_all(b"back").await.expect("answered");
        let mut answer = [0; 4];
        client.read_exact(&mut answer).await.expect("answer");
        assert_eq!(&answer, b"back");
    }
}
