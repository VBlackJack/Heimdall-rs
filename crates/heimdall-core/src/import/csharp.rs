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

use crate::profile::{
    DEFAULT_RDP_PORT, DEFAULT_SSH_PORT, DEFAULT_TELNET_PORT, DEFAULT_VNC_PORT, LocalArguments,
    LocalCommand, LocalProfile, ProfileId, RdpProfile, SshGateway, SshProfile, TelnetProfile,
    VncProfile,
};

/// `connectionType` of an SSH profile.
const SSH_CONNECTION_TYPE: &str = "SSH";

/// `connectionType` of an RDP profile.
const RDP_CONNECTION_TYPE: &str = "RDP";

/// `connectionType` of a Telnet profile, compared without case as the C# catalog does.
const TELNET_CONNECTION_TYPE: &str = "Telnet";

/// `connectionType` of a VNC profile, compared without case as the C# catalog does.
const VNC_CONNECTION_TYPE: &str = "VNC";

/// `connectionType` of a local shell profile, compared without case as the C# trust check
/// does.
const LOCAL_CONNECTION_TYPE: &str = "LOCAL";

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
    /// SSH gateways ready to be merged into the store; each one's parent is among them.
    pub gateways: Vec<SshGateway>,
    /// Profiles left out, with the reason.
    pub skipped: Vec<Skipped>,
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
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
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
    /// Zero or less means the default port, as `TelnetHandler` reads it.
    telnet_port: Option<i64>,
    /// Zero or less means the default port, as `VncHandler` reads it.
    vnc_port: Option<i64>,
    /// Encrypted by the C# Heimdall; only whether it is set is read.
    vnc_password: Option<String>,
    #[serde(default)]
    vnc_view_only: bool,
    local_shell_executable: Option<String>,
    local_shell_arguments: Option<String>,
    local_shell_working_directory: Option<String>,
    /// Before `elevationMode`: `true` meant elevated.
    #[serde(default)]
    local_shell_elevated: bool,
    /// A name or a number: the C# does not tie the enum to its names.
    elevation_mode: Option<serde_json::Value>,
    #[serde(default)]
    post_connect_steps: Vec<LegacyPostConnectStep>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LegacyPostConnectStep {
    #[serde(default)]
    enabled: bool,
    input: Option<String>,
    command_library_id: Option<String>,
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
    let servers: LegacyServers =
        serde_json::from_str(servers_json).map_err(ImportError::Servers)?;
    let settings: LegacySettings = match settings_json {
        Some(text) => serde_json::from_str(text).map_err(ImportError::Settings)?,
        None => LegacySettings::default(),
    };

    let mut report = ImportReport::default();
    let (gateways, skipped_gateways) = convert_gateways(&settings.ssh_gateways);
    let known: HashSet<&str> = gateways.iter().map(|gateway| gateway.id.as_str()).collect();
    for mut server in servers.servers {
        resolve_group_defaults(server.group.as_deref(), &settings.group_defaults)
            .apply_to(&mut server);
        let converted = if server
            .connection_type
            .eq_ignore_ascii_case(LOCAL_CONNECTION_TYPE)
        {
            convert_local(&server).map(|profile| report.local.push(profile))
        } else if server.connection_type == RDP_CONNECTION_TYPE {
            convert_rdp(&server, &known).map(|profile| report.rdp.push(profile))
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
        } else {
            convert(&server, &known).map(|profile| report.profiles.push(profile))
        };
        match converted {
            Ok(()) => {}
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

/// An SSH profile, through its gateway when it names one among `gateways`.
fn convert(server: &LegacyServer, gateways: &HashSet<&str>) -> Result<SshProfile, SkipReason> {
    if server.connection_type != SSH_CONNECTION_TYPE {
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
    })
}

/// An RDP profile, through its SSH gateway when it goes through one among `gateways`.
fn convert_rdp(server: &LegacyServer, gateways: &HashSet<&str>) -> Result<RdpProfile, SkipReason> {
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
        allow_tls_only: server.rdp_nla == Some(false),
        gateway,
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
        allow_no_password: is_null_or_empty(server.vnc_password.as_ref()),
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
    if server.post_connect_steps.iter().any(|step| {
        step.enabled
            && (!is_blank(step.input.as_deref()) || !is_blank(step.command_library_id.as_deref()))
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
