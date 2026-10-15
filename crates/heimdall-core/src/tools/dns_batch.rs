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

//! DNS batch resolver engine: multiple hostname input parsing, concurrent
//! address resolution, status classification, summary metrics, and CSV export.

use std::fmt::Write as _;
use std::net::ToSocketAddrs;
use std::time::Instant;

/// Status outcome of resolving a single hostname in a batch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DnsBatchItemStatus {
    /// Host resolved at least one IPv4 or IPv6 address.
    Resolved,
    /// Host failed resolution (NXDOMAIN, timeout, unreachable).
    Failed,
    /// Pending resolution.
    Pending,
}

impl DnsBatchItemStatus {
    /// Display label for the status.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Resolved => "Resolved",
            Self::Failed => "Failed",
            Self::Pending => "Pending",
        }
    }
}

/// Resolved result for a single hostname in a batch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DnsBatchItemResult {
    /// The hostname queried.
    pub hostname: String,
    /// Resolved IPv4 addresses.
    pub ipv4: Vec<String>,
    /// Resolved IPv6 addresses.
    pub ipv6: Vec<String>,
    /// Status outcome.
    pub status: DnsBatchItemStatus,
    /// Resolution duration in milliseconds.
    pub elapsed_ms: u64,
    /// Error message, if resolution failed.
    pub error: Option<String>,
}

impl DnsBatchItemResult {
    /// Formatted comma-separated IPv4 list.
    #[must_use]
    pub fn ipv4_text(&self) -> String {
        self.ipv4.join(", ")
    }

    /// Formatted comma-separated IPv6 list.
    #[must_use]
    pub fn ipv6_text(&self) -> String {
        self.ipv6.join(", ")
    }
}

/// Summary counts across all batch resolution items.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DnsBatchSummary {
    /// Total hostnames in the batch.
    pub total: usize,
    /// Successfully resolved count.
    pub resolved: usize,
    /// Failed resolution count.
    pub failed: usize,
}

/// Parses and deduplicates a raw multiline/comma/whitespace string into distinct hostnames.
#[must_use]
pub fn parse_hostnames(input: &str) -> Vec<String> {
    let mut hostnames = Vec::new();
    for token in
        input.split(|c: char| c == '\n' || c == '\r' || c == ',' || c == ';' || c.is_whitespace())
    {
        let trimmed = token.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Strip leading scheme if user pasted full URLs e.g. https://example.com/path
        let without_scheme = if let Some(pos) = trimmed.find("://") {
            &trimmed[pos + 3..]
        } else {
            trimmed
        };

        // Strip path / query
        let host_only = without_scheme.split('/').next().unwrap_or(without_scheme);
        let host_clean = host_only.split(':').next().unwrap_or(host_only).trim();

        if !host_clean.is_empty() && !hostnames.contains(&host_clean.to_string()) {
            hostnames.push(host_clean.to_string());
        }
    }
    hostnames
}

/// Resolves a single hostname synchronously into its IPv4 and IPv6 addresses.
#[must_use]
pub fn resolve_hostname_sync(hostname: &str) -> DnsBatchItemResult {
    let host = hostname.trim();
    if host.is_empty() {
        return DnsBatchItemResult {
            hostname: String::new(),
            ipv4: Vec::new(),
            ipv6: Vec::new(),
            status: DnsBatchItemStatus::Failed,
            elapsed_ms: 0,
            error: Some("Empty hostname".to_string()),
        };
    }

    let start = Instant::now();
    let query = format!("{host}:80");

    match query.to_socket_addrs() {
        Ok(addrs) => {
            let elapsed_ms = start.elapsed().as_millis() as u64;
            let mut ipv4 = Vec::new();
            let mut ipv6 = Vec::new();

            for socket_addr in addrs {
                let ip = socket_addr.ip();
                if ip.is_ipv4() {
                    let s = ip.to_string();
                    if !ipv4.contains(&s) {
                        ipv4.push(s);
                    }
                } else if ip.is_ipv6() {
                    let s = ip.to_string();
                    if !ipv6.contains(&s) {
                        ipv6.push(s);
                    }
                }
            }

            if ipv4.is_empty() && ipv6.is_empty() {
                DnsBatchItemResult {
                    hostname: host.to_string(),
                    ipv4,
                    ipv6,
                    status: DnsBatchItemStatus::Failed,
                    elapsed_ms,
                    error: Some("No addresses found".to_string()),
                }
            } else {
                DnsBatchItemResult {
                    hostname: host.to_string(),
                    ipv4,
                    ipv6,
                    status: DnsBatchItemStatus::Resolved,
                    elapsed_ms,
                    error: None,
                }
            }
        }
        Err(err) => {
            let elapsed_ms = start.elapsed().as_millis() as u64;
            DnsBatchItemResult {
                hostname: host.to_string(),
                ipv4: Vec::new(),
                ipv6: Vec::new(),
                status: DnsBatchItemStatus::Failed,
                elapsed_ms,
                error: Some(err.to_string()),
            }
        }
    }
}

