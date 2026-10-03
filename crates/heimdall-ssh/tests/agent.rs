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

//! Authentication through an SSH agent, against an in-process agent on a Unix socket.
//!
//! Windows has no such test: its agents are the OpenSSH service and Pageant, neither of
//! which a test can start; the pipe selection is unit tested in `src/agent.rs`.

#![cfg(unix)]

mod common;

use std::path::Path;
use std::sync::Arc;

use common::{
    FIXTURE_PASSPHRASE, PASSWORD, ScriptedPrompter, Spec, client_key_path, client_public_key,
    host_private_key, options_trusting, profile, start,
};
use heimdall_ssh::{AgentSource, KeyFile, Secret, connect};
use russh::MethodKind;
use russh::keys::PrivateKey;
use russh::keys::agent::client::AgentClient;
use tokio::net::UnixListener;
use tokio_stream::wrappers::UnixListenerStream;
use tokio_util::sync::CancellationToken;

/// Keys an agent test can load that no test server authorises.
const UNRELATED_HOST_KEYS: [&str; 3] = ["host-ed25519", "host-ed25519-other", "host-ecdsa"];

fn fixture_private_key(name: &str) -> PrivateKey {
    KeyFile::read(client_key_path(name))
        .expect("fixture")
        .decrypt(Some(&Secret::new(FIXTURE_PASSPHRASE.to_owned())))
        .expect("decrypts")
}

/// Starts an agent on a socket in `dir` holding `keys`.
async fn start_agent(dir: &Path, keys: &[PrivateKey]) -> std::path::PathBuf {
    start_named_agent(dir, "agent.sock", keys).await
}

/// Starts an agent on socket `name` in `dir` holding `keys`.
async fn start_named_agent(dir: &Path, name: &str, keys: &[PrivateKey]) -> std::path::PathBuf {
    let socket = dir.join(name);
    let listener = UnixListener::bind(&socket).expect("bind agent socket");
    tokio::spawn(russh::keys::agent::server::serve(
        UnixListenerStream::new(listener),
        (),
    ));
    let mut client = AgentClient::connect_uds(&socket)
        .await
        .expect("agent client");
    for key in keys {
        client.add_identity(key, &[]).await.expect("add identity");
    }
    socket
}

#[tokio::test]
async fn an_encrypted_profile_key_held_by_the_agent_is_used_without_a_passphrase() {
    let server = start(Spec {
        methods: vec![MethodKind::PublicKey],
        authorized: vec![client_public_key("ed25519-openssh-encrypted")],
        ..Spec::default()
    })
    .await;
    let dir = tempfile::tempdir().expect("temp dir");
    let socket = start_agent(
        dir.path(),
        &[fixture_private_key("ed25519-openssh-encrypted")],
    )
    .await;
    let mut options = options_trusting(dir.path(), server.port, "host-ed25519");
    options.agent = AgentSource::Path(socket);
    let prompter = Arc::new(ScriptedPrompter::default());

    let result = connect(
        &profile(server.port, Some("ed25519-openssh-encrypted")),
        &options,
        prompter.clone(),
        CancellationToken::new(),
    )
    .await;
    assert!(result.is_ok(), "{:?}", result.err());
    assert!(prompter.asked().is_empty(), "asked {:?}", prompter.asked());
}

#[tokio::test]
async fn without_a_profile_key_at_most_three_agent_keys_are_offered() {
    let server = start(Spec::default()).await;
    let dir = tempfile::tempdir().expect("temp dir");
    let mut keys: Vec<PrivateKey> = UNRELATED_HOST_KEYS
        .iter()
        .map(|name| host_private_key(name))
        .collect();
    keys.push(fixture_private_key("ed25519-openssh"));
    keys.push(fixture_private_key("rsa-openssh"));
    let socket = start_agent(dir.path(), &keys).await;
    let mut options = options_trusting(dir.path(), server.port, "host-ed25519");
    options.agent = AgentSource::Path(socket);
    let prompter = Arc::new(ScriptedPrompter::passwords(&[PASSWORD]));

    let result = connect(
        &profile(server.port, None),
        &options,
        prompter.clone(),
        CancellationToken::new(),
    )
    .await;
    assert!(result.is_ok(), "{:?}", result.err());
    assert_eq!(
        server
            .observed
            .lock()
            .expect("observed")
            .publickey_offers
            .len(),
        3,
        "five agent keys held, three offered"
    );
    assert_eq!(prompter.asked(), vec!["password"]);
}

