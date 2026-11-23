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
use jiff::tz::{AmbiguousOffset, Offset, TimeZone as IanaZone};
use jiff::{Timestamp, ZonedDateTime};

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

/// Bytes of a Windows `SYSTEMTIME`, as the registry keeps the dates of the yearly changes.
#[cfg(any(windows, test))]
const SYSTEMTIME_BYTES: usize = 16;

/// Seconds in a minute, for the registry's biases, in minutes west of UTC.
#[cfg(any(windows, test))]
const MINUTE_SECONDS: i32 = 60;

/// The rule of the zone the system clock follows, from what the registry says of it: the
/// biases (bias, standard bias, daylight bias), in minutes west of UTC, and the dates of the
/// two changes. `adjusts` is the "Adjust for daylight saving time automatically" setting:
/// cleared, the clock keeps the standard time all year, as the Windows API chrono asked did.
/// `None` for dates that are not `SYSTEMTIME`s.
#[cfg(any(windows, test))]
fn registry_rules(
    biases: [i32; 3],
    standard_start: &[u8],
    daylight_start: &[u8],
    adjusts: bool,
) -> Option<ZoneRules> {
    if !adjusts {
        let [bias, standard_bias, _] = biases;
        return Some(ZoneRules::fixed(-(bias + standard_bias) * MINUTE_SECONDS));
    }
    if standard_start.len() != SYSTEMTIME_BYTES || daylight_start.len() != SYSTEMTIME_BYTES {
        return None;
    }
    // The layout of a zone's `TZI` value: the three biases, then the two dates.
    let mut tzi = Vec::with_capacity(time_zone_rules::TZI_BYTES);
    for value in biases {
        tzi.extend_from_slice(&value.to_le_bytes());
    }
    tzi.extend_from_slice(standard_start);
    tzi.extend_from_slice(daylight_start);
    ZoneRules::from_windows_tzi(&tzi)
}

