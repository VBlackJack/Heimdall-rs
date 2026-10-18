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

//! The preferences of the C# Heimdall: those of its `settings.json` (`AppSettings.cs`),
//! and those of the file its "Export settings" writes (`SettingsTransfer.cs`), `{"format": "heimdall-settings", "version": 1,
//! "settings": {...}}`. Keys are read without case: `settings.json` writes them in camel
//! case, the export file in Pascal case.
//!
//! A preference is carried over only when Heimdall-rs has its equivalent, its value is one
//! Heimdall-rs reads, the C# user changed it (a value equal to the C# default says nothing
//! about the user), and the user did not change it here: a preference no longer at its
//! default here is kept, and said so. What is carried over is then shown before it is
//! taken, as a settings file's import shows it.
//!
//! Secrets never cross, whatever the file: the PIN hash and salt, the HMAC key, the Git
//! access token, the credential provider's unlock secret and the vault's material are left
//! out, and named. They are encrypted with Windows DPAPI for the C# Windows account anyway.

use std::collections::HashMap;
use std::time::Duration;

use serde_json::{Map, Value};
use thiserror::Error;

use super::csharp::audio_playback;
use crate::credential_provider::{MAX_TIMEOUT, MIN_TIMEOUT, ProviderKind};
use crate::profile::{ColorDepth, RESOLUTION_PRESETS, RdpMode, SshMode};
use crate::settings::{
    Accent, AgentPreference, AppTheme, BroadcastScope, ColorScheme, ExecutionPolicy, Language,
    Settings, anti_idle_interval_accepted, auto_lock_idle_minutes_accepted, max_sessions_accepted,
    rdp_auto_reconnect_attempts_accepted, rdp_keep_alive_interval_accepted,
    rdp_resize_enable_delay_accepted, reachability_interval_accepted, reachability_probes_accepted,
    reachability_timeout_accepted, reconnect_delay_accepted, session_log_retention_days_accepted,
    ssh_auto_reconnect_attempts_accepted, ssh_keep_alive_interval_accepted,
    ssh_tmout_reset_interval_accepted, terminal_font_size_accepted, update_interval_accepted,
    windows_hello_grace_minutes_accepted, windows_hello_vault_max_days_accepted,
};

/// What the C# "Export settings" file says it is.
const EXPORT_FORMAT: &str = "heimdall-settings";
/// The version of the C# "Export settings" file read.
const EXPORT_VERSION: i64 = 1;
/// Its keys.
const EXPORT_FORMAT_KEY: &str = "format";
const EXPORT_VERSION_KEY: &str = "version";
const EXPORT_SETTINGS_KEY: &str = "settings";

/// The C# settings that are secrets: never carried over, whatever the file, and named when
/// a file holds one (`AppSettings.cs:186-197`, `:547`, `:585`, `:603-631`).
const SECRETS: [&str; 9] = [
    "PinHash",
    "PinSalt",
    "HmacKey",
    "CmdLibGitSyncToken",
    "CredentialProviderUnlockSecretEncrypted",
    "VaultWrappedDek",
    "VaultHelloWrappedDek",
    "VaultHelloChallenge",
    "VaultHelloSalt",
];

/// Milliseconds in a second, for the C# settings written in milliseconds.
const MILLIS_PER_SECOND: i64 = 1000;
/// Fewest and most milliseconds of the C# RDP connection watchdog besides 0
/// (`AppSettings.cs:118`).
const RDP_WATCHDOG_MIN_MS: i64 = 5000;
const RDP_WATCHDOG_MAX_MS: i64 = 600_000;
/// Fewest and most bits of colour the C# accepts (`RdpDisplayLimits.cs:112`).
const COLOR_DEPTH_MIN: i64 = 16;
const COLOR_DEPTH_MAX: i64 = 32;
/// The C# audio mode "not played" (`AppSettings.RdpDefaultAudioMode`), its default, and
/// its highest value.
const AUDIO_MODE_DEFAULT: i64 = 0;
const AUDIO_MODE_MAX: i64 = 2;

/// What became of one preference of the C# file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreferenceOutcome {
    /// Carried over: its value is laid over this computer's, to be shown before it is taken.
    Taken,
    /// Changed here already: the value here is kept.
    KeptHere,
    /// A secret: never carried over.
    Secret,
    /// A value Heimdall-rs does not read: of another type, out of the C# range, or a name
    /// it does not know.
    Invalid,
    /// A path relative to the C# installation, which means nothing here.
    RelativePath,
    /// A preference of the export file Heimdall-rs has no equivalent of.
    NoEquivalent,
}

/// One preference of the C# file, and what became of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreferenceRow {
    /// Its C# name, as `AppSettings` names it.
    pub key: String,
    /// What became of it.
    pub outcome: PreferenceOutcome,
}

