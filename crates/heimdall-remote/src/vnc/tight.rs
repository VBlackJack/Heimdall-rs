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

//! Tight (encoding 7), read as noVNC reads it: a control byte, then the rectangle filled
//! with one colour, a JPEG image, or its pixels copied, from a palette or predicted by
//! gradient, through one of four zlib streams that last as long as the connection.
//!
//! Pixels arrive as 3-byte TPIXELs, red, green and blue: the format the client asks for is
//! 32 bits, depth 24, true colour, 255 a colour. `TightPNG` is not asked for, nor read.
//!
//! A rectangle carries no length of its own: it is read whole before anything is decoded,
//! so one cut between two reads leaves the streams as they were.

use flate2::{Decompress, FlushDecompress, Status};
use zune_jpeg::JpegDecoder;
use zune_jpeg::zune_core::bytestream::ZCursor;
use zune_jpeg::zune_core::colorspace::ColorSpace;
use zune_jpeg::zune_core::options::DecoderOptions;

use super::screen::{Rect, Screen};

/// Bytes of a Tight pixel.
const TPIXEL: usize = 3;

/// Zlib streams of a connection.
const STREAMS: usize = 4;

/// The control byte's low half: the streams reset before this rectangle, one bit each.
const RESET_BITS: u8 = 0x0f;

/// The control byte's high half says how the rectangle is coded.
const KIND_SHIFT: u8 = 4;

/// Kinds: one colour, a JPEG image, a PNG one.
const FILL: u8 = 0x08;
const JPEG: u8 = 0x09;
const PNG: u8 = 0x0a;

/// Set in every kind but basic compression.
const NOT_BASIC: u8 = 0x08;

/// In basic compression: a filter byte follows.
const EXPLICIT_FILTER: u8 = 0x04;

/// In basic compression: the stream the data goes through.
const STREAM_BITS: u8 = 0x03;

/// Filters.
const FILTER_COPY: u8 = 0;
const FILTER_PALETTE: u8 = 1;
const FILTER_GRADIENT: u8 = 2;

/// Most colours a palette of one bit a pixel holds.
const MONO_COLOURS: usize = 2;

/// Bits of a byte, for rows of one bit a pixel.
const BYTE_BITS: usize = 8;

/// Data shorter than this comes as is, not through zlib.
const MIN_COMPRESSED: usize = 12;

/// A compact length: 7 bits in each of its first two bytes, 8 in the third.
const LENGTH_BYTES: usize = 3;
const LENGTH_SHIFT: usize = 7;
const LENGTH_LOW: u8 = 0x7f;
const LENGTH_MORE: u8 = 0x80;

/// Compressed data allowed, as a multiple of what it inflates to, plus room for zlib's header,
/// checksum and flush markers: past that it is not compression.
const ZLIB_EXPANSION: usize = 2;
const ZLIB_SLACK: usize = 1024;

/// JPEG data allowed per pixel of its rectangle, plus room for its headers and tables.
const JPEG_BYTES_PER_PIXEL: usize = 2 * TPIXEL;
const JPEG_SLACK: usize = 64 * 1024;

/// The decoder of one connection.
pub(crate) struct Tight {
    streams: [Decompress; STREAMS],
}

/// A rectangle read whole, not yet decoded.
struct Parsed<'a> {
    /// The streams to reset first, one bit each.
    resets: u8,
    coding: Coding<'a>,
    /// Bytes the rectangle took.
    length: usize,
}

/// How a rectangle is coded.
enum Coding<'a> {
    /// One colour.
    Fill([u8; TPIXEL]),
    /// A JPEG image.
    Jpeg(&'a [u8]),
    /// Pixels through a filter, compressed through a stream unless short.
    Basic {
        /// The stream it goes through, when compressed.
        stream: usize,
        /// The filter.
        filter: Filter<'a>,
        /// The data.
        data: Data<'a>,
    },
}

/// What the pixels of basic compression went through.
#[derive(Clone, Copy)]
enum Filter<'a> {
    /// Nothing: TPIXELs.
    Copy,
    /// Indexes into these TPIXELs.
    Palette(&'a [u8]),
    /// Differences from a prediction of each colour.
    Gradient,
}

