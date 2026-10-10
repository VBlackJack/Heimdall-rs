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

//! The PNG images of `TightPNG`, decoded here and nowhere else: what a server's encoder
//! writes for a rectangle, and nothing more.
//!
//! Read strictly: the signature, IHDR first, PLTE and tRNS before the image data, the IDAT
//! chunks one after another, IEND last and nothing after it. Every chunk's CRC-32 is checked,
//! ancillary chunks are skipped, an unknown critical one is refused. Only 8 bits a channel
//! and no interlacing: grey, RGB, a palette (its tRNS as alpha), grey and alpha, RGBA.
//!
//! The image must be the size of its rectangle, checked in IHDR before any data is read, and
//! its data inflates to exactly the rows that size takes: never a byte past.

use flate2::{Decompress, FlushDecompress, Status};

use super::screen::PIXEL_BYTES;

/// What every PNG starts with.
const SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];

/// Chunk types.
const IHDR: [u8; 4] = *b"IHDR";
const PLTE: [u8; 4] = *b"PLTE";
const TRNS: [u8; 4] = *b"tRNS";
const IDAT: [u8; 4] = *b"IDAT";
const IEND: [u8; 4] = *b"IEND";

/// A chunk's length and type before its data, its CRC after.
const CHUNK_LENGTH_BYTES: usize = 4;
const CHUNK_TYPE_BYTES: usize = 4;
const CHUNK_CRC_BYTES: usize = 4;

/// Longest chunk data the format allows.
const MAX_CHUNK_LENGTH: usize = 0x7fff_ffff;

/// Set in the first letter of an ancillary chunk's type: lower case.
const ANCILLARY: u8 = 0x20;

/// IHDR's data: width and height of 4 bytes each, then bit depth, colour type, compression,
/// filter and interlace methods.
const IHDR_BYTES: usize = 13;

/// The only bit depth read: 8 bits a channel.
const DEPTH: u8 = 8;

/// Colour types.
pub(super) const GREY: u8 = 0;
pub(super) const RGB: u8 = 2;
pub(super) const PALETTE: u8 = 3;
pub(super) const GREY_ALPHA: u8 = 4;
pub(super) const RGBA: u8 = 6;

/// The only compression and filter methods there are: deflate, five filter types.
const METHOD: u8 = 0;

/// No interlacing; Adam7 is refused.
const NOT_INTERLACED: u8 = 0;

/// Bytes of a palette entry, and most entries a palette at 8 bits holds.
const PALETTE_ENTRY_BYTES: usize = 3;
const MAX_PALETTE_ENTRIES: usize = 256;

/// tRNS of a grey image: one grey of 16 bits; of an RGB image: one colour of 16-bit channels.
const GREY_KEY_BYTES: usize = 2;
const RGB_KEY_BYTES: usize = 6;

/// Filter types, one byte before each row.
const FILTER_NONE: u8 = 0;
const FILTER_SUB: u8 = 1;
const FILTER_UP: u8 = 2;
const FILTER_AVERAGE: u8 = 3;
const FILTER_PAETH: u8 = 4;

/// Opaque, and fully transparent.
const OPAQUE: u8 = u8::MAX;
const TRANSPARENT: u8 = 0;

/// CRC-32 of the ISO 3309 polynomial, reflected, as PNG's chunks carry it.
const CRC_POLYNOMIAL: u32 = 0xedb8_8320;
const CRC_TABLE: [u32; 256] = crc_table();

/// What IHDR says.
struct Header {
    width: usize,
    height: usize,
    colour: u8,
}

impl Header {
    /// Bytes of a pixel, one a channel.
    fn pixel_bytes(&self) -> usize {
        match self.colour {
            GREY | PALETTE => 1,
            GREY_ALPHA => 2,
            RGB => 3,
            _ => 4,
        }
    }
}

/// The transparency tRNS gives.
enum Transparency {
    None,
    /// One alpha per palette entry, from the first; the rest opaque.
    Palette(Vec<u8>),
    /// The one grey, or colour, that is transparent, 16 bits a channel.
    Key([u16; 3]),
}

