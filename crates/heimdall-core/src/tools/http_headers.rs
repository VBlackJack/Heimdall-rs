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

//! HTTP header analyzer engine: URL normalization, HTTP response header parsing,
//! security posture evaluation, information disclosure detection, grading, and text reports.

use std::fmt::Write as _;

/// Standard HTTP default port.
pub const DEFAULT_HTTP_PORT: u16 = 80;
/// Standard HTTPS default port.
pub const DEFAULT_HTTPS_PORT: u16 = 443;
/// Maximum bytes to read from the response header section.
pub const MAX_RESPONSE_BYTES: usize = 16384;

/// Standard security header names.
pub const HEADER_HSTS: &str = "Strict-Transport-Security";
pub const HEADER_CSP: &str = "Content-Security-Policy";
pub const HEADER_XFO: &str = "X-Frame-Options";
pub const HEADER_XCTO: &str = "X-Content-Type-Options";
pub const HEADER_REFERRER: &str = "Referrer-Policy";
pub const HEADER_PERMISSIONS: &str = "Permissions-Policy";
pub const HEADER_XXSS: &str = "X-XSS-Protection";
pub const HEADER_SET_COOKIE: &str = "Set-Cookie";

/// Disclosure header names.
pub const HEADER_SERVER: &str = "Server";
pub const HEADER_X_POWERED_BY: &str = "X-Powered-By";
pub const HEADER_X_ASPNET_VERSION: &str = "X-AspNet-Version";

/// Expected value for X-Content-Type-Options.
const NOSNIFF_VALUE: &str = "nosniff";

/// Status classification of an individual header security check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum HeaderCheckStatus {
    /// Recommended security header present and correctly configured.
    Pass,
    /// Header present but with deprecated, weak, or suboptimal settings.
    Warn,
    /// Header completely absent or dangerously configured.
    Fail,
}

/// Overall security grade assigned to the target site.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum HttpGrade {
    /// Perfect score, all key security headers enforced.
    APlus,
    /// Excellent score, almost all headers enforced.
    A,
    /// Good score with minor omissions.
    BPlus,
    /// Fair score, multiple headers missing.
    B,
    /// Moderate security posture, basic headers missing.
    C,
    /// Poor posture, missing almost all modern protections.
    D,
    /// Critical failure, no modern defenses detected.
    F,
}

impl HttpGrade {
    /// Standard display text for the letter grade.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::APlus => "A+",
            Self::A => "A",
            Self::BPlus => "B+",
            Self::B => "B",
            Self::C => "C",
            Self::D => "D",
            Self::F => "F",
        }
    }
}

/// Result of evaluating a specific header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeaderCheckResult {
    /// Header name (e.g. "Strict-Transport-Security").
    pub name: String,
    /// Display name key for localization.
    pub display_key: &'static str,
    /// Raw value from the response, or empty if absent.
    pub value: String,
    /// Status outcome (Pass, Warn, Fail).
    pub status: HeaderCheckStatus,
    /// Localization key for recommendation, if applicable.
    pub recommendation_key: Option<&'static str>,
    /// Optional dynamic detail message (e.g. missing cookie flags).
    pub detail: Option<String>,
}

/// Parsed HTTP response information.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpResponseInfo {
    /// HTTP status code (e.g. 200, 301, 404).
    pub status_code: u16,
    /// Parsed headers as key-value pairs (preserving order).
    pub headers: Vec<(String, String)>,
    /// Raw header section text.
    pub header_section: String,
}

/// Full evaluation outcome for an HTTP response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeaderEvaluation {
    /// Evaluated security headers.
    pub security_checks: Vec<HeaderCheckResult>,
    /// Evaluated information disclosure headers.
    pub disclosure_checks: Vec<HeaderCheckResult>,
    /// Computed letter grade.
    pub grade: HttpGrade,
}

