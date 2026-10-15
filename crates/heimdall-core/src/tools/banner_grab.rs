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

//! The banner grabber's engine, as the C# `BannerGrabEngine`: the port list read, the banner
//! cleaned, the service told from the port and the banner, and the results written out. The
//! probes themselves are `heimdall_app::network_tools`'.

use std::collections::BTreeSet;
use std::time::Duration;

use super::default_creds::sanitize_csv_cell;

/// Most bytes read of a banner, the C# `BannerMaxBytes`.
pub const BANNER_MAX_BYTES: usize = 512;

/// Time a port has to accept the connection, the C# `ConnectTimeoutMs`.
pub const CONNECT_TIMEOUT: Duration = Duration::from_millis(2000);

/// Time an open port has to send its banner, the C# `BannerReadTimeoutMs`.
pub const READ_TIMEOUT: Duration = Duration::from_millis(2000);

/// Ports probed at once, the C# `MaxConcurrent`.
pub const MAX_CONCURRENT: usize = 20;

/// Parses a comma-separated or range-based port specification (e.g. "21,22,80,443,8000-8005").
#[must_use]
pub fn parse_ports(input: &str) -> Vec<u16> {
    let mut ports = BTreeSet::new();

    for segment in input.split(',') {
        let trimmed = segment.trim();
        if trimmed.is_empty() {
            continue;
        }

        if let Some((start_str, end_str)) = trimmed.split_once('-') {
            if let (Ok(start), Ok(end)) = (
                start_str.trim().parse::<u16>(),
                end_str.trim().parse::<u16>(),
            ) {
                if start <= end && start >= 1 {
                    for p in start..=end {
                        ports.insert(p);
                    }
                }
            }
        } else if let Ok(port) = trimmed.parse::<u16>() {
            if port >= 1 {
                ports.insert(port);
            }
        }
    }

    ports.into_iter().collect()
}

/// Cleans control characters from raw banner bytes, preserving printable ASCII and whitespace.
#[must_use]
pub fn clean_banner(raw: &str) -> Option<String> {
    let mut cleaned = String::with_capacity(raw.len());
    let mut last_was_space = false;

    for c in raw.chars() {
        if c.is_control() || c == '\x7F' {
            if !last_was_space {
                cleaned.push(' ');
                last_was_space = true;
            }
        } else if c.is_whitespace() {
            if !last_was_space {
                cleaned.push(' ');
                last_was_space = true;
            }
        } else {
            cleaned.push(c);
            last_was_space = false;
        }
    }

    let trimmed = cleaned.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// The service a well-known port usually runs, if it is one.
#[must_use]
pub fn default_port_service(port: u16) -> Option<&'static str> {
    Some(match port {
        21 => "FTP",
        22 => "SSH",
        23 => "Telnet",
        25 => "SMTP",
        53 => "DNS",
        80 => "HTTP",
        110 => "POP3",
        143 => "IMAP",
        389 => "LDAP",
        443 => "HTTPS",
        465 => "SMTPS",
        587 => "SMTP (Submission)",
        636 => "LDAPS",
        993 => "IMAPS",
        995 => "POP3S",
        1433 => "MSSQL",
        1521 => "Oracle",
        3306 => "MySQL",
        3389 => "RDP",
        5432 => "PostgreSQL",
        5900 => "VNC",
        6379 => "Redis",
        8080 => "HTTP-Alt",
        8443 => "HTTPS-Alt",
        27017 => "MongoDB",
        _ => return None,
    })
}

