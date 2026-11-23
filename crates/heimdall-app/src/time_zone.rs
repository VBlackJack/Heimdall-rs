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

//! This computer's time zone, as an RDP server is told it, as mstsc does: a server whose
//! policy redirects the time zone then shows the session this computer's time.
//!
//! On Windows, the zone the system clock follows, read from the registry: its biases, the
//! rules of this year's two changes, and the zone's English names. Elsewhere, none: the
//! server is told UTC, as before.
//!
//! And for the date and time converter, this computer's offset from UTC at a moment, and
//! the zones it knows.

use heimdall_core::tools::date_time::Instant;
use heimdall_core::tools::time_zone_rules::{self, TimeZoneEntry, ZoneRules};
use heimdall_rdp::TimeZone;
#[cfg(windows)]
use heimdall_rdp::Transition;
use jiff::Timestamp;
use jiff::tz::{AmbiguousOffset, Offset, TimeZone as IanaZone};

/// The zone the system clock follows, with this year's change rules.
#[cfg(windows)]
const ZONE_KEY: &str = r"SYSTEM\CurrentControlSet\Control\TimeZoneInformation";

/// The zones Windows knows, each under its key name, with its names.
#[cfg(windows)]
const ZONES_KEY: &str = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Time Zones";

/// This computer's time zone; `None` when it cannot be read, or off Windows.
#[must_use]
pub fn local() -> Option<TimeZone> {
    #[cfg(windows)]
    {
        use windows_registry::LOCAL_MACHINE;
        let zone = LOCAL_MACHINE.open(ZONE_KEY).ok()?;
        let signed = |name: &str| zone.get_u32(name).ok().map(u32::cast_signed);
        let rule = |name: &str| {
            zone.get_value(name)
                .ok()
                .and_then(|value| Transition::from_systemtime(&value))
        };
        // "Adjust for daylight saving time automatically" cleared: the clock never changes.
        let adjusts = zone.get_u32("DynamicDaylightTimeDisabled").unwrap_or(0) == 0;
        // The names the registry keeps here point into a resource file; the zone's own key
        // has them written out.
        let key_name = zone.get_string("TimeZoneKeyName").unwrap_or_default();
        let names = LOCAL_MACHINE.open(format!(r"{ZONES_KEY}\{key_name}")).ok();
        let name = |value: &str| {
            names
                .as_ref()
                .and_then(|names| names.get_string(value).ok())
                .unwrap_or_else(|| key_name.clone())
        };
        Some(TimeZone {
            bias: signed("Bias")?,
            standard_name: name("Std"),
            standard_start: rule("StandardStart").filter(|_| adjusts),
            standard_bias: signed("StandardBias").unwrap_or(0),
            daylight_name: name("Dlt"),
            daylight_start: rule("DaylightStart").filter(|_| adjusts),
            daylight_bias: signed("DaylightBias").unwrap_or(0),
        })
    }
    #[cfg(not(windows))]
    {
        None
    }
}

/// The offset east of UTC, in seconds, of this computer's clock at `instant`, as .NET's
/// `ToLocalTime`: the system's own zone and its history.
#[must_use]
pub fn local_offset_at(instant: Instant) -> i32 {
    offset_at_in(&IanaZone::system(), instant.unix_seconds())
}

/// The offset east of UTC, in seconds, of `zone` at `unix_seconds`; 0 for a time jiff cannot
/// hold.
fn offset_at_in(zone: &IanaZone, unix_seconds: i64) -> i32 {
    Timestamp::from_second(unix_seconds).map_or(0, |instant| zone.to_offset(instant).seconds())
}

/// The offset east of UTC, in seconds, of this computer's clock when it shows
/// `wall_seconds`, seconds since 1970-01-01 on the wall clock: the earlier of two when the
/// clock goes back, the one before a gap when it skips ahead.
#[must_use]
pub fn local_offset_of_wall_clock(wall_seconds: i64) -> i32 {
    wall_clock_offset_in(&IanaZone::system(), wall_seconds)
}

