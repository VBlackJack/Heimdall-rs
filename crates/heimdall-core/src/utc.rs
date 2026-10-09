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

//! A moment as a UTC calendar date and time, as the error report and the session
//! transcripts write it, without a time zone database.

use std::time::{SystemTime, UNIX_EPOCH};

/// Seconds in a day.
const DAY_SECONDS: u64 = 86_400;

/// A moment in UTC, to the second.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UtcTime {
    /// Year.
    pub year: u64,
    /// Month, 1 to 12.
    pub month: u64,
    /// Day of the month, from 1.
    pub day: u64,
    /// Hour, 0 to 23.
    pub hour: u64,
    /// Minute, 0 to 59.
    pub minute: u64,
    /// Second, 0 to 59.
    pub second: u64,
}

impl UtcTime {
    /// `time` in UTC; the epoch for a time before it.
    #[must_use]
    pub fn of(time: SystemTime) -> Self {
        let seconds = time
            .duration_since(UNIX_EPOCH)
            .map_or(0, |since| since.as_secs());
        let (days, of_day) = (seconds / DAY_SECONDS, seconds % DAY_SECONDS);
        let (year, month, day) = civil_date(days);
        Self {
            year,
            month,
            day,
            hour: of_day / 3600,
            minute: of_day % 3600 / 60,
            second: of_day % 60,
        }
    }

    /// As `2026-09-27T21:05:03Z`, ISO 8601.
    #[must_use]
    pub fn iso(&self) -> String {
        format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
            self.year, self.month, self.day, self.hour, self.minute, self.second
        )
    }

    /// As `20260927_210503`, for a file name, as the C# session logs name theirs.
    #[must_use]
    pub fn compact(&self) -> String {
        format!(
            "{:04}{:02}{:02}_{:02}{:02}{:02}",
            self.year, self.month, self.day, self.hour, self.minute, self.second
        )
    }
}

/// Seconds in an hour.
const HOUR_SECONDS: i64 = 3600;

/// Seconds in a minute.
const MINUTE_SECONDS: i64 = 60;

/// The moment `text` names in the ISO 8601 form .NET writes a `DateTimeOffset` in:
/// `2026-03-15T12:00:30.1234567+01:00`, or `Z` for UTC; the fraction of a second dropped.
/// `None` for any other text, and for a moment before 1970, `DateTimeOffset.MinValue`
/// among them, which the C# writes for a date it does not know.
#[must_use]
pub fn parse_iso(text: &str) -> Option<SystemTime> {
    let text = text.trim();
    let (date, rest) = text.split_once(['T', 't'])?;
    let mut fields = date.split('-');
    let year = number(fields.next()?, 4)?;
    let month = number(fields.next()?, 2)?;
    let day = number(fields.next()?, 2)?;
    if fields.next().is_some() || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let (time, offset) = rest.split_at(rest.find(['Z', 'z', '+', '-'])?);
    let time = time.split_once('.').map_or(time, |(whole, fraction)| {
        if fraction.bytes().all(|byte| byte.is_ascii_digit()) {
            whole
        } else {
            ""
        }
    });
    let mut fields = time.split(':');
    let hour = number(fields.next()?, 2)?;
    let minute = number(fields.next()?, 2)?;
    let second = number(fields.next()?, 2)?;
    if fields.next().is_some() || hour > 23 || minute > 59 || second > 59 {
        return None;
    }
    let offset = match offset {
        "Z" | "z" => 0,
        signed => {
            let (sign, hours_minutes) = signed.split_at(1);
            let (hours, minutes) = hours_minutes.split_once(':')?;
            let (hours, minutes) = (number(hours, 2)?, number(minutes, 2)?);
            if hours > 23 || minutes > 59 {
                return None;
            }
            let magnitude = hours * HOUR_SECONDS + minutes * MINUTE_SECONDS;
            if sign == "-" { -magnitude } else { magnitude }
        }
    };
    let seconds = days_from_civil(year, month, day) * DAY_SECONDS_SIGNED
        + hour * HOUR_SECONDS
        + minute * MINUTE_SECONDS
        + second
        - offset;
    let seconds = u64::try_from(seconds).ok()?;
    Some(UNIX_EPOCH + std::time::Duration::from_secs(seconds))
}

