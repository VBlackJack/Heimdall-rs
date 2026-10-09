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

//! Auto-reconnect of a dropped RDP desktop, as the C# Heimdall's: after 2, 5, then 15
//! seconds, up to 20 attempts, in its tab; stopped by Cancel, by success, or by anything
//! that needs the user.

use std::path::Path;
use std::time::{Duration, Instant};

use heimdall_app::{
    App, AppConfig, AttemptId, ConnectionEvent, Effect, Message, NetworkFailure, Phase, TabId,
    UiError,
};
use heimdall_core::profile::{ProfileId, RdpProfile, SshProfile};
use heimdall_core::settings::RDP_AUTO_RECONNECT_ATTEMPTS_MAX;
use heimdall_core::store::ProfileStore;
use heimdall_rdp::Framebuffer;
use heimdall_ssh::AgentSource;
use heimdall_term::GridSize;
use tokio::sync::mpsc;

fn app(dir: &Path) -> App {
    let profiles_file = dir.join("profiles.toml");
    let mut store = ProfileStore::open(&profiles_file).expect("store");
    let dc = RdpProfile {
        extras: heimdall_core::profile::RdpExtras::default(),
        id: ProfileId::new("dc"),
        name: "Domain controller".to_owned(),
        group: None,
        host: "dc.lab".to_owned(),
        port: 3389,
        username: Some("admin".to_owned()),
        domain: None,
        allow_tls_only: false,
        gateway: None,
        local_tunnel_port: None,
        redirect_clipboard: false,
        redirect_drives: false,
        options: heimdall_core::profile::RdpOptions::default(),
        vault_entry: None,
        forwards: heimdall_core::profile::Forwards::default(),
        follow_defaults: false,
        several_servers: false,
        anti_idle: false,
        auto_reconnect: true,
    };
    store.merge_rdp([
        dc.clone(),
        // Auto-reconnect cleared in its profile.
        RdpProfile {
            id: ProfileId::new("quiet"),
            name: "Quiet".to_owned(),
            auto_reconnect: false,
            ..dc.clone()
        },
        // Following the application's RDP options.
        RdpProfile {
            id: ProfileId::new("follows"),
            name: "Follows".to_owned(),
            follow_defaults: true,
            ..dc
        },
    ]);
    store.merge([SshProfile {
        id: ProfileId::new("web"),
        name: "web".to_owned(),
        group: None,
        host: "web.lab".to_owned(),
        port: 22,
        username: Some("admin".to_owned()),
        key_path: None,
        gateway: None,
        local_tunnel_port: None,
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

fn connect_rdp(effects: &[Effect]) -> (TabId, AttemptId) {
    match effects {
        [Effect::ConnectRdp { tab, attempt, .. }] => (*tab, *attempt),
        other => panic!("expected one ConnectRdp, got {other:?}"),
    }
}

fn event(app: &mut App, tab: TabId, attempt: AttemptId, event: ConnectionEvent) -> Vec<Effect> {
    app.update(Message::Connection {
        tab,
        attempt,
        event,
    })
}

fn ready(app: &mut App, tab: TabId, attempt: AttemptId) {
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

fn dropped() -> ConnectionEvent {
    ConnectionEvent::Failed(UiError::Network {
        failure: NetworkFailure::Reset,
        detail: "reset".to_owned(),
    })
}

/// A live desktop of profile "dc".
fn live(app: &mut App) -> (TabId, AttemptId) {
    let (tab, attempt) = connect_rdp(&app.update(Message::OpenRdp(ProfileId::new("dc"))));
    ready(app, tab, attempt);
    (tab, attempt)
}

/// The wake-up `effects` ask for, checked to be `wait` from now.
fn wake(effects: &[Effect], tab: TabId, failed: AttemptId, wait: Duration) {
    let [
        Effect::RetryAt {
            tab: woken,
            attempt,
            deadline,
        },
    ] = effects
    else {
        panic!("expected one RetryAt, got {effects:?}");
    };
    assert_eq!((*woken, *attempt), (tab, failed));
    let left = deadline.saturating_duration_since(Instant::now());
    assert!(
        left <= wait && left + Duration::from_millis(500) >= wait,
        "{left:?} for {wait:?}"
    );
}

#[test]
fn a_dropped_desktop_opens_again_by_itself_after_2_then_5_seconds() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (tab, attempt) = live(&mut app);

    let effects = event(&mut app, tab, attempt, dropped());
    wake(&effects, tab, attempt, Duration::from_secs(2));
    let retry = app.tab(tab).expect("tab").retry.expect("waiting");
    assert_eq!(
        (retry.attempt, retry.max),
        (1, RDP_AUTO_RECONNECT_ATTEMPTS_MAX)
    );
    assert!(matches!(app.tab(tab).expect("tab").phase, Phase::Failed(_)));

    let (again, second) = connect_rdp(&app.update(Message::AutoReconnect { tab, attempt }));
    assert_eq!(again, tab, "in its own tab");
    assert_ne!(second, attempt);
    assert_eq!(app.tab(tab).expect("tab").phase, Phase::Connecting);
    assert!(
        app.update(Message::AutoReconnect { tab, attempt })
            .is_empty(),
        "a wake-up for an attempt gone does nothing"
    );

    // The server is still away: the second attempt waits longer.
    let effects = event(
        &mut app,
        tab,
        second,
        ConnectionEvent::Failed(UiError::Timeout),
    );
    wake(&effects, tab, second, Duration::from_secs(5));
    assert_eq!(
        app.tab(tab).expect("tab").retry.expect("waiting").attempt,
        2
    );
    assert!(
        app.update(Message::AutoReconnect { tab, attempt })
            .is_empty(),
        "the first wake-up, late, does not cut the second wait short"
    );

    let (_, third) = connect_rdp(&app.update(Message::AutoReconnect {
        tab,
        attempt: second,
    }));
    ready(&mut app, tab, third);
    assert_eq!(app.tab(tab).expect("tab").retry, None, "back: stopped");
}

#[test]
fn the_attempts_stop_at_the_twentieth() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (tab, mut attempt) = live(&mut app);
    let mut effects = event(&mut app, tab, attempt, dropped());
    for expected in 1..=RDP_AUTO_RECONNECT_ATTEMPTS_MAX {
        assert_eq!(
            app.tab(tab).expect("tab").retry.expect("waiting").attempt,
            expected
        );
        assert_eq!(effects.len(), 1);
        attempt = connect_rdp(&app.update(Message::AutoReconnect { tab, attempt })).1;
        effects = event(&mut app, tab, attempt, dropped());
    }
    assert!(effects.is_empty(), "no twenty-first");
    assert_eq!(app.tab(tab).expect("tab").retry, None);
    assert!(app.can_reconnect(app.tab(tab).expect("tab")));
}

#[test]
fn cancel_stops_the_attempts() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (tab, attempt) = live(&mut app);
    event(&mut app, tab, attempt, dropped());
    app.update(Message::CancelAutoReconnect(tab));
    assert_eq!(app.tab(tab).expect("tab").retry, None);
    assert!(
        app.update(Message::AutoReconnect { tab, attempt })
            .is_empty()
    );
    assert!(matches!(app.tab(tab).expect("tab").phase, Phase::Failed(_)));
}

#[test]
fn only_a_live_desktop_that_dropped_for_a_passing_reason_comes_back() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    // Never connected: the failure is shown.
    let (first, attempt) = connect_rdp(&app.update(Message::OpenRdp(ProfileId::new("dc"))));
    assert!(event(&mut app, first, attempt, dropped()).is_empty());
    assert_eq!(app.tab(first).expect("tab").retry, None);

    // A reason that will not pass by itself.
    let (tab, attempt) = live(&mut app);
    let refused = ConnectionEvent::Failed(UiError::AuthenticationFailed {
        tried: Vec::new(),
        agent_keys: None,
    });
    assert!(event(&mut app, tab, attempt, refused).is_empty());
    assert_eq!(app.tab(tab).expect("tab").retry, None);

    // An SSH session: not by default, as in the C# Heimdall.
    let effects = app.update(Message::OpenProfile(ProfileId::new("web")));
    let [Effect::Connect { tab, attempt, .. }] = effects.as_slice() else {
        panic!("{effects:?}");
    };
    let (tab, attempt) = (*tab, *attempt);
    event(
        &mut app,
        tab,
        attempt,
        ConnectionEvent::Connected {
            input: std::sync::Arc::new(NullSink),
        },
    );
    assert!(event(&mut app, tab, attempt, dropped()).is_empty());
}

#[test]
fn a_desktop_whose_profile_or_the_options_it_follows_clear_auto_reconnect_stays_down() {
    use heimdall_app::SettingsMessage;
    use heimdall_core::profile::RdpDefaults;

    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let down = |app: &mut App, id: &str| {
        let (tab, attempt) = connect_rdp(&app.update(Message::OpenRdp(ProfileId::new(id))));
        ready(app, tab, attempt);
        let effects = event(app, tab, attempt, dropped());
        (
            effects.is_empty(),
            app.tab(tab).expect("tab").retry.is_none(),
        )
    };
    assert_eq!(
        down(&mut app, "quiet"),
        (true, true),
        "cleared in the profile"
    );
    assert_eq!(
        down(&mut app, "follows"),
        (false, false),
        "the options followed have it on"
    );
    app.update(Message::Settings(SettingsMessage::RdpDefaults(
        RdpDefaults {
            auto_reconnect: false,
            ..RdpDefaults::default()
        },
    )));
    assert_eq!(
        down(&mut app, "follows"),
        (true, true),
        "cleared in the options followed"
    );
    assert_eq!(down(&mut app, "dc"), (false, false), "its own choice kept");
}

#[test]
fn a_question_on_the_way_back_stops_the_attempts() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (tab, attempt) = live(&mut app);
    event(&mut app, tab, attempt, dropped());
    let (_, again) = connect_rdp(&app.update(Message::AutoReconnect { tab, attempt }));
    event(
        &mut app,
        tab,
        again,
        ConnectionEvent::UnknownRdpCertificate {
            subject: None,
            details: None,
            host: "dc.lab".to_owned(),
            port: 3389,
            fingerprint: "SHA256:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"
                .parse()
                .expect("fingerprint"),
            certificate: heimdall_rdp::CertificateHash::of(b"a certificate"),
        },
    );
    assert_eq!(app.tab(tab).expect("tab").retry, None, "the user decides");
    app.update(Message::HostKeyDecision { tab, accept: false });
    assert_eq!(
        app.tab(tab).expect("tab").phase,
        Phase::Failed(UiError::CertificateRefused)
    );
    assert_eq!(app.tab(tab).expect("tab").retry, None);
}

