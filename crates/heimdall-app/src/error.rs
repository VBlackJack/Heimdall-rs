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

//! Connection failures as data the UI can clone, compare and translate.
//!
//! The UI chooses a localised sentence per variant; `detail` fields carry technical text
//! (an OS error, a path) shown as-is under it.

use heimdall_core::profile::display_address;
use heimdall_core::store::RouteError;
use heimdall_core::winrm::CommandError;
use heimdall_ssh::{AuthMethod, ConnectError, KeyFileError, KnownHostsError};

/// A server by host and port, as the `known_hosts` file records it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerAddress {
    /// Host name or address.
    pub host: String,
    /// Port.
    pub port: u16,
}

impl std::fmt::Display for ServerAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&display_address(&self.host, self.port))
    }
}

/// Why a key file could not be used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyProblem {
    /// Not readable.
    Unreadable,
    /// Not a private key in a known format.
    UnknownFormat,
    /// Encrypted and no passphrase given.
    NeedsPassphrase,
    /// The passphrase did not decrypt it.
    WrongPassphrase,
    /// Unencrypted and still did not load.
    Invalid,
}

/// Why a connection failed or ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiError {
    /// The host name cannot be used.
    InvalidHost,
    /// The account name cannot be used.
    InvalidUsername,
    /// `WinRM` over HTTPS through an SSH gateway, which the C# Heimdall refuses.
    WinRmHttpsThroughGateway,
    /// The `WinRM` server's name does not resolve, as the C# preflight says.
    WinRmHostUnresolved {
        /// The name.
        host: String,
    },
    /// The `WinRM` server's port refused or did not answer, as the C# preflight says.
    WinRmUnreachable {
        /// The server.
        host: String,
        /// Its port.
        port: u16,
    },
    /// The `WinRM` server's TLS handshake failed: a certificate not trusted, or no TLS.
    WinRmTlsFailed {
        /// The server.
        host: String,
        /// Its port.
        port: u16,
    },
    /// The network connection failed.
    Network {
        /// How, as far as the operating system tells.
        failure: NetworkFailure,
        /// Operating system message.
        detail: String,
    },
    /// Connection plus key exchange took too long.
    Timeout,
    /// The RDP server refused the logon, and why.
    RdpRefused {
        /// Why.
        refusal: heimdall_rdp::Refusal,
    },
    /// The RDP server ended the connection before its session started, and why.
    RdpEnded {
        /// Why.
        ending: heimdall_rdp::Ending,
    },
    /// An RDP session or connection failed in the protocol.
    RdpProtocol {
        /// What failed, in the library's words.
        detail: String,
    },
    /// A VNC session or connection failed in the protocol.
    VncProtocol {
        /// What failed, in the library's words.
        detail: String,
    },
    /// A local shell could not be started.
    LocalShell {
        /// Operating system message.
        detail: String,
    },
    /// The server refused the security the client requires (an RDP server without Network
    /// Level Authentication).
    SecurityRefused {
        /// What the server selected, in the library's words.
        detail: String,
    },
    /// The server's key is not the recorded one: possible interception.
    HostKeyChanged {
        /// The SSH server whose key changed: the tab's own, or a gateway on the way.
        /// `None` for an RDP server's certificate, the tab's own server.
        target: Option<ServerAddress>,
        /// SHA-256 fingerprint on record.
        recorded: String,
        /// SHA-256 fingerprint presented.
        offered: String,
    },
    /// The server no longer offers the recorded key type.
    HostKeyAlgorithmMismatch {
        /// Recorded algorithms.
        recorded: Vec<String>,
    },
    /// The server presented a certificate; not supported.
    HostCertificateRefused,
    /// The `known_hosts` file could not be used.
    KnownHosts {
        /// What went wrong, technically.
        detail: String,
    },
    /// The key file could not be used.
    KeyFile {
        /// What went wrong.
        problem: KeyProblem,
        /// The file.
        path: String,
    },
    /// Every allowed method was refused.
    AuthenticationFailed {
        /// Methods tried.
        tried: Vec<AuthMethod>,
    },
    /// The server closed the connection.
    Disconnected {
        /// The server's own words, untrusted.
        server_message: Option<String>,
    },
    /// The connection went away under a live session that the server never closed: the
    /// network dropped, or keepalives went unanswered.
    ConnectionLost,
    /// Cancelled by the user.
    Cancelled,
    /// The user did not approve the certificate an RDP server presented.
    CertificateRefused,
    /// A question was not answered in time.
    PromptTimedOut,
    /// No terminal allocated.
    PtyRefused,
    /// No shell started.
    ShellRefused,
    /// The SSH gateways of the profile cannot be followed.
    Route(RouteError),
    /// A gateway on the way would not connect onward to `host`.
    JumpRefused {
        /// Host the gateway was asked to reach.
        host: String,
        /// Port.
        port: u16,
    },
    /// The local port of the profile's SOCKS proxy could not be taken.
    ProxyPort {
        /// The port.
        port: u16,
        /// Operating system message.
        detail: String,
    },
    /// The gateway would not listen on its `port` for the remote forward.
    RemoteForwardRefused {
        /// The port.
        port: u16,
    },
    /// The server refused a subsystem, such as SFTP.
    SubsystemRefused {
        /// Subsystem asked for.
        name: String,
    },
    /// Anything else in the SSH protocol.
    Protocol {
        /// Technical description.
        detail: String,
    },
}

