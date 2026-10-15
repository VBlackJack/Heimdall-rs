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

//! DNS record lookup engine: record types, server presets, resolution via standard
//! query and nslookup parsing, record classification, and timing.

use std::fmt;
use std::net::ToSocketAddrs;
use std::process::Command;
use std::time::Instant;

/// Supported DNS query record types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DnsRecordType {
    /// IPv4 address.
    A,
    /// IPv6 address.
    Aaaa,
    /// Mail exchange server.
    Mx,
    /// Canonical name alias.
    Cname,
    /// Text annotations / verification records.
    Txt,
    /// Authoritative name server.
    Ns,
    /// Pointer for reverse DNS lookup.
    Ptr,
    /// Start of authority zone info.
    Soa,
    /// Service location record.
    Srv,
    /// Any available records.
    Any,
}

impl DnsRecordType {
    /// Every supported record type in UI display order.
    pub const ALL: [Self; 10] = [
        Self::A,
        Self::Aaaa,
        Self::Mx,
        Self::Cname,
        Self::Txt,
        Self::Ns,
        Self::Ptr,
        Self::Soa,
        Self::Srv,
        Self::Any,
    ];

    /// The wire / command-line token for this record type.
    #[must_use]
    pub const fn wire_format(self) -> &'static str {
        match self {
            Self::A => "A",
            Self::Aaaa => "AAAA",
            Self::Mx => "MX",
            Self::Cname => "CNAME",
            Self::Txt => "TXT",
            Self::Ns => "NS",
            Self::Ptr => "PTR",
            Self::Soa => "SOA",
            Self::Srv => "SRV",
            Self::Any => "ANY",
        }
    }
}

impl fmt::Display for DnsRecordType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.wire_format())
    }
}

/// Popular public DNS server presets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DnsServerPreset {
    /// System default resolver.
    DefaultSystem,
    /// Cloudflare DNS (1.1.1.1).
    Cloudflare,
    /// Google Public DNS (8.8.8.8).
    Google,
    /// Quad9 DNS (9.9.9.9).
    Quad9,
    /// OpenDNS (208.67.222.222).
    OpenDns,
}

impl DnsServerPreset {
    /// All presets in order.
    pub const ALL: [Self; 5] = [
        Self::DefaultSystem,
        Self::Cloudflare,
        Self::Google,
        Self::Quad9,
        Self::OpenDns,
    ];

    /// IP address of the server, or `None` for system default.
    #[must_use]
    pub const fn server_ip(self) -> Option<&'static str> {
        match self {
            Self::DefaultSystem => None,
            Self::Cloudflare => Some("1.1.1.1"),
            Self::Google => Some("8.8.8.8"),
            Self::Quad9 => Some("9.9.9.9"),
            Self::OpenDns => Some("208.67.222.222"),
        }
    }

    /// English display name with IP for dropdown selection.
    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::DefaultSystem => "Default System Resolver",
            Self::Cloudflare => "Cloudflare (1.1.1.1)",
            Self::Google => "Google (8.8.8.8)",
            Self::Quad9 => "Quad9 (9.9.9.9)",
            Self::OpenDns => "OpenDNS (208.67.222.222)",
        }
    }
}

impl fmt::Display for DnsServerPreset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.display_name())
    }
}

/// A request to query DNS records.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DnsLookupRequest {
    /// Target host name or domain.
    pub host: String,
    /// The record type to query.
    pub record_type: DnsRecordType,
    /// Optional custom DNS server IP address.
    pub server: Option<String>,
}

/// The result of a DNS record lookup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DnsLookupResult {
    /// Target host queried.
    pub host: String,
    /// Record type queried.
    pub record_type: DnsRecordType,
    /// DNS server used, if any.
    pub server: Option<String>,
    /// Extracted answer records.
    pub records: Vec<String>,
    /// Any alias (CNAME) records reported.
    pub aliases: Vec<String>,
    /// Execution duration in milliseconds.
    pub elapsed_ms: u64,
    /// Raw command or resolver output text.
    pub raw_output: String,
    /// Failure error message, if unsuccessful.
    pub error: Option<String>,
}

impl DnsLookupResult {
    /// True if the query resolved records without an error.
    #[must_use]
    pub fn is_success(&self) -> bool {
        self.error.is_none() && (!self.records.is_empty() || !self.raw_output.is_empty())
    }
}

/// Cleans raw nslookup command output by stripping the resolver header block and trailing noise.
#[must_use]
pub fn clean_nslookup_output(raw: &str) -> String {
    let mut lines: Vec<&str> = raw.lines().map(str::trim).collect();

    // Drop leading "Server: ...", "Address: ..."
    let mut start_idx = 0;
    let mut saw_server = false;
    for (i, line) in lines.iter().enumerate() {
        if line.starts_with("Server:")
            || line.starts_with("Address:")
            || line.starts_with("Addresses:")
        {
            saw_server = true;
        } else if saw_server && line.is_empty() {
            start_idx = i + 1;
            break;
        } else if saw_server && (line.starts_with("Non-authoritative") || line.starts_with("Name:"))
        {
            start_idx = i;
            break;
        }
    }

    if start_idx < lines.len() {
        lines.drain(0..start_idx);
    }

    // Filter out redundant "Non-authoritative answer:" markers while preserving records
    let mut cleaned = Vec::new();
    for line in lines {
        if line.is_empty() && cleaned.last().is_some_and(|l: &&str| l.is_empty()) {
            continue;
        }
        cleaned.push(line);
    }

    cleaned.join("\n").trim().to_string()
}

