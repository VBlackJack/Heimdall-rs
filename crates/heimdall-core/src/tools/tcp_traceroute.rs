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

//! Pure traceroute engine matching the C# `Heimdall.Core.Network.TracerouteEngine`.
//!
//! Provides input validation, hop probe aggregation, line output parsing for both
//! Windows (`tracert`) and Linux (`traceroute`), the system program run by its whole path
//! under a deadline, and CSV export formatting.

use std::sync::atomic::AtomicBool;
use std::time::Duration;

use super::banner_grab::clean_banner;
use super::csv_cell::quoted;
use super::net_host::checked_host;
use super::system_program::{self, ProgramError, RunEnd};

/// Default maximum number of hops.
pub const DEFAULT_MAX_HOPS: usize = 30;
/// Minimum allowed maximum hops.
pub const MIN_MAX_HOPS: usize = 1;
/// Maximum allowed maximum hops.
pub const MAX_MAX_HOPS: usize = 128;
/// Default timeout per probe in milliseconds.
pub const DEFAULT_PING_TIMEOUT_MS: u64 = 3000;
/// How long `tracert` and `traceroute` wait for each probe's answer, in milliseconds.
pub const PROBE_WAIT_MS: u64 = 1000;
/// Probes sent to each hop, as both programs send them by default.
pub const PROBES_PER_HOP: u64 = 3;
/// Time granted beyond the probes' own waits for the program to start, resolve the name
/// and print, in milliseconds.
pub const DEADLINE_MARGIN_MS: u64 = 10_000;
/// The Windows program, in the system folder.
pub const WINDOWS_PROGRAM: &str = "tracert.exe";
/// The Unix program, in a system folder.
pub const UNIX_PROGRAM: &str = "traceroute";

/// Status of a trace hop.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HopStatus {
    Reply,
    Timeout,
    Destination,
}

/// A single hop result in the traceroute path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceHopResult {
    pub hop: usize,
    pub address: String,
    pub hostname: String,
    pub latency: String,
    pub status: HopStatus,
}

/// Validated traceroute parameters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceInputs {
    pub host: String,
    pub max_hops: usize,
}

/// Why traceroute inputs were refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraceInputError {
    /// No host typed.
    HostRequired,
    /// The host is neither an address nor a DNS name.
    InvalidHost,
}

/// Validates raw traceroute inputs and normalizes max_hops to a safe range, as the C#
/// `ValidateInputs`: the host must be an address or a DNS name, never an option.
///
/// # Errors
///
/// [`TraceInputError`] when the host is missing or invalid.
pub fn validate_inputs(
    host_text: &str,
    max_hops_text: &str,
) -> Result<TraceInputs, TraceInputError> {
    if host_text.trim().is_empty() {
        return Err(TraceInputError::HostRequired);
    }
    let host = checked_host(host_text).ok_or(TraceInputError::InvalidHost)?;

    let max_hops = if let Ok(parsed) = max_hops_text.trim().parse::<usize>() {
        if (MIN_MAX_HOPS..=MAX_MAX_HOPS).contains(&parsed) {
            parsed
        } else {
            DEFAULT_MAX_HOPS
        }
    } else {
        DEFAULT_MAX_HOPS
    };

    Ok(TraceInputs { host, max_hops })
}

/// Formats successful latencies into "*" / "N ms" / "min/avg/max ms".
#[must_use]
pub fn format_latency(successful_ms: &[f64]) -> String {
    if successful_ms.is_empty() {
        return "*".to_owned();
    }

    if successful_ms.len() == 1 {
        return format!("{:.0} ms", successful_ms[0]);
    }

    let mut min = f64::MAX;
    let mut max = f64::MIN;
    let mut sum = 0.0;

    for &val in successful_ms {
        if val < min {
            min = val;
        }
        if val > max {
            max = val;
        }
        sum += val;
    }

    let avg = sum / (successful_ms.len() as f64);
    format!("{:.0}/{:.0}/{:.0} ms", min, avg, max)
}

