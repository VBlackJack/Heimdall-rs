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
use crate::profile::{
    RESOLUTION_PRESETS, RdpDefaults, preset_fits, resolution_preset, resolution_text,
};
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

/// Which SSH agent's keys are offered first, or alone, as the C# `SshAgentPreference`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AgentPreference {
    /// Every agent running, the Windows OpenSSH agent's keys first.
    #[default]
    OpenSshFirst,
    /// Every agent running, Pageant's keys first.
    PageantFirst,
    /// The Windows OpenSSH agent alone.
    OpenSshOnly,
    /// Pageant alone.
    PageantOnly,
}

impl AgentPreference {
    /// Every preference, in the order of the C# list.
    pub const ALL: [Self; 4] = [
        Self::OpenSshFirst,
        Self::PageantFirst,
        Self::OpenSshOnly,
        Self::PageantOnly,
    ];

    /// The name the file holds: the C# one.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::OpenSshFirst => "AutoOpenSshFirst",
            Self::PageantFirst => "AutoPageantFirst",
            Self::OpenSshOnly => "OpenSshOnly",
            Self::PageantOnly => "PageantOnly",
        }
    }

    /// The preference named `name`; the default for a name not known, as the C# load.
    #[must_use]
    pub fn named(name: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|preference| preference.name().eq_ignore_ascii_case(name.trim()))
            .unwrap_or_default()
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
    /// Attempts of an RDP desktop's auto-reconnect, as the C# `RdpAutoReconnectMaxAttempts`:
    /// within [`RDP_AUTO_RECONNECT_ATTEMPTS_MIN`] and [`RDP_AUTO_RECONNECT_ATTEMPTS_MAX`].
    pub rdp_auto_reconnect_attempts: u32,
    /// The sizes RDP tabs' Resolution menus offer, as the C# `RdpResolutionPresets`; empty
    /// offers [`RESOLUTION_PRESETS`]. See [`Settings::resolution_presets`].
    pub rdp_resolution_presets: Vec<(u16, u16)>,
    /// Seconds between two anti-idle keys of an RDP session whose profile asks for them, as
    /// the C# `AntiIdleIntervalSeconds`: 0 turns them off, else within
    /// [`ANTI_IDLE_INTERVAL_MIN`] and [`ANTI_IDLE_INTERVAL_MAX`].
    pub anti_idle_interval: u32,
    /// Seconds between two SSH keep-alives of every connection (shells, files, tunnels,
    /// gateways), as the C# `SshKeepAliveIntervalSeconds`: within
    /// [`SSH_KEEP_ALIVE_INTERVAL_MIN`] and [`SSH_KEEP_ALIVE_INTERVAL_MAX`].
    pub ssh_keep_alive_interval: u32,
    /// Seconds of no input after which an SSH shell is sent a bare carriage return, so a
    /// remote `TMOUT` does not log the user out, as the C# `SshTmoutResetIntervalSeconds`: 0
    /// turns it off, at most [`SSH_TMOUT_RESET_INTERVAL_MAX`].
    pub ssh_tmout_reset_interval: u32,
    /// The RDP options profiles following the application's take.
    pub rdp_defaults: RdpDefaults,
    /// The program a server's file is edited with, as the C# `ExternalEditorPath`; empty
    /// takes the system's own text editor.
    pub external_editor: String,
    /// Which SSH agent's keys are offered first, or alone; applied to the next connection.
    pub ssh_agent_preference: AgentPreference,
    /// The tunnels panel starts collapsed, as the C# `CollapseTunnelsPanelByDefault`: on.
    pub collapse_tunnels_panel: bool,
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

/// Most attempts of an RDP auto-reconnect, and their number by default, as the C#
/// `DefaultRdpAutoReconnectMaxAttempts`.
pub const RDP_AUTO_RECONNECT_ATTEMPTS_MAX: u32 = 20;

/// Fewest attempts of an RDP auto-reconnect accepted, as the C# setting's range.
pub const RDP_AUTO_RECONNECT_ATTEMPTS_MIN: u32 = 1;

/// Whether `attempts` is a number of RDP auto-reconnect attempts the settings accept.
#[must_use]
pub fn rdp_auto_reconnect_attempts_accepted(attempts: u32) -> bool {
    (RDP_AUTO_RECONNECT_ATTEMPTS_MIN..=RDP_AUTO_RECONNECT_ATTEMPTS_MAX).contains(&attempts)
}

/// `value` read from the file when `accepted`; `default` when absent or out of the range,
/// as the C# load warns and keeps the default.
fn within(value: Option<u32>, accepted: fn(u32) -> bool, default: u32) -> u32 {
    value.filter(|value| accepted(*value)).unwrap_or(default)
}