/// The preferences of a C# file, laid over this computer's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preferences {
    /// This computer's settings with the preferences carried over laid over them.
    pub settings: Settings,
    /// Each preference read that is carried over, kept, or left out, in a fixed order:
    /// those the file holds at the C# default, or at the value here, are not listed.
    pub rows: Vec<PreferenceRow>,
    /// The file is the C# "Export settings" file, not `settings.json`.
    pub export_file: bool,
}

impl Preferences {
    /// How many preferences had `outcome`.
    #[must_use]
    pub fn count(&self, outcome: PreferenceOutcome) -> usize {
        self.rows
            .iter()
            .filter(|row| row.outcome == outcome)
            .count()
    }

    /// The C# names of the preferences that had `outcome`, in the order of the rows.
    pub fn named(&self, outcome: PreferenceOutcome) -> impl Iterator<Item = &str> {
        self.rows
            .iter()
            .filter(move |row| row.outcome == outcome)
            .map(|row| row.key.as_str())
    }
}

/// Why a C# file could not be read for its preferences.
#[derive(Debug, Error)]
pub enum CsharpSettingsError {
    /// It is not JSON.
    #[error("not JSON: {0}")]
    NotJson(#[source] serde_json::Error),
    /// It is JSON, but no object of settings.
    #[error("not an object of settings")]
    NotSettings,
    /// It is an export file of a later version, this many.
    #[error("an export file of version {0}")]
    Newer(i64),
}

/// A JSON object whose keys are looked up without case.
struct Folded<'a> {
    /// Each value by its key in lower case; the first of a key counts.
    values: HashMap<String, &'a Value>,
    /// The keys as written, in the file's order.
    keys: Vec<&'a str>,
}

impl<'a> Folded<'a> {
    fn of(object: &'a Map<String, Value>) -> Self {
        let mut values = HashMap::new();
        let mut keys = Vec::new();
        for (key, value) in object {
            values.entry(key.to_lowercase()).or_insert(value);
            keys.push(key.as_str());
        }
        Self { values, keys }
    }

    /// The value of `key`; `None` when absent or null, as the C# writes a setting it has no
    /// value for.
    fn get(&self, key: &str) -> Option<&'a Value> {
        self.values
            .get(&key.to_lowercase())
            .copied()
            .filter(|value| !value.is_null())
    }
}

/// The document of `text`, parsed.
fn parse(text: &str) -> Result<Value, CsharpSettingsError> {
    serde_json::from_str(text).map_err(CsharpSettingsError::NotJson)
}

/// The settings object of `document`, and whether it is an export file's.
fn read_object(document: &Value) -> Result<(&Map<String, Value>, bool), CsharpSettingsError> {
    let Value::Object(root) = document else {
        return Err(CsharpSettingsError::NotSettings);
    };
    let root_folded = Folded::of(root);
    let Some(format) = root_folded.get(EXPORT_FORMAT_KEY) else {
        return Ok((root, false));
    };
    if format.as_str() != Some(EXPORT_FORMAT) {
        return Err(CsharpSettingsError::NotSettings);
    }
    match root_folded.get(EXPORT_VERSION_KEY).and_then(Value::as_i64) {
        Some(EXPORT_VERSION) => {}
        Some(newer) if newer > EXPORT_VERSION => return Err(CsharpSettingsError::Newer(newer)),
        _ => return Err(CsharpSettingsError::NotSettings),
    }
    match root_folded.get(EXPORT_SETTINGS_KEY) {
        Some(Value::Object(settings)) => Ok((settings, true)),
        _ => Err(CsharpSettingsError::NotSettings),
    }
}

/// What reading one C# value did.
enum Read {
    /// It is laid over the settings given.
    Set,
    /// It is the C# default: the user did not choose it.
    CSharpDefault,
    /// It is not a value Heimdall-rs reads.
    Invalid,
    /// It is a path relative to the C# installation.
    Relative,
}

/// One C# preference and its equivalent here.
struct Preference {
    /// Its C# name.
    key: &'static str,
    /// Lays its value over the settings given.
    read: fn(&Value, &mut Settings) -> Read,
    /// Whether two settings hold the same value for it.
    same: fn(&Settings, &Settings) -> bool,
}

/// A preference whose equivalent is the field `$($field).+` of the settings.
macro_rules! preference {
    ($key:literal, |$value:ident, $settings:ident| $read:expr, $($field:ident).+) => {
        Preference {
            key: $key,
            read: |$value, $settings| $read,
            same: |a, b| a.$($field).+ == b.$($field).+,
        }
    };
}

