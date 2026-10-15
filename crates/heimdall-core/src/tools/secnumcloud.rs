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

//! The SecNumCloud v3.2 Compliance Audit engine, mirroring the C#
//! `Heimdall.Core.Security.SecNumCloudAuditEngine`, `AuditModels`,
//! `CsvEvidenceExporter`, and `HtmlReportGenerator`:
//! provides audit domain models, four SecNumCloud chapters (Network,
//! Cryptography, Access Control, Operations) covering 15 verifiable controls,
//! banner parsing and embedded CVE matcher, RFC 4180 CSV evidence exporter,
//! and standalone HTML compliance report generator.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::doc_markdown,
    clippy::fn_params_excessive_bools,
    clippy::module_name_repetitions,
    clippy::redundant_closure_for_method_calls,
    clippy::too_many_lines
)]

use serde::{Deserialize, Serialize};

/// Controls the breadth and duration of the audit scan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum AuditDepth {
    /// Essential checks only (~2 min).
    Quick,
    /// Full audit across all standard controls (~5 min).
    #[default]
    Standard,
    /// Extended enumeration and brute-force credential checks (~15 min).
    Deep,
}

impl AuditDepth {
    /// All depth options for UI dropdowns.
    pub const ALL: [Self; 3] = [Self::Quick, Self::Standard, Self::Deep];

    /// Returns the stable identifier for serialization.
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::Quick => "quick",
            Self::Standard => "standard",
            Self::Deep => "deep",
        }
    }
}

/// Outcome status of a single audit check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum AuditStatus {
    /// The control passed without issues.
    Pass,
    /// Warning or informational finding that requires review.
    Warning,
    /// The control failed; non-compliant with SecNumCloud requirements.
    Fail,
    /// The check could not execute due to an operational error.
    Error,
    /// The check was skipped (no relevant services or targets discovered).
    #[default]
    Skipped,
}

impl AuditStatus {
    /// Returns the uppercase code used in machine-readable reports.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Pass => "PASS",
            Self::Warning => "WARN",
            Self::Fail => "FAIL",
            Self::Error => "ERROR",
            Self::Skipped => "SKIP",
        }
    }
}

/// Defines what the audit targets: an explicit host list, a CIDR subnet,
/// and/or an optional gateway to tunnel through.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AuditScope {
    /// Explicit target hostnames or IP addresses.
    pub targets: Vec<String>,
    /// Optional CIDR subnet for automatic network discovery (e.g. "10.0.1.0/24").
    pub subnet: Option<String>,
    /// Optional gateway identifier to tunnel the audit through.
    pub gateway_id: Option<String>,
}

/// Toggles for individual audit chapters and overall depth.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditOptions {
    /// Scan depth.
    pub depth: AuditDepth,
    /// Chapter 9: Network Security.
    pub check_network: bool,
    /// Chapter 10: Cryptographic Controls.
    pub check_crypto: bool,
    /// Chapter 11: Access Management.
    pub check_access: bool,
    /// Chapter 12: Operations Security.
    pub check_operations: bool,
}

impl Default for AuditOptions {
    fn default() -> Self {
        Self {
            depth: AuditDepth::Standard,
            check_network: true,
            check_crypto: true,
            check_access: true,
            check_operations: true,
        }
    }
}

/// A single piece of evidence collected during a check, tied to a specific host or domain.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditEvidence {
    /// Target host IP, domain name, or aggregate scope.
    pub host: String,
    /// Human-readable detail of the finding or observation.
    pub detail: String,
    /// Raw protocol banners, headers, or probe output.
    pub raw_data: String,
}

/// A single verifiable audit control mapped to an ANSSI SecNumCloud clause.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditCheck {
    /// Short check identifier (e.g. "NET-01", "CRY-02").
    pub id: String,
    /// Check title.
    pub name: String,
    /// Corresponding clause in the SecNumCloud v3.2 reference framework.
    pub secnumcloud_clause: String,
    /// Outcome status after execution.
    pub status: AuditStatus,
    /// Summary explanation of the result.
    pub summary: String,
    /// Collected evidence items.
    pub evidence: Vec<AuditEvidence>,
}

/// A chapter grouping related controls under a single SecNumCloud domain.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditChapter {
    /// Chapter code (e.g. "NET", "CRY", "ACC", "OPS").
    pub id: String,
    /// Chapter display title.
    pub name: String,
    /// Reference section in SecNumCloud v3.2.
    pub secnumcloud_ref: String,
    /// List of checks in this chapter.
    pub checks: Vec<AuditCheck>,
}

impl AuditChapter {
    /// Count of checks that passed.
    #[must_use]
    pub fn pass_count(&self) -> usize {
        self.checks
            .iter()
            .filter(|c| c.status == AuditStatus::Pass)
            .count()
    }

