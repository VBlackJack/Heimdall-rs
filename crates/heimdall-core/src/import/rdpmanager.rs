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

//! Migration from the legacy PowerShell Heimdall, `RDPManager`, as the C# `MigrationService`.
//!
//! Its folder is found as the C# start finds it (`App.xaml.cs:1253-1290`): walking up from
//! the program's folder, each folder's parent's `RDPManager`, then the folder's own, holding
//! `config\settings.json` and `config\servers.json`.
//!
//! Each legacy object is written in its `PascalCase` keys. The keys the C# `MapLegacyServer` and
//! `MapLegacyGateway` read are taken, with the same value checks (a key of the wrong kind is
//! ignored, a number that is no 32-bit integer leaves the profile out), turned into the
//! `camelCase` of the C# session document, and given to [`super::csharp::import`]: a profile is
//! then imported, left out or reported exactly as an imported C# file's.
//!
//! Unlike the C#, a folder found there can have been put there by someone else, on a computer
//! shared: nothing that runs, nothing that is trusted and nothing secret is taken from it. No
//! program path (`PuTTY`, `plink`, `psftp`, the external editor), no log path, no transcript
//! folder, no PIN, no HMAC key, no DPAPI user, no password and no passphrase, encrypted or
//! not, and no SSH host key fingerprint; passwords are asked again. The C# projects have no
//! equivalent: they are counted and left out.

use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use super::csharp::{self, ImportError, ImportReport, SkipReason};
use crate::profile::{AudioPlayback, ColorDepth, RdpMode, SshMode};
use crate::settings::{
    ANTI_IDLE_INTERVAL_DEFAULT, AppTheme, Language, MAX_SESSIONS_DEFAULT,
    RDP_CONNECT_TIMEOUT_DEFAULT, SSH_TMOUT_RESET_INTERVAL_DEFAULT, Settings,
    anti_idle_interval_accepted, max_sessions_accepted, rdp_connect_timeout_accepted,
    ssh_tmout_reset_interval_accepted,
};

/// The folder of the legacy application, as the C# `AppConstants.LegacyAppFolderName`.
pub const LEGACY_APP_FOLDER_NAME: &str = "RDPManager";

/// The folder of its configuration, as the C# `AppConstants.BundledConfigDirectoryName`.
pub const CONFIG_DIRECTORY_NAME: &str = "config";

/// Its settings.
pub const SETTINGS_FILE_NAME: &str = "settings.json";

/// Its servers.
pub const SERVERS_FILE_NAME: &str = "servers.json";

/// Largest legacy file read: a larger one is not the legacy application's.
pub const MAX_FILE_BYTES: u64 = 32 * 1024 * 1024;

/// Longest name a profile left out is reported under, in UTF-16 code units as the C#
/// `MaxWarningIdentityLength` counts them.
pub const MAX_IDENTITY_LENGTH: usize = 64;

/// The C# key of a profile's display name.
const DISPLAY_NAME_KEY: &str = "DisplayName";

/// The C# key of the gateways in the settings.
const GATEWAYS_KEY: &str = "SshGateways";

/// The C# key of the projects in the settings.
const PROJECTS_KEY: &str = "Projects";

/// The `camelCase` keys of the session document [`super::csharp::import`] reads.
const SERVERS_DOCUMENT_GATEWAYS_KEY: &str = "sshGateways";

/// Milliseconds in a second, for the RDP connection timeout the C# keeps in milliseconds.
const MILLIS_PER_SECOND: u64 = 1000;

/// Separators of a locale's language from its region, as `fr-FR` or `fr_FR`.
const LOCALE_SEPARATORS: [char; 2] = ['-', '_'];

/// How the C# reads a key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// `MapString` and `MapNullableString`: a string, anything else ignored.
    Text,
    /// `MapBool`: `true` or `false`, anything else ignored.
    Flag,
    /// `MapInt`: a number, which must be a 32-bit integer; anything else ignored.
    Integer,
    /// A key file, read as `Text`, and taken only when [`is_plain_local_path`]: never one
    /// that would reach another machine or a device when read.
    LocalPath,
}

