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

//! The DNS Security evaluation engine, as the C# `Heimdall.Core.Security.DnsSecurityEvaluationEngine`:
//! pure, stateless evaluator for the six DNS security controls:
//! - SPF (Sender Policy Framework)
//! - DKIM (DomainKeys Identified Mail)
//! - DMARC (Domain-based Message Authentication, Reporting and Conformance)
//! - CAA (Certification Authority Authorization)
//! - DNSSEC (DNS Security Extensions)
//! - MX (Mail Exchangers)
//!
//! Parses raw DNS output (`dig +short` or `nslookup` format), classifies posture,
//! calculates overall security summary score, and formats text reports.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::doc_markdown,
    clippy::fn_params_excessive_bools,
    clippy::module_name_repetitions,
    clippy::redundant_closure_for_method_calls,
    clippy::too_many_lines
)]

use std::fmt;

/// Identifies which DNS security control a [`DnsCheckResult`] represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DnsCheckKind {
    Spf,
    Dkim,
    Dmarc,
    Caa,
    Dnssec,
    Mx,
}

impl DnsCheckKind {
    /// Every check kind in canonical evaluation order.
    pub const ALL: [Self; 6] = [
        Self::Spf,
        Self::Dkim,
        Self::Dmarc,
        Self::Caa,
        Self::Dnssec,
        Self::Mx,
    ];

    /// The display name of the security check.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Spf => "SPF",
            Self::Dkim => "DKIM",
            Self::Dmarc => "DMARC",
            Self::Caa => "CAA",
            Self::Dnssec => "DNSSEC",
            Self::Mx => "MX",
        }
    }
}

impl fmt::Display for DnsCheckKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// Outcome classification of a single DNS security check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DnsCheckStatus {
    Pass,
    Warn,
    Fail,
}

/// Overall posture of a domain across the six DNS security checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DnsSummaryStatus {
    /// All checks passed (ratio >= 1.0).
    AllPass,
    /// At least two thirds of checks passed (ratio >= 0.66).
    Good,
    /// At least one third of checks passed (ratio >= 0.33).
    Partial,
    /// Fewer than one third of checks passed, or no checks evaluated.
    Bad,
}

/// Finding details for a single DNS security check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DnsCheckFinding {
    /// Record missing or failed.
    Missing,
    /// SPF uses +all (permissive).
    SpfPermissive,
    /// SPF uses ?all (neutral).
    SpfNeutral,
    /// SPF has no all mechanism.
    SpfNoAll,
    /// SPF restricts senders (pass).
    SpfGood,
    /// DMARC policy is none (monitoring only).
    DmarcNone,
    /// DMARC policy enforces quarantine or reject.
    DmarcEnforced(String),
    /// DMARC policy has pct=0 (not enforced).
    DmarcPctZero(String),
    /// DKIM found for a selector.
    DkimFound(String),
    /// DKIM record has empty/revoked public key.
    DkimRevoked(String),
    /// No DKIM record found for selectors.
    NoDkim,
    /// CAA missing.
    NoCaa,
    /// CAA present with listed issuers.
    CaaPresent(String),
    /// DNSSEC enabled.
    DnssecPresent,
    /// DNSSEC not configured.
    DnssecMissing,
    /// MX mail handlers.
    MxServers(String),
    /// No MX records.
    NoMx,
    /// Generic error message.
    Error(String),
}

/// Result of a single DNS security control evaluation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DnsCheckResult {
    pub kind: DnsCheckKind,
    pub status: DnsCheckStatus,
    pub raw_record: String,
    pub finding: DnsCheckFinding,
}

/// Aggregate DNS security report for a domain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DnsSecurityReport {
    pub domain: String,
    pub results: Vec<DnsCheckResult>,
    pub pass_count: usize,
    pub total: usize,
    pub summary: DnsSummaryStatus,
}

