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

//! What the application decides for an RDP tab, through `update` and its effects.

use std::path::Path;

use heimdall_app::{
    App, AppConfig, AttemptId, ConnectionEvent, DesktopInput, Effect, Message, Phase, Purpose,
    TabId, TabProfile, UiError,
};
use heimdall_core::profile::{ProfileId, RdpProfile};
use heimdall_core::store::ProfileStore;
use heimdall_rdp::{Fingerprint, Framebuffer, KnownRdpHosts, Operation, Scancode};
use heimdall_ssh::AgentSource;
use heimdall_term::GridSize;
use tokio::sync::mpsc;

const KEY: &str = "SHA256:rgJ0Y04RcqyBpgUaWIbkDqk3bEPjR9jNUxyEc7cvFq0";

fn app(dir: &Path) -> App {
    let profiles_file = dir.join("profiles.toml");
    let mut store = ProfileStore::open(&profiles_file).expect("store");
    store.merge_rdp([RdpProfile {
        id: ProfileId::new("dc"),
        name: "Domain controller".to_owned(),
        group: None,
        host: "dc.lab".to_owned(),
        port: 3389,
        username: Some("admin".to_owned()),
        domain: Some("LAB".to_owned()),
        allow_tls_only: false,
        gateway: None,
        redirect_clipboard: true,
    }]);
    store.save().expect("save");
    App::new(AppConfig {
        profiles_file,
        known_hosts: dir.join("known_hosts"),
        legacy_dir: None,
        agent: AgentSource::Disabled,
        initial_grid: GridSize { cols: 80, rows: 24 },
        files_start: dir.to_owned(),
        system_credentials: heimdall_app::SystemCredentials::memory(),
    })
}

fn key() -> Fingerprint {
    KEY.parse().expect("fingerprint")
}

/// An RDP tab, and the attempt its opening started.
fn open(app: &mut App) -> (TabId, AttemptId) {
    match app
        .update(Message::OpenRdp(ProfileId::new("dc")))
        .as_slice()
    {
        [
            Effect::ConnectRdp {
                tab,
                attempt,
                request,
            },
        ] => {
            assert_eq!(request.profile.host, "dc.lab");
            assert_eq!(request.accepted, None);
            (*tab, *attempt)
        }
        other => panic!("{other:?}"),
    }
}

fn event(app: &mut App, tab: TabId, attempt: AttemptId, event: ConnectionEvent) -> Vec<Effect> {
    app.update(Message::Connection {
        tab,
        attempt,
        event,
    })
}

#[test]
fn an_rdp_profile_opens_an_rdp_tab_trusted_beside_the_ssh_hosts() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let effects = app.update(Message::OpenRdp(ProfileId::new("dc")));
    let [Effect::ConnectRdp { request, .. }] = effects.as_slice() else {
        panic!("{effects:?}");
    };
    assert_eq!(request.known_hosts, dir.path().join("known_rdp_hosts"));
    let tab = &app.tabs[0];
    assert_eq!(tab.purpose, Purpose::Rdp);
    assert_eq!(tab.title, "Domain controller");
    assert!(
        matches!(&tab.profile, TabProfile::Rdp(profile) if profile.domain.as_deref() == Some("LAB"))
    );
    assert!(
        app.update(Message::OpenRdp(ProfileId::new("nowhere")))
            .is_empty()
    );
}

#[test]
fn an_accepted_certificate_reconnects_with_that_key_and_a_refused_one_ends() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (tab, attempt) = open(&mut app);
    event(
        &mut app,
        tab,
        attempt,
        ConnectionEvent::UnknownRdpCertificate {
            host: "dc.lab".to_owned(),
            port: 3389,
            fingerprint: key(),
        },
    );
    assert!(matches!(&app.tabs[0].phase, Phase::HostKey { fingerprint, .. } if fingerprint == KEY));
    let effects = app.update(Message::HostKeyDecision { tab, accept: true });
    let [
        Effect::ConnectRdp {
            attempt: second,
            request,
            ..
        },
    ] = effects.as_slice()
    else {
        panic!("{effects:?}");
    };
    assert_eq!(
        request.accepted,
        Some(key()),
        "the next attempt records exactly this key"
    );
    assert_ne!(*second, attempt, "a new attempt");
    assert_eq!(app.tabs[0].phase, Phase::Connecting);

    event(
        &mut app,
        tab,
        *second,
        ConnectionEvent::UnknownRdpCertificate {
            host: "dc.lab".to_owned(),
            port: 3389,
            fingerprint: key(),
        },
    );
    assert!(
        app.update(Message::HostKeyDecision { tab, accept: false })
            .is_empty()
    );
    assert_eq!(app.tabs[0].phase, Phase::Failed(UiError::Cancelled));
}