/// Whether `attempts` is a number of SSH auto-reconnect attempts the settings accept.
#[must_use]
pub fn ssh_auto_reconnect_attempts_accepted(attempts: u32) -> bool {
    (SSH_AUTO_RECONNECT_ATTEMPTS_MIN..=SSH_AUTO_RECONNECT_ATTEMPTS_MAX).contains(&attempts)
}

/// Seconds between two anti-idle keys by default, as the C# `AntiIdleIntervalSeconds`.
pub const ANTI_IDLE_INTERVAL_DEFAULT: u32 = 60;

/// Shortest anti-idle interval accepted, in seconds, as the C# setting's range.
pub const ANTI_IDLE_INTERVAL_MIN: u32 = 10;

/// Longest anti-idle interval accepted, in seconds, as the C# setting's range.
pub const ANTI_IDLE_INTERVAL_MAX: u32 = 3600;

/// Whether `seconds` is an anti-idle interval the settings accept: 0, which turns it off, or
/// one within the C# range.
#[must_use]
pub fn anti_idle_interval_accepted(seconds: u32) -> bool {
    seconds == 0 || (ANTI_IDLE_INTERVAL_MIN..=ANTI_IDLE_INTERVAL_MAX).contains(&seconds)
}

/// Seconds between two SSH keep-alives by default, as the C#
/// `DefaultSshKeepAliveIntervalSeconds`.
pub const SSH_KEEP_ALIVE_INTERVAL_DEFAULT: u32 = 30;

/// Shortest SSH keep-alive interval accepted, in seconds, as the C# setting's range.
pub const SSH_KEEP_ALIVE_INTERVAL_MIN: u32 = 5;

/// Longest SSH keep-alive interval accepted, in seconds, as the C# setting's range.
pub const SSH_KEEP_ALIVE_INTERVAL_MAX: u32 = 600;

/// Whether `seconds` is an SSH keep-alive interval the settings accept.
#[must_use]
pub fn ssh_keep_alive_interval_accepted(seconds: u32) -> bool {
    (SSH_KEEP_ALIVE_INTERVAL_MIN..=SSH_KEEP_ALIVE_INTERVAL_MAX).contains(&seconds)
}

/// Seconds of no input before a TMOUT reset by default, as the C#
/// `DefaultSshTmoutResetIntervalSeconds`.
pub const SSH_TMOUT_RESET_INTERVAL_DEFAULT: u32 = 240;

/// Longest TMOUT reset interval accepted, in seconds, as the C# setting's range.
pub const SSH_TMOUT_RESET_INTERVAL_MAX: u32 = 3600;

/// Whether `seconds` is a TMOUT reset interval the settings accept: 0, which turns it off,
/// up to [`SSH_TMOUT_RESET_INTERVAL_MAX`].
#[must_use]
pub fn ssh_tmout_reset_interval_accepted(seconds: u32) -> bool {
    seconds <= SSH_TMOUT_RESET_INTERVAL_MAX
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
            rdp_auto_reconnect_attempts: RDP_AUTO_RECONNECT_ATTEMPTS_MAX,
            rdp_resolution_presets: RESOLUTION_PRESETS.to_vec(),
            anti_idle_interval: ANTI_IDLE_INTERVAL_DEFAULT,
            ssh_keep_alive_interval: SSH_KEEP_ALIVE_INTERVAL_DEFAULT,
            ssh_tmout_reset_interval: SSH_TMOUT_RESET_INTERVAL_DEFAULT,
            rdp_defaults: RdpDefaults::default(),
            external_editor: String::new(),
            ssh_agent_preference: AgentPreference::default(),
            collapse_tunnels_panel: true,
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
    #[serde(default)]
    rdp_session: RdpSessionSection,
    #[serde(default)]
    files: FilesSection,
}

#[derive(Serialize, Deserialize, Default)]
struct RdpSessionSection {
    #[serde(default)]
    auto_reconnect_attempts: Option<u32>,
    /// One `WIDTHxHEIGHT` per preset.
    #[serde(default)]
    resolution_presets: Option<Vec<String>>,
}

#[derive(Serialize, Deserialize, Default)]
struct FilesSection {
    #[serde(default)]
    external_editor: String,
}

