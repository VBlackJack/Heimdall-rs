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

//! Moments as the date and time converter reads and writes them, as the C#
//! `DateTimeParser` (`Heimdall.Core/Temporal/DateTimeParser.cs`) and
//! `RelativeTimeComputer` (`RelativeTimeComputer.cs`): a Unix time in seconds or
//! milliseconds, or a written date; written back as .NET's round-trip "o" form; and how far
//! it is from now, in the largest unit under the next.
//!
//! A moment is held to .NET's tick, a tenth of a microsecond, between the years 1 and 9999
//! as a `DateTimeOffset`, without a time zone database: the offset of a date written
//! without one is asked of the caller.

/// Ticks in a second, as .NET's `TimeSpan.TicksPerSecond`.
pub const TICKS_PER_SECOND: i64 = 10_000_000;

/// Ticks in a millisecond.
const TICKS_PER_MILLISECOND: i64 = 10_000;

/// Seconds in a minute.
const MINUTE_SECONDS: i64 = 60;

/// Seconds in an hour.
const HOUR_SECONDS: i64 = 3_600;

/// Seconds in a day.
pub const DAY_SECONDS: i64 = 86_400;

/// The earliest Unix time in seconds, 0001-01-01, as the C# `MinUnixSeconds`.
pub const MIN_UNIX_SECONDS: i64 = -62_135_596_800;

/// The latest, 3000-01-01, as the C# `MaxUnixSeconds`.
pub const MAX_UNIX_SECONDS: i64 = 32_503_680_000;

/// The earliest Unix time in milliseconds, as the C# `MinUnixMilliseconds`.
pub const MIN_UNIX_MILLISECONDS: i64 = MIN_UNIX_SECONDS * 1_000;

/// The latest, as the C# `MaxUnixMilliseconds`.
pub const MAX_UNIX_MILLISECONDS: i64 = MAX_UNIX_SECONDS * 1_000;

/// Digits from which a number is milliseconds whatever its value, as the C#'s 13.
const MILLISECONDS_MIN_LENGTH: usize = 13;

/// The last moment .NET holds, 9999-12-31T23:59:59.9999999, in seconds.
const MAX_DATE_SECONDS: i64 = 253_402_300_799;

/// The widest offset .NET allows a `DateTimeOffset`, 14 hours.
const MAX_OFFSET_SECONDS: i64 = 14 * HOUR_SECONDS;

/// Digits of the fraction .NET writes, its ticks.
const FRACTION_DIGITS: usize = 7;

/// Days from 0000-03-01 to 1970-01-01, for the civil calendar's arithmetic.
const EPOCH_SHIFT_DAYS: i64 = 719_468;

/// Days in a 400-year era.
const ERA_DAYS: i64 = 146_097;

/// Years in an era.
const ERA_YEARS: i64 = 400;

/// The weekday of 1970-01-01, a Thursday, Sunday being 0.
const EPOCH_WEEKDAY: i64 = 4;

/// Days in a week.
const WEEK_DAYS: i64 = 7;

/// The first and last year .NET holds.
const MIN_YEAR: i64 = 1;
const MAX_YEAR: i64 = 9_999;

/// Hours on a twelve-hour clock.
const HALF_DAY_HOURS: i64 = 12;

/// A moment, in ticks since the Unix epoch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Instant {
    ticks: i64,
}

impl Instant {
    /// The moment `seconds` after the epoch.
    #[must_use]
    pub const fn from_unix_seconds(seconds: i64) -> Self {
        Self {
            ticks: seconds * TICKS_PER_SECOND,
        }
    }

    /// The moment `millis` after the epoch.
    #[must_use]
    pub const fn from_unix_milliseconds(millis: i64) -> Self {
        Self {
            ticks: millis * TICKS_PER_MILLISECOND,
        }
    }

    /// The moment `ticks` after the epoch.
    #[must_use]
    pub const fn from_unix_ticks(ticks: i64) -> Self {
        Self { ticks }
    }

