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

//! Minimum contrast: a text colour too close to its background is moved lighter or darker
//! until it reaches a WCAG 2 contrast ratio, keeping its hue, as xterm.js's
//! `minimumContrastRatio` and Windows Terminal's `adjustIndistinguishableColors` do. Only
//! text colours move; a background is never changed.

use std::collections::HashMap;
use std::sync::OnceLock;

use crate::palette::Rgb;

/// Ratios are held in tenths so a setting stays `Eq` and exact (45 is 4.5:1).
const TENTHS_PER_UNIT: f64 = 10.0;

/// Colour channel values, 0 to 255.
const CHANNEL_MAX: f64 = 255.0;

/// Entries of the sRGB to linear table: one per channel value.
const CHANNEL_VALUES: usize = 256;

/// sRGB channel level (0 to 1) under which the transfer curve is linear, and the slope there.
const SRGB_LINEAR_CUTOFF: f64 = 0.040_45;
const SRGB_LINEAR_SLOPE: f64 = 12.92;

/// sRGB transfer curve parameters above the cutoff: offset, divisor and exponent.
const SRGB_OFFSET: f64 = 0.055;
const SRGB_DIVISOR: f64 = 1.055;
const SRGB_EXPONENT: f64 = 2.4;

/// WCAG 2 relative luminance weights of the red, green and blue channels.
const LUMA_RED: f64 = 0.2126;
const LUMA_GREEN: f64 = 0.7152;
const LUMA_BLUE: f64 = 0.0722;

/// WCAG 2 flare term added to both luminances in a contrast ratio.
const FLARE: f64 = 0.05;

/// Steps of the lightness search; the range is 1, so 2^-LIGHTNESS_STEPS is its precision.
const LIGHTNESS_STEPS: u32 = 20;

/// Colours the memo remembers before it starts again: bounds its memory when a program
/// paints with many truecolor pairs.
const MEMO_CAPACITY: usize = 8192;

/// Bit width of one packed colour.
const COLOR_BITS: u32 = 24;

/// Bit width of the packed pair; the ratio sits above it.
const PAIR_BITS: u32 = COLOR_BITS * 2;

/// Bit width of one channel.
const CHANNEL_BITS: u32 = 8;

/// A WCAG 2 contrast ratio text must reach against its background.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MinimumContrast {
    tenths: u16,
}

impl MinimumContrast {
    /// The ratio `tenths` out of ten: 45 is 4.5:1.
    #[must_use]
    pub const fn from_tenths(tenths: u16) -> Self {
        Self { tenths }
    }

    /// Half the ratio, which faint text asks as xterm.js does.
    #[must_use]
    pub const fn halved(self) -> Self {
        Self {
            tenths: self.tenths / 2,
        }
    }

    /// The ratio, such as 4.5 for 4.5:1.
    #[must_use]
    pub fn ratio(self) -> f64 {
        f64::from(self.tenths) / TENTHS_PER_UNIT
    }
}

/// Linear light of each sRGB channel value.
fn linear_table() -> &'static [f64; CHANNEL_VALUES] {
    static TABLE: OnceLock<[f64; CHANNEL_VALUES]> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut table = [0.0; CHANNEL_VALUES];
        for (value, slot) in (0_u8..=u8::MAX).zip(table.iter_mut()) {
            let level = f64::from(value) / CHANNEL_MAX;
            *slot = if level <= SRGB_LINEAR_CUTOFF {
                level / SRGB_LINEAR_SLOPE
            } else {
                ((level + SRGB_OFFSET) / SRGB_DIVISOR).powf(SRGB_EXPONENT)
            };
        }
        table
    })
}

/// WCAG 2 relative luminance of `color`, 0 for black to 1 for white.
fn luminance(color: Rgb) -> f64 {
    let table = linear_table();
    LUMA_RED * table[usize::from(color.r)]
        + LUMA_GREEN * table[usize::from(color.g)]
        + LUMA_BLUE * table[usize::from(color.b)]
}

/// Contrast ratio of two luminances, 1 to 21.
fn ratio_of(a: f64, b: f64) -> f64 {
    let (light, dark) = if a >= b { (a, b) } else { (b, a) };
    (light + FLARE) / (dark + FLARE)
}

