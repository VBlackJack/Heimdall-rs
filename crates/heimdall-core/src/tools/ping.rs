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

//! Continuous ping monitor engine: models, response parsing, validation, running
//! statistics calculation, live graph scaling, and CSV export.

use std::fmt::Write as _;
use std::process::Command;
use std::time::Instant;

/// Maximum number of data points retained in the live latency graph.
pub const MAX_DATA_POINTS: usize = 60;
/// Default ping timeout in milliseconds.
pub const DEFAULT_PING_TIMEOUT_MS: u64 = 2000;
/// Default ping count (0 = continuous).
pub const DEFAULT_PING_COUNT: usize = 0;
/// Minimum allowed timeout in milliseconds.
pub const MIN_TIMEOUT_MS: u64 = 100;
/// Maximum allowed timeout in milliseconds.
pub const MAX_TIMEOUT_MS: u64 = 30000;
/// Maximum allowed probe count limit.
pub const MAX_COUNT: usize = 100000;

/// Status outcome of a single ICMP ping probe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PingStatus {
    /// Echo reply received successfully.
    Success,
    /// Probe request timed out.
    Timeout,
    /// Probe encountered an error (network unreachable, host not found, etc.).
    Error,
}

impl PingStatus {
    /// Display label for the status.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Success => "Success",
            Self::Timeout => "Timeout",
            Self::Error => "Error",
        }
    }
}

/// Validated input parameters for a ping probe session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PingInputs {
    /// Target host name or IP address.
    pub host: String,
    /// Timeout in milliseconds per probe.
    pub timeout_ms: u64,
    /// Number of probes to issue (0 = continuous until cancelled).
    pub count: usize,
}

/// Result of a single ICMP ping probe attempt.
#[derive(Debug, Clone, PartialEq)]
pub struct PingProbeResult {
    /// Sequence counter (1-based).
    pub seq: usize,
    /// Wall-clock timestamp (HH:mm:ss format).
    pub timestamp: String,
    /// Round-trip latency in milliseconds (-1.0 if lost/timed out).
    pub latency_ms: f64,
    /// Status outcome.
    pub status: PingStatus,
    /// Time-to-live reported in the reply (0 if unavailable).
    pub ttl: u32,
    /// Target host name or resolved IP address.
    pub ip_or_host: String,
    /// Status detail or error message.
    pub detail: String,
}

impl PingProbeResult {
    /// Creates a successful probe result.
    #[must_use]
    pub fn success(
        seq: usize,
        timestamp: String,
        latency_ms: f64,
        ttl: u32,
        ip_or_host: String,
    ) -> Self {
        Self {
            seq,
            timestamp,
            latency_ms,
            status: PingStatus::Success,
            ttl,
            ip_or_host,
            detail: "OK".to_string(),
        }
    }

    /// Creates a timed-out probe result.
    #[must_use]
    pub fn timeout(seq: usize, timestamp: String, host: String) -> Self {
        Self {
            seq,
            timestamp,
            latency_ms: -1.0,
            status: PingStatus::Timeout,
            ttl: 0,
            ip_or_host: host,
            detail: "Request timed out".to_string(),
        }
    }

    /// Creates an erroneous probe result.
    #[must_use]
    pub fn error(
        seq: usize,
        timestamp: String,
        host: String,
        error_message: impl Into<String>,
    ) -> Self {
        Self {
            seq,
            timestamp,
            latency_ms: -1.0,
            status: PingStatus::Error,
            ttl: 0,
            ip_or_host: host,
            detail: error_message.into(),
        }
    }
}

/// Aggregated running statistics across a collection of ping probes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PingStatsSnapshot {
    /// Minimum round-trip time in milliseconds (0.0 if no replies).
    pub min_ms: f64,
    /// Maximum round-trip time in milliseconds.
    pub max_ms: f64,
    /// Arithmetic mean round-trip time in milliseconds.
    pub avg_ms: f64,
    /// Latency jitter (standard deviation) in milliseconds.
    pub jitter_ms: f64,
    /// Packet loss percentage (0.0 to 100.0).
    pub loss_percent: f64,
    /// Total probes sent.
    pub sent: usize,
    /// Successful probes received.
    pub received: usize,
    /// Probes lost or failed.
    pub lost: usize,
}

