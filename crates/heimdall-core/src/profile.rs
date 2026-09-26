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

//! Server profiles.

use std::fmt;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Port an SSH server listens on unless a profile says otherwise.
pub const DEFAULT_SSH_PORT: u16 = 22;

/// Port an RDP server listens on unless a profile says otherwise.
pub const DEFAULT_RDP_PORT: u16 = 3389;

/// Port a Telnet server listens on unless a profile says otherwise.
pub const DEFAULT_TELNET_PORT: u16 = 23;

/// Stable identifier of a profile.
///
/// A profile imported from the C# Heimdall keeps the identifier it had there, so a second
/// import updates it instead of adding a copy.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProfileId(String);

impl ProfileId {
    /// Wraps an identifier.
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// The identifier as text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ProfileId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A saved SSH destination.
///
/// Holds no secret. A password or a key passphrase is asked for when connecting.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SshProfile {
    /// Stable identifier.
    pub id: ProfileId,
    /// Name shown to the user.
    pub name: String,
    /// Folder path, `/`-separated, when the profile is filed in one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    /// Host name or address.
    pub host: String,
    /// TCP port.
    pub port: u16,
    /// Login name; asked for when connecting if absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    /// Private key file, OpenSSH or `PuTTY` format.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_path: Option<PathBuf>,
}

/// A saved RDP destination, reached directly.
///
/// Holds no secret: the password is asked for when connecting.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RdpProfile {
    /// Stable identifier.
    pub id: ProfileId,
    /// Name shown to the user.
    pub name: String,
    /// Folder path, `/`-separated, when the profile is filed in one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    /// Host name or address.
    pub host: String,
    /// TCP port.
    pub port: u16,
    /// Login name; asked for when connecting if absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    /// Windows domain of the account, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    /// Also accept a server without Network Level Authentication (xrdp): the password then
    /// travels in the logon packet, inside TLS, once the server's key is trusted.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub allow_tls_only: bool,
}

/// A saved Telnet destination, reached directly.
///
/// Holds no account: a Telnet server asks for one in the session itself, as text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TelnetProfile {
    /// Stable identifier.
    pub id: ProfileId,
    /// Name shown to the user.
    pub name: String,
    /// Folder path, `/`-separated, when the profile is filed in one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    /// Host name or address.
    pub host: String,
    /// TCP port.
    pub port: u16,
}

/// `host:port` as people write it: an IPv6 address in brackets, so the port stays
/// apart from it (`[fe80::1]:22`).
#[must_use]
pub fn display_address(host: &str, port: u16) -> String {
    if host.contains(':') {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    }
}

#[cfg(test)]
mod tests {
    use super::display_address;

    #[test]
    fn an_ipv6_address_is_bracketed_so_its_port_stays_apart() {
        assert_eq!(display_address("fe80::1", 22), "[fe80::1]:22");
        assert_eq!(display_address("srv.lab", 2222), "srv.lab:2222");
    }
}
