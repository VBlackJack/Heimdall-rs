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

//! Pure analysis engine for X.509 certificate inspection, matching the C#
//! `Heimdall.Core.Security.CertInspectorEngine`.
//!
//! Provides hostname matching with wildcard support, expiration evaluation, the summary of
//! the chain a server presented (read through `sealvault::cert`, the handshake made by the
//! caller), and CSV export generation.

use super::banner_grab::clean_banner;
use super::csv_cell::quoted;

/// Default warning threshold in days before certificate expiry.
pub const DEFAULT_WARNING_THRESHOLD_DAYS: i64 = 30;
/// Probes run at the same time in a scan, as the C# `MaxConcurrentProbes`.
pub const MAX_CONCURRENT_PROBES: usize = 10;
/// The longest a connection and its handshake take, as the C# `ConnectionTimeout` and
/// `PerPortTimeout`.
pub const PROBE_TIMEOUT_MS: u64 = 10_000;
/// The port inspected when none is typed: HTTPS.
pub const DEFAULT_PORT: u16 = 443;
/// The most ports one scan inspects: far above the C# extended preset's 28, far below a
/// range typed by mistake (`1-65535`), which would take hours.
pub const MAX_SCAN_PORTS: usize = 100;

/// Seconds in a day.
const SECONDS_PER_DAY: i64 = 86_400;

/// Certificate expiration status for color-coded display.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CertExpirationStatus {
    Valid,
    Warning,
    Expired,
}

/// A single element in the certificate chain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CertChainElement {
    pub subject: String,
    pub expiry: String,
}

/// Fully analyzed certificate summary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CertInspectionSummary {
    pub subject: String,
    pub issuer: String,
    pub not_before: String,
    pub not_after: String,
    pub days_remaining: i64,
    pub serial: String,
    pub thumbprint: String,
    pub sig_algorithm: String,
    pub key_size_bits: usize,
    /// The subject alternative names, DNS names then addresses; empty when there is none.
    pub sans: Vec<String>,
    pub tls_version: String,
    pub hostname_matches: bool,
    pub expiration_status: CertExpirationStatus,
    pub chain: Vec<CertChainElement>,
    pub port: u16,
}

/// Tests whether a hostname matches a pattern, supporting wildcard certificates (*.domain.com).
#[must_use]
pub fn matches_hostname(pattern: &str, host: &str) -> bool {
    if pattern.eq_ignore_ascii_case(host) {
        return true;
    }

    if let Some(suffix) = pattern.strip_prefix("*.")
        && let Some(dot_idx) = host.find('.')
        && dot_idx > 0
    {
        return suffix.eq_ignore_ascii_case(&host[dot_idx + 1..]);
    }

    false
}

/// Extracts the Common Name (CN) from an X.500 subject string.
#[must_use]
pub fn extract_cn(subject: &str) -> String {
    const PREFIX: &str = "CN=";
    // ASCII upper-casing keeps every byte where it was: the index found is the subject's.
    let upper = subject.to_ascii_uppercase();
    if let Some(start) = upper.find(PREFIX) {
        let after = &subject[start + PREFIX.len()..];
        if let Some(end) = after.find(',') {
            after[..end].trim().to_owned()
        } else {
            after.trim().to_owned()
        }
    } else {
        String::new()
    }
}

/// Checks whether the certificate matches the given hostname by examining SANs first,
/// then falling back to the CN.
#[must_use]
pub fn check_hostname_match(sans: &[String], cn: &str, host: &str) -> bool {
    for san in sans {
        if matches_hostname(san, host) {
            return true;
        }
    }

    if !cn.is_empty() && matches_hostname(cn, host) {
        return true;
    }

    false
}

/// Determines the expiration status based on days remaining.
#[must_use]
pub fn determine_expiration_status(
    days_remaining: i64,
    warning_threshold_days: i64,
) -> CertExpirationStatus {
    if days_remaining < 0 {
        CertExpirationStatus::Expired
    } else if days_remaining <= warning_threshold_days {
        CertExpirationStatus::Warning
    } else {
        CertExpirationStatus::Valid
    }
}

