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

//! The default RDP mode, as the C# `RdpDefaultMode`: a new profile's form starts with it,
//! "Apply to all saved sessions" writes it into every RDP profile once agreed to, in one
//! save that changes nothing when it fails, and "Reset RDP defaults" puts it back.

use std::fs;
use std::path::Path;

use heimdall_app::profile_draft::{DraftProtocol, ProfileField};
use heimdall_app::{
    App, AppConfig, Dialog, FolderMessage, Message, SettingsMessage, SystemCredentials,
};
use heimdall_core::profile::{Forwards, ProfileId, RdpExtras, RdpMode, RdpOptions, RdpProfile};
use heimdall_core::settings::{SETTINGS_FILE_NAME, Settings};
use heimdall_core::store::ProfileStore;
use heimdall_ssh::AgentSource;
use heimdall_term::GridSize;

fn profile(id: &str, mode: RdpMode) -> RdpProfile {
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
        username: None,
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
    }
}

/// Three desktops, one already in Remote Desktop Connection.
fn saved() -> [RdpProfile; 3] {
    [
        profile("dc", RdpMode::Embedded),
        profile("web", RdpMode::External),
        profile("app", RdpMode::Embedded),
    ]
}

fn app(dir: &Path) -> App {
    let profiles_file = dir.join("profiles.toml");
    let mut store = ProfileStore::open(&profiles_file).expect("store");
    store.merge_rdp(saved());
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
fn modes(profiles: &[RdpProfile]) -> Vec<(String, RdpMode)> {
    profiles
        .iter()
        .map(|profile| (profile.id.to_string(), RdpMode::of(profile.extras.external)))
        .collect()
}

/// The modes the profile file holds.
fn modes_on_disk(dir: &Path) -> Vec<(String, RdpMode)> {
    let store = ProfileStore::open(dir.join("profiles.toml")).expect("store");
    modes(store.rdp_profiles())
}

fn choose(app: &mut App, mode: RdpMode) {
    app.update(Message::Settings(SettingsMessage::RdpDefaultMode(mode)));
}

/// The RDP mode of the form open.
fn form_mode(app: &App) -> RdpMode {
    match &app.dialog {
        Some(Dialog::EditProfile { draft, .. }) => RdpMode::of(draft.rdp_extras.external),
        other => panic!("{other:?}"),
    }
}

#[test]
fn the_default_is_embedded_saved_once_chosen_and_new_forms_start_with_it() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    assert_eq!(app.settings().rdp_default_mode, RdpMode::Embedded);
    app.update(Message::NewProfile);
    assert_eq!(form_mode(&app), RdpMode::Embedded, "the C# default");
    app.update(Message::DismissDialog);

    choose(&mut app, RdpMode::External);
    let read = Settings::load(&dir.path().join(SETTINGS_FILE_NAME)).expect("load");
    assert_eq!(read.rdp_default_mode, RdpMode::External, "saved at once");

    app.update(Message::NewProfile);
    assert_eq!(form_mode(&app), RdpMode::External);
    app.update(Message::ChooseProtocol(DraftProtocol::Rdp));
    assert_eq!(form_mode(&app), RdpMode::External, "the protocol chosen");
    app.update(Message::Folder(FolderMessage::NewProfileIn(String::new())));
    assert_eq!(form_mode(&app), RdpMode::External, "from a folder's menu");

    // Saved with it.
    app.update(Message::ChooseProtocol(DraftProtocol::Rdp));
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
        .rdp_profiles()
        .iter()
        .find(|profile| profile.name == "new")
        .expect("saved");
    assert!(new.extras.external);

    // A saved profile keeps its own.
    app.update(Message::EditProfile(ProfileId::new("dc")));
    assert_eq!(form_mode(&app), RdpMode::Embedded);
    // And the SSH default is its own.
    assert_eq!(
        app.settings().ssh_default_mode,
        heimdall_core::profile::SshMode::Embedded
    );
}

#[test]
fn apply_to_all_asks_with_the_count_then_rewrites_every_rdp_profile_in_one_save() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    choose(&mut app, RdpMode::External);

    app.update(Message::Settings(SettingsMessage::ApplyRdpModeToAll));
    assert_eq!(
        app.dialog,
        Some(Dialog::ConfirmApplyRdpMode {
            mode: RdpMode::External,
            changes: 2,
            total: 3,
        }),
        "the profiles that change, of the profiles there are"
    );
    app.update(Message::DismissDialog);
    assert_eq!(
        modes(app.rdp_profiles()),
        modes(&saved()),
        "nothing changed"
    );

    app.update(Message::Settings(SettingsMessage::ApplyRdpModeToAll));
    app.update(Message::ConfirmDialog);
    let applied = vec![
        ("dc".to_owned(), RdpMode::External),
        ("web".to_owned(), RdpMode::External),
        ("app".to_owned(), RdpMode::External),
    ];
    assert_eq!(modes(app.rdp_profiles()), applied);
    assert_eq!(modes_on_disk(dir.path()), applied, "saved");
    assert_eq!(app.dialog, None);
    assert_eq!(app.settings().rdp_default_mode, RdpMode::External);

    // Nothing left to change: nothing is asked, as the C# stops there.
    app.update(Message::Settings(SettingsMessage::ApplyRdpModeToAll));
    assert_eq!(app.dialog, None);

    // Back to Embedded: all three change.
    choose(&mut app, RdpMode::Embedded);
    app.update(Message::Settings(SettingsMessage::ApplyRdpModeToAll));
    assert_eq!(
        app.dialog,
        Some(Dialog::ConfirmApplyRdpMode {
            mode: RdpMode::Embedded,
            changes: 3,
            total: 3,
        })
    );
    // The SSH one is not asked about.
    app.update(Message::DismissDialog);
    app.update(Message::Settings(SettingsMessage::ApplySshModeToAll));
    assert_eq!(app.dialog, None, "no SSH profile");
}

#[test]
fn a_save_that_fails_changes_no_profile_and_says_why() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    choose(&mut app, RdpMode::External);
    app.update(Message::Settings(SettingsMessage::ApplyRdpModeToAll));
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
    assert_eq!(
        modes(app.rdp_profiles()),
        modes(&saved()),
        "not one of them"
    );
    assert_eq!(fs::read_to_string(&file).expect("read"), outside);
}

#[test]
fn reset_rdp_defaults_puts_the_mode_back_and_changes_no_profile() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    choose(&mut app, RdpMode::External);
    app.update(Message::Settings(SettingsMessage::ResetRdpDefaults));
    app.update(Message::ConfirmDialog);
    assert_eq!(app.settings().rdp_default_mode, RdpMode::Embedded);
    let read = Settings::load(&dir.path().join(SETTINGS_FILE_NAME)).expect("load");
    assert_eq!(read.rdp_default_mode, RdpMode::Embedded, "saved");
    assert_eq!(modes(app.rdp_profiles()), modes(&saved()));
}
