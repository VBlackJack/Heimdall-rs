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
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

use crate::credential_provider::{MAX_TIMEOUT, MIN_TIMEOUT, ProviderKind, ProviderSettings};
use crate::lockout::Lockout;
use crate::pin::PinHash;
use crate::profile::RdpDefaults;
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

/// Which terminals broadcast input reaches besides the one typed into.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BroadcastScope {
    /// Every terminal of every tab, asked before it starts.
    #[default]
    AllTabs,
    /// The tabs marked as targets.
    SelectedTabs,
}

impl BroadcastScope {
    /// The name the file holds.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::AllTabs => "AllTabs",
            Self::SelectedTabs => "SelectedTabs",
        }
    }

    /// The scope named `name`, all tabs for a name not known.
    #[must_use]
    pub fn named(name: &str) -> Self {
        if name.trim().eq_ignore_ascii_case(Self::SelectedTabs.name()) {
            Self::SelectedTabs
        } else {
            Self::AllTabs
        }
    }
}

/// Where transcripts go when no folder is chosen, beside the settings, as the C# one.
pub const DEFAULT_SESSION_LOG_DIRECTORY: &str = "logs/sessions";

/// What the Settings page changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    /// The terminal's colours.
    pub color_scheme: ColorScheme,
    /// Which terminals broadcast input reaches.
    pub broadcast_scope: BroadcastScope,
    /// Every SSH, Telnet and local session keeps a transcript from when it connects.
    pub session_logging: bool,
    /// Where transcripts go: a folder, or one relative to the settings file's.
    pub session_log_directory: String,
    /// Size of the terminals' text a new tab starts at, and Ctrl+0 comes back to, within
    /// [`TERMINAL_FONT_SIZE_MIN`] and [`TERMINAL_FONT_SIZE_MAX`].
    pub terminal_font_size: u16,
    /// The language chosen; `None` follows the desktop's.
    pub language: Option<Language>,
    /// Wrong master passwords in a row when the application starts, kept across runs as
    /// the C# startup gate keeps them: quitting does not give the tries back.
    pub vault_unlock: Lockout,
    /// The application PIN asked at start, as the C# one; `None` when none is set.
    pub pin: Option<PinHash>,
    /// Wrong PINs in a row, kept across runs as the master password's are.
    pub pin_unlock: Lockout,
    /// The external credential provider.
    pub credential_provider: ProviderSettings,
    /// An SSH session whose connection is lost opens again by itself, as the C#
    /// `SshAutoReconnect`: off unless chosen.
    pub ssh_auto_reconnect: bool,
    /// Attempts before the reconnect is left to the user, within
    /// [`SSH_AUTO_RECONNECT_ATTEMPTS_MIN`] and [`SSH_AUTO_RECONNECT_ATTEMPTS_MAX`].
    pub ssh_auto_reconnect_attempts: u32,
    /// The RDP options profiles following the application's take.
    pub rdp_defaults: RdpDefaults,
}

/// A language the application is written in, as the C# language list offers them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Language {
    /// English, the language every text is written in first.
    English,
    /// French.
    French,
    /// Spanish.
    Spanish,
}

impl Language {
    /// Every language, in the order of the C# list.
    pub const ALL: [Self; 3] = [Self::English, Self::French, Self::Spanish];

    /// Its code, as the C# `DefaultLocale` and the translation folders name it.
    #[must_use]
    pub fn code(self) -> &'static str {
        match self {
            Self::English => "en",
            Self::French => "fr",
            Self::Spanish => "es",
        }
    }

    /// The language of `code`, whatever its case; `None` for one not offered.
    #[must_use]
    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|language| language.code().eq_ignore_ascii_case(code.trim()))
    }
}

/// Size of the terminals' text unless chosen: the one this terminal was drawn at before.
pub const TERMINAL_FONT_SIZE_DEFAULT: u16 = 15;
/// Smallest terminal font size accepted, as the C# setting's range.
pub const TERMINAL_FONT_SIZE_MIN: u16 = 8;
/// Attempts of an SSH auto-reconnect by default, as the C# `SshAutoReconnectAttempts`.
pub const SSH_AUTO_RECONNECT_ATTEMPTS_DEFAULT: u32 = 3;

/// Fewest attempts of an SSH auto-reconnect accepted, as the C# setting's range.
pub const SSH_AUTO_RECONNECT_ATTEMPTS_MIN: u32 = 1;

/// Most attempts of an SSH auto-reconnect accepted, as the C# setting's range.
pub const SSH_AUTO_RECONNECT_ATTEMPTS_MAX: u32 = 10;

