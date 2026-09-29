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

//! The terminals' colour scheme, from the Settings page: saved beside the profiles, and
//! applied to the terminals open and to come.

use std::path::Path;

use heimdall_app::{App, AppConfig, Dialog, Message, SettingsMessage};
use heimdall_core::profile::{ProfileId, SshProfile};
use heimdall_core::settings::{ColorScheme, SETTINGS_FILE_NAME};
use heimdall_core::store::ProfileStore;
use heimdall_ssh::AgentSource;
use heimdall_term::{GridSize, Palette, Rgb};

fn app(dir: &Path) -> App {
    let profiles_file = dir.join("profiles.toml");
    let mut store = ProfileStore::open(&profiles_file).expect("store");
    store.merge([SshProfile {
        id: ProfileId::new("web"),
        name: "web".to_owned(),
        group: None,
        host: "web.lab".to_owned(),
        port: 22,
        username: None,
        key_path: None,
        gateway: None,
        vault_entry: None,
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

fn backgrounds(app: &App) -> Vec<Rgb> {
    app.tabs
        .iter()
        .map(|tab| tab.terminal.snapshot().background)
        .collect()
}

fn open(app: &mut App) {
    app.update(Message::OpenProfile(ProfileId::new("web")));
}

#[test]
fn a_scheme_chosen_colours_every_terminal_and_is_kept_for_the_next_run() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    assert_eq!(app.settings().color_scheme, ColorScheme::Dracula);
    open(&mut app);
    assert_eq!(backgrounds(&app), [Palette::dracula().background]);

    app.update(Message::Settings(SettingsMessage::ColorScheme(
        ColorScheme::Nord,
    )));
    assert_eq!(app.settings().color_scheme, ColorScheme::Nord);
    assert_eq!(
        backgrounds(&app),
        [Palette::nord().background],
        "the terminal open"
    );
    open(&mut app);
    assert_eq!(
        backgrounds(&app),
        [Palette::nord().background, Palette::nord().background],
        "and one opened after"
    );

    let again = self::app(dir.path());
    assert_eq!(again.settings().color_scheme, ColorScheme::Nord);
    for (scheme, palette) in [
        (ColorScheme::Standard, Palette::standard()),
        (ColorScheme::SolarizedDark, Palette::solarized_dark()),
        (ColorScheme::Monokai, Palette::monokai()),
        (ColorScheme::Dracula, Palette::dracula()),
    ] {
        app.update(Message::Settings(SettingsMessage::ColorScheme(scheme)));
        assert_eq!(backgrounds(&app)[0], palette.background, "{scheme:?}");
    }
    assert_eq!(Palette::standard().background, Rgb { r: 0, g: 0, b: 0 });
}

#[test]
fn an_unreadable_settings_file_is_said_and_never_written_over() {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join(SETTINGS_FILE_NAME);
    std::fs::write(&path, "version = 1\nterminal = [").expect("written");
    let mut app = app(dir.path());
    assert!(matches!(app.dialog, Some(Dialog::StoreError { .. })));
    assert_eq!(app.settings().color_scheme, ColorScheme::Dracula);
    app.update(Message::DismissDialog);

    app.update(Message::Settings(SettingsMessage::ColorScheme(
        ColorScheme::Monokai,
    )));
    assert_eq!(app.settings().color_scheme, ColorScheme::Monokai);
    assert_eq!(
        std::fs::read_to_string(&path).expect("kept"),
        "version = 1\nterminal = [",
        "the unreadable file is left as it was"
    );
    assert!(dir.path().join("settings.recovered.toml").is_file());
}

#[test]
fn a_scheme_that_cannot_be_saved_is_said_and_not_applied() {
    let dir = tempfile::tempdir().expect("dir");
    // Directories where the file and its recovery would go: neither can be written.
    std::fs::create_dir(dir.path().join(SETTINGS_FILE_NAME)).expect("dir");
    std::fs::create_dir(dir.path().join("settings.recovered.toml")).expect("dir");
    let mut app = app(dir.path());
    app.update(Message::DismissDialog);
    open(&mut app);
    app.update(Message::Settings(SettingsMessage::ColorScheme(
        ColorScheme::Nord,
    )));
    assert!(matches!(app.dialog, Some(Dialog::StoreError { .. })));
    assert_eq!(app.settings().color_scheme, ColorScheme::Dracula);
    assert_eq!(backgrounds(&app), [Palette::dracula().background]);
}

#[test]
fn a_terminal_font_size_in_the_range_is_kept_for_the_next_run_and_another_ignored() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    app.update(Message::Settings(SettingsMessage::TerminalFontSize(20)));
    assert_eq!(app.settings().terminal_font_size, 20);
    for refused in [7, 73] {
        app.update(Message::Settings(SettingsMessage::TerminalFontSize(
            refused,
        )));
        assert_eq!(app.settings().terminal_font_size, 20, "{refused} ignored");
    }
    assert_eq!(self::app(dir.path()).settings().terminal_font_size, 20);
}

#[test]
fn a_language_chosen_is_kept_for_the_next_run() {
    use heimdall_core::settings::Language;

    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    assert_eq!(app.settings().language, None, "the desktop's until chosen");
    app.update(Message::Settings(SettingsMessage::Language(
        Language::Spanish,
    )));
    assert_eq!(
        self::app(dir.path()).settings().language,
        Some(Language::Spanish)
    );
}
