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

//! A gateway route tested hop by hop, as the C# "Test route", against in-process servers:
//! each outcome told apart, nothing dialled without a trusted key, nothing written.

mod common;

use std::path::Path;
use std::sync::{Arc, Mutex};

use common::{
    KbdRound, LOOPBACK, PASSWORD, STEP_TIMEOUT, Spec, TestServer, host_public_key, options_empty,
    profile, start,
};
use heimdall_core::profile::SshProfile;
use heimdall_ssh::{
    ConnectOptions, HopSecrets, KnownHosts, Outcome, Pins, Secret, Step, StepOf, diagnose_route,
    fingerprint,
};
use russh::MethodKind;
use tokio_util::sync::CancellationToken;

fn gateway_spec(methods: Vec<MethodKind>) -> Spec {
    Spec {
        methods,
        forwarding: true,
        ..Spec::default()
    }
}

/// Options whose `known_hosts` trusts every server in `trusted`.
fn trusting(dir: &Path, trusted: &[&TestServer]) -> ConnectOptions {
    let options = options_empty(dir);
    for server in trusted {
        KnownHosts::new(&options.known_hosts)
            .learn(LOOPBACK, server.port, &host_public_key("host-ed25519"))
            .expect("learn");
    }
    options
}

fn password(value: &str) -> HopSecrets {
    HopSecrets {
        password: Some(Secret::new(value.to_owned())),
        passphrase: None,
    }
}

/// The outcomes of a test of `hops`, each with `secrets`, then `destination`.
async fn outcomes(
    hops: &[SshProfile],
    secrets: &[HopSecrets],
    destination: Option<(String, u16)>,
    options: &ConnectOptions,
) -> Vec<(StepOf, Outcome)> {
    let steps: Arc<Mutex<Vec<Step>>> = Arc::default();
    let seen = Arc::clone(&steps);
    tokio::time::timeout(
        STEP_TIMEOUT * 3,
        diagnose_route(
            hops,
            secrets,
            destination,
            options,
            CancellationToken::new(),
            move |step| seen.lock().expect("steps").push(step),
        ),
    )
    .await
    .expect("ended in time");
    let steps = steps.lock().expect("steps");
    steps.iter().map(|step| (step.of, step.outcome)).collect()
}

/// A port nothing listens on, held so that nothing else takes it while the test runs.
fn closed_port() -> (tokio::net::TcpSocket, u16) {
    let socket = tokio::net::TcpSocket::new_v4().expect("socket");
    socket
        .bind("127.0.0.1:0".parse().expect("address"))
        .expect("bind");
    let port = socket.local_addr().expect("address").port();
    (socket, port)
}

#[tokio::test]
async fn two_gateways_then_a_destination_each_pass_in_turn() {
    let first = start(gateway_spec(vec![MethodKind::Password])).await;
    let second = start(gateway_spec(vec![MethodKind::Password])).await;
    let destination = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let target = destination.local_addr().expect("address").port();
    let dir = tempfile::tempdir().expect("dir");
    let options = trusting(dir.path(), &[&first, &second]);
    let seen = outcomes(
        &[profile(first.port, None), profile(second.port, None)],
        &[password(PASSWORD), password(PASSWORD)],
        Some((LOOPBACK.to_owned(), target)),
        &options,
    )
    .await;
    assert_eq!(
        seen,
        [
            (StepOf::Gateway(0), Outcome::Passed),
            (StepOf::Gateway(1), Outcome::Passed),
            (StepOf::Destination, Outcome::Passed),
        ]
    );
}

#[tokio::test]
async fn a_gateway_without_a_trusted_key_is_not_dialled() {
    // Nothing listens there: had it been dialled, the step would say the network failed.
    let (_held, port) = closed_port();
    let dir = tempfile::tempdir().expect("dir");
    let seen = outcomes(
        &[profile(port, None)],
        &[password(PASSWORD)],
        None,
        &options_empty(dir.path()),
    )
    .await;
    assert_eq!(seen, [(StepOf::Gateway(0), Outcome::TrustRequired)]);
}

