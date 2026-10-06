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

//! The settings file, beside the profiles.

use std::path::Path;
use std::time::{Duration, SystemTime};

use heimdall_core::lockout::{LOCKOUT_DURATION, MAX_FAILED_ATTEMPTS};
use heimdall_core::pin::PinHash;
use heimdall_core::settings::{
    BroadcastScope, ColorScheme, SETTINGS_FILE_NAME, Settings, settings_path,
};
use heimdall_core::store::StoreError;

#[test]
fn it_sits_beside_the_profiles_and_holds_the_defaults_until_written() {
    let dir = tempfile::tempdir().expect("dir");
    let path = settings_path(&dir.path().join("profiles.toml"));
    assert_eq!(path, dir.path().join(SETTINGS_FILE_NAME));
    let settings = Settings::load(&path).expect("missing is fine");
    assert_eq!(
        settings.color_scheme,
        ColorScheme::Dracula,
        "as the C# default"
    );
}

#[test]
fn each_scheme_is_written_by_its_csharp_name_and_read_back() {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join("nested").join(SETTINGS_FILE_NAME);
    for scheme in ColorScheme::ALL {
        Settings {
            color_scheme: scheme,
            ..Settings::default()
        }
        .save(&path)
        .expect("saved");
        assert_eq!(Settings::load(&path).expect("read").color_scheme, scheme);
    }
    let text = std::fs::read_to_string(&path).expect("text");
    assert!(text.contains("color_scheme = \"Nord\""), "{text}");
    Settings {
        color_scheme: ColorScheme::SolarizedDark,
        ..Settings::default()
    }
    .save(&path)
    .expect("saved");
    let text = std::fs::read_to_string(&path).expect("text");
    assert!(text.contains("color_scheme = \"Solarized Dark\""), "{text}");
}

fn written(dir: &Path, text: &str) -> Settings {
    let path = dir.join(SETTINGS_FILE_NAME);
    std::fs::write(&path, text).expect("written");
    Settings::load(&path).expect("read")
}

#[test]
fn a_name_is_read_whatever_its_case_and_an_unknown_one_is_dracula() {
    let dir = tempfile::tempdir().expect("dir");
    let read = |text: &str| written(dir.path(), text).color_scheme;
    assert_eq!(
        read("version = 1\n[terminal]\ncolor_scheme = \" solarized DARK \"\n"),
        ColorScheme::SolarizedDark
    );
    assert_eq!(
        read("version = 1\n[terminal]\ncolor_scheme = \"default\"\n"),
        ColorScheme::Standard
    );
    assert_eq!(
        read("version = 1\n[terminal]\ncolor_scheme = \"Gruvbox\"\n"),
        ColorScheme::Dracula
    );
    assert_eq!(read("version = 1\n"), ColorScheme::Dracula, "no section");
    assert_eq!(
        ColorScheme::ALL.map(ColorScheme::name),
        ["Default", "Dracula", "Solarized Dark", "Monokai", "Nord"]
    );
}

#[test]
fn a_newer_or_broken_file_is_refused_not_guessed() {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join(SETTINGS_FILE_NAME);
    std::fs::write(&path, "version = 2\n").expect("written");
    assert!(matches!(
        Settings::load(&path),
        Err(StoreError::UnsupportedVersion {
            found: 2,
            expected: 1,
            ..
        })
    ));
    std::fs::write(&path, "version = 1\nterminal = [").expect("written");
    assert!(matches!(
        Settings::load(&path),
        Err(StoreError::Parse { .. })
    ));
    std::fs::write(&path, "version = 1\n").expect("written");
    assert!(Settings::load(&path).is_ok(), "the current version");
}

