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

//! How long the computer has had no input, for the idle auto-lock, as the C# `SystemIdle`.
//!
//! On Windows it is measured system-wide (`GetLastInputInfo`), whatever has the focus: typing
//! into an RDP desktop or another program counts, so the lock never fires under a user at
//! work. Elsewhere the window has no such measure, and counts the time since its own last
//! key, click or wheel turn, in any of its windows; a pointer only moved is not counted, as
//! following every move would redraw the window at each.

use std::time::{Duration, Instant};

use iced::{event, window};

/// Whether the idle time is the computer's own; else the window counts its own input.
pub const SYSTEM_WIDE: bool = cfg!(windows);

/// The time since the last input: the computer's, when it can be read, else since
/// `last_input`, the window's own last one.
#[must_use]
pub fn idle_time(last_input: Instant) -> Duration {
    system_idle().unwrap_or_else(|| last_input.elapsed())
}

/// The time between two readings of the millisecond tick count, `last` then `now`: both
/// wrap every 49.7 days, and the wrapping difference stays right across a wrap.
#[cfg(any(windows, test))]
#[must_use]
fn ticks_between(last: u32, now: u32) -> Duration {
    Duration::from_millis(u64::from(now.wrapping_sub(last)))
}

/// The computer's idle time, from the tick of its last input; `None` when it cannot be read.
#[cfg(windows)]
fn system_idle() -> Option<Duration> {
    let info = winsafe::GetLastInputInfo()
        .inspect_err(|error| log::warn!("the computer's idle time cannot be read: {error}"))
        .ok()?;
    #[expect(
        clippy::cast_possible_truncation,
        reason = "the last input's tick is the low 32 bits of the same count"
    )]
    let now = winsafe::GetTickCount64() as u32;
    Some(ticks_between(info.dwTime, now))
}

/// No system-wide measure here: the window counts its own input.
#[cfg(not(windows))]
fn system_idle() -> Option<Duration> {
    None
}

/// The window's own input, where the computer's idle time cannot be read: a key, a click or
/// a wheel turn, in any of its windows.
#[expect(
    clippy::needless_pass_by_value,
    reason = "the signature `event::listen_with` takes"
)]
pub fn input_event(
    event: iced::Event,
    _status: event::Status,
    _window: window::Id,
) -> Option<crate::shell::Message> {
    matches!(
        event,
        iced::Event::Keyboard(iced::keyboard::Event::KeyPressed { .. })
            | iced::Event::Mouse(
                iced::mouse::Event::ButtonPressed(_) | iced::mouse::Event::WheelScrolled { .. }
            )
            | iced::Event::Touch(iced::touch::Event::FingerPressed { .. })
    )
    .then_some(crate::shell::Message::UserInput)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ticks_between_two_readings_survive_a_wrap() {
        assert_eq!(ticks_between(1_000, 6_000), Duration::from_secs(5));
        assert_eq!(ticks_between(u32::MAX - 999, 4_000), Duration::from_secs(5));
        assert_eq!(ticks_between(42, 42), Duration::ZERO);
    }

    #[test]
    fn a_key_a_click_or_a_wheel_is_input_and_a_move_is_not() {
        let window = window::Id::unique();
        let input = |event| input_event(event, event::Status::Ignored, window).is_some();
        assert!(input(iced::Event::Mouse(
            iced::mouse::Event::ButtonPressed(iced::mouse::Button::Left)
        )));
        assert!(input(iced::Event::Mouse(
            iced::mouse::Event::WheelScrolled {
                delta: iced::mouse::ScrollDelta::Lines { x: 0.0, y: 1.0 }
            }
        )));
        assert!(!input(iced::Event::Mouse(
            iced::mouse::Event::CursorMoved {
                position: iced::Point::ORIGIN
            }
        )));
        assert!(!input(iced::Event::Window(window::Event::Focused)));
    }
}
