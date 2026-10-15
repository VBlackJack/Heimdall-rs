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

//! The Offline CVE Lookup engine, as the C# `Heimdall.Core.Discovery.CveLookupEngine`:
//! provides an embedded, curated database of known high-impact vulnerabilities for common
//! infrastructure servers (OpenSSH, Apache, nginx, Tomcat, MySQL, PostgreSQL, Redis,
//! MongoDB, Elasticsearch, IIS, ProFTPD, vsftpd, Postfix, Exim, PHP, Node.js, Jenkins,
//! Plink, PuTTY).
//!
//! Features banner detection, semantic version normalization, version range checking,
//! CVSS score classification, and formatted report output. Completely offline and thread-safe.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::doc_markdown,
    clippy::fn_params_excessive_bools,
    clippy::module_name_repetitions,
    clippy::redundant_closure_for_method_calls,
    clippy::too_many_lines
)]

use regex::Regex;
use std::cmp::Ordering;
use std::sync::LazyLock;

/// Severity classification for a CVE entry based on CVSS score.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CveSeverity {
    None,
    Low,
    Medium,
    High,
    Critical,
}

impl CveSeverity {
    /// Canonical English label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::None => "None",
            Self::Low => "Low",
            Self::Medium => "Medium",
            Self::High => "High",
            Self::Critical => "Critical",
        }
    }
}

/// A matched CVE entry returned by the lookup engine.
#[derive(Debug, Clone, PartialEq)]
pub struct CveMatch {
    pub id: String,
    pub cvss_score: f64,
    pub severity: CveSeverity,
    pub summary: String,
    pub affected_versions: String,
}

/// The result of a CVE lookup search.
#[derive(Debug, Clone, PartialEq)]
pub struct CveSearchResult {
    pub resolved_query: String,
    pub matches: Vec<CveMatch>,
}

/// Internal database entry with version matching predicate.
struct CveEntry {
    id: &'static str,
    cvss_score: f64,
    severity: CveSeverity,
    summary: &'static str,
    affected_versions: &'static str,
    is_affected: fn(&str) -> bool,
}

type ProductDb = (&'static str, &'static [CveEntry]);