/// Parses a single Linux or Windows traceroute line into a hop result.
#[must_use]
pub fn parse_traceroute_line(line: &str) -> Option<TraceHopResult> {
    let trimmed = line.trim();
    if trimmed.is_empty()
        || trimmed.starts_with("traceroute")
        || trimmed.starts_with("Tracing")
        || trimmed.starts_with("over a maximum")
        || trimmed.starts_with("Trace complete")
    {
        return None;
    }

    // Windows format:
    // "  1     1 ms     1 ms     1 ms  192.168.1.1"
    // "  2     *        *        *     Request timed out."
    let parts: Vec<&str> = trimmed.split_whitespace().collect();
    if parts.is_empty() {
        return None;
    }

    let hop: usize = parts[0].parse().ok()?;

    // Check for Windows timeout row: "1  *  *  *  Request timed out."
    if parts.len() >= 4 && parts[1] == "*" && parts[2] == "*" && parts[3] == "*" {
        return Some(TraceHopResult {
            hop,
            address: "*".to_owned(),
            hostname: String::new(),
            latency: "*".to_owned(),
            status: HopStatus::Timeout,
        });
    }

    // Windows with latencies: parts like ["1", "<1", "ms", "1", "ms", "1", "ms", "192.168.1.1"]
    // Or Linux: ["1", "gateway (192.168.1.1)", "1.123", "ms", "1.456", "ms"]
    let mut latencies = Vec::new();
    let mut address = String::new();
    let mut hostname = String::new();

    for (i, &p) in parts.iter().enumerate().skip(1) {
        if p == "ms" {
            if i > 0 {
                let num_str = parts[i - 1].trim_start_matches('<');
                if let Ok(lat) = num_str.parse::<f64>() {
                    latencies.push(lat);
                }
            }
        } else if p.contains('.') || p.contains(':') {
            let clean = p.trim_matches(|c| c == '(' || c == ')' || c == '[' || c == ']');
            if clean.parse::<std::net::IpAddr>().is_ok() {
                address = clean.to_owned();
            } else if address.is_empty() {
                hostname = clean_banner(clean).unwrap_or_default();
            }
        }
    }

    if address.is_empty() && !parts.is_empty() {
        let last = parts[parts.len() - 1].trim_matches(|c| c == '(' || c == ')');
        if last.parse::<std::net::IpAddr>().is_ok() {
            address = last.to_owned();
        }
    }

    if address.is_empty() && latencies.is_empty() {
        return None;
    }

    let latency_text = format_latency(&latencies);
    let status = if latencies.is_empty() {
        HopStatus::Timeout
    } else {
        HopStatus::Reply
    };

    Some(TraceHopResult {
        hop,
        address: if address.is_empty() {
            "*".to_owned()
        } else {
            address
        },
        hostname,
        latency: latency_text,
        status,
    })
}

/// Builds CSV content for the traceroute path under the column names `header` (hop,
/// address, hostname, latency, status), each status said by `status_label`; every text cell
/// sanitized against formulas and quoted.
#[must_use]
pub fn build_export_csv(
    hops: &[TraceHopResult],
    header: [&str; 5],
    status_label: impl Fn(HopStatus) -> String,
) -> String {
    let mut out = header.map(quoted).join(",");
    out.push('\n');

    for h in hops {
        out.push_str(&format!(
            "{},{},{},{},{}\n",
            h.hop,
            quoted(&h.address),
            quoted(&h.hostname),
            quoted(&h.latency),
            quoted(&status_label(h.status))
        ));
    }

    out
}

/// The longest a trace of `max_hops` may take: every probe of every hop waited for, and a
/// margin.
#[must_use]
pub fn trace_deadline(max_hops: usize) -> Duration {
    let hops = u64::try_from(max_hops).unwrap_or(u64::MAX);
    Duration::from_millis(
        hops.saturating_mul(PROBES_PER_HOP * PROBE_WAIT_MS)
            .saturating_add(DEADLINE_MARGIN_MS),
    )
}

