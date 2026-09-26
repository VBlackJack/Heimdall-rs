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

//! Against a real RDP server, opt-in.
//!
//! Runs only when `HEIMDALL_LIVE_RDP_PORT` names a local port served by the Heimdall-TestEnv
//! xrdp image. xrdp has no Network Level Authentication, so it is reached with plain TLS;
//! no credential is needed, as its login screen is enough to see a session draw.

#![allow(
    clippy::large_futures,
    reason = "a connection future is large; tests await it once"
)]

use std::time::Duration;

use heimdall_rdp::session::{self, RdpEvent};
use heimdall_rdp::{KnownRdpHosts, RdpConfig, RdpError, Security, Timeouts, connect};
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;

const PORT_VARIABLE: &str = "HEIMDALL_LIVE_RDP_PORT";

/// Bound on waiting for the first picture.
const FIRST_PICTURE: Duration = Duration::from_secs(20);

fn live_port() -> Option<u16> {
    std::env::var(PORT_VARIABLE).ok()?.parse().ok()
}

fn config(port: u16, known: &std::path::Path, security: Security) -> RdpConfig {
    RdpConfig {
        host: "127.0.0.1".to_owned(),
        port,
        username: "nobody".to_owned(),
        domain: None,
        desktop: (1024, 768),
        keyboard_layout: 0,
        security,
        known_hosts: KnownRdpHosts::new(known),
        accepted: None,
        timeouts: Timeouts::default(),
    }
}

#[tokio::test]
async fn a_server_without_nla_is_refused_when_nla_is_required() {
    let Some(port) = live_port() else {
        eprintln!("{PORT_VARIABLE} is not set; skipped");
        return;
    };
    let dir = tempfile::tempdir().expect("dir");
    let outcome = connect(
        &config(port, &dir.path().join("known"), Security::Nla),
        &Zeroizing::new(String::new()),
        &CancellationToken::new(),
    )
    .await;
    assert!(
        matches!(outcome, Err(RdpError::Negotiation(_))),
        "{:?}",
        outcome.map(|_| ())
    );
}

#[tokio::test]
async fn a_trusted_server_draws_its_login_screen() {
    let Some(port) = live_port() else {
        eprintln!("{PORT_VARIABLE} is not set; skipped");
        return;
    };
    let dir = tempfile::tempdir().expect("dir");
    let mut config = config(port, &dir.path().join("known"), Security::NlaOrTls);
    let password = Zeroizing::new(String::new());
    let cancel = CancellationToken::new();
    let outcome = connect(&config, &password, &cancel).await;
    let Err(RdpError::UnknownCertificate(certificate)) = outcome else {
        panic!("{:?}", outcome.map(|_| ()));
    };
    // What the user does on the question: accept, and connect again.
    config.accepted = Some(certificate.fingerprint);
    let connection = connect(&config, &password, &cancel)
        .await
        .expect("connected");
    let mut session = session::start(connection, cancel.clone());
    let drawn = tokio::time::timeout(FIRST_PICTURE, async {
        while let Some(event) = session.events.recv().await {
            match event {
                RdpEvent::Updated { .. } => {
                    let colours = session.framebuffer.read(|_, _, pixels| {
                        let mut seen: Vec<&[u8; 4]> = pixels.as_chunks::<4>().0.iter().collect();
                        seen.sort_unstable();
                        seen.dedup();
                        seen.len()
                    });
                    eprintln!("colours on screen: {colours}");
                    if colours > 1 {
                        return true;
                    }
                }
                RdpEvent::Closed(reason) => panic!("closed: {reason:?}"),
                RdpEvent::Resized { .. } => {}
            }
        }
        false
    })
    .await
    .expect("a picture in time");
    assert!(drawn, "more than one colour on screen");
    cancel.cancel();
}