/// Common DKIM selectors probed when no explicit selector is known.
pub const DEFAULT_DKIM_SELECTORS: [&str; 4] = ["default", "google", "selector1", "selector2"];

/// Normalizes raw user input into a canonical lowercase domain name.
/// Strips whitespace, schemes (`http://`, `https://`), userinfo (`user@domain` -> `domain`),
/// and trailing path or port segment.
#[must_use]
pub fn normalize_domain_input(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    let mut value = trimmed.to_ascii_lowercase();

    if let Some(idx) = value.find("://") {
        value = value[idx + 3..].to_owned();
    }

    if let Some(idx) = value.rfind('@') {
        value = value[idx + 1..].to_owned();
    }

    let trimmed_end = value.trim_end_matches('/');
    value = trimmed_end.to_owned();

    if let Some(idx) = value.find('/') {
        value = value[..idx].to_owned();
    }

    if let Some(idx) = value.find('?') {
        value = value[..idx].to_owned();
    }

    if let Some(idx) = value.find(':') {
        value = value[..idx].to_owned();
    }

    value.trim().to_owned()
}

/// Extracts the first TXT record line containing the supplied marker from raw DNS output.
#[must_use]
pub fn extract_record(raw: &str, marker: &str) -> String {
    if raw.trim().is_empty() || marker.is_empty() {
        return String::new();
    }

    for raw_line in raw.lines() {
        let mut line = raw_line.trim().trim_matches('"');

        if let Some(idx) = line.to_ascii_lowercase().find("text =") {
            line = line[idx + 6..].trim().trim_matches('"');
        }

        if line
            .to_ascii_lowercase()
            .contains(&marker.to_ascii_lowercase())
        {
            return line.to_owned();
        }
    }

    String::new()
}

/// Extracts the value assigned to a single tag inside a semicolon-delimited record.
#[must_use]
pub fn extract_tag(record: &str, tag: &str) -> String {
    if record.trim().is_empty() || tag.is_empty() {
        return String::new();
    }

    for part in record.split(';') {
        let trimmed = part.trim();
        if trimmed
            .to_ascii_lowercase()
            .starts_with(&format!("{}=", tag.to_ascii_lowercase()))
        {
            return trimmed[tag.len() + 1..].trim().to_owned();
        }
    }

    String::new()
}

/// Parses CAA record directives (`issue`, `issuewild`, `iodef`) from raw DNS output.
#[must_use]
pub fn parse_caa_records(raw: &str) -> Vec<String> {
    let mut results = Vec::new();
    if raw.trim().is_empty() {
        return results;
    }

    for raw_line in raw.lines() {
        let line = raw_line.trim();
        let lower = line.to_ascii_lowercase();

        if lower.contains("issue") || lower.contains("issuewild") || lower.contains("iodef") {
            let value = line.trim().trim_matches('"').to_owned();
            if !value.is_empty() && !results.contains(&value) {
                results.push(value);
            }
        }
    }

    results
}

/// Parses MX records from raw DNS output (`dig +short` or `nslookup` format).
#[must_use]
pub fn parse_mx_records(raw: &str) -> Vec<String> {
    let mut results = Vec::new();
    if raw.trim().is_empty() {
        return results;
    }

    for raw_line in raw.lines() {
        let mut line = raw_line.trim();

        if let Some(idx) = line.to_ascii_lowercase().find("mail exchanger") {
            if let Some(eq_idx) = line[idx..].find('=') {
                line = line[idx + eq_idx + 1..].trim();
            }
        }

        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 && parts[0].parse::<u32>().is_ok() {
            let server = parts[1].trim_end_matches('.');
            if !server.is_empty() && !results.contains(&server.to_owned()) {
                results.push(server.to_owned());
            }
        } else if parts.len() == 1 && line.contains('.') && !line.contains(' ') {
            let server = parts[0].trim_end_matches('.');
            if !server.is_empty() && !results.contains(&server.to_owned()) {
                results.push(server.to_owned());
            }
        }
    }

    results
}

