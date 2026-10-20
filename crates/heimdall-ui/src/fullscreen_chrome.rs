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

//! The full-screen exit button that puts itself away, as the C# floating chrome.
//!
//! In full screen the chrome (the way out) fades after a few seconds with the pointer
//! still and comes back when the pointer is moved or reaches the top edge, as
//! `MainWindow.WindowUI.cs` (`OnFullscreenChromeAutoHideTick`,
//! `OnFullscreenChromeMousePollTick`, `OnFullscreenChromeMouseActivity`). The state is
//! kept apart from the window with the time given to it, so it is tested without a clock.

use std::time::{Duration, Instant};

use iced::Point;

/// How long the pointer is still before the chrome hides, as the C#
/// `FullscreenChromeAutoHideDelay`.
pub const AUTO_HIDE_DELAY: Duration = Duration::from_secs(3);

/// How long the chrome takes to fade in or out, as the C# `FullscreenChromeFadeDuration`.
pub const FADE_DURATION: Duration = Duration::from_millis(200);

/// How often the pointer is read, as the C# `FullscreenChromeMousePollInterval`.
pub const POLL_INTERVAL: Duration = Duration::from_millis(100);

/// How often the fade is drawn while it runs: about one frame at 60 Hz.
pub const FADE_FRAME: Duration = Duration::from_millis(16);

/// How far from the window's top edge the pointer brings the chrome back, in logical
/// pixels, as the C# `FullscreenChromeRevealThresholdPx`.
pub const REVEAL_THRESHOLD: f32 = 30.0;

/// The log target of everything about full screen: `HEIMDALL_LOG=debug` writes it.
pub const LOG_TARGET: &str = "heimdall::fullscreen";

/// How often the readings are summed up in the log.
const TRACE_EVERY: Duration = Duration::from_secs(1);

/// What the window tells the chrome at each reading.
#[derive(Debug, Clone, Copy)]
pub struct Reading {
    /// The pointer, in the window's coordinates.
    pub pointer: Point,
    /// A menu, a dialog, the palette or the tour is open: the chrome stays.
    pub overlay_open: bool,
    /// Animations are turned off: the chrome shows and hides at once.
    pub reduced_motion: bool,
    /// How far, in logical pixels, the pointer must go between two readings to count as
    /// moved: the `fullscreen_pointer_tolerance` setting; 0 counts any change.
    pub tolerance: f32,
}

/// What the window's pointer events were over the last second, for the log: how many, how far
/// in all, so a pointer that is still but reported moving shows which feed does it.
#[derive(Debug, Clone, Copy)]
pub struct PointerTrace {
    events: u32,
    travel: f32,
    since: Option<Instant>,
}

impl PointerTrace {
    /// No event yet.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            events: 0,
            travel: 0.0,
            since: None,
        }
    }

    /// An event of the window's pointer feed that moved the pointer by `distance`.
    pub fn note(&mut self, distance: f32) {
        if distance > 0.0 {
            self.events += 1;
            self.travel += distance;
            if distance < SMALL_MOVE {
                log::debug!(
                    target: LOG_TARGET,
                    "window pointer event (feed: window CursorMoved): small move of {distance:.1} px"
                );
            }
        }
    }

    /// Says the last second's events in the log once a second, with how far the other feed,
    /// the widget tracker, is from the window's (`tracker_gap`).
    pub fn flush(&mut self, now: Instant, tracker_gap: f32) {
        let since = *self.since.get_or_insert(now);
        if now.saturating_duration_since(since) >= TRACE_EVERY {
            log::debug!(
                target: LOG_TARGET,
                "window pointer feed: {} moves, {:.1} px in all, in the last {:?}; widget tracker is {:.1} px from it",
                self.events,
                self.travel,
                now.saturating_duration_since(since),
                tracker_gap
            );
            *self = Self {
                since: Some(now),
                ..Self::new()
            };
        }
    }
}

impl Default for PointerTrace {
    fn default() -> Self {
        Self::new()
    }
}

/// A move below this, in logical pixels, is logged one by one: the creeping of a still
/// pointer, not a hand.
const SMALL_MOVE: f32 = 8.0;