#[test]
fn session_logging_is_off_by_default_and_its_folder_beside_the_settings() {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join(SETTINGS_FILE_NAME);
    let settings = Settings::load(&path).expect("defaults");
    assert!(!settings.session_logging, "as the C# default");
    assert_eq!(
        settings.session_log_folder(&path).display().to_string(),
        dir.path()
            .join("logs")
            .join("sessions")
            .display()
            .to_string(),
        "written with the platform's separator"
    );

    let chosen = Settings {
        session_logging: true,
        session_log_directory: "transcripts".to_owned(),
        ..Settings::default()
    };
    chosen.save(&path).expect("saved");
    let read = Settings::load(&path).expect("read");
    assert_eq!(read, chosen);
    assert_eq!(
        read.session_log_folder(&path),
        dir.path().join("transcripts")
    );

    let absolute = Settings {
        session_log_directory: dir.path().join("elsewhere").display().to_string(),
        ..Settings::default()
    };
    assert_eq!(
        absolute.session_log_folder(&path),
        dir.path().join("elsewhere"),
        "an absolute folder as it is"
    );
    let blank = written(
        dir.path(),
        "version = 1\n[session_log]\nenabled = true\ndirectory = \"  \"\n",
    );
    assert!(blank.session_logging);
    assert_eq!(
        blank.session_log_directory, "logs/sessions",
        "blank: the default"
    );
}

#[test]
fn the_broadcast_scope_is_all_tabs_until_another_is_chosen() {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join(SETTINGS_FILE_NAME);
    assert_eq!(
        Settings::load(&path).expect("defaults").broadcast_scope,
        BroadcastScope::AllTabs
    );
    let chosen = Settings {
        broadcast_scope: BroadcastScope::SelectedTabs,
        ..Settings::default()
    };
    chosen.save(&path).expect("saved");
    assert_eq!(Settings::load(&path).expect("read"), chosen);
    assert_eq!(
        BroadcastScope::named(" selectedtabs "),
        BroadcastScope::SelectedTabs
    );
    assert_eq!(BroadcastScope::named("CurrentTab"), BroadcastScope::AllTabs);
}

#[test]
fn the_terminal_font_size_is_kept_within_the_csharp_range_and_else_the_default() {
    use heimdall_core::settings::{
        TERMINAL_FONT_SIZE_DEFAULT, TERMINAL_FONT_SIZE_MAX, TERMINAL_FONT_SIZE_MIN,
    };

    let dir = tempfile::tempdir().expect("dir");
    assert_eq!(
        written(
            dir.path(),
            "version = 1
"
        )
        .terminal_font_size,
        TERMINAL_FONT_SIZE_DEFAULT
    );
    for (size, read) in [
        (TERMINAL_FONT_SIZE_MIN, TERMINAL_FONT_SIZE_MIN),
        (TERMINAL_FONT_SIZE_MAX, TERMINAL_FONT_SIZE_MAX),
        (TERMINAL_FONT_SIZE_MIN - 1, TERMINAL_FONT_SIZE_DEFAULT),
        (TERMINAL_FONT_SIZE_MAX + 1, TERMINAL_FONT_SIZE_DEFAULT),
    ] {
        let text = format!(
            "version = 1
[terminal]
font_size = {size}
"
        );
        assert_eq!(
            written(dir.path(), &text).terminal_font_size,
            read,
            "{size}"
        );
    }
    let path = dir.path().join(SETTINGS_FILE_NAME);
    let settings = Settings {
        terminal_font_size: 20,
        ..Settings::default()
    };
    settings.save(&path).expect("saved");
    assert_eq!(Settings::load(&path).expect("read").terminal_font_size, 20);
}

#[test]
fn a_language_is_written_once_chosen_and_one_not_offered_follows_the_desktop() {
    use heimdall_core::settings::Language;

    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join(SETTINGS_FILE_NAME);
    Settings::default().save(&path).expect("saved");
    let text = std::fs::read_to_string(&path).expect("read");
    assert!(
        !text.contains("language"),
        "not chosen, not written: {text}"
    );
    assert_eq!(Settings::load(&path).expect("read").language, None);
    for language in Language::ALL {
        let settings = Settings {
            language: Some(language),
            ..Settings::default()
        };
        settings.save(&path).expect("saved");
        assert_eq!(
            Settings::load(&path).expect("read").language,
            Some(language)
        );
    }
    for (code, read) in [
        ("FR", Some(Language::French)),
        (" es ", Some(Language::Spanish)),
        ("de", None),
    ] {
        let text = format!(
            "version = 1
[general]
language = \"{code}\"
"
        );
        assert_eq!(written(dir.path(), &text).language, read, "{code}");
    }
}