/// Returns `true` when raw DNS output contains at least one real record for `record_type`.
#[must_use]
pub fn contains_dns_records(raw: &str, record_type: &str) -> bool {
    if raw.trim().is_empty() || record_type.is_empty() {
        return false;
    }

    for raw_line in raw.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }

        let lower = line.to_ascii_lowercase();
        if lower.starts_with("server:")
            || lower.starts_with("address:")
            || lower.starts_with("non-authoritative")
            || line.starts_with("***")
            || line.starts_with(";;")
        {
            continue;
        }

        if lower.contains(&record_type.to_ascii_lowercase()) || line.len() > 10 {
            return true;
        }
    }

    false
}

/// Evaluates an SPF record string.
#[must_use]
pub fn evaluate_spf(raw: &str) -> DnsCheckResult {
    let spf = extract_record(raw, "v=spf1");
    if spf.is_empty() {
        return DnsCheckResult {
            kind: DnsCheckKind::Spf,
            status: DnsCheckStatus::Fail,
            raw_record: String::new(),
            finding: DnsCheckFinding::Missing,
        };
    }

    if let Some(qualifier) = get_spf_all_qualifier(&spf) {
        if qualifier == '+' || qualifier == '?' {
            let finding = if qualifier == '+' {
                DnsCheckFinding::SpfPermissive
            } else {
                DnsCheckFinding::SpfNeutral
            };

            return DnsCheckResult {
                kind: DnsCheckKind::Spf,
                status: DnsCheckStatus::Warn,
                raw_record: spf,
                finding,
            };
        }

        return DnsCheckResult {
            kind: DnsCheckKind::Spf,
            status: DnsCheckStatus::Pass,
            raw_record: spf,
            finding: DnsCheckFinding::SpfGood,
        };
    }

    if contains_spf_redirect(&spf) {
        return DnsCheckResult {
            kind: DnsCheckKind::Spf,
            status: DnsCheckStatus::Pass,
            raw_record: spf,
            finding: DnsCheckFinding::SpfGood,
        };
    }

    DnsCheckResult {
        kind: DnsCheckKind::Spf,
        status: DnsCheckStatus::Warn,
        raw_record: spf,
        finding: DnsCheckFinding::SpfNoAll,
    }
}

fn get_spf_all_qualifier(spf: &str) -> Option<char> {
    for token in spf.split_whitespace() {
        let lower = token.to_ascii_lowercase();
        if lower == "all" {
            return Some('+');
        }
        if lower.len() == 4 && lower.ends_with("all") {
            let q = lower.chars().next()?;
            if q == '-' || q == '~' || q == '?' || q == '+' {
                return Some(q);
            }
        }
    }
    None
}

fn contains_spf_redirect(spf: &str) -> bool {
    for token in spf.split_whitespace() {
        if token.to_ascii_lowercase().starts_with("redirect=") {
            return true;
        }
    }
    false
}

/// Evaluates a DKIM record for a selector.
#[must_use]
pub fn evaluate_dkim(selector: Option<&str>, raw_response: Option<&str>) -> DnsCheckResult {
    let Some(sel) = selector else {
        return DnsCheckResult {
            kind: DnsCheckKind::Dkim,
            status: DnsCheckStatus::Fail,
            raw_record: String::new(),
            finding: DnsCheckFinding::NoDkim,
        };
    };

    let Some(raw) = raw_response else {
        return DnsCheckResult {
            kind: DnsCheckKind::Dkim,
            status: DnsCheckStatus::Fail,
            raw_record: String::new(),
            finding: DnsCheckFinding::NoDkim,
        };
    };

    let dkim = extract_record(raw, "v=DKIM1");
    if dkim.is_empty() {
        return DnsCheckResult {
            kind: DnsCheckKind::Dkim,
            status: DnsCheckStatus::Fail,
            raw_record: String::new(),
            finding: DnsCheckFinding::NoDkim,
        };
    }

    let public_key = extract_tag(&dkim, "p");
    if public_key.is_empty() {
        return DnsCheckResult {
            kind: DnsCheckKind::Dkim,
            status: DnsCheckStatus::Warn,
            raw_record: format!("[{sel}] {dkim}"),
            finding: DnsCheckFinding::DkimRevoked(sel.to_owned()),
        };
    }

    DnsCheckResult {
        kind: DnsCheckKind::Dkim,
        status: DnsCheckStatus::Pass,
        raw_record: format!("[{sel}] {dkim}"),
        finding: DnsCheckFinding::DkimFound(sel.to_owned()),
    }
}

