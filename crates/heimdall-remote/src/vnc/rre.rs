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

//! RRE (encoding 2), read as noVNC's `RREDecoder`: a count of subrectangles and the
//! background, then each subrectangle's colour, place and size, within the rectangle.
//!
//! The rectangle is read whole before anything is drawn. Its count is bounded twice: never
//! more subrectangles than the rectangle has pixels, never more bytes than `limit`; and the
//! pixels its subrectangles fill, together, never more than [`MAX_FILL_FACTOR`] times its
//! own, so a few bytes cannot make the client paint the desktop over and over.

use super::screen::{PIXEL_BYTES, Rect, Screen};

/// Bytes of the count and the background.
const HEADER_BYTES: usize = 4 + PIXEL_BYTES;

/// Bytes of a subrectangle: its colour, then x, y, width and height of 16 bits each.
const SUBRECT_BYTES: usize = PIXEL_BYTES + 4 * 2;

/// Pixels the subrectangles of a rectangle may fill together, as a multiple of its own: a
/// server's subrectangles hardly overlap, and past this they only repaint.
pub(crate) const MAX_FILL_FACTOR: usize = 4;

/// Draws the RRE rectangle `rect`, coded at the start of `data`, onto `screen`: the bytes it
/// took, or `None` when more are needed, nothing changed. The caller checked that `rect`
/// lies inside it.
pub(crate) fn decode(
    data: &[u8],
    rect: Rect,
    screen: &mut Screen,
    limit: usize,
) -> Result<Option<usize>, String> {
    let Some(header) = data.get(..HEADER_BYTES) else {
        return Ok(None);
    };
    let count = u32::from_be_bytes([header[0], header[1], header[2], header[3]]);
    let count = usize::try_from(count)
        .ok()
        .filter(|count| *count <= rect.area())
        .ok_or_else(|| format!("{count} RRE subrectangles for {} pixels", rect.area()))?;
    let length = count
        .checked_mul(SUBRECT_BYTES)
        .and_then(|bytes| bytes.checked_add(HEADER_BYTES))
        .filter(|length| *length <= limit)
        .ok_or_else(|| format!("{count} RRE subrectangles, past {limit} bytes"))?;
    let Some(subrects) = data.get(HEADER_BYTES..length) else {
        return Ok(None);
    };
    let subrects = subrects.as_chunks::<SUBRECT_BYTES>().0;
    // Every subrectangle checked, and their pixels counted, before anything is drawn.
    let budget = rect.area().saturating_mul(MAX_FILL_FACTOR);
    let mut filled: usize = 0;
    let mut places = Vec::with_capacity(subrects.len());
    for subrect in subrects {
        let field = |at: usize| u16::from_be_bytes([subrect[at], subrect[at + 1]]);
        let (x, y, width, height) = (
            field(PIXEL_BYTES),
            field(PIXEL_BYTES + 2),
            field(PIXEL_BYTES + 4),
            field(PIXEL_BYTES + 6),
        );
        if u32::from(x) + u32::from(width) > u32::from(rect.width)
            || u32::from(y) + u32::from(height) > u32::from(rect.height)
        {
            return Err(format!(
                "an RRE subrectangle {width}x{height} at {x},{y} outside its {}x{} rectangle",
                rect.width, rect.height
            ));
        }
        filled = filled.saturating_add(usize::from(width) * usize::from(height));
        if filled > budget {
            return Err(format!(
                "RRE subrectangles filling past {MAX_FILL_FACTOR} times their {}x{} rectangle",
                rect.width, rect.height
            ));
        }
        places.push(Rect {
            x: rect.x + x,
            y: rect.y + y,
            width,
            height,
        });
    }
    screen.fill(rect, [header[4], header[5], header[6]]);
    for (subrect, place) in subrects.iter().zip(places) {
        screen.fill(place, [subrect[0], subrect[1], subrect[2]]);
    }
    Ok(Some(length))
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIMIT: usize = 1 << 20;

    fn subrect(colour: [u8; 4], x: u16, y: u16, width: u16, height: u16) -> Vec<u8> {
        let mut bytes = colour.to_vec();
        for value in [x, y, width, height] {
            bytes.extend_from_slice(&value.to_be_bytes());
        }
        bytes
    }

    fn rgb_at(screen: &Screen, x: usize, y: usize) -> [u8; 3] {
        let at = (y * usize::from(screen.width()) + x) * PIXEL_BYTES;
        [
            screen.pixels()[at],
            screen.pixels()[at + 1],
            screen.pixels()[at + 2],
        ]
    }

    #[test]
    fn the_background_then_each_subrectangle_is_filled_and_a_cut_waits() {
        let mut screen = Screen::new(6, 4);
        let area = Rect {
            x: 1,
            y: 1,
            width: 4,
            height: 3,
        };
        let mut data = vec![0, 0, 0, 2, 10, 20, 30, 0];
        data.extend(subrect([255, 0, 0, 0], 0, 0, 2, 1));
        data.extend(subrect([0, 0, 255, 0], 3, 2, 1, 1));
        for cut in 0..data.len() {
            assert_eq!(
                decode(&data[..cut], area, &mut screen, LIMIT),
                Ok(None),
                "cut at {cut}"
            );
        }
        assert_eq!(
            screen,
            Screen::new(6, 4),
            "nothing drawn before it is whole"
        );
        assert_eq!(
            decode(&data, area, &mut screen, LIMIT),
            Ok(Some(data.len()))
        );
        assert_eq!(rgb_at(&screen, 0, 0), [0, 0, 0], "outside the rectangle");
        assert_eq!(rgb_at(&screen, 1, 1), [255, 0, 0]);
        assert_eq!(rgb_at(&screen, 2, 1), [255, 0, 0]);
        assert_eq!(rgb_at(&screen, 3, 1), [10, 20, 30], "background");
        assert_eq!(rgb_at(&screen, 4, 3), [0, 0, 255]);
    }

    #[test]
    fn a_subrectangle_outside_or_a_count_past_its_bounds_is_refused() {
        let mut screen = Screen::new(4, 4);
        let area = Rect {
            x: 0,
            y: 0,
            width: 2,
            height: 2,
        };
        let mut data = vec![0, 0, 0, 1, 0, 0, 0, 0];
        data.extend(subrect([1, 2, 3, 0], 1, 0, 2, 1));
        assert!(decode(&data, area, &mut screen, LIMIT).is_err());
        // Five subrectangles for four pixels, refused before they come.
        let data = [0, 0, 0, 5, 0, 0, 0, 0];
        assert!(decode(&data, area, &mut screen, LIMIT).is_err());
        let data = [0xff, 0xff, 0xff, 0xff, 0, 0, 0, 0];
        assert!(decode(&data, area, &mut screen, LIMIT).is_err());
        // Each inside, but four of the whole rectangle fill four times its pixels, and a
        // fifth is past that; nothing drawn.
        let mut data = vec![0, 0, 0, 4, 9, 9, 9, 0];
        for _ in 0..4 {
            data.extend(subrect([1, 2, 3, 0], 0, 0, 2, 2));
        }
        assert!(decode(&data, area, &mut screen, LIMIT).is_ok());
        let mut screen = Screen::new(4, 4);
        let area = Rect {
            x: 0,
            y: 0,
            width: 3,
            height: 2,
        };
        let mut data = vec![0, 0, 0, 5, 9, 9, 9, 0];
        for _ in 0..5 {
            data.extend(subrect([1, 2, 3, 0], 0, 0, 3, 2));
        }
        assert!(decode(&data, area, &mut screen, LIMIT).is_err());
        assert_eq!(screen, Screen::new(4, 4), "nothing drawn");
        let area = Rect {
            x: 0,
            y: 0,
            width: 2,
            height: 2,
        };
        // Within the pixels but past the bytes allowed.
        let data = [0, 0, 0, 4, 0, 0, 0, 0];
        assert!(decode(&data, area, &mut screen, HEADER_BYTES + SUBRECT_BYTES).is_err());
    }
}