    /// Count of checks with warnings.
    #[must_use]
    pub fn warn_count(&self) -> usize {
        self.checks
            .iter()
            .filter(|c| c.status == AuditStatus::Warning)
            .count()
    }

    /// Count of checks that failed.
    #[must_use]
    pub fn fail_count(&self) -> usize {
        self.checks
            .iter()
            .filter(|c| c.status == AuditStatus::Fail)
            .count()
    }
}

/// Root container for a complete SecNumCloud compliance audit run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditReport {
    /// Timestamp when the audit started (ISO 8601 or formatted).
    pub start_time: String,
    /// Timestamp when the audit finished.
    pub end_time: String,
    /// Target scope.
    pub scope: AuditScope,
    /// Audited chapters and their checks.
    pub chapters: Vec<AuditChapter>,
}

impl AuditReport {
    /// Total number of checks performed across all chapters.
    #[must_use]
    pub fn total_checks(&self) -> usize {
        self.chapters.iter().map(|c| c.checks.len()).sum()
    }

    /// Total number of passed checks.
    #[must_use]
    pub fn total_pass(&self) -> usize {
        self.chapters.iter().map(AuditChapter::pass_count).sum()
    }

    /// Total number of warning checks.
    #[must_use]
    pub fn total_warn(&self) -> usize {
        self.chapters.iter().map(AuditChapter::warn_count).sum()
    }

    /// Total number of failed checks.
    #[must_use]
    pub fn total_fail(&self) -> usize {
        self.chapters.iter().map(AuditChapter::fail_count).sum()
    }

    /// Overall compliance score as an integer percentage (0..=100).
    #[must_use]
    pub fn compliance_percent(&self) -> u32 {
        let total = self.total_checks();
        if total == 0 {
            return 0;
        }
        let pass = self.total_pass();
        ((pass as f64 / total as f64) * 100.0).round() as u32
    }
}

/// Discovered service on a target host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscoveredService {
    /// Port number.
    pub port: u16,
    /// Whether the port responded as open.
    pub is_open: bool,
    /// Guessed or detected service name.
    pub service_name: Option<String>,
    /// Raw banner captured from the service.
    pub banner: Option<String>,
    /// TLS certificate subject, if detected.
    pub cert_subject: Option<String>,
    /// Certificate expiry timestamp (YYYY-MM-DD), if detected.
    pub cert_expiry: Option<String>,
    /// Whether certificate is expired.
    pub cert_expired: bool,
    /// Whether certificate expires soon (within 30 days).
    pub cert_expires_soon: bool,
    /// Certificate signature algorithm.
    pub cert_sig_algo: Option<String>,
    /// Subject Alternative Names extracted from certificate.
    pub cert_sans: Vec<String>,
}

/// Discovered host containing its probed services.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscoveredHost {
    /// IP address or hostname.
    pub ip_address: String,
    /// Open services discovered.
    pub services: Vec<DiscoveredService>,
}

// ── Constants & Knowledge Bases ──────────────────────────────────────

/// Ports considered standard/expected in a secure enterprise environment.
pub const STANDARD_PORTS: &[u16] = &[22, 80, 443, 3389];

/// Ports that typically serve TLS-encrypted traffic.
pub const TLS_PORTS: &[u16] = &[443, 8443, 636, 993, 995, 465, 990, 3269, 9443];

/// Ports that typically serve HTTP/HTTPS traffic.
pub const HTTP_PORTS: &[u16] = &[80, 443, 8080, 8443, 9443];

/// Substrings matching known weak or broken cipher suites.
pub const WEAK_CIPHER_PATTERNS: &[&str] = &["RC4", "3DES", "DES_CBC", "NULL", "EXPORT", "anon"];

/// Default SSH credential pairs to test during Access Control checks.
pub const DEFAULT_SSH_CREDENTIALS: &[(&str, &str)] = &[
    ("root", "root"),
    ("admin", "admin"),
    ("admin", "password"),
    ("root", "toor"),
    ("pi", "raspberry"),
];

/// Default SNMP community strings to test.
pub const DEFAULT_SNMP_COMMUNITIES: &[&str] = &["public", "private", "community"];

/// HTTP security headers required by SecNumCloud hardening guidelines.
pub const REQUIRED_SECURITY_HEADERS: &[&str] = &[
    "Strict-Transport-Security",
    "Content-Security-Policy",
    "X-Frame-Options",
    "X-Content-Type-Options",
];

/// Known CVE entry for banner matching.
#[derive(Debug, Clone)]
pub struct CveEntry {
    /// CVE identifier (e.g. "CVE-2024-6387").
    pub id: &'static str,
    /// CVSS score.
    pub cvss: f32,
    /// Severity label (Critical, High, Medium, Low).
    pub severity: &'static str,
    /// Short summary of the vulnerability.
    pub summary: &'static str,
    /// Affected versions description.
    pub affected_versions: &'static str,
}