/// Evaluates a DMARC TXT record.
#[must_use]
pub fn evaluate_dmarc(raw: &str) -> DnsCheckResult {
    let dmarc = extract_record(raw, "v=DMARC1");
    if dmarc.is_empty() {
        return DnsCheckResult {
            kind: DnsCheckKind::Dmarc,
            status: DnsCheckStatus::Fail,
            raw_record: String::new(),
            finding: DnsCheckFinding::Missing,
        };
    }

    let policy = extract_tag(&dmarc, "p");
    if policy.is_empty() || policy.eq_ignore_ascii_case("none") {
        return DnsCheckResult {
            kind: DnsCheckKind::Dmarc,
            status: DnsCheckStatus::Warn,
            raw_record: dmarc,
            finding: DnsCheckFinding::DmarcNone,
        };
    }

    let pct = extract_tag(&dmarc, "pct");
    if let Ok(0) = pct.parse::<u32>() {
        return DnsCheckResult {
            kind: DnsCheckKind::Dmarc,
            status: DnsCheckStatus::Warn,
            raw_record: dmarc,
            finding: DnsCheckFinding::DmarcPctZero(policy),
        };
    }

    DnsCheckResult {
        kind: DnsCheckKind::Dmarc,
        status: DnsCheckStatus::Pass,
        raw_record: dmarc,
        finding: DnsCheckFinding::DmarcEnforced(policy),
    }
}

/// Evaluates a CAA record.
#[must_use]
pub fn evaluate_caa(raw: &str) -> DnsCheckResult {
    let records = parse_caa_records(raw);
    if records.is_empty() {
        return DnsCheckResult {
            kind: DnsCheckKind::Caa,
            status: DnsCheckStatus::Fail,
            raw_record: String::new(),
            finding: DnsCheckFinding::NoCaa,
        };
    }

    let issuers = records.join(", ");
    DnsCheckResult {
        kind: DnsCheckKind::Caa,
        status: DnsCheckStatus::Pass,
        raw_record: issuers.clone(),
        finding: DnsCheckFinding::CaaPresent(issuers),
    }
}

/// Evaluates DNSSEC presence from raw DNSKEY and fallback RRSIG responses.
#[must_use]
pub fn evaluate_dnssec(dnskey_raw: &str, rrsig_raw: &str) -> DnsCheckResult {
    if contains_dns_records(dnskey_raw, "DNSKEY") {
        return DnsCheckResult {
            kind: DnsCheckKind::Dnssec,
            status: DnsCheckStatus::Pass,
            raw_record: dnskey_raw.trim().to_owned(),
            finding: DnsCheckFinding::DnssecPresent,
        };
    }

    if contains_dns_records(rrsig_raw, "RRSIG") {
        return DnsCheckResult {
            kind: DnsCheckKind::Dnssec,
            status: DnsCheckStatus::Pass,
            raw_record: rrsig_raw.trim().to_owned(),
            finding: DnsCheckFinding::DnssecPresent,
        };
    }

    DnsCheckResult {
        kind: DnsCheckKind::Dnssec,
        status: DnsCheckStatus::Fail,
        raw_record: String::new(),
        finding: DnsCheckFinding::DnssecMissing,
    }
}