    /// Now, from the system clock.
    #[must_use]
    pub fn now() -> Self {
        let since = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH);
        let ticks = since.map_or(0, |since| {
            i64::try_from(since.as_nanos() / 100).unwrap_or(i64::MAX)
        });
        Self { ticks }
    }

    /// Its ticks since the epoch.
    #[must_use]
    pub const fn unix_ticks(self) -> i64 {
        self.ticks
    }

    /// Its whole seconds since the epoch, rounded down, as .NET's `ToUnixTimeSeconds`.
    #[must_use]
    pub const fn unix_seconds(self) -> i64 {
        self.ticks.div_euclid(TICKS_PER_SECOND)
    }

    /// The wall clock `offset_seconds` east of UTC shows at it.
    #[must_use]
    pub fn wall_clock(self, offset_seconds: i32) -> WallClock {
        let local = self.ticks + i64::from(offset_seconds) * TICKS_PER_SECOND;
        WallClock::of_ticks(local)
    }
}

/// What a wall clock shows: a date, a time and its fraction, and the weekday.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WallClock {
    /// Year.
    pub year: i64,
    /// Month, 1 to 12.
    pub month: u32,
    /// Day of the month, from 1.
    pub day: u32,
    /// Hour, 0 to 23.
    pub hour: u32,
    /// Minute.
    pub minute: u32,
    /// Second.
    pub second: u32,
    /// Ticks into the second.
    pub ticks: u32,
    /// Day of the week, Sunday being 0, as .NET's `DayOfWeek`.
    pub weekday: u32,
}

impl WallClock {
    /// The wall clock `ticks` after midnight of 1970-01-01 shows.
    fn of_ticks(ticks: i64) -> Self {
        let seconds = ticks.div_euclid(TICKS_PER_SECOND);
        let fraction = ticks.rem_euclid(TICKS_PER_SECOND);
        let days = seconds.div_euclid(DAY_SECONDS);
        let of_day = seconds.rem_euclid(DAY_SECONDS);
        let (year, month, day) = civil_from_days(days);
        let small = |value: i64| u32::try_from(value).unwrap_or(0);
        Self {
            year,
            month,
            day,
            hour: small(of_day / HOUR_SECONDS),
            minute: small(of_day % HOUR_SECONDS / MINUTE_SECONDS),
            second: small(of_day % MINUTE_SECONDS),
            ticks: small(fraction),
            weekday: small(weekday_of_days(days)),
        }
    }

    /// Its time as `HH:mm:ss`.
    #[must_use]
    pub fn time_text(&self) -> String {
        format!("{:02}:{:02}:{:02}", self.hour, self.minute, self.second)
    }

    /// Its date and time as .NET's "o" writes them, without the offset:
    /// `2024-04-05T19:34:38.0000000`.
    fn round_trip_text(&self) -> String {
        format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:0width$}",
            self.year,
            self.month,
            self.day,
            self.hour,
            self.minute,
            self.second,
            self.ticks,
            width = FRACTION_DIGITS
        )
    }
}

/// `instant` in UTC as .NET's "o" writes a UTC `DateTime`: `2024-04-05T19:34:38.0000000Z`,
/// as the C# `IsoUtcText` (`DateTimeConverterViewModel.cs:192`).
#[must_use]
pub fn round_trip_utc(instant: Instant) -> String {
    format!("{}Z", instant.wall_clock(0).round_trip_text())
}

/// `instant` at `offset_seconds` as .NET's "o" writes a `DateTimeOffset`:
/// `2024-04-05T21:34:38.0000000+02:00`, as the C# `IsoLocalText`
/// (`DateTimeConverterViewModel.cs:193`).
#[must_use]
pub fn round_trip_with_offset(instant: Instant, offset_seconds: i32) -> String {
    let sign = if offset_seconds < 0 { '-' } else { '+' };
    let minutes = i64::from(offset_seconds).abs() / MINUTE_SECONDS;
    format!(
        "{}{sign}{:02}:{:02}",
        instant.wall_clock(offset_seconds).round_trip_text(),
        minutes / MINUTE_SECONDS,
        minutes % MINUTE_SECONDS
    )
}

/// The form the input was read as, as the C# `DateTimeFormat`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetectedFormat {
    /// A Unix time in seconds.
    UnixSeconds,
    /// A Unix time in milliseconds.
    UnixMilliseconds,
    /// A written date, the C#'s `Iso8601`.
    Iso8601,
}

