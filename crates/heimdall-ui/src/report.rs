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

//! What a failed session's "Copy error" copies, as the C# Heimdall's error report: a
//! heading, when (UTC), which server, the gateways it went through, how long it was connected,
//! which version, then the error as the card says it.

use std::time::{Duration, SystemTime};

use heimdall_core::paths::APPLICATION;
use heimdall_core::utc::UtcTime;

use crate::i18n::fl;

/// Seconds in a minute.
const MINUTE_SECONDS: u64 = 60;

/// What a report says of the failed session beside the error: the gateways it went through,
/// and how long it was connected.
#[derive(Debug, Clone, Copy, Default)]
pub struct Session<'a> {
    /// The names of its gateways, nearest to this machine first; none when it went straight.
    pub route: &'a [String],
    /// How long it was connected before it failed; `None` when it never connected.
    pub lasted: Option<Duration>,
}

impl Session<'_> {
    /// The C# report's "Tunnel" line: the gateways named, made safe, or, for the anonymized
    /// report, only counted, their names being able to name the network.
    fn tunnel_line(&self, anonymous: bool) -> Option<String> {
        if self.route.is_empty() {
            return None;
        }
        let value = if anonymous {
            fl!("ui-error-report-tunnel-hops", count = self.route.len())
        } else {
            let names: Vec<String> = self
                .route
                .iter()
                .map(|name| heimdall_app::server_text(name))
                .collect();
            fl!(
                "ui-error-report-tunnel-route",
                route = names.join(&fl!("ui-route-test-separator"))
            )
        };
        Some(format!("{} {value}", fl!("ui-error-report-tunnel")))
    }

    /// The C# report's "Session" line: how long it was connected, in minutes and seconds.
    fn session_line(&self) -> Option<String> {
        let seconds = self.lasted?.as_secs();
        let duration = fl!(
            "ui-error-report-duration",
            minutes = (seconds / MINUTE_SECONDS).to_string(),
            seconds = format!("{:02}", seconds % MINUTE_SECONDS)
        );
        Some(format!(
            "{} {}",
            fl!("ui-error-report-session"),
            fl!("ui-error-report-session-duration", duration = duration)
        ))
    }
}

/// The "App" line: the application and its version.
fn app_line() -> String {
    format!(
        "{} {APPLICATION} v{}",
        fl!("ui-error-report-app"),
        env!("CARGO_PKG_VERSION")
    )
}

/// The report of a failure of `protocol`'s session with `server`, at `now`, with what
/// `session` says of it, in the C# report's order.
pub fn error_report(
    protocol: &str,
    server: Option<&str>,
    session: Session<'_>,
    error: &str,
    now: SystemTime,
) -> String {
    let mut lines = vec![
        fl!("ui-error-report-header", protocol = protocol),
        format!("{} {}", fl!("ui-error-report-time"), utc_time(now)),
    ];
    if let Some(server) = server {
        lines.push(format!("{} {server}", fl!("ui-error-report-server")));
    }
    lines.extend(session.tunnel_line(false));
    lines.extend(session.session_line());
    lines.push(app_line());
    lines.push(String::new());
    lines.push(error.to_owned());
    lines.join("\n")
}

/// The report "Copy anonymized report" copies, as the C# RDP one: when, how many gateways
/// the session went through and how long it was connected, which version, and what kind of
/// failure, its name in the code; never the server, a gateway, the account nor the message,
/// which can name them.
pub fn anonymous_report(
    protocol: &str,
    session: Session<'_>,
    kind: &str,
    now: SystemTime,
) -> String {
    let mut lines = vec![
        fl!("ui-error-report-anonymous-header", protocol = protocol),
        format!("{} {}", fl!("ui-error-report-time"), utc_time(now)),
    ];
    lines.extend(session.tunnel_line(true));
    lines.extend(session.session_line());
    lines.push(app_line());
    lines.push(format!("{} {kind}", fl!("ui-error-report-kind")));
    lines.push(fl!("ui-error-report-anonymous-hint"));
    lines.join("\n")
}

