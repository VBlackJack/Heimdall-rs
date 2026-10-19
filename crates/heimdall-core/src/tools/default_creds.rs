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

//! The Default Credentials catalog, mirroring the preset catalog of the C#
//! `Heimdall.App.Services.DefaultCredentialPresets` but never the engine that tries
//! them (`Heimdall.Core.Security.DefaultCredentialEngine`): owner decision
//! (2026-10-17, item 10) keeps the Rust tool list-only, so this module provides the
//! credential catalog for well-known services (SSH, Telnet, FTP, HTTP, SNMP, MySQL,
//! PostgreSQL, Redis, MSSQL, VNC), the entry structure, and CSV/report formatters. It
//! never opens a socket.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::doc_markdown,
    clippy::module_name_repetitions,
    clippy::redundant_closure_for_method_calls
)]

/// A single catalogued (service, port, username, password) default credential entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DefaultCredEntry {
    /// The name of the service (e.g. "SSH", "HTTP", "MySQL").
    pub service: &'static str,
    /// The standard network port for the service.
    pub port: u16,
    /// The default username.
    pub username: &'static str,
    /// The default password.
    pub password: &'static str,
}

/// Known service port mappings.
pub const SERVICE_PORTS: &[(u16, &str)] = &[
    (22, "SSH"),
    (23, "Telnet"),
    (21, "FTP"),
    (80, "HTTP"),
    (443, "HTTP"),
    (8080, "HTTP"),
    (8443, "HTTP"),
    (3306, "MySQL"),
    (5432, "PostgreSQL"),
    (1433, "MSSQL"),
    (6379, "Redis"),
    (161, "SNMP"),
    (5900, "VNC"),
    (5901, "VNC"),
];

/// Returns the service name associated with the given standard port.
#[must_use]
pub fn service_for_port(port: u16) -> Option<&'static str> {
    SERVICE_PORTS
        .iter()
        .find(|(p, _)| *p == port)
        .map(|(_, s)| *s)
}

/// Standard default credentials by service name.
pub const CREDENTIALS_BY_SERVICE: &[(&str, &[(&str, &str)])] = &[
    (
        "SSH",
        &[
            ("root", "root"),
            ("root", "toor"),
            ("admin", "admin"),
            ("admin", "password"),
            ("admin", "1234"),
            ("pi", "raspberry"),
            ("ubnt", "ubnt"),
            ("vagrant", "vagrant"),
        ],
    ),
    (
        "Telnet",
        &[
            ("admin", "admin"),
            ("root", "root"),
            ("admin", "password"),
            ("admin", "1234"),
        ],
    ),
    (
        "HTTP",
        &[
            ("admin", "admin"),
            ("admin", "password"),
            ("admin", "1234"),
            ("root", "root"),
            ("admin", ""),
            ("user", "user"),
        ],
    ),
    (
        "FTP",
        &[
            ("anonymous", ""),
            ("admin", "admin"),
            ("ftp", "ftp"),
            ("root", "root"),
        ],
    ),
    (
        "SNMP",
        &[
            ("public", ""),
            ("private", ""),
            ("community", ""),
            ("default", ""),
            ("monitor", ""),
        ],
    ),
    (
        "MySQL",
        &[
            ("root", ""),
            ("root", "root"),
            ("root", "mysql"),
            ("admin", "admin"),
        ],
    ),
    (
        "PostgreSQL",
        &[("postgres", "postgres"), ("admin", "admin")],
    ),
    ("Redis", &[("", "")]),
    (
        "MSSQL",
        &[
            ("sa", ""),
            ("sa", "sa"),
            ("sa", "password"),
            ("sa", "Password1"),
        ],
    ),
    ("VNC", &[("", "password"), ("", "1234"), ("", "vnc")]),
];

/// Returns the credential pairs associated with a service name.
#[must_use]
pub fn credentials_for_service(service: &str) -> Option<&'static [(&'static str, &'static str)]> {
    CREDENTIALS_BY_SERVICE
        .iter()
        .find(|(s, _)| s.eq_ignore_ascii_case(service))
        .map(|(_, creds)| *creds)
}

