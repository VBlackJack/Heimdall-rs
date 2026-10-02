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

//! Import of the profiles of the C# Heimdall.
//!
//! Reads `servers.json` and, for the group defaults, `settings.json`. Group defaults are
//! resolved exactly as `GroupDefaultsDto.Resolve` and `GroupDefaultsDto.ApplyTo` do in the
//! C# Heimdall:
//!
//! - groups are `/`-separated paths; the deepest group that sets a field wins over its
//!   ancestors, and a group that sets a field to an empty string still stops the search;
//! - a profile keeps its own gateway, user name and key path unless they are empty, and its
//!   own port unless the `sshPort` field is absent.
//!
//! Secrets are never imported: the C# Heimdall encrypts them with Windows DPAPI, which only
//! that Windows account can reverse.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use serde::Deserialize;
use thiserror::Error;

use crate::post_connect::{DEFAULT_STEP_DELAY_MS, OnFailure, PostConnect, PostConnectStep};
use crate::profile::{
    AudioPlayback, ColorDepth, DEFAULT_FIXED_SIZE, DEFAULT_FTP_PORT, DEFAULT_RDP_PORT,
    DEFAULT_SSH_PORT, DEFAULT_TELNET_PORT, DEFAULT_VNC_PORT, DEFAULT_WINRM_HTTP_PORT,
    DEFAULT_WINRM_HTTPS_PORT, Forwards, FtpProfile, LocalArguments, LocalCommand, LocalProfile,
    ProfileId, RdpOptions, RdpProfile, Resolution, SshGateway, SshProfile, TelnetProfile,
    VncProfile, WinRmProfile, fixed_desktop,
};

/// `connectionType` of an SSH profile.
const SSH_CONNECTION_TYPE: &str = "SSH";

/// `connectionType` of an SFTP profile: the SSH fields, opening its files.
const SFTP_CONNECTION_TYPE: &str = "SFTP";

/// `connectionType` of an RDP profile.
const RDP_CONNECTION_TYPE: &str = "RDP";

/// `connectionType` of a Telnet profile, compared without case as the C# catalog does.
const TELNET_CONNECTION_TYPE: &str = "Telnet";

/// `connectionType` of a VNC profile, compared without case as the C# catalog does.
const VNC_CONNECTION_TYPE: &str = "VNC";

/// `connectionType` of an FTP profile, compared without case as the C# catalog does.
const FTP_CONNECTION_TYPE: &str = "FTP";

/// `connectionType` of a local shell profile, compared without case as the C# trust check
/// does.
const LOCAL_CONNECTION_TYPE: &str = "LOCAL";

/// `connectionType` of a `WinRM` profile, compared without case as the C# does.
const WINRM_CONNECTION_TYPE: &str = "WINRM";

/// The C# `WinRmIdentityMode` that logs in with a stored account, by name and by value.
const WINRM_CREDENTIAL_NAME: &str = "Credential";
const WINRM_CREDENTIAL_VALUE: i64 = 1;

/// The C# `WinRmIdentityMode` that logs in as the current user, by name and by value.
const WINRM_CURRENT_USER_NAME: &str = "CurrentUser";
const WINRM_CURRENT_USER_VALUE: i64 = 0;

/// The C# `ElevationMode` that asks for no elevation, by name and by value.
const NO_ELEVATION: &str = "None";

/// `connectionType` the C# Heimdall assumes when the field is absent.
const DEFAULT_CONNECTION_TYPE: &str = "RDP";

/// Separator of group paths.
const GROUP_SEPARATOR: char = '/';

/// Why a profile of the C# file was not imported.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkipReason {
    /// A protocol not supported yet; carries its `connectionType`.
    NotSsh(String),
    /// Names an SSH gateway that is not in the file, or that was itself left out.
    MissingGateway,
    /// An SSH gateway reached through itself, by way of its parents.
    GatewayLoop,
    /// Reaches its server through a Remote Desktop Gateway, not supported yet.
    NeedsRdGateway,
    /// Has no host.
    MissingHost,
    /// Has no identifier, so a later import could not update it.
    MissingId,
    /// Its port is outside 1 to 65535; carries the value found.
    InvalidPort(i64),
    /// A local shell run elevated, not supported yet.
    NeedsElevation,
    /// Runs commands once connected, not supported yet.
    NeedsPostConnectCommands,
    /// A `WinRM` profile logging in with an account it does not name.
    MissingUsername,
    /// A `WinRM` identity mode the C# Heimdall does not define.
    UnknownIdentityMode,
    /// A local shell whose program, arguments or folder cannot be run as written: a quote or
    /// a NUL in the program, a NUL anywhere, a relative program path, or a folder on another
    /// machine, which Windows would reach, and hand its credentials to, on its own.
    UnsafeLocalCommand,
}

/// A profile that was not imported.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skipped {
    /// Identifier in the C# file, possibly empty.
    pub id: String,
    /// Display name in the C# file.
    pub name: String,
    /// Why it was left out.
    pub reason: SkipReason,
}

/// Result of an import.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImportReport {
    /// SSH profiles ready to be merged into the store.
    pub profiles: Vec<SshProfile>,
    /// RDP profiles ready to be merged into the store.
    pub rdp: Vec<RdpProfile>,
    /// Telnet profiles ready to be merged into the store.
    pub telnet: Vec<TelnetProfile>,
    /// VNC profiles ready to be merged into the store.
    pub vnc: Vec<VncProfile>,
    /// Local shell profiles ready to be merged into the store, none approved.
    pub local: Vec<LocalProfile>,
    /// `WinRM` profiles ready to be merged into the store.
    pub winrm: Vec<WinRmProfile>,
    /// FTP profiles ready to be merged into the store.
    pub ftp: Vec<FtpProfile>,
    /// SSH gateways ready to be merged into the store; each one's parent is among them.
    pub gateways: Vec<SshGateway>,
    /// Profiles imported without some of their settings, which Heimdall-rs does not have.
    pub dropped: Vec<DroppedSettings>,
    /// The SSH servers `settings.json` trusts, and its gateways' fingerprints: for the
    /// migration of this computer's own C# store only, never from a file picked.
    pub host_keys: Vec<TrustedHostKey>,
    /// Profiles left out, with the reason.
    pub skipped: Vec<Skipped>,
}

