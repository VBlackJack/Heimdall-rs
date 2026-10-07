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
        forwards: heimdall_core::profile::Forwards::default(),
        post_connect: heimdall_core::post_connect::PostConnect::default(),
        forward_agent: false,
        compression: false,
        sftp: false,
        legacy_algorithms: false,
        session_logging: None,
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
                ..
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
fn a_move_over_a_profile_file_changed_outside_is_refused_and_said_so() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    // Another instance on the same file records a profile of its own.
    let path = dir.path().join("profiles.toml");
    let mut other = ProfileStore::open(&path).expect("opens");
    other
        .apply(|store| store.merge([profile("other", "other", None)]))
        .expect("saves");
    let written = std::fs::read_to_string(&path).expect("readable");

    app.update(Message::ProfileMenu(ProfileMenuMessage::Move {
        id: ProfileId::new("db"),
        to: Some("dev".to_owned()),
    }));

    assert!(
        matches!(app.dialog, Some(Dialog::StoreChanged { .. })),
        "{:?}",
        app.dialog
    );
    assert_eq!(group_of(&app, "db").as_deref(), Some("Prod"));
    assert_eq!(std::fs::read_to_string(&path).expect("readable"), written);
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

#[test]
fn a_folder_counts_the_profiles_it_holds_its_sub_folders_included() {
    let dir = tempfile::tempdir().expect("dir");
    let app = app(dir.path());
    let counts = |rows: Vec<TreeRow>| -> Vec<(String, usize)> {
        rows.into_iter()
            .filter_map(|row| match row {
                TreeRow::Folder { path, count, .. } => Some((path, count)),
                TreeRow::Profile { .. } => None,
            })
            .collect()
    };
    assert_eq!(
        counts(app.tree_rows("")),
        [
            ("dev".to_owned(), 1),
            ("Prod".to_owned(), 4),
            ("Prod/Web".to_owned(), 2),
            (NO_FOLDER.to_owned(), 2),
        ]
    );
    assert_eq!(
        counts(app.tree_rows("web")),
        [("Prod".to_owned(), 2), ("Prod/Web".to_owned(), 2)],
        "while searching, only what was found (api by its folder)"
    );
}

#[test]
fn a_folder_is_given_a_colour_from_its_menu_and_its_folders_show_it() {
    use heimdall_app::FolderMessage;
    use heimdall_core::folder::FolderColor;

    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (parent, child) = ("Prod".to_owned(), "Prod/Web".to_owned());
    app.update(Message::Folder(FolderMessage::Color {
        path: parent.clone(),
        color: Some(FolderColor::Amber),
    }));
    assert_eq!(app.own_folder_color(&parent), Some(FolderColor::Amber));
    assert_eq!(
        app.folder_color(&child),
        Some(FolderColor::Amber),
        "inherited"
    );
    assert_eq!(app.own_folder_color(&child), None);
    app.update(Message::Folder(FolderMessage::Color {
        path: parent.clone(),
        color: None,
    }));
    assert_eq!(app.folder_color(&child), None);
}

#[test]
fn sessions_and_folders_dropped_move_and_ctrl_z_puts_them_back() {
    use heimdall_app::{DropTarget, Notice};

    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let group = |app: &App, id: &str| {
        app.profile_summary(&ProfileId::new(id))
            .and_then(|profile| profile.group)
    };
    // Dropped on a folder: in it.
    app.update(Message::DropProfiles {
        ids: vec![ProfileId::new("web"), ProfileId::new("loose")],
        onto: DropTarget::Folder("dev".to_owned()),
    });
    assert_eq!(group(&app, "web").as_deref(), Some("dev"));
    assert_eq!(group(&app, "loose").as_deref(), Some("dev"));
    assert_eq!(
        app.notice(),
        Some(&Notice::DroppedProfiles {
            count: 2,
            folder: Some("dev".to_owned())
        })
    );
    // Ctrl+Z: each back where it was.
    app.update(Message::UndoMove);
    assert_eq!(group(&app, "web").as_deref(), Some("Prod/Web"));
    assert_eq!(group(&app, "loose"), None);
    assert_eq!(app.notice(), Some(&Notice::MoveUndone));
    app.update(Message::UndoMove);
    assert_eq!(app.notice(), Some(&Notice::NothingToUndo), "once");

    // Dropped on a session: into its folder; on "(No Folder)": out of any.
    app.update(Message::DropProfiles {
        ids: vec![ProfileId::new("loose")],
        onto: DropTarget::Profile(ProfileId::new("db")),
    });
    assert_eq!(group(&app, "loose").as_deref(), Some("Prod"));
    app.update(Message::DropProfiles {
        ids: vec![ProfileId::new("loose")],
        onto: DropTarget::Folder(heimdall_app::NO_FOLDER.to_owned()),
    });
    assert_eq!(group(&app, "loose"), None);

    // A folder dropped on another: moved into it, with all it holds; undone, back.
    app.update(Message::DropFolder {
        path: "Prod/Web".to_owned(),
        onto: DropTarget::Folder("dev".to_owned()),
    });
    assert_eq!(group(&app, "web").as_deref(), Some("dev/Web"));
    app.update(Message::UndoMove);
    assert_eq!(group(&app, "web").as_deref(), Some("Prod/Web"));
    // Into itself, or where it is: nothing moves.
    app.update(Message::DropFolder {
        path: "Prod".to_owned(),
        onto: DropTarget::Folder("Prod/Web".to_owned()),
    });
    assert_eq!(group(&app, "db").as_deref(), Some("Prod"));
}