/// The keys of a legacy server the C# `MapLegacyServer` reads, its secrets left out.
const SERVER_KEYS: [(&str, Kind); 45] = [
    ("Id", Kind::Text),
    (DISPLAY_NAME_KEY, Kind::Text),
    ("RemoteServer", Kind::Text),
    ("RemotePort", Kind::Integer),
    ("LocalPort", Kind::Integer),
    ("Group", Kind::Text),
    ("SshGatewayId", Kind::Text),
    ("UseDirectConnection", Kind::Flag),
    ("ProjectId", Kind::Text),
    ("ConnectionType", Kind::Text),
    ("RdpUsername", Kind::Text),
    ("SshUsername", Kind::Text),
    ("SshPort", Kind::Integer),
    ("SshMode", Kind::Text),
    ("SshAgentForwarding", Kind::Flag),
    ("SshKeyPath", Kind::LocalPath),
    ("SshCompression", Kind::Flag),
    ("SshX11Forwarding", Kind::Flag),
    ("RdpAntiIdle", Kind::Flag),
    ("RdpAspectRatio", Kind::Text),
    ("IsFavorite", Kind::Flag),
    ("SortOrder", Kind::Integer),
    ("Tags", Kind::Text),
    ("RdpMode", Kind::Text),
    ("RdpUseGlobalDefaults", Kind::Flag),
    ("RdpRedirectClipboard", Kind::Flag),
    ("RdpRedirectDrives", Kind::Flag),
    ("RdpRedirectPrinters", Kind::Flag),
    ("RdpRedirectComPorts", Kind::Flag),
    ("RdpRedirectSmartCards", Kind::Flag),
    ("RdpRedirectWebcam", Kind::Flag),
    ("RdpRedirectUsb", Kind::Flag),
    ("RdpAudioMode", Kind::Integer),
    ("RdpAudioCapture", Kind::Flag),
    ("RdpMultiMonitor", Kind::Flag),
    ("RdpDynamicResolution", Kind::Flag),
    ("RdpNla", Kind::Flag),
    ("RdpColorDepth", Kind::Integer),
    ("RdpBitmapCaching", Kind::Flag),
    ("RdpCompression", Kind::Flag),
    ("RdpHardwareAcceleration", Kind::Flag),
    ("RdpAutoReconnect", Kind::Flag),
    ("RdpGateway", Kind::Text),
    ("Environment", Kind::Text),
    ("MacAddress", Kind::Text),
];

/// The keys of a legacy gateway the C# `MapLegacyGateway` reads, its secrets and its host
/// key fingerprint left out: a server's identity is learnt on its first connection, never
/// from a folder anyone could have written.
const GATEWAY_KEYS: [(&str, Kind); 8] = [
    ("Id", Kind::Text),
    ("Name", Kind::Text),
    ("Host", Kind::Text),
    ("Port", Kind::Integer),
    ("User", Kind::Text),
    ("KeyPath", Kind::LocalPath),
    ("IsDefault", Kind::Flag),
    ("ParentGatewayId", Kind::Text),
];

/// The RDP choices of the legacy settings a profile on the global defaults takes, which
/// [`super::csharp::import`] resolves as the C# `RdpProfileResolver` does.
const RDP_DEFAULT_KEYS: [(&str, Kind); 15] = [
    ("RdpDefaultMode", Kind::Text),
    ("RdpDefaultRedirectClipboard", Kind::Flag),
    ("RdpDefaultRedirectDrives", Kind::Flag),
    ("RdpDefaultRedirectPrinters", Kind::Flag),
    ("RdpDefaultRedirectComPorts", Kind::Flag),
    ("RdpDefaultRedirectSmartCards", Kind::Flag),
    ("RdpDefaultRedirectWebcam", Kind::Flag),
    ("RdpDefaultRedirectUsb", Kind::Flag),
    ("RdpDefaultAudioMode", Kind::Integer),
    ("RdpDefaultAudioCapture", Kind::Flag),
    ("RdpDefaultMultiMonitor", Kind::Flag),
    ("RdpDefaultNla", Kind::Flag),
    ("RdpDefaultColorDepth", Kind::Integer),
    ("RdpDefaultAutoReconnect", Kind::Flag),
    ("RdpDefaultDynamicResolution", Kind::Flag),
];

