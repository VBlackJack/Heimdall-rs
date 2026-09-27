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

//! Profiles created, edited and deleted in the window, and what reaches the file.

use std::path::{Path, PathBuf};

use heimdall_app::profile_draft::{DraftError, ProfileField};
use heimdall_app::{App, AppConfig, Dialog, Message};
use heimdall_core::profile::{ProfileId, SshProfile};
use heimdall_core::store::ProfileStore;
use heimdall_ssh::AgentSource;
use heimdall_term::GridSize;

fn config(profiles_file: PathBuf, dir: &Path) -> AppConfig {
    AppConfig {
        profiles_file,
        known_hosts: dir.join("known_hosts"),
        legacy_dir: None,
        agent: AgentSource::Disabled,
        initial_grid: GridSize { cols: 80, rows: 24 },
        files_start: dir.to_owned(),
        system_credentials: heimdall_app::SystemCredentials::memory(),
    }
}

fn saved(path: &Path) -> Vec<SshProfile> {
    ProfileStore::open(path)
        .expect("readable")
        .ssh_profiles()
        .to_vec()
}

fn type_in(app: &mut App, field: ProfileField, value: &str) {
    app.update(Message::ProfileField {
        field,
        value: value.to_owned(),
    });
}

fn fill(app: &mut App, name: &str, host: &str, port: &str) {
    type_in(app, ProfileField::Name, name);
    type_in(app, ProfileField::Host, host);
    type_in(app, ProfileField::Port, port);
}

#[test]
fn a_new_profile_is_saved_to_the_file() {
    let dir = tempfile::tempdir().expect("dir");
    let file = dir.path().join("profiles.toml");
    let mut app = App::new(config(file.clone(), dir.path()));
    app.update(Message::NewProfile);
    assert!(
        matches!(&app.dialog, Some(Dialog::EditProfile { draft, error: None })
        if draft.editing.is_none() && draft.name.is_empty())
    );
    fill(&mut app, "web", "web.example.org", "2222");
    type_in(&mut app, ProfileField::Username, "admin");
    app.update(Message::ConfirmDialog);
    assert!(app.dialog.is_none(), "{:?}", app.dialog);
    let on_disk = saved(&file);
    assert_eq!(on_disk.len(), 1);
    assert_eq!(on_disk[0].name, "web");
    assert_eq!(on_disk[0].port, 2222);
    assert_eq!(on_disk[0].username.as_deref(), Some("admin"));
    assert_eq!(app.profiles(), on_disk.as_slice());
}

#[test]
fn an_edit_keeps_the_identifier_and_replaces_the_profile() {
    let dir = tempfile::tempdir().expect("dir");
    let file = dir.path().join("profiles.toml");
    let mut app = App::new(config(file.clone(), dir.path()));
    app.update(Message::NewProfile);
    fill(&mut app, "web", "web.example.org", "");
    app.update(Message::ConfirmDialog);
    let id = app.profiles()[0].id.clone();

    app.update(Message::EditProfile(id.clone()));
    assert!(
        matches!(&app.dialog, Some(Dialog::EditProfile { draft, .. })
        if draft.editing.as_ref() == Some(&id) && draft.host == "web.example.org"
            && draft.port == "22")
    );
    type_in(&mut app, ProfileField::Host, "web2.example.org");
    app.update(Message::ConfirmDialog);
    let on_disk = saved(&file);
    assert_eq!(on_disk.len(), 1, "replaced, not added");
    assert_eq!(on_disk[0].id, id);
    assert_eq!(on_disk[0].host, "web2.example.org");
}