/// A moment read, as the C# `DateTimeParseOutcome`: the moment, the offset it was written
/// at, and how it was written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Parsed {
    /// The moment.
    pub instant: Instant,
    /// The offset east of UTC it was written at; 0 for a Unix time.
    pub offset_seconds: i32,
    /// How it was written.
    pub detected: DetectedFormat,
}

/// The moment `input` names, as the C# `DateTimeParser.Parse` (`DateTimeParser.cs:38-72`):
/// a whole number is a Unix time, in milliseconds from 13 characters or past
/// [`MAX_UNIX_SECONDS`], else in seconds, each within its bounds; anything else a written
/// date as [`parse_written`] reads it. `local_offset` gives the offset east of UTC, in
/// seconds, of this computer's wall clock at a time it shows, in seconds since 1970-01-01:
/// a date written without an offset is taken as this computer's, as .NET's
/// `DateTimeStyles.None`.
#[must_use]
pub fn parse(input: &str, local_offset: &dyn Fn(i64) -> i32) -> Option<Parsed> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some(value) = parse_integer(trimmed) {
        return parse_unix(trimmed, value);
    }
    parse_written(trimmed, local_offset).map(|(instant, offset_seconds)| Parsed {
        instant,
        offset_seconds,
        detected: DetectedFormat::Iso8601,
    })
}

/// `text` as .NET's `long.TryParse` with `NumberStyles.Integer` reads it, once trimmed: a
/// sign, then digits.
fn parse_integer(text: &str) -> Option<i64> {
    let digits = text.strip_prefix(['-', '+']).unwrap_or(text);
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}

/// The Unix time `value`, written as `input`, as the C# `ParseUnix`
/// (`DateTimeParser.cs:56-69`).
fn parse_unix(input: &str, value: i64) -> Option<Parsed> {
    let milliseconds = input.len() >= MILLISECONDS_MIN_LENGTH || value > MAX_UNIX_SECONDS;
    let (instant, detected) = if milliseconds {
        if !(MIN_UNIX_MILLISECONDS..=MAX_UNIX_MILLISECONDS).contains(&value) {
            return None;
        }
        (
            Instant::from_unix_milliseconds(value),
            DetectedFormat::UnixMilliseconds,
        )
    } else {
        if !(MIN_UNIX_SECONDS..=MAX_UNIX_SECONDS).contains(&value) {
            return None;
        }
        (
            Instant::from_unix_seconds(value),
            DetectedFormat::UnixSeconds,
        )
    };
    Some(Parsed {
        instant,
        offset_seconds: 0,
        detected,
    })
}

/// The English names of the months, as the invariant culture writes and reads them.
const MONTH_NAMES: [&str; 12] = [
    "january",
    "february",
    "march",
    "april",
    "may",
    "june",
    "july",
    "august",
    "september",
    "october",
    "november",
    "december",
];

/// The English names of the days, Sunday first.
const DAY_NAMES: [&str; 7] = [
    "sunday",
    "monday",
    "tuesday",
    "wednesday",
    "thursday",
    "friday",
    "saturday",
];

/// Letters an abbreviated name keeps.
const ABBREVIATION_LENGTH: usize = 3;

/// The names that mark UTC after a time.
const UTC_NAMES: [&str; 3] = ["z", "gmt", "utc"];

/// The marks of a twelve-hour clock.
const MORNING: &str = "am";
const AFTERNOON: &str = "pm";

/// A date read: year, month, day.
type Date = (i64, u32, u32);