#[derive(Serialize, Deserialize, Default)]
struct SshSection {
    #[serde(default)]
    auto_reconnect: bool,
    #[serde(default)]
    auto_reconnect_attempts: Option<u32>,
    /// Kept beside the SSH settings, where the C# Settings page shows it.
    #[serde(default)]
    anti_idle_interval: Option<u32>,
    #[serde(default)]
    keep_alive_interval: Option<u32>,
    #[serde(default)]
    tmout_reset_interval: Option<u32>,
    /// The C# name of the agent preference.
    #[serde(default)]
    agent_preference: Option<String>,
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
    /// Absent is the C# default: collapsed.
    #[serde(default)]
    collapse_tunnels_panel: Option<bool>,
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
    /// The sizes the Resolution menus offer: the presets chosen, else the built-in ones, as
    /// the C# `ResolutionPresetCatalog`.
    #[must_use]
    pub fn resolution_presets(&self) -> &[(u16, u16)] {
        if self.rdp_resolution_presets.is_empty() {
            &RESOLUTION_PRESETS
        } else {
            &self.rdp_resolution_presets
        }
    }

    /// Whether `presets` can be the Resolution menus' presets: each within the limits.
    #[must_use]
    pub fn resolution_presets_accepted(presets: &[(u16, u16)]) -> bool {
        presets.iter().copied().all(preset_fits)
    }

    /// The RDP settings back to their own values, as the C# "Reset RDP defaults": the options
    /// profiles following the application take, the auto-reconnect attempts and the
    /// resolution presets. Nothing else changes, and no profile.
    pub fn reset_rdp(&mut self) {
        let defaults = Self::default();
        self.rdp_defaults = defaults.rdp_defaults;
        self.rdp_auto_reconnect_attempts = defaults.rdp_auto_reconnect_attempts;
        self.rdp_resolution_presets = defaults.rdp_resolution_presets;
    }

    /// Reads the settings at `path`; a missing file holds the defaults.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the file exists and cannot be read, does not parse, or has
    /// a newer format version.
    #[expect(clippy::too_many_lines, reason = "one field of the file per setting")]
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
            rdp_auto_reconnect_attempts: within(
                file.rdp_session.auto_reconnect_attempts,
                rdp_auto_reconnect_attempts_accepted,
                RDP_AUTO_RECONNECT_ATTEMPTS_MAX,
            ),
            // A line that is not a preset is left out, as the C# menu leaves it out.
            rdp_resolution_presets: file.rdp_session.resolution_presets.map_or_else(
                || RESOLUTION_PRESETS.to_vec(),
                |lines| {
                    lines
                        .iter()
                        .filter_map(|line| resolution_preset(line))
                        .collect()
                },
            ),
            ssh_auto_reconnect_attempts: within(
                file.ssh.auto_reconnect_attempts,
                ssh_auto_reconnect_attempts_accepted,
                SSH_AUTO_RECONNECT_ATTEMPTS_DEFAULT,
            ),
            anti_idle_interval: within(
                file.ssh.anti_idle_interval,
                anti_idle_interval_accepted,
                ANTI_IDLE_INTERVAL_DEFAULT,
            ),
            ssh_keep_alive_interval: within(
                file.ssh.keep_alive_interval,
                ssh_keep_alive_interval_accepted,
                SSH_KEEP_ALIVE_INTERVAL_DEFAULT,
            ),
            ssh_tmout_reset_interval: within(
                file.ssh.tmout_reset_interval,
                ssh_tmout_reset_interval_accepted,
                SSH_TMOUT_RESET_INTERVAL_DEFAULT,
            ),
            ssh_agent_preference: file
                .ssh
                .agent_preference
                .as_deref()
                .map(AgentPreference::named)
                .unwrap_or_default(),
            rdp_defaults: file.rdp,
            external_editor: file.files.external_editor.trim().to_owned(),
            collapse_tunnels_panel: file.general.collapse_tunnels_panel.unwrap_or(true),
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
                collapse_tunnels_panel: Some(self.collapse_tunnels_panel),
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
                anti_idle_interval: Some(self.anti_idle_interval),
                keep_alive_interval: Some(self.ssh_keep_alive_interval),
                tmout_reset_interval: Some(self.ssh_tmout_reset_interval),
                agent_preference: Some(self.ssh_agent_preference.name().to_owned()),
            },
            rdp: self.rdp_defaults,
            rdp_session: RdpSessionSection {
                auto_reconnect_attempts: Some(self.rdp_auto_reconnect_attempts),
                resolution_presets: Some(
                    self.rdp_resolution_presets
                        .iter()
                        .copied()
                        .map(resolution_text)
                        .collect(),
                ),
            },
            files: FilesSection {
                external_editor: self.external_editor.clone(),
            },
        })?;
        write_atomic(path, &text)
    }
}
