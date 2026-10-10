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

//! A profile's own local tunnel port, as the C# `LocalPort`, against two in-process servers:
//! each program started on this computer through a gateway, `PuTTY`, Remote Desktop
//! Connection and `PowerShell`, is pointed at it when it is free; a shell in a tab, whose
//! channel stays in the application, opens nothing there.

#[path = "../../heimdall-ssh/tests/common/mod.rs"]
mod ssh;

use std::net::Ipv4Addr;
use std::path::Path;
use std::sync::mpsc as std_mpsc;

use heimdall_app::mstsc_driver::{MstscRouteEvent, MstscRouteRequest, mstsc_route_events};
use heimdall_app::putty::{self, PuttyLaunch, PuttyRefusal, PuttyStarted};
use heimdall_app::putty_driver::{PuttyRouteEvent, PuttyRouteRequest, putty_route_events};
use heimdall_app::rdp_external::{ExternalRefusal, Running as MstscRunning};
use heimdall_app::{
    AnswerRegistry, ConnectRequest, ConnectionEvent, Purpose, QuestionKind, connection_events,
};
use heimdall_core::profile::{Forwards, ProfileId, RdpExtras, RdpOptions, RdpProfile, SshMode};
use heimdall_core::settings::Settings;
use heimdall_ssh::{ConnectOptions, KnownHosts, Secret};
use tokio::sync::oneshot;
use tokio_stream::{Stream, StreamExt as _};
use tokio_util::sync::CancellationToken;

/// The address line the `.rdp` file opens with.
const FULL_ADDRESS: &str = "full address:s:";

/// Two servers: a gateway that connects onward, and the server behind it.
struct Lab {
    gateway: ssh::TestServer,
    server: ssh::TestServer,
}

async fn lab() -> Lab {
    let gateway = ssh::start(ssh::Spec {
        forwarding: true,
        methods: vec![russh::MethodKind::Password],
        ..ssh::Spec::default()
    })
    .await;
    let server = ssh::start(ssh::Spec::default()).await;
    Lab { gateway, server }
}

/// Options trusting the gateway and the server.
fn options(dir: &Path, lab: &Lab) -> ConnectOptions {
    let options = ssh::options_trusting(dir, lab.gateway.port, "host-ed25519");
    KnownHosts::new(&options.known_hosts)
        .learn(
            ssh::LOOPBACK,
            lab.server.port,
            &ssh::host_public_key("host-ed25519"),
        )
        .expect("learn");
    options
}

/// A port nothing listens on now, chosen as a user would choose one.
fn free_port() -> u16 {
    std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .expect("bound")
        .local_addr()
        .expect("address")
        .port()
}

/// Whether nothing listens on `port` of the loopback address: it can be taken.
fn nothing_listens_on(port: u16) -> bool {
    std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, port)).is_ok()
}

/// The answer to a password question, if `event` is one.
fn answered(event: &ConnectionEvent, registry: &AnswerRegistry) -> bool {
    if let ConnectionEvent::Question {
        question,
        kind: QuestionKind::Password(_),
    } = event
    {
        let answer = heimdall_app::Answer::Secret(Secret::new(ssh::PASSWORD.to_owned()));
        assert!(registry.answer(*question, Some(answer)), "still waiting");
        return true;
    }
    false
}

#[tokio::test]
async fn putty_is_pointed_at_the_profile_s_port() {
    let lab = lab().await;
    let dir = tempfile::tempdir().expect("dir");
    let registry = AnswerRegistry::default();
    let chosen = free_port();
    let mut profile = ssh::profile(lab.server.port, None);
    profile.gateway = Some(ProfileId::new("gw"));
    profile.ssh_mode = SshMode::External;
    profile.local_tunnel_port = Some(chosen);
    let request = PuttyRouteRequest {
        before: Vec::new(),
        gateway: ssh::profile(lab.gateway.port, None),
        launch: putty::plan(&profile, &Settings::default(), String::new()),
        profile,
        ssh: options(dir.path(), &lab),
        cancel: CancellationToken::new(),
    };
    let (hand, handed) = std_mpsc::channel::<PuttyLaunch>();
    let (_exit, exited) = oneshot::channel::<()>();
    let start = move |launch: &PuttyLaunch| -> Result<putty::Running, PuttyRefusal> {
        hand.send(launch.clone()).expect("the test listens");
        Ok(putty::Running {
            started: PuttyStarted { x11: None },
            exited: Box::pin(async move {
                let _ = exited.await;
            }),
        })
    };
    let mut events = Box::pin(putty_route_events(request, registry.clone(), start));
    let launched = loop {
        let event = tokio::time::timeout(ssh::STEP_TIMEOUT, events.next())
            .await
            .expect("in time")
            .expect("an event");
        match event {
            PuttyRouteEvent::Route(question) if answered(&question, &registry) => {}
            other => break other,
        }
    };
    assert!(
        matches!(launched, PuttyRouteEvent::Launched(Ok(_))),
        "{launched:?}"
    );
    let launch = handed.try_recv().expect("started");
    assert_eq!(launch.port, chosen, "the profile's own port");
    assert!(!nothing_listens_on(chosen), "the forward holds it");
}

