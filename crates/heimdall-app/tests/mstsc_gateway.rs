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

//! Remote Desktop Connection opened through an SSH gateway, as the C# external mode with a
//! tunnel: against two in-process servers, the gateway signed in to, a forward on the
//! loopback address to the server, and the `.rdp` file pointed at it; then the application's
//! side, its questions, notices and releases. No `mstsc.exe` is started: the starter is the
//! test's.

#[path = "../../heimdall-ssh/tests/common/mod.rs"]
mod ssh;

use std::path::Path;
use std::sync::Arc;
use std::sync::mpsc as std_mpsc;

use heimdall_app::mstsc_driver::{
    MSTSC_CLIENTS, MstscRouteEvent, MstscRouteId, MstscRouteRequest, mstsc_route_events,
};
use heimdall_app::rdp_external::{ExternalRefusal, Running};
use heimdall_app::{
    AnswerRegistry, App, AppConfig, ConnectionEvent, Dialog, Effect, Message, Notice, QuestionKind,
    SystemCredentials, UiError,
};
use heimdall_core::profile::{Forwards, ProfileId, RdpExtras, RdpOptions, RdpProfile, SshGateway};
use heimdall_core::store::ProfileStore;
use heimdall_ssh::{AgentSource, ConnectOptions, KnownHosts, PublicKey, Secret};
use heimdall_term::GridSize;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::TcpStream;
use tokio::sync::oneshot;
use tokio_stream::{Stream, StreamExt as _};
use tokio_util::sync::CancellationToken;

/// What an SSH server says first, which the forward carries back from the server.
const SSH_BANNER: &[u8] = b"SSH-2.0-";
/// The C# suggested RDP tunnel port, `DefaultPorts.RdpTunnel`.
const SUGGESTED_RDP_TUNNEL_PORT: u16 = 33890;
/// The address line the `.rdp` file opens with.
const FULL_ADDRESS: &str = "full address:s:";

/// Two servers: a gateway that connects onward, and the server behind it.
struct Lab {
    gateway: ssh::TestServer,
    server: ssh::TestServer,
}

async fn lab() -> Lab {
    // A password alone signs in to the gateway, as the questions answered here are.
    let gateway = ssh::start(ssh::Spec {
        forwarding: true,
        methods: vec![russh::MethodKind::Password],
        ..ssh::Spec::default()
    })
    .await;
    let server = ssh::start(ssh::Spec::default()).await;
    Lab { gateway, server }
}