impl From<&CommandError> for UiError {
    fn from(error: &CommandError) -> Self {
        match error {
            CommandError::InvalidHost => Self::InvalidHost,
            CommandError::InvalidUsername => Self::InvalidUsername,
            CommandError::HttpsThroughGateway => Self::WinRmHttpsThroughGateway,
        }
    }
}

impl From<&KeyFileError> for UiError {
    fn from(error: &KeyFileError) -> Self {
        let (problem, path) = match error {
            KeyFileError::Io { path, .. } => (KeyProblem::Unreadable, path),
            KeyFileError::UnknownFormat { path } => (KeyProblem::UnknownFormat, path),
            KeyFileError::NeedsPassphrase { path } => (KeyProblem::NeedsPassphrase, path),
            KeyFileError::WrongPassphrase { path } => (KeyProblem::WrongPassphrase, path),
            KeyFileError::Invalid { path, .. } => (KeyProblem::Invalid, path),
        };
        Self::KeyFile {
            problem,
            path: path.display().to_string(),
        }
    }
}

impl From<&KnownHostsError> for UiError {
    fn from(error: &KnownHostsError) -> Self {
        match error {
            KnownHostsError::InvalidHost => Self::InvalidHost,
            other => Self::KnownHosts {
                detail: other.to_string(),
            },
        }
    }
}

/// How a network connection failed, as the C# Heimdall tells them apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkFailure {
    /// Nothing accepted the connection on that port.
    Refused,
    /// The connection was cut.
    Reset,
    /// Nothing answered in time.
    TimedOut,
    /// No route to the host, or its name could not be resolved.
    Unreachable,
    /// Anything else: its message says.
    Other,
}

/// Windows' "no such host is known" and "no data record of the requested type".
const WINDOWS_NAME_NOT_FOUND: [i32; 2] = [11001, 11004];

/// How the standard library begins the error of a name that could not be resolved.
const LOOKUP_FAILED: &str = "failed to lookup address information";

impl NetworkFailure {
    /// How `error` failed.
    #[must_use]
    pub fn of(error: &std::io::Error) -> Self {
        use std::io::ErrorKind;
        match error.kind() {
            ErrorKind::ConnectionRefused => Self::Refused,
            ErrorKind::ConnectionReset | ErrorKind::ConnectionAborted => Self::Reset,
            ErrorKind::TimedOut => Self::TimedOut,
            ErrorKind::HostUnreachable | ErrorKind::NetworkUnreachable | ErrorKind::NetworkDown => {
                Self::Unreachable
            }
            _ if error
                .raw_os_error()
                .is_some_and(|code| cfg!(windows) && WINDOWS_NAME_NOT_FOUND.contains(&code))
                || error.to_string().starts_with(LOOKUP_FAILED) =>
            {
                Self::Unreachable
            }
            _ => Self::Other,
        }
    }
}

impl UiError {
    /// The error of a network connection that failed with `error`.
    #[must_use]
    pub fn network(error: &std::io::Error) -> Self {
        Self::Network {
            failure: NetworkFailure::of(error),
            detail: error.to_string(),
        }
    }
}

