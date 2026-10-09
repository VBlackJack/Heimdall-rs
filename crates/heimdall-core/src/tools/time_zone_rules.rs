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

//! The time zones the date and time converter offers, as the C# lists the system's with
//! `TimeZoneInfo.GetSystemTimeZones` and converts with `TimeZoneInfo.ConvertTime`
//! (`DateTimeConverterViewModel.cs:155-223`), without a time zone database: each zone's
//! current rule, its standard offset and, when it has one, its daylight saving time from a
//! yearly change to another. On Windows the rule is the registry's `TZI`; elsewhere the
//! POSIX `TZ` rule closing a zone's `TZif` file.

use super::date_time::{self, DAY_SECONDS, Instant};

/// Seconds in a minute.
const MINUTE_SECONDS: i32 = 60;

/// Seconds in an hour.
const HOUR_SECONDS: i32 = 3_600;

/// Where a POSIX change happens when its rule names no time: 02:00.
const DEFAULT_CHANGE_SECONDS: i32 = 2 * HOUR_SECONDS;

/// The week that means the last of a month.
const LAST_WEEK: u32 = 5;

/// Days in a week.
const WEEK_DAYS: u32 = 7;

/// Bytes of a Windows `SYSTEMTIME`.
const SYSTEMTIME_BYTES: usize = 16;

/// Bytes of a Windows `REG_TZI_FORMAT`: three biases, then two `SYSTEMTIME`s.
pub const TZI_BYTES: usize = 12 + 2 * SYSTEMTIME_BYTES;

/// The signature of a `TZif` file.
const TZIF_MAGIC: &[u8] = b"TZif";

/// Where a `TZif` file says its version.
const TZIF_VERSION_AT: usize = 4;

/// A yearly change, as a POSIX `Mm.w.d/time` rule and a Windows `SYSTEMTIME` in its yearly
/// form both say it: the `week`th `weekday` of `month`, 5 being the last, at `seconds` of
/// the wall clock in force before it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Transition {
    /// Month, 1 to 12.
    pub month: u32,
    /// Which of the month's such weekdays, 1 to 5, 5 the last.
    pub week: u32,
    /// Day of the week, Sunday being 0.
    pub weekday: u32,
    /// Seconds into that day, which may be past it or before it.
    pub seconds: i32,
}

impl Transition {
    /// The wall-clock moment of the change in `year`, in seconds since 1970-01-01.
    fn wall_seconds(self, year: i64) -> i64 {
        let first = date_time::days_from_civil(year, self.month, 1);
        let first_weekday = u32::try_from(date_time::weekday_of_days(first)).unwrap_or(0);
        let first_match = 1 + (self.weekday + WEEK_DAYS - first_weekday) % WEEK_DAYS;
        let mut day = first_match + WEEK_DAYS * (self.week.clamp(1, LAST_WEEK) - 1);
        while day > date_time::days_in_month(year, self.month) {
            day -= WEEK_DAYS;
        }
        date_time::days_from_civil(year, self.month, day) * DAY_SECONDS + i64::from(self.seconds)
    }
}

/// A zone's daylight saving time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Daylight {
    /// Its offset east of UTC, in seconds.
    pub offset_seconds: i32,
    /// When it starts, on the standard wall clock.
    pub start: Transition,
    /// When it ends, on its own wall clock.
    pub end: Transition,
}

/// A zone's current rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ZoneRules {
    /// The standard offset east of UTC, in seconds, as .NET's `BaseUtcOffset`.
    pub standard_seconds: i32,
    /// Its daylight saving time, when it has one.
    pub daylight: Option<Daylight>,
}

impl ZoneRules {
    /// A zone always `seconds` east of UTC.
    #[must_use]
    pub const fn fixed(seconds: i32) -> Self {
        Self {
            standard_seconds: seconds,
            daylight: None,
        }
    }

