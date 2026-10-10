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

//! The Cursor pseudo-encoding (-239), read as noVNC's `_handleCursor`: the server's pointer
//! shape, drawn by the client where its own pointer is. The rectangle's place is the hotspot,
//! its size the image's; then its pixels in the client's format, and a mask of one bit a
//! pixel, rows padded to a byte, set where the pixel shows.

use std::sync::{Arc, Mutex, PoisonError};

use super::screen::{PIXEL_BYTES, Rect};

/// Widest and tallest cursor accepted, in pixels: bounds what a server can make the client
/// wait for and keep. Cursors are 256 at most on the systems a server runs.
pub const MAX_CURSOR_SIDE: u16 = 512;

/// Bits of a mask byte.
const MASK_BITS: usize = 8;

/// The mask's first pixel of a byte is its highest bit.
const MASK_FIRST: u8 = 0x80;

/// Alpha of a pixel the mask shows, and of one it hides.
const SHOWN: u8 = u8::MAX;
const HIDDEN: u8 = 0;

/// Where the alpha is in an RGBA pixel.
const ALPHA: usize = 3;

/// noVNC's `RFB.cursors.dot`: shown instead of a cursor nothing of which shows, as the C#
/// Heimdall asks noVNC with `showDotCursor`.
const DOT_SIDE: u16 = 3;
const DOT_HOTSPOT: u16 = 1;
const DOT: [u8; 36] = [
    255, 255, 255, 255, 0, 0, 0, 255, 255, 255, 255, 255, //
    0, 0, 0, 255, 0, 0, 0, 0, 0, 0, 0, 255, //
    255, 255, 255, 255, 0, 0, 0, 255, 255, 255, 255, 255, //
];

/// A pointer shape: RGBA pixels, rows top to bottom, and the pixel that points.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Cursor {
    width: u16,
    height: u16,
    hotspot: (u16, u16),
    rgba: Vec<u8>,
}

impl Cursor {
    /// Width, 0 for no cursor.
    #[must_use]
    pub fn width(&self) -> u16 {
        self.width
    }

    /// Height, 0 for no cursor.
    #[must_use]
    pub fn height(&self) -> u16 {
        self.height
    }

    /// The pixel that points, from the image's corner.
    #[must_use]
    pub fn hotspot(&self) -> (u16, u16) {
        self.hotspot
    }

    /// RGBA pixels, rows top to bottom; transparent where the mask hides them.
    #[must_use]
    pub fn rgba(&self) -> &[u8] {
        &self.rgba
    }

    /// Whether nothing of it shows: empty, or every pixel masked.
    #[must_use]
    pub fn is_invisible(&self) -> bool {
        self.rgba
            .as_chunks::<PIXEL_BYTES>()
            .0
            .iter()
            .all(|pixel| pixel[ALPHA] == HIDDEN)
    }

    /// noVNC's dot, a 3 by 3 ring pointing at its middle.
    #[must_use]
    pub fn dot() -> Self {
        Self {
            width: DOT_SIDE,
            height: DOT_SIDE,
            hotspot: (DOT_HOTSPOT, DOT_HOTSPOT),
            rgba: DOT.to_vec(),
        }
    }

    /// What to draw, as noVNC with `showDotCursor` as the C# Heimdall sets it: this cursor,
    /// or the dot when nothing of it shows.
    #[must_use]
    pub fn shown(&self) -> Self {
        if self.is_invisible() {
            Self::dot()
        } else {
            self.clone()
        }
    }
}

/// Bytes of a cursor the size of `rect`: its pixels, then its mask. A cursor past
/// [`MAX_CURSOR_SIDE`] is refused before anything is waited for.
pub(crate) fn sizes(rect: Rect) -> Result<(usize, usize), String> {
    if rect.width > MAX_CURSOR_SIDE || rect.height > MAX_CURSOR_SIDE {
        return Err(format!(
            "a {}x{} cursor, past {MAX_CURSOR_SIDE} pixels a side",
            rect.width, rect.height
        ));
    }
    let mask_row = usize::from(rect.width).div_ceil(MASK_BITS);
    Ok((
        rect.area() * PIXEL_BYTES,
        mask_row * usize::from(rect.height),
    ))
}