static CVE_DATABASE: &[ProductDb] = &[
    (
        "OpenSSH",
        &[
            CveEntry {
                id: "CVE-2024-6387",
                cvss_score: 8.1,
                severity: CveSeverity::High,
                summary: "regreSSHion: Race condition in signal handler allows unauthenticated RCE",
                affected_versions: "8.5p1 - 9.7p1",
                is_affected: |v| version_in_range(v, "8.5", "9.7"),
            },
            CveEntry {
                id: "CVE-2023-38408",
                cvss_score: 9.8,
                severity: CveSeverity::Critical,
                summary: "PKCS#11 provider: RCE via forwarded ssh-agent",
                affected_versions: "< 9.3p2",
                is_affected: |v| version_below(v, "9.3.2"),
            },
            CveEntry {
                id: "CVE-2023-48795",
                cvss_score: 5.9,
                severity: CveSeverity::Medium,
                summary: "Terrapin: Prefix truncation attack on SSH BPP (strict kex bypass)",
                affected_versions: "< 9.6",
                is_affected: |v| version_below(v, "9.6"),
            },
            CveEntry {
                id: "CVE-2021-41617",
                cvss_score: 7.0,
                severity: CveSeverity::High,
                summary: "Privilege escalation via AuthorizedKeysCommand/AuthorizedPrincipalsCommand",
                affected_versions: "6.2 - 8.7",
                is_affected: |v| version_in_range(v, "6.2", "8.7"),
            },
            CveEntry {
                id: "CVE-2020-15778",
                cvss_score: 7.8,
                severity: CveSeverity::High,
                summary: "Command injection via crafted filenames in scp",
                affected_versions: "< 8.8",
                is_affected: |v| version_below(v, "8.8"),
            },
        ],
    ),
    (
        "Apache",
        &[
            CveEntry {
                id: "CVE-2021-44228",
                cvss_score: 10.0,
                severity: CveSeverity::Critical,
                summary: "Log4Shell: RCE via JNDI lookup in log messages (affects mod_jk/Tomcat behind Apache)",
                affected_versions: "N/A (Log4j)",
                is_affected: |_| false,
            },
            CveEntry {
                id: "CVE-2021-41773",
                cvss_score: 7.5,
                severity: CveSeverity::High,
                summary: "Path traversal and file disclosure in Apache HTTP Server 2.4.49",
                affected_versions: "2.4.49",
                is_affected: |v| version_equals(v, "2.4.49"),
            },
            CveEntry {
                id: "CVE-2021-42013",
                cvss_score: 9.8,
                severity: CveSeverity::Critical,
                summary: "RCE via path traversal (incomplete fix for CVE-2021-41773)",
                affected_versions: "2.4.49 - 2.4.50",
                is_affected: |v| version_in_range(v, "2.4.49", "2.4.50"),
            },
            CveEntry {
                id: "CVE-2023-25690",
                cvss_score: 9.8,
                severity: CveSeverity::Critical,
                summary: "HTTP request smuggling via mod_proxy with RewriteRule",
                affected_versions: "< 2.4.56",
                is_affected: |v| version_below(v, "2.4.56"),
            },
            CveEntry {
                id: "CVE-2023-43622",
                cvss_score: 7.5,
                severity: CveSeverity::High,
                summary: "HTTP/2 initial window zero DoS (rapid reset variant)",
                affected_versions: "< 2.4.58",
                is_affected: |v| version_below(v, "2.4.58"),
            },
        ],
    ),
    (
        "nginx",
        &[
            CveEntry {
                id: "CVE-2021-23017",
                cvss_score: 7.7,
                severity: CveSeverity::High,
                summary: "DNS resolver off-by-one heap write allows RCE",
                affected_versions: "0.6.18 - 1.20.0",
                is_affected: |v| version_in_range(v, "0.6.18", "1.20.0"),
            },
            CveEntry {
                id: "CVE-2022-41741",
                cvss_score: 7.8,
                severity: CveSeverity::High,
                summary: "Memory corruption via crafted mp4 file (ngx_http_mp4_module)",
                affected_versions: "< 1.23.2",
                is_affected: |v| version_below(v, "1.23.2"),
            },
            CveEntry {
                id: "CVE-2022-41742",
                cvss_score: 7.1,
                severity: CveSeverity::High,
                summary: "Memory disclosure via mp4 module (same root cause as CVE-2022-41741)",
                affected_versions: "< 1.23.2",
                is_affected: |v| version_below(v, "1.23.2"),
            },
            CveEntry {
                id: "CVE-2024-7347",
                cvss_score: 4.7,
                severity: CveSeverity::Medium,
                summary: "Out-of-bounds read in ngx_http_mp4_module with crafted mp4",
                affected_versions: "< 1.27.1",
                is_affected: |v| version_below(v, "1.27.1"),
            },
        ],
    ),
    (
        "Apache Tomcat",
        &[
            CveEntry {
                id: "CVE-2024-50379",
                cvss_score: 9.8,
                severity: CveSeverity::Critical,
                summary: "RCE via partial PUT and concurrent read on case-insensitive FS",
                affected_versions: "< 11.0.2 / < 10.1.34 / < 9.0.98",
                is_affected: |v| version_below(v, "11.0.2"),
            },
            CveEntry {
                id: "CVE-2024-52317",
                cvss_score: 6.5,
                severity: CveSeverity::Medium,
                summary: "Response mix-up due to incorrect HTTP/2 recycling",
                affected_versions: "< 11.0.1 / < 10.1.33 / < 9.0.97",
                is_affected: |v| version_below(v, "11.0.1"),
            },
            CveEntry {
                id: "CVE-2023-46589",
                cvss_score: 7.5,
                severity: CveSeverity::High,
                summary: "HTTP request smuggling via malformed trailer headers",
                affected_versions: "< 10.1.18 / < 9.0.83 / < 8.5.96",
                is_affected: |v| version_below(v, "10.1.18"),
            },
            CveEntry {
                id: "CVE-2023-44487",
                cvss_score: 7.5,
                severity: CveSeverity::High,
                summary: "HTTP/2 rapid reset DoS attack",
                affected_versions: "< 10.1.14 / < 9.0.81 / < 8.5.94",
                is_affected: |v| version_below(v, "10.1.14"),
            },
        ],
    ),
    (
        "MySQL",
        &[
            CveEntry {
                id: "CVE-2024-21047",
                cvss_score: 4.9,
                severity: CveSeverity::Medium,
                summary: "Server: InnoDB DoS via crafted queries",
                affected_versions: "< 8.0.37",
                is_affected: |v| version_below(v, "8.0.37"),
            },
            CveEntry {
                id: "CVE-2023-22008",
                cvss_score: 4.9,
                severity: CveSeverity::Medium,
                summary: "Server optimizer DoS vulnerability",
                affected_versions: "< 8.0.34",
                is_affected: |v| version_below(v, "8.0.34"),
            },
            CveEntry {
                id: "CVE-2023-21980",
                cvss_score: 7.1,
                severity: CveSeverity::High,
                summary: "Client: buffer overflow in C API",
                affected_versions: "< 8.0.33",
                is_affected: |v| version_below(v, "8.0.33"),
            },
            CveEntry {
                id: "CVE-2024-20960",
                cvss_score: 6.5,
                severity: CveSeverity::Medium,
                summary: "Server: RAPID memory DoS in optimizer",
                affected_versions: "< 8.0.36",
                is_affected: |v| version_below(v, "8.0.36"),
            },
        ],
    ),
    (
        "PostgreSQL",
        &[
            CveEntry {
                id: "CVE-2024-10979",
                cvss_score: 8.8,
                severity: CveSeverity::High,
                summary: "Arbitrary code execution via environment variable manipulation in PL/Perl",
                affected_versions: "< 17.1 / < 16.5 / < 15.9",
                is_affected: |v| version_below(v, "17.1"),
            },
            CveEntry {
                id: "CVE-2023-5868",
                cvss_score: 4.3,
                severity: CveSeverity::Medium,
                summary: "Memory disclosure via aggregate function calls",
                affected_versions: "< 16.1",
                is_affected: |v| version_below(v, "16.1"),
            },
            CveEntry {
                id: "CVE-2023-5869",
                cvss_score: 8.8,
                severity: CveSeverity::High,
                summary: "Buffer overflow in integer overflow in array modification",
                affected_versions: "< 16.1",
                is_affected: |v| version_below(v, "16.1"),
            },
            CveEntry {
                id: "CVE-2023-39417",
                cvss_score: 8.8,
                severity: CveSeverity::High,
                summary: "SQL injection in extension script @extowner@, @extschema@ replacements",
                affected_versions: "< 15.4",
                is_affected: |v| version_below(v, "15.4"),
            },
        ],
    ),
    (
        "Redis",
        &[
            CveEntry {
                id: "CVE-2024-31449",
                cvss_score: 8.8,
                severity: CveSeverity::High,
                summary: "Lua library: heap buffer overflow from crafted script",
                affected_versions: "< 7.4.1",
                is_affected: |v| version_below(v, "7.4.1"),
            },
            CveEntry {
                id: "CVE-2023-45145",
                cvss_score: 3.6,
                severity: CveSeverity::Low,
                summary: "Unix socket permission race condition on startup",
                affected_versions: "< 7.2.4",
                is_affected: |v| version_below(v, "7.2.4"),
            },
            CveEntry {
                id: "CVE-2023-41056",
                cvss_score: 8.1,
                severity: CveSeverity::High,
                summary: "Heap overflow in Cluster-mode pattern matching",
                affected_versions: "< 7.2.1",
                is_affected: |v| version_below(v, "7.2.1"),
            },
            CveEntry {
                id: "CVE-2023-28856",
                cvss_score: 6.5,
                severity: CveSeverity::Medium,
                summary: "AUTH command crash allows authenticated DoS",
                affected_versions: "< 7.0.12",
                is_affected: |v| version_below(v, "7.0.12"),
            },
        ],
    ),
    (
        "MongoDB",
        &[
            CveEntry {
                id: "CVE-2024-1351",
                cvss_score: 8.1,
                severity: CveSeverity::High,
                summary: "Incorrect validation of SCRAM authentication leading to auth bypass",
                affected_versions: "< 7.0.5 / < 6.0.13",
                is_affected: |v| version_below(v, "7.0.5"),
            },
            CveEntry {
                id: "CVE-2023-1409",
                cvss_score: 7.5,
                severity: CveSeverity::High,
                summary: "TLS certificate validation bypass on non-default configurations",
                affected_versions: "< 6.0.7 / < 5.0.18",
                is_affected: |v| version_below(v, "6.0.7"),
            },
            CveEntry {
                id: "CVE-2024-3372",
                cvss_score: 7.5,
                severity: CveSeverity::High,
                summary: "Server crash via malformed BSON in validated collection",
                affected_versions: "< 7.0.9 / < 6.0.15",
                is_affected: |v| version_below(v, "7.0.9"),
            },
        ],
    ),
    (
        "Elasticsearch",
        &[
            CveEntry {
                id: "CVE-2023-31419",
                cvss_score: 7.5,
                severity: CveSeverity::High,
                summary: "Stack overflow DoS from malformed _search API query",
                affected_versions: "< 8.9.1 / < 7.17.13",
                is_affected: |v| version_below(v, "8.9.1"),
            },
            CveEntry {
                id: "CVE-2023-46674",
                cvss_score: 8.8,
                severity: CveSeverity::High,
                summary: "File descriptor leak allows local file read",
                affected_versions: "< 8.11.1 / < 7.17.15",
                is_affected: |v| version_below(v, "8.11.1"),
            },
            CveEntry {
                id: "CVE-2024-23450",
                cvss_score: 7.5,
                severity: CveSeverity::High,
                summary: "DoS via excessive recursion in nested document expansion",
                affected_versions: "< 8.13.0 / < 7.17.19",
                is_affected: |v| version_below(v, "8.13.0"),
            },
        ],
    ),
    (
        "Microsoft IIS",
        &[
            CveEntry {
                id: "CVE-2023-36899",
                cvss_score: 8.8,
                severity: CveSeverity::High,
                summary: "Elevation of privilege via ASP.NET identity spoofing",
                affected_versions: "IIS 10.0",
                is_affected: |v| v.starts_with("10.") || v == "10",
            },
            CveEntry {
                id: "CVE-2024-38169",
                cvss_score: 8.8,
                severity: CveSeverity::High,
                summary: "RCE via crafted HTTP request in ASP.NET IIS module",
                affected_versions: "IIS 10.0",
                is_affected: |v| v.starts_with("10.") || v == "10",
            },
            CveEntry {
                id: "CVE-2022-21907",
                cvss_score: 9.8,
                severity: CveSeverity::Critical,
                summary: "HTTP Protocol Stack (http.sys) RCE in trailer support",
                affected_versions: "IIS 10.0 (Win Server 2022)",
                is_affected: |v| v.starts_with("10.") || v == "10",
            },
        ],
    ),
    (
        "ProFTPD",
        &[
            CveEntry {
                id: "CVE-2023-51713",
                cvss_score: 7.5,
                severity: CveSeverity::High,
                summary: "Out-of-bounds read in mod_sftp during key exchange",
                affected_versions: "< 1.3.8b",
                is_affected: |v| version_below(v, "1.3.8.2"),
            },
            CveEntry {
                id: "CVE-2021-46854",
                cvss_score: 7.5,
                severity: CveSeverity::High,
                summary: "Memory disclosure in mod_radius via crafted RADIUS response",
                affected_versions: "< 1.3.7d",
                is_affected: |v| version_below(v, "1.3.7.4"),
            },
            CveEntry {
                id: "CVE-2020-9273",
                cvss_score: 8.8,
                severity: CveSeverity::High,
                summary: "Use-after-free in memory pool transfer leading to RCE",
                affected_versions: "< 1.3.6c",
                is_affected: |v| version_below(v, "1.3.6.3"),
            },
        ],
    ),
    (
        "vsftpd",
        &[
            CveEntry {
                id: "CVE-2021-3618",
                cvss_score: 7.4,
                severity: CveSeverity::High,
                summary: "ALPACA: Application-layer protocol cross-protocol attack on TLS",
                affected_versions: "< 3.0.4",
                is_affected: |v| version_below(v, "3.0.4"),
            },
            CveEntry {
                id: "CVE-2015-1419",
                cvss_score: 5.3,
                severity: CveSeverity::Medium,
                summary: "Config parsing allows directory listing bypass",
                affected_versions: "< 3.0.3",
                is_affected: |v| version_below(v, "3.0.3"),
            },
        ],
    ),
    (
        "Postfix",
        &[
            CveEntry {
                id: "CVE-2023-51764",
                cvss_score: 5.3,
                severity: CveSeverity::Medium,
                summary: "SMTP smuggling allows email spoofing via multi-line pipelining",
                affected_versions: "< 3.8.4",
                is_affected: |v| version_below(v, "3.8.4"),
            },
            CveEntry {
                id: "CVE-2023-32182",
                cvss_score: 5.3,
                severity: CveSeverity::Medium,
                summary: "Incorrect permission on mail directories during creation",
                affected_versions: "< 3.7.5",
                is_affected: |v| version_below(v, "3.7.5"),
            },
            CveEntry {
                id: "CVE-2021-28085",
                cvss_score: 4.3,
                severity: CveSeverity::Medium,
                summary: "Denial of service via large MIME attachment processing",
                affected_versions: "< 3.6.0",
                is_affected: |v| version_below(v, "3.6.0"),
            },
        ],
    ),
    (
        "Exim",
        &[
            CveEntry {
                id: "CVE-2023-42117",
                cvss_score: 8.1,
                severity: CveSeverity::High,
                summary: "Improper neutralization of special elements in SMTP AUTH",
                affected_versions: "< 4.96.2",
                is_affected: |v| version_below(v, "4.96.2"),
            },
            CveEntry {
                id: "CVE-2023-42119",
                cvss_score: 3.1,
                severity: CveSeverity::Low,
                summary: "DNS name parsing out-of-bounds read",
                affected_versions: "< 4.96.2",
                is_affected: |v| version_below(v, "4.96.2"),
            },
            CveEntry {
                id: "CVE-2023-51766",
                cvss_score: 5.3,
                severity: CveSeverity::Medium,
                summary: "SMTP smuggling via multi-line pipelining",
                affected_versions: "< 4.97.1",
                is_affected: |v| version_below(v, "4.97.1"),
            },
            CveEntry {
                id: "CVE-2024-39929",
                cvss_score: 9.1,
                severity: CveSeverity::Critical,
                summary: "RFC 2231 header filename parsing bypass allows attachment filter evasion",
                affected_versions: "< 4.98",
                is_affected: |v| version_below(v, "4.98"),
            },
        ],
    ),
    (
        "PHP",
        &[
            CveEntry {
                id: "CVE-2024-4577",
                cvss_score: 9.8,
                severity: CveSeverity::Critical,
                summary: "CGI argument injection on Windows (Best-Fit mapping bypass)",
                affected_versions: "< 8.3.8 / < 8.2.20 / < 8.1.29",
                is_affected: |v| version_below(v, "8.3.8"),
            },
            CveEntry {
                id: "CVE-2024-2961",
                cvss_score: 8.8,
                severity: CveSeverity::High,
                summary: "Buffer overflow in glibc iconv via PHP filters",
                affected_versions: "< 8.3.8",
                is_affected: |v| version_below(v, "8.3.8"),
            },
            CveEntry {
                id: "CVE-2023-3824",
                cvss_score: 9.8,
                severity: CveSeverity::Critical,
                summary: "Buffer overflow in PHAR reading for tar/zip/phar",
                affected_versions: "< 8.2.10 / < 8.1.23",
                is_affected: |v| version_below(v, "8.2.10"),
            },
            CveEntry {
                id: "CVE-2024-8932",
                cvss_score: 9.8,
                severity: CveSeverity::Critical,
                summary: "Heap overflow in ldap_escape on 32-bit systems",
                affected_versions: "< 8.3.14 / < 8.2.26 / < 8.1.31",
                is_affected: |v| version_below(v, "8.3.14"),
            },
        ],
    ),
    (
        "Node.js",
        &[
            CveEntry {
                id: "CVE-2024-22019",
                cvss_score: 7.5,
                severity: CveSeverity::High,
                summary: "Reading unprocessed HTTP request with unbounded chunk extension causes DoS",
                affected_versions: "< 21.6.1 / < 20.11.1 / < 18.19.1",
                is_affected: |v| version_below(v, "21.6.1"),
            },
            CveEntry {
                id: "CVE-2024-22025",
                cvss_score: 6.5,
                severity: CveSeverity::Medium,
                summary: "Denial of service via fetch with long Content-Length in Undici",
                affected_versions: "< 21.6.1 / < 20.11.1",
                is_affected: |v| version_below(v, "21.6.1"),
            },
            CveEntry {
                id: "CVE-2023-44487",
                cvss_score: 7.5,
                severity: CveSeverity::High,
                summary: "HTTP/2 rapid reset attack causing DoS",
                affected_versions: "< 20.8.1 / < 18.18.2",
                is_affected: |v| version_below(v, "20.8.1"),
            },
            CveEntry {
                id: "CVE-2023-32002",
                cvss_score: 9.8,
                severity: CveSeverity::Critical,
                summary: "Policy bypass via Module._load",
                affected_versions: "< 20.5.1 / < 18.17.1",
                is_affected: |v| version_below(v, "20.5.1"),
            },
        ],
    ),
    (
        "Jenkins",
        &[
            CveEntry {
                id: "CVE-2024-23897",
                cvss_score: 9.8,
                severity: CveSeverity::Critical,
                summary: "Arbitrary file read via CLI command parser (args4j @-file expansion)",
                affected_versions: "< 2.442 / < 2.426.3",
                is_affected: |v| version_below(v, "2.442"),
            },
            CveEntry {
                id: "CVE-2024-43044",
                cvss_score: 8.8,
                severity: CveSeverity::High,
                summary: "Arbitrary file read on agent via Remoting library ClassLoaderProxy",
                affected_versions: "< 2.471 / < 2.452.4",
                is_affected: |v| version_below(v, "2.471"),
            },
            CveEntry {
                id: "CVE-2023-27898",
                cvss_score: 8.8,
                severity: CveSeverity::High,
                summary: "Stored XSS via crafted plugin name leading to RCE",
                affected_versions: "< 2.394 / < 2.375.4",
                is_affected: |v| version_below(v, "2.394"),
            },
        ],
    ),
    (
        "Plink",
        &[
            CveEntry {
                id: "CVE-2024-31497",
                cvss_score: 5.9,
                severity: CveSeverity::Medium,
                summary: "Biased ECDSA nonce generation allows private key recovery (P-521)",
                affected_versions: "< 0.81",
                is_affected: |v| version_below(v, "0.81"),
            },
            CveEntry {
                id: "CVE-2023-48795",
                cvss_score: 5.9,
                severity: CveSeverity::Medium,
                summary: "Terrapin: Prefix truncation attack on SSH BPP (strict kex bypass)",
                affected_versions: "< 0.80",
                is_affected: |v| version_below(v, "0.80"),
            },
        ],
    ),
    (
        "PuTTY",
        &[
            CveEntry {
                id: "CVE-2024-31497",
                cvss_score: 5.9,
                severity: CveSeverity::Medium,
                summary: "Biased ECDSA nonce generation allows private key recovery (P-521)",
                affected_versions: "< 0.81",
                is_affected: |v| version_below(v, "0.81"),
            },
            CveEntry {
                id: "CVE-2023-48795",
                cvss_score: 5.9,
                severity: CveSeverity::Medium,
                summary: "Terrapin: Prefix truncation attack on SSH BPP (strict kex bypass)",
                affected_versions: "< 0.80",
                is_affected: |v| version_below(v, "0.80"),
            },
            CveEntry {
                id: "CVE-2021-36367",
                cvss_score: 7.4,
                severity: CveSeverity::High,
                summary: "Host key verification bypass allows MITM via fake authentication prompts",
                affected_versions: "< 0.76",
                is_affected: |v| version_below(v, "0.76"),
            },
        ],
    ),
];