/// Whole days from `now` to `not_after`, both Unix seconds, rounded down: a certificate
/// that expired an hour ago has -1 days left, never 0, so it reads as expired.
#[must_use]
pub fn days_remaining(not_after: u64, now: u64) -> i64 {
    let not_after = i64::try_from(not_after).unwrap_or(i64::MAX);
    let now = i64::try_from(now).unwrap_or(i64::MAX);
    not_after.saturating_sub(now).div_euclid(SECONDS_PER_DAY)
}

/// Parses a comma-separated list of ports and ranges (e.g. "80,443,8000-8010").
/// Returns an ordered deduplicated list of valid ports.
#[must_use]
pub fn parse_ports(input: &str) -> Vec<u16> {
    let mut ports = std::collections::BTreeSet::new();

    for segment in input.split(',') {
        let seg = segment.trim();
        if seg.is_empty() {
            continue;
        }

        if let Some((start_s, end_s)) = seg.split_once('-') {
            if let (Ok(start), Ok(end)) =
                (start_s.trim().parse::<u16>(), end_s.trim().parse::<u16>())
                && start >= 1
                && start <= end
            {
                ports.extend(start..=end);
            }
        } else if let Ok(port) = seg.parse::<u16>()
            && port >= 1
        {
            ports.insert(port);
        }
    }

    ports.into_iter().collect()
}

/// Text a server wrote, made fit to show and copy: control characters out.
fn shown(raw: &str) -> String {
    clean_banner(raw).unwrap_or_default()
}

/// The summary of the chain `chain` (DER, the server's own certificate first) that the
/// server at `host`:`port` presented in a handshake of `tls_version`, at `now` (Unix
/// seconds), as the C# `BuildSummary`; `None` when its first certificate does not decode.
#[must_use]
pub fn build_summary(
    chain: &[Vec<u8>],
    tls_version: &str,
    host: &str,
    port: u16,
    now: u64,
) -> Option<CertInspectionSummary> {
    let leaf_der = chain.first()?;
    let leaf = sealvault::cert::inspect(leaf_der)?;
    let thumbprint = sealvault::hash::sha256(leaf_der)
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect();
    let sans: Vec<String> = leaf
        .alt_names
        .iter()
        .map(|name| shown(name))
        .filter(|name| !name.is_empty())
        .collect();
    let subject = shown(&leaf.subject);
    let hostname_matches = check_hostname_match(&sans, &extract_cn(&subject), host);
    let days = days_remaining(leaf.not_after_unix, now);
    // Past its last second it is expired, whatever the days rounded say.
    let expiration_status = if now > leaf.not_after_unix {
        CertExpirationStatus::Expired
    } else {
        determine_expiration_status(days, DEFAULT_WARNING_THRESHOLD_DAYS)
    };
    let chain = chain
        .iter()
        .filter_map(|der| sealvault::cert::inspect(der))
        .map(|element| CertChainElement {
            subject: shown(&element.subject),
            expiry: element.not_after,
        })
        .collect();

    Some(CertInspectionSummary {
        subject,
        issuer: shown(&leaf.issuer),
        not_before: leaf.not_before,
        not_after: leaf.not_after,
        days_remaining: days,
        serial: leaf.serial,
        thumbprint,
        sig_algorithm: leaf.signature_algorithm,
        key_size_bits: leaf.key_size_bits,
        sans,
        tls_version: tls_version.to_owned(),
        hostname_matches,
        expiration_status,
        chain,
        port,
    })
}

/// The column names of [`build_scan_csv`], in order.
pub const SCAN_CSV_COLUMNS: usize = 10;