/// A C# `bool`, laid over `field` unless it is `default`.
fn flag(value: &Value, default: bool, field: &mut bool) -> Read {
    match value.as_bool() {
        None => Read::Invalid,
        Some(on) if on == default => Read::CSharpDefault,
        Some(on) => {
            *field = on;
            Read::Set
        }
    }
}

/// A C# `int`: a JSON integer, never a fraction nor a text.
fn whole(value: &Value) -> Option<i64> {
    value.as_i64()
}

/// A C# `int`, laid over `field` unless it is `default`, when `accepted` here.
fn number(value: &Value, default: i64, accepted: fn(u32) -> bool, field: &mut u32) -> Read {
    match whole(value) {
        None => Read::Invalid,
        Some(found) if found == default => Read::CSharpDefault,
        Some(found) => match u32::try_from(found).ok().filter(|found| accepted(*found)) {
            Some(found) => {
                *field = found;
                Read::Set
            }
            None => Read::Invalid,
        },
    }
}

/// A C# choice among `options`, by name without case, laid over `field` unless it is
/// `default`; also by its number in the C# enum when `numbered`, as the C# enum converters
/// read one (`TolerantEnumConverter.cs`). A name not known is invalid, where the C# would
/// fall back to its default.
fn choice<T: Copy>(
    value: &Value,
    (default, numbered): (&str, bool),
    options: &[(&str, T)],
    field: &mut T,
) -> Read {
    let index = match value {
        Value::String(name) => options
            .iter()
            .position(|(option, _)| option.eq_ignore_ascii_case(name.trim())),
        Value::Number(number) if numbered => number
            .as_u64()
            .and_then(|number| usize::try_from(number).ok())
            .filter(|index| *index < options.len()),
        _ => None,
    };
    match index {
        None => Read::Invalid,
        Some(index) if options[index].0.eq_ignore_ascii_case(default) => Read::CSharpDefault,
        Some(index) => {
            *field = options[index].1;
            Read::Set
        }
    }
}

/// The options of a choice whose names are `names` of `all`.
fn named_options<T: Copy, const N: usize>(
    all: [T; N],
    name: fn(T) -> &'static str,
) -> Vec<(&'static str, T)> {
    all.into_iter()
        .map(|option| (name(option), option))
        .collect()
}

/// Whether `text` holds a character a setting's text never holds.
fn has_control(text: &str) -> bool {
    text.chars().any(char::is_control)
}

/// A C# text, laid over `field` unless it is `default` or empty, as the C# reads an empty
/// one as not set.
fn words(value: &Value, default: &str, field: &mut String) -> Read {
    let Some(text) = value.as_str() else {
        return Read::Invalid;
    };
    let text = text.trim();
    if text.is_empty() || text == default {
        return Read::CSharpDefault;
    }
    if has_control(text) {
        return Read::Invalid;
    }
    text.clone_into(field);
    Read::Set
}

/// Whether `path` is absolute as Windows reads it, whatever the folder Heimdall runs from:
/// a drive, a share or a root. Read as text, so that a test on any system reads it alike.
fn absolute(path: &str) -> bool {
    let bytes = path.as_bytes();
    path.starts_with('/')
        || path.starts_with("\\\\")
        || (bytes.len() > 2
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && matches!(bytes[2], b'\\' | b'/'))
}

/// Whether `path` is absolute, or starts with a `%VARIABLE%`, which the editor's launch
/// expands as the C# does (`external_edit.rs`), as its default `%windir%` is written.
fn absolute_or_expanded(path: &str) -> bool {
    absolute(path) || path.starts_with('%')
}

/// A C# path, laid over `field` unless it is `default` or empty, when its consumer here
/// finds it as written: `found` tells. Any other, relative to the C# installation or a bare
/// name its consumer here does not look up on `PATH`, means nothing here: `PuTTY` and the X
/// server are taken from an absolute path only (`find_putty`, `x11_server::resolve`), the
/// editor and the transcripts' folder are not looked up on `PATH` at all.
fn path(value: &Value, default: &str, found: fn(&str) -> bool, field: &mut String) -> Read {
    let mut path = String::new();
    match words(value, default, &mut path) {
        Read::Set if found(&path) => {
            *field = path;
            Read::Set
        }
        Read::Set => Read::Relative,
        other => other,
    }
}

/// The C# milliseconds of the RDP connection watchdog, as seconds here: 0 for none.
fn watchdog(value: &Value, settings: &mut Settings) -> Read {
    const DEFAULT_MS: i64 = 45_000;
    match whole(value) {
        Some(DEFAULT_MS) => Read::CSharpDefault,
        Some(0) => {
            settings.rdp_connect_timeout = 0;
            Read::Set
        }
        Some(ms) if (RDP_WATCHDOG_MIN_MS..=RDP_WATCHDOG_MAX_MS).contains(&ms) => {
            let seconds = (ms + MILLIS_PER_SECOND / 2) / MILLIS_PER_SECOND;
            match u32::try_from(seconds) {
                Ok(seconds) => {
                    settings.rdp_connect_timeout = seconds;
                    Read::Set
                }
                Err(_) => Read::Invalid,
            }
        }
        _ => Read::Invalid,
    }
}