/// [`local_offset_of_wall_clock`] in `zone`.
fn wall_clock_offset_in(zone: &IanaZone, wall_seconds: i64) -> i32 {
    // The wall clock's seconds read as a civil date and time, whatever the zone: the
    // calendar of the UTC reading of the same count.
    let Ok(wall) = Timestamp::from_second(wall_seconds) else {
        return 0;
    };
    let civil = Offset::UTC.to_datetime(wall);
    match zone.to_ambiguous_timestamp(civil).offset() {
        // The earlier of two when the clock goes back, as .NET's daylight time.
        AmbiguousOffset::Unambiguous { offset } | AmbiguousOffset::Fold { before: offset, .. } => {
            offset.seconds()
        }
        // A time the clock skips: the offset before the jump, as .NET's standard time. (The
        // chrono code probed the instant an hour earlier, reading the wall clock as UTC, which
        // landed after the jump in zones east of UTC.)
        AmbiguousOffset::Gap { before, .. } => before.seconds(),
    }
}

/// This computer's clock, for the scheduled connections' timing.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl heimdall_core::scheduled::LocalClock for SystemClock {
    fn offset_at(&self, unix_seconds: i64) -> i32 {
        local_offset_at(Instant::from_unix_seconds(unix_seconds))
    }

    fn offset_of_wall_clock(&self, wall_seconds: i64) -> i32 {
        local_offset_of_wall_clock(wall_seconds)
    }
}

/// Now, in whole seconds since the Unix epoch.
#[must_use]
pub fn unix_now() -> i64 {
    Instant::now().unix_seconds()
}

/// The time zones this computer knows, as .NET's `TimeZoneInfo.GetSystemTimeZones`, in its
/// order: on Windows, the registry's, each under its key with its display name and its
/// `TZI` rule; elsewhere, the IANA zones of the system's zone information, each with the
/// rule closing its file. Empty when none can be read.
#[must_use]
pub fn system_zones() -> Vec<TimeZoneEntry> {
    let mut zones = read_zones();
    time_zone_rules::sort(&mut zones);
    zones
}

#[cfg(windows)]
fn read_zones() -> Vec<TimeZoneEntry> {
    use windows_registry::LOCAL_MACHINE;
    let Ok(root) = LOCAL_MACHINE.open(ZONES_KEY) else {
        return Vec::new();
    };
    let Ok(names) = root.keys() else {
        return Vec::new();
    };
    names
        .filter_map(|id| {
            let zone = root.open(&id).ok()?;
            let rules = ZoneRules::from_windows_tzi(&zone.get_value("TZI").ok()?)?;
            let display_name = zone
                .get_string("Display")
                .unwrap_or_else(|_| time_zone_rules::display_name(&id, rules.standard_seconds));
            Some(TimeZoneEntry {
                id,
                display_name,
                rules,
            })
        })
        .collect()
}

/// Where the system keeps its zone information.
#[cfg(not(windows))]
const ZONEINFO_DIR: &str = "/usr/share/zoneinfo";

/// The table naming its zones, a zone a line, its name in the third column.
#[cfg(not(windows))]
const ZONE_TABLE: &str = "zone.tab";

/// The column of a zone's name in it.
#[cfg(not(windows))]
const ZONE_NAME_COLUMN: usize = 2;

/// What separates its columns.
#[cfg(not(windows))]
const TAB: char = '\t';

#[cfg(not(windows))]
fn read_zones() -> Vec<TimeZoneEntry> {
    let dir = std::path::Path::new(ZONEINFO_DIR);
    let table = std::fs::read_to_string(dir.join(ZONE_TABLE)).unwrap_or_default();
    let names = table
        .lines()
        .filter(|line| !line.starts_with('#'))
        .filter_map(|line| line.split(TAB).nth(ZONE_NAME_COLUMN))
        .chain(std::iter::once(time_zone_rules::UTC_ID));
    let mut zones: Vec<TimeZoneEntry> = names
        .filter_map(|id| {
            let rules = ZoneRules::from_tzif(&std::fs::read(dir.join(id)).ok()?)?;
            Some(TimeZoneEntry {
                id: id.to_owned(),
                display_name: time_zone_rules::display_name(id, rules.standard_seconds),
                rules,
            })
        })
        .collect();
    zones.dedup_by(|a, b| a.id == b.id);
    zones
}