/// The keys of the legacy files the C# migrates and this migration never takes: programs it
/// would run, paths it would write to, trust and secrets.
pub const REFUSED_KEYS: [&str; 16] = [
    "PlinkPath",
    "PuttyPath",
    "PsftpPath",
    "ExternalEditorPath",
    "LogFilePath",
    "SessionLogDirectory",
    "PinHash",
    "PinSalt",
    "HmacKey",
    "HmacKeyCreatedAt",
    "LastDpapiUser",
    "RdpPasswordEncrypted",
    "SshPasswordEncrypted",
    "SshKeyPassphraseEncrypted",
    "HostKeyFingerprint",
    "TrustedHostKeys",
];

/// Why the legacy files could not be migrated at all.
#[derive(Debug, thiserror::Error)]
pub enum ConversionError {
    /// `settings.json` is not a JSON object.
    #[error("legacy settings file: {0}")]
    Settings(String),
    /// `servers.json` is not a JSON list of servers, one server, or nothing.
    #[error("legacy servers file: {0}")]
    Servers(String),
    /// A gateway of the settings cannot be read: the C# migration fails whole on it.
    #[error("legacy gateway {0} cannot be read")]
    Gateway(usize),
    /// The document handed to the C# import was refused.
    #[error(transparent)]
    Import(#[from] ImportError),
}

/// Why a legacy profile was left out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LeftOutBecause {
    /// A value cannot be read, as the C# `InvalidLegacyField`: a number that is no 32-bit
    /// integer, or an entry that is no object.
    InvalidLegacyField,
    /// Refused as an imported C# profile would be.
    Refused(SkipReason),
}

/// A legacy profile left out, as the C# `MigrationWarning` reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeftOut {
    /// Its place in `servers.json`, from 1.
    pub index: usize,
    /// Its display name made safe to show, as the C# `GetSafeProfileIdentity`; `None` when
    /// it has none.
    pub name: Option<String>,
    /// Why.
    pub reason: LeftOutBecause,
}

/// The legacy settings Heimdall-rs takes: each one it has an equivalent for, a value out of
/// its range taken as the default. Nothing else of the legacy settings is held.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LegacySettings {
    /// `DefaultLocale`.
    pub language: Option<Language>,
    /// `DefaultTheme`.
    pub theme: Option<AppTheme>,
    /// `EnableLogging`.
    pub diagnostics_log: Option<bool>,
    /// `RequireCredentialGuard`, taken only when on: a migration never lowers a protection.
    pub require_credential_guard: bool,
    /// `SshDefaultMode`.
    pub ssh_default_mode: Option<SshMode>,
    /// `AntiIdleIntervalSeconds`.
    pub anti_idle_interval: Option<u32>,
    /// `SshTmoutResetIntervalSeconds`.
    pub ssh_tmout_reset_interval: Option<u32>,
    /// `RdpDefaultMode`.
    pub rdp_default_mode: Option<RdpMode>,
    /// `RdpDefaultRedirectClipboard`.
    pub rdp_redirect_clipboard: Option<bool>,
    /// `RdpDefaultRedirectDrives`.
    pub rdp_redirect_drives: Option<bool>,
    /// `RdpDefaultRedirectPrinters`.
    pub rdp_redirect_printers: Option<bool>,
    /// `RdpDefaultRedirectComPorts`.
    pub rdp_redirect_com_ports: Option<bool>,
    /// `RdpDefaultRedirectSmartCards`.
    pub rdp_redirect_smart_cards: Option<bool>,
    /// `RdpDefaultRedirectWebcam`.
    pub rdp_redirect_webcam: Option<bool>,
    /// `RdpDefaultRedirectUsb`.
    pub rdp_redirect_usb: Option<bool>,
    /// `RdpDefaultAudioMode`.
    pub rdp_audio: Option<AudioPlayback>,
    /// `RdpDefaultAudioCapture`.
    pub rdp_microphone: Option<bool>,
    /// `RdpDefaultMultiMonitor`.
    pub rdp_multi_monitor: Option<bool>,
    /// `RdpDefaultDynamicResolution`.
    pub rdp_dynamic_resolution: Option<bool>,
    /// `RdpDefaultNla`.
    pub rdp_nla: Option<bool>,
    /// `RdpDefaultColorDepth`.
    pub rdp_color_depth: Option<ColorDepth>,
    /// `RdpDefaultBitmapCaching`.
    pub rdp_bitmap_caching: Option<bool>,
    /// `RdpDefaultCompression`.
    pub rdp_compression: Option<bool>,
    /// `RdpDefaultHardwareAcceleration`.
    pub rdp_hardware_acceleration: Option<bool>,
    /// `RdpDefaultAutoReconnect`.
    pub rdp_auto_reconnect: Option<bool>,
    /// `MaxEmbeddedSessions`.
    pub max_sessions: Option<u32>,
    /// `EmbeddedRdpTimeoutMs`, in seconds.
    pub rdp_connect_timeout: Option<u32>,
    /// `SftpBrowserEnabled`.
    pub sftp_browser: Option<bool>,
    /// `SftpAutoOpenOnSsh`.
    pub sftp_auto_open_on_ssh: Option<bool>,
    /// `SftpFollowSshDirectory`.
    pub sftp_follow_ssh_directory: Option<bool>,
    /// `PreventSleepDuringSession`.
    pub prevent_sleep: Option<bool>,
    /// `SessionLoggingEnabled`: on or off, its folder never taken.
    pub session_logging: Option<bool>,
}

