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

//! TCP ping probe engine and latency statistics.

use std::net::{TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

/// Status of a TCP ping probe attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TcpPingProbeStatus {
    /// Handshake completed successfully within timeout.
    Success,
    /// Connection failed, timed out, or refused.
    Failed,
}

/// Request parameters for a single TCP ping probe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TcpPingProbeRequest {
    /// Target host name or IP address.
    pub host: String,
    /// Target TCP port.
    pub port: u16,
    /// Sequence counter (1-based).
    pub seq: usize,
    /// Connection timeout in milliseconds.
    pub timeout_ms: u64,
}

/// Result of a single TCP ping probe.
#[derive(Debug, Clone, PartialEq)]
pub struct TcpPingProbeResult {
    /// Sequence number of this probe.
    pub seq: usize,
    /// Target host queried.
    pub host: String,
    /// Target port queried.
    pub port: u16,
    /// Round-trip handshake latency in milliseconds.
    pub latency_ms: f64,
    /// Status outcome.
    pub status: TcpPingProbeStatus,
    /// Optional failure error message.
    pub error_message: Option<String>,
}

impl TcpPingProbeResult {
    /// Creates a successful probe result.
    #[must_use]
    pub fn ok(seq: usize, host: String, port: u16, latency_ms: f64) -> Self {
        Self {
            seq,
            host,
            port,
            latency_ms,
            status: TcpPingProbeStatus::Success,
            error_message: None,
        }
    }

    /// Creates a failed probe result.
    #[must_use]
    pub fn failed(seq: usize, host: String, port: u16, error_message: impl Into<String>) -> Self {
        Self {
            seq,
            host,
            port,
            latency_ms: -1.0,
            status: TcpPingProbeStatus::Failed,
            error_message: Some(error_message.into()),
        }
    }
}

/// Aggregated latency statistics across multiple TCP ping probes.
#[derive(Debug, Clone, PartialEq)]
pub struct TcpPingSummary {
    /// Minimum latency observed in milliseconds.
    pub min_ms: f64,
    /// Average latency observed in milliseconds.
    pub avg_ms: f64,
    /// Maximum latency observed in milliseconds.
    pub max_ms: f64,
    /// Count of failed / lost probes.
    pub lost: usize,
    /// Total probes issued.
    pub total: usize,
}

impl TcpPingSummary {
    /// Computes summary statistics from a slice of probe results.
    #[must_use]
    pub fn from_probes(probes: &[TcpPingProbeResult]) -> Option<Self> {
        if probes.is_empty() {
            return None;
        }

        let mut min = f64::MAX;
        let mut max = 0.0f64;
        let mut total_latency = 0.0f64;
        let mut success_count = 0;
        let mut lost = 0;

        for probe in probes {
            if probe.status == TcpPingProbeStatus::Success && probe.latency_ms >= 0.0 {
                success_count += 1;
                total_latency += probe.latency_ms;
                if probe.latency_ms < min {
                    min = probe.latency_ms;
                }
                if probe.latency_ms > max {
                    max = probe.latency_ms;
                }
            } else {
                lost += 1;
            }
        }

        let avg = if success_count > 0 {
            total_latency / (success_count as f64)
        } else {
            0.0
        };

        if success_count == 0 {
            min = 0.0;
        }

        Some(Self {
            min_ms: min,
            avg_ms: avg,
            max_ms: max,
            lost,
            total: probes.len(),
        })
    }

    /// Computes the loss percentage (0.0 to 100.0).
    #[must_use]
    pub fn loss_percentage(&self) -> f64 {
        if self.total == 0 {
            0.0
        } else {
            (self.lost as f64 / self.total as f64) * 100.0
        }
    }
}

/// Executes a single TCP connection handshake probe synchronously.
#[must_use]
pub fn probe_tcp(request: &TcpPingProbeRequest) -> TcpPingProbeResult {
    let start = Instant::now();
    let timeout = Duration::from_millis(request.timeout_ms);
    let Ok(mut addrs) = (request.host.as_str(), request.port).to_socket_addrs() else {
        return TcpPingProbeResult::failed(
            request.seq,
            request.host.clone(),
            request.port,
            "DNS resolution failed",
        );
    };

    let Some(addr) = addrs.next() else {
        return TcpPingProbeResult::failed(
            request.seq,
            request.host.clone(),
            request.port,
            "No address found",
        );
    };

    match TcpStream::connect_timeout(&addr, timeout) {
        Ok(_stream) => {
            let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
            TcpPingProbeResult::ok(request.seq, request.host.clone(), request.port, elapsed_ms)
        }
        Err(err) => TcpPingProbeResult::failed(
            request.seq,
            request.host.clone(),
            request.port,
            err.to_string(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_summary_calculation() {
        let probes = vec![
            TcpPingProbeResult::ok(1, "localhost".into(), 80, 10.0),
            TcpPingProbeResult::ok(2, "localhost".into(), 80, 20.0),
            TcpPingProbeResult::ok(3, "localhost".into(), 80, 30.0),
            TcpPingProbeResult::failed(4, "localhost".into(), 80, "Timeout"),
        ];

        let summary = TcpPingSummary::from_probes(&probes).expect("summary exists");
        assert_eq!(summary.total, 4);
        assert_eq!(summary.lost, 1);
        assert_eq!(summary.min_ms, 10.0);
        assert_eq!(summary.max_ms, 30.0);
        assert_eq!(summary.avg_ms, 20.0);
        assert!((summary.loss_percentage() - 25.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_summary_all_failed() {
        let probes = vec![
            TcpPingProbeResult::failed(1, "localhost".into(), 80, "Refused"),
            TcpPingProbeResult::failed(2, "localhost".into(), 80, "Timeout"),
        ];

        let summary = TcpPingSummary::from_probes(&probes).expect("summary exists");
        assert_eq!(summary.total, 2);
        assert_eq!(summary.lost, 2);
        assert_eq!(summary.avg_ms, 0.0);
        assert_eq!(summary.loss_percentage(), 100.0);
    }
}
