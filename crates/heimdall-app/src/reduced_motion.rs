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

//! Whether Windows asks for reduced motion: Settings > Accessibility > Visual effects >
//! Animation effects turned off. That switch is `SPI_SETCLIENTAREAANIMATION`, the setting a
//! browser reads for `prefers-reduced-motion`, which the C# terminal's xterm.js honours. It
//! is read from where Windows keeps it, without a system call: the user's
//! `UserPreferencesMask`, whose bit `(SPI_GETCLIENTAREAANIMATION - 0x1000) / 2` holds it, as
//! every boolean `SPI_GET*` from `0x1000` is kept. Elsewhere, never: nothing asks.

use std::time::Duration;

/// How often the window looks again: a registry value read, cheap enough to follow a
/// change made while the window is open.
pub const POLL: Duration = Duration::from_secs(2);

/// Whether the window looks at all: only Windows has the setting.
pub const WATCHED: bool = cfg!(windows);

/// The user's desktop settings.
#[cfg(windows)]
const KEY: &str = r"Control Panel\Desktop";

/// The value that holds the boolean user interface settings, one bit each.
#[cfg(windows)]
const MASK: &str = "UserPreferencesMask";

/// `SPI_GETCLIENTAREAANIMATION`: whether animations in the client area are on.
const SPI_GETCLIENTAREAANIMATION: u32 = 0x1042;

/// The first boolean `SPI_GET*` kept in the mask, at its bit 0.
const FIRST_MASKED: u32 = 0x1000;

/// Each setting has a `GET` and a `SET` code, two apart: one bit for both.
const CODES_PER_BIT: u32 = 2;

/// The bit of the mask that says client area animations are on.
const ANIMATION_BIT: u32 = (SPI_GETCLIENTAREAANIMATION - FIRST_MASKED) / CODES_PER_BIT;

/// Whether Windows asks for reduced motion now; false when it cannot be read, and off
/// Windows.
#[must_use]
pub fn system_on() -> bool {
    #[cfg(windows)]
    {
        use windows_registry::CURRENT_USER;
        CURRENT_USER
            .open(KEY)
            .and_then(|key| key.get_value(MASK))
            .is_ok_and(|mask| is_reduced(&mask))
    }
    #[cfg(not(windows))]
    {
        false
    }
}

/// Whether `mask`, the bytes of `UserPreferencesMask`, says animations are off. A mask too
/// short to hold the bit says nothing: motion is not reduced.
#[must_use]
pub fn is_reduced(mask: &[u8]) -> bool {
    let byte = usize::try_from(ANIMATION_BIT / u8::BITS).unwrap_or(usize::MAX);
    let bit = 1u8 << (ANIMATION_BIT % u8::BITS);
    mask.get(byte).is_some_and(|value| value & bit == 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_animation_bit_is_the_second_of_the_fifth_byte() {
        assert_eq!(ANIMATION_BIT, 33);
    }

    #[test]
    fn animations_off_in_the_mask_reduce_motion() {
        // Windows 11's default: animations on.
        assert!(!is_reduced(&[0x9E, 0x1E, 0x07, 0x80, 0x12, 0x00, 0x00, 0x00]));
        // Animation effects turned off: only that bit changes.
        assert!(is_reduced(&[0x9E, 0x1E, 0x07, 0x80, 0x10, 0x00, 0x00, 0x00]));
        // "Adjust for best performance" turns it off too.
        assert!(is_reduced(&[0x90, 0x12, 0x03, 0x80, 0x10, 0x00, 0x00, 0x00]));
        // The other bits of that byte say nothing about it.
        assert!(!is_reduced(&[0, 0, 0, 0, 0x02]));
        assert!(is_reduced(&[0xFF, 0xFF, 0xFF, 0xFF, 0xFD]));
    }

    #[test]
    fn a_mask_too_short_to_say_does_not_reduce_motion() {
        assert!(!is_reduced(&[]));
        assert!(!is_reduced(&[0x90, 0x12, 0x03, 0x80]));
    }

    #[test]
    fn off_windows_it_is_never_on() {
        if !WATCHED {
            assert!(!system_on());
        }
    }
}