impl LegacySettings {
    /// Reads the keys taken of `legacy`.
    fn read(legacy: &Map<String, Value>) -> Self {
        let flag = |key: &str| legacy.get(key).and_then(Value::as_bool);
        let text = |key: &str| legacy.get(key).and_then(Value::as_str);
        let integer = |key: &str| legacy.get(key).and_then(Value::as_i64);
        // Out of its range, or no number of seconds: the default, as the settings load.
        let ranged = |key: &str, accepted: fn(u32) -> bool, default: u32| {
            legacy
                .get(key)
                .filter(|value| value.is_number())
                .map(|value| {
                    value
                        .as_i64()
                        .and_then(|value| u32::try_from(value).ok())
                        .filter(|value| accepted(*value))
                        .unwrap_or(default)
                })
        };
        Self {
            language: text("DefaultLocale").and_then(language_of),
            theme: text("DefaultTheme").map(AppTheme::named),
            diagnostics_log: flag("EnableLogging"),
            require_credential_guard: flag("RequireCredentialGuard") == Some(true),
            ssh_default_mode: text("SshDefaultMode").map(SshMode::named),
            anti_idle_interval: ranged(
                "AntiIdleIntervalSeconds",
                anti_idle_interval_accepted,
                ANTI_IDLE_INTERVAL_DEFAULT,
            ),
            ssh_tmout_reset_interval: ranged(
                "SshTmoutResetIntervalSeconds",
                ssh_tmout_reset_interval_accepted,
                SSH_TMOUT_RESET_INTERVAL_DEFAULT,
            ),
            rdp_default_mode: text("RdpDefaultMode").map(RdpMode::named),
            rdp_redirect_clipboard: flag("RdpDefaultRedirectClipboard"),
            rdp_redirect_drives: flag("RdpDefaultRedirectDrives"),
            rdp_redirect_printers: flag("RdpDefaultRedirectPrinters"),
            rdp_redirect_com_ports: flag("RdpDefaultRedirectComPorts"),
            rdp_redirect_smart_cards: flag("RdpDefaultRedirectSmartCards"),
            rdp_redirect_webcam: flag("RdpDefaultRedirectWebcam"),
            rdp_redirect_usb: flag("RdpDefaultRedirectUsb"),
            rdp_audio: integer("RdpDefaultAudioMode").map(csharp::audio_playback),
            rdp_microphone: flag("RdpDefaultAudioCapture"),
            rdp_multi_monitor: flag("RdpDefaultMultiMonitor"),
            rdp_dynamic_resolution: flag("RdpDefaultDynamicResolution"),
            rdp_nla: flag("RdpDefaultNla"),
            rdp_color_depth: integer("RdpDefaultColorDepth").map(ColorDepth::nearest),
            rdp_bitmap_caching: flag("RdpDefaultBitmapCaching"),
            rdp_compression: flag("RdpDefaultCompression"),
            rdp_hardware_acceleration: flag("RdpDefaultHardwareAcceleration"),
            rdp_auto_reconnect: flag("RdpDefaultAutoReconnect"),
            max_sessions: ranged(
                "MaxEmbeddedSessions",
                max_sessions_accepted,
                MAX_SESSIONS_DEFAULT,
            ),
            rdp_connect_timeout: legacy
                .get("EmbeddedRdpTimeoutMs")
                .filter(|value| value.is_number())
                .map(|value| {
                    value
                        .as_u64()
                        .map(|millis| millis.div_ceil(MILLIS_PER_SECOND))
                        .and_then(|seconds| u32::try_from(seconds).ok())
                        .filter(|seconds| rdp_connect_timeout_accepted(*seconds))
                        .unwrap_or(RDP_CONNECT_TIMEOUT_DEFAULT)
                }),
            sftp_browser: flag("SftpBrowserEnabled"),
            sftp_auto_open_on_ssh: flag("SftpAutoOpenOnSsh"),
            sftp_follow_ssh_directory: flag("SftpFollowSshDirectory"),
            prevent_sleep: flag("PreventSleepDuringSession"),
            session_logging: flag("SessionLoggingEnabled"),
        }
    }