#[test]
fn a_refused_form_stays_open_with_the_reason_and_touches_nothing() {
    let dir = tempfile::tempdir().expect("dir");
    let file = dir.path().join("profiles.toml");
    let mut app = App::new(config(file.clone(), dir.path()));
    app.update(Message::NewProfile);
    fill(&mut app, "web", "root@web", "");
    app.update(Message::ConfirmDialog);
    assert!(
        matches!(&app.dialog, Some(Dialog::EditProfile { draft, error: Some(DraftError::HostHasUser) })
        if draft.host == "root@web"),
        "{:?}",
        app.dialog
    );
    assert!(!file.exists(), "nothing saved");
    assert!(app.profiles().is_empty());
    // Changing a field clears the reason.
    type_in(&mut app, ProfileField::Host, "web");
    assert!(matches!(
        &app.dialog,
        Some(Dialog::EditProfile { error: None, .. })
    ));
}

#[test]
fn a_delete_asks_first_and_a_dismissed_one_keeps_the_profile() {
    let dir = tempfile::tempdir().expect("dir");
    let file = dir.path().join("profiles.toml");
    let mut app = App::new(config(file.clone(), dir.path()));
    app.update(Message::NewProfile);
    fill(&mut app, "web", "web", "");
    app.update(Message::ConfirmDialog);
    let id = app.profiles()[0].id.clone();

    app.update(Message::EditProfile(id.clone()));
    app.update(Message::DeleteProfile);
    assert!(
        matches!(&app.dialog, Some(Dialog::ConfirmDeleteProfile { id: asked, name })
        if *asked == id && name == "web")
    );
    app.update(Message::DismissDialog);
    assert!(app.update(Message::ConfirmDialog).is_empty());
    assert_eq!(saved(&file).len(), 1, "a dismissed delete deletes nothing");

    app.update(Message::EditProfile(id));
    app.update(Message::DeleteProfile);
    app.update(Message::ConfirmDialog);
    assert!(saved(&file).is_empty());
    assert!(app.profiles().is_empty());
}

#[test]
fn a_new_profile_form_has_nothing_to_delete() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = App::new(config(dir.path().join("profiles.toml"), dir.path()));
    app.update(Message::NewProfile);
    app.update(Message::DeleteProfile);
    assert!(matches!(&app.dialog, Some(Dialog::EditProfile { .. })));
}

#[test]
fn a_save_that_fails_leaves_the_list_as_its_file_is() {
    let dir = tempfile::tempdir().expect("dir");
    // The profile file's folder is a file: every save fails.
    let blocker = dir.path().join("blocker");
    std::fs::write(&blocker, b"").expect("blocker");
    let mut app = App::new(config(blocker.join("profiles.toml"), dir.path()));
    app.update(Message::NewProfile);
    fill(&mut app, "web", "web", "");
    app.update(Message::ConfirmDialog);
    assert!(matches!(&app.dialog, Some(Dialog::StoreError { .. })));
    assert!(
        app.profiles().is_empty(),
        "the list does not show a profile its file does not hold"
    );
}

#[test]
fn an_unknown_profile_opens_no_form() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = App::new(config(dir.path().join("profiles.toml"), dir.path()));
    app.update(Message::EditProfile(ProfileId::new("nowhere")));
    assert!(app.dialog.is_none());
}

#[test]
fn an_import_whose_save_fails_leaves_the_list_as_its_file_is() {
    let dir = tempfile::tempdir().expect("dir");
    let legacy = dir.path().join("legacy");
    std::fs::create_dir(&legacy).expect("legacy");
    std::fs::write(
        legacy.join("servers.json"),
        r#"{"servers":[{"id":"x","displayName":"X","remoteServer":"x.lab","connectionType":"SSH"}]}"#,
    )
    .expect("servers");
    let blocker = dir.path().join("blocker");
    std::fs::write(&blocker, b"").expect("blocker");
    let mut config = config(blocker.join("profiles.toml"), dir.path());
    config.legacy_dir = Some(legacy);
    let mut app = App::new(config);
    app.update(Message::ImportLegacy);
    assert!(matches!(&app.dialog, Some(Dialog::StoreError { .. })));
    assert!(app.profiles().is_empty());
}
