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

//! The Settings page's About tab: what it says, and its diagnostics log switch.

mod common;

use std::path::Path;

use heimdall_app::{App, AppConfig, SystemCredentials};
use heimdall_ssh::AgentSource;
use heimdall_term::GridSize;
use heimdall_ui::shell::{Message, SettingsTab, Shell};
use heimdall_ui::terminal_view::FONTS;
use iced::{Settings, Size};

/// A window tall enough for the whole tab.
const WINDOW: Size = Size::new(1100.0, 1400.0);

fn shell(dir: &Path) -> Shell {
    let mut shell = Shell::with_app(App::new(AppConfig {
        profiles_file: dir.join("profiles.toml"),
        known_hosts: dir.join("known_hosts"),
        legacy_dir: None,
        agent: AgentSource::Disabled,
        initial_grid: GridSize { cols: 80, rows: 24 },
        files_start: dir.to_owned(),
        system_credentials: SystemCredentials::memory(),
    }));
    let _ = shell.update(Message::ShowSettings);
    let _ = shell.update(Message::SettingsTab(SettingsTab::About));
    shell
}

fn simulator(shell: &Shell) -> common::Drawn<'_> {
    let settings = Settings {
        fonts: FONTS.iter().map(|face| (*face).into()).collect(),
        ..Settings::default()
    };
    common::simulator(settings, WINDOW, shell.view())
}

#[test]
fn the_about_tab_says_the_version_and_where_the_data_is_and_turns_the_log_off() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = shell(dir.path());
    let messages: Vec<Message> = {
        let mut ui = simulator(&shell);
        let version = format!("Version {}", env!("CARGO_PKG_VERSION"));
        let config = dir.path().display().to_string();
        for label in [
            version.as_str(),
            "System",
            "Data",
            "Quick access",
            "Open config folder",
            "Open logs folder",
            config.as_str(),
        ] {
            ui.find(label).expect(label);
        }
        ui.click("Write the application diagnostics log (Heimdall's own events and errors)")
            .expect("the switch");
        ui.into_messages().collect()
    };
    for message in messages {
        let _ = shell.update(message);
    }
    assert!(!shell.app().settings().diagnostics_log, "off, and saved");
}