/// Builds CSV content for a collection of scan results under the column names `header`
/// (port, subject, issuer, valid from, valid to, days remaining, TLS version, key size,
/// hostname match, SANs), the match said `yes` or `no`; every text cell sanitized against
/// formulas and quoted, as the C# `BuildScanCsv`.
#[must_use]
pub fn build_scan_csv(
    results: &[CertInspectionSummary],
    header: [&str; SCAN_CSV_COLUMNS],
    (yes, no): (&str, &str),
) -> String {
    let mut out = header.map(quoted).join(",");
    out.push('\n');

    for r in results {
        out.push_str(&format!(
            "{},{},{},{},{},{},{},{},{},{}\n",
            r.port,
            quoted(&r.subject),
            quoted(&r.issuer),
            quoted(&r.not_before),
            quoted(&r.not_after),
            r.days_remaining,
            quoted(&r.tls_version),
            r.key_size_bits,
            quoted(if r.hostname_matches { yes } else { no }),
            quoted(&r.sans.join("; "))
        ));
    }

    out
}

#[cfg(test)]
mod tests {
    use std::sync::LazyLock;

    use sealvault::cert::{AltName, Name, Profile, SigningKey, Spec, self_signed};
    use sealvault::keys::KeyPair;

    use super::*;

    /// 2026-01-02 03:04:05 UTC.
    const NOW: u64 = 1_767_323_045;

    /// One test key for every certificate: an RSA key takes a while to make.
    static KEY: LazyLock<SigningKey> = LazyLock::new(|| {
        let pair = KeyPair::rsa(2048).expect("made");
        SigningKey::rsa_sha256(pair.pkcs8_der()).expect("RSA")
    });

    /// A certificate for `cn` with `names`, valid from `not_before` to `not_after`.
    fn certificate(cn: &str, names: Vec<AltName>, not_before: u64, not_after: u64) -> Vec<u8> {
        let spec = Spec {
            subject: Name {
                common_name: cn.to_owned(),
                organization: Some("Lab".to_owned()),
                country: None,
            },
            not_before,
            not_after,
            serial: vec![0x0A, 0x0B],
            profile: Profile::TlsLeaf(names),
        };
        self_signed(&spec, &KEY).expect("signed")
    }

    #[test]
    fn hostname_wildcard_matching() {
        assert!(matches_hostname("*.example.com", "api.example.com"));
        assert!(matches_hostname("*.example.com", "sub.example.com"));
        assert!(!matches_hostname("*.example.com", "example.com"));
        assert!(!matches_hostname("*.example.com", "deep.sub.example.com"));
        assert!(!matches_hostname("*.example.com", ".example.com"));
        assert!(matches_hostname("example.com", "example.com"));
        assert!(matches_hostname("EXAMPLE.COM", "example.com"));
    }

    #[test]
    fn extract_cn_from_subject() {
        assert_eq!(extract_cn("CN=example.com, O=MyOrg, C=US"), "example.com");
        assert_eq!(extract_cn("O=MyOrg, CN=api.test.org"), "api.test.org");
        assert_eq!(extract_cn("cn=lower.org,O=x"), "lower.org");
        assert_eq!(
            extract_cn("O=Soci\u{e9}t\u{e9}, CN=caf\u{e9}.lab"),
            "caf\u{e9}.lab"
        );
        assert_eq!(extract_cn("O=MyOrg"), "");
    }

    #[test]
    fn check_hostname_match_evaluation() {
        let sans = vec!["api.example.com".to_owned(), "*.internal.net".to_owned()];
        assert!(check_hostname_match(
            &sans,
            "example.com",
            "api.example.com"
        ));
        assert!(check_hostname_match(
            &sans,
            "example.com",
            "router.internal.net"
        ));
        assert!(check_hostname_match(&sans, "example.com", "example.com"));
        assert!(!check_hostname_match(&sans, "example.com", "other.org"));
    }

