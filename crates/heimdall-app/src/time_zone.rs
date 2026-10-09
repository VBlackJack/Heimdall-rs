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
    use chrono::{Offset as _, TimeZone as _};
    let seconds = instant.unix_seconds();
    chrono::Local
        .timestamp_opt(seconds, 0)
        .earliest()
        .map_or(0, |local| local.offset().fix().local_minus_utc())
}

/// The offset east of UTC, in seconds, of this computer's clock when it shows
/// `wall_seconds`, seconds since 1970-01-01 on the wall clock: the earlier of two when the
/// clock goes back, the one before a gap when it skips ahead.
#[must_use]
pub fn local_offset_of_wall_clock(wall_seconds: i64) -> i32 {
    use chrono::{Offset as _, TimeZone as _};
    let Some(naive) = chrono::DateTime::from_timestamp(wall_seconds, 0).map(|utc| utc.naive_utc())
    else {
        return 0;
    };
    match chrono::Local.from_local_datetime(&naive).earliest() {
        Some(local) => local.offset().fix().local_minus_utc(),
        // A time the clock skips: the offset an hour before, as .NET's standard time.
        None => local_offset_at(Instant::from_unix_seconds(wall_seconds - GAP_PROBE_SECONDS)),
    }
}

/// How far before a skipped wall-clock time its offset is looked for.
const GAP_PROBE_SECONDS: i64 = 3_600;

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