/// Normalizes raw user input into an absolute HTTP or HTTPS URL.
pub fn try_normalize_url(input: &str) -> Result<String, &'static str> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("ui-tool-headers-error-url-required");
    }

    if trimmed.contains("://")
        && !trimmed.to_ascii_lowercase().starts_with("http://")
        && !trimmed.to_ascii_lowercase().starts_with("https://")
    {
        return Err("ui-tool-headers-error-invalid-url");
    }

    let url_str = if !trimmed.to_ascii_lowercase().starts_with("http://")
        && !trimmed.to_ascii_lowercase().starts_with("https://")
    {
        format!("https://{trimmed}")
    } else {
        trimmed.to_string()
    };

    if !url_str.contains('.') && !url_str.contains("localhost") {
        return Err("ui-tool-headers-error-invalid-url");
    }

    Ok(url_str)
}

/// Parses a raw HTTP response string into status code, headers, and the header section.
#[must_use]
pub fn parse_http_response(raw_response: &str) -> HttpResponseInfo {
    if raw_response.is_empty() {
        return HttpResponseInfo {
            status_code: 0,
            headers: Vec::new(),
            header_section: String::new(),
        };
    }

    let header_section = if let Some(pos) = raw_response.find("\r\n\r\n") {
        &raw_response[..pos]
    } else if let Some(pos) = raw_response.find("\n\n") {
        &raw_response[..pos]
    } else {
        raw_response
    };

    let mut status_code = 0u16;
    let mut headers = Vec::new();
    let mut lines = header_section.lines();

    if let Some(first_line) = lines.next() {
        let parts: Vec<&str> = first_line.split_whitespace().collect();
        if parts.len() >= 2 {
            if let Ok(code) = parts[1].parse::<u16>() {
                status_code = code;
            }
        }
    }

    for line in lines {
        if let Some(colon) = line.find(':') {
            let name = line[..colon].trim().to_string();
            let value = line[colon + 1..].trim().to_string();
            if !name.is_empty() {
                headers.push((name, value));
            }
        }
    }

    HttpResponseInfo {
        status_code,
        headers,
        header_section: header_section.to_string(),
    }
}

/// Looks up the first header matching `name` (case-insensitive).
fn find_header<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    for (k, v) in headers {
        if k.eq_ignore_ascii_case(name) {
            return Some(v.as_str());
        }
    }
    None
}