    #[test]
    fn expiration_status_thresholds() {
        assert_eq!(
            determine_expiration_status(-5, 30),
            CertExpirationStatus::Expired
        );
        assert_eq!(
            determine_expiration_status(0, 30),
            CertExpirationStatus::Warning
        );
        assert_eq!(
            determine_expiration_status(15, 30),
            CertExpirationStatus::Warning
        );
        assert_eq!(
            determine_expiration_status(30, 30),
            CertExpirationStatus::Warning
        );
        assert_eq!(
            determine_expiration_status(31, 30),
            CertExpirationStatus::Valid
        );
        assert_eq!(
            determine_expiration_status(365, 30),
            CertExpirationStatus::Valid
        );
    }

    #[test]
    fn days_remaining_round_down_so_an_hour_past_is_a_day_past() {
        let day = SECONDS_PER_DAY as u64;
        assert_eq!(days_remaining(NOW + 10 * day, NOW), 10);
        assert_eq!(days_remaining(NOW + 10 * day - 1, NOW), 9);
        assert_eq!(days_remaining(NOW, NOW), 0);
        assert_eq!(days_remaining(NOW - 3600, NOW), -1);
        assert_eq!(days_remaining(NOW - 400 * day, NOW), -400);
        assert_eq!(days_remaining(u64::MAX, 0), i64::MAX / SECONDS_PER_DAY);
    }

    #[test]
    fn port_parsing_ranges_and_lists() {
        assert_eq!(parse_ports("443"), vec![443]);
        assert_eq!(parse_ports("80, 443, 8443"), vec![80, 443, 8443]);
        assert_eq!(parse_ports("8000-8003"), vec![8000, 8001, 8002, 8003]);
        assert_eq!(
            parse_ports("443, 8000-8002, 443"),
            vec![443, 8000, 8001, 8002]
        );
        assert_eq!(parse_ports("invalid, 0, 999999"), Vec::<u16>::new());
        assert_eq!(parse_ports("1-65535").len(), 65535);
    }

    #[test]
    fn a_valid_certificate_reports_its_real_dates_names_and_algorithm() {
        let day = SECONDS_PER_DAY as u64;
        let der = certificate(
            "files.lab",
            vec![
                AltName::Dns("files.lab".to_owned()),
                AltName::Dns("*.files.lab".to_owned()),
                AltName::Ip("10.0.0.5".parse().expect("IP")),
            ],
            NOW - day,
            NOW + 200 * day,
        );
        let summary =
            build_summary(&[der.clone()], "TLS 1.3", "www.files.lab", 8443, NOW).expect("read");
        assert_eq!(summary.days_remaining, 200);
        assert_eq!(summary.expiration_status, CertExpirationStatus::Valid);
        assert_eq!(summary.sans, ["files.lab", "*.files.lab", "10.0.0.5"]);
        assert!(summary.hostname_matches, "the wildcard SAN");
        assert_eq!(summary.sig_algorithm, "sha256RSA");
        assert_eq!(summary.serial, "0A0B");
        assert_eq!(summary.tls_version, "TLS 1.3");
        assert_eq!(summary.key_size_bits, 2048);
        assert_eq!(summary.port, 8443);
        assert_eq!(summary.not_before, "2026-01-01T03:04:05Z");
        assert_eq!(summary.not_after, "2026-07-21T03:04:05Z");
        assert_eq!(summary.thumbprint.len(), 64);
        assert_eq!(summary.chain.len(), 1);
        assert!(summary.subject.contains("CN=files.lab"));
        let by_address = build_summary(&[der], "TLS 1.2", "10.0.0.5", 443, NOW).expect("read");
        assert!(by_address.hostname_matches, "the IP address SAN");
    }