/// The data of basic compression.
enum Data<'a> {
    /// As is: it was short.
    Plain(&'a [u8]),
    /// Through zlib.
    Zlib(&'a [u8]),
}

impl Tight {
    /// A decoder, its four streams fresh.
    pub(crate) fn new() -> Self {
        Self {
            streams: std::array::from_fn(|_| Decompress::new(true)),
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
        let Some(parsed) = parse(data, rect)? else {
            return Ok(None);
        };
        for (index, stream) in self.streams.iter_mut().enumerate() {
            if (parsed.resets >> index) & 1 != 0 {
                stream.reset(true);
            }
        }
        match parsed.coding {
            Coding::Fill(rgb) => screen.fill(rect, rgb),
            Coding::Jpeg(image) => jpeg(image, rect, screen)?,
            Coding::Basic {
                stream,
                filter,
                data,
            } => {
                let size = filtered_size(filter, rect);
                let inflated;
                let bytes = match data {
                    Data::Plain(bytes) => bytes,
                    Data::Zlib(compressed) => {
                        inflated = inflate(&mut self.streams[stream], compressed, size)?;
                        inflated.as_slice()
                    }
                };
                match filter {
                    Filter::Copy => paint(screen, rect, bytes),
                    Filter::Palette(colours) => palette(bytes, colours, rect, screen)?,
                    Filter::Gradient => paint(screen, rect, &gradient(bytes, rect)),
                }
            }
        }
        Ok(Some(parsed.length))
    }
}

/// A cursor over the bytes of a rectangle; `None` when more are needed.
struct Bytes<'a> {
    data: &'a [u8],
    at: usize,
}

impl<'a> Bytes<'a> {
    fn take(&mut self, count: usize) -> Option<&'a [u8]> {
        let end = self.at.checked_add(count)?;
        let taken = self.data.get(self.at..end)?;
        self.at = end;
        Some(taken)
    }

    fn byte(&mut self) -> Option<u8> {
        Some(self.take(1)?[0])
    }

    fn pixel(&mut self) -> Option<[u8; TPIXEL]> {
        let bytes = self.take(TPIXEL)?;
        Some([bytes[0], bytes[1], bytes[2]])
    }

    /// A compact length: 1 to 3 bytes, low bits first, the top bit of the first two saying
    /// another byte follows.
    fn compact_length(&mut self) -> Option<usize> {
        let mut length = 0;
        for index in 0..LENGTH_BYTES {
            let byte = self.byte()?;
            let last = index + 1 == LENGTH_BYTES;
            let bits = if last { byte } else { byte & LENGTH_LOW };
            length |= usize::from(bits) << (LENGTH_SHIFT * index);
            if last || byte & LENGTH_MORE == 0 {
                break;
            }
        }
        Some(length)
    }
}

/// Reads a rectangle whole; `None` when more bytes are needed.
fn parse(data: &[u8], rect: Rect) -> Result<Option<Parsed<'_>>, String> {
    let mut bytes = Bytes { data, at: 0 };
    let Some(control) = bytes.byte() else {
        return Ok(None);
    };
    let kind = control >> KIND_SHIFT;
    let coding = match kind {
        FILL => bytes.pixel().map(Coding::Fill),
        JPEG => {
            let Some(length) = bytes.compact_length() else {
                return Ok(None);
            };
            let limit = rect.area() * JPEG_BYTES_PER_PIXEL + JPEG_SLACK;
            check_length(length, limit, "a Tight JPEG")?;
            bytes.take(length).map(Coding::Jpeg)
        }
        PNG => return Err("a TightPNG rectangle, which was not asked for".to_owned()),
        _ if kind & NOT_BASIC == 0 => parse_basic(&mut bytes, kind, rect)?,
        other => return Err(format!("an unknown Tight compression {other}")),
    };
    Ok(coding.map(|coding| Parsed {
        resets: control & RESET_BITS,
        coding,
        length: bytes.at,
    }))
}

