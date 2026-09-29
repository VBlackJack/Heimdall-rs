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

//! The profile tree as the C# Heimdall draws it: nested folders, opened and closed, sub-
//! folders before profiles, "(No Folder)" last; a search opens what it finds.

use std::path::Path;

use heimdall_app::{
    App, AppConfig, Dialog, Effect, FolderMessage, FolderNaming, Message, NO_FOLDER,
    ProfileMenuMessage, TreeRow,
};
use heimdall_core::folder::FolderError;
use heimdall_core::profile::{LocalArguments, LocalCommand, LocalProfile, ProfileId, SshProfile};
use heimdall_core::store::ProfileStore;
use heimdall_ssh::AgentSource;
use heimdall_term::GridSize;

fn profile(id: &str, name: &str, group: Option<&str>) -> SshProfile {
    SshProfile {
        id: ProfileId::new(id),
        name: name.to_owned(),
        group: group.map(str::to_owned),
        host: format!("{id}.lab"),
        port: 22,
        username: None,
        key_path: None,
        gateway: None,
        vault_entry: None,
    }
}

fn app(dir: &Path) -> App {
    let profiles_file = dir.join("profiles.toml");
    let mut store = ProfileStore::open(&profiles_file).expect("store");
    store.merge([
        profile("web", "web", Some(" Prod / Web ")),
        profile("api", "Api", Some("Prod/Web")),
        profile("db", "db", Some("Prod")),
        profile("dev", "dev", Some("dev")),
        profile("loose", "loose", None),
        profile("empty", "empty", Some("")),
    ]);
    // A program of this computer: counted in its folder, never opened by "Connect all".
    store.merge_local([LocalProfile {
        id: ProfileId::new("tool"),
        name: "tool".to_owned(),
        group: Some("Prod".to_owned()),
        command: LocalCommand {
            program: Some("tool".to_owned()),
            arguments: LocalArguments::List(Vec::new()),
            working_directory: None,
        },
        approved: None,
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

/// The rows, written short: `+name@depth` for an open folder, `-name@depth` a closed one,
/// `name@depth` a profile.
fn outline(rows: &[TreeRow]) -> Vec<String> {
    rows.iter()
        .map(|row| match row {
            TreeRow::Folder {
                path,
                name,
                depth,
                open,
            } => {
                let name = if path == NO_FOLDER { "(none)" } else { name };
                format!("{}{name}@{depth}", if *open { '+' } else { '-' })
            }
            TreeRow::Profile { profile, depth } => format!("{}@{depth}", profile.id),
        })
        .collect()
}

#[test]
fn folders_nest_by_their_path_sub_folders_first_and_no_folder_last() {
    let dir = tempfile::tempdir().expect("dir");
    let app = app(dir.path());
    assert_eq!(
        outline(&app.tree_rows("")),
        [
            "+dev@0",
            "dev@1", // By name, whatever the case: "dev" before "Prod".
            "+Prod@0",
            "+Web@1",
            "api@2",
            "web@2",
            "db@1",
            "tool@1",
            "+(none)@0",
            "empty@1",
            "loose@1",
        ]
    );
}

#[test]
fn a_folder_closes_and_opens_again_and_keeps_its_sub_folders_state() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    app.update(Message::ToggleFolder("Prod/Web".to_owned()));
    assert_eq!(
        outline(&app.tree_rows("")),
        [
            "+dev@0",
            "dev@1",
            "+Prod@0",
            "-Web@1",
            "db@1",
            "tool@1",
            "+(none)@0",
            "empty@1",
            "loose@1",
        ]
    );
    app.update(Message::ToggleFolder(" Prod ".to_owned()));
    assert_eq!(
        outline(&app.tree_rows("")),
        [
            "+dev@0",
            "dev@1",
            "-Prod@0",
            "+(none)@0",
            "empty@1",
            "loose@1"
        ],
        "a path is read whatever its spaces"
    );
    app.update(Message::ToggleFolder("Prod".to_owned()));
    assert!(
        outline(&app.tree_rows("")).contains(&"-Web@1".to_owned()),
        "Web stays closed"
    );
    app.update(Message::ToggleFolder(NO_FOLDER.to_owned()));
    assert_eq!(
        outline(&app.tree_rows("")).last(),
        Some(&"-(none)@0".to_owned())
    );
}

#[test]
fn a_search_shows_what_it_finds_every_folder_on_the_way_open() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    app.update(Message::ToggleFolder("Prod".to_owned()));
    app.update(Message::ToggleFolder("Prod/Web".to_owned()));
    assert_eq!(
        outline(&app.tree_rows("api")),
        ["+Prod@0", "+Web@1", "api@2"]
    );
    assert_eq!(outline(&app.tree_rows("loose")), ["+(none)@0", "loose@1"]);
    assert!(app.tree_rows("nothing like it").is_empty());
    // Emptied, the folders are as they were.
    assert_eq!(
        outline(&app.tree_rows(" ")).get(2),
        Some(&"-Prod@0".to_owned())
    );
}

fn folder(app: &mut App, message: FolderMessage) -> Vec<Effect> {
    app.update(Message::Folder(message))
}

fn named(app: &mut App, value: &str) {
    folder(app, FolderMessage::NameEdited(value.to_owned()));
    app.update(Message::ConfirmDialog);
}

fn group_of(app: &App, id: &str) -> Option<String> {
    app.profile_summaries()
        .into_iter()
        .find(|profile| profile.id.as_str() == id)
        .and_then(|profile| profile.group)
}

#[test]
fn a_new_folder_is_shown_empty_and_its_name_is_checked() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    folder(
        &mut app,
        FolderMessage::New {
            parent: "Prod".to_owned(),
        },
    );
    assert!(matches!(
        &app.dialog,
        Some(Dialog::FolderName { naming: FolderNaming::New(parent), value, error: None })
            if parent == "Prod" && value.is_empty()
    ));
    named(&mut app, " Archive ");
    assert_eq!(app.dialog, None);
    assert!(outline(&app.tree_rows("")).contains(&"+Archive@1".to_owned()));
    assert!(
        !outline(&app.tree_rows("web"))
            .iter()
            .any(|row| row.contains("Archive")),
        "a search shows no empty folder"
    );

    folder(
        &mut app,
        FolderMessage::New {
            parent: "Prod".to_owned(),
        },
    );
    named(&mut app, "archive");
    assert!(matches!(
        &app.dialog,
        Some(Dialog::FolderName { error: Some(FolderError::Collision), value, .. }) if value == "archive"
    ));
    folder(&mut app, FolderMessage::NameEdited("a/b".to_owned()));
    assert!(
        matches!(&app.dialog, Some(Dialog::FolderName { error: None, .. })),
        "typing clears it"
    );
    app.update(Message::ConfirmDialog);
    assert!(matches!(
        &app.dialog,
        Some(Dialog::FolderName {
            error: Some(FolderError::InvalidName),
            ..
        })
    ));
}

#[test]
fn a_renamed_folder_keeps_its_profiles_and_its_folds() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    app.update(Message::ToggleFolder("Prod/Web".to_owned()));
    folder(&mut app, FolderMessage::Rename("Prod".to_owned()));
    assert!(matches!(&app.dialog, Some(Dialog::FolderName { value, .. }) if value == "Prod"));
    named(&mut app, "Live");
    assert_eq!(group_of(&app, "db").as_deref(), Some("Live"));
    assert_eq!(group_of(&app, "api").as_deref(), Some("Live/Web"));
    assert!(
        outline(&app.tree_rows("")).contains(&"-Web@1".to_owned()),
        "the fold followed"
    );
}

