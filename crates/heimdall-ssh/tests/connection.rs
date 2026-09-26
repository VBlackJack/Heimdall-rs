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
    PASSWORD, STEP_TIMEOUT, SUBSYSTEM_ACCEPTED, SUBSYSTEM_SILENT, ScriptedPrompter, Spec,
    TestServer, options_trusting, profile, start,
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
    // for the latter (measured on 2026-09-26).
    assert_eq!(
        server.observed.lock().expect("observed").endings,
        vec!["Ok(())".to_owned()]
    );
}
