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

//! ZRLE (RFC 6143 7.7.6): 64 by 64 tiles, each raw, solid, palette-packed or run-length
//! coded, through one zlib stream that lasts as long as the connection.
//!
//! Pixels arrive as 3-byte CPIXELs, red, green and blue: the format the client asks for is
//! 32 bits, little-endian, with its colours in the low three bytes.

use flate2::{Decompress, FlushDecompress, Status};

use super::screen::{Rect, Screen};

/// Side of a tile.
const TILE: u16 = 64;

/// Bytes of a compressed pixel.
const CPIXEL: usize = 3;

/// Decompressed bytes allowed per pixel of a rectangle: the worst honest coding, a run of
/// one for every pixel, takes 4.
const MAX_BYTES_PER_PIXEL: usize = 4;

/// Decompressed bytes allowed per tile beyond its pixels: a subencoding and a palette.
const MAX_TILE_OVERHEAD: usize = 1 + 127 * CPIXEL;

/// Output reserved at a time while inflating.
const INFLATE_STEP: usize = 1 << 20;

/// Subencodings.
const RAW: u8 = 0;
const SOLID: u8 = 1;
const PACKED_PALETTE_LAST: u8 = 16;
const PLAIN_RLE: u8 = 128;
const PALETTE_RLE_FIRST: u8 = 130;

/// The decoder of one connection.
pub(crate) struct Zrle {
    inflater: Decompress,
}

impl Zrle {
    pub(crate) fn new() -> Self {
        Self {
            inflater: Decompress::new(true),
        }
    }

    /// Draws `rect`, coded as `compressed`, onto `screen`. The caller checked that `rect` lies
    /// inside it.
    pub(crate) fn decode(
        &mut self,
        compressed: &[u8],
        rect: Rect,
        screen: &mut Screen,
    ) -> Result<(), String> {
        let tiles =
            usize::from(rect.width.div_ceil(TILE)) * usize::from(rect.height.div_ceil(TILE));
        let limit = rect.area() * MAX_BYTES_PER_PIXEL + tiles * MAX_TILE_OVERHEAD;
        let data = self.inflate(compressed, limit)?;
        let mut bytes = Bytes { data: &data, at: 0 };
        for tile_y in (rect.y..rect.y + rect.height).step_by(usize::from(TILE)) {
            for tile_x in (rect.x..rect.x + rect.width).step_by(usize::from(TILE)) {
                let tile = Rect {
                    x: tile_x,
                    y: tile_y,
                    width: TILE.min(rect.x + rect.width - tile_x),
                    height: TILE.min(rect.y + rect.height - tile_y),
                };
                decode_tile(&mut bytes, tile, screen)?;
            }
        }
        Ok(())
    }

    fn inflate(&mut self, compressed: &[u8], limit: usize) -> Result<Vec<u8>, String> {
        let start = self.inflater.total_in();
        let mut out = Vec::new();
        loop {
            let consumed = usize::try_from(self.inflater.total_in() - start)
                .map_err(|_| "compressed data too large".to_owned())?;
            if out.len() == out.capacity() {
                if out.len() >= limit {
                    return Err("a ZRLE rectangle inflates past its size".to_owned());
                }
                out.reserve(INFLATE_STEP.min(limit - out.len()));
            }
            let before = (consumed, out.len());
            let status = self
                .inflater
                .decompress_vec(&compressed[consumed..], &mut out, FlushDecompress::Sync)
                .map_err(|error| format!("ZRLE data does not inflate: {error}"))?;
            let consumed = usize::try_from(self.inflater.total_in() - start)
                .map_err(|_| "compressed data too large".to_owned())?;
            let filled = out.len() == out.capacity();
            if status == Status::StreamEnd || (consumed == compressed.len() && !filled) {
                return Ok(out);
            }
            if (consumed, out.len()) == before && !filled {
                return Err("ZRLE data stops inflating".to_owned());
            }
        }
    }
}

/// A cursor over the inflated bytes of a rectangle.
struct Bytes<'a> {
    data: &'a [u8],
    at: usize,
}

impl Bytes<'_> {
    fn take(&mut self, count: usize) -> Result<&[u8], String> {
        let end = self
            .at
            .checked_add(count)
            .filter(|end| *end <= self.data.len())
            .ok_or_else(|| "ZRLE data ends inside a tile".to_owned())?;
        let taken = &self.data[self.at..end];
        self.at = end;
        Ok(taken)
    }

    fn byte(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }

    fn pixel(&mut self) -> Result<[u8; 3], String> {
        let bytes = self.take(CPIXEL)?;
        Ok([bytes[0], bytes[1], bytes[2]])
    }

    fn palette(&mut self, size: usize) -> Result<Vec<[u8; 3]>, String> {
        (0..size).map(|_| self.pixel()).collect()
    }

    /// A run length: 1 plus bytes while they are 255, plus the last one.
    fn run_length(&mut self, room: usize) -> Result<usize, String> {
        let mut length = 1_usize;
        loop {
            let byte = self.byte()?;
            length += usize::from(byte);
            if length > room {
                return Err("a ZRLE run overflows its tile".to_owned());
            }
            if byte != u8::MAX {
                return Ok(length);
            }
        }
    }
}

