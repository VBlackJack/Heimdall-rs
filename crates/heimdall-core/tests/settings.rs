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
    Accent, AppTheme, BroadcastScope, ColorScheme, SETTINGS_FILE_NAME, Settings, settings_path,
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
fn the_theme_and_accent_are_drakul_and_its_own_until_chosen_then_kept_by_their_csharp_names() {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join(SETTINGS_FILE_NAME);
    let settings = Settings::load(&path).expect("defaults");
    assert_eq!(settings.theme, AppTheme::Drakul, "as the C# default");
    assert_eq!(settings.accent, Accent::Default);
    for theme in AppTheme::ALL {
        for accent in Accent::ALL {
            Settings {
                theme,
                accent,
                ..Settings::default()
            }
            .save(&path)
            .expect("saved");
            let read = Settings::load(&path).expect("read");
            assert_eq!((read.theme, read.accent), (theme, accent));
        }
    }
    let text = std::fs::read_to_string(&path).expect("text");
    assert!(text.contains("theme = \"HighContrast\""), "{text}");
    assert!(text.contains("accent = \"Yellow\""), "{text}");
    assert_eq!(
        AppTheme::ALL.map(AppTheme::name),
        [
            "Dracula",
            "Drakul",
            "Striga",
            "Cinder",
            "Bracken",
            "Tarn",
            "Mortis",
            "Slate",
            "Magellan",
            "Voivode",
            "Carmilla",
            "Whitby",
            "Vesper",
            "Parchment",
            "Folio",
            "Wormwood",
            "Sconce",
            "HighContrast"
        ]
    );
    assert_eq!(
        Accent::ALL.map(Accent::name),
        [
            "Default", "Blue", "Cyan", "Green", "Orange", "Pink", "Purple", "Red", "Yellow"
        ]
    );
}

#[test]
fn a_theme_or_accent_is_read_whatever_its_case_and_an_unknown_one_is_the_default() {
    let dir = tempfile::tempdir().expect("dir");
    let read = |text: &str| {
        let settings = written(dir.path(), text);
        (settings.theme, settings.accent)
    };
    assert_eq!(
        read("version = 1\n[general]\ntheme = \" parchment \"\naccent = \"CYAN\"\n"),
        (AppTheme::Parchment, Accent::Cyan)
    );
    assert_eq!(
        read("version = 1\n[general]\ntheme = \"Dracula\"\n"),
        (AppTheme::Dracula, Accent::Default),
        "the C# names its root palette Dracula too"
    );
    assert_eq!(
        read("version = 1\n[general]\ntheme = \"Alucard\"\naccent = \"Teal\"\n"),
        (AppTheme::Drakul, Accent::Default)
    );
    assert_eq!(
        read("version = 1\n"),
        (AppTheme::Drakul, Accent::Default),
        "no section"
    );
}