#[test]
fn every_folder_folds_and_unfolds_at_once_and_the_tree_comes_back_as_it_was_left() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    app.update(Message::FoldAll(true));
    assert_eq!(
        outline(&app.tree_rows("")),
        ["-dev@0", "-Prod@0", "-(none)@0"],
        "Collapse all, as the C# menu's"
    );
    app.update(Message::FoldAll(false));
    assert!(
        outline(&app.tree_rows(""))
            .iter()
            .all(|row| !row.starts_with('-')),
        "Expand all"
    );

    // Left with Web folded and api selected, the tree opens again so.
    app.update(Message::ToggleFolder("Prod/Web".to_owned()));
    app.update(Message::SelectProfile(ProfileId::new("api")));
    let folded = app.folded_folders();
    assert_eq!(folded, ["Prod/Web"]);
    let mut again = self::app(dir.path());
    again.restore_tree(&folded, Some(ProfileId::new("api")));
    assert!(outline(&again.tree_rows("")).contains(&"-Web@1".to_owned()));
    assert_eq!(again.selected_profile, Some(ProfileId::new("api")));
    let mut gone = self::app(dir.path());
    gone.restore_tree(&[], Some(ProfileId::new("deleted since")));
    assert_eq!(
        gone.selected_profile, None,
        "a session gone is not selected"
    );
}

/// The sessions of folder `path`, in the tree's order.
fn in_folder(app: &App, path: &str) -> Vec<String> {
    app.tree_rows("")
        .into_iter()
        .filter_map(|row| match row {
            TreeRow::Profile { profile, .. }
                if profile
                    .group
                    .as_deref()
                    .map(heimdall_core::folder::normal)
                    .as_deref()
                    == Some(path) =>
            {
                Some(profile.id.to_string())
            }
            _ => None,
        })
        .collect()
}

fn rank(app: &App, id: &str) -> Option<i32> {
    app.profile_summary(&ProfileId::new(id))
        .and_then(|profile| profile.metadata.sort_order)
}

#[test]
fn sessions_dropped_before_or_after_another_take_that_place_and_ctrl_z_undoes_it() {
    use heimdall_app::{DropTarget, Notice, OrganizationChange};

    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    assert_eq!(in_folder(&app, "Prod/Web"), ["api", "web"], "by name");
    assert_eq!(app.undo_offer(), None);

    app.update(Message::DropProfiles {
        ids: vec![ProfileId::new("web")],
        onto: DropTarget::Before(ProfileId::new("api")),
    });
    assert_eq!(in_folder(&app, "Prod/Web"), ["web", "api"]);
    assert_eq!(
        (rank(&app, "web"), rank(&app, "api")),
        (Some(10), Some(20)),
        "as the C# tens"
    );
    assert_eq!(
        app.notice(),
        Some(&Notice::Reordered {
            count: 1,
            name: Some("web".to_owned()),
            folder: Some("Prod/Web".to_owned()),
        })
    );
    assert_eq!(app.undo_offer(), Some(OrganizationChange::Reorder));
    // Dropped on itself: nothing moves, nothing is offered anew.
    app.update(Message::DropProfiles {
        ids: vec![ProfileId::new("web")],
        onto: DropTarget::After(ProfileId::new("web")),
    });
    assert_eq!(in_folder(&app, "Prod/Web"), ["web", "api"]);

    app.update(Message::UndoMove);
    assert_eq!(in_folder(&app, "Prod/Web"), ["api", "web"]);
    assert_eq!((rank(&app, "web"), rank(&app, "api")), (None, None));
    assert_eq!(app.notice(), Some(&Notice::MoveUndone));

    // From another folder, two at once: moved in, placed after, in the tree's order.
    app.update(Message::DropProfiles {
        ids: vec![ProfileId::new("loose"), ProfileId::new("db")],
        onto: DropTarget::After(ProfileId::new("api")),
    });
    assert_eq!(in_folder(&app, "Prod/Web"), ["api", "db", "loose", "web"]);
    assert_eq!(group_of(&app, "loose").as_deref(), Some("Prod/Web"));
    app.update(Message::UndoMove);
    assert_eq!(group_of(&app, "loose"), None);
    assert_eq!(group_of(&app, "db").as_deref(), Some("Prod"));
}