/// WCAG 2 contrast ratio between two colours, 1 to 21.
#[must_use]
pub fn contrast_ratio(a: Rgb, b: Rgb) -> f64 {
    ratio_of(luminance(a), luminance(b))
}

/// Hue (degrees), saturation and lightness of `color`, each of the last two 0 to 1.
fn to_hsl(color: Rgb) -> (f64, f64, f64) {
    let r = f64::from(color.r) / CHANNEL_MAX;
    let g = f64::from(color.g) / CHANNEL_MAX;
    let b = f64::from(color.b) / CHANNEL_MAX;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let lightness = f64::midpoint(max, min);
    let delta = max - min;
    if delta <= f64::EPSILON {
        return (0.0, 0.0, lightness);
    }
    let saturation = delta / (1.0 - (2.0 * lightness - 1.0).abs());
    let sector = if (max - r).abs() <= f64::EPSILON {
        ((g - b) / delta).rem_euclid(6.0)
    } else if (max - g).abs() <= f64::EPSILON {
        (b - r) / delta + 2.0
    } else {
        (r - g) / delta + 4.0
    };
    (sector * 60.0, saturation, lightness)
}

/// The colour of hue `hue` (degrees), `saturation` and `lightness`.
fn from_hsl(hue: f64, saturation: f64, lightness: f64) -> Rgb {
    let chroma = (1.0 - (2.0 * lightness - 1.0).abs()) * saturation;
    let sector = hue.rem_euclid(360.0) / 60.0;
    let second = chroma * (1.0 - (sector.rem_euclid(2.0) - 1.0).abs());
    let (r, g, b) = match sector {
        s if s < 1.0 => (chroma, second, 0.0),
        s if s < 2.0 => (second, chroma, 0.0),
        s if s < 3.0 => (0.0, chroma, second),
        s if s < 4.0 => (0.0, second, chroma),
        s if s < 5.0 => (second, 0.0, chroma),
        _ => (chroma, 0.0, second),
    };
    let offset = lightness - chroma / 2.0;
    let channel = |level: f64| {
        let scaled = ((level + offset) * CHANNEL_MAX)
            .round()
            .clamp(0.0, CHANNEL_MAX);
        // Rounded and clamped to 0..=255, so nothing is lost.
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a channel rounded into 0 to 255"
        )]
        let value = scaled as u8;
        value
    };
    Rgb {
        r: channel(r),
        g: channel(g),
        b: channel(b),
    }
}

/// `foreground` moved lighter or darker, its hue kept as far as the lightness allows, until
/// it reaches `minimum` against `background`; unchanged when it already does. The side it
/// was on is tried first, the other when that side cannot reach the ratio even at white or
/// black; a colour level with its background goes towards the side with more room. When
/// neither side reaches it, the side with more room is taken to its end.
#[must_use]
pub fn adjust_foreground(foreground: Rgb, background: Rgb, minimum: MinimumContrast) -> Rgb {
    let target = minimum.ratio();
    let background_luma = luminance(background);
    let foreground_luma = luminance(foreground);
    if ratio_of(foreground_luma, background_luma) >= target {
        return foreground;
    }
    let white = Rgb {
        r: u8::MAX,
        g: u8::MAX,
        b: u8::MAX,
    };
    let black = Rgb { r: 0, g: 0, b: 0 };
    let room_up = contrast_ratio(white, background);
    let room_down = contrast_ratio(black, background);
    let lighten_first = if foreground_luma > background_luma {
        true
    } else if foreground_luma < background_luma {
        false
    } else {
        room_up >= room_down
    };
    let (hue, saturation, lightness) = to_hsl(foreground);
    let reaches = |color: Rgb| contrast_ratio(color, background) >= target;
    for lighten in [lighten_first, !lighten_first] {
        let end = if lighten { white } else { black };
        if !reaches(end) {
            continue;
        }
        // Luminance only grows with lightness at a fixed hue and saturation, so the search
        // converges on the nearest lightness that passes; `passing` always does.
        let (mut failing, mut passing) = if lighten {
            (lightness, 1.0)
        } else {
            (lightness, 0.0)
        };
        for _ in 0..LIGHTNESS_STEPS {
            let middle = f64::midpoint(failing, passing);
            if reaches(from_hsl(hue, saturation, middle)) {
                passing = middle;
            } else {
                failing = middle;
            }
        }
        return from_hsl(hue, saturation, passing);
    }
    if room_up >= room_down { white } else { black }
}