    /// Lays these settings over `target`: a setting the legacy file does not hold is left as
    /// it is.
    pub fn apply_to(&self, target: &mut Settings) {
        fn set<T: Copy>(target: &mut T, value: Option<T>) {
            if let Some(value) = value {
                *target = value;
            }
        }
        if self.language.is_some() {
            target.language = self.language;
        }
        set(&mut target.theme, self.theme);
        set(&mut target.diagnostics_log, self.diagnostics_log);
        target.require_credential_guard |= self.require_credential_guard;
        set(&mut target.ssh_default_mode, self.ssh_default_mode);
        set(&mut target.anti_idle_interval, self.anti_idle_interval);
        set(
            &mut target.ssh_tmout_reset_interval,
            self.ssh_tmout_reset_interval,
        );
        set(&mut target.rdp_default_mode, self.rdp_default_mode);
        let rdp = &mut target.rdp_defaults;
        set(&mut rdp.redirect_clipboard, self.rdp_redirect_clipboard);
        set(&mut rdp.redirect_drives, self.rdp_redirect_drives);
        set(&mut rdp.redirect_printers, self.rdp_redirect_printers);
        set(&mut rdp.redirect_com_ports, self.rdp_redirect_com_ports);
        set(&mut rdp.redirect_smart_cards, self.rdp_redirect_smart_cards);
        set(&mut rdp.redirect_webcam, self.rdp_redirect_webcam);
        set(&mut rdp.redirect_usb, self.rdp_redirect_usb);
        set(&mut rdp.audio, self.rdp_audio);
        set(&mut rdp.microphone, self.rdp_microphone);
        set(&mut rdp.multi_monitor, self.rdp_multi_monitor);
        set(&mut rdp.dynamic_resolution, self.rdp_dynamic_resolution);
        set(&mut rdp.nla, self.rdp_nla);
        set(&mut rdp.color_depth, self.rdp_color_depth);
        set(&mut rdp.bitmap_caching, self.rdp_bitmap_caching);
        set(&mut rdp.compression, self.rdp_compression);
        set(
            &mut rdp.hardware_acceleration,
            self.rdp_hardware_acceleration,
        );
        set(&mut rdp.auto_reconnect, self.rdp_auto_reconnect);
        set(&mut target.max_sessions, self.max_sessions);
        set(&mut target.rdp_connect_timeout, self.rdp_connect_timeout);
        set(&mut target.sftp_browser.enabled, self.sftp_browser);
        set(
            &mut target.sftp_browser.auto_open_on_ssh,
            self.sftp_auto_open_on_ssh,
        );
        set(
            &mut target.sftp_browser.follow_ssh_directory,
            self.sftp_follow_ssh_directory,
        );
        set(&mut target.prevent_sleep, self.prevent_sleep);
        set(&mut target.session_logging, self.session_logging);
    }
}

/// What the legacy files give.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conversion {
    /// The profiles and gateways imported, as an imported C# file's; no host key is in it,
    /// and a gateway left out is in its [`ImportReport::skipped`].
    pub report: ImportReport,
    /// Entries of `servers.json` examined, as the C# `ServersExamined`.
    pub examined: usize,
    /// The profiles left out, in the file's order.
    pub left_out: Vec<LeftOut>,
    /// Gateways the legacy settings hold.
    pub gateways: usize,
    /// Projects the legacy settings hold, which Heimdall-rs has no equivalent for: left out.
    pub projects: usize,
    /// Key paths of the profiles and gateways imported left out, being no plain local path:
    /// each is imported without its key file.
    pub key_paths_left_out: usize,
    /// The settings taken.
    pub settings: LegacySettings,
}