static BANNER_PATTERNS: LazyLock<Vec<(Regex, &'static str)>> = LazyLock::new(|| {
    vec![
        (
            Regex::new(r"(?i)OpenSSH[_/ ]?(\d[\d.p]*)").expect("valid regex"),
            "OpenSSH",
        ),
        (
            Regex::new(r"(?i)PuTTY[_/ ]?(?:Release[_/ ]?)?(\d[\d.]*)").expect("valid regex"),
            "PuTTY",
        ),
        (
            Regex::new(r"(?i)Plink[_/ ]?(?:Release[_/ ]?)?(\d[\d.]*)").expect("valid regex"),
            "Plink",
        ),
        (
            Regex::new(r"(?i)Apache(?:/| HTTPD |httpd[ /])(\d[\d.]*)").expect("valid regex"),
            "Apache",
        ),
        (
            Regex::new(r"(?i)Apache Tomcat/(\d[\d.]*)").expect("valid regex"),
            "Apache Tomcat",
        ),
        (
            Regex::new(r"(?i)nginx/(\d[\d.]*)").expect("valid regex"),
            "nginx",
        ),
        (
            Regex::new(r"(?i)Microsoft-IIS/(\d[\d.]*)").expect("valid regex"),
            "Microsoft IIS",
        ),
        (
            Regex::new(r"(?i)ProFTPD[/ ](\d[\d.a-z]*)").expect("valid regex"),
            "ProFTPD",
        ),
        (
            Regex::new(r"(?i)vsftpd[/ ](\d[\d.]*)").expect("valid regex"),
            "vsftpd",
        ),
        (
            Regex::new(r"(?i)Postfix(?:.*?version )?(\d[\d.]*)?").expect("valid regex"),
            "Postfix",
        ),
        (
            Regex::new(r"(?i)Exim[/ ](\d[\d.]*)").expect("valid regex"),
            "Exim",
        ),
        (
            Regex::new(r"(?i)MySQL(?:.*?)?(\d[\d.]*)").expect("valid regex"),
            "MySQL",
        ),
        (
            Regex::new(r"(?i)PostgreSQL[/ ](\d[\d.]*)").expect("valid regex"),
            "PostgreSQL",
        ),
        (
            Regex::new(r"(?i)Redis(?:.*?v=?| server v=?)(\d[\d.]*)").expect("valid regex"),
            "Redis",
        ),
        (
            Regex::new(r"(?i)MongoDB(?:.*?v?| server v?)(\d[\d.]*)").expect("valid regex"),
            "MongoDB",
        ),
        (
            Regex::new(r"(?i)Elasticsearch[/ ](\d[\d.]*)").expect("valid regex"),
            "Elasticsearch",
        ),
        (
            Regex::new(r"(?i)Jenkins[/ :]*(\d[\d.]*)").expect("valid regex"),
            "Jenkins",
        ),
        (
            Regex::new(r"(?i)PHP[/ ](\d[\d.]*)").expect("valid regex"),
            "PHP",
        ),
        (
            Regex::new(r"(?i)Node\.?js[/ ]v?(\d[\d.]*)").expect("valid regex"),
            "Node.js",
        ),
    ]
});

