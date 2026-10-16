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
//! `Heimdall.Core.Network.FirewallProbeEngine` and `FirewallTesterService`.
//!
//! Provides host and port list parsing within the C# limits, TCP connection probing with
//! latency measurement a bounded number at a time, summary calculation, and matrix CSV
//! export.

use std::collections::HashMap;
use std::net::{SocketAddr, TcpStream, ToSocketAddrs as _};
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use super::csv_cell::quoted;
use super::net_command;
use super::net_host::checked_host;

/// Default TCP connection timeout in milliseconds, as the C# `ConnectTimeoutMs`.
pub const DEFAULT_CONNECT_TIMEOUT_MS: u64 = 3000;
/// The most hosts one test probes, as the C# `MaxHosts`.
pub const MAX_HOSTS: usize = 50;
/// The most ports one test probes, as the C# `MaxPorts`.
pub const MAX_PORTS: usize = 50;
/// Probes run at the same time, as the C# `MaxConcurrentDirect`.
pub const MAX_CONCURRENT: usize = 20;
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
    /// The host's name did not resolve: nothing was probed, so nothing is said of the
    /// firewall.
    Unresolved,
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
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FwProbeSummary {
    pub open: usize,
    pub closed: usize,
    pub timeout: usize,
    pub unresolved: usize,
    pub total: usize,
}

/// Why a test's inputs were refused, as the C# `ParseAndValidateInputs` says it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FwInputError {
    /// No host typed.
    NoHosts,
    /// Hosts typed, none of them an address or a DNS name.
    InvalidHost,
    /// No valid port typed.
    NoPorts,
}

/// A test's inputs, within the C# limits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FwInputs {
    /// The hosts probed, [`MAX_HOSTS`] at most.
    pub hosts: Vec<String>,
    /// The ports probed, [`MAX_PORTS`] at most.
    pub ports: Vec<u16>,
    /// Hosts left out: invalid, or past [`MAX_HOSTS`].
    pub hosts_left_out: usize,
    /// Ports left out past [`MAX_PORTS`].
    pub ports_left_out: usize,
}

/// Parses host input into unique, trimmed host strings: one per line as the C#
/// `FirewallProbeEngine.ParseHosts`, and also separated by commas, semicolons or spaces on a
/// line, which no host contains.
#[must_use]
pub fn parse_hosts(input: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for host in input.split(|c: char| c == ',' || c == ';' || c.is_whitespace()) {
        if !host.is_empty() && seen.insert(host.to_owned()) {
            out.push(host.to_owned());
        }
    }

    out
}

/// Parses comma-separated ports and ranges into an ordered, deduplicated list.
#[must_use]
pub fn parse_ports(input: &str) -> Vec<u16> {
    super::cert_inspector::parse_ports(input)
}

/// The hosts and ports of a test, as the C# `ParseAndValidateInputs`: invalid hosts left
/// out, then the first [`MAX_HOSTS`] hosts and [`MAX_PORTS`] ports kept.
///
/// # Errors
///
/// [`FwInputError`] when no host, no valid host or no port remains.
pub fn validate_inputs(hosts_text: &str, ports_text: &str) -> Result<FwInputs, FwInputError> {
    let typed = parse_hosts(hosts_text);
    if typed.is_empty() {
        return Err(FwInputError::NoHosts);
    }
    let typed_count = typed.len();
    let mut hosts: Vec<String> = typed.iter().filter_map(|host| checked_host(host)).collect();
    if hosts.is_empty() {
        return Err(FwInputError::InvalidHost);
    }
    let mut ports = parse_ports(ports_text);
    if ports.is_empty() {
        return Err(FwInputError::NoPorts);
    }
    hosts.truncate(MAX_HOSTS);
    let ports_left_out = ports.len().saturating_sub(MAX_PORTS);
    ports.truncate(MAX_PORTS);
    Ok(FwInputs {
        hosts_left_out: typed_count - hosts.len(),
        hosts,
        ports,
        ports_left_out,
    })
}

/// Milliseconds elapsed since `start`.
fn elapsed_ms(start: Instant) -> u64 {
    u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX)
}