/// The service on `port`, told from its banner when one names a known server, else from
/// the port, else the port number itself, as the C# `IdentifyService`.
#[must_use]
pub fn identify_service(port: u16, banner: Option<&str>) -> String {
    let base = default_port_service(port).map_or_else(|| port.to_string(), str::to_owned);

    let Some(b) = banner else {
        return base;
    };

    let lower = b.to_ascii_lowercase();

    if lower.contains("openssh") {
        return "OpenSSH".to_string();
    }
    if lower.contains("dropbear") {
        return "Dropbear SSH".to_string();
    }
    if lower.contains("apache") {
        return "Apache HTTP".to_string();
    }
    if lower.contains("nginx") {
        return "nginx".to_string();
    }
    if lower.contains("microsoft-iis") {
        return "IIS".to_string();
    }
    if lower.contains("postfix") {
        return "Postfix SMTP".to_string();
    }
    if lower.contains("exim") {
        return "Exim SMTP".to_string();
    }
    if lower.contains("dovecot") {
        return "Dovecot".to_string();
    }
    if lower.contains("mysql") {
        return "MySQL".to_string();
    }
    if lower.contains("postgresql") || lower.contains("pgsql") {
        return "PostgreSQL".to_string();
    }
    if lower.contains("redis") {
        return "Redis".to_string();
    }
    if lower.contains("mongodb") || lower.contains("mongod") {
        return "MongoDB".to_string();
    }
    if lower.contains("proftpd") {
        return "ProFTPD".to_string();
    }
    if lower.contains("vsftpd") {
        return "vsftpd".to_string();
    }
    if lower.contains("filezilla") {
        return "FileZilla FTP".to_string();
    }

    base
}

/// What a port answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BannerProbeResult {
    /// The port.
    pub port: u16,
    /// The service, as [`identify_service`] tells it.
    pub service: String,
    /// How long the connection took, in milliseconds; `None` when the port did not accept it.
    pub response_ms: Option<u64>,
    /// What the server sent first, made safe and cleaned, if anything.
    pub banner: Option<String>,
}

impl BannerProbeResult {
    /// A port that did not accept the connection.
    #[must_use]
    pub fn closed(port: u16) -> Self {
        Self {
            port,
            service: identify_service(port, None),
            response_ms: None,
            banner: None,
        }
    }

    /// A port that accepted the connection in `millis`, having sent `banner`.
    #[must_use]
    pub fn open(port: u16, millis: u64, banner: Option<String>) -> Self {
        Self {
            port,
            service: identify_service(port, banner.as_deref()),
            response_ms: Some(millis),
            banner,
        }
    }
}

/// The words the exports are written with, in the user's language, as the C# passes its
/// localizer to `BuildCsvExport` and `BuildClipboardText`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportLabels {
    /// The port column's header.
    pub port: String,
    /// The service column's header.
    pub service: String,
    /// The banner column's header.
    pub banner: String,
    /// The time column's header.
    pub time: String,
    /// What stands for no value.
    pub none: String,
}

/// Width of the port column of [`build_clipboard_text`], as the C#.
const CLIPBOARD_PORT_WIDTH: usize = 8;

/// Width of the service column of [`build_clipboard_text`], as the C#.
const CLIPBOARD_SERVICE_WIDTH: usize = 20;

/// Width of the time column of [`build_clipboard_text`], as the C#.
const CLIPBOARD_TIME_WIDTH: usize = 12;

/// Length of the rule under the header of [`build_clipboard_text`], as the C#.
const CLIPBOARD_RULE_LENGTH: usize = 72;

/// One CSV cell: made unable to start a spreadsheet formula, as the C#
/// `InputValidator.SanitizeCsvCell`, then quoted (RFC 4180).
fn csv_cell(value: &str) -> String {
    format!("\"{}\"", sanitize_csv_cell(value).replace('"', "\"\""))
}

/// `results` as CSV, ordered by port, in the C# columns (port, service, banner, time);
/// `time` writes a time in milliseconds; `labels.none` is not used. Every cell, the headers too, goes through
/// [`csv_cell`]: a banner is the server's words.
#[must_use]
pub fn build_csv_export(
    results: &[BannerProbeResult],
    labels: &ExportLabels,
    time: impl Fn(u64) -> String,
    line_end: &str,
) -> String {
    let mut sorted: Vec<&BannerProbeResult> = results.iter().collect();
    sorted.sort_by_key(|result| result.port);
    let mut out = [&labels.port, &labels.service, &labels.banner, &labels.time]
        .map(|header| csv_cell(header))
        .join(",");
    out.push_str(line_end);
    for result in sorted {
        // A closed port's time is an empty cell: the "no value" mark would be read as a
        // formula sign and come out quoted.
        let time = result.response_ms.map_or_else(String::new, &time);
        let row = [
            result.port.to_string(),
            result.service.clone(),
            result.banner.clone().unwrap_or_default(),
            time,
        ]
        .map(|cell| csv_cell(&cell))
        .join(",");
        out.push_str(&row);
        out.push_str(line_end);
    }
    out
}

