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

//! The connection driver against a real OpenSSH server, opt-in.
//!
//! Runs when `HEIMDALL_LIVE_SSH_PORT` names the local port of the Heimdall-TestEnv
//! `ssh-base` image with the `pwuser` account (see heimdall-ssh's live tests). Exercises the
//! whole path the interface uses: first contact, recording the key, reconnecting, a password
//! question answered through the registry, typing, and reading output.

use std::time::Duration;

use heimdall_app::{Answer, AnswerRegistry, ConnectRequest, ConnectionEvent, connection_events};
use heimdall_core::profile::{ProfileId, SshProfile};
use heimdall_ssh::{AgentSource, ConnectOptions, KnownHosts, Secret};
use tokio_stream::StreamExt;
use tokio_util::sync::CancellationToken;

const LIVE_PORT_VARIABLE: &str = "HEIMDALL_LIVE_SSH_PORT";
const LIVE_USER: &str = "pwuser";
const LIVE_PASSWORD: &str = "LiveP@ss123";
const PROBE_COMMAND: &[u8] = b"echo heimdall-app-$((6*7))\n";
const PROBE_OUTPUT: &str = "heimdall-app-42";
const STEP_TIMEOUT: Duration = Duration::from_secs(30);

fn request(port: u16, known_hosts: std::path::PathBuf) -> ConnectRequest {
    let mut options = ConnectOptions::new(known_hosts);
    options.agent = AgentSource::Disabled;
    ConnectRequest {
        profile: SshProfile {
            id: ProfileId::new("live"),
            name: "live".to_owned(),
            group: None,
            host: "127.0.0.1".to_owned(),
            port,
            username: Some(LIVE_USER.to_owned()),
            key_path: None,
        },
        options,
        cancel: CancellationToken::new(),
    }
}

#[tokio::test]
async fn the_driver_connects_answers_and_runs_a_command() {
    let Some(port) = std::env::var(LIVE_PORT_VARIABLE)
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
    else {
        return;
    };
    let dir = tempfile::tempdir().expect("temp dir");
    let known_hosts = dir.path().join("known_hosts");
    let registry = AnswerRegistry::default();

    // First attempt: the host is unknown, the attempt stops and reports the key.
    let mut first = connection_events(request(port, known_hosts.clone()), registry.clone());
    let event = tokio::time::timeout(STEP_TIMEOUT, first.next())
        .await
        .expect("in time")
        .expect("an event");
    let ConnectionEvent::UnknownHostKey {
        host,
        port: key_port,
        key,
        ..
    } = event
    else {
        panic!("expected a first contact, got {event:?}");
    };
    KnownHosts::new(&known_hosts)
        .learn(&host, key_port, &key)
        .expect("learn");

    // Second attempt: answer questions, then type and read.
    let mut events = connection_events(request(port, known_hosts), registry.clone());
    let mut screen = String::new();
    let outcome = tokio::time::timeout(STEP_TIMEOUT, async {
        while let Some(event) = events.next().await {
            match event {
                ConnectionEvent::Question { question, kind } => {
                    let secret = Secret::new(LIVE_PASSWORD.to_owned());
                    let answer = match kind {
                        heimdall_app::QuestionKind::KeyboardInteractive(round) => {
                            Answer::Secrets(vec![secret; round.prompts.len()])
                        }
                        _ => Answer::Secret(secret),
                    };
                    assert!(registry.answer(question, Some(answer)), "someone waits");
                }
                ConnectionEvent::Connected { input } => {
                    input.write(PROBE_COMMAND.to_vec()).expect("write");
                }
                ConnectionEvent::Output(bytes) => {
                    screen.push_str(&String::from_utf8_lossy(&bytes));
                    if screen.contains(PROBE_OUTPUT) {
                        return;
                    }
                }
                other => panic!("unexpected {other:?}"),
            }
        }
        panic!("the stream ended first");
    })
    .await;
    assert!(
        outcome.is_ok(),
        "no probe output in time; screen: {screen:?}"
    );
    assert_eq!(registry.pending(), 0, "no question left behind");
}