/// Decodes `data`, a PNG image that must be `width` by `height`, to RGBA pixels, rows left
/// to right, top to bottom. The image is refused before its data is read when IHDR gives
/// another size; its data never inflates past the rows of that size.
pub(crate) fn decode(data: &[u8], width: u16, height: u16) -> Result<Vec<u8>, String> {
    let rest = data
        .strip_prefix(&SIGNATURE)
        .ok_or_else(|| "it has no PNG signature".to_owned())?;
    let mut chunks = Chunks { data: rest, at: 0 };
    let (kind, body) = chunks.next()?;
    if kind != IHDR {
        return Err("its first chunk is not IHDR".to_owned());
    }
    let header = header(body, width, height)?;
    let mut palette: Option<&[u8]> = None;
    let mut transparency = Transparency::None;
    let mut compressed = Vec::new();
    // Before any IDAT, among them, or after the last.
    let mut idat_seen = false;
    let mut idat_done = false;
    loop {
        let (kind, body) = chunks.next()?;
        if idat_seen && kind != IDAT {
            idat_done = true;
        }
        match kind {
            IHDR => return Err("it has a second IHDR".to_owned()),
            PLTE => {
                if idat_seen || palette.is_some() {
                    return Err("its PLTE is misplaced".to_owned());
                }
                palette = Some(read_palette(body, header.colour)?);
            }
            TRNS => {
                if idat_seen || !matches!(transparency, Transparency::None) {
                    return Err("its tRNS is misplaced".to_owned());
                }
                transparency = read_transparency(body, header.colour, palette)?;
            }
            IDAT => {
                if idat_done {
                    return Err("its IDAT chunks are not one after another".to_owned());
                }
                idat_seen = true;
                compressed.extend_from_slice(body);
            }
            IEND => {
                if !body.is_empty() {
                    return Err("its IEND has data".to_owned());
                }
                if !chunks.is_empty() {
                    return Err("it has data after IEND".to_owned());
                }
                break;
            }
            other if other[0] & ANCILLARY != 0 => {}
            other => {
                return Err(format!(
                    "it has an unknown critical chunk {}",
                    String::from_utf8_lossy(&other)
                ));
            }
        }
    }
    if !idat_seen {
        return Err("it has no IDAT".to_owned());
    }
    if header.colour == PALETTE && palette.is_none() {
        return Err("it has a palette colour type and no PLTE".to_owned());
    }
    let pixel = header.pixel_bytes();
    let stride = header
        .width
        .checked_mul(pixel)
        .ok_or_else(|| "its rows are too large".to_owned())?;
    let size = stride
        .checked_add(1)
        .and_then(|line| line.checked_mul(header.height))
        .ok_or_else(|| "its image is too large".to_owned())?;
    let mut raw = inflate(&compressed, size)?;
    unfilter(&mut raw, stride, pixel)?;
    to_rgba(&raw, &header, palette.unwrap_or_default(), &transparency)
}

/// Reads IHDR's data, and checks the image is `width` by `height` and of a kind read here.
fn header(body: &[u8], width: u16, height: u16) -> Result<Header, String> {
    let Ok(body) = <[u8; IHDR_BYTES]>::try_from(body) else {
        return Err("its IHDR is not 13 bytes".to_owned());
    };
    let size = (
        u32::from_be_bytes([body[0], body[1], body[2], body[3]]),
        u32::from_be_bytes([body[4], body[5], body[6], body[7]]),
    );
    let [depth, colour, compression, filter, interlace] =
        [body[8], body[9], body[10], body[11], body[12]];
    if size != (u32::from(width), u32::from(height)) {
        return Err(format!(
            "an image of {}x{} for a {width}x{height} rectangle",
            size.0, size.1
        ));
    }
    if width == 0 || height == 0 {
        return Err("an image of no pixels".to_owned());
    }
    if !matches!(colour, GREY | RGB | PALETTE | GREY_ALPHA | RGBA) {
        return Err(format!("an unknown colour type {colour}"));
    }
    if depth != DEPTH {
        return Err(format!("a bit depth of {depth}, not 8"));
    }
    if compression != METHOD || filter != METHOD {
        return Err("an unknown compression or filter method".to_owned());
    }
    if interlace != NOT_INTERLACED {
        return Err("an interlaced image".to_owned());
    }
    Ok(Header {
        width: usize::from(width),
        height: usize::from(height),
        colour,
    })
}

/// Reads PLTE's data: the palette of a palette image; a suggestion an RGB image may carry,
/// unused; never one of a grey image.
fn read_palette(body: &[u8], colour: u8) -> Result<&[u8], String> {
    if matches!(colour, GREY | GREY_ALPHA) {
        return Err("a grey image has a PLTE".to_owned());
    }
    let entries = body.len() / PALETTE_ENTRY_BYTES;
    if !body.len().is_multiple_of(PALETTE_ENTRY_BYTES)
        || entries == 0
        || entries > MAX_PALETTE_ENTRIES
    {
        return Err(format!("its PLTE is {} bytes", body.len()));
    }
    Ok(body)
}

/// Reads tRNS's data for an image of `colour`, after its `palette`.
fn read_transparency(
    body: &[u8],
    colour: u8,
    palette: Option<&[u8]>,
) -> Result<Transparency, String> {
    let key = |at: usize| u16::from_be_bytes([body[at], body[at + 1]]);
    match colour {
        PALETTE => {
            let Some(palette) = palette else {
                return Err("its tRNS comes before its PLTE".to_owned());
            };
            if body.len() > palette.len() / PALETTE_ENTRY_BYTES {
                return Err("its tRNS has more entries than its palette".to_owned());
            }
            Ok(Transparency::Palette(body.to_vec()))
        }
        GREY if body.len() == GREY_KEY_BYTES => Ok(Transparency::Key([key(0); 3])),
        RGB if body.len() == RGB_KEY_BYTES => Ok(Transparency::Key([key(0), key(2), key(4)])),
        GREY | RGB => Err(format!("its tRNS is {} bytes", body.len())),
        _ => Err("an image with alpha has a tRNS".to_owned()),
    }
}

