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

//! A row of the Password Generator's placement bar, as the C# `PlacementDigitsTrack` and
//! `PlacementSpecialsTrack` with their thumbs and `PlacementBarGeometry`: a notch per place
//! a character can go, a cursor per character; a cursor dragged previews on every move and
//! writes its place when dropped; the wheel over a cursor and, once one was clicked, the
//! arrows, Home and End move it by one place.

use iced::advanced::mouse;
use iced::keyboard::{self, Key, key::Named};
use iced::widget::canvas::{self, Action, Frame, Geometry, Path, Stroke};
use iced::{Point, Rectangle, Renderer, Size, Theme};

use heimdall_core::tools::password_rules::{self as rules, MAXIMUM_POSITION_PERCENT};

use crate::shell::Message;

/// Height of a row, as the C# canvas.
pub const TRACK_HEIGHT: f32 = 30.0;

/// The width of a cursor, which the track leaves room for at both ends, as the C#
/// `PlacementBarGeometry.CursorWidth`.
const CURSOR_WIDTH: f32 = 14.0;

/// The height of a cursor and its distance from the top, as the C# `Height="22"` and
/// `Canvas.SetTop(cursor, 3)`.
const CURSOR_HEIGHT: f32 = 22.0;
const CURSOR_TOP: f32 = 3.0;

/// The corners of a cursor, as the C# `CornerRadiusSm`.
const CURSOR_RADIUS: f32 = 3.0;

/// How tall a notch is, how dark, and where it hangs from, as the C# `TickHeight`,
/// `TickOpacity` and `PlacementTickBaseline`.
const TICK_HEIGHT: f32 = 5.0;
const TICK_OPACITY: f32 = 0.35;
const TICK_BASELINE: f32 = 28.0;

/// The closest two notches may be before none are drawn, as the C# `TickMergeGap`.
const TICK_MERGE_GAP: f32 = 2.5;

/// The line of the selected cursor.
const SELECTED_LINE: f32 = 2.0;

/// The width a cursor's left edge travels, as the C# `Usable`.
fn usable(width: f32) -> f32 {
    (width - CURSOR_WIDTH).max(1.0)
}

/// `value` as `f32`, exact for a place on a bar.
fn as_f32(value: usize) -> f32 {
    u16::try_from(value).map_or(f32::from(u16::MAX), f32::from)
}

/// The notches drawn across a track this wide: one per place, or none, as the C#
/// `TickCount`.
#[must_use]
pub fn tick_count(slots: usize, width: f32) -> usize {
    if slots >= 2 && usable(width) / as_f32(slots - 1) >= TICK_MERGE_GAP {
        slots
    } else {
        0
    }
}

/// A percent as the `f32` a drawing takes.
#[expect(
    clippy::cast_possible_truncation,
    reason = "a percent between 0 and 100, drawn on screen"
)]
fn percent_f32(percent: f64) -> f32 {
    percent as f32
}

/// The left edge of a cursor at `percent`, as the C# `CursorLeft`.
fn cursor_left(percent: f64, width: f32) -> f32 {
    usable(width) * percent_f32(percent) / percent_f32(MAXIMUM_POSITION_PERCENT)
}

/// A row of the bar.
pub struct PlacementTrack<'a> {
    positions: Vec<f64>,
    slots: usize,
    on_move: Box<dyn Fn(usize, f64, bool) -> Message + 'a>,
}

impl<'a> PlacementTrack<'a> {
    /// A row of cursors at `positions` over `slots` places; `on_move` says a cursor moved,
    /// its index, its place, and whether it is written down.
    pub fn new(
        positions: Vec<f64>,
        slots: usize,
        on_move: impl Fn(usize, f64, bool) -> Message + 'a,
    ) -> Self {
        Self {
            positions,
            slots,
            on_move: Box::new(on_move),
        }
    }

    /// The cursor under `x`, the last drawn first.
    fn hit(&self, x: f32, width: f32) -> Option<usize> {
        (0..self.positions.len()).rev().find(|index| {
            let left = cursor_left(self.positions[*index], width);
            (left..=left + CURSOR_WIDTH).contains(&x)
        })
    }

    /// The place under `x`, snapped, as the C# `OnPlacementCursorDrag`.
    fn percent_at(&self, x: f32, width: f32) -> f64 {
        let moved = (x - CURSOR_WIDTH / 2.0).clamp(0.0, usable(width));
        rules::snap_to_slot(
            self.slots,
            f64::from(moved / usable(width)) * MAXIMUM_POSITION_PERCENT,
        )
    }

    /// Cursor `index` moved by `steps` places, written down.
    fn step(&self, index: usize, steps: f64) -> Option<Action<Message>> {
        let current = *self.positions.get(index)?;
        let one = rules::step_percent(self.slots);
        let percent = rules::snap_to_slot(self.slots, current + steps * one);
        Some(Action::publish((self.on_move)(index, percent, true)).and_capture())
    }
}

/// What a row keeps between frames: the cursor held, the one selected, the place shown
/// while it is dragged.
#[derive(Debug, Default)]
pub struct TrackState {
    dragging: Option<usize>,
    selected: Option<usize>,
    preview: Option<(usize, f64)>,
}

