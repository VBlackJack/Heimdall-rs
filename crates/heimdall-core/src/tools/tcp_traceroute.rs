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
//! Windows (`tracert`) and Linux (`traceroute`), and clipboard/CSV export formatting.

use std::fmt;

/// Default maximum number of hops.
pub const DEFAULT_MAX_HOPS: usize = 30;
/// Minimum allowed maximum hops.
pub const MIN_MAX_HOPS: usize = 1;
/// Maximum allowed maximum hops.
pub const MAX_MAX_HOPS: usize = 128;
/// Default timeout per probe in milliseconds.
pub const DEFAULT_PING_TIMEOUT_MS: u64 = 3000;

/// Status of a trace hop.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HopStatus {
    Reply,
    Timeout,
    Destination,
}

impl fmt::Display for HopStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Reply => write!(f, "Reply"),
            Self::Timeout => write!(f, "Timeout"),
            Self::Destination => write!(f, "Destination"),
        }
    }
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

/// Validates raw traceroute inputs and normalizes max_hops to a safe range.
pub fn validate_inputs(host_text: &str, max_hops_text: &str) -> Result<TraceInputs, &'static str> {
    let host = host_text.trim();
    if host.is_empty() {
        return Err("Host is required");
    }

    let max_hops = if let Ok(parsed) = max_hops_text.trim().parse::<usize>() {
        if (MIN_MAX_HOPS..=MAX_MAX_HOPS).contains(&parsed) {
            parsed
        } else {
            DEFAULT_MAX_HOPS
        }
    } else {
        DEFAULT_MAX_HOPS
    };

    Ok(TraceInputs {
        host: host.to_owned(),
        max_hops,
    })
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
                hostname = clean.to_owned();
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

/// Builds plain text of the entire traceroute path.
#[must_use]
pub fn build_export_text(hops: &[TraceHopResult], target: &str) -> String {
    let mut out = String::new();
    out.push_str(&format!("Traceroute to {}\n\n", target));
    out.push_str("Hop | Address         | Hostname             | Latency    | Status\n");
    out.push_str("----+-----------------+----------------------+------------+--------\n");

    for h in hops {
        out.push_str(&format!(
            "{:<3} | {:<15} | {:<20} | {:<10} | {}\n",
            h.hop, h.address, h.hostname, h.latency, h.status
        ));
    }

    out
}

/// Builds CSV content for the traceroute path.
#[must_use]
pub fn build_export_csv(hops: &[TraceHopResult]) -> String {
    let mut out = String::new();
    out.push_str("Hop,Address,Hostname,Latency,Status\n");

    for h in hops {
        let safe_host = h.hostname.replace('"', "\"\"");
        out.push_str(&format!(
            "{},{},\"{}\",\"{}\",{}\n",
            h.hop, h.address, safe_host, h.latency, h.status
        ));
    }

    out
}

/// Executes a traceroute command against `host` up to `max_hops` using the system traceroute utility.
pub fn run_traceroute_command(host: &str, max_hops: usize) -> Vec<TraceHopResult> {
    use std::process::Command;

    let mut cmd = if cfg!(windows) {
        let mut c = Command::new("tracert");
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            c.creation_flags(CREATE_NO_WINDOW);
        }
        c.arg("-d");
        c.arg("-h").arg(max_hops.to_string());
        c.arg("-w").arg("1000");
        c.arg(host);
        c
    } else {
        let mut c = Command::new("traceroute");
        c.arg("-n");
        c.arg("-m").arg(max_hops.to_string());
        c.arg("-w").arg("1");
        c.arg(host);
        c
    };

    let Ok(output) = cmd.output() else {
        return Vec::new();
    };

    let text = String::from_utf8_lossy(&output.stdout);
    let mut hops = Vec::new();
    for line in text.lines() {
        if let Some(hop) = parse_traceroute_line(line) {
            hops.push(hop);
        }
    }
    hops
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

        assert!(validate_inputs("", "10").is_err());
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

        let csv = build_export_csv(&hops);
        assert!(csv.contains("Hop,Address,Hostname,Latency,Status"));
        assert!(csv.contains("1,10.0.0.1,\"router\",\"1 ms\",Reply"));
    }
}