impl Conversion {
    /// Profiles imported, as the C# `ServersImported`.
    #[must_use]
    pub fn imported(&self) -> usize {
        self.examined.saturating_sub(self.left_out.len())
    }
}

/// Where the legacy settings of the installation `installation` are.
#[must_use]
pub fn settings_file(installation: &Path) -> PathBuf {
    installation
        .join(CONFIG_DIRECTORY_NAME)
        .join(SETTINGS_FILE_NAME)
}

/// Where the legacy servers of the installation `installation` are.
#[must_use]
pub fn servers_file(installation: &Path) -> PathBuf {
    installation
        .join(CONFIG_DIRECTORY_NAME)
        .join(SERVERS_FILE_NAME)
}

/// Whether `folder` holds a legacy installation, as the C# `DetectLegacyInstallation`: both
/// files there.
#[must_use]
pub fn is_installation(folder: &Path) -> bool {
    settings_file(folder).is_file() && servers_file(folder).is_file()
}

/// The legacy installation found walking up from `start`, as the C# start looks for it: for
/// each folder with a parent, the parent's `RDPManager`, then the folder's own.
#[must_use]
pub fn find_installation(start: &Path) -> Option<PathBuf> {
    let mut folder = start;
    while let Some(parent) = folder.parent() {
        for candidate in [
            parent.join(LEGACY_APP_FOLDER_NAME),
            folder.join(LEGACY_APP_FOLDER_NAME),
        ] {
            if is_installation(&candidate) {
                return Some(candidate);
            }
        }
        folder = parent;
    }
    None
}

/// Converts the legacy settings `settings_text` and servers `servers_text`.
///
/// `servers.json` is a list of servers, or a single one, as `ConvertTo-Json` writes a list of
/// one; empty or `null`, it holds none.
///
/// # Errors
///
/// [`ConversionError`] when a file is not JSON of the expected shape, or a gateway cannot be
/// read; a profile that cannot be is no error, it is in [`Conversion::left_out`].
pub fn convert(settings_text: &str, servers_text: &str) -> Result<Conversion, ConversionError> {
    let settings = match serde_json::from_str::<Value>(settings_text) {
        Ok(Value::Object(settings)) => settings,
        Ok(_) => return Err(ConversionError::Settings("not an object".to_owned())),
        Err(error) => return Err(ConversionError::Settings(error.to_string())),
    };
    let servers = servers_of(servers_text)?;

    let mut left_out = Vec::new();
    // The place in the legacy file of each server handed to the C# import.
    let mut places = Vec::new();
    // Key paths left out of each server handed to the C# import.
    let mut paths_left_out = Vec::new();
    let mut rows = Vec::new();
    for (index, server) in servers.iter().enumerate() {
        let index = index + 1;
        match taken(server, &SERVER_KEYS) {
            Some((row, left)) => {
                places.push(index);
                paths_left_out.push(left);
                rows.push(Value::Object(row));
            }
            None => left_out.push(LeftOut {
                index,
                name: safe_identity(server),
                reason: LeftOutBecause::InvalidLegacyField,
            }),
        }
    }

    let legacy_gateways = list(&settings, GATEWAYS_KEY);
    let mut gateways = Vec::new();
    let mut gateway_paths_left_out = Vec::new();
    for (index, gateway) in legacy_gateways.iter().enumerate() {
        let (row, left) =
            taken(gateway, &GATEWAY_KEYS).ok_or(ConversionError::Gateway(index + 1))?;
        gateway_paths_left_out.push((gateway.get("Id").and_then(Value::as_str), left));
        gateways.push(Value::Object(row));
    }
    let mut document = rdp_defaults(&settings);
    document.insert(
        SERVERS_DOCUMENT_GATEWAYS_KEY.to_owned(),
        Value::Array(gateways),
    );

    let mut report = csharp::import(
        &Value::Array(rows).to_string(),
        Some(&Value::Object(document).to_string()),
    )?;
    // Never a trust decision from this folder.
    report.host_keys.clear();
    let (refused, gateways_refused): (Vec<_>, Vec<_>) = std::mem::take(&mut report.skipped)
        .into_iter()
        .partition(|skipped| skipped.position.is_some());
    // A key path counts as left out of what is imported only: a gateway the C# import
    // refuses has no identifier or no host, and is not counted.
    let mut key_paths_left_out: usize = gateway_paths_left_out
        .iter()
        .filter(|(id, _)| {
            id.is_some_and(|id| !gateways_refused.iter().any(|refused| refused.id == id))
        })
        .map(|(_, left)| left)
        .sum();
    report.skipped = gateways_refused;
    let mut imported_places = vec![true; places.len()];
    for skipped in refused {
        let Some(position) = skipped.position.map(|position| position.wrapping_sub(1)) else {
            continue;
        };
        let Some(index) = places.get(position).copied() else {
            continue;
        };
        imported_places[position] = false;
        left_out.push(LeftOut {
            index,
            name: safe_identity(&servers[index - 1]),
            reason: LeftOutBecause::Refused(skipped.reason),
        });
    }
    left_out.sort_by_key(|left| left.index);
    key_paths_left_out += paths_left_out
        .iter()
        .zip(&imported_places)
        .filter(|(_, imported)| **imported)
        .map(|(left, _)| left)
        .sum::<usize>();

    Ok(Conversion {
        report,
        examined: servers.len(),
        left_out,
        gateways: legacy_gateways.len(),
        projects: list(&settings, PROJECTS_KEY).len(),
        key_paths_left_out,
        settings: LegacySettings::read(&settings),
    })
}