/// An external RDP profile through the gateway, on `port`.
fn rdp_profile(server_port: u16, port: Option<u16>) -> RdpProfile {
    RdpProfile {
        extras: RdpExtras {
            external: true,
            ..RdpExtras::default()
        },
        id: ProfileId::new("far"),
        name: "Far".to_owned(),
        group: None,
        host: ssh::LOOPBACK.to_owned(),
        port: server_port,
        username: Some("admin".to_owned()),
        domain: None,
        allow_tls_only: false,
        gateway: Some(ProfileId::new("gw")),
        local_tunnel_port: port,
        redirect_clipboard: true,
        redirect_drives: false,
        options: RdpOptions::default(),
        vault_entry: None,
        forwards: Forwards::default(),
        follow_defaults: false,
        several_servers: false,
        anti_idle: false,
        auto_reconnect: true,
    }
}

/// The launch's events, held while the client runs: dropped, the forward goes.
type Launch = std::pin::Pin<Box<dyn Stream<Item = MstscRouteEvent> + Send>>;

/// Launches Remote Desktop Connection for `profile` and gives the port its `.rdp` file
/// names, with the launch that holds it.
async fn mstsc_port(lab: &Lab, dir: &Path, profile: RdpProfile) -> (u16, Launch) {
    let registry = AnswerRegistry::default();
    let request = MstscRouteRequest {
        before: Vec::new(),
        gateway: ssh::profile(lab.gateway.port, None),
        profile,
        ssh: options(dir, lab),
        cancel: CancellationToken::new(),
    };
    let (hand, handed) = std_mpsc::channel::<String>();
    let start = move |content: &str| -> Result<MstscRunning, ExternalRefusal> {
        hand.send(content.to_owned()).expect("the test listens");
        Ok(MstscRunning {
            exited: Box::pin(std::future::pending()),
        })
    };
    let mut events: Launch = Box::pin(mstsc_route_events(request, registry.clone(), start));
    let launched = loop {
        let event = tokio::time::timeout(ssh::STEP_TIMEOUT, events.next())
            .await
            .expect("in time")
            .expect("an event");
        match event {
            MstscRouteEvent::Route(question) if answered(&question, &registry) => {}
            other => break other,
        }
    };
    assert!(
        matches!(launched, MstscRouteEvent::Launched(Ok(()))),
        "{launched:?}"
    );
    let content = handed.try_recv().expect("started");
    let line = content
        .lines()
        .find_map(|line| line.strip_prefix(FULL_ADDRESS))
        .expect("an address");
    let (host, port) = line.rsplit_once(':').expect("host:port");
    assert_eq!(host, "127.0.0.1", "on the loopback address only");
    (port.parse().expect("port"), events)
}

#[tokio::test]
async fn mstsc_is_pointed_at_the_profile_s_port_and_another_when_it_is_taken() {
    let lab = lab().await;
    let dir = tempfile::tempdir().expect("dir");
    let chosen = free_port();
    let (port, first) =
        mstsc_port(&lab, dir.path(), rdp_profile(lab.server.port, Some(chosen))).await;
    assert_eq!(port, chosen, "the profile's own port");
    assert!(!nothing_listens_on(chosen), "the forward holds it");

    // Taken by another program: the system chooses, the launch goes on.
    let holder = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("holder");
    let held = holder.local_addr().expect("address").port();
    let (port, second) =
        mstsc_port(&lab, dir.path(), rdp_profile(lab.server.port, Some(held))).await;
    assert_ne!(port, held, "another port, not refused");
    assert_ne!(port, 0);
    drop((first, second, holder));
}