/// Seconds in a day, signed, for moments counted from 1970 either way.
const DAY_SECONDS_SIGNED: i64 = 86_400;

/// `text` as a number of exactly `digits` decimal digits.
fn number(text: &str, digits: usize) -> Option<i64> {
    (text.len() == digits && text.bytes().all(|byte| byte.is_ascii_digit()))
        .then(|| text.parse().ok())
        .flatten()
}

/// Days from 1970-01-01 to the calendar date given, by Howard Hinnant's
/// `days_from_civil`.
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    // Counted from 0000-03-01, so a leap day ends its year.
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year.rem_euclid(400);
    let month_from_march = if month > 2 { month - 3 } else { month + 9 };
    let day_of_year = (153 * month_from_march + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
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
        UtcTime::of(UNIX_EPOCH + Duration::from_secs(seconds)).iso()
    }

    #[test]
    fn dates_fall_on_the_calendar_leap_days_included() {
        assert_eq!(at(0), "1970-01-01T00:00:00Z");
        assert_eq!(at(59), "1970-01-01T00:00:59Z");
        assert_eq!(at(DAY_SECONDS - 1), "1970-01-01T23:59:59Z");
        // A leap day, the day after, the end of a year, and 2000, a leap century.
        assert_eq!(at(951_782_400), "2000-02-29T00:00:00Z");
        assert_eq!(at(951_868_800), "2000-03-01T00:00:00Z");
        assert_eq!(at(1_704_067_199), "2023-12-31T23:59:59Z");
        assert_eq!(at(4_107_542_400), "2100-03-01T00:00:00Z");
        assert_eq!(at(1_790_536_503), "2026-09-27T19:15:03Z");
        assert_eq!(
            UtcTime::of(UNIX_EPOCH - Duration::from_secs(1)).iso(),
            "1970-01-01T00:00:00Z"
        );
    }

    #[test]
    fn a_dotnet_date_time_offset_is_read_to_the_second_its_offset_applied() {
        let read = |text: &str| parse_iso(text).map(|time| UtcTime::of(time).iso());
        assert_eq!(
            read("2026-09-27T19:15:03Z").as_deref(),
            Some("2026-09-27T19:15:03Z")
        );
        assert_eq!(
            read("2026-09-27T21:15:03.1234567+02:00").as_deref(),
            Some("2026-09-27T19:15:03Z"),
            "the offset applied, the fraction dropped"
        );
        assert_eq!(
            read("2000-02-29T23:30:00-01:00").as_deref(),
            Some("2000-03-01T00:30:00Z"),
            "a leap day, west of UTC"
        );
        assert_eq!(
            parse_iso("1970-01-01T00:00:00+00:00"),
            Some(UNIX_EPOCH),
            "the epoch itself"
        );
        for refused in [
            "0001-01-01T00:00:00+00:00",
            "1969-12-31T23:59:59Z",
            "2026-09-27T19:15:03",
            "2026-09-27 19:15:03Z",
            "2026-13-01T00:00:00Z",
            "2026-09-27T24:00:00Z",
            "2026-09-27T19:15:03.12a+00:00",
            "2026-09-27T19:15:03+0200",
            "26-09-27T19:15:03Z",
            "",
        ] {
            assert_eq!(parse_iso(refused), None, "{refused}");
        }
    }

    #[test]
    fn a_file_name_takes_the_compact_form() {
        assert_eq!(
            UtcTime::of(UNIX_EPOCH + Duration::from_secs(1_790_536_503)).compact(),
            "20260927_191503"
        );
    }
}
