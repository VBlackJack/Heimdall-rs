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

use heimdall_core::settings::{ColorScheme, SETTINGS_FILE_NAME, Settings, settings_path};
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
        }
        .save(&path)
        .expect("saved");
        assert_eq!(Settings::load(&path).expect("read").color_scheme, scheme);
    }
    let text = std::fs::read_to_string(&path).expect("text");
    assert!(text.contains("color_scheme = \"Nord\""), "{text}");
    Settings {
        color_scheme: ColorScheme::SolarizedDark,
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
