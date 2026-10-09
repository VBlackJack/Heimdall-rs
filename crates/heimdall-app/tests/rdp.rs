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

#[path = "support/log_capture.rs"]
mod log_capture;

use std::path::Path;

use heimdall_app::rdp_driver::DEFAULT_DESKTOP;
use heimdall_app::rdp_external::ExternalRefusal;
use heimdall_app::{
    App, AppConfig, AttemptId, ConnectionEvent, DesktopInput, Effect, Message, Notice, Phase,
    Purpose, TabId, TabProfile, UiError,
};
use heimdall_core::profile::{ProfileId, RdpProfile};
use heimdall_core::store::ProfileStore;
use heimdall_rdp::{
    AcceptedCertificate, CertificateHash, Ending, Fingerprint, Framebuffer, KnownRdpHosts,
    Operation, Scancode,
};
use heimdall_ssh::AgentSource;
use heimdall_term::GridSize;
use tokio::sync::mpsc;

const KEY: &str = "SHA256:rgJ0Y04RcqyBpgUaWIbkDqk3bEPjR9jNUxyEc7cvFq0";

fn app(dir: &Path) -> App {
    let profiles_file = dir.join("profiles.toml");
    let mut store = ProfileStore::open(&profiles_file).expect("store");
    store.merge_rdp([RdpProfile {
        extras: heimdall_core::profile::RdpExtras::default(),
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
        redirect_drives: false,
        // No wait after connecting: each size reaches the session at once here; the wait
        // has its own tests, in `rdp_stabilization.rs`.
        options: heimdall_core::profile::RdpOptions {
            resize_enable_delay_ms: Some(0),
            ..heimdall_core::profile::RdpOptions::default()
        },
        vault_entry: None,
        forwards: heimdall_core::profile::Forwards::default(),
        follow_defaults: false,
        several_servers: false,
        anti_idle: false,
        auto_reconnect: true,
    }]);
    // The same port on another server.
    store.merge_rdp([RdpProfile {
        extras: heimdall_core::profile::RdpExtras::default(),
        id: ProfileId::new("web"),
        name: "Web".to_owned(),
        group: None,
        host: "web.lab".to_owned(),
        port: 3389,
        username: None,
        domain: None,
        allow_tls_only: false,
        gateway: None,
        redirect_clipboard: true,
        redirect_drives: false,
        options: heimdall_core::profile::RdpOptions::default(),
        vault_entry: None,
        forwards: heimdall_core::profile::Forwards::default(),
        follow_defaults: false,
        several_servers: false,
        // Asks for anti-idle keys.
        anti_idle: true,
        auto_reconnect: true,
    }]);
    store.save().expect("save");
    App::new(config(dir))
}

fn config(dir: &Path) -> AppConfig {
    AppConfig {
        profiles_file: dir.join("profiles.toml"),
        known_hosts: dir.join("known_hosts"),
        legacy_dir: None,
        agent: AgentSource::Disabled,
        initial_grid: GridSize { cols: 80, rows: 24 },
        files_start: dir.to_owned(),
        system_credentials: heimdall_app::SystemCredentials::memory(),
    }
}

fn key() -> Fingerprint {
    KEY.parse().expect("fingerprint")
}

/// The hash of the whole certificate the questions ask about, on [`key`].
fn whole() -> CertificateHash {
    CertificateHash::of(b"the certificate of dc.lab")
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
fn an_accepted_certificate_reconnects_with_that_certificate_and_a_refused_one_ends() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (tab, attempt) = open(&mut app);
    event(
        &mut app,
        tab,
        attempt,
        ConnectionEvent::UnknownRdpCertificate {
            subject: None,
            details: None,
            host: "dc.lab".to_owned(),
            port: 3389,
            fingerprint: key(),
            certificate: whole(),
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
        Some(AcceptedCertificate {
            key: key(),
            certificate: whole(),
        }),
        "the next attempt records exactly this certificate"
    );
    assert_ne!(*second, attempt, "a new attempt");
    assert_eq!(app.tabs[0].phase, Phase::Connecting);

    event(
        &mut app,
        tab,
        *second,
        ConnectionEvent::UnknownRdpCertificate {
            subject: None,
            details: None,
            host: "dc.lab".to_owned(),
            port: 3389,
            fingerprint: key(),
            certificate: whole(),
        },
    );
    assert!(
        app.update(Message::HostKeyDecision { tab, accept: false })
            .is_empty()
    );
    assert_eq!(
        app.tabs[0].phase,
        Phase::Failed(UiError::CertificateRefused)
    );
}

#[test]
fn a_certificate_trusted_this_once_is_offered_again_but_never_recorded() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (tab, attempt) = open(&mut app);
    event(
        &mut app,
        tab,
        attempt,
        ConnectionEvent::UnknownRdpCertificate {
            subject: None,
            details: None,
            host: "dc.lab".to_owned(),
            port: 3389,
            fingerprint: key(),
            certificate: whole(),
        },
    );
    assert!(app.tabs[0].asks_about_certificate());
    let effects = app.update(Message::HostKeyTrustOnce(tab));
    let [Effect::ConnectRdp { request, .. }] = effects.as_slice() else {
        panic!("{effects:?}");
    };
    assert_eq!(request.accepted, None, "nothing to record");
    assert_eq!(request.trusted_for_run, [whole()]);
    assert!(!app.tabs[0].asks_about_certificate());

    // Another tab to the same server carries it too, once.
    let (again, again_attempt) = open(&mut app);
    event(
        &mut app,
        again,
        again_attempt,
        ConnectionEvent::UnknownRdpCertificate {
            subject: None,
            details: None,
            host: "dc.lab".to_owned(),
            port: 3389,
            fingerprint: key(),
            certificate: whole(),
        },
    );
    let effects = app.update(Message::HostKeyTrustOnce(again));
    let [Effect::ConnectRdp { request, .. }] = effects.as_slice() else {
        panic!("{effects:?}");
    };
    assert_eq!(request.trusted_for_run, [whole()], "listed once");
    assert!(!dir.path().join("known_rdp_hosts").exists());

    // Another server is not trusted by it.
    let effects = app.update(Message::OpenRdp(ProfileId::new("web")));
    let [Effect::ConnectRdp { request, .. }] = effects.as_slice() else {
        panic!("{effects:?}");
    };
    assert!(request.trusted_for_run.is_empty());
}

