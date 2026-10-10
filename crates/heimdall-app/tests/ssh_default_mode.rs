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

//! The default SSH mode, as the C# `SshDefaultMode`: a new profile's form starts with it,
//! and "Apply to all saved sessions" writes it into every SSH profile once agreed to, in one
//! save that changes nothing when it fails.

use std::fs;
use std::path::Path;

use heimdall_app::profile_draft::{DraftProtocol, ProfileField};
use heimdall_app::{
    App, AppConfig, Dialog, FolderMessage, Message, SettingsMessage, SystemCredentials,
};
use heimdall_core::profile::{Forwards, ProfileId, SshMode, SshProfile};
use heimdall_core::settings::{SETTINGS_FILE_NAME, Settings};
use heimdall_core::store::ProfileStore;
use heimdall_ssh::AgentSource;
use heimdall_term::GridSize;

fn profile(id: &str, mode: SshMode, sftp: bool) -> SshProfile {
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
        forwards: Forwards::default(),
        post_connect: heimdall_core::post_connect::PostConnect::default(),
        forward_agent: false,
        compression: false,
        sftp,
        legacy_algorithms: false,
        session_logging: None,
        ssh_mode: mode,
        x11_forwarding: false,
    }
}

/// Three SSH shells, one already in `PuTTY`, and an SFTP profile.
fn saved() -> [SshProfile; 4] {
    [
        profile("web", SshMode::Embedded, false),
        profile("db", SshMode::External, false),
        profile("app", SshMode::Embedded, false),
        profile("files", SshMode::Embedded, true),
    ]
}

fn app(dir: &Path) -> App {
    let profiles_file = dir.join("profiles.toml");
    let mut store = ProfileStore::open(&profiles_file).expect("store");
    store.merge(saved());
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

/// Each profile's id and mode, in file order.
fn modes(profiles: &[SshProfile]) -> Vec<(String, SshMode)> {
    profiles
        .iter()
        .map(|profile| (profile.id.to_string(), profile.ssh_mode))
        .collect()
}

/// The modes the profile file holds.
fn modes_on_disk(dir: &Path) -> Vec<(String, SshMode)> {
    let store = ProfileStore::open(dir.join("profiles.toml")).expect("store");
    modes(store.ssh_profiles())
}

fn choose(app: &mut App, mode: SshMode) {
    app.update(Message::Settings(SettingsMessage::SshDefaultMode(mode)));
}

/// The SSH mode of the form open.
fn form_mode(app: &App) -> SshMode {
    match &app.dialog {
        Some(Dialog::EditProfile { draft, .. }) => draft.ssh_mode,
        other => panic!("{other:?}"),
    }
}

#[test]
fn the_default_is_embedded_saved_once_chosen_and_new_forms_start_with_it() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    assert_eq!(app.settings().ssh_default_mode, SshMode::Embedded);
    choose(&mut app, SshMode::External);
    let read = Settings::load(&dir.path().join(SETTINGS_FILE_NAME)).expect("load");
    assert_eq!(read.ssh_default_mode, SshMode::External, "saved at once");

    app.update(Message::NewProfile);
    assert_eq!(form_mode(&app), SshMode::External);
    app.update(Message::ChooseProtocol(DraftProtocol::Ssh));
    assert_eq!(form_mode(&app), SshMode::External, "the protocol chosen");
    app.update(Message::Folder(FolderMessage::NewProfileIn(String::new())));
    assert_eq!(form_mode(&app), SshMode::External, "from a folder's menu");

    // Saved with it.
    app.update(Message::ChooseProtocol(DraftProtocol::Ssh));
    for (field, value) in [(ProfileField::Name, "new"), (ProfileField::Host, "new.lab")] {
        app.update(Message::ProfileField {
            field,
            value: value.to_owned(),
        });
    }
    app.update(Message::SaveProfile {
        password: None,
        passphrase: None,
    });
    let new = app
        .profiles()
        .iter()
        .find(|profile| profile.name == "new")
        .expect("saved");
    assert_eq!(new.ssh_mode, SshMode::External);

    // A saved profile keeps its own.
    app.update(Message::EditProfile(ProfileId::new("web")));
    assert_eq!(form_mode(&app), SshMode::Embedded);
}

#[test]
fn apply_to_all_asks_with_the_count_then_rewrites_every_ssh_shell_in_one_save() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    choose(&mut app, SshMode::External);

    app.update(Message::Settings(SettingsMessage::ApplySshModeToAll));
    assert_eq!(
        app.dialog,
        Some(Dialog::ConfirmApplySshMode {
            mode: SshMode::External,
            changes: 2,
            total: 3,
        }),
        "the shells that change, of the shells there are; never the SFTP profile"
    );
    app.update(Message::DismissDialog);
    assert_eq!(modes(app.profiles()), modes(&saved()), "nothing changed");

    app.update(Message::Settings(SettingsMessage::ApplySshModeToAll));
    app.update(Message::ConfirmDialog);
    let applied = vec![
        ("web".to_owned(), SshMode::External),
        ("db".to_owned(), SshMode::External),
        ("app".to_owned(), SshMode::External),
        ("files".to_owned(), SshMode::Embedded),
    ];
    assert_eq!(modes(app.profiles()), applied);
    assert_eq!(modes_on_disk(dir.path()), applied, "saved");
    assert_eq!(app.dialog, None);
    assert_eq!(app.settings().ssh_default_mode, SshMode::External);

    // Nothing left to change: nothing is asked, as the C# stops there.
    app.update(Message::Settings(SettingsMessage::ApplySshModeToAll));
    assert_eq!(app.dialog, None);

    // Back to Embedded: the SFTP profile was already, and is not counted.
    choose(&mut app, SshMode::Embedded);
    app.update(Message::Settings(SettingsMessage::ApplySshModeToAll));
    assert!(matches!(
        app.dialog,
        Some(Dialog::ConfirmApplySshMode {
            mode: SshMode::Embedded,
            changes: 3,
            total: 3,
        })
    ));
}

#[test]
fn a_save_that_fails_changes_no_profile_and_says_why() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    choose(&mut app, SshMode::External);
    app.update(Message::Settings(SettingsMessage::ApplySshModeToAll));
    // The file changed by something else meanwhile: it is not written over.
    let file = dir.path().join("profiles.toml");
    let outside = format!("{}\n# edited\n", fs::read_to_string(&file).expect("read"));
    fs::write(&file, &outside).expect("write");

    app.update(Message::ConfirmDialog);
    assert!(
        matches!(app.dialog, Some(Dialog::StoreChanged { .. })),
        "{:?}",
        app.dialog
    );
    assert_eq!(modes(app.profiles()), modes(&saved()), "not one of them");
    assert_eq!(fs::read_to_string(&file).expect("read"), outside);
}