/// Returns every catalogued default credential entry, service by service, in the
/// order they are listed above.
#[must_use]
pub fn all_entries() -> Vec<DefaultCredEntry> {
    let mut entries = Vec::new();
    for &(port, service) in SERVICE_PORTS {
        if let Some(creds) = credentials_for_service(service) {
            for &(username, password) in creds {
                entries.push(DefaultCredEntry {
                    service,
                    port,
                    username,
                    password,
                });
            }
        }
    }
    entries
}

/// Returns the catalogued entries whose service name contains `filter`,
/// case-insensitively. An empty (or all-whitespace) filter returns every entry.
#[must_use]
pub fn filter_entries(filter: &str) -> Vec<DefaultCredEntry> {
    let needle = filter.trim().to_lowercase();
    if needle.is_empty() {
        return all_entries();
    }
    all_entries()
        .into_iter()
        .filter(|e| e.service.to_lowercase().contains(&needle))
        .collect()
}

/// Returns the password's display text: a localized placeholder for the catalogued
/// services that have no password at all (e.g. Redis, anonymous FTP), else the
/// password itself.
#[must_use]
pub fn password_display(password: &str, localize: Option<&dyn Fn(&str) -> String>) -> String {
    if password.is_empty() {
        localize.map_or_else(|| "(empty)".to_string(), |loc| loc("ToolDefCredEmpty"))
    } else {
        password.to_string()
    }
}

/// Builds a plain-text tabular report of `entries` for clipboard export.
#[must_use]
pub fn build_report_text(
    entries: &[DefaultCredEntry],
    localize: Option<&dyn Fn(&str) -> String>,
) -> String {
    let mut out = String::new();

    let col_svc =
        localize.map_or_else(|| "Service".to_string(), |loc| loc("ToolDefCredColService"));
    let col_port = localize.map_or_else(|| "Port".to_string(), |loc| loc("ToolDefCredColPort"));
    let col_user = localize.map_or_else(|| "User".to_string(), |loc| loc("ToolDefCredColUser"));
    let col_pass = localize.map_or_else(|| "Password".to_string(), |loc| loc("ToolDefCredColPass"));

    out.push_str(&format!(
        "{col_svc:<12}{col_port:<8}{col_user:<16}{col_pass}\n"
    ));
    out.push_str(&format!("{}\n", "-".repeat(60)));

    for e in entries {
        let pass = password_display(e.password, localize);
        out.push_str(&format!(
            "{:<12}{:<8}{:<16}{pass}\n",
            e.service, e.port, e.username
        ));
    }

    out
}