#[test]
fn a_session_the_server_ended_keeps_its_reason_until_it_opens_again() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (tab, attempt) = open(&mut app);
    event(
        &mut app,
        tab,
        attempt,
        ConnectionEvent::Ended {
            reason: Ending::Other("Another user connected".to_owned()),
        },
    );
    let ended = app.tab(tab).expect("tab");
    assert_eq!(ended.phase, Phase::Closed { exit_status: None });
    assert_eq!(
        ended.end_reason,
        Some(Ending::Other("Another user connected".to_owned()))
    );
    assert!(app.can_reconnect(ended));
    let effects = app.update(Message::ReconnectTab(tab));
    let [Effect::ConnectRdp { tab: again, .. }] = effects.as_slice() else {
        panic!("{effects:?}");
    };
    assert_eq!(app.tab(*again).expect("again").end_reason, None);
}

#[test]
fn a_desktop_asks_for_the_scale_of_the_windows_screen_as_mstsc() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let scale = |app: &mut App| match app
        .update(Message::OpenRdp(ProfileId::new("dc")))
        .as_slice()
    {
        [Effect::ConnectRdp { request, .. }] => request.desktop_scale,
        other => panic!("{other:?}"),
    };
    assert_eq!(scale(&mut app), 100, "a screen of one pixel per pixel");
    app.update(Message::DisplayScale(1.5));
    assert_eq!(scale(&mut app), 150);
    // A density no screen has is not taken.
    app.update(Message::DisplayScale(f32::NAN));
    app.update(Message::DisplayScale(0.0));
    assert_eq!(scale(&mut app), 150);
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
fn anti_idle_sends_shift_to_the_sessions_asking_for_it_until_stopped() {
    use heimdall_app::SettingsMessage;
    use std::time::Duration;

    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let connect = |app: &mut App, tab, attempt| {
        let (input, received) = mpsc::unbounded_channel();
        event(
            app,
            tab,
            attempt,
            ConnectionEvent::RdpReady {
                framebuffer: Framebuffer::new(64, 48),
                input,
                size: tokio::sync::watch::channel(None).0,
                clipboard: None,
            },
        );
        received
    };
    // "dc" does not ask for them.
    let (dc, attempt) = open(&mut app);
    let mut dc_received = connect(&mut app, dc, attempt);
    assert_eq!(app.anti_idle_interval(), None, "no session asks");
    let effects = app.update(Message::OpenRdp(ProfileId::new("web")));
    let [Effect::ConnectRdp { tab, attempt, .. }] = effects.as_slice() else {
        panic!("{effects:?}");
    };
    let web = *tab;
    assert_eq!(app.anti_idle_interval(), None, "not before the session");
    let mut web_received = connect(&mut app, web, *attempt);
    assert_eq!(app.anti_idle_interval(), Some(Duration::from_secs(60)));
    assert!(app.anti_idle_on(web));
    assert!(!app.anti_idle_on(dc));

    // No input of the user's: sent behind a dialog too.
    app.update(Message::WindowCloseRequested);
    assert!(app.dialog.is_some());
    app.update(Message::AntiIdleTick);
    let shift = Scancode::from_u8(false, 0x2A);
    let sent = web_received.try_recv().expect("shift");
    assert!(
        matches!(sent.as_slice(), [Operation::KeyPressed(down), Operation::KeyReleased(up)]
            if *down == shift && *up == shift),
        "{sent:?}"
    );
    assert!(dc_received.try_recv().is_err(), "only the session asking");
    app.update(Message::DismissDialog);

    // Off in the settings: nothing, and no badge.
    app.update(Message::Settings(SettingsMessage::AntiIdleInterval(0)));
    assert_eq!(app.anti_idle_interval(), None);
    assert!(!app.anti_idle_on(web));
    app.update(Message::AntiIdleTick);
    assert!(web_received.try_recv().is_err());
    // Out of the C# range: refused, the setting kept.
    app.update(Message::Settings(SettingsMessage::AntiIdleInterval(5)));
    assert_eq!(app.settings().anti_idle_interval, 0);

    // On again, then stopped for this session, as the badge's click does.
    app.update(Message::Settings(SettingsMessage::AntiIdleInterval(30)));
    assert_eq!(app.anti_idle_interval(), Some(Duration::from_secs(30)));
    app.update(Message::StopAntiIdle(web));
    assert!(!app.anti_idle_on(web));
    assert_eq!(app.anti_idle_interval(), None);
    app.update(Message::AntiIdleTick);
    assert!(web_received.try_recv().is_err());
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
fn the_profile_decides_the_desktop_asked_and_which_tab_sizes_reach_the_server() {
    use heimdall_core::profile::{RdpOptions, Resolution};

    let fixed = RdpOptions {
        resolution: Resolution::Fixed,
        fixed_width: 1366,
        fixed_height: 768,
        ..RdpOptions::default()
    };
    let once = RdpOptions {
        dynamic_resolution: false,
        ..RdpOptions::default()
    };
    // Asked when connecting, then what reaches the server of two sizes reported in turn.
    for (options, asked, reached) in [
        (
            RdpOptions::default(),
            DEFAULT_DESKTOP,
            [Some((1600, 900)), Some((1024, 700))],
        ),
        (
            once,
            DEFAULT_DESKTOP,
            [Some((1600, 900)), Some((1600, 900))],
        ),
        // Brought down to a multiple of 4, as the C# resolver does.
        (fixed, (1364, 768), [None, None]),
        (
            RdpOptions {
                resolution: Resolution::SmartSizing,
                ..RdpOptions::default()
            },
            DEFAULT_DESKTOP,
            [Some((1600, 900)), Some((1024, 700))],
        ),
    ] {
        let dir = tempfile::tempdir().expect("dir");
        let mut profile = app(dir.path()).rdp_profiles()[0].clone();
        profile.options = RdpOptions {
            resize_enable_delay_ms: Some(0),
            ..options
        };
        let profiles_file = dir.path().join("profiles.toml");
        let mut store = ProfileStore::open(&profiles_file).expect("store");
        store.merge_rdp([profile]);
        store.save().expect("save");
        let mut app = App::new(AppConfig {
            profiles_file,
            known_hosts: dir.path().join("known_hosts"),
            legacy_dir: None,
            agent: AgentSource::Disabled,
            initial_grid: GridSize { cols: 80, rows: 24 },
            files_start: dir.path().to_owned(),
            system_credentials: heimdall_app::SystemCredentials::memory(),
        });
        let effects = app.update(Message::OpenRdp(ProfileId::new("dc")));
        let [
            Effect::ConnectRdp {
                tab,
                attempt,
                request,
            },
        ] = effects.as_slice()
        else {
            panic!("{effects:?}");
        };
        assert_eq!(request.desktop, asked, "{options:?}");
        let (tab, attempt) = (*tab, *attempt);
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
        let pane = app.tabs[0].desktop.as_ref().expect("pane");
        assert_eq!(
            pane.wants_first_size(),
            options == once,
            "asks the tab's size while scaled: {options:?}"
        );
        assert_eq!(pane.has_fixed_size(), options == fixed, "{options:?}");
        for ((width, height), expected) in [(1600, 900), (1024, 700)].into_iter().zip(reached) {
            app.update(Message::DesktopResize { tab, width, height });
            assert_eq!(*wanted.borrow(), expected, "{options:?}");
        }
        let pane = app.tabs[0].desktop.as_ref().expect("pane");
        assert!(!pane.wants_first_size(), "had it: {options:?}");
    }
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

#[test]
fn the_resolution_menu_sizes_the_live_desktop_and_keeps_it_as_the_profile_s_own() {
    use heimdall_app::{Dialog, Notice, ResolutionChoice, TabMenuMessage};
    use heimdall_core::profile::Resolution;

    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (tab, attempt) = open(&mut app);
    let (input, _received) = mpsc::unbounded_channel();
    let (size, watched) = tokio::sync::watch::channel(None);
    app.update(Message::Connection {
        tab,
        attempt,
        event: ConnectionEvent::RdpReady {
            framebuffer: Framebuffer::new(64, 48),
            input,
            size,
            clipboard: None,
        },
    });
    let choose = |app: &mut App, choice| {
        app.update(Message::TabMenu(TabMenuMessage::Resolution { tab, choice }));
    };
    // What the profiles file holds now.
    let saved = || {
        let store = ProfileStore::open(dir.path().join("profiles.toml")).expect("store");
        let profile = store
            .rdp_profiles()
            .iter()
            .find(|profile| profile.id == ProfileId::new("dc"))
            .cloned()
            .expect("saved");
        profile.options
    };

    choose(
        &mut app,
        ResolutionChoice::Fixed {
            width: 1366,
            height: 768,
        },
    );
    assert_eq!(
        *watched.borrow(),
        Some((1364, 768)),
        "asked of the server, width to 4"
    );
    choose(&mut app, ResolutionChoice::SaveDefault);
    let options = saved();
    assert_eq!(
        (
            options.resolution,
            options.fixed_width,
            options.fixed_height
        ),
        (Resolution::Fixed, 1364, 768)
    );
    assert_eq!(app.notice(), Some(&Notice::ResolutionSaved));

    // "Custom...": the C# line when what is typed is not a size.
    choose(&mut app, ResolutionChoice::Custom);
    assert!(
        matches!(&app.dialog, Some(Dialog::CustomResolution { value, .. }) if value == "1920x1080")
    );
    app.update(Message::TabMenu(TabMenuMessage::ResolutionEdited(
        "big".to_owned(),
    )));
    app.update(Message::ConfirmDialog);
    assert_eq!(app.notice(), Some(&Notice::ResolutionInvalid));
    choose(&mut app, ResolutionChoice::Custom);
    app.update(Message::TabMenu(TabMenuMessage::ResolutionEdited(
        "2560x1440".to_owned(),
    )));
    app.update(Message::ConfirmDialog);
    assert_eq!(*watched.borrow(), Some((2560, 1440)));

    choose(&mut app, ResolutionChoice::MatchWindow);
    choose(&mut app, ResolutionChoice::SaveDefault);
    let options = saved();
    assert_eq!(
        (options.resolution, options.dynamic_resolution),
        (Resolution::FitWindow, true)
    );
}

#[test]
fn a_size_chosen_for_the_session_outlives_its_reconnections_and_the_tab_s_is_known_meanwhile() {
    use heimdall_app::{ResolutionChoice, TabMenuMessage};

    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (tab, attempt) = open(&mut app);
    let ready = |app: &mut App, attempt| {
        let (input, _received) = mpsc::unbounded_channel();
        let (size, watched) = tokio::sync::watch::channel(None);
        app.update(Message::Connection {
            tab,
            attempt,
            event: ConnectionEvent::RdpReady {
                framebuffer: Framebuffer::new(64, 48),
                input,
                size,
                clipboard: None,
            },
        });
        watched
    };
    let choose = |app: &mut App, choice| {
        app.update(Message::TabMenu(TabMenuMessage::Resolution { tab, choice }));
    };
    let watched = ready(&mut app, attempt);
    choose(
        &mut app,
        ResolutionChoice::Fixed {
            width: 1280,
            height: 720,
        },
    );
    // The tab's size is kept while the desktop keeps its own, and is the one asked again.
    app.update(Message::DesktopShown {
        tab,
        width: 1000,
        height: 700,
    });
    assert_eq!(
        *watched.borrow(),
        Some((1280, 720)),
        "nothing asked meanwhile"
    );
    let pane = app.tabs[0].desktop.as_ref().expect("pane");
    assert_eq!(pane.tab_size(), Some((1000, 700)));
    choose(&mut app, ResolutionChoice::MatchWindow);
    assert_eq!(*watched.borrow(), Some((1000, 700)), "the tab's size, now");

    choose(
        &mut app,
        ResolutionChoice::Fixed {
            width: 1280,
            height: 720,
        },
    );
    // The connection drops and opens again by itself, in its place.
    app.update(Message::Connection {
        tab,
        attempt,
        event: ConnectionEvent::Failed(heimdall_app::UiError::Network {
            failure: heimdall_app::NetworkFailure::Reset,
            detail: "reset".to_owned(),
        }),
    });
    let effects = app.update(Message::AutoReconnect { tab, attempt });
    let (again, desktop) = match effects.as_slice() {
        [
            Effect::ConnectRdp {
                attempt, request, ..
            },
        ] => (*attempt, request.desktop),
        other => panic!("expected a connection, got {other:?}"),
    };
    assert_eq!(desktop, (1280, 720), "asked at the size chosen");
    ready(&mut app, again);
    let pane = app.tabs[0].desktop.as_ref().expect("pane");
    assert_eq!(pane.fixed_size(), Some((1280, 720)), "and kept");
}

#[test]
fn a_size_chosen_larger_than_the_tab_says_it_is_shown_scaled() {
    use heimdall_app::{Notice, ResolutionChoice, TabMenuMessage};

    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (tab, attempt) = open(&mut app);
    let (input, _received) = mpsc::unbounded_channel();
    app.update(Message::Connection {
        tab,
        attempt,
        event: ConnectionEvent::RdpReady {
            framebuffer: Framebuffer::new(64, 48),
            input,
            size: tokio::sync::watch::channel(None).0,
            clipboard: None,
        },
    });
    app.update(Message::DesktopShown {
        tab,
        width: 1000,
        height: 700,
    });
    let choose = |app: &mut App, width, height| {
        app.update(Message::TabMenu(TabMenuMessage::Resolution {
            tab,
            choice: ResolutionChoice::Fixed { width, height },
        }));
    };
    choose(&mut app, 800, 600);
    assert_eq!(app.notice(), None, "it fits");
    choose(&mut app, 1920, 1080);
    assert_eq!(app.notice(), Some(&Notice::ResolutionScaled));
}

#[test]
fn a_size_the_server_cannot_take_live_connects_again_at_it_once_and_says_so() {
    use heimdall_app::Notice;

    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (tab, attempt) = open(&mut app);
    let ready = |app: &mut App, attempt| {
        let (input, _received) = mpsc::unbounded_channel();
        app.update(Message::Connection {
            tab,
            attempt,
            event: ConnectionEvent::RdpReady {
                framebuffer: Framebuffer::new(64, 48),
                input,
                size: tokio::sync::watch::channel(None).0,
                clipboard: None,
            },
        });
    };
    let refused = |app: &mut App, attempt, (width, height)| {
        app.update(Message::Connection {
            tab,
            attempt,
            event: ConnectionEvent::DesktopResizeRefused { width, height },
        })
    };
    let connected_again = |effects: &[Effect]| match effects {
        [
            Effect::ConnectRdp {
                attempt, request, ..
            },
        ] => (*attempt, request.desktop),
        other => panic!("expected a connection, got {other:?}"),
    };
    ready(&mut app, attempt);

    let (again, desktop) = connected_again(&refused(&mut app, attempt, (1600, 900)));
    assert_eq!(desktop, (1600, 900), "asked as the connection opens");
    assert_eq!(app.notice(), None, "said once back, not before");
    ready(&mut app, again);
    assert_eq!(app.notice(), Some(&Notice::ResolutionReconnected));

    // The server keeps a size of its own even then: no second connection for it.
    assert!(
        refused(&mut app, again, (1600, 900)).is_empty(),
        "never a loop"
    );
    // Another size gets its own.
    let (_, desktop) = connected_again(&refused(&mut app, again, (1280, 720)));
    assert_eq!(desktop, (1280, 720));
}

#[test]
fn the_certificate_question_counts_the_other_certificates_and_names_the_route() {
    use heimdall_app::{CertificateContext, Notice};
    use heimdall_core::profile::SshGateway;

    let dir = tempfile::tempdir().expect("dir");
    drop(app(dir.path()));
    // Two machines already trusted at the name, one at another server's.
    std::fs::write(
        dir.path().join("known_rdp_hosts"),
        "dc.lab:3389 SHA256:AgJ0Y04RcqyBpgUaWIbkDqk3bEPjR9jNUxyEc7cvFq0\n\
         dc.lab:3389 SHA256:bgJ0Y04RcqyBpgUaWIbkDqk3bEPjR9jNUxyEc7cvFq0\n\
         web.lab:3389 SHA256:cgJ0Y04RcqyBpgUaWIbkDqk3bEPjR9jNUxyEc7cvFq0\n",
    )
    .expect("write");
    // Reached through a bastion, itself behind an edge gateway.
    let mut store = ProfileStore::open(dir.path().join("profiles.toml")).expect("store");
    let gateway = |id: &str, name: &str, parent: Option<&str>| SshGateway {
        id: ProfileId::new(id),
        name: name.to_owned(),
        host: format!("{id}.lab"),
        port: 22,
        username: None,
        key_path: None,
        parent: parent.map(ProfileId::new),
    };
    store.merge_gateways([
        gateway("edge", "Edge", None),
        gateway("bastion", "Bastion", Some("edge")),
    ]);
    let mut dc = store.rdp_profiles()[0].clone();
    dc.gateway = Some(ProfileId::new("bastion"));
    store.merge_rdp([dc]);
    store.save().expect("save");
    let mut app = App::new(config(dir.path()));

    let (tab, attempt) = open(&mut app);
    event(
        &mut app,
        tab,
        attempt,
        ConnectionEvent::UnknownRdpCertificate {
            subject: Some("CN=dc.lab".to_owned()),
            details: None,
            host: "dc.lab".to_owned(),
            port: 3389,
            fingerprint: key(),
            certificate: whole(),
        },
    );
    let asked = app.tab(tab).expect("tab");
    assert_eq!(
        asked.certificate_context,
        Some(CertificateContext {
            others: 2,
            route: vec!["Edge".to_owned(), "Bastion".to_owned()],
            subject: Some("CN=dc.lab".to_owned()),
            details: None,
        })
    );

    // Its fingerprint can be copied, as the C# prompt's Copy.
    let effects = app.update(Message::CopyHostKeyFingerprint(tab));
    assert!(
        matches!(effects.as_slice(), [Effect::WriteClipboard(text)] if text == KEY),
        "{effects:?}"
    );
    assert_eq!(
        app.notice(),
        Some(&Notice::FingerprintCopied("dc.lab:3389".to_owned()))
    );

    // Answered, the question and what it said go.
    app.update(Message::HostKeyDecision { tab, accept: false });
    assert_eq!(app.tab(tab).expect("tab").certificate_context, None);
    assert!(
        app.update(Message::CopyHostKeyFingerprint(tab)).is_empty(),
        "nothing to copy once answered"
    );
}

#[test]
fn match_window_fits_the_desktop_to_a_ratio_and_keeps_it_for_the_reconnections() {
    use heimdall_app::{Aspect, ResolutionChoice, TabMenuMessage};

    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (tab, attempt) = open(&mut app);
    let ready = |app: &mut App, attempt| {
        let (input, _received) = mpsc::unbounded_channel();
        let (size, watched) = tokio::sync::watch::channel(None);
        app.update(Message::Connection {
            tab,
            attempt,
            event: ConnectionEvent::RdpReady {
                framebuffer: Framebuffer::new(64, 48),
                input,
                size,
                clipboard: None,
            },
        });
        watched
    };
    let watched = ready(&mut app, attempt);
    let choose = |app: &mut App, choice| {
        app.update(Message::TabMenu(TabMenuMessage::Resolution { tab, choice }));
    };
    choose(&mut app, ResolutionChoice::MatchWindow);
    app.update(Message::DesktopResize {
        tab,
        width: 1600,
        height: 1000,
    });
    assert_eq!(*watched.borrow(), Some((1600, 1000)), "the whole tab");

    choose(&mut app, ResolutionChoice::MatchAspect(Aspect::Wide));
    assert_eq!(
        *watched.borrow(),
        Some((1600, 900)),
        "16:9 inside, at once, as wide as the tab"
    );
    app.update(Message::DesktopResize {
        tab,
        width: 2000,
        height: 900,
    });
    assert_eq!(
        *watched.borrow(),
        Some((1600, 900)),
        "a tab wider than 16:9: as high, bars at the sides"
    );

    // Connected again by itself, as an auto-reconnect does: its new desktop keeps the
    // ratio.
    let watched = ready(&mut app, attempt);
    app.update(Message::DesktopResize {
        tab,
        width: 1200,
        height: 1200,
    });
    assert_eq!(
        app.tab(tab)
            .and_then(|tab| tab.desktop.as_ref())
            .map(|pane| pane.aspect),
        Some(Aspect::Wide)
    );
    assert_eq!(*watched.borrow(), Some((1200, 675)));
}

/// `app` with profile `dc` changed by `change`.
fn app_with(dir: &Path, change: impl FnOnce(&mut RdpProfile)) -> App {
    drop(app(dir));
    let mut store = ProfileStore::open(dir.join("profiles.toml")).expect("store");
    let mut profile = store
        .rdp_profiles()
        .iter()
        .find(|profile| profile.id.as_str() == "dc")
        .cloned()
        .expect("dc");
    change(&mut profile);
    store.merge_rdp([profile]);
    store.save().expect("save");
    App::new(config(dir))
}

#[test]
fn a_desktop_starts_at_the_proportions_its_profile_keeps() {
    use heimdall_app::Aspect;

    let dir = tempfile::tempdir().expect("dir");
    let mut app = app_with(dir.path(), |profile| {
        profile.options.aspect = Aspect::Wide;
    });
    let (tab, attempt) = open(&mut app);
    let (input, _received) = mpsc::unbounded_channel();
    let (size, watched) = tokio::sync::watch::channel(None);
    app.update(Message::Connection {
        tab,
        attempt,
        event: ConnectionEvent::RdpReady {
            framebuffer: Framebuffer::new(64, 48),
            input,
            size,
            clipboard: None,
        },
    });
    app.update(Message::DesktopResize {
        tab,
        width: 1600,
        height: 1000,
    });
    assert_eq!(
        *watched.borrow(),
        Some((1600, 900)),
        "16:9 from the start, as the C# reads the profile's ratio"
    );
}

#[test]
fn a_server_behind_a_remote_desktop_gateway_opens_in_remote_desktop_connection() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app_with(dir.path(), |profile| {
        profile.extras.rd_gateway = Some("rdg.lab".to_owned());
    });
    let effects = app.update(Message::OpenRdp(ProfileId::new("dc")));
    let [
        Effect::LaunchRdpExternal {
            name,
            gateway,
            content,
        },
    ] = effects.as_slice()
    else {
        panic!("{effects:?}");
    };
    assert_eq!(name, "Domain controller");
    assert_eq!(
        gateway.as_deref(),
        Some("rdg.lab"),
        "the gateway sent it there"
    );
    assert!(
        content.contains("\r\ngatewayhostname:s:rdg.lab\r\n"),
        "{content}"
    );
    assert!(content.contains("\r\nusername:s:admin\r\ndomain:s:LAB\r\n"));
    assert!(
        app.tabs.is_empty(),
        "no tab: the window is Remote Desktop Connection's"
    );

    app.update(Message::RdpExternalLaunched {
        name: name.clone(),
        gateway: gateway.clone(),
        result: Ok(()),
    });
    assert_eq!(
        app.notice(),
        Some(&Notice::RdpExternalLaunched {
            name: "Domain controller".to_owned(),
            gateway: Some("rdg.lab".to_owned()),
        })
    );
}

#[test]
fn a_profile_set_to_the_external_client_opens_in_it_and_no_tab() {
    let dir = tempfile::tempdir().expect("dir");
    // Saved, then read back by the application: the choice is kept.
    let mut app = app_with(dir.path(), |profile| profile.extras.external = true);
    let effects = app.update(Message::OpenRdp(ProfileId::new("dc")));
    let [
        Effect::LaunchRdpExternal {
            name,
            gateway: None,
            content,
        },
    ] = effects.as_slice()
    else {
        panic!("{effects:?}");
    };
    assert!(content.starts_with("full address:s:dc.lab:3389\r\n"));
    assert!(
        !content.lines().any(|line| line.starts_with("password")),
        "no password: Remote Desktop Connection asks for it"
    );
    assert!(app.tabs.is_empty());

    app.update(Message::RdpExternalLaunched {
        name: name.clone(),
        gateway: None,
        result: Err(ExternalRefusal::NotFound),
    });
    assert_eq!(
        app.notice(),
        Some(&Notice::RdpExternalRefused(ExternalRefusal::NotFound))
    );
}

#[test]
fn full_screen_opens_remote_desktop_connection_full_screen_and_leaves_a_tab_as_it_is() {
    use heimdall_core::profile::Resolution;

    // As the C#: `RdpFullScreen` is written into the connection file of the external
    // client, outside its automatic mode, and never read by the embedded session.
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app_with(dir.path(), |profile| {
        profile.extras.full_screen = true;
        profile.options.resolution = Resolution::Fixed;
    });
    let (tab, _attempt) = open(&mut app);
    assert!(app.tab(tab).is_some(), "a tab, as without it");

    let dir = tempfile::tempdir().expect("dir");
    let mut app = app_with(dir.path(), |profile| {
        profile.extras.full_screen = true;
        profile.extras.external = true;
        profile.options.resolution = Resolution::Fixed;
    });
    let effects = app.update(Message::OpenRdp(ProfileId::new("dc")));
    let [Effect::LaunchRdpExternal { content, .. }] = effects.as_slice() else {
        panic!("{effects:?}");
    };
    assert!(content.contains("\r\nscreen mode id:i:2\r\n"), "{content}");
}

#[test]
fn an_external_profile_through_a_gateway_gone_is_refused_with_its_reason_not_sent_straight() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app_with(dir.path(), |profile| {
        profile.extras.external = true;
        profile.gateway = Some(ProfileId::new("bastion"));
    });
    let effects = app.update(Message::OpenRdp(ProfileId::new("dc")));
    assert!(effects.is_empty(), "{effects:?}");
    assert!(app.tabs.is_empty());
    assert_eq!(
        app.notice(),
        Some(&Notice::RdpExternalRefused(ExternalRefusal::Gateway(
            UiError::Route(heimdall_core::store::RouteError::MissingGateway(
                ProfileId::new("bastion")
            ))
        )))
    );
}

