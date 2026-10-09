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

//! The keys Heimdall-rs trusts, written into another `known_hosts` file, as the C#
//! "Export `known_hosts`" writes them into the user's OpenSSH one: what the file holds of
//! other hosts stays as it is.

use std::fs;
use std::io;
use std::path::Path;

use russh::keys::PublicKey;

use crate::known_hosts::{DEFAULT_SSH_PORT, KnownHosts, KnownHostsError, plain_host};
use crate::pins::Pins;
use crate::trust_files::{self, followed};

/// What a file may start with, and the C# export leaves out.
const BYTE_ORDER_MARK: char = '\u{feff}';

/// What an export wrote, as the C# `KnownHostsExportReport` counts it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct KnownHostsExport {
    /// Lines written for the keys trusted.
    pub written: usize,
    /// Lines of the file kept as they were.
    pub preserved: usize,
    /// Servers trusted by a fingerprint alone: no key to write.
    pub skipped: usize,
}

/// One key trusted for a server.
struct Trusted {
    host: String,
    port: u16,
    /// The key as the line writes it: its algorithm and its base64, no comment.
    key: String,
}

impl Trusted {
    fn names(&self, host: &str, port: u16) -> bool {
        self.port == port && self.host.eq_ignore_ascii_case(host)
    }

    /// The key's algorithm, `ssh-ed25519` for one.
    fn algorithm(&self) -> &str {
        self.key
            .split_once(' ')
            .map_or(self.key.as_str(), |(algorithm, _)| algorithm)
    }

    fn line(&self) -> String {
        if self.port == DEFAULT_SSH_PORT {
            format!("{} {}", self.host, self.key)
        } else {
            format!("[{}]:{} {}", self.host, self.port, self.key)
        }
    }
}

impl KnownHosts {
    /// Writes the keys this file trusts into the `known_hosts` file `target`, as the C#
    /// export: a line of `target` naming only servers trusted here, with a key of a kind
    /// trusted for each, is written again in its place with the keys trusted for them;
    /// every other line stays as it is (comments, marked, hashed or wildcard lines, other
    /// servers, keys of another kind); the servers left are added at the end. The file is
    /// replaced whole, through a file beside it that takes its permissions; a link is
    /// followed to the file it names. Nothing is written when no key is trusted.
    ///
    /// # Errors
    ///
    /// Either file cannot be read, or `target` cannot be written: it is read-only, or the
    /// system refused.
    pub fn export_to(&self, target: &Path) -> Result<KnownHostsExport, KnownHostsError> {
        // The target may be this very file: read and written as every other writer does.
        let _lock = trust_files::lock();
        let trusted = self.trusted()?;
        let mut pinned: Vec<(String, u16)> = Pins::beside(self.path())
            .all()?
            .into_iter()
            .filter(|(host, port, _)| !trusted.iter().any(|key| key.names(host, *port)))
            .map(|(host, port, _)| (host, port))
            .collect();
        pinned.sort();
        pinned.dedup();
        let mut report = KnownHostsExport {
            skipped: pinned.len(),
            ..KnownHostsExport::default()
        };
        if trusted.is_empty() {
            return Ok(report);
        }
        let target = &followed(target);
        let existing = read(target)?;
        let mut written = vec![false; trusted.len()];
        let mut lines = Vec::new();
        for line in existing.lines() {
            if let Some(servers) = managed_servers(line, &trusted) {
                for (host, port) in servers {
                    write_keys(&trusted, &mut written, &host, port, &mut lines);
                }
            } else {
                report.preserved += usize::from(!line.trim().is_empty());
                lines.push(line.to_owned());
            }
        }
        for (index, key) in trusted.iter().enumerate() {
            if !written[index] {
                write_keys(&trusted, &mut written, &key.host, key.port, &mut lines);
            }
        }
        report.written = written.iter().filter(|done| **done).count();
        let mut text = lines.join("\n");
        text.push('\n');
        replace(target, &text)?;
        Ok(report)
    }

    /// The keys trusted, one per server and key, in the order of the file.
    fn trusted(&self) -> Result<Vec<Trusted>, KnownHostsError> {
        let text = read(self.path())?;
        let mut trusted: Vec<Trusted> = Vec::new();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') || line.starts_with('@') {
                continue;
            }
            let Some((patterns, rest)) = line.split_once(char::is_whitespace) else {
                continue;
            };
            let Some(key) = key_text(rest) else {
                continue;
            };
            for (host, port) in patterns.split(',').filter_map(plain_host) {
                if !trusted
                    .iter()
                    .any(|known| known.names(&host, port) && known.key == key)
                {
                    trusted.push(Trusted {
                        host,
                        port,
                        key: key.clone(),
                    });
                }
            }
        }
        Ok(trusted)
    }
}

/// The key of a line, after its hosts: its algorithm and base64 when it reads as a key.
fn key_text(rest: &str) -> Option<String> {
    let mut parts = rest.split_whitespace();
    let key = format!("{} {}", parts.next()?, parts.next()?);
    PublicKey::from_openssh(&key).ok().map(|_| key)
}

/// The servers `line` names, when it names only servers trusted here, its key reads and
/// a key of its kind is trusted for each: such a line is written again from the keys
/// trusted. `None` for every other line, kept as it is: one holding a carriage return of
/// its own is never read as one line, lest a file ending its lines so be taken whole.
fn managed_servers(line: &str, trusted: &[Trusted]) -> Option<Vec<(String, u16)>> {
    if line.contains('\r') {
        return None;
    }
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with('@') {
        return None;
    }
    let (patterns, rest) = trimmed.split_once(char::is_whitespace)?;
    let key = key_text(rest)?;
    let algorithm = key.split_once(' ').map(|(algorithm, _)| algorithm)?;
    patterns
        .split(',')
        .map(|pattern| {
            plain_host(pattern).filter(|(host, port)| {
                trusted
                    .iter()
                    .any(|key| key.names(host, *port) && key.algorithm() == algorithm)
            })
        })
        .collect()
}

/// Writes the keys trusted for `host` on `port` not written yet.
fn write_keys(
    trusted: &[Trusted],
    written: &mut [bool],
    host: &str,
    port: u16,
    lines: &mut Vec<String>,
) {
    for (index, key) in trusted.iter().enumerate() {
        if !written[index] && key.names(host, port) {
            written[index] = true;
            lines.push(key.line());
        }
    }
}

/// The text of `path`, without a byte order mark; empty when there is no such file.
fn read(path: &Path) -> Result<String, KnownHostsError> {
    match fs::read_to_string(path) {
        Ok(mut text) => {
            if text.starts_with(BYTE_ORDER_MARK) {
                text.drain(..BYTE_ORDER_MARK.len_utf8());
            }
            Ok(text)
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(String::new()),
        Err(source) => Err(KnownHostsError::Unreadable {
            path: path.to_owned(),
            source,
        }),
    }
}

/// Replaces `path` with `text` whole, as the trust files are: written beside it first, with
/// its permissions, then moved over it. A read-only file is left as it is.
fn replace(path: &Path, text: &str) -> Result<(), KnownHostsError> {
    trust_files::replace(path, text).map_err(|source| KnownHostsError::ExportFailed {
        path: path.to_owned(),
        source,
    })
}
