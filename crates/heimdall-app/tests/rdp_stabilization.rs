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

//! The wait an RDP desktop following its tab keeps after connecting, as the C# post-connect
//! stabilization: started on each connection, ended by its time or skipped, and ticked only
//! while some desktop waits.

use std::path::Path;
use std::time::{Duration, Instant};

use heimdall_app::{
    App, AppConfig, AttemptId, ConnectionEvent, Effect, Message, Notice, SettingsMessage, TabId,
};
use heimdall_core::profile::{Forwards, ProfileId, RdpExtras, RdpOptions, RdpProfile, Resolution};
use heimdall_core::settings::RDP_RESIZE_ENABLE_DELAY_DEFAULT_MS;
use heimdall_core::store::ProfileStore;
use heimdall_rdp::Framebuffer;
use heimdall_ssh::AgentSource;
use heimdall_term::GridSize;
use tokio::sync::{mpsc, watch};

/// What the session of a desktop is asked.
type Asked = watch::Receiver<Option<(u16, u16)>>;

/// An application holding one RDP profile with `options`.
fn app_with(dir: &Path, options: RdpOptions) -> App {
    let profiles_file = dir.join("profiles.toml");
    let mut store = ProfileStore::open(&profiles_file).expect("store");
    store.merge_rdp([RdpProfile {
        extras: RdpExtras::default(),
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
        redirect_clipboard: true,
        redirect_drives: false,
        options,
        vault_entry: None,
        forwards: Forwards::default(),
        follow_defaults: false,
        several_servers: false,
        anti_idle: false,
        auto_reconnect: true,
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

/// The attempt of the connection `effects` start.
fn attempt_of(effects: &[Effect]) -> (TabId, AttemptId) {
    match effects {
        [Effect::ConnectRdp { tab, attempt, .. }] => (*tab, *attempt),
        _ => panic!("{effects:?}"),
    }
}

/// The desktop of `attempt` ready; what its session is asked.
fn ready(app: &mut App, tab: TabId, attempt: AttemptId) -> Asked {
    let (input, _received) = mpsc::unbounded_channel();
    let (size, asked) = watch::channel(None);
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
    asked
}

/// Until when the desktop of `tab` settles.
fn until(app: &App, tab: TabId) -> Option<Instant> {
    app.tab(tab)
        .and_then(|found| found.desktop.as_ref())
        .expect("desktop")
        .stabilizing_until()
}

#[test]
fn a_desktop_following_its_tab_settles_for_the_default_wait_then_follows_it() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app_with(dir.path(), RdpOptions::default());
    assert_eq!(app.stabilization_interval(), None, "nothing to tick for");
    let (tab, attempt) = attempt_of(&app.update(Message::OpenRdp(ProfileId::new("dc"))));
    let before = Instant::now();
    let asked = ready(&mut app, tab, attempt);
    let after = Instant::now();
    let wait = Duration::from_millis(u64::from(RDP_RESIZE_ENABLE_DELAY_DEFAULT_MS));
    let end = until(&app, tab).expect("settling");
    assert!(
        end >= before + wait && end <= after + wait,
        "the C# 10 s from the connection"
    );
    assert_eq!(app.stabilization_interval(), Some(Duration::from_secs(1)));

    // The first size replaces the 1280x800 connected with; a later one waits.
    app.update(Message::DesktopResize {
        tab,
        width: 1600,
        height: 900,
    });
    assert_eq!(*asked.borrow(), Some((1600, 900)));
    app.update(Message::DesktopResize {
        tab,
        width: 1200,
        height: 800,
    });
    assert_eq!(*asked.borrow(), Some((1600, 900)), "held");

    app.update(Message::StabilizationTick(
        end.checked_sub(Duration::from_millis(1))
            .expect("an instant"),
    ));
    assert_eq!(until(&app, tab), Some(end), "not over yet");
    app.update(Message::StabilizationTick(end));
    assert_eq!(until(&app, tab), None, "over");
    assert_eq!(*asked.borrow(), Some((1200, 800)), "the latest, at the end");
    assert_eq!(
        app.stabilization_interval(),
        None,
        "the tick stops with the wait"
    );
    assert_eq!(app.notice(), None, "said only when skipped");
}

#[test]
fn skip_stabilization_follows_the_tab_now_and_says_so() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app_with(dir.path(), RdpOptions::default());
    let (tab, attempt) = attempt_of(&app.update(Message::OpenRdp(ProfileId::new("dc"))));
    let asked = ready(&mut app, tab, attempt);
    for (width, height) in [(1600, 900), (1440, 810)] {
        app.update(Message::DesktopResize { tab, width, height });
    }
    app.update(Message::SkipStabilization(tab));
    assert_eq!(until(&app, tab), None);
    assert_eq!(*asked.borrow(), Some((1440, 810)));
    assert_eq!(app.notice(), Some(&Notice::StabilizationSkipped));
    assert_eq!(app.stabilization_interval(), None);
}

#[test]
fn each_connection_settles_again_a_reconnection_included() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app_with(dir.path(), RdpOptions::default());
    let (tab, attempt) = attempt_of(&app.update(Message::OpenRdp(ProfileId::new("dc"))));
    let _asked = ready(&mut app, tab, attempt);
    app.update(Message::SkipStabilization(tab));
    assert_eq!(until(&app, tab), None);

    // The connection lost, then made again.
    app.update(Message::Connection {
        tab,
        attempt,
        event: ConnectionEvent::Ended {
            reason: heimdall_rdp::Ending::Other("Connection lost".to_owned()),
        },
    });
    let (tab, attempt) = attempt_of(&app.update(Message::ReconnectTab(tab)));
    let reconnected = Instant::now();
    let asked = ready(&mut app, tab, attempt);
    let end = until(&app, tab).expect("the countdown starts again");
    assert!(end > reconnected, "from the new connection");
    app.update(Message::DesktopResize {
        tab,
        width: 1600,
        height: 900,
    });
    assert_eq!(
        *asked.borrow(),
        Some((1600, 900)),
        "the new connection's first size is not held either"
    );
}

#[test]
fn the_profile_s_own_wait_comes_before_the_settings_and_zero_turns_it_off() {
    let dir = tempfile::tempdir().expect("dir");
    let own = RdpOptions {
        resize_enable_delay_ms: Some(2_500),
        ..RdpOptions::default()
    };
    let mut app = app_with(dir.path(), own);
    let (tab, attempt) = attempt_of(&app.update(Message::OpenRdp(ProfileId::new("dc"))));
    let before = Instant::now();
    let _asked = ready(&mut app, tab, attempt);
    let end = until(&app, tab).expect("settling");
    assert!(end <= Instant::now() + Duration::from_millis(2_500) && end >= before);

    // Off in the settings, a profile without its own does not wait.
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app_with(dir.path(), RdpOptions::default());
    app.update(Message::Settings(SettingsMessage::RdpResizeEnableDelay(0)));
    assert_eq!(app.settings().rdp_resize_enable_delay_ms, 0);
    let (tab, attempt) = attempt_of(&app.update(Message::OpenRdp(ProfileId::new("dc"))));
    let asked = ready(&mut app, tab, attempt);
    assert_eq!(until(&app, tab), None);
    for (width, height) in [(1600, 900), (1200, 800)] {
        app.update(Message::DesktopResize { tab, width, height });
    }
    assert_eq!(*asked.borrow(), Some((1200, 800)), "followed at once");

    // Out of the range, the settings refuse it.
    app.update(Message::Settings(SettingsMessage::RdpResizeEnableDelay(
        500,
    )));
    assert_eq!(app.settings().rdp_resize_enable_delay_ms, 0, "kept");
}

#[test]
fn a_desktop_keeping_its_own_size_or_the_tab_s_first_one_never_waits() {
    for options in [
        RdpOptions {
            resolution: Resolution::Fixed,
            ..RdpOptions::default()
        },
        RdpOptions {
            dynamic_resolution: false,
            ..RdpOptions::default()
        },
    ] {
        let dir = tempfile::tempdir().expect("dir");
        let mut app = app_with(dir.path(), options);
        let (tab, attempt) = attempt_of(&app.update(Message::OpenRdp(ProfileId::new("dc"))));
        let _asked = ready(&mut app, tab, attempt);
        assert_eq!(until(&app, tab), None, "{options:?}");
        assert_eq!(app.stabilization_interval(), None, "{options:?}");
    }
}

#[test]
fn a_resolution_menu_choice_made_while_it_settles_is_asked_at_the_end() {
    use heimdall_app::{ResolutionChoice, TabMenuMessage};

    let dir = tempfile::tempdir().expect("dir");
    let mut app = app_with(dir.path(), RdpOptions::default());
    let (tab, attempt) = attempt_of(&app.update(Message::OpenRdp(ProfileId::new("dc"))));
    let asked = ready(&mut app, tab, attempt);
    app.update(Message::DesktopResize {
        tab,
        width: 1600,
        height: 900,
    });
    let end = until(&app, tab).expect("settling");
    app.update(Message::TabMenu(TabMenuMessage::Resolution {
        tab,
        choice: ResolutionChoice::Fixed {
            width: 1280,
            height: 720,
        },
    }));
    assert_eq!(*asked.borrow(), Some((1600, 900)), "deferred");
    app.update(Message::StabilizationTick(end));
    assert_eq!(*asked.borrow(), Some((1280, 720)));
}