/// Probes a single host and port using a TCP handshake, as the C# `ProbeDirectAsync`: open
/// when it connects, closed when refused or unreachable, a timeout when nothing answers in
/// `timeout_ms`, unresolved when the name does not resolve. Every address of the host is
/// tried within the one timeout, as .NET's `ConnectAsync(host, port)` does.
#[must_use]
pub fn probe_host_port(host: &str, port: u16, timeout_ms: u64) -> FwProbeResult {
    let result = |status, latency_ms| FwProbeResult {
        host: host.to_owned(),
        port,
        status,
        latency_ms,
    };
    let addresses: Vec<SocketAddr> = match (host, port).to_socket_addrs() {
        Ok(addresses) => addresses.collect(),
        Err(_) => return result(FwProbeStatus::Unresolved, None),
    };
    if addresses.is_empty() {
        return result(FwProbeStatus::Unresolved, None);
    }

    let timeout = Duration::from_millis(timeout_ms);
    let start = Instant::now();
    let mut status = FwProbeStatus::Timeout;
    for address in &addresses {
        let Some(left) = timeout
            .checked_sub(start.elapsed())
            .filter(|left| !left.is_zero())
        else {
            break;
        };
        match TcpStream::connect_timeout(address, left) {
            Ok(_) => return result(FwProbeStatus::Open, Some(elapsed_ms(start))),
            Err(error) if error.kind() == std::io::ErrorKind::TimedOut => {
                status = FwProbeStatus::Timeout;
            }
            // Refused, unreachable: a socket error the C# says closed.
            Err(_) => status = FwProbeStatus::Closed,
        }
    }
    result(
        status,
        Some(elapsed_ms(start)).filter(|_| status != FwProbeStatus::Timeout),
    )
}

/// Probes every port of every host, [`MAX_CONCURRENT`] at a time, as the C#
/// `TestMatrixAsync`, handing each result to `on_result` as it comes; once `stop` is set no
/// probe starts.
pub fn run_matrix(
    hosts: &[String],
    ports: &[u16],
    timeout_ms: u64,
    stop: &AtomicBool,
    on_result: impl FnMut(FwProbeResult),
) {
    let jobs: Vec<(&str, u16)> = hosts
        .iter()
        .flat_map(|host| ports.iter().map(move |port| (host.as_str(), *port)))
        .collect();
    net_command::for_each_bounded(
        jobs,
        MAX_CONCURRENT,
        stop,
        |(host, port)| probe_host_port(host, port, timeout_ms),
        on_result,
    );
}

/// Aggregates probe results into open/closed/timeout counts.
#[must_use]
pub fn compute_summary(results: &[FwProbeResult]) -> FwProbeSummary {
    let mut summary = FwProbeSummary {
        total: results.len(),
        ..FwProbeSummary::default()
    };

    for r in results {
        match r.status {
            FwProbeStatus::Open => summary.open += 1,
            FwProbeStatus::Closed => summary.closed += 1,
            FwProbeStatus::Timeout => summary.timeout += 1,
            FwProbeStatus::Unresolved => summary.unresolved += 1,
        }
    }

    summary
}

/// Builds CSV content formatted as a matrix, as the C# `BuildMatrixCsv`: one row per host,
/// one column per port under `host_header`, each cell said by `cell`, a probe that did not
/// run said `not_run`; every cell sanitized against formulas and quoted.
#[must_use]
pub fn build_matrix_csv(
    results: &[FwProbeResult],
    hosts: &[String],
    ports: &[u16],
    host_header: &str,
    cell: impl Fn(&FwProbeResult) -> String,
    not_run: &str,
) -> String {
    let map: HashMap<(&str, u16), &FwProbeResult> = results
        .iter()
        .map(|r| ((r.host.as_str(), r.port), r))
        .collect();

    let mut out = quoted(host_header);
    for &p in ports {
        out.push_str(&format!(",{p}"));
    }
    out.push('\n');

    for h in hosts {
        out.push_str(&quoted(h));
        for &p in ports {
            out.push(',');
            let text = map
                .get(&(h.as_str(), p))
                .map_or_else(|| not_run.to_owned(), |r| cell(r));
            out.push_str(&quoted(&text));
        }
        out.push('\n');
    }

    out
}

#[cfg(test)]
mod tests {
    use std::net::TcpListener;
    use std::sync::atomic::{AtomicBool, Ordering};

    use super::*;