impl Default for PingStatsSnapshot {
    fn default() -> Self {
        Self {
            min_ms: 0.0,
            max_ms: 0.0,
            avg_ms: 0.0,
            jitter_ms: 0.0,
            loss_percent: 0.0,
            sent: 0,
            received: 0,
            lost: 0,
        }
    }
}

/// A point on the live latency graph.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PingDataPoint {
    /// Sequence counter.
    pub seq: usize,
    /// Latency in milliseconds (0.0 if timed out).
    pub latency_ms: f64,
    /// Whether this point represents a timeout/loss.
    pub is_timeout: bool,
}

/// Validates raw textual inputs for ping parameters.
///
/// Returns `Ok(PingInputs)` on success, or an error translation key on failure.
pub fn validate_inputs(
    host_text: &str,
    timeout_text: &str,
    count_text: &str,
) -> Result<PingInputs, &'static str> {
    let host = host_text.trim();
    if host.is_empty() {
        return Err("ui-tool-ping-error-host-required");
    }

    let timeout_trimmed = timeout_text.trim();
    let timeout_ms = if timeout_trimmed.is_empty() {
        DEFAULT_PING_TIMEOUT_MS
    } else {
        match timeout_trimmed.parse::<u64>() {
            Ok(val) if (MIN_TIMEOUT_MS..=MAX_TIMEOUT_MS).contains(&val) => val,
            _ => return Err("ui-tool-ping-error-timeout-invalid"),
        }
    };

    let count_trimmed = count_text.trim();
    let count = if count_trimmed.is_empty() {
        DEFAULT_PING_COUNT
    } else {
        match count_trimmed.parse::<usize>() {
            Ok(val) if val <= MAX_COUNT => val,
            _ => return Err("ui-tool-ping-error-count-invalid"),
        }
    };

    Ok(PingInputs {
        host: host.to_string(),
        timeout_ms,
        count,
    })
}

/// Parses the round-trip latency in milliseconds from standard ping command output.
#[must_use]
pub fn parse_ping_latency(output: &str) -> Option<f64> {
    if output.is_empty() {
        return None;
    }

    // Windows: "time=24ms", "time<1ms"
    // Linux/macOS: "time=24.3 ms", "time=0.456 ms"
    let lower = output.to_ascii_lowercase();
    if let Some(pos) = lower.find("time") {
        let after = &lower[pos + 4..];
        let trimmed = after.trim_start();
        let stripped = trimmed
            .strip_prefix('=')
            .or_else(|| trimmed.strip_prefix('<'))?;
        let after_op = stripped.trim_start();
        let ms_pos = after_op.find("ms")?;
        let num_str = after_op[..ms_pos].trim();
        if let Ok(val) = num_str.parse::<f64>() {
            return Some(val);
        }
    }

    None
}

/// Parses the TTL (Time-To-Live) value from standard ping command output.
#[must_use]
pub fn parse_ping_ttl(output: &str) -> Option<u32> {
    let lower = output.to_ascii_lowercase();
    if let Some(pos) = lower.find("ttl=") {
        let after = &lower[pos + 4..];
        let end = after
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(after.len());
        if let Ok(val) = after[..end].parse::<u32>() {
            return Some(val);
        }
    }
    None
}

/// Parses the resolved IP address from standard ping command output.
#[must_use]
pub fn parse_ping_address(output: &str) -> Option<String> {
    // Windows: Reply from 142.250.74.206: bytes=32...
    // Unix: 64 bytes from 142.250.74.206: icmp_seq=1...
    let lower = output.to_ascii_lowercase();
    if let Some(pos) = lower.find("from ") {
        let after = &output[pos + 5..];
        let end = after
            .find(|c: char| c == ':' || c == ' ' || c == '(' || c == ')')
            .unwrap_or(after.len());
        let addr = after[..end].trim();
        if !addr.is_empty() {
            return Some(addr.to_string());
        }
    }
    None
}