/// Computes summary metrics from a slice of batch item results.
#[must_use]
pub fn compute_summary(results: &[DnsBatchItemResult]) -> DnsBatchSummary {
    let mut summary = DnsBatchSummary {
        total: results.len(),
        resolved: 0,
        failed: 0,
    };

    for r in results {
        match r.status {
            DnsBatchItemStatus::Resolved => summary.resolved += 1,
            DnsBatchItemStatus::Failed => summary.failed += 1,
            DnsBatchItemStatus::Pending => {}
        }
    }

    summary
}

/// Generates a CSV text export from the batch resolution results.
#[must_use]
pub fn format_batch_csv(results: &[DnsBatchItemResult]) -> String {
    let mut csv = String::from("Hostname,Status,IPv4,IPv6,ElapsedMs,Error\n");
    for r in results {
        let _ = writeln!(
            csv,
            "{},{},\"{}\",\"{}\",{},\"{}\"",
            r.hostname,
            r.status.label(),
            r.ipv4.join("; "),
            r.ipv6.join("; "),
            r.elapsed_ms,
            r.error.as_deref().unwrap_or("")
        );
    }
    csv
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_hostnames_various_formats() {
        let input =
            "google.com, cloudflare.com\nhttps://github.com/path\r\n1.1.1.1:443  google.com";
        let parsed = parse_hostnames(input);
        assert_eq!(
            parsed,
            vec!["google.com", "cloudflare.com", "github.com", "1.1.1.1"]
        );
    }

    #[test]
    fn test_parse_hostnames_empty() {
        assert!(parse_hostnames("   \n\r\t  ").is_empty());
    }

    #[test]
    fn test_compute_summary() {
        let results = vec![
            DnsBatchItemResult {
                hostname: "a.com".into(),
                ipv4: vec!["1.2.3.4".into()],
                ipv6: Vec::new(),
                status: DnsBatchItemStatus::Resolved,
                elapsed_ms: 10,
                error: None,
            },
            DnsBatchItemResult {
                hostname: "bad.invalid".into(),
                ipv4: Vec::new(),
                ipv6: Vec::new(),
                status: DnsBatchItemStatus::Failed,
                elapsed_ms: 15,
                error: Some("Lookup failed".into()),
            },
        ];

        let summary = compute_summary(&results);
        assert_eq!(summary.total, 2);
        assert_eq!(summary.resolved, 1);
        assert_eq!(summary.failed, 1);
    }

    #[test]
    fn test_format_batch_csv() {
        let results = vec![DnsBatchItemResult {
            hostname: "test.local".into(),
            ipv4: vec!["10.0.0.1".into()],
            ipv6: vec!["::1".into()],
            status: DnsBatchItemStatus::Resolved,
            elapsed_ms: 5,
            error: None,
        }];
        let csv = format_batch_csv(&results);
        assert!(csv.contains("Hostname,Status,IPv4,IPv6,ElapsedMs,Error"));
        assert!(csv.contains("test.local,Resolved,\"10.0.0.1\",\"::1\",5,\"\""));
    }
}