/// Matches service name and version against known high-severity vulnerabilities.
#[must_use]
pub fn find_cves_for_software(software: &str, version: &str) -> Vec<CveEntry> {
    let mut matches = Vec::new();

    if software.eq_ignore_ascii_case("OpenSSH") {
        if version_in_range(version, "8.5", "9.7") {
            matches.push(CveEntry {
                id: "CVE-2024-6387",
                cvss: 8.1,
                severity: "High",
                summary: "regreSSHion: Race condition in signal handler allows unauthenticated RCE",
                affected_versions: "8.5p1 - 9.7p1",
            });
        }
        if version_below(version, "9.3.2") {
            matches.push(CveEntry {
                id: "CVE-2023-38408",
                cvss: 9.8,
                severity: "Critical",
                summary: "PKCS#11 provider: RCE via forwarded ssh-agent",
                affected_versions: "< 9.3p2",
            });
        }
        if version_below(version, "9.6") {
            matches.push(CveEntry {
                id: "CVE-2023-48795",
                cvss: 5.9,
                severity: "Medium",
                summary: "Terrapin: Prefix truncation attack on SSH BPP (strict kex bypass)",
                affected_versions: "< 9.6",
            });
        }
    } else if software.eq_ignore_ascii_case("Apache") {
        if version_equals(version, "2.4.49") {
            matches.push(CveEntry {
                id: "CVE-2021-41773",
                cvss: 7.5,
                severity: "High",
                summary: "Path traversal and file disclosure in Apache HTTP Server 2.4.49",
                affected_versions: "2.4.49",
            });
        }
        if version_below(version, "2.4.56") {
            matches.push(CveEntry {
                id: "CVE-2023-25690",
                cvss: 9.8,
                severity: "Critical",
                summary: "HTTP request smuggling via mod_proxy with RewriteRule",
                affected_versions: "< 2.4.56",
            });
        }
    } else if software.eq_ignore_ascii_case("nginx") {
        if version_in_range(version, "0.6.18", "1.20.0") {
            matches.push(CveEntry {
                id: "CVE-2021-23017",
                cvss: 7.7,
                severity: "High",
                summary: "DNS resolver off-by-one heap write allows RCE",
                affected_versions: "0.6.18 - 1.20.0",
            });
        }
        if version_below(version, "1.23.2") {
            matches.push(CveEntry {
                id: "CVE-2022-41741",
                cvss: 7.8,
                severity: "High",
                summary: "Memory corruption via crafted mp4 file (ngx_http_mp4_module)",
                affected_versions: "< 1.23.2",
            });
        }
    } else if software.eq_ignore_ascii_case("MySQL") && version_below(version, "8.0.33") {
        matches.push(CveEntry {
            id: "CVE-2023-21980",
            cvss: 7.1,
            severity: "High",
            summary: "Client: buffer overflow in C API",
            affected_versions: "< 8.0.33",
        });
    } else if software.eq_ignore_ascii_case("PostgreSQL") {
        if version_below(version, "17.1") {
            matches.push(CveEntry {
                id: "CVE-2024-10979",
                cvss: 8.8,
                severity: "High",
                summary: "Arbitrary code execution via environment variable manipulation in PL/Perl",
                affected_versions: "< 17.1 / < 16.5 / < 15.9",
            });
        }
        if version_below(version, "16.1") {
            matches.push(CveEntry {
                id: "CVE-2023-5869",
                cvss: 8.8,
                severity: "High",
                summary: "Buffer overflow in integer overflow in array modification",
                affected_versions: "< 16.1",
            });
        }
    } else if software.eq_ignore_ascii_case("Microsoft IIS")
        && (version.starts_with("10.") || version == "10")
    {
        matches.push(CveEntry {
            id: "CVE-2022-21907",
            cvss: 9.8,
            severity: "Critical",
            summary: "HTTP Protocol Stack (http.sys) RCE in trailer support",
            affected_versions: "IIS 10.0 (Win Server 2022)",
        });
    }

    matches
}

// ── Banner Parsing & Version Helpers ─────────────────────────────────

