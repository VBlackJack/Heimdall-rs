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
//! heading, when (UTC), which server, which version, then the error as the card says it.

use std::time::{SystemTime, UNIX_EPOCH};

use heimdall_core::paths::APPLICATION;

use crate::i18n::fl;

/// Seconds in a day.
const DAY_SECONDS: u64 = 86_400;

/// The report of a failure of `protocol`'s session with `server`, at `now`.
pub fn error_report(protocol: &str, server: Option<&str>, error: &str, now: SystemTime) -> String {
    let mut lines = vec![
        fl!("ui-error-report-header", protocol = protocol),
        format!("{} {}", fl!("ui-error-report-time"), utc_time(now)),
    ];
    if let Some(server) = server {
        lines.push(format!("{} {server}", fl!("ui-error-report-server")));
    }
    lines.push(format!(
        "{} {APPLICATION} v{}",
        fl!("ui-error-report-app"),
        env!("CARGO_PKG_VERSION")
    ));
    lines.push(String::new());
    lines.push(error.to_owned());
    lines.join("\n")
}

/// `time` in UTC as `2026-09-27 21:05:03Z`, the C# report's "u" format; the epoch for a time
/// before it.
fn utc_time(time: SystemTime) -> String {
    let seconds = time
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs());
    let (days, of_day) = (seconds / DAY_SECONDS, seconds % DAY_SECONDS);
    let (year, month, day) = civil_date(days);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}:{:02}Z",
        of_day / 3600,
        of_day % 3600 / 60,
        of_day % 60
    )
}

/// The calendar date `days` after 1970-01-01, by Howard Hinnant's `civil_from_days`.
fn civil_date(days: u64) -> (u64, u64, u64) {
    // Counted from 0000-03-01, so a leap day ends its year.
    let shifted = days + 719_468;
    let era = shifted / 146_097;
    let day_of_era = shifted % 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    };
    let year = year_of_era + era * 400 + u64::from(month <= 2);
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

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
        let local = error_report("Local", None, "No shell.", UNIX_EPOCH);
        assert!(!local.contains("Server:"), "{local}");
    }
}
