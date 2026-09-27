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

//! SSH profiles reached through gateways: the route an opening asks for, a route that cannot
//! be followed, and a gateway that editing a profile or importing it keeps.

use std::path::Path;

use heimdall_app::{App, AppConfig, Dialog, Effect, Message, Phase, UiError};
use heimdall_core::paths::{LEGACY_SERVERS_FILE_NAME, LEGACY_SETTINGS_FILE_NAME};
use heimdall_core::profile::{ProfileId, SshGateway, SshProfile};
use heimdall_core::store::{ProfileStore, RouteError};
use heimdall_ssh::AgentSource;
use heimdall_term::GridSize;

fn gateway(id: &str, parent: Option<&str>) -> SshGateway {
    SshGateway {
        id: ProfileId::new(id),
        name: id.to_uppercase(),
        host: format!("{id}.lab"),
        port: 22,
        username: Some("ops".to_owned()),
        key_path: None,
        parent: parent.map(ProfileId::new),
    }
}

fn web(gateway: Option<&str>) -> SshProfile {
    SshProfile {
        id: ProfileId::new("web"),
        name: "Web".to_owned(),
        group: None,
        host: "web.internal".to_owned(),
        port: 22,
        username: Some("admin".to_owned()),
        key_path: None,
        gateway: gateway.map(ProfileId::new),
    }
}

fn app(dir: &Path, profile: SshProfile, gateways: Vec<SshGateway>) -> App {
    let profiles_file = dir.join("profiles.toml");
    let mut store = ProfileStore::open(&profiles_file).expect("store");
    store.merge([profile]);
    store.merge_gateways(gateways);
    store.save().expect("save");
    App::new(config(dir))
}

fn config(dir: &Path) -> AppConfig {
    AppConfig {
        profiles_file: dir.join("profiles.toml"),
        known_hosts: dir.join("known_hosts"),
        legacy_dir: Some(dir.join("legacy")),
        agent: AgentSource::Disabled,
        initial_grid: GridSize { cols: 80, rows: 24 },
        files_start: dir.to_owned(),
    }
}

#[test]
fn opening_a_profile_asks_for_its_gateways_nearest_first() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(
        dir.path(),
        web(Some("inner")),
        vec![gateway("inner", Some("outer")), gateway("outer", None)],
    );
    let effects = app.update(Message::OpenProfile(ProfileId::new("web")));
    let Some(Effect::Connect { request, .. }) = effects.first() else {
        panic!("{effects:?}");
    };
    let hops: Vec<&str> = request.route.iter().map(|hop| hop.host.as_str()).collect();
    assert_eq!(hops, ["outer.lab", "inner.lab"]);
    assert!(
        request.route.iter().all(|hop| hop.gateway.is_none()),
        "each hop is direct"
    );
    assert_eq!(request.profile.host, "web.internal");

    // The Files view takes the same way.
    let effects = app.update(Message::OpenFiles(ProfileId::new("web")));
    let Some(Effect::Connect { request, .. }) = effects.first() else {
        panic!("{effects:?}");
    };
    assert_eq!(request.route.len(), 2);
}

#[test]
fn a_profile_without_a_gateway_goes_straight_to_its_server() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path(), web(None), vec![gateway("unused", None)]);
    let effects = app.update(Message::OpenProfile(ProfileId::new("web")));
    let Some(Effect::Connect { request, .. }) = effects.first() else {
        panic!("{effects:?}");
    };
    assert!(request.route.is_empty());
}

#[test]
fn a_gateway_that_is_not_there_fails_the_tab_without_an_attempt() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path(), web(Some("gone")), Vec::new());
    let effects = app.update(Message::OpenProfile(ProfileId::new("web")));
    assert!(effects.is_empty(), "{effects:?}");
    let tab = app.active_tab().expect("a tab says why");
    assert_eq!(
        tab.phase,
        Phase::Failed(UiError::Route(RouteError::MissingGateway(ProfileId::new(
            "gone"
        ))))
    );
}

#[test]
fn editing_a_profile_keeps_its_gateway() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path(), web(Some("outer")), vec![gateway("outer", None)]);
    app.update(Message::EditProfile(ProfileId::new("web")));
    assert!(matches!(app.dialog, Some(Dialog::EditProfile { .. })));
    app.update(Message::ProfileField {
        field: heimdall_app::profile_draft::ProfileField::Name,
        value: "Web front".to_owned(),
    });
    app.update(Message::ConfirmDialog);
    let stored = ProfileStore::open(dir.path().join("profiles.toml")).expect("reopen");
    let profile = &stored.ssh_profiles()[0];
    assert_eq!(profile.name, "Web front");
    assert_eq!(
        profile.gateway,
        Some(ProfileId::new("outer")),
        "the form does not drop it"
    );
}

#[test]
fn importing_brings_the_gateways_and_the_profiles_through_them() {
    let dir = tempfile::tempdir().expect("dir");
    let legacy = dir.path().join("legacy");
    std::fs::create_dir_all(&legacy).expect("legacy dir");
    std::fs::write(
        legacy.join(LEGACY_SERVERS_FILE_NAME),
        r#"{"servers": [{"id": "db", "displayName": "DB", "remoteServer": "db.internal",
            "connectionType": "SSH", "sshGatewayId": "bastion"}]}"#,
    )
    .expect("servers");
    std::fs::write(
        legacy.join(LEGACY_SETTINGS_FILE_NAME),
        r#"{"sshGateways": [{"id": "bastion", "name": "Bastion", "host": "bastion.example.org",
            "port": 2222, "user": "jump"}]}"#,
    )
    .expect("settings");
    let mut app = App::new(config(dir.path()));
    app.update(Message::ImportLegacy);
    assert!(
        matches!(app.dialog, Some(Dialog::ImportDone(_))),
        "{:?}",
        app.dialog
    );
    let effects = app.update(Message::OpenProfile(ProfileId::new("db")));
    let Some(Effect::Connect { request, .. }) = effects.first() else {
        panic!("{effects:?}");
    };
    let hops: Vec<(&str, u16)> = request
        .route
        .iter()
        .map(|hop| (hop.host.as_str(), hop.port))
        .collect();
    assert_eq!(hops, [("bastion.example.org", 2222)]);
}