/// Parses a service banner and extracts the detected software name and normalized version.
#[must_use]
pub fn parse_banner(banner: &str) -> Option<(&'static str, String)> {
    let lower = banner.to_ascii_lowercase();

    if let Some(pos) = lower.find("openssh") {
        let after = &banner[pos + 7..];
        let v = extract_version_after_delim(after);
        return Some(("OpenSSH", v));
    }

    if lower.contains("apache") || lower.contains("httpd") {
        let (pos, len) = if let Some(p) = lower.find("apache") {
            (p, 6)
        } else if let Some(p) = lower.find("httpd") {
            (p, 5)
        } else {
            (0, 0)
        };
        let after = &banner[pos + len..];
        let v = extract_version_after_delim(after);
        return Some(("Apache", v));
    }

    if let Some(pos) = lower.find("nginx") {
        let after = &banner[pos + 5..];
        let v = extract_version_after_delim(after);
        return Some(("nginx", v));
    }

    if lower.contains("microsoft-iis") || lower.contains("iis") {
        let pos = lower.find("iis").unwrap_or(0);
        let after = &banner[pos + 3..];
        let v = extract_version_after_delim(after);
        return Some(("Microsoft IIS", v));
    }

    if let Some(pos) = lower.find("mysql") {
        let after = &banner[pos + 5..];
        let v = extract_version_after_delim(after);
        return Some(("MySQL", v));
    }

    if let Some(pos) = lower.find("postgresql") {
        let after = &banner[pos + 10..];
        let v = extract_version_after_delim(after);
        return Some(("PostgreSQL", v));
    }

    None
}

fn extract_version_after_delim(s: &str) -> String {
    let trimmed = s.trim_start_matches(|c: char| c == '/' || c == '_' || c == ' ' || c == '-');
    normalize_version(trimmed)
}

/// Normalizes a version string by taking leading digits and dots, stripping suffix letters ("8.9p1" -> "8.9").
#[must_use]
pub fn normalize_version(raw: &str) -> String {
    let mut out = String::new();
    let mut last_was_dot = false;

    for c in raw.chars() {
        if c.is_ascii_digit() {
            out.push(c);
            last_was_dot = false;
        } else if c == '.' && !last_was_dot && !out.is_empty() {
            out.push('.');
            last_was_dot = true;
        } else {
            break;
        }
    }

    out.trim_end_matches('.').to_string()
}

/// Parses a version string into an array of numeric components.
#[must_use]
pub fn parse_version(v: &str) -> Vec<u32> {
    let norm = normalize_version(v);
    if norm.is_empty() {
        return Vec::new();
    }

    norm.split('.')
        .filter_map(|part| part.parse::<u32>().ok())
        .collect()
}

/// Compares two numeric version slices component by component.
#[must_use]
pub fn compare_versions(a: &[u32], b: &[u32]) -> std::cmp::Ordering {
    let max_len = a.len().max(b.len());
    for i in 0..max_len {
        let va = a.get(i).copied().unwrap_or(0);
        let vb = b.get(i).copied().unwrap_or(0);
        match va.cmp(&vb) {
            std::cmp::Ordering::Equal => continue,
            other => return other,
        }
    }
    std::cmp::Ordering::Equal
}

/// Checks if version is strictly below a threshold.
#[must_use]
pub fn version_below(version: &str, threshold: &str) -> bool {
    let v = parse_version(version);
    let t = parse_version(threshold);
    !v.is_empty() && !t.is_empty() && compare_versions(&v, &t).is_lt()
}

/// Checks if version is within an inclusive range [from, to].
#[must_use]
pub fn version_in_range(version: &str, from: &str, to: &str) -> bool {
    let v = parse_version(version);
    let f = parse_version(from);
    let t = parse_version(to);
    !v.is_empty()
        && !f.is_empty()
        && !t.is_empty()
        && compare_versions(&v, &f).is_ge()
        && compare_versions(&v, &t).is_le()
}

/// Checks if version equals a target version.
#[must_use]
pub fn version_equals(version: &str, target: &str) -> bool {
    let v = parse_version(version);
    let t = parse_version(target);
    !v.is_empty() && !t.is_empty() && compare_versions(&v, &t).is_eq()
}

// ── Chapter Builders ─────────────────────────────────────────────────

/// Builds the default definition of Chapter 9: Network Security.
#[must_use]
pub fn build_network_chapter(chapter_name: String, check_names: [String; 4]) -> AuditChapter {
    let [name1, name2, name3, name4] = check_names;
    AuditChapter {
        id: "NET".to_string(),
        name: chapter_name,
        secnumcloud_ref: "SecNumCloud 3.2 - 9.1 Network Architecture".to_string(),
        checks: vec![
            AuditCheck {
                id: "NET-01".to_string(),
                name: name1,
                secnumcloud_clause: "9.1.2".to_string(),
                status: AuditStatus::Skipped,
                summary: String::new(),
                evidence: Vec::new(),
            },
            AuditCheck {
                id: "NET-02".to_string(),
                name: name2,
                secnumcloud_clause: "9.1.3".to_string(),
                status: AuditStatus::Skipped,
                summary: String::new(),
                evidence: Vec::new(),
            },
            AuditCheck {
                id: "NET-03".to_string(),
                name: name3,
                secnumcloud_clause: "9.1.4".to_string(),
                status: AuditStatus::Skipped,
                summary: String::new(),
                evidence: Vec::new(),
            },
            AuditCheck {
                id: "NET-04".to_string(),
                name: name4,
                secnumcloud_clause: "9.1.1".to_string(),
                status: AuditStatus::Skipped,
                summary: String::new(),
                evidence: Vec::new(),
            },
        ],
    }
}

