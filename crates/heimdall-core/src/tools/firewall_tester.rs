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

//! Pure firewall-probe and egress connectivity testing engine matching the C#
//! `Heimdall.Core.Network.FirewallProbeEngine`.
//!
//! Provides host and port list parsing, TCP connection probing with latency measurement,
//! summary calculation, and matrix CSV export.

use std::fmt;
use std::net::{TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

/// Default TCP connection timeout in milliseconds.
pub const DEFAULT_CONNECT_TIMEOUT_MS: u64 = 3000;
/// Default common egress ports for quick testing.
pub const DEFAULT_EGRESS_PORTS: [u16; 15] = [
    21, 22, 25, 53, 80, 110, 143, 443, 465, 587, 993, 995, 3389, 8080, 8443,
];

/// Result status of a firewall port probe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FwProbeStatus {
    Open,
    Closed,
    Timeout,
}

impl fmt::Display for FwProbeStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Open => write!(f, "Open"),
            Self::Closed => write!(f, "Closed"),
            Self::Timeout => write!(f, "Filtered/Timeout"),
        }
    }
}

/// A single host-port probe result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FwProbeResult {
    pub host: String,
    pub port: u16,
    pub status: FwProbeStatus,
    pub latency_ms: Option<u64>,
}

/// Summary counts of a firewall scan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FwProbeSummary {
    pub open: usize,
    pub closed: usize,
    pub timeout: usize,
    pub total: usize,
}

/// Parses multi-line host input into unique, trimmed host strings.
#[must_use]
pub fn parse_hosts(input: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for line in input.lines() {
        let trimmed = line.trim();
        if !trimmed.is_empty() && seen.insert(trimmed.to_owned()) {
            out.push(trimmed.to_owned());
        }
    }

    out
}

/// Parses comma-separated ports and ranges into an ordered, deduplicated list.
#[must_use]
pub fn parse_ports(input: &str) -> Vec<u16> {
    super::cert_inspector::parse_ports(input)
}

/// Probes a single host and port using a TCP handshake.
pub fn probe_host_port(host: &str, port: u16, timeout_ms: u64) -> FwProbeResult {
    let addr_str = format!("{}:{}", host, port);
    let socket_addrs = match addr_str.to_socket_addrs() {
        Ok(addrs) => addrs.collect::<Vec<_>>(),
        Err(_) => {
            return FwProbeResult {
                host: host.to_owned(),
                port,
                status: FwProbeStatus::Timeout,
                latency_ms: None,
            };
        }
    };

    if socket_addrs.is_empty() {
        return FwProbeResult {
            host: host.to_owned(),
            port,
            status: FwProbeStatus::Timeout,
            latency_ms: None,
        };
    }

    let timeout = Duration::from_millis(timeout_ms.max(200));
    let start = Instant::now();

    match TcpStream::connect_timeout(&socket_addrs[0], timeout) {
        Ok(_) => {
            let elapsed = start.elapsed().as_millis() as u64;
            FwProbeResult {
                host: host.to_owned(),
                port,
                status: FwProbeStatus::Open,
                latency_ms: Some(elapsed),
            }
        }
        Err(e) => {
            let status = match e.kind() {
                std::io::ErrorKind::ConnectionRefused => FwProbeStatus::Closed,
                std::io::ErrorKind::TimedOut => FwProbeStatus::Timeout,
                _ => FwProbeStatus::Timeout,
            };
            FwProbeResult {
                host: host.to_owned(),
                port,
                status,
                latency_ms: None,
            }
        }
    }
}

/// Aggregates probe results into open/closed/timeout counts.
#[must_use]
pub fn compute_summary(results: &[FwProbeResult]) -> FwProbeSummary {
    let mut open = 0;
    let mut closed = 0;
    let mut timeout = 0;

    for r in results {
        match r.status {
            FwProbeStatus::Open => open += 1,
            FwProbeStatus::Closed => closed += 1,
            FwProbeStatus::Timeout => timeout += 1,
        }
    }

    FwProbeSummary {
        open,
        closed,
        timeout,
        total: results.len(),
    }
}

/// Builds CSV content formatted as a matrix: one row per host, one column per port.
#[must_use]
pub fn build_matrix_csv(results: &[FwProbeResult], hosts: &[String], ports: &[u16]) -> String {
    let mut map: std::collections::HashMap<(&str, u16), &FwProbeResult> =
        std::collections::HashMap::new();
    for r in results {
        map.insert((r.host.as_str(), r.port), r);
    }

    let mut out = String::new();
    out.push_str("Host");
    for &p in ports {
        out.push_str(&format!(",Port {}", p));
    }
    out.push('\n');

    for h in hosts {
        out.push_str(h);
        for &p in ports {
            out.push(',');
            if let Some(res) = map.get(&(h.as_str(), p)) {
                match res.status {
                    FwProbeStatus::Open => {
                        if let Some(lat) = res.latency_ms {
                            out.push_str(&format!("Open ({}ms)", lat));
                        } else {
                            out.push_str("Open");
                        }
                    }
                    FwProbeStatus::Closed => out.push_str("Closed"),
                    FwProbeStatus::Timeout => out.push_str("Filtered/Timeout"),
                }
            } else {
                out.push('-');
            }
        }
        out.push('\n');
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_hosts_deduplication() {
        let input = "192.168.1.1\n  192.168.1.2  \n\n192.168.1.1\n";
        let parsed = parse_hosts(input);
        assert_eq!(parsed, vec!["192.168.1.1", "192.168.1.2"]);
    }

    #[test]
    fn summary_computation() {
        let results = vec![
            FwProbeResult {
                host: "a".to_owned(),
                port: 80,
                status: FwProbeStatus::Open,
                latency_ms: Some(10),
            },
            FwProbeResult {
                host: "a".to_owned(),
                port: 443,
                status: FwProbeStatus::Closed,
                latency_ms: None,
            },
            FwProbeResult {
                host: "b".to_owned(),
                port: 80,
                status: FwProbeStatus::Timeout,
                latency_ms: None,
            },
        ];

        let summary = compute_summary(&results);
        assert_eq!(summary.open, 1);
        assert_eq!(summary.closed, 1);
        assert_eq!(summary.timeout, 1);
        assert_eq!(summary.total, 3);
    }

    #[test]
    fn matrix_csv_formatting() {
        let hosts = vec!["host1".to_owned()];
        let ports = vec![80, 443];
        let results = vec![
            FwProbeResult {
                host: "host1".to_owned(),
                port: 80,
                status: FwProbeStatus::Open,
                latency_ms: Some(5),
            },
            FwProbeResult {
                host: "host1".to_owned(),
                port: 443,
                status: FwProbeStatus::Closed,
                latency_ms: None,
            },
        ];

        let csv = build_matrix_csv(&results, &hosts, &ports);
        assert!(csv.contains("Host,Port 80,Port 443"));
        assert!(csv.contains("host1,Open (5ms),Closed"));
    }
}
