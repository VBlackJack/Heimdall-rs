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

//! The Default Credentials engine and preset catalog, mirroring the C#
//! `Heimdall.Core.Security.DefaultCredentialEngine` and
//! `Heimdall.App.Services.DefaultCredentialPresets`:
//! provides credential test definitions, preset catalogs for well-known
//! services (SSH, Telnet, FTP, HTTP, SNMP, MySQL, PostgreSQL, Redis, MSSQL, VNC),
//! result structures, CSV/report formatters, and HTTP response validation.

use serde::{Deserialize, Serialize};

/// Connection timeout in milliseconds for network probes.
pub const CONNECT_TIMEOUT_MS: u64 = 5000;

/// Rate limit delay in milliseconds between requests to avoid overloading targets.
pub const RATE_LIMIT_DELAY_MS: u64 = 1000;

/// Maximum number of services scanned concurrently.
pub const MAX_CONCURRENT_SERVICES: usize = 3;

/// TCP port probe timeout in milliseconds.
pub const PORT_PROBE_TIMEOUT_MS: u64 = 2000;

/// Credential test result status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CredTestStatus {
    /// Default or factory credential was accepted.
    Default,
    /// Credential was rejected or changed.
    Changed,
    /// Credential test failed due to an operational error.
    Error,
}

/// Immutable result of a single default-credential test.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CredTestResult {
    /// The name of the service (e.g. "SSH", "HTTP", "MySQL").
    pub service: String,
    /// The network port tested.
    pub port: u16,
    /// The username tested.
    pub username: String,
    /// The password tested.
    pub password: String,
    /// Outcome of the credential test.
    pub status: CredTestStatus,
    /// Optional error detail when status is `CredTestStatus::Error`.
    pub error_detail: Option<String>,
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

/// Checks if an HTTP status line indicates successful authentication (status code in 200..399).
#[must_use]
pub fn is_http_success_response(response_line: &str) -> bool {
    if response_line.is_empty() {
        return false;
    }

    let mut parts = response_line.split_whitespace();
    let Some(_proto) = parts.next() else {
        return false;
    };
    let Some(status_str) = parts.next() else {
        return false;
    };

    if let Ok(status_code) = status_str.parse::<u16>() {
        (200..400).contains(&status_code)
    } else {
        false
    }
}

/// Returns the localized label key or formatted label for a result status.
#[must_use]
pub fn status_to_label(
    status: CredTestStatus,
    localize: Option<&dyn Fn(&str) -> String>,
) -> String {
    let key = match status {
        CredTestStatus::Default => "ToolDefCredStatusDefault",
        CredTestStatus::Changed => "ToolDefCredStatusChanged",
        CredTestStatus::Error => "ToolDefCredStatusError",
    };
    localize.map_or_else(|| key.to_string(), |loc| loc(key))
}

/// Returns the localized detail text for a non-error status.
#[must_use]
pub fn status_to_detail(
    status: CredTestStatus,
    service: &str,
    localize: Option<&dyn Fn(&str) -> String>,
) -> String {
    match status {
        CredTestStatus::Default => {
            let key = "ToolDefCredDetailAccepted";
            localize.map_or_else(
                || format!("{service} accepted"),
                |loc| {
                    let template = loc(key);
                    template.replace("{0}", service)
                },
            )
        }
        CredTestStatus::Changed => {
            let key = "ToolDefCredDetailRejected";
            localize.map_or_else(
                || format!("{service} rejected"),
                |loc| {
                    let template = loc(key);
                    template.replace("{0}", service)
                },
            )
        }
        CredTestStatus::Error => String::new(),
    }
}

/// Sanitizes a value for safe inclusion in CSV cells by prefixing formula-triggering
/// characters with a single quote, preventing spreadsheet formula injection.
#[must_use]
pub fn sanitize_csv_cell(value: &str) -> String {
    if value.is_empty() {
        return String::new();
    }
    let first = value.chars().next().unwrap_or('\0');
    if matches!(first, '=' | '+' | '-' | '@' | '\t' | '\r' | '\n') {
        format!("'{value}")
    } else {
        value.to_string()
    }
}

/// Escapes a CSV cell value with double quotes and escaping embedded quotes.
#[must_use]
pub fn escape_csv(value: &str) -> String {
    let sanitized = sanitize_csv_cell(value).replace('"', "\"\"");
    format!("\"{sanitized}\"")
}