/// The chunks after the signature, each checked against its CRC.
struct Chunks<'a> {
    data: &'a [u8],
    at: usize,
}

impl<'a> Chunks<'a> {
    /// The next chunk's type and data.
    fn next(&mut self) -> Result<([u8; 4], &'a [u8]), String> {
        let cut = || "it is cut short".to_owned();
        let rest = self.data.get(self.at..).ok_or_else(cut)?;
        let Some((length, rest)) = rest.split_first_chunk::<CHUNK_LENGTH_BYTES>() else {
            return Err(cut());
        };
        let length = usize::try_from(u32::from_be_bytes(*length))
            .ok()
            .filter(|&length| length <= MAX_CHUNK_LENGTH)
            .ok_or_else(|| "a chunk longer than PNG allows".to_owned())?;
        let typed = rest.get(..CHUNK_TYPE_BYTES + length).ok_or_else(cut)?;
        let Some(crc) = rest
            .get(CHUNK_TYPE_BYTES + length..)
            .and_then(<[u8]>::first_chunk::<CHUNK_CRC_BYTES>)
        else {
            return Err(cut());
        };
        if crc32(typed) != u32::from_be_bytes(*crc) {
            return Err("a chunk fails its CRC".to_owned());
        }
        let Some((kind, body)) = typed.split_first_chunk::<CHUNK_TYPE_BYTES>() else {
            return Err(cut());
        };
        if !kind.iter().all(u8::is_ascii_alphabetic) {
            return Err("a chunk type that is not four letters".to_owned());
        }
        self.at += CHUNK_LENGTH_BYTES + CHUNK_TYPE_BYTES + length + CHUNK_CRC_BYTES;
        Ok((*kind, body))
    }

    /// Whether every byte has been read.
    fn is_empty(&self) -> bool {
        self.at >= self.data.len()
    }
}

/// Inflates the zlib stream `compressed` to exactly `size` bytes, and not one more.
fn inflate(compressed: &[u8], size: usize) -> Result<Vec<u8>, String> {
    let mut stream = Decompress::new(true);
    let consumed = |stream: &Decompress| {
        usize::try_from(stream.total_in()).map_err(|_| "its image data is too large".to_owned())
    };
    // One byte past the size: data inflating further is caught, never kept.
    let mut out = Vec::with_capacity(size + 1);
    loop {
        let before = (consumed(&stream)?, out.len());
        let status = stream
            .decompress_vec(&compressed[before.0..], &mut out, FlushDecompress::Finish)
            .map_err(|error| format!("its image data does not inflate: {error}"))?;
        if out.len() > size {
            return Err("its image data inflates past its size".to_owned());
        }
        if status == Status::StreamEnd {
            break;
        }
        if (consumed(&stream)?, out.len()) == before {
            return Err("its image data stops inflating".to_owned());
        }
    }
    if out.len() != size {
        return Err("its image data inflates short of its size".to_owned());
    }
    if consumed(&stream)? != compressed.len() {
        return Err("its image data goes on past its zlib stream".to_owned());
    }
    Ok(out)
}

/// Undoes each row's filter in place: `raw` holds rows of a filter byte, then `stride`
/// bytes of pixels of `pixel` bytes each.
fn unfilter(raw: &mut [u8], stride: usize, pixel: usize) -> Result<(), String> {
    let line = stride + 1;
    let first_prior = vec![0; stride];
    let mut start = 0;
    while start < raw.len() {
        let (done, rest) = raw.split_at_mut(start);
        let prior = if start == 0 {
            &first_prior[..]
        } else {
            &done[start - stride..]
        };
        let Some((filter, row)) = rest.get_mut(..line).and_then(<[u8]>::split_first_mut) else {
            return Err("its rows are cut short".to_owned());
        };
        match *filter {
            FILTER_NONE => {}
            FILTER_SUB => {
                for at in pixel..stride {
                    row[at] = row[at].wrapping_add(row[at - pixel]);
                }
            }
            FILTER_UP => {
                for (byte, above) in row.iter_mut().zip(prior) {
                    *byte = byte.wrapping_add(*above);
                }
            }
            FILTER_AVERAGE => {
                for at in 0..stride {
                    let left = if at >= pixel { row[at - pixel] } else { 0 };
                    row[at] = row[at].wrapping_add(average(left, prior[at]));
                }
            }
            FILTER_PAETH => {
                for at in 0..stride {
                    let (left, corner) = if at >= pixel {
                        (row[at - pixel], prior[at - pixel])
                    } else {
                        (0, 0)
                    };
                    row[at] = row[at].wrapping_add(paeth(left, prior[at], corner));
                }
            }
            other => return Err(format!("an unknown filter type {other}")),
        }
        start += line;
    }
    Ok(())
}

