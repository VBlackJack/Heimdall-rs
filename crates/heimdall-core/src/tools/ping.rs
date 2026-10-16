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

//! Continuous ping monitor engine, as the C# `PingStatsEngine`: models, input validation,
//! the system `ping` program's arguments and replies, running statistics and CSV export.
//!
//! The C# sends its echo requests through `System.Net.NetworkInformation.Ping`. Rust has
//! no unprivileged ICMP socket without a new crate or `unsafe`, so the probe runs the
//! system's own `ping` by its full path, once per request, on an address the host was
//! resolved to first: no name ever reaches the program, and nothing typed can be read as
//! one of its options. Its reply is read without its words, which Windows translates
//! (`temps=14 ms`, `Délai d'attente`): an address, a number followed by `ms` and a `TTL`.

use std::fmt::Write as _;
use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::LazyLock;

use regex::Regex;

use super::csv_cell::quoted;
use super::net_host;

/// Maximum number of data points retained in the live latency graph.
pub const MAX_DATA_POINTS: usize = 60;
/// Default ping timeout in milliseconds.
pub const DEFAULT_PING_TIMEOUT_MS: u64 = 2000;
/// Default ping count (0 = continuous).
pub const DEFAULT_PING_COUNT: usize = 0;
/// Minimum allowed timeout in milliseconds.
pub const MIN_TIMEOUT_MS: u64 = 100;
/// Maximum allowed timeout in milliseconds.
pub const MAX_TIMEOUT_MS: u64 = 30000;
/// Maximum allowed probe count limit.
pub const MAX_COUNT: usize = 100000;
/// Time between two probes, as the C# view's default interval (`PingToolView.xaml.cs:513`).
pub const PING_INTERVAL_MS: u64 = 1000;
/// Probes kept in the history of a continuous run: the oldest go first, the statistics
/// covering what is kept.
pub const MAX_HISTORY: usize = MAX_COUNT;
/// Time the `ping` program may take beyond the probe's own timeout before it is stopped,
/// as the C# gateway probe's `CommandTimeout` (`PingService.cs`, `timeoutMs + 5000`).
pub const PROGRAM_GRACE_MS: u64 = 5000;

/// The `ping` program of Windows, in its system folder.
#[cfg(windows)]
pub const PING_PROGRAM: &str = "PING.EXE";

/// Where the `ping` program lives on the other systems, tried in order.
#[cfg(not(windows))]
pub const PING_PROGRAMS: [&str; 4] = ["/usr/bin/ping", "/bin/ping", "/usr/sbin/ping", "/sbin/ping"];

/// A number of milliseconds as `ping` writes it after its time word: `=14ms`, `<1ms`,
/// `=14 ms`, `=14.3 ms`, `=0,456 ms`, in whatever language the word is.
static LATENCY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)[=<]\s*([0-9]+(?:[.,][0-9]+)?)\s*ms\b").expect("valid regex")
});

/// The time-to-live of a reply: `TTL=57`, `ttl=57`.
static TTL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\bttl[=:]\s*([0-9]+)").expect("valid regex"));

/// Status outcome of a single ICMP ping probe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PingStatus {
    /// Echo reply received successfully.
    Success,
    /// No echo reply: timed out, or the host was unreachable.
    Timeout,
    /// Probe encountered an error (host not found, no `ping` program).
    Error,
}

impl PingStatus {
    /// Display label for the status.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Success => "Success",
            Self::Timeout => "Timeout",
            Self::Error => "Error",
        }
    }
}

/// What a probe came to, beyond its status; the view says it in the user's language.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PingDetail {
    /// An echo reply.
    Ok,
    /// No reply in time.
    TimedOut,
    /// A router said the host cannot be reached (Windows `ping` still exits with 0 then).
    Unreachable,
    /// The host name did not resolve.
    Unresolved,
    /// No `ping` program where the system keeps it.
    NoProgram,
    /// The program could not run, or ran past its time: why, made safe to show.
    Failed(String),
}

impl PingDetail {
    /// The status this detail is counted under; the C# counts every reply but an echo as
    /// a timeout (`PingService.cs`, `PingStatus.Timeout`).
    #[must_use]
    pub const fn status(&self) -> PingStatus {
        match self {
            Self::Ok => PingStatus::Success,
            Self::TimedOut | Self::Unreachable => PingStatus::Timeout,
            Self::Unresolved | Self::NoProgram | Self::Failed(_) => PingStatus::Error,
        }
    }

