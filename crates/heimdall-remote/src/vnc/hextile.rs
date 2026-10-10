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
//! leaves the colours kept as they were. The measure goes on from the last whole tile at the
//! next read, never from the first again, and stops at `limit` bytes; each tile's
//! subrectangles are checked inside it, and together never fill more than
//! [`MAX_FILL_FACTOR`] times the rectangle's pixels.

use super::rre::MAX_FILL_FACTOR;
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

/// The decoder of one connection: the colours a tile leaves to the next, and its flags; how
/// far the rectangle waited for is measured.
pub(crate) struct Hextile {
    background: [u8; 3],
    foreground: [u8; 3],
    last_flags: u8,
    measured: Option<Measured>,
    /// Tiles looked at by the measure, whole or not: its work, for the tests.
    #[cfg(test)]
    tiles_looked_at: usize,
}

/// A rectangle measured up to a tile: the tiles before it whole, their bytes and the pixels
/// their subrectangles fill.
#[derive(Clone, Copy)]
struct Measured {
    rect: Rect,
    tile: usize,
    bytes: usize,
    filled: usize,
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
            measured: None,
            #[cfg(test)]
            tiles_looked_at: 0,
        }
    }

    /// Draws `rect`, coded at the start of `data`, onto `screen`: the bytes it took, or
    /// `None` when more are needed, nothing changed. A rectangle past `limit` bytes is
    /// refused as soon as its measure passes it. The caller checked that `rect` lies inside
    /// `screen`, and gives the same rectangle again, its bytes from the same start, until it
    /// is drawn.
    pub(crate) fn decode(
        &mut self,
        data: &[u8],
        rect: Rect,
        screen: &mut Screen,
        limit: usize,
    ) -> Result<Option<usize>, String> {
        let Some(length) = self.measure(data, rect, limit)? else {
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
    (0..tile_count(rect)).map(move |index| tile_at(rect, index))
}

/// Tiles across `rect`.
fn tile_columns(rect: Rect) -> usize {
    usize::from(rect.width.div_ceil(TILE))
}

/// Tiles of `rect` in all.
fn tile_count(rect: Rect) -> usize {
    tile_columns(rect) * usize::from(rect.height.div_ceil(TILE))
}

/// The tile of `rect` at `index`, counted rows first; `index` is below [`tile_count`].
fn tile_at(rect: Rect, index: usize) -> Tile {
    let columns = tile_columns(rect).max(1);
    // Both fit: a rectangle of 16-bit sides is at most 4096 tiles across and down.
    let dx = u16::try_from(index % columns).unwrap_or(0) * TILE;
    let dy = u16::try_from(index / columns).unwrap_or(0) * TILE;
    Tile {
        x: rect.x + dx,
        y: rect.y + dy,
        width: TILE.min(rect.width - dx),
        height: TILE.min(rect.height - dy),
    }
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

impl Hextile {
    /// The bytes of the whole rectangle at the start of `data`, its flags and subrectangles
    /// checked; `None` when more are needed, the tiles whole so far kept for the next call.
    fn measure(&mut self, data: &[u8], rect: Rect, limit: usize) -> Result<Option<usize>, String> {
        let mut measured = match self.measured.take() {
            Some(measured) if measured.rect == rect && measured.bytes <= data.len() => measured,
            _ => Measured {
                rect,
                tile: 0,
                bytes: 0,
                filled: 0,
            },
        };
        let budget = rect.area().saturating_mul(MAX_FILL_FACTOR);
        while measured.tile < tile_count(rect) {
            #[cfg(test)]
            {
                self.tiles_looked_at += 1;
            }
            let tile = tile_at(rect, measured.tile);
            let Some((bytes, filled)) = measure_tile(&data[measured.bytes..], tile)? else {
                self.measured = Some(measured);
                return Ok(None);
            };
            measured.bytes += bytes;
            measured.filled += filled;
            if measured.bytes > limit {
                return Err(format!("a Hextile rectangle past {limit} bytes"));
            }
            if measured.filled > budget {
                return Err(format!(
                    "Hextile subrectangles filling past {MAX_FILL_FACTOR} times their {}x{} \
                     rectangle",
                    rect.width, rect.height
                ));
            }
            measured.tile += 1;
        }
        Ok(Some(measured.bytes))
    }
}

/// The bytes of `tile` at the start of `data`, and the pixels its subrectangles fill, its
/// flags and subrectangles checked; `None` when more are needed.
fn measure_tile(data: &[u8], tile: Tile) -> Result<Option<(usize, usize)>, String> {
    let Some(&flags) = data.first() else {
        return Ok(None);
    };
    if flags > MAX_FLAGS {
        return Err(format!("an illegal Hextile tile of flags {flags}"));
    }
    let mut at = 1;
    if flags & RAW != 0 {
        at += tile_area(tile) * PIXEL_BYTES;
        return Ok((at <= data.len()).then_some((at, 0)));
    }
    if flags & BACKGROUND != 0 {
        at += PIXEL_BYTES;
    }
    if flags & FOREGROUND != 0 {
        at += PIXEL_BYTES;
    }
    if flags & ANY_SUBRECTS == 0 {
        return Ok((at <= data.len()).then_some((at, 0)));
    }
    let Some(&count) = data.get(at) else {
        return Ok(None);
    };
    at += 1;
    let colour = if flags & SUBRECTS_COLOURED != 0 {
        PIXEL_BYTES
    } else {
        0
    };
    let each = colour + SUBRECT_BYTES;
    let Some(subrects) = data.get(at..at + usize::from(count) * each) else {
        return Ok(None);
    };
    let mut filled = 0;
    for bytes in subrects.chunks_exact(each) {
        filled += subrect(tile, bytes[colour], bytes[colour + 1])?.area();
    }
    Ok(Some((at + subrects.len(), filled)))
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIMIT: usize = 1 << 20;

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
                decoder.decode(&data[..cut], area, &mut screen, LIMIT),
                Ok(None),
                "cut at {cut}"
            );
        }
        assert_eq!(screen, Screen::new(20, 2));
        assert_eq!(
            decoder.decode(&data, area, &mut screen, LIMIT),
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
            decoder.decode(&data, area, &mut screen, LIMIT),
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
            decoder.decode(&data, area, &mut screen, LIMIT),
            Ok(Some(data.len()))
        );
        assert_eq!(rgb_at(&screen, 0, 0), [255, 0, 0]);
        assert_eq!(rgb_at(&screen, 1, 0), [0, 255, 0]);
        // Blank right after a raw tile: noVNC ignores it.
        assert_eq!(decoder.decode(&[0], area, &mut screen, LIMIT), Ok(Some(1)));
        assert_eq!(rgb_at(&screen, 1, 0), [0, 255, 0]);
        // A second blank fills with the background, black.
        assert_eq!(decoder.decode(&[0], area, &mut screen, LIMIT), Ok(Some(1)));
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
        assert!(decoder.decode(&[31], area, &mut screen, LIMIT).is_err());
        assert!(decoder.decode(&[0xff], area, &mut screen, LIMIT).is_err());
        // At 3,3, 2 by 1: past the 4 by 4 tile.
        let data = [ANY_SUBRECTS, 1, 0x33, 0x10];
        assert!(decoder.decode(&data, area, &mut screen, LIMIT).is_err());
    }

    /// A rectangle of `columns` by `rows` tiles of 16, each a blue background and
    /// `subrects` foreground subrectangles of one pixel, at 0,0.
    fn tiles_of_subrects(columns: u16, rows: u16, subrects: u8) -> (Rect, Vec<u8>) {
        let area = Rect {
            x: 0,
            y: 0,
            width: columns * TILE,
            height: rows * TILE,
        };
        let mut data = Vec::new();
        for _ in 0..usize::from(columns) * usize::from(rows) {
            data.push(BACKGROUND | ANY_SUBRECTS);
            data.extend_from_slice(&BLUE);
            data.push(subrects);
            for _ in 0..subrects {
                data.extend_from_slice(&[0x00, 0x00]);
            }
        }
        (area, data)
    }

    #[test]
    fn a_rectangle_fed_byte_by_byte_is_measured_once_tile_by_tile() {
        let (area, data) = tiles_of_subrects(4, 3, 2);
        let mut whole = Screen::new(64, 48);
        assert_eq!(
            Hextile::new().decode(&data, area, &mut whole, LIMIT),
            Ok(Some(data.len()))
        );
        let mut screen = Screen::new(64, 48);
        let mut decoder = Hextile::new();
        for cut in 0..data.len() {
            assert_eq!(
                decoder.decode(&data[..cut], area, &mut screen, LIMIT),
                Ok(None),
                "cut at {cut}"
            );
        }
        assert_eq!(
            decoder.decode(&data, area, &mut screen, LIMIT),
            Ok(Some(data.len()))
        );
        assert_eq!(screen, whole);
        // Each call looks at the tiles that became whole, and one more at most: never every
        // tile from the first again.
        let calls = data.len() + 1;
        assert!(
            decoder.tiles_looked_at <= tile_count(area) + calls,
            "{} tiles looked at in {calls} calls",
            decoder.tiles_looked_at
        );
        // A rectangle drawn leaves the next one measured afresh.
        assert_eq!(
            decoder.decode(&data, area, &mut screen, LIMIT),
            Ok(Some(data.len()))
        );
    }

    #[test]
    fn a_rectangle_past_its_byte_limit_is_refused_once_measured_past_it() {
        let (area, data) = tiles_of_subrects(4, 4, 1);
        let mut screen = Screen::new(64, 64);
        let tile_bytes = data.len() / 16;
        let limit = 3 * tile_bytes;
        // Three tiles fit; the fourth passes the limit, refused though more are to come.
        assert_eq!(
            Hextile::new().decode(&data[..limit], area, &mut screen, limit),
            Ok(None)
        );
        assert!(
            Hextile::new()
                .decode(&data[..4 * tile_bytes], area, &mut screen, limit)
                .is_err()
        );
        assert_eq!(screen, Screen::new(64, 64), "nothing drawn");
    }

    #[test]
    fn subrectangles_filling_past_the_budget_are_refused() {
        // One 16 by 16 tile of subrectangles each the whole tile: four fill four times the
        // rectangle, five are past it.
        let area = Rect {
            x: 0,
            y: 0,
            width: 16,
            height: 16,
        };
        let tile = |count: u8| {
            let mut data = vec![ANY_SUBRECTS, count];
            for _ in 0..count {
                data.extend_from_slice(&[0x00, 0xff]);
            }
            data
        };
        let mut screen = Screen::new(16, 16);
        let four = tile(4);
        assert_eq!(
            Hextile::new().decode(&four, area, &mut screen, LIMIT),
            Ok(Some(four.len()))
        );
        let mut screen = Screen::new(16, 16);
        assert!(
            Hextile::new()
                .decode(&tile(5), area, &mut screen, LIMIT)
                .is_err()
        );
        assert_eq!(screen, Screen::new(16, 16), "nothing drawn");
    }
}