    /// Its offset east of UTC at `instant`, in seconds.
    #[must_use]
    pub fn offset_at(&self, instant: Instant) -> i32 {
        let Some(daylight) = self.daylight else {
            return self.standard_seconds;
        };
        let utc = instant.unix_seconds();
        let year = instant.wall_clock(self.standard_seconds).year;
        let start = daylight.start.wall_seconds(year) - i64::from(self.standard_seconds);
        let end = daylight.end.wall_seconds(year) - i64::from(daylight.offset_seconds);
        let in_daylight = if start <= end {
            (start..end).contains(&utc)
        } else {
            // The southern hemisphere's: from late in the year into the next.
            utc >= start || utc < end
        };
        if in_daylight {
            daylight.offset_seconds
        } else {
            self.standard_seconds
        }
    }

    /// The rule a Windows registry `TZI` value holds: the bias, the standard and daylight
    /// biases, in minutes west of UTC, then the change to standard time and the change to
    /// daylight time as `SYSTEMTIME`s, the month 0 when the zone does not change. `None`
    /// when the value is not one.
    #[must_use]
    pub fn from_windows_tzi(bytes: &[u8]) -> Option<Self> {
        if bytes.len() != TZI_BYTES {
            return None;
        }
        let bias = |at: usize| {
            i32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
        };
        let (base, standard_bias, daylight_bias) = (bias(0), bias(4), bias(8));
        let standard_seconds = -(base + standard_bias) * MINUTE_SECONDS;
        let to_standard = systemtime(&bytes[12..12 + SYSTEMTIME_BYTES]);
        let to_daylight = systemtime(&bytes[12 + SYSTEMTIME_BYTES..]);
        let daylight = match (to_daylight, to_standard) {
            (Some(start), Some(end)) => Some(Daylight {
                offset_seconds: -(base + daylight_bias) * MINUTE_SECONDS,
                start,
                end,
            }),
            _ => None,
        };
        Some(Self {
            standard_seconds,
            daylight,
        })
    }

    /// The rule a POSIX `TZ` string says, such as `CET-1CEST,M3.5.0,M10.5.0/3`: names,
    /// offsets west of UTC, and changes in the `Mm.w.d` form. `None` for anything else,
    /// a change by day of the year among them.
    #[must_use]
    pub fn from_posix(rule: &str) -> Option<Self> {
        let mut reader = PosixReader { rest: rule };
        reader.name()?;
        let standard_seconds = -reader.offset()?;
        if reader.rest.is_empty() {
            return Some(Self::fixed(standard_seconds));
        }
        reader.name()?;
        let offset_seconds = if reader.rest.starts_with(',') {
            standard_seconds + HOUR_SECONDS
        } else {
            -reader.offset()?
        };
        let start = reader.change()?;
        let end = reader.change()?;
        if !reader.rest.is_empty() {
            return None;
        }
        Some(Self {
            standard_seconds,
            daylight: Some(Daylight {
                offset_seconds,
                start,
                end,
            }),
        })
    }

    /// The rule closing a `TZif` file, version 2 or later: the POSIX `TZ` string of its last
    /// line, which holds for the times past its table. `None` for a file of version 1, or
    /// without such a rule.
    #[must_use]
    pub fn from_tzif(bytes: &[u8]) -> Option<Self> {
        if !bytes.starts_with(TZIF_MAGIC) || bytes.get(TZIF_VERSION_AT).is_none_or(|v| *v < b'2') {
            return None;
        }
        let body = bytes.strip_suffix(b"\n")?;
        let start = body.iter().rposition(|byte| *byte == b'\n')? + 1;
        Self::from_posix(std::str::from_utf8(&body[start..]).ok()?)
    }
}

/// The yearly change a `SYSTEMTIME` in its yearly form says: year 0, the day the
/// weekday's occurrence; `None` for month 0, no change.
fn systemtime(bytes: &[u8]) -> Option<Transition> {
    let field = |index: usize| u16::from_le_bytes([bytes[2 * index], bytes[2 * index + 1]]);
    let (month, weekday, week) = (field(1), field(2), field(3));
    let (hour, minute, second) = (field(4), field(5), field(6));
    if month == 0 || month > 12 || weekday > 6 {
        return None;
    }
    Some(Transition {
        month: u32::from(month),
        week: u32::from(week),
        weekday: u32::from(weekday),
        seconds: i32::from(hour) * HOUR_SECONDS
            + i32::from(minute) * MINUTE_SECONDS
            + i32::from(second),
    })
}