/// The servers of `text`: a list, one server alone, or none.
fn servers_of(text: &str) -> Result<Vec<Value>, ConversionError> {
    if text.trim().is_empty() {
        return Ok(Vec::new());
    }
    match serde_json::from_str::<Value>(text) {
        Ok(Value::Array(servers)) => Ok(servers),
        Ok(server @ Value::Object(_)) => Ok(vec![server]),
        Ok(Value::Null) => Ok(Vec::new()),
        Ok(_) => Err(ConversionError::Servers("not a list of servers".to_owned())),
        Err(error) => Err(ConversionError::Servers(error.to_string())),
    }
}

/// The list `key` of `settings`; none when it is no list, as the C# reads only an array.
fn list<'a>(settings: &'a Map<String, Value>, key: &str) -> &'a [Value] {
    settings
        .get(key)
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}

/// The legacy RDP defaults, in the `camelCase` of the C# settings.
fn rdp_defaults(settings: &Map<String, Value>) -> Map<String, Value> {
    let mut document = Map::new();
    for (key, kind) in RDP_DEFAULT_KEYS {
        // A value of the wrong kind is ignored, as the C#: the default applies.
        if let Some(value) = settings.get(key).and_then(|value| of_kind(value, kind)) {
            document.insert(camel_case(key), value);
        }
    }
    document
}

/// The keys `keys` of the legacy object `legacy` in `camelCase`, each of the kind the C# reads;
/// `None` when `legacy` is no object or holds a number that is no 32-bit integer, which the
/// C# `GetInt32` refuses.
fn taken(legacy: &Value, keys: &[(&str, Kind)]) -> Option<(Map<String, Value>, usize)> {
    let legacy = legacy.as_object()?;
    let mut row = Map::new();
    let mut paths_left_out = 0;
    for (key, kind) in keys {
        let Some(value) = legacy.get(*key) else {
            continue;
        };
        if *kind == Kind::LocalPath
            && let Some(path) = value.as_str()
            && (path.trim().is_empty() || !is_plain_local_path(path))
        {
            // A blank one is none; any other is counted.
            paths_left_out += usize::from(!path.trim().is_empty());
        } else if *kind == Kind::Integer && value.is_number() {
            row.insert(camel_case(key), Value::from(int32(value)?));
        } else if let Some(value) = of_kind(value, *kind) {
            row.insert(camel_case(key), value);
        }
    }
    Some((row, paths_left_out))
}

