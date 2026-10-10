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

//! The remote desktop as the client holds it: RGBA pixels, rows top to bottom.

/// Widest and tallest desktop accepted, in pixels: bounds what a server can make the client
/// allocate.
pub const MAX_SIDE: u16 = 8192;

/// Bytes of one pixel (RGBA).
pub(crate) const PIXEL_BYTES: usize = 4;

/// Opaque alpha.
const OPAQUE: u8 = 255;

/// A rectangle of the desktop, in pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    /// Left.
    pub x: u16,
    /// Top.
    pub y: u16,
    /// Width.
    pub width: u16,
    /// Height.
    pub height: u16,
}

impl Rect {
    /// Number of pixels.
    #[must_use]
    pub fn area(self) -> usize {
        usize::from(self.width) * usize::from(self.height)
    }
}

/// The desktop's pixels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Screen {
    width: u16,
    height: u16,
    pixels: Vec<u8>,
}

impl Screen {
    /// A black desktop of `width` by `height`.
    #[must_use]
    pub fn new(width: u16, height: u16) -> Self {
        let mut pixels = vec![0; usize::from(width) * usize::from(height) * PIXEL_BYTES];
        for pixel in pixels.as_chunks_mut::<PIXEL_BYTES>().0 {
            pixel[3] = OPAQUE;
        }
        Self {
            width,
            height,
            pixels,
        }
    }

    /// Width.
    #[must_use]
    pub fn width(&self) -> u16 {
        self.width
    }

    /// Height.
    #[must_use]
    pub fn height(&self) -> u16 {
        self.height
    }

    /// RGBA pixels, rows top to bottom.
    #[must_use]
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    /// Whether `rect` lies inside the desktop.
    #[must_use]
    pub fn contains(&self, rect: Rect) -> bool {
        u32::from(rect.x) + u32::from(rect.width) <= u32::from(self.width)
            && u32::from(rect.y) + u32::from(rect.height) <= u32::from(self.height)
    }

    fn offset(&self, x: u16, y: u16) -> usize {
        (usize::from(y) * usize::from(self.width) + usize::from(x)) * PIXEL_BYTES
    }

    /// Sets the pixel at `x`, `y` to `rgb`. The caller checked the bounds.
    pub(crate) fn set(&mut self, x: u16, y: u16, rgb: [u8; 3]) {
        let at = self.offset(x, y);
        self.pixels[at..at + PIXEL_BYTES].copy_from_slice(&[rgb[0], rgb[1], rgb[2], OPAQUE]);
    }

    /// Lays `rgba` over the pixel at `x`, `y`, as a canvas draws an image with alpha: its
    /// colour weighed by its alpha against the one under it. The caller checked the bounds.
    pub(crate) fn blend(&mut self, x: u16, y: u16, rgba: [u8; PIXEL_BYTES]) {
        let at = self.offset(x, y);
        let alpha = u16::from(rgba[3]);
        let under = u16::from(OPAQUE) - alpha;
        for (channel, over) in self.pixels[at..at + 3].iter_mut().zip(rgba) {
            let mixed =
                (u16::from(over) * alpha + u16::from(*channel) * under + u16::from(OPAQUE) / 2)
                    / u16::from(OPAQUE);
            *channel = u8::try_from(mixed).unwrap_or(OPAQUE);
        }
        self.pixels[at + 3] = OPAQUE;
    }

    /// Copies `rect` of `source`, a screen of the same size; nothing outside either.
    pub(crate) fn copy_from(&mut self, source: &Screen, rect: Rect) {
        if !source.contains(rect) || !self.contains(rect) || source.width != self.width {
            return;
        }
        let row = usize::from(rect.width) * PIXEL_BYTES;
        for y in rect.y..rect.y + rect.height {
            let at = self.offset(rect.x, y);
            self.pixels[at..at + row].copy_from_slice(&source.pixels[at..at + row]);
        }
    }

    /// Fills `rect` with `rgb`. The caller checked the bounds.
    pub(crate) fn fill(&mut self, rect: Rect, rgb: [u8; 3]) {
        for y in rect.y..rect.y + rect.height {
            for x in rect.x..rect.x + rect.width {
                self.set(x, y, rgb);
            }
        }
    }

    /// Copies the `width` by `height` block at `from` to `to`, as if through a buffer: the two
    /// may overlap. The caller checked the bounds.
    pub(crate) fn copy(&mut self, from: (u16, u16), to: Rect) {
        let row = usize::from(to.width) * PIXEL_BYTES;
        let mut block = Vec::with_capacity(row * usize::from(to.height));
        for line in 0..to.height {
            let at = self.offset(from.0, from.1 + line);
            block.extend_from_slice(&self.pixels[at..at + row]);
        }
        for (line, source) in (0..to.height).zip(block.chunks_exact(row.max(1))) {
            let at = self.offset(to.x, to.y + line);
            self.pixels[at..at + row].copy_from_slice(source);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(screen: &Screen, x: u16, y: u16) -> [u8; 4] {
        let offset = screen.offset(x, y);
        screen.pixels[offset..offset + PIXEL_BYTES]
            .try_into()
            .expect("pixel")
    }

    #[test]
    fn a_new_screen_is_opaque_black() {
        let screen = Screen::new(3, 2);
        assert_eq!(screen.pixels().len(), 3 * 2 * PIXEL_BYTES);
        assert!(
            screen
                .pixels()
                .as_chunks::<PIXEL_BYTES>()
                .0
                .iter()
                .all(|pixel| *pixel == [0, 0, 0, OPAQUE])
        );
    }

    #[test]
    fn contains_stops_at_the_last_column_and_row() {
        let screen = Screen::new(10, 5);
        let rect = |x, y, width, height| Rect {
            x,
            y,
            width,
            height,
        };
        assert!(screen.contains(rect(0, 0, 10, 5)));
        assert!(screen.contains(rect(9, 4, 1, 1)));
        assert!(!screen.contains(rect(9, 4, 2, 1)));
        assert!(!screen.contains(rect(0, 5, 1, 1)));
        // No overflow at the edge of u16.
        assert!(!screen.contains(rect(u16::MAX, 0, 2, 1)));
    }

    #[test]
    fn an_overlapping_copy_moves_the_block_as_a_whole() {
        // One row: 0 1 2 3 4, as red values. Moving [0..3] one to the right gives 0 0 1 2 4.
        let mut screen = Screen::new(5, 1);
        for x in 0..5 {
            screen.set(x, 0, [u8::try_from(x).expect("small"), 0, 0]);
        }
        screen.copy(
            (0, 0),
            Rect {
                x: 1,
                y: 0,
                width: 3,
                height: 1,
            },
        );
        let reds: Vec<u8> = (0..5).map(|x| at(&screen, x, 0)[0]).collect();
        assert_eq!(reds, [0, 0, 1, 2, 4]);
    }
}