/// The C# milliseconds the credential provider's command is given.
fn provider_timeout(value: &Value, settings: &mut Settings) -> Read {
    const DEFAULT_MS: i64 = 10_000;
    match whole(value) {
        None => Read::Invalid,
        Some(DEFAULT_MS) => Read::CSharpDefault,
        Some(ms) => match u64::try_from(ms).map(Duration::from_millis) {
            Ok(timeout) if (MIN_TIMEOUT..=MAX_TIMEOUT).contains(&timeout) => {
                settings.credential_provider.timeout = timeout;
                Read::Set
            }
            _ => Read::Invalid,
        },
    }
}

/// The C# terminal font size.
fn font_size(value: &Value, settings: &mut Settings) -> Read {
    const DEFAULT: i64 = 14;
    match whole(value) {
        None => Read::Invalid,
        Some(DEFAULT) => Read::CSharpDefault,
        Some(size) => match u16::try_from(size)
            .ok()
            .filter(|size| terminal_font_size_accepted(*size))
        {
            Some(size) => {
                settings.terminal_font_size = size;
                Read::Set
            }
            None => Read::Invalid,
        },
    }
}

/// The C# language, by its code: those this version is written in only.
fn language(value: &Value, settings: &mut Settings) -> Read {
    const DEFAULT: &str = "en";
    let Some(code) = value.as_str() else {
        return Read::Invalid;
    };
    if code.trim().eq_ignore_ascii_case(DEFAULT) {
        return Read::CSharpDefault;
    }
    match Language::from_code(code) {
        Some(language) => {
            settings.language = Some(language);
            Read::Set
        }
        None => Read::Invalid,
    }
}

/// The C# resolution presets, each `WIDTHxHEIGHT`: all of them read, or none taken.
fn presets(value: &Value, settings: &mut Settings) -> Read {
    let Some(items) = value.as_array() else {
        return Read::Invalid;
    };
    let mut sizes = Vec::with_capacity(items.len());
    for item in items {
        match item.as_str().and_then(crate::profile::resolution_preset) {
            Some(size) => sizes.push(size),
            None => return Read::Invalid,
        }
    }
    // Empty is the C# built-in list, as here.
    if sizes.is_empty() || sizes == RESOLUTION_PRESETS {
        return Read::CSharpDefault;
    }
    settings.rdp_resolution_presets = sizes;
    Read::Set
}

/// The C# colour depth of an RDP desktop, as `NormalizeColorDepth` takes it.
fn color_depth(value: &Value, settings: &mut Settings) -> Read {
    match whole(value) {
        Some(COLOR_DEPTH_MAX) => Read::CSharpDefault,
        Some(bits) if (COLOR_DEPTH_MIN..COLOR_DEPTH_MAX).contains(&bits) => {
            settings.rdp_defaults.color_depth = ColorDepth::nearest(bits);
            Read::Set
        }
        _ => Read::Invalid,
    }
}

/// The C# audio mode of an RDP desktop.
fn audio(value: &Value, settings: &mut Settings) -> Read {
    match whole(value) {
        Some(AUDIO_MODE_DEFAULT) => Read::CSharpDefault,
        Some(mode) if (AUDIO_MODE_DEFAULT..=AUDIO_MODE_MAX).contains(&mode) => {
            settings.rdp_defaults.audio = audio_playback(mode);
            Read::Set
        }
        _ => Read::Invalid,
    }
}

/// The names of the C# `BroadcastScope`, in its order, and their equivalent here.
const BROADCAST_SCOPES: [(&str, BroadcastScope); 3] = [
    ("CurrentTab", BroadcastScope::CurrentTab),
    ("AllTabs", BroadcastScope::AllTabs),
    ("SelectedPanes", BroadcastScope::SelectedTabs),
];