/// The moment a written date names, and the offset it was written at, as .NET's
/// `DateTimeOffset.TryParse` with the invariant culture reads the common forms of it:
/// `2024-12-25`, `2024/12/25` or `12/25/2024`, `25 Dec 2024` or `Dec 25, 2024`, after an
/// optional day name; then an optional time, `T` or a space before it, `10:30`,
/// `10:30:45` or `10:30:45.1234567`, with `AM` or `PM`; then an optional offset, `Z`,
/// `GMT`, `UTC`, `+02:00`, `+0200` or `+02`. A date without an offset is this computer's,
/// as `local_offset` gives it. `None` for anything else, a date that does not exist, or a
/// moment out of the years 1 to 9999.
#[must_use]
pub fn parse_written(text: &str, local_offset: &dyn Fn(i64) -> i32) -> Option<(Instant, i32)> {
    let mut cursor = Cursor::new(text);
    cursor.skip_day_name();
    let (year, month, day) = cursor.date()?;
    if !(MIN_YEAR..=MAX_YEAR).contains(&year) || day == 0 || day > days_in_month(year, month) {
        return None;
    }
    let had_separator = cursor.eat_time_separator();
    let time = if had_separator || cursor.starts_with_digit() {
        cursor.time()?
    } else {
        (0, 0, 0, 0)
    };
    let offset = cursor.offset()?;
    cursor.skip_spaces();
    if !cursor.done() {
        return None;
    }
    let (hour, minute, second, ticks) = time;
    let wall_seconds = days_from_civil(year, month, day) * DAY_SECONDS
        + i64::from(hour) * HOUR_SECONDS
        + i64::from(minute) * MINUTE_SECONDS
        + i64::from(second);
    let offset_seconds = match offset {
        WrittenOffset::Seconds(seconds) => seconds,
        WrittenOffset::Absent => local_offset(wall_seconds),
    };
    let utc_seconds = wall_seconds - i64::from(offset_seconds);
    if !(MIN_UNIX_SECONDS..=MAX_DATE_SECONDS).contains(&utc_seconds) {
        return None;
    }
    let instant = Instant::from_unix_ticks(utc_seconds * TICKS_PER_SECOND + i64::from(ticks));
    Some((instant, offset_seconds))
}

/// The offset a written date says.
enum WrittenOffset {
    /// None: the computer's.
    Absent,
    /// This many seconds east of UTC.
    Seconds(i32),
}

/// A reader of a written date.
struct Cursor<'a> {
    rest: &'a str,
}

impl<'a> Cursor<'a> {
    fn new(text: &'a str) -> Self {
        Self { rest: text.trim() }
    }

    fn done(&self) -> bool {
        self.rest.is_empty()
    }

    fn skip_spaces(&mut self) {
        self.rest = self.rest.trim_start();
    }

    fn starts_with_digit(&self) -> bool {
        self.rest.starts_with(|c: char| c.is_ascii_digit())
    }

    /// Takes `prefix`, whatever its case.
    fn eat(&mut self, prefix: &str) -> bool {
        match self.rest.get(..prefix.len()) {
            Some(head) if head.eq_ignore_ascii_case(prefix) => {
                self.rest = &self.rest[prefix.len()..];
                true
            }
            _ => false,
        }
    }

    /// The digits at the start, with how many there were.
    fn number(&mut self) -> Option<(i64, usize)> {
        let length = self.rest.bytes().take_while(u8::is_ascii_digit).count();
        if length == 0 {
            return None;
        }
        let value = self.rest[..length].parse().ok()?;
        self.rest = &self.rest[length..];
        Some((value, length))
    }

