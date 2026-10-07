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

//! The host key probe an external client is launched after: the server's key checked as a
//! connection checks it, and nobody signed in.

mod common;

use common::{
    LOOPBACK, STEP_TIMEOUT, Spec, host_public_key, options_empty, options_trusting, profile, start,
};
use heimdall_ssh::{ConnectError, KnownHosts, Pins, PublicKey, fingerprint, trusted_host_key};
use tokio_util::sync::CancellationToken;

async fn probe(
    port: u16,
    options: &heimdall_ssh::ConnectOptions,
) -> Result<PublicKey, ConnectError> {
    tokio::time::timeout(
        STEP_TIMEOUT,
        trusted_host_key(&profile(port, None), options, &CancellationToken::new()),
    )
    .await
    .expect("the probe finished in time")
}

#[tokio::test]
async fn a_trusted_key_is_returned_and_nobody_signs_in() {
    let server = start(Spec::default()).await;
    let dir = tempfile::tempdir().expect("temp dir");
    let options = options_trusting(dir.path(), server.port, "host-ed25519");

    let key = probe(server.port, &options).await.expect("trusted");
    assert_eq!(key.key_data(), host_public_key("host-ed25519").key_data());
    assert!(fingerprint(&key).starts_with("SHA256:"));
    let observed = server.observed.lock().expect("observed").clone();
    assert_eq!(observed.password_attempts, 0, "no password sent");
    assert!(observed.publickey_offers.is_empty(), "no key offered");
}

#[tokio::test]
async fn an_unknown_key_is_returned_to_ask_about_and_nothing_is_written() {
    let server = start(Spec::default()).await;
    let dir = tempfile::tempdir().expect("temp dir");
    let options = options_empty(dir.path());

    let error = probe(server.port, &options).await.expect_err("unknown");
    let ConnectError::UnknownHostKey { host, port, key } = error else {
        panic!("expected UnknownHostKey, got {error:?}");
    };
    assert_eq!((host.as_str(), port), (LOOPBACK, server.port));
    assert!(!options.known_hosts.exists(), "nothing written unasked");
    // Learnt, the next probe trusts it.
    KnownHosts::new(&options.known_hosts)
        .learn(&host, port, &key)
        .expect("learn");
    let trusted = probe(server.port, &options).await.expect("trusted now");
    assert_eq!(trusted.key_data(), key.key_data());
}

#[tokio::test]
async fn a_changed_key_is_refused() {
    let server = start(Spec::default()).await;
    let dir = tempfile::tempdir().expect("temp dir");
    let options = options_trusting(dir.path(), server.port, "host-ed25519-other");

    let error = probe(server.port, &options).await.expect_err("changed");
    assert!(
        matches!(error, ConnectError::HostKeyChanged { .. }),
        "{error:?}"
    );
}

#[tokio::test]
async fn a_key_trusted_for_the_run_or_pinned_is_trusted_and_a_pin_is_recorded_in_full() {
    let server = start(Spec::default()).await;
    let dir = tempfile::tempdir().expect("temp dir");
    let mut options = options_empty(dir.path());
    let trust = heimdall_ssh::RunTrust::default();
    trust.trust(LOOPBACK, server.port, host_public_key("host-ed25519"));
    options.run_trust = trust;
    probe(server.port, &options)
        .await
        .expect("trusted for the run");
    assert!(!options.known_hosts.exists(), "never written");

    let pinned_dir = tempfile::tempdir().expect("temp dir");
    let options = options_empty(pinned_dir.path());
    let pins = Pins::beside(&options.known_hosts);
    let pinned = fingerprint(&host_public_key("host-ed25519"));
    pins.pin(LOOPBACK, server.port, &pinned).expect("pinned");
    probe(server.port, &options).await.expect("pinned");
    let recorded = KnownHosts::new(&options.known_hosts)
        .recorded(LOOPBACK, server.port)
        .expect("read");
    assert_eq!(recorded.len(), 1, "the pinned key recorded in full");
}
