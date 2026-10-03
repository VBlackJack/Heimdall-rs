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

//! Connections through SSH gateways, against in-process servers: each hop checks its own
//! host key and proves its own user, and the shell is the last server's.

mod common;

use std::sync::Arc;

use common::{
    EXIT_COMMAND, LOOPBACK, PASSWORD, STEP_TIMEOUT, ScriptedPrompter, Spec, TestServer,
    host_public_key, options_empty, profile, start,
};
use heimdall_core::profile::SshProfile;
use heimdall_ssh::{
    ConnectError, ConnectOptions, Connection, KnownHosts, SessionEvent, establish_via,
};
use russh::MethodKind;
use tokio_util::sync::CancellationToken;

/// Exit status the server behind the gateways ends its shell with: the shell is its.
const SERVER_EXIT: u32 = 42;

/// Exit status a gateway would end a shell with, were one opened on it.
const GATEWAY_EXIT: u32 = 7;

fn password_only(exit_status: u32, forwarding: bool) -> Spec {
    Spec {
        methods: vec![MethodKind::Password],
        exit_status,
        forwarding,
        ..Spec::default()
    }
}

/// Options whose `known_hosts` trusts every server in `trusted`.
fn trusting(dir: &std::path::Path, trusted: &[&TestServer]) -> ConnectOptions {
    let options = options_empty(dir);
    for server in trusted {
        KnownHosts::new(&options.known_hosts)
            .learn(LOOPBACK, server.port, &host_public_key("host-ed25519"))
            .expect("learn");
    }
    options
}

async fn through(
    route: &[SshProfile],
    server: &TestServer,
    options: &ConnectOptions,
    prompter: Arc<ScriptedPrompter>,
) -> Result<Connection, ConnectError> {
    tokio::time::timeout(
        STEP_TIMEOUT,
        establish_via(
            route,
            &profile(server.port, None),
            options,
            prompter,
            CancellationToken::new(),
        ),
    )
    .await
    .expect("connected in time")
}

/// Opens a shell on `connection`, ends it, and gives its exit status.
async fn shell_exit(connection: &Connection, options: &ConnectOptions) -> Option<u32> {
    let mut shell = connection
        .open_shell(options, CancellationToken::new())
        .await
        .expect("shell");
    shell.input.write(EXIT_COMMAND.to_vec()).expect("sent");
    tokio::time::timeout(STEP_TIMEOUT, async {
        while let Some(event) = shell.events.recv().await {
            if let SessionEvent::Closed { exit_status } = event {
                return exit_status;
            }
        }
        None
    })
    .await
    .expect("closed in time")
}

#[tokio::test]
async fn a_shell_through_a_gateway_is_the_servers() {
    let gateway = start(password_only(GATEWAY_EXIT, true)).await;
    let server = start(password_only(SERVER_EXIT, false)).await;
    let dir = tempfile::tempdir().expect("dir");
    let options = trusting(dir.path(), &[&gateway, &server]);
    let prompter = Arc::new(ScriptedPrompter::passwords(&[PASSWORD, PASSWORD]));

    let connection = through(
        &[profile(gateway.port, None)],
        &server,
        &options,
        prompter.clone(),
    )
    .await
    .expect("connected");
    assert_eq!(shell_exit(&connection, &options).await, Some(SERVER_EXIT));
    assert_eq!(
        prompter.asked(),
        ["password", "password"],
        "each hop proves its user"
    );
    let gateway_saw = gateway.observed.lock().expect("observed").clone();
    assert_eq!(
        gateway_saw.forwards,
        [(LOOPBACK.to_owned(), u32::from(server.port))]
    );
    assert!(gateway_saw.pty.is_none(), "no shell on the gateway");
    assert!(server.observed.lock().expect("observed").pty.is_some());
}

#[tokio::test]
async fn gateways_chain_nearest_first() {
    let near = start(password_only(GATEWAY_EXIT, true)).await;
    let far = start(password_only(GATEWAY_EXIT, true)).await;
    let server = start(password_only(SERVER_EXIT, false)).await;
    let dir = tempfile::tempdir().expect("dir");
    let options = trusting(dir.path(), &[&near, &far, &server]);
    let prompter = Arc::new(ScriptedPrompter::passwords(&[PASSWORD; 3]));

    let connection = through(
        &[profile(near.port, None), profile(far.port, None)],
        &server,
        &options,
        prompter,
    )
    .await
    .expect("connected");
    assert_eq!(shell_exit(&connection, &options).await, Some(SERVER_EXIT));
    let forwards =
        |gateway: &TestServer| gateway.observed.lock().expect("observed").forwards.clone();
    assert_eq!(
        forwards(&near),
        [(LOOPBACK.to_owned(), u32::from(far.port))]
    );
    assert_eq!(
        forwards(&far),
        [(LOOPBACK.to_owned(), u32::from(server.port))]
    );
}

