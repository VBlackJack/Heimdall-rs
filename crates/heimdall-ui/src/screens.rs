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

//! The window put back where it was left, on the screens there are now, as the C# does.
//!
//! The window opens hidden, then is moved and shown: the screens are listed once the window
//! is open, when their coordinates are those of the pixels on them, and the place it is moved
//! to is given in the scale of the screen it opened on, as the move reads it.

use heimdall_core::window_state::{Bounds, WindowState, place};
use iced::{Point, Task, window};

/// Every screen's working area, the screen less its taskbar, in physical pixels.
#[cfg(windows)]
#[must_use]
pub fn work_areas() -> Vec<Bounds> {
    use winsafe::prelude::*;

    let mut areas = Vec::new();
    let listed = winsafe::HDC::NULL.EnumDisplayMonitors(None, |monitor, _, _| {
        if let Ok(info) = monitor.GetMonitorInfo() {
            let work = info.rcWork;
            areas.push(Bounds {
                x: f64::from(work.left),
                y: f64::from(work.top),
                width: f64::from(work.right) - f64::from(work.left),
                height: f64::from(work.bottom) - f64::from(work.top),
            });
        }
        true
    });
    if let Err(error) = listed {
        log::warn!("the screens could not be listed: {error}");
    }
    areas
}

/// The screens are listed on Windows only: elsewhere, the system places the window.
#[cfg(not(windows))]
#[must_use]
pub fn work_areas() -> Vec<Bounds> {
    Vec::new()
}

/// Whether the window opens hidden, to be put back where it was by [`restore`]: when its
/// place was kept.
#[must_use]
pub fn opens_hidden(left: &WindowState) -> bool {
    left.bounds().is_some()
}

/// Puts the window `id`, opened hidden, where it was `left` on the screens there are, then
/// shows it, maximized if it was. Nothing when its place was not kept: it opened shown.
pub fn restore<Message: Send + 'static>(id: window::Id, left: &WindowState) -> Task<Message> {
    let Some(saved) = left.bounds() else {
        return Task::none();
    };
    let maximized = left.maximized;
    window::scale_factor(id).then(move |scale| {
        let moved = place(saved, &work_areas()).map_or_else(Task::none, |(x, y)| {
            window::move_to(id, logical(x, y, scale))
        });
        let shown = moved.chain(window::set_mode(id, window::Mode::Windowed));
        if maximized {
            shown.chain(window::maximize(id, true))
        } else {
            shown
        }
    })
}

/// A place in physical pixels, in the logical pixels of a screen of this `scale`.
#[expect(
    clippy::cast_possible_truncation,
    reason = "a desktop coordinate is far within an f32's range"
)]
fn logical(x: f64, y: f64, scale: f32) -> Point {
    let scale = f64::from(scale);
    Point::new((x / scale) as f32, (y / scale) as f32)
}

/// A place in logical pixels, in the physical pixels of a screen of this `scale`; what
/// [`logical`] undoes.
#[expect(
    clippy::cast_possible_truncation,
    reason = "a desktop coordinate is far within an i32's range"
)]
#[must_use]
pub fn physical(position: Point, scale: f32) -> (i32, i32) {
    let scale = f64::from(scale);
    (
        (f64::from(position.x) * scale).round() as i32,
        (f64::from(position.y) * scale).round() as i32,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_place_is_carried_between_physical_and_logical_pixels() {
        assert_eq!(
            physical(Point::new(-1266.6666, 26.666_666), 1.5),
            (-1900, 40)
        );
        assert_eq!(
            logical(-1900.0, 40.0, 1.5),
            Point::new(-1266.6666, 26.666_666)
        );
        assert_eq!(physical(Point::new(100.0, 50.0), 1.0), (100, 50));
    }

    #[test]
    fn only_a_window_whose_place_was_kept_opens_hidden() {
        let placed = WindowState {
            width: Some(1200.0),
            height: Some(800.0),
            x: Some(10),
            y: Some(20),
            scale: Some(1.0),
            ..WindowState::default()
        };
        assert!(opens_hidden(&placed));
        assert!(!opens_hidden(&WindowState { x: None, ..placed }));
        assert!(!opens_hidden(&WindowState::default()));
    }
}