/// Builds the default definition of Chapter 10: Cryptography.
#[must_use]
pub fn build_crypto_chapter(chapter_name: String, check_names: [String; 4]) -> AuditChapter {
    let [name1, name2, name3, name4] = check_names;
    AuditChapter {
        id: "CRY".to_string(),
        name: chapter_name,
        secnumcloud_ref: "SecNumCloud 3.2 - 10.1 Cryptographic Controls".to_string(),
        checks: vec![
            AuditCheck {
                id: "CRY-01".to_string(),
                name: name1,
                secnumcloud_clause: "10.1.1".to_string(),
                status: AuditStatus::Skipped,
                summary: String::new(),
                evidence: Vec::new(),
            },
            AuditCheck {
                id: "CRY-02".to_string(),
                name: name2,
                secnumcloud_clause: "10.1.2".to_string(),
                status: AuditStatus::Skipped,
                summary: String::new(),
                evidence: Vec::new(),
            },
            AuditCheck {
                id: "CRY-03".to_string(),
                name: name3,
                secnumcloud_clause: "10.1.3".to_string(),
                status: AuditStatus::Skipped,
                summary: String::new(),
                evidence: Vec::new(),
            },
            AuditCheck {
                id: "CRY-04".to_string(),
                name: name4,
                secnumcloud_clause: "10.1.4".to_string(),
                status: AuditStatus::Skipped,
                summary: String::new(),
                evidence: Vec::new(),
            },
        ],
    }
}

/// Builds the default definition of Chapter 11: Access Control.
#[must_use]
pub fn build_access_chapter(chapter_name: String, check_names: [String; 3]) -> AuditChapter {
    let [name1, name2, name3] = check_names;
    AuditChapter {
        id: "ACC".to_string(),
        name: chapter_name,
        secnumcloud_ref: "SecNumCloud 3.2 - 9.2 Access Management".to_string(),
        checks: vec![
            AuditCheck {
                id: "ACC-01".to_string(),
                name: name1,
                secnumcloud_clause: "9.2.3".to_string(),
                status: AuditStatus::Skipped,
                summary: String::new(),
                evidence: Vec::new(),
            },
            AuditCheck {
                id: "ACC-02".to_string(),
                name: name2,
                secnumcloud_clause: "9.2.4".to_string(),
                status: AuditStatus::Skipped,
                summary: String::new(),
                evidence: Vec::new(),
            },
            AuditCheck {
                id: "ACC-03".to_string(),
                name: name3,
                secnumcloud_clause: "9.2.5".to_string(),
                status: AuditStatus::Skipped,
                summary: String::new(),
                evidence: Vec::new(),
            },
        ],
    }
}

/// Builds the default definition of Chapter 12: Operations Security.
#[must_use]
pub fn build_operations_chapter(chapter_name: String, check_names: [String; 4]) -> AuditChapter {
    let [name1, name2, name3, name4] = check_names;
    AuditChapter {
        id: "OPS".to_string(),
        name: chapter_name,
        secnumcloud_ref: "SecNumCloud 3.2 - 12.1 Operational Procedures".to_string(),
        checks: vec![
            AuditCheck {
                id: "OPS-01".to_string(),
                name: name1,
                secnumcloud_clause: "12.1.1".to_string(),
                status: AuditStatus::Skipped,
                summary: String::new(),
                evidence: Vec::new(),
            },
            AuditCheck {
                id: "OPS-02".to_string(),
                name: name2,
                secnumcloud_clause: "12.1.2".to_string(),
                status: AuditStatus::Skipped,
                summary: String::new(),
                evidence: Vec::new(),
            },
            AuditCheck {
                id: "OPS-03".to_string(),
                name: name3,
                secnumcloud_clause: "12.6.1".to_string(),
                status: AuditStatus::Skipped,
                summary: String::new(),
                evidence: Vec::new(),
            },
            AuditCheck {
                id: "OPS-04".to_string(),
                name: name4,
                secnumcloud_clause: "12.1.3".to_string(),
                status: AuditStatus::Skipped,
                summary: String::new(),
                evidence: Vec::new(),
            },
        ],
    }
}

// ── Export Helpers ───────────────────────────────────────────────────

