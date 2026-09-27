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

/// Port of `WinRM` over HTTP unless a profile says otherwise.
pub const DEFAULT_WINRM_HTTP_PORT: u16 = 5985;

/// Port of `WinRM` over HTTPS unless a profile says otherwise.
pub const DEFAULT_WINRM_HTTPS_PORT: u16 = 5986;

/// Port a VNC server listens on unless a profile says otherwise: display 0.
pub const DEFAULT_VNC_PORT: u16 = 5900;

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
    /// The SSH gateway the server is reached through, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gateway: Option<ProfileId>,
}

/// An SSH gateway: a server that other connections go through, itself possibly reached
/// through another one, its parent.
///
/// Holds no secret, and no host key: its key is checked against `known_hosts` like any
/// server's.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SshGateway {
    /// Stable identifier.
    pub id: ProfileId,
    /// Name shown to the user.
    pub name: String,
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
    /// The gateway this one is reached through, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<ProfileId>,
}

impl SshGateway {
    /// The gateway as the one hop it is: a server to log in to, reached directly.
    #[must_use]
    pub fn as_hop(&self) -> SshProfile {
        SshProfile {
            id: self.id.clone(),
            name: self.name.clone(),
            group: None,
            host: self.host.clone(),
            port: self.port,
            username: self.username.clone(),
            key_path: self.key_path.clone(),
            gateway: None,
        }
    }
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
    /// The SSH gateway the server is reached through, if any: the RDP connection then runs
    /// in a tunnel the gateway opens to the server.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gateway: Option<ProfileId>,
    /// Share the clipboard with the server, text only. On unless turned off, as in the C#
    /// Heimdall and in the Windows client: written down only when off.
    #[serde(default = "shared", skip_serializing_if = "is_shared")]
    pub redirect_clipboard: bool,
    /// Share this computer's drives with the server, as mstsc does. Off unless turned on,
    /// as in the C# Heimdall: written down only when on.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub redirect_drives: bool,
}

/// The clipboard is shared unless a profile says otherwise.
fn shared() -> bool {
    true
}

#[expect(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde's skip_serializing_if passes a reference"
)]
fn is_shared(value: &bool) -> bool {
    *value
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

/// A saved `WinRM` destination: a remote `PowerShell` session on a Windows server.
///
/// Holds no password: with a user name, `PowerShell` asks for it when connecting; without,
/// the session runs as the account running Heimdall.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WinRmProfile {
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
    /// `WinRM` over HTTPS.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub use_ssl: bool,
    /// Over HTTPS, accept whatever certificate the server shows: for test servers only.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub skip_certificate_check: bool,
    /// Account to log in as, `DOMAIN\user` or `user@domain`; `None` for the account running
    /// Heimdall.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
}

/// A saved VNC destination, reached directly.
///
/// Holds no password: it is asked for when connecting.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VncProfile {
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
    /// Watch only: no keyboard or pointer goes to the server.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub view_only: bool,
    /// Whether a server asking for no password is accepted. Off by default: for a profile
    /// that expects a password, an impostor offering none would otherwise be let in.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub allow_no_password: bool,
}

/// The arguments of a local program.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocalArguments {
    /// One per entry, each quoted as needed when the command line is built.
    List(Vec<String>),
    /// A Windows argument string, put after the program exactly as written: how the C#
    /// Heimdall passed it, and what a profile imported from it keeps. Splitting it would not
    /// give back the same line for every program, `cmd` first.
    WindowsLine(String),
}

impl Default for LocalArguments {
    fn default() -> Self {
        Self::List(Vec::new())
    }
}

impl LocalArguments {
    /// Whether there are none.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        match self {
            Self::List(list) => list.is_empty(),
            Self::WindowsLine(line) => line.is_empty(),
        }
    }
}

/// What a local profile runs.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct LocalCommand {
    /// The program, a full path or a name looked up in `PATH`; `None` for the default shell.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub program: Option<String>,
    /// Its arguments.
    #[serde(default, skip_serializing_if = "LocalArguments::is_empty")]
    pub arguments: LocalArguments,
    /// Folder it starts in; `None` for the home folder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_directory: Option<PathBuf>,
}

