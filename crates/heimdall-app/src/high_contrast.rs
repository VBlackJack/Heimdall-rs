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

//! Whether Windows high contrast is on, read from the registry as the C# `TwinShell`
//! backdrop service reads it: the `Flags` of the user's high contrast settings, whose
//! lowest bit is `HCF_HIGHCONTRASTON`. Elsewhere, never: the theme is chosen by hand.

use std::time::Duration;

/// How often the window looks again: a registry value read, cheap enough to follow a
/// change made while the window is open.
pub const POLL: Duration = Duration::from_secs(2);

/// Whether the window looks at all: only Windows has the setting.
pub const WATCHED: bool = cfg!(windows);

/// The user's high contrast settings.
#[cfg(windows)]
const KEY: &str = r"Control Panel\Accessibility\HighContrast";

/// The value that holds their flags.
#[cfg(windows)]
const FLAGS: &str = "Flags";

/// `HCF_HIGHCONTRASTON`: high contrast is on.
const HIGH_CONTRAST_ON: u32 = 0x1;

/// Whether Windows high contrast is on now; false when it cannot be read, and off Windows.
#[must_use]
pub fn system_on() -> bool {
    #[cfg(windows)]
    {
        use windows_registry::CURRENT_USER;
        let Ok(key) = CURRENT_USER.open(KEY) else {
            return false;
        };
        // Written as a decimal string; a number is read too, should a tool write one.
        key.get_string(FLAGS)
            .ok()
            .and_then(|text| flags(&text))
            .or_else(|| key.get_u32(FLAGS).ok())
            .is_some_and(is_on)
    }
    #[cfg(not(windows))]
    {
        false
    }
}

/// The flags `text` holds, as the registry writes them: a decimal number.
#[must_use]
pub fn flags(text: &str) -> Option<u32> {
    text.trim().parse().ok()
}

/// Whether `flags` say high contrast is on.
#[must_use]
pub const fn is_on(flags: u32) -> bool {
    flags & HIGH_CONTRAST_ON != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_lowest_bit_says_high_contrast_is_on() {
        assert!(!is_on(0));
        assert!(is_on(1));
        // Windows' own default: available, with its hotkey, and off.
        assert!(!is_on(0x7E));
        assert!(is_on(0x7F));
    }

    #[test]
    fn the_flags_are_read_as_the_decimal_text_the_registry_holds() {
        assert_eq!(flags("126"), Some(0x7E));
        assert_eq!(flags(" 127 "), Some(0x7F));
        assert_eq!(flags("0"), Some(0));
        assert_eq!(flags("1"), Some(1));
        for unreadable in ["", "on", "0x7F", "-1"] {
            assert_eq!(flags(unreadable), None, "{unreadable:?}");
        }
        assert!(flags("127").is_some_and(is_on));
        assert!(!flags("126").is_some_and(is_on));
    }

    #[test]
    fn off_windows_it_is_never_on() {
        if !WATCHED {
            assert!(!system_on());
        }
    }
}