/// What [`adjust_foreground`] answered, kept so drawing a screen does the search once per
/// colour pair: a screen of text has few pairs, and no cell allocates.
#[derive(Debug, Default)]
pub(crate) struct ContrastMemo {
    answers: HashMap<u64, Rgb>,
}

impl ContrastMemo {
    /// [`adjust_foreground`], remembered by foreground, background and ratio.
    pub(crate) fn adjust(
        &mut self,
        foreground: Rgb,
        background: Rgb,
        minimum: MinimumContrast,
    ) -> Rgb {
        let pack = |color: Rgb| {
            (u64::from(color.r) << (2 * CHANNEL_BITS))
                | (u64::from(color.g) << CHANNEL_BITS)
                | u64::from(color.b)
        };
        let key = (u64::from(minimum.tenths) << PAIR_BITS)
            | (pack(foreground) << COLOR_BITS)
            | pack(background);
        if let Some(answer) = self.answers.get(&key) {
            return *answer;
        }
        if self.answers.len() >= MEMO_CAPACITY {
            self.answers.clear();
        }
        let answer = adjust_foreground(foreground, background, minimum);
        self.answers.insert(key, answer);
        answer
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ContrastMemo, MinimumContrast, adjust_foreground, contrast_ratio, from_hsl, to_hsl,
    };
    use crate::palette::{Palette, Rgb};

    const RATIOS: [u16; 3] = [30, 45, 70];

    fn rgb(r: u8, g: u8, b: u8) -> Rgb {
        Rgb { r, g, b }
    }

    #[test]
    fn the_ratio_follows_wcag() {
        let white = rgb(255, 255, 255);
        let black = rgb(0, 0, 0);
        assert!((contrast_ratio(white, black) - 21.0).abs() < 1e-9);
        assert!((contrast_ratio(white, white) - 1.0).abs() < 1e-9);
        // #767676 on white is the well known 4.54:1.
        let grey = contrast_ratio(rgb(0x76, 0x76, 0x76), white);
        assert!((grey - 4.54).abs() < 0.01, "{grey}");
    }

    #[test]
    fn a_colour_that_already_passes_is_unchanged() {
        let minimum = MinimumContrast::from_tenths(45);
        let (white, black) = (rgb(255, 255, 255), rgb(0, 0, 0));
        assert_eq!(adjust_foreground(white, black, minimum), white);
        assert_eq!(adjust_foreground(black, white, minimum), black);
        let dracula = Palette::dracula();
        assert_eq!(
            adjust_foreground(dracula.foreground, dracula.background, minimum),
            dracula.foreground
        );
    }

    #[test]
    fn a_failing_colour_reaches_each_ratio_on_each_side() {
        for tenths in RATIOS {
            let minimum = MinimumContrast::from_tenths(tenths);
            for (fg, bg) in [
                (rgb(0x21, 0x22, 0x2C), rgb(0x28, 0x2A, 0x36)),
                (rgb(0x62, 0x72, 0xA4), rgb(0x28, 0x2A, 0x36)),
                (rgb(0x58, 0x6E, 0x75), rgb(0x00, 0x2B, 0x36)),
                (rgb(0xDD, 0xDD, 0xAA), rgb(0xFF, 0xFF, 0xFF)),
                (rgb(0x40, 0x40, 0x40), rgb(0x40, 0x40, 0x40)),
            ] {
                let adjusted = adjust_foreground(fg, bg, minimum);
                assert!(
                    contrast_ratio(adjusted, bg) >= minimum.ratio(),
                    "{fg:?} on {bg:?} at {tenths}: {adjusted:?}"
                );
            }
        }
    }

    #[test]
    fn the_direction_is_the_side_with_room() {
        let minimum = MinimumContrast::from_tenths(45);
        // Black on black can only get lighter; white on white only darker.
        let up = adjust_foreground(rgb(0, 0, 0), rgb(0, 0, 0), minimum);
        assert!(up.r > 0, "{up:?}");
        let down = adjust_foreground(rgb(255, 255, 255), rgb(255, 255, 255), minimum);
        assert!(down.r < 255, "{down:?}");
        // On a mid grey no ratio of 7 is reachable: an end is taken.
        let mid = rgb(0x77, 0x77, 0x77);
        let seven = MinimumContrast::from_tenths(70);
        let result = adjust_foreground(rgb(0x70, 0x70, 0x70), mid, seven);
        assert!(
            result == rgb(0, 0, 0) || result == rgb(255, 255, 255),
            "{result:?}"
        );
    }