#[tokio::test]
async fn a_gateway_with_forwarding_off_is_named_as_refusing() {
    let gateway = start(password_only(GATEWAY_EXIT, false)).await;
    let server = start(password_only(SERVER_EXIT, false)).await;
    let dir = tempfile::tempdir().expect("dir");
    let options = trusting(dir.path(), &[&gateway, &server]);
    let prompter = Arc::new(ScriptedPrompter::passwords(&[PASSWORD, PASSWORD]));

    let error = through(&[profile(gateway.port, None)], &server, &options, prompter)
        .await
        .expect_err("refused");
    let ConnectError::JumpRefused { host, port } = error else {
        panic!("{error:?}");
    };
    assert_eq!((host.as_str(), port), (LOOPBACK, server.port));
}

#[tokio::test]
async fn the_servers_own_host_key_is_checked_behind_a_trusted_gateway() {
    let gateway = start(password_only(GATEWAY_EXIT, true)).await;
    let server = start(password_only(SERVER_EXIT, false)).await;
    let dir = tempfile::tempdir().expect("dir");
    // The gateway only.
    let options = trusting(dir.path(), &[&gateway]);
    let prompter = Arc::new(ScriptedPrompter::passwords(&[PASSWORD, PASSWORD]));

    let error = through(
        &[profile(gateway.port, None)],
        &server,
        &options,
        prompter.clone(),
    )
    .await
    .expect_err("unknown");
    let ConnectError::UnknownHostKey { host, port, .. } = error else {
        panic!("{error:?}");
    };
    assert_eq!(
        (host.as_str(), port),
        (LOOPBACK, server.port),
        "the server's, not the gateway's"
    );
    assert_eq!(
        prompter.asked(),
        ["password"],
        "only the gateway was logged into"
    );
}

#[tokio::test]
async fn an_unknown_gateway_stops_before_anything_goes_through_it() {
    let gateway = start(password_only(GATEWAY_EXIT, true)).await;
    let server = start(password_only(SERVER_EXIT, false)).await;
    let dir = tempfile::tempdir().expect("dir");
    let options = trusting(dir.path(), &[&server]);
    let prompter = Arc::new(ScriptedPrompter::passwords(&[PASSWORD, PASSWORD]));

    let error = through(
        &[profile(gateway.port, None)],
        &server,
        &options,
        prompter.clone(),
    )
    .await
    .expect_err("unknown");
    let ConnectError::UnknownHostKey { port, .. } = error else {
        panic!("{error:?}");
    };
    assert_eq!(port, gateway.port);
    assert!(
        prompter.asked().is_empty(),
        "no password to an unknown gateway"
    );
    assert!(
        gateway
            .observed
            .lock()
            .expect("observed")
            .forwards
            .is_empty()
    );
}

#[tokio::test]
async fn dropping_the_connection_ends_the_gateways_too() {
    let gateway = start(password_only(GATEWAY_EXIT, true)).await;
    let server = start(password_only(SERVER_EXIT, false)).await;
    let dir = tempfile::tempdir().expect("dir");
    let options = trusting(dir.path(), &[&gateway, &server]);
    let prompter = Arc::new(ScriptedPrompter::passwords(&[PASSWORD, PASSWORD]));

    let connection = through(&[profile(gateway.port, None)], &server, &options, prompter)
        .await
        .expect("connected");
    drop(connection);
    tokio::time::timeout(STEP_TIMEOUT, async {
        while gateway.observed.lock().expect("observed").connections_ended == 0 {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the gateway's connection ended");
}

#[tokio::test]
async fn a_refusal_says_whether_the_gateway_or_the_server_refused() {
    let gateway = start(password_only(GATEWAY_EXIT, true)).await;
    let server = start(password_only(SERVER_EXIT, false)).await;
    let dir = tempfile::tempdir().expect("dir");
    let options = trusting(dir.path(), &[&gateway, &server]);

    // The gateway is given a wrong password at each of its tries.
    let refused = through(
        &[profile(gateway.port, None)],
        &server,
        &options,
        Arc::new(ScriptedPrompter::passwords(&["wrong"; 3])),
    )
    .await
    .expect_err("refused");
    assert!(
        matches!(
            refused,
            ConnectError::AuthenticationFailed { gateway: true, .. }
        ),
        "{refused:?}"
    );

    // The gateway lets the client in; the server behind it refuses.
    let refused = through(
        &[profile(gateway.port, None)],
        &server,
        &options,
        Arc::new(ScriptedPrompter::passwords(&[
            PASSWORD, "wrong", "wrong", "wrong",
        ])),
    )
    .await
    .expect_err("refused");
    assert!(
        matches!(
            refused,
            ConnectError::AuthenticationFailed { gateway: false, .. }
        ),
        "{refused:?}"
    );
}