    /// The letters at the start.
    fn word(&mut self) -> &'a str {
        let length = self
            .rest
            .bytes()
            .take_while(u8::is_ascii_alphabetic)
            .count();
        let (word, rest) = self.rest.split_at(length);
        self.rest = rest;
        word
    }

    /// An optional day name, its comma and the spaces after it.
    fn skip_day_name(&mut self) {
        let saved = self.rest;
        let word = self.word();
        if !word.is_empty() && name_index(&DAY_NAMES, word).is_some() {
            self.eat(",");
            self.skip_spaces();
        } else {
            self.rest = saved;
        }
    }

    /// A date in one of its forms.
    fn date(&mut self) -> Option<Date> {
        if self.starts_with_digit() {
            let (first, digits) = self.number()?;
            if self.eat("-") || self.eat("/") {
                let (second, _) = self.number()?;
                if !(self.eat("-") || self.eat("/")) {
                    return None;
                }
                let (third, third_digits) = self.number()?;
                return if digits == 4 {
                    Some((first, month_of(second)?, day_of(third)?))
                } else if third_digits == 4 {
                    // The invariant culture's month first: 12/25/2024.
                    Some((third, month_of(first)?, day_of(second)?))
                } else {
                    None
                };
            }
            // 25 Dec 2024.
            self.skip_spaces();
            let month = name_index(&MONTH_NAMES, self.word())?;
            self.skip_spaces();
            let (year, _) = self.number()?;
            return Some((year, month, day_of(first)?));
        }
        // Dec 25, 2024.
        let month = name_index(&MONTH_NAMES, self.word())?;
        self.skip_spaces();
        let (day, _) = self.number()?;
        self.eat(",");
        self.skip_spaces();
        let (year, _) = self.number()?;
        Some((year, month, day_of(day)?))
    }

    /// The `T` or spaces between the date and the time.
    fn eat_time_separator(&mut self) -> bool {
        if self.eat("T") {
            return true;
        }
        let before = self.rest.len();
        self.skip_spaces();
        before != self.rest.len() && self.starts_with_digit()
    }

    /// A time: hours, minutes, optional seconds and fraction, optional AM or PM.
    fn time(&mut self) -> Option<(u32, u32, u32, u32)> {
        let (hour, _) = self.number()?;
        if !self.eat(":") {
            return None;
        }
        let (minute, _) = self.number()?;
        let mut second = 0;
        let mut ticks = 0;
        if self.eat(":") {
            second = self.number()?.0;
            if self.eat(".") {
                let digits: String = self.rest.chars().take_while(char::is_ascii_digit).collect();
                if digits.is_empty() {
                    return None;
                }
                self.rest = &self.rest[digits.len()..];
                let kept: String = digits.chars().take(FRACTION_DIGITS).collect();
                ticks = format!("{kept:0<FRACTION_DIGITS$}").parse().ok()?;
            }
        }
        let saved = self.rest;
        self.skip_spaces();
        let half = if self.eat(MORNING) {
            Some(0)
        } else if self.eat(AFTERNOON) {
            Some(HALF_DAY_HOURS)
        } else {
            self.rest = saved;
            None
        };
        let hour = match half {
            Some(_) if !(1..=HALF_DAY_HOURS).contains(&hour) => return None,
            Some(shift) => hour % HALF_DAY_HOURS + shift,
            None => hour,
        };
        if hour > 23 || minute > 59 || second > 59 {
            return None;
        }
        let small = |value: i64| u32::try_from(value).ok();
        Some((small(hour)?, small(minute)?, small(second)?, ticks))
    }

    /// An optional offset; `None` when it is wrong.
    fn offset(&mut self) -> Option<WrittenOffset> {
        let saved = self.rest;
        self.skip_spaces();
        for name in UTC_NAMES {
            let before = self.rest;
            if self.eat(name) && !self.rest.starts_with(|c: char| c.is_ascii_alphabetic()) {
                return Some(WrittenOffset::Seconds(0));
            }
            self.rest = before;
        }
        let negative = if self.eat("+") {
            false
        } else if self.eat("-") {
            true
        } else {
            self.rest = saved;
            return Some(WrittenOffset::Absent);
        };
        let (first, digits) = self.number()?;
        let (hours, minutes) = match digits {
            1 | 2 => {
                if self.eat(":") {
                    let (minutes, minute_digits) = self.number()?;
                    if minute_digits != 2 {
                        return None;
                    }
                    (first, minutes)
                } else {
                    (first, 0)
                }
            }
            4 => (first / 100, first % 100),
            _ => return None,
        };
        if minutes > 59 {
            return None;
        }
        let seconds = hours * HOUR_SECONDS + minutes * MINUTE_SECONDS;
        if seconds > MAX_OFFSET_SECONDS {
            return None;
        }
        let seconds = if negative { -seconds } else { seconds };
        Some(WrittenOffset::Seconds(i32::try_from(seconds).ok()?))
    }
}

/// The month `value` is, 1 to 12.
fn month_of(value: i64) -> Option<u32> {
    u32::try_from(value)
        .ok()
        .filter(|month| (1..=12).contains(month))
}

/// The day `value` is, from 1; its month says how far.
fn day_of(value: i64) -> Option<u32> {
    u32::try_from(value)
        .ok()
        .filter(|day| (1..=31).contains(day))
}