    /// The detail's code in an export, as the C# `IPStatus` names it.
    #[must_use]
    pub fn code(&self) -> &str {
        match self {
            Self::Ok => "Success",
            Self::TimedOut => "TimedOut",
            Self::Unreachable => "DestinationHostUnreachable",
            Self::Unresolved => "Unresolved",
            Self::NoProgram => "NoProgram",
            Self::Failed(why) => why,
        }
    }
}

/// Validated input parameters for a ping probe session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PingInputs {
    /// Target host name or IP address, checked by [`net_host::checked_host`].
    pub host: String,
    /// Timeout in milliseconds per probe.
    pub timeout_ms: u64,
    /// Number of probes to issue (0 = continuous until stopped).
    pub count: usize,
}

/// Result of a single ICMP ping probe attempt.
#[derive(Debug, Clone, PartialEq)]
pub struct PingProbeResult {
    /// Sequence counter (1-based).
    pub seq: usize,
    /// Local wall-clock time of the probe (HH:mm:ss), as the C# `DateTime.Now`.
    pub timestamp: String,
    /// Round-trip latency in milliseconds (-1.0 if lost/timed out).
    pub latency_ms: f64,
    /// Status outcome.
    pub status: PingStatus,
    /// Time-to-live reported in the reply (0 if unavailable).
    pub ttl: u32,
    /// Target host name or the address that replied.
    pub ip_or_host: String,
    /// What the probe came to.
    pub detail: PingDetail,
}

impl PingProbeResult {
    /// A probe that came to `detail`, without a reply.
    #[must_use]
    pub fn without_reply(seq: usize, timestamp: String, host: String, detail: PingDetail) -> Self {
        Self {
            seq,
            timestamp,
            latency_ms: -1.0,
            status: detail.status(),
            ttl: 0,
            ip_or_host: host,
            detail,
        }
    }

    /// A probe answered by `reply`.
    #[must_use]
    pub fn replied(seq: usize, timestamp: String, host: String, reply: &PingReply) -> Self {
        Self {
            seq,
            timestamp,
            latency_ms: reply.latency_ms,
            status: PingStatus::Success,
            ttl: reply.ttl.unwrap_or(0),
            ip_or_host: reply.address.map_or(host, |address| address.to_string()),
            detail: PingDetail::Ok,
        }
    }
}

/// An echo reply, as read from the `ping` program's output.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PingReply {
    /// Round-trip time in milliseconds.
    pub latency_ms: f64,
    /// Time-to-live, when the reply says it (Windows says none over IPv6).
    pub ttl: Option<u32>,
    /// The address that replied.
    pub address: Option<IpAddr>,
}

/// Aggregated running statistics across a collection of ping probes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PingStatsSnapshot {
    /// Minimum round-trip time in milliseconds (0.0 if no replies).
    pub min_ms: f64,
    /// Maximum round-trip time in milliseconds.
    pub max_ms: f64,
    /// Arithmetic mean round-trip time in milliseconds.
    pub avg_ms: f64,
    /// Latency jitter (standard deviation) in milliseconds.
    pub jitter_ms: f64,
    /// Packet loss percentage (0.0 to 100.0).
    pub loss_percent: f64,
    /// Total probes sent.
    pub sent: usize,
    /// Successful probes received.
    pub received: usize,
    /// Probes lost or failed.
    pub lost: usize,
}

impl Default for PingStatsSnapshot {
    fn default() -> Self {
        Self {
            min_ms: 0.0,
            max_ms: 0.0,
            avg_ms: 0.0,
            jitter_ms: 0.0,
            loss_percent: 0.0,
            sent: 0,
            received: 0,
            lost: 0,
        }
    }
}

/// A point on the live latency graph.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PingDataPoint {
    /// Sequence counter.
    pub seq: usize,
    /// Latency in milliseconds (0.0 if timed out).
    pub latency_ms: f64,
    /// Whether this point represents a timeout/loss.
    pub is_timeout: bool,
}

