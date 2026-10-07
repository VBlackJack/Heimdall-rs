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

//! `PuTTY` opened through an SSH gateway, against two in-process servers: the gateway signed
//! in to, the server's key probed through it, a forward on the loopback address to the
//! server, and `PuTTY` pointed at it. No `PuTTY` is started: the starter is the test's.

#[path = "../../heimdall-ssh/tests/common/mod.rs"]
mod ssh;

use std::ffi::OsString;
use std::path::Path;
use std::sync::mpsc as std_mpsc;

use heimdall_app::putty::{self, HostKeyProbe, PuttyLaunch, PuttyRefusal, PuttyStarted, Running};
use heimdall_app::putty_driver::{PuttyRouteEvent, PuttyRouteRequest, putty_route_events};
use heimdall_app::{AnswerRegistry, ConnectionEvent, QuestionKind};
use heimdall_core::profile::{ProfileId, SshMode};
use heimdall_core::settings::Settings;
use heimdall_ssh::{ConnectOptions, KnownHosts, Secret, fingerprint};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::sync::oneshot;
use tokio_stream::{Stream, StreamExt as _};
use tokio_util::sync::CancellationToken;

/// What an SSH server says first, which the forward carries back from the server.
const SSH_BANNER: &[u8] = b"SSH-2.0-";

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

/// Options trusting the gateway, and the server too when `server_known`.
fn options(dir: &Path, lab: &Lab, server_known: bool) -> ConnectOptions {
    let options = ssh::options_trusting(dir, lab.gateway.port, "host-ed25519");
    if server_known {
        KnownHosts::new(&options.known_hosts)
            .learn(
                ssh::LOOPBACK,
                lab.server.port,
                &ssh::host_public_key("host-ed25519"),
            )
            .expect("learn");
    }
    options
}

fn request(lab: &Lab, options: ConnectOptions, cancel: CancellationToken) -> PuttyRouteRequest {
    let mut profile = ssh::profile(lab.server.port, None);
    profile.gateway = Some(ProfileId::new("gw"));
    profile.ssh_mode = SshMode::External;
    PuttyRouteRequest {
        before: Vec::new(),
        gateway: ssh::profile(lab.gateway.port, None),
        launch: putty::plan(&profile, &Settings::default(), String::new()),
        profile,
        ssh: options,
        cancel,
    }
}

/// A starter that hands the launch it was given to the test, and a `PuTTY` that exits when
/// the test says so.
fn starter() -> (
    impl FnOnce(&PuttyLaunch) -> Result<Running, PuttyRefusal> + Send + 'static,
    std_mpsc::Receiver<PuttyLaunch>,
    oneshot::Sender<()>,
) {
    let (hand, handed) = std_mpsc::channel();
    let (exit, exited) = oneshot::channel::<()>();
    let start = move |launch: &PuttyLaunch| {
        hand.send(launch.clone()).expect("the test listens");
        Ok(Running {
            started: PuttyStarted { x11: None },
            exited: Box::pin(async move {
                let _ = exited.await;
            }),
        })
    };
    (start, handed, exit)
}