/// The floor of the mean of `a` and `b`, without overflow.
fn average(a: u8, b: u8) -> u8 {
    (a >> 1) + (b >> 1) + (a & b & 1)
}

/// The Paeth predictor: of `left`, `above` and `corner`, the nearest to
/// `left + above - corner`, ties to `left`, then `above`.
fn paeth(left: u8, above: u8, corner: u8) -> u8 {
    let (a, b, c) = (i16::from(left), i16::from(above), i16::from(corner));
    let to_left = (b - c).abs();
    let to_above = (a - c).abs();
    let to_corner = (a + b - 2 * c).abs();
    if to_left <= to_above && to_left <= to_corner {
        left
    } else if to_above <= to_corner {
        above
    } else {
        corner
    }
}

/// The unfiltered rows `raw` as RGBA pixels.
fn to_rgba(
    raw: &[u8],
    header: &Header,
    palette: &[u8],
    transparency: &Transparency,
) -> Result<Vec<u8>, String> {
    let pixel = header.pixel_bytes();
    let line = header.width * pixel + 1;
    let keyed = |channels: [u8; 3]| match transparency {
        Transparency::Key(key) if channels.map(u16::from) == *key => TRANSPARENT,
        _ => OPAQUE,
    };
    let mut out = Vec::with_capacity(header.width * header.height * PIXEL_BYTES);
    for row in raw.chunks_exact(line) {
        for bytes in row[1..].chunks_exact(pixel) {
            let rgba = match (header.colour, bytes) {
                (GREY, &[grey]) => [grey, grey, grey, keyed([grey; 3])],
                (GREY_ALPHA, &[grey, alpha]) => [grey, grey, grey, alpha],
                (RGB, &[red, green, blue]) => [red, green, blue, keyed([red, green, blue])],
                (RGBA, &[red, green, blue, alpha]) => [red, green, blue, alpha],
                (PALETTE, &[index]) => {
                    let index = usize::from(index);
                    let Some(&[red, green, blue]) = palette
                        .get(index * PALETTE_ENTRY_BYTES..)
                        .and_then(<[u8]>::first_chunk::<PALETTE_ENTRY_BYTES>)
                    else {
                        return Err(format!("a pixel at palette index {index}, past its PLTE"));
                    };
                    let alpha = match transparency {
                        Transparency::Palette(alphas) => {
                            alphas.get(index).copied().unwrap_or(OPAQUE)
                        }
                        _ => OPAQUE,
                    };
                    [red, green, blue, alpha]
                }
                _ => return Err("its pixels do not match its colour type".to_owned()),
            };
            out.extend_from_slice(&rgba);
        }
    }
    Ok(out)
}

/// The CRC-32 table, one entry per byte value.
const fn crc_table() -> [u32; 256] {
    let mut table = [0; 256];
    let mut value: u32 = 0;
    while value < 256 {
        let mut crc = value;
        let mut bit = 0;
        while bit < 8 {
            crc = if crc & 1 == 0 {
                crc >> 1
            } else {
                CRC_POLYNOMIAL ^ (crc >> 1)
            };
            bit += 1;
        }
        table[value as usize] = crc;
        value += 1;
    }
    table
}

/// The CRC-32 of `bytes`, as a PNG chunk carries it over its type and data.
fn crc32(bytes: &[u8]) -> u32 {
    !bytes.iter().fold(u32::MAX, |crc, &byte| {
        CRC_TABLE[usize::from(crc.to_le_bytes()[0] ^ byte)] ^ (crc >> 8)
    })
}

/// A PNG encoder for tests, as a server's would write: the decoder's own CRC, zlib through
/// flate2, and each row's filter chosen by the test.
#[cfg(test)]
pub(crate) mod encoder {
    use flate2::{Compress, Compression, FlushCompress};

    use super::{
        FILTER_AVERAGE, FILTER_NONE, FILTER_PAETH, FILTER_SUB, FILTER_UP, GREY, GREY_ALPHA, IDAT,
        IEND, IHDR, PALETTE, PLTE, RGB, SIGNATURE, TRNS, average, crc32, paeth,
    };