/// Validates raw textual inputs for ping parameters, as the C# `ValidateInputs`: a host,
/// a timeout within its bounds, a count within its bounds or none for a continuous run.
///
/// Returns `Ok(PingInputs)` on success, or an error translation key on failure.
pub fn validate_inputs(
    host_text: &str,
    timeout_text: &str,
    count_text: &str,
) -> Result<PingInputs, &'static str> {
    if host_text.trim().is_empty() {
        return Err("ui-tool-ping-error-host-required");
    }
    let host = net_host::checked_host(host_text).ok_or("ui-tool-ping-error-host-invalid")?;

    // Required, as the C# `int.TryParse` of an empty field fails.
    let timeout_ms = match timeout_text.trim().parse::<u64>() {
        Ok(val) if (MIN_TIMEOUT_MS..=MAX_TIMEOUT_MS).contains(&val) => val,
        _ => return Err("ui-tool-ping-error-timeout-invalid"),
    };

    let count_trimmed = count_text.trim();
    let count = if count_trimmed.is_empty() {
        DEFAULT_PING_COUNT
    } else {
        match count_trimmed.parse::<usize>() {
            Ok(val) if val <= MAX_COUNT => val,
            _ => return Err("ui-tool-ping-error-count-invalid"),
        }
    };

    Ok(PingInputs {
        host,
        timeout_ms,
        count,
    })
}

/// The system's `ping` program, by its full path: never one found on the `PATH` or beside
/// Heimdall. `None` when the system has none where it keeps it.
#[must_use]
pub fn ping_program() -> Option<PathBuf> {
    #[cfg(windows)]
    return crate::paths::system_program(PING_PROGRAM);
    #[cfg(not(windows))]
    return PING_PROGRAMS
        .iter()
        .map(PathBuf::from)
        .find(|path| path.is_file());
}

/// The arguments of one echo request to `address`, waiting `timeout_ms` for its reply.
#[must_use]
pub fn ping_arguments(address: IpAddr, timeout_ms: u64) -> Vec<String> {
    let mut arguments = Vec::new();
    #[cfg(windows)]
    {
        arguments.extend(["-n".to_owned(), "1".to_owned(), "-w".to_owned()]);
        arguments.push(timeout_ms.to_string());
        arguments.push(if address.is_ipv6() { "-6" } else { "-4" }.to_owned());
    }
    #[cfg(target_os = "macos")]
    {
        // macOS: one request, the wait in milliseconds, numeric output.
        arguments.extend(["-n".to_owned(), "-c".to_owned(), "1".to_owned()]);
        arguments.extend(["-W".to_owned(), timeout_ms.to_string()]);
    }
    #[cfg(all(not(windows), not(target_os = "macos")))]
    {
        // iputils: one request, the wait in whole seconds, numeric output.
        let seconds = timeout_ms.div_ceil(1000).max(1);
        arguments.extend(["-n".to_owned(), "-c".to_owned(), "1".to_owned()]);
        arguments.extend(["-W".to_owned(), seconds.to_string()]);
    }
    arguments.push(address.to_string());
    arguments
}

/// The echo reply in `output`, the `ping` program's: the first line holding an address
/// and a time in milliseconds. A line of statistics holds no address, and a router's
/// "unreachable" no time, so neither passes for a reply.
#[must_use]
pub fn parse_reply(output: &str) -> Option<PingReply> {
    output.lines().find_map(|line| {
        let latency_ms = LATENCY
            .captures(line)
            .and_then(|found| found[1].replace(',', ".").parse::<f64>().ok())?;
        let address = address_in(line)?;
        let ttl = TTL
            .captures(line)
            .and_then(|found| found[1].parse::<u32>().ok());
        Some(PingReply {
            latency_ms,
            ttl,
            address: Some(address),
        })
    })
}

/// The first IP address written in `line`, whatever surrounds it: a colon after it
/// (`1.1.1.1:`, `::1:`), parentheses, or a space the console's code page turned into a
/// replacement character.
fn address_in(line: &str) -> Option<IpAddr> {
    line.split(|c: char| !(c.is_ascii_hexdigit() || c == '.' || c == ':'))
        .find_map(|piece| {
            piece
                .parse::<IpAddr>()
                .ok()
                .or_else(|| piece.strip_suffix(':')?.parse::<IpAddr>().ok())
        })
}