#[cfg(test)]
mod tests {
    #[cfg(windows)]
    #[test]
    fn this_windows_computer_tells_its_zone_with_utc_within_a_day() {
        let zone = super::local().expect("a Windows computer has a time zone");
        // Biases run from UTC-14 to UTC+12, in minutes.
        assert!((-14 * 60..=12 * 60).contains(&zone.bias), "{zone:?}");
        assert!(!zone.standard_name.is_empty(), "{zone:?}");
        assert!(
            !zone.standard_name.starts_with('@'),
            "a name, not a resource reference: {zone:?}"
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn elsewhere_no_zone_is_told() {
        assert_eq!(super::local(), None);
    }

    #[test]
    fn the_local_offset_is_within_a_day_and_the_same_read_either_way() {
        use heimdall_core::tools::date_time::Instant;
        let instant = Instant::from_unix_seconds(1_735_122_645);
        let offset = super::local_offset_at(instant);
        assert!((-14 * 3_600..=14 * 3_600).contains(&offset), "{offset}");
        let wall = instant.unix_seconds() + i64::from(offset);
        assert_eq!(super::local_offset_of_wall_clock(wall), offset);
    }

    /// Paris's offsets: winter and summer time, in seconds east of UTC.
    const PARIS_WINTER: i32 = 3_600;
    const PARIS_SUMMER: i32 = 7_200;

    /// The count of seconds the wall clock shows at the civil date and time given, read as
    /// the count since 1970-01-01 on a clock without a zone.
    fn wall(year: i16, month: i8, day: i8, hour: i8, minute: i8) -> i64 {
        jiff::civil::date(year, month, day)
            .at(hour, minute, 0, 0)
            .to_zoned_datetime(jiff::tz::TimeZone::UTC)
            .expect("a civil time in range")
            .timestamp()
            .as_second()
    }

    fn paris() -> jiff::tz::TimeZone {
        jiff::tz::TimeZone::get("Europe/Paris").expect("Paris is in the zone database")
    }

    #[test]
    fn a_wall_clock_time_outside_a_change_has_its_one_offset() {
        let zone = paris();
        assert_eq!(
            super::wall_clock_offset_in(&zone, wall(2026, 1, 15, 12, 0)),
            PARIS_WINTER
        );
        assert_eq!(
            super::wall_clock_offset_in(&zone, wall(2026, 7, 15, 12, 0)),
            PARIS_SUMMER
        );
    }

    #[test]
    fn a_wall_clock_time_the_clock_shows_twice_takes_the_earlier_offset() {
        // 2026-10-25: at 03:00 summer time the clock goes back to 02:00 winter time, so
        // 02:30 is shown twice; the first is summer time, as .NET's daylight time.
        let zone = paris();
        assert_eq!(
            super::wall_clock_offset_in(&zone, wall(2026, 10, 25, 2, 30)),
            PARIS_SUMMER
        );
        assert_eq!(
            super::wall_clock_offset_in(&zone, wall(2026, 10, 25, 1, 59)),
            PARIS_SUMMER,
            "before the overlap"
        );
        assert_eq!(
            super::wall_clock_offset_in(&zone, wall(2026, 10, 25, 3, 0)),
            PARIS_WINTER,
            "after the overlap"
        );
    }

    #[test]
    fn a_wall_clock_time_the_clock_skips_takes_the_offset_before_the_gap() {
        // 2026-03-29: at 02:00 winter time the clock jumps to 03:00 summer time, so 02:30
        // is never shown; the offset before the jump, as .NET's standard time.
        let zone = paris();
        assert_eq!(
            super::wall_clock_offset_in(&zone, wall(2026, 3, 29, 2, 30)),
            PARIS_WINTER
        );
        assert_eq!(
            super::wall_clock_offset_in(&zone, wall(2026, 3, 29, 1, 59)),
            PARIS_WINTER,
            "before the gap"
        );
        assert_eq!(
            super::wall_clock_offset_in(&zone, wall(2026, 3, 29, 3, 0)),
            PARIS_SUMMER,
            "after the gap"
        );
    }

    #[test]
    fn the_offset_at_an_instant_follows_the_zone_history() {
        let zone = paris();
        // 2026-03-29 00:59:59 UTC is still winter time, 01:00:00 UTC is summer time.
        let change = wall(2026, 3, 29, 1, 0);
        assert_eq!(super::offset_at_in(&zone, change - 1), PARIS_WINTER);
        assert_eq!(super::offset_at_in(&zone, change), PARIS_SUMMER);
        assert_eq!(super::offset_at_in(&zone, i64::MAX), 0, "out of range");
    }

    #[test]
    fn the_zones_listed_are_sorted_and_hold_utc_when_the_system_has_any() {
        let zones = super::system_zones();
        assert!(
            zones
                .windows(2)
                .all(|pair| pair[0].rules.standard_seconds <= pair[1].rules.standard_seconds)
        );
        if !zones.is_empty() {
            assert!(
                zones
                    .iter()
                    .any(|zone| zone.id == heimdall_core::tools::time_zone_rules::UTC_ID),
                "{zones:?}"
            );
        }
    }
}