    /// An image to encode.
    pub(crate) struct Image<'a> {
        pub(crate) width: u32,
        pub(crate) height: u32,
        pub(crate) colour: u8,
        pub(crate) depth: u8,
        pub(crate) interlace: u8,
        /// Rows of pixels, unfiltered, without filter bytes.
        pub(crate) pixels: &'a [u8],
        /// Each row's filter type, taken in turn.
        pub(crate) filters: &'a [u8],
        pub(crate) palette: Option<&'a [u8]>,
        pub(crate) transparency: Option<&'a [u8]>,
        /// Chunks the image data is split over.
        pub(crate) pieces: usize,
    }

    impl<'a> Image<'a> {
        /// A `width` by `height` image of `colour`, unfiltered, in one IDAT.
        pub(crate) fn new(width: u32, height: u32, colour: u8, pixels: &'a [u8]) -> Self {
            Self {
                width,
                height,
                colour,
                depth: 8,
                interlace: 0,
                pixels,
                filters: &[FILTER_NONE],
                palette: None,
                transparency: None,
                pieces: 1,
            }
        }

        /// The PNG file.
        pub(crate) fn encode(&self) -> Vec<u8> {
            let mut ihdr = Vec::new();
            ihdr.extend_from_slice(&self.width.to_be_bytes());
            ihdr.extend_from_slice(&self.height.to_be_bytes());
            ihdr.extend_from_slice(&[self.depth, self.colour, 0, 0, self.interlace]);
            let mut out = SIGNATURE.to_vec();
            chunk(&mut out, IHDR, &ihdr);
            if let Some(palette) = self.palette {
                chunk(&mut out, PLTE, palette);
            }
            if let Some(transparency) = self.transparency {
                chunk(&mut out, TRNS, transparency);
            }
            let data = zlib(&self.filtered());
            let piece = data.len().div_ceil(self.pieces).max(1);
            for part in data.chunks(piece) {
                chunk(&mut out, IDAT, part);
            }
            chunk(&mut out, IEND, &[]);
            out
        }

        /// The rows, each filtered and led by its filter type.
        fn filtered(&self) -> Vec<u8> {
            let pixel = match self.colour {
                GREY | PALETTE => 1,
                GREY_ALPHA => 2,
                RGB => 3,
                _ => 4,
            };
            let stride = usize::try_from(self.width).expect("width") * pixel;
            let mut out = Vec::new();
            let zeros = vec![0; stride];
            for (index, row) in self.pixels.chunks(stride).enumerate() {
                let prior = if index == 0 {
                    &zeros[..]
                } else {
                    &self.pixels[(index - 1) * stride..index * stride]
                };
                let filter = self.filters[index % self.filters.len()];
                out.push(filter);
                for at in 0..row.len() {
                    let left = if at >= pixel { row[at - pixel] } else { 0 };
                    let corner = if at >= pixel { prior[at - pixel] } else { 0 };
                    let predicted = match filter {
                        FILTER_SUB => left,
                        FILTER_UP => prior[at],
                        FILTER_AVERAGE => average(left, prior[at]),
                        FILTER_PAETH => paeth(left, prior[at], corner),
                        _ => 0,
                    };
                    out.push(row[at].wrapping_sub(predicted));
                }
            }
            out
        }
    }

    /// Appends a chunk of `kind` and `body` to `out`.
    pub(crate) fn chunk(out: &mut Vec<u8>, kind: [u8; 4], body: &[u8]) {
        out.extend_from_slice(&u32::try_from(body.len()).expect("length").to_be_bytes());
        let mut typed = kind.to_vec();
        typed.extend_from_slice(body);
        out.extend_from_slice(&typed);
        out.extend_from_slice(&crc32(&typed).to_be_bytes());
    }

    /// `data` as one zlib stream.
    pub(crate) fn zlib(data: &[u8]) -> Vec<u8> {
        let mut stream = Compress::new(Compression::default(), true);
        let mut out = Vec::with_capacity(data.len() + 1024);
        stream
            .compress_vec(data, &mut out, FlushCompress::Finish)
            .expect("compressed");
        out
    }
}

#[cfg(test)]
mod tests {
    use super::encoder::{Image, chunk, zlib};
    use super::*;

    /// Bytes from a fixed sequence, varied enough to take every branch of every filter.
    fn noise(length: usize) -> Vec<u8> {
        let mut state: u32 = 0x1234_5678;
        (0..length)
            .map(|_| {
                state = state.wrapping_mul(1_103_515_245).wrapping_add(12_345);
                state.to_be_bytes()[1]
            })
            .collect()
    }

    /// A 2 by 1 RGB image of red and blue.
    fn red_blue() -> Vec<u8> {
        Image::new(2, 1, RGB, &[255, 0, 0, 0, 0, 255]).encode()
    }

    /// A PNG of these chunks, each with its right CRC.
    fn with_chunks(chunks: &[([u8; 4], &[u8])]) -> Vec<u8> {
        let mut out = SIGNATURE.to_vec();
        for (kind, body) in chunks {
            chunk(&mut out, *kind, body);
        }
        out
    }

