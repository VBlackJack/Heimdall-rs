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

//! HTTP header analyzer engine, as the C# `HttpHeaderEvaluationEngine`: the target URL,
//! the request, the response header parsing, security posture evaluation, information
//! disclosure detection, grading, and text reports.
//!
//! The request is sent by Heimdall's own client, as the C# `HttpHeaderService` sends it over
//! its own `TcpClient`: a `HEAD`, then a `GET` when the server refuses it with 405, of which
//! at most [`MAX_RESPONSE_BYTES`] are read. That client, in `heimdall_app::net_tools`,
//! checks an HTTPS server's certificate.

use std::fmt::Write as _;

use super::net_host;

/// Standard HTTP default port.
pub const DEFAULT_HTTP_PORT: u16 = 80;
/// Standard HTTPS default port.
pub const DEFAULT_HTTPS_PORT: u16 = 443;
/// Maximum bytes to read from the response header section, as the C#
/// `HttpHeaderEvaluationEngine.MaxResponseBytes`.
pub const MAX_RESPONSE_BYTES: usize = 16384;
/// How long the whole fetch may take, as the C# `HttpHeaderService.ConnectionTimeout`.
pub const FETCH_TIMEOUT_MS: u64 = 15000;
/// The status a server refuses a `HEAD` with, when the request is sent again as a `GET`.
pub const METHOD_NOT_ALLOWED: u16 = 405;
/// The end of a response's header section.
pub const HEADER_END: &[u8] = b"\r\n\r\n";
/// The user agent the request names, as the C#.
pub const USER_AGENT: &str = "Heimdall";

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
    /// Localization key of why a header present is not a pass: its value, not its name, is
    /// what is wrong (`max-age=0`, an empty policy, an unknown value).
    pub reason_key: Option<&'static str>,
    /// The cookie flags missing, comma-separated, for the cookie check: the view says it
    /// in the user's language.
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

/// The server and the resource the analyzer asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpTarget {
    /// Over TLS.
    pub https: bool,
    /// The host, checked by [`net_host::checked_host`]; an IPv6 address without brackets.
    pub host: String,
    /// The port.
    pub port: u16,
    /// The path and query, never empty.
    pub path: String,
}

impl HttpTarget {
    /// The target as a URL, for the report.
    #[must_use]
    pub fn url(&self) -> String {
        let scheme = if self.https { "https" } else { "http" };
        format!("{scheme}://{}{}", self.authority(), self.path)
    }

    /// The host, an IPv6 address in brackets, and the port unless it is the scheme's own.
    fn authority(&self) -> String {
        let default_port = if self.https {
            DEFAULT_HTTPS_PORT
        } else {
            DEFAULT_HTTP_PORT
        };
        let host = if self.host.contains(':') {
            format!("[{}]", self.host)
        } else {
            self.host.clone()
        };
        if self.port == default_port {
            host
        } else {
            format!("{host}:{}", self.port)
        }
    }

    /// The `method` request for this target, as the C# `SendRequestAsync` writes it; its
    /// `Host` names the port when it is not the scheme's own, as RFC 9112 asks.
    #[must_use]
    pub fn request(&self, method: &str) -> String {
        format!(
            "{method} {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\nUser-Agent: {USER_AGENT}\r\n\r\n",
            self.path,
            self.authority()
        )
    }
}