/// A reader of a POSIX `TZ` string.
struct PosixReader<'a> {
    rest: &'a str,
}

impl PosixReader<'_> {
    /// A zone's name: letters, or anything between `<` and `>`.
    fn name(&mut self) -> Option<()> {
        if let Some(quoted) = self.rest.strip_prefix('<') {
            let end = quoted.find('>')?;
            self.rest = &quoted[end + 1..];
            return Some(());
        }
        let length = self
            .rest
            .bytes()
            .take_while(u8::is_ascii_alphabetic)
            .count();
        if length < 3 {
            return None;
        }
        self.rest = &self.rest[length..];
        Some(())
    }

    /// A time, `[+-]hh[:mm[:ss]]`, in seconds.
    fn offset(&mut self) -> Option<i32> {
        let negative = self.rest.starts_with('-');
        if negative || self.rest.starts_with('+') {
            self.rest = &self.rest[1..];
        }
        let mut seconds = 0;
        for (index, unit) in [HOUR_SECONDS, MINUTE_SECONDS, 1].into_iter().enumerate() {
            if index > 0 {
                match self.rest.strip_prefix(':') {
                    Some(rest) => self.rest = rest,
                    None => break,
                }
            }
            let length = self.rest.bytes().take_while(u8::is_ascii_digit).count();
            if length == 0 {
                return None;
            }
            let value: i32 = self.rest[..length].parse().ok()?;
            self.rest = &self.rest[length..];
            seconds += value * unit;
        }
        Some(if negative { -seconds } else { seconds })
    }

    /// A change, `,Mm.w.d[/time]`.
    fn change(&mut self) -> Option<Transition> {
        self.rest = self.rest.strip_prefix(",M")?;
        let mut fields = [0_u32; 3];
        for (index, field) in fields.iter_mut().enumerate() {
            if index > 0 {
                self.rest = self.rest.strip_prefix('.')?;
            }
            let length = self.rest.bytes().take_while(u8::is_ascii_digit).count();
            *field = self.rest.get(..length)?.parse().ok()?;
            self.rest = &self.rest[length..];
        }
        let [month, week, weekday] = fields;
        if !(1..=12).contains(&month) || !(1..=LAST_WEEK).contains(&week) || weekday > 6 {
            return None;
        }
        let seconds = match self.rest.strip_prefix('/') {
            Some(rest) => {
                self.rest = rest;
                self.offset()?
            }
            None => DEFAULT_CHANGE_SECONDS,
        };
        Some(Transition {
            month,
            week,
            weekday,
            seconds,
        })
    }
}

/// A zone of the list, as the C# `TimezoneItem`: its identifier, the name the list shows
/// and its rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimeZoneEntry {
    /// Its identifier: the registry's key on Windows, the IANA name elsewhere.
    pub id: String,
    /// What the list shows, as .NET's `DisplayName`.
    pub display_name: String,
    /// Its rule.
    pub rules: ZoneRules,
}

/// The identifier of UTC, which the list starts on, as the C# `SelectedTimezoneId`.
pub const UTC_ID: &str = "UTC";

/// The name a zone without one of its own shows, as .NET writes it on Linux:
/// `(UTC+01:00) Europe/Paris`, `(UTC) UTC` for no offset.
#[must_use]
pub fn display_name(id: &str, standard_seconds: i32) -> String {
    if standard_seconds == 0 {
        return format!("(UTC) {id}");
    }
    let sign = if standard_seconds < 0 { '-' } else { '+' };
    let minutes = standard_seconds.abs() / MINUTE_SECONDS;
    format!(
        "(UTC{sign}{:02}:{:02}) {id}",
        minutes / MINUTE_SECONDS,
        minutes % MINUTE_SECONDS
    )
}

/// `zones` in .NET's order of `GetSystemTimeZones`: by standard offset, then by name.
pub fn sort(zones: &mut [TimeZoneEntry]) {
    zones.sort_by(|a, b| {
        a.rules
            .standard_seconds
            .cmp(&b.rules.standard_seconds)
            .then_with(|| a.display_name.cmp(&b.display_name))
    });
}