/// Every C# preference Heimdall-rs has an equivalent of, in the order of `AppSettings`.
#[expect(
    clippy::too_many_lines,
    reason = "one entry per C# preference, the table is the mapping"
)]
fn preferences_table() -> Vec<Preference> {
    vec![
        preference!(
            "DefaultLocale",
            |value, settings| language(value, settings),
            language
        ),
        preference!(
            "DefaultTheme",
            |value, settings| choice(
                value,
                ("Drakul", false),
                &named_options(AppTheme::ALL, AppTheme::name),
                &mut settings.theme
            ),
            theme
        ),
        preference!(
            "AccentTint",
            |value, settings| choice(
                value,
                ("Default", false),
                &named_options(Accent::ALL, Accent::name),
                &mut settings.accent
            ),
            accent
        ),
        preference!(
            "UpdateCheckEnabled",
            |value, settings| flag(value, true, &mut settings.updates.enabled),
            updates.enabled
        ),
        preference!(
            "UpdateCheckIntervalHours",
            |value, settings| number(
                value,
                24,
                update_interval_accepted,
                &mut settings.updates.interval_hours
            ),
            updates.interval_hours
        ),
        preference!(
            "CredentialProviderTimeoutMs",
            |value, settings| provider_timeout(value, settings),
            credential_provider.timeout
        ),
        preference!(
            "RdpResizeEnableDelayMs",
            |value, settings| number(
                value,
                10_000,
                rdp_resize_enable_delay_accepted,
                &mut settings.rdp_resize_enable_delay_ms
            ),
            rdp_resize_enable_delay_ms
        ),
        preference!(
            "RdpConnectWatchdogTimeoutMs",
            |value, settings| watchdog(value, settings),
            rdp_connect_timeout
        ),
        preference!(
            "RdpAutoReconnectMaxAttempts",
            |value, settings| number(
                value,
                20,
                rdp_auto_reconnect_attempts_accepted,
                &mut settings.rdp_auto_reconnect_attempts
            ),
            rdp_auto_reconnect_attempts
        ),
        preference!(
            "RdpKeepAliveIntervalMs",
            |value, settings| number(
                value,
                60_000,
                rdp_keep_alive_interval_accepted,
                &mut settings.rdp_keep_alive_interval_ms
            ),
            rdp_keep_alive_interval_ms
        ),
        preference!(
            "SshKeepAliveIntervalSeconds",
            |value, settings| number(
                value,
                30,
                ssh_keep_alive_interval_accepted,
                &mut settings.ssh_keep_alive_interval
            ),
            ssh_keep_alive_interval
        ),
        preference!(
            "EnableLogging",
            |value, settings| flag(value, true, &mut settings.diagnostics_log),
            diagnostics_log
        ),
        preference!(
            "RequireCredentialGuard",
            |value, settings| flag(value, false, &mut settings.require_credential_guard),
            require_credential_guard
        ),
        preference!(
            "TerminalFontFamily",
            |value, settings| words(value, "Consolas", &mut settings.terminal_font_family),
            terminal_font_family
        ),
        preference!(
            "TerminalFontSize",
            |value, settings| font_size(value, settings),
            terminal_font_size
        ),
        preference!(
            "TerminalColorScheme",
            |value, settings| choice(
                value,
                ("Dracula", false),
                &named_options(ColorScheme::ALL, ColorScheme::name),
                &mut settings.color_scheme
            ),
            color_scheme
        ),
        preference!(
            "PowerShellExecutionPolicy",
            |value, settings| choice(
                value,
                ("Default", false),
                &named_options(ExecutionPolicy::ALL, ExecutionPolicy::name),
                &mut settings.powershell_execution_policy
            ),
            powershell_execution_policy
        ),
        preference!(
            "SshDefaultMode",
            |value, settings| choice(
                value,
                ("Embedded", false),
                &named_options(SshMode::ALL, SshMode::name),
                &mut settings.ssh_default_mode
            ),
            ssh_default_mode
        ),
        preference!(
            "SshAgentPreference",
            |value, settings| choice(
                value,
                ("AutoOpenSshFirst", true),
                &named_options(AgentPreference::ALL, AgentPreference::name),
                &mut settings.ssh_agent_preference
            ),
            ssh_agent_preference
        ),
        preference!(
            "SyncKnownHostsAtStartup",
            |value, settings| flag(value, false, &mut settings.sync_known_hosts_at_startup),
            sync_known_hosts_at_startup
        ),
        preference!(
            "AntiIdleIntervalSeconds",
            |value, settings| number(
                value,
                60,
                anti_idle_interval_accepted,
                &mut settings.anti_idle_interval
            ),
            anti_idle_interval
        ),
        preference!(
            "SshTmoutResetIntervalSeconds",
            |value, settings| number(
                value,
                240,
                ssh_tmout_reset_interval_accepted,
                &mut settings.ssh_tmout_reset_interval
            ),
            ssh_tmout_reset_interval
        ),
        preference!(
            "SshAutoReconnect",
            |value, settings| flag(value, false, &mut settings.ssh_auto_reconnect),
            ssh_auto_reconnect
        ),
        preference!(
            "SshAutoReconnectAttempts",
            |value, settings| number(
                value,
                3,
                ssh_auto_reconnect_attempts_accepted,
                &mut settings.ssh_auto_reconnect_attempts
            ),
            ssh_auto_reconnect_attempts
        ),
        preference!(
            "SshAutoReconnectFirstDelaySeconds",
            |value, settings| number(
                value,
                2,
                reconnect_delay_accepted,
                &mut settings.ssh_auto_reconnect_first_delay
            ),
            ssh_auto_reconnect_first_delay
        ),
        preference!(
            "SshAutoReconnectSecondDelaySeconds",
            |value, settings| number(
                value,
                5,
                reconnect_delay_accepted,
                &mut settings.ssh_auto_reconnect_second_delay
            ),
            ssh_auto_reconnect_second_delay
        ),
        preference!(
            "SshAutoReconnectSubsequentDelaySeconds",
            |value, settings| number(
                value,
                15,
                reconnect_delay_accepted,
                &mut settings.ssh_auto_reconnect_subsequent_delay
            ),
            ssh_auto_reconnect_subsequent_delay
        ),
        preference!(
            "RdpConfirmDisconnect",
            |value, settings| flag(value, true, &mut settings.rdp_confirm_disconnect),
            rdp_confirm_disconnect
        ),
        preference!(
            "RdpRedirectionIndicatorsAlwaysExpanded",
            |value, settings| flag(
                value,
                false,
                &mut settings.rdp_redirection_indicators_expanded
            ),
            rdp_redirection_indicators_expanded
        ),
        preference!(
            "RdpDefaultMode",
            |value, settings| choice(
                value,
                ("Embedded", false),
                &named_options(RdpMode::ALL, RdpMode::name),
                &mut settings.rdp_default_mode
            ),
            rdp_default_mode
        ),
        preference!(
            "RdpDefaultRedirectClipboard",
            |value, settings| flag(value, true, &mut settings.rdp_defaults.redirect_clipboard),
            rdp_defaults.redirect_clipboard
        ),
        preference!(
            "RdpDefaultRedirectDrives",
            |value, settings| flag(value, false, &mut settings.rdp_defaults.redirect_drives),
            rdp_defaults.redirect_drives
        ),
        preference!(
            "RdpDefaultRedirectPrinters",
            |value, settings| flag(value, false, &mut settings.rdp_defaults.redirect_printers),
            rdp_defaults.redirect_printers
        ),
        preference!(
            "RdpDefaultRedirectComPorts",
            |value, settings| flag(value, false, &mut settings.rdp_defaults.redirect_com_ports),
            rdp_defaults.redirect_com_ports
        ),
        preference!(
            "RdpDefaultRedirectSmartCards",
            |value, settings| flag(
                value,
                false,
                &mut settings.rdp_defaults.redirect_smart_cards
            ),
            rdp_defaults.redirect_smart_cards
        ),
        preference!(
            "RdpDefaultRedirectWebcam",
            |value, settings| flag(value, false, &mut settings.rdp_defaults.redirect_webcam),
            rdp_defaults.redirect_webcam
        ),
        preference!(
            "RdpDefaultRedirectUsb",
            |value, settings| flag(value, false, &mut settings.rdp_defaults.redirect_usb),
            rdp_defaults.redirect_usb
        ),
        preference!(
            "RdpDefaultAudioMode",
            |value, settings| audio(value, settings),
            rdp_defaults.audio
        ),
        preference!(
            "RdpDefaultAudioCapture",
            |value, settings| flag(value, false, &mut settings.rdp_defaults.microphone),
            rdp_defaults.microphone
        ),
        preference!(
            "RdpDefaultMultiMonitor",
            |value, settings| flag(value, false, &mut settings.rdp_defaults.multi_monitor),
            rdp_defaults.multi_monitor
        ),
        preference!(
            "RdpDefaultDynamicResolution",
            |value, settings| flag(value, true, &mut settings.rdp_defaults.dynamic_resolution),
            rdp_defaults.dynamic_resolution
        ),
        preference!(
            "RdpDefaultNla",
            |value, settings| flag(value, true, &mut settings.rdp_defaults.nla),
            rdp_defaults.nla
        ),
        preference!(
            "RdpDefaultStrictServerAuthentication",
            |value, settings| flag(
                value,
                false,
                &mut settings.rdp_defaults.strict_server_authentication
            ),
            rdp_defaults.strict_server_authentication
        ),
        preference!(
            "RdpDefaultColorDepth",
            |value, settings| color_depth(value, settings),
            rdp_defaults.color_depth
        ),
        preference!(
            "RdpDefaultBitmapCaching",
            |value, settings| flag(value, true, &mut settings.rdp_defaults.bitmap_caching),
            rdp_defaults.bitmap_caching
        ),
        preference!(
            "RdpDefaultCompression",
            |value, settings| flag(value, true, &mut settings.rdp_defaults.compression),
            rdp_defaults.compression
        ),
        preference!(
            "RdpDefaultHardwareAcceleration",
            |value, settings| flag(
                value,
                false,
                &mut settings.rdp_defaults.hardware_acceleration
            ),
            rdp_defaults.hardware_acceleration
        ),
        preference!(
            "RdpDefaultAutoReconnect",
            |value, settings| flag(value, true, &mut settings.rdp_defaults.auto_reconnect),
            rdp_defaults.auto_reconnect
        ),
        preference!(
            "RdpResolutionPresets",
            |value, settings| presets(value, settings),
            rdp_resolution_presets
        ),
        preference!(
            "MaxEmbeddedSessions",
            |value, settings| number(value, 10, max_sessions_accepted, &mut settings.max_sessions),
            max_sessions
        ),
        preference!(
            "SftpBrowserEnabled",
            |value, settings| flag(value, true, &mut settings.sftp_browser.enabled),
            sftp_browser.enabled
        ),
        preference!(
            "SftpAutoOpenOnSsh",
            |value, settings| flag(value, true, &mut settings.sftp_browser.auto_open_on_ssh),
            sftp_browser.auto_open_on_ssh
        ),
        preference!(
            "SftpFollowSshDirectory",
            |value, settings| flag(
                value,
                false,
                &mut settings.sftp_browser.follow_ssh_directory
            ),
            sftp_browser.follow_ssh_directory
        ),
        preference!(
            "ExternalEditorPath",
            |value, settings| path(
                value,
                "%windir%\\system32\\notepad.exe",
                absolute_or_expanded,
                &mut settings.external_editor
            ),
            external_editor
        ),
        preference!(
            "PreventSleepDuringSession",
            |value, settings| flag(value, true, &mut settings.prevent_sleep),
            prevent_sleep
        ),
        preference!(
            "SessionLoggingEnabled",
            |value, settings| flag(value, false, &mut settings.session_logging),
            session_logging
        ),
        preference!(
            "SessionLogDirectory",
            |value, settings| path(
                value,
                "logs\\sessions",
                absolute,
                &mut settings.session_log_directory
            ),
            session_log_directory
        ),
        preference!(
            "SessionLogRetentionDays",
            |value, settings| number(
                value,
                0,
                session_log_retention_days_accepted,
                &mut settings.session_log_retention_days
            ),
            session_log_retention_days
        ),
        preference!(
            "BroadcastScope",
            |value, settings| choice(
                value,
                ("CurrentTab", true),
                &BROADCAST_SCOPES,
                &mut settings.broadcast_scope
            ),
            broadcast_scope
        ),
        preference!(
            "CollapseTunnelsPanelByDefault",
            |value, settings| flag(value, true, &mut settings.collapse_tunnels_panel),
            collapse_tunnels_panel
        ),
        preference!(
            "ShowGatewayBadge",
            |value, settings| flag(value, true, &mut settings.show_gateway_badge),
            show_gateway_badge
        ),
        preference!(
            "GroupSessionTabsByFolder",
            |value, settings| flag(value, false, &mut settings.group_tabs_by_folder),
            group_tabs_by_folder
        ),
        preference!(
            "UseExternalCredentialProvider",
            |value, settings| flag(value, false, &mut settings.credential_provider.enabled),
            credential_provider.enabled
        ),
        preference!(
            "CredentialProviderType",
            |value, settings| choice(
                value,
                ("Command", true),
                &named_options(
                    [
                        ProviderKind::Command,
                        ProviderKind::WindowsCredentialManager
                    ],
                    ProviderKind::name
                ),
                &mut settings.credential_provider.kind
            ),
            credential_provider.kind
        ),
        preference!(
            "CredentialProviderCommand",
            |value, settings| words(value, "", &mut settings.credential_provider.command),
            credential_provider.command
        ),
        preference!(
            "CredentialProviderDatabase",
            |value, settings| words(value, "", &mut settings.credential_provider.database),
            credential_provider.database
        ),
        preference!(
            "CredentialProviderKeyFile",
            |value, settings| words(value, "", &mut settings.credential_provider.key_file),
            credential_provider.key_file
        ),
        preference!(
            "CredentialProviderUsernameCommand",
            |value, settings| words(
                value,
                "",
                &mut settings.credential_provider.username_command
            ),
            credential_provider.username_command
        ),
        preference!(
            "CredentialProviderFirstLineOnly",
            |value, settings| flag(
                value,
                false,
                &mut settings.credential_provider.first_line_only
            ),
            credential_provider.first_line_only
        ),
        preference!(
            "RequireWindowsHelloOnConnect",
            |value, settings| flag(value, false, &mut settings.windows_hello.require_on_connect),
            windows_hello.require_on_connect
        ),
        preference!(
            "WindowsHelloGraceMinutes",
            |value, settings| number(
                value,
                5,
                windows_hello_grace_minutes_accepted,
                &mut settings.windows_hello.grace_minutes
            ),
            windows_hello.grace_minutes
        ),
        preference!(
            "PuttyPath",
            |value, settings| path(value, "", absolute, &mut settings.putty_path),
            putty_path
        ),
        preference!(
            "X11ServerPath",
            |value, settings| path(value, "", absolute, &mut settings.x11_server_path),
            x11_server_path
        ),
        preference!(
            "X11AutoStart",
            |value, settings| flag(value, true, &mut settings.x11_auto_start),
            x11_auto_start
        ),
        preference!(
            "SessionHealthMonitorEnabled",
            |value, settings| flag(value, true, &mut settings.reachability.enabled),
            reachability.enabled
        ),
        preference!(
            "SessionHealthCheckIntervalSeconds",
            |value, settings| number(
                value,
                60,
                reachability_interval_accepted,
                &mut settings.reachability.interval
            ),
            reachability.interval
        ),
        preference!(
            "SessionHealthProbeTimeoutMs",
            |value, settings| number(
                value,
                2000,
                reachability_timeout_accepted,
                &mut settings.reachability.timeout
            ),
            reachability.timeout
        ),
        preference!(
            "SessionHealthMaxConcurrent",
            |value, settings| number(
                value,
                10,
                reachability_probes_accepted,
                &mut settings.reachability.probes
            ),
            reachability.probes
        ),
        preference!(
            "VaultHelloMaxDaysBeforeMasterPassword",
            |value, settings| number(
                value,
                0,
                windows_hello_vault_max_days_accepted,
                &mut settings.windows_hello.vault_max_days
            ),
            windows_hello.vault_max_days
        ),
        preference!(
            "AutoLockIdleMinutes",
            |value, settings| number(
                value,
                0,
                auto_lock_idle_minutes_accepted,
                &mut settings.auto_lock_idle_minutes
            ),
            auto_lock_idle_minutes
        ),
        preference!(
            "DisconnectOnLock",
            |value, settings| flag(value, false, &mut settings.disconnect_on_lock),
            disconnect_on_lock
        ),
    ]
}

