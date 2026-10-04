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

//! A shell through a real OpenSSH gateway, to a server only the gateway can reach.
//!
//! Runs only when `HEIMDALL_LIVE_JUMP_KEYS` names the key folder of the Heimdall-TestEnv
//! lab: its gateway listens on `127.0.0.1:2222` for the `gateway` account, and reaches
//! `linux-a:22`, where the `admin` account logs in with its own key. Each host key is
//! learnt on first contact, as a user accepting it would.

mod common;

use std::path::PathBuf;
use std::sync::Arc;

use common::{STEP_TIMEOUT, ScriptedPrompter, options_empty};
use heimdall_core::profile::{ProfileId, SshProfile};
use heimdall_ssh::{ConnectError, Connection, KnownHosts, SessionEvent, establish_via};
use tokio_util::sync::CancellationToken;

const KEYS_VARIABLE: &str = "HEIMDALL_LIVE_JUMP_KEYS";

/// Host keys learnt on the way: the gateway's, then the server's.
const HOSTS_TO_LEARN: usize = 2;

fn hop(id: &str, host: &str, port: u16, user: &str, key: PathBuf) -> SshProfile {
    SshProfile {
        id: ProfileId::new(id),
        name: id.to_owned(),
        group: None,
        host: host.to_owned(),
        port,
        username: Some(user.to_owned()),
        key_path: Some(key),
        gateway: None,
        vault_entry: None,
        forwards: heimdall_core::profile::Forwards::default(),
        post_connect: heimdall_core::post_connect::PostConnect::default(),
        forward_agent: false,
        compression: false,
        sftp: false,
        legacy_algorithms: false,
        session_logging: None,
    }
}

#[tokio::test]
async fn a_shell_through_the_lab_gateway_reaches_a_server_only_it_can_see() {
    let Some(keys) = std::env::var_os(KEYS_VARIABLE).map(PathBuf::from) else {
        eprintln!("{KEYS_VARIABLE} not set: skipped");
        return;
    };
    let gateway = hop("gw", "127.0.0.1", 2222, "gateway", keys.join("gateway"));
    let server = hop("a", "linux-a", 22, "admin", keys.join("admin"));
    let dir = tempfile::tempdir().expect("dir");
    let options = options_empty(dir.path());
    let prompter = Arc::new(ScriptedPrompter::default());

    let mut connection: Option<Connection> = None;
    for _ in 0..=HOSTS_TO_LEARN {
        match tokio::time::timeout(
            STEP_TIMEOUT,
            establish_via(
                std::slice::from_ref(&gateway),
                &server,
                &options,
                prompter.clone(),
                CancellationToken::new(),
            ),
        )
        .await
        .expect("in time")
        {
            Ok(open) => {
                connection = Some(open);
                break;
            }
            Err(ConnectError::UnknownHostKey { host, port, key }) => {
                KnownHosts::new(&options.known_hosts)
                    .learn(&host, port, &key)
                    .expect("learnt");
            }
            Err(error) => panic!("{error:?}"),
        }
    }
    let connection = connection.expect("connected once both keys were learnt");
    let recorded = KnownHosts::new(&options.known_hosts);
    assert!(
        !recorded
            .recorded("127.0.0.1", 2222)
            .expect("read")
            .is_empty()
    );
    assert!(!recorded.recorded("linux-a", 22).expect("read").is_empty());

    let mut shell = connection
        .open_shell(&options, CancellationToken::new())
        .await
        .expect("shell");
    // Computed there, so the text is the server's answer and not an echo of what was typed.
    shell
        .input
        .write(b"echo \"$(hostname)-$((6*7))\"; exit\n".to_vec())
        .expect("typed");
    let mut seen = Vec::new();
    tokio::time::timeout(STEP_TIMEOUT, async {
        while let Some(event) = shell.events.recv().await {
            match event {
                SessionEvent::Output(bytes) => seen.extend_from_slice(&bytes),
                SessionEvent::Closed { .. } | SessionEvent::Lost => return,
            }
        }
    })
    .await
    .expect("the shell ended");
    let text = String::from_utf8_lossy(&seen);
    assert!(text.contains("-42"), "{text}");
    assert!(
        !text.contains("heimdall-gateway-42"),
        "the gateway's own shell: {text}"
    );
    assert!(
        prompter.asked().is_empty(),
        "keys only: {:?}",
        prompter.asked()
    );
}