#[test]
fn a_pin_and_its_wrong_tries_are_kept_across_runs() {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join(SETTINGS_FILE_NAME);
    let mut settings = Settings {
        pin: Some(PinHash::new("2468").expect("hash")),
        ..Settings::default()
    };
    let later = SystemTime::now() + Duration::from_secs(600);
    for _ in 0..MAX_FAILED_ATTEMPTS {
        settings
            .pin_unlock
            .register_failure(later - LOCKOUT_DURATION);
    }
    settings.save(&path).expect("saved");
    let read = Settings::load(&path).expect("read");
    assert!(read.pin.as_ref().is_some_and(|pin| pin.verify("2468")));
    assert_eq!(read.pin_unlock.failures(), MAX_FAILED_ATTEMPTS);
    assert!(read.pin_unlock.until().is_some(), "still locked out");
    assert_eq!(
        read.vault_unlock.failures(),
        0,
        "the master password's are apart"
    );
    let text = std::fs::read_to_string(&path).expect("text");
    assert!(!text.contains("2468"), "{text}");
}

#[test]
fn no_pin_writes_none_and_half_a_pin_takes_nothing() {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join(SETTINGS_FILE_NAME);
    Settings::default().save(&path).expect("saved");
    let text = std::fs::read_to_string(&path).expect("text");
    assert!(!text.contains("salt") && !text.contains("hash"), "{text}");
    assert_eq!(Settings::load(&path).expect("read").pin, None);

    let kept = PinHash::new("2468").expect("hash");
    for half in [
        format!("version = 1\n[pin]\nhash = \"{}\"\n", kept.hash()),
        format!("version = 1\n[pin]\nsalt = \"{}\"\n", kept.salt()),
    ] {
        let read = written(dir.path(), &half);
        let pin = read.pin.expect("a PIN is still set");
        assert!(!pin.verify("2468"), "{half}");
    }
}

#[test]
fn the_credential_provider_is_kept_by_its_csharp_names() {
    use heimdall_core::credential_provider::{DEFAULT_TIMEOUT, ProviderKind, ProviderSettings};

    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join(SETTINGS_FILE_NAME);
    let provider = ProviderSettings {
        enabled: true,
        kind: ProviderKind::WindowsCredentialManager,
        command: "keepassxc-cli show -s \"{Title}\"".to_owned(),
        username_command: "get-user {Title}".to_owned(),
        database: "/vaults/team.kdbx".to_owned(),
        key_file: "/keys/team.keyx".to_owned(),
        first_line_only: true,
        timeout: Duration::from_secs(30),
    };
    Settings {
        credential_provider: provider.clone(),
        ..Settings::default()
    }
    .save(&path)
    .expect("saved");
    assert_eq!(
        Settings::load(&path).expect("read").credential_provider,
        provider
    );
    let text = std::fs::read_to_string(&path).expect("text");
    assert!(
        text.contains("kind = \"WindowsCredentialManager\""),
        "{text}"
    );
    assert!(text.contains("timeout_ms = 30000"), "{text}");

    // Out of the C# range, the default; nothing written, the provider off.
    for timeout in [999, 120_001] {
        let read = written(
            dir.path(),
            &format!("version = 1\n[credential_provider]\ntimeout_ms = {timeout}\n"),
        );
        assert_eq!(
            read.credential_provider.timeout, DEFAULT_TIMEOUT,
            "{timeout}"
        );
    }
    for timeout in [1000, 120_000] {
        let read = written(
            dir.path(),
            &format!("version = 1\n[credential_provider]\ntimeout_ms = {timeout}\n"),
        );
        assert_eq!(
            read.credential_provider.timeout,
            Duration::from_millis(timeout),
            "{timeout} is in the range"
        );
    }
    assert_eq!(
        written(dir.path(), "version = 1\n").credential_provider,
        ProviderSettings::default()
    );
}

