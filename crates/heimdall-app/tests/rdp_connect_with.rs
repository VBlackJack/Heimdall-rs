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

//! The profile menu's one-time "Connect with", as the C# `ConnectEmbedded` and
//! `ConnectExternal`: an RDP profile opens in the mode chosen this once, its own mode
//! unchanged, through every gate a plain connection passes; Remote Desktop Connection goes
//! through an SSH gateway as a plain connection does, and Reconnect keeps the mode chosen.

use std::path::Path;

use heimdall_app::windows_hello::HelloRefusal;
use heimdall_app::{
    App, AppConfig, Effect, Message, Notice, Phase, SettingsMessage, SystemCredentials, UiError,
};
use heimdall_core::profile::{
    Forwards, ProfileId, RdpExtras, RdpMode, RdpOptions, RdpProfile, SshGateway,
};
use heimdall_core::store::{ProfileStore, RouteError};
use heimdall_ssh::AgentSource;
use heimdall_term::GridSize;

fn profile(id: &str, mode: RdpMode, gateway: Option<&str>) -> RdpProfile {
    RdpProfile {
        extras: RdpExtras {
            external: mode.is_external(),
            ..RdpExtras::default()
        },
        id: ProfileId::new(id),
        name: id.to_owned(),
        group: None,
        host: format!("{id}.lab"),
        port: 3389,
        username: Some("admin".to_owned()),
        domain: None,
        allow_tls_only: false,
        gateway: gateway.map(ProfileId::new),
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
    }
}

fn bastion() -> SshGateway {
    SshGateway {
        id: ProfileId::new("bastion"),
        name: "Bastion".to_owned(),
        host: "bastion.lab".to_owned(),
        port: 22,
        username: Some("jump".to_owned()),
        key_path: None,
        parent: None,
    }
}

/// `mstsc` set to Remote Desktop Connection, `tab` to a tab; `far` set to Remote Desktop
/// Connection through an SSH gateway; `lost` likewise through a gateway gone since.
fn app(dir: &Path) -> App {
    let profiles_file = dir.join("profiles.toml");
    let mut store = ProfileStore::open(&profiles_file).expect("store");
    store.merge_gateways(vec![bastion()]);
    store.merge_rdp([
        profile("mstsc", RdpMode::External, None),
        profile("tab", RdpMode::Embedded, None),
        profile("far", RdpMode::External, Some("bastion")),
        profile("lost", RdpMode::External, Some("gone")),
    ]);
    store.save().expect("save");
    App::new(AppConfig {
        profiles_file,
        known_hosts: dir.join("known_hosts"),
        legacy_dir: None,
        agent: AgentSource::Disabled,
        initial_grid: GridSize { cols: 80, rows: 24 },
        files_start: dir.to_owned(),
        system_credentials: SystemCredentials::memory(),
    })
}

fn once(id: &str, mode: RdpMode) -> Message {
    Message::OpenRdpWith {
        id: ProfileId::new(id),
        mode,
    }
}

/// Each profile's mode as the application holds it, and as the file does.
fn modes(app: &App, dir: &Path) -> (Vec<bool>, Vec<bool>) {
    let held = app
        .rdp_profiles()
        .iter()
        .map(|profile| profile.extras.external)
        .collect();
    let store = ProfileStore::open(dir.join("profiles.toml")).expect("store");
    let saved = store
        .rdp_profiles()
        .iter()
        .map(|profile| profile.extras.external)
        .collect();
    (held, saved)
}

const SAVED_MODES: [bool; 4] = [true, false, true, true];

#[test]
fn embedded_once_opens_a_tab_for_a_profile_set_to_remote_desktop_connection() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let effects = app.update(once("mstsc", RdpMode::Embedded));
    let [Effect::ConnectRdp { tab, request, .. }] = effects.as_slice() else {
        panic!("{effects:?}");
    };
    assert_eq!(request.profile.id, ProfileId::new("mstsc"));
    assert!(!request.profile.extras.external);
    let opened = app.tab(*tab).expect("a tab");
    assert_eq!(opened.rdp_mode_override, Some(RdpMode::Embedded));
    assert_eq!(app.active_tab().map(|tab| tab.id), Some(*tab));

    // This once: the profile is not changed, and opens as it says next time.
    assert_eq!(
        modes(&app, dir.path()),
        (SAVED_MODES.to_vec(), SAVED_MODES.to_vec())
    );
    let effects = app.update(Message::OpenRdp(ProfileId::new("mstsc")));
    assert!(
        matches!(effects.as_slice(), [Effect::LaunchRdpExternal { .. }]),
        "{effects:?}"
    );
    assert_eq!(app.tabs.len(), 1, "no second tab");
}

#[test]
fn external_once_opens_remote_desktop_connection_for_a_profile_set_to_a_tab() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let effects = app.update(once("tab", RdpMode::External));
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
    assert_eq!(name, "tab");
    assert!(
        content.starts_with("full address:s:tab.lab:3389\r\n"),
        "{content}"
    );
    assert!(app.tabs.is_empty(), "no tab: the window is mstsc's");

    assert_eq!(
        modes(&app, dir.path()),
        (SAVED_MODES.to_vec(), SAVED_MODES.to_vec())
    );
    let effects = app.update(Message::OpenRdp(ProfileId::new("tab")));
    assert!(
        matches!(effects.as_slice(), [Effect::ConnectRdp { .. }]),
        "{effects:?}"
    );
}

