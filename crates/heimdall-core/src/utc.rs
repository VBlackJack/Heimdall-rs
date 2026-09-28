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
    fn a_file_name_takes_the_compact_form() {
        assert_eq!(
            UtcTime::of(UNIX_EPOCH + Duration::from_secs(1_790_536_503)).compact(),
            "20260927_191503"
        );
    }
}