/// A setting of a C# profile that Heimdall-rs does not have: the profile is imported
/// without it, and the user is told.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dropped {
    /// Opened in an external program (`PuTTY`, `mstsc`) rather than in a tab.
    ExternalClient,
    /// X11 forwarding.
    X11Forwarding,
    /// A `WinRM` session through an SSH gateway.
    WinRmGateway,
    /// RDP printer redirection.
    RdpPrinters,
    /// RDP serial port redirection.
    RdpComPorts,
    /// RDP smart card redirection.
    RdpSmartCards,
    /// RDP webcam redirection.
    RdpWebcam,
    /// RDP USB device redirection.
    RdpUsb,
    /// RDP microphone capture.
    RdpMicrophone,
    /// RDP across several monitors.
    RdpMultiMonitor,
}

/// A profile imported without some of its settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DroppedSettings {
    /// The profile's display name.
    pub name: String,
    /// What was left out, in a fixed order.
    pub settings: Vec<Dropped>,
}

/// An SSH server the C# Heimdall trusted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrustedHostKey {
    /// Host, as the C# wrote it.
    pub host: String,
    /// Port.
    pub port: u16,
    /// The key's fingerprint, `SHA256:...` as OpenSSH writes it.
    pub fingerprint: String,
    /// The key's SSH wire form in base64, when the C# kept it.
    pub key: Option<String>,
}

