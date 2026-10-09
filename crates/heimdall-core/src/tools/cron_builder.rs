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

//! Five-field cron expressions as the crontab builder reads, checks, describes and runs
//! them forward, as the C# `CrontabBuilderView` (`Views/Tools/CrontabBuilderView.xaml.cs`)
//! does in its own code: minute, hour, day of the month, month and day of the week, each
//! `*`, a number, a range `a-b`, a step `*/n` or `a/n`, or a list of them.

use std::collections::BTreeSet;

use super::date_time::{self, WallClock};

/// Fields of an expression, as the C#'s 5.
pub const FIELD_COUNT: usize = 5;

/// The field that matches anything.
pub const ANY: &str = "*";

/// The bounds of each field, in order, as the C# `ranges` (`CrontabBuilderView.xaml.cs:285`).
pub const FIELD_RANGES: [(i64, i64); FIELD_COUNT] = [(0, 59), (0, 23), (1, 31), (1, 12), (0, 6)];

/// Runs listed ahead, as the C# `NextRunsCount`.
pub const NEXT_RUNS_COUNT: usize = 5;

/// The most minutes looked at for them, about a year, as the C# `maxIterations`.
const MAX_MINUTES_SCANNED: usize = 525_960;

/// Seconds in a minute, and minutes in an hour.
const SIXTY: i64 = 60;

/// Hours in a day.
const DAY_HOURS: i64 = 24;

/// The English abbreviations of the days, as the invariant culture's `ddd`, Sunday first.
const INVARIANT_DAY_ABBREVIATIONS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

/// The characters a field may hold, as the C# `IsValidCronField`: digits and these.
const FIELD_SYMBOLS: [char; 4] = ['*', '/', '-', ','];

/// What is wrong with a typed expression, as the C# `OnManualInputChanged`
/// (`CrontabBuilderView.xaml.cs:252-312`) checks it in turn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CronProblem {
    /// Not five fields, as `ToolCronValidationFieldCount`.
    FieldCount,
    /// Field `field` (0 to 4) holds a character it may not, as
    /// `ToolCronValidationInvalidField`; `text` is the field.
    InvalidField {
        /// Which field.
        field: usize,
        /// What it holds.
        text: String,
    },
    /// Field `field`'s values are out of its bounds, as `ToolCronValidationOutOfRange`.
    OutOfRange {
        /// Which field.
        field: usize,
    },
}

/// The fields of `text`, split on spaces, as the C# `TryParseCron`
/// (`CrontabBuilderView.xaml.cs:399-413`): five, each of the characters a field may hold;
/// its ranges unchecked.
#[must_use]
pub fn parse(text: &str) -> Option<[String; FIELD_COUNT]> {
    let parts = split(text);
    if parts.len() != FIELD_COUNT || !parts.iter().all(|part| is_valid_field(part)) {
        return None;
    }
    let mut fields: [String; FIELD_COUNT] = Default::default();
    for (field, part) in fields.iter_mut().zip(parts) {
        part.clone_into(field);
    }
    Some(fields)
}

/// What is wrong with `text`, as the C# checks it while it is typed: its count of fields,
/// then each field's characters, then each field's bounds; `None` when nothing is.
#[must_use]
pub fn validate(text: &str) -> Option<CronProblem> {
    let parts = split(text);
    if parts.len() != FIELD_COUNT {
        return Some(CronProblem::FieldCount);
    }
    if let Some((field, part)) = parts
        .iter()
        .enumerate()
        .find(|(_, part)| !is_valid_field(part))
    {
        return Some(CronProblem::InvalidField {
            field,
            text: (*part).to_owned(),
        });
    }
    parts
        .iter()
        .zip(FIELD_RANGES)
        .position(|(part, (min, max))| !field_in_range(part, min, max))
        .map(|field| CronProblem::OutOfRange { field })
}

/// `text` split on spaces, empty parts dropped, as .NET's `Split(' ', RemoveEmptyEntries)`.
fn split(text: &str) -> Vec<&str> {
    text.split(' ').filter(|part| !part.is_empty()).collect()
}