static SIMPLE_NAME_VERSION: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^([A-Za-z][A-Za-z0-9 .+_-]*?)\s+v?(\d[\d.p]*\d)$").expect("valid regex")
});

/// Total number of embedded CVE entries in the database.
#[must_use]
pub fn total_cve_count() -> usize {
    CVE_DATABASE.iter().map(|(_, entries)| entries.len()).sum()
}

/// Total number of unique products covered in the database.
#[must_use]
pub fn total_product_count() -> usize {
    CVE_DATABASE.len()
}

/// Derives a [`CveSeverity`] from a CVSS v3 base score.
#[must_use]
pub fn severity_from_cvss(score: f64) -> CveSeverity {
    if score >= 9.0 {
        CveSeverity::Critical
    } else if score >= 7.0 {
        CveSeverity::High
    } else if score >= 4.0 {
        CveSeverity::Medium
    } else if score > 0.0 {
        CveSeverity::Low
    } else {
        CveSeverity::None
    }
}

/// Searches the embedded CVE database using software name, version, or raw banner.
#[must_use]
pub fn search(input: &str) -> CveSearchResult {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return CveSearchResult {
            resolved_query: String::new(),
            matches: Vec::new(),
        };
    }

    let (software, version) = if let Some((s, v)) = parse_banner(trimmed) {
        (s, v)
    } else if let Some(captures) = SIMPLE_NAME_VERSION.captures(trimmed) {
        let s = captures.get(1).map_or("", |m| m.as_str()).trim().to_owned();
        let v = captures.get(2).map_or("", |m| m.as_str()).trim().to_owned();
        (s, v)
    } else {
        (trimmed.to_owned(), String::new())
    };

    let resolved_query = if version.is_empty() {
        software.clone()
    } else {
        format!("{software} {version}")
    };

    // 1. Exact match on product name (case-insensitive)
    for (prod_name, entries) in CVE_DATABASE {
        if prod_name.eq_ignore_ascii_case(&software) {
            return CveSearchResult {
                resolved_query,
                matches: build_matches(entries, &version),
            };
        }
    }

    // 2. Fuzzy match
    let soft_lower = software.to_ascii_lowercase();
    for (prod_name, entries) in CVE_DATABASE {
        let prod_lower = prod_name.to_ascii_lowercase();
        if prod_lower.contains(&soft_lower) || soft_lower.contains(&prod_lower) {
            let fuzzy_resolved = if version.is_empty() {
                (*prod_name).to_owned()
            } else {
                format!("{prod_name} {version}")
            };
            return CveSearchResult {
                resolved_query: fuzzy_resolved,
                matches: build_matches(entries, &version),
            };
        }
    }

    CveSearchResult {
        resolved_query,
        matches: Vec::new(),
    }
}