/// Whether `path` names a file on a drive of this computer, which reading reaches no other
/// machine nor any device: on Windows absolute on a drive letter, `C:\...` or `C:/...`;
/// elsewhere absolute. Never a network path (`\\host\...`, `//host/...`), nor a device or
/// verbatim one (`\\.\...`, `\\?\...`, `\??\...`), whatever the system.
#[must_use]
pub fn is_plain_local_path(path: &str) -> bool {
    const SEPARATORS: [char; 2] = ['\\', '/'];
    let path = path.trim();
    let doubled = path.starts_with(SEPARATORS) && path[1..].starts_with(SEPARATORS);
    if doubled || path.starts_with(NT_OBJECT_PREFIX) || path.contains('\0') {
        return false;
    }
    if cfg!(windows) {
        let bytes = path.as_bytes();
        bytes.len() > DRIVE_PREFIX_LENGTH
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && matches!(bytes[2], b'\\' | b'/')
            // A second colon names an alternate data stream or a device, never a file.
            && !path[DRIVE_PREFIX_LENGTH..].contains(':')
    } else {
        Path::new(path).is_absolute()
    }
}

/// `C:\`: a drive letter, its colon and a separator.
const DRIVE_PREFIX_LENGTH: usize = 3;

/// What starts a path of the Windows object manager, a device's.
const NT_OBJECT_PREFIX: &str = "\\??\\";

/// `value` when it is of `kind`; a number only when a 32-bit integer.
fn of_kind(value: &Value, kind: Kind) -> Option<Value> {
    match kind {
        Kind::Text | Kind::LocalPath => value.is_string().then(|| value.clone()),
        Kind::Flag => value.is_boolean().then(|| value.clone()),
        Kind::Integer => int32(value).map(Value::from),
    }
}

/// `value` as a 32-bit integer, as the C# `GetInt32` reads it.
fn int32(value: &Value) -> Option<i32> {
    value.as_i64().and_then(|value| i32::try_from(value).ok())
}

/// `key` with its first letter in lower case, as the C# session document names it.
fn camel_case(key: &str) -> String {
    let mut chars = key.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_ascii_lowercase().to_string() + chars.as_str()
    })
}

/// The language of the C# `DefaultLocale`, `fr` or `fr-FR`; `None` for one not offered.
fn language_of(locale: &str) -> Option<Language> {
    Language::from_code(locale.split(LOCALE_SEPARATORS).next().unwrap_or_default())
}

/// The display name of the legacy server `legacy` made safe to show, as the C#
/// `GetSafeProfileIdentity`: trimmed, each run of control characters and spaces one space,
/// cut at [`MAX_IDENTITY_LENGTH`] UTF-16 code units; `None` when it has none.
#[must_use]
pub fn safe_identity(legacy: &Value) -> Option<String> {
    let name = legacy.get(DISPLAY_NAME_KEY)?.as_str()?.trim();
    let mut identity = String::new();
    let mut length = 0;
    for ch in name.chars() {
        if ch.is_control() || ch.is_whitespace() {
            if !identity.is_empty() && !identity.ends_with(' ') {
                identity.push(' ');
                length += 1;
            }
            continue;
        }
        if length + ch.len_utf16() > MAX_IDENTITY_LENGTH {
            break;
        }
        identity.push(ch);
        length += ch.len_utf16();
    }
    let identity = identity.trim_end();
    (!identity.is_empty()).then(|| identity.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_takes_a_lower_case_first_letter() {
        assert_eq!(camel_case("RdpRedirectUsb"), "rdpRedirectUsb");
        assert_eq!(camel_case("SshX11Forwarding"), "sshX11Forwarding");
        assert_eq!(camel_case(""), "");
    }

    #[test]
    fn a_locale_gives_its_language() {
        assert_eq!(language_of("fr-FR"), Some(Language::French));
        assert_eq!(language_of("es"), Some(Language::Spanish));
        assert_eq!(language_of("en_US"), Some(Language::English));
        assert_eq!(language_of("de-DE"), None);
    }

    #[test]
    fn no_refused_key_is_ever_taken() {
        for refused in REFUSED_KEYS {
            assert!(
                SERVER_KEYS
                    .iter()
                    .chain(GATEWAY_KEYS.iter())
                    .chain(RDP_DEFAULT_KEYS.iter())
                    .all(|(key, _)| *key != refused),
                "{refused}"
            );
        }
    }
}