/// Whether `field` holds only digits and `* / - ,`, and something, as the C#
/// `IsValidCronField` (`CrontabBuilderView.xaml.cs:415-425`).
fn is_valid_field(field: &str) -> bool {
    !field.is_empty()
        && field
            .chars()
            .all(|c| c.is_ascii_digit() || FIELD_SYMBOLS.contains(&c))
}

/// `text` as .NET's `int.TryParse` reads it here: digits only, as the field holds no sign
/// nor space.
fn number(text: &str) -> Option<i64> {
    if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    text.parse::<i32>().ok().map(i64::from)
}

/// Whether the numbers of `field` are within `min` and `max`, as the C#
/// `ValidateFieldRange` (`CrontabBuilderView.xaml.cs:317-349`), with its leniency: the
/// start of a step that is not a number is not checked.
fn field_in_range(field: &str, min: i64, max: i64) -> bool {
    if field == ANY {
        return true;
    }
    let within = |value: i64| (min..=max).contains(&value);
    field.split(',').all(|part| {
        if part.contains('/') {
            let pieces: Vec<&str> = part.split('/').collect();
            if pieces.len() != 2 {
                return false;
            }
            if pieces[0] != ANY && number(pieces[0]).is_some_and(|start| !within(start)) {
                return false;
            }
            number(pieces[1]).is_some_and(|step| step > 0)
        } else if part.contains('-') {
            let pieces: Vec<&str> = part.split('-').collect();
            pieces.len() == 2 && pieces.iter().all(|piece| number(piece).is_some_and(within))
        } else {
            number(part).is_some_and(within)
        }
    })
}

/// The values `field` matches between `min` and `max`, as the C# `ExpandField`
/// (`CrontabBuilderView.xaml.cs:518-562`); `None` when it cannot be read.
fn expand(field: &str, min: i64, max: i64) -> Option<BTreeSet<i64>> {
    if field == ANY {
        return Some((min..=max).collect());
    }
    let mut values = BTreeSet::new();
    for part in field.split(',') {
        if part.contains('/') {
            let pieces: Vec<&str> = part.split('/').collect();
            if pieces.len() != 2 {
                return None;
            }
            let step = number(pieces[1]).filter(|step| *step > 0)?;
            let start = if pieces[0] == ANY {
                min
            } else {
                number(pieces[0]).unwrap_or(min)
            };
            let mut value = start;
            while value <= max {
                values.insert(value);
                value += step;
            }
        } else if part.contains('-') {
            let pieces: Vec<&str> = part.split('-').collect();
            if pieces.len() != 2 {
                return None;
            }
            let (start, end) = (number(pieces[0])?, number(pieces[1])?);
            values.extend(start..=end);
        } else {
            values.insert(number(part)?);
        }
    }
    Some(values)
}

/// A date and time to the minute, on this computer's wall clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Minute {
    /// Minutes since 1970-01-01T00:00 of the wall clock.
    minutes: i64,
}

impl Minute {
    /// The minute of `clock`, its seconds dropped.
    #[must_use]
    pub fn of(clock: &WallClock) -> Self {
        let days = date_time::days_from_civil(clock.year, clock.month, clock.day);
        Self {
            minutes: days * DAY_HOURS * SIXTY
                + i64::from(clock.hour) * SIXTY
                + i64::from(clock.minute),
        }
    }

    /// What the wall clock shows at it.
    fn clock(self) -> WallClock {
        date_time::Instant::from_unix_seconds(self.minutes * SIXTY).wall_clock(0)
    }
}