/// Traces the route to `host`, already validated (see [`validate_inputs`]), up to
/// `max_hops`, with the system's `tracert` or `traceroute` run by its whole path, handing
/// each hop to `on_hop` as it is printed; ended when `stop` is set or past
/// [`trace_deadline`].
pub fn run_traceroute(
    host: &str,
    max_hops: usize,
    stop: &AtomicBool,
    mut on_hop: impl FnMut(TraceHopResult),
) -> Result<RunEnd, ProgramError> {
    let Some(program) = system_program::system_tool(WINDOWS_PROGRAM, UNIX_PROGRAM) else {
        return Err(ProgramError::NotAvailable);
    };
    let hops = max_hops.to_string();
    let wait_ms = PROBE_WAIT_MS.to_string();
    let wait_secs = (PROBE_WAIT_MS / 1000).max(1).to_string();
    // No name resolution of the hops: the C# shows addresses, and a lookup per hop would
    // outlast the deadline.
    let args: Vec<&str> = if cfg!(windows) {
        vec!["-d", "-h", &hops, "-w", &wait_ms, host]
    } else {
        vec!["-n", "-m", &hops, "-w", &wait_secs, host]
    };
    system_program::run_lines(&program, &args, trace_deadline(max_hops), stop, |line| {
        if let Some(hop) = parse_traceroute_line(line) {
            on_hop(hop);
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_inputs_normalization() {
        let res = validate_inputs("1.1.1.1", "15").unwrap();
        assert_eq!(res.host, "1.1.1.1");
        assert_eq!(res.max_hops, 15);

        let res_default = validate_inputs("example.com", "999").unwrap();
        assert_eq!(res_default.max_hops, DEFAULT_MAX_HOPS);

        assert_eq!(
            validate_inputs("", "10"),
            Err(TraceInputError::HostRequired)
        );
        assert_eq!(
            validate_inputs("-h 1 evil", "10"),
            Err(TraceInputError::InvalidHost),
            "never read as an option"
        );
        assert_eq!(
            validate_inputs("-w", "10"),
            Err(TraceInputError::InvalidHost)
        );
    }

    #[test]
    fn the_deadline_covers_every_probe_of_every_hop() {
        assert_eq!(
            trace_deadline(30),
            Duration::from_millis(30 * PROBES_PER_HOP * PROBE_WAIT_MS + DEADLINE_MARGIN_MS)
        );
        assert!(trace_deadline(usize::MAX) > trace_deadline(MAX_MAX_HOPS));
    }

    #[test]
    fn french_tracert_output_is_read_and_a_hostname_cleaned() {
        let line = "  3    12 ms    11 ms    13 ms  10.10.0.1";
        assert_eq!(
            parse_traceroute_line(line).expect("hop").latency,
            "11/12/13 ms"
        );
        let timeout =
            "  4     *        *        *     D\u{e9}lai d'attente de la demande d\u{e9}pass\u{e9}.";
        assert_eq!(
            parse_traceroute_line(timeout).expect("hop").status,
            HopStatus::Timeout
        );
        assert!(parse_traceroute_line("D\u{e9}termination de l'itin\u{e9}raire vers x").is_none());
        let named = " 2  gw.lab\u{1b}[31m.x (10.0.0.1)  1.0 ms";
        let hop = parse_traceroute_line(named).expect("hop");
        assert_eq!(hop.address, "10.0.0.1");
        assert!(!hop.hostname.contains('\u{1b}'), "{:?}", hop.hostname);
    }

    #[test]
    fn latency_formatting() {
        assert_eq!(format_latency(&[]), "*");
        assert_eq!(format_latency(&[12.0]), "12 ms");
        assert_eq!(format_latency(&[10.0, 20.0, 30.0]), "10/20/30 ms");
    }

    #[test]
    fn parse_windows_tracert_output() {
        let line = "  1     2 ms     1 ms     1 ms  192.168.1.1";
        let hop = parse_traceroute_line(line).unwrap();
        assert_eq!(hop.hop, 1);
        assert_eq!(hop.address, "192.168.1.1");
        assert_eq!(hop.status, HopStatus::Reply);

        let timeout_line = "  2     *        *        *     Request timed out.";
        let hop_timeout = parse_traceroute_line(timeout_line).unwrap();
        assert_eq!(hop_timeout.hop, 2);
        assert_eq!(hop_timeout.status, HopStatus::Timeout);
    }

    #[test]
    fn parse_linux_traceroute_output() {
        let line = " 1  gateway (192.168.1.1)  1.234 ms  1.567 ms  1.890 ms";
        let hop = parse_traceroute_line(line).unwrap();
        assert_eq!(hop.hop, 1);
        assert_eq!(hop.address, "192.168.1.1");
        assert_eq!(hop.status, HopStatus::Reply);
    }

    #[test]
    fn export_csv_contains_headers_and_data() {
        let hops = vec![TraceHopResult {
            hop: 1,
            address: "10.0.0.1".to_owned(),
            hostname: "router".to_owned(),
            latency: "1 ms".to_owned(),
            status: HopStatus::Reply,
        }];

        let csv = build_export_csv(
            &hops,
            ["Hop", "Address", "Hostname", "Latency", "Status"],
            |status| format!("{status:?}"),
        );
        assert!(csv.contains("\"Hop\",\"Address\",\"Hostname\",\"Latency\",\"Status\""));
        assert!(csv.contains("1,\"10.0.0.1\",\"router\",\"1 ms\",\"Reply\""));
    }
}
