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
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use russh::keys::PublicKey;

use crate::known_hosts::{DEFAULT_SSH_PORT, KnownHosts, KnownHostsError, plain_host};
use crate::pins::Pins;

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
    /// export: a line of `target` naming only servers trusted here is written again in its
    /// place, with the keys trusted for them; every other line stays as it is (comments,
    /// marked, hashed or wildcard lines, other servers); the servers left are added at the
    /// end. The file is replaced whole, through a file beside it. Nothing is written when
    /// no key is trusted.
    ///
    /// # Errors
    ///
    /// Either file cannot be read, or `target` cannot be written.
    pub fn export_to(&self, target: &Path) -> Result<KnownHostsExport, KnownHostsError> {
        let trusted = self.trusted()?;
        let mut report = KnownHostsExport {
            skipped: Pins::beside(self.path())
                .all()?
                .into_iter()
                .filter(|(host, port, _)| !trusted.iter().any(|key| key.names(host, *port)))
                .count(),
            ..KnownHostsExport::default()
        };
        if trusted.is_empty() {
            return Ok(report);
        }
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

/// The servers `line` names, when it names only servers trusted here and its key reads:
/// such a line is written again from the keys trusted. `None` for every other line.
fn managed_servers(line: &str, trusted: &[Trusted]) -> Option<Vec<(String, u16)>> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with('@') {
        return None;
    }
    let (patterns, rest) = trimmed.split_once(char::is_whitespace)?;
    key_text(rest)?;
    patterns
        .split(',')
        .map(|pattern| {
            plain_host(pattern)
                .filter(|(host, port)| trusted.iter().any(|key| key.names(host, *port)))
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

/// The text of `path`; empty when there is no such file.
fn read(path: &Path) -> Result<String, KnownHostsError> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(text),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(String::new()),
        Err(source) => Err(KnownHostsError::Unreadable {
            path: path.to_owned(),
            source,
        }),
    }
}

/// Replaces `path` with `text` whole: written beside it first, then moved over it.
fn replace(path: &Path, text: &str) -> Result<(), KnownHostsError> {
    let failed = || KnownHostsError::WriteFailed {
        path: path.to_owned(),
    };
    let dir = path
        .parent()
        .filter(|dir| !dir.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(dir).map_err(|_| failed())?;
    let beside = beside(dir, path);
    let written = fs::File::create_new(&beside).and_then(|mut file| {
        file.write_all(text.as_bytes())?;
        file.sync_all()
    });
    if written.and_then(|()| fs::rename(&beside, path)).is_err() {
        let _ = fs::remove_file(&beside);
        return Err(failed());
    }
    Ok(())
}

/// A name for the file written beside `path` before it replaces it, never used before.
fn beside(dir: &Path, path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .map_or_else(|| "known_hosts".into(), |name| name.to_string_lossy());
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos());
    dir.join(format!(".{name}.{}.{stamp}.tmp", std::process::id()))
}
