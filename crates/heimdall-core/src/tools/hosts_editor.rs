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

//! Hosts file editor and parser.
//!
//! Provides parsing, validation, serialization, and enable/disable toggling
//! of system hosts file entries.

use std::fs;
use std::net::IpAddr;
use std::path::{Path, PathBuf};

/// A single entry in the system hosts file.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct HostEntry {
    /// Whether the mapping is active (true) or commented out (false).
    pub enabled: bool,
    /// Destination IP address (IPv4 or IPv6).
    pub ip: String,
    /// Hostname mapped to the IP.
    pub host: String,
    /// Optional inline comment.
    pub comment: String,
}

impl HostEntry {
    /// Formats the host entry as a hosts file line.
    #[must_use]
    pub fn to_line(&self) -> String {
        let prefix = if self.enabled { "" } else { "# " };
        if self.comment.is_empty() {
            format!("{prefix}{:<16} {}", self.ip, self.host)
        } else {
            format!("{prefix}{:<16} {} # {}", self.ip, self.host, self.comment)
        }
    }
}

/// Parser and serializer for system hosts files.
pub struct HostsParser;

impl HostsParser {
    /// Validates whether `ip` is a valid IPv4 or IPv6 address.
    #[must_use]
    pub fn validate_ip(ip: &str) -> bool {
        ip.trim().parse::<IpAddr>().is_ok()
    }

    /// Parses the hosts file content into preamble comments and a list of [`HostEntry`].
    #[must_use]
    pub fn parse(content: &str) -> (Vec<String>, Vec<HostEntry>) {
        let mut preamble = Vec::new();
        let mut entries = Vec::new();
        let mut in_preamble = true;

        for raw_line in content.lines() {
            let line = raw_line.trim();

            if line.is_empty() {
                if in_preamble {
                    preamble.push(raw_line.to_owned());
                }
                continue;
            }

            if let Some(stripped) = line.strip_prefix('#') {
                let after_hash = stripped.trim();
                if let Some(mut disabled_entry) = Self::try_parse_host_line(after_hash) {
                    disabled_entry.enabled = false;
                    entries.push(disabled_entry);
                    in_preamble = false;
                } else if in_preamble {
                    preamble.push(raw_line.to_owned());
                }
                continue;
            }

            in_preamble = false;

            if let Some(mut active_entry) = Self::try_parse_host_line(line) {
                active_entry.enabled = true;
                entries.push(active_entry);
            }
        }

        (preamble, entries)
    }

    /// Serializes preamble and entries back to a string suitable for writing to disk.
    #[must_use]
    pub fn serialize(preamble: &[String], entries: &[HostEntry]) -> String {
        let mut output = String::new();

        for line in preamble {
            output.push_str(line);
            output.push('\n');
        }

        if !preamble.is_empty()
            && !entries.is_empty()
            && !preamble.last().map_or(false, |l| l.trim().is_empty())
        {
            output.push('\n');
        }

        for entry in entries {
            output.push_str(&entry.to_line());
            output.push('\n');
        }

        output
    }

    fn try_parse_host_line(line: &str) -> Option<HostEntry> {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 2 {
            return None;
        }

        let ip = parts[0];
        if !Self::validate_ip(ip) {
            return None;
        }

        let host = parts[1].to_owned();
        let comment = if let Some(idx) = line.find('#') {
            line[idx + 1..].trim().to_owned()
        } else {
            String::new()
        };

        Some(HostEntry {
            enabled: true,
            ip: ip.to_owned(),
            host,
            comment,
        })
    }

    /// Returns the default system hosts file path for the target platform.
    #[must_use]
    pub fn default_path() -> PathBuf {
        #[cfg(windows)]
        {
            if let Ok(sys_root) = std::env::var("SystemRoot") {
                Path::new(&sys_root)
                    .join("System32")
                    .join("drivers")
                    .join("etc")
                    .join("hosts")
            } else {
                PathBuf::from(r"C:\Windows\System32\drivers\etc\hosts")
            }
        }
        #[cfg(not(windows))]
        {
            PathBuf::from("/etc/hosts")
        }
    }