impl canvas::Program<Message> for PlacementTrack<'_> {
    type State = TrackState;

    fn update(
        &self,
        state: &mut TrackState,
        event: &canvas::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<Action<Message>> {
        match event {
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let hit = cursor
                    .position_in(bounds)
                    .and_then(|point| self.hit(point.x, bounds.width));
                state.selected = hit;
                let index = hit?;
                state.dragging = Some(index);
                Some(Action::request_redraw().and_capture())
            }
            canvas::Event::Mouse(mouse::Event::CursorMoved { position }) => {
                let index = state.dragging?;
                let percent = self.percent_at(position.x - bounds.x, bounds.width);
                if state.preview == Some((index, percent)) {
                    return Some(Action::capture());
                }
                state.preview = Some((index, percent));
                Some(Action::publish((self.on_move)(index, percent, false)).and_capture())
            }
            canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                let index = state.dragging.take()?;
                let percent = state
                    .preview
                    .take()
                    .map_or_else(|| self.positions.get(index).copied(), |(_, at)| Some(at))?;
                Some(Action::publish((self.on_move)(index, percent, true)).and_capture())
            }
            canvas::Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                let point = cursor.position_in(bounds)?;
                let index = self.hit(point.x, bounds.width)?;
                let rise = match delta {
                    mouse::ScrollDelta::Lines { y, .. } | mouse::ScrollDelta::Pixels { y, .. } => {
                        *y
                    }
                };
                self.step(index, if rise > 0.0 { 1.0 } else { -1.0 })
            }
            canvas::Event::Keyboard(keyboard::Event::KeyPressed { key, .. }) => {
                let index = state.selected?;
                let steps = match key {
                    Key::Named(Named::ArrowLeft | Named::ArrowDown) => -1.0,
                    Key::Named(Named::ArrowRight | Named::ArrowUp) => 1.0,
                    Key::Named(Named::Home) => -f64::from(u16::MAX),
                    Key::Named(Named::End) => f64::from(u16::MAX),
                    _ => return None,
                };
                self.step(index, steps)
            }
            _ => None,
        }
    }

    fn draw(
        &self,
        state: &TrackState,
        renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let palette = theme.extended_palette();
        let width = bounds.width;
        let mut frame = Frame::new(renderer, bounds.size());
        let ticks = tick_count(self.slots, width);
        let tick_color = palette.secondary.base.color.scale_alpha(TICK_OPACITY);
        for slot in 0..ticks {
            let x = CURSOR_WIDTH / 2.0 + usable(width) * as_f32(slot) / as_f32(ticks - 1);
            frame.stroke(
                &Path::line(
                    Point::new(x, TICK_BASELINE - TICK_HEIGHT),
                    Point::new(x, TICK_BASELINE),
                ),
                Stroke::default().with_color(tick_color).with_width(1.0),
            );
        }
        for (index, position) in self.positions.iter().enumerate() {
            let percent = match state.preview {
                Some((dragged, at)) if dragged == index => at,
                _ => *position,
            };
            let cursor = Path::rounded_rectangle(
                Point::new(cursor_left(percent, width), CURSOR_TOP),
                Size::new(CURSOR_WIDTH, CURSOR_HEIGHT),
                CURSOR_RADIUS.into(),
            );
            frame.fill(&cursor, palette.primary.base.color);
            if state.selected == Some(index) {
                frame.stroke(
                    &cursor,
                    Stroke::default()
                        .with_color(palette.background.base.text)
                        .with_width(SELECTED_LINE),
                );
            }
        }
        vec![frame.into_geometry()]
    }

    fn mouse_interaction(
        &self,
        state: &TrackState,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if state.dragging.is_some() {
            return mouse::Interaction::Grabbing;
        }
        cursor
            .position_in(bounds)
            .and_then(|point| self.hit(point.x, bounds.width))
            .map_or(mouse::Interaction::default(), |_| mouse::Interaction::Grab)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notches_are_one_per_place_or_none_when_too_close() {
        // As the C# PlacementBarGeometryTests.
        assert_eq!(tick_count(17, 400.0), 17);
        assert_eq!(tick_count(65, 400.0), 65);
        assert_eq!(tick_count(60, 64.0), 0);
        assert_eq!(tick_count(21, 64.0), 21);
        assert_eq!(tick_count(1, 400.0), 0);
        assert!((cursor_left(100.0, 414.0) - 400.0).abs() < f32::EPSILON);
    }

    #[test]
    fn a_cursor_is_hit_where_it_is_drawn_and_a_place_snaps() {
        let track = PlacementTrack::new(vec![0.0, 100.0], 17, |_, _, _| {
            Message::ToolsFilter(String::new())
        });
        assert_eq!(track.hit(5.0, 414.0), Some(0));
        assert_eq!(track.hit(405.0, 414.0), Some(1));
        assert_eq!(track.hit(200.0, 414.0), None);
        assert!((track.percent_at(7.0 + 400.0 * 0.07, 414.0) - 6.25).abs() < 1e-9);
    }
}
