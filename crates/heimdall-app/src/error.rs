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

use heimdall_ssh::{AuthMethod, ConnectError, KeyFileError, KnownHostsError};

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
    /// The network connection failed.
    Network {
        /// Operating system message.
        detail: String,
    },
    /// Connection plus key exchange took too long.
    Timeout,
    /// The server's key is not the recorded one: possible interception.
    HostKeyChanged {
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
    /// Cancelled by the user.
    Cancelled,
    /// A question was not answered in time.
    PromptTimedOut,
    /// No terminal allocated.
    PtyRefused,
    /// No shell started.
    ShellRefused,
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

impl From<ConnectError> for UiError {
    fn from(error: ConnectError) -> Self {
        match error {
            ConnectError::InvalidHost => Self::InvalidHost,
            ConnectError::Network(source) => Self::Network {
                detail: source.to_string(),
            },
            ConnectError::Timeout => Self::Timeout,
            // Handled before conversion, as a question to the user; kept total here.
            ConnectError::UnknownHostKey { .. } => Self::Protocol {
                detail: error_text(&error),
            },
            ConnectError::HostKeyChanged {
                recorded, offered, ..
            } => Self::HostKeyChanged { recorded, offered },
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
            ConnectError::Protocol(source) => Self::Protocol {
                detail: source.to_string(),
            },
        }
    }
}

fn error_text(error: &ConnectError) -> String {
    error.to_string()
}
