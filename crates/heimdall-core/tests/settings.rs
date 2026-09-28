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