#[test]
fn the_anti_idle_interval_is_a_minute_by_default_zero_or_within_the_csharp_range() {
    use heimdall_core::settings::{
        ANTI_IDLE_INTERVAL_DEFAULT, ANTI_IDLE_INTERVAL_MAX, ANTI_IDLE_INTERVAL_MIN,
    };

    let dir = tempfile::tempdir().expect("dir");
    assert_eq!(written(dir.path(), "version = 1\n").anti_idle_interval, 60);
    assert_eq!(ANTI_IDLE_INTERVAL_DEFAULT, 60);
    assert_eq!((ANTI_IDLE_INTERVAL_MIN, ANTI_IDLE_INTERVAL_MAX), (10, 3600));
    for (seconds, read) in [
        (0, 0),
        (ANTI_IDLE_INTERVAL_MIN, ANTI_IDLE_INTERVAL_MIN),
        (ANTI_IDLE_INTERVAL_MAX, ANTI_IDLE_INTERVAL_MAX),
        // Out of the range, as the C# load warns and keeps the default.
        (ANTI_IDLE_INTERVAL_MIN - 1, ANTI_IDLE_INTERVAL_DEFAULT),
        (ANTI_IDLE_INTERVAL_MAX + 1, ANTI_IDLE_INTERVAL_DEFAULT),
    ] {
        let text = format!("version = 1\n[ssh]\nanti_idle_interval = {seconds}\n");
        assert_eq!(
            written(dir.path(), &text).anti_idle_interval,
            read,
            "{seconds}"
        );
    }

    let path = dir.path().join(SETTINGS_FILE_NAME);
    let settings = Settings {
        anti_idle_interval: 0,
        ..Settings::default()
    };
    settings.save(&path).expect("save");
    assert_eq!(
        Settings::load(&path).expect("load"),
        settings,
        "off, kept off"
    );
}

#[test]
fn ssh_keep_alive_and_tmout_reset_intervals_are_the_csharp_defaults_and_ranges() {
    use heimdall_core::settings::{
        SSH_KEEP_ALIVE_INTERVAL_DEFAULT, SSH_KEEP_ALIVE_INTERVAL_MAX, SSH_KEEP_ALIVE_INTERVAL_MIN,
        SSH_TMOUT_RESET_INTERVAL_DEFAULT, SSH_TMOUT_RESET_INTERVAL_MAX,
    };

    let dir = tempfile::tempdir().expect("dir");
    let read = written(dir.path(), "version = 1\n");
    assert_eq!(
        (read.ssh_keep_alive_interval, read.ssh_tmout_reset_interval),
        (30, 240)
    );
    assert_eq!(
        (
            SSH_KEEP_ALIVE_INTERVAL_DEFAULT,
            SSH_KEEP_ALIVE_INTERVAL_MIN,
            SSH_KEEP_ALIVE_INTERVAL_MAX
        ),
        (30, 5, 600)
    );
    assert_eq!(
        (
            SSH_TMOUT_RESET_INTERVAL_DEFAULT,
            SSH_TMOUT_RESET_INTERVAL_MAX
        ),
        (240, 3600)
    );
    for (keep_alive, tmout, read) in [
        (5, 0, (5, 0)),
        (600, 3600, (600, 3600)),
        // Out of the range, as the C# load warns and keeps the default.
        (4, 3601, (30, 240)),
        (601, 3601, (30, 240)),
    ] {
        let text = format!(
            "version = 1\n[ssh]\nkeep_alive_interval = {keep_alive}\ntmout_reset_interval = {tmout}\n"
        );
        let settings = written(dir.path(), &text);
        assert_eq!(
            (
                settings.ssh_keep_alive_interval,
                settings.ssh_tmout_reset_interval
            ),
            read,
            "{keep_alive} {tmout}"
        );
    }

    let path = dir.path().join(SETTINGS_FILE_NAME);
    let settings = Settings {
        ssh_keep_alive_interval: 120,
        ssh_tmout_reset_interval: 0,
        ..Settings::default()
    };
    settings.save(&path).expect("save");
    assert_eq!(Settings::load(&path).expect("load"), settings, "read back");
}

