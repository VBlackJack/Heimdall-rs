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

//! A remote desktop through a real OpenSSH gateway, opt-in.
//!
//! Runs when `HEIMDALL_LIVE_JUMP_KEYS` names the key folder of the Heimdall-TestEnv lab and
//! `HEIMDALL_LIVE_RDP_USER` and `HEIMDALL_LIVE_RDP_PASSWORD` hold the account of its xrdp
//! server. The gateway on `127.0.0.1:2222` opens a tunnel to `heimdall-rdp:3389`, a name
//! only it resolves; the gateway's SSH key and the server's certificate are both accepted
//! as the user would, and the desktop must come up, with the profile's SOCKS proxy.

use std::net::Ipv4Addr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use heimdall_app::rdp_driver::{DEFAULT_DESKTOP, RdpRequest, rdp_events};
use heimdall_app::{Answer, AnswerRegistry, ConnectionEvent};
use heimdall_core::profile::{Forwards, ProfileId, RdpProfile, SshProfile};
use heimdall_rdp::Fingerprint;
use heimdall_ssh::{AgentSource, ConnectOptions, KnownHosts, Secret};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::TcpStream;
use tokio_stream::StreamExt;
use tokio_util::sync::CancellationToken;

const KEYS_VARIABLE: &str = "HEIMDALL_LIVE_JUMP_KEYS";
const USER_VARIABLE: &str = "HEIMDALL_LIVE_RDP_USER";
const PASSWORD_VARIABLE: &str = "HEIMDALL_LIVE_RDP_PASSWORD";
const STEP: Duration = Duration::from_secs(60);
/// How long the session must stay up once the clipboard channel is in use.
const CLIPBOARD_SETTLE: Duration = Duration::from_secs(3);
/// Attempts: the gateway's key, the server's certificate, then the desktop.
const ATTEMPTS: usize = 3;

fn request(
    keys: &Path,
    dir: &Path,
    user: &str,
    accepted: Option<Fingerprint>,
    socks_port: u16,
) -> RdpRequest {
    let mut ssh = ConnectOptions::new(dir.join("known_hosts"));
    ssh.agent = AgentSource::Disabled;
    RdpRequest {
        profile: RdpProfile {
            extras: heimdall_core::profile::RdpExtras::default(),
            id: ProfileId::new("rdp"),
            name: "rdp".to_owned(),
            group: None,
            host: "heimdall-rdp".to_owned(),
            port: 3389,
            username: Some(user.to_owned()),
            domain: None,
            // The lab's xrdp offers TLS without Network Level Authentication.
            allow_tls_only: true,
            gateway: None,
            redirect_clipboard: true,
            redirect_drives: false,
            options: heimdall_core::profile::RdpOptions::default(),
            vault_entry: None,
            forwards: Forwards {
                socks_port: Some(socks_port),
                ..Forwards::default()
            },
            follow_defaults: false,
            several_servers: false,
            anti_idle: false,
            auto_reconnect: true,
        },
        known_hosts: dir.join("known_rdp_hosts"),
        accepted,
        trusted_for_run: Vec::new(),
        desktop: DEFAULT_DESKTOP,
        desktop_scale: 100,
        logon_timeout: None,
        route: vec![SshProfile {
            id: ProfileId::new("gw"),
            name: "gw".to_owned(),
            group: None,
            host: "127.0.0.1".to_owned(),
            port: 2222,
            username: Some("gateway".to_owned()),
            key_path: Some(keys.join("gateway")),
            gateway: None,
            vault_entry: None,
            forwards: heimdall_core::profile::Forwards::default(),
            post_connect: heimdall_core::post_connect::PostConnect::default(),
            forward_agent: false,
            compression: false,
            sftp: false,
            legacy_algorithms: false,
            session_logging: None,
            ssh_mode: heimdall_core::profile::SshMode::Embedded,
            x11_forwarding: false,
        }],
        ssh,
        cancel: CancellationToken::new(),
    }
}

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
    let mut answer = [0; 12];
    stream.read_exact(&mut answer).await.expect("answered");
    assert_eq!(answer[..4], [5, 0, 5, 0], "{answer:?}");
    let mut first = vec![0; 64];
    let read = tokio::time::timeout(STEP, stream.read(&mut first))
        .await
        .expect("in time")
        .expect("read");
    first.truncate(read);
    first
}