/// Computes summary statistics from a slice of probe results.
#[must_use]
#[allow(clippy::cast_precision_loss)]
pub fn compute_stats(results: &[PingProbeResult]) -> PingStatsSnapshot {
    if results.is_empty() {
        return PingStatsSnapshot::default();
    }

    let mut min = f64::MAX;
    let mut max = 0.0f64;
    let mut total = 0.0f64;
    let mut sum_squared = 0.0f64;
    let mut received = 0usize;
    let mut lost = 0usize;

    for r in results {
        if r.status == PingStatus::Success && r.latency_ms >= 0.0 {
            received += 1;
            total += r.latency_ms;
            sum_squared += r.latency_ms * r.latency_ms;
            if r.latency_ms < min {
                min = r.latency_ms;
            }
            if r.latency_ms > max {
                max = r.latency_ms;
            }
        } else {
            lost += 1;
        }
    }

    if received == 0 {
        min = 0.0;
    }

    let sent = results.len();
    let avg = if received > 0 {
        total / (received as f64)
    } else {
        0.0
    };
    let mean = avg;
    let variance = if received > 0 {
        (sum_squared / (received as f64)) - (mean * mean)
    } else {
        0.0
    };
    let jitter = variance.max(0.0).sqrt();
    let loss_percent = if sent > 0 {
        (lost as f64 * 100.0) / (sent as f64)
    } else {
        0.0
    };

    PingStatsSnapshot {
        min_ms: min,
        max_ms: max,
        avg_ms: avg,
        jitter_ms: jitter,
        loss_percent,
        sent,
        received,
        lost,
    }
}

/// Computes the recommended Y-axis upper limit and midpoint for graph rendering.
#[must_use]
#[allow(clippy::cast_precision_loss)]
pub fn compute_graph_y_scale(data_points: &[PingDataPoint]) -> (f64, f64) {
    let mut max_y = 10.0f64;
    for pt in data_points {
        if !pt.is_timeout && pt.latency_ms > max_y {
            max_y = pt.latency_ms;
        }
    }

    let ceiling = (max_y * 1.25).max(10.0);
    (ceiling, ceiling / 2.0)
}

/// Builds CSV formatted export text from probe history.
#[must_use]
pub fn build_csv_export(results: &[PingProbeResult]) -> String {
    let mut csv = String::from("Seq,Timestamp,Host,Status,LatencyMs,TTL,Detail\n");
    for r in results {
        let lat_str = if r.latency_ms >= 0.0 {
            format!("{:.1}", r.latency_ms)
        } else {
            String::new()
        };
        let _ = writeln!(
            csv,
            "{},{},{},{},{},{},{}",
            r.seq,
            r.timestamp,
            r.ip_or_host,
            r.status.label(),
            lat_str,
            r.ttl,
            r.detail.replace(',', " ")
        );
    }
    csv
}