/// Reads the typed URL as the C# `TryNormalizeUrl`: `https://` added when no scheme is
/// given, any scheme but HTTP and HTTPS refused. The host must be one
/// [`net_host::checked_host`] takes, the port a number, and nothing in the URL a space or a
/// control character, so the request cannot be given a line of its own. User information
/// (`user@`) is refused rather than sent nowhere. The fragment is dropped.
///
/// # Errors
///
/// A translation key: the URL is empty, or not one the analyzer can ask.
pub fn parse_target(input: &str) -> Result<HttpTarget, &'static str> {
    const INVALID: &str = "ui-tool-headers-error-invalid-url";
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("ui-tool-headers-error-url-required");
    }
    if trimmed.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err(INVALID);
    }
    let lower = trimmed.to_ascii_lowercase();
    let (https, rest) = if lower.starts_with("https://") {
        (true, &trimmed["https://".len()..])
    } else if lower.starts_with("http://") {
        (false, &trimmed["http://".len()..])
    } else if trimmed.contains("://") {
        return Err(INVALID);
    } else {
        (true, trimmed)
    };
    let rest = rest.split('#').next().unwrap_or_default();
    let split = rest.find(['/', '?']).unwrap_or(rest.len());
    let (authority, path) = rest.split_at(split);
    if authority.contains('@') {
        return Err(INVALID);
    }
    let (host_text, port_text) = if let Some(bracketed) = authority.strip_prefix('[') {
        let (inner, after) = bracketed.split_once(']').ok_or(INVALID)?;
        let port = match after {
            "" => None,
            _ => Some(after.strip_prefix(':').ok_or(INVALID)?),
        };
        (format!("[{inner}]"), port)
    } else {
        match authority.split_once(':') {
            Some((host, port)) => (host.to_owned(), Some(port)),
            None => (authority.to_owned(), None),
        }
    };
    let host = net_host::checked_host(&host_text).ok_or(INVALID)?;
    let port = match port_text {
        None => {
            if https {
                DEFAULT_HTTPS_PORT
            } else {
                DEFAULT_HTTP_PORT
            }
        }
        Some(text) => match text.parse::<u16>() {
            Ok(port) if port > 0 => port,
            _ => return Err(INVALID),
        },
    };
    let path = if path.starts_with('/') {
        path.to_owned()
    } else {
        format!("/{path}")
    };
    Ok(HttpTarget {
        https,
        host,
        port,
        path,
    })
}

/// Where the header section of `received` ends, the blank line after it included; `None`
/// while it has not ended. A line may end with a bare LF, as RFC 9112 section 2.2 lets a
/// recipient read it, so `\n\n` ends it as `\r\n\r\n` does.
#[must_use]
pub fn header_end(received: &[u8]) -> Option<usize> {
    received
        .iter()
        .enumerate()
        .filter(|(_, byte)| **byte == b'\n')
        .find_map(|(index, _)| match received.get(index + 1..) {
            Some([b'\n', ..]) => Some(index + 2),
            Some([b'\r', b'\n', ..]) => Some(index + 3),
            _ => None,
        })
}

/// The informational statuses, 1xx (RFC 9110 section 15.2).
const INFORMATIONAL: std::ops::RangeInclusive<u16> = 100..=199;

/// `101 Switching Protocols`: informational, yet the last response on its connection.
const SWITCHING_PROTOCOLS: u16 = 101;

/// The status code of the response whose header section is `head`; `None` when its first
/// line names none.
#[must_use]
pub fn status_code(head: &[u8]) -> Option<u16> {
    let line = head.split(|byte| *byte == b'\n').next()?;
    let code = line
        .split(u8::is_ascii_whitespace)
        .find(|word| !word.is_empty() && !word.starts_with(b"HTTP/"))?;
    std::str::from_utf8(code).ok()?.parse().ok()
}

