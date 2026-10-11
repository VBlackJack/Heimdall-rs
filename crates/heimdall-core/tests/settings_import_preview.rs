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

//! What an import of a portable settings file shows before it is taken, as the C#
//! `SettingsImportPreview`: the settings deciding what runs on this computer first, the
//! others after, and what the file holds that is not imported.

use heimdall_core::credential_provider::ProviderSettings;
use heimdall_core::settings::{ColorScheme, Sensitivity, Settings, SettingsImport};

/// The settings file header every file below starts with.
const HEADER: &str = "format = \"heimdall-settings\"\nversion = 1\n";

/// The keys of `changes`.
fn keys<'a>(
    changes: impl Iterator<Item = &'a heimdall_core::settings::SettingChange>,
) -> Vec<&'a str> {
    changes.map(|change| change.key.as_str()).collect()
}

/// `elsewhere` exported, then read over the defaults.
fn read_over_defaults(elsewhere: &Settings) -> SettingsImport {
    let (text, _) = elsewhere.export(None, false);
    Settings::default().import(&text).expect("read")
}

#[test]
fn a_file_of_the_same_settings_changes_nothing_and_leaves_nothing_out() {
    let read = read_over_defaults(&Settings::default());
    assert!(read.changes.is_empty(), "{:?}", read.changes);
    assert!(read.ignored.is_empty(), "{:?}", read.ignored);
    assert_eq!(read.sensitive().count(), 0);
}

#[test]
fn harmless_changes_are_listed_with_none_shown_first() {
    let read = read_over_defaults(&Settings {
        color_scheme: ColorScheme::SolarizedDark,
        ssh_keep_alive_interval: 45,
        ..Settings::default()
    });
    assert_eq!(
        keys(read.benign()),
        ["terminal.color_scheme", "ssh.keep_alive_interval"]
    );
    assert_eq!(read.sensitive().count(), 0);
    assert!(
        read.changes
            .iter()
            .all(|change| change.sensitivity.is_none())
    );
    assert!(read.ignored.is_empty(), "{:?}", read.ignored);
}

#[test]
fn what_runs_on_this_computer_is_shown_first_whatever_the_files_order() {
    let read = read_over_defaults(&Settings {
        color_scheme: ColorScheme::SolarizedDark,
        putty_path: "/opt/evil/putty".to_owned(),
        x11_server_path: "/opt/evil/x".to_owned(),
        external_editor: "/opt/evil/edit".to_owned(),
        credential_provider: ProviderSettings {
            enabled: true,
            command: "/opt/evil/secret {host}".to_owned(),
            username_command: "/opt/evil/user".to_owned(),
            database: "/opt/evil/db.kdbx".to_owned(),
            key_file: "/opt/evil/db.key".to_owned(),
            first_line_only: true,
            ..ProviderSettings::default()
        },
        ..Settings::default()
    });
    assert_eq!(
        keys(read.sensitive()),
        [
            "credential_provider.command",
            "credential_provider.database",
            "credential_provider.enabled",
            "credential_provider.key_file",
            "credential_provider.username_command",
            "ssh.putty_path",
            "ssh.x11_server_path",
            "files.external_editor",
        ]
    );
    assert!(
        read.sensitive()
            .all(|change| change.sensitivity == Some(Sensitivity::RunsOnThisComputer))
    );
    // How the provider reads its output is not what it runs: among the others, as in C#.
    assert_eq!(
        keys(read.benign()),
        [
            "terminal.color_scheme",
            "credential_provider.first_line_only"
        ]
    );
}

#[test]
fn a_setting_this_version_does_not_import_is_named_and_the_rest_taken() {
    let text = format!(
        "{HEADER}[settings]\nloose = 3\n\
         [settings.ssh]\nkeep_alive_interval = 45\nfuture_option = true\n\
         [settings.telemetry]\nenabled = true\n\
         [settings.pin]\nhash = \"AAAA\"\n"
    );
    let read = Settings::default().import(&text).expect("read");
    let mut ignored = read.ignored.clone();
    ignored.sort();
    assert_eq!(
        ignored,
        [
            "loose",
            "pin.hash",
            "ssh.future_option",
            "telemetry.enabled"
        ]
    );
    assert_eq!(keys(read.changes.iter()), ["ssh.keep_alive_interval"]);
    assert_eq!(read.settings.ssh_keep_alive_interval, 45);
    assert_eq!(read.settings.pin, None, "the PIN stays this computer's");
}
