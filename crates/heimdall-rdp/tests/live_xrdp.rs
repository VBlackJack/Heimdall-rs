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

use heimdall_core::profile::{ColorDepth, RdpOptions};
use heimdall_rdp::session::{self, CloseReason, RdpEvent};
use heimdall_rdp::{
    KnownRdpHosts, MouseButton, MousePosition, Operation, RdpConfig, RdpError, Security, Timeouts,
    connect, given,
};
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
        domain: None,
        desktop: (1024, 768),
        keyboard_layout: 0,
        security,
        known_hosts: KnownRdpHosts::new(known),
        accepted: None,
        timeouts: Timeouts::default(),
        // The channel negotiated with a real server must not break the session.
        clipboard: true,
        drives: Vec::new(),
        trusted_for_run: Vec::new(),
        options: RdpOptions::default(),
        several_servers: false,
        kerberos: false,
        time_zone: None,
        desktop_scale: 100,
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
        config(port, &dir.path().join("known"), Security::Nla),
        given("nobody".to_owned(), Zeroizing::new(String::new())),
        CancellationToken::new(),
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
    let outcome = connect(
        config.clone(),
        given("nobody".to_owned(), password.clone()),
        cancel.clone(),
    )
    .await;
    let Err(RdpError::UnknownCertificate(certificate)) = outcome else {
        panic!("{:?}", outcome.map(|_| ()));
    };
    // What the user does on the question: accept, and connect again.
    config.accepted = Some(certificate.fingerprint);
    let connection = connect(config, given("nobody".to_owned(), password), cancel.clone())
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
                RdpEvent::Resized { .. } | RdpEvent::RemoteClipboard(_) => {}
            }
        }
        false
    })
    .await
    .expect("a picture in time");
    assert!(drawn, "more than one colour on screen");
    // Inside the login window, the white areas (the logo, the fields) have straight left
    // edges: on every row of the middle band, the first white pixel sits in the same column.
    // Bitmap rows read with the wrong width shift, and that column wanders (measured: 11
    // columns with the published IronRDP 0.11, 1 with the vendored fix).
    tokio::time::sleep(Duration::from_secs(2)).await;
    let starts: Vec<Option<usize>> = session.framebuffer.read(|width, height, pixels| {
        let (width, height) = (usize::from(width), usize::from(height));
        (height / 3..2 * height / 3)
            .map(|y| {
                pixels[y * width * 4..(y + 1) * width * 4]
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .position(|pixel| *pixel == [255, 255, 255, 255])
            })
            .collect()
    });
    let mut columns: Vec<usize> = starts.iter().flatten().copied().collect();
    columns.sort_unstable();
    columns.dedup();
    assert_eq!(
        columns.len(),
        1,
        "white areas with a ragged left edge: {columns:?}"
    );
    // For a visual pass: the raw desktop, width and height first (little-endian u16).
    if let Some(dir) = std::env::var_os("HEIMDALL_SNAPSHOT_DIR") {
        tokio::time::sleep(Duration::from_secs(2)).await;
        session.framebuffer.read(|width, height, pixels| {
            let mut raw = Vec::with_capacity(4 + pixels.len());
            raw.extend_from_slice(&width.to_le_bytes());
            raw.extend_from_slice(&height.to_le_bytes());
            raw.extend_from_slice(pixels);
            std::fs::write(std::path::Path::new(&dir).join("xrdp.rgba"), raw).expect("written");
        });
    }
    cancel.cancel();
}

/// Whether a pixel is white, give or take what 16 bits per pixel lose.
fn near_white(pixel: [u8; 4]) -> bool {
    pixel[..3].iter().all(|channel| *channel >= NEAR_WHITE)
}

/// Lowest channel value counted as white: 31 on 5 bits is 248 once shifted.
const NEAR_WHITE: u8 = 240;

/// Values a 5-bit red channel can take.
const RED_VALUES_AT_16_BITS: usize = 32;

#[tokio::test]
async fn each_colour_depth_draws_the_login_screen() {
    let Some(port) = live_port() else {
        eprintln!("{PORT_VARIABLE} is not set; skipped");
        return;
    };
    for depth in [ColorDepth::Bpp16, ColorDepth::Bpp24] {
        let dir = tempfile::tempdir().expect("dir");
        let mut config = config(port, &dir.path().join("known"), Security::NlaOrTls);
        config.options = RdpOptions {
            color_depth: depth,
            ..RdpOptions::default()
        };
        let password = Zeroizing::new(String::new());
        let cancel = CancellationToken::new();
        let outcome = connect(
            config.clone(),
            given("nobody".to_owned(), password.clone()),
            cancel.clone(),
        )
        .await;
        let Err(RdpError::UnknownCertificate(certificate)) = outcome else {
            panic!("{depth:?}: {:?}", outcome.map(|_| ()));
        };
        config.accepted = Some(certificate.fingerprint);
        let connection = connect(config, given("nobody".to_owned(), password), cancel.clone())
            .await
            .expect("connected");
        let mut session = session::start(connection, cancel.clone());
        tokio::time::timeout(FIRST_PICTURE, async {
            while let Some(event) = session.events.recv().await {
                if let RdpEvent::Closed(reason) = event {
                    panic!("{depth:?} closed: {reason:?}");
                }
                let drawn = session.framebuffer.read(|_, _, pixels| {
                    pixels
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .any(|pixel| near_white(*pixel))
                });
                if drawn {
                    return;
                }
            }
        })
        .await
        .expect("a picture in time");
        tokio::time::sleep(Duration::from_secs(2)).await;
        // As at 32 bits: the white areas of the login window have one straight left edge.
        let columns: Vec<usize> = session.framebuffer.read(|width, height, pixels| {
            let (width, height) = (usize::from(width), usize::from(height));
            let mut columns: Vec<usize> = (height / 3..2 * height / 3)
                .filter_map(|y| {
                    pixels[y * width * 4..(y + 1) * width * 4]
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .position(|pixel| near_white(*pixel))
                })
                .collect();
            columns.sort_unstable();
            columns.dedup();
            columns
        });
        assert_eq!(columns.len(), 1, "{depth:?}: ragged left edge {columns:?}");
        // The server drew at the depth asked: in 5-6-5, red takes at most 32 values.
        let reds = session.framebuffer.read(|_, _, pixels| {
            let mut reds: Vec<u8> = pixels.as_chunks::<4>().0.iter().map(|p| p[0]).collect();
            reds.sort_unstable();
            reds.dedup();
            reds.len()
        });
        eprintln!("{depth:?}: {reds} red values");
        if depth == ColorDepth::Bpp16 {
            assert!(
                reds <= RED_VALUES_AT_16_BITS,
                "{reds} red values at 16 bits"
            );
        }
        cancel.cancel();
    }
}