/// The place in `names`, from 1 for months and from 0 for days, of `word` written whole or
/// in its three letters, whatever its case.
fn name_index(names: &[&str], word: &str) -> Option<u32> {
    let word = word.to_ascii_lowercase();
    if word.len() < ABBREVIATION_LENGTH {
        return None;
    }
    let found = names
        .iter()
        .position(|name| *name == word || name[..ABBREVIATION_LENGTH] == word)?;
    let base = usize::from(names.len() == MONTH_NAMES.len());
    u32::try_from(found + base).ok()
}

/// Whether `year` has a 29 February.
#[must_use]
pub const fn is_leap_year(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

/// Days in `month` of `year`.
#[must_use]
pub const fn days_in_month(year: i64, month: u32) -> u32 {
    match month {
        2 if is_leap_year(year) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// Days from 1970-01-01 to `year`-`month`-`day`, by Howard Hinnant's civil calendar.
#[must_use]
pub fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let month = i64::from(month);
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(ERA_YEARS);
    let year_of_era = year - era * ERA_YEARS;
    let shifted_month = (month + 9) % 12;
    let day_of_year = (153 * shifted_month + 2) / 5 + i64::from(day) - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * ERA_DAYS + day_of_era - EPOCH_SHIFT_DAYS
}

/// The date `days` after 1970-01-01, by the same calendar.
#[must_use]
pub fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let days = days + EPOCH_SHIFT_DAYS;
    let era = days.div_euclid(ERA_DAYS);
    let day_of_era = days - era * ERA_DAYS;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * shifted_month + 2) / 5 + 1;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    };
    let year = year_of_era + era * ERA_YEARS + i64::from(month <= 2);
    let small = |value: i64| u32::try_from(value).unwrap_or(1);
    (year, small(month), small(day))
}

/// The weekday of the day `days` after 1970-01-01, Sunday being 0.
#[must_use]
pub const fn weekday_of_days(days: i64) -> i64 {
    (days + EPOCH_WEEKDAY).rem_euclid(WEEK_DAYS)
}

/// The unit a relative time is told in, as the C# `RelativeTimeUnit`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelativeUnit {
    /// Seconds.
    Seconds,
    /// Minutes.
    Minutes,
    /// Hours.
    Hours,
    /// Days.
    Days,
    /// Months of 30 days.
    Months,
    /// Years of 365 days.
    Years,
}

/// How far a moment is from now, as the C# `RelativeDuration`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RelativeDuration {
    /// The unit.
    pub unit: RelativeUnit,
    /// How many, rounded down.
    pub value: i64,
    /// Whether the moment is past, or now.
    pub is_past: bool,
}

/// Days in the C#'s month.
const MONTH_DAYS: i64 = 30;

/// Days in the C#'s year.
const YEAR_DAYS: i64 = 365;

/// How far `input` is from `now`, as the C# `RelativeTimeComputer.Compute`
/// (`RelativeTimeComputer.cs:105-138`): seconds under a minute, minutes under an hour,
/// hours under a day, days under 30, months of 30 days under 365 days, then years of 365.
#[must_use]
pub fn relative(input: Instant, now: Instant) -> RelativeDuration {
    let difference = i128::from(now.ticks) - i128::from(input.ticks);
    let is_past = difference >= 0;
    let ticks = difference.abs();
    let per = |seconds: i64| i128::from(seconds) * i128::from(TICKS_PER_SECOND);
    let (unit, value) = if ticks < per(MINUTE_SECONDS) {
        (RelativeUnit::Seconds, ticks / per(1))
    } else if ticks < per(HOUR_SECONDS) {
        (RelativeUnit::Minutes, ticks / per(MINUTE_SECONDS))
    } else if ticks < per(DAY_SECONDS) {
        (RelativeUnit::Hours, ticks / per(HOUR_SECONDS))
    } else if ticks < per(MONTH_DAYS * DAY_SECONDS) {
        (RelativeUnit::Days, ticks / per(DAY_SECONDS))
    } else if ticks < per(YEAR_DAYS * DAY_SECONDS) {
        (RelativeUnit::Months, ticks / per(MONTH_DAYS * DAY_SECONDS))
    } else {
        (RelativeUnit::Years, ticks / per(YEAR_DAYS * DAY_SECONDS))
    };
    RelativeDuration {
        unit,
        value: i64::try_from(value).unwrap_or(i64::MAX),
        is_past,
    }
}
