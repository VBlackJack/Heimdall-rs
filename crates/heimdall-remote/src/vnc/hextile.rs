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

//! Hextile (encoding 5), read as noVNC's `HextileDecoder`: the rectangle in tiles of 16 by
//! 16, left to right then top to bottom, the last ones of a row or column cut to the
//! rectangle. Each tile starts with a byte of flags: raw pixels; or a background, a
//! foreground and subrectangles, the colours kept from tile to tile.
//!
//! The rectangle is measured whole before anything is drawn, so one cut between two reads
//! leaves the colours kept as they were.

use super::screen::{PIXEL_BYTES, Rect, Screen};

/// Side of a tile.
const TILE: u16 = 16;

/// The flags of a tile.
const RAW: u8 = 0x01;
const BACKGROUND: u8 = 0x02;
const FOREGROUND: u8 = 0x04;
const ANY_SUBRECTS: u8 = 0x08;
const SUBRECTS_COLOURED: u8 = 0x10;

/// Highest flags noVNC accepts.
const MAX_FLAGS: u8 = 30;

/// A subrectangle's place and size: a nibble each, the size less one.
const NIBBLE_SHIFT: u8 = 4;
const NIBBLE: u8 = 0x0f;

/// Bytes of a subrectangle's place and size.
const SUBRECT_BYTES: usize = 2;

/// The decoder of one connection: the colours a tile leaves to the next, and its flags.
pub(crate) struct Hextile {
    background: [u8; 3],
    foreground: [u8; 3],
    last_flags: u8,
}

/// A tile: where it is on the desktop, and its size.
#[derive(Clone, Copy)]
struct Tile {
    x: u16,
    y: u16,
    width: u16,
    height: u16,
}

impl Hextile {
    /// A decoder, its colours black.
    pub(crate) fn new() -> Self {
        Self {
            background: [0; 3],
            foreground: [0; 3],
            last_flags: 0,
        }
    }

    /// Draws `rect`, coded at the start of `data`, onto `screen`: the bytes it took, or
    /// `None` when more are needed, nothing changed. The caller checked that `rect` lies
    /// inside it.
    pub(crate) fn decode(
        &mut self,
        data: &[u8],
        rect: Rect,
        screen: &mut Screen,
    ) -> Result<Option<usize>, String> {
        let Some(length) = measure(data, rect)? else {
            return Ok(None);
        };
        let mut at = 0;
        for tile in tiles(rect) {
            let flags = data[at];
            at += 1;
            if flags & RAW != 0 {
                let size = tile_area(tile) * PIXEL_BYTES;
                let mut pixels = data[at..at + size].as_chunks::<PIXEL_BYTES>().0.iter();
                at += size;
                for y in tile.y..tile.y + tile.height {
                    for x in tile.x..tile.x + tile.width {
                        if let Some(pixel) = pixels.next() {
                            screen.set(x, y, [pixel[0], pixel[1], pixel[2]]);
                        }
                    }
                }
            } else if flags == 0 {
                // A blank tile after a raw one is left as it is, as noVNC does.
                if self.last_flags & RAW == 0 {
                    screen.fill(tile.rect(), self.background);
                }
            } else {
                if flags & BACKGROUND != 0 {
                    self.background = rgb(&data[at..]);
                    at += PIXEL_BYTES;
                }
                if flags & FOREGROUND != 0 {
                    self.foreground = rgb(&data[at..]);
                    at += PIXEL_BYTES;
                }
                screen.fill(tile.rect(), self.background);
                if flags & ANY_SUBRECTS != 0 {
                    let count = data[at];
                    at += 1;
                    for _ in 0..count {
                        let colour = if flags & SUBRECTS_COLOURED != 0 {
                            let colour = rgb(&data[at..]);
                            at += PIXEL_BYTES;
                            colour
                        } else {
                            self.foreground
                        };
                        let (place, size) = (data[at], data[at + 1]);
                        at += SUBRECT_BYTES;
                        let sub = subrect(tile, place, size)?;
                        screen.fill(sub, colour);
                    }
                }
            }
            self.last_flags = flags;
        }
        Ok(Some(length))
    }
}