// ---- the session events log ------------------------------------------------------------

/// Session logging turned on from the settings, the question it asks agreed to.
fn session_logging_on(app: &mut App) {
    app.update(Message::Settings(
        heimdall_app::SettingsMessage::SessionLogging(true),
    ));
    if matches!(
        app.dialog,
        Some(heimdall_app::Dialog::ConfirmSessionLogging)
    ) {
        app.update(Message::ConfirmDialog);
    }
    assert!(app.settings().session_logging);
}

/// The lines of the session events log beside the transcripts of the application over
/// `dir`, read as JSON.
fn session_events(app: &App, dir: &Path) -> Vec<serde_json::Value> {
    app.sync_session_logs();
    let folder = app
        .settings()
        .session_log_folder(&dir.join(heimdall_core::settings::SETTINGS_FILE_NAME));
    std::fs::read_to_string(folder.join(heimdall_app::session_log::SESSION_EVENTS_FILE))
        .unwrap_or_default()
        .lines()
        .map(|line| serde_json::from_str(line).expect("one JSON object a line"))
        .collect()
}

/// The desktop of `tab` connected.
fn desktop_ready(app: &mut App, tab: TabId, attempt: AttemptId) {
    let (input, _received) = mpsc::unbounded_channel();
    event(
        app,
        tab,
        attempt,
        ConnectionEvent::RdpReady {
            framebuffer: Framebuffer::new(64, 48),
            input,
            size: tokio::sync::watch::channel(None).0,
            clipboard: None,
        },
    );
    assert_eq!(app.tab(tab).expect("tab").phase, Phase::Connected);
}