    /// IHDR's data for a `width` by `height` image of 8-bit `colour`.
    fn ihdr(width: u32, height: u32, colour: u8) -> Vec<u8> {
        let mut body = width.to_be_bytes().to_vec();
        body.extend_from_slice(&height.to_be_bytes());
        body.extend_from_slice(&[DEPTH, colour, 0, 0, 0]);
        body
    }

    #[test]
    fn crc_is_iso_3309() {
        assert_eq!(crc32(b"123456789"), 0xcbf4_3926);
        assert_eq!(crc32(b""), 0);
    }

    #[test]
    fn every_filter_type_is_undone() {
        for (colour, pixel) in [(GREY, 1), (GREY_ALPHA, 2), (RGB, 3), (RGBA, 4)] {
            let pixels = noise(7 * 10 * pixel);
            for filters in [
                &[FILTER_NONE][..],
                &[FILTER_SUB],
                &[FILTER_UP],
                &[FILTER_AVERAGE],
                &[FILTER_PAETH],
                &[0, 1, 2, 3, 4],
            ] {
                let image = Image {
                    filters,
                    ..Image::new(7, 10, colour, &pixels)
                };
                let decoded = decode(&image.encode(), 7, 10).expect("decoded");
                let unfiltered = decode(&Image::new(7, 10, colour, &pixels).encode(), 7, 10);
                if colour == RGBA {
                    assert_eq!(decoded, pixels, "filters {filters:?}");
                }
                assert_eq!(
                    Ok(decoded),
                    unfiltered,
                    "colour {colour}, filters {filters:?}"
                );
            }
        }
    }

    #[test]
    fn every_colour_type_reads_as_rgba() {
        let grey = Image::new(2, 1, GREY, &[0, 200]).encode();
        assert_eq!(
            decode(&grey, 2, 1),
            Ok(vec![0, 0, 0, 255, 200, 200, 200, 255])
        );
        let grey_alpha = Image::new(2, 1, GREY_ALPHA, &[10, 20, 30, 40]).encode();
        assert_eq!(
            decode(&grey_alpha, 2, 1),
            Ok(vec![10, 10, 10, 20, 30, 30, 30, 40])
        );
        let rgb = Image::new(2, 1, RGB, &[1, 2, 3, 4, 5, 6]).encode();
        assert_eq!(decode(&rgb, 2, 1), Ok(vec![1, 2, 3, 255, 4, 5, 6, 255]));
        let rgba = Image::new(2, 1, RGBA, &[1, 2, 3, 4, 5, 6, 7, 8]).encode();
        assert_eq!(decode(&rgba, 2, 1), Ok(vec![1, 2, 3, 4, 5, 6, 7, 8]));
        let palette = Image {
            palette: Some(&[9, 8, 7, 6, 5, 4]),
            ..Image::new(3, 1, PALETTE, &[1, 0, 1])
        };
        assert_eq!(
            decode(&palette.encode(), 3, 1),
            Ok(vec![6, 5, 4, 255, 9, 8, 7, 255, 6, 5, 4, 255])
        );
    }

    #[test]
    fn trns_gives_alpha_to_palettes_and_keys() {
        // Two alphas for three entries: the third is opaque.
        let palette = Image {
            palette: Some(&[1, 1, 1, 2, 2, 2, 3, 3, 3]),
            transparency: Some(&[0, 128]),
            ..Image::new(3, 1, PALETTE, &[0, 1, 2])
        };
        assert_eq!(
            decode(&palette.encode(), 3, 1),
            Ok(vec![1, 1, 1, 0, 2, 2, 2, 128, 3, 3, 3, 255])
        );
        let grey = Image {
            transparency: Some(&[0, 50]),
            ..Image::new(2, 1, GREY, &[50, 51])
        };
        assert_eq!(
            decode(&grey.encode(), 2, 1),
            Ok(vec![50, 50, 50, 0, 51, 51, 51, 255])
        );
        let rgb = Image {
            transparency: Some(&[0, 1, 0, 2, 0, 3]),
            ..Image::new(2, 1, RGB, &[1, 2, 3, 1, 2, 4])
        };
        assert_eq!(
            decode(&rgb.encode(), 2, 1),
            Ok(vec![1, 2, 3, 0, 1, 2, 4, 255])
        );
        // A key past 8 bits matches nothing.
        let wide = Image {
            transparency: Some(&[1, 50]),
            ..Image::new(1, 1, GREY, &[50])
        };
        assert_eq!(decode(&wide.encode(), 1, 1), Ok(vec![50, 50, 50, 255]));
    }