/// Evaluates security headers against modern web defense standards.
#[must_use]
pub fn evaluate_security_headers(headers: &[(String, String)]) -> Vec<HeaderCheckResult> {
    let mut results = Vec::new();

    // 1. HSTS
    if let Some(val) = find_header(headers, HEADER_HSTS) {
        results.push(HeaderCheckResult {
            name: HEADER_HSTS.to_string(),
            display_key: "ui-tool-headers-hsts",
            value: val.to_string(),
            status: HeaderCheckStatus::Pass,
            recommendation_key: None,
            detail: None,
        });
    } else {
        results.push(HeaderCheckResult {
            name: HEADER_HSTS.to_string(),
            display_key: "ui-tool-headers-hsts",
            value: String::new(),
            status: HeaderCheckStatus::Fail,
            recommendation_key: Some("ui-tool-headers-rec-hsts"),
            detail: None,
        });
    }

    // 2. CSP
    if let Some(val) = find_header(headers, HEADER_CSP) {
        results.push(HeaderCheckResult {
            name: HEADER_CSP.to_string(),
            display_key: "ui-tool-headers-csp",
            value: val.to_string(),
            status: HeaderCheckStatus::Pass,
            recommendation_key: None,
            detail: None,
        });
    } else {
        results.push(HeaderCheckResult {
            name: HEADER_CSP.to_string(),
            display_key: "ui-tool-headers-csp",
            value: String::new(),
            status: HeaderCheckStatus::Fail,
            recommendation_key: Some("ui-tool-headers-rec-csp"),
            detail: None,
        });
    }

    // 3. XFO
    if let Some(val) = find_header(headers, HEADER_XFO) {
        results.push(HeaderCheckResult {
            name: HEADER_XFO.to_string(),
            display_key: "ui-tool-headers-xfo",
            value: val.to_string(),
            status: HeaderCheckStatus::Pass,
            recommendation_key: None,
            detail: None,
        });
    } else {
        results.push(HeaderCheckResult {
            name: HEADER_XFO.to_string(),
            display_key: "ui-tool-headers-xfo",
            value: String::new(),
            status: HeaderCheckStatus::Fail,
            recommendation_key: Some("ui-tool-headers-rec-xfo"),
            detail: None,
        });
    }

    // 4. XCTO (nosniff)
    if let Some(val) = find_header(headers, HEADER_XCTO) {
        let is_nosniff = val.eq_ignore_ascii_case(NOSNIFF_VALUE);
        results.push(HeaderCheckResult {
            name: HEADER_XCTO.to_string(),
            display_key: "ui-tool-headers-xcto",
            value: val.to_string(),
            status: if is_nosniff {
                HeaderCheckStatus::Pass
            } else {
                HeaderCheckStatus::Warn
            },
            recommendation_key: if is_nosniff {
                None
            } else {
                Some("ui-tool-headers-rec-xcto")
            },
            detail: None,
        });
    } else {
        results.push(HeaderCheckResult {
            name: HEADER_XCTO.to_string(),
            display_key: "ui-tool-headers-xcto",
            value: String::new(),
            status: HeaderCheckStatus::Fail,
            recommendation_key: Some("ui-tool-headers-rec-xcto"),
            detail: None,
        });
    }

    // 5. Referrer-Policy
    if let Some(val) = find_header(headers, HEADER_REFERRER) {
        results.push(HeaderCheckResult {
            name: HEADER_REFERRER.to_string(),
            display_key: "ui-tool-headers-referrer",
            value: val.to_string(),
            status: HeaderCheckStatus::Pass,
            recommendation_key: None,
            detail: None,
        });
    } else {
        results.push(HeaderCheckResult {
            name: HEADER_REFERRER.to_string(),
            display_key: "ui-tool-headers-referrer",
            value: String::new(),
            status: HeaderCheckStatus::Fail,
            recommendation_key: Some("ui-tool-headers-rec-referrer"),
            detail: None,
        });
    }

    // 6. Permissions-Policy
    if let Some(val) = find_header(headers, HEADER_PERMISSIONS) {
        results.push(HeaderCheckResult {
            name: HEADER_PERMISSIONS.to_string(),
            display_key: "ui-tool-headers-permissions",
            value: val.to_string(),
            status: HeaderCheckStatus::Pass,
            recommendation_key: None,
            detail: None,
        });
    } else {
        results.push(HeaderCheckResult {
            name: HEADER_PERMISSIONS.to_string(),
            display_key: "ui-tool-headers-permissions",
            value: String::new(),
            status: HeaderCheckStatus::Fail,
            recommendation_key: Some("ui-tool-headers-rec-permissions"),
            detail: None,
        });
    }

    // 7. X-XSS-Protection (modern best practice is 0 or absent, 1; mode=block is deprecated)
    if let Some(val) = find_header(headers, HEADER_XXSS) {
        let trimmed = val.trim();
        let (status, rec) = if trimmed == "0" {
            (HeaderCheckStatus::Pass, None)
        } else {
            (HeaderCheckStatus::Warn, Some("ui-tool-headers-rec-xxss"))
        };
        results.push(HeaderCheckResult {
            name: HEADER_XXSS.to_string(),
            display_key: "ui-tool-headers-xxss",
            value: val.to_string(),
            status,
            recommendation_key: rec,
            detail: None,
        });
    } else {
        results.push(HeaderCheckResult {
            name: HEADER_XXSS.to_string(),
            display_key: "ui-tool-headers-xxss",
            value: String::new(),
            status: HeaderCheckStatus::Pass,
            recommendation_key: None,
            detail: None,
        });
    }

    // 8. Set-Cookie flags (Secure, HttpOnly, SameSite)
    let cookie_headers: Vec<&str> = headers
        .iter()
        .filter(|(k, _)| k.eq_ignore_ascii_case(HEADER_SET_COOKIE))
        .map(|(_, v)| v.as_str())
        .collect();

    if cookie_headers.is_empty() {
        results.push(HeaderCheckResult {
            name: HEADER_SET_COOKIE.to_string(),
            display_key: "ui-tool-headers-cookies",
            value: String::new(),
            status: HeaderCheckStatus::Pass,
            recommendation_key: None,
            detail: None,
        });
    } else {
        let mut missing_flags = Vec::new();
        for cookie in &cookie_headers {
            let lower = cookie.to_ascii_lowercase();
            if !lower.contains("secure") && !missing_flags.contains(&"Secure") {
                missing_flags.push("Secure");
            }
            if !lower.contains("httponly") && !missing_flags.contains(&"HttpOnly") {
                missing_flags.push("HttpOnly");
            }
            if !lower.contains("samesite") && !missing_flags.contains(&"SameSite") {
                missing_flags.push("SameSite");
            }
        }

        if missing_flags.is_empty() {
            results.push(HeaderCheckResult {
                name: HEADER_SET_COOKIE.to_string(),
                display_key: "ui-tool-headers-cookies",
                value: cookie_headers.join("\n"),
                status: HeaderCheckStatus::Pass,
                recommendation_key: None,
                detail: None,
            });
        } else {
            let detail_text = format!("Missing flags: {}", missing_flags.join(", "));
            results.push(HeaderCheckResult {
                name: HEADER_SET_COOKIE.to_string(),
                display_key: "ui-tool-headers-cookies",
                value: cookie_headers.join("\n"),
                status: HeaderCheckStatus::Warn,
                recommendation_key: Some("ui-tool-headers-rec-cookie"),
                detail: Some(detail_text),
            });
        }
    }

    results
}