fn rdp_profile(host: &str, port: u16, gateway: Option<&str>) -> RdpProfile {
    RdpProfile {
        extras: RdpExtras {
            external: true,
            ..RdpExtras::default()
        },
        id: ProfileId::new("far"),
        name: "Far".to_owned(),
        group: None,
        host: host.to_owned(),
        port,
        username: Some("admin".to_owned()),
        domain: None,
        allow_tls_only: false,
        gateway: gateway.map(ProfileId::new),
        local_tunnel_port: None,
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

fn request(
    lab: &Lab,
    profile: RdpProfile,
    options: ConnectOptions,
    cancel: CancellationToken,
) -> MstscRouteRequest {
    MstscRouteRequest {
        before: Vec::new(),
        gateway: ssh::profile(lab.gateway.port, None),
        profile,
        ssh: options,
        cancel,
    }
}

fn lab_request(lab: &Lab, dir: &Path, cancel: CancellationToken) -> MstscRouteRequest {
    request(
        lab,
        rdp_profile(ssh::LOOPBACK, lab.server.port, Some("gw")),
        ssh::options_trusting(dir, lab.gateway.port, "host-ed25519"),
        cancel,
    )
}

/// A starter that hands the `.rdp` content it was given to the test, and a client that
/// exits when the test says so.
fn starter() -> (
    impl FnOnce(&str) -> Result<Running, ExternalRefusal> + Send + 'static,
    std_mpsc::Receiver<String>,
    oneshot::Sender<()>,
) {
    let (hand, handed) = std_mpsc::channel();
    let (exit, exited) = oneshot::channel::<()>();
    let start = move |content: &str| {
        hand.send(content.to_owned()).expect("the test listens");
        Ok(Running {
            exited: Box::pin(async move {
                let _ = exited.await;
            }),
        })
    };
    (start, handed, exit)
}

/// The next event that is not a password question, each one answered with `password`.
async fn next(
    events: &mut (impl Stream<Item = MstscRouteEvent> + Unpin),
    registry: &AnswerRegistry,
    password: &str,
) -> Option<MstscRouteEvent> {
    loop {
        let event = tokio::time::timeout(ssh::STEP_TIMEOUT, events.next())
            .await
            .expect("in time")?;
        match event {
            MstscRouteEvent::Route(ConnectionEvent::Question {
                question,
                kind: QuestionKind::Password(_),
            }) => {
                let answer = heimdall_app::Answer::Secret(Secret::new(password.to_owned()));
                assert!(registry.answer(question, Some(answer)), "still waiting");
            }
            other => return Some(other),
        }
    }
}

/// The address of the `.rdp` file, host and port.
fn address(content: &str) -> (String, u16) {
    let line = content
        .lines()
        .find_map(|line| line.strip_prefix(FULL_ADDRESS))
        .expect("an address");
    let (host, port) = line.rsplit_once(':').expect("host:port");
    (host.to_owned(), port.parse().expect("port"))
}

/// Says the SSH banner through a client of the forward at `port`, and reads the server's.
async fn reaches_the_server(port: u16) -> TcpStream {
    let mut client = TcpStream::connect((ssh::LOOPBACK, port))
        .await
        .expect("the forward listens");
    client
        .write_all(b"SSH-2.0-probe\r\n")
        .await
        .expect("written");
    let mut banner = [0; SSH_BANNER.len()];
    tokio::time::timeout(ssh::STEP_TIMEOUT, client.read_exact(&mut banner))
        .await
        .expect("in time")
        .expect("the server answered");
    assert_eq!(&banner, SSH_BANNER);
    client
}

/// Whether the peer of `client` closed it.
async fn closed(client: &mut TcpStream) -> bool {
    let mut read = [0; 1];
    let read = tokio::time::timeout(ssh::STEP_TIMEOUT, client.read(&mut read))
        .await
        .expect("closed in time");
    matches!(read, Ok(0) | Err(_))
}

#[tokio::test]
async fn mstsc_goes_to_a_loopback_forward_through_the_gateway_until_it_exits() {
    let lab = lab().await;
    let dir = tempfile::tempdir().expect("dir");
    let registry = AnswerRegistry::default();
    let (start, handed, exit) = starter();
    let request = lab_request(&lab, dir.path(), CancellationToken::new());
    let mut events = Box::pin(mstsc_route_events(request, registry.clone(), start));

    let launched = next(&mut events, &registry, ssh::PASSWORD).await;
    assert!(
        matches!(launched, Some(MstscRouteEvent::Launched(Ok(())))),
        "{launched:?}"
    );
    let content = handed.try_recv().expect("started once");
    let (host, local) = address(&content);
    assert_eq!(host, "127.0.0.1", "on the loopback address only");
    assert_ne!(
        local, lab.server.port,
        "the forward's port, not the server's"
    );
    assert!(
        content.starts_with(&format!("{FULL_ADDRESS}127.0.0.1:{local}\r\n")),
        "{content}"
    );
    assert!(content.contains("\r\nusername:s:admin\r\n"), "{content}");
    assert!(
        !content.lines().any(|line| line.starts_with("password")),
        "no password"
    );
    assert!(content.contains("\r\ngatewayusagemethod:i:0\r\n"));

    // The forward reaches the server through the gateway.
    let first = reaches_the_server(local).await;
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
        "through the gateway, to the server's own host and port"
    );

    // As many clients as the client opens, no more.
    let second = reaches_the_server(local).await;
    let mut third = TcpStream::connect((ssh::LOOPBACK, local))
        .await
        .expect("accepted, then dropped");
    assert!(closed(&mut third).await, "beyond {MSTSC_CLIENTS}: refused");
    drop((first, second));

    // The client exits: the forward goes.
    exit.send(()).expect("the driver waits");
    let released = next(&mut events, &registry, ssh::PASSWORD).await;
    assert!(
        matches!(released, Some(MstscRouteEvent::Released)),
        "{released:?}"
    );
    tokio::task::yield_now().await;
    if let Ok(mut late) = TcpStream::connect((ssh::LOOPBACK, local)).await {
        assert!(closed(&mut late).await, "nothing carried once released");
    }
    assert!(next(&mut events, &registry, ssh::PASSWORD).await.is_none());
}

#[tokio::test]
async fn the_forward_takes_a_port_the_system_picks_whether_33890_is_free_or_not() {
    // As the C# `ShouldUseOsAssignedLocalPort`: a profile on the suggested port, which is
    // every profile here, takes the one the system assigns. Held or not, 33890 is not
    // waited for, nor a launch refused for it.
    let held = std::net::TcpListener::bind((ssh::LOOPBACK, SUGGESTED_RDP_TUNNEL_PORT)).ok();
    let lab = lab().await;
    let dir = tempfile::tempdir().expect("dir");
    let registry = AnswerRegistry::default();
    let (start, handed, _exit) = starter();
    let request = lab_request(&lab, dir.path(), CancellationToken::new());
    let mut events = Box::pin(mstsc_route_events(request, registry.clone(), start));
    let launched = next(&mut events, &registry, ssh::PASSWORD).await;
    assert!(
        matches!(launched, Some(MstscRouteEvent::Launched(Ok(())))),
        "{launched:?}"
    );
    let (_, local) = address(&handed.try_recv().expect("started"));
    if held.is_some() {
        assert_ne!(local, SUGGESTED_RDP_TUNNEL_PORT, "33890 held elsewhere");
    }
    drop(reaches_the_server(local).await);
    drop(held);
}

#[tokio::test]
async fn an_rd_gateway_behind_an_ssh_gateway_is_written_as_the_csharp_writes_it() {
    // The C# tunnels to the server and keeps the RD Gateway lines in the file.
    let lab = lab().await;
    let dir = tempfile::tempdir().expect("dir");
    let registry = AnswerRegistry::default();
    let (start, handed, _exit) = starter();
    let mut profile = rdp_profile(ssh::LOOPBACK, lab.server.port, Some("gw"));
    profile.extras.external = false;
    profile.extras.rd_gateway = Some("rdg.lab".to_owned());
    let request = request(
        &lab,
        profile,
        ssh::options_trusting(dir.path(), lab.gateway.port, "host-ed25519"),
        CancellationToken::new(),
    );
    let mut events = Box::pin(mstsc_route_events(request, registry.clone(), start));
    let launched = next(&mut events, &registry, ssh::PASSWORD).await;
    assert!(
        matches!(launched, Some(MstscRouteEvent::Launched(Ok(())))),
        "{launched:?}"
    );
    let content = handed.try_recv().expect("started");
    let (host, local) = address(&content);
    assert_eq!(host, "127.0.0.1");
    assert_ne!(local, lab.server.port);
    assert!(
        content.ends_with(
            "gatewayusagemethod:i:1\r\ngatewayprofileusagemethod:i:1\r\n\
             gatewayhostname:s:rdg.lab\r\ngatewaycredentialssource:i:0\r\n"
        ),
        "{content}"
    );
}

#[tokio::test]
async fn cancelled_while_mstsc_runs_the_forward_is_released() {
    let lab = lab().await;
    let dir = tempfile::tempdir().expect("dir");
    let registry = AnswerRegistry::default();
    let (start, handed, _exit) = starter();
    let cancel = CancellationToken::new();
    let request = lab_request(&lab, dir.path(), cancel.clone());
    let mut events = Box::pin(mstsc_route_events(request, registry.clone(), start));
    let launched = next(&mut events, &registry, ssh::PASSWORD).await;
    assert!(
        matches!(launched, Some(MstscRouteEvent::Launched(Ok(())))),
        "{launched:?}"
    );
    let (_, local) = address(&handed.try_recv().expect("started"));
    cancel.cancel();
    let released = next(&mut events, &registry, ssh::PASSWORD).await;
    assert!(
        matches!(released, Some(MstscRouteEvent::Released)),
        "{released:?}"
    );
    if let Ok(mut late) = TcpStream::connect((ssh::LOOPBACK, local)).await {
        assert!(closed(&mut late).await);
    }
}

#[tokio::test]
async fn a_client_that_does_not_start_releases_the_forward_at_once() {
    let lab = lab().await;
    let dir = tempfile::tempdir().expect("dir");
    let registry = AnswerRegistry::default();
    let request = lab_request(&lab, dir.path(), CancellationToken::new());
    let start = |_: &str| -> Result<Running, ExternalRefusal> { Err(ExternalRefusal::NotFound) };
    let mut events = Box::pin(mstsc_route_events(request, registry.clone(), start));
    let launched = next(&mut events, &registry, ssh::PASSWORD).await;
    assert!(
        matches!(
            launched,
            Some(MstscRouteEvent::Launched(Err(ExternalRefusal::NotFound)))
        ),
        "{launched:?}"
    );
    assert!(next(&mut events, &registry, ssh::PASSWORD).await.is_none());
}

#[tokio::test]
async fn a_gateway_that_refuses_the_user_launches_nothing() {
    let lab = lab().await;
    let dir = tempfile::tempdir().expect("dir");
    let registry = AnswerRegistry::default();
    let (start, handed, _exit) = starter();
    let request = lab_request(&lab, dir.path(), CancellationToken::new());
    let mut events = Box::pin(mstsc_route_events(request, registry.clone(), start));
    let failed = next(&mut events, &registry, "wrong").await;
    assert!(
        matches!(
            failed,
            Some(MstscRouteEvent::Route(ConnectionEvent::Failed(_)))
        ),
        "{failed:?}"
    );
    assert!(next(&mut events, &registry, "wrong").await.is_none());
    assert!(handed.try_recv().is_err(), "never started");
    assert!(
        lab.gateway
            .observed
            .lock()
            .expect("observed")
            .forwards
            .is_empty(),
        "nothing went onward"
    );
}

#[tokio::test]
async fn an_unknown_gateway_key_is_asked_about_and_nothing_is_launched() {
    let lab = lab().await;
    let dir = tempfile::tempdir().expect("dir");
    let registry = AnswerRegistry::default();
    let (start, handed, _exit) = starter();
    let request = request(
        &lab,
        rdp_profile(ssh::LOOPBACK, lab.server.port, Some("gw")),
        ssh::options_empty(dir.path()),
        CancellationToken::new(),
    );
    let mut events = Box::pin(mstsc_route_events(request, registry.clone(), start));
    let asked = next(&mut events, &registry, ssh::PASSWORD).await;
    assert!(
        matches!(
            &asked,
            Some(MstscRouteEvent::Route(ConnectionEvent::UnknownHostKey { port, .. }))
                if *port == lab.gateway.port
        ),
        "{asked:?}"
    );
    assert!(next(&mut events, &registry, ssh::PASSWORD).await.is_none());
    assert!(handed.try_recv().is_err(), "never started");
}

// ---- the application's side ---------------------------------------------------------------

const GATEWAY_KEY: &str =
    include_str!("../../heimdall-ssh/tests/fixtures/hostkeys/host-ed25519.pub");
const FINGERPRINT: &str = "SHA256:47DEQpj8HBSa+/TImW+5JCeuQeRkm5NMpJWZG3hSuFU";

/// An application with the RDP profile "far", set to Remote Desktop Connection behind the
/// gateway "bastion", changed by `change`.
fn app_with(dir: &Path, change: impl FnOnce(&mut RdpProfile)) -> App {
    let profiles_file = dir.join("profiles.toml");
    let mut store = ProfileStore::open(&profiles_file).expect("store");
    store.merge_gateways(vec![SshGateway {
        id: ProfileId::new("bastion"),
        name: "Bastion".to_owned(),
        host: "bastion.lab".to_owned(),
        port: 22,
        username: Some("jump".to_owned()),
        key_path: None,
        parent: None,
    }]);
    let mut profile = rdp_profile("far.lab", 3389, Some("bastion"));
    change(&mut profile);
    store.merge_rdp([profile]);
    store.save().expect("save");
    App::new(AppConfig {
        profiles_file,
        known_hosts: dir.join("known_hosts"),
        legacy_dir: None,
        agent: AgentSource::Disabled,
        initial_grid: GridSize { cols: 80, rows: 24 },
        files_start: dir.to_owned(),
        system_credentials: SystemCredentials::memory(),
    })
}

/// Opens "far": the launch through its gateway it asks for.
fn open_routed(app: &mut App) -> (MstscRouteId, Box<MstscRouteRequest>) {
    let effects = app.update(Message::OpenRdp(ProfileId::new("far")));
    let mut effects = effects.into_iter();
    let (Some(Effect::OpenMstscRoute { id, request }), None) = (effects.next(), effects.next())
    else {
        panic!("one launch through the gateway");
    };
    assert!(app.tabs.is_empty(), "no tab");
    (id, request)
}

#[test]
fn an_external_profile_behind_a_gateway_is_launched_through_it_not_refused() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app_with(dir.path(), |_| {});
    let (id, request) = open_routed(&mut app);
    assert!(request.before.is_empty());
    assert_eq!(
        (request.gateway.host.as_str(), request.gateway.port),
        ("bastion.lab", 22)
    );
    assert_eq!(
        (request.profile.host.as_str(), request.profile.port),
        ("far.lab", 3389)
    );
    assert_eq!(request.ssh.known_hosts, dir.path().join("known_hosts"));
    assert_eq!(app.notice(), None);

    app.update(Message::MstscRoute {
        id,
        event: MstscRouteEvent::Launched(Ok(())),
    });
    assert_eq!(
        app.notice(),
        Some(&Notice::RdpExternalLaunchedThrough {
            name: "Far".to_owned(),
            gateway: "Bastion".to_owned(),
        })
    );
    // Its gateway's questions are answered from what is saved, here nothing.
    let question = heimdall_app::QuestionId::fresh();
    let asked = |app: &mut App| {
        app.update(Message::MstscRoute {
            id,
            event: MstscRouteEvent::Route(ConnectionEvent::Question {
                question,
                kind: QuestionKind::Password(heimdall_ssh::PasswordQuestion {
                    host: "bastion.lab".to_owned(),
                    port: 22,
                    username: "jump".to_owned(),
                    attempt: 1,
                }),
            }),
        })
    };
    assert!(
        matches!(
            asked(&mut app).as_slice(),
            [Effect::Answer { answer: None, .. }]
        ),
        "nothing saved"
    );
    app.update(Message::MstscRoute {
        id,
        event: MstscRouteEvent::Released,
    });
    assert!(
        matches!(
            asked(&mut app).as_slice(),
            [Effect::Answer { answer: None, .. }]
        ),
        "released: declined"
    );
}