/// What one run of the `ping` program came to, from whether it `exited_ok` and its
/// `output`: a reply, or why there was none. Windows `ping` exits with 0 when a router
/// answers "Destination host unreachable": no time in that answer, so it is no reply.
#[must_use]
pub fn classify_run(exited_ok: bool, output: &str) -> Result<PingReply, PingDetail> {
    match parse_reply(output) {
        Some(reply) => Ok(reply),
        None if exited_ok => Err(PingDetail::Unreachable),
        None => Err(PingDetail::TimedOut),
    }
}

/// Computes summary statistics from a slice of probe results.
#[must_use]
#[allow(clippy::cast_precision_loss)]
pub fn compute_stats(results: &[PingProbeResult]) -> PingStatsSnapshot {
    if results.is_empty() {
        return PingStatsSnapshot::default();
    }

    let mut min = f64::MAX;
    let mut max = 0.0f64;
    let mut total = 0.0f64;
    let mut sum_squared = 0.0f64;
    let mut received = 0usize;
    let mut lost = 0usize;

    for r in results {
        if r.status == PingStatus::Success && r.latency_ms >= 0.0 {
            received += 1;
            total += r.latency_ms;
            sum_squared += r.latency_ms * r.latency_ms;
            if r.latency_ms < min {
                min = r.latency_ms;
            }
            if r.latency_ms > max {
                max = r.latency_ms;
            }
        } else {
            lost += 1;
        }
    }

    if received == 0 {
        min = 0.0;
    }

    let sent = results.len();
    let avg = if received > 0 {
        total / (received as f64)
    } else {
        0.0
    };
    let mean = avg;
    let variance = if received > 0 {
        (sum_squared / (received as f64)) - (mean * mean)
    } else {
        0.0
    };
    let jitter = variance.max(0.0).sqrt();
    let loss_percent = if sent > 0 {
        (lost as f64 * 100.0) / (sent as f64)
    } else {
        0.0
    };

    PingStatsSnapshot {
        min_ms: min,
        max_ms: max,
        avg_ms: avg,
        jitter_ms: jitter,
        loss_percent,
        sent,
        received,
        lost,
    }
}

/// Computes the recommended Y-axis upper limit and midpoint for graph rendering.
#[must_use]
#[allow(clippy::cast_precision_loss)]
pub fn compute_graph_y_scale(data_points: &[PingDataPoint]) -> (f64, f64) {
    let mut max_y = 10.0f64;
    for pt in data_points {
        if !pt.is_timeout && pt.latency_ms > max_y {
            max_y = pt.latency_ms;
        }
    }

    let ceiling = (max_y * 1.25).max(10.0);
    (ceiling, ceiling / 2.0)
}