#[test]
fn a_folder_moves_where_the_menu_offers() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    assert_eq!(
        app.folder_targets("Prod/Web"),
        ["", "dev"],
        "the top level, then neither its parent nor itself"
    );
    assert_eq!(app.folder_targets("Prod"), ["dev"], "already at the top");
    folder(
        &mut app,
        FolderMessage::Move {
            path: "Prod/Web".to_owned(),
            to: "dev".to_owned(),
        },
    );
    assert_eq!(group_of(&app, "api").as_deref(), Some("dev/Web"));
    folder(
        &mut app,
        FolderMessage::Move {
            path: "dev/Web".to_owned(),
            to: String::new(),
        },
    );
    assert_eq!(group_of(&app, "api").as_deref(), Some("Web"));
}

#[test]
fn a_deleted_folder_asks_first_and_sends_its_profiles_to_no_folder() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    folder(&mut app, FolderMessage::RequestDelete("Prod".to_owned()));
    assert_eq!(
        app.dialog,
        Some(Dialog::ConfirmDeleteFolder {
            path: "Prod".to_owned(),
            name: "Prod".to_owned(),
            count: 4,
        })
    );
    app.update(Message::DismissDialog);
    assert_eq!(group_of(&app, "db").as_deref(), Some("Prod"), "declined");
    folder(&mut app, FolderMessage::RequestDelete("Prod".to_owned()));
    app.update(Message::ConfirmDialog);
    assert_eq!(group_of(&app, "db"), None);
    assert_eq!(group_of(&app, "web"), None);
    assert_eq!(group_of(&app, "dev").as_deref(), Some("dev"));
}

