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

//! The TCP ping tool's settings and summary, as the C# `TcpPingViewModel` and
//! `TcpPingSummary`: how many probes, how long each may take, the pause between them, and
//! the minimum, average and maximum of the answers. The probes themselves are
//! `heimdall_app::network_tools`'.

use std::time::Duration;

/// Default port, the C# `TcpPingViewModel.DefaultPort`.
pub const DEFAULT_PORT: u16 = 443;

/// Default number of probes, the C# `DefaultCount`.
pub const DEFAULT_COUNT: usize = 10;

/// Fewest probes, the C# `MinCount`.
pub const MIN_COUNT: usize = 1;

/// Most probes, the C# `MaxCount`.
pub const MAX_COUNT: usize = 10_000;

/// Time a probe has to connect, the C# `ConnectTimeoutMs`.
pub const CONNECT_TIMEOUT: Duration = Duration::from_millis(5000);

/// Pause between two probes, the C# `DelayBetweenPingsMs`.
pub const DELAY_BETWEEN_PROBES: Duration = Duration::from_millis(1000);

/// The number of probes `typed` asks for, from [`MIN_COUNT`] to [`MAX_COUNT`].
#[must_use]
pub fn parse_count(typed: &str) -> Option<usize> {
    let typed = typed.trim();
    if typed.is_empty() || !typed.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    typed
        .parse::<usize>()
        .ok()
        .filter(|count| (MIN_COUNT..=MAX_COUNT).contains(count))
}

/// Latency statistics over the probes of a run.
#[derive(Debug, Clone, PartialEq)]
pub struct TcpPingSummary {
    /// Shortest answer, in milliseconds; 0 when none answered.
    pub min_ms: f64,
    /// Average answer, in milliseconds; 0 when none answered.
    pub avg_ms: f64,
    /// Longest answer, in milliseconds; 0 when none answered.
    pub max_ms: f64,
    /// Probes that got no answer.
    pub lost: usize,
    /// Probes sent.
    pub total: usize,
}

impl TcpPingSummary {
    /// The statistics of `latencies`, one per probe: its time in milliseconds, or `None` for a
    /// probe lost. `None` for no probe at all.
    #[must_use]
    pub fn from_latencies(latencies: &[Option<f64>]) -> Option<Self> {
        if latencies.is_empty() {
            return None;
        }
        let answered: Vec<f64> = latencies.iter().filter_map(|latency| *latency).collect();
        let lost = latencies.len() - answered.len();
        let (min_ms, avg_ms, max_ms) = if answered.is_empty() {
            (0.0, 0.0, 0.0)
        } else {
            let min = answered.iter().copied().fold(f64::MAX, f64::min);
            let max = answered.iter().copied().fold(0.0, f64::max);
            let avg = answered.iter().sum::<f64>() / answered.len() as f64;
            (min, avg, max)
        };
        Some(Self {
            min_ms,
            avg_ms,
            max_ms,
            lost,
            total: latencies.len(),
        })
    }

    /// Whether no probe got an answer: the C# then shows no time.
    #[must_use]
    pub fn all_lost(&self) -> bool {
        self.lost == self.total
    }

    /// The share of probes lost, from 0 to 100.
    #[must_use]
    pub fn loss_percentage(&self) -> f64 {
        if self.total == 0 {
            0.0
        } else {
            (self.lost as f64 / self.total as f64) * 100.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_summary_counts_answers_and_losses() {
        let summary = TcpPingSummary::from_latencies(&[Some(10.0), Some(20.0), Some(30.0), None])
            .expect("summary exists");
        assert_eq!(summary.total, 4);
        assert_eq!(summary.lost, 1);
        assert_eq!(summary.min_ms, 10.0);
        assert_eq!(summary.max_ms, 30.0);
        assert_eq!(summary.avg_ms, 20.0);
        assert!(!summary.all_lost());
        assert!((summary.loss_percentage() - 25.0).abs() < f64::EPSILON);
    }

    #[test]
    fn a_run_without_answers_has_no_time() {
        let summary = TcpPingSummary::from_latencies(&[None, None]).expect("summary exists");
        assert!(summary.all_lost());
        assert_eq!(summary.avg_ms, 0.0);
        assert_eq!(summary.loss_percentage(), 100.0);
        assert_eq!(TcpPingSummary::from_latencies(&[]), None);
    }

    #[test]
    fn the_count_is_checked_as_the_csharp() {
        assert_eq!(parse_count(" 10 "), Some(10));
        assert_eq!(parse_count("10000"), Some(MAX_COUNT));
        for bad in ["", "0", "10001", "-1", "+5", "ten"] {
            assert_eq!(parse_count(bad), None, "{bad}");
        }
    }
}