#[tokio::test]
async fn a_desktop_comes_up_through_the_lab_gateway() {
    let (Some(keys), Ok(user), Ok(password)) = (
        std::env::var_os(KEYS_VARIABLE).map(PathBuf::from),
        std::env::var(USER_VARIABLE),
        std::env::var(PASSWORD_VARIABLE),
    ) else {
        eprintln!("{KEYS_VARIABLE}, {USER_VARIABLE} or {PASSWORD_VARIABLE} not set: skipped");
        return;
    };
    let dir = tempfile::tempdir().expect("dir");
    let socks_port = free_port();
    let mut accepted = None;
    for _ in 0..ATTEMPTS {
        let registry = AnswerRegistry::default();
        let request = request(&keys, dir.path(), &user, accepted, socks_port);
        let cancel = request.cancel.clone();
        let mut events = rdp_events(request, registry.clone());
        loop {
            let event = tokio::time::timeout(STEP, events.next())
                .await
                .expect("in time")
                .expect("an event");
            eprintln!("event: {event:?}");
            match event {
                ConnectionEvent::UnknownHostKey {
                    host, port, key, ..
                } => {
                    assert_eq!(
                        (host.as_str(), port),
                        ("127.0.0.1", 2222),
                        "the gateway's key"
                    );
                    KnownHosts::new(dir.path().join("known_hosts"))
                        .learn(&host, port, &key)
                        .expect("learnt");
                    break;
                }
                ConnectionEvent::UnknownRdpCertificate {
                    host, fingerprint, ..
                } => {
                    assert_eq!(host, "heimdall-rdp", "the server's own certificate");
                    accepted = Some(fingerprint);
                    break;
                }
                ConnectionEvent::Question { question, .. } => {
                    assert!(registry.answer(
                        question,
                        Some(Answer::Secret(Secret::new(password.clone())))
                    ));
                }
                ConnectionEvent::RdpReady { clipboard, .. } => {
                    // The clipboard channel, negotiated with a real server, must keep the
                    // session up, offers of this side's text and files included.
                    let offers = clipboard.expect("the profile shares the clipboard");
                    offers
                        .send(heimdall_rdp::LocalClipboard::Text(zeroize::Zeroizing::new(
                            "heimdall-offer".to_owned(),
                        )))
                        .expect("offered");
                    let copied = tempfile::tempdir().expect("copied");
                    let file = copied.path().join("heimdall-offer.txt");
                    std::fs::write(&file, b"heimdall").expect("file");
                    offers
                        .send(heimdall_rdp::LocalClipboard::Files(vec![file]))
                        .expect("offered");
                    let settle = tokio::time::sleep(CLIPBOARD_SETTLE);
                    tokio::pin!(settle);
                    loop {
                        tokio::select! {
                            () = &mut settle => break,
                            event = events.next() => match event {
                                Some(ConnectionEvent::DesktopFrame | ConnectionEvent::RemoteClipboard(_)) => {}
                                other => panic!("the session did not stay up: {other:?}"),
                            },
                        }
                    }
                    // The profile's SOCKS proxy runs with the desktop: linux-a, which only the
                    // gateway reaches, answers through it.
                    let banner = through_socks(socks_port, "linux-a", 22).await;
                    assert!(
                        banner.starts_with(b"SSH-2.0-"),
                        "{}",
                        String::from_utf8_lossy(&banner)
                    );
                    cancel.cancel();
                    return;
                }
                other => panic!("{other:?}"),
            }
        }
    }
    panic!("no desktop after {ATTEMPTS} attempts");
}
