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

//! TCP banner grabbing engine, service fingerprinting, and port parser.

use std::collections::BTreeSet;
use std::io::Read;
use std::net::{TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

/// Maximum bytes to read from a banner response.
pub const BANNER_MAX_BYTES: usize = 512;

/// Default connection timeout for banner probing in milliseconds.
pub const DEFAULT_CONNECT_TIMEOUT_MS: u64 = 2000;

/// Default banner read timeout in milliseconds.
pub const DEFAULT_READ_TIMEOUT_MS: u64 = 2000;

/// Parses a comma-separated or range-based port specification (e.g. "21,22,80,443,8000-8005").
#[must_use]
pub fn parse_ports(input: &str) -> Vec<u16> {
    let mut ports = BTreeSet::new();

    for segment in input.split(',') {
        let trimmed = segment.trim();
        if trimmed.is_empty() {
            continue;
        }

        if let Some((start_str, end_str)) = trimmed.split_once('-') {
            if let (Ok(start), Ok(end)) = (
                start_str.trim().parse::<u16>(),
                end_str.trim().parse::<u16>(),
            ) {
                if start <= end && start >= 1 {
                    for p in start..=end {
                        ports.insert(p);
                    }
                }
            }
        } else if let Ok(port) = trimmed.parse::<u16>() {
            if port >= 1 {
                ports.insert(port);
            }
        }
    }

    ports.into_iter().collect()
}

/// Cleans control characters from raw banner bytes, preserving printable ASCII and whitespace.
#[must_use]
pub fn clean_banner(raw: &str) -> Option<String> {
    let mut cleaned = String::with_capacity(raw.len());
    let mut last_was_space = false;

    for c in raw.chars() {
        if c.is_control() || c == '\x7F' {
            if !last_was_space {
                cleaned.push(' ');
                last_was_space = true;
            }
        } else if c.is_whitespace() {
            if !last_was_space {
                cleaned.push(' ');
                last_was_space = true;
            }
        } else {
            cleaned.push(c);
            last_was_space = false;
        }
    }

    let trimmed = cleaned.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Common default service name for a well-known port.
#[must_use]
pub fn default_port_service(port: u16) -> &'static str {
    match port {
        21 => "FTP",
        22 => "SSH",
        23 => "Telnet",
        25 => "SMTP",
        53 => "DNS",
        80 => "HTTP",
        110 => "POP3",
        143 => "IMAP",
        389 => "LDAP",
        443 => "HTTPS",
        465 => "SMTPS",
        587 => "SMTP (Submission)",
        636 => "LDAPS",
        993 => "IMAPS",
        995 => "POP3S",
        1433 => "MSSQL",
        1521 => "Oracle",
        3306 => "MySQL",
        3389 => "RDP",
        5432 => "PostgreSQL",
        5900 => "VNC",
        6379 => "Redis",
        8080 => "HTTP-Alt",
        8443 => "HTTPS-Alt",
        27017 => "MongoDB",
        _ => "Unknown",
    }
}

/// Identifies the network service from port and inspected banner content.
#[must_use]
pub fn identify_service(port: u16, banner: Option<&str>) -> String {
    let base = default_port_service(port);

    let Some(b) = banner else {
        return base.to_string();
    };

    let lower = b.to_ascii_lowercase();

    if lower.contains("openssh") {
        return "OpenSSH".to_string();
    }
    if lower.contains("dropbear") {
        return "Dropbear SSH".to_string();
    }
    if lower.contains("apache") {
        return "Apache HTTP".to_string();
    }
    if lower.contains("nginx") {
        return "nginx".to_string();
    }
    if lower.contains("microsoft-iis") {
        return "IIS".to_string();
    }
    if lower.contains("postfix") {
        return "Postfix SMTP".to_string();
    }
    if lower.contains("exim") {
        return "Exim SMTP".to_string();
    }
    if lower.contains("dovecot") {
        return "Dovecot".to_string();
    }
    if lower.contains("mysql") {
        return "MySQL".to_string();
    }
    if lower.contains("postgresql") || lower.contains("pgsql") {
        return "PostgreSQL".to_string();
    }
    if lower.contains("redis") {
        return "Redis".to_string();
    }
    if lower.contains("mongodb") || lower.contains("mongod") {
        return "MongoDB".to_string();
    }
    if lower.contains("proftpd") {
        return "ProFTPD".to_string();
    }
    if lower.contains("vsftpd") {
        return "vsftpd".to_string();
    }
    if lower.contains("filezilla") {
        return "FileZilla FTP".to_string();
    }

    base.to_string()
}

/// Outcome of banner probing on a single port.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BannerProbeResult {
    /// Port probed.
    pub port: u16,
    /// Service identification.
    pub service: String,
    /// Response time formatted as string (e.g. "12 ms" or "-").
    pub response_time: String,
    /// Captured banner content if open and returned data.
    pub banner: Option<String>,
}