#[test]
fn an_rd_gateway_profile_behind_an_ssh_gateway_goes_through_the_ssh_gateway() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app_with(dir.path(), |profile| {
        profile.extras.external = false;
        profile.extras.rd_gateway = Some("rdg.lab".to_owned());
    });
    let (_, request) = open_routed(&mut app);
    assert_eq!(request.gateway.host, "bastion.lab");
    assert_eq!(request.profile.extras.rd_gateway(), Some("rdg.lab"));
}

#[test]
fn a_gateway_that_fails_stops_the_launch_with_its_reason() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app_with(dir.path(), |_| {});
    let (id, _) = open_routed(&mut app);
    let effects = app.update(Message::MstscRoute {
        id,
        event: MstscRouteEvent::Route(ConnectionEvent::Failed(UiError::ConnectionLost)),
    });
    assert!(effects.is_empty(), "{effects:?}");
    assert_eq!(
        app.notice(),
        Some(&Notice::RdpExternalRefused(ExternalRefusal::Gateway(
            UiError::ConnectionLost
        )))
    );
    // Gone: a late launch is not said.
    app.update(Message::MstscRoute {
        id,
        event: MstscRouteEvent::Launched(Ok(())),
    });
    assert!(matches!(app.notice(), Some(Notice::RdpExternalRefused(_))));
}