/// Reads basic compression: the filter, its palette, then the data.
fn parse_basic<'a>(
    bytes: &mut Bytes<'a>,
    kind: u8,
    rect: Rect,
) -> Result<Option<Coding<'a>>, String> {
    let filter = if kind & EXPLICIT_FILTER == 0 {
        FILTER_COPY
    } else {
        let Some(filter) = bytes.byte() else {
            return Ok(None);
        };
        filter
    };
    let filter = match filter {
        FILTER_COPY => Filter::Copy,
        FILTER_PALETTE => {
            let Some(last) = bytes.byte() else {
                return Ok(None);
            };
            let Some(colours) = bytes.take((usize::from(last) + 1) * TPIXEL) else {
                return Ok(None);
            };
            Filter::Palette(colours)
        }
        FILTER_GRADIENT => Filter::Gradient,
        other => return Err(format!("an unknown Tight filter {other}")),
    };
    let size = filtered_size(filter, rect);
    let data = if size < MIN_COMPRESSED {
        bytes.take(size).map(Data::Plain)
    } else {
        let Some(length) = bytes.compact_length() else {
            return Ok(None);
        };
        let limit = size.saturating_mul(ZLIB_EXPANSION) + ZLIB_SLACK;
        check_length(length, limit, "Tight zlib data")?;
        bytes.take(length).map(Data::Zlib)
    };
    Ok(data.map(|data| Coding::Basic {
        stream: usize::from(kind & STREAM_BITS),
        filter,
        data,
    }))
}

/// Refuses a length of nothing, or past `limit`: no honest data is either.
fn check_length(length: usize, limit: usize, what: &str) -> Result<(), String> {
    if length == 0 || length > limit {
        return Err(format!("{what} of {length} bytes"));
    }
    Ok(())
}

/// Bytes the pixels of `rect` take once through `filter`: rows of one bit a pixel, padded
/// to bytes, for two colours; a byte a pixel for more; a TPIXEL otherwise.
fn filtered_size(filter: Filter<'_>, rect: Rect) -> usize {
    match filter {
        Filter::Palette(colours) if colours.len() <= MONO_COLOURS * TPIXEL => {
            usize::from(rect.width).div_ceil(BYTE_BITS) * usize::from(rect.height)
        }
        Filter::Palette(_) => rect.area(),
        Filter::Copy | Filter::Gradient => rect.area() * TPIXEL,
    }
}

/// Inflates `compressed` through `stream` to exactly `size` bytes.
fn inflate(stream: &mut Decompress, compressed: &[u8], size: usize) -> Result<Vec<u8>, String> {
    let start = stream.total_in();
    let consumed = |stream: &Decompress| {
        usize::try_from(stream.total_in() - start)
            .map_err(|_| "compressed data too large".to_owned())
    };
    // One byte past the size: data inflating further is caught, not left in the stream.
    let mut out = Vec::with_capacity(size + 1);
    loop {
        let before = (consumed(stream)?, out.len());
        let status = stream
            .decompress_vec(&compressed[before.0..], &mut out, FlushDecompress::Sync)
            .map_err(|error| format!("Tight data does not inflate: {error}"))?;
        let after = (consumed(stream)?, out.len());
        if out.len() > size {
            return Err("Tight data inflates past its rectangle".to_owned());
        }
        if status == Status::StreamEnd || after.0 == compressed.len() {
            break;
        }
        if after == before {
            return Err("Tight data stops inflating".to_owned());
        }
    }
    if out.len() != size {
        return Err("Tight data inflates short of its rectangle".to_owned());
    }
    Ok(out)
}

/// Draws TPIXELs through `rect`, rows left to right, top to bottom.
fn paint(screen: &mut Screen, rect: Rect, pixels: &[u8]) {
    let mut pixels = pixels.as_chunks::<TPIXEL>().0.iter();
    for y in rect.y..rect.y + rect.height {
        for x in rect.x..rect.x + rect.width {
            if let Some(pixel) = pixels.next() {
                screen.set(x, y, *pixel);
            }
        }
    }
}

/// Draws palette indexes through `rect`: rows of one bit a pixel, high bits first and each
/// padded to a byte, for two colours or fewer; a byte a pixel for more.
fn palette(bytes: &[u8], colours: &[u8], rect: Rect, screen: &mut Screen) -> Result<(), String> {
    let colours = colours.as_chunks::<TPIXEL>().0;
    let mono = colours.len() <= MONO_COLOURS;
    let width = usize::from(rect.width);
    let row_bytes = if mono {
        width.div_ceil(BYTE_BITS)
    } else {
        width
    };
    for (dy, row) in (0..rect.height).zip(bytes.chunks_exact(row_bytes.max(1))) {
        for (dx, column) in (0..rect.width).zip(0_usize..) {
            let index = if mono {
                usize::from((row[column / BYTE_BITS] >> (BYTE_BITS - 1 - column % BYTE_BITS)) & 1)
            } else {
                usize::from(row[column])
            };
            let rgb = colours
                .get(index)
                .ok_or_else(|| "a Tight index outside its palette".to_owned())?;
            screen.set(rect.x + dx, rect.y + dy, *rgb);
        }
    }
    Ok(())
}

