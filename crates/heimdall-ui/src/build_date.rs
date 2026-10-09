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

//! The day of an instant, as the About page's "Build date" writes it: shared by the build
//! script, which turns `SOURCE_DATE_EPOCH` or the commit's time into it, and by the tests.
//! No dependency: the build script compiles this file on its own.

/// The variable the build script sets to the build's day, read by the About page.
pub const BUILD_DATE_VARIABLE: &str = "HEIMDALL_BUILD_DATE";

/// Seconds in a day: the instants are UTC, with no leap second, as Unix time counts them.
const SECONDS_PER_DAY: i64 = 86_400;

/// Days from 0000-03-01 to 1970-01-01 in the proleptic Gregorian calendar: the count
/// starts on a 1 March, so that the leap day ends a year.
const EPOCH_FROM_MARCH_ZERO: i64 = 719_468;

/// An era: 400 years, the Gregorian calendar's full cycle, and its days.
const YEARS_PER_ERA: i64 = 400;
const DAYS_PER_ERA: i64 = 146_097;

/// Days in a common year, and the corrections of the leap years: every 4 years, not every
/// 100, every 400, counted in days within an era.
const DAYS_PER_YEAR: i64 = 365;
const DAYS_PER_4_YEARS: i64 = 1_460;
const DAYS_PER_100_YEARS: i64 = 36_524;
const LAST_DAY_OF_ERA: i64 = 146_096;
const LEAP_CYCLE: i64 = 4;
const CENTURY: i64 = 100;

/// The months from March are counted in a 153-day block of five months, as the algorithm
/// of Howard Hinnant's `civil_from_days` counts them.
const FIVE_MONTH_DAYS: i64 = 153;
const MONTHS_PER_BLOCK: i64 = 5;
const MONTH_ROUNDING: i64 = 2;

/// March, first month of the count, and the months that end it, January and February,
/// which belong to the next civil year.
const MARCH: i64 = 3;
const MONTHS_FROM_MARCH_TO_DECEMBER: i64 = 10;
const MONTHS_IN_YEAR: i64 = 12;
const FEBRUARY: i64 = 2;

/// The day `seconds` after 1970-01-01T00:00:00Z, written `YYYY-MM-DD` as the C# About page
/// writes its build date; `None` before 1970, which no build is.
#[must_use]
pub fn civil_date(seconds: i64) -> Option<String> {
    if seconds < 0 {
        return None;
    }
    let days = seconds / SECONDS_PER_DAY + EPOCH_FROM_MARCH_ZERO;
    let era = days / DAYS_PER_ERA;
    let day_of_era = days % DAYS_PER_ERA;
    let year_of_era = (day_of_era - day_of_era / DAYS_PER_4_YEARS
        + day_of_era / DAYS_PER_100_YEARS
        - day_of_era / LAST_DAY_OF_ERA)
        / DAYS_PER_YEAR;
    let day_of_year = day_of_era
        - (DAYS_PER_YEAR * year_of_era + year_of_era / LEAP_CYCLE - year_of_era / CENTURY);
    let month_from_march = (MONTHS_PER_BLOCK * day_of_year + MONTH_ROUNDING) / FIVE_MONTH_DAYS;
    let day =
        day_of_year - (FIVE_MONTH_DAYS * month_from_march + MONTH_ROUNDING) / MONTHS_PER_BLOCK + 1;
    let month = if month_from_march < MONTHS_FROM_MARCH_TO_DECEMBER {
        month_from_march + MARCH
    } else {
        month_from_march + MARCH - MONTHS_IN_YEAR
    };
    let year = year_of_era + era * YEARS_PER_ERA + i64::from(month <= FEBRUARY);
    Some(format!("{year:04}-{month:02}-{day:02}"))
}

#[cfg(test)]
mod tests {
    use super::civil_date;

    #[test]
    fn instants_are_written_as_their_utc_day() {
        assert_eq!(civil_date(0).as_deref(), Some("1970-01-01"));
        assert_eq!(civil_date(86_399).as_deref(), Some("1970-01-01"));
        assert_eq!(civil_date(86_400).as_deref(), Some("1970-01-02"));
        // A leap day, and the days around the end of a leap February.
        assert_eq!(civil_date(951_782_400).as_deref(), Some("2000-02-29"));
        assert_eq!(civil_date(951_868_800).as_deref(), Some("2000-03-01"));
        assert_eq!(civil_date(1_709_164_800).as_deref(), Some("2024-02-29"));
        // The last second of a year, and the first of the next.
        assert_eq!(civil_date(1_735_689_599).as_deref(), Some("2024-12-31"));
        assert_eq!(civil_date(1_735_689_600).as_deref(), Some("2025-01-01"));
        assert_eq!(civil_date(1_791_331_200).as_deref(), Some("2026-10-07"));
        assert_eq!(civil_date(-1), None, "before 1970");
    }
}