    #[test]
    fn an_expired_certificate_is_shown_expired_with_its_days_past() {
        let day = SECONDS_PER_DAY as u64;
        let long_ago = certificate("old.lab", Vec::new(), NOW - 400 * day, NOW - 30 * day);
        let summary = build_summary(&[long_ago], "TLS 1.2", "old.lab", 443, NOW).expect("read");
        assert_eq!(summary.days_remaining, -30);
        assert_eq!(summary.expiration_status, CertExpirationStatus::Expired);
        assert!(
            summary.hostname_matches,
            "by its common name, having no SAN"
        );
        assert!(summary.sans.is_empty());
        let an_hour_ago = certificate("old.lab", Vec::new(), NOW - 400 * day, NOW - 3600);
        let summary = build_summary(&[an_hour_ago], "TLS 1.2", "old.lab", 443, NOW).expect("read");
        assert_eq!(summary.expiration_status, CertExpirationStatus::Expired);
        let soon = certificate("old.lab", Vec::new(), NOW - day, NOW + 10 * day);
        let summary = build_summary(&[soon], "TLS 1.2", "old.lab", 443, NOW).expect("read");
        assert_eq!(summary.expiration_status, CertExpirationStatus::Warning);
    }

    #[test]
    fn the_names_of_the_certificate_decide_the_match_not_its_common_name_alone() {
        let day = SECONDS_PER_DAY as u64;
        let der = certificate(
            "files.lab",
            vec![AltName::Dns("files.lab".to_owned())],
            NOW - day,
            NOW + day * 90,
        );
        let summary = build_summary(&[der], "TLS 1.3", "mail.lab", 443, NOW).expect("read");
        assert!(
            !summary.hostname_matches,
            "neither the SAN nor the CN is mail.lab"
        );
    }

    #[test]
    fn the_whole_chain_is_listed_and_what_is_no_certificate_refused() {
        let day = SECONDS_PER_DAY as u64;
        let leaf = certificate("leaf.lab", Vec::new(), NOW - day, NOW + day);
        let issuer = certificate("issuer.lab", Vec::new(), NOW - day, NOW + 2 * day);
        let summary = build_summary(
            &[leaf, issuer, b"junk".to_vec()],
            "TLS 1.3",
            "leaf.lab",
            443,
            NOW,
        )
        .expect("read");
        let subjects: Vec<bool> = summary
            .chain
            .iter()
            .map(|element| element.subject.contains("CN=leaf.lab"))
            .collect();
        assert_eq!(subjects, [true, false], "what does not decode is left out");
        assert!(build_summary(&[], "TLS 1.3", "x", 443, NOW).is_none());
        assert!(build_summary(&[b"junk".to_vec()], "TLS 1.3", "x", 443, NOW).is_none());
    }

    #[test]
    fn scan_csv_formatting() {
        let summaries = vec![CertInspectionSummary {
            subject: "=CN=test.com".to_owned(),
            issuer: "CN=CA \"root\"".to_owned(),
            not_before: "2026-01-01".to_owned(),
            not_after: "2027-01-01".to_owned(),
            days_remaining: 300,
            serial: "1234".to_owned(),
            thumbprint: "ABCD".to_owned(),
            sig_algorithm: "SHA256".to_owned(),
            key_size_bits: 2048,
            sans: vec!["test.com".to_owned(), "www.test.com".to_owned()],
            tls_version: "TLS 1.3".to_owned(),
            hostname_matches: true,
            expiration_status: CertExpirationStatus::Valid,
            chain: vec![],
            port: 443,
        }];

        let header = ["P", "S", "I", "F", "T", "D", "V", "K", "M", "N"];
        let csv = build_scan_csv(&summaries, header, ("Yes", "No"));
        assert!(csv.starts_with("\"P\",\"S\",\"I\",\"F\",\"T\",\"D\",\"V\",\"K\",\"M\",\"N\"\n"));
        assert!(
            csv.contains(
                "443,\"'=CN=test.com\",\"CN=CA \"\"root\"\"\",\"2026-01-01\",\"2027-01-01\",300,\"TLS 1.3\",2048,\"Yes\",\"test.com; www.test.com\""
            ),
            "{csv}"
        );
    }
}