#[derive(Debug)]
struct NullSink;

impl heimdall_app::InputSink for NullSink {
    fn write(&self, _bytes: Vec<u8>) -> Result<(), heimdall_ssh::SessionClosed> {
        Ok(())
    }
    fn resize(&self, _size: heimdall_ssh::TerminalSize) -> Result<(), heimdall_ssh::SessionClosed> {
        Ok(())
    }
    fn close(&self) {}
}

/// A live SSH shell of profile "web".
fn live_ssh(app: &mut App) -> (TabId, AttemptId) {
    let effects = app.update(Message::OpenProfile(ProfileId::new("web")));
    let [Effect::Connect { tab, attempt, .. }] = effects.as_slice() else {
        panic!("{effects:?}");
    };
    let (tab, attempt) = (*tab, *attempt);
    event(
        app,
        tab,
        attempt,
        ConnectionEvent::Connected {
            input: std::sync::Arc::new(NullSink),
        },
    );
    (tab, attempt)
}

fn ssh_auto_reconnect(app: &mut App, attempts: u32) {
    use heimdall_app::SettingsMessage;
    app.update(Message::Settings(SettingsMessage::SshAutoReconnect(true)));
    app.update(Message::Settings(
        SettingsMessage::SshAutoReconnectAttempts(attempts),
    ));
}