/// Evaluates an MX response.
#[must_use]
pub fn evaluate_mx(raw: &str) -> DnsCheckResult {
    let servers = parse_mx_records(raw);
    if servers.is_empty() {
        return DnsCheckResult {
            kind: DnsCheckKind::Mx,
            status: DnsCheckStatus::Fail,
            raw_record: String::new(),
            finding: DnsCheckFinding::NoMx,
        };
    }

    let server_list = servers.join(", ");
    DnsCheckResult {
        kind: DnsCheckKind::Mx,
        status: DnsCheckStatus::Pass,
        raw_record: server_list.clone(),
        finding: DnsCheckFinding::MxServers(server_list),
    }
}

/// Builds an error result for a failed query.
#[must_use]
pub fn build_error_result(kind: DnsCheckKind, error_message: &str) -> DnsCheckResult {
    DnsCheckResult {
        kind,
        status: DnsCheckStatus::Fail,
        raw_record: error_message.to_owned(),
        finding: DnsCheckFinding::Error(error_message.to_owned()),
    }
}

/// Computes the overall posture summary status.
#[must_use]
pub fn compute_summary(pass_count: usize, total: usize) -> DnsSummaryStatus {
    if total == 0 {
        return DnsSummaryStatus::Bad;
    }

    let ratio = pass_count as f64 / total as f64;
    if ratio >= 1.0 {
        DnsSummaryStatus::AllPass
    } else if ratio >= 0.66 {
        DnsSummaryStatus::Good
    } else if ratio >= 0.33 {
        DnsSummaryStatus::Partial
    } else {
        DnsSummaryStatus::Bad
    }
}

