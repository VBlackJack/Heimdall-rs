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

//! The settings carried to another computer, as the C# "Export settings" and "Import
//! settings".

use std::path::Path;

use heimdall_app::{
    App, AppConfig, Dialog, Effect, Message, Notice, SettingsMessage, SettingsTransferMessage,
};
use heimdall_core::settings::{ColorScheme, SETTINGS_FILE_NAME, Settings};
use heimdall_ssh::AgentSource;
use heimdall_term::GridSize;

fn app(dir: &Path) -> App {
    App::new(AppConfig {
        profiles_file: dir.join("profiles.toml"),
        known_hosts: dir.join("known_hosts"),
        legacy_dir: None,
        agent: AgentSource::Disabled,
        initial_grid: GridSize { cols: 80, rows: 24 },
        files_start: dir.to_owned(),
        system_credentials: heimdall_app::SystemCredentials::memory(),
    })
}

fn transfer(app: &mut App, message: SettingsTransferMessage) -> Vec<Effect> {
    app.update(Message::SettingsTransfer(message))
}

fn exported(effects: &[Effect]) -> String {
    match effects {
        [Effect::SaveSettingsFile { document }] => document.clone(),
        other => panic!("{other:?}"),
    }
}

#[test]
fn an_export_asks_where_and_says_how_it_went() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let document = exported(&transfer(&mut app, SettingsTransferMessage::Export));
    assert!(document.contains("heimdall-settings"), "{document}");
    transfer(&mut app, SettingsTransferMessage::Written(Ok(())));
    assert_eq!(app.notice(), Some(&Notice::SettingsExported));
    transfer(
        &mut app,
        SettingsTransferMessage::Written(Err("disk full".to_owned())),
    );
    assert_eq!(
        app.notice(),
        Some(&Notice::SettingsExportFailed("disk full".to_owned()))
    );
}

#[test]
fn a_path_of_this_users_folder_is_asked_about_and_left_out_unless_included() {
    let Some(home) = heimdall_core::paths::home_dir() else {
        return;
    };
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let editor = home.join("tools").join("edit").display().to_string();
    app.update(Message::Settings(SettingsMessage::ExternalEditor(
        editor.clone(),
    )));
    assert!(transfer(&mut app, SettingsTransferMessage::Export).is_empty());
    assert_eq!(
        app.dialog,
        Some(Dialog::ConfirmSettingsExportPaths { count: 1 })
    );
    // No: written without it.
    let without = exported(&app.update(Message::DismissDialog));
    assert!(!without.contains("edit"), "{without}");
    assert_eq!(app.dialog, None);
    // Yes: written with it.
    transfer(&mut app, SettingsTransferMessage::Export);
    let with = exported(&app.update(Message::ConfirmDialog));
    assert!(with.contains("edit"), "{with}");
}

#[test]
fn an_import_shows_what_it_changes_and_takes_it_once_agreed_to() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let elsewhere = Settings {
        color_scheme: ColorScheme::SolarizedDark,
        ssh_keep_alive_interval: 45,
        ..Settings::default()
    };
    let (text, _) = elsewhere.export(None, false);

    assert!(matches!(
        transfer(&mut app, SettingsTransferMessage::Import).as_slice(),
        [Effect::PickSettingsFile]
    ));
    transfer(&mut app, SettingsTransferMessage::Read(Ok(text.clone())));
    let Some(Dialog::ConfirmSettingsImport(read)) = &app.dialog else {
        panic!("{:?}", app.dialog);
    };
    assert_eq!(read.changes.len(), 2, "{:?}", read.changes);
    // Cancelled: nothing changes.
    app.update(Message::DismissDialog);
    assert_eq!(
        app.settings().color_scheme,
        Settings::default().color_scheme
    );

    transfer(&mut app, SettingsTransferMessage::Read(Ok(text.clone())));
    app.update(Message::ConfirmDialog);
    assert_eq!(app.settings().color_scheme, ColorScheme::SolarizedDark);
    assert_eq!(app.settings().ssh_keep_alive_interval, 45);
    assert_eq!(app.notice(), Some(&Notice::SettingsImported(2)));
    // Saved.
    let saved = Settings::load(&dir.path().join(SETTINGS_FILE_NAME)).expect("saved");
    assert_eq!(saved.ssh_keep_alive_interval, 45);

    // The same file again: nothing to change.
    transfer(&mut app, SettingsTransferMessage::Read(Ok(text)));
    assert_eq!(app.dialog, None);
    assert_eq!(app.notice(), Some(&Notice::SettingsImportNothing));
}

#[test]
fn a_file_that_is_not_settings_or_cannot_be_read_changes_nothing() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    transfer(
        &mut app,
        SettingsTransferMessage::Read(Ok("[servers]\nname = 1\n".to_owned())),
    );
    assert_eq!(app.notice(), Some(&Notice::SettingsImportInvalid));
    transfer(
        &mut app,
        SettingsTransferMessage::Read(Ok(
            "format = \"heimdall-settings\"\nversion = 9\n[settings]\n".to_owned(),
        )),
    );
    assert_eq!(app.notice(), Some(&Notice::SettingsImportNewer));
    transfer(
        &mut app,
        SettingsTransferMessage::Read(Err("denied".to_owned())),
    );
    assert_eq!(
        app.notice(),
        Some(&Notice::SettingsImportFailed("denied".to_owned()))
    );
    assert_eq!(app.dialog, None);
}