#[test]
fn a_lost_ssh_shell_opens_again_in_its_place_when_the_setting_asks_up_to_its_attempts() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    ssh_auto_reconnect(&mut app, 2);
    let (tab, attempt) = live_ssh(&mut app);
    let lost = ConnectionEvent::Failed(UiError::ConnectionLost);
    wake(
        &event(&mut app, tab, attempt, lost),
        tab,
        attempt,
        Duration::from_secs(2),
    );
    let retry = app.tab(tab).expect("tab").retry.expect("waiting");
    assert_eq!((retry.attempt, retry.max), (1, 2), "the setting's attempts");

    // The time comes: the shell opens again in a tab in the same place, the chain with it.
    let index = app.tabs.iter().position(|t| t.id == tab).expect("index");
    let effects = app.update(Message::AutoReconnect { tab, attempt });
    let [
        Effect::Connect {
            tab: again,
            attempt: second,
            ..
        },
    ] = effects.as_slice()
    else {
        panic!("{effects:?}");
    };
    let (again, second) = (*again, *second);
    assert_eq!(app.tabs[index].id, again, "in the place of the lost one");
    assert_eq!(app.active, Some(again));
    assert_eq!(
        app.tab(again).expect("tab").retry.map(|r| r.attempt),
        Some(1)
    );

    // It cannot connect yet: the second and last attempt, after 5 seconds.
    wake(
        &event(&mut app, again, second, dropped()),
        again,
        second,
        Duration::from_secs(5),
    );
    assert_eq!(
        app.tab(again).expect("tab").retry.map(|r| r.attempt),
        Some(2)
    );
    let effects = app.update(Message::AutoReconnect {
        tab: again,
        attempt: second,
    });
    let [
        Effect::Connect {
            tab: third_tab,
            attempt: third,
            ..
        },
    ] = effects.as_slice()
    else {
        panic!("{effects:?}");
    };
    let (third_tab, third) = (*third_tab, *third);
    assert!(
        event(&mut app, third_tab, third, dropped()).is_empty(),
        "no third"
    );
    assert_eq!(
        app.tab(third_tab).expect("tab").retry,
        None,
        "left to the user"
    );
}