impl From<ConnectError> for UiError {
    fn from(error: ConnectError) -> Self {
        match error {
            ConnectError::InvalidHost => Self::InvalidHost,
            ConnectError::Network(source) => Self::network(&source),
            ConnectError::Timeout => Self::Timeout,
            // Handled before conversion, as a question to the user; kept total here.
            ConnectError::UnknownHostKey { .. } => Self::Protocol {
                detail: error_text(&error),
            },
            ConnectError::HostKeyChanged {
                host,
                port,
                recorded,
                offered,
            } => Self::HostKeyChanged {
                target: Some(ServerAddress { host, port }),
                recorded,
                offered,
            },
            ConnectError::HostKeyAlgorithmMismatch { recorded, .. } => {
                Self::HostKeyAlgorithmMismatch { recorded }
            }
            ConnectError::HostCertificateRefused => Self::HostCertificateRefused,
            ConnectError::KnownHosts(ref source) => source.into(),
            ConnectError::KeyFile(ref source) => source.into(),
            ConnectError::AuthenticationFailed { tried } => Self::AuthenticationFailed { tried },
            ConnectError::Disconnected { server_message } => Self::Disconnected { server_message },
            ConnectError::Cancelled => Self::Cancelled,
            ConnectError::PromptTimedOut => Self::PromptTimedOut,
            ConnectError::PtyRefused => Self::PtyRefused,
            ConnectError::ShellRefused => Self::ShellRefused,
            ConnectError::SubsystemRefused { name } => Self::SubsystemRefused { name },
            ConnectError::JumpRefused { host, port } => Self::JumpRefused { host, port },
            ConnectError::RemoteForwardRefused { port } => Self::RemoteForwardRefused { port },
            ConnectError::ProxyPort { port, source } => Self::ProxyPort {
                port,
                detail: source.to_string(),
            },
            ConnectError::Protocol(source) => Self::Protocol {
                detail: source.to_string(),
            },
        }
    }
}

fn error_text(error: &ConnectError) -> String {
    error.to_string()
}

#[cfg(test)]
mod tests {
    use std::io::{self, ErrorKind};
    use std::net::{Ipv4Addr, TcpStream};

    use super::{NetworkFailure, UiError};

    /// A port under 1024, which no test process can take: a port set free and asked again at
    /// once was taken by a test running beside this one, and the connection went through.
    const CLOSED_PORT: u16 = 1;

    #[test]
    fn a_failure_is_told_by_its_kind() {
        for (kind, failure) in [
            (ErrorKind::ConnectionRefused, NetworkFailure::Refused),
            (ErrorKind::ConnectionReset, NetworkFailure::Reset),
            (ErrorKind::ConnectionAborted, NetworkFailure::Reset),
            (ErrorKind::TimedOut, NetworkFailure::TimedOut),
            (ErrorKind::HostUnreachable, NetworkFailure::Unreachable),
            (ErrorKind::NetworkUnreachable, NetworkFailure::Unreachable),
            (ErrorKind::NetworkDown, NetworkFailure::Unreachable),
            (ErrorKind::PermissionDenied, NetworkFailure::Other),
        ] {
            assert_eq!(
                NetworkFailure::of(&io::Error::from(kind)),
                failure,
                "{kind:?}"
            );
        }
        assert_eq!(
            NetworkFailure::of(&io::Error::other(
                "failed to lookup address information: Name or service not known"
            )),
            NetworkFailure::Unreachable,
            "a name that did not resolve"
        );
        assert_eq!(
            NetworkFailure::of(&io::Error::other("something else")),
            NetworkFailure::Other
        );
        let windows_no_host = NetworkFailure::of(&io::Error::from_raw_os_error(11001));
        if cfg!(windows) {
            assert_eq!(windows_no_host, NetworkFailure::Unreachable);
        }
    }

    #[test]
    fn a_closed_port_is_refused_and_an_unknown_name_unreachable() {
        let refused = TcpStream::connect((Ipv4Addr::LOCALHOST, CLOSED_PORT)).expect_err("closed");
        assert!(matches!(
            UiError::network(&refused),
            UiError::Network {
                failure: NetworkFailure::Refused,
                ..
            }
        ));
        // `.invalid` never resolves (RFC 2606); without DNS at all the lookup fails too.
        let unknown = TcpStream::connect(("heimdall-test.invalid", 22)).expect_err("no such name");
        assert_eq!(
            NetworkFailure::of(&unknown),
            NetworkFailure::Unreachable,
            "{unknown}"
        );
    }
}