/// Generates an RFC 4180 compliant CSV string containing all audit checks and their evidence.
#[must_use]
pub fn build_csv_export(report: &AuditReport) -> String {
    let mut out = String::with_capacity(4096);
    out.push_str("Chapter,CheckID,CheckName,Clause,Status,Host,Detail,RawData\r\n");

    for chapter in &report.chapters {
        for check in &chapter.checks {
            if check.evidence.is_empty() {
                append_csv_row(
                    &mut out,
                    &chapter.name,
                    &check.id,
                    &check.name,
                    &check.secnumcloud_clause,
                    check.status.code(),
                    "",
                    &check.summary,
                    "",
                );
            } else {
                for ev in &check.evidence {
                    append_csv_row(
                        &mut out,
                        &chapter.name,
                        &check.id,
                        &check.name,
                        &check.secnumcloud_clause,
                        check.status.code(),
                        &ev.host,
                        &ev.detail,
                        &ev.raw_data,
                    );
                }
            }
        }
    }

    out
}

#[allow(clippy::too_many_arguments)]
fn append_csv_row(
    out: &mut String,
    chapter: &str,
    check_id: &str,
    check_name: &str,
    clause: &str,
    status: &str,
    host: &str,
    detail: &str,
    raw_data: &str,
) {
    out.push_str(&super::csv_cell::quoted(chapter));
    out.push(',');
    out.push_str(&super::csv_cell::quoted(check_id));
    out.push(',');
    out.push_str(&super::csv_cell::quoted(check_name));
    out.push(',');
    out.push_str(&super::csv_cell::quoted(clause));
    out.push(',');
    out.push_str(&super::csv_cell::quoted(status));
    out.push(',');
    out.push_str(&super::csv_cell::quoted(host));
    out.push(',');
    out.push_str(&super::csv_cell::quoted(detail));
    out.push(',');
    out.push_str(&super::csv_cell::quoted(raw_data));
    out.push_str("\r\n");
}

/// Localized text for the standalone HTML report, supplied by the UI so the document is
/// written in the language the application currently shows (the `lang` attribute, the
/// title and headings, the field labels), while machine codes such as `PASS`/`WARN`
/// stay language-independent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HtmlLabels {
    /// BCP-47 language code for the document's `lang` attribute (e.g. "en", "fr", "es").
    pub lang: String,
    /// The `<title>` of the document.
    pub document_title: String,
    /// The `<h1>` heading.
    pub heading: String,
    /// Label preceding the overall compliance percentage.
    pub compliance_label: String,
    /// Label for the passing-count figure.
    pub pass_label: String,
    /// Label for the warning-count figure.
    pub warn_label: String,
    /// Label for the failing-count figure.
    pub fail_label: String,
    /// Word preceding a check's SecNumCloud clause.
    pub clause_label: String,
    /// Footer attribution line.
    pub generated_by: String,
}