    /// Reads and parses the hosts file from `path` (or default if `None`).
    pub fn load_from_file(
        path: Option<&Path>,
    ) -> Result<(Vec<String>, Vec<HostEntry>, PathBuf, String), String> {
        let target_path = path.map_or_else(Self::default_path, Path::to_path_buf);
        let content = fs::read_to_string(&target_path)
            .map_err(|err| format!("{}: {}", target_path.display(), err))?;

        let last_modified = fs::metadata(&target_path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| {
                t.duration_since(std::time::UNIX_EPOCH)
                    .ok()
                    .map(|d| format!("{}s", d.as_secs()))
            })
            .unwrap_or_default();

        let (preamble, entries) = Self::parse(&content);
        Ok((preamble, entries, target_path, last_modified))
    }

    /// Validates whether `ip` is a valid IPv4 or IPv6 address.
    #[must_use]
    pub fn is_valid_ip(ip: &str) -> bool {
        Self::validate_ip(ip)
    }

    /// Writes the serialized hosts file content to `path` (or default if `None`).
    pub fn save_to_file(path: Option<&Path>, content: &str) -> Result<(), String> {
        let target_path = path.map_or_else(Self::default_path, Path::to_path_buf);
        fs::write(&target_path, content)
            .map_err(|err| format!("{}: {}", target_path.display(), err))
    }
}

/// Returns the default system hosts file path for the target platform.
#[must_use]
pub fn default_path() -> PathBuf {
    HostsParser::default_path()
}

/// Reads and parses the hosts file from `path` (or default if `None`).
pub fn load_from_file(
    path: Option<&Path>,
) -> Result<(Vec<String>, Vec<HostEntry>, PathBuf, String), String> {
    HostsParser::load_from_file(path)
}

/// Writes the serialized hosts file content to `path` (or default if `None`).
pub fn save_to_file(path: Option<&Path>, content: &str) -> Result<(), String> {
    HostsParser::save_to_file(path, content)
}

/// Validates whether `ip` is a valid IPv4 or IPv6 address.
#[must_use]
pub fn is_valid_ip(ip: &str) -> bool {
    HostsParser::validate_ip(ip)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_ip() {
        assert!(HostsParser::validate_ip("127.0.0.1"));
        assert!(HostsParser::validate_ip("::1"));
        assert!(!HostsParser::validate_ip("invalid.host.name"));
        assert!(!HostsParser::validate_ip("999.999.999.999"));
    }

    #[test]
    fn test_hosts_parser_roundtrip() {
        let sample = "\
# Copyright (c) 1993-2009 Microsoft Corp.
# This is a sample HOSTS file used by Microsoft TCP/IP for Windows.

127.0.0.1       localhost
::1             localhost
# 192.168.1.100 dev.local # local dev server
";
        let (preamble, entries) = HostsParser::parse(sample);
        assert_eq!(preamble.len(), 3);
        assert_eq!(entries.len(), 3);

        assert!(entries[0].enabled);
        assert_eq!(entries[0].ip, "127.0.0.1");
        assert_eq!(entries[0].host, "localhost");

        assert!(entries[1].enabled);
        assert_eq!(entries[1].ip, "::1");
        assert_eq!(entries[1].host, "localhost");

        assert!(!entries[2].enabled);
        assert_eq!(entries[2].ip, "192.168.1.100");
        assert_eq!(entries[2].host, "dev.local");
        assert_eq!(entries[2].comment, "local dev server");

        let serialized = HostsParser::serialize(&preamble, &entries);
        assert!(serialized.contains("127.0.0.1"));
        assert!(serialized.contains("# 192.168.1.100"));
    }
}