#[test]
fn a_desktop_s_connect_and_each_kind_of_end_go_to_the_session_events_log() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    session_logging_on(&mut app);

    // Ended by the server, saying why.
    let (tab, attempt) = open(&mut app);
    desktop_ready(&mut app, tab, attempt);
    event(
        &mut app,
        tab,
        attempt,
        ConnectionEvent::Ended {
            reason: Ending::AdminDisconnect,
        },
    );
    // Disconnected by the user, then the tab closed: one end only.
    let (user, attempt) = open(&mut app);
    desktop_ready(&mut app, user, attempt);
    app.update(Message::DisconnectDesktop(user));
    app.update(Message::ConfirmDialog);
    app.update(Message::RequestCloseTab(user));
    if app.dialog.is_some() {
        app.update(Message::ConfirmDialog);
    }
    // Its tab closed while connected.
    let (closed, attempt) = open(&mut app);
    desktop_ready(&mut app, closed, attempt);
    app.update(Message::RequestCloseTab(closed));
    if app.dialog.is_some() {
        app.update(Message::ConfirmDialog);
    }
    assert!(app.tab(closed).is_none());
    // A connection that never opened logs nothing.
    let (failed, attempt) = open(&mut app);
    event(
        &mut app,
        failed,
        attempt,
        ConnectionEvent::Failed(UiError::Timeout),
    );

    let lines = session_events(&app, dir.path());
    let field = |index: usize, name: &str| lines[index][name].clone();
    let kinds: Vec<String> = (0..lines.len())
        .map(|index| {
            field(index, "event")
                .as_str()
                .unwrap_or_default()
                .to_owned()
        })
        .collect();
    assert_eq!(
        kinds,
        [
            "Connected",
            "Disconnected",
            "Connected",
            "Disconnected",
            "Connected",
            "Disconnected"
        ],
        "{lines:?}"
    );
    for line in &lines {
        assert_eq!(line["protocol"], "RDP");
        assert_eq!(line["host"], "dc.lab");
        assert_eq!(line["title"], "Domain controller");
        assert!(
            line["ts"].as_str().is_some_and(|ts| ts.ends_with('Z')),
            "{line}"
        );
    }
    assert!(
        lines[0].get("durationMs").is_none(),
        "a connect has no duration"
    );
    assert_eq!(field(1, "reason"), "RDP_ADMIN_DISCONNECT");
    assert!(lines[1].get("endTrigger").is_none(), "the reason says it");
    assert!(lines[1]["durationMs"].is_u64());
    assert_eq!(field(3, "endTrigger"), "user");
    assert!(lines[3].get("reason").is_none());
    assert_eq!(field(5, "endTrigger"), "teardown");
}

