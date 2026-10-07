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

//! The SSH gateways and what goes through each, as the C# Gateways list and Overview.

use std::path::Path;

use heimdall_app::{App, AppConfig, Dialog, GatewaysMessage, Message, Notice};
use heimdall_core::profile::{ProfileId, SshGateway, SshProfile};
use heimdall_core::store::ProfileStore;
use heimdall_ssh::AgentSource;
use heimdall_term::GridSize;

fn ssh(id: &str, gateway: Option<&str>) -> SshProfile {
    SshProfile {
        id: ProfileId::new(id),
        name: id.to_owned(),
        group: None,
        host: format!("{id}.lab"),
        port: 22,
        username: None,
        key_path: None,
        gateway: gateway.map(ProfileId::new),
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
    }
}

fn gateway(id: &str, parent: Option<&str>) -> SshGateway {
    SshGateway {
        id: ProfileId::new(id),
        name: format!("{id} gateway"),
        host: format!("{id}.example"),
        port: 22,
        username: None,
        key_path: None,
        parent: parent.map(ProfileId::new),
    }
}

/// Two gateways, the second reached through the first, a third reached through one that
/// is not configured; servers through each, one through a missing gateway, one direct.
fn app(dir: &Path) -> App {
    let profiles_file = dir.join("profiles.toml");
    let mut store = ProfileStore::open(&profiles_file).expect("store");
    store.merge_gateways([
        gateway("edge", None),
        gateway("inner", Some("edge")),
        gateway("orphan", Some("gone")),
    ]);
    store.merge([
        ssh("web", Some("edge")),
        ssh("api", Some("edge")),
        ssh("db", Some("inner")),
        ssh("old", Some("gone")),
        ssh("lab", None),
    ]);
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

fn names(sessions: &[heimdall_app::RoutedSession]) -> Vec<&str> {
    sessions
        .iter()
        .map(|session| session.name.as_str())
        .collect()
}

#[test]
fn each_gateway_lists_its_sessions_and_a_missing_one_its_references() {
    let dir = tempfile::tempdir().expect("dir");
    let app = app(dir.path());
    let overview = app.gateway_overview();
    let edge = &overview.gateways[0];
    assert_eq!(edge.name, "edge gateway");
    assert_eq!(names(&edge.sessions), ["api", "web"], "by name");
    assert_eq!(edge.parent, None);
    let inner = &overview.gateways[1];
    assert_eq!(inner.parent.as_deref(), Some("edge gateway"));
    assert_eq!(names(&inner.sessions), ["db"]);
    assert_eq!(overview.gateways[2].parent.as_deref(), Some("gone"));

    assert_eq!(overview.missing.len(), 1);
    let gone = &overview.missing[0];
    assert_eq!(gone.id, ProfileId::new("gone"));
    assert_eq!(names(&gone.sessions), ["old"]);
    assert_eq!(
        gone.gateways,
        [(ProfileId::new("orphan"), "orphan gateway".to_owned())]
    );
    assert_eq!(overview.routed(), 4);
    assert_eq!(overview.unresolved(), 2);
}

#[test]
fn a_gateway_deleted_once_agreed_to_clears_its_references() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    app.update(Message::Gateways(GatewaysMessage::AskDelete(
        ProfileId::new("edge"),
    )));
    assert_eq!(
        app.dialog,
        Some(Dialog::ConfirmDeleteGateway {
            id: ProfileId::new("edge"),
            name: "edge gateway".to_owned(),
            servers: 2,
            gateways: 1,
        })
    );
    // Cancelled: nothing goes.
    app.update(Message::DismissDialog);
    assert_eq!(app.gateways().len(), 3);

    app.update(Message::Gateways(GatewaysMessage::AskDelete(
        ProfileId::new("edge"),
    )));
    app.update(Message::ConfirmDialog);
    assert_eq!(
        app.notice(),
        Some(&Notice::GatewayDeleted("edge gateway".to_owned()))
    );
    let overview = app.gateway_overview();
    assert_eq!(overview.gateways.len(), 2);
    assert_eq!(overview.gateways[0].parent, None, "inner, reached directly");
    assert_eq!(overview.routed(), 2, "web and api connect directly");
    // Saved.
    let saved = ProfileStore::open(dir.path().join("profiles.toml")).expect("store");
    assert_eq!(saved.gateways().len(), 2);
    assert!(
        saved
            .ssh_profiles()
            .iter()
            .all(|profile| profile.id.as_str() != "web" || profile.gateway.is_none())
    );
}

#[test]
fn the_sessions_of_a_missing_gateway_are_reassigned_or_cleared() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let gone = ProfileId::new("gone");
    app.update(Message::Gateways(GatewaysMessage::Reassign {
        missing: gone.clone(),
        to: ProfileId::new("inner"),
    }));
    assert_eq!(app.notice(), Some(&Notice::GatewaysReassigned(1)));
    let overview = app.gateway_overview();
    assert_eq!(names(&overview.gateways[1].sessions), ["db", "old"]);
    // The child gateway is still unresolved: it is edited on its own.
    assert_eq!(overview.unresolved(), 1);

    // Nothing left to clear.
    app.update(Message::Gateways(GatewaysMessage::Clear(gone)));
    assert!(app.notice().is_some());
}

#[test]
fn the_sessions_of_a_missing_gateway_cleared_connect_directly() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    app.update(Message::Gateways(GatewaysMessage::Clear(ProfileId::new(
        "gone",
    ))));
    assert_eq!(app.notice(), Some(&Notice::GatewaysCleared(1)));
    assert_eq!(app.gateway_overview().routed(), 3);
}