/// What `echo` standing in for `PowerShell` prints: the command it was given.
#[cfg(unix)]
async fn winrm_command(lab: &Lab, dir: &Path, port: Option<u16>) -> String {
    let registry = AnswerRegistry::default();
    let request = heimdall_app::winrm_driver::WinRmRequest {
        profile: heimdall_core::profile::WinRmProfile {
            id: ProfileId::new("w"),
            name: "DC".to_owned(),
            group: None,
            host: ssh::LOOPBACK.to_owned(),
            port: heimdall_core::profile::DEFAULT_WINRM_HTTP_PORT,
            use_ssl: false,
            skip_certificate_check: false,
            username: None,
            gateway: Some(ProfileId::new("gw")),
            local_tunnel_port: port,
        },
        program: "echo".to_owned(),
        route: vec![ssh::profile(lab.gateway.port, None)],
        ssh: options(dir, lab),
        size: heimdall_ssh::TerminalSize {
            cols: 200,
            rows: 24,
            pixel_width: 0,
            pixel_height: 0,
        },
        fallback_directory: dir.to_owned(),
        cancel: CancellationToken::new(),
        password: None,
    };
    let mut events = heimdall_app::winrm_driver::winrm_events(request, registry.clone());
    let mut printed = String::new();
    loop {
        let event = tokio::time::timeout(ssh::STEP_TIMEOUT, events.next())
            .await
            .expect("in time");
        match event {
            Some(ConnectionEvent::Output(bytes)) => {
                printed.push_str(&String::from_utf8_lossy(&bytes));
            }
            // The forward goes once `PowerShell` has: the stream ends after.
            Some(ConnectionEvent::Closed { .. }) => {}
            None => return printed,
            Some(ConnectionEvent::Failed(failed)) => panic!("{failed:?}"),
            Some(other) => {
                answered(&other, &registry);
            }
        }
    }
}

#[cfg(unix)]
#[tokio::test]
async fn powershell_enters_through_the_profile_s_port() {
    let lab = lab().await;
    let dir = tempfile::tempdir().expect("dir");
    let chosen = free_port();
    let printed = winrm_command(&lab, dir.path(), Some(chosen)).await;
    let printed: String = printed.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        printed.contains(&format!("-ComputerName '127.0.0.1' -Port {chosen} ")),
        "{printed}"
    );
    // Released once `PowerShell` has gone.
    assert!(nothing_listens_on(chosen));
}

#[tokio::test]
async fn a_shell_in_a_tab_opens_nothing_on_the_profile_s_port() {
    let lab = lab().await;
    let dir = tempfile::tempdir().expect("dir");
    let registry = AnswerRegistry::default();
    let chosen = free_port();
    let mut profile = ssh::profile(lab.server.port, None);
    profile.gateway = Some(ProfileId::new("gw"));
    profile.local_tunnel_port = Some(chosen);
    let request = ConnectRequest {
        profile,
        route: vec![ssh::profile(lab.gateway.port, None)],
        purpose: Purpose::Shell,
        options: options(dir.path(), &lab),
        x11: None,
        cancel: CancellationToken::new(),
    };
    let mut events: std::pin::Pin<Box<dyn Stream<Item = ConnectionEvent> + Send>> =
        Box::pin(connection_events(request, registry.clone()));
    let connected = loop {
        let event = tokio::time::timeout(ssh::STEP_TIMEOUT, events.next())
            .await
            .expect("in time")
            .expect("an event");
        if !answered(&event, &registry) {
            break event;
        }
    };
    assert!(
        matches!(connected, ConnectionEvent::Connected { .. }),
        "{connected:?}"
    );
    assert!(
        nothing_listens_on(chosen),
        "the shell's channel stays in the application"
    );
    let onward = lab
        .gateway
        .observed
        .lock()
        .expect("observed")
        .forwards
        .clone();
    assert_eq!(
        onward,
        [(ssh::LOOPBACK.to_owned(), u32::from(lab.server.port))],
        "through the gateway all the same"
    );
}