/// Formats a search result as human-readable plaintext report suitable for copying to clipboard.
#[must_use]
pub fn format_report(result: &CveSearchResult) -> String {
    let mut out = String::new();
    let count = result.matches.len();
    if count == 1 {
        out.push_str(&format!("1 CVE found for {}\n", result.resolved_query));
    } else {
        out.push_str(&format!(
            "{count} CVEs found for {}\n",
            result.resolved_query
        ));
    }
    out.push_str(&"=".repeat(72));
    out.push_str("\n\n");

    for m in &result.matches {
        out.push_str(&format!(
            "{}  [{}]  CVSS {:.1}\n",
            m.id,
            m.severity.label(),
            m.cvss_score
        ));
        out.push_str(&format!("  {}\n", m.summary));
        out.push_str(&format!("  Affected Versions: {}\n\n", m.affected_versions));
    }

    out
}

/// Parses a service banner into a product name and version string.
#[must_use]
pub fn parse_banner(input: &str) -> Option<(String, String)> {
    for (regex, software) in &*BANNER_PATTERNS {
        if let Some(captures) = regex.captures(input) {
            let version = if captures.len() > 1 {
                captures
                    .get(1)
                    .map_or(String::new(), |m| normalize_version(m.as_str()))
            } else {
                String::new()
            };
            return Some(((*software).to_owned(), version));
        }
    }
    None
}