/// Writes pixels one after the other through a tile, rows left to right, top to bottom.
struct Painter {
    tile: Rect,
    next: usize,
}

impl Painter {
    fn room(&self) -> usize {
        self.tile.area() - self.next
    }

    fn paint(&mut self, screen: &mut Screen, rgb: [u8; 3], count: usize) {
        let width = usize::from(self.tile.width);
        for index in self.next..self.next + count {
            // In the tile, so below 64 either way.
            let dx = u16::try_from(index % width).unwrap_or(0);
            let dy = u16::try_from(index / width).unwrap_or(0);
            screen.set(self.tile.x + dx, self.tile.y + dy, rgb);
        }
        self.next += count;
    }
}

fn decode_tile(bytes: &mut Bytes<'_>, tile: Rect, screen: &mut Screen) -> Result<(), String> {
    let subencoding = bytes.byte()?;
    let mut painter = Painter { tile, next: 0 };
    match subencoding {
        RAW => {
            for _ in 0..tile.area() {
                let rgb = bytes.pixel()?;
                painter.paint(screen, rgb, 1);
            }
        }
        SOLID => screen.fill(tile, bytes.pixel()?),
        2..=PACKED_PALETTE_LAST => {
            let palette = bytes.palette(usize::from(subencoding))?;
            let bits: usize = match subencoding {
                2 => 1,
                3..=4 => 2,
                _ => 4,
            };
            let row_bytes = (usize::from(tile.width) * bits).div_ceil(8);
            for _ in 0..tile.height {
                let row = bytes.take(row_bytes)?;
                for column in 0..usize::from(tile.width) {
                    let bit = column * bits;
                    let shift = 8 - bits - bit % 8;
                    let index = usize::from(row[bit / 8] >> shift) & ((1 << bits) - 1);
                    let rgb = *palette
                        .get(index)
                        .ok_or_else(|| "a ZRLE index outside its palette".to_owned())?;
                    painter.paint(screen, rgb, 1);
                }
            }
        }
        PLAIN_RLE => {
            while painter.room() > 0 {
                let rgb = bytes.pixel()?;
                let length = bytes.run_length(painter.room())?;
                painter.paint(screen, rgb, length);
            }
        }
        PALETTE_RLE_FIRST..=u8::MAX => {
            let palette = bytes.palette(usize::from(subencoding - PLAIN_RLE))?;
            while painter.room() > 0 {
                let entry = bytes.byte()?;
                let rgb = *palette
                    .get(usize::from(entry & 0x7f))
                    .ok_or_else(|| "a ZRLE index outside its palette".to_owned())?;
                let length = if entry & 0x80 == 0 {
                    1
                } else {
                    bytes.run_length(painter.room())?
                };
                painter.paint(screen, rgb, length);
            }
        }
        other => return Err(format!("unknown ZRLE subencoding {other}")),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use flate2::{Compress, Compression, FlushCompress};

    use super::*;

    const RED: [u8; 3] = [255, 0, 0];
    const GREEN: [u8; 3] = [0, 255, 0];
    const BLUE: [u8; 3] = [0, 0, 255];

    /// A zlib stream fed rectangle after rectangle, as a server keeps one.
    struct Server(Compress);

    impl Server {
        fn new() -> Self {
            Self(Compress::new(Compression::default(), true))
        }

        fn rect(&mut self, tiles: &[u8]) -> Vec<u8> {
            let mut out = Vec::with_capacity(tiles.len() + 64);
            self.0
                .compress_vec(tiles, &mut out, FlushCompress::Sync)
                .expect("compress");
            out
        }
    }

    fn rect(width: u16, height: u16) -> Rect {
        Rect {
            x: 0,
            y: 0,
            width,
            height,
        }
    }

    /// The RGB of every pixel, rows top to bottom.
    fn colours(screen: &Screen) -> Vec<[u8; 3]> {
        screen
            .pixels()
            .as_chunks::<4>()
            .0
            .iter()
            .map(|pixel| [pixel[0], pixel[1], pixel[2]])
            .collect()
    }

    fn decode(tiles: &[u8], area: Rect, screen: &mut Screen) -> Result<(), String> {
        Zrle::new().decode(&Server::new().rect(tiles), area, screen)
    }

    #[test]
    fn a_solid_tile_fills_it() {
        let mut screen = Screen::new(3, 2);
        decode(&[SOLID, 0, 255, 0], rect(3, 2), &mut screen).expect("decoded");
        assert_eq!(colours(&screen), [GREEN; 6]);
    }

    #[test]
    fn a_raw_tile_takes_three_bytes_a_pixel() {
        let mut screen = Screen::new(2, 1);
        decode(&[RAW, 255, 0, 0, 0, 0, 255], rect(2, 1), &mut screen).expect("decoded");
        assert_eq!(colours(&screen), [RED, BLUE]);
    }

    #[test]
    fn a_two_colour_palette_packs_a_bit_a_pixel_and_pads_each_row() {
        // 3 by 2: rows "red blue red" and "blue blue red", one byte each, high bits first:
        // 010_00000 and 110_00000.
        let mut screen = Screen::new(3, 2);
        let tiles = [2, 255, 0, 0, 0, 0, 255, 0b0100_0000, 0b1100_0000];
        decode(&tiles, rect(3, 2), &mut screen).expect("decoded");
        assert_eq!(colours(&screen), [RED, BLUE, RED, BLUE, BLUE, RED]);
    }

    #[test]
    fn a_plain_run_crosses_rows_and_a_255_byte_continues_its_length() {
        // 20 by 20 = 400 pixels: a run of 300 red (1 + 255 + 44), then 100 green (1 + 99).
        let mut screen = Screen::new(20, 20);
        let tiles = [PLAIN_RLE, 255, 0, 0, 255, 44, 0, 255, 0, 99];
        decode(&tiles, rect(20, 20), &mut screen).expect("decoded");
        let expected: Vec<[u8; 3]> = std::iter::repeat_n(RED, 300)
            .chain(std::iter::repeat_n(GREEN, 100))
            .collect();
        assert_eq!(colours(&screen), expected);
    }

    #[test]
    fn a_palette_run_mixes_single_pixels_and_runs() {
        // 5 by 1, palette red, green: green, then red run of 3, then green.
        let mut screen = Screen::new(5, 1);
        let tiles = [130, 255, 0, 0, 0, 255, 0, 1, 0x80, 2, 1];
        decode(&tiles, rect(5, 1), &mut screen).expect("decoded");
        assert_eq!(colours(&screen), [GREEN, RED, RED, RED, GREEN]);
    }

    #[test]
    fn a_rectangle_wider_than_a_tile_is_coded_tile_by_tile() {
        // 65 by 1: a 64-wide red tile, then a 1-wide blue one.
        let mut screen = Screen::new(65, 1);
        decode(
            &[SOLID, 255, 0, 0, SOLID, 0, 0, 255],
            rect(65, 1),
            &mut screen,
        )
        .expect("decoded");
        let colours = colours(&screen);
        assert!(colours[..64].iter().all(|pixel| *pixel == RED));
        assert_eq!(colours[64], BLUE);
    }

    #[test]
    fn rectangles_share_one_zlib_stream() {
        let mut server = Server::new();
        let mut client = Zrle::new();
        let mut screen = Screen::new(2, 1);
        let left = Rect {
            x: 0,
            y: 0,
            width: 1,
            height: 1,
        };
        let right = Rect { x: 1, ..left };
        client
            .decode(&server.rect(&[SOLID, 255, 0, 0]), left, &mut screen)
            .expect("first");
        // The second rectangle continues the stream: a fresh inflater would not read it.
        client
            .decode(&server.rect(&[SOLID, 0, 0, 255]), right, &mut screen)
            .expect("second");
        assert_eq!(colours(&screen), [RED, BLUE]);
    }

    #[test]
    fn broken_tiles_are_refused() {
        let mut screen = Screen::new(4, 1);
        let area = rect(4, 1);
        // A run longer than the tile.
        assert!(decode(&[PLAIN_RLE, 255, 0, 0, 10], area, &mut screen).is_err());
        // A palette index past the palette.
        assert!(decode(&[130, 255, 0, 0, 0, 5, 1, 2, 3], area, &mut screen).is_err());
        // Data that stops in the middle of the tile.
        assert!(decode(&[RAW, 1, 2, 3], area, &mut screen).is_err());
        // Subencodings the specification leaves unused.
        assert!(decode(&[17], area, &mut screen).is_err());
        assert!(decode(&[129], area, &mut screen).is_err());
    }

    #[test]
    fn a_rectangle_inflating_past_its_size_is_refused() {
        // A 1 by 1 rectangle whose data inflates to a megabyte of zeros.
        let mut screen = Screen::new(1, 1);
        let bomb = vec![0; 1 << 20];
        assert!(decode(&bomb, rect(1, 1), &mut screen).is_err());
    }
}