/// Evaluates disclosure-oriented headers such as Server, X-Powered-By, X-AspNet-Version.
#[must_use]
pub fn evaluate_disclosure_headers(headers: &[(String, String)]) -> Vec<HeaderCheckResult> {
    let mut results = Vec::new();

    let check_disclosure = |name: &str, display_key: &'static str| -> HeaderCheckResult {
        if let Some(val) = find_header(headers, name) {
            HeaderCheckResult {
                name: name.to_string(),
                display_key,
                value: val.to_string(),
                status: HeaderCheckStatus::Warn,
                recommendation_key: Some("ui-tool-headers-rec-disclosure"),
                detail: None,
            }
        } else {
            HeaderCheckResult {
                name: name.to_string(),
                display_key,
                value: String::new(),
                status: HeaderCheckStatus::Pass,
                recommendation_key: None,
                detail: None,
            }
        }
    };

    results.push(check_disclosure(HEADER_SERVER, "ui-tool-headers-server"));
    results.push(check_disclosure(
        HEADER_X_POWERED_BY,
        "ui-tool-headers-x-powered-by",
    ));
    results.push(check_disclosure(
        HEADER_X_ASPNET_VERSION,
        "ui-tool-headers-x-aspnet-version",
    ));

    results
}

/// Computes the overall letter grade from the evaluated security checks.
#[must_use]
pub fn calculate_grade(security_results: &[HeaderCheckResult]) -> HttpGrade {
    if security_results.is_empty() {
        return HttpGrade::F;
    }

    let pass_count = security_results
        .iter()
        .filter(|r| r.status == HeaderCheckStatus::Pass)
        .count();
    let warn_count = security_results
        .iter()
        .filter(|r| r.status == HeaderCheckStatus::Warn)
        .count();
    let fail_count = security_results
        .iter()
        .filter(|r| r.status == HeaderCheckStatus::Fail)
        .count();
    let total = security_results.len();

    if fail_count == 0 && warn_count == 0 {
        return HttpGrade::APlus;
    }

    if fail_count == 0 && warn_count <= 1 {
        return HttpGrade::A;
    }

    let score = (pass_count as f64) / (total as f64);
    if score >= 0.85 {
        HttpGrade::BPlus
    } else if score >= 0.75 {
        HttpGrade::B
    } else if score >= 0.60 {
        HttpGrade::C
    } else if score >= 0.45 {
        HttpGrade::D
    } else {
        HttpGrade::F
    }
}

/// Evaluates full header section and computes final report and letter grade.
#[must_use]
pub fn evaluate_headers(headers: &[(String, String)]) -> HeaderEvaluation {
    let security_checks = evaluate_security_headers(headers);
    let disclosure_checks = evaluate_disclosure_headers(headers);
    let grade = calculate_grade(&security_checks);

    HeaderEvaluation {
        security_checks,
        disclosure_checks,
        grade,
    }
}