/// Probes a single host:port for banner data synchronously.
#[must_use]
pub fn probe_port_banner(
    host: &str,
    port: u16,
    connect_timeout_ms: u64,
    read_timeout_ms: u64,
) -> BannerProbeResult {
    let start = Instant::now();
    let conn_dur = Duration::from_millis(connect_timeout_ms);
    let read_dur = Duration::from_millis(read_timeout_ms);

    let Ok(mut addrs) = (host, port).to_socket_addrs() else {
        let service = identify_service(port, None);
        return BannerProbeResult {
            port,
            service,
            response_time: "-".to_string(),
            banner: None,
        };
    };

    let Some(addr) = addrs.next() else {
        let service = identify_service(port, None);
        return BannerProbeResult {
            port,
            service,
            response_time: "-".to_string(),
            banner: None,
        };
    };

    match TcpStream::connect_timeout(&addr, conn_dur) {
        Ok(mut stream) => {
            let elapsed_ms = start.elapsed().as_millis();
            let response_time = format!("{elapsed_ms} ms");

            let _ = stream.set_read_timeout(Some(read_dur));
            let mut buffer = [0u8; BANNER_MAX_BYTES];
            let banner = match stream.read(&mut buffer) {
                Ok(n) if n > 0 => {
                    let raw = String::from_utf8_lossy(&buffer[..n]);
                    clean_banner(&raw)
                }
                _ => None,
            };

            let service = identify_service(port, banner.as_deref());
            BannerProbeResult {
                port,
                service,
                response_time,
                banner,
            }
        }
        _ => {
            let service = identify_service(port, None);
            BannerProbeResult {
                port,
                service,
                response_time: "-".to_string(),
                banner: None,
            }
        }
    }
}

/// Formats a list of banner probe results as CSV text.
#[must_use]
pub fn build_csv_export(results: &[BannerProbeResult]) -> String {
    let mut out = String::from("Port,Service,Response Time,Banner\n");
    for r in results {
        let banner_escaped = match &r.banner {
            Some(b) => format!("\"{}\"", b.replace('"', "\"\"")),
            None => String::new(),
        };
        out.push_str(&format!(
            "{},{},{},{}\n",
            r.port, r.service, r.response_time, banner_escaped
        ));
    }
    out
}

/// Formats a list of banner probe results as tabulated plain text.
#[must_use]
pub fn build_clipboard_text(results: &[BannerProbeResult]) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "{:<8} {:<18} {:<12} {}\n",
        "Port", "Service", "Time", "Banner"
    ));
    out.push_str(&format!("{}\n", "-".repeat(60)));
    for r in results {
        let b = r.banner.as_deref().unwrap_or("-");
        out.push_str(&format!(
            "{:<8} {:<18} {:<12} {}\n",
            r.port, r.service, r.response_time, b
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_ports_ranges_and_singles() {
        assert_eq!(parse_ports("21, 22, 80"), vec![21, 22, 80]);
        assert_eq!(parse_ports("80-83"), vec![80, 81, 82, 83]);
        assert_eq!(parse_ports("443, 80-82, 22"), vec![22, 80, 81, 82, 443]);
        assert_eq!(parse_ports("invalid, -5, 0, 80"), vec![80]);
    }

    #[test]
    fn test_clean_banner() {
        let raw = "SSH-2.0-OpenSSH_8.9p1 Ubuntu-3ubuntu0.6\r\n";
        assert_eq!(
            clean_banner(raw),
            Some("SSH-2.0-OpenSSH_8.9p1 Ubuntu-3ubuntu0.6".to_string())
        );
        assert_eq!(clean_banner("\0\0   \r\n"), None);
    }

    #[test]
    fn test_identify_service_with_banner() {
        assert_eq!(identify_service(22, Some("SSH-2.0-OpenSSH_8.9")), "OpenSSH");
        assert_eq!(identify_service(80, Some("Server: nginx/1.18.0")), "nginx");
        assert_eq!(identify_service(22, None), "SSH");
        assert_eq!(identify_service(9999, None), "Unknown");
    }

    #[test]
    fn test_csv_export() {
        let results = vec![BannerProbeResult {
            port: 22,
            service: "OpenSSH".into(),
            response_time: "5 ms".into(),
            banner: Some("SSH-2.0-OpenSSH_8.9".into()),
        }];
        let csv = build_csv_export(&results);
        assert!(csv.contains("Port,Service,Response Time,Banner"));
        assert!(csv.contains("22,OpenSSH,5 ms,\"SSH-2.0-OpenSSH_8.9\""));
    }
}