/// Builds a summary line for the current results.
#[must_use]
pub fn build_summary_text(
    results: &[CredTestResult],
    localize: Option<&dyn Fn(&str) -> String>,
) -> String {
    let default_count = results
        .iter()
        .filter(|r| r.status == CredTestStatus::Default)
        .count();

    let mut services_seen = std::collections::HashSet::new();
    for r in results.iter().filter(|r| r.status == CredTestStatus::Default) {
        services_seen.insert((r.service.as_str(), r.port));
    }
    let service_count = services_seen.len();

    if default_count == 0 {
        return localize.map_or_else(
            || "No defaults found".to_string(),
            |loc| loc("ToolDefCredNoDefaults"),
        );
    }

    localize.map_or_else(
        || {
            let default_str = if default_count == 1 {
                "1 default".to_string()
            } else {
                format!("{default_count} defaults")
            };
            let service_str = if service_count == 1 {
                "1 service".to_string()
            } else {
                format!("{service_count} services")
            };
            format!("{default_str} on {service_str}")
        },
        |loc| {
            let template = loc("ToolDefCredSummary");
            let cred_key = if default_count == 1 {
                "ToolDefCredSummaryCredentialsOne"
            } else {
                "ToolDefCredSummaryCredentials"
            };
            let svc_key = if service_count == 1 {
                "ToolDefCredSummaryServicesOne"
            } else {
                "ToolDefCredSummaryServices"
            };

            let cred_str = loc(cred_key).replace("{0}", &default_count.to_string());
            let svc_str = loc(svc_key).replace("{0}", &service_count.to_string());

            template.replace("{0}", &cred_str).replace("{1}", &svc_str)
        },
    )
}

/// Builds a plain-text tabular report for clipboard export.
#[must_use]
pub fn build_report_text(
    results: &[CredTestResult],
    localize: Option<&dyn Fn(&str) -> String>,
) -> String {
    let mut out = String::new();

    let col_svc = localize.map_or_else(|| "Service", |loc| loc("ToolDefCredColService").leak());
    let col_port = localize.map_or_else(|| "Port", |loc| loc("ToolDefCredColPort").leak());
    let col_user = localize.map_or_else(|| "User", |loc| loc("ToolDefCredColUser").leak());
    let col_pass = localize.map_or_else(|| "Password", |loc| loc("ToolDefCredColPass").leak());
    let col_status = localize.map_or_else(|| "Status", |loc| loc("ToolDefCredColStatus").leak());
    let col_detail = localize.map_or_else(|| "Detail", |loc| loc("ToolDefCredColDetail").leak());

    out.push_str(&format!(
        "{col_svc:<12}{col_port:<8}{col_user:<16}{col_pass:<16}{col_status:<12}{col_detail}\n"
    ));
    out.push_str(&format!("{}\n", "-".repeat(80)));

    for r in results {
        let status_label = status_to_label(r.status, localize);
        let detail = if r.status == CredTestStatus::Error {
            r.error_detail.clone().unwrap_or_default()
        } else {
            status_to_detail(r.status, &r.service, localize)
        };

        out.push_str(&format!(
            "{:<12}{:<8}{:<16}{:<16}{:<12}{detail}\n",
            r.service, r.port, r.username, r.password, status_label
        ));
    }

    out
}

