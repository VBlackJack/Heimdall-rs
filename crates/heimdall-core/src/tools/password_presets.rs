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

//! What the Password Generator keeps between runs, as the C# `PasswordPreset`
//! (`PasswordGeneratorViewModel.cs:40-98`) and `PasswordPresetStorage`
//! (`PasswordPresetStorage.cs:35-178`): the presets the user named, whether the tool reopens
//! where it was left, and where that was, in `password-presets.json`.
//!
//! The file is the C#'s JSON, its property names and its defaults, so a preset written by
//! the C# reads here. It lives beside the profiles, in the configuration folder restricted to
//! the user on Windows, written with mode 0600 on Unix. The C# also seals it with the
//! vault's or the Windows account's key; that sealing is not ported, and a sealed C# file is
//! not read (the tool starts on its defaults, as the C# does on a file it cannot unseal).

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::password_rules::DEFAULT_CASE_BLOCKS;
use super::private_file;

/// The file's name, as the C# `PresetsFileName`.
pub const PRESETS_FILE_NAME: &str = "password-presets.json";

/// The byte order mark a file may start with, as the C# `ByteOrderMark`.
const BYTE_ORDER_MARK: char = '\u{feff}';

/// The default length of a random password, as the C#'s 24.
pub const DEFAULT_LENGTH: i32 = 24;

/// The default length of a syllable password, as the C#'s 16.
pub const DEFAULT_SYLLABLE_LENGTH: i32 = 16;

/// The default word count, as the C#'s 4.
pub const DEFAULT_WORD_COUNT: i32 = 4;

/// The default separator of a passphrase, as the C# `DefaultPassphraseSeparator`.
pub const DEFAULT_PASSPHRASE_SEPARATOR: &str = "-";

fn yes() -> bool {
    true
}
fn default_length() -> i32 {
    DEFAULT_LENGTH
}
fn default_syllable_length() -> i32 {
    DEFAULT_SYLLABLE_LENGTH
}
fn two() -> i32 {
    2
}
fn one() -> i32 {
    1
}
fn not_written() -> i32 {
    -1
}
fn default_word_count() -> i32 {
    DEFAULT_WORD_COUNT
}
fn default_separator() -> String {
    DEFAULT_PASSPHRASE_SEPARATOR.to_owned()
}
fn default_case_blocks() -> String {
    DEFAULT_CASE_BLOCKS.to_owned()
}

/// A preset, every setting of the tool under a name, as the C# `PasswordPreset`: its
/// properties named and defaulted as the C# writes and reads them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[expect(
    clippy::struct_excessive_bools,
    reason = "the C# preset's shape, one switch per box of the tool"
)]
pub struct PasswordPreset {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub mode: i32,
    #[serde(default = "default_length")]
    pub length: i32,
    #[serde(default = "yes")]
    pub upper: bool,
    #[serde(default = "yes")]
    pub lower: bool,
    #[serde(default = "yes")]
    pub digits: bool,
    #[serde(default)]
    pub symbols: bool,
    #[serde(default)]
    pub layout_safe: bool,
    #[serde(default)]
    pub exclude_ambiguous: bool,
    #[serde(default)]
    pub cli_safe: bool,
    #[serde(default)]
    pub custom_specials: String,
    #[serde(default = "default_syllable_length")]
    pub syl_length: i32,
    #[serde(default)]
    pub syl_case: i32,
    #[serde(default = "two")]
    pub syl_digits: i32,
    #[serde(default = "one")]
    pub syl_specials: i32,
    #[serde(default)]
    pub syl_placement: i32,
    #[serde(default)]
    pub syl_separator: String,
    #[serde(default)]
    pub syl_cvc: bool,
    /// Written since the length covers the digits and the specials; a file written before
    /// carries false, and the counts are added back to the length.
    #[serde(default)]
    pub syl_length_includes_extras: bool,
    #[serde(default = "default_word_count")]
    pub pp_word_count: i32,
    #[serde(default = "default_separator")]
    pub pp_separator: String,
    #[serde(default)]
    pub pp_language: i32,
    #[serde(default = "yes")]
    pub pp_capitalize: bool,
    #[serde(default = "yes")]
    pub pp_digit: bool,
    #[serde(default = "yes")]
    pub pp_special: bool,
    /// Counts, written since the passphrase took more than one of each; -1 when absent.
    #[serde(default = "not_written")]
    pub pp_digits: i32,
    #[serde(default = "not_written")]
    pub pp_specials: i32,
    #[serde(default = "not_written")]
    pub pp_case: i32,
    #[serde(default)]
    pub pp_placement: i32,
    #[serde(default)]
    pub leet_base_word: String,
    #[serde(default = "yes")]
    pub leet_random_word: bool,
    #[serde(default = "yes")]
    pub leet_full_substitution: bool,
    #[serde(default = "two")]
    pub leet_digits: i32,
    #[serde(default = "one")]
    pub leet_specials: i32,
    #[serde(default)]
    pub leet_placement: i32,
    #[serde(default)]
    pub leet_case: i32,
    #[serde(default)]
    pub entropy_floor: i32,
    #[serde(default = "default_case_blocks")]
    pub case_blocks: String,
    #[serde(default = "yes")]
    pub case_blocks_auto_sync: bool,
    #[serde(default)]
    pub digit_positions: String,
    #[serde(default)]
    pub special_positions: String,
    #[serde(default = "one")]
    pub batch_count: i32,
}