/// Builds an exportable plain-text report of the header analysis.
#[must_use]
pub fn build_text_report(
    target_url: &str,
    evaluation: &HeaderEvaluation,
    raw_response: &str,
) -> String {
    let mut report = String::new();
    let _ = writeln!(report, "=== HTTP Header Security Report ===");
    let _ = writeln!(report, "Target: {target_url}");
    let _ = writeln!(report, "Grade: {}", evaluation.grade.label());
    let _ = writeln!(report);

    let _ = writeln!(report, "--- Security Headers ---");
    for check in &evaluation.security_checks {
        let status_str = match check.status {
            HeaderCheckStatus::Pass => "[PASS]",
            HeaderCheckStatus::Warn => "[WARN]",
            HeaderCheckStatus::Fail => "[FAIL]",
        };
        let val = if check.value.is_empty() {
            "(missing)"
        } else {
            check.value.as_str()
        };
        let _ = writeln!(report, "{status_str} {}: {val}", check.name);
        if let Some(detail) = &check.detail {
            let _ = writeln!(report, "  Detail: {detail}");
        }
    }
    let _ = writeln!(report);

    let _ = writeln!(report, "--- Information Disclosure ---");
    for check in &evaluation.disclosure_checks {
        let status_str = match check.status {
            HeaderCheckStatus::Pass => "[CLEAN]",
            HeaderCheckStatus::Warn => "[EXPOSED]",
            HeaderCheckStatus::Fail => "[FAIL]",
        };
        let val = if check.value.is_empty() {
            "(not exposed)"
        } else {
            check.value.as_str()
        };
        let _ = writeln!(report, "{status_str} {}: {val}", check.name);
    }
    let _ = writeln!(report);

    let _ = writeln!(report, "--- Raw Response Headers ---");
    report.push_str(raw_response);

    report
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_try_normalize_url() {
        assert_eq!(
            try_normalize_url("example.com").unwrap(),
            "https://example.com"
        );
        assert_eq!(
            try_normalize_url("http://example.com").unwrap(),
            "http://example.com"
        );
        assert!(try_normalize_url("   ").is_err());
        assert!(try_normalize_url("ftp://example.com").is_err());
    }

    #[test]
    fn test_parse_http_response() {
        let raw = "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nServer: nginx\r\n\r\n<html>";
        let parsed = parse_http_response(raw);
        assert_eq!(parsed.status_code, 200);
        assert_eq!(parsed.headers.len(), 2);
        assert_eq!(
            parsed.headers[0],
            ("Content-Type".into(), "text/html".into())
        );
        assert_eq!(parsed.headers[1], ("Server".into(), "nginx".into()));
    }

    #[test]
    fn test_calculate_grade_all_pass() {
        let results = vec![
            HeaderCheckResult {
                name: "HSTS".into(),
                display_key: "hsts",
                value: "val".into(),
                status: HeaderCheckStatus::Pass,
                recommendation_key: None,
                detail: None,
            },
            HeaderCheckResult {
                name: "CSP".into(),
                display_key: "csp",
                value: "val".into(),
                status: HeaderCheckStatus::Pass,
                recommendation_key: None,
                detail: None,
            },
        ];
        assert_eq!(calculate_grade(&results), HttpGrade::APlus);
    }

    #[test]
    fn test_calculate_grade_all_fail() {
        let results = vec![HeaderCheckResult {
            name: "HSTS".into(),
            display_key: "hsts",
            value: String::new(),
            status: HeaderCheckStatus::Fail,
            recommendation_key: None,
            detail: None,
        }];
        assert_eq!(calculate_grade(&results), HttpGrade::F);
    }

    #[test]
    fn test_evaluate_headers_disclosure() {
        let headers = vec![("Server".to_string(), "Apache/2.4.41".to_string())];
        let eval = evaluate_headers(&headers);
        let srv = eval
            .disclosure_checks
            .iter()
            .find(|c| c.name == HEADER_SERVER)
            .unwrap();
        assert_eq!(srv.status, HeaderCheckStatus::Warn);
        assert_eq!(srv.value, "Apache/2.4.41");
    }
}