/// The chrome's visibility in full screen.
#[derive(Debug, Clone, Copy)]
pub struct FullscreenChrome {
    /// Whether the chrome is meant to be shown (it may still be fading).
    shown: bool,
    /// When the chrome hides if nothing moves before.
    hide_at: Instant,
    /// The pointer at the last reading.
    last_pointer: Option<Point>,
    /// The opacity when the last fade began, and when.
    fade_from: f32,
    fade_start: Instant,
    /// Readings since the last summary in the log, and when it was written.
    traced: (u32, Instant),
    /// The overlay flag at the last reading, to log a change only.
    last_overlay: bool,
    /// Whether the pointer's position changed since the last summary in the log.
    moved_since_trace: bool,
}

impl FullscreenChrome {
    /// A chrome shown at `now`, as full screen begins.
    #[must_use]
    pub fn entered(now: Instant) -> Self {
        log::debug!(
            target: LOG_TARGET,
            "chrome timer armed: entered full screen, hides in {AUTO_HIDE_DELAY:?}"
        );
        Self {
            shown: true,
            hide_at: now + AUTO_HIDE_DELAY,
            last_pointer: None,
            fade_from: 1.0,
            fade_start: now,
            traced: (0, now),
            last_overlay: false,
            moved_since_trace: false,
        }
    }

    /// Whether the chrome is meant to be shown.
    #[cfg(test)]
    #[must_use]
    pub fn is_shown(&self) -> bool {
        self.shown
    }

    /// The opacity at `now`: 1 shown, 0 hidden, between while it fades.
    #[must_use]
    pub fn opacity(&self, now: Instant) -> f32 {
        let target = if self.shown { 1.0 } else { 0.0 };
        let elapsed = now.saturating_duration_since(self.fade_start);
        if elapsed >= FADE_DURATION {
            return target;
        }
        let progress = elapsed.as_secs_f32() / FADE_DURATION.as_secs_f32();
        // Quadratic ease in and out, as the C# `QuadraticEase`.
        let eased = if progress < 0.5 {
            2.0 * progress * progress
        } else {
            1.0 - 2.0 * (1.0 - progress) * (1.0 - progress)
        };
        self.fade_from + (target - self.fade_from) * eased
    }