#[tokio::test]
async fn cancelling_the_login_screen_is_a_close_not_a_failure() {
    let Some(port) = live_port() else {
        eprintln!("{PORT_VARIABLE} is not set; skipped");
        return;
    };
    let dir = tempfile::tempdir().expect("dir");
    let mut config = config(port, &dir.path().join("known"), Security::NlaOrTls);
    let password = Zeroizing::new(String::new());
    let cancel = CancellationToken::new();
    let Err(RdpError::UnknownCertificate(certificate)) = connect(
        config.clone(),
        given("nobody".to_owned(), password.clone()),
        cancel.clone(),
    )
    .await
    else {
        panic!("expected the certificate question");
    };
    config.accepted = Some(certificate.fingerprint);
    let connection = connect(config, given("nobody".to_owned(), password), cancel.clone())
        .await
        .expect("connected");
    let mut session = session::start(connection, cancel.clone());
    tokio::time::sleep(Duration::from_secs(3)).await;
    // The Cancel button of xrdp's login window on a 1024x768 desktop. xrdp then sends its
    // short Disconnect Provider Ultimatum, which IronRDP 0.11 does not decode.
    let cancel_button = MousePosition { x: 616, y: 554 };
    session
        .input
        .send(vec![
            Operation::MouseMove(cancel_button),
            Operation::MouseButtonPressed(MouseButton::Left),
            Operation::MouseButtonReleased(MouseButton::Left),
        ])
        .expect("sent");
    let ended = tokio::time::timeout(FIRST_PICTURE, async {
        while let Some(event) = session.events.recv().await {
            if let RdpEvent::Closed(reason) = event {
                return Some(reason);
            }
        }
        None
    })
    .await
    .expect("closed in time");
    assert_eq!(ended, Some(CloseReason::Server));
}

/// The lab account of the xrdp container, from `HEIMDALL_LIVE_XRDP_USER` and
/// `HEIMDALL_LIVE_XRDP_PASSWORD`: resizing needs a logged-in session, the login screen ignores
/// it.
fn lab_account() -> Option<(String, Zeroizing<String>)> {
    Some((
        std::env::var("HEIMDALL_LIVE_XRDP_USER").ok()?,
        Zeroizing::new(std::env::var("HEIMDALL_LIVE_XRDP_PASSWORD").ok()?),
    ))
}

#[tokio::test]
async fn a_logged_in_desktop_follows_the_size_asked_for() {
    // Only against a server that applies Display Control: the lab's xrdp 0.9.21 receives the
    // request (sent once, after settling) and ignores it. Opt in with HEIMDALL_LIVE_RDP_RESIZE.
    let (Some(port), Some((user, password)), true) = (
        live_port(),
        lab_account(),
        std::env::var_os("HEIMDALL_LIVE_RDP_RESIZE").is_some(),
    ) else {
        eprintln!(
            "{PORT_VARIABLE}, the lab account or HEIMDALL_LIVE_RDP_RESIZE is not set; skipped"
        );
        return;
    };
    let dir = tempfile::tempdir().expect("dir");
    let mut config = config(port, &dir.path().join("known"), Security::NlaOrTls);
    let cancel = CancellationToken::new();
    let Err(RdpError::UnknownCertificate(certificate)) = connect(
        config.clone(),
        given(user.clone(), password.clone()),
        cancel.clone(),
    )
    .await
    else {
        panic!("expected the certificate question");
    };
    config.accepted = Some(certificate.fingerprint);
    // The password given, the server logs in without its own form.
    let connection = connect(config, given(user, password), cancel.clone())
        .await
        .expect("connected");
    let mut session = session::start(connection, cancel.clone());
    // Asked twice in a row: only the last size counts, once it has settled.
    session.size.send_replace(Some((900, 700)));
    session.size.send_replace(Some((1000, 700)));
    let resized = tokio::time::timeout(Duration::from_secs(60), async {
        while let Some(event) = session.events.recv().await {
            match event {
                RdpEvent::Resized { width, height } if (width, height) == (1000, 700) => {
                    return (width, height);
                }
                RdpEvent::Closed(reason) => panic!("closed: {reason:?}"),
                _ => {}
            }
        }
        panic!("no more events");
    })
    .await
    .expect("resized in time");
    assert_eq!(resized, (1000, 700));
    let size = session.framebuffer.read(|width, height, _| (width, height));
    assert_eq!(size, (1000, 700));
    cancel.cancel();
}