#[test]
fn ssh_auto_reconnect_is_off_by_default_and_its_attempts_kept_within_the_csharp_range() {
    use heimdall_core::settings::{
        SSH_AUTO_RECONNECT_ATTEMPTS_DEFAULT, SSH_AUTO_RECONNECT_ATTEMPTS_MAX,
        SSH_AUTO_RECONNECT_ATTEMPTS_MIN,
    };

    let dir = tempfile::tempdir().expect("dir");
    let read = written(dir.path(), "version = 1\n");
    assert!(!read.ssh_auto_reconnect, "off unless chosen, as the C#");
    assert_eq!(read.ssh_auto_reconnect_attempts, 3);
    assert_eq!(SSH_AUTO_RECONNECT_ATTEMPTS_DEFAULT, 3);
    for (attempts, read) in [
        (
            SSH_AUTO_RECONNECT_ATTEMPTS_MIN,
            SSH_AUTO_RECONNECT_ATTEMPTS_MIN,
        ),
        (
            SSH_AUTO_RECONNECT_ATTEMPTS_MAX,
            SSH_AUTO_RECONNECT_ATTEMPTS_MAX,
        ),
        (
            SSH_AUTO_RECONNECT_ATTEMPTS_MIN - 1,
            SSH_AUTO_RECONNECT_ATTEMPTS_DEFAULT,
        ),
        (
            SSH_AUTO_RECONNECT_ATTEMPTS_MAX + 1,
            SSH_AUTO_RECONNECT_ATTEMPTS_DEFAULT,
        ),
    ] {
        let text = format!(
            "version = 1\n[ssh]\nauto_reconnect = true\nauto_reconnect_attempts = {attempts}\n"
        );
        let settings = written(dir.path(), &text);
        assert!(settings.ssh_auto_reconnect);
        assert_eq!(settings.ssh_auto_reconnect_attempts, read, "{attempts}");
    }
    assert_eq!(
        (
            SSH_AUTO_RECONNECT_ATTEMPTS_MIN,
            SSH_AUTO_RECONNECT_ATTEMPTS_MAX
        ),
        (1, 10)
    );

    let path = dir.path().join(SETTINGS_FILE_NAME);
    let settings = Settings {
        ssh_auto_reconnect: true,
        ssh_auto_reconnect_attempts: 7,
        ..Settings::default()
    };
    settings.save(&path).expect("save");
    assert_eq!(
        Settings::load(&path).expect("load"),
        settings,
        "written and read back"
    );
}

#[test]
fn the_ssh_agent_preference_is_kept_by_its_csharp_name_and_auto_openssh_first_by_default() {
    use heimdall_core::settings::AgentPreference;

    let dir = tempfile::tempdir().expect("dir");
    assert_eq!(
        Settings::default().ssh_agent_preference,
        AgentPreference::OpenSshFirst
    );
    for (name, read) in [
        ("AutoPageantFirst", AgentPreference::PageantFirst),
        ("openSshOnly", AgentPreference::OpenSshOnly),
        ("PageantOnly", AgentPreference::PageantOnly),
        // A name not known, as the C# load: the default.
        ("Plink", AgentPreference::OpenSshFirst),
    ] {
        let text = format!("version = 1\n[ssh]\nagent_preference = \"{name}\"\n");
        assert_eq!(
            written(dir.path(), &text).ssh_agent_preference,
            read,
            "{name}"
        );
    }

    let path = dir.path().join(SETTINGS_FILE_NAME);
    let settings = Settings {
        ssh_agent_preference: AgentPreference::PageantFirst,
        ..Settings::default()
    };
    settings.save(&path).expect("save");
    assert!(
        std::fs::read_to_string(&path)
            .expect("written")
            .contains("agent_preference = \"AutoPageantFirst\""),
        "the C# name"
    );
    assert_eq!(Settings::load(&path).expect("load"), settings, "read back");
}