/// Why the C# files could not be read at all.
#[derive(Debug, Error)]
pub enum ImportError {
    /// `servers.json` is not valid for the expected shape.
    #[error("servers file: {0}")]
    Servers(#[source] serde_json::Error),
    /// `settings.json` is not valid for the expected shape.
    #[error("settings file: {0}")]
    Settings(#[source] serde_json::Error),
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LegacyServers {
    #[serde(default)]
    servers: Vec<LegacyServer>,
    /// The SSH gateways of an exported document (schema 2); the app's own file keeps them
    /// in `settings.json`.
    #[serde(default)]
    gateways: Vec<LegacyGateway>,
}

/// `servers.json` as the C# writes it: an object, or a bare array of servers in the first
/// export format (schema 1).
fn read_servers(text: &str) -> Result<LegacyServers, serde_json::Error> {
    let value: serde_json::Value = serde_json::from_str(text)?;
    if value.is_array() {
        Ok(LegacyServers {
            servers: serde_json::from_value(value)?,
            gateways: Vec::new(),
        })
    } else {
        serde_json::from_value(value)
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[expect(
    clippy::struct_excessive_bools,
    reason = "mirrors the flags of the C# server JSON, one field per key"
)]
struct LegacyServer {
    #[serde(default)]
    id: String,
    #[serde(default)]
    display_name: String,
    #[serde(default)]
    remote_server: String,
    #[serde(default = "default_connection_type")]
    connection_type: String,
    group: Option<String>,
    ssh_gateway_id: Option<String>,
    ssh_username: Option<String>,
    ssh_port: Option<i64>,
    ssh_key_path: Option<String>,
    remote_port: Option<i64>,
    rdp_username: Option<String>,
    rdp_domain: Option<String>,
    rdp_gateway: Option<String>,
    #[serde(default)]
    use_direct_connection: bool,
    /// Absent means the C# default: Network Level Authentication required.
    rdp_nla: Option<bool>,
    /// Absent means the C# default: the clipboard is shared.
    rdp_redirect_clipboard: Option<bool>,
    /// Absent means the C# default: no drive is shared.
    #[serde(default)]
    rdp_redirect_drives: bool,
    /// Absent means the C# default: the RDP choices of `settings.json` apply, not the
    /// profile's own.
    rdp_use_global_defaults: Option<bool>,
    /// Absent means the C# default, 32 bits.
    rdp_color_depth: Option<i64>,
    /// 0 not played (the C# default), 1 played here, 2 played on the server.
    #[serde(default)]
    rdp_audio_mode: i64,
    #[serde(default)]
    rdp_admin_mode: bool,
    /// `FitWindow`, `Fixed`, `SmartSizing`, `Multimon` or `Auto`; absent, derived as
    /// `RdpResolutionProfileMigration` does.
    rdp_resolution_mode: Option<String>,
    #[serde(rename = "rdpFixedResolutionWidth")]
    rdp_fixed_width: Option<i64>,
    #[serde(rename = "rdpFixedResolutionHeight")]
    rdp_fixed_height: Option<i64>,
    /// Older name of the fixed width, read when the current one is absent.
    rdp_default_resolution_width: Option<i64>,
    /// Older name of the fixed height.
    rdp_default_resolution_height: Option<i64>,
    /// Absent means the C# default: a fixed desktop is scaled into the pane.
    rdp_initial_smart_sizing: Option<bool>,
    /// Absent means the C# default: the desktop follows the pane.
    rdp_dynamic_resolution: Option<bool>,
    /// `Embedded` (the C# default) or `External`.
    ssh_mode: Option<String>,
    #[serde(default)]
    ssh_x11_forwarding: bool,
    /// `Embedded` (the C# default) or `External`.
    rdp_mode: Option<String>,
    #[serde(default)]
    rdp_redirect_printers: bool,
    #[serde(default)]
    rdp_redirect_com_ports: bool,
    #[serde(default)]
    rdp_redirect_smart_cards: bool,
    #[serde(default)]
    rdp_redirect_webcam: bool,
    #[serde(default)]
    rdp_redirect_usb: bool,
    #[serde(default)]
    rdp_audio_capture: bool,
    #[serde(default)]
    rdp_multi_monitor: bool,
    #[serde(default)]
    rdp_anti_idle: bool,
    /// Absent means the C# default: on.
    rdp_auto_reconnect: Option<bool>,
    /// Zero or less means the default port, as `TelnetHandler` reads it.
    telnet_port: Option<i64>,
    /// Zero or less means the default port, as `VncHandler` reads it.
    vnc_port: Option<i64>,
    /// Encrypted by the C# Heimdall; only whether it is set is read.
    vnc_password: Option<String>,
    #[serde(default)]
    vnc_view_only: bool,
    /// Heimdall-rs's own key, written by its export: the C# derives it from the stored
    /// password, which no export carries.
    vnc_allow_no_password: Option<bool>,
    /// The entry in the external password manager, for the provider's `{Title}`.
    vault_entry_name: Option<String>,
    /// "Forward SSH agent"; absent is off.
    #[serde(default)]
    ssh_agent_forwarding: bool,
    /// "Enable compression"; absent is off.
    #[serde(default)]
    ssh_compression: bool,
    /// Zero or less means the default port.
    ftp_port: Option<i64>,
    ftp_username: Option<String>,
    /// Absent means the C# default: passive.
    ftp_passive_mode: Option<bool>,
    /// "Enable SSL/TLS (FTPS)"; absent is off.
    #[serde(default)]
    ftp_use_ssl: bool,
    /// The SOCKS5 proxy's local port; 0 opens none.
    socks_proxy_port: Option<i64>,
    /// The post-connect sequence; a null entry is dropped, as the C# migration does.
    #[serde(default)]
    post_connect_steps: Vec<Option<LegacyStep>>,
    /// The sequence before steps: one command per line.
    post_connect_command: Option<String>,
    /// The gateway's port of the remote forward; 0 opens none.
    remote_bind_port: Option<i64>,
    /// The local port of the remote forward; 0 is the same port.
    remote_local_port: Option<i64>,
    local_shell_executable: Option<String>,
    local_shell_arguments: Option<String>,
    local_shell_working_directory: Option<String>,
    /// Before `elevationMode`: `true` meant elevated.
    #[serde(default)]
    local_shell_elevated: bool,
    /// A name or a number: the C# does not tie the enum to its names.
    elevation_mode: Option<serde_json::Value>,
    /// Absent or zero or less is the default port of the transport.
    win_rm_port: Option<i64>,
    win_rm_username: Option<String>,
    #[serde(default)]
    win_rm_use_ssl: bool,
    #[serde(default)]
    win_rm_skip_certificate_check: bool,
    /// A name or a number, as the C# converter accepts both.
    win_rm_identity_mode: Option<serde_json::Value>,
}

fn default_connection_type() -> String {
    DEFAULT_CONNECTION_TYPE.to_owned()
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LegacySettings {
    #[serde(default)]
    group_defaults: HashMap<String, LegacyGroupDefaults>,
    #[serde(default)]
    ssh_gateways: Vec<LegacyGateway>,
    /// `host:port` or `[ipv6]:port` to the fingerprint and, when kept, the key.
    #[serde(default, rename = "trustedHostKeysV2")]
    trusted_host_keys_v2: HashMap<String, LegacyHostKey>,
    /// The first store, fingerprints alone, still written beside the second.
    #[serde(default)]
    trusted_host_keys: HashMap<String, String>,
    #[serde(flatten)]
    rdp_defaults: LegacyRdpDefaults,
}

/// The RDP choices of `settings.json`, which a profile on the global defaults takes, as
/// `RdpProfileResolver` does. Absent keys are the C# `AppSettings` defaults.
#[derive(Debug, Default, Deserialize)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "the shape of the C# settings, one switch per redirection"
)]
struct LegacyRdpDefaults {
    #[serde(rename = "rdpDefaultRedirectClipboard")]
    clipboard: Option<bool>,
    #[serde(default, rename = "rdpDefaultRedirectDrives")]
    drives: bool,
    #[serde(rename = "rdpDefaultNla")]
    nla: Option<bool>,
    #[serde(rename = "rdpDefaultColorDepth")]
    color_depth: Option<i64>,
    #[serde(default, rename = "rdpDefaultAudioMode")]
    audio_mode: i64,
    #[serde(rename = "rdpDefaultMode")]
    mode: Option<String>,
    #[serde(default, rename = "rdpDefaultRedirectPrinters")]
    printers: bool,
    #[serde(default, rename = "rdpDefaultRedirectComPorts")]
    com_ports: bool,
    #[serde(default, rename = "rdpDefaultRedirectSmartCards")]
    smart_cards: bool,
    #[serde(default, rename = "rdpDefaultRedirectWebcam")]
    webcam: bool,
    #[serde(default, rename = "rdpDefaultRedirectUsb")]
    usb: bool,
    #[serde(default, rename = "rdpDefaultAudioCapture")]
    audio_capture: bool,
    #[serde(default, rename = "rdpDefaultMultiMonitor")]
    multi_monitor: bool,
    #[serde(rename = "rdpDefaultAutoReconnect")]
    auto_reconnect: Option<bool>,
}

/// What an RDP profile is given, from its own choices or from the global defaults.
#[expect(
    clippy::struct_excessive_bools,
    reason = "one switch per C# choice, each resolved on its own"
)]
struct RdpChoices {
    clipboard: bool,
    drives: bool,
    nla: bool,
    color_depth: Option<i64>,
    audio_mode: i64,
    auto_reconnect: bool,
    /// What the choices turn on that Heimdall-rs does not have.
    dropped: Vec<Dropped>,
}

impl RdpChoices {
    /// As `RdpProfileResolver`: the global defaults unless the profile turned them off.
    fn of(server: &LegacyServer, defaults: &LegacyRdpDefaults) -> Self {
        if server.rdp_use_global_defaults.unwrap_or(true) {
            Self {
                clipboard: defaults.clipboard.unwrap_or(true),
                drives: defaults.drives,
                nla: defaults.nla.unwrap_or(true),
                color_depth: defaults.color_depth,
                audio_mode: defaults.audio_mode,
                auto_reconnect: defaults.auto_reconnect.unwrap_or(true),
                dropped: turned_on(&[
                    (
                        is_external(defaults.mode.as_deref()),
                        Dropped::ExternalClient,
                    ),
                    (defaults.printers, Dropped::RdpPrinters),
                    (defaults.com_ports, Dropped::RdpComPorts),
                    (defaults.smart_cards, Dropped::RdpSmartCards),
                    (defaults.webcam, Dropped::RdpWebcam),
                    (defaults.usb, Dropped::RdpUsb),
                    (defaults.audio_capture, Dropped::RdpMicrophone),
                    (defaults.multi_monitor, Dropped::RdpMultiMonitor),
                ]),
            }
        } else {
            Self {
                clipboard: server.rdp_redirect_clipboard.unwrap_or(true),
                drives: server.rdp_redirect_drives,
                nla: server.rdp_nla.unwrap_or(true),
                color_depth: server.rdp_color_depth,
                audio_mode: server.rdp_audio_mode,
                auto_reconnect: server.rdp_auto_reconnect.unwrap_or(true),
                dropped: turned_on(&[
                    (
                        is_external(server.rdp_mode.as_deref()),
                        Dropped::ExternalClient,
                    ),
                    (server.rdp_redirect_printers, Dropped::RdpPrinters),
                    (server.rdp_redirect_com_ports, Dropped::RdpComPorts),
                    (server.rdp_redirect_smart_cards, Dropped::RdpSmartCards),
                    (server.rdp_redirect_webcam, Dropped::RdpWebcam),
                    (server.rdp_redirect_usb, Dropped::RdpUsb),
                    (server.rdp_audio_capture, Dropped::RdpMicrophone),
                    (server.rdp_multi_monitor, Dropped::RdpMultiMonitor),
                ]),
            }
        }
    }

    /// The C# audio mode: not played, played here, or played on the server.
    fn audio(&self) -> AudioPlayback {
        match self.audio_mode {
            CSHARP_AUDIO_LOCAL => AudioPlayback::Local,
            CSHARP_AUDIO_ON_SERVER => AudioPlayback::OnServer,
            _ => AudioPlayback::Off,
        }
    }
}

/// What an imported profile turned on that Heimdall-rs does not have.
fn dropped_settings(server: &LegacyServer, defaults: &LegacyRdpDefaults) -> Vec<Dropped> {
    let kind = server.connection_type.as_str();
    if kind == RDP_CONNECTION_TYPE {
        RdpChoices::of(server, defaults).dropped
    } else if kind.eq_ignore_ascii_case(WINRM_CONNECTION_TYPE) {
        let gateway = non_empty(server.ssh_gateway_id.as_ref()).is_some();
        turned_on(&[(
            gateway && !server.use_direct_connection,
            Dropped::WinRmGateway,
        )])
    } else if [
        LOCAL_CONNECTION_TYPE,
        TELNET_CONNECTION_TYPE,
        VNC_CONNECTION_TYPE,
        FTP_CONNECTION_TYPE,
    ]
    .iter()
    .any(|other| kind.eq_ignore_ascii_case(other))
    {
        Vec::new()
    } else {
        turned_on(&[
            (
                is_external(server.ssh_mode.as_deref()),
                Dropped::ExternalClient,
            ),
            (server.ssh_x11_forwarding, Dropped::X11Forwarding),
        ])
    }
}

/// The settings whose flag is set, in the order given.
fn turned_on(flags: &[(bool, Dropped)]) -> Vec<Dropped> {
    flags
        .iter()
        .filter(|(on, _)| *on)
        .map(|(_, dropped)| *dropped)
        .collect()
}

/// Whether a C# mode opens the session in an external program.
fn is_external(mode: Option<&str>) -> bool {
    mode.is_some_and(|mode| mode.trim().eq_ignore_ascii_case(EXTERNAL_MODE))
}

/// The C# mode that opens a session in `PuTTY` or `mstsc`.
const EXTERNAL_MODE: &str = "External";

/// The C# audio mode "Local playback".
const CSHARP_AUDIO_LOCAL: i64 = 1;
/// The C# audio mode "Remote playback".
const CSHARP_AUDIO_ON_SERVER: i64 = 2;

/// How the C# embedded session sizes the desktop of `server`: its resolution settings are
/// always the profile's own, never global defaults.
fn resolution_of(server: &LegacyServer) -> (Resolution, (u16, u16)) {
    let width = server
        .rdp_fixed_width
        .or(server.rdp_default_resolution_width)
        .unwrap_or(0);
    let height = server
        .rdp_fixed_height
        .or(server.rdp_default_resolution_height)
        .unwrap_or(0);
    let sized = width > 0 && height > 0;
    let mode = match server.rdp_resolution_mode.as_deref() {
        Some(mode) if mode.eq_ignore_ascii_case("Fixed") => Resolution::Fixed,
        Some(mode) if mode.eq_ignore_ascii_case("SmartSizing") => Resolution::SmartSizing,
        None if sized => Resolution::Fixed,
        // Multi-monitor and Auto have no sense in a tab: Auto windowed fits the window.
        Some(_) | None => Resolution::FitWindow,
    };
    if !sized {
        // A fixed mode without its size follows the pane in the C# Heimdall.
        let mode = if mode == Resolution::Fixed {
            Resolution::FitWindow
        } else {
            mode
        };
        return (mode, DEFAULT_FIXED_SIZE);
    }
    let side = |value: i64| u16::try_from(value).unwrap_or(u16::MAX);
    (mode, fixed_desktop(side(width), side(height)))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LegacyHostKey {
    #[serde(default)]
    fingerprint: String,
    public_key_base64: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LegacyGateway {
    #[serde(default)]
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    host: String,
    /// Absent is the SSH default, as `SshGatewayDto` initialises it.
    port: Option<i64>,
    user: Option<String>,
    key_path: Option<String>,
    parent_gateway_id: Option<String>,
    /// The gateway's host key, pinned by fingerprint in its own settings.
    host_key_fingerprint: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LegacyGroupDefaults {
    ssh_gateway_id: Option<String>,
    ssh_username: Option<String>,
    ssh_key_path: Option<String>,
    ssh_port: Option<i64>,
    connection_type: Option<String>,
}

/// Imports the SSH profiles from the text of `servers.json` and, when available, of
/// `settings.json`.
///
/// # Errors
///
/// Returns [`ImportError`] when either text is not JSON of the expected shape. A profile that
/// cannot be imported is not an error: it is listed in [`ImportReport::skipped`].
pub fn import(
    servers_json: &str,
    settings_json: Option<&str>,
) -> Result<ImportReport, ImportError> {
    let mut servers = read_servers(servers_json).map_err(ImportError::Servers)?;
    let settings: LegacySettings = match settings_json {
        Some(text) => serde_json::from_str(text).map_err(ImportError::Settings)?,
        None => LegacySettings::default(),
    };

    let mut report = ImportReport {
        host_keys: trusted_host_keys(&settings),
        ..ImportReport::default()
    };
    // The document's gateways, then the settings' ones: an identifier seen twice is the
    // first one's.
    let mut legacy_gateways = std::mem::take(&mut servers.gateways);
    let mut seen: HashSet<String> = legacy_gateways.iter().map(|g| g.id.clone()).collect();
    for gateway in settings.ssh_gateways {
        if seen.insert(gateway.id.clone()) {
            legacy_gateways.push(gateway);
        }
    }
    let (gateways, skipped_gateways) = convert_gateways(&legacy_gateways);
    let known: HashSet<&str> = gateways.iter().map(|gateway| gateway.id.as_str()).collect();
    for mut server in servers.servers {
        resolve_group_defaults(server.group.as_deref(), &settings.group_defaults)
            .apply_to(&mut server);
        let converted = if server
            .connection_type
            .eq_ignore_ascii_case(LOCAL_CONNECTION_TYPE)
        {
            convert_local(&server).map(|profile| report.local.push(profile))
        } else if server
            .connection_type
            .eq_ignore_ascii_case(WINRM_CONNECTION_TYPE)
        {
            convert_winrm(&server).map(|profile| report.winrm.push(profile))
        } else if server.connection_type == RDP_CONNECTION_TYPE {
            convert_rdp(&server, &known, &settings.rdp_defaults)
                .map(|profile| report.rdp.push(profile))
        } else if server
            .connection_type
            .eq_ignore_ascii_case(TELNET_CONNECTION_TYPE)
        {
            convert_telnet(&server).map(|profile| report.telnet.push(profile))
        } else if server
            .connection_type
            .eq_ignore_ascii_case(VNC_CONNECTION_TYPE)
        {
            convert_vnc(&server).map(|profile| report.vnc.push(profile))
        } else if server
            .connection_type
            .eq_ignore_ascii_case(FTP_CONNECTION_TYPE)
        {
            convert_ftp(&server).map(|profile| report.ftp.push(profile))
        } else {
            convert(&server, &known).map(|profile| report.profiles.push(profile))
        };
        match converted {
            Ok(()) => {
                let left_out = dropped_settings(&server, &settings.rdp_defaults);
                if !left_out.is_empty() {
                    report.dropped.push(DroppedSettings {
                        name: if server.display_name.is_empty() {
                            server.remote_server.clone()
                        } else {
                            server.display_name.clone()
                        },
                        settings: left_out,
                    });
                }
            }
            Err(reason) => report.skipped.push(Skipped {
                id: server.id,
                name: server.display_name,
                reason,
            }),
        }
    }
    report.skipped.extend(skipped_gateways);
    report.gateways = gateways;
    Ok(report)
}

/// The servers `settings.json` trusts, sorted by host and port: the second store's entries,
/// the first store's for a server the second does not name, then a gateway's own
/// fingerprint for a server neither names. Entries without a host, a port or a fingerprint
/// are left out.
fn trusted_host_keys(settings: &LegacySettings) -> Vec<TrustedHostKey> {
    let mut trusted: Vec<TrustedHostKey> = Vec::new();
    let mut add = |host: &str, port: u16, fingerprint: &str, key: Option<&String>| {
        let fingerprint = fingerprint.trim();
        if host.is_empty()
            || fingerprint.is_empty()
            || trusted
                .iter()
                .any(|known| known.host.eq_ignore_ascii_case(host) && known.port == port)
        {
            return;
        }
        trusted.push(TrustedHostKey {
            host: host.to_owned(),
            port,
            fingerprint: fingerprint.to_owned(),
            key: key.filter(|key| !key.trim().is_empty()).cloned(),
        });
    };
    let mut second: Vec<_> = settings.trusted_host_keys_v2.iter().collect();
    second.sort_by(|a, b| a.0.cmp(b.0));
    for (name, entry) in second {
        if let Some((host, port)) = host_and_port(name) {
            add(
                host,
                port,
                &entry.fingerprint,
                entry.public_key_base64.as_ref(),
            );
        }
    }
    let mut first: Vec<_> = settings.trusted_host_keys.iter().collect();
    first.sort();
    for (name, fingerprint) in first {
        if let Some((host, port)) = host_and_port(name) {
            add(host, port, fingerprint, None);
        }
    }
    for gateway in &settings.ssh_gateways {
        let port = gateway.port.map_or(Some(DEFAULT_SSH_PORT), |port| {
            u16::try_from(port).ok().filter(|port| *port != 0)
        });
        if let (Some(port), Some(fingerprint)) = (port, &gateway.host_key_fingerprint) {
            add(gateway.host.trim(), port, fingerprint, None);
        }
    }
    trusted
}

/// `host:port` or `[ipv6]:port`, as the C# keys its trust store.
fn host_and_port(name: &str) -> Option<(&str, u16)> {
    let (host, port) = match name.strip_prefix('[') {
        Some(bracketed) => bracketed.split_once("]:")?,
        None => name.rsplit_once(':')?,
    };
    let port = port.parse().ok().filter(|port| *port != 0)?;
    (!host.is_empty()).then_some((host, port))
}

/// The gateways that can be used: each with an identifier, a host and a valid port, and a
/// parent that is itself usable; a chain of parents that comes back on itself is left out
/// whole. The others are listed with the reason.
fn convert_gateways(legacy: &[LegacyGateway]) -> (Vec<SshGateway>, Vec<Skipped>) {
    let mut skipped = Vec::new();
    let mut candidates: HashMap<&str, SshGateway> = HashMap::new();
    for gateway in legacy {
        let port = match gateway.port {
            None => Ok(DEFAULT_SSH_PORT),
            Some(value) => u16::try_from(value)
                .ok()
                .filter(|port| *port != 0)
                .ok_or(SkipReason::InvalidPort(value)),
        };
        let checked = if gateway.id.is_empty() {
            Err(SkipReason::MissingId)
        } else if gateway.host.trim().is_empty() {
            Err(SkipReason::MissingHost)
        } else {
            port
        };
        match checked {
            Ok(port) => {
                candidates.insert(
                    gateway.id.as_str(),
                    SshGateway {
                        id: ProfileId::new(gateway.id.clone()),
                        name: if gateway.name.is_empty() {
                            gateway.host.trim().to_owned()
                        } else {
                            gateway.name.clone()
                        },
                        host: gateway.host.trim().to_owned(),
                        port,
                        username: non_empty(gateway.user.as_ref()),
                        key_path: non_empty(gateway.key_path.as_ref()).map(PathBuf::from),
                        parent: non_empty(gateway.parent_gateway_id.as_ref()).map(ProfileId::new),
                    },
                );
            }
            Err(reason) => skipped.push(Skipped {
                id: gateway.id.clone(),
                name: gateway.name.clone(),
                reason,
            }),
        }
    }
    // Kept in the file's order; each one's whole chain of parents must be kept too.
    let mut kept = Vec::new();
    for gateway in legacy {
        let Some(candidate) = candidates.get(gateway.id.as_str()) else {
            continue;
        };
        match chain_ends(candidate, &candidates) {
            Ok(()) => kept.push(candidate.clone()),
            Err(reason) => skipped.push(Skipped {
                id: gateway.id.clone(),
                name: gateway.name.clone(),
                reason,
            }),
        }
    }
    (kept, skipped)
}

/// Whether following `gateway`'s parents ends on one with none, every parent there.
fn chain_ends(gateway: &SshGateway, all: &HashMap<&str, SshGateway>) -> Result<(), SkipReason> {
    let mut seen = HashSet::from([gateway.id.as_str()]);
    let mut current = gateway;
    while let Some(parent) = &current.parent {
        let Some(next) = all.get(parent.as_str()) else {
            return Err(SkipReason::MissingGateway);
        };
        if !seen.insert(next.id.as_str()) {
            return Err(SkipReason::GatewayLoop);
        }
        current = next;
    }
    Ok(())
}

/// Group defaults in force for `group`, as `GroupDefaultsDto.Resolve` computes them.
fn resolve_group_defaults(
    group: Option<&str>,
    all: &HashMap<String, LegacyGroupDefaults>,
) -> LegacyGroupDefaults {
    let mut result = LegacyGroupDefaults::default();
    let Some(group) = group.filter(|group| !group.trim().is_empty()) else {
        return result;
    };
    for ancestor in ancestors(group) {
        if let Some(defaults) = all.get(ancestor) {
            // `??=` in the C#: only an absent value is filled, an empty string counts as set.
            fill(&mut result.ssh_gateway_id, defaults.ssh_gateway_id.as_ref());
            fill(&mut result.ssh_username, defaults.ssh_username.as_ref());
            fill(&mut result.ssh_key_path, defaults.ssh_key_path.as_ref());
            if result.ssh_port.is_none() {
                result.ssh_port = defaults.ssh_port;
            }
            fill(
                &mut result.connection_type,
                defaults.connection_type.as_ref(),
            );
        }
    }
    result
}

/// `"PROD/Linux/Web"` yields `PROD/Linux/Web`, `PROD/Linux`, `PROD`, deepest first.
fn ancestors(group: &str) -> impl Iterator<Item = &str> {
    std::iter::successors(Some(group), |path| match path.rfind(GROUP_SEPARATOR) {
        Some(index) if index > 0 => Some(&path[..index]),
        _ => None,
    })
    .filter(|path| !path.is_empty())
}

fn fill(target: &mut Option<String>, source: Option<&String>) {
    if target.is_none() {
        *target = source.cloned();
    }
}

fn is_null_or_empty(value: Option<&String>) -> bool {
    value.is_none_or(String::is_empty)
}

impl LegacyGroupDefaults {
    /// Fills the fields `server` leaves unset, as `GroupDefaultsDto.ApplyTo` does.
    fn apply_to(&self, server: &mut LegacyServer) {
        if is_null_or_empty(server.ssh_gateway_id.as_ref()) {
            server.ssh_gateway_id.clone_from(&self.ssh_gateway_id);
        }
        if is_null_or_empty(server.ssh_username.as_ref()) {
            server.ssh_username.clone_from(&self.ssh_username);
        }
        if is_null_or_empty(server.ssh_key_path.as_ref()) {
            server.ssh_key_path.clone_from(&self.ssh_key_path);
        }
        if server.ssh_port.is_none() {
            server.ssh_port = self.ssh_port;
        }
        if server.connection_type.is_empty()
            && let Some(connection_type) = self.connection_type.as_ref().filter(|c| !c.is_empty())
        {
            server.connection_type.clone_from(connection_type);
        }
    }
}

fn non_empty(value: Option<&String>) -> Option<String> {
    value.filter(|value| !value.is_empty()).cloned()
}

/// An SSH or SFTP profile, through its gateway when it names one among `gateways`.
fn convert(server: &LegacyServer, gateways: &HashSet<&str>) -> Result<SshProfile, SkipReason> {
    let sftp = server.connection_type == SFTP_CONNECTION_TYPE;
    if server.connection_type != SSH_CONNECTION_TYPE && !sftp {
        return Err(SkipReason::NotSsh(server.connection_type.clone()));
    }
    if server.id.is_empty() {
        return Err(SkipReason::MissingId);
    }
    if server.remote_server.trim().is_empty() {
        return Err(SkipReason::MissingHost);
    }
    let gateway = match non_empty(server.ssh_gateway_id.as_ref()) {
        None => None,
        Some(id) if gateways.contains(id.as_str()) => Some(ProfileId::new(id)),
        Some(_) => return Err(SkipReason::MissingGateway),
    };
    let port = match server.ssh_port {
        None => DEFAULT_SSH_PORT,
        Some(value) => match u16::try_from(value) {
            Ok(port) if port != 0 => port,
            _ => return Err(SkipReason::InvalidPort(value)),
        },
    };
    Ok(SshProfile {
        id: ProfileId::new(server.id.clone()),
        name: if server.display_name.is_empty() {
            server.remote_server.clone()
        } else {
            server.display_name.clone()
        },
        group: non_empty(server.group.as_ref()),
        host: server.remote_server.trim().to_owned(),
        port,
        username: non_empty(server.ssh_username.as_ref()),
        key_path: non_empty(server.ssh_key_path.as_ref()).map(PathBuf::from),
        gateway,
        vault_entry: non_empty(server.vault_entry_name.as_ref()),
        forwards: forwards_of(server)?,
        post_connect: PostConnect {
            steps: post_connect_of(server),
            approved: None,
        },
        forward_agent: server.ssh_agent_forwarding,
        compression: server.ssh_compression,
        sftp,
    })
}

/// A post-connect step as the C# writes it.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LegacyStep {
    #[serde(default)]
    input: String,
    delay_ms: Option<i64>,
    /// Absent is on, as a new C# step is.
    enabled: Option<bool>,
    /// A command of the C# Command Library, which Heimdall-rs has not.
    command_library_id: Option<String>,
    /// 0 or `Continue`, 1 or `Stop`: the C# writes the number.
    on_failure: Option<serde_json::Value>,
}

/// The C# `PostConnectFailurePolicy` value meaning "stop".
const STOP_POLICY: i64 = 1;

/// The post-connect sequence as `PostConnectMigration` reads it: the steps, or else the old
/// command field, one step per line. A step linked to the C# Command Library keeps only its
/// own text, which is typed if there is any. Never approved: the user approves what arrives.
fn post_connect_of(server: &LegacyServer) -> Vec<PostConnectStep> {
    let steps: Vec<PostConnectStep> = server
        .post_connect_steps
        .iter()
        .flatten()
        .map(|step| PostConnectStep {
            input: step.input.clone(),
            delay_ms: step.delay_ms.map_or(DEFAULT_STEP_DELAY_MS, |delay| {
                u32::try_from(delay.max(0)).unwrap_or(u32::MAX)
            }),
            enabled: step.enabled.unwrap_or(true),
            on_failure: match &step.on_failure {
                Some(serde_json::Value::Number(number)) if number.as_i64() == Some(STOP_POLICY) => {
                    OnFailure::Stop
                }
                Some(serde_json::Value::String(name)) if name.eq_ignore_ascii_case("stop") => {
                    OnFailure::Stop
                }
                _ => OnFailure::Continue,
            },
        })
        .collect();
    if !steps.is_empty() {
        return steps;
    }
    server
        .post_connect_command
        .as_deref()
        .unwrap_or_default()
        .split('\n')
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(PostConnectStep::new)
        .collect()
}

/// The ports a profile opens through its gateway; a port out of range skips it, as the C#
/// import refuses it.
fn forwards_of(server: &LegacyServer) -> Result<Forwards, SkipReason> {
    let port = |value: Option<i64>| match value {
        None => Ok(None),
        Some(value) => match u16::try_from(value) {
            Ok(port) => Ok(Some(port).filter(|port| *port != 0)),
            Err(_) => Err(SkipReason::InvalidPort(value)),
        },
    };
    Ok(Forwards {
        socks_port: port(server.socks_proxy_port)?,
        remote_bind_port: port(server.remote_bind_port)?,
        remote_local_port: port(server.remote_local_port)?,
    })
}

/// An RDP profile, through its SSH gateway when it goes through one among `gateways`.
fn convert_rdp(
    server: &LegacyServer,
    gateways: &HashSet<&str>,
    defaults: &LegacyRdpDefaults,
) -> Result<RdpProfile, SkipReason> {
    if server.id.is_empty() {
        return Err(SkipReason::MissingId);
    }
    if server.remote_server.trim().is_empty() {
        return Err(SkipReason::MissingHost);
    }
    // As `ConnectionService` decides: through the SSH gateway unless the profile asks for a
    // direct connection or names no gateway.
    let gateway = match non_empty(server.ssh_gateway_id.as_ref()) {
        Some(_) if server.use_direct_connection => None,
        None => None,
        Some(id) if gateways.contains(id.as_str()) => Some(ProfileId::new(id)),
        Some(_) => return Err(SkipReason::MissingGateway),
    };
    if server
        .rdp_gateway
        .as_ref()
        .is_some_and(|gateway| !gateway.trim().is_empty())
    {
        return Err(SkipReason::NeedsRdGateway);
    }
    let port = match server.remote_port {
        None => DEFAULT_RDP_PORT,
        Some(value) => match u16::try_from(value) {
            Ok(port) if port != 0 => port,
            _ => return Err(SkipReason::InvalidPort(value)),
        },
    };
    let choices = RdpChoices::of(server, defaults);
    let (resolution, (fixed_width, fixed_height)) = resolution_of(server);
    Ok(RdpProfile {
        id: ProfileId::new(server.id.clone()),
        name: if server.display_name.is_empty() {
            server.remote_server.clone()
        } else {
            server.display_name.clone()
        },
        group: non_empty(server.group.as_ref()),
        host: server.remote_server.trim().to_owned(),
        port,
        username: non_empty(server.rdp_username.as_ref()),
        domain: non_empty(server.rdp_domain.as_ref()),
        allow_tls_only: !choices.nla,
        gateway,
        redirect_clipboard: choices.clipboard,
        redirect_drives: choices.drives,
        options: RdpOptions {
            color_depth: choices
                .color_depth
                .map_or_else(ColorDepth::default, ColorDepth::nearest),
            audio: choices.audio(),
            // Not one of the global defaults in the C# Heimdall: always the profile's.
            admin_session: server.rdp_admin_mode,
            resolution,
            fixed_width,
            fixed_height,
            scale_fixed: server.rdp_initial_smart_sizing.unwrap_or(true),
            dynamic_resolution: server.rdp_dynamic_resolution.unwrap_or(true),
        },
        vault_entry: non_empty(server.vault_entry_name.as_ref()),
        forwards: forwards_of(server)?,
        follow_defaults: false,
        several_servers: false,
        // Not one of the global defaults in the C# Heimdall either.
        anti_idle: server.rdp_anti_idle,
        auto_reconnect: choices.auto_reconnect,
    })
}

/// A Telnet profile as `TelnetHandler` connects it: directly, never through a gateway.
fn convert_telnet(server: &LegacyServer) -> Result<TelnetProfile, SkipReason> {
    if server.id.is_empty() {
        return Err(SkipReason::MissingId);
    }
    if server.remote_server.trim().is_empty() {
        return Err(SkipReason::MissingHost);
    }
    let port = match server.telnet_port {
        Some(value) if value > 0 => {
            u16::try_from(value).map_err(|_| SkipReason::InvalidPort(value))?
        }
        _ => DEFAULT_TELNET_PORT,
    };
    Ok(TelnetProfile {
        id: ProfileId::new(server.id.clone()),
        name: if server.display_name.is_empty() {
            server.remote_server.clone()
        } else {
            server.display_name.clone()
        },
        group: non_empty(server.group.as_ref()),
        host: server.remote_server.trim().to_owned(),
        port,
    })
}

/// A VNC profile as `VncHandler` connects it: directly. The C# stored password is not
/// carried over; a profile that had none reached its server without one, and keeps doing so.
fn convert_vnc(server: &LegacyServer) -> Result<VncProfile, SkipReason> {
    if server.id.is_empty() {
        return Err(SkipReason::MissingId);
    }
    if server.remote_server.trim().is_empty() {
        return Err(SkipReason::MissingHost);
    }
    let port = match server.vnc_port {
        Some(value) if value > 0 => {
            u16::try_from(value).map_err(|_| SkipReason::InvalidPort(value))?
        }
        _ => DEFAULT_VNC_PORT,
    };
    Ok(VncProfile {
        id: ProfileId::new(server.id.clone()),
        name: if server.display_name.is_empty() {
            server.remote_server.clone()
        } else {
            server.display_name.clone()
        },
        group: non_empty(server.group.as_ref()),
        host: server.remote_server.trim().to_owned(),
        port,
        view_only: server.vnc_view_only,
        allow_no_password: server
            .vnc_allow_no_password
            .unwrap_or_else(|| is_null_or_empty(server.vnc_password.as_ref())),
        vault_entry: non_empty(server.vault_entry_name.as_ref()),
    })
}

/// An FTP profile as `FtpHandler` connects it: directly, never through a gateway.
fn convert_ftp(server: &LegacyServer) -> Result<FtpProfile, SkipReason> {
    if server.id.is_empty() {
        return Err(SkipReason::MissingId);
    }
    if server.remote_server.trim().is_empty() {
        return Err(SkipReason::MissingHost);
    }
    let port = match server.ftp_port {
        None => DEFAULT_FTP_PORT,
        Some(value) if value <= 0 => DEFAULT_FTP_PORT,
        Some(value) => u16::try_from(value).map_err(|_| SkipReason::InvalidPort(value))?,
    };
    Ok(FtpProfile {
        id: ProfileId::new(server.id.clone()),
        name: if server.display_name.is_empty() {
            server.remote_server.clone()
        } else {
            server.display_name.clone()
        },
        group: non_empty(server.group.as_ref()),
        host: server.remote_server.trim().to_owned(),
        port,
        username: non_empty(server.ftp_username.as_ref()),
        passive: server.ftp_passive_mode.unwrap_or(true),
        tls: server.ftp_use_ssl,
        vault_entry: non_empty(server.vault_entry_name.as_ref()),
    })
}

/// A local shell profile as `LocalShellHandler` runs it, never approved: whatever the C# file
/// says was confirmed there, the user has not seen it here. The arguments stay the string the
/// C# handed to the program.
fn convert_local(server: &LegacyServer) -> Result<LocalProfile, SkipReason> {
    if server.id.is_empty() {
        return Err(SkipReason::MissingId);
    }
    if is_elevated(server) {
        return Err(SkipReason::NeedsElevation);
    }
    if server.post_connect_steps.iter().flatten().any(|step| {
        step.enabled.unwrap_or(true)
            && (!step.input.trim().is_empty() || !is_blank(step.command_library_id.as_deref()))
    }) {
        return Err(SkipReason::NeedsPostConnectCommands);
    }
    // Blank is absent, as `string.IsNullOrWhiteSpace` makes it in the C#.
    let program = trimmed(server.local_shell_executable.as_deref());
    let working_directory = trimmed(server.local_shell_working_directory.as_deref());
    let arguments = match server.local_shell_arguments.as_deref() {
        Some(line) if !line.trim().is_empty() => LocalArguments::WindowsLine(line.to_owned()),
        _ => LocalArguments::default(),
    };
    if !is_safe_local(program.as_deref(), &arguments, working_directory.as_deref()) {
        return Err(SkipReason::UnsafeLocalCommand);
    }
    Ok(LocalProfile {
        id: ProfileId::new(server.id.clone()),
        name: if server.display_name.is_empty() {
            program.clone().unwrap_or_else(|| server.id.clone())
        } else {
            server.display_name.clone()
        },
        group: non_empty(server.group.as_ref()),
        command: LocalCommand {
            program,
            arguments,
            working_directory: working_directory.map(PathBuf::from),
        },
        approved: None,
    })
}

/// Whether the profile asks to run elevated, as `EffectiveElevationMode` reads it.
fn is_elevated(server: &LegacyServer) -> bool {
    let by_mode = match &server.elevation_mode {
        None | Some(serde_json::Value::Null) => false,
        Some(serde_json::Value::Number(number)) => number.as_i64() != Some(0),
        Some(serde_json::Value::String(name)) => !name.eq_ignore_ascii_case(NO_ELEVATION),
        // Anything else is not a mode the C# wrote: taken as a request, never as none.
        Some(_) => true,
    };
    by_mode || server.local_shell_elevated
}

/// Whether the command can be run as written, without a folder that depends on how the
/// program is started and without reaching another machine.
fn is_safe_local(
    program: Option<&str>,
    arguments: &LocalArguments,
    working_directory: Option<&str>,
) -> bool {
    let line = match arguments {
        LocalArguments::WindowsLine(line) => line.as_str(),
        LocalArguments::List(_) => "",
    };
    let program_ok = program.is_none_or(|program| {
        !program.contains(['"', '\0'])
            && (is_windows_absolute(program) || !program.contains(['\\', '/']))
    });
    // A drive letter is required: it also keeps out `\\host\share` and the `\\?\` forms.
    let folder_ok = working_directory
        .is_none_or(|folder| !folder.contains('\0') && is_windows_absolute(folder));
    program_ok && folder_ok && !line.contains('\0')
}

/// `C:\...` or `C:/...`: a path with a drive and a root, the only absolute form kept.
fn is_windows_absolute(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && (bytes[2] == b'\\' || bytes[2] == b'/')
}

fn is_blank(value: Option<&str>) -> bool {
    value.is_none_or(|value| value.trim().is_empty())
}

fn trimmed(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

/// A `WinRM` profile as `WinRmPowerShellLaunchBuilder` connects it: directly, never through a
/// gateway. The stored password is not carried over: `PowerShell` asks for it.
fn convert_winrm(server: &LegacyServer) -> Result<WinRmProfile, SkipReason> {
    if server.id.is_empty() {
        return Err(SkipReason::MissingId);
    }
    if server.remote_server.trim().is_empty() {
        return Err(SkipReason::MissingHost);
    }
    let port = match server.win_rm_port {
        Some(value) if value > 0 => {
            u16::try_from(value).map_err(|_| SkipReason::InvalidPort(value))?
        }
        _ if server.win_rm_use_ssl => DEFAULT_WINRM_HTTPS_PORT,
        _ => DEFAULT_WINRM_HTTP_PORT,
    };
    let username = if winrm_uses_credential(server.win_rm_identity_mode.as_ref())? {
        Some(
            non_empty(server.win_rm_username.as_ref())
                .map(|name| name.trim().to_owned())
                .filter(|name| !name.is_empty())
                .ok_or(SkipReason::MissingUsername)?,
        )
    } else {
        None
    };
    Ok(WinRmProfile {
        id: ProfileId::new(server.id.clone()),
        name: if server.display_name.is_empty() {
            server.remote_server.clone()
        } else {
            server.display_name.clone()
        },
        group: non_empty(server.group.as_ref()),
        host: server.remote_server.trim().to_owned(),
        port,
        use_ssl: server.win_rm_use_ssl,
        skip_certificate_check: server.win_rm_use_ssl && server.win_rm_skip_certificate_check,
        username,
    })
}

/// Whether the identity mode logs in with a stored account; absent is the current user.
fn winrm_uses_credential(mode: Option<&serde_json::Value>) -> Result<bool, SkipReason> {
    match mode {
        None | Some(serde_json::Value::Null) => Ok(false),
        Some(serde_json::Value::Number(number)) => match number.as_i64() {
            Some(WINRM_CURRENT_USER_VALUE) => Ok(false),
            Some(WINRM_CREDENTIAL_VALUE) => Ok(true),
            _ => Err(SkipReason::UnknownIdentityMode),
        },
        Some(serde_json::Value::String(name))
            if name.eq_ignore_ascii_case(WINRM_CURRENT_USER_NAME) =>
        {
            Ok(false)
        }
        Some(serde_json::Value::String(name))
            if name.eq_ignore_ascii_case(WINRM_CREDENTIAL_NAME) =>
        {
            Ok(true)
        }
        Some(_) => Err(SkipReason::UnknownIdentityMode),
    }
}
