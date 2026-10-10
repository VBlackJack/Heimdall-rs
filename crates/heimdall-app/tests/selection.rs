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

//! Several profiles selected together, as in the C# tree: Ctrl, Shift, and what applies to
//! them all.

use std::path::Path;

use heimdall_app::{App, AppConfig, Dialog, Effect, Message, SelectionMessage};
use heimdall_core::profile::{LocalArguments, LocalCommand, LocalProfile, ProfileId, SshProfile};
use heimdall_core::store::ProfileStore;
use heimdall_ssh::AgentSource;
use heimdall_term::GridSize;

fn ssh(id: &str) -> SshProfile {
    SshProfile {
        id: ProfileId::new(id),
        name: id.to_owned(),
        group: None,
        host: format!("{id}.lab"),
        port: 22,
        username: None,
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
    }
}

fn app(dir: &Path) -> App {
    let profiles_file = dir.join("profiles.toml");
    let mut store = ProfileStore::open(&profiles_file).expect("store");
    store.merge(["a", "b", "c", "d"].map(ssh));
    store.merge_local([LocalProfile {
        id: ProfileId::new("tool"),
        name: "tool".to_owned(),
        group: None,
        command: LocalCommand {
            program: Some("tool".to_owned()),
            arguments: LocalArguments::List(Vec::new()),
            working_directory: None,
            run_as_administrator: false,
        },
        approved: None,
        session_logging: None,
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

fn id(name: &str) -> ProfileId {
    ProfileId::new(name)
}

fn select(app: &mut App, name: &str) {
    app.update(Message::SelectProfile(id(name)));
}

fn toggle(app: &mut App, name: &str) {
    app.update(Message::Selection(SelectionMessage::Toggle(id(name))));
}

fn range(app: &mut App, name: &str) {
    let order = ["a", "b", "c", "d", "tool"].map(id).to_vec();
    app.update(Message::Selection(SelectionMessage::Range {
        to: id(name),
        order,
    }));
}

fn selected(app: &App) -> Vec<String> {
    app.selected_profiles()
        .iter()
        .map(ToString::to_string)
        .collect()
}

#[test]
fn ctrl_adds_and_takes_one_and_a_plain_click_leaves_one() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    select(&mut app, "a");
    assert!(selected(&app).is_empty(), "one is no selection of several");
    assert!(app.is_selected(&id("a")));
    toggle(&mut app, "c");
    assert_eq!(selected(&app), ["a", "c"], "the one clicked before joins");
    toggle(&mut app, "a");
    assert!(selected(&app).is_empty(), "one left");
    assert!(!app.is_selected(&id("a")), "taken out");
    assert!(app.is_selected(&id("c")));
    toggle(&mut app, "d");
    assert_eq!(selected(&app), ["c", "d"]);
    select(&mut app, "b");
    assert!(selected(&app).is_empty());
    assert!(app.is_selected(&id("b")) && !app.is_selected(&id("c")));
    toggle(&mut app, "nowhere");
    assert!(selected(&app).is_empty(), "no such profile");
}

#[test]
fn shift_takes_all_from_the_last_one_clicked_either_way() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    select(&mut app, "b");
    range(&mut app, "d");
    assert_eq!(selected(&app), ["b", "c", "d"]);
    range(&mut app, "a");
    assert_eq!(selected(&app), ["a", "b"], "from the same start, upwards");
    app.update(Message::Selection(SelectionMessage::Range {
        to: id("c"),
        order: vec![id("c")],
    }));
    assert_eq!(selected(&app), ["a", "b"], "a start not shown: unchanged");
}

#[test]
fn connect_selected_opens_all_but_the_local_programs() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    select(&mut app, "a");
    range(&mut app, "tool");
    assert!(app.connects_in_bulk(&id("a")));
    assert!(!app.connects_in_bulk(&id("tool")));
    let effects = app.update(Message::Selection(SelectionMessage::Connect));
    let hosts: Vec<&str> = effects
        .iter()
        .map(|effect| match effect {
            Effect::Connect { request, .. } => request.profile.host.as_str(),
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(hosts, ["a.lab", "b.lab", "c.lab", "d.lab"]);
}

#[test]
fn the_selected_ones_move_duplicate_and_are_deleted_once_asked() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    select(&mut app, "b");
    toggle(&mut app, "c");
    app.update(Message::Selection(SelectionMessage::Move(Some(
        "Batch".to_owned(),
    ))));
    let group = |app: &App, name: &str| app.profile_summary(&id(name)).expect("profile").group;
    assert_eq!(group(&app, "b").as_deref(), Some("Batch"));
    assert_eq!(group(&app, "c").as_deref(), Some("Batch"));
    assert_eq!(group(&app, "a"), None);

    let before = app.profile_summaries().len();
    app.update(Message::Selection(SelectionMessage::Duplicate {
        suffix: " (copy)".to_owned(),
    }));
    assert_eq!(app.profile_summaries().len(), before + 2);

    app.update(Message::Selection(SelectionMessage::RequestDelete));
    assert_eq!(
        app.dialog,
        Some(Dialog::ConfirmDeleteProfiles {
            ids: vec![id("b"), id("c")],
            names: vec!["b".to_owned(), "c".to_owned()],
        })
    );
    app.update(Message::ConfirmDialog);
    assert!(app.profile_summary(&id("b")).is_none());
    assert!(app.profile_summary(&id("c")).is_none());
    assert!(app.profile_summary(&id("a")).is_some());
    assert!(selected(&app).is_empty());
}

#[test]
fn a_long_selection_is_counted_not_listed() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    for index in 0..11 {
        app.update(Message::SelectProfile(id("a")));
        app.update(Message::DuplicateProfile {
            id: id("a"),
            suffix: format!(" {index}"),
        });
    }
    let order: Vec<ProfileId> = app
        .profile_summaries()
        .into_iter()
        .map(|profile| profile.id)
        .collect();
    select(&mut app, order[0].as_str());
    app.update(Message::Selection(SelectionMessage::Range {
        to: order[11].clone(),
        order: order.clone(),
    }));
    app.update(Message::Selection(SelectionMessage::RequestDelete));
    assert!(matches!(
        &app.dialog,
        Some(Dialog::ConfirmDeleteProfiles { ids, names }) if ids.len() == 12 && names.is_empty()
    ));
}

#[test]
fn set_gateway_routes_those_selected_that_can_be_through_one_or_directly() {
    use heimdall_app::Notice;
    use heimdall_core::profile::SshGateway;

    let dir = tempfile::tempdir().expect("dir");
    drop(app(dir.path()));
    let mut store = ProfileStore::open(dir.path().join("profiles.toml")).expect("store");
    store.merge_gateways([SshGateway {
        id: id("edge"),
        name: "Edge".to_owned(),
        host: "edge.lab".to_owned(),
        port: 22,
        username: None,
        key_path: None,
        parent: None,
    }]);
    store.save().expect("save");
    let mut app = App::new(AppConfig {
        profiles_file: dir.path().join("profiles.toml"),
        known_hosts: dir.path().join("known_hosts"),
        legacy_dir: None,
        agent: AgentSource::Disabled,
        initial_grid: GridSize { cols: 80, rows: 24 },
        files_start: dir.path().to_owned(),
        system_credentials: heimdall_app::SystemCredentials::memory(),
    });
    let gateway_of = |app: &App, name: &str| {
        app.profiles()
            .iter()
            .find(|profile| profile.id == id(name))
            .and_then(|profile| profile.gateway.clone())
    };
    select(&mut app, "a");
    toggle(&mut app, "b");
    toggle(&mut app, "tool");
    assert_eq!(
        app.gateway_targets(&app.selected_profiles()),
        2,
        "a local program goes through none"
    );

    let set = |app: &mut App, gateway: Option<&str>| {
        app.update(Message::Selection(SelectionMessage::SetGateway(
            gateway.map(id),
        )));
    };
    set(&mut app, Some("edge"));
    assert_eq!(gateway_of(&app, "a"), Some(id("edge")));
    assert_eq!(gateway_of(&app, "b"), Some(id("edge")));
    assert_eq!(gateway_of(&app, "c"), None, "not selected");
    assert_eq!(app.notice(), Some(&Notice::BulkGatewayUpdated(2)));
    set(&mut app, Some("edge"));
    assert_eq!(
        app.notice(),
        Some(&Notice::BulkGatewayUpdated(0)),
        "already so"
    );
    set(&mut app, Some("nowhere"));
    assert_eq!(
        gateway_of(&app, "a"),
        Some(id("edge")),
        "a gateway not saved is not set"
    );
    set(&mut app, None);
    assert_eq!(gateway_of(&app, "a"), None, "direct");
    assert_eq!(app.notice(), Some(&Notice::BulkGatewayUpdated(2)));
}