/// The C# names of every preference carried over, as `AppSettings` names them.
#[must_use]
pub fn carried_keys() -> Vec<&'static str> {
    preferences_table()
        .iter()
        .map(|preference| preference.key)
        .collect()
}

/// The preferences of the C# file `text`, `settings.json` or an export file, laid over
/// `current`: only those the C# user changed, and the user here did not; secrets never.
/// For an export file, a preference Heimdall-rs has no equivalent of is listed too.
///
/// # Errors
///
/// [`CsharpSettingsError`] when `text` is not a C# settings object, or an export file of a
/// later version.
pub fn preferences(text: &str, current: &Settings) -> Result<Preferences, CsharpSettingsError> {
    let document = parse(text)?;
    let (object, export) = read_object(&document)?;
    let values = Folded::of(object);
    let defaults = Settings::default();
    let mut settings = current.clone();
    let mut rows = Vec::new();
    let table = preferences_table();
    for preference in &table {
        let Some(value) = values.get(preference.key) else {
            continue;
        };
        let mut laid = settings.clone();
        let outcome = match (preference.read)(value, &mut laid) {
            Read::CSharpDefault => continue,
            Read::Invalid => PreferenceOutcome::Invalid,
            Read::Relative => PreferenceOutcome::RelativePath,
            // The value here already.
            Read::Set if (preference.same)(&laid, current) => continue,
            Read::Set if !(preference.same)(current, &defaults) => PreferenceOutcome::KeptHere,
            Read::Set => {
                settings = laid;
                PreferenceOutcome::Taken
            }
        };
        rows.push(PreferenceRow {
            key: preference.key.to_owned(),
            outcome,
        });
    }
    for secret in SECRETS {
        let held = values
            .get(secret)
            .is_some_and(|value| value.as_str().is_none_or(|text| !text.trim().is_empty()));
        if held {
            rows.push(PreferenceRow {
                key: secret.to_owned(),
                outcome: PreferenceOutcome::Secret,
            });
        }
    }
    if export {
        for key in &values.keys {
            let known = table
                .iter()
                .map(|preference| preference.key)
                .chain(SECRETS)
                .any(|name| name.eq_ignore_ascii_case(key));
            if !known {
                rows.push(PreferenceRow {
                    key: (*key).to_owned(),
                    outcome: PreferenceOutcome::NoEquivalent,
                });
            }
        }
    }
    Ok(Preferences {
        settings,
        rows,
        export_file: export,
    })
}