    #[test]
    fn parse_hosts_deduplication() {
        let input = "192.168.1.1\n  192.168.1.2  \n\n192.168.1.1\n";
        let parsed = parse_hosts(input);
        assert_eq!(parsed, vec!["192.168.1.1", "192.168.1.2"]);
        assert_eq!(
            parse_hosts("a.lab, b.lab;c.lab  a.lab"),
            ["a.lab", "b.lab", "c.lab"],
            "one line holds several hosts"
        );
        assert_eq!(
            parse_hosts(
                "a.lab

  b.lab	
c.lab, d.lab
"
            ),
            ["a.lab", "b.lab", "c.lab", "d.lab"],
            "the lines of the box, Windows line ends and blank lines included"
        );
    }

    #[test]
    fn inputs_keep_the_csharp_limits_and_leave_invalid_hosts_out() {
        let hosts: Vec<String> = (0..60).map(|n| format!("10.0.0.{n}")).collect();
        let inputs = validate_inputs(&format!("-x {}", hosts.join(" ")), "1-80").expect("valid");
        assert_eq!(inputs.hosts.len(), MAX_HOSTS);
        assert_eq!(inputs.hosts[0], "10.0.0.0", "the option is left out");
        assert_eq!(inputs.hosts_left_out, 11);
        assert_eq!(inputs.ports, (1..=50).collect::<Vec<u16>>());
        assert_eq!(inputs.ports_left_out, 30);
        assert_eq!(validate_inputs(" ", "80"), Err(FwInputError::NoHosts));
        assert_eq!(
            validate_inputs("-n --x", "80"),
            Err(FwInputError::InvalidHost)
        );
        assert_eq!(validate_inputs("a.lab", "0, x"), Err(FwInputError::NoPorts));
    }

    #[test]
    fn a_name_that_does_not_resolve_is_unresolved_not_filtered() {
        let result = probe_host_port("no-such-host.invalid", 80, 500);
        assert_eq!(result.status, FwProbeStatus::Unresolved);
        assert_eq!(result.latency_ms, None);
    }

    #[test]
    fn an_open_port_is_open() {
        // A refused port is checked by the firewall tester's view tests, which hold it
        // reserved with a socket bound but not listening.
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let open = listener.local_addr().expect("addr").port();
        let probed = probe_host_port("127.0.0.1", open, 5000);
        assert_eq!(probed.status, FwProbeStatus::Open);
        assert!(probed.latency_ms.is_some());
    }

    #[test]
    fn the_matrix_probes_every_pair_and_a_stop_starts_no_more() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let open = listener.local_addr().expect("addr").port();
        let hosts = vec!["127.0.0.1".to_owned(), "localhost".to_owned()];
        let stop = AtomicBool::new(false);
        let mut results = Vec::new();
        run_matrix(&hosts, &[open, open], 2000, &stop, |r| results.push(r));
        assert_eq!(results.len(), 4);
        let stop = AtomicBool::new(true);
        let mut none = Vec::new();
        run_matrix(&hosts, &[open], 2000, &stop, |r| none.push(r));
        assert!(none.is_empty());
        stop.store(false, Ordering::SeqCst);
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
            FwProbeResult {
                host: "c".to_owned(),
                port: 80,
                status: FwProbeStatus::Unresolved,
                latency_ms: None,
            },
        ];

        let summary = compute_summary(&results);
        assert_eq!(summary.open, 1);
        assert_eq!(summary.closed, 1);
        assert_eq!(summary.timeout, 1);
        assert_eq!(summary.unresolved, 1);
        assert_eq!(summary.total, 4);
    }

    #[test]
    fn matrix_csv_formatting() {
        let hosts = vec!["host1".to_owned(), "=evil".to_owned()];
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

        let csv = build_matrix_csv(
            &results,
            &hosts,
            &ports,
            "Host",
            |r| match r.latency_ms {
                Some(ms) => format!("{:?} ({ms}ms)", r.status),
                None => format!("{:?}", r.status),
            },
            "-",
        );
        assert!(csv.contains("\"Host\",80,443\n"));
        assert!(csv.contains("\"host1\",\"Open (5ms)\",\"Closed\""));
        assert!(csv.contains("\"'=evil\",\"'-\",\"'-\""), "{csv}");
    }
}
