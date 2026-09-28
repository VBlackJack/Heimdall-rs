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

//! The language chosen on the Settings page.
//!
//! The texts are one loader for the whole process: switching it here would change what the
//! other test files read, so this file, its own process, holds the only test that switches.

use std::path::Path;

use heimdall_app::{App, AppConfig, SystemCredentials};
use heimdall_core::settings::{Language, SETTINGS_FILE_NAME, Settings};
use heimdall_ssh::AgentSource;
use heimdall_term::GridSize;
use heimdall_ui::shell::{Message, Shell};
use heimdall_ui::terminal_view::FONTS;
use iced::{Settings as IcedSettings, Size};
use iced_test::simulator::Simulator;

const WINDOW: Size = Size::new(1100.0, 1800.0);

fn shell(dir: &Path) -> Shell {
    Shell::with_app(App::new(AppConfig {
        profiles_file: dir.join("profiles.toml"),
        known_hosts: dir.join("known_hosts"),
        legacy_dir: None,
        agent: AgentSource::Disabled,
        initial_grid: GridSize { cols: 80, rows: 24 },
        files_start: dir.to_owned(),
        system_credentials: SystemCredentials::memory(),
    }))
}

fn simulator(shell: &Shell) -> Simulator<'_, Message> {
    let settings = IcedSettings {
        fonts: FONTS.iter().map(|face| (*face).into()).collect(),
        ..IcedSettings::default()
    };
    Simulator::with_size(settings, WINDOW, shell.view())
}

#[test]
fn each_language_chosen_shows_at_once_and_is_kept_for_the_next_run() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = shell(dir.path());
    let _ = shell.update(Message::ShowSettings);
    {
        let mut ui = simulator(&shell);
        for label in ["Appearance", "Language", "Profiles"] {
            ui.find(label).expect(label);
        }
    }
    for (language, title) in [
        (Language::French, "Profils"),
        (Language::Spanish, "Perfiles"),
        (Language::English, "Profiles"),
    ] {
        let _ = shell.update(Message::LanguageChosen(language));
        assert_eq!(heimdall_ui::i18n::current(), language);
        let mut ui = simulator(&shell);
        ui.find(title).expect(title);
        let saved = Settings::load(&dir.path().join(SETTINGS_FILE_NAME)).expect("saved");
        assert_eq!(saved.language, Some(language));
    }
}
