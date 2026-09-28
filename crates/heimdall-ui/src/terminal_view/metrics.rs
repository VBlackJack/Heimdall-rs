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

//! Cell geometry of the embedded terminal font, and pixel to cell conversions.
//!
//! Measured in the four embedded Source Code Pro faces on 2026-09-26 (`hmtx`, `hhea`):
//! every glyph advances 600 units of a 1000-unit em; ascender 984, descender 273, no line
//! gap. So a cell is 0.6 em wide and 1.257 em tall, whatever the face.

use heimdall_term::{CellPixels, CellPoint, GridSize};
use iced::{Point, Rectangle, Size};

/// Terminal font size in logical pixels; 15 makes the cell exactly 9 pixels wide.
pub const DEFAULT_FONT_SIZE: f32 = 15.0;

/// Advance of every glyph, in em.
const ADVANCE_EM: f32 = 0.6;

/// Ascender plus descender, in em.
const LINE_EM: f32 = 1.257;

/// Size of one cell.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CellMetrics {
    /// Font size.
    pub font_size: f32,
    /// Cell width in logical pixels.
    pub width: f32,
    /// Cell height in logical pixels, rounded up so rows never overlap.
    pub height: f32,
}

impl CellMetrics {
    /// Metrics at `font_size`.
    #[must_use]
    pub fn for_size(font_size: f32) -> Self {
        Self {
            font_size,
            width: font_size * ADVANCE_EM,
            height: (font_size * LINE_EM).ceil(),
        }
    }

    /// The grid that fits in `size`.
    #[must_use]
    pub fn grid(&self, size: Size) -> GridSize {
        GridSize {
            cols: whole(size.width / self.width),
            rows: whole(size.height / self.height),
        }
        .clamped()
    }

    /// The cell under `point`, relative to `bounds`, clamped to the grid, with the half of
    /// the cell it falls in: the half decides whether a selection includes the cell.
    #[must_use]
    pub fn cell_at(&self, bounds: Rectangle, grid: GridSize, point: Point) -> CellPoint {
        let x = (point.x - bounds.x).max(0.0) / self.width;
        let y = (point.y - bounds.y).max(0.0) / self.height;
        let col = whole(x).min(grid.cols.saturating_sub(1));
        CellPoint {
            row: whole(y).min(grid.rows.saturating_sub(1)),
            col,
            right_half: x - as_f32(col) >= 0.5,
        }
    }

    /// Cell size in whole pixels, for applications that ask for the window size.
    #[must_use]
    pub fn pixels(&self) -> CellPixels {
        CellPixels {
            width: whole_u16(self.width),
            height: whole_u16(self.height),
        }
    }

    /// Top-left corner of a cell.
    #[must_use]
    pub fn origin(&self, bounds: Rectangle, row: usize, col: usize) -> Point {
        Point::new(
            bounds.x + as_f32(col) * self.width,
            bounds.y + as_f32(row) * self.height,
        )
    }
}

impl Default for CellMetrics {
    fn default() -> Self {
        Self::for_size(DEFAULT_FONT_SIZE)
    }
}

/// Whole part of a non-negative float, 0 for negative or not-a-number values.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "floor of a finite, non-negative, screen-sized value"
)]
fn whole(value: f32) -> usize {
    if value.is_finite() && value > 0.0 {
        value.floor() as usize
    } else {
        0
    }
}

fn whole_u16(value: f32) -> u16 {
    u16::try_from(whole(value.round())).unwrap_or(u16::MAX)
}

/// A grid index as a float; grids are far below the range where precision is lost.
#[allow(
    clippy::cast_precision_loss,
    reason = "grid indices stay far below 2^24"
)]
pub(crate) fn as_f32(value: usize) -> f32 {
    value as f32
}

#[cfg(test)]
mod tests {
    use iced::{Point, Rectangle, Size};

    use super::{CellMetrics, DEFAULT_FONT_SIZE};

    #[test]
    fn the_default_size_is_the_one_the_settings_start_at() {
        // Two constants, one decision: a terminal drawn before the settings are read, and one
        // drawn from them, look the same.
        assert!(
            (DEFAULT_FONT_SIZE - f32::from(heimdall_core::settings::TERMINAL_FONT_SIZE_DEFAULT))
                .abs()
                < f32::EPSILON
        );
    }

    #[test]
    fn the_default_cell_is_nine_by_nineteen() {
        let metrics = CellMetrics::for_size(DEFAULT_FONT_SIZE);
        assert!((metrics.width - 9.0).abs() < 1e-4);
        assert!((metrics.height - 19.0).abs() < 1e-4);
    }

    #[test]
    fn the_grid_is_what_fits_whole() {
        let metrics = CellMetrics::default();
        let grid = metrics.grid(Size::new(9.0 * 80.0 + 8.9, 19.0 * 24.0 + 18.0));
        assert_eq!((grid.cols, grid.rows), (80, 24));
        let tiny = metrics.grid(Size::new(0.0, -5.0));
        assert_eq!((tiny.cols, tiny.rows), (2, 1), "clamped, never zero");
    }

    #[test]
    fn a_point_maps_to_its_cell_and_half() {
        let metrics = CellMetrics::default();
        let bounds = Rectangle::new(Point::new(100.0, 50.0), Size::new(900.0, 600.0));
        let grid = metrics.grid(bounds.size());
        let left = metrics.cell_at(
            bounds,
            grid,
            Point::new(100.0 + 9.0 * 3.0 + 2.0, 50.0 + 20.0),
        );
        assert_eq!((left.row, left.col, left.right_half), (1, 3, false));
        let right = metrics.cell_at(bounds, grid, Point::new(100.0 + 9.0 * 3.0 + 6.0, 50.0));
        assert!(right.right_half);
        let beyond = metrics.cell_at(bounds, grid, Point::new(5000.0, 5000.0));
        assert_eq!((beyond.row, beyond.col), (grid.rows - 1, grid.cols - 1));
    }

    #[test]
    fn the_middle_of_a_cell_belongs_to_its_right_half() {
        let metrics = CellMetrics::default();
        let bounds = Rectangle::new(Point::ORIGIN, Size::new(900.0, 600.0));
        let grid = metrics.grid(bounds.size());
        let middle = metrics.cell_at(bounds, grid, Point::new(9.0 * 2.0 + 4.5, 0.0));
        assert_eq!((middle.col, middle.right_half), (2, true));
        let before = metrics.cell_at(bounds, grid, Point::new(9.0 * 2.0 + 4.4, 0.0));
        assert!(!before.right_half);
        assert_eq!(metrics.pixels().width, 9);
        assert_eq!(metrics.origin(bounds, 2, 3), Point::new(27.0, 38.0));
    }
}