/// The next event that is not a password question, each one answered with `password`.
async fn next(
    events: &mut (impl Stream<Item = PuttyRouteEvent> + Unpin),
    registry: &AnswerRegistry,
    password: &str,
) -> Option<PuttyRouteEvent> {
    loop {
        let event = tokio::time::timeout(ssh::STEP_TIMEOUT, events.next())
            .await
            .expect("in time")?;
        match event {
            PuttyRouteEvent::Route(ConnectionEvent::Question {
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

fn strings(arguments: &[OsString]) -> Vec<String> {
    arguments
        .iter()
        .map(|argument| argument.to_string_lossy().into_owned())
        .collect()
}

#[tokio::test]
async fn putty_goes_to_a_loopback_forward_with_the_server_s_key_until_it_exits() {
    let lab = lab().await;
    let dir = tempfile::tempdir().expect("dir");
    let registry = AnswerRegistry::default();
    let (start, handed, exit) = starter();
    let request = request(
        &lab,
        options(dir.path(), &lab, true),
        CancellationToken::new(),
    );
    let mut events = Box::pin(putty_route_events(request, registry.clone(), start));

    let launched = next(&mut events, &registry, ssh::PASSWORD).await;
    assert!(
        matches!(launched, Some(PuttyRouteEvent::Launched(Ok(_)))),
        "{launched:?}"
    );
    let launch = handed.try_recv().expect("PuTTY started once");
    let arguments = strings(&putty::arguments(&launch, false));
    let server_key = fingerprint(&ssh::host_public_key("host-ed25519"));
    let local = launch.port;
    assert_eq!(
        arguments,
        [
            "-ssh",
            "-P",
            &local.to_string(),
            "-hostkey",
            &server_key,
            "tester@127.0.0.1"
        ]
    );
    assert_ne!(
        local, lab.server.port,
        "the forward's port, not the server's"
    );

    // The forward reaches the server through the gateway.
    let mut client = tokio::net::TcpStream::connect((ssh::LOOPBACK, local))
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
    drop(client);
    let onward = lab
        .gateway
        .observed
        .lock()
        .expect("observed")
        .forwards
        .clone();
    let to_server = (ssh::LOOPBACK.to_owned(), u32::from(lab.server.port));
    assert!(
        onward.len() >= 2 && onward.iter().all(|forward| *forward == to_server),
        "the probe and the forward both went through the gateway: {onward:?}"
    );

    // PuTTY exits: the forward goes.
    exit.send(()).expect("the driver waits");
    let released = next(&mut events, &registry, ssh::PASSWORD).await;
    assert!(
        matches!(released, Some(PuttyRouteEvent::Released)),
        "{released:?}"
    );
    tokio::task::yield_now().await;
    if let Ok(mut late) = tokio::net::TcpStream::connect((ssh::LOOPBACK, local)).await {
        let mut read = [0; 1];
        let closed = tokio::time::timeout(ssh::STEP_TIMEOUT, late.read(&mut read))
            .await
            .expect("closed in time");
        assert!(matches!(closed, Ok(0) | Err(_)), "{closed:?}");
    }
    assert!(next(&mut events, &registry, ssh::PASSWORD).await.is_none());
}

#[tokio::test]
async fn cancelled_while_putty_runs_the_forward_is_released() {
    let lab = lab().await;
    let dir = tempfile::tempdir().expect("dir");
    let registry = AnswerRegistry::default();
    let (start, _launches, _exit) = starter();
    let cancel = CancellationToken::new();
    let request = request(&lab, options(dir.path(), &lab, true), cancel.clone());
    let mut events = Box::pin(putty_route_events(request, registry.clone(), start));
    let launched = next(&mut events, &registry, ssh::PASSWORD).await;
    assert!(
        matches!(launched, Some(PuttyRouteEvent::Launched(Ok(_)))),
        "{launched:?}"
    );
    cancel.cancel();
    let released = next(&mut events, &registry, ssh::PASSWORD).await;
    assert!(
        matches!(released, Some(PuttyRouteEvent::Released)),
        "{released:?}"
    );
}

#[tokio::test]
async fn an_unknown_server_key_probed_through_the_gateway_is_asked_about() {
    let lab = lab().await;
    let dir = tempfile::tempdir().expect("dir");
    let registry = AnswerRegistry::default();
    let (start, handed, _exit) = starter();
    let request = request(
        &lab,
        options(dir.path(), &lab, false),
        CancellationToken::new(),
    );
    let mut events = Box::pin(putty_route_events(request, registry.clone(), start));
    let asked = next(&mut events, &registry, ssh::PASSWORD).await;
    let Some(PuttyRouteEvent::HostKey(HostKeyProbe::Unknown {
        host,
        port,
        fingerprint: presented,
        ..
    })) = asked
    else {
        panic!("{asked:?}");
    };
    // The server's own host and port, never the forward's.
    assert_eq!((host.as_str(), port), (ssh::LOOPBACK, lab.server.port));
    assert_eq!(
        presented,
        fingerprint(&ssh::host_public_key("host-ed25519"))
    );
    assert!(next(&mut events, &registry, ssh::PASSWORD).await.is_none());
    assert!(handed.try_recv().is_err(), "PuTTY never started");
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
        "probed through the gateway"
    );
}

#[tokio::test]
async fn a_gateway_that_refuses_the_user_stops_the_launch() {
    let lab = lab().await;
    let dir = tempfile::tempdir().expect("dir");
    let registry = AnswerRegistry::default();
    let (start, handed, _exit) = starter();
    let request = request(
        &lab,
        options(dir.path(), &lab, true),
        CancellationToken::new(),
    );
    let mut events = Box::pin(putty_route_events(request, registry.clone(), start));
    let failed = next(&mut events, &registry, "wrong").await;
    assert!(
        matches!(
            failed,
            Some(PuttyRouteEvent::Route(ConnectionEvent::Failed(_)))
        ),
        "{failed:?}"
    );
    assert!(next(&mut events, &registry, "wrong").await.is_none());
    assert!(handed.try_recv().is_err(), "PuTTY never started");
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
async fn an_unknown_gateway_key_is_asked_about_before_anything_goes_onward() {
    let lab = lab().await;
    let dir = tempfile::tempdir().expect("dir");
    let registry = AnswerRegistry::default();
    let (start, handed, _exit) = starter();
    let request = request(
        &lab,
        ssh::options_empty(dir.path()),
        CancellationToken::new(),
    );
    let mut events = Box::pin(putty_route_events(request, registry.clone(), start));
    let asked = next(&mut events, &registry, ssh::PASSWORD).await;
    assert!(
        matches!(
            &asked,
            Some(PuttyRouteEvent::Route(ConnectionEvent::UnknownHostKey { port, .. }))
                if *port == lab.gateway.port
        ),
        "{asked:?}"
    );
    assert!(handed.try_recv().is_err(), "PuTTY never started");
}