#[test]
fn the_theme_and_accent_travel_with_an_export() {
    let settings = Settings {
        theme: AppTheme::Folio,
        accent: Accent::Orange,
        ..Settings::default()
    };
    let (text, _) = settings.export(None, false);
    let read = Settings::default().import(&text).expect("read");
    assert_eq!(read.settings.theme, AppTheme::Folio);
    assert_eq!(read.settings.accent, Accent::Orange);
    let keys: Vec<&str> = read
        .changes
        .iter()
        .map(|change| change.key.as_str())
        .collect();
    assert!(keys.contains(&"general.theme"), "{keys:?}");
    assert!(keys.contains(&"general.accent"), "{keys:?}");
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
fn transcripts_are_kept_forever_by_default_and_their_retention_within_the_csharp_range() {
    use heimdall_core::settings::{
        SESSION_LOG_RETENTION_DAYS_DEFAULT, SESSION_LOG_RETENTION_DAYS_MAX,
        SESSION_LOG_RETENTION_DAYS_MIN, session_log_retention_days_accepted,
    };

    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join(SETTINGS_FILE_NAME);
    let mut settings = Settings::load(&path).expect("defaults");
    assert_eq!(settings.session_log_retention_days, 0, "the C# default");
    assert_eq!(SESSION_LOG_RETENTION_DAYS_DEFAULT, 0);
    assert_eq!(
        (
            SESSION_LOG_RETENTION_DAYS_MIN,
            SESSION_LOG_RETENTION_DAYS_MAX
        ),
        (7, 3650),
        "the C# range"
    );
    for (days, accepted) in [(0, true), (1, false), (6, false), (7, true), (3650, true)] {
        assert_eq!(
            session_log_retention_days_accepted(days),
            accepted,
            "{days}"
        );
    }
    assert!(!session_log_retention_days_accepted(3651));

    settings.session_log_retention_days = 30;
    settings.save(&path).expect("save");
    assert_eq!(
        Settings::load(&path)
            .expect("load")
            .session_log_retention_days,
        30
    );

    // A file written before the setting: every transcript kept.
    let older = written(
        dir.path(),
        "version = 1\n[session_log]\nenabled = true\ndirectory = \"transcripts\"\n",
    );
    assert!(older.session_logging);
    assert_eq!(older.session_log_retention_days, 0);
    // Edited by hand out of the range: the default.
    let out_of_range = written(
        dir.path(),
        "version = 1\n[session_log]\nretention_days = 3\n",
    );
    assert_eq!(out_of_range.session_log_retention_days, 0);
}

#[test]
fn the_broadcast_scope_is_the_current_tab_until_another_is_chosen() {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join(SETTINGS_FILE_NAME);
    assert_eq!(
        Settings::load(&path).expect("defaults").broadcast_scope,
        BroadcastScope::CurrentTab,
        "the C#'s default"
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
    assert_eq!(
        BroadcastScope::named("SelectedPanes"),
        BroadcastScope::SelectedTabs,
        "the C#'s name"
    );
    assert_eq!(BroadcastScope::named("alltabs"), BroadcastScope::AllTabs);
    assert_eq!(
        BroadcastScope::named("Everywhere"),
        BroadcastScope::CurrentTab,
        "a name not known: the narrowest"
    );
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
fn the_idle_auto_lock_is_off_by_default_and_kept_within_the_csharp_range() {
    use heimdall_core::settings::{
        AUTO_LOCK_IDLE_MINUTES_DEFAULT, AUTO_LOCK_IDLE_MINUTES_MAX, AUTO_LOCK_IDLE_MINUTES_OFF,
        auto_lock_idle_minutes_accepted,
    };

    let dir = tempfile::tempdir().expect("dir");
    let defaults = written(dir.path(), "version = 1\n");
    assert_eq!(
        defaults.auto_lock_idle_minutes, AUTO_LOCK_IDLE_MINUTES_OFF,
        "as the C# default"
    );
    assert!(!defaults.disconnect_on_lock, "survive and mask, as the C#");
    assert!(auto_lock_idle_minutes_accepted(AUTO_LOCK_IDLE_MINUTES_OFF));
    assert!(auto_lock_idle_minutes_accepted(AUTO_LOCK_IDLE_MINUTES_MAX));
    assert!(!auto_lock_idle_minutes_accepted(
        AUTO_LOCK_IDLE_MINUTES_MAX + 1
    ));
    for (minutes, read) in [
        (AUTO_LOCK_IDLE_MINUTES_OFF, AUTO_LOCK_IDLE_MINUTES_OFF),
        (AUTO_LOCK_IDLE_MINUTES_MAX, AUTO_LOCK_IDLE_MINUTES_MAX),
        (
            AUTO_LOCK_IDLE_MINUTES_MAX + 1,
            AUTO_LOCK_IDLE_MINUTES_DEFAULT,
        ),
    ] {
        let text = format!("version = 1\n[vault]\nauto_lock_idle_minutes = {minutes}\n");
        assert_eq!(
            written(dir.path(), &text).auto_lock_idle_minutes,
            read,
            "{minutes}"
        );
    }

    let path = dir.path().join(SETTINGS_FILE_NAME);
    Settings {
        auto_lock_idle_minutes: 15,
        disconnect_on_lock: true,
        ..Settings::default()
    }
    .save(&path)
    .expect("saved");
    let read = Settings::load(&path).expect("read");
    assert_eq!(read.auto_lock_idle_minutes, 15);
    assert!(read.disconnect_on_lock);
}

#[test]
fn the_workspace_lock_travels_with_an_export_without_the_lockout() {
    let mut settings = Settings {
        auto_lock_idle_minutes: 30,
        disconnect_on_lock: true,
        ..Settings::default()
    };
    settings.vault_unlock.register_failure(SystemTime::now());
    let (text, _) = settings.export(None, false);
    assert!(text.contains("auto_lock_idle_minutes = 30"), "{text}");
    assert!(!text.contains("vault_unlock"), "{text}");
    let read = Settings::default().import(&text).expect("read");
    assert_eq!(read.settings.auto_lock_idle_minutes, 30);
    assert!(read.settings.disconnect_on_lock);
    let keys: Vec<&str> = read
        .changes
        .iter()
        .map(|change| change.key.as_str())
        .collect();
    assert_eq!(
        keys,
        ["vault.auto_lock_idle_minutes", "vault.disconnect_on_lock"]
    );
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
fn the_terminal_font_family_is_the_embedded_one_until_chosen_and_kept_as_named() {
    use heimdall_core::settings::TERMINAL_FONT_FAMILY_DEFAULT;

    let dir = tempfile::tempdir().expect("dir");
    assert_eq!(TERMINAL_FONT_FAMILY_DEFAULT, "Source Code Pro");
    assert_eq!(
        Settings::default().terminal_font_family,
        TERMINAL_FONT_FAMILY_DEFAULT
    );
    // A file written before the family could be chosen.
    let older = written(dir.path(), "version = 1\n[terminal]\nfont_size = 15\n");
    assert_eq!(older.terminal_font_family, TERMINAL_FONT_FAMILY_DEFAULT);
    for (named, read) in [
        ("Consolas", "Consolas"),
        ("  Cascadia Mono ", "Cascadia Mono"),
        ("Not Installed Anywhere", "Not Installed Anywhere"),
        ("   ", TERMINAL_FONT_FAMILY_DEFAULT),
    ] {
        let text = format!("version = 1\n[terminal]\nfont_family = \"{named}\"\n");
        assert_eq!(
            written(dir.path(), &text).terminal_font_family,
            read,
            "{named:?}"
        );
    }
    let path = dir.path().join(SETTINGS_FILE_NAME);
    Settings {
        terminal_font_family: "Consolas".to_owned(),
        ..Settings::default()
    }
    .save(&path)
    .expect("saved");
    assert_eq!(
        Settings::load(&path).expect("read").terminal_font_family,
        "Consolas"
    );
}

#[test]
fn the_gateway_badge_is_shown_until_hidden_and_an_older_file_shows_it() {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join(SETTINGS_FILE_NAME);
    let mut settings = Settings::load(&path).expect("defaults");
    assert!(settings.show_gateway_badge, "as the C# default");
    assert!(
        written(dir.path(), "version = 1\n[general]\nprevent_sleep = true\n").show_gateway_badge,
        "a file written before the choice was kept"
    );
    settings.show_gateway_badge = false;
    settings.save(&path).expect("save");
    assert!(!Settings::load(&path).expect("load").show_gateway_badge);
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
fn update_checks_are_on_daily_kept_within_the_csharp_range_and_their_state_stays_here() {
    use heimdall_core::settings::{UpdateCheck, Updates, update_interval_accepted};

    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join(SETTINGS_FILE_NAME);
    let mut settings = Settings::load(&path).expect("defaults");
    assert_eq!(
        settings.updates,
        Updates {
            enabled: true,
            interval_hours: 24,
        },
        "the C# defaults"
    );
    assert_eq!(settings.update_check, UpdateCheck::default());
    for (hours, accepted) in [(0, false), (1, true), (8760, true), (8761, false)] {
        assert_eq!(update_interval_accepted(hours), accepted, "{hours}");
    }
    let checked = SystemTime::UNIX_EPOCH + Duration::from_secs(1_791_000_123);
    settings.updates = Updates {
        enabled: false,
        interval_hours: 168,
    };
    settings.update_check = UpdateCheck {
        last_check: Some(checked),
        skipped: Some("v2026.100901".to_owned()),
    };
    settings.save(&path).expect("save");
    assert_eq!(Settings::load(&path).expect("load"), settings, "read back");
    // Out of the C# range: the default, as the C# load keeps it.
    let read = written(
        dir.path(),
        "version = 1
[updates]
interval_hours = 9000
",
    );
    assert_eq!(read.updates, Updates::default());
    // The preferences travel; when this computer last looked, and what it skips, do not.
    let (text, _) = settings.export(None, false);
    assert!(text.contains("interval_hours = 168"), "{text}");
    assert!(!text.contains("update_check"), "{text}");
    assert!(!text.contains("v2026.100901"), "{text}");
    let imported = Settings::default().import(&text).expect("read");
    assert_eq!(imported.settings.updates, settings.updates);
    assert_eq!(imported.settings.update_check, UpdateCheck::default());
    let keys: Vec<&str> = imported
        .changes
        .iter()
        .map(|change| change.key.as_str())
        .collect();
    assert_eq!(keys, ["updates.enabled", "updates.interval_hours"]);
}

#[test]
fn windows_hello_is_off_with_the_csharp_grace_kept_within_its_range_and_carried() {
    use heimdall_core::settings::{WindowsHello, windows_hello_grace_minutes_accepted};

    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join(SETTINGS_FILE_NAME);
    let mut settings = Settings::load(&path).expect("defaults");
    assert_eq!(
        settings.windows_hello,
        WindowsHello {
            require_on_connect: false,
            grace_minutes: 5,
            vault_max_days: 0,
        },
        "the C# defaults"
    );
    for (minutes, accepted) in [(0, true), (1440, true), (1441, false)] {
        assert_eq!(
            windows_hello_grace_minutes_accepted(minutes),
            accepted,
            "{minutes}"
        );
    }
    settings.windows_hello = WindowsHello {
        require_on_connect: true,
        grace_minutes: 0,
        ..WindowsHello::default()
    };
    settings.save(&path).expect("save");
    assert_eq!(Settings::load(&path).expect("load"), settings, "read back");
    // Out of the C# range: the default, as the C# load keeps it.
    let read = written(
        dir.path(),
        "version = 1
[windows_hello]
require_on_connect = true
grace_minutes = 5000
",
    );
    assert_eq!(
        read.windows_hello,
        WindowsHello {
            require_on_connect: true,
            ..WindowsHello::default()
        }
    );
    // A preference: it travels with the others.
    let (text, _) = settings.export(None, false);
    assert!(text.contains("require_on_connect = true"), "{text}");
    let imported = Settings::default().import(&text).expect("read");
    assert_eq!(imported.settings.windows_hello, settings.windows_hello);
    let keys: Vec<&str> = imported
        .changes
        .iter()
        .map(|change| change.key.as_str())
        .filter(|key| key.starts_with("windows_hello."))
        .collect();
    assert_eq!(
        keys,
        [
            "windows_hello.grace_minutes",
            "windows_hello.require_on_connect"
        ]
    );
}

#[test]
fn the_vault_s_windows_hello_days_travel_within_the_csharp_range_and_the_last_master_unlock_stays()
{
    use heimdall_core::settings::{WindowsHello, windows_hello_vault_max_days_accepted};

    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join(SETTINGS_FILE_NAME);
    let mut settings = Settings::load(&path).expect("defaults");
    assert_eq!(settings.windows_hello.vault_max_days, 0, "never, as the C#");
    assert_eq!(settings.vault_last_master_unlock, None);
    for (days, accepted) in [(0, true), (3650, true), (3651, false)] {
        assert_eq!(
            windows_hello_vault_max_days_accepted(days),
            accepted,
            "{days}"
        );
    }
    let unlocked =
        std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
    settings.windows_hello.vault_max_days = 30;
    settings.vault_last_master_unlock = Some(unlocked);
    settings.save(&path).expect("save");
    assert_eq!(Settings::load(&path).expect("load"), settings, "read back");
    // Out of the C# range: the default, as the C# load keeps it.
    let read = written(
        dir.path(),
        "version = 1
[windows_hello]
vault_max_days = 4000
",
    );
    assert_eq!(read.windows_hello, WindowsHello::default());
    // The days are a preference and travel; when the master password was last typed is this
    // computer's, and does not.
    let (text, _) = settings.export(None, false);
    assert!(text.contains("vault_max_days = 30"), "{text}");
    assert!(!text.contains("last_master_unlock"), "{text}");
    let imported = Settings::default().import(&text).expect("read");
    assert_eq!(imported.settings.windows_hello.vault_max_days, 30);
    assert_eq!(imported.settings.vault_last_master_unlock, None);
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

#[test]
fn the_sessions_limit_is_none_by_default_and_kept_within_the_csharp_range() {
    use heimdall_core::settings::{MAX_SESSIONS_MAX, max_sessions_accepted};

    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join(SETTINGS_FILE_NAME);
    let mut settings = Settings::load(&path).expect("defaults");
    assert_eq!(settings.max_sessions, 0, "no limit");
    assert!(max_sessions_accepted(0) && max_sessions_accepted(MAX_SESSIONS_MAX));
    assert!(!max_sessions_accepted(MAX_SESSIONS_MAX + 1));
    settings.max_sessions = 5;
    settings.save(&path).expect("save");
    assert_eq!(Settings::load(&path).expect("load").max_sessions, 5);
    std::fs::write(&path, "version = 1\n[general]\nmax_sessions = 99\n").expect("write");
    assert_eq!(
        Settings::load(&path).expect("load").max_sessions,
        0,
        "out of range"
    );
}

#[test]
fn ctrl_v_pastes_outside_full_screen_programs_by_default_and_is_kept() {
    use heimdall_core::settings::CtrlVPaste;

    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join(SETTINGS_FILE_NAME);
    let mut settings = Settings::load(&path).expect("defaults");
    assert_eq!(settings.ctrl_v_paste, CtrlVPaste::OutsideFullScreenPrograms);
    assert!(settings.ctrl_v_paste.pastes(false));
    assert!(!settings.ctrl_v_paste.pastes(true), "vim's ^V");
    assert!(CtrlVPaste::Always.pastes(true));
    assert!(!CtrlVPaste::Never.pastes(false));

    settings.ctrl_v_paste = CtrlVPaste::Never;
    settings.save(&path).expect("save");
    assert_eq!(
        Settings::load(&path).expect("load").ctrl_v_paste,
        CtrlVPaste::Never
    );
    assert_eq!(
        CtrlVPaste::named("bogus"),
        CtrlVPaste::OutsideFullScreenPrograms
    );
}

#[test]
fn ctrl_k_opens_quick_connect_by_default_and_is_kept_and_carried() {
    use heimdall_core::settings::CtrlKTerminal;

    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join(SETTINGS_FILE_NAME);
    let mut settings = Settings::load(&path).expect("defaults");
    assert_eq!(settings.ctrl_k_terminal, CtrlKTerminal::QuickConnect);
    assert!(settings.ctrl_k_terminal.opens_quick_connect(), "as the C#");
    assert!(!CtrlKTerminal::SendToSession.opens_quick_connect());

    settings.ctrl_k_terminal = CtrlKTerminal::SendToSession;
    settings.save(&path).expect("save");
    assert_eq!(
        Settings::load(&path).expect("load").ctrl_k_terminal,
        CtrlKTerminal::SendToSession
    );
    assert_eq!(CtrlKTerminal::named("bogus"), CtrlKTerminal::QuickConnect);

    // Carried by an export, as every preference.
    let (text, _) = settings.export(None, false);
    assert!(text.contains("ctrl_k = \"send-to-session\""), "{text}");
    let read = Settings::default().import(&text).expect("read");
    assert_eq!(read.settings.ctrl_k_terminal, CtrlKTerminal::SendToSession);
    assert!(
        read.changes
            .iter()
            .any(|change| change.key == "terminal.ctrl_k"),
        "{:?}",
        read.changes
    );
}

#[test]
fn the_sftp_browser_opens_beside_ssh_by_default_and_an_older_file_keeps_the_csharp_defaults() {
    use heimdall_core::settings::SftpBrowser;

    let dir = tempfile::tempdir().expect("dir");
    let defaults = SftpBrowser::default();
    assert!(defaults.enabled, "as the C# SftpBrowserEnabled");
    assert!(defaults.auto_open_on_ssh, "as the C# SftpAutoOpenOnSsh");
    assert!(
        !defaults.follow_ssh_directory,
        "as the C# SftpFollowSshDirectory"
    );
    assert!(defaults.auto_opens());
    // Written before these settings were: its files section holds the editor alone.
    let older = written(
        dir.path(),
        "version = 1\n[files]\nexternal_editor = \"notepad.exe\"\n",
    );
    assert_eq!(older.sftp_browser, defaults);
    assert_eq!(older.external_editor, "notepad.exe");
    let none = written(dir.path(), "version = 1\n");
    assert_eq!(none.sftp_browser, defaults, "no files section at all");
}

#[test]
fn a_local_shell_docks_its_file_browser_by_default_and_an_older_file_keeps_it() {
    use heimdall_core::settings::SftpBrowser;

    let dir = tempfile::tempdir().expect("dir");
    assert!(
        SftpBrowser::default().dock_local_browser,
        "as the C# always docks it"
    );
    // Written before the setting was: the browser still docks.
    let older = written(
        dir.path(),
        "version = 1\n[files]\nbrowser_enabled = false\nauto_open_on_ssh = false\n",
    );
    assert!(older.sftp_browser.dock_local_browser);
    assert!(
        !older.sftp_browser.enabled,
        "apart from the SFTP browser's own"
    );
    let off = written(
        dir.path(),
        "version = 1\n[files]\ndock_local_browser = false\n",
    );
    assert!(!off.sftp_browser.dock_local_browser);
    assert!(
        off.sftp_browser.auto_opens(),
        "the SFTP pane left as it was"
    );
}

#[test]
fn the_local_browser_follows_its_shell_by_default_and_an_older_file_keeps_it() {
    use heimdall_core::settings::SftpBrowser;

    let dir = tempfile::tempdir().expect("dir");
    assert!(
        SftpBrowser::default().follow_local_directory,
        "nothing is typed into the shell, and the browser starts in its folder"
    );
    // Written before the setting was: the browser follows.
    let older = written(
        dir.path(),
        "version = 1\n[files]\ndock_local_browser = true\nfollow_ssh_directory = false\n",
    );
    assert!(older.sftp_browser.follow_local_directory);
    let off = written(
        dir.path(),
        "version = 1\n[files]\nfollow_local_directory = false\n",
    );
    assert!(!off.sftp_browser.follow_local_directory);
    assert_eq!(
        off.sftp_browser,
        SftpBrowser {
            follow_local_directory: false,
            ..SftpBrowser::default()
        },
        "apart from the SFTP pane's own following"
    );
}

#[test]
fn the_sftp_browser_settings_are_kept_and_the_auto_open_needs_the_browser() {
    use heimdall_core::settings::SftpBrowser;

    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join(SETTINGS_FILE_NAME);
    let chosen = SftpBrowser {
        enabled: false,
        auto_open_on_ssh: true,
        follow_ssh_directory: true,
        dock_local_browser: false,
        follow_local_directory: false,
    };
    assert!(
        !chosen.auto_opens(),
        "under the browser, as the C# checkbox"
    );
    Settings {
        sftp_browser: chosen,
        ..Settings::default()
    }
    .save(&path)
    .expect("saved");
    let text = std::fs::read_to_string(&path).expect("text");
    assert!(text.contains("browser_enabled = false"), "{text}");
    assert!(text.contains("dock_local_browser = false"), "{text}");
    assert!(text.contains("follow_local_directory = false"), "{text}");
    assert_eq!(Settings::load(&path).expect("read").sftp_browser, chosen);
    let off = written(
        dir.path(),
        "version = 1\n[files]\nauto_open_on_ssh = false\n",
    );
    assert!(off.sftp_browser.enabled && !off.sftp_browser.auto_opens());
}

#[test]
fn putty_and_the_x_server_are_looked_for_and_started_by_default_and_kept_as_chosen() {
    let dir = tempfile::tempdir().expect("dir");
    let defaults = Settings::default();
    assert!(defaults.putty_path.is_empty(), "PATH is searched");
    assert!(defaults.x11_server_path.is_empty(), "the known places");
    assert!(defaults.x11_auto_start, "as the C# X11AutoStart");
    // Written before these settings were: the C# defaults.
    let older = written(dir.path(), "version = 1\n[ssh]\nauto_reconnect = true\n");
    assert!(older.putty_path.is_empty() && older.x11_server_path.is_empty());
    assert!(older.x11_auto_start);
    let path = dir.path().join(SETTINGS_FILE_NAME);
    let chosen = Settings {
        putty_path: r"C:\Tools\putty.exe".to_owned(),
        x11_server_path: r"D:\X\vcxsrv.exe".to_owned(),
        x11_auto_start: false,
        ..Settings::default()
    };
    chosen.save(&path).expect("saved");
    let read = Settings::load(&path).expect("read");
    assert_eq!(read.putty_path, chosen.putty_path);
    assert_eq!(read.x11_server_path, chosen.x11_server_path);
    assert!(!read.x11_auto_start);
    // Typed with spaces around: trimmed when read.
    let padded = written(
        dir.path(),
        "version = 1\n[ssh]\nputty_path = \"  /usr/bin/putty \"\nx11_auto_start = true\n",
    );
    assert_eq!(padded.putty_path, "/usr/bin/putty");
    // They travel with an export, beside the other SSH settings.
    let (text, _) = chosen.export(None, false);
    let imported = Settings::default().import(&text).expect("read");
    assert_eq!(imported.settings.putty_path, chosen.putty_path);
    assert!(!imported.settings.x11_auto_start);
    let keys: Vec<&str> = imported
        .changes
        .iter()
        .map(|change| change.key.as_str())
        .collect();
    for key in [
        "ssh.putty_path",
        "ssh.x11_server_path",
        "ssh.x11_auto_start",
    ] {
        assert!(keys.contains(&key), "{keys:?}");
    }
}

#[test]
fn the_default_ssh_mode_is_embedded_kept_by_its_csharp_name_and_travels() {
    use heimdall_core::profile::SshMode;

    let dir = tempfile::tempdir().expect("dir");
    assert_eq!(Settings::default().ssh_default_mode, SshMode::Embedded);
    // Written before it was: the C# default.
    let older = written(dir.path(), "version = 1\n[ssh]\nauto_reconnect = true\n");
    assert_eq!(older.ssh_default_mode, SshMode::Embedded);
    let path = dir.path().join(SETTINGS_FILE_NAME);
    let chosen = Settings {
        ssh_default_mode: SshMode::External,
        ..Settings::default()
    };
    chosen.save(&path).expect("saved");
    let text = std::fs::read_to_string(&path).expect("text");
    assert!(text.contains("default_mode = \"External\""), "{text}");
    assert_eq!(
        Settings::load(&path).expect("read").ssh_default_mode,
        SshMode::External
    );
    // Whatever its case; a name not known is the default.
    let cased = written(
        dir.path(),
        "version = 1\n[ssh]\ndefault_mode = \"external\"\n",
    );
    assert_eq!(cased.ssh_default_mode, SshMode::External);
    let unknown = written(
        dir.path(),
        "version = 1\n[ssh]\ndefault_mode = \"Inline\"\n",
    );
    assert_eq!(unknown.ssh_default_mode, SshMode::Embedded);
    // It travels with an export.
    let (exported, _) = chosen.export(None, false);
    let imported = Settings::default().import(&exported).expect("read");
    assert_eq!(imported.settings.ssh_default_mode, SshMode::External);
    assert!(
        imported
            .changes
            .iter()
            .any(|change| change.key == "ssh.default_mode")
    );
}

#[test]
fn the_last_gateway_used_is_kept_on_this_computer_and_never_exported_nor_imported() {
    use heimdall_core::profile::ProfileId;

    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join(SETTINGS_FILE_NAME);
    let mut settings = Settings::load(&path).expect("defaults");
    assert_eq!(
        settings.last_used_gateway, None,
        "none until a profile is saved"
    );
    settings.last_used_gateway = Some(ProfileId::new("gw-bastion"));
    settings.save(&path).expect("save");
    assert_eq!(Settings::load(&path).expect("load"), settings, "read back");
    // Blank in a file edited by hand: none.
    let read = written(
        dir.path(),
        "version = 1
[profile_form]
last_used_gateway = \"  \"
",
    );
    assert_eq!(read.last_used_gateway, None);

    // As the C# settings transfer leaves `LastUsedGatewayId` behind: the gateways do not
    // travel, so neither does the one last used.
    let (text, _) = settings.export(None, true);
    assert!(!text.contains("gw-bastion"), "{text}");
    assert!(!text.contains("profile_form"), "{text}");
    let forged = format!("{text}\n[settings.profile_form]\nlast_used_gateway = \"gw-elsewhere\"\n");
    let imported = settings.import(&forged).expect("read");
    assert_eq!(
        imported.settings.last_used_gateway,
        Some(ProfileId::new("gw-bastion")),
        "a file cannot set it"
    );
    assert!(imported.changes.is_empty(), "{:?}", imported.changes);
}

#[test]
fn reset_all_puts_every_preference_back_and_keeps_what_the_csharp_keeps() {
    use heimdall_core::credential_provider::ProviderSettings;
    use heimdall_core::profile::ProfileId;
    use heimdall_core::settings::{Language, UpdateCheck};

    let now = SystemTime::now();
    let mut settings = Settings {
        // Kept: the language, the theme and the accent, as the C#.
        language: Some(Language::French),
        theme: AppTheme::Tarn,
        accent: Accent::Orange,
        // Kept: state, not preferences.
        pin: Some(PinHash::new("2468").expect("pin")),
        vault_last_master_unlock: Some(now),
        update_check: UpdateCheck {
            last_check: Some(now),
            skipped: Some("v2026.100901".to_owned()),
        },
        last_used_gateway: Some(ProfileId::new("gw")),
        // Kept: no C# Settings panel edits them.
        broadcast_scope: BroadcastScope::AllTabs,
        show_gateway_badge: false,
        // Reset: preferences of every tab.
        color_scheme: ColorScheme::Nord,
        session_logging: true,
        ssh_keep_alive_interval: 45,
        putty_path: "C:/Tools/putty.exe".to_owned(),
        external_editor: "C:/Tools/edit.exe".to_owned(),
        prevent_sleep: false,
        auto_lock_idle_minutes: 15,
        disconnect_on_lock: true,
        credential_provider: ProviderSettings {
            enabled: true,
            command: "pass show {Title}".to_owned(),
            ..ProviderSettings::default()
        },
        ..Settings::default()
    };
    settings.windows_hello.require_on_connect = true;
    settings.windows_hello.grace_minutes = 30;
    settings.rdp_defaults.compression = !settings.rdp_defaults.compression;
    settings.pin_unlock.register_failure(now);
    settings.vault_unlock.register_failure(now);
    let before = settings.clone();

    settings.reset_all();

    let kept = Settings {
        language: before.language,
        theme: before.theme,
        accent: before.accent,
        pin: before.pin.clone(),
        pin_unlock: before.pin_unlock,
        vault_unlock: before.vault_unlock,
        vault_last_master_unlock: before.vault_last_master_unlock,
        update_check: before.update_check.clone(),
        last_used_gateway: before.last_used_gateway.clone(),
        broadcast_scope: before.broadcast_scope,
        show_gateway_badge: before.show_gateway_badge,
        ..Settings::default()
    };
    assert_eq!(settings, kept);
    assert_eq!(settings.pin_unlock.failures(), 1, "the wrong tries stay");
    assert!(!settings.credential_provider.enabled, "the provider is off");
    assert!(!settings.windows_hello.require_on_connect);
}

#[test]
fn the_default_rdp_mode_is_embedded_kept_by_its_csharp_name_reset_and_travels() {
    use heimdall_core::profile::RdpMode;

    let dir = tempfile::tempdir().expect("dir");
    assert_eq!(Settings::default().rdp_default_mode, RdpMode::Embedded);
    // Written before it was: the C# default.
    let older = written(
        dir.path(),
        "version = 1\n[rdp_session]\nconnect_timeout = 30\n",
    );
    assert_eq!(older.rdp_default_mode, RdpMode::Embedded);
    let path = dir.path().join(SETTINGS_FILE_NAME);
    let chosen = Settings {
        rdp_default_mode: RdpMode::External,
        ..Settings::default()
    };
    chosen.save(&path).expect("saved");
    let text = std::fs::read_to_string(&path).expect("text");
    assert!(
        text.contains("[rdp_session]") && text.contains("default_mode = \"External\""),
        "{text}"
    );
    let read = Settings::load(&path).expect("read");
    assert_eq!(read.rdp_default_mode, RdpMode::External);
    assert_eq!(
        read.ssh_default_mode,
        Settings::default().ssh_default_mode,
        "its own"
    );
    // Whatever its case; a name not known is the default.
    let cased = written(
        dir.path(),
        "version = 1\n[rdp_session]\ndefault_mode = \"external\"\n",
    );
    assert_eq!(cased.rdp_default_mode, RdpMode::External);
    let unknown = written(
        dir.path(),
        "version = 1\n[rdp_session]\ndefault_mode = \"Inline\"\n",
    );
    assert_eq!(unknown.rdp_default_mode, RdpMode::Embedded);
    // "Reset RDP defaults" puts it back, as the C# `ApplyRdpDefaults`.
    let mut reset = chosen.clone();
    reset.reset_rdp();
    assert_eq!(reset.rdp_default_mode, RdpMode::Embedded);
    // It travels with an export.
    let (exported, _) = chosen.export(None, false);
    let imported = Settings::default().import(&exported).expect("read");
    assert_eq!(imported.settings.rdp_default_mode, RdpMode::External);
    assert!(
        imported
            .changes
            .iter()
            .any(|change| change.key == "rdp_session.default_mode")
    );
}

#[test]
fn credential_guard_is_not_required_by_default_kept_carried_and_reset_with_everything_only() {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join(SETTINGS_FILE_NAME);
    let mut settings = Settings::load(&path).expect("defaults");
    assert!(!settings.require_credential_guard, "off, as the C# default");

    settings.require_credential_guard = true;
    settings.save(&path).expect("save");
    let text = std::fs::read_to_string(&path).expect("read");
    assert!(
        text.contains("[rdp_session]") && text.contains("require_credential_guard = true"),
        "{text}"
    );
    assert!(
        Settings::load(&path)
            .expect("load")
            .require_credential_guard
    );
    // An older file, without the key: the C# default.
    let read = written(
        dir.path(),
        "version = 1\n[rdp_session]\nconnect_timeout = 30\n",
    );
    assert!(!read.require_credential_guard);

    // A preference: it travels with the others, as the C# `RequireCredentialGuard`.
    let (exported, _) = settings.export(None, false);
    assert!(
        exported.contains("require_credential_guard = true"),
        "{exported}"
    );
    let imported = Settings::default().import(&exported).expect("read");
    assert!(imported.settings.require_credential_guard);
    assert!(
        imported
            .changes
            .iter()
            .any(|change| change.key == "rdp_session.require_credential_guard"),
        "said among the changes"
    );

    // "Reset RDP defaults" leaves it, as the C# one; "Reset defaults" turns it off.
    settings.reset_rdp();
    assert!(settings.require_credential_guard);
    settings.reset_all();
    assert!(!settings.require_credential_guard);
}

#[test]
fn the_rdp_resize_delay_is_kept_within_the_csharp_range_exported_and_reset_with_rdp() {
    use heimdall_core::settings::{
        RDP_RESIZE_ENABLE_DELAY_DEFAULT_MS, rdp_resize_enable_delay_accepted,
    };

    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join(SETTINGS_FILE_NAME);
    let mut settings = Settings::load(&path).expect("defaults");
    assert_eq!(
        settings.rdp_resize_enable_delay_ms, 10_000,
        "the C# 10 000 ms"
    );
    assert_eq!(RDP_RESIZE_ENABLE_DELAY_DEFAULT_MS, 10_000);
    for (ms, accepted) in [
        (0, true),
        (1, false),
        (999, false),
        (1_000, true),
        (60_000, true),
        (60_001, false),
    ] {
        assert_eq!(rdp_resize_enable_delay_accepted(ms), accepted, "{ms}");
    }

    settings.rdp_resize_enable_delay_ms = 0;
    settings.save(&path).expect("save");
    assert_eq!(
        Settings::load(&path)
            .expect("load")
            .rdp_resize_enable_delay_ms,
        0,
        "off, kept"
    );

    // Edited by hand out of the range: the default.
    std::fs::write(
        &path,
        "version = 1\n[rdp_session]\nresize_enable_delay_ms = 500\n",
    )
    .expect("write");
    let mut read = Settings::load(&path).expect("load");
    assert_eq!(
        read.rdp_resize_enable_delay_ms,
        RDP_RESIZE_ENABLE_DELAY_DEFAULT_MS
    );

    // In a portable settings file, as the C# exports it.
    read.rdp_resize_enable_delay_ms = 4_000;
    let (text, _) = read.export(None, false);
    assert!(text.contains("resize_enable_delay_ms = 4000"), "{text}");
    let imported = Settings::default().import(&text).expect("import");
    assert_eq!(imported.settings.rdp_resize_enable_delay_ms, 4_000);
    assert!(
        imported
            .changes
            .iter()
            .any(|change| change.key == "rdp_session.resize_enable_delay_ms"),
        "{:?}",
        imported.changes
    );

    read.reset_rdp();
    assert_eq!(
        read.rdp_resize_enable_delay_ms, RDP_RESIZE_ENABLE_DELAY_DEFAULT_MS,
        "reset with RDP, as the C# `ApplyRdpDefaults`"
    );
}

#[test]
fn the_wait_after_connecting_is_the_profile_s_else_the_settings_and_never_out_of_range() {
    use heimdall_core::settings::rdp_resize_enable_delay;

    let ms = Duration::from_millis;
    assert_eq!(
        rdp_resize_enable_delay(Some(2_000), 10_000),
        ms(2_000),
        "own"
    );
    assert_eq!(rdp_resize_enable_delay(Some(0), 10_000), ms(0), "own off");
    assert_eq!(rdp_resize_enable_delay(None, 30_000), ms(30_000), "global");
    assert_eq!(rdp_resize_enable_delay(None, 0), ms(0), "global off");
    assert_eq!(
        rdp_resize_enable_delay(Some(500), 30_000),
        ms(30_000),
        "an own value out of the range is not taken"
    );
    assert_eq!(
        rdp_resize_enable_delay(Some(u32::MAX), 0),
        ms(0),
        "nor one past it"
    );
    assert_eq!(
        rdp_resize_enable_delay(None, 70_000),
        ms(10_000),
        "a global value out of the range is the default"
    );
}

#[test]
fn the_known_hosts_import_at_startup_is_off_by_default_kept_carried_and_reset() {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join(SETTINGS_FILE_NAME);
    let mut settings = Settings::load(&path).expect("defaults");
    assert!(
        !settings.sync_known_hosts_at_startup,
        "off, as the C# default"
    );

    settings.sync_known_hosts_at_startup = true;
    settings.save(&path).expect("save");
    let text = std::fs::read_to_string(&path).expect("read");
    assert!(
        text.contains("[ssh]") && text.contains("sync_known_hosts_at_startup = true"),
        "{text}"
    );
    assert!(
        Settings::load(&path)
            .expect("load")
            .sync_known_hosts_at_startup
    );
    // An older file, without the key: the C# default.
    let read = written(dir.path(), "version = 1\n[ssh]\nauto_reconnect = true\n");
    assert!(!read.sync_known_hosts_at_startup);

    // A preference: it travels with the others, as the C# `SyncKnownHostsAtStartup`.
    let (exported, _) = settings.export(None, false);
    assert!(
        exported.contains("sync_known_hosts_at_startup = true"),
        "{exported}"
    );
    let imported = Settings::default().import(&exported).expect("read");
    assert!(imported.settings.sync_known_hosts_at_startup);
    assert!(
        imported
            .changes
            .iter()
            .any(|change| change.key == "ssh.sync_known_hosts_at_startup"),
        "said among the changes"
    );

    // "Reset RDP defaults" leaves it; "Reset defaults" turns it off.
    settings.reset_rdp();
    assert!(settings.sync_known_hosts_at_startup);
    settings.reset_all();
    assert!(!settings.sync_known_hosts_at_startup);
}