/// Generates a self-contained standalone HTML report for auditor presentation.
#[must_use]
pub fn build_html_export(report: &AuditReport, labels: &HtmlLabels) -> String {
    let mut html = String::with_capacity(8192);
    html.push_str("<!DOCTYPE html>\n<html lang=\"");
    html.push_str(&escape_html(&labels.lang));
    html.push_str("\">\n<head>\n");
    html.push_str("<meta charset=\"utf-8\">\n<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n");
    html.push_str(&format!(
        "<title>{}</title>\n",
        escape_html(&labels.document_title)
    ));
    html.push_str("<style>\n");
    html.push_str(":root { --pass: #28a745; --warn: #ffc107; --fail: #dc3545; --error: #6f42c1; --skip: #6c757d; --bg: #ffffff; --card: #f8f9fa; --border: #dee2e6; }\n");
    html.push_str("body { font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; max-width: 1000px; margin: 0 auto; padding: 2rem; color: #212529; background: var(--bg); }\n");
    html.push_str(".header { border-bottom: 2px solid var(--border); padding-bottom: 1rem; margin-bottom: 2rem; }\n");
    html.push_str(".badge { display: inline-block; padding: 0.25rem 0.6rem; border-radius: 4px; font-weight: 600; font-size: 0.85rem; color: #fff; }\n");
    html.push_str(".badge-pass { background-color: var(--pass); }\n");
    html.push_str(".badge-warn { background-color: var(--warn); color: #000; }\n");
    html.push_str(".badge-fail { background-color: var(--fail); }\n");
    html.push_str(".badge-error { background-color: var(--error); }\n");
    html.push_str(".badge-skip { background-color: var(--skip); }\n");
    html.push_str(".chapter { margin-bottom: 2rem; border: 1px solid var(--border); border-radius: 6px; overflow: hidden; }\n");
    html.push_str(".chapter-header { background: var(--card); padding: 0.8rem 1.2rem; font-weight: 600; display: flex; justify-content: space-between; border-bottom: 1px solid var(--border); }\n");
    html.push_str(".check { padding: 0.8rem 1.2rem; border-bottom: 1px solid var(--border); }\n");
    html.push_str(".check:last-child { border-bottom: none; }\n");
    html.push_str(".evidence { margin-top: 0.5rem; background: var(--card); padding: 0.5rem; border-radius: 4px; font-family: monospace; font-size: 0.85rem; }\n");
    html.push_str("</style>\n</head>\n<body>\n");

    // Header
    html.push_str("<div class=\"header\">\n");
    html.push_str(&format!("<h1>{}</h1>\n", escape_html(&labels.heading)));
    html.push_str(&format!(
        "<p><strong>{}:</strong> {}% | <strong>{}:</strong> {} | <strong>{}:</strong> {} | <strong>{}:</strong> {}</p>\n",
        escape_html(&labels.compliance_label),
        report.compliance_percent(),
        escape_html(&labels.pass_label),
        report.total_pass(),
        escape_html(&labels.warn_label),
        report.total_warn(),
        escape_html(&labels.fail_label),
        report.total_fail()
    ));
    html.push_str("</div>\n");

    // Chapters
    for ch in &report.chapters {
        html.push_str("<div class=\"chapter\">\n");
        html.push_str(&format!(
            "<div class=\"chapter-header\"><span>{} ({})</span><span>{}: {} | {}: {} | {}: {}</span></div>\n",
            escape_html(&ch.name),
            escape_html(&ch.secnumcloud_ref),
            escape_html(&labels.pass_label),
            ch.pass_count(),
            escape_html(&labels.warn_label),
            ch.warn_count(),
            escape_html(&labels.fail_label),
            ch.fail_count()
        ));

        for check in &ch.checks {
            let badge_cls = match check.status {
                AuditStatus::Pass => "badge-pass",
                AuditStatus::Warning => "badge-warn",
                AuditStatus::Fail => "badge-fail",
                AuditStatus::Error => "badge-error",
                AuditStatus::Skipped => "badge-skip",
            };

            html.push_str("<div class=\"check\">\n");
            html.push_str(&format!(
                "<div><span class=\"badge {}\">{}</span> <strong>[{}] {}</strong> ({} {})</div>\n",
                badge_cls,
                check.status.code(),
                escape_html(&check.id),
                escape_html(&check.name),
                escape_html(&labels.clause_label),
                escape_html(&check.secnumcloud_clause)
            ));
            if !check.summary.is_empty() {
                html.push_str(&format!("<p>{}</p>\n", escape_html(&check.summary)));
            }
            for ev in &check.evidence {
                html.push_str(&format!(
                    "<div class=\"evidence\"><strong>{}</strong>: {}</div>\n",
                    escape_html(&ev.host),
                    escape_html(&ev.detail)
                ));
            }
            html.push_str("</div>\n");
        }
        html.push_str("</div>\n");
    }

    html.push_str(&format!(
        "<p style=\"text-align:center;color:#6c757d;font-size:0.85rem;\">{}</p>\n",
        escape_html(&labels.generated_by)
    ));
    html.push_str("</body>\n</html>\n");
    html
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

// ── Unit Tests ───────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_banner_detects_common_software_and_versions() {
        assert_eq!(
            parse_banner("SSH-2.0-OpenSSH_9.3p1 Debian-1"),
            Some(("OpenSSH", "9.3".to_string()))
        );
        assert_eq!(
            parse_banner("Apache/2.4.49 (Unix)"),
            Some(("Apache", "2.4.49".to_string()))
        );
        assert_eq!(
            parse_banner("nginx/1.20.1"),
            Some(("nginx", "1.20.1".to_string()))
        );
        assert_eq!(
            parse_banner("Microsoft-IIS/10.0"),
            Some(("Microsoft IIS", "10.0".to_string()))
        );
    }

    #[test]
    fn version_comparison_logic_works_correctly() {
        assert!(version_below("8.9", "9.0"));
        assert!(version_below("9.3.1", "9.3.2"));
        assert!(!version_below("9.4", "9.3.2"));

        assert!(version_in_range("8.6", "8.5", "9.7"));
        assert!(version_in_range("8.5", "8.5", "9.7"));
        assert!(version_in_range("9.7", "8.5", "9.7"));
        assert!(!version_in_range("9.8", "8.5", "9.7"));

        assert!(version_equals("2.4.49", "2.4.49"));
        assert!(!version_equals("2.4.50", "2.4.49"));
    }

    #[test]
    fn find_cves_identifies_vulnerable_versions() {
        let cves = find_cves_for_software("OpenSSH", "8.9");
        assert!(!cves.is_empty());
        assert!(cves.iter().any(|c| c.id == "CVE-2024-6387"));
        assert!(cves.iter().any(|c| c.id == "CVE-2023-38408"));

        let secure_openssh = find_cves_for_software("OpenSSH", "9.8");
        assert!(secure_openssh.is_empty());
    }

    #[test]
    fn compliance_calculation_reflects_pass_ratio() {
        let mut report = AuditReport {
            start_time: "2026-10-14T12:00:00Z".to_string(),
            end_time: "2026-10-14T12:02:00Z".to_string(),
            scope: AuditScope::default(),
            chapters: vec![AuditChapter {
                id: "NET".to_string(),
                name: "Network".to_string(),
                secnumcloud_ref: "9.1".to_string(),
                checks: vec![
                    AuditCheck {
                        id: "NET-01".to_string(),
                        name: "Open ports".to_string(),
                        secnumcloud_clause: "9.1.2".to_string(),
                        status: AuditStatus::Pass,
                        summary: "OK".to_string(),
                        evidence: vec![],
                    },
                    AuditCheck {
                        id: "NET-02".to_string(),
                        name: "Unnecessary services".to_string(),
                        secnumcloud_clause: "9.1.3".to_string(),
                        status: AuditStatus::Fail,
                        summary: "Non-standard port".to_string(),
                        evidence: vec![],
                    },
                ],
            }],
        };

        assert_eq!(report.total_checks(), 2);
        assert_eq!(report.total_pass(), 1);
        assert_eq!(report.total_fail(), 1);
        assert_eq!(report.compliance_percent(), 50);

        report.chapters[0].checks[1].status = AuditStatus::Pass;
        assert_eq!(report.compliance_percent(), 100);
    }

    #[test]
    fn csv_export_generates_valid_structure() {
        let report = AuditReport {
            start_time: "2026-10-14T12:00:00Z".to_string(),
            end_time: "2026-10-14T12:02:00Z".to_string(),
            scope: AuditScope::default(),
            chapters: vec![AuditChapter {
                id: "NET".to_string(),
                name: "Network Security".to_string(),
                secnumcloud_ref: "9.1".to_string(),
                checks: vec![AuditCheck {
                    id: "NET-01".to_string(),
                    name: "Open Ports".to_string(),
                    secnumcloud_clause: "9.1.2".to_string(),
                    status: AuditStatus::Pass,
                    summary: "All clean".to_string(),
                    evidence: vec![AuditEvidence {
                        host: "10.0.0.1".to_string(),
                        detail: "Port 22 open".to_string(),
                        raw_data: "OpenSSH 9.3".to_string(),
                    }],
                }],
            }],
        };

        let csv = build_csv_export(&report);
        assert!(csv.starts_with("Chapter,CheckID,CheckName,Clause,Status,Host,Detail,RawData"));
        assert!(csv.contains("\"Network Security\",\"NET-01\",\"Open Ports\",\"9.1.2\",\"PASS\",\"10.0.0.1\",\"Port 22 open\",\"OpenSSH 9.3\""));
    }

    #[test]
    fn html_export_contains_essential_elements() {
        let report = AuditReport {
            start_time: "2026-10-14T12:00:00Z".to_string(),
            end_time: "2026-10-14T12:02:00Z".to_string(),
            scope: AuditScope::default(),
            chapters: vec![AuditChapter {
                id: "NET".to_string(),
                name: "Network Security".to_string(),
                secnumcloud_ref: "9.1".to_string(),
                checks: vec![AuditCheck {
                    id: "NET-01".to_string(),
                    name: "Open Ports".to_string(),
                    secnumcloud_clause: "9.1.2".to_string(),
                    status: AuditStatus::Pass,
                    summary: "All clean".to_string(),
                    evidence: vec![],
                }],
            }],
        };

        let labels = HtmlLabels {
            lang: "fr".to_string(),
            document_title: "Rapport SecNumCloud".to_string(),
            heading: "SecNumCloud v3.2 - Rapport de conformite".to_string(),
            compliance_label: "Conformite".to_string(),
            pass_label: "Reussite".to_string(),
            warn_label: "Avertissement".to_string(),
            fail_label: "Echec".to_string(),
            clause_label: "Clause".to_string(),
            generated_by: "Genere par Heimdall".to_string(),
        };
        let html = build_html_export(&report, &labels);
        assert!(html.starts_with("<!DOCTYPE html>"));
        // The lang attribute, title and labels come from the supplied localization.
        assert!(html.contains("<html lang=\"fr\">"));
        assert!(html.contains("SecNumCloud v3.2 - Rapport de conformite"));
        assert!(html.contains("Conformite"));
        assert!(html.contains("NET-01"));
        // Status codes stay machine-readable regardless of language.
        assert!(html.contains("PASS"));
    }
}
