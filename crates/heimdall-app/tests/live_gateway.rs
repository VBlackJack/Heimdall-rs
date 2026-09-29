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
//! on a server that only the gateway can reach, and the SOCKS proxy and remote forward a
//! profile opens there.
//!
//! Runs when `HEIMDALL_LIVE_JUMP_KEYS` names the key folder of the Heimdall-TestEnv lab: its
//! gateway listens on `127.0.0.1:2222` for the `gateway` account and reaches `linux-a:22`,
//! where `admin` logs in with its own key. Host keys are learnt as each first contact reports
//! them, the way the interface asks the user.

use std::net::Ipv4Addr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use heimdall_app::{
    AnswerRegistry, ConnectRequest, ConnectionEvent, Purpose, UiError, connection_events,
};
use heimdall_core::profile::{Forwards, ProfileId, SshProfile};
use heimdall_ssh::{AgentSource, ConnectOptions, KnownHosts};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::TcpStream;
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
        vault_entry: None,
        forwards: heimdall_core::profile::Forwards::default(),
        post_connect: heimdall_core::post_connect::PostConnect::default(),
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

/// The gateway's port of the remote forward: unprivileged, and nothing on the lab's gateway
/// listens there.
const REMOTE_PORT: u16 = 47_000;

/// A port an unprivileged account on the gateway cannot listen on.
const PRIVILEGED_PORT: u16 = 22;

/// What the local end of the remote forward says.
const REMOTE_GREETING: &[u8] = b"heimdall-remote-forward";

/// How long a shell with unapproved steps is watched for anything typed.
const UNAPPROVED_WAIT: Duration = Duration::from_secs(3);

/// A port nothing listens on now.
fn free_port() -> u16 {
    std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .expect("bound")
        .local_addr()
        .expect("address")
        .port()
}

/// The first bytes `host:port` sends, reached through the SOCKS5 proxy on `proxy`.
async fn through_socks(proxy: u16, host: &str, port: u16) -> Vec<u8> {
    let mut stream = TcpStream::connect((Ipv4Addr::LOCALHOST, proxy))
        .await
        .expect("the proxy listens");
    let name = u8::try_from(host.len()).expect("short name");
    let mut hello = vec![5, 1, 0, 5, 1, 0, 3, name];
    hello.extend_from_slice(host.as_bytes());
    hello.extend_from_slice(&port.to_be_bytes());
    stream.write_all(&hello).await.expect("asked");
    // The method chosen, then the reply to CONNECT with its bound address, IPv4.
    let mut answer = [0; 12];
    stream.read_exact(&mut answer).await.expect("answered");
    assert_eq!(answer[..4], [5, 0, 5, 0], "{answer:?}");
    let mut first = vec![0; 64];
    let read = tokio::time::timeout(STEP_TIMEOUT, stream.read(&mut first))
        .await
        .expect("in time")
        .expect("read");
    first.truncate(read);
    first
}

