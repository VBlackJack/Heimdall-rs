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

//! Against a real OpenSSH server, opt-in.
//!
//! Runs only when `HEIMDALL_LIVE_SSH_PORT` names a local port served by the Heimdall-TestEnv
//! `ssh-base` image with three users: `rsauser` and `eduser` authorised with the
//! `rsa-openssh.pub` and `ed25519-openssh.pub` fixtures, and `pwuser` with the password in
//! `HEIMDALL_LIVE_SSH_PASSWORD`. The in-process server does not check signature algorithms; OpenSSH
//! refuses SHA-1 RSA signatures, which is what these tests add.

mod common;

use std::sync::Arc;

use common::{FIXTURE_PASSPHRASE, STEP_TIMEOUT, ScriptedPrompter, options_empty, profile};
use heimdall_ssh::{ConnectError, KnownHosts, SessionEvent, ShellSession, connect};
use tokio_util::sync::CancellationToken;

/// Environment variable naming the port of the live server.
const LIVE_PORT_VARIABLE: &str = "HEIMDALL_LIVE_SSH_PORT";

/// Environment variable holding the password of `pwuser` in the test container.
const LIVE_PASSWORD_VARIABLE: &str = "HEIMDALL_LIVE_SSH_PASSWORD";

/// The password of `pwuser`; required once the live port is set.
fn live_password() -> String {
    std::env::var(LIVE_PASSWORD_VARIABLE)
        .unwrap_or_else(|_| panic!("{LIVE_PASSWORD_VARIABLE} must be set with the live port"))
}

/// Command whose output proves the shell runs: the server computes 42.
const PROBE_COMMAND: &[u8] = b"echo heimdall-live-$((6*7))\n";
const PROBE_OUTPUT: &[u8] = b"heimdall-live-42";

fn live_port() -> Option<u16> {
    std::env::var(LIVE_PORT_VARIABLE).ok()?.parse().ok()
}

async fn connect_as(
    port: u16,
    user: &str,
    key: Option<&str>,
    prompter: ScriptedPrompter,
) -> Result<ShellSession, ConnectError> {
    let dir = tempfile::tempdir().expect("temp dir");
    let options = options_empty(dir.path());
    let mut target = profile(port, key);
    target.username = Some(user.to_owned());
    let prompter = Arc::new(prompter);

    // First contact is expected: record the key the server presented, then reconnect.
    let first = connect(
        &target,
        &options,
        prompter.clone(),
        CancellationToken::new(),
    )
    .await;
    let Err(ConnectError::UnknownHostKey { host, port, key }) = first else {
        panic!("expected a first contact, got {:?}", first.err());
    };
    KnownHosts::new(&options.known_hosts)
        .learn(&host, port, &key)
        .expect("learn");
    tokio::time::timeout(
        STEP_TIMEOUT,
        connect(&target, &options, prompter, CancellationToken::new()),
    )
    .await
    .expect("connected in time")
}

async fn assert_shell_runs(mut session: ShellSession) {
    session.input.write(PROBE_COMMAND.to_vec()).expect("write");
    let mut seen = Vec::new();
    tokio::time::timeout(STEP_TIMEOUT, async {
        while let Some(event) = session.events.recv().await {
            if let SessionEvent::Output(bytes) = event {
                seen.extend_from_slice(&bytes);
                if seen.windows(PROBE_OUTPUT.len()).any(|w| w == PROBE_OUTPUT) {
                    return;
                }
            }
        }
    })
    .await
    .expect("probe output arrived");
    session.input.close();
}

#[tokio::test]
async fn rsa_keys_log_in_to_openssh_in_both_formats() {
    let Some(port) = live_port() else { return };
    for key in ["rsa-openssh", "rsa-ppk3.ppk"] {
        let session = connect_as(port, "rsauser", Some(key), ScriptedPrompter::default()).await;
        assert_shell_runs(session.unwrap_or_else(|e| panic!("{key}: {e:?}"))).await;
    }
}

#[tokio::test]
async fn an_encrypted_putty_key_logs_in_to_openssh() {
    let Some(port) = live_port() else { return };
    let session = connect_as(
        port,
        "eduser",
        Some("ed25519-ppk3-encrypted.ppk"),
        ScriptedPrompter::passphrases(&[FIXTURE_PASSPHRASE]),
    )
    .await;
    assert_shell_runs(session.expect("session")).await;
}

#[tokio::test]
async fn a_password_account_logs_in_to_openssh() {
    let Some(port) = live_port() else { return };
    // OpenSSH may ask through keyboard-interactive or password; answer both.
    let prompter = ScriptedPrompter {
        kbd: vec![Some(vec![live_password()])]
            .into_iter()
            .collect::<std::collections::VecDeque<_>>()
            .into(),
        ..ScriptedPrompter::passwords(&[live_password().as_str()])
    };
    let session = connect_as(port, "pwuser", None, prompter).await;
    assert_shell_runs(session.expect("session")).await;
}
