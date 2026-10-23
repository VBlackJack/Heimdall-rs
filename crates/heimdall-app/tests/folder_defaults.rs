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

//! What a folder gives the sessions made in it or moved into it, as the C#
//! `GroupDefaultsDto` does, and what the form says about it.

use std::path::{Path, PathBuf};

use heimdall_app::profile_draft::{DraftProtocol, ProfileField};
use heimdall_app::{
    App, AppConfig, Dialog, FolderMessage, GatewaysMessage, Message, ProfileMenuMessage,
};
use heimdall_core::folder_defaults::{FolderDefaults, InheritedField};
use heimdall_core::metadata::Environment;
use heimdall_core::profile::{ProfileId, SshGateway, SshProfile};
use heimdall_core::store::ProfileStore;
use heimdall_ssh::AgentSource;
use heimdall_term::GridSize;

fn profile(id: &str, group: Option<&str>) -> SshProfile {
    SshProfile {
        id: ProfileId::new(id),
        name: id.to_owned(),
        group: group.map(str::to_owned),
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
        remote_os: heimdall_core::profile::RemoteOs::Auto,
    }
}

/// An app whose `Prod` gives a gateway, a user, a key, a port and an environment, and whose
/// `Prod/Web` gives another user and port.
fn app(dir: &Path) -> App {
    let profiles_file = dir.join("profiles.toml");
    let mut store = ProfileStore::open(&profiles_file).expect("store");
    store.merge_gateways([SshGateway {
        id: ProfileId::new("edge"),
        name: "Edge".to_owned(),
        host: "edge.lab".to_owned(),
        port: 22,
        username: None,
        key_path: None,
        parent: None,
    }]);
    store.merge([
        profile("web", Some("Prod/Web")),
        profile("loose", None),
        profile("typed", None),
    ]);
    store
        .set_folder_defaults(
            "Prod",
            FolderDefaults {
                gateway: Some(ProfileId::new("edge")),
                username: Some("ops".to_owned()),
                key_path: Some(PathBuf::from("/keys/prod")),
                port: Some(2200),
                environment: Some(Environment::Production),
            },
        )
        .expect("folder");
    store
        .set_folder_defaults(
            "Prod/Web",
            FolderDefaults {
                username: Some("www".to_owned()),
                port: Some(2300),
                ..FolderDefaults::default()
            },
        )
        .expect("folder");
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

fn draft(app: &App) -> &heimdall_app::profile_draft::ProfileDraft {
    match &app.dialog {
        Some(Dialog::EditProfile { draft, .. }) => draft,
        other => panic!("not the form: {other:?}"),
    }
}

fn new_in(app: &mut App, folder: &str, protocol: DraftProtocol) {
    app.update(Message::Folder(FolderMessage::NewProfileIn(
        folder.to_owned(),
    )));
    app.update(Message::ChooseProtocol(protocol));
}

fn type_in(app: &mut App, field: ProfileField, value: &str) {
    app.update(Message::ProfileField {
        field,
        value: value.to_owned(),
    });
}

#[test]
fn a_session_made_in_a_folder_starts_with_its_defaults_nearest_folder_first() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    new_in(&mut app, "Prod/Web", DraftProtocol::Ssh);
    let form = draft(&app);
    assert_eq!(
        form.group, "Prod/Web",
        "the folder survives the protocol pick"
    );
    assert_eq!(form.username, "www");
    assert_eq!(form.port, "2300");
    assert_eq!(form.key_path, "/keys/prod");
    assert_eq!(form.gateway, Some(ProfileId::new("edge")));
    assert_eq!(form.environment, Some(Environment::Production));
    assert_eq!(
        form.inherited_from(InheritedField::Username),
        Some("Prod/Web")
    );
    assert_eq!(form.inherited_from(InheritedField::KeyPath), Some("Prod"));
    assert_eq!(form.inherited_from(InheritedField::Port), Some("Prod/Web"));
    assert_eq!(form.inherited_from(InheritedField::Gateway), Some("Prod"));
}