    #[test]
    fn the_hue_is_kept() {
        let minimum = MinimumContrast::from_tenths(45);
        let background = rgb(0x28, 0x2A, 0x36);
        for fg in [
            rgb(0x62, 0x72, 0xA4),
            rgb(0x58, 0x6E, 0x75),
            rgb(0x4E, 0x3A, 0x60),
        ] {
            let adjusted = adjust_foreground(fg, background, minimum);
            assert_ne!(adjusted, fg);
            let (before, _, _) = to_hsl(fg);
            let (after, _, _) = to_hsl(adjusted);
            let distance = (before - after).abs().min(360.0 - (before - after).abs());
            assert!(
                distance < 6.0,
                "{fg:?} -> {adjusted:?}: {before} vs {after}"
            );
        }
    }

    #[test]
    fn hsl_round_trips() {
        for color in [
            rgb(12, 200, 99),
            rgb(255, 0, 0),
            rgb(1, 2, 3),
            rgb(128, 128, 128),
        ] {
            let (h, s, l) = to_hsl(color);
            assert_eq!(from_hsl(h, s, l), color);
        }
    }

    #[test]
    fn every_shipped_scheme_has_no_text_pair_below_each_ratio() {
        for palette in [
            Palette::dracula(),
            Palette::standard(),
            Palette::solarized_dark(),
            Palette::monokai(),
            Palette::nord(),
        ] {
            for tenths in RATIOS {
                let minimum = MinimumContrast::from_tenths(tenths);
                let background = palette.background;
                let colors = palette.ansi.iter().copied().chain([palette.foreground]);
                for color in colors {
                    let adjusted = adjust_foreground(color, background, minimum);
                    assert!(
                        contrast_ratio(adjusted, background) >= minimum.ratio(),
                        "{color:?} on {background:?} at {tenths}"
                    );
                }
                for index in 0..=u8::MAX {
                    let adjusted = adjust_foreground(palette.indexed(index), background, minimum);
                    assert!(
                        contrast_ratio(adjusted, background) >= minimum.ratio(),
                        "colour {index} on {background:?} at {tenths}"
                    );
                }
            }
        }
    }

    #[test]
    fn the_memo_stays_within_its_cap_and_keeps_answering() {
        let minimum = MinimumContrast::from_tenths(45);
        let mut memo = ContrastMemo::default();
        let bg = rgb(0x28, 0x2A, 0x36);
        for n in 0..(super::MEMO_CAPACITY + 10) {
            let n = u32::try_from(n).expect("small");
            let fg = rgb(
                u8::try_from(n % 256).expect("byte"),
                u8::try_from(n / 256 % 256).expect("byte"),
                0x30,
            );
            assert_eq!(
                memo.adjust(fg, bg, minimum),
                adjust_foreground(fg, bg, minimum)
            );
            assert!(memo.answers.len() <= super::MEMO_CAPACITY);
        }
        assert!(
            memo.answers.len() < super::MEMO_CAPACITY,
            "it started again"
        );
    }

    #[test]
    fn half_the_ratio_is_half_in_tenths() {
        assert!((MinimumContrast::from_tenths(45).halved().ratio() - 2.2).abs() < 1e-9);
        assert!((MinimumContrast::from_tenths(70).halved().ratio() - 3.5).abs() < 1e-9);
    }

    #[test]
    fn the_memo_answers_as_the_function_does() {
        let minimum = MinimumContrast::from_tenths(45);
        let mut memo = ContrastMemo::default();
        let (fg, bg) = (rgb(0x62, 0x72, 0xA4), rgb(0x28, 0x2A, 0x36));
        let expected = adjust_foreground(fg, bg, minimum);
        assert_eq!(memo.adjust(fg, bg, minimum), expected);
        assert_eq!(memo.adjust(fg, bg, minimum), expected);
        assert_eq!(memo.answers.len(), 1);
        let seven = MinimumContrast::from_tenths(70);
        assert_eq!(memo.adjust(fg, bg, seven), adjust_foreground(fg, bg, seven));
        assert_eq!(memo.answers.len(), 2, "the ratio is part of the key");
    }
}