/// Whether a response of `code` is an interim one a final response follows on the same
/// connection, as `103 Early Hints` or `100 Continue`: every 1xx but `101`.
#[must_use]
pub fn is_interim(code: u16) -> bool {
    INFORMATIONAL.contains(&code) && code != SWITCHING_PROTOCOLS
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

/// What a header present is worth: its status, and the translation key of why when it is
/// not a pass.
type Verdict = (HeaderCheckStatus, Option<&'static str>);

/// The check of header `name`: absent, a failure with `recommendation_key`; present, what
/// `verdict` makes of its value, the recommendation kept unless it passes. The C#
/// `CheckPresence` passes any value, `max-age=0` and `X-Frame-Options: nonsense` included.
fn header_check(
    headers: &[(String, String)],
    name: &str,
    display_key: &'static str,
    recommendation_key: &'static str,
    verdict: impl Fn(&str) -> Verdict,
) -> HeaderCheckResult {
    let (value, status, reason_key) = match find_header(headers, name) {
        Some(value) => {
            let (status, reason) = verdict(value);
            (value.to_string(), status, reason)
        }
        None => (String::new(), HeaderCheckStatus::Fail, None),
    };
    HeaderCheckResult {
        name: name.to_string(),
        display_key,
        value,
        status,
        recommendation_key: (status != HeaderCheckStatus::Pass).then_some(recommendation_key),
        reason_key,
        detail: None,
    }
}

/// The shortest `max-age` that passes: six months, the threshold of Mozilla's Observatory.
pub const HSTS_MIN_MAX_AGE_SECONDS: u64 = 15_768_000;

/// The `Strict-Transport-Security` value: read over HTTPS only, as RFC 6797 section 8.1 has
/// browsers ignore it over plain HTTP; its one `max-age` a number (section 6.1), of which 0
/// removes the policy (section 6.1.1).
fn hsts_verdict(value: &str, https: bool) -> Verdict {
    if !https {
        return (
            HeaderCheckStatus::Warn,
            Some("ui-tool-headers-reason-hsts-http"),
        );
    }
    match hsts_max_age(value) {
        None => (
            HeaderCheckStatus::Fail,
            Some("ui-tool-headers-reason-hsts-max-age-invalid"),
        ),
        Some(0) => (
            HeaderCheckStatus::Fail,
            Some("ui-tool-headers-reason-hsts-max-age-zero"),
        ),
        Some(age) if age < HSTS_MIN_MAX_AGE_SECONDS => (
            HeaderCheckStatus::Warn,
            Some("ui-tool-headers-reason-hsts-max-age-short"),
        ),
        Some(_) => (HeaderCheckStatus::Pass, None),
    }
}

/// The `max-age` of an HSTS value, its directive named once and its value digits, quoted
/// or not; `None` otherwise, which a browser ignores the header for. A number too large
/// reads as the largest.
#[must_use]
pub fn hsts_max_age(value: &str) -> Option<u64> {
    let mut ages = value.split(';').filter_map(|directive| {
        let (name, age) = directive.split_once('=').unwrap_or((directive, ""));
        name.trim().eq_ignore_ascii_case("max-age").then(|| {
            let age = age.trim();
            let age = age
                .strip_prefix('"')
                .and_then(|quoted| quoted.strip_suffix('"'))
                .unwrap_or(age);
            (!age.is_empty() && age.bytes().all(|byte| byte.is_ascii_digit()))
                .then(|| age.parse::<u64>().unwrap_or(u64::MAX))
        })
    });
    match (ages.next(), ages.next()) {
        (Some(age), None) => age,
        _ => None,
    }
}

/// The `Content-Security-Policy` value: an empty policy enforces nothing; the directive
/// that governs scripts (`script-src`, else `default-src`) is looked for, and an
/// `'unsafe-inline'` without a nonce nor a hash beside it, or an `'unsafe-eval'`, in it.
fn csp_verdict(value: &str) -> Verdict {
    if value.trim().is_empty() {
        return (
            HeaderCheckStatus::Fail,
            Some("ui-tool-headers-reason-csp-empty"),
        );
    }
    let directive = |wanted: &str| {
        value.split(';').find_map(|directive| {
            let mut words = directive.split_ascii_whitespace();
            words
                .next()
                .is_some_and(|name| name.eq_ignore_ascii_case(wanted))
                .then(|| words.map(str::to_ascii_lowercase).collect::<Vec<_>>())
        })
    };
    let Some(sources) = directive("script-src").or_else(|| directive("default-src")) else {
        return (
            HeaderCheckStatus::Warn,
            Some("ui-tool-headers-reason-csp-no-script"),
        );
    };
    let has = |source: &str| sources.iter().any(|word| word == source);
    let nonce_or_hash = sources.iter().any(|word| {
        ["'nonce-", "'sha256-", "'sha384-", "'sha512-"]
            .iter()
            .any(|prefix| word.starts_with(prefix))
    });
    if (has("'unsafe-inline'") && !nonce_or_hash) || has("'unsafe-eval'") {
        (
            HeaderCheckStatus::Warn,
            Some("ui-tool-headers-reason-csp-unsafe"),
        )
    } else {
        (HeaderCheckStatus::Pass, None)
    }
}

/// The `X-Frame-Options` value, as the HTML standard reads it: its comma-separated values
/// all `DENY` or all `SAMEORIGIN`, case aside; `ALLOW-FROM`, which no current browser
/// applies, a warning; anything else is ignored by browsers, a failure.
fn xfo_verdict(value: &str) -> Verdict {
    let mut values: Vec<String> = value
        .split(',')
        .map(|part| part.trim().to_ascii_lowercase())
        .collect();
    values.dedup();
    match values.as_slice() {
        [only] if only == "deny" || only == "sameorigin" => (HeaderCheckStatus::Pass, None),
        [only] if only.starts_with("allow-from") => (
            HeaderCheckStatus::Warn,
            Some("ui-tool-headers-reason-xfo-allow-from"),
        ),
        _ => (
            HeaderCheckStatus::Fail,
            Some("ui-tool-headers-reason-xfo-invalid"),
        ),
    }
}

/// The `X-Content-Type-Options` value, as the Fetch standard reads it: its first value
/// `nosniff`, case aside; anything else is ignored by browsers, a failure, where the C#
/// warns.
fn xcto_verdict(value: &str) -> Verdict {
    let first = value.split(',').next().unwrap_or_default().trim();
    if first.eq_ignore_ascii_case(NOSNIFF_VALUE) {
        (HeaderCheckStatus::Pass, None)
    } else {
        (
            HeaderCheckStatus::Fail,
            Some("ui-tool-headers-reason-xcto-invalid"),
        )
    }
}

/// The policies `Referrer-Policy` may name.
const REFERRER_POLICIES: [&str; 8] = [
    "no-referrer",
    "no-referrer-when-downgrade",
    "same-origin",
    "origin",
    "strict-origin",
    "origin-when-cross-origin",
    "strict-origin-when-cross-origin",
    "unsafe-url",
];

/// The policies that send the whole URL to other sites.
const LEAKING_REFERRER_POLICIES: [&str; 2] = ["unsafe-url", "no-referrer-when-downgrade"];

/// The `Referrer-Policy` value, as the Referrer Policy standard reads it: the last policy
/// of the list a browser knows applies; none known, the header is ignored, a failure; a
/// policy sending the whole URL to other sites, a warning.
fn referrer_verdict(value: &str) -> Verdict {
    let policy = value
        .split(',')
        .map(|part| part.trim().to_ascii_lowercase())
        .filter(|part| REFERRER_POLICIES.contains(&part.as_str()))
        .last();
    match policy {
        None => (
            HeaderCheckStatus::Fail,
            Some("ui-tool-headers-reason-referrer-invalid"),
        ),
        Some(policy) if LEAKING_REFERRER_POLICIES.contains(&policy.as_str()) => (
            HeaderCheckStatus::Warn,
            Some("ui-tool-headers-reason-referrer-unsafe"),
        ),
        Some(_) => (HeaderCheckStatus::Pass, None),
    }
}

/// The `Permissions-Policy` value: an empty one restricts nothing.
fn permissions_verdict(value: &str) -> Verdict {
    if value.trim().is_empty() {
        (
            HeaderCheckStatus::Fail,
            Some("ui-tool-headers-reason-permissions-empty"),
        )
    } else {
        (HeaderCheckStatus::Pass, None)
    }
}

/// Evaluates security headers against modern web defense standards; `https` when they
/// came over TLS, without which a browser ignores `Strict-Transport-Security`.
#[must_use]
pub fn evaluate_security_headers(
    headers: &[(String, String)],
    https: bool,
) -> Vec<HeaderCheckResult> {
    let mut results = Vec::new();

    results.push(header_check(
        headers,
        HEADER_HSTS,
        "ui-tool-headers-hsts",
        "ui-tool-headers-rec-hsts",
        |value| hsts_verdict(value, https),
    ));
    results.push(header_check(
        headers,
        HEADER_CSP,
        "ui-tool-headers-csp",
        "ui-tool-headers-rec-csp",
        csp_verdict,
    ));
    results.push(header_check(
        headers,
        HEADER_XFO,
        "ui-tool-headers-xfo",
        "ui-tool-headers-rec-xfo",
        xfo_verdict,
    ));
    results.push(header_check(
        headers,
        HEADER_XCTO,
        "ui-tool-headers-xcto",
        "ui-tool-headers-rec-xcto",
        xcto_verdict,
    ));
    results.push(header_check(
        headers,
        HEADER_REFERRER,
        "ui-tool-headers-referrer",
        "ui-tool-headers-rec-referrer",
        referrer_verdict,
    ));
    results.push(header_check(
        headers,
        HEADER_PERMISSIONS,
        "ui-tool-headers-permissions",
        "ui-tool-headers-rec-permissions",
        permissions_verdict,
    ));

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
            reason_key: None,
            detail: None,
        });
    } else {
        results.push(HeaderCheckResult {
            name: HEADER_XXSS.to_string(),
            display_key: "ui-tool-headers-xxss",
            value: String::new(),
            status: HeaderCheckStatus::Pass,
            recommendation_key: None,
            reason_key: None,
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
            reason_key: None,
            detail: None,
        });
    } else {
        let mut missing_flags = Vec::new();
        for cookie in &cookie_headers {
            for flag in COOKIE_FLAGS {
                if !has_cookie_attribute(cookie, flag) && !missing_flags.contains(&flag) {
                    missing_flags.push(flag);
                }
            }
        }

        if missing_flags.is_empty() {
            results.push(HeaderCheckResult {
                name: HEADER_SET_COOKIE.to_string(),
                display_key: "ui-tool-headers-cookies",
                value: cookie_headers.join("\n"),
                status: HeaderCheckStatus::Pass,
                recommendation_key: None,
                reason_key: None,
                detail: None,
            });
        } else {
            let detail_text = missing_flags.join(", ");
            results.push(HeaderCheckResult {
                name: HEADER_SET_COOKIE.to_string(),
                display_key: "ui-tool-headers-cookies",
                value: cookie_headers.join("\n"),
                status: HeaderCheckStatus::Warn,
                recommendation_key: Some("ui-tool-headers-rec-cookie"),
                reason_key: None,
                detail: Some(detail_text),
            });
        }
    }

    results
}