#[tokio::test]
async fn an_agent_key_the_server_accepts_logs_in_when_the_profile_names_none() {
    let server = start(Spec {
        methods: vec![MethodKind::PublicKey],
        authorized: vec![client_public_key("ed25519-openssh")],
        ..Spec::default()
    })
    .await;
    let dir = tempfile::tempdir().expect("temp dir");
    let socket = start_agent(dir.path(), &[fixture_private_key("ed25519-openssh")]).await;
    let mut options = options_trusting(dir.path(), server.port, "host-ed25519");
    options.agent = AgentSource::Path(socket);

    let result = connect(
        &profile(server.port, None),
        &options,
        Arc::new(ScriptedPrompter::default()),
        CancellationToken::new(),
    )
    .await;
    assert!(result.is_ok(), "{:?}", result.err());
}

/// Two agents as on Windows with the OpenSSH agent and Pageant: the first holds keys the
/// server refuses, the second the one it accepts.
async fn two_agents(dir: &Path, second: PrivateKey) -> AgentSource {
    let unrelated: Vec<PrivateKey> = UNRELATED_HOST_KEYS
        .iter()
        .map(|name| host_private_key(name))
        .collect();
    let first = start_named_agent(dir, "openssh.sock", &unrelated).await;
    let second = start_named_agent(dir, "pageant.sock", &[second]).await;
    AgentSource::Paths(vec![first, dir.join("gone.sock"), second])
}

#[tokio::test]
async fn a_key_held_by_the_second_agent_is_offered_while_the_first_holds_three() {
    let server = start(Spec {
        methods: vec![MethodKind::PublicKey],
        authorized: vec![client_public_key("ed25519-openssh")],
        ..Spec::default()
    })
    .await;
    let dir = tempfile::tempdir().expect("temp dir");
    let mut options = options_trusting(dir.path(), server.port, "host-ed25519");
    options.agent = two_agents(dir.path(), fixture_private_key("ed25519-openssh")).await;

    let result = connect(
        &profile(server.port, None),
        &options,
        Arc::new(ScriptedPrompter::default()),
        CancellationToken::new(),
    )
    .await;
    assert!(result.is_ok(), "{:?}", result.err());
    assert_eq!(
        server
            .observed
            .lock()
            .expect("observed")
            .publickey_offers
            .len(),
        2,
        "the first agent's first key, then the second agent's: each agent in turn"
    );
}

#[tokio::test]
async fn a_profile_key_held_by_the_second_agent_is_signed_by_it_without_a_passphrase() {
    let server = start(Spec {
        methods: vec![MethodKind::PublicKey],
        authorized: vec![client_public_key("ed25519-openssh-encrypted")],
        ..Spec::default()
    })
    .await;
    let dir = tempfile::tempdir().expect("temp dir");
    let mut options = options_trusting(dir.path(), server.port, "host-ed25519");
    options.agent = two_agents(dir.path(), fixture_private_key("ed25519-openssh-encrypted")).await;
    let prompter = Arc::new(ScriptedPrompter::default());

    let result = connect(
        &profile(server.port, Some("ed25519-openssh-encrypted")),
        &options,
        prompter.clone(),
        CancellationToken::new(),
    )
    .await;
    assert!(result.is_ok(), "{:?}", result.err());
    assert!(prompter.asked().is_empty(), "asked {:?}", prompter.asked());
}

#[tokio::test]
async fn the_agents_answering_are_named_with_how_many_keys_each_holds() {
    let dir = tempfile::tempdir().expect("dir");
    let holding = start_named_agent(
        dir.path(),
        "holding.sock",
        &[fixture_private_key("ed25519-openssh")],
    )
    .await;
    let empty = start_named_agent(dir.path(), "empty.sock", &[]).await;
    let gone = dir.path().join("gone.sock");
    let found = heimdall_ssh::survey_agents(&AgentSource::Paths(vec![empty, gone, holding])).await;
    let seen: Vec<(&str, usize)> = found
        .iter()
        .map(|agent| (agent.name.as_str(), agent.keys))
        .collect();
    assert_eq!(
        seen,
        [("empty.sock", 0), ("holding.sock", 1)],
        "an agent not there is left out"
    );
    assert!(
        heimdall_ssh::survey_agents(&AgentSource::Disabled)
            .await
            .is_empty()
    );
}
