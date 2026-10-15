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

//! TCP port scanner engine: port range parsing, well-known service identification,
//! handshake probes, open/closed/filtered status detection, summary statistics, and CSV export.

use std::fmt::Write as _;
use std::net::{TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

/// Standard preset of popular network service ports.
pub const DEFAULT_COMMON_PORTS: &str =
    "21,22,23,25,53,80,110,135,139,143,443,445,993,995,1433,1521,3306,3389,5432,5900,8080,8443";
/// Default timeout in milliseconds for each port probe attempt.
pub const DEFAULT_PORT_TIMEOUT_MS: u64 = 1000;

/// Status classification of a scanned TCP port.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortStatus {
    /// Port accepted TCP handshake (open and listening).
    Open,
    /// Connection was actively refused (closed).
    Closed,
    /// Connection attempt timed out (filtered by firewall or dropped).
    Filtered,
}

impl PortStatus {
    /// Display label for the status.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Open => "Open",
            Self::Closed => "Closed",
            Self::Filtered => "Filtered",
        }
    }
}

/// Result of probing a single TCP port.
#[derive(Debug, Clone, PartialEq)]
pub struct PortProbeResult {
    /// The TCP port number.
    pub port: u16,
    /// Identified service name (e.g. "HTTP", "SSH", "HTTPS", "Unknown").
    pub service: &'static str,
    /// Port status outcome.
    pub status: PortStatus,
    /// Latency of the connection attempt in milliseconds (-1.0 if filtered/closed without timing).
    pub latency_ms: f64,
}

/// Summary counts across all scanned ports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PortScanSummary {
    /// Total ports in the scan target.
    pub total: usize,
    /// Number of open ports found.
    pub open: usize,
    /// Number of closed ports found.
    pub closed: usize,
    /// Number of filtered / dropped ports.
    pub filtered: usize,
}

/// Maps standard well-known TCP port numbers to their standard protocol names.
#[must_use]
pub const fn service_name_for_port(port: u16) -> &'static str {
    match port {
        20 => "FTP-Data",
        21 => "FTP",
        22 => "SSH",
        23 => "Telnet",
        25 => "SMTP",
        53 => "DNS",
        67 | 68 => "DHCP",
        69 => "TFTP",
        80 => "HTTP",
        88 => "Kerberos",
        110 => "POP3",
        119 => "NNTP",
        123 => "NTP",
        135 => "MS-RPC",
        137 | 138 => "NetBIOS",
        139 => "NetBIOS-SSN",
        143 => "IMAP",
        161 | 162 => "SNMP",
        389 => "LDAP",
        443 => "HTTPS",
        445 => "SMB",
        465 => "SMTPS",
        514 => "Syslog",
        587 => "SMTP-Submission",
        636 => "LDAPS",
        993 => "IMAPS",
        995 => "POP3S",
        1433 => "MSSQL",
        1521 => "Oracle",
        2049 => "NFS",
        2375 | 2376 => "Docker",
        3306 => "MySQL",
        3389 => "RDP",
        5432 => "PostgreSQL",
        5900..=5905 => "VNC",
        5985 => "WinRM-HTTP",
        5986 => "WinRM-HTTPS",
        6379 => "Redis",
        8080 => "HTTP-Proxy",
        8443 => "HTTPS-Alt",
        9000 => "SonarQube/Portainer",
        9090 => "Prometheus",
        9200 => "Elasticsearch",
        27017 => "MongoDB",
        _ => "Unknown",
    }
}

/// Executes a synchronous TCP handshake probe on `port` of `host`.
#[must_use]
pub fn probe_single_port(host: &str, port: u16, timeout_ms: u64) -> PortProbeResult {
    let service = service_name_for_port(port);
    let timeout = Duration::from_millis(timeout_ms);
    let start = Instant::now();

    let Ok(mut addrs) = (host, port).to_socket_addrs() else {
        return PortProbeResult {
            port,
            service,
            status: PortStatus::Filtered,
            latency_ms: -1.0,
        };
    };

    let Some(addr) = addrs.next() else {
        return PortProbeResult {
            port,
            service,
            status: PortStatus::Filtered,
            latency_ms: -1.0,
        };
    };

    match TcpStream::connect_timeout(&addr, timeout) {
        Ok(_stream) => {
            let latency_ms = start.elapsed().as_secs_f64() * 1000.0;
            PortProbeResult {
                port,
                service,
                status: PortStatus::Open,
                latency_ms,
            }
        }
        Err(err) => {
            let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
            let status = match err.kind() {
                std::io::ErrorKind::ConnectionRefused => PortStatus::Closed,
                std::io::ErrorKind::TimedOut => PortStatus::Filtered,
                _ => {
                    if elapsed_ms >= (timeout_ms as f64 * 0.9) {
                        PortStatus::Filtered
                    } else {
                        PortStatus::Closed
                    }
                }
            };
            PortProbeResult {
                port,
                service,
                status,
                latency_ms: -1.0,
            }
        }
    }
}

/// Computes summary statistics from a slice of port probe results.
#[must_use]
pub fn compute_port_summary(results: &[PortProbeResult]) -> PortScanSummary {
    let mut summary = PortScanSummary {
        total: results.len(),
        open: 0,
        closed: 0,
        filtered: 0,
    };

    for r in results {
        match r.status {
            PortStatus::Open => summary.open += 1,
            PortStatus::Closed => summary.closed += 1,
            PortStatus::Filtered => summary.filtered += 1,
        }
    }

    summary
}

/// Formats a CSV export string from the scan results.
#[must_use]
pub fn format_port_scan_csv(results: &[PortProbeResult]) -> String {
    let mut csv = String::from("Port,Service,Status,LatencyMs\n");
    for r in results {
        let lat_str = if r.latency_ms >= 0.0 {
            format!("{:.1}", r.latency_ms)
        } else {
            String::new()
        };
        let _ = writeln!(
            csv,
            "{},{},{},{}",
            r.port,
            r.service,
            r.status.label(),
            lat_str
        );
    }
    csv
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_service_names() {
        assert_eq!(service_name_for_port(22), "SSH");
        assert_eq!(service_name_for_port(80), "HTTP");
        assert_eq!(service_name_for_port(443), "HTTPS");
        assert_eq!(service_name_for_port(3389), "RDP");
        assert_eq!(service_name_for_port(5985), "WinRM-HTTP");
        assert_eq!(service_name_for_port(12345), "Unknown");
    }

    #[test]
    fn test_compute_port_summary() {
        let results = vec![
            PortProbeResult {
                port: 80,
                service: "HTTP",
                status: PortStatus::Open,
                latency_ms: 5.2,
            },
            PortProbeResult {
                port: 81,
                service: "Unknown",
                status: PortStatus::Closed,
                latency_ms: -1.0,
            },
            PortProbeResult {
                port: 82,
                service: "Unknown",
                status: PortStatus::Filtered,
                latency_ms: -1.0,
            },
        ];

        let summary = compute_port_summary(&results);
        assert_eq!(summary.total, 3);
        assert_eq!(summary.open, 1);
        assert_eq!(summary.closed, 1);
        assert_eq!(summary.filtered, 1);
    }

    #[test]
    fn test_format_port_scan_csv() {
        let results = vec![PortProbeResult {
            port: 443,
            service: "HTTPS",
            status: PortStatus::Open,
            latency_ms: 12.0,
        }];
        let csv = format_port_scan_csv(&results);
        assert!(csv.contains("Port,Service,Status,LatencyMs"));
        assert!(csv.contains("443,HTTPS,Open,12.0"));
    }
}
