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

//! DNS batch resolver engine, as the C# `DnsBatchResolverModels`: hostname list parsing,
//! status classification, summary metrics, and CSV export. The lookups themselves, at most
//! [`MAX_CONCURRENT_LOOKUPS`] at once and stoppable, run in `heimdall_app::net_tools`.

use std::fmt::Write as _;
use std::net::IpAddr;

use super::csv_cell::quoted;

/// Hostnames resolved in one batch at most: a longer list is refused, not cut.
pub const MAX_BATCH_HOSTS: usize = 1000;
/// Lookups under way at once.
pub const MAX_CONCURRENT_LOOKUPS: usize = 16;
/// How long one lookup may take, as the C# `DnsBatchResolverViewModel.ResolveTimeout`.
pub const RESOLVE_TIMEOUT_MS: u64 = 30000;

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
    /// Why resolution failed, if it did.
    pub error: Option<DnsBatchFailure>,
}

/// Why a hostname of a batch did not resolve.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DnsBatchFailure {
    /// The resolver answered with no address.
    NoAddress,
    /// No answer within [`RESOLVE_TIMEOUT_MS`].
    TimedOut,
    /// The resolver failed: why, made safe to show.
    Failed(String),
}

impl DnsBatchFailure {
    /// The failure's code in an export.
    #[must_use]
    pub fn code(&self) -> &str {
        match self {
            Self::NoAddress => "NoAddress",
            Self::TimedOut => "TimedOut",
            Self::Failed(why) => why,
        }
    }
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

/// Parses and deduplicates a raw multiline/comma/whitespace string into distinct hostnames,
/// told apart without regard to case as the C# `Distinct(OrdinalIgnoreCase)`. A pasted URL
/// keeps its host, a `host:port` its host; an IPv6 address stays whole, bare or in brackets.
#[must_use]
pub fn parse_hostnames(input: &str) -> Vec<String> {
    let mut hostnames: Vec<String> = Vec::new();
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
        let host_clean = host_of(host_only);

        if !host_clean.is_empty()
            && !hostnames
                .iter()
                .any(|known| known.eq_ignore_ascii_case(host_clean))
        {
            hostnames.push(host_clean.to_string());
        }
    }
    hostnames
}

/// The host of `authority`: an address as it is, `[v6]` or `[v6]:port` without brackets
/// nor port, `name:port` without its port.
fn host_of(authority: &str) -> &str {
    if authority.parse::<IpAddr>().is_ok() {
        return authority;
    }
    if let Some(rest) = authority.strip_prefix('[') {
        return rest.split(']').next().unwrap_or(rest);
    }
    match authority.split_once(':') {
        Some((host, port)) if !port.contains(':') => host.trim(),
        _ => authority.trim(),
    }
}

/// The result of `hostname` resolved to `addresses` in `elapsed_ms`: its IPv4 and IPv6
/// addresses, each once, in order; failed when there is none.
#[must_use]
pub fn item_from_addresses(
    hostname: &str,
    addresses: impl IntoIterator<Item = IpAddr>,
    elapsed_ms: u64,
) -> DnsBatchItemResult {
    let mut ipv4 = Vec::new();
    let mut ipv6 = Vec::new();
    for address in addresses {
        let list = if address.is_ipv4() {
            &mut ipv4
        } else {
            &mut ipv6
        };
        let shown = address.to_string();
        if !list.contains(&shown) {
            list.push(shown);
        }
    }
    let resolved = !(ipv4.is_empty() && ipv6.is_empty());
    DnsBatchItemResult {
        hostname: hostname.to_owned(),
        ipv4,
        ipv6,
        status: if resolved {
            DnsBatchItemStatus::Resolved
        } else {
            DnsBatchItemStatus::Failed
        },
        elapsed_ms,
        error: (!resolved).then_some(DnsBatchFailure::NoAddress),
    }
}

/// The result of `hostname` whose resolution came to `failure` after `elapsed_ms`.
#[must_use]
pub fn failed_item(
    hostname: &str,
    failure: DnsBatchFailure,
    elapsed_ms: u64,
) -> DnsBatchItemResult {
    DnsBatchItemResult {
        hostname: hostname.to_owned(),
        ipv4: Vec::new(),
        ipv6: Vec::new(),
        status: DnsBatchItemStatus::Failed,
        elapsed_ms,
        error: Some(failure),
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

/// Generates a CSV text export from the batch resolution results, every text cell quoted
/// and a pasted hostname a spreadsheet would read as a formula neutralized, as the C#
/// `SanitizeCsvCell`.
#[must_use]
pub fn format_batch_csv(results: &[DnsBatchItemResult]) -> String {
    let mut csv = String::from("Hostname,Status,IPv4,IPv6,ElapsedMs,Error\n");
    for r in results {
        let _ = writeln!(
            csv,
            "{},{},{},{},{},{}",
            quoted(&r.hostname),
            r.status.label(),
            quoted(&r.ipv4.join("; ")),
            quoted(&r.ipv6.join("; ")),
            r.elapsed_ms,
            quoted(r.error.as_ref().map_or("", DnsBatchFailure::code))
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
    fn an_ipv6_address_stays_whole() {
        let parsed =
            parse_hostnames("2001:db8::1\n[2001:db8::2]:443\nhttp://[::1]:8080/x\nHost.lan:22");
        assert_eq!(parsed, ["2001:db8::1", "2001:db8::2", "::1", "Host.lan"]);
    }

    #[test]
    fn hostnames_are_told_apart_without_regard_to_case() {
        assert_eq!(
            parse_hostnames("Example.com\nexample.COM\nexample.org"),
            ["Example.com", "example.org"]
        );
    }

    #[test]
    fn the_addresses_found_make_the_item_and_none_fails_it() {
        let item = item_from_addresses(
            "dual.example",
            [
                "192.0.2.1".parse().unwrap(),
                "2001:db8::1".parse().unwrap(),
                "192.0.2.1".parse().unwrap(),
            ],
            7,
        );
        assert_eq!(item.status, DnsBatchItemStatus::Resolved);
        assert_eq!(item.ipv4, ["192.0.2.1"]);
        assert_eq!(item.ipv6, ["2001:db8::1"]);
        let none = item_from_addresses("empty.example", [], 3);
        assert_eq!(none.status, DnsBatchItemStatus::Failed);
        assert_eq!(none.error, Some(DnsBatchFailure::NoAddress));
    }

    #[test]
    fn a_pasted_formula_is_neutralized_in_the_csv() {
        let csv = format_batch_csv(&[failed_item(
            "=HYPERLINK(\"http://x\")",
            DnsBatchFailure::TimedOut,
            30000,
        )]);
        assert!(
            csv.contains("\"'=HYPERLINK(\"\"http://x\"\")\",Failed,\"\",\"\",30000,\"TimedOut\""),
            "{csv}"
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
                error: Some(DnsBatchFailure::Failed("Lookup failed".into())),
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
        assert!(csv.contains("\"test.local\",Resolved,\"10.0.0.1\",\"::1\",5,\"\""));
    }
}
