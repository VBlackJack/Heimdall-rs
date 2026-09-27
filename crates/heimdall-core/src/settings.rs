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

//! The settings of Heimdall-rs, beside its profiles: what the Settings page changes.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::store::{StoreError, write_atomic};

/// Name of the settings file, beside the profile file.
pub const SETTINGS_FILE_NAME: &str = "settings.toml";

/// Format version written into the settings file.
pub const SETTINGS_FILE_VERSION: u32 = 1;

/// The terminal's colours, as the C# Heimdall names its schemes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ColorScheme {
    /// The terminal's own: black, white text, the Tango colours.
    Standard,
    /// Dracula, the Heimdall design system's.
    #[default]
    Dracula,
    /// Solarized Dark.
    SolarizedDark,
    /// Monokai.
    Monokai,
    /// Nord.
    Nord,
}

impl ColorScheme {
    /// Every scheme, in the order the C# Settings page lists them.
    pub const ALL: [Self; 5] = [
        Self::Standard,
        Self::Dracula,
        Self::SolarizedDark,
        Self::Monokai,
        Self::Nord,
    ];

    /// The name the file holds, the C# one.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Standard => "Default",
            Self::Dracula => "Dracula",
            Self::SolarizedDark => "Solarized Dark",
            Self::Monokai => "Monokai",
            Self::Nord => "Nord",
        }
    }

    /// The scheme named `name`, whatever its case, as the C# Heimdall reads it; Dracula
    /// for a name it does not know.
    #[must_use]
    pub fn named(name: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|scheme| scheme.name().eq_ignore_ascii_case(name.trim()))
            .unwrap_or_default()
    }
}

/// What the Settings page changes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Settings {
    /// The terminal's colours.
    pub color_scheme: ColorScheme,
}

#[derive(Serialize, Deserialize, Default)]
struct SettingsFile {
    version: u32,
    #[serde(default)]
    terminal: TerminalSection,
}

#[derive(Serialize, Deserialize, Default)]
struct TerminalSection {
    #[serde(default)]
    color_scheme: Option<String>,
}

/// The settings file of the profile file `profiles_file`: beside it.
#[must_use]
pub fn settings_path(profiles_file: &Path) -> PathBuf {
    profiles_file.with_file_name(SETTINGS_FILE_NAME)
}

impl Settings {
    /// Reads the settings at `path`; a missing file holds the defaults.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the file exists and cannot be read, does not parse, or has
    /// a newer format version.
    pub fn load(path: &Path) -> Result<Self, StoreError> {
        let text = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(source) => {
                return Err(StoreError::Io {
                    path: path.to_owned(),
                    source,
                });
            }
        };
        let file: SettingsFile = toml::from_str(&text).map_err(|source| StoreError::Parse {
            path: path.to_owned(),
            source,
        })?;
        if file.version > SETTINGS_FILE_VERSION {
            return Err(StoreError::UnsupportedVersion {
                path: path.to_owned(),
                found: file.version,
                expected: SETTINGS_FILE_VERSION,
            });
        }
        Ok(Self {
            color_scheme: file
                .terminal
                .color_scheme
                .as_deref()
                .map(ColorScheme::named)
                .unwrap_or_default(),
        })
    }

    /// Writes the settings to `path`, through a temporary file.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when serialisation or any file operation fails.
    pub fn save(&self, path: &Path) -> Result<(), StoreError> {
        let text = toml::to_string_pretty(&SettingsFile {
            version: SETTINGS_FILE_VERSION,
            terminal: TerminalSection {
                color_scheme: Some(self.color_scheme.name().to_owned()),
            },
        })?;
        write_atomic(path, &text)
    }
}
