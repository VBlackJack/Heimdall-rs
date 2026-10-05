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

//! How the window was left, as the C# settings keep it: its place and size, whether it was
//! maximized, and the sidebar's width and whether it was hidden. Its place is checked against
//! the screens there are when it opens again, as the C# does: a screen unplugged since does
//! not open it where nothing shows it.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::store::{StoreError, write_atomic};

/// Name of the file, beside the profiles.
pub const WINDOW_STATE_FILE_NAME: &str = "window-state.toml";

/// How the window was left; what is not known is left to the defaults.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct WindowState {
    /// Its width when last not maximized, in logical pixels.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<f32>,
    /// Its height when last not maximized, in logical pixels.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<f32>,
    /// It was maximized.
    #[serde(default)]
    pub maximized: bool,
    /// The sidebar's width, as dragged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sidebar_width: Option<f32>,
    /// The sidebar was hidden.
    #[serde(default)]
    pub sidebar_hidden: bool,
    /// Its left edge when last not maximized, in physical pixels of the desktop.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub x: Option<i32>,
    /// Its top edge when last not maximized, in physical pixels of the desktop.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub y: Option<i32>,
    /// The scale of the screen it was on then: its size in physical pixels is its logical
    /// size times this.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scale: Option<f32>,
}

/// A rectangle of the desktop, in physical pixels: a window, or a screen's working area.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bounds {
    /// Its left edge.
    pub x: f64,
    /// Its top edge.
    pub y: f64,
    /// Its width.
    pub width: f64,
    /// Its height.
    pub height: f64,
}

impl Bounds {
    fn is_valid(&self) -> bool {
        [self.x, self.y, self.width, self.height]
            .iter()
            .all(|value| value.is_finite())
            && self.width > 0.0
            && self.height > 0.0
    }

    fn right(&self) -> f64 {
        self.x + self.width
    }

    fn bottom(&self) -> f64 {
        self.y + self.height
    }

    fn overlap(&self, other: &Self) -> f64 {
        let width = self.right().min(other.right()) - self.x.max(other.x);
        let height = self.bottom().min(other.bottom()) - self.y.max(other.y);
        if width > 0.0 && height > 0.0 {
            width * height
        } else {
            0.0
        }
    }

    fn squared_distance(&self, other: &Self) -> f64 {
        let across = axis_gap(self.x, self.right(), other.x, other.right());
        let down = axis_gap(self.y, self.bottom(), other.y, other.bottom());
        across * across + down * down
    }
}

/// The gap between two spans of one axis; none when they meet.
fn axis_gap(start: f64, end: f64, other_start: f64, other_end: f64) -> f64 {
    if end < other_start {
        other_start - end
    } else if other_end < start {
        start - other_end
    } else {
        0.0
    }
}

/// Where a window `length` long starts on one axis to lie within an area: where it was, moved
/// in as little as it takes; longer than the area, covering it with neither edge inside.
fn clamp_axis(start: f64, length: f64, area_start: f64, area_length: f64) -> f64 {
    let far = area_start + area_length - length;
    if length <= area_length {
        start.clamp(area_start, far)
    } else {
        start.clamp(far, area_start)
    }
}

/// Where a window last at `saved` opens among the screens' working `areas`, as the C# does: on
/// the screen it overlaps most, or, on none, the nearest; moved within it as little as it
/// takes. None when there is no valid area, or `saved` is not a valid rectangle.
#[must_use]
pub fn place(saved: Bounds, areas: &[Bounds]) -> Option<(f64, f64)> {
    if !saved.is_valid() {
        return None;
    }
    let mut areas = areas.iter().filter(|area| area.is_valid());
    let mut chosen = *areas.next()?;
    let (mut overlap, mut distance) = (saved.overlap(&chosen), saved.squared_distance(&chosen));
    for area in areas {
        let (area_overlap, area_distance) = (saved.overlap(area), saved.squared_distance(area));
        if area_overlap > overlap
            || (overlap <= 0.0 && area_overlap <= 0.0 && area_distance < distance)
        {
            (chosen, overlap, distance) = (*area, area_overlap, area_distance);
        }
    }
    Some((
        clamp_axis(saved.x, saved.width, chosen.x, chosen.width),
        clamp_axis(saved.y, saved.height, chosen.y, chosen.height),
    ))
}

impl WindowState {
    /// The size kept, when both sides are, within `min` and `max` each.
    #[must_use]
    pub fn size_within(&self, min: (f32, f32), max: (f32, f32)) -> Option<(f32, f32)> {
        let (width, height) = (self.width?, self.height?);
        (width.is_finite() && height.is_finite())
            .then(|| (width.clamp(min.0, max.0), height.clamp(min.1, max.1)))
    }

    /// Where the window was and how large, in physical pixels; none when not all is kept.
    #[must_use]
    pub fn bounds(&self) -> Option<Bounds> {
        let scale = f64::from(self.scale?);
        Some(Bounds {
            x: f64::from(self.x?),
            y: f64::from(self.y?),
            width: f64::from(self.width?) * scale,
            height: f64::from(self.height?) * scale,
        })
    }
}