#[tokio::test]
async fn the_forwards_of_a_profile_run_through_the_lab_gateway_while_the_shell_runs() {
    let Some(keys) = std::env::var_os(KEYS_VARIABLE).map(PathBuf::from) else {
        eprintln!("{KEYS_VARIABLE} not set: skipped");
        return;
    };
    let dir = tempfile::tempdir().expect("dir");
    let known_hosts = dir.path().join("known_hosts");
    // Both host keys learnt first, without a proxy.
    let (event, events) = first_event_past_host_keys(&keys, &known_hosts, Purpose::Shell).await;
    assert!(
        matches!(event, ConnectionEvent::Connected { .. }),
        "{event:?}"
    );
    drop(events);

    // A port another program holds: the attempt fails and says which port.
    let holder = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("holder");
    let held = holder.local_addr().expect("address").port();
    let mut taken = request(&keys, &known_hosts, Purpose::Shell);
    taken.profile.forwards = Forwards {
        socks_port: Some(held),
        ..Forwards::default()
    };
    let mut events = connection_events(taken, AnswerRegistry::default());
    let event = tokio::time::timeout(STEP_TIMEOUT, events.next())
        .await
        .expect("in time")
        .expect("an event");
    assert!(
        matches!(event, ConnectionEvent::Failed(UiError::ProxyPort { port, .. }) if port == held),
        "{event:?}"
    );
    drop(holder);

    // A port the gateway cannot listen on, privileged: the attempt fails and says which.
    let mut privileged = request(&keys, &known_hosts, Purpose::Shell);
    privileged.profile.forwards = Forwards {
        remote_bind_port: Some(PRIVILEGED_PORT),
        ..Forwards::default()
    };
    let mut events = connection_events(privileged, AnswerRegistry::default());
    let event = tokio::time::timeout(STEP_TIMEOUT, events.next())
        .await
        .expect("in time")
        .expect("an event");
    assert!(
        matches!(event, ConnectionEvent::Failed(UiError::RemoteForwardRefused { port }) if port == PRIVILEGED_PORT),
        "{event:?}"
    );

    // What the gateway takes on its port comes back to a local one here.
    let local = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .await
        .expect("local");
    let local_port = local.local_addr().expect("address").port();
    tokio::spawn(async move {
        if let Ok((mut taken, _)) = local.accept().await {
            let _ = taken.write_all(REMOTE_GREETING).await;
        }
    });
    let port = free_port();
    let mut with_proxy = request(&keys, &known_hosts, Purpose::Shell);
    with_proxy.profile.forwards = Forwards {
        socks_port: Some(port),
        remote_bind_port: Some(REMOTE_PORT),
        remote_local_port: Some(local_port),
    };
    let mut events = connection_events(with_proxy, AnswerRegistry::default());
    let event = tokio::time::timeout(STEP_TIMEOUT, events.next())
        .await
        .expect("in time")
        .expect("an event");
    let ConnectionEvent::Connected { input } = event else {
        panic!("{event:?}");
    };
    // linux-a is reached only from the gateway: its banner came through the proxy.
    let banner = through_socks(port, "linux-a", 22).await;
    assert!(
        banner.starts_with(b"SSH-2.0-"),
        "{}",
        String::from_utf8_lossy(&banner)
    );
    // The gateway's own loopback port, reached through the proxy, leads back here.
    let greeting = through_socks(port, "127.0.0.1", REMOTE_PORT).await;
    assert_eq!(
        greeting,
        REMOTE_GREETING,
        "{}",
        String::from_utf8_lossy(&greeting)
    );

    input.write(b"exit\n".to_vec()).expect("typed");
    tokio::time::timeout(STEP_TIMEOUT, async {
        while let Some(event) = events.next().await {
            if matches!(event, ConnectionEvent::Closed { .. }) {
                return;
            }
        }
    })
    .await
    .expect("the shell ended");
    // The proxy goes with the session.
    tokio::time::timeout(STEP_TIMEOUT, async {
        while TcpStream::connect((Ipv4Addr::LOCALHOST, port))
            .await
            .is_ok()
        {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("the proxy stopped with the session");
}

#[tokio::test]
async fn approved_post_connect_steps_are_typed_into_the_lab_shell() {
    use heimdall_core::post_connect::{PostConnect, PostConnectStep};

    let Some(keys) = std::env::var_os(KEYS_VARIABLE).map(PathBuf::from) else {
        eprintln!("{KEYS_VARIABLE} not set: skipped");
        return;
    };
    let dir = tempfile::tempdir().expect("dir");
    let known_hosts = dir.path().join("known_hosts");
    let (event, events) = first_event_past_host_keys(&keys, &known_hosts, Purpose::Shell).await;
    assert!(
        matches!(event, ConnectionEvent::Connected { .. }),
        "{event:?}"
    );
    drop(events);

    // Steps nobody approved: the shell opens and nothing is typed.
    let mut unapproved = request(&keys, &known_hosts, Purpose::Shell);
    unapproved.profile.post_connect = PostConnect {
        steps: vec![PostConnectStep::new("exit")],
        approved: None,
    };
    let mut events = connection_events(unapproved, AnswerRegistry::default());
    let quiet = tokio::time::timeout(UNAPPROVED_WAIT, async {
        while let Some(event) = events.next().await {
            assert!(
                matches!(
                    event,
                    ConnectionEvent::Connected { .. } | ConnectionEvent::Output(_)
                ),
                "nothing typed, nothing ended: {event:?}"
            );
        }
    })
    .await;
    assert!(quiet.is_err(), "the shell stayed open, its step not typed");
    drop(events);

    let mut with_steps = request(&keys, &known_hosts, Purpose::Shell);
    with_steps.profile.post_connect = PostConnect::approved_as(vec![
        PostConnectStep::new("echo \"post-$((6*7))\""),
        PostConnectStep::new("exit"),
    ]);
    let mut events = connection_events(with_steps, AnswerRegistry::default());
    let mut screen = String::new();
    let mut completed = 0;
    let mut done = false;
    tokio::time::timeout(STEP_TIMEOUT, async {
        while let Some(event) = events.next().await {
            match event {
                ConnectionEvent::Connected { .. } => {}
                ConnectionEvent::Output(bytes) => screen.push_str(&String::from_utf8_lossy(&bytes)),
                ConnectionEvent::PostConnect(progress) => {
                    if progress.status == heimdall_app::StepStatus::Completed {
                        completed += 1;
                    }
                }
                ConnectionEvent::PostConnectDone => done = true,
                // The last step ends the shell.
                ConnectionEvent::Closed { .. } => return,
                other => panic!("{other:?}"),
            }
        }
    })
    .await
    .expect("the shell ended by its last step");
    assert!(screen.contains("post-42"), "{screen}");
    assert_eq!(completed, 2);
    assert!(
        done,
        "the sequence said it was over before the shell closed"
    );
}
