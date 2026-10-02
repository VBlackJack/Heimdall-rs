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
}