#[test]
fn connect_all_opens_every_session_a_folder_holds_once_asked() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    assert_eq!(app.folder_connectable("Prod"), 3);
    assert_eq!(app.folder_connectable(NO_FOLDER), 2);
    assert_eq!(app.folder_connectable("Nowhere"), 0);
    folder(
        &mut app,
        FolderMessage::RequestConnectAll("Nowhere".to_owned()),
    );
    assert_eq!(app.dialog, None, "nothing to connect: not asked");
    folder(
        &mut app,
        FolderMessage::RequestConnectAll("Prod".to_owned()),
    );
    assert_eq!(
        app.dialog,
        Some(Dialog::ConfirmConnectFolder {
            path: "Prod".to_owned(),
            count: 3,
        })
    );
    let effects = app.update(Message::ConfirmDialog);
    let hosts: Vec<String> = effects
        .iter()
        .map(|effect| match effect {
            Effect::Connect { request, .. } => request.profile.host.clone(),
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(hosts.len(), 3);
    assert!(hosts.contains(&"db.lab".to_owned()) && hosts.contains(&"api.lab".to_owned()));
    assert_eq!(app.tabs.len(), 3);
}

#[test]
fn a_session_added_in_a_folder_has_it_written_in() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    folder(&mut app, FolderMessage::NewProfileIn("Prod/Web".to_owned()));
    assert!(
        matches!(&app.dialog, Some(Dialog::EditProfile { draft, .. }) if draft.group == "Prod/Web")
    );
    app.update(Message::DismissDialog);
    folder(&mut app, FolderMessage::NewProfileIn(NO_FOLDER.to_owned()));
    assert!(
        matches!(&app.dialog, Some(Dialog::EditProfile { draft, .. }) if draft.group.is_empty())
    );
}

#[test]
fn a_profile_moves_to_a_folder_or_to_none_as_the_menu_lists_them() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let targets = app.profile_move_targets(&ProfileId::new("db"));
    assert_eq!(targets.first(), Some(&(None, true)), "(No Folder) first");
    assert!(
        targets.contains(&(Some("Prod".to_owned()), false)),
        "its own greyed"
    );
    assert!(targets.contains(&(Some("Prod/Web".to_owned()), true)));
    assert_eq!(
        app.profile_move_targets(&ProfileId::new("loose")).first(),
        Some(&(None, false))
    );
    let moved = |to: Option<&str>| {
        Message::ProfileMenu(ProfileMenuMessage::Move {
            id: ProfileId::new("db"),
            to: to.map(str::to_owned),
        })
    };
    app.update(moved(Some(" dev ")));
    assert_eq!(group_of(&app, "db").as_deref(), Some("dev"));
    app.update(moved(None));
    assert_eq!(group_of(&app, "db"), None);
}

#[test]
fn a_profile_is_renamed_and_an_empty_name_leaves_it() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let name = |app: &App| app.profile_summary(&ProfileId::new("db")).expect("db").name;
    app.update(Message::ProfileMenu(ProfileMenuMessage::Rename(
        ProfileId::new("db"),
    )));
    assert!(matches!(&app.dialog, Some(Dialog::RenameProfile { value, .. }) if value == "db"));
    app.update(Message::ProfileMenu(ProfileMenuMessage::NameEdited(
        "  Database  ".to_owned(),
    )));
    app.update(Message::ConfirmDialog);
    assert_eq!(name(&app), "Database");
    app.update(Message::ProfileMenu(ProfileMenuMessage::Rename(
        ProfileId::new("db"),
    )));
    app.update(Message::ProfileMenu(ProfileMenuMessage::NameEdited(
        " ".to_owned(),
    )));
    app.update(Message::ConfirmDialog);
    assert_eq!(name(&app), "Database", "unchanged");
}