    #[test]
    fn misplaced_or_wrong_palettes_and_trns_are_refused() {
        // More alphas than entries, a tRNS on alpha, a key of the wrong size.
        let too_many = Image {
            palette: Some(&[1, 1, 1]),
            transparency: Some(&[0, 0]),
            ..Image::new(1, 1, PALETTE, &[0])
        };
        assert!(decode(&too_many.encode(), 1, 1).is_err());
        let on_alpha = Image {
            transparency: Some(&[0, 0]),
            ..Image::new(1, 1, RGBA, &[0; 4])
        };
        assert!(decode(&on_alpha.encode(), 1, 1).is_err());
        let short_key = Image {
            transparency: Some(&[0, 0]),
            ..Image::new(1, 1, RGB, &[0; 3])
        };
        assert!(decode(&short_key.encode(), 1, 1).is_err());
        // No palette, an index past it, a PLTE on grey, one not of whole entries.
        assert!(decode(&Image::new(1, 1, PALETTE, &[0]).encode(), 1, 1).is_err());
        let past = Image {
            palette: Some(&[1, 1, 1]),
            ..Image::new(1, 1, PALETTE, &[1])
        };
        assert!(decode(&past.encode(), 1, 1).is_err());
        let on_grey = Image {
            palette: Some(&[1, 1, 1]),
            ..Image::new(1, 1, GREY, &[0])
        };
        assert!(decode(&on_grey.encode(), 1, 1).is_err());
        let partial = Image {
            palette: Some(&[1, 1]),
            ..Image::new(1, 1, PALETTE, &[0])
        };
        assert!(decode(&partial.encode(), 1, 1).is_err());
        // tRNS before PLTE.
        let data = zlib(&[0, 0]);
        let early = with_chunks(&[
            (IHDR, &ihdr(1, 1, PALETTE)),
            (TRNS, &[0]),
            (PLTE, &[1, 1, 1]),
            (IDAT, &data),
            (IEND, &[]),
        ]);
        assert!(decode(&early, 1, 1).is_err());
        // An RGB image's suggested palette is read past.
        let suggested = Image {
            palette: Some(&[1, 1, 1]),
            ..Image::new(1, 1, RGB, &[4, 5, 6])
        };
        assert_eq!(decode(&suggested.encode(), 1, 1), Ok(vec![4, 5, 6, 255]));
    }

    #[test]
    fn image_data_split_over_chunks_reads_whole() {
        let pixels = noise(5 * 4 * 3);
        let whole = Image::new(5, 4, RGB, &pixels).encode();
        for pieces in [2, 3, 1000] {
            let split = Image {
                pieces,
                ..Image::new(5, 4, RGB, &pixels)
            }
            .encode();
            assert!(split.len() > whole.len());
            assert_eq!(decode(&split, 5, 4), decode(&whole, 5, 4));
        }
        assert_eq!(
            decode(&whole, 5, 4),
            Ok(pixels
                .as_chunks::<3>()
                .0
                .iter()
                .flat_map(|&[red, green, blue]| [red, green, blue, 255])
                .collect())
        );
    }

    #[test]
    fn a_bad_crc_or_signature_is_refused() {
        let image = red_blue();
        assert!(decode(&image, 2, 1).is_ok());
        let mut bad_crc = image.clone();
        let last = bad_crc.len() - 1;
        bad_crc[last] ^= 1;
        assert!(decode(&bad_crc, 2, 1).is_err());
        let mut bad_signature = image.clone();
        bad_signature[1] = b'Q';
        assert!(decode(&bad_signature, 2, 1).is_err());
        assert!(decode(&[], 2, 1).is_err());
        // A bad type letter, its CRC made right.
        let data = zlib(&[0, 1, 2, 3, 4, 5, 6]);
        let digit = with_chunks(&[(IHDR, &ihdr(2, 1, RGB)), (*b"tE1t", &[]), (IDAT, &data)]);
        assert!(decode(&digit, 2, 1).is_err());
    }

    #[test]
    fn every_cut_and_every_flipped_byte_is_an_error_not_a_panic() {
        let image = Image {
            filters: &[0, 1, 2, 3, 4],
            palette: Some(&[1, 2, 3, 4, 5, 6]),
            transparency: Some(&[7]),
            pieces: 3,
            ..Image::new(
                3,
                5,
                PALETTE,
                &[0, 1, 0, 1, 1, 0, 0, 0, 1, 1, 1, 1, 0, 1, 0],
            )
        }
        .encode();
        assert!(decode(&image, 3, 5).is_ok());
        for end in 0..image.len() {
            assert!(decode(&image[..end], 3, 5).is_err(), "cut at {end}");
        }
        for at in 0..image.len() {
            for bit in 0..8 {
                let mut flipped = image.clone();
                flipped[at] ^= 1 << bit;
                assert!(decode(&flipped, 3, 5).is_err(), "flip at {at}, bit {bit}");
            }
        }
    }