/// Builds CSV content for export.
#[must_use]
pub fn build_csv_export(
    results: &[CredTestResult],
    localize: Option<&dyn Fn(&str) -> String>,
) -> String {
    let mut out = String::new();

    let col_svc = localize.map_or_else(|| "Service".to_string(), |loc| loc("ToolDefCredColService"));
    let col_port = localize.map_or_else(|| "Port".to_string(), |loc| loc("ToolDefCredColPort"));
    let col_user = localize.map_or_else(|| "User".to_string(), |loc| loc("ToolDefCredColUser"));
    let col_pass = localize.map_or_else(|| "Password".to_string(), |loc| loc("ToolDefCredColPass"));
    let col_status = localize.map_or_else(|| "Status".to_string(), |loc| loc("ToolDefCredColStatus"));
    let col_detail = localize.map_or_else(|| "Detail".to_string(), |loc| loc("ToolDefCredColDetail"));

    out.push_str(&format!(
        "{col_svc},{col_port},{col_user},{col_pass},{col_status},{col_detail}\n"
    ));

    for r in results {
        let detail = if r.status == CredTestStatus::Error {
            r.error_detail.clone().unwrap_or_default()
        } else {
            status_to_detail(r.status, &r.service, localize)
        };

        let status_label = status_to_label(r.status, localize);

        out.push_str(&format!(
            "{},{},{},{},{},{}\n",
            escape_csv(&r.service),
            r.port,
            escape_csv(&r.username),
            escape_csv(&r.password),
            escape_csv(&status_label),
            escape_csv(&detail)
        ));
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_localize(key: &str) -> String {
        match key {
            "ToolDefCredSummary" => "{0} on {1}".to_string(),
            "ToolDefCredSummaryCredentials" => "{0} defaults".to_string(),
            "ToolDefCredSummaryCredentialsOne" => "{0} default".to_string(),
            "ToolDefCredSummaryServices" => "{0} services".to_string(),
            "ToolDefCredSummaryServicesOne" => "{0} service".to_string(),
            "ToolDefCredNoDefaults" => "No defaults found".to_string(),
            "ToolDefCredColService" => "Service".to_string(),
            "ToolDefCredColPort" => "Port".to_string(),
            "ToolDefCredColUser" => "User".to_string(),
            "ToolDefCredColPass" => "Password".to_string(),
            "ToolDefCredColStatus" => "Status".to_string(),
            "ToolDefCredColDetail" => "Detail".to_string(),
            "ToolDefCredStatusDefault" => "Default".to_string(),
            "ToolDefCredStatusChanged" => "Changed".to_string(),
            "ToolDefCredStatusError" => "Error".to_string(),
            "ToolDefCredDetailAccepted" => "{0} accepted".to_string(),
            "ToolDefCredDetailRejected" => "{0} rejected".to_string(),
            _ => key.to_string(),
        }
    }

    fn make_result(service: &str, port: u16, status: CredTestStatus) -> CredTestResult {
        CredTestResult {
            service: service.to_string(),
            port,
            username: "root".to_string(),
            password: "root".to_string(),
            status,
            error_detail: None,
        }
    }

    #[test]
    fn is_http_success_response_returns_expected() {
        assert!(is_http_success_response("HTTP/1.1 200 OK"));
        assert!(is_http_success_response("HTTP/1.0 301 Moved"));
        assert!(!is_http_success_response("HTTP/1.1 401 Unauthorized"));
        assert!(!is_http_success_response("HTTP/1.1 403 Forbidden"));
        assert!(!is_http_success_response("HTTP/1.1 500 Internal Server Error"));
        assert!(!is_http_success_response(""));
        assert!(!is_http_success_response("garbage"));
    }

    #[test]
    fn status_to_label_returns_expected_key() {
        assert_eq!(
            status_to_label(CredTestStatus::Default, None),
            "ToolDefCredStatusDefault"
        );
        assert_eq!(
            status_to_label(CredTestStatus::Changed, None),
            "ToolDefCredStatusChanged"
        );
        assert_eq!(
            status_to_label(CredTestStatus::Error, None),
            "ToolDefCredStatusError"
        );
    }

    #[test]
    fn status_to_detail_default_contains_service_with_localized_template() {
        let detail = status_to_detail(
            CredTestStatus::Default,
            "SSH",
            Some(&test_localize),
        );
        assert!(detail.contains("SSH"));
        assert!(detail.contains("accepted"));
    }

    #[test]
    fn build_summary_text_with_defaults_contains_counts() {
        let results = vec![
            make_result("SSH", 22, CredTestStatus::Default),
            make_result("SSH", 22, CredTestStatus::Changed),
            make_result("FTP", 21, CredTestStatus::Default),
        ];

        let summary = build_summary_text(&results, Some(&test_localize));
        assert_eq!(summary, "2 defaults on 2 services");
    }

    #[test]
    fn build_summary_text_no_defaults_uses_no_defaults_message() {
        let results = vec![make_result("SSH", 22, CredTestStatus::Changed)];
        let summary = build_summary_text(&results, Some(&test_localize));
        assert_eq!(summary, "No defaults found");
    }

    #[test]
    fn build_report_text_contains_header_and_data() {
        let results = vec![make_result("SSH", 22, CredTestStatus::Default)];
        let report = build_report_text(&results, Some(&test_localize));
        assert!(report.contains("Service"));
        assert!(report.contains("SSH"));
        assert!(report.contains("22"));
    }

    #[test]
    fn build_csv_export_contains_header_and_sanitized_data() {
        let results = vec![CredTestResult {
            service: "=SSH".to_string(),
            port: 22,
            username: "root".to_string(),
            password: "root".to_string(),
            status: CredTestStatus::Default,
            error_detail: None,
        }];

        let csv = build_csv_export(&results, Some(&test_localize));
        assert!(csv.contains("Service"));
        assert!(csv.contains("\"'=SSH\""));
    }

    #[test]
    fn build_csv_export_escapes_quotes_in_password() {
        let results = vec![CredTestResult {
            service: "FTP".to_string(),
            port: 21,
            username: "admin".to_string(),
            password: "pass\"word".to_string(),
            status: CredTestStatus::Changed,
            error_detail: None,
        }];

        let csv = build_csv_export(&results, Some(&test_localize));
        assert!(csv.contains("pass\"\"word"));
    }

    #[test]
    fn build_csv_export_error_status_includes_error_detail() {
        let results = vec![CredTestResult {
            service: "MySQL".to_string(),
            port: 3306,
            username: "root".to_string(),
            password: String::new(),
            status: CredTestStatus::Error,
            error_detail: Some("Connection refused".to_string()),
        }];

        let csv = build_csv_export(&results, Some(&test_localize));
        assert!(csv.contains("Connection refused"));
    }

    #[test]
    fn build_report_text_error_status_includes_error_detail() {
        let results = vec![CredTestResult {
            service: "Redis".to_string(),
            port: 6379,
            username: String::new(),
            password: String::new(),
            status: CredTestStatus::Error,
            error_detail: Some("Timed out".to_string()),
        }];

        let report = build_report_text(&results, Some(&test_localize));
        assert!(report.contains("Timed out"));
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
}