impl Default for PasswordPreset {
    /// A preset with the C#'s defaults, as `new PasswordPreset()`.
    fn default() -> Self {
        serde_json::from_str("{}").expect("every property has a default")
    }
}

/// Everything kept between runs, as the C# `PasswordGeneratorStore`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct PasswordGeneratorStore {
    /// The presets named.
    #[serde(default)]
    pub presets: Vec<PasswordPreset>,
    /// Whether the tool reopens where it was left.
    #[serde(default)]
    pub remember_settings: bool,
    /// Where it was left, written only while [`Self::remember_settings`] is on.
    #[serde(default)]
    pub settings: Option<PasswordPreset>,
}

/// The file of the presets beside the profiles file `profiles_file`.
#[must_use]
pub fn presets_path(profiles_file: &Path) -> PathBuf {
    profiles_file.with_file_name(PRESETS_FILE_NAME)
}

/// The store read from `path`, as the C# `Load`: a file missing, empty, sealed or unreadable
/// gives an empty store; a bare array of presets, the shape before the settings were kept,
/// is read as the presets.
#[must_use]
pub fn load(path: &Path) -> PasswordGeneratorStore {
    let Ok(raw) = fs::read_to_string(path) else {
        return PasswordGeneratorStore::default();
    };
    let raw = raw.trim_start_matches(BYTE_ORDER_MARK).trim();
    if raw.starts_with('[') {
        return serde_json::from_str::<Vec<PasswordPreset>>(raw)
            .map(|presets| PasswordGeneratorStore {
                presets,
                ..PasswordGeneratorStore::default()
            })
            .unwrap_or_default();
    }
    if raw.starts_with('{') {
        return serde_json::from_str(raw).unwrap_or_default();
    }
    // Empty, or sealed by the C#: nothing this tool can read.
    PasswordGeneratorStore::default()
}

/// Writes `store` at `path`, indented as the C# writes it, readable by the user alone.
///
/// # Errors
///
/// The encoding's or the write's error.
pub fn save(path: &Path, store: &PasswordGeneratorStore) -> io::Result<()> {
    if let Some(folder) = path
        .parent()
        .filter(|folder| !folder.as_os_str().is_empty())
    {
        fs::create_dir_all(folder)?;
    }
    let json = serde_json::to_string_pretty(store).map_err(io::Error::other)?;
    private_file::write_private(path, json.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_preset_written_by_the_csharp_reads_with_its_defaults() {
        let preset: PasswordPreset =
            serde_json::from_str(r#"{"Name":"Mine","Mode":2,"PpWordCount":6}"#).expect("read");
        assert_eq!(preset.name, "Mine");
        assert_eq!(preset.mode, 2);
        assert_eq!(preset.pp_word_count, 6);
        assert_eq!(preset.length, 24);
        assert_eq!(preset.pp_digits, -1, "not written: the flags decide");
        assert_eq!(preset.case_blocks, "Tl");
        assert!(preset.upper && !preset.symbols);
        assert_eq!(preset.batch_count, 1);
        assert_eq!(PasswordPreset::default().syl_length, 16);
    }

    #[test]
    fn the_store_round_trips_and_the_older_array_reads_as_its_presets() {
        let dir = tempfile::tempdir().expect("dir");
        let path = presets_path(&dir.path().join("profiles.toml"));
        assert_eq!(load(&path), PasswordGeneratorStore::default(), "missing");
        let store = PasswordGeneratorStore {
            presets: vec![PasswordPreset {
                name: "A".to_owned(),
                ..PasswordPreset::default()
            }],
            remember_settings: true,
            settings: Some(PasswordPreset::default()),
        };
        save(&path, &store).expect("saved");
        let written = fs::read_to_string(&path).expect("read");
        assert!(written.contains("\"RememberSettings\": true"), "{written}");
        assert_eq!(load(&path), store);
        fs::write(&path, "\u{feff}[{\"Name\":\"Old\"}]").expect("written");
        let old = load(&path);
        assert_eq!(old.presets.len(), 1);
        assert_eq!(old.presets[0].name, "Old");
        assert!(!old.remember_settings);
        fs::write(&path, "AQAAANCMnd8BFdERjHoAwE").expect("written");
        assert_eq!(load(&path), PasswordGeneratorStore::default(), "sealed");
    }
}
