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

//! The time zone a session is told, as mstsc tells it: a server whose policy redirects the
//! time zone shows the session this computer's time, its daylight saving changes included.

use ironrdp::pdu::rdp::client_info::{
    DayOfWeek, DayOfWeekOccurrence, Month, OptionalSystemTime, SystemTime, TimezoneInfo,
};

/// UTF-16 units a name may take, its terminating null left out: the PDU holds 32.
const NAME_UNITS: usize = 31;

/// Bytes of a Windows `SYSTEMTIME`: eight 16-bit fields.
const SYSTEMTIME_LEN: usize = 16;

/// A time zone as Windows describes it: biases in minutes, UTC being local time plus the
/// bias, and the rule of each of the year's two changes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TimeZone {
    /// Minutes from local time to UTC.
    pub bias: i32,
    /// Name of standard time.
    pub standard_name: String,
    /// When standard time starts; `None` without daylight saving.
    pub standard_start: Option<Transition>,
    /// Added to `bias` during standard time.
    pub standard_bias: i32,
    /// Name of daylight saving time.
    pub daylight_name: String,
    /// When daylight saving time starts; `None` without it.
    pub daylight_start: Option<Transition>,
    /// Added to `bias` during daylight saving time.
    pub daylight_bias: i32,
}

/// When one of the year's changes happens: the nth weekday of a month, at a time of day.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Transition {
    month: Month,
    weekday: DayOfWeek,
    occurrence: DayOfWeekOccurrence,
    hour: u16,
    minute: u16,
    second: u16,
}

impl Transition {
    /// The change a Windows `SYSTEMTIME` describes in its yearly form (year 0, the day being
    /// the weekday's occurrence, 5 for the last). `None` for no change (month 0), a date of
    /// one year only, which the PDU cannot carry, and anything out of range.
    #[must_use]
    pub fn from_systemtime(bytes: &[u8]) -> Option<Self> {
        if bytes.len() != SYSTEMTIME_LEN {
            return None;
        }
        let field = |index: usize| u16::from_le_bytes([bytes[2 * index], bytes[2 * index + 1]]);
        let (year, month, weekday, day) = (field(0), field(1), field(2), field(3));
        let (hour, minute, second) = (field(4), field(5), field(6));
        if year != 0 || hour > 23 || minute > 59 || second > 59 {
            return None;
        }
        Some(Self {
            month: month_of(month)?,
            weekday: weekday_of(weekday)?,
            occurrence: occurrence_of(day)?,
            hour,
            minute,
            second,
        })
    }

    fn system_time(self) -> SystemTime {
        SystemTime {
            month: self.month,
            day_of_week: self.weekday,
            day: self.occurrence,
            hour: self.hour,
            minute: self.minute,
            second: self.second,
            milliseconds: 0,
        }
    }
}

impl TimeZone {
    /// As the Client Info PDU carries it.
    pub(crate) fn info(&self) -> TimezoneInfo {
        // Both changes or neither: a year with one change only is no rule a server can apply.
        let (standard, daylight) = match (self.standard_start, self.daylight_start) {
            (Some(standard), Some(daylight)) => {
                (Some(standard.system_time()), Some(daylight.system_time()))
            }
            _ => (None, None),
        };
        TimezoneInfo {
            bias: self.bias,
            standard_name: name(&self.standard_name),
            standard_date: OptionalSystemTime(standard),
            standard_bias: self.standard_bias,
            daylight_name: name(&self.daylight_name),
            daylight_date: OptionalSystemTime(daylight),
            daylight_bias: self.daylight_bias,
        }
    }
}

/// `text` cut to what the PDU holds, whole characters only.
fn name(text: &str) -> String {
    let mut units = 0;
    text.chars()
        .take_while(|character| {
            units += character.len_utf16();
            units <= NAME_UNITS
        })
        .collect()
}

fn month_of(value: u16) -> Option<Month> {
    Some(match value {
        1 => Month::January,
        2 => Month::February,
        3 => Month::March,
        4 => Month::April,
        5 => Month::May,
        6 => Month::June,
        7 => Month::July,
        8 => Month::August,
        9 => Month::September,
        10 => Month::October,
        11 => Month::November,
        12 => Month::December,
        _ => return None,
    })
}

