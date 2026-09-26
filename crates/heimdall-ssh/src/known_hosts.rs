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

//! The `known_hosts` file owned by Heimdall-rs, and the decision it drives.
//!
//! Every failure refuses the connection. russh's own reader treats a file it cannot open as
//! an empty one, which would turn a known host into a first contact; this module checks
//! readability itself before handing the file to russh.

use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};

use russh::keys::known_hosts::{known_host_keys_path, learn_known_hosts_path};
use russh::keys::{Algorithm, HashAlg, PublicKey};
use thiserror::Error;

/// Characters refused in a host name: they carry meaning in a `known_hosts` line, and could
/// otherwise add or alter entries.
const REFUSED_HOST_CHARACTERS: [char; 9] = [',', '#', '[', ']', '|', '*', '?', '!', '@'];

/// Why the `known_hosts` file could not be used.
#[derive(Debug, Error)]
pub enum KnownHostsError {
    /// The host name contains characters that are not safe in a `known_hosts` line.
    #[error("host name not usable in known_hosts")]
    InvalidHost,
    /// The file exists and cannot be read.
    #[error("{path}: {source}")]
    Unreadable {
        /// File concerned.
        path: PathBuf,
        /// Underlying error.
        #[source]
        source: io::Error,
    },
    /// A line for this host could not be parsed.
    #[error("{path}: an entry for this host is corrupt")]
    Corrupt {
        /// File concerned.
        path: PathBuf,
    },
    /// The key could not be recorded.
    #[error("{path}: the key could not be recorded")]
    WriteFailed {
        /// File concerned.
        path: PathBuf,
    },
}

/// What the recorded keys say about the key a server presents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// The presented key is recorded for this host.
    Trusted,
    /// Nothing is recorded for this host and port.
    Unknown,
    /// A different key of the same algorithm is recorded.
    Changed {
        /// The recorded key.
        recorded: Box<PublicKey>,
    },
    /// Keys are recorded, none of the presented algorithm.
    OtherAlgorithm {
        /// Algorithms recorded for the host.
        recorded: Vec<Algorithm>,
    },
}

/// Compares a presented key with the keys recorded for its host.
#[must_use]
pub fn verdict(recorded: &[PublicKey], offered: &PublicKey) -> Verdict {
    if recorded.is_empty() {
        return Verdict::Unknown;
    }
    if recorded
        .iter()
        .any(|key| key.key_data() == offered.key_data())
    {
        return Verdict::Trusted;
    }
    if let Some(same_algorithm) = recorded
        .iter()
        .find(|key| key.algorithm() == offered.algorithm())
    {
        return Verdict::Changed {
            recorded: Box::new(same_algorithm.clone()),
        };
    }
    Verdict::OtherAlgorithm {
        recorded: recorded.iter().map(PublicKey::algorithm).collect(),
    }
}

/// SHA-256 fingerprint in the OpenSSH form, `SHA256:...`.
#[must_use]
pub fn fingerprint(key: &PublicKey) -> String {
    key.fingerprint(HashAlg::Sha256).to_string()
}

/// Normalises a host name for `known_hosts`: trimmed, ASCII-lowercased, and refused when it
/// contains whitespace, control characters or characters meaningful in the file.
///
/// # Errors
///
/// Returns [`KnownHostsError::InvalidHost`] for an empty or unsafe name.
pub fn validate_host(host: &str) -> Result<String, KnownHostsError> {
    let host = host.trim();
    let unsafe_character =
        |c: char| c.is_whitespace() || c.is_control() || REFUSED_HOST_CHARACTERS.contains(&c);
    if host.is_empty() || host.chars().any(unsafe_character) {
        return Err(KnownHostsError::InvalidHost);
    }
    Ok(host.to_ascii_lowercase())
}

/// The `known_hosts` file of Heimdall-rs.
#[derive(Debug, Clone)]
pub struct KnownHosts {
    path: PathBuf,
}

impl KnownHosts {
    /// The file at `path`; it need not exist yet.
    #[must_use]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// File read and written.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Keys recorded for `host` on `port`.
    ///
    /// # Errors
    ///
    /// Refuses, rather than answering "nothing recorded", when the host name is unsafe, the
    /// file exists but cannot be read, or an entry for the host is corrupt.
    pub fn recorded(&self, host: &str, port: u16) -> Result<Vec<PublicKey>, KnownHostsError> {
        let host = validate_host(host)?;
        match File::open(&self.path) {
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(source) => {
                return Err(KnownHostsError::Unreadable {
                    path: self.path.clone(),
                    source,
                });
            }
        }
        known_host_keys_path(&host, port, &self.path)
            .map(|entries| entries.into_iter().map(|(_, key)| key).collect())
            .map_err(|_| KnownHostsError::Corrupt {
                path: self.path.clone(),
            })
    }

    /// Records `key` for `host` on `port`, creating the file and its directory if needed.
    ///
    /// # Errors
    ///
    /// Returns [`KnownHostsError`] for an unsafe host name or a failed write.
    pub fn learn(&self, host: &str, port: u16, key: &PublicKey) -> Result<(), KnownHostsError> {
        let host = validate_host(host)?;
        if let Some(dir) = self.path.parent().filter(|dir| !dir.as_os_str().is_empty()) {
            fs::create_dir_all(dir).map_err(|_| KnownHostsError::WriteFailed {
                path: self.path.clone(),
            })?;
        }
        learn_known_hosts_path(&host, port, key, &self.path).map_err(|_| {
            KnownHostsError::WriteFailed {
                path: self.path.clone(),
            }
        })
    }
}