#[test]
fn input_reaches_a_connected_desktop_and_nothing_else() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (tab, attempt) = open(&mut app);
    let (input, mut received) = mpsc::unbounded_channel();
    let press = || Message::DesktopInput {
        tab,
        inputs: vec![DesktopInput::Key {
            scancode: Some(Scancode::from_u8(false, 0x1E)),
            keysym: Some(u32::from(b'a')),
            pressed: true,
        }],
    };
    // Not connected yet: dropped.
    app.update(press());
    event(
        &mut app,
        tab,
        attempt,
        ConnectionEvent::RdpReady {
            framebuffer: Framebuffer::new(64, 48),
            input,
            size: tokio::sync::watch::channel(None).0,
            clipboard: None,
        },
    );
    assert_eq!(app.tabs[0].phase, Phase::Connected);
    assert!(received.try_recv().is_err(), "nothing before the session");
    app.update(press());
    // The key goes to RDP by its position.
    let sent = received.try_recv().expect("sent");
    assert!(
        matches!(sent.as_slice(), [Operation::KeyPressed(code)] if *code == Scancode::from_u8(false, 0x1E)),
        "{sent:?}"
    );

    // A dialog owns the input.
    app.update(Message::WindowCloseRequested);
    assert!(app.dialog.is_some());
    app.update(press());
    assert!(received.try_recv().is_err(), "nothing behind a dialog");
    app.update(Message::DismissDialog);

    let before = app.tabs[0].desktop.as_ref().expect("pane").generation;
    event(&mut app, tab, attempt, ConnectionEvent::DesktopFrame);
    assert_eq!(
        app.tabs[0].desktop.as_ref().expect("pane").generation,
        before + 1
    );

    event(
        &mut app,
        tab,
        attempt,
        ConnectionEvent::Closed { exit_status: None },
    );
    assert!(
        app.tabs[0].desktop.is_none(),
        "the desktop goes with the session"
    );
    app.update(press());
    assert!(received.try_recv().is_err());
}

#[test]
fn a_changed_key_can_be_forgotten_and_the_question_comes_back() {
    let dir = tempfile::tempdir().expect("dir");
    let known = KnownRdpHosts::new(dir.path().join("known_rdp_hosts"));
    known.record("dc.lab", 3389, &key()).expect("record");
    known.record("other.lab", 3389, &key()).expect("record");
    let mut app = app(dir.path());
    let (tab, attempt) = open(&mut app);

    // Only a changed key can be forgotten.
    event(
        &mut app,
        tab,
        attempt,
        ConnectionEvent::Failed(UiError::Timeout),
    );
    assert!(app.update(Message::ForgetServer(tab)).is_empty());

    let effects = app.update(Message::HostKeyDecision { tab, accept: true });
    assert!(effects.is_empty(), "no question pending");
    let (tab, attempt) = {
        app.update(Message::RequestCloseTab(tab));
        open(&mut app)
    };
    event(
        &mut app,
        tab,
        attempt,
        ConnectionEvent::Failed(UiError::HostKeyChanged {
            target: None,
            recorded: KEY.to_owned(),
            offered: "SHA256:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".to_owned(),
        }),
    );
    let effects = app.update(Message::ForgetServer(tab));
    assert!(
        matches!(effects.as_slice(), [Effect::ConnectRdp { request, .. }] if request.accepted.is_none()),
        "{effects:?}"
    );
    assert_eq!(
        known.verdict("dc.lab", 3389, &key()).expect("readable"),
        heimdall_rdp::Verdict::Unknown,
        "forgotten"
    );
    assert_eq!(
        known.verdict("other.lab", 3389, &key()).expect("readable"),
        heimdall_rdp::Verdict::Known,
        "the other servers stay"
    );
}

#[test]
fn the_size_the_tab_shows_its_desktop_at_reaches_the_session() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (tab, attempt) = open(&mut app);
    let (input, _received) = mpsc::unbounded_channel();
    let (size, wanted) = tokio::sync::watch::channel(None);
    event(
        &mut app,
        tab,
        attempt,
        ConnectionEvent::RdpReady {
            framebuffer: Framebuffer::new(64, 48),
            input,
            size,
            clipboard: None,
        },
    );
    app.update(Message::DesktopResize {
        tab,
        width: 1600,
        height: 900,
    });
    assert_eq!(*wanted.borrow(), Some((1600, 900)));
}

#[test]
fn every_key_combination_releases_what_it_pressed_modifiers_last() {
    use heimdall_app::{DesktopInput, SpecialKeys};
    for keys in SpecialKeys::ALL {
        let inputs = keys.inputs();
        let (pressed, released): (Vec<_>, Vec<_>) = inputs
            .iter()
            .partition(|input| matches!(input, DesktopInput::Key { pressed: true, .. }));
        assert_eq!(pressed.len(), released.len(), "{keys:?}");
        assert!(
            inputs[..pressed.len()]
                .iter()
                .all(|input| matches!(input, DesktopInput::Key { pressed: true, .. })),
            "{keys:?}: every key down first"
        );
        let names = |input: &&DesktopInput| match input {
            DesktopInput::Key {
                scancode, keysym, ..
            } => (*scancode, *keysym),
            other => panic!("{other:?}"),
        };
        let down: Vec<_> = pressed.iter().map(names).collect();
        let mut up: Vec<_> = released.iter().map(names).collect();
        up.reverse();
        assert_eq!(down, up, "{keys:?}: up in the reverse order");
        assert!(
            down.iter()
                .all(|(scancode, keysym)| scancode.is_some() && keysym.is_some()),
            "{keys:?}: named for RDP and VNC alike"
        );
    }
    // The Windows key is Super_L for VNC, the extended 0x5B for RDP.
    assert_eq!(
        SpecialKeys::Windows.inputs()[0],
        DesktopInput::Key {
            scancode: Some(heimdall_rdp::Scancode::from_u8(true, 0x5B)),
            keysym: Some(0xFFEB),
            pressed: true,
        }
    );
}