/// The next `count` minutes after `now` that `fields` match, as the C# `CalculateNextRuns`
/// (`CrontabBuilderView.xaml.cs:475-516`): from the minute after now, each minute tried in
/// turn for about a year, a minute matching when its month, day, hour, minute and weekday
/// all do; each written `2026-10-09 14:05 (Fri)`, as the invariant culture's
/// `yyyy-MM-dd HH:mm (ddd)`. None when a field cannot be read.
#[must_use]
pub fn next_runs(fields: &[String; FIELD_COUNT], now: Minute, count: usize) -> Vec<String> {
    let expanded: Option<Vec<BTreeSet<i64>>> = fields
        .iter()
        .zip(FIELD_RANGES)
        .map(|(field, (min, max))| expand(field, min, max))
        .collect();
    let Some(sets) = expanded else {
        return Vec::new();
    };
    let mut runs = Vec::new();
    for minute in (now.minutes + 1..).take(MAX_MINUTES_SCANNED) {
        if runs.len() >= count {
            break;
        }
        let clock = Minute { minutes: minute }.clock();
        let matches = sets[3].contains(&i64::from(clock.month))
            && sets[2].contains(&i64::from(clock.day))
            && sets[1].contains(&i64::from(clock.hour))
            && sets[0].contains(&i64::from(clock.minute))
            && sets[4].contains(&i64::from(clock.weekday));
        if matches {
            runs.push(format!(
                "{:04}-{:02}-{:02} {:02}:{:02} ({})",
                clock.year,
                clock.month,
                clock.day,
                clock.hour,
                clock.minute,
                INVARIANT_DAY_ABBREVIATIONS
                    .get(usize::try_from(clock.weekday).unwrap_or(0))
                    .copied()
                    .unwrap_or_default()
            ));
        }
    }
    runs
}

/// What an expression does, as the C# `DescribeCron` (`CrontabBuilderView.xaml.cs:427-465`)
/// recognises it: its window writes each in its language.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Description {
    /// `* * * * *`.
    EveryMinute,
    /// `0 * * * *`.
    EveryHour,
    /// `0 0 * * *`.
    EveryDay,
    /// `*/n * * * *`: every `.0` minutes, as written.
    EveryNMinutes(String),
    /// A minute and an hour every day; the time as `HH:mm`.
    DailyAt(String),
    /// A minute and an hour on a day of the week: that day (0 to 6) when it is one number,
    /// else the field as written; the time.
    WeeklyAt(WeekDay, String),
    /// A minute and an hour on a day of the month; the day and the time.
    MonthlyAt(i64, String),
    /// Anything else: the fields joined by spaces.
    Custom(String),
}

/// The day of a weekly description.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WeekDay {
    /// A day, Sunday being 0.
    Day(usize),
    /// A field that is not one day, as written.
    Field(String),
}

/// What `fields` do.
#[must_use]
pub fn describe(fields: &[String; FIELD_COUNT]) -> Description {
    let [minute, hour, dom, month, dow] = fields.each_ref().map(String::as_str);
    let rest_any = dom == ANY && month == ANY;
    if minute == ANY && hour == ANY && rest_any && dow == ANY {
        return Description::EveryMinute;
    }
    if minute == "0" && hour == ANY && rest_any && dow == ANY {
        return Description::EveryHour;
    }
    if minute == "0" && hour == "0" && rest_any && dow == ANY {
        return Description::EveryDay;
    }
    if let Some(interval) = minute.strip_prefix("*/")
        && hour == ANY
        && rest_any
        && dow == ANY
    {
        return Description::EveryNMinutes(interval.to_owned());
    }
    if let (Some(m), Some(h)) = (number(minute), number(hour)) {
        let time = format!("{h:02}:{m:02}");
        if rest_any && dow == ANY {
            return Description::DailyAt(time);
        }
        if rest_any {
            let day = number(dow)
                .and_then(|day| usize::try_from(day).ok())
                .filter(|day| *day < INVARIANT_DAY_ABBREVIATIONS.len())
                .map_or_else(|| WeekDay::Field(dow.to_owned()), WeekDay::Day);
            return Description::WeeklyAt(day, time);
        }
        if let Some(day) = number(dom)
            && month == ANY
            && dow == ANY
        {
            return Description::MonthlyAt(day, time);
        }
    }
    Description::Custom(fields.join(" "))
}

/// The expression the five selectors make, as the C# `UpdateFromSelectors`.
#[must_use]
pub fn join(fields: &[String; FIELD_COUNT]) -> String {
    fields.join(" ")
}