#[test]
fn alt_up_and_alt_down_move_a_session_within_its_folder_never_out_of_it() {
    use heimdall_app::OrganizationChange;

    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let nudge = |app: &mut App, id: &str, down: bool| {
        app.update(Message::NudgeProfile {
            id: ProfileId::new(id),
            down,
        });
    };
    nudge(&mut app, "api", false);
    assert_eq!(in_folder(&app, "Prod/Web"), ["api", "web"], "first already");
    assert_eq!(app.undo_offer(), None, "nothing changed");
    nudge(&mut app, "api", true);
    assert_eq!(in_folder(&app, "Prod/Web"), ["web", "api"]);
    assert_eq!(app.undo_offer(), Some(OrganizationChange::Reorder));
    nudge(&mut app, "api", true);
    assert_eq!(in_folder(&app, "Prod/Web"), ["web", "api"], "last already");
    nudge(&mut app, "api", false);
    assert_eq!(in_folder(&app, "Prod/Web"), ["api", "web"]);
}

#[test]
fn a_move_from_the_menu_or_a_rename_is_undone_but_not_once_changed_since() {
    use heimdall_app::{DropTarget, Notice, OrganizationChange};

    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    app.update(Message::ProfileMenu(ProfileMenuMessage::Move {
        id: ProfileId::new("db"),
        to: Some("dev".to_owned()),
    }));
    assert_eq!(app.undo_offer(), Some(OrganizationChange::Move));
    app.update(Message::UndoMove);
    assert_eq!(group_of(&app, "db").as_deref(), Some("Prod"));

    app.update(Message::ProfileMenu(ProfileMenuMessage::Rename(
        ProfileId::new("db"),
    )));
    app.update(Message::ProfileMenu(ProfileMenuMessage::NameEdited(
        "Database".to_owned(),
    )));
    app.update(Message::ConfirmDialog);
    assert_eq!(app.undo_offer(), Some(OrganizationChange::Rename));
    app.update(Message::UndoMove);
    assert_eq!(
        app.profile_summary(&ProfileId::new("db")).expect("db").name,
        "db"
    );

    // Moved, then its folder deleted, which keeps no undo: the move is not undone over it.
    app.update(Message::DropProfiles {
        ids: vec![ProfileId::new("loose")],
        onto: DropTarget::Folder("dev".to_owned()),
    });
    folder(&mut app, FolderMessage::RequestDelete("dev".to_owned()));
    app.update(Message::ConfirmDialog);
    app.update(Message::UndoMove);
    assert_eq!(app.notice(), Some(&Notice::UndoConflict));
    assert_eq!(group_of(&app, "loose"), None, "left as it is");
}

#[test]
fn a_renamed_folder_is_named_back_unless_its_old_name_was_taken_since() {
    use heimdall_app::{Notice, OrganizationChange};

    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    folder(&mut app, FolderMessage::Rename("Prod".to_owned()));
    named(&mut app, "Live");
    assert_eq!(app.undo_offer(), Some(OrganizationChange::FolderRename));
    app.update(Message::UndoMove);
    assert_eq!(app.notice(), Some(&Notice::MoveUndone));
    assert_eq!(group_of(&app, "db").as_deref(), Some("Prod"));
    assert_eq!(group_of(&app, "api").as_deref(), Some("Prod/Web"));

    // Its old name taken by a new folder since: left as it is.
    folder(&mut app, FolderMessage::Rename("dev".to_owned()));
    named(&mut app, "Dev2");
    folder(
        &mut app,
        FolderMessage::New {
            parent: String::new(),
        },
    );
    named(&mut app, "dev");
    app.update(Message::UndoMove);
    assert_eq!(app.notice(), Some(&Notice::UndoConflict));
    assert_eq!(group_of(&app, "dev").as_deref(), Some("Dev2"));
}