    #[test]
    fn an_image_of_another_size_or_kind_is_refused() {
        let image = red_blue();
        assert!(decode(&image, 1, 1).is_err());
        assert!(decode(&image, 2, 2).is_err());
        assert!(decode(&image, 4, 1).is_err());
        let empty = with_chunks(&[(IHDR, &ihdr(0, 0, RGB)), (IEND, &[])]);
        assert!(decode(&empty, 0, 0).is_err());
        let sixteen = Image {
            depth: 16,
            ..Image::new(1, 1, GREY, &[0, 0])
        };
        assert!(decode(&sixteen.encode(), 1, 1).is_err());
        let unknown = Image::new(1, 1, 1, &[0]);
        assert!(decode(&unknown.encode(), 1, 1).is_err());
        let interlaced = Image {
            interlace: 1,
            ..Image::new(1, 1, GREY, &[0])
        };
        assert!(decode(&interlaced.encode(), 1, 1).is_err());
        // An IHDR of another length.
        let long = with_chunks(&[(IHDR, &[0; 14]), (IEND, &[])]);
        assert!(decode(&long, 2, 1).is_err());
    }

    #[test]
    fn a_chunk_longer_than_png_allows_is_refused_before_it_is_read() {
        let mut image = SIGNATURE.to_vec();
        image.extend_from_slice(&[0x80, 0, 0, 0]);
        image.extend_from_slice(b"IHDR");
        assert_eq!(
            decode(&image, 1, 1),
            Err("a chunk longer than PNG allows".to_owned())
        );
    }

    #[test]
    fn image_data_inflating_past_or_short_of_its_size_is_refused() {
        // A 2 by 2 grey image whose data inflates to a megabyte of zeros.
        let bomb = zlib(&vec![0; 1 << 20]);
        let image = with_chunks(&[(IHDR, &ihdr(2, 2, GREY)), (IDAT, &bomb), (IEND, &[])]);
        assert_eq!(
            decode(&image, 2, 2),
            Err("its image data inflates past its size".to_owned())
        );
        let short = zlib(&[0, 1, 2]);
        let image = with_chunks(&[(IHDR, &ihdr(2, 2, GREY)), (IDAT, &short), (IEND, &[])]);
        assert!(decode(&image, 2, 2).is_err());
        // Not zlib, and zlib followed by more.
        let image = with_chunks(&[(IHDR, &ihdr(1, 1, GREY)), (IDAT, &[1, 2]), (IEND, &[])]);
        assert!(decode(&image, 1, 1).is_err());
        let mut trailing = zlib(&[0, 0]);
        trailing.push(0);
        let image = with_chunks(&[(IHDR, &ihdr(1, 1, GREY)), (IDAT, &trailing), (IEND, &[])]);
        assert!(decode(&image, 1, 1).is_err());
        // An unknown filter type.
        let image = with_chunks(&[
            (IHDR, &ihdr(1, 1, GREY)),
            (IDAT, &zlib(&[5, 0])),
            (IEND, &[]),
        ]);
        assert!(decode(&image, 1, 1).is_err());
    }

    #[test]
    fn chunk_order_is_kept() {
        let data = zlib(&[0, 7]);
        let header = ihdr(1, 1, GREY);
        let read = |chunks: &[([u8; 4], &[u8])]| decode(&with_chunks(chunks), 1, 1);
        // Ancillary chunks anywhere are read past.
        assert_eq!(
            read(&[
                (IHDR, &header),
                (*b"tEXt", b"Comment\0x"),
                (IDAT, &data),
                (*b"zzZz", &[1, 2]),
                (IEND, &[]),
            ]),
            Ok(vec![7, 7, 7, 255])
        );
        // An unknown critical chunk, IHDR not first or twice, no IDAT, IDAT apart, PLTE
        // after IDAT, no IEND, IEND with data, data after IEND.
        let refused: [&[([u8; 4], &[u8])]; 9] = [
            &[(IHDR, &header), (*b"ZZZZ", &[]), (IDAT, &data), (IEND, &[])],
            &[(*b"tEXt", &[]), (IHDR, &header), (IDAT, &data), (IEND, &[])],
            &[(IHDR, &header), (IHDR, &header), (IDAT, &data), (IEND, &[])],
            &[(IHDR, &header), (IEND, &[])],
            &[
                (IHDR, &header),
                (IDAT, &data[..2]),
                (*b"tEXt", &[]),
                (IDAT, &data[2..]),
                (IEND, &[]),
            ],
            &[
                (IHDR, &ihdr(1, 1, RGB)),
                (IDAT, &zlib(&[0, 1, 2, 3])),
                (PLTE, &[1, 1, 1]),
                (IEND, &[]),
            ],
            &[(IHDR, &header), (IDAT, &data)],
            &[(IHDR, &header), (IDAT, &data), (IEND, &[0])],
            &[(IHDR, &header), (IDAT, &data), (IEND, &[]), (IEND, &[])],
        ];
        for (index, chunks) in refused.iter().enumerate() {
            assert!(read(chunks).is_err(), "case {index}");
        }
    }
}