    /// Whether a fade is running at `now`: the frames are drawn only then.
    #[must_use]
    pub fn is_fading(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.fade_start) < FADE_DURATION
    }

    /// Whether the chrome is drawn and answers the pointer at `now`.
    #[must_use]
    pub fn is_visible(&self, now: Instant) -> bool {
        self.opacity(now) > 0.0
    }

    /// One reading of the pointer at `now`.
    pub fn read(&mut self, now: Instant, reading: Reading) {
        // Between two readings, not from an anchor: a pointer creeping a pixel in seven
        // seconds (measured in the lab) never adds up to a move.
        let moved = self.last_pointer.is_some_and(|last| {
            let (dx, dy) = (last.x - reading.pointer.x, last.y - reading.pointer.y);
            let distance = dx.hypot(dy);
            distance > 0.0 && distance >= reading.tolerance
        });
        if let Some(last) = self.last_pointer.filter(|_| moved) {
            log::debug!(
                target: LOG_TARGET,
                "chrome pointer moved by {:.1} px",
                (last.x - reading.pointer.x).hypot(last.y - reading.pointer.y)
            );
        }
        self.last_pointer = Some(reading.pointer);
        self.moved_since_trace |= moved;
        let near_top = reading.pointer.y >= 0.0 && reading.pointer.y <= REVEAL_THRESHOLD;
        if near_top {
            log::debug!(
                target: LOG_TARGET,
                "chrome timer reset: pointer at the top edge"
            );
            self.show(now, reading.reduced_motion);
        } else if moved && self.shown {
            // Only a shown chrome waits again: a hidden one comes back at the top edge alone,
            // as the C# poll (`OnFullscreenChromeMousePollTick`).
            log::debug!(target: LOG_TARGET, "chrome timer reset: pointer position changed");
            self.hide_at = now + AUTO_HIDE_DELAY;
        }
        if reading.overlay_open != self.last_overlay {
            log::debug!(
                target: LOG_TARGET,
                "chrome overlay open: {}",
                reading.overlay_open
            );
            self.last_overlay = reading.overlay_open;
        }
        // Under a menu or a dialog the wait starts again at each reading: the chrome goes
        // a full delay after the last of them closes, never while one is open.
        if reading.overlay_open && self.shown {
            self.hide_at = now + AUTO_HIDE_DELAY;
        }
        if self.shown && now >= self.hide_at {
            log::debug!(
                target: LOG_TARGET,
                "chrome timer fired: hiding"
            );
            self.set_shown(false, now, reading.reduced_motion);
        }
        self.traced.0 += 1;
        if now.saturating_duration_since(self.traced.1) >= TRACE_EVERY {
            log::debug!(
                target: LOG_TARGET,
                "chrome: {} readings in the last {:?}, shown={}, pointer changed since the last summary={}, overlay={}, hides in {:?}",
                self.traced.0,
                now.saturating_duration_since(self.traced.1),
                self.shown,
                self.moved_since_trace,
                reading.overlay_open,
                self.hide_at.saturating_duration_since(now)
            );
            self.traced = (0, now);
            self.moved_since_trace = false;
        }
    }

    fn show(&mut self, now: Instant, reduced_motion: bool) {
        self.hide_at = now + AUTO_HIDE_DELAY;
        self.set_shown(true, now, reduced_motion);
    }

    fn set_shown(&mut self, shown: bool, now: Instant, reduced_motion: bool) {
        if self.shown == shown {
            return;
        }
        if reduced_motion {
            // No animation: the fade starts at its end.
            self.fade_from = if shown { 1.0 } else { 0.0 };
            self.fade_start = now.checked_sub(FADE_DURATION).unwrap_or(now);
        } else {
            self.fade_from = self.opacity(now);
            self.fade_start = now;
        }
        self.shown = shown;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOP: Point = Point::new(100.0, 5.0);
    const MIDDLE: Point = Point::new(100.0, 400.0);

    fn reading(pointer: Point) -> Reading {
        Reading {
            pointer,
            overlay_open: false,
            reduced_motion: false,
            tolerance: 3.0,
        }
    }

    fn at(start: Instant, millis: u64) -> Instant {
        start + Duration::from_millis(millis)
    }

    #[test]
    fn entering_shows_the_chrome_fully() {
        let start = Instant::now();
        let chrome = FullscreenChrome::entered(start);
        assert!(chrome.is_shown());
        assert!((chrome.opacity(start) - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn a_still_pointer_hides_the_chrome_after_the_delay() {
        let start = Instant::now();
        let mut chrome = FullscreenChrome::entered(start);
        chrome.read(at(start, 100), reading(MIDDLE));
        assert!(chrome.is_shown());
        let late = start + AUTO_HIDE_DELAY;
        chrome.read(late, reading(MIDDLE));
        assert!(!chrome.is_shown());
        assert!(chrome.is_fading(late));
        let done = late + FADE_DURATION;
        assert!(chrome.opacity(done).abs() < f32::EPSILON);
        assert!(!chrome.is_visible(done));
    }

    #[test]
    fn a_still_pointer_whose_readings_wander_a_little_does_not_keep_the_chrome() {
        let start = Instant::now();
        let mut chrome = FullscreenChrome::entered(start);
        let (a, b) = (Point::new(1700.0, 1000.0), Point::new(1701.0, 999.5));
        for step in 0..=60_u32 {
            let pointer = if step % 2 == 0 { a } else { b };
            chrome.read(at(start, u64::from(step) * 100), reading(pointer));
        }
        assert!(
            !chrome.is_shown(),
            "hidden after 3 s although the readings differ"
        );
    }

    #[test]
    fn only_the_top_edge_brings_a_hidden_chrome_back_not_a_creep_nor_a_real_move() {
        let start = Instant::now();
        let mut chrome = FullscreenChrome::entered(start);
        chrome.read(start, reading(MIDDLE));
        chrome.read(at(start, 3_000), reading(MIDDLE));
        assert!(!chrome.is_shown());
        // One pixel every 100 ms for a minute: 600 px in all, none of them a move.
        for step in 1..=600_u16 {
            let creep = Point::new(100.0 + f32::from(step), 400.0);
            chrome.read(at(start, 3_000 + u64::from(step) * 100), reading(creep));
        }
        assert!(!chrome.is_shown(), "the readings differ by a pixel each");
        let last = Point::new(700.0, 400.0);
        chrome.read(
            at(start, 70_000),
            reading(Point::new(last.x + 20.0, last.y)),
        );
        assert!(
            !chrome.is_shown(),
            "a move far from the top edge does not show it"
        );
        chrome.read(at(start, 70_100), reading(TOP));
        assert!(chrome.is_shown(), "the top edge does");
    }

    #[test]
    fn a_tolerance_of_zero_counts_any_change_as_a_move() {
        let start = Instant::now();
        let mut chrome = FullscreenChrome::entered(start);
        let exact = |pointer| Reading {
            tolerance: 0.0,
            ..reading(pointer)
        };
        chrome.read(start, exact(MIDDLE));
        // A pixel each 2 s, as the lab's creeping pointer: with no tolerance each one counts
        // and the chrome never hides.
        for step in 1..=5_u16 {
            let creep = Point::new(100.0 + f32::from(step), 400.0);
            chrome.read(at(start, u64::from(step) * 2_000), exact(creep));
        }
        assert!(chrome.is_shown());
    }

    #[test]
    fn a_moving_pointer_keeps_the_chrome() {
        let start = Instant::now();
        let mut chrome = FullscreenChrome::entered(start);
        chrome.read(start, reading(MIDDLE));
        chrome.read(at(start, 2_500), reading(Point::new(110.0, 400.0)));
        // 3 s after the start, but only 0.5 s after the last move.
        chrome.read(at(start, 3_000), reading(Point::new(110.0, 400.0)));
        assert!(chrome.is_shown());
        chrome.read(at(start, 5_500), reading(Point::new(110.0, 400.0)));
        assert!(!chrome.is_shown());
    }

    #[test]
    fn the_top_edge_brings_a_hidden_chrome_back() {
        let start = Instant::now();
        let mut chrome = FullscreenChrome::entered(start);
        chrome.read(start, reading(MIDDLE));
        chrome.read(at(start, 3_000), reading(MIDDLE));
        assert!(!chrome.is_shown());
        let back = at(start, 4_000);
        chrome.read(back, reading(TOP));
        assert!(chrome.is_shown());
        // The pointer stays at the edge: the chrome stays past the delay.
        chrome.read(back + AUTO_HIDE_DELAY * 2, reading(TOP));
        assert!(chrome.is_shown());
    }

    #[test]
    fn the_reveal_rule_stops_at_the_threshold() {
        let start = Instant::now();
        let mut chrome = FullscreenChrome::entered(start);
        let just_below = Point::new(100.0, REVEAL_THRESHOLD + 1.0);
        chrome.read(start, reading(just_below));
        chrome.read(at(start, 3_000), reading(just_below));
        chrome.read(at(start, 3_100), reading(just_below));
        assert!(!chrome.is_shown());
        let on_edge = Point::new(100.0, REVEAL_THRESHOLD);
        chrome.read(at(start, 3_200), reading(on_edge));
        assert!(chrome.is_shown());
    }

    #[test]
    fn an_open_menu_or_dialog_keeps_the_chrome() {
        let start = Instant::now();
        let mut chrome = FullscreenChrome::entered(start);
        let open = Reading {
            overlay_open: true,
            ..reading(MIDDLE)
        };
        chrome.read(at(start, 2_900), open);
        chrome.read(at(start, 10_000), open);
        assert!(chrome.is_shown());
        // The wait starts again when it closes.
        chrome.read(at(start, 12_900), reading(MIDDLE));
        assert!(chrome.is_shown());
        chrome.read(at(start, 13_000), reading(MIDDLE));
        assert!(!chrome.is_shown());
    }

    #[test]
    fn the_fade_runs_between_the_two_opacities() {
        let start = Instant::now();
        let mut chrome = FullscreenChrome::entered(start);
        let hidden_at = start + AUTO_HIDE_DELAY;
        chrome.read(hidden_at, reading(MIDDLE));
        let half = chrome.opacity(hidden_at + FADE_DURATION / 2);
        assert!((half - 0.5).abs() < 0.01);
        let quarter = chrome.opacity(hidden_at + FADE_DURATION / 4);
        assert!(quarter > 0.5 && quarter < 1.0);
    }

    #[test]
    fn reduced_motion_shows_and_hides_at_once() {
        let start = Instant::now();
        let mut chrome = FullscreenChrome::entered(start);
        let still = Reading {
            reduced_motion: true,
            ..reading(MIDDLE)
        };
        let late = start + AUTO_HIDE_DELAY;
        chrome.read(late, still);
        assert!(!chrome.is_shown());
        assert!(!chrome.is_fading(late));
        assert!(!chrome.is_visible(late));
        let top = Reading {
            reduced_motion: true,
            ..reading(TOP)
        };
        chrome.read(late + POLL_INTERVAL, top);
        assert!(chrome.is_shown());
        assert!(!chrome.is_fading(late + POLL_INTERVAL));
        assert!((chrome.opacity(late + POLL_INTERVAL) - 1.0).abs() < f32::EPSILON);
    }
}