/// Whether `attempts` is a number of SSH auto-reconnect attempts the settings accept.
#[must_use]
pub fn ssh_auto_reconnect_attempts_accepted(attempts: u32) -> bool {
    (SSH_AUTO_RECONNECT_ATTEMPTS_MIN..=SSH_AUTO_RECONNECT_ATTEMPTS_MAX).contains(&attempts)
}

/// Largest terminal font size accepted, as the C# setting's range.
pub const TERMINAL_FONT_SIZE_MAX: u16 = 72;

/// Whether `size` is a terminal font size the settings accept.
#[must_use]
pub fn terminal_font_size_accepted(size: u16) -> bool {
    (TERMINAL_FONT_SIZE_MIN..=TERMINAL_FONT_SIZE_MAX).contains(&size)
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            color_scheme: ColorScheme::default(),
            broadcast_scope: BroadcastScope::default(),
            session_logging: false,
            session_log_directory: DEFAULT_SESSION_LOG_DIRECTORY.to_owned(),
            terminal_font_size: TERMINAL_FONT_SIZE_DEFAULT,
            language: None,
            vault_unlock: Lockout::default(),
            pin: None,
            pin_unlock: Lockout::default(),
            credential_provider: ProviderSettings::default(),
            ssh_auto_reconnect: false,
            ssh_auto_reconnect_attempts: SSH_AUTO_RECONNECT_ATTEMPTS_DEFAULT,
            rdp_defaults: RdpDefaults::default(),
        }
    }
}

#[derive(Serialize, Deserialize, Default)]
struct SettingsFile {
    version: u32,
    #[serde(default)]
    terminal: TerminalSection,
    #[serde(default)]
    session_log: SessionLogSection,
    #[serde(default)]
    general: GeneralSection,
    #[serde(default)]
    vault_unlock: VaultUnlockSection,
    #[serde(default)]
    pin: PinSection,
    #[serde(default)]
    credential_provider: ProviderSection,
    #[serde(default)]
    ssh: SshSection,
    #[serde(default)]
    rdp: RdpDefaults,
}

#[derive(Serialize, Deserialize, Default)]
struct SshSection {
    #[serde(default)]
    auto_reconnect: bool,
    #[serde(default)]
    auto_reconnect_attempts: Option<u32>,
}

#[derive(Serialize, Deserialize, Default)]
struct ProviderSection {
    #[serde(default)]
    enabled: bool,
    /// The C# name of the kind.
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    command: String,
    #[serde(default)]
    username_command: String,
    #[serde(default)]
    database: String,
    #[serde(default)]
    key_file: String,
    #[serde(default)]
    first_line_only: bool,
    #[serde(default)]
    timeout_ms: Option<u64>,
}

impl ProviderSection {
    fn settings(self) -> ProviderSettings {
        let defaults = ProviderSettings::default();
        ProviderSettings {
            enabled: self.enabled,
            kind: self
                .kind
                .as_deref()
                .map(ProviderKind::named)
                .unwrap_or_default(),
            command: self.command,
            username_command: self.username_command,
            database: self.database,
            key_file: self.key_file,
            first_line_only: self.first_line_only,
            // Out of the range, as the C# load keeps the default.
            timeout: self
                .timeout_ms
                .map(Duration::from_millis)
                .filter(|timeout| (MIN_TIMEOUT..=MAX_TIMEOUT).contains(timeout))
                .unwrap_or(defaults.timeout),
        }
    }

    fn of(settings: &ProviderSettings) -> Self {
        Self {
            enabled: settings.enabled,
            kind: Some(settings.kind.name().to_owned()),
            command: settings.command.clone(),
            username_command: settings.username_command.clone(),
            database: settings.database.clone(),
            key_file: settings.key_file.clone(),
            first_line_only: settings.first_line_only,
            timeout_ms: Some(u64::try_from(settings.timeout.as_millis()).unwrap_or(u64::MAX)),
        }
    }
}

#[derive(Serialize, Deserialize, Default)]
struct PinSection {
    /// Base64; with the hash, a PIN is set.
    #[serde(default)]
    salt: Option<String>,
    /// Base64 of the Argon2id hash.
    #[serde(default)]
    hash: Option<String>,
    #[serde(default)]
    failures: u32,
    /// Seconds since 1970, UTC.
    #[serde(default)]
    locked_until: Option<u64>,
}

#[derive(Serialize, Deserialize, Default)]
struct VaultUnlockSection {
    #[serde(default)]
    failures: u32,
    /// Seconds since 1970, UTC.
    #[serde(default)]
    locked_until: Option<u64>,
}