/// The rule of the zone the Windows clock follows, as Windows says it, honouring the
/// automatic daylight saving setting; `None` when it cannot be read, and off Windows.
///
/// The one source of the local offset on Windows: jiff's own reading of the system zone maps
/// the zone's key to an IANA zone and applies a bundled database, which ignores that setting
/// and falls back to UTC for a key it does not know, as chrono's `Local` never did.
fn system_rules() -> Option<ZoneRules> {
    #[cfg(windows)]
    {
        use windows_registry::LOCAL_MACHINE;
        let zone = LOCAL_MACHINE.open(ZONE_KEY).ok()?;
        let signed = |name: &str| zone.get_u32(name).ok().map(u32::cast_signed);
        let adjusts = zone.get_u32("DynamicDaylightTimeDisabled").unwrap_or(0) == 0;
        let standard_start = zone.get_value("StandardStart").ok()?;
        let daylight_start = zone.get_value("DaylightStart").ok()?;
        registry_rules(
            [
                signed("Bias")?,
                signed("StandardBias").unwrap_or(0),
                signed("DaylightBias").unwrap_or(0),
            ],
            &standard_start,
            &daylight_start,
            adjusts,
        )
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
    match system_rules() {
        Some(rules) => rules.offset_at(instant),
        None => offset_at_in(&IanaZone::system(), instant.unix_seconds()),
    }
}

/// `instant` on this computer's clock, with the offset the clock had then.
#[must_use]
pub fn local_zoned(instant: Timestamp) -> ZonedDateTime {
    let seconds = local_offset_at(Instant::from_unix_seconds(instant.as_second()));
    let offset = Offset::from_seconds(seconds).unwrap_or(Offset::UTC);
    instant.to_zoned_datetime(IanaZone::fixed(offset))
}

/// Now, on this computer's clock.
#[must_use]
pub fn local_now() -> ZonedDateTime {
    local_zoned(Timestamp::now())
}

/// `time` on this computer's clock; `None` for a time jiff cannot hold.
#[must_use]
pub fn local_system_time(time: std::time::SystemTime) -> Option<ZonedDateTime> {
    Timestamp::try_from(time).ok().map(local_zoned)
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
    match system_rules() {
        Some(rules) => rules.offset_of_wall_clock(wall_seconds),
        None => wall_clock_offset_in(&IanaZone::system(), wall_seconds),
    }
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

    /// A yearly `SYSTEMTIME`: the `week`th Sunday of `month` at `hour`, 5 the last.
    fn systemtime(month: u16, week: u16, hour: u16) -> Vec<u8> {
        [0, month, 0, week, hour, 0, 0, 0]
            .iter()
            .flat_map(|field: &u16| field.to_le_bytes())
            .collect()
    }

    /// Central Europe's registry values: bias -60, daylight bias -60, back to standard on
    /// the last Sunday of October at 03:00, to daylight on the last of March at 02:00.
    fn central_europe(adjusts: bool) -> Option<super::ZoneRules> {
        super::registry_rules(
            [-60, 0, -60],
            &systemtime(10, 5, 3),
            &systemtime(3, 5, 2),
            adjusts,
        )
    }

    #[test]
    fn registry_values_give_the_zone_its_daylight_time() {
        let rules = central_europe(true).expect("rules");
        let at = super::Instant::from_unix_seconds;
        assert_eq!(rules.offset_at(at(wall(2026, 1, 15, 12, 0))), PARIS_WINTER);
        assert_eq!(rules.offset_at(at(wall(2026, 7, 15, 12, 0))), PARIS_SUMMER);
        // The changes, in UTC: 2026-03-29 01:00 and 2026-10-25 01:00.
        let spring = wall(2026, 3, 29, 1, 0);
        assert_eq!(rules.offset_at(at(spring - 1)), PARIS_WINTER);
        assert_eq!(rules.offset_at(at(spring)), PARIS_SUMMER);
        let autumn = wall(2026, 10, 25, 1, 0);
        assert_eq!(rules.offset_at(at(autumn - 1)), PARIS_SUMMER);
        assert_eq!(rules.offset_at(at(autumn)), PARIS_WINTER);
    }

    #[test]
    fn automatic_daylight_saving_turned_off_keeps_the_standard_offset_all_year() {
        let rules = central_europe(false).expect("rules");
        assert_eq!(rules.daylight, None);
        for wall_seconds in [wall(2026, 1, 15, 12, 0), wall(2026, 7, 15, 12, 0)] {
            let at = super::Instant::from_unix_seconds(wall_seconds);
            assert_eq!(rules.offset_at(at), PARIS_WINTER);
            assert_eq!(rules.offset_of_wall_clock(wall_seconds), PARIS_WINTER);
        }
    }

    #[test]
    fn registry_rules_of_the_clock_follow_the_overlap_and_the_gap() {
        let rules = central_europe(true).expect("rules");
        // 02:30 on the autumn change is shown twice, the first in summer time.
        assert_eq!(
            rules.offset_of_wall_clock(wall(2026, 10, 25, 2, 30)),
            PARIS_SUMMER
        );
        assert_eq!(
            rules.offset_of_wall_clock(wall(2026, 10, 25, 3, 0)),
            PARIS_WINTER
        );
        // 02:30 on the spring change is never shown: the offset before the jump.
        assert_eq!(
            rules.offset_of_wall_clock(wall(2026, 3, 29, 2, 30)),
            PARIS_WINTER
        );
        assert_eq!(
            rules.offset_of_wall_clock(wall(2026, 3, 29, 3, 0)),
            PARIS_SUMMER
        );
    }

    #[test]
    fn registry_dates_that_are_not_systemtimes_give_no_rule() {
        assert_eq!(
            super::registry_rules([-60, 0, -60], &[0; 3], &systemtime(3, 5, 2), true),
            None
        );
    }

    #[test]
    fn a_local_time_is_shown_with_the_local_offset() {
        let instant = jiff::Timestamp::from_second(1_735_122_645).expect("in range");
        let zoned = super::local_zoned(instant);
        assert_eq!(zoned.timestamp(), instant);
        assert_eq!(
            zoned.offset().seconds(),
            super::local_offset_at(super::Instant::from_unix_seconds(1_735_122_645))
        );
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