/// Executes a single synchronous ICMP ping probe against `host`.
#[must_use]
pub fn probe_ping(host: &str, seq: usize, timeout_ms: u64) -> PingProbeResult {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let hour = (secs / 3600) % 24;
    let min = (secs / 60) % 60;
    let sec = secs % 60;
    let timestamp = format!("{hour:02}:{min:02}:{sec:02}");

    let mut cmd = Command::new("ping");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
        cmd.arg("-n").arg("1");
        cmd.arg("-w").arg(timeout_ms.to_string());
        cmd.arg(host);
    }
    #[cfg(not(windows))]
    {
        let timeout_secs = ((timeout_ms + 999) / 1000).max(1).to_string();
        cmd.arg("-c").arg("1");
        cmd.arg("-W").arg(timeout_secs);
        cmd.arg(host);
    }

    let start = Instant::now();
    match cmd.output() {
        Ok(output) => {
            let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            let combined = format!("{stdout} {stderr}");

            if output.status.success() {
                let latency = parse_ping_latency(&combined).unwrap_or(elapsed_ms);
                let ttl = parse_ping_ttl(&combined).unwrap_or(0);
                let addr = parse_ping_address(&combined).unwrap_or_else(|| host.to_string());
                PingProbeResult::success(seq, timestamp, latency, ttl, addr)
            } else if combined.to_ascii_lowercase().contains("timed out")
                || combined.to_ascii_lowercase().contains("100% packet loss")
            {
                PingProbeResult::timeout(seq, timestamp, host.to_string())
            } else {
                let err_msg = if stderr.trim().is_empty() {
                    stdout.trim().to_string()
                } else {
                    stderr.trim().to_string()
                };
                let detail = if err_msg.is_empty() {
                    "Destination unreachable".to_string()
                } else {
                    err_msg
                };
                PingProbeResult::error(seq, timestamp, host.to_string(), detail)
            }
        }
        Err(err) => PingProbeResult::error(seq, timestamp, host.to_string(), err.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_inputs_valid() {
        let res = validate_inputs("8.8.8.8", "1500", "5");
        assert!(res.is_ok());
        let inputs = res.unwrap();
        assert_eq!(inputs.host, "8.8.8.8");
        assert_eq!(inputs.timeout_ms, 1500);
        assert_eq!(inputs.count, 5);
    }

    #[test]
    fn test_validate_inputs_empty_defaults() {
        let res = validate_inputs("google.com", "", "");
        assert!(res.is_ok());
        let inputs = res.unwrap();
        assert_eq!(inputs.host, "google.com");
        assert_eq!(inputs.timeout_ms, DEFAULT_PING_TIMEOUT_MS);
        assert_eq!(inputs.count, DEFAULT_PING_COUNT);
    }

    #[test]
    fn test_validate_inputs_empty_host_fails() {
        let res = validate_inputs("   ", "1000", "1");
        assert!(res.is_err());
        assert_eq!(res.unwrap_err(), "ui-tool-ping-error-host-required");
    }

    #[test]
    fn test_validate_inputs_bad_timeout_fails() {
        assert!(validate_inputs("example.com", "50", "1").is_err());
        assert!(validate_inputs("example.com", "999999", "1").is_err());
    }

    #[test]
    fn test_parse_ping_latency_windows() {
        let out = "Reply from 1.1.1.1: bytes=32 time=14ms TTL=57";
        assert_eq!(parse_ping_latency(out), Some(14.0));

        let out_less = "Reply from 127.0.0.1: bytes=32 time<1ms TTL=128";
        assert_eq!(parse_ping_latency(out_less), Some(1.0));
    }

    #[test]
    fn test_parse_ping_latency_unix() {
        let out = "64 bytes from 1.1.1.1: icmp_seq=1 ttl=57 time=14.32 ms";
        assert_eq!(parse_ping_latency(out), Some(14.32));
    }

    #[test]
    fn test_parse_ping_ttl() {
        let out = "Reply from 1.1.1.1: bytes=32 time=14ms TTL=57";
        assert_eq!(parse_ping_ttl(out), Some(57));
    }

    #[test]
    fn test_compute_stats_mixed() {
        let results = vec![
            PingProbeResult::success(1, "12:00:00".into(), 10.0, 64, "host".into()),
            PingProbeResult::success(2, "12:00:01".into(), 20.0, 64, "host".into()),
            PingProbeResult::timeout(3, "12:00:02".into(), "host".into()),
            PingProbeResult::success(4, "12:00:03".into(), 30.0, 64, "host".into()),
        ];

        let stats = compute_stats(&results);
        assert_eq!(stats.sent, 4);
        assert_eq!(stats.received, 3);
        assert_eq!(stats.lost, 1);
        assert_eq!(stats.min_ms, 10.0);
        assert_eq!(stats.max_ms, 30.0);
        assert_eq!(stats.avg_ms, 20.0);
        assert_eq!(stats.loss_percent, 25.0);
        assert!(stats.jitter_ms > 0.0);
    }

    #[test]
    fn test_compute_graph_y_scale() {
        let points = vec![
            PingDataPoint {
                seq: 1,
                latency_ms: 40.0,
                is_timeout: false,
            },
            PingDataPoint {
                seq: 2,
                latency_ms: 100.0,
                is_timeout: false,
            },
        ];
        let (max_y, mid_y) = compute_graph_y_scale(&points);
        assert_eq!(max_y, 125.0);
        assert_eq!(mid_y, 62.5);
    }

    #[test]
    fn test_build_csv_export() {
        let results = vec![
            PingProbeResult::success(1, "10:00:00".into(), 12.5, 64, "1.1.1.1".into()),
            PingProbeResult::timeout(2, "10:00:01".into(), "1.1.1.1".into()),
        ];
        let csv = build_csv_export(&results);
        assert!(csv.contains("Seq,Timestamp,Host,Status,LatencyMs,TTL,Detail"));
        assert!(csv.contains("1,10:00:00,1.1.1.1,Success,12.5,64,OK"));
        assert!(csv.contains("2,10:00:01,1.1.1.1,Timeout,,0,Request timed out"));
    }
}