#[test]
fn resolution_presets_round_trip_and_reset_with_the_other_rdp_settings() {
    use heimdall_core::profile::RESOLUTION_PRESETS;

    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join(SETTINGS_FILE_NAME);
    let mut settings = Settings::load(&path).expect("defaults");
    assert_eq!(settings.rdp_resolution_presets, RESOLUTION_PRESETS);
    assert_eq!(settings.resolution_presets(), RESOLUTION_PRESETS);

    settings.rdp_resolution_presets = vec![(1366, 768), (800, 600)];
    settings.rdp_auto_reconnect_attempts = 3;
    settings.save(&path).expect("save");
    let read = Settings::load(&path).expect("load");
    assert_eq!(read.rdp_resolution_presets, [(1366, 768), (800, 600)]);
    let text = std::fs::read_to_string(&path).expect("read");
    assert!(
        text.contains(r#""1366x768""#) && text.contains(r#""800x600""#),
        "one WIDTHxHEIGHT per preset: {text}"
    );

    // Emptied, the menus offer the built-in list, as the C# catalog.
    settings.rdp_resolution_presets.clear();
    assert_eq!(settings.resolution_presets(), RESOLUTION_PRESETS);

    // A line edited by hand that is not a preset is left out, the others kept.
    std::fs::write(
        &path,
        "version = 1\n[rdp_session]\nresolution_presets =[\"1920x1080\", \"huge\", \"99999x1\"]\n",
    )
    .expect("write");
    let read = Settings::load(&path).expect("load");
    assert_eq!(read.rdp_resolution_presets, [(1920, 1080)]);

    // Reset: the RDP settings only.
    let mut settings = read;
    settings.rdp_auto_reconnect_attempts = 3;
    settings.rdp_defaults.redirect_drives = !settings.rdp_defaults.redirect_drives;
    settings.ssh_auto_reconnect = true;
    settings.reset_rdp();
    let defaults = Settings::default();
    assert_eq!(settings.rdp_resolution_presets, RESOLUTION_PRESETS);
    assert_eq!(
        settings.rdp_auto_reconnect_attempts,
        defaults.rdp_auto_reconnect_attempts
    );
    assert_eq!(settings.rdp_defaults, defaults.rdp_defaults);
    assert!(settings.ssh_auto_reconnect, "outside RDP, untouched");
}

#[test]
fn the_diagnostics_log_is_written_unless_turned_off_as_the_csharp_default() {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join(SETTINGS_FILE_NAME);
    let mut settings = Settings::load(&path).expect("defaults");
    assert!(settings.diagnostics_log);
    settings.diagnostics_log = false;
    settings.save(&path).expect("save");
    assert!(!Settings::load(&path).expect("load").diagnostics_log);
}

#[test]
fn an_export_carries_the_preferences_and_never_the_pin_nor_a_lockout() {
    let mut settings = Settings {
        color_scheme: ColorScheme::SolarizedDark,
        ssh_keep_alive_interval: 45,
        pin: Some(PinHash::new("2468").expect("pin")),
        ..Settings::default()
    };
    settings.vault_unlock.register_failure(SystemTime::now());
    let (text, held_back) = settings.export(None, false);
    assert_eq!(held_back, 0);
    assert!(text.contains("format = \"heimdall-settings\""), "{text}");
    assert!(text.contains("color_scheme = \"Solarized Dark\""), "{text}");
    assert!(!text.contains("[settings.pin]"), "{text}");
    assert!(!text.contains("vault_unlock"), "{text}");

    // Read back over the defaults: the preferences come, this computer's PIN stays.
    let read = Settings::default().import(&text).expect("read");
    assert_eq!(read.settings.color_scheme, ColorScheme::SolarizedDark);
    assert_eq!(read.settings.ssh_keep_alive_interval, 45);
    assert_eq!(read.settings.pin, None);
    let keys: Vec<&str> = read
        .changes
        .iter()
        .map(|change| change.key.as_str())
        .collect();
    assert!(keys.contains(&"terminal.color_scheme"), "{keys:?}");
    assert!(keys.contains(&"ssh.keep_alive_interval"), "{keys:?}");
    // Read over the same settings: nothing to change.
    assert!(settings.import(&text).expect("read").changes.is_empty());
}

#[test]
fn a_path_under_the_home_folder_stays_behind_unless_asked() {
    let home = Path::new("/home/admin");
    let settings = Settings {
        external_editor: "/home/admin/bin/edit".to_owned(),
        session_log_directory: "/srv/logs".to_owned(),
        ..Settings::default()
    };
    let (text, held_back) = settings.export(Some(home), false);
    assert_eq!(held_back, 1);
    assert!(!text.contains("/home/admin/bin/edit"), "{text}");
    assert!(text.contains("/srv/logs"), "{text}");
    let (text, _) = settings.export(Some(home), true);
    assert!(text.contains("/home/admin/bin/edit"), "{text}");
}

#[test]
fn a_file_that_is_not_a_settings_file_or_is_newer_is_refused() {
    use heimdall_core::settings::TransferError;

    let settings = Settings::default();
    for text in [
        "not toml at all [",
        "version = 1\n",
        "format = \"heimdall-settings\"\n",
        "format = \"other\"\nversion = 1\n[settings]\n",
        "format = \"heimdall-settings\"\nversion = 1\nsettings = 3\n",
        "format = \"heimdall-settings\"\nversion = 1\n[settings.ssh]\nkeep_alive_interval = \"x\"\n",
    ] {
        assert_eq!(
            settings.import(text),
            Err(TransferError::NotSettings),
            "{text}"
        );
    }
    assert_eq!(
        settings.import("format = \"heimdall-settings\"\nversion = 2\n[settings]\n"),
        Err(TransferError::Newer)
    );
    // A PIN written in by hand is not taken.
    let read = settings
        .import(
            "format = \"heimdall-settings\"\nversion = 1\n[settings.pin]\nsalt = \"AAAA\"\nhash = \"AAAA\"\n",
        )
        .expect("read");
    assert_eq!(read.settings.pin, None);
    assert!(read.changes.is_empty());
}

#[test]
fn the_reachability_check_is_on_with_the_csharp_numbers_and_kept_within_their_ranges() {
    use heimdall_core::settings::Reachability;

    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join(SETTINGS_FILE_NAME);
    let mut settings = Settings::load(&path).expect("defaults");
    assert_eq!(
        settings.reachability,
        Reachability {
            enabled: true,
            interval: 60,
            timeout: 2000,
            probes: 10,
        }
    );
    settings.reachability = Reachability {
        enabled: false,
        interval: 300,
        timeout: 500,
        probes: 4,
    };
    settings.save(&path).expect("save");
    assert_eq!(
        Settings::load(&path).expect("load").reachability,
        settings.reachability
    );
    // Out of the C# ranges: the default, as the C# load keeps it.
    let read = written(
        dir.path(),
        "version = 1\n[reachability]\ninterval = 5\ntimeout = 99999\nprobes = 0\n",
    );
    assert_eq!(read.reachability, Reachability::default());
}

#[test]
fn the_powershell_execution_policy_is_kept_by_its_csharp_name_and_powershell_s_own_by_default() {
    use heimdall_core::settings::ExecutionPolicy;

    let dir = tempfile::tempdir().expect("dir");
    assert_eq!(
        Settings::default().powershell_execution_policy,
        ExecutionPolicy::Default
    );
    for (name, read) in [
        ("bypass", ExecutionPolicy::Bypass),
        ("AllSigned", ExecutionPolicy::AllSigned),
        // A name not known, as the C# load: the default.
        ("Restricted", ExecutionPolicy::Default),
    ] {
        let text = format!("version = 1\n[terminal]\npowershell_execution_policy = \"{name}\"\n");
        assert_eq!(
            written(dir.path(), &text).powershell_execution_policy,
            read,
            "{name}"
        );
    }
    let path = dir.path().join(SETTINGS_FILE_NAME);
    let settings = Settings {
        powershell_execution_policy: ExecutionPolicy::RemoteSigned,
        ..Settings::default()
    };
    settings.save(&path).expect("save");
    assert_eq!(Settings::load(&path).expect("load"), settings, "read back");
}

#[test]
fn the_rdp_connection_timeout_is_kept_within_the_csharp_range_and_reset_with_rdp() {
    use heimdall_core::settings::{RDP_CONNECT_TIMEOUT_DEFAULT, rdp_connect_timeout_accepted};

    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join(SETTINGS_FILE_NAME);
    let mut settings = Settings::load(&path).expect("defaults");
    assert_eq!(settings.rdp_connect_timeout, 45, "the C# 45 000 ms");
    assert_eq!(RDP_CONNECT_TIMEOUT_DEFAULT, 45);
    for (seconds, accepted) in [(0, true), (4, false), (5, true), (600, true), (601, false)] {
        assert_eq!(rdp_connect_timeout_accepted(seconds), accepted, "{seconds}");
    }

    settings.rdp_connect_timeout = 0;
    settings.save(&path).expect("save");
    assert_eq!(
        Settings::load(&path).expect("load").rdp_connect_timeout,
        0,
        "off, kept"
    );

    // Edited by hand out of the range: the default.
    std::fs::write(&path, "version = 1\n[rdp_session]\nconnect_timeout = 3\n").expect("write");
    let mut read = Settings::load(&path).expect("load");
    assert_eq!(read.rdp_connect_timeout, RDP_CONNECT_TIMEOUT_DEFAULT);

    read.rdp_connect_timeout = 120;
    read.reset_rdp();
    assert_eq!(
        read.rdp_connect_timeout, RDP_CONNECT_TIMEOUT_DEFAULT,
        "reset with RDP"
    );
}