/// Builds an aggregate report from check results.
#[must_use]
pub fn build_report(domain: &str, results: Vec<DnsCheckResult>) -> DnsSecurityReport {
    let pass_count = results
        .iter()
        .filter(|r| r.status == DnsCheckStatus::Pass)
        .count();
    let total = results.len();
    let summary = compute_summary(pass_count, total);

    DnsSecurityReport {
        domain: domain.to_owned(),
        results,
        pass_count,
        total,
        summary,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_domain_variations() {
        assert_eq!(normalize_domain_input("  google.com  "), "google.com");
        assert_eq!(
            normalize_domain_input("https://example.com/path"),
            "example.com"
        );
        assert_eq!(
            normalize_domain_input("admin@sub.domain.org:8080"),
            "sub.domain.org"
        );
        assert_eq!(
            normalize_domain_input("http://USER:pass@DOMAIN.NET/"),
            "domain.net"
        );
        assert_eq!(normalize_domain_input(""), "");
    }

    #[test]
    fn extract_record_finds_txt() {
        let raw = "google.com text = \"v=spf1 include:_spf.google.com ~all\"\nguogle.com text = \"other\"";
        assert_eq!(
            extract_record(raw, "v=spf1"),
            "v=spf1 include:_spf.google.com ~all"
        );
        assert_eq!(extract_record(raw, "notfound"), "");
    }

    #[test]
    fn extract_tag_parses_values() {
        let record = "v=DMARC1; p=reject; pct=100; rua=mailto:reports@example.com";
        assert_eq!(extract_tag(record, "p"), "reject");
        assert_eq!(extract_tag(record, "pct"), "100");
        assert_eq!(extract_tag(record, "v"), "DMARC1");
        assert_eq!(extract_tag(record, "rua"), "mailto:reports@example.com");
        assert_eq!(extract_tag(record, "missing"), "");
    }

    #[test]
    fn evaluate_spf_rules() {
        // Softfail pass
        let res = evaluate_spf("v=spf1 include:_spf.google.com ~all");
        assert_eq!(res.status, DnsCheckStatus::Pass);

        // Hardfail pass
        let res = evaluate_spf("v=spf1 ip4:1.2.3.4 -all");
        assert_eq!(res.status, DnsCheckStatus::Pass);

        // Permissive warn
        let res = evaluate_spf("v=spf1 +all");
        assert_eq!(res.status, DnsCheckStatus::Warn);
        assert_eq!(res.finding, DnsCheckFinding::SpfPermissive);

        // Neutral warn
        let res = evaluate_spf("v=spf1 ?all");
        assert_eq!(res.status, DnsCheckStatus::Warn);
        assert_eq!(res.finding, DnsCheckFinding::SpfNeutral);

        // No all mechanism warn
        let res = evaluate_spf("v=spf1 ip4:1.2.3.4");
        assert_eq!(res.status, DnsCheckStatus::Warn);
        assert_eq!(res.finding, DnsCheckFinding::SpfNoAll);

        // Redirect pass
        let res = evaluate_spf("v=spf1 redirect=_spf.example.com");
        assert_eq!(res.status, DnsCheckStatus::Pass);

        // Missing fail
        let res = evaluate_spf("");
        assert_eq!(res.status, DnsCheckStatus::Fail);
    }

    #[test]
    fn evaluate_dmarc_rules() {
        // Enforced reject
        let res = evaluate_dmarc("v=DMARC1; p=reject; rua=mailto:d@example.com");
        assert_eq!(res.status, DnsCheckStatus::Pass);
        assert_eq!(
            res.finding,
            DnsCheckFinding::DmarcEnforced("reject".to_owned())
        );

        // Enforced quarantine
        let res = evaluate_dmarc("v=DMARC1; p=quarantine;");
        assert_eq!(res.status, DnsCheckStatus::Pass);

        // P=none warn
        let res = evaluate_dmarc("v=DMARC1; p=none");
        assert_eq!(res.status, DnsCheckStatus::Warn);
        assert_eq!(res.finding, DnsCheckFinding::DmarcNone);

        // Pct=0 warn
        let res = evaluate_dmarc("v=DMARC1; p=reject; pct=0");
        assert_eq!(res.status, DnsCheckStatus::Warn);
        assert_eq!(
            res.finding,
            DnsCheckFinding::DmarcPctZero("reject".to_owned())
        );

        // Missing fail
        let res = evaluate_dmarc("");
        assert_eq!(res.status, DnsCheckStatus::Fail);
    }

    #[test]
    fn evaluate_dkim_rules() {
        let valid = "v=DKIM1; k=rsa; p=MIGfMA0GCSqGSIb3DQEBAQUAA4GNADCBiQ";
        let res = evaluate_dkim(Some("default"), Some(valid));
        assert_eq!(res.status, DnsCheckStatus::Pass);

        let revoked = "v=DKIM1; p=";
        let res = evaluate_dkim(Some("default"), Some(revoked));
        assert_eq!(res.status, DnsCheckStatus::Warn);

        let missing = evaluate_dkim(None, None);
        assert_eq!(missing.status, DnsCheckStatus::Fail);
    }

    #[test]
    fn evaluate_caa_and_mx() {
        let caa_raw = "0 issue \"letsencrypt.org\"\n0 issuewild \";\"";
        let res = evaluate_caa(caa_raw);
        assert_eq!(res.status, DnsCheckStatus::Pass);

        let mx_raw = "10 mail.example.com.\n20 mail2.example.com.";
        let res = evaluate_mx(mx_raw);
        assert_eq!(res.status, DnsCheckStatus::Pass);
    }

    #[test]
    fn compute_summary_thresholds() {
        assert_eq!(compute_summary(6, 6), DnsSummaryStatus::AllPass);
        assert_eq!(compute_summary(4, 6), DnsSummaryStatus::Good);
        assert_eq!(compute_summary(2, 6), DnsSummaryStatus::Partial);
        assert_eq!(compute_summary(1, 6), DnsSummaryStatus::Bad);
        assert_eq!(compute_summary(0, 0), DnsSummaryStatus::Bad);
    }
}