/// Builds CSV content of `entries` for export.
#[must_use]
pub fn build_csv_export(
    entries: &[DefaultCredEntry],
    localize: Option<&dyn Fn(&str) -> String>,
) -> String {
    let mut out = String::new();

    let col_svc =
        localize.map_or_else(|| "Service".to_string(), |loc| loc("ToolDefCredColService"));
    let col_port = localize.map_or_else(|| "Port".to_string(), |loc| loc("ToolDefCredColPort"));
    let col_user = localize.map_or_else(|| "User".to_string(), |loc| loc("ToolDefCredColUser"));
    let col_pass = localize.map_or_else(|| "Password".to_string(), |loc| loc("ToolDefCredColPass"));

    out.push_str(&format!("{col_svc},{col_port},{col_user},{col_pass}\n"));

    for e in entries {
        let pass = password_display(e.password, localize);
        out.push_str(&format!(
            "{},{},{},{}\n",
            super::csv_cell::quoted(e.service),
            e.port,
            super::csv_cell::quoted(e.username),
            super::csv_cell::quoted(&pass)
        ));
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_localize(key: &str) -> String {
        match key {
            "ToolDefCredColService" => "Service".to_string(),
            "ToolDefCredColPort" => "Port".to_string(),
            "ToolDefCredColUser" => "User".to_string(),
            "ToolDefCredColPass" => "Password".to_string(),
            "ToolDefCredEmpty" => "(empty)".to_string(),
            _ => key.to_string(),
        }
    }

    #[test]
    fn service_ports_keep_core_network_mappings() {
        assert_eq!(service_for_port(22), Some("SSH"));
        assert_eq!(service_for_port(23), Some("Telnet"));
        assert_eq!(service_for_port(21), Some("FTP"));
        assert_eq!(service_for_port(161), Some("SNMP"));
    }

    #[test]
    fn credentials_by_service_keep_expected_snmp_and_http_entries() {
        let snmp = credentials_for_service("SNMP").expect("SNMP credentials present");
        assert!(snmp.contains(&("public", "")));

        let http = credentials_for_service("HTTP").expect("HTTP credentials present");
        assert!(http.contains(&("admin", "admin")));

        let ssh = credentials_for_service("SSH").expect("SSH credentials present");
        assert!(ssh.contains(&("root", "root")));
    }

    #[test]
    fn all_entries_covers_every_service_port_pair() {
        let entries = all_entries();
        assert!(!entries.is_empty());
        assert!(
            entries
                .iter()
                .any(|e| e.service == "SSH" && e.port == 22 && e.username == "root")
        );
        // HTTP is mapped to four ports: every one of them yields its own entries.
        let http_ports: std::collections::HashSet<u16> = entries
            .iter()
            .filter(|e| e.service == "HTTP")
            .map(|e| e.port)
            .collect();
        assert_eq!(http_ports.len(), 4);
    }

    #[test]
    fn filter_entries_is_case_insensitive_and_matches_service_only() {
        let filtered = filter_entries("ssh");
        assert!(!filtered.is_empty());
        assert!(filtered.iter().all(|e| e.service == "SSH"));

        let filtered_mixed_case = filter_entries("SsH");
        assert_eq!(filtered, filtered_mixed_case);
    }

    #[test]
    fn filter_entries_empty_filter_returns_everything() {
        assert_eq!(filter_entries(""), all_entries());
        assert_eq!(filter_entries("   "), all_entries());
    }

    #[test]
    fn filter_entries_no_match_is_empty() {
        assert!(filter_entries("does-not-exist").is_empty());
    }

    #[test]
    fn password_display_uses_placeholder_for_empty_password() {
        assert_eq!(password_display("", Some(&test_localize)), "(empty)");
        assert_eq!(password_display("toor", Some(&test_localize)), "toor");
        assert_eq!(password_display("", None), "(empty)");
    }

    #[test]
    fn build_report_text_contains_header_and_data() {
        let entries = vec![DefaultCredEntry {
            service: "SSH",
            port: 22,
            username: "root",
            password: "toor",
        }];
        let report = build_report_text(&entries, Some(&test_localize));
        assert!(report.contains("Service"));
        assert!(report.contains("SSH"));
        assert!(report.contains("22"));
        assert!(report.contains("toor"));
    }

    #[test]
    fn build_report_text_shows_placeholder_for_empty_password() {
        let entries = vec![DefaultCredEntry {
            service: "Redis",
            port: 6379,
            username: "",
            password: "",
        }];
        let report = build_report_text(&entries, Some(&test_localize));
        assert!(report.contains("(empty)"));
    }

    #[test]
    fn build_csv_export_contains_header_and_sanitized_data() {
        let entries = vec![DefaultCredEntry {
            service: "=SSH",
            port: 22,
            username: "root",
            password: "root",
        }];

        let csv = build_csv_export(&entries, Some(&test_localize));
        assert!(csv.contains("Service"));
        assert!(csv.contains("\"'=SSH\""));
    }

    #[test]
    fn build_csv_export_escapes_quotes_in_password() {
        let entries = vec![DefaultCredEntry {
            service: "FTP",
            port: 21,
            username: "admin",
            password: "pass\"word",
        }];

        let csv = build_csv_export(&entries, Some(&test_localize));
        assert!(csv.contains("pass\"\"word"));
    }

    #[test]
    fn build_csv_export_without_localize_uses_english_defaults() {
        let entries = vec![DefaultCredEntry {
            service: "FTP",
            port: 21,
            username: "anonymous",
            password: "",
        }];

        let csv = build_csv_export(&entries, None);
        assert!(csv.contains("Service,Port,User,Password"));
        assert!(csv.contains("(empty)"));
    }
}
