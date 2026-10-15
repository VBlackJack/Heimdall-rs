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

//! TCP port scanner engine, as the C# `PortScanEngine`: input validation, well-known service
//! identification, open/closed/filtered status, summary statistics, and CSV export. The
//! scan itself, the host resolved once and its ports probed [`MAX_CONCURRENT`] at a time,
//! runs in `heimdall_app::net_tools`.

use std::fmt::Write as _;
use std::io::ErrorKind;

use super::default_creds::escape_csv;
use super::net_host;

/// Standard preset of popular network service ports.
pub const DEFAULT_COMMON_PORTS: &str =
    "21,22,23,25,53,80,110,135,139,143,443,445,993,995,1433,1521,3306,3389,5432,5900,8080,8443";
/// Default timeout in milliseconds for each port probe attempt, as the C#
/// `PortScanEngine.ConnectTimeoutMs`: Windows takes about two seconds to report a refusal,
/// so a shorter wait would show a closed port as filtered.
pub const DEFAULT_PORT_TIMEOUT_MS: u64 = 2000;
/// Shortest timeout of a connection attempt, in milliseconds.
pub const MIN_TIMEOUT_MS: u64 = 100;
/// Longest timeout of a connection attempt, in milliseconds.
pub const MAX_TIMEOUT_MS: u64 = 10000;
/// Ports probed at once, as the C# `PortScanEngine.MaxConcurrent`.
pub const MAX_CONCURRENT: usize = 50;
/// Port count above which a scan is confirmed first, as the C#
/// `PortScanEngine.LargePortCountWarningThreshold`.
pub const LARGE_PORT_COUNT_WARNING: usize = 10000;
/// Time the host name may take to resolve, in milliseconds.
pub const RESOLVE_TIMEOUT_MS: u64 = 5000;

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

/// Validates the typed host, as the C# `InputValidator.Validate(host, "Address")` before a
/// scan (`PortScanViewModel.cs`); returns it trimmed, or an error translation key.
pub fn validate_host(host_text: &str) -> Result<String, &'static str> {
    if host_text.trim().is_empty() {
        return Err("ui-tool-portscan-error-host-required");
    }
    net_host::checked_host(host_text).ok_or("ui-tool-portscan-error-host-invalid")
}

/// Validates the typed timeout of each connection attempt: a whole number of milliseconds
/// within [`MIN_TIMEOUT_MS`] and [`MAX_TIMEOUT_MS`], never replaced by the default.
pub fn validate_timeout(timeout_text: &str) -> Result<u64, &'static str> {
    match timeout_text.trim().parse::<u64>() {
        Ok(ms) if (MIN_TIMEOUT_MS..=MAX_TIMEOUT_MS).contains(&ms) => Ok(ms),
        _ => Err("ui-tool-portscan-error-timeout-invalid"),
    }
}

/// Whether a scan of `port_count` ports is to be confirmed first, as the C# asks above
/// [`LARGE_PORT_COUNT_WARNING`] (`PortScannerView.xaml.cs:219`).
#[must_use]
pub const fn needs_confirmation(port_count: usize) -> bool {
    port_count > LARGE_PORT_COUNT_WARNING
}

/// The status of a port whose connection failed with `kind`, the most telling of the
/// failures over the host's addresses; `None` when no attempt ended before the timeout.
/// Only a refusal says the port is closed: a timeout, or a router's "unreachable", says
/// something stands in between.
#[must_use]
pub fn status_of_failure(kind: Option<ErrorKind>) -> PortStatus {
    match kind {
        Some(ErrorKind::ConnectionRefused) => PortStatus::Closed,
        _ => PortStatus::Filtered,
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

/// Formats a CSV export string from the scan results, in port order, its text cells
/// quoted and neutralized as the C# `SanitizeCsvCell` does.
#[must_use]
pub fn format_port_scan_csv(results: &[PortProbeResult]) -> String {
    let mut sorted: Vec<&PortProbeResult> = results.iter().collect();
    sorted.sort_by_key(|r| r.port);
    let mut csv = String::from("Port,Service,Status,LatencyMs\n");
    for r in sorted {
        let lat_str = if r.latency_ms >= 0.0 {
            format!("{:.1}", r.latency_ms)
        } else {
            String::new()
        };
        let _ = writeln!(
            csv,
            "{},{},{},{}",
            r.port,
            escape_csv(r.service),
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
        assert!(csv.contains("443,\"HTTPS\",Open,12.0"));
    }

    #[test]
    fn the_csv_is_in_port_order_whatever_order_the_probes_ended_in() {
        let probe = |port| PortProbeResult {
            port,
            service: service_name_for_port(port),
            status: PortStatus::Closed,
            latency_ms: -1.0,
        };
        let csv = format_port_scan_csv(&[probe(443), probe(22), probe(80)]);
        let ports: Vec<&str> = csv
            .lines()
            .skip(1)
            .map(|line| line.split(',').next().unwrap())
            .collect();
        assert_eq!(ports, ["22", "80", "443"]);
    }

    #[test]
    fn the_host_and_the_timeout_are_validated_not_replaced() {
        assert_eq!(validate_host(" 10.0.0.1 "), Ok("10.0.0.1".into()));
        assert_eq!(
            validate_host(""),
            Err("ui-tool-portscan-error-host-required")
        );
        assert_eq!(
            validate_host("-sS example.com"),
            Err("ui-tool-portscan-error-host-invalid")
        );
        assert_eq!(validate_timeout("250"), Ok(250));
        for refused in ["", "abc", "50", "10001"] {
            assert_eq!(
                validate_timeout(refused),
                Err("ui-tool-portscan-error-timeout-invalid"),
                "{refused:?}"
            );
        }
    }

    #[test]
    fn a_scan_above_ten_thousand_ports_is_confirmed_first() {
        assert!(!needs_confirmation(LARGE_PORT_COUNT_WARNING));
        assert!(needs_confirmation(LARGE_PORT_COUNT_WARNING + 1));
    }

    #[test]
    fn only_a_refusal_says_a_port_is_closed() {
        assert_eq!(
            status_of_failure(Some(ErrorKind::ConnectionRefused)),
            PortStatus::Closed
        );
        assert_eq!(
            status_of_failure(Some(ErrorKind::TimedOut)),
            PortStatus::Filtered
        );
        assert_eq!(status_of_failure(None), PortStatus::Filtered);
    }
}