/// The file beside `profiles_file`.
#[must_use]
pub fn state_path(profiles_file: &Path) -> PathBuf {
    profiles_file.with_file_name(WINDOW_STATE_FILE_NAME)
}

/// The state kept at `path`; the defaults when there is none or it cannot be read.
#[must_use]
pub fn load(path: &Path) -> WindowState {
    fs::read_to_string(path)
        .ok()
        .and_then(|text| toml::from_str(&text).ok())
        .unwrap_or_default()
}

/// Keeps `state` at `path`.
///
/// # Errors
///
/// Returns [`StoreError`] when the file cannot be written.
pub fn save(path: &Path, state: &WindowState) -> Result<(), StoreError> {
    let text = toml::to_string_pretty(state)?;
    write_atomic(path, &text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_state_is_kept_read_back_and_held_within_bounds() {
        let dir = tempfile::tempdir().expect("dir");
        let path = state_path(&dir.path().join("profiles.toml"));
        assert_eq!(load(&path), WindowState::default(), "none yet");
        let state = WindowState {
            width: Some(1500.0),
            height: Some(900.0),
            maximized: true,
            sidebar_width: Some(320.0),
            sidebar_hidden: true,
            x: Some(-1900),
            y: Some(40),
            scale: Some(1.5),
        };
        save(&path, &state).expect("save");
        assert_eq!(load(&path), state);
        assert_eq!(
            state.size_within((640.0, 400.0), (1400.0, 1000.0)),
            Some((1400.0, 900.0))
        );
        assert_eq!(
            WindowState::default().size_within((1.0, 1.0), (2.0, 2.0)),
            None
        );
        assert_eq!(
            state.bounds(),
            Some(Bounds {
                x: -1900.0,
                y: 40.0,
                width: 2250.0,
                height: 1350.0
            })
        );
        assert_eq!(
            WindowState {
                scale: None,
                ..state
            }
            .bounds(),
            None,
            "a kept place needs its scale"
        );
        fs::write(&path, "width = \"wide\"").expect("write");
        assert_eq!(
            load(&path),
            WindowState::default(),
            "unreadable: the defaults"
        );
    }

    fn area(x: f64, y: f64, width: f64, height: f64) -> Bounds {
        Bounds {
            x,
            y,
            width,
            height,
        }
    }

    #[test]
    fn a_window_opens_where_it_was_on_the_screen_it_overlaps_most() {
        let screens = [
            area(0.0, 0.0, 1920.0, 1040.0),
            area(1920.0, 0.0, 2560.0, 1400.0),
        ];
        assert_eq!(
            place(area(100.0, 50.0, 800.0, 600.0), &screens),
            Some((100.0, 50.0)),
            "within a screen: as it was"
        );
        assert_eq!(
            place(area(1800.0, 100.0, 800.0, 600.0), &screens),
            Some((1920.0, 100.0)),
            "astride: moved onto the screen it overlaps most"
        );
        assert_eq!(
            place(area(1500.0, 900.0, 800.0, 600.0), &screens[..1]),
            Some((1120.0, 440.0)),
            "past an edge: moved in as little as it takes"
        );
    }

    #[test]
    fn a_window_left_on_a_screen_unplugged_since_opens_on_the_nearest() {
        let screens = [
            area(0.0, 0.0, 1920.0, 1040.0),
            area(1920.0, 0.0, 2560.0, 1400.0),
        ];
        assert_eq!(
            place(area(-1700.0, 200.0, 800.0, 600.0), &screens),
            Some((0.0, 200.0)),
            "left of every screen: the nearest, the first"
        );
        assert_eq!(
            place(area(5000.0, 1600.0, 800.0, 600.0), &screens),
            Some((3680.0, 800.0)),
            "below and right: the nearest, the second"
        );
        assert_eq!(
            place(area(-32000.0, -32000.0, 800.0, 600.0), &screens),
            Some((0.0, 0.0)),
            "where a minimized window is reported"
        );
    }

    #[test]
    fn a_window_larger_than_its_screen_covers_it() {
        assert_eq!(
            place(
                area(-50.0, 10.0, 2000.0, 1200.0),
                &[area(0.0, 0.0, 1920.0, 1040.0)]
            ),
            Some((-50.0, 0.0)),
            "neither edge inside it"
        );
    }

    #[test]
    fn without_a_valid_screen_or_place_the_window_is_left_to_the_system() {
        let saved = area(100.0, 100.0, 800.0, 600.0);
        let screen = area(0.0, 0.0, 1920.0, 1040.0);
        assert_eq!(place(saved, &[]), None);
        assert_eq!(place(saved, &[area(0.0, 0.0, 0.0, 1040.0)]), None);
        assert_eq!(place(area(f64::NAN, 0.0, 800.0, 600.0), &[screen]), None);
        assert_eq!(place(area(0.0, 0.0, -1.0, 600.0), &[screen]), None);
    }
}
