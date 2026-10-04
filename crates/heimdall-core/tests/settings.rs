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