fn weekday_of(value: u16) -> Option<DayOfWeek> {
    Some(match value {
        0 => DayOfWeek::Sunday,
        1 => DayOfWeek::Monday,
        2 => DayOfWeek::Tuesday,
        3 => DayOfWeek::Wednesday,
        4 => DayOfWeek::Thursday,
        5 => DayOfWeek::Friday,
        6 => DayOfWeek::Saturday,
        _ => return None,
    })
}

fn occurrence_of(value: u16) -> Option<DayOfWeekOccurrence> {
    Some(match value {
        1 => DayOfWeekOccurrence::First,
        2 => DayOfWeekOccurrence::Second,
        3 => DayOfWeekOccurrence::Third,
        4 => DayOfWeekOccurrence::Fourth,
        5 => DayOfWeekOccurrence::Last,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `SYSTEMTIME` from its eight fields.
    fn systemtime(fields: [u16; 8]) -> Vec<u8> {
        fields
            .iter()
            .flat_map(|field| field.to_le_bytes())
            .collect()
    }

    #[test]
    fn the_paris_rules_read_from_the_registry_are_sent_as_mstsc_sends_them() {
        // As this computer's registry holds them for "Romance Standard Time".
        let zone = TimeZone {
            bias: -60,
            standard_name: "Romance Standard Time".to_owned(),
            standard_start: Transition::from_systemtime(&systemtime([0, 10, 0, 5, 3, 0, 0, 0])),
            standard_bias: 0,
            daylight_name: "Romance Daylight Time".to_owned(),
            daylight_start: Transition::from_systemtime(&systemtime([0, 3, 0, 5, 2, 0, 0, 0])),
            daylight_bias: -60,
        };
        let info = zone.info();
        assert_eq!(info.bias, -60);
        assert_eq!(info.daylight_bias, -60);
        assert_eq!(info.standard_name, "Romance Standard Time");
        assert_eq!(
            info.standard_date,
            OptionalSystemTime(Some(SystemTime {
                month: Month::October,
                day_of_week: DayOfWeek::Sunday,
                day: DayOfWeekOccurrence::Last,
                hour: 3,
                minute: 0,
                second: 0,
                milliseconds: 0,
            }))
        );
        assert_eq!(
            info.daylight_date,
            OptionalSystemTime(Some(SystemTime {
                month: Month::March,
                day_of_week: DayOfWeek::Sunday,
                day: DayOfWeekOccurrence::Last,
                hour: 2,
                minute: 0,
                second: 0,
                milliseconds: 0,
            }))
        );
    }

    #[test]
    fn no_change_a_dated_one_or_one_out_of_range_is_no_transition() {
        for fields in [
            // No daylight saving.
            [0, 0, 0, 0, 0, 0, 0, 0],
            // A date of one year only.
            [2026, 3, 0, 29, 2, 0, 0, 0],
            [0, 13, 0, 5, 2, 0, 0, 0],
            [0, 3, 7, 5, 2, 0, 0, 0],
            [0, 3, 0, 6, 2, 0, 0, 0],
            [0, 3, 0, 0, 2, 0, 0, 0],
            [0, 3, 0, 5, 24, 0, 0, 0],
        ] {
            assert_eq!(
                Transition::from_systemtime(&systemtime(fields)),
                None,
                "{fields:?}"
            );
        }
        assert_eq!(Transition::from_systemtime(&[0; 15]), None, "too short");
    }

    #[test]
    fn a_zone_with_one_change_only_is_sent_without_any() {
        let zone = TimeZone {
            bias: 300,
            daylight_start: Transition::from_systemtime(&systemtime([0, 3, 0, 2, 2, 0, 0, 0])),
            ..TimeZone::default()
        };
        let info = zone.info();
        assert_eq!(info.bias, 300);
        assert_eq!(info.standard_date, OptionalSystemTime(None));
        assert_eq!(info.daylight_date, OptionalSystemTime(None));
    }

    #[test]
    fn a_long_name_is_cut_to_what_the_pdu_holds_whole_characters_only() {
        let long = "Ouest de l'Europe centrale (heure d'été)";
        let cut = name(long);
        assert_eq!(cut.encode_utf16().count(), NAME_UNITS);
        assert!(long.starts_with(&cut));
        // A character beyond the basic plane takes two units and is not split.
        let wide = format!("{}\u{1F30D}", "a".repeat(NAME_UNITS - 1));
        assert_eq!(name(&wide), "a".repeat(NAME_UNITS - 1));
    }
}