#[test]
fn without_session_logging_a_desktop_writes_no_event_and_quitting_ends_one_still_open() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (tab, attempt) = open(&mut app);
    desktop_ready(&mut app, tab, attempt);
    event(
        &mut app,
        tab,
        attempt,
        ConnectionEvent::Closed { exit_status: None },
    );
    assert!(session_events(&app, dir.path()).is_empty());

    // On while connected: the end is written, the connect it closes having been off.
    let (tab, attempt) = open(&mut app);
    desktop_ready(&mut app, tab, attempt);
    session_logging_on(&mut app);
    let (other, attempt) = open(&mut app);
    desktop_ready(&mut app, other, attempt);
    app.update(Message::WindowCloseRequested);
    let effects = app.update(Message::ConfirmDialog);
    assert!(
        effects.iter().any(|effect| matches!(effect, Effect::Exit)),
        "{effects:?}"
    );
    let lines = session_events(&app, dir.path());
    let ends: Vec<(String, String)> = lines
        .iter()
        .map(|line| {
            (
                line["event"].as_str().unwrap_or_default().to_owned(),
                line["endTrigger"].as_str().unwrap_or_default().to_owned(),
            )
        })
        .collect();
    assert_eq!(
        ends,
        [
            ("Connected".to_owned(), String::new()),
            ("Disconnected".to_owned(), "teardown".to_owned()),
            ("Disconnected".to_owned(), "teardown".to_owned()),
        ],
        "{lines:?}"
    );
}