/// Undoes the gradient filter: each colour is its difference, modulo 256, from left plus
/// above minus above-left, kept within 0 to 255; outside the rectangle counts as 0.
fn gradient(differences: &[u8], rect: Rect) -> Vec<u8> {
    let row = usize::from(rect.width) * TPIXEL;
    let mut pixels = vec![0; differences.len()];
    for (at, difference) in differences.iter().enumerate() {
        let has_left = at % row >= TPIXEL;
        let has_above = at >= row;
        let left = if has_left { pixels[at - TPIXEL] } else { 0 };
        let above = if has_above { pixels[at - row] } else { 0 };
        let above_left = if has_left && has_above {
            pixels[at - row - TPIXEL]
        } else {
            0
        };
        let predicted = (i16::from(left) + i16::from(above) - i16::from(above_left))
            .clamp(0, i16::from(u8::MAX));
        pixels[at] = difference.wrapping_add(u8::try_from(predicted).unwrap_or(u8::MAX));
    }
    pixels
}

/// Draws a JPEG image the size of `rect`; one of another size is refused, a larger one before
/// it is decoded.
fn jpeg(image: &[u8], rect: Rect, screen: &mut Screen) -> Result<(), String> {
    let size = (usize::from(rect.width), usize::from(rect.height));
    let options = DecoderOptions::default()
        .jpeg_set_out_colorspace(ColorSpace::RGB)
        .set_max_width(size.0)
        .set_max_height(size.1);
    let mut decoder = JpegDecoder::new_with_options(ZCursor::new(image), options);
    let fault = |error| format!("a Tight JPEG does not decode: {error}");
    decoder.decode_headers().map_err(fault)?;
    if decoder.dimensions() != Some(size) {
        return Err(format!(
            "a Tight JPEG of {:?} for a {}x{} rectangle",
            decoder.dimensions(),
            size.0,
            size.1
        ));
    }
    let pixels = decoder.decode().map_err(fault)?;
    if pixels.len() != rect.area() * TPIXEL {
        return Err("a Tight JPEG decodes short of its rectangle".to_owned());
    }
    paint(screen, rect, &pixels);
    Ok(())
}

#[cfg(test)]
mod tests {
    use flate2::{Compress, Compression, FlushCompress};

    use super::*;

    const RED: [u8; 3] = [255, 0, 0];
    const GREEN: [u8; 3] = [0, 255, 0];
    const BLUE: [u8; 3] = [0, 0, 255];

    /// A 16 by 8 JPEG, its left half red and its right half blue: written once by Pillow
    /// 11.2 (`Image.new("RGB", (16, 8))`, painted, then saved with quality 90, subsampling 0
    /// and optimize), as a server's encoder would.
    const RED_BLUE_JPEG: [u8; 291] = [
        0xff, 0xd8, 0xff, 0xe0, 0x00, 0x10, 0x4a, 0x46, 0x49, 0x46, 0x00, 0x01, 0x01, 0x00, 0x00,
        0x01, 0x00, 0x01, 0x00, 0x00, 0xff, 0xdb, 0x00, 0x43, 0x00, 0x03, 0x02, 0x02, 0x03, 0x02,
        0x02, 0x03, 0x03, 0x03, 0x03, 0x04, 0x03, 0x03, 0x04, 0x05, 0x08, 0x05, 0x05, 0x04, 0x04,
        0x05, 0x0a, 0x07, 0x07, 0x06, 0x08, 0x0c, 0x0a, 0x0c, 0x0c, 0x0b, 0x0a, 0x0b, 0x0b, 0x0d,
        0x0e, 0x12, 0x10, 0x0d, 0x0e, 0x11, 0x0e, 0x0b, 0x0b, 0x10, 0x16, 0x10, 0x11, 0x13, 0x14,
        0x15, 0x15, 0x15, 0x0c, 0x0f, 0x17, 0x18, 0x16, 0x14, 0x18, 0x12, 0x14, 0x15, 0x14, 0xff,
        0xdb, 0x00, 0x43, 0x01, 0x03, 0x04, 0x04, 0x05, 0x04, 0x05, 0x09, 0x05, 0x05, 0x09, 0x14,
        0x0d, 0x0b, 0x0d, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14,
        0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14,
        0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14,
        0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0xff, 0xc0, 0x00, 0x11, 0x08, 0x00, 0x08,
        0x00, 0x10, 0x03, 0x01, 0x11, 0x00, 0x02, 0x11, 0x01, 0x03, 0x11, 0x01, 0xff, 0xc4, 0x00,
        0x15, 0x00, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x07, 0x08, 0xff, 0xc4, 0x00, 0x14, 0x10, 0x01, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xff, 0xc4, 0x00,
        0x15, 0x01, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x09, 0x07, 0xff, 0xc4, 0x00, 0x14, 0x11, 0x01, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xff, 0xda, 0x00,
        0x0c, 0x03, 0x01, 0x00, 0x02, 0x11, 0x03, 0x11, 0x00, 0x3f, 0x00, 0x9d, 0x10, 0xc2, 0xa6,
        0x02, 0x38, 0xc1, 0xd1, 0xff, 0xd9,
    ];

