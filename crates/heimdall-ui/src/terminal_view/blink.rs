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

//! The cursor's blinking, as the C# terminal's xterm.js with `cursorBlink` on
//! (`CursorBlinkStateManager`): shown and hidden in turn for [`BLINK_INTERVAL`] each. It
//! starts over, shown, when the cursor moves (so it stays steady while typed text is echoed)
//! and when the window gets the focus back; it stops, shown, while the window does not have
//! the focus, or the application asked for a steady cursor. There is no setting: the C# has
//! none.

use std::time::{Duration, Instant};

/// How long the cursor is shown, then hidden: xterm.js's `BLINK_INTERVAL`.
pub const BLINK_INTERVAL: Duration = Duration::from_millis(600);

/// Where the cursor is: grid line and column.
pub type CursorPosition = (i32, usize);

/// The blinking of one terminal's cursor, told the time rather than reading the clock.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CursorBlink {
    /// When the cursor was last shown afresh; `None` while it does not blink.
    since: Option<Instant>,
    /// Where it was when last seen.
    at: Option<CursorPosition>,
}

impl CursorBlink {
    /// Takes note of the cursor at `now`: at `at`, and blinking or not (`blinks`: the window
    /// has the focus and the application did not ask for a steady cursor). Its blinking
    /// starts over when it moved, or when it starts blinking again.
    pub fn observe(&mut self, now: Instant, at: CursorPosition, blinks: bool) {
        if !blinks {
            self.since = None;
        } else if self.since.is_none() || self.at != Some(at) {
            self.since = Some(now);
        }
        self.at = Some(at);
    }

    /// Whether the cursor is shown at `now`: always, while it does not blink.
    #[must_use]
    pub fn visible(&self, now: Instant) -> bool {
        self.since
            .is_none_or(|since| Self::phases(since, now).is_multiple_of(2))
    }

    /// When the cursor is next shown or hidden, after `now`: `None` while it does not blink.
    #[must_use]
    pub fn next_change(&self, now: Instant) -> Option<Instant> {
        let since = self.since?;
        let phases = u32::try_from(Self::phases(since, now).checked_add(1)?).ok()?;
        since.checked_add(BLINK_INTERVAL.checked_mul(phases)?)
    }

    /// How many whole intervals have passed from `since` to `now`.
    fn phases(since: Instant, now: Instant) -> u128 {
        now.saturating_duration_since(since).as_nanos() / BLINK_INTERVAL.as_nanos()
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::{BLINK_INTERVAL, CursorBlink};

    const AT: (i32, usize) = (3, 7);

    fn ms(millis: u64) -> Duration {
        Duration::from_millis(millis)
    }

    #[test]
    fn the_cursor_is_shown_then_hidden_for_an_interval_each() {
        let start = Instant::now();
        let mut blink = CursorBlink::default();
        assert!(blink.visible(start), "shown before it is first seen");
        blink.observe(start, AT, true);
        assert_eq!(BLINK_INTERVAL, ms(600), "xterm.js's interval");
        assert!(blink.visible(start));
        assert!(blink.visible(start + ms(599)));
        assert!(!blink.visible(start + ms(600)));
        assert!(!blink.visible(start + ms(1199)));
        assert!(blink.visible(start + ms(1200)));
        assert_eq!(blink.next_change(start), Some(start + ms(600)));
        assert_eq!(blink.next_change(start + ms(700)), Some(start + ms(1200)));
        // Seen again in the same place: nothing starts over.
        blink.observe(start + ms(700), AT, true);
        assert!(!blink.visible(start + ms(700)));
    }

    #[test]
    fn a_move_shows_the_cursor_for_a_whole_interval_again() {
        let start = Instant::now();
        let mut blink = CursorBlink::default();
        blink.observe(start, AT, true);
        let moved = start + ms(900);
        assert!(!blink.visible(moved));
        blink.observe(moved, (3, 8), true);
        assert!(blink.visible(moved));
        assert!(blink.visible(moved + ms(599)), "steady while typing");
        assert!(!blink.visible(moved + ms(600)));
        assert_eq!(blink.next_change(moved), Some(moved + ms(600)));
    }

    #[test]
    fn without_the_focus_or_when_asked_steady_it_is_shown_and_then_starts_over() {
        let start = Instant::now();
        let mut blink = CursorBlink::default();
        blink.observe(start, AT, true);
        let away = start + ms(700);
        blink.observe(away, AT, false);
        assert!(blink.visible(away), "shown, not blinking");
        assert!(blink.visible(away + ms(5000)));
        assert_eq!(blink.next_change(away), None, "no frame asked for");
        let back = away + ms(5000);
        blink.observe(back, AT, true);
        assert!(
            blink.visible(back + ms(599)),
            "a whole interval shown first"
        );
        assert!(!blink.visible(back + ms(600)));
    }
}