impl Tile {
    fn rect(self) -> Rect {
        Rect {
            x: self.x,
            y: self.y,
            width: self.width,
            height: self.height,
        }
    }
}

/// The tiles of `rect`, rows of them top to bottom, each left to right.
fn tiles(rect: Rect) -> impl Iterator<Item = Tile> {
    let rows = (0..rect.height).step_by(usize::from(TILE));
    rows.flat_map(move |dy| {
        (0..rect.width)
            .step_by(usize::from(TILE))
            .map(move |dx| Tile {
                x: rect.x + dx,
                y: rect.y + dy,
                width: TILE.min(rect.width - dx),
                height: TILE.min(rect.height - dy),
            })
    })
}

fn tile_area(tile: Tile) -> usize {
    usize::from(tile.width) * usize::from(tile.height)
}

/// The red, green and blue of a pixel at the start of `bytes`, which holds one.
fn rgb(bytes: &[u8]) -> [u8; 3] {
    [bytes[0], bytes[1], bytes[2]]
}

/// A subrectangle of `tile` from its place and size bytes; one past the tile is refused.
fn subrect(tile: Tile, place: u8, size: u8) -> Result<Rect, String> {
    let (x, y) = (u16::from(place >> NIBBLE_SHIFT), u16::from(place & NIBBLE));
    let (width, height) = (
        u16::from(size >> NIBBLE_SHIFT) + 1,
        u16::from(size & NIBBLE) + 1,
    );
    if x + width > tile.width || y + height > tile.height {
        return Err(format!(
            "a Hextile subrectangle {width}x{height} at {x},{y} outside its {}x{} tile",
            tile.width, tile.height
        ));
    }
    Ok(Rect {
        x: tile.x + x,
        y: tile.y + y,
        width,
        height,
    })
}

