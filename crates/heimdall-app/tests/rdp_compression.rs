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

//! A tab whose RDP server sent what the built-in client could not read compressed: said in
//! the status bar, and its connections go without compression from then on, its
//! reconnections included; the profile is not changed.

use std::path::Path;

use heimdall_app::{App, AppConfig, AttemptId, ConnectionEvent, Effect, Message, Notice, TabId};
use heimdall_core::profile::{Forwards, ProfileId, RdpExtras, RdpOptions, RdpProfile};
use heimdall_core::store::ProfileStore;
use heimdall_rdp::Framebuffer;
use heimdall_ssh::AgentSource;
use heimdall_term::GridSize;
use tokio::sync::{mpsc, watch};

/// An application holding one RDP profile, compressed as by default.
fn app(dir: &Path) -> App {
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
        options: RdpOptions::default(),
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

/// The connection `effects` start: its tab, its attempt and whether it compresses.
fn connecting(effects: &[Effect]) -> (TabId, AttemptId, bool) {
    match effects {
        [
            Effect::ConnectRdp {
                tab,
                attempt,
                request,
            },
        ] => (*tab, *attempt, request.profile.extras.compression),
        _ => panic!("{effects:?}"),
    }
}

fn event(app: &mut App, tab: TabId, attempt: AttemptId, event: ConnectionEvent) -> Vec<Effect> {
    app.update(Message::Connection {
        tab,
        attempt,
        event,
    })
}

fn ready() -> ConnectionEvent {
    let (input, _received) = mpsc::unbounded_channel();
    let (size, _asked) = watch::channel(None);
    ConnectionEvent::RdpReady {
        framebuffer: Framebuffer::new(64, 48),
        input,
        size,
        clipboard: None,
    }
}

/// The server cannot take the tab's size live: the tab connects again at it.
fn resize_refused() -> ConnectionEvent {
    ConnectionEvent::DesktopResizeRefused {
        width: 1600,
        height: 900,
    }
}

/// Whether the saved profile still compresses.
fn profile_compresses(app: &App) -> bool {
    app.rdp_profiles()
        .iter()
        .find(|profile| profile.id == ProfileId::new("dc"))
        .expect("profile")
        .extras
        .compression
}

#[test]
fn a_session_that_could_not_read_what_was_compressed_connects_again_without_it() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (tab, attempt, compressed) =
        connecting(&app.update(Message::OpenRdp(ProfileId::new("dc"))));
    assert!(compressed, "compressed by default");
    event(&mut app, tab, attempt, ready());

    let effects = event(
        &mut app,
        tab,
        attempt,
        ConnectionEvent::RdpCompressionOff { reconnect: true },
    );
    assert_eq!(app.notice(), Some(&Notice::RdpCompressionOff));
    let (again, next, compressed) = connecting(&effects);
    assert_eq!(again, tab, "the same tab");
    assert_ne!(next, attempt);
    assert!(!compressed, "without compression");

    // Later reconnections of the tab stay so; the profile is not changed.
    event(&mut app, tab, next, ready());
    let (_, _, compressed) = connecting(&event(&mut app, tab, next, resize_refused()));
    assert!(!compressed, "kept across a reconnect");
    assert!(profile_compresses(&app), "the profile is the user's");
}

#[test]
fn a_connection_opened_again_without_compression_is_said_and_its_tab_keeps_it_off() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (tab, attempt, _) = connecting(&app.update(Message::OpenRdp(ProfileId::new("dc"))));

    // heimdall-rdp connected again without compression already: said once the session
    // shows, nothing more to start.
    event(&mut app, tab, attempt, ready());
    let effects = event(
        &mut app,
        tab,
        attempt,
        ConnectionEvent::RdpCompressionOff { reconnect: false },
    );
    assert!(effects.is_empty(), "{effects:?}");
    assert_eq!(app.notice(), Some(&Notice::RdpCompressionOff));
    let (_, _, compressed) = connecting(&event(&mut app, tab, attempt, resize_refused()));
    assert!(!compressed);

    // A tab opened anew compresses as its profile says.
    let (_, _, compressed) = connecting(&app.update(Message::OpenRdp(ProfileId::new("dc"))));
    assert!(compressed);
}