/// The cookie flags every `Set-Cookie` should carry.
const COOKIE_FLAGS: [&str; 3] = ["Secure", "HttpOnly", "SameSite"];

/// Whether `set_cookie` carries the attribute `name`, as RFC 6265 reads its attributes:
/// after the first `;`, each `name` or `name=value`, the name compared without regard to
/// case. A cookie named `insecure_pref` or valued `secure` carries no `Secure`.
#[must_use]
pub fn has_cookie_attribute(set_cookie: &str, name: &str) -> bool {
    set_cookie.split(';').skip(1).any(|attribute| {
        attribute
            .split('=')
            .next()
            .is_some_and(|attribute_name| attribute_name.trim().eq_ignore_ascii_case(name))
    })
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
                reason_key: None,
                detail: None,
            }
        } else {
            HeaderCheckResult {
                name: name.to_string(),
                display_key,
                value: String::new(),
                status: HeaderCheckStatus::Pass,
                recommendation_key: None,
                reason_key: None,
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

/// Evaluates full header section and computes final report and letter grade; `https` when
/// the headers came over TLS.
#[must_use]
pub fn evaluate_headers(headers: &[(String, String)], https: bool) -> HeaderEvaluation {
    let security_checks = evaluate_security_headers(headers, https);
    let disclosure_checks = evaluate_disclosure_headers(headers);
    let grade = calculate_grade(&security_checks);

    HeaderEvaluation {
        security_checks,
        disclosure_checks,
        grade,
    }
}

/// A text of the report, said by the caller in the user's language.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportText<'a> {
    /// The title, naming the target.
    Title(&'a str),
    /// The grade line.
    Grade(&'a str),
    /// The security headers' heading.
    SecurityHeaders,
    /// The information disclosure heading.
    Disclosure,
    /// The raw headers' heading.
    RawHeaders,
    /// A security check's status.
    Status(HeaderCheckStatus),
    /// A disclosure check's status.
    DisclosureStatus(HeaderCheckStatus),
    /// A header absent.
    Missing,
    /// A disclosure header absent.
    NotExposed,
    /// The cookie flags missing.
    MissingFlags(&'a str),
    /// Why a header present is not a pass: the translation key of
    /// [`HeaderCheckResult::reason_key`].
    Reason(&'static str),
}

/// Builds an exportable plain-text report of the header analysis, its words from `say`, as
/// the C# `BuildTextReport` takes its `localize`; `certificate`, when given, is the line
/// saying whether the server's certificate was trusted.
#[must_use]
pub fn build_text_report(
    target_url: &str,
    evaluation: &HeaderEvaluation,
    raw_response: &str,
    certificate: Option<&str>,
    say: &dyn Fn(ReportText<'_>) -> String,
) -> String {
    let mut report = String::new();
    let _ = writeln!(report, "{}", say(ReportText::Title(target_url)));
    let _ = writeln!(
        report,
        "{}",
        say(ReportText::Grade(evaluation.grade.label()))
    );
    if let Some(certificate) = certificate {
        let _ = writeln!(report, "{certificate}");
    }
    let _ = writeln!(report);

    let _ = writeln!(report, "{}", say(ReportText::SecurityHeaders));
    for check in &evaluation.security_checks {
        let val = if check.value.is_empty() {
            say(ReportText::Missing)
        } else {
            check.value.clone()
        };
        let _ = writeln!(
            report,
            "[{}] {}: {val}",
            say(ReportText::Status(check.status)),
            check.name
        );
        if let Some(reason) = check.reason_key {
            let _ = writeln!(report, "  {}", say(ReportText::Reason(reason)));
        }
        if let Some(detail) = &check.detail {
            let _ = writeln!(report, "  {}", say(ReportText::MissingFlags(detail)));
        }
    }
    let _ = writeln!(report);

    let _ = writeln!(report, "{}", say(ReportText::Disclosure));
    for check in &evaluation.disclosure_checks {
        let val = if check.value.is_empty() {
            say(ReportText::NotExposed)
        } else {
            check.value.clone()
        };
        let _ = writeln!(
            report,
            "[{}] {}: {val}",
            say(ReportText::DisclosureStatus(check.status)),
            check.name
        );
    }
    let _ = writeln!(report);

    let _ = writeln!(report, "{}", say(ReportText::RawHeaders));
    report.push_str(raw_response);

    report
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_url_is_read_into_its_target() {
        let target = parse_target("example.com").unwrap();
        assert_eq!(
            target,
            HttpTarget {
                https: true,
                host: "example.com".into(),
                port: DEFAULT_HTTPS_PORT,
                path: "/".into(),
            }
        );
        assert_eq!(target.url(), "https://example.com/");
        let target = parse_target(" HTTP://Example.com:8080/a?b=c#frag ").unwrap();
        assert!(!target.https);
        assert_eq!(target.port, 8080);
        assert_eq!(target.path, "/a?b=c");
        assert_eq!(target.url(), "http://Example.com:8080/a?b=c");
        let target = parse_target("https://[2001:db8::1]:8443?x").unwrap();
        assert_eq!(target.host, "2001:db8::1");
        assert_eq!(target.path, "/?x");
        assert_eq!(target.url(), "https://[2001:db8::1]:8443/?x");
        assert_eq!(
            parse_target("   "),
            Err("ui-tool-headers-error-url-required")
        );
    }

    #[test]
    fn a_url_that_could_bend_the_request_is_refused() {
        for refused in [
            "ftp://example.com",
            "https://example.com/a b",
            "https://example.com/\r\nX-Injected: 1",
            "https://user:pw@example.com/",
            "https://-oProxy=x/",
            "https://example.com:0/",
            "https://example.com:99999/",
            "https://example.com:http/",
            "https://[example.com]/",
            "https://[2001:db8::1]x/",
        ] {
            assert_eq!(
                parse_target(refused),
                Err("ui-tool-headers-error-invalid-url"),
                "{refused:?}"
            );
        }
    }

    #[test]
    fn the_request_is_the_csharp_one() {
        let target = parse_target("http://[::1]:8080/x").unwrap();
        assert_eq!(
            target.request("HEAD"),
            "HEAD /x HTTP/1.1\r\nHost: [::1]:8080\r\nConnection: close\r\nUser-Agent: Heimdall\r\n\r\n"
        );
    }

    #[test]
    fn the_header_section_ends_at_its_blank_line() {
        assert_eq!(header_end(b"HTTP/1.1 200 OK\r\nA: b\r\n\r\nbody"), Some(25));
        assert_eq!(header_end(b"HTTP/1.1 200 OK\r\nA: b\r\n"), None);
        assert_eq!(header_end(b"HTTP/1.1 200 OK\nA: b\n\nbody"), Some(22));
        assert_eq!(header_end(b"HTTP/1.1 200 OK\nA: b\n"), None);
    }

    #[test]
    fn an_interim_response_is_every_1xx_but_switching_protocols() {
        assert_eq!(
            status_code(b"HTTP/1.1 103 Early Hints\r\nLink: </a>\r\n\r\n"),
            Some(103)
        );
        assert_eq!(status_code(b"HTTP/1.1 200 OK"), Some(200));
        assert_eq!(status_code(b"garbage"), None);
        assert!(is_interim(100) && is_interim(103) && is_interim(199));
        assert!(!is_interim(101) && !is_interim(200) && !is_interim(99));
    }

    fn check_of(headers: &[(&str, &str)], https: bool, name: &str) -> HeaderCheckResult {
        let headers: Vec<(String, String)> = headers
            .iter()
            .map(|(name, value)| ((*name).to_string(), (*value).to_string()))
            .collect();
        evaluate_security_headers(&headers, https)
            .into_iter()
            .find(|check| check.name == name)
            .unwrap()
    }

    fn verdict_of(name: &str, value: &str) -> (HeaderCheckStatus, Option<&'static str>) {
        let check = check_of(&[(name, value)], true, name);
        (check.status, check.reason_key)
    }

    #[test]
    fn a_security_header_is_judged_by_its_value_not_its_name() {
        use HeaderCheckStatus::{Fail, Pass, Warn};
        for (name, value, status, reason) in [
            (
                HEADER_HSTS,
                "max-age=31536000; includeSubDomains",
                Pass,
                None,
            ),
            (HEADER_HSTS, "max-age=\"31536000\"", Pass, None),
            (
                HEADER_HSTS,
                "max-age=0",
                Fail,
                Some("ui-tool-headers-reason-hsts-max-age-zero"),
            ),
            (
                HEADER_HSTS,
                "max-age=300",
                Warn,
                Some("ui-tool-headers-reason-hsts-max-age-short"),
            ),
            (
                HEADER_HSTS,
                "includeSubDomains",
                Fail,
                Some("ui-tool-headers-reason-hsts-max-age-invalid"),
            ),
            (
                HEADER_HSTS,
                "max-age=abc",
                Fail,
                Some("ui-tool-headers-reason-hsts-max-age-invalid"),
            ),
            (
                HEADER_HSTS,
                "max-age=1; max-age=31536000",
                Fail,
                Some("ui-tool-headers-reason-hsts-max-age-invalid"),
            ),
            (
                HEADER_CSP,
                "",
                Fail,
                Some("ui-tool-headers-reason-csp-empty"),
            ),
            (HEADER_CSP, "default-src 'self'", Pass, None),
            (
                HEADER_CSP,
                "script-src 'self' 'unsafe-inline'",
                Warn,
                Some("ui-tool-headers-reason-csp-unsafe"),
            ),
            (
                HEADER_CSP,
                "script-src 'self' 'unsafe-inline' 'nonce-abc'",
                Pass,
                None,
            ),
            (
                HEADER_CSP,
                "default-src 'self' 'UNSAFE-EVAL'",
                Warn,
                Some("ui-tool-headers-reason-csp-unsafe"),
            ),
            (
                HEADER_CSP,
                "frame-ancestors 'none'",
                Warn,
                Some("ui-tool-headers-reason-csp-no-script"),
            ),
            (HEADER_XFO, "DENY", Pass, None),
            (HEADER_XFO, "sameorigin, SAMEORIGIN", Pass, None),
            (
                HEADER_XFO,
                "ALLOW-FROM https://a.example",
                Warn,
                Some("ui-tool-headers-reason-xfo-allow-from"),
            ),
            (
                HEADER_XFO,
                "nonsense",
                Fail,
                Some("ui-tool-headers-reason-xfo-invalid"),
            ),
            (
                HEADER_XFO,
                "DENY, SAMEORIGIN",
                Fail,
                Some("ui-tool-headers-reason-xfo-invalid"),
            ),
            (HEADER_XCTO, "nosniff", Pass, None),
            (
                HEADER_XCTO,
                "sniff",
                Fail,
                Some("ui-tool-headers-reason-xcto-invalid"),
            ),
            (
                HEADER_REFERRER,
                "strict-origin-when-cross-origin",
                Pass,
                None,
            ),
            (HEADER_REFERRER, "no-referrer, bogus", Pass, None),
            (
                HEADER_REFERRER,
                "unsafe-url",
                Warn,
                Some("ui-tool-headers-reason-referrer-unsafe"),
            ),
            (
                HEADER_REFERRER,
                "nonsense",
                Fail,
                Some("ui-tool-headers-reason-referrer-invalid"),
            ),
            (HEADER_PERMISSIONS, "camera=()", Pass, None),
            (
                HEADER_PERMISSIONS,
                "",
                Fail,
                Some("ui-tool-headers-reason-permissions-empty"),
            ),
        ] {
            assert_eq!(
                verdict_of(name, value),
                (status, reason),
                "{name}: {value:?}"
            );
        }
    }

    #[test]
    fn hsts_counts_over_https_only() {
        let check = check_of(&[(HEADER_HSTS, "max-age=31536000")], false, HEADER_HSTS);
        assert_eq!(check.status, HeaderCheckStatus::Warn);
        assert_eq!(check.reason_key, Some("ui-tool-headers-reason-hsts-http"));
        let check = check_of(&[], true, HEADER_HSTS);
        assert_eq!(check.status, HeaderCheckStatus::Fail);
        assert_eq!(check.reason_key, None);
        assert_eq!(check.recommendation_key, Some("ui-tool-headers-rec-hsts"));
    }

    #[test]
    fn the_report_says_why_a_header_present_fails() {
        let headers = vec![(HEADER_HSTS.to_string(), "max-age=0".to_string())];
        let evaluation = evaluate_headers(&headers, true);
        let say = |text: ReportText<'_>| format!("<{text:?}>");
        let report = build_text_report("https://a/", &evaluation, "", None, &say);
        assert!(report.contains(
            "[<Status(Fail)>] Strict-Transport-Security: max-age=0\n  <Reason(\"ui-tool-headers-reason-hsts-max-age-zero\")>\n"
        ), "{report}");
    }

    #[test]
    fn a_cookie_flag_is_an_attribute_not_a_substring() {
        assert!(!has_cookie_attribute("insecure_pref=1; Path=/", "Secure"));
        assert!(!has_cookie_attribute("session=secure; HttpOnly", "Secure"));
        assert!(has_cookie_attribute(
            "id=1; Path=/; secure; HttpOnly",
            "Secure"
        ));
        assert!(has_cookie_attribute("id=1;SameSite=Lax", "SameSite"));
        assert!(!has_cookie_attribute("samesite=1", "SameSite"), "its name");
        let headers = vec![(
            "Set-Cookie".to_string(),
            "insecure_pref=1; HttpOnly; SameSite=Lax".to_string(),
        )];
        let cookie = evaluate_security_headers(&headers, true)
            .into_iter()
            .find(|check| check.name == HEADER_SET_COOKIE)
            .unwrap();
        assert_eq!(cookie.status, HeaderCheckStatus::Warn);
        assert_eq!(cookie.detail.as_deref(), Some("Secure"));
    }

    #[test]
    fn the_report_takes_its_words_from_the_caller() {
        let headers = vec![("Server".to_string(), "nginx".to_string())];
        let evaluation = evaluate_headers(&headers, true);
        let say = |text: ReportText<'_>| format!("<{text:?}>");
        let report = build_text_report(
            "https://example.com/",
            &evaluation,
            "HTTP/1.1 200 OK",
            Some("<certificate>"),
            &say,
        );
        assert!(
            report
                .starts_with("<Title(\"https://example.com/\")>\n<Grade(\"F\")>\n<certificate>\n")
        );
        assert!(report.contains("[<DisclosureStatus(Warn)>] Server: nginx"));
        assert!(report.ends_with("<RawHeaders>\nHTTP/1.1 200 OK"));
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
                reason_key: None,
                detail: None,
            },
            HeaderCheckResult {
                name: "CSP".into(),
                display_key: "csp",
                value: "val".into(),
                status: HeaderCheckStatus::Pass,
                recommendation_key: None,
                reason_key: None,
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
            reason_key: None,
            detail: None,
        }];
        assert_eq!(calculate_grade(&results), HttpGrade::F);
    }

    #[test]
    fn test_evaluate_headers_disclosure() {
        let headers = vec![("Server".to_string(), "Apache/2.4.41".to_string())];
        let eval = evaluate_headers(&headers, true);
        let srv = eval
            .disclosure_checks
            .iter()
            .find(|c| c.name == HEADER_SERVER)
            .unwrap();
        assert_eq!(srv.status, HeaderCheckStatus::Warn);
        assert_eq!(srv.value, "Apache/2.4.41");
    }
}