#[tokio::test]
async fn a_changed_key_a_refused_password_and_a_question_only_a_person_answers_are_told_apart() {
    let dir = tempfile::tempdir().expect("dir");

    let changed = start(gateway_spec(vec![MethodKind::Password])).await;
    let options = options_empty(dir.path());
    KnownHosts::new(&options.known_hosts)
        .learn(
            LOOPBACK,
            changed.port,
            &host_public_key("host-ed25519-other"),
        )
        .expect("learn another key");
    let seen = outcomes(
        &[profile(changed.port, None)],
        &[password(PASSWORD)],
        None,
        &options,
    )
    .await;
    assert_eq!(seen, [(StepOf::Gateway(0), Outcome::TrustChanged)]);

    let refusing = start(gateway_spec(vec![MethodKind::Password])).await;
    let options = trusting(dir.path(), &[&refusing]);
    let seen = outcomes(
        &[profile(refusing.port, None)],
        &[password("wrong")],
        None,
        &options,
    )
    .await;
    assert_eq!(
        seen,
        [(StepOf::Gateway(0), Outcome::Auth)],
        "asked again: not answered"
    );

    // A one-time code, asked by the gateway.
    let interactive = start(Spec {
        kbd_rounds: vec![KbdRound {
            prompts: vec![("Verification code: ".to_owned(), false)],
            expected: vec!["123456".to_owned()],
        }],
        ..gateway_spec(vec![MethodKind::KeyboardInteractive])
    })
    .await;
    let options = trusting(dir.path(), &[&interactive]);
    let seen = outcomes(
        &[profile(interactive.port, None)],
        &[password(PASSWORD)],
        None,
        &options,
    )
    .await;
    assert_eq!(seen, [(StepOf::Gateway(0), Outcome::Interactive)]);
}

#[tokio::test]
async fn a_destination_the_gateway_cannot_reach_is_a_forwarding_failure() {
    let gateway = start(gateway_spec(vec![MethodKind::Password])).await;
    let (_held, closed) = closed_port();
    let dir = tempfile::tempdir().expect("dir");
    let options = trusting(dir.path(), &[&gateway]);
    let seen = outcomes(
        &[profile(gateway.port, None)],
        &[password(PASSWORD)],
        Some((LOOPBACK.to_owned(), closed)),
        &options,
    )
    .await;
    assert_eq!(
        seen,
        [
            (StepOf::Gateway(0), Outcome::Passed),
            (StepOf::Destination, Outcome::Forwarding),
        ]
    );
}

#[tokio::test]
async fn a_pinned_gateway_is_tested_without_writing_anything() {
    let gateway = start(gateway_spec(vec![MethodKind::Password])).await;
    let dir = tempfile::tempdir().expect("dir");
    let options = options_empty(dir.path());
    let pins = Pins::beside(&options.known_hosts);
    pins.pin(
        LOOPBACK,
        gateway.port,
        &fingerprint(&host_public_key("host-ed25519")),
    )
    .expect("pin");
    let seen = outcomes(
        &[profile(gateway.port, None)],
        &[password(PASSWORD)],
        None,
        &options,
    )
    .await;
    assert_eq!(
        seen,
        [(StepOf::Gateway(0), Outcome::Passed)],
        "a pin is trusted"
    );
    assert!(
        KnownHosts::new(&options.known_hosts)
            .recorded(LOOPBACK, gateway.port)
            .expect("read")
            .is_empty(),
        "the key is not recorded"
    );
    assert_eq!(
        pins.pinned(LOOPBACK, gateway.port).expect("read").len(),
        1,
        "the pin stays"
    );
}

#[tokio::test]
async fn a_test_stopped_by_the_user_says_so() {
    let gateway = start(gateway_spec(vec![MethodKind::Password])).await;
    let dir = tempfile::tempdir().expect("dir");
    let options = trusting(dir.path(), &[&gateway]);
    let cancel = CancellationToken::new();
    cancel.cancel();
    let steps: Arc<Mutex<Vec<Step>>> = Arc::default();
    let seen = Arc::clone(&steps);
    diagnose_route(
        &[profile(gateway.port, None)],
        &[password(PASSWORD)],
        None,
        &options,
        cancel,
        move |step| seen.lock().expect("steps").push(step),
    )
    .await;
    let steps = steps.lock().expect("steps");
    assert_eq!(
        steps.iter().map(|step| step.outcome).collect::<Vec<_>>(),
        [Outcome::Cancelled]
    );
}
