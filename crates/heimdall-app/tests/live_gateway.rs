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

//! The connection driver through a real OpenSSH gateway, opt-in: a shell and a Files session
//! on a server that only the gateway can reach.
//!
//! Runs when `HEIMDALL_LIVE_JUMP_KEYS` names the key folder of the Heimdall-TestEnv lab: its
//! gateway listens on `127.0.0.1:2222` for the `gateway` account and reaches `linux-a:22`,
//! where `admin` logs in with its own key. Host keys are learnt as each first contact reports
//! them, the way the interface asks the user.

use std::path::{Path, PathBuf};
use std::time::Duration;

use heimdall_app::{AnswerRegistry, ConnectRequest, ConnectionEvent, Purpose, connection_events};
use heimdall_core::profile::{ProfileId, SshProfile};
use heimdall_ssh::{AgentSource, ConnectOptions, KnownHosts};
use tokio_stream::StreamExt;
use tokio_util::sync::CancellationToken;

const KEYS_VARIABLE: &str = "HEIMDALL_LIVE_JUMP_KEYS";
const STEP_TIMEOUT: Duration = Duration::from_secs(30);
/// Host keys learnt on the way: the gateway's, then the server's.
const HOSTS_TO_LEARN: usize = 2;

fn hop(host: &str, port: u16, user: &str, key: PathBuf) -> SshProfile {
    SshProfile {
        id: ProfileId::new(host),
        name: host.to_owned(),
        group: None,
        host: host.to_owned(),
        port,
        username: Some(user.to_owned()),
        key_path: Some(key),
        gateway: None,
    }
}

fn request(keys: &Path, known_hosts: &Path, purpose: Purpose) -> ConnectRequest {
    let mut options = ConnectOptions::new(known_hosts.to_owned());
    options.agent = AgentSource::Disabled;
    ConnectRequest {
        profile: hop("linux-a", 22, "admin", keys.join("admin")),
        route: vec![hop("127.0.0.1", 2222, "gateway", keys.join("gateway"))],
        purpose,
        options,
        cancel: CancellationToken::new(),
    }
}

/// Runs attempts until one gets past its host keys, learning each as it is reported, and
/// gives that attempt's events after the first contact.
async fn first_event_past_host_keys(
    keys: &Path,
    known_hosts: &Path,
    purpose: Purpose,
) -> (
    ConnectionEvent,
    impl StreamExt<Item = ConnectionEvent> + Unpin,
) {
    for _ in 0..=HOSTS_TO_LEARN {
        let mut events = connection_events(
            request(keys, known_hosts, purpose),
            AnswerRegistry::default(),
        );
        let event = tokio::time::timeout(STEP_TIMEOUT, events.next())
            .await
            .expect("in time")
            .expect("an event");
        if let ConnectionEvent::UnknownHostKey {
            host, port, key, ..
        } = event
        {
            KnownHosts::new(known_hosts)
                .learn(&host, port, &key)
                .expect("learnt");
            continue;
        }
        return (event, events);
    }
    panic!("still asking about host keys");
}

#[tokio::test]
async fn a_shell_and_a_files_session_open_through_the_lab_gateway() {
    let Some(keys) = std::env::var_os(KEYS_VARIABLE).map(PathBuf::from) else {
        eprintln!("{KEYS_VARIABLE} not set: skipped");
        return;
    };
    let dir = tempfile::tempdir().expect("dir");
    let known_hosts = dir.path().join("known_hosts");

    let (event, mut events) = first_event_past_host_keys(&keys, &known_hosts, Purpose::Shell).await;
    let ConnectionEvent::Connected { input } = event else {
        panic!("{event:?}");
    };
    input
        .write(b"echo \"$(hostname)-$((6*7))\"; exit\n".to_vec())
        .expect("typed");
    let mut screen = String::new();
    tokio::time::timeout(STEP_TIMEOUT, async {
        while let Some(event) = events.next().await {
            match event {
                ConnectionEvent::Output(bytes) => screen.push_str(&String::from_utf8_lossy(&bytes)),
                ConnectionEvent::Closed { .. } => return,
                other => panic!("{other:?}"),
            }
        }
    })
    .await
    .expect("the shell ended");
    assert!(screen.contains("-42"), "{screen}");
    let recorded = KnownHosts::new(&known_hosts);
    assert!(!recorded.recorded("linux-a", 22).expect("read").is_empty());

    // Both keys known now: the Files session goes straight through.
    let (event, _events) = first_event_past_host_keys(&keys, &known_hosts, Purpose::Files).await;
    assert!(
        matches!(event, ConnectionEvent::FilesReady { .. }),
        "{event:?}"
    );
}