#[test]
fn external_once_behind_an_ssh_gateway_goes_through_it_as_embedded_once_does() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let effects = app.update(once("far", RdpMode::External));
    let [Effect::OpenMstscRoute { request, .. }] = effects.as_slice() else {
        panic!("{effects:?}: never sent straight to the server");
    };
    assert!(request.before.is_empty());
    assert_eq!(
        (request.gateway.host.as_str(), request.gateway.port),
        ("bastion.lab", 22),
        "through its gateway"
    );
    assert_eq!(
        (request.profile.host.as_str(), request.profile.port),
        ("far.lab", 3389)
    );
    assert!(app.tabs.is_empty());
    assert_eq!(app.notice(), None);
    assert_eq!(
        modes(&app, dir.path()),
        (SAVED_MODES.to_vec(), SAVED_MODES.to_vec())
    );

    let effects = app.update(once("far", RdpMode::Embedded));
    let [Effect::ConnectRdp { request, .. }] = effects.as_slice() else {
        panic!("{effects:?}");
    };
    let hops: Vec<(&str, u16)> = request
        .route
        .iter()
        .map(|hop| (hop.host.as_str(), hop.port))
        .collect();
    assert_eq!(hops, [("bastion.lab", 22)], "through its gateway");
}

#[test]
fn a_profile_not_rdp_or_gone_opens_nothing() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    for mode in RdpMode::ALL {
        assert!(app.update(once("nowhere", mode)).is_empty());
        assert!(app.update(once("bastion", mode)).is_empty(), "a gateway");
    }
    assert!(app.tabs.is_empty());
}

#[test]
fn windows_hello_is_asked_first_in_either_mode_and_a_refusal_opens_nothing() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    app.update(Message::Settings(SettingsMessage::RequireWindowsHello(
        true,
    )));
    app.update(Message::Settings(
        SettingsMessage::WindowsHelloGraceMinutes(0),
    ));

    for mode in RdpMode::ALL {
        let effects = app.update(once("mstsc", mode));
        assert!(
            matches!(effects.as_slice(), [Effect::VerifyWindowsHello]),
            "{mode:?}: {effects:?}"
        );
        assert!(app.tabs.is_empty(), "{mode:?}: nothing before the answer");
        let effects = app.update(Message::WindowsHello(Err(HelloRefusal::NotVerified)));
        assert!(effects.is_empty(), "{mode:?}: {effects:?}");
        assert!(app.tabs.is_empty(), "{mode:?}: fail closed");
    }

    assert!(matches!(
        app.update(once("mstsc", RdpMode::Embedded)).as_slice(),
        [Effect::VerifyWindowsHello]
    ));
    let effects = app.update(Message::WindowsHello(Ok(())));
    let [Effect::ConnectRdp { tab, .. }] = effects.as_slice() else {
        panic!("{effects:?}");
    };
    assert_eq!(
        app.tab(*tab).expect("tab").rdp_mode_override,
        Some(RdpMode::Embedded),
        "the mode chosen, past the gate"
    );

    assert!(matches!(
        app.update(once("tab", RdpMode::External)).as_slice(),
        [Effect::VerifyWindowsHello]
    ));
    let effects = app.update(Message::WindowsHello(Ok(())));
    assert!(
        matches!(effects.as_slice(), [Effect::LaunchRdpExternal { .. }]),
        "{effects:?}"
    );

    // Behind an SSH gateway too: nothing is dialled before the answer.
    for message in [
        Message::OpenRdp(ProfileId::new("far")),
        once("far", RdpMode::External),
    ] {
        assert!(matches!(
            app.update(message).as_slice(),
            [Effect::VerifyWindowsHello]
        ));
        assert!(
            app.update(Message::WindowsHello(Err(HelloRefusal::NotVerified)))
                .is_empty(),
            "fail closed: no route opened"
        );
    }
    assert!(matches!(
        app.update(Message::OpenRdp(ProfileId::new("far")))
            .as_slice(),
        [Effect::VerifyWindowsHello]
    ));
    let effects = app.update(Message::WindowsHello(Ok(())));
    assert!(
        matches!(effects.as_slice(), [Effect::OpenMstscRoute { .. }]),
        "{effects:?}"
    );
}

#[test]
fn reconnect_keeps_the_mode_chosen_not_the_profiles() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    // Set to Remote Desktop Connection, opened in a tab this once: its gateway is gone, so
    // the tab fails at once.
    assert!(app.update(once("lost", RdpMode::Embedded)).is_empty());
    let failed = app.tabs[0].id;
    let missing = Phase::Failed(UiError::Route(RouteError::MissingGateway(ProfileId::new(
        "gone",
    ))));
    assert_eq!(app.tabs[0].phase, missing);

    let effects = app.update(Message::ReconnectTab(failed));
    assert!(
        effects.is_empty(),
        "{effects:?}: never Remote Desktop Connection"
    );
    assert_eq!(app.tabs.len(), 1, "in its place");
    assert_ne!(app.tabs[0].id, failed, "opened again");
    assert_eq!(app.tabs[0].rdp_mode_override, Some(RdpMode::Embedded));
    assert_eq!(app.tabs[0].phase, missing);
    assert!(!matches!(app.notice(), Some(Notice::RdpExternalRefused(_))));

    // A plain connection's tab opens again as its profile says.
    let effects = app.update(Message::OpenRdp(ProfileId::new("tab")));
    let [Effect::ConnectRdp { tab, .. }] = effects.as_slice() else {
        panic!("{effects:?}");
    };
    assert_eq!(app.tab(*tab).expect("tab").rdp_mode_override, None);
}