#[test]
fn a_shell_back_ends_its_chain_and_a_clean_end_or_a_refusal_never_starts_one() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    ssh_auto_reconnect(&mut app, 3);
    let (tab, attempt) = live_ssh(&mut app);
    event(
        &mut app,
        tab,
        attempt,
        ConnectionEvent::Failed(UiError::ConnectionLost),
    );
    let effects = app.update(Message::AutoReconnect { tab, attempt });
    let [
        Effect::Connect {
            tab: again,
            attempt: second,
            ..
        },
    ] = effects.as_slice()
    else {
        panic!("{effects:?}");
    };
    let (again, second) = (*again, *second);
    event(
        &mut app,
        again,
        second,
        ConnectionEvent::Connected {
            input: std::sync::Arc::new(NullSink),
        },
    );
    assert_eq!(
        app.tab(again).expect("tab").retry,
        None,
        "back: the chain ends"
    );

    // The shell exits: nothing comes back.
    assert!(
        event(
            &mut app,
            again,
            second,
            ConnectionEvent::Closed {
                exit_status: Some(0)
            }
        )
        .is_empty()
    );

    // A refusal needs the user.
    let (tab, attempt) = live_ssh(&mut app);
    let refused = ConnectionEvent::Failed(UiError::AuthenticationFailed {
        tried: Vec::new(),
        agent_keys: None,
    });
    assert!(event(&mut app, tab, attempt, refused).is_empty());
    assert_eq!(app.tab(tab).expect("tab").retry, None);
}

#[test]
fn an_attempts_setting_out_of_the_csharp_range_is_ignored() {
    use heimdall_app::SettingsMessage;
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    for refused in [0, 11] {
        app.update(Message::Settings(
            SettingsMessage::SshAutoReconnectAttempts(refused),
        ));
        assert_eq!(app.settings().ssh_auto_reconnect_attempts, 3, "{refused}");
    }
    app.update(Message::Settings(
        SettingsMessage::SshAutoReconnectAttempts(10),
    ));
    assert_eq!(app.settings().ssh_auto_reconnect_attempts, 10);
}

