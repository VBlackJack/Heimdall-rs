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

//! Import of the SSH profiles of the C# Heimdall.
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

use std::collections::HashMap;
use std::path::PathBuf;

use serde::Deserialize;
use thiserror::Error;

use crate::profile::{DEFAULT_SSH_PORT, ProfileId, SshProfile};

/// `connectionType` of an SSH profile.
const SSH_CONNECTION_TYPE: &str = "SSH";

/// `connectionType` the C# Heimdall assumes when the field is absent.
const DEFAULT_CONNECTION_TYPE: &str = "RDP";

/// Separator of group paths.
const GROUP_SEPARATOR: char = '/';

/// Why a profile of the C# file was not imported.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkipReason {
    /// Not an SSH profile; carries its `connectionType`.
    NotSsh(String),
    /// Reaches its server through an SSH gateway, not supported yet.
    NeedsJumpHost,
    /// Has no host.
    MissingHost,
    /// Has no identifier, so a later import could not update it.
    MissingId,
    /// Its port is outside 1 to 65535; carries the value found.
    InvalidPort(i64),
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
    /// Profiles ready to be merged into the store.
    pub profiles: Vec<SshProfile>,
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
}

fn default_connection_type() -> String {
    DEFAULT_CONNECTION_TYPE.to_owned()
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LegacySettings {
    #[serde(default)]
    group_defaults: HashMap<String, LegacyGroupDefaults>,
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
    for mut server in servers.servers {
        resolve_group_defaults(server.group.as_deref(), &settings.group_defaults)
            .apply_to(&mut server);
        match convert(&server) {
            Ok(profile) => report.profiles.push(profile),
            Err(reason) => report.skipped.push(Skipped {
                id: server.id,
                name: server.display_name,
                reason,
            }),
        }
    }
    Ok(report)
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

fn convert(server: &LegacyServer) -> Result<SshProfile, SkipReason> {
    if server.connection_type != SSH_CONNECTION_TYPE {
        return Err(SkipReason::NotSsh(server.connection_type.clone()));
    }
    if server.id.is_empty() {
        return Err(SkipReason::MissingId);
    }
    if server.remote_server.trim().is_empty() {
        return Err(SkipReason::MissingHost);
    }
    if !is_null_or_empty(server.ssh_gateway_id.as_ref()) {
        return Err(SkipReason::NeedsJumpHost);
    }
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
    })
}