/// Resolves standard A or AAAA records using the native platform socket resolver.
fn resolve_native_socket(host: &str, record_type: DnsRecordType) -> Option<Vec<String>> {
    let query = format!("{host}:80");
    let addrs = query.to_socket_addrs().ok()?;
    let mut records = Vec::new();

    for socket_addr in addrs {
        let ip = socket_addr.ip();
        match record_type {
            DnsRecordType::A if ip.is_ipv4() => {
                let s = ip.to_string();
                if !records.contains(&s) {
                    records.push(s);
                }
            }
            DnsRecordType::Aaaa if ip.is_ipv6() => {
                let s = ip.to_string();
                if !records.contains(&s) {
                    records.push(s);
                }
            }
            _ => {}
        }
    }

    if records.is_empty() {
        None
    } else {
        Some(records)
    }
}

/// Executes a DNS record query synchronously according to `request`.
#[must_use]
pub fn query_dns_sync(request: &DnsLookupRequest) -> DnsLookupResult {
    let host = request.host.trim();
    if host.is_empty() {
        return DnsLookupResult {
            host: String::new(),
            record_type: request.record_type,
            server: request.server.clone(),
            records: Vec::new(),
            aliases: Vec::new(),
            elapsed_ms: 0,
            raw_output: String::new(),
            error: Some("Host cannot be empty".to_string()),
        };
    }

    let start = Instant::now();

    // Fast path: if querying A or AAAA against default system resolver, try native socket resolution first
    if request.server.is_none()
        && (request.record_type == DnsRecordType::A || request.record_type == DnsRecordType::Aaaa)
    {
        if let Some(records) = resolve_native_socket(host, request.record_type) {
            let elapsed_ms = start.elapsed().as_millis() as u64;
            let raw = records.join("\n");
            return DnsLookupResult {
                host: host.to_string(),
                record_type: request.record_type,
                server: None,
                records: records.clone(),
                aliases: Vec::new(),
                elapsed_ms,
                raw_output: raw,
                error: None,
            };
        }
    }

    // Full path: invoke system nslookup tool
    let mut cmd = Command::new("nslookup");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd.arg(format!("-type={}", request.record_type.wire_format()));
    cmd.arg(host);
    if let Some(srv) = &request.server {
        cmd.arg(srv);
    }

    match cmd.output() {
        Ok(output) => {
            let elapsed_ms = start.elapsed().as_millis() as u64;
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            let combined = if stdout.trim().is_empty() {
                stderr.to_string()
            } else {
                stdout.to_string()
            };

            let cleaned = clean_nslookup_output(&combined);
            let mut records = Vec::new();
            let mut aliases = Vec::new();

            for line in cleaned.lines() {
                let trimmed = line.trim();
                if let Some(alias) = trimmed.strip_prefix("aliases:") {
                    aliases.push(alias.trim().to_string());
                } else if let Some(alias) = trimmed.strip_prefix("canonical name =") {
                    aliases.push(alias.trim().to_string());
                } else if trimmed.starts_with("Address:") || trimmed.starts_with("Addresses:") {
                    let addr = trimmed
                        .strip_prefix("Addresses:")
                        .or_else(|| trimmed.strip_prefix("Address:"))
                        .unwrap_or_default()
                        .trim();
                    if !addr.is_empty() {
                        records.push(addr.to_string());
                    }
                } else if !trimmed.is_empty()
                    && !trimmed.starts_with("Name:")
                    && !trimmed.starts_with("Non-authoritative")
                {
                    records.push(trimmed.to_string());
                }
            }

            let error = if !output.status.success() && records.is_empty() && aliases.is_empty() {
                Some(if cleaned.is_empty() {
                    "DNS lookup failed".to_string()
                } else {
                    cleaned.clone()
                })
            } else {
                None
            };

            DnsLookupResult {
                host: host.to_string(),
                record_type: request.record_type,
                server: request.server.clone(),
                records,
                aliases,
                elapsed_ms,
                raw_output: if cleaned.is_empty() {
                    combined
                } else {
                    cleaned
                },
                error,
            }
        }
        Err(err) => DnsLookupResult {
            host: host.to_string(),
            record_type: request.record_type,
            server: request.server.clone(),
            records: Vec::new(),
            aliases: Vec::new(),
            elapsed_ms: start.elapsed().as_millis() as u64,
            raw_output: String::new(),
            error: Some(format!("Failed to execute nslookup: {err}")),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_record_types_all() {
        assert_eq!(DnsRecordType::ALL.len(), 10);
        assert_eq!(DnsRecordType::A.wire_format(), "A");
        assert_eq!(DnsRecordType::Aaaa.wire_format(), "AAAA");
        assert_eq!(DnsRecordType::Mx.wire_format(), "MX");
    }

    #[test]
    fn test_server_presets() {
        assert_eq!(DnsServerPreset::ALL.len(), 5);
        assert_eq!(DnsServerPreset::Cloudflare.server_ip(), Some("1.1.1.1"));
        assert_eq!(DnsServerPreset::Google.server_ip(), Some("8.8.8.8"));
        assert_eq!(DnsServerPreset::DefaultSystem.server_ip(), None);
    }

    #[test]
    fn test_clean_nslookup_output() {
        let raw = "\
Server:  dns.google
Address:  8.8.8.8

Non-authoritative answer:
Name:    example.com
Addresses:  93.184.216.34
";
        let cleaned = clean_nslookup_output(raw);
        assert!(cleaned.contains("example.com"));
        assert!(cleaned.contains("93.184.216.34"));
        assert!(!cleaned.starts_with("Server:"));
    }

    #[test]
    fn test_empty_host_returns_error() {
        let req = DnsLookupRequest {
            host: "   ".into(),
            record_type: DnsRecordType::A,
            server: None,
        };
        let res = query_dns_sync(&req);
        assert!(res.error.is_some());
    }
}
