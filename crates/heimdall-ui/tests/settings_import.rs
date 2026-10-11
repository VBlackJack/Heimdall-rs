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

//! The question an import of a settings file asks, as the C# preview: what decides what
//! runs on this computer first, under its warning, then the other changes, then what the
//! file holds that is not imported. Import takes it; Cancel leaves the settings as they were,
//! down to the byte.

mod common;

use std::path::Path;

use heimdall_app::{
    App, AppConfig, Message as AppMessage, SettingsTransferMessage, SystemCredentials,
};
use heimdall_core::profile::RdpDefaults;
use heimdall_core::settings::{ColorScheme, SETTINGS_FILE_NAME, Settings};
use heimdall_ssh::AgentSource;
use heimdall_term::GridSize;
use heimdall_ui::shell::{Message, Shell};
use heimdall_ui::terminal_view::FONTS;
use iced::Size;

const WINDOW: Size = Size::new(1200.0, 900.0);

/// The C# warning above the settings deciding what runs on this computer.
const SENSITIVE_WARNING: &str =
    "Check these first: they decide what Heimdall runs on this computer.";

/// The heading above the settings weakening a security check.
const WEAKENS_HEADING: &str = "These weaken a security check:";

/// A program the file points the `PuTTY` path at.
const PUTTY: &str = "/opt/elsewhere/putty";

/// A settings file changing the terminal's colours and the `PuTTY` path, and holding a
/// setting this version does not know.
fn file() -> String {
    let (text, _) = Settings {
        color_scheme: ColorScheme::SolarizedDark,
        putty_path: PUTTY.to_owned(),
        ..Settings::default()
    }
    .export(None, false);
    format!("{text}\n[settings.telemetry]\nenabled = true\n")
}

/// The window of a fresh application whose settings file is written, asked about `file`.
fn asked(dir: &Path, file: String) -> Shell {
    Settings::default()
        .save(&dir.join(SETTINGS_FILE_NAME))
        .expect("settings written");
    let mut shell = Shell::with_app(App::new(AppConfig {
        profiles_file: dir.join("profiles.toml"),
        known_hosts: dir.join("known_hosts"),
        legacy_dir: None,
        agent: AgentSource::Disabled,
        initial_grid: GridSize { cols: 80, rows: 24 },
        files_start: dir.to_owned(),
        system_credentials: SystemCredentials::memory(),
    }));
    let _ = shell.update(Message::App(AppMessage::SettingsTransfer(
        SettingsTransferMessage::Read(Ok(file)),
    )));
    shell
}

fn simulator(shell: &Shell) -> common::Drawn<'_> {
    common::simulator(
        iced::Settings {
            fonts: FONTS.iter().map(|face| (*face).into()).collect(),
            ..iced::Settings::default()
        },
        WINDOW,
        shell.view(),
    )
}

/// Clicks `label` in the window, then hands the window what it sent.
fn click(shell: &mut Shell, label: &str) {
    let messages: Vec<Message> = {
        let mut ui = simulator(shell);
        ui.click(label).expect(label);
        ui.into_messages().collect()
    };
    for message in messages {
        let _ = shell.update(message);
    }
}

#[test]
fn the_preview_shows_what_runs_here_first_then_the_rest_and_what_is_left_out() {
    let dir = tempfile::tempdir().expect("dir");
    let shell = asked(dir.path(), file());
    let mut ui = simulator(&shell);
    ui.find("Import settings").expect("title");
    ui.find(SENSITIVE_WARNING).expect("the C# warning");
    ui.find(format!("ssh.putty_path: (empty) -> {PUTTY}").as_str())
        .expect("the PuTTY path, under the warning");
    ui.find("2 settings will change:").expect("how many");
    ui.find("terminal.color_scheme: Dracula -> Solarized Dark")
        .expect("the harmless change");
    ui.find("The file also holds 1 setting this version does not import. It is left out:")
        .expect("said, not dropped");
    ui.find("telemetry.enabled").expect("named");
}

#[test]
fn harmless_changes_alone_carry_no_warning() {
    let dir = tempfile::tempdir().expect("dir");
    let (text, _) = Settings {
        color_scheme: ColorScheme::SolarizedDark,
        ..Settings::default()
    }
    .export(None, false);
    let shell = asked(dir.path(), text);
    let mut ui = simulator(&shell);
    ui.find("1 setting will change:").expect("how many");
    assert!(ui.find(SENSITIVE_WARNING).is_err(), "no warning");
    assert!(
        ui.find("The file also holds 1 setting this version does not import. It is left out:")
            .is_err(),
        "nothing left out"
    );
}

#[test]
fn import_takes_the_files_settings() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = asked(dir.path(), file());
    click(&mut shell, "Import");
    assert_eq!(shell.app().dialog, None);
    assert_eq!(shell.app().settings().putty_path, PUTTY);
    assert_eq!(
        shell.app().settings().color_scheme,
        ColorScheme::SolarizedDark
    );
    let saved = Settings::load(&dir.path().join(SETTINGS_FILE_NAME)).expect("saved");
    assert_eq!(saved.putty_path, PUTTY);
}

#[test]
fn cancel_leaves_the_settings_byte_for_byte() {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join(SETTINGS_FILE_NAME);
    let mut shell = asked(dir.path(), file());
    let before = std::fs::read(&path).expect("read");
    let settings = shell.app().settings().clone();
    click(&mut shell, "Cancel");
    assert_eq!(shell.app().dialog, None);
    assert_eq!(shell.app().settings(), &settings);
    assert_eq!(std::fs::read(&path).expect("read"), before);
}

#[test]
fn a_weakened_check_is_shown_first_under_its_heading_and_a_tightened_one_is_not() {
    let dir = tempfile::tempdir().expect("dir");
    let (text, _) = Settings {
        rdp_defaults: RdpDefaults {
            nla: false,
            strict_server_authentication: true,
            ..RdpDefaults::default()
        },
        ..Settings::default()
    }
    .export(None, false);
    let shell = asked(dir.path(), text);
    let mut ui = simulator(&shell);
    ui.find(WEAKENS_HEADING).expect("the heading");
    ui.find("rdp.nla: On -> Off")
        .expect("NLA turned off, under it");
    ui.find("rdp.strict_server_authentication: Off -> On")
        .expect("the stricter check, among the others");
    assert!(ui.find(SENSITIVE_WARNING).is_err(), "nothing runs here");
    drop(ui);

    // Tightening alone: no warning card at all.
    let dir = tempfile::tempdir().expect("dir");
    let (text, _) = Settings {
        rdp_defaults: RdpDefaults {
            strict_server_authentication: true,
            ..RdpDefaults::default()
        },
        ..Settings::default()
    }
    .export(None, false);
    let shell = asked(dir.path(), text);
    let mut ui = simulator(&shell);
    ui.find("1 setting will change:").expect("how many");
    assert!(ui.find(WEAKENS_HEADING).is_err(), "nothing weakened");
}