#[test]
fn what_the_user_sets_is_never_replaced_and_loses_its_hint() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    new_in(&mut app, "Prod", DraftProtocol::Ssh);
    type_in(&mut app, ProfileField::Username, "me");
    type_in(&mut app, ProfileField::Port, "22");
    assert_eq!(draft(&app).inherited_from(InheritedField::Username), None);
    assert_eq!(draft(&app).inherited_from(InheritedField::Port), None);

    // Another folder typed: what came from the first follows, what was typed stays.
    type_in(&mut app, ProfileField::Group, "Prod/Web");
    let form = draft(&app);
    assert_eq!(form.username, "me");
    assert_eq!(
        form.port, "22",
        "a port typed, even the default one, is a choice"
    );
    assert_eq!(form.key_path, "/keys/prod");
    type_in(&mut app, ProfileField::Group, "Elsewhere");
    let form = draft(&app);
    assert_eq!(form.username, "me");
    assert!(form.key_path.is_empty(), "the key came from a folder left");
    assert_eq!(form.gateway, None);
    assert_eq!(form.environment, None);
    assert!(form.from_folder.is_empty());
}

#[test]
fn the_form_of_a_saved_session_and_a_folder_without_defaults_are_left_alone() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    app.update(Message::EditProfile(ProfileId::new("web")));
    type_in(&mut app, ProfileField::Group, "Prod");
    assert!(draft(&app).username.is_empty());
    assert!(draft(&app).from_folder.is_empty());

    app.update(Message::DismissDialog);
    new_in(&mut app, "Nowhere", DraftProtocol::Ssh);
    assert!(draft(&app).username.is_empty());
    assert_eq!(draft(&app).port, "22");
}

#[test]
fn only_the_fields_of_the_protocol_are_filled() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    new_in(&mut app, "Prod", DraftProtocol::Rdp);
    let form = draft(&app);
    assert!(form.username.is_empty(), "the account is SSH's");
    assert_eq!(form.port, "3389");
    assert_eq!(form.gateway, Some(ProfileId::new("edge")));
    assert_eq!(form.environment, Some(Environment::Production));
}

#[test]
fn a_session_moved_into_a_folder_is_filled_and_one_moved_out_is_not() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    app.update(Message::ProfileMenu(ProfileMenuMessage::Move {
        id: ProfileId::new("loose"),
        to: Some("Prod/Web".to_owned()),
    }));
    app.update(Message::EditProfile(ProfileId::new("loose")));
    let form = draft(&app);
    assert_eq!(form.username, "www");
    assert_eq!(form.key_path, "/keys/prod");
    assert_eq!(form.gateway, Some(ProfileId::new("edge")));
    assert_eq!(form.environment, Some(Environment::Production));
    assert_eq!(form.port, "22", "a saved port is never replaced by a move");
    app.update(Message::DismissDialog);

    app.update(Message::ProfileMenu(ProfileMenuMessage::Move {
        id: ProfileId::new("loose"),
        to: None,
    }));
    app.update(Message::EditProfile(ProfileId::new("loose")));
    assert_eq!(draft(&app).username, "www", "moving out takes nothing back");
}

#[test]
fn the_question_before_deleting_a_gateway_lists_the_folders_that_name_it() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    app.update(Message::Gateways(GatewaysMessage::AskDelete(
        ProfileId::new("edge"),
    )));
    assert!(
        matches!(
            &app.dialog,
            Some(Dialog::ConfirmDeleteGateway { folders, .. }) if folders == &["Prod".to_owned()]
        ),
        "{:?}",
        app.dialog
    );
    app.update(Message::ConfirmDialog);
    new_in(&mut app, "Prod", DraftProtocol::Ssh);
    assert_eq!(draft(&app).gateway, None, "the folder no longer names it");
    assert_eq!(draft(&app).username, "ops");
}