#[test]
fn the_answer_to_a_certificate_question_is_in_the_diagnostics_log_with_the_key() {
    let dir = tempfile::tempdir().expect("dir");
    log_capture::start();
    let mut app = app(dir.path());
    for (accept, answer) in [(true, "trusted, recorded"), (false, "refused")] {
        let (tab, attempt) = open(&mut app);
        event(
            &mut app,
            tab,
            attempt,
            ConnectionEvent::UnknownRdpCertificate {
                subject: None,
                host: "dc.lab".to_owned(),
                port: 3389,
                fingerprint: key(),
                certificate: whole(),
                details: None,
            },
        );
        app.update(Message::HostKeyDecision { tab, accept });
        assert!(
            log_capture::has("INFO", &["RDP server dc.lab:3389", KEY, answer]),
            "{:?}",
            log_capture::lines()
        );
        app.update(Message::RequestCloseTab(tab));
    }
}

#[test]
fn forgetting_an_rdp_server_from_its_card_forgets_its_key_trusted_once() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (tab, attempt) = open(&mut app);
    event(
        &mut app,
        tab,
        attempt,
        ConnectionEvent::UnknownRdpCertificate {
            subject: None,
            details: None,
            host: "dc.lab".to_owned(),
            port: 3389,
            fingerprint: key(),
            certificate: whole(),
        },
    );
    let effects = app.update(Message::HostKeyTrustOnce(tab));
    let [
        Effect::ConnectRdp {
            attempt, request, ..
        },
    ] = effects.as_slice()
    else {
        panic!("{effects:?}");
    };
    assert_eq!(request.trusted_for_run, [whole()]);
    event(
        &mut app,
        tab,
        *attempt,
        ConnectionEvent::Failed(UiError::HostKeyChanged {
            target: None,
            recorded: KEY.to_owned(),
            offered: "SHA256:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".to_owned(),
        }),
    );
    let effects = app.update(Message::ForgetServer(tab));
    let [Effect::ConnectRdp { request, .. }] = effects.as_slice() else {
        panic!("{effects:?}");
    };
    assert!(request.trusted_for_run.is_empty(), "asked about again");
}

