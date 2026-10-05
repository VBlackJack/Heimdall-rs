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

//! The terminal macros on the Settings page, and their editor.

mod common;

use std::path::Path;

use heimdall_app::{App, AppConfig, MacroMessage, Message as AppMessage, SystemCredentials};
use heimdall_core::macros::{Expect, MacroEntry, Macros, OnTimeout, TerminalMacro, macros_path};
use heimdall_ssh::AgentSource;
use heimdall_term::GridSize;
use heimdall_ui::shell::{Message, SettingsTab, Shell};
use heimdall_ui::terminal_view::FONTS;
use iced::{Settings, Size};

/// A window tall enough for the Terminal tab.
const WINDOW: Size = Size::new(1200.0, 1400.0);

/// The folder a picture of the editor is written to, when set.
const SNAPSHOT_VARIABLE: &str = "HEIMDALL_SNAPSHOT_DIR";

fn shell(dir: &Path) -> Shell {
    let profiles_file = dir.join("profiles.toml");
    let mut macros = Macros::default();
    macros.put(TerminalMacro {
        name: "deploy".to_owned(),
        entries: vec![
            MacroEntry {
                input: "sudo -i\r".to_owned(),
                delay_ms: 0,
                expect: None,
            },
            MacroEntry {
                input: "systemctl restart web\r".to_owned(),
                delay_ms: 500,
                expect: Some(Expect {
                    pattern: "# $".to_owned(),
                    regex: false,
                    timeout_ms: 5000,
                    on_timeout: OnTimeout::Abort,
                }),
            },
        ],
    });
    macros.save(&macros_path(&profiles_file)).expect("saved");
    let mut shell = Shell::with_app(App::new(AppConfig {
        profiles_file,
        known_hosts: dir.join("known_hosts"),
        legacy_dir: None,
        agent: AgentSource::Disabled,
        initial_grid: GridSize { cols: 80, rows: 24 },
        files_start: dir.to_owned(),
        system_credentials: SystemCredentials::memory(),
    }));
    let _ = shell.update(Message::ShowSettings);
    let _ = shell.update(Message::SettingsTab(SettingsTab::Terminal));
    shell
}

fn simulator(shell: &Shell) -> common::Drawn<'_> {
    let settings = Settings {
        fonts: FONTS.iter().map(|face| (*face).into()).collect(),
        ..Settings::default()
    };
    common::simulator(settings, WINDOW, shell.view())
}

/// A picture of the window, written as `name` to the folder [`SNAPSHOT_VARIABLE`] names.
fn snapshot(shell: &Shell, name: &str) {
    let Some(dir) = std::env::var_os(SNAPSHOT_VARIABLE) else {
        return;
    };
    let path = Path::new(&dir).join(name);
    let stem = name.trim_end_matches(".png");
    if let Ok(entries) = std::fs::read_dir(Path::new(&dir)) {
        for entry in entries.flatten() {
            let file = entry.file_name();
            let file = file.to_string_lossy();
            if file == name || file.starts_with(&format!("{stem}-")) {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
    simulator(shell)
        .snapshot(&shell.theme())
        .expect("drawn")
        .matches_image(path)
        .expect("written");
}

fn macro_messages(ui: common::Drawn<'_>) -> Vec<MacroMessage> {
    ui.into_messages()
        .filter_map(|message| match message {
            Message::App(AppMessage::Macro(message)) => Some(message),
            _ => None,
        })
        .collect()
}

#[test]
fn the_macros_kept_are_listed_and_one_is_edited_with_what_it_waits_for() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = shell(dir.path());
    {
        let mut ui = simulator(&shell);
        ui.find("deploy").expect("the macro");
        ui.find("2 inputs").expect("its inputs");
        ui.click("Edit").expect("Edit");
        assert_eq!(
            macro_messages(ui),
            [MacroMessage::Edit("deploy".to_owned())]
        );
    }
    let _ = shell.update(Message::App(AppMessage::Macro(MacroMessage::Edit(
        "deploy".to_owned(),
    ))));
    snapshot(&shell, "macro-editor.png");
    let mut ui = simulator(&shell);
    for label in [
        "Edit macro",
        r"sudo -i\r",
        r"systemctl restart web\r",
        "# $",
        "5000",
        "Add expect step",
        "Delete macro",
    ] {
        ui.find(label).expect(label);
    }
}