/// Builds CSV formatted export text from probe history: every text cell quoted, and one
/// that a spreadsheet would read as a formula neutralized, as the C# `SanitizeCsvCell`.
#[must_use]
pub fn build_csv_export(results: &[PingProbeResult]) -> String {
    let mut csv = String::from("Seq,Timestamp,Host,Status,LatencyMs,TTL,Detail\n");
    for r in results {
        let lat_str = if r.latency_ms >= 0.0 {
            format!("{:.1}", r.latency_ms)
        } else {
            String::new()
        };
        let _ = writeln!(
            csv,
            "{},{},{},{},{},{},{}",
            r.seq,
            quoted(&r.timestamp),
            quoted(&r.ip_or_host),
            r.status.label(),
            lat_str,
            r.ttl,
            quoted(r.detail.code())
        );
    }
    csv
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_inputs_valid() {
        let res = validate_inputs("8.8.8.8", "1500", "5");
        assert!(res.is_ok());
        let inputs = res.unwrap();
        assert_eq!(inputs.host, "8.8.8.8");
        assert_eq!(inputs.timeout_ms, 1500);
        assert_eq!(inputs.count, 5);
    }

    #[test]
    fn an_empty_count_is_a_continuous_run_and_an_empty_timeout_refused() {
        let inputs = validate_inputs("google.com", "2000", "").expect("valid");
        assert_eq!(inputs.count, DEFAULT_PING_COUNT);
        assert_eq!(inputs.count, 0, "continuous, as the C#");
        assert_eq!(
            validate_inputs("google.com", "", ""),
            Err("ui-tool-ping-error-timeout-invalid")
        );
    }

    #[test]
    fn test_validate_inputs_empty_host_fails() {
        let res = validate_inputs("   ", "1000", "1");
        assert!(res.is_err());
        assert_eq!(res.unwrap_err(), "ui-tool-ping-error-host-required");
    }

    #[test]
    fn a_host_that_reads_as_an_option_is_refused() {
        for host in ["-t", "-n 1000 example.com", "example.com -l 65500", "a b"] {
            assert_eq!(
                validate_inputs(host, "1000", "1"),
                Err("ui-tool-ping-error-host-invalid"),
                "{host}"
            );
        }
    }

    #[test]
    fn test_validate_inputs_bad_timeout_fails() {
        assert!(validate_inputs("example.com", "50", "1").is_err());
        assert!(validate_inputs("example.com", "999999", "1").is_err());
        assert!(validate_inputs("example.com", "1000", "100001").is_err());
    }

    #[test]
    fn the_arguments_end_with_the_address_and_ask_for_one_request() {
        let arguments = ping_arguments("192.0.2.1".parse().unwrap(), 1500);
        assert_eq!(arguments.last().map(String::as_str), Some("192.0.2.1"));
        assert!(arguments.iter().any(|argument| argument == "1"));
        assert!(
            arguments[..arguments.len() - 1]
                .iter()
                .all(|argument| argument.starts_with('-') || argument.parse::<u64>().is_ok())
        );
    }

    #[test]
    fn an_english_windows_reply_is_read() {
        let out = "\r\nPinging 1.1.1.1 with 32 bytes of data:\r\n\
                   Reply from 1.1.1.1: bytes=32 time=14ms TTL=57\r\n\r\n\
                   Ping statistics for 1.1.1.1:\r\n\
                   Approximate round trip times in milli-seconds:\r\n\
                   Minimum = 14ms, Maximum = 14ms, Average = 14ms\r\n";
        let reply = classify_run(true, out).expect("a reply");
        assert_eq!(reply.latency_ms, 14.0);
        assert_eq!(reply.ttl, Some(57));
        assert_eq!(reply.address, Some("1.1.1.1".parse().unwrap()));
    }

    #[test]
    fn a_french_windows_reply_is_read_whatever_its_accents_became() {
        // The console's code page turns the accents into replacement characters.
        let out = "Envoi d'une requ\u{fffd}te 'Ping'  192.168.1.1 avec 32 octets de donn\u{fffd}es :\r\n\
                   R\u{fffd}ponse de 192.168.1.1\u{fffd}: octets=32 temps=3 ms TTL=64\r\n\r\n\
                   Statistiques Ping pour 192.168.1.1:\r\n\
                   Minimum = 3ms, Maximum = 3ms, Moyenne = 3ms\r\n";
        let reply = classify_run(true, out).expect("a reply");
        assert_eq!(reply.latency_ms, 3.0);
        assert_eq!(reply.ttl, Some(64));
        assert_eq!(reply.address, Some("192.168.1.1".parse().unwrap()));
        let below = "R\u{fffd}ponse de 127.0.0.1 : octets=32 temps<1ms TTL=128";
        assert_eq!(parse_reply(below).map(|reply| reply.latency_ms), Some(1.0));
    }

    #[test]
    fn a_linux_reply_and_a_windows_ipv6_reply_are_read() {
        let linux = "PING 1.1.1.1 (1.1.1.1) 56(84) bytes of data.\n\
                     64 bytes from 1.1.1.1: icmp_seq=1 ttl=57 time=14.32 ms\n\n\
                     --- 1.1.1.1 ping statistics ---\n\
                     rtt min/avg/max/mdev = 14.320/14.320/14.320/0.000 ms\n";
        let reply = classify_run(true, linux).expect("a reply");
        assert_eq!(reply.latency_ms, 14.32);
        assert_eq!(reply.ttl, Some(57));
        let v6 = "Reply from ::1: time<1ms\r\n";
        let reply = classify_run(true, v6).expect("a reply");
        assert_eq!(reply.ttl, None, "Windows says no TTL over IPv6");
        assert_eq!(reply.address, Some("::1".parse().unwrap()));
    }

    #[test]
    fn destination_host_unreachable_is_no_reply_though_windows_exits_with_zero() {
        let english = "Reply from 192.168.1.10: Destination host unreachable.\r\n\r\n\
                       Ping statistics for 192.168.1.20:\r\n\
                       Packets: Sent = 1, Received = 1, Lost = 0 (0% loss),\r\n";
        assert_eq!(classify_run(true, english), Err(PingDetail::Unreachable));
        let french = "R\u{fffd}ponse de 192.168.1.10\u{a0}: Impossible de joindre l'h\u{fffd}te de destination.\r\n";
        assert_eq!(classify_run(true, french), Err(PingDetail::Unreachable));
        assert_eq!(
            PingDetail::Unreachable.status(),
            PingStatus::Timeout,
            "lost, as the C# counts it"
        );
    }

    #[test]
    fn no_reply_in_any_language_is_a_timeout() {
        assert_eq!(
            classify_run(false, "Request timed out.\r\n"),
            Err(PingDetail::TimedOut)
        );
        assert_eq!(
            classify_run(
                false,
                "D\u{fffd}lai d'attente de la demande d\u{fffd}pass\u{fffd}.\r\n"
            ),
            Err(PingDetail::TimedOut)
        );
        assert_eq!(
            classify_run(
                false,
                "1 packets transmitted, 0 received, 100% packet loss, time 0ms\n"
            ),
            Err(PingDetail::TimedOut)
        );
    }

    #[test]
    fn test_compute_stats_mixed() {
        let reply = |latency_ms| PingReply {
            latency_ms,
            ttl: Some(64),
            address: None,
        };
        let results = vec![
            PingProbeResult::replied(1, "12:00:00".into(), "host".into(), &reply(10.0)),
            PingProbeResult::replied(2, "12:00:01".into(), "host".into(), &reply(20.0)),
            PingProbeResult::without_reply(
                3,
                "12:00:02".into(),
                "host".into(),
                PingDetail::TimedOut,
            ),
            PingProbeResult::replied(4, "12:00:03".into(), "host".into(), &reply(30.0)),
        ];

        let stats = compute_stats(&results);
        assert_eq!(stats.sent, 4);
        assert_eq!(stats.received, 3);
        assert_eq!(stats.lost, 1);
        assert_eq!(stats.min_ms, 10.0);
        assert_eq!(stats.max_ms, 30.0);
        assert_eq!(stats.avg_ms, 20.0);
        assert_eq!(stats.loss_percent, 25.0);
        assert!(stats.jitter_ms > 0.0);
    }

    #[test]
    fn test_compute_graph_y_scale() {
        let points = vec![
            PingDataPoint {
                seq: 1,
                latency_ms: 40.0,
                is_timeout: false,
            },
            PingDataPoint {
                seq: 2,
                latency_ms: 100.0,
                is_timeout: false,
            },
        ];
        let (max_y, mid_y) = compute_graph_y_scale(&points);
        assert_eq!(max_y, 125.0);
        assert_eq!(mid_y, 62.5);
    }

    #[test]
    fn the_csv_quotes_its_text_and_neutralizes_formulas() {
        let reply = PingReply {
            latency_ms: 12.5,
            ttl: Some(64),
            address: Some("1.1.1.1".parse().unwrap()),
        };
        let results = vec![
            PingProbeResult::replied(1, "10:00:00".into(), "one.one".into(), &reply),
            PingProbeResult::without_reply(
                2,
                "10:00:01".into(),
                "1.1.1.1".into(),
                PingDetail::TimedOut,
            ),
            PingProbeResult::without_reply(
                3,
                "10:00:02".into(),
                "=cmd|' /C calc'!A0".into(),
                PingDetail::Failed("-oops, \"quoted\"".into()),
            ),
        ];
        let csv = build_csv_export(&results);
        assert!(csv.starts_with("Seq,Timestamp,Host,Status,LatencyMs,TTL,Detail\n"));
        assert!(csv.contains("1,\"10:00:00\",\"1.1.1.1\",Success,12.5,64,\"Success\""));
        assert!(csv.contains("2,\"10:00:01\",\"1.1.1.1\",Timeout,,0,\"TimedOut\""));
        assert!(csv.contains("\"'=cmd|' /C calc'!A0\""), "{csv}");
        assert!(csv.contains("\"'-oops, \"\"quoted\"\"\""), "{csv}");
    }
}