/// Normalizes version strings by keeping leading numeric dot-separated segments.
#[must_use]
pub fn normalize_version(raw: &str) -> String {
    let mut sb = String::new();
    let mut last_was_dot = false;

    for c in raw.chars() {
        if c.is_ascii_digit() {
            sb.push(c);
            last_was_dot = false;
        } else if c == '.' && !last_was_dot && !sb.is_empty() {
            sb.push('.');
            last_was_dot = true;
        } else {
            break;
        }
    }

    sb.trim_end_matches('.').to_owned()
}

/// Parses a version string into individual numeric components.
#[must_use]
pub fn parse_version(v: &str) -> Vec<i32> {
    if v.trim().is_empty() {
        return Vec::new();
    }

    let normalized = normalize_version(v);
    if normalized.is_empty() {
        return Vec::new();
    }

    normalized
        .split('.')
        .filter_map(|p| p.parse::<i32>().ok())
        .collect()
}

/// Returns `true` if `version` is strictly less than `threshold`.
#[must_use]
pub fn version_below(version: &str, threshold: &str) -> bool {
    let current = parse_version(version);
    let target = parse_version(threshold);
    !current.is_empty()
        && !target.is_empty()
        && compare_versions(&current, &target) == Ordering::Less
}

/// Returns `true` if `version` falls within `[from, to]` inclusive.
#[must_use]
pub fn version_in_range(version: &str, from: &str, to: &str) -> bool {
    let current = parse_version(version);
    let lower = parse_version(from);
    let upper = parse_version(to);
    !current.is_empty()
        && !lower.is_empty()
        && !upper.is_empty()
        && compare_versions(&current, &lower) != Ordering::Less
        && compare_versions(&current, &upper) != Ordering::Greater
}