#[derive(Serialize, Deserialize, Default)]
struct GeneralSection {
    /// Written only once chosen, as TOML leaves an absent value out: until then the
    /// desktop's language is followed.
    #[serde(default)]
    language: Option<String>,
}

#[derive(Serialize, Deserialize, Default)]
struct SessionLogSection {
    #[serde(default)]
    enabled: bool,
    #[serde(default)]
    directory: Option<String>,
}

#[derive(Serialize, Deserialize, Default)]
struct TerminalSection {
    #[serde(default)]
    color_scheme: Option<String>,
    #[serde(default)]
    broadcast_scope: Option<String>,
    #[serde(default)]
    font_size: Option<u16>,
}

/// The instant `seconds` after 1970, UTC.
fn from_epoch(seconds: u64) -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(seconds)
}

/// Seconds from 1970, UTC, to `instant`; 0 for an instant before.
fn to_epoch(instant: SystemTime) -> u64 {
    instant
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
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
            broadcast_scope: file
                .terminal
                .broadcast_scope
                .as_deref()
                .map(BroadcastScope::named)
                .unwrap_or_default(),
            session_logging: file.session_log.enabled,
            session_log_directory: file
                .session_log
                .directory
                .filter(|directory| !directory.trim().is_empty())
                .unwrap_or_else(|| DEFAULT_SESSION_LOG_DIRECTORY.to_owned()),
            // Out of the range, as the C# load warns and keeps the default.
            terminal_font_size: file
                .terminal
                .font_size
                .filter(|size| terminal_font_size_accepted(*size))
                .unwrap_or(TERMINAL_FONT_SIZE_DEFAULT),
            vault_unlock: Lockout::restored(
                file.vault_unlock.failures,
                file.vault_unlock.locked_until.map(from_epoch),
                SystemTime::now(),
            ),
            // Half a PIN is still one: it takes nothing, and the application stays closed.
            pin: match (file.pin.salt, file.pin.hash) {
                (None, None) => None,
                (salt, hash) => Some(PinHash::saved(
                    salt.unwrap_or_default(),
                    hash.unwrap_or_default(),
                )),
            },
            pin_unlock: Lockout::restored(
                file.pin.failures,
                file.pin.locked_until.map(from_epoch),
                SystemTime::now(),
            ),
            credential_provider: file.credential_provider.settings(),
            ssh_auto_reconnect: file.ssh.auto_reconnect,
            // Out of the range, as the C# load warns and keeps the default.
            ssh_auto_reconnect_attempts: file
                .ssh
                .auto_reconnect_attempts
                .filter(|attempts| ssh_auto_reconnect_attempts_accepted(*attempts))
                .unwrap_or(SSH_AUTO_RECONNECT_ATTEMPTS_DEFAULT),
            rdp_defaults: file.rdp,
            // A language not offered is not guessed: the desktop's is followed.
            language: file
                .general
                .language
                .as_deref()
                .and_then(Language::from_code),
        })
    }

    /// The folder transcripts go to: the one chosen when absolute, else under the folder of
    /// `settings_file`.
    #[must_use]
    pub fn session_log_folder(&self, settings_file: &Path) -> PathBuf {
        // Rebuilt from its parts: the default "logs/sessions" takes the platform's separator.
        let chosen: PathBuf = Path::new(self.session_log_directory.trim())
            .components()
            .collect();
        if chosen.is_absolute() {
            return chosen;
        }
        settings_file
            .parent()
            .map_or_else(|| chosen.clone(), |base| base.join(&chosen))
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
                broadcast_scope: Some(self.broadcast_scope.name().to_owned()),
                font_size: Some(self.terminal_font_size),
            },
            session_log: SessionLogSection {
                enabled: self.session_logging,
                directory: Some(self.session_log_directory.clone()),
            },
            general: GeneralSection {
                language: self.language.map(|language| language.code().to_owned()),
            },
            vault_unlock: VaultUnlockSection {
                failures: self.vault_unlock.failures(),
                locked_until: self.vault_unlock.until().map(to_epoch),
            },
            pin: PinSection {
                salt: self.pin.as_ref().map(|pin| pin.salt().to_owned()),
                hash: self.pin.as_ref().map(|pin| pin.hash().to_owned()),
                failures: self.pin_unlock.failures(),
                locked_until: self.pin_unlock.until().map(to_epoch),
            },
            credential_provider: ProviderSection::of(&self.credential_provider),
            ssh: SshSection {
                auto_reconnect: self.ssh_auto_reconnect,
                auto_reconnect_attempts: Some(self.ssh_auto_reconnect_attempts),
            },
            rdp: self.rdp_defaults,
        })?;
        write_atomic(path, &text)
    }
}