/// The bytes of the whole rectangle at the start of `data`, its flags checked; `None` when
/// more are needed.
fn measure(data: &[u8], rect: Rect) -> Result<Option<usize>, String> {
    let mut at = 0;
    for tile in tiles(rect) {
        let Some(&flags) = data.get(at) else {
            return Ok(None);
        };
        if flags > MAX_FLAGS {
            return Err(format!("an illegal Hextile tile of flags {flags}"));
        }
        at += 1;
        if flags & RAW != 0 {
            at += tile_area(tile) * PIXEL_BYTES;
            continue;
        }
        if flags & BACKGROUND != 0 {
            at += PIXEL_BYTES;
        }
        if flags & FOREGROUND != 0 {
            at += PIXEL_BYTES;
        }
        if flags & ANY_SUBRECTS != 0 {
            let Some(&count) = data.get(at) else {
                return Ok(None);
            };
            at += 1;
            let each = if flags & SUBRECTS_COLOURED != 0 {
                PIXEL_BYTES + SUBRECT_BYTES
            } else {
                SUBRECT_BYTES
            };
            at += usize::from(count) * each;
        }
    }
    Ok((at <= data.len()).then_some(at))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rgb_at(screen: &Screen, x: usize, y: usize) -> [u8; 3] {
        let at = (y * usize::from(screen.width()) + x) * PIXEL_BYTES;
        rgb(&screen.pixels()[at..])
    }

    const RED: [u8; 4] = [255, 0, 0, 0];
    const GREEN: [u8; 4] = [0, 255, 0, 0];
    const BLUE: [u8; 4] = [0, 0, 255, 0];

    #[test]
    fn tiles_are_cut_to_the_rectangle_rows_first() {
        let all: Vec<(u16, u16, u16, u16)> = tiles(Rect {
            x: 2,
            y: 3,
            width: 20,
            height: 17,
        })
        .map(|tile| (tile.x, tile.y, tile.width, tile.height))
        .collect();
        assert_eq!(
            all,
            [
                (2, 3, 16, 16),
                (18, 3, 4, 16),
                (2, 19, 16, 1),
                (18, 19, 4, 1)
            ]
        );
    }

    /// Two tiles of 16 and 4 by 2: the first a background and two subrectangles, one in the
    /// foreground and one coloured; the second blank, so the background kept.
    #[test]
    fn subrectangles_paint_over_the_background_kept_from_tile_to_tile() {
        let mut screen = Screen::new(20, 2);
        let area = Rect {
            x: 0,
            y: 0,
            width: 20,
            height: 2,
        };
        let mut data = vec![BACKGROUND | FOREGROUND | ANY_SUBRECTS];
        data.extend_from_slice(&BLUE);
        data.extend_from_slice(&RED);
        data.extend_from_slice(&[1, 0x00, 0x10]); // one: at 0,0, 2 by 1, in the foreground
        // Second tile: blank.
        data.push(0);
        let mut decoder = Hextile::new();
        for cut in 0..data.len() {
            assert_eq!(
                decoder.decode(&data[..cut], area, &mut screen),
                Ok(None),
                "cut at {cut}"
            );
        }
        assert_eq!(screen, Screen::new(20, 2));
        assert_eq!(
            decoder.decode(&data, area, &mut screen),
            Ok(Some(data.len()))
        );
        assert_eq!(rgb_at(&screen, 0, 0), [255, 0, 0]);
        assert_eq!(rgb_at(&screen, 1, 0), [255, 0, 0]);
        assert_eq!(rgb_at(&screen, 2, 0), [0, 0, 255]);
        assert_eq!(rgb_at(&screen, 0, 1), [0, 0, 255]);
        assert_eq!(rgb_at(&screen, 19, 1), [0, 0, 255], "blank: the background");

        // Coloured subrectangles, the background kept from the last rectangle.
        let area = Rect {
            x: 16,
            y: 0,
            width: 4,
            height: 2,
        };
        let mut data = vec![ANY_SUBRECTS | SUBRECTS_COLOURED, 1];
        data.extend_from_slice(&GREEN);
        data.extend_from_slice(&[0x31, 0x00]); // at 3,1, 1 by 1
        assert_eq!(
            decoder.decode(&data, area, &mut screen),
            Ok(Some(data.len()))
        );
        assert_eq!(rgb_at(&screen, 19, 1), [0, 255, 0]);
        assert_eq!(rgb_at(&screen, 16, 0), [0, 0, 255]);
    }

    #[test]
    fn raw_tiles_copy_pixels_and_a_blank_after_one_is_left_alone() {
        let mut screen = Screen::new(2, 1);
        let area = Rect {
            x: 0,
            y: 0,
            width: 2,
            height: 1,
        };
        let mut decoder = Hextile::new();
        let mut data = vec![RAW];
        data.extend_from_slice(&RED);
        data.extend_from_slice(&GREEN);
        assert_eq!(
            decoder.decode(&data, area, &mut screen),
            Ok(Some(data.len()))
        );
        assert_eq!(rgb_at(&screen, 0, 0), [255, 0, 0]);
        assert_eq!(rgb_at(&screen, 1, 0), [0, 255, 0]);
        // Blank right after a raw tile: noVNC ignores it.
        assert_eq!(decoder.decode(&[0], area, &mut screen), Ok(Some(1)));
        assert_eq!(rgb_at(&screen, 1, 0), [0, 255, 0]);
        // A second blank fills with the background, black.
        assert_eq!(decoder.decode(&[0], area, &mut screen), Ok(Some(1)));
        assert_eq!(rgb_at(&screen, 1, 0), [0, 0, 0]);
    }

    #[test]
    fn illegal_flags_and_a_subrectangle_outside_its_tile_are_refused() {
        let mut screen = Screen::new(4, 4);
        let area = Rect {
            x: 0,
            y: 0,
            width: 4,
            height: 4,
        };
        let mut decoder = Hextile::new();
        assert!(decoder.decode(&[31], area, &mut screen).is_err());
        assert!(decoder.decode(&[0xff], area, &mut screen).is_err());
        // At 3,3, 2 by 1: past the 4 by 4 tile.
        let data = [ANY_SUBRECTS, 1, 0x33, 0x10];
        assert!(decoder.decode(&data, area, &mut screen).is_err());
    }
}