#[test]
fn the_gateway_s_unknown_key_is_asked_then_launched_again_or_refused() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app_with(dir.path(), |_| {});
    let key = || Arc::new(PublicKey::from_openssh(GATEWAY_KEY.trim()).expect("key"));
    let unknown = |id| Message::MstscRoute {
        id,
        event: MstscRouteEvent::Route(ConnectionEvent::UnknownHostKey {
            host: "bastion.lab".to_owned(),
            port: 22,
            fingerprint: FINGERPRINT.to_owned(),
            key: key(),
        }),
    };

    // Refused: nothing launched, the reason said.
    let (id, _) = open_routed(&mut app);
    assert!(app.update(unknown(id)).is_empty());
    assert!(matches!(
        &app.dialog,
        Some(Dialog::TunnelHostKey { host, port: 22, .. }) if host == "bastion.lab"
    ));
    assert!(app.update(Message::DismissDialog).is_empty());
    assert_eq!(
        app.notice(),
        Some(&Notice::RdpExternalRefused(ExternalRefusal::Gateway(
            UiError::Cancelled
        )))
    );
    assert!(
        KnownHosts::new(dir.path().join("known_hosts"))
            .recorded("bastion.lab", 22)
            .expect("read")
            .is_empty(),
        "never learnt"
    );

    // Accepted: learnt for the gateway, and the launch goes again.
    let (id, _) = open_routed(&mut app);
    app.update(unknown(id));
    let effects = app.update(Message::ConfirmDialog);
    assert!(
        matches!(effects.as_slice(), [Effect::OpenMstscRoute { id: again, .. }] if *again != id),
        "{effects:?}"
    );
    assert_eq!(
        KnownHosts::new(dir.path().join("known_hosts"))
            .recorded("bastion.lab", 22)
            .expect("read")
            .len(),
        1
    );
}

#[test]
fn closing_the_application_releases_every_forward() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app_with(dir.path(), |_| {});
    let (id, request) = open_routed(&mut app);
    app.update(Message::MstscRoute {
        id,
        event: MstscRouteEvent::Launched(Ok(())),
    });
    assert!(!request.cancel.is_cancelled());
    let effects = app.update(Message::WindowCloseRequested);
    assert!(
        effects.iter().any(|effect| matches!(effect, Effect::Exit)),
        "{effects:?}"
    );
    assert!(request.cancel.is_cancelled(), "released at exit");
}
