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

//! How an RDP profile's options size its desktop, as the C# embedded session does.

use heimdall_core::profile::{DesktopSizing, Experience, RdpOptions, Resolution, fixed_desktop};

#[test]
fn a_fixed_size_is_brought_within_the_csharp_limits_and_its_width_to_a_multiple_of_4() {
    assert_eq!(fixed_desktop(1920, 1080), (1920, 1080));
    assert_eq!(fixed_desktop(1366, 768), (1364, 768), "rounded down");
    assert_eq!(fixed_desktop(1367, 768), (1364, 768));
    assert_eq!(fixed_desktop(199, 199), (200, 200), "smallest side");
    assert_eq!(fixed_desktop(200, 200), (200, 200));
    assert_eq!(fixed_desktop(0, 0), (200, 200));
    assert_eq!(fixed_desktop(7680, 4320), (7680, 4320), "largest");
    assert_eq!(fixed_desktop(7681, 4321), (7680, 4320));
    assert_eq!(fixed_desktop(u16::MAX, u16::MAX), (7680, 4320));
}

#[test]
fn each_mode_sizes_and_shows_the_desktop_as_the_csharp_embedded_session() {
    let with = |resolution, scale_fixed, dynamic_resolution| RdpOptions {
        resolution,
        fixed_width: 1366,
        fixed_height: 768,
        scale_fixed,
        dynamic_resolution,
        ..RdpOptions::default()
    };
    let fixed = DesktopSizing::Fixed {
        width: 1364,
        height: 768,
    };
    for (options, sizing, scaled) in [
        (
            with(Resolution::FitWindow, true, true),
            DesktopSizing::FollowsTab,
            false,
        ),
        (
            with(Resolution::SmartSizing, true, true),
            DesktopSizing::FollowsTab,
            false,
        ),
        (
            with(Resolution::FitWindow, true, false),
            DesktopSizing::TabSizeOnce,
            true,
        ),
        (
            with(Resolution::SmartSizing, false, false),
            DesktopSizing::TabSizeOnce,
            true,
        ),
        (with(Resolution::Fixed, true, true), fixed, true),
        // Dynamic resolution never moves a fixed desktop.
        (with(Resolution::Fixed, true, false), fixed, true),
        (with(Resolution::Fixed, false, true), fixed, false),
    ] {
        assert_eq!(options.sizing(), sizing, "{options:?}");
        assert_eq!(options.scaled(), scaled, "{options:?}");
    }
}

#[test]
fn each_experience_box_is_its_csharp_flag_and_the_others_are_kept() {
    assert_eq!(
        Experience::ALL.map(Experience::bit),
        [0x01, 0x08, 0x04, 0x02, 0x20, 0x80, 0x100],
        "the C# constants, in the C# card's order"
    );
    // A bit no box shows, as a C# .rdp import may bring: kept through the boxes' changes.
    let mut options = RdpOptions {
        performance_flags: 0x40,
        ..RdpOptions::default()
    };
    assert_eq!(RdpOptions::default().performance_flags, 0, "none ticked");
    options.set(Experience::DisableThemes, true);
    options.set(Experience::EnableFontSmoothing, true);
    assert!(options.has(Experience::DisableThemes));
    assert!(!options.has(Experience::DisableWallpaper));
    assert_eq!(options.performance_flags, 0x40 | 0x08 | 0x80);
    options.set(Experience::DisableThemes, false);
    assert_eq!(options.performance_flags, 0x40 | 0x80);
}