    /// The four zlib streams of a server, each fed rectangle after rectangle.
    struct Server([Compress; STREAMS]);

    impl Server {
        fn new() -> Self {
            Self(std::array::from_fn(|_| {
                Compress::new(Compression::default(), true)
            }))
        }

        /// `data` through `stream`, flushed as a server ends a rectangle.
        fn compress(&mut self, stream: usize, data: &[u8]) -> Vec<u8> {
            let mut out = Vec::with_capacity(data.len() * 2 + 64);
            self.0[stream]
                .compress_vec(data, &mut out, FlushCompress::Sync)
                .expect("compress");
            out
        }

        /// A basic rectangle: `control`, an optional filter and palette, then `data`
        /// through `stream` behind its compact length.
        fn basic(&mut self, control: u8, head: &[u8], stream: usize, data: &[u8]) -> Vec<u8> {
            let mut bytes = vec![control];
            bytes.extend_from_slice(head);
            let compressed = self.compress(stream, data);
            bytes.extend_from_slice(&compact(compressed.len()));
            bytes.extend_from_slice(&compressed);
            bytes
        }
    }

    /// `length` as a compact length.
    fn compact(length: usize) -> Vec<u8> {
        let low = |value: usize| u8::try_from(value & 0x7f).expect("seven bits");
        if length < 0x80 {
            vec![low(length)]
        } else if length < 0x4000 {
            vec![low(length) | 0x80, low(length >> 7)]
        } else {
            let high = u8::try_from(length >> 14).expect("eight bits");
            vec![low(length) | 0x80, low(length >> 7) | 0x80, high]
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

    /// Decodes `bytes` whole onto a fresh `width` by `height` screen.
    fn decoded(bytes: &[u8], width: u16, height: u16) -> Result<Vec<[u8; 3]>, String> {
        let mut screen = Screen::new(width, height);
        let taken = Tight::new().decode(bytes, rect(width, height), &mut screen)?;
        assert_eq!(taken, Some(bytes.len()), "the whole rectangle taken");
        Ok(colours(&screen))
    }

    #[test]
    fn a_fill_paints_one_colour_and_takes_four_bytes() {
        let bytes = [FILL << KIND_SHIFT, 0, 255, 0, 99];
        let mut screen = Screen::new(3, 2);
        let taken = Tight::new()
            .decode(&bytes, rect(3, 2), &mut screen)
            .expect("decoded");
        assert_eq!(taken, Some(4), "the next byte is not the rectangle's");
        assert_eq!(colours(&screen), [GREEN; 6]);
    }

    #[test]
    fn a_short_copy_comes_as_is() {
        // 2 by 1: 6 bytes, under 12, so no zlib and no length.
        let bytes = [0x00, 255, 0, 0, 0, 0, 255];
        assert_eq!(decoded(&bytes, 2, 1).expect("decoded"), [RED, BLUE]);
    }

    #[test]
    fn a_longer_copy_goes_through_its_zlib_stream() {
        // 2 by 2: 12 bytes, through stream 1 with the filter implicit.
        let pixels = [255, 0, 0, 0, 255, 0, 0, 0, 255, 1, 2, 3];
        let bytes = Server::new().basic(0x10, &[], 1, &pixels);
        assert_eq!(
            decoded(&bytes, 2, 2).expect("decoded"),
            [RED, GREEN, BLUE, [1, 2, 3]]
        );
    }

    #[test]
    fn a_two_colour_palette_packs_a_bit_a_pixel_and_pads_each_row() {
        // 3 by 2, palette red and blue: rows "red blue red" and "blue blue red", one byte
        // each, high bits first: 010_00000 and 110_00000. 2 bytes: as is.
        let bytes = [
            0x40,
            FILTER_PALETTE,
            1,
            255,
            0,
            0,
            0,
            0,
            255,
            0b0100_0000,
            0b1100_0000,
        ];
        assert_eq!(
            decoded(&bytes, 3, 2).expect("decoded"),
            [RED, BLUE, RED, BLUE, BLUE, RED]
        );
    }

    #[test]
    fn a_wider_palette_takes_a_byte_a_pixel() {
        // 4 by 3, palette red, green and blue: 12 bytes, through stream 3.
        let indexes = [0, 1, 2, 1, 2, 2, 0, 0, 1, 0, 2, 1];
        let head = [FILTER_PALETTE, 2, 255, 0, 0, 0, 255, 0, 0, 0, 255];
        let bytes = Server::new().basic(0x40 | 0x30, &head, 3, &indexes);
        let palette = [RED, GREEN, BLUE];
        let expected: Vec<[u8; 3]> = indexes
            .iter()
            .map(|index| palette[usize::from(*index)])
            .collect();
        assert_eq!(decoded(&bytes, 4, 3).expect("decoded"), expected);
    }

    #[test]
    fn a_gradient_predicts_from_left_above_and_above_left_clamped() {
        // 2 by 2, wanted: (10,200,0) (20,250,0) / (30,250,5) (41,0,5). The last one's
        // prediction is (30+20-10, 250+250-200, 5+0-0) = (40, 300 kept at 255, 5): its
        // differences are (1, 0-255 = 1 modulo 256, 0).
        let differences = [10, 200, 0, 10, 50, 0, 20, 50, 5, 1, 1, 0];
        let bytes = Server::new().basic(0x40 | 0x20, &[FILTER_GRADIENT], 2, &differences);
        assert_eq!(
            decoded(&bytes, 2, 2).expect("decoded"),
            [[10, 200, 0], [20, 250, 0], [30, 250, 5], [41, 0, 5]]
        );
    }

    #[test]
    fn a_jpeg_is_drawn_at_its_rectangle() {
        let mut bytes = vec![JPEG << KIND_SHIFT];
        // 291 bytes: a compact length of two bytes.
        bytes.extend_from_slice(&compact(RED_BLUE_JPEG.len()));
        assert_eq!(bytes[1..], [0xa3, 0x02]);
        bytes.extend_from_slice(&RED_BLUE_JPEG);
        let colours = decoded(&bytes, 16, 8).expect("decoded");
        let near = |pixel: [u8; 3], wanted: [u8; 3]| {
            pixel
                .iter()
                .zip(wanted)
                .all(|(got, wanted)| got.abs_diff(wanted) <= 8)
        };
        for (at, pixel) in colours.iter().enumerate() {
            let wanted = if at % 16 < 8 { RED } else { BLUE };
            assert!(near(*pixel, wanted), "pixel {at}: {pixel:?}");
        }
    }

    #[test]
    fn a_jpeg_of_another_size_is_refused() {
        let mut bytes = vec![JPEG << KIND_SHIFT];
        bytes.extend_from_slice(&compact(RED_BLUE_JPEG.len()));
        bytes.extend_from_slice(&RED_BLUE_JPEG);
        // Larger than the rectangle, and smaller.
        assert!(decoded(&bytes, 8, 8).is_err());
        assert!(decoded(&bytes, 32, 8).is_err());
        // Not a JPEG at all.
        assert!(decoded(&[JPEG << KIND_SHIFT, 3, 1, 2, 3], 1, 1).is_err());
    }

    #[test]
    fn compact_lengths_take_one_to_three_bytes() {
        let read = |bytes: &[u8]| {
            let mut cursor = Bytes { data: bytes, at: 0 };
            cursor.compact_length().map(|length| (length, cursor.at))
        };
        assert_eq!(read(&[0x7f]), Some((127, 1)));
        assert_eq!(read(&[0x90, 0x4e]), Some((0x10 | (0x4e << 7), 2)));
        // The third byte gives all its 8 bits.
        assert_eq!(read(&[0xff, 0xff, 0xff]), Some((0x3f_ffff, 3)));
        assert_eq!(read(&[0x80]), None, "more to come");
    }

    #[test]
    fn a_reset_bit_starts_its_stream_anew_and_the_others_go_on() {
        let pixels = [9; 12];
        let mut server = Server::new();
        let mut client = Tight::new();
        let mut screen = Screen::new(2, 2);
        let area = rect(2, 2);
        for stream in [0, 1, 0] {
            let control = u8::try_from(stream << 4).expect("small");
            let bytes = server.basic(control, &[], stream, &pixels);
            client
                .decode(&bytes, area, &mut screen)
                .expect("the streams go on");
        }
        // The server starts stream 0 again and says so: the client follows.
        server.0[0] = Compress::new(Compression::default(), true);
        let bytes = server.basic(0x01, &[], 0, &pixels);
        client.decode(&bytes, area, &mut screen).expect("reset");
        // Stream 1 still goes on where it was.
        let bytes = server.basic(0x10, &[], 1, &pixels);
        client.decode(&bytes, area, &mut screen).expect("stream 1");
        // A stream started again without the bit does not inflate.
        server.0[1] = Compress::new(Compression::default(), true);
        let bytes = server.basic(0x10, &[], 1, &pixels);
        assert!(client.decode(&bytes, area, &mut screen).is_err());
    }

    #[test]
    fn a_rectangle_cut_short_waits_and_changes_nothing() {
        let mut server = Server::new();
        let mut client = Tight::new();
        let mut screen = Screen::new(2, 2);
        let area = rect(2, 2);
        let first = server.basic(0x00, &[], 0, &[1; 12]);
        for end in 0..first.len() {
            assert_eq!(
                client.decode(&first[..end], area, &mut screen),
                Ok(None),
                "{end} bytes"
            );
        }
        assert_eq!(
            client.decode(&first, area, &mut screen),
            Ok(Some(first.len()))
        );
        // The stream was not touched by the waits: the next rectangle follows on.
        let second = server.basic(0x00, &[], 0, &[2; 12]);
        client.decode(&second, area, &mut screen).expect("second");
        assert_eq!(colours(&screen), [[2, 2, 2]; 4]);
    }

    #[test]
    fn malformed_rectangles_are_refused() {
        // A compact length of nothing, and one far past what a 2 by 2 copy can compress to,
        // refused before its bytes come.
        assert!(decoded(&[0x00, 0x00], 2, 2).is_err());
        assert!(decoded(&[0x00, 0xff, 0xff, 0xff], 2, 2).is_err());
        assert!(decoded(&[JPEG << KIND_SHIFT, 0x00], 1, 1).is_err());
        // A palette index past three colours.
        let bytes = [0x40, FILTER_PALETTE, 2, 1, 1, 1, 2, 2, 2, 3, 3, 3, 0, 5];
        assert!(decoded(&bytes, 2, 1).is_err());
        // A palette of one colour, a pixel at its second.
        assert!(decoded(&[0x40, FILTER_PALETTE, 0, 1, 1, 1, 0x40], 2, 1).is_err());
        // An unknown filter, compression and TightPNG.
        assert!(decoded(&[0x40, 3, 0, 0, 0], 1, 1).is_err());
        assert!(decoded(&[0xb0, 0, 0, 0], 1, 1).is_err());
        assert!(decoded(&[PNG << KIND_SHIFT, 1, 0], 1, 1).is_err());
        // Data that is not zlib, and zlib data short of or past its rectangle.
        assert!(decoded(&[0x00, 4, 1, 2, 3, 4], 2, 2).is_err());
        assert!(decoded(&Server::new().basic(0x00, &[], 0, &[1; 11]), 2, 2).is_err());
        assert!(decoded(&Server::new().basic(0x00, &[], 0, &[1; 13]), 2, 2).is_err());
    }

    #[test]
    fn a_rectangle_inflating_far_past_its_size_is_refused() {
        // A 2 by 2 rectangle whose data inflates to a megabyte of zeros.
        let bomb = Server::new().basic(0x00, &[], 0, &vec![0; 1 << 20]);
        assert!(decoded(&bomb, 2, 2).is_err());
    }
}