/// Returns `true` if `version` equals `target` after normalization.
#[must_use]
pub fn version_equals(version: &str, target: &str) -> bool {
    let current = parse_version(version);
    let expected = parse_version(target);
    !current.is_empty()
        && !expected.is_empty()
        && compare_versions(&current, &expected) == Ordering::Equal
}

/// Compares two parsed version slices component-by-component.
#[must_use]
pub fn compare_versions(a: &[i32], b: &[i32]) -> Ordering {
    let max_len = a.len().max(b.len());
    for i in 0..max_len {
        let left = *a.get(i).unwrap_or(&0);
        let right = *b.get(i).unwrap_or(&0);
        match left.cmp(&right) {
            Ordering::Equal => continue,
            other => return other,
        }
    }
    Ordering::Equal
}

fn build_matches(entries: &[CveEntry], version: &str) -> Vec<CveMatch> {
    let mut matches = Vec::new();

    for entry in entries {
        let is_affected = version.is_empty() || (entry.is_affected)(version);
        if !is_affected && !version.is_empty() {
            continue;
        }

        matches.push(CveMatch {
            id: entry.id.to_owned(),
            cvss_score: entry.cvss_score,
            severity: entry.severity,
            summary: entry.summary.to_owned(),
            affected_versions: entry.affected_versions.to_owned(),
        });
    }

    matches.sort_by(|a, b| {
        b.cvss_score
            .partial_cmp(&a.cvss_score)
            .unwrap_or(Ordering::Equal)
    });
    matches
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn database_counts_match_csharp() {
        assert_eq!(total_product_count(), 19);
        assert_eq!(total_cve_count(), 67);
    }

    #[test]
    fn version_parsing_and_normalization() {
        assert_eq!(normalize_version("8.9p1"), "8.9");
        assert_eq!(normalize_version("1.3.8b"), "1.3.8");
        assert_eq!(normalize_version("10.0"), "10.0");
        assert_eq!(normalize_version("v9"), "");
        assert_eq!(parse_version("8.9p1"), vec![8, 9]);
        assert_eq!(parse_version("10.1.18"), vec![10, 1, 18]);
        assert_eq!(parse_version(""), Vec::<i32>::new());
    }

    #[test]
    fn version_comparisons() {
        assert!(version_below("8.9", "9.6"));
        assert!(!version_below("9.6", "9.6"));
        assert!(!version_below("9.7", "9.6"));

        assert!(version_in_range("8.6", "8.5", "9.7"));
        assert!(version_in_range("8.5", "8.5", "9.7"));
        assert!(version_in_range("9.7", "8.5", "9.7"));
        assert!(!version_in_range("8.4", "8.5", "9.7"));
        assert!(!version_in_range("9.8", "8.5", "9.7"));

        assert!(version_equals("2.4.49", "2.4.49"));
        assert!(!version_equals("2.4.50", "2.4.49"));
    }

    #[test]
    fn banner_detection() {
        let (soft, ver) = parse_banner("SSH-2.0-OpenSSH_8.9p1 Ubuntu-3ubuntu0.1").unwrap();
        assert_eq!(soft, "OpenSSH");
        assert_eq!(ver, "8.9");

        let (soft, ver) = parse_banner("Apache/2.4.49 (Unix)").unwrap();
        assert_eq!(soft, "Apache");
        assert_eq!(ver, "2.4.49");

        let (soft, ver) = parse_banner("nginx/1.20.0").unwrap();
        assert_eq!(soft, "nginx");
        assert_eq!(ver, "1.20.0");
    }

    #[test]
    fn search_exact_product_with_version() {
        let res = search("OpenSSH 8.9");
        assert_eq!(res.resolved_query, "OpenSSH 8.9");
        assert!(!res.matches.is_empty());
        // regreSSHion affects 8.5 - 9.7, so 8.9 must be matched
        assert!(res.matches.iter().any(|m| m.id == "CVE-2024-6387"));
        // Terrapin affects < 9.6, so 8.9 must be matched
        assert!(res.matches.iter().any(|m| m.id == "CVE-2023-48795"));
    }

    #[test]
    fn search_from_banner() {
        let res = search("Apache/2.4.49 (Debian)");
        assert_eq!(res.resolved_query, "Apache 2.4.49");
        assert!(res.matches.iter().any(|m| m.id == "CVE-2021-41773"));
        assert!(res.matches.iter().any(|m| m.id == "CVE-2021-42013"));
    }

    #[test]
    fn search_product_without_version_returns_all() {
        let res = search("Redis");
        assert_eq!(res.resolved_query, "Redis");
        assert_eq!(res.matches.len(), 4);
    }

    #[test]
    fn search_fuzzy_name() {
        let res = search("postgres");
        assert_eq!(res.resolved_query, "PostgreSQL");
        assert_eq!(res.matches.len(), 4);
    }

    #[test]
    fn search_empty_input() {
        let res = search("");
        assert!(res.resolved_query.is_empty());
        assert!(res.matches.is_empty());
    }

    #[test]
    fn severity_from_cvss_thresholds() {
        assert_eq!(severity_from_cvss(9.8), CveSeverity::Critical);
        assert_eq!(severity_from_cvss(8.1), CveSeverity::High);
        assert_eq!(severity_from_cvss(6.5), CveSeverity::Medium);
        assert_eq!(severity_from_cvss(3.6), CveSeverity::Low);
        assert_eq!(severity_from_cvss(0.0), CveSeverity::None);
    }

    #[test]
    fn format_report_output() {
        let res = CveSearchResult {
            resolved_query: "OpenSSH 8.9".to_owned(),
            matches: vec![CveMatch {
                id: "CVE-2024-6387".to_owned(),
                cvss_score: 8.1,
                severity: CveSeverity::High,
                summary: "Sample summary".to_owned(),
                affected_versions: "8.5 - 9.7".to_owned(),
            }],
        };
        let report = format_report(&res);
        assert!(report.contains("1 CVE found for OpenSSH 8.9"));
        assert!(report.contains("CVE-2024-6387  [High]  CVSS 8.1"));
        assert!(report.contains("  Sample summary"));
        assert!(report.contains("  Affected Versions: 8.5 - 9.7"));
    }
}
