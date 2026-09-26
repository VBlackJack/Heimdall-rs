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

//! Reads the translations of the C# Heimdall, so a ported feature reuses them.
//!
//! The C# Heimdall keeps one flat JSON object per language, in `locales/<language>.json`
//! at the root of its repository.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use heimdall_i18n::SUPPORTED_LANGUAGES;
use serde_json::{Map, Value};

/// Environment variable naming the root of a C# Heimdall checkout.
pub const LEGACY_REPO_VARIABLE: &str = "HEIMDALL_CS_REPO";

/// Directory of the locale files inside the C# repository.
const LEGACY_LOCALES_DIR: &str = "locales";

/// Extension of a C# locale file.
const LEGACY_LOCALE_EXTENSION: &str = "json";

/// Value of one C# key in one language; `None` when that language lacks the key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Translation {
    /// Language code.
    pub language: &'static str,
    /// Translated text.
    pub value: Option<String>,
}

/// Path of the C# locale file for `language`.
#[must_use]
pub fn locale_path(legacy_repo: &Path, language: &str) -> PathBuf {
    legacy_repo
        .join(LEGACY_LOCALES_DIR)
        .join(language)
        .with_extension(LEGACY_LOCALE_EXTENSION)
}

/// Looks `key` up in every supported language of the C# repository at `legacy_repo`.
///
/// # Errors
///
/// Returns an error when a locale file is unreadable or is not a JSON object.
pub fn lookup(legacy_repo: &Path, key: &str) -> io::Result<Vec<Translation>> {
    SUPPORTED_LANGUAGES
        .iter()
        .map(|language| {
            let table = read_table(&locale_path(legacy_repo, language))?;
            Ok(Translation {
                language,
                value: table.get(key).and_then(Value::as_str).map(str::to_owned),
            })
        })
        .collect()
}

fn read_table(path: &Path) -> io::Result<Map<String, Value>> {
    let text = fs::read_to_string(path)?;
    match serde_json::from_str(&text)? {
        Value::Object(table) => Ok(table),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            path.display().to_string(),
        )),
    }
}