/// `time` in UTC as `2026-09-27 21:05:03Z`, the C# report's "u" format; the epoch for a time
/// before it.
fn utc_time(time: SystemTime) -> String {
    let at = UtcTime::of(time);
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}Z",
        at.year, at.month, at.day, at.hour, at.minute, at.second
    )
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, UNIX_EPOCH};

    use super::*;

    /// Seconds in a day.
    const DAY_SECONDS: u64 = 86_400;

    fn at(seconds: u64) -> String {
        utc_time(UNIX_EPOCH + Duration::from_secs(seconds))
    }

    #[test]
    fn times_are_written_as_the_csharp_report_writes_them() {
        assert_eq!(at(0), "1970-01-01 00:00:00Z");
        assert_eq!(at(59), "1970-01-01 00:00:59Z");
        assert_eq!(at(DAY_SECONDS - 1), "1970-01-01 23:59:59Z");
        // A leap day, the day after, the end of a year, and 2000, a leap century.
        assert_eq!(at(951_782_400), "2000-02-29 00:00:00Z");
        assert_eq!(at(951_868_800), "2000-03-01 00:00:00Z");
        assert_eq!(at(1_704_067_199), "2023-12-31 23:59:59Z");
        assert_eq!(at(4_107_542_400), "2100-03-01 00:00:00Z");
        assert_eq!(at(1_790_536_503), "2026-09-27 19:15:03Z");
        assert_eq!(
            utc_time(UNIX_EPOCH - Duration::from_secs(1)),
            "1970-01-01 00:00:00Z"
        );
    }

    #[test]
    fn a_report_says_when_where_which_version_and_what() {
        let report = error_report(
            "SSH",
            Some("web (web.lab:22)"),
            Session::default(),
            "Connection refused.",
            UNIX_EPOCH,
        );
        let lines: Vec<&str> = report.lines().collect();
        assert_eq!(lines[0], "Heimdall SSH error report");
        assert_eq!(lines[1], "Time: 1970-01-01 00:00:00Z");
        assert_eq!(lines[2], "Server: web (web.lab:22)");
        assert!(lines[3].starts_with("App: Heimdall-rs v"), "{}", lines[3]);
        assert_eq!(lines[4], "");
        assert_eq!(lines[5], "Connection refused.");
        let local = error_report("Local", None, Session::default(), "No shell.", UNIX_EPOCH);
        assert!(!local.contains("Server:"), "{local}");
    }

    #[test]
    fn a_session_through_gateways_says_its_route_and_how_long_it_lasted() {
        let route = ["Edge".to_owned(), "bastion.corp".to_owned()];
        let session = Session {
            route: &route,
            lasted: Some(Duration::from_secs(2 * MINUTE_SECONDS + 5)),
        };
        let report = error_report(
            "RDP",
            Some("dc (dc.lab:3389)"),
            session,
            "The connection was lost.",
            UNIX_EPOCH,
        );
        let lines: Vec<&str> = report.lines().collect();
        assert_eq!(lines[2], "Server: dc (dc.lab:3389)");
        assert_eq!(lines[3], "Tunnel: via Edge \u{2192} bastion.corp");
        assert_eq!(lines[4], "Session: connected for 2m 05s");
        assert!(lines[5].starts_with("App: "), "{report}");

        let anonymous = anonymous_report("RDP", session, "ConnectionLost", UNIX_EPOCH);
        let lines: Vec<&str> = anonymous.lines().collect();
        assert_eq!(lines[2], "Tunnel: through 2 SSH gateways");
        assert_eq!(lines[3], "Session: connected for 2m 05s");
        for named in ["Edge", "bastion.corp", "dc.lab"] {
            assert!(!anonymous.contains(named), "{named} in {anonymous}");
        }
        let one = ["Edge".to_owned()];
        let through_one = Session {
            route: &one,
            lasted: None,
        };
        let anonymous = anonymous_report("SSH", through_one, "Timeout", UNIX_EPOCH);
        assert!(
            anonymous.contains("Tunnel: through 1 SSH gateway\n"),
            "{anonymous}"
        );
        assert!(
            !anonymous.contains("Session:"),
            "never connected: {anonymous}"
        );
    }
}