#[test]
fn a_renewed_rdp_certificate_is_asked_about_as_a_renewal_and_trusted_by_its_hash_alone() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (tab, attempt) = open(&mut app);
    let renewed = CertificateHash::of(b"the renewed certificate of dc.lab");
    let at = |seconds| std::time::UNIX_EPOCH + std::time::Duration::from_secs(seconds);
    let details = heimdall_app::CertificateDetails {
        issuer: "CN=dc.lab".to_owned(),
        validity: heimdall_rdp::Validity {
            not_before: at(200),
            not_after: at(300),
        },
        issue: None,
        renewal: Some(heimdall_app::Renewal {
            recorded: Some(heimdall_rdp::Validity {
                not_before: at(100),
                not_after: at(200),
            }),
        }),
    };
    event(
        &mut app,
        tab,
        attempt,
        ConnectionEvent::UnknownRdpCertificate {
            subject: Some("CN=dc.lab".to_owned()),
            details: Some(Box::new(details.clone())),
            host: "dc.lab".to_owned(),
            port: 3389,
            fingerprint: key(),
            certificate: renewed,
        },
    );
    // Asked, never taken silently: the question says it is a renewal, with both validities.
    assert!(app.tabs[0].asks_about_certificate());
    assert_eq!(
        app.tabs[0]
            .certificate_context
            .as_ref()
            .and_then(|context| context.details.clone()),
        Some(details)
    );
    // Trusted this once: the renewed certificate, not its key.
    let effects = app.update(Message::HostKeyTrustOnce(tab));
    let [Effect::ConnectRdp { request, .. }] = effects.as_slice() else {
        panic!("{effects:?}");
    };
    assert_eq!(request.accepted, None);
    assert_eq!(request.trusted_for_run, [renewed]);
    assert!(
        !dir.path().join("known_rdp_hosts").exists(),
        "nothing written"
    );
}