impl LocalCommand {
    /// Whether it is the default shell, as the Local shell button opens it: nothing to run
    /// that the user did not choose.
    #[must_use]
    pub fn is_default(&self) -> bool {
        self.program.is_none() && self.arguments.is_empty() && self.working_directory.is_none()
    }
}

/// What the user agreed to run: the command as shown, and the file its program was found
/// at then. It holds for as long as both are still what would run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalApproval {
    /// The command shown.
    pub command: LocalCommand,
    /// The full path its program was found at.
    pub program_path: PathBuf,
}

/// A saved local program: a shell, or any command-line program, run in a terminal tab.
///
/// A profile that runs anything other than the default shell runs only once the user has
/// approved exactly what it runs; an imported profile never carries an approval.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalProfile {
    /// Stable identifier.
    pub id: ProfileId,
    /// Name shown to the user.
    pub name: String,
    /// Folder path, `/`-separated, when the profile is filed in one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    /// What it runs.
    pub command: LocalCommand,
    /// What the user approved, if anything.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approved: Option<LocalApproval>,
}

impl LocalProfile {
    /// Whether `program_path`, where the program is found now, may run without asking:
    /// the default shell always may; anything else only as approved, program path included.
    #[must_use]
    pub fn may_run(&self, program_path: &std::path::Path) -> bool {
        self.command.is_default()
            || self.approved.as_ref().is_some_and(|approval| {
                approval.command == self.command && approval.program_path == program_path
            })
    }
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
    use std::path::{Path, PathBuf};

    use super::{
        LocalApproval, LocalArguments, LocalCommand, LocalProfile, ProfileId, display_address,
    };

    #[test]
    fn an_ipv6_address_is_bracketed_so_its_port_stays_apart() {
        assert_eq!(display_address("fe80::1", 22), "[fe80::1]:22");
        assert_eq!(display_address("srv.lab", 2222), "srv.lab:2222");
    }

    fn tool() -> LocalCommand {
        LocalCommand {
            program: Some("tool".to_owned()),
            arguments: LocalArguments::List(vec!["-x".to_owned()]),
            working_directory: None,
        }
    }

    fn profile(command: LocalCommand, approved: Option<LocalApproval>) -> LocalProfile {
        LocalProfile {
            id: ProfileId::new("p"),
            name: "P".to_owned(),
            group: None,
            command,
            approved,
        }
    }

    const FOUND: &str = "/usr/bin/tool";

    fn approval(command: LocalCommand) -> LocalApproval {
        LocalApproval {
            command,
            program_path: PathBuf::from(FOUND),
        }
    }

    #[test]
    fn the_default_shell_runs_without_approval() {
        assert!(profile(LocalCommand::default(), None).may_run(Path::new("/bin/sh")));
    }

    #[test]
    fn anything_else_runs_only_once_approved() {
        assert!(!profile(tool(), None).may_run(Path::new(FOUND)));
        assert!(profile(tool(), Some(approval(tool()))).may_run(Path::new(FOUND)));
    }

    #[test]
    fn an_approval_lapses_when_the_command_changes() {
        let mut changed = tool();
        changed.arguments = LocalArguments::List(vec!["-y".to_owned()]);
        assert!(!profile(changed.clone(), Some(approval(tool()))).may_run(Path::new(FOUND)));
        changed = tool();
        changed.working_directory = Some(PathBuf::from("/tmp"));
        assert!(!profile(changed, Some(approval(tool()))).may_run(Path::new(FOUND)));
    }

    #[test]
    fn an_approval_lapses_when_the_program_is_found_elsewhere() {
        assert!(!profile(tool(), Some(approval(tool()))).may_run(Path::new("/opt/evil/tool")));
    }

    #[test]
    fn arguments_alone_are_not_the_default_shell() {
        let arguments_only = LocalCommand {
            arguments: LocalArguments::WindowsLine("-NoExit".to_owned()),
            ..LocalCommand::default()
        };
        assert!(!arguments_only.is_default());
        assert!(!profile(arguments_only, None).may_run(Path::new("/bin/sh")));
    }
}