#[test]
fn a_shell_in_the_background_comes_back_there() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    ssh_auto_reconnect(&mut app, 3);
    let (tab, attempt) = live_ssh(&mut app);
    let (shown, _) = live(&mut app);
    assert_eq!(app.active, Some(shown));
    event(
        &mut app,
        tab,
        attempt,
        ConnectionEvent::Failed(UiError::ConnectionLost),
    );
    let index = app.tabs.iter().position(|t| t.id == tab).expect("index");
    app.update(Message::AutoReconnect { tab, attempt });
    assert_eq!(app.active, Some(shown), "the tab looked at stays in front");
    let again = &app.tabs[index];
    assert_ne!(again.id, tab);
    assert_eq!(
        again.retry.map(|r| r.attempt),
        Some(1),
        "the chain went with it"
    );
}

#[test]
fn the_settings_choose_how_many_times_a_desktop_is_tried_again() {
    use heimdall_app::SettingsMessage;

    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    app.update(Message::Settings(
        SettingsMessage::RdpAutoReconnectAttempts(0),
    ));
    assert_eq!(
        app.settings().rdp_auto_reconnect_attempts,
        RDP_AUTO_RECONNECT_ATTEMPTS_MAX,
        "out of the range: kept"
    );
    app.update(Message::Settings(
        SettingsMessage::RdpAutoReconnectAttempts(3),
    ));
    let (tab, attempt) = live(&mut app);
    event(&mut app, tab, attempt, dropped());
    let retry = app.tab(tab).expect("tab").retry.expect("waiting");
    assert_eq!((retry.attempt, retry.max), (1, 3));
}

#[test]
fn the_settings_choose_how_long_a_desktop_may_take_to_log_on() {
    use heimdall_app::SettingsMessage;

    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let logon = |app: &mut App| {
        let effects = app.update(Message::OpenRdp(ProfileId::new("dc")));
        let [Effect::ConnectRdp { request, .. }] = effects.as_slice() else {
            panic!("one connection: {effects:?}");
        };
        request.logon_timeout
    };
    assert_eq!(
        logon(&mut app),
        Some(std::time::Duration::from_secs(45)),
        "the C# default"
    );
    app.update(Message::Settings(SettingsMessage::RdpConnectTimeout(3)));
    assert_eq!(
        app.settings().rdp_connect_timeout,
        45,
        "out of the range: kept"
    );
    app.update(Message::Settings(SettingsMessage::RdpConnectTimeout(0)));
    assert_eq!(logon(&mut app), None, "off");
}

#[test]
fn the_session_bars_disconnect_asks_then_ends_the_desktop_and_keeps_the_tab() {
    use heimdall_app::Dialog;

    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (tab, attempt) = live(&mut app);
    app.update(Message::DisconnectDesktop(tab));
    assert!(
        matches!(app.dialog, Some(Dialog::ConfirmDisconnectDesktop { tab: asked, .. }) if asked == tab),
        "asked first, as the C# RdpConfirmDisconnect: {:?}",
        app.dialog
    );
    app.update(Message::DismissDialog);
    assert_eq!(app.tab(tab).expect("tab").phase, Phase::Connected, "kept");

    app.update(Message::DisconnectDesktop(tab));
    app.update(Message::ConfirmDialog);
    let ended = app.tab(tab).expect("the tab stays");
    assert!(
        matches!(ended.phase, Phase::Closed { .. }),
        "{:?}",
        ended.phase
    );
    assert!(ended.desktop.is_none());
    assert!(app.can_reconnect(ended), "Reconnect is offered");
    // What the old session still says changes nothing, and nothing opens by itself.
    assert!(event(&mut app, tab, attempt, dropped()).is_empty());
    let ended = app.tab(tab).expect("tab");
    assert!(ended.retry.is_none() && matches!(ended.phase, Phase::Closed { .. }));
}