/// `results` as aligned text, as the C# `BuildClipboardText`; `time` writes a time in
/// milliseconds.
#[must_use]
pub fn build_clipboard_text(
    results: &[BannerProbeResult],
    labels: &ExportLabels,
    time: impl Fn(u64) -> String,
    line_end: &str,
) -> String {
    let mut out = format!(
        "{:<pw$}{:<sw$}{:<tw$}{}{line_end}",
        labels.port,
        labels.service,
        labels.time,
        labels.banner,
        pw = CLIPBOARD_PORT_WIDTH,
        sw = CLIPBOARD_SERVICE_WIDTH,
        tw = CLIPBOARD_TIME_WIDTH,
    );
    out.push_str(&"-".repeat(CLIPBOARD_RULE_LENGTH));
    out.push_str(line_end);
    for result in results {
        let time = result
            .response_ms
            .map_or_else(|| labels.none.clone(), &time);
        let banner = result.banner.as_deref().unwrap_or(&labels.none);
        out.push_str(&format!(
            "{:<pw$}{:<sw$}{:<tw$}{banner}{line_end}",
            result.port,
            result.service,
            time,
            pw = CLIPBOARD_PORT_WIDTH,
            sw = CLIPBOARD_SERVICE_WIDTH,
            tw = CLIPBOARD_TIME_WIDTH,
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_ports_ranges_and_singles() {
        assert_eq!(parse_ports("21, 22, 80"), vec![21, 22, 80]);
        assert_eq!(parse_ports("80-83"), vec![80, 81, 82, 83]);
        assert_eq!(parse_ports("443, 80-82, 22"), vec![22, 80, 81, 82, 443]);
        assert_eq!(parse_ports("invalid, -5, 0, 80"), vec![80]);
    }

    #[test]
    fn test_clean_banner() {
        let raw = "SSH-2.0-OpenSSH_8.9p1 Ubuntu-3ubuntu0.6\r\n";
        assert_eq!(
            clean_banner(raw),
            Some("SSH-2.0-OpenSSH_8.9p1 Ubuntu-3ubuntu0.6".to_string())
        );
        assert_eq!(clean_banner("\0\0   \r\n"), None);
    }

    #[test]
    fn test_identify_service_with_banner() {
        assert_eq!(identify_service(22, Some("SSH-2.0-OpenSSH_8.9")), "OpenSSH");
        assert_eq!(identify_service(80, Some("Server: nginx/1.18.0")), "nginx");
        assert_eq!(identify_service(22, None), "SSH");
        // An unknown port is named by its number, as the C#: no English word to translate.
        assert_eq!(identify_service(9999, None), "9999");
    }

    fn labels() -> ExportLabels {
        ExportLabels {
            port: "Port".into(),
            service: "Service".into(),
            banner: "Banner".into(),
            time: "Time".into(),
            none: "-".into(),
        }
    }

    #[test]
    fn the_csv_export_quotes_every_cell_and_defuses_formulas() {
        let results = vec![
            BannerProbeResult::open(22, 5, Some("SSH-2.0-OpenSSH_8.9".into())),
            BannerProbeResult::open(80, 7, Some("=HYPERLINK(\"http://x\")".into())),
            BannerProbeResult::closed(21),
        ];
        let csv = build_csv_export(&results, &labels(), |ms| format!("{ms} ms"), "\n");
        let lines: Vec<&str> = csv.lines().collect();
        assert_eq!(
            lines,
            [
                "\"Port\",\"Service\",\"Banner\",\"Time\"",
                "\"21\",\"FTP\",\"\",\"\"",
                "\"22\",\"OpenSSH\",\"SSH-2.0-OpenSSH_8.9\",\"5 ms\"",
                "\"80\",\"HTTP\",\"'=HYPERLINK(\"\"http://x\"\")\",\"7 ms\"",
            ]
        );
    }

    #[test]
    fn the_clipboard_text_is_aligned_in_the_given_words() {
        let results = vec![BannerProbeResult::closed(9999)];
        let text = build_clipboard_text(&results, &labels(), |ms| format!("{ms} ms"), "\n");
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(
            lines[0],
            format!("{:<8}{:<20}{:<12}Banner", "Port", "Service", "Time")
        );
        assert_eq!(lines[1], "-".repeat(72));
        assert_eq!(lines[2], format!("{:<8}{:<20}{:<12}-", "9999", "9999", "-"));
    }
}