/// The cursor of `rect`, from its `pixels` and `mask`, of the [`sizes`] of `rect`. The
/// pixels are in the format the client asked for, red in the lowest byte; the hotspot is
/// kept inside the image.
pub(crate) fn decode(rect: Rect, pixels: &[u8], mask: &[u8]) -> Cursor {
    if rect.width == 0 || rect.height == 0 {
        return Cursor::default();
    }
    let mask_row = usize::from(rect.width).div_ceil(MASK_BITS);
    let mut rgba = Vec::with_capacity(rect.area() * PIXEL_BYTES);
    let mut pixels = pixels.as_chunks::<PIXEL_BYTES>().0.iter();
    for y in 0..usize::from(rect.height) {
        for x in 0..usize::from(rect.width) {
            let pixel = pixels.next().copied().unwrap_or_default();
            let bits = mask.get(y * mask_row + x / MASK_BITS).copied().unwrap_or(0);
            let shown = (bits << (x % MASK_BITS)) & MASK_FIRST != 0;
            rgba.extend_from_slice(&[
                pixel[0],
                pixel[1],
                pixel[2],
                if shown { SHOWN } else { HIDDEN },
            ]);
        }
    }
    Cursor {
        width: rect.width,
        height: rect.height,
        hotspot: (rect.x.min(rect.width - 1), rect.y.min(rect.height - 1)),
        rgba,
    }
}

/// The server's cursor, shared between the session and whoever draws it, with a count that
/// grows each time it changes.
#[derive(Debug, Clone, Default)]
pub struct RemoteCursor(Arc<Mutex<(u64, Cursor)>>);

impl RemoteCursor {
    /// Calls `read` with the count of changes and the cursor; none until the server sends
    /// one, as noVNC starts.
    pub fn read<T>(&self, read: impl FnOnce(u64, &Cursor) -> T) -> T {
        let held = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        read(held.0, &held.1)
    }

    /// Takes the server's new cursor.
    pub(crate) fn set(&self, cursor: Cursor) {
        let mut held = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        held.0 = held.0.wrapping_add(1);
        held.1 = cursor;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: u16, y: u16, width: u16, height: u16) -> Rect {
        Rect {
            x,
            y,
            width,
            height,
        }
    }

    #[test]
    fn the_mask_sets_alpha_bit_by_bit_high_bit_first_rows_padded() {
        // 9 by 2: two mask bytes a row. Row 0 shows pixels 0 and 8, row 1 pixel 1 only.
        let area = rect(4, 1, 9, 2);
        let (pixel_bytes, mask_bytes) = sizes(area).expect("sizes");
        assert_eq!((pixel_bytes, mask_bytes), (9 * 2 * 4, 2 * 2));
        let pixels: Vec<u8> = (0..18u8)
            .flat_map(|index| [index, 0x10, 0x20, 0x99])
            .collect();
        let mask = [0b1000_0000, 0b1000_0000, 0b0100_0000, 0];
        let cursor = decode(area, &pixels, &mask);
        assert_eq!((cursor.width(), cursor.height()), (9, 2));
        assert_eq!(cursor.hotspot(), (4, 1));
        let alphas: Vec<u8> = cursor.rgba().chunks(4).map(|pixel| pixel[3]).collect();
        let mut expected = vec![0; 18];
        expected[0] = 255;
        expected[8] = 255;
        expected[10] = 255;
        assert_eq!(alphas, expected);
        // Red, green and blue as they came: red in the lowest byte, as asked.
        assert_eq!(&cursor.rgba()[..4], &[0, 0x10, 0x20, 255]);
        assert!(!cursor.is_invisible());
    }

    #[test]
    fn an_empty_or_fully_masked_cursor_is_invisible_and_shows_the_dot() {
        let empty = decode(rect(0, 0, 0, 0), &[], &[]);
        assert!(empty.is_invisible());
        assert_eq!(empty.shown(), Cursor::dot());
        let masked = decode(rect(0, 0, 2, 1), &[1; 8], &[0]);
        assert!(masked.is_invisible());
        assert_eq!(masked.shown().width(), DOT_SIDE);
    }

    #[test]
    fn a_hotspot_outside_its_image_is_kept_inside() {
        let cursor = decode(rect(40, 7, 2, 2), &[0; 16], &[0xc0, 0xc0]);
        assert_eq!(cursor.hotspot(), (1, 1));
    }

    #[test]
    fn a_cursor_past_its_bound_is_refused() {
        assert!(sizes(rect(0, 0, MAX_CURSOR_SIDE + 1, 1)).is_err());
        assert!(sizes(rect(0, 0, 1, MAX_CURSOR_SIDE + 1)).is_err());
        assert!(sizes(rect(0, 0, MAX_CURSOR_SIDE, MAX_CURSOR_SIDE)).is_ok());
    }

    #[test]
    fn a_shared_cursor_counts_its_changes() {
        let shared = RemoteCursor::default();
        assert_eq!(shared.read(|count, cursor| (count, cursor.width())), (0, 0));
        shared.set(Cursor::dot());
        assert_eq!(shared.read(|count, cursor| (count, cursor.width())), (1, 3));
    }
}
