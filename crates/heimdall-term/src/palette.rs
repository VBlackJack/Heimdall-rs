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

//! Terminal colours: the 16 theme colours, the xterm 256-colour extension, and the special
//! foreground, background and cursor colours.

pub use alacritty_terminal::vte::ansi::Rgb;

/// Number of ANSI theme colours: 8 normal, 8 bright.
pub const ANSI_COLORS: usize = 16;

/// First index of the 6x6x6 colour cube.
const CUBE_START: usize = 16;

/// Steps per channel in the colour cube.
const CUBE_STEPS: usize = 6;

/// Channel intensity of each cube step, as xterm defines it.
const CUBE_LEVELS: [u8; CUBE_STEPS] = [0, 95, 135, 175, 215, 255];

/// First index of the grey ramp.
const GRAY_START: usize = CUBE_START + CUBE_STEPS * CUBE_STEPS * CUBE_STEPS;

/// Last index of the 256-colour table.
const LAST_INDEXED: usize = 255;

/// Intensity of the first grey step.
const GRAY_BASE: u8 = 8;

/// Intensity added per grey step.
const GRAY_STEP: u8 = 10;

const fn rgb(value: u32) -> Rgb {
    let [_, r, g, b] = value.to_be_bytes();
    Rgb { r, g, b }
}

/// The colours a terminal draws with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    /// Default text colour.
    pub foreground: Rgb,
    /// Default background colour.
    pub background: Rgb,
    /// Cursor colour.
    pub cursor: Rgb,
    /// Background of selected cells: the C# scheme's selection colour, laid over the
    /// background as xterm.js lays it.
    pub selection: Rgb,
    /// Colours 0 to 15: black, red, green, yellow, blue, magenta, cyan, white, then bright.
    pub ansi: [Rgb; ANSI_COLORS],
}

impl Palette {
    /// The Dracula theme, base of the Heimdall design system.
    #[must_use]
    pub const fn dracula() -> Self {
        Self {
            foreground: rgb(0x00F8_F8F2),
            background: rgb(0x0028_2A36),
            cursor: rgb(0x00F8_F8F2),
            selection: rgb(0x0045_4E6D),
            ansi: [
                rgb(0x0021_222C),
                rgb(0x00FF_5555),
                rgb(0x0050_FA7B),
                rgb(0x00F1_FA8C),
                rgb(0x00BD_93F9),
                rgb(0x00FF_79C6),
                rgb(0x008B_E9FD),
                rgb(0x00F8_F8F2),
                rgb(0x0062_72A4),
                rgb(0x00FF_6E6E),
                rgb(0x0069_FF94),
                rgb(0x00FF_FFA5),
                rgb(0x00D6_ACFF),
                rgb(0x00FF_92DF),
                rgb(0x00A4_FFFF),
                rgb(0x00FF_FFFF),
            ],
        }
    }

    /// The terminal's own colours, xterm.js's when no theme is given, as the C# Heimdall's
    /// "Default" scheme: white on black, the Tango palette.
    #[must_use]
    pub const fn standard() -> Self {
        Self {
            foreground: rgb(0x00FF_FFFF),
            background: rgb(0x0000_0000),
            cursor: rgb(0x00FF_FFFF),
            selection: rgb(0x004D_4D4D),
            ansi: [
                rgb(0x002E_3436),
                rgb(0x00CC_0000),
                rgb(0x004E_9A06),
                rgb(0x00C4_A000),
                rgb(0x0034_65A4),
                rgb(0x0075_507B),
                rgb(0x0006_989A),
                rgb(0x00D3_D7CF),
                rgb(0x0055_5753),
                rgb(0x00EF_2929),
                rgb(0x008A_E234),
                rgb(0x00FC_E94F),
                rgb(0x0072_9FCF),
                rgb(0x00AD_7FA8),
                rgb(0x0034_E2E2),
                rgb(0x00EE_EEEC),
            ],
        }
    }

    /// Solarized Dark, as the C# Heimdall's.
    #[must_use]
    pub const fn solarized_dark() -> Self {
        Self {
            foreground: rgb(0x0083_9496),
            background: rgb(0x0000_2B36),
            cursor: rgb(0x0093_A1A1),
            selection: rgb(0x000B_4865),
            ansi: [
                rgb(0x0007_3642),
                rgb(0x00DC_322F),
                rgb(0x0085_9900),
                rgb(0x00B5_8900),
                rgb(0x0026_8BD2),
                rgb(0x00D3_3682),
                rgb(0x002A_A198),
                rgb(0x00EE_E8D5),
                rgb(0x0058_6E75),
                rgb(0x00CB_4B16),
                rgb(0x0058_6E75),
                rgb(0x0065_7B83),
                rgb(0x0083_9496),
                rgb(0x006C_71C4),
                rgb(0x0093_A1A1),
                rgb(0x00FD_F6E3),
            ],
        }
    }

    /// Monokai, as the C# Heimdall's.
    #[must_use]
    pub const fn monokai() -> Self {
        Self {
            foreground: rgb(0x00F8_F8F2),
            background: rgb(0x0027_2822),
            cursor: rgb(0x00F8_F8F0),
            selection: rgb(0x003F_3E36),
            ansi: [
                rgb(0x0027_2822),
                rgb(0x00F9_2672),
                rgb(0x00A6_E22E),
                rgb(0x00F4_BF75),
                rgb(0x0066_D9EF),
                rgb(0x00AE_81FF),
                rgb(0x00A1_EFE4),
                rgb(0x00F8_F8F2),
                rgb(0x0075_715E),
                rgb(0x00F9_2672),
                rgb(0x00A6_E22E),
                rgb(0x00F4_BF75),
                rgb(0x0066_D9EF),
                rgb(0x00AE_81FF),
                rgb(0x00A1_EFE4),
                rgb(0x00F9_F8F5),
            ],
        }
    }

    /// Nord, as the C# Heimdall's.
    #[must_use]
    pub const fn nord() -> Self {
        Self {
            foreground: rgb(0x00D8_DEE9),
            background: rgb(0x002E_3440),
            cursor: rgb(0x00D8_DEE9),
            selection: rgb(0x003D_4555),
            ansi: [
                rgb(0x003B_4252),
                rgb(0x00BF_616A),
                rgb(0x00A3_BE8C),
                rgb(0x00EB_CB8B),
                rgb(0x0081_A1C1),
                rgb(0x00B4_8EAD),
                rgb(0x0088_C0D0),
                rgb(0x00E5_E9F0),
                rgb(0x004C_566A),
                rgb(0x00BF_616A),
                rgb(0x00A3_BE8C),
                rgb(0x00EB_CB8B),
                rgb(0x0081_A1C1),
                rgb(0x00B4_8EAD),
                rgb(0x008F_BCBB),
                rgb(0x00EC_EFF4),
            ],
        }
    }

    /// Colour of entry `index` of the 256-colour table: the theme for 0 to 15, the xterm
    /// cube for 16 to 231, the grey ramp for 232 to 255.
    #[must_use]
    pub fn indexed(&self, index: u8) -> Rgb {
        let index = usize::from(index);
        if index < ANSI_COLORS {
            return self.ansi[index];
        }
        if index < GRAY_START {
            let cube = index - CUBE_START;
            let level = |step: usize| CUBE_LEVELS[step % CUBE_STEPS];
            return Rgb {
                r: level(cube / (CUBE_STEPS * CUBE_STEPS)),
                g: level(cube / CUBE_STEPS),
                b: level(cube),
            };
        }
        debug_assert!(index <= LAST_INDEXED);
        let step = u8::try_from(index - GRAY_START).unwrap_or(u8::MAX);
        let gray = GRAY_BASE.saturating_add(GRAY_STEP.saturating_mul(step));
        Rgb {
            r: gray,
            g: gray,
            b: gray,
        }
    }
}

/// Special colour slots after the 256-colour table, as vte numbers them.
const FOREGROUND_SLOT: usize = 256;
const BACKGROUND_SLOT: usize = 257;
const CURSOR_SLOT: usize = 258;
const FIRST_DIM_SLOT: usize = 259;
const LAST_DIM_SLOT: usize = 266;
const BRIGHT_FOREGROUND_SLOT: usize = 267;
const DIM_FOREGROUND_SLOT: usize = 268;

/// Brightness kept by a dimmed colour, as a fraction: numerator over denominator.
const DIM_NUMERATOR: u16 = 2;
const DIM_DENOMINATOR: u16 = 3;

/// A colour dimmed the way faint text (SGR 2) is drawn.
#[must_use]
pub fn dim(color: Rgb) -> Rgb {
    let scale = |channel: u8| {
        u8::try_from(u16::from(channel) * DIM_NUMERATOR / DIM_DENOMINATOR).unwrap_or(channel)
    };
    Rgb {
        r: scale(color.r),
        g: scale(color.g),
        b: scale(color.b),
    }
}

impl Palette {
    /// Default colour of any slot the emulator addresses, 0 to 268: the 256-colour table,
    /// then foreground, background, cursor, the dim versions of colours 0 to 7, bright
    /// foreground and dim foreground. A slot the server overrides (OSC 4, 10, 11, 12) is
    /// looked up in the emulator first; this is only the fallback.
    #[must_use]
    pub fn slot(&self, slot: usize) -> Rgb {
        match slot {
            FOREGROUND_SLOT | BRIGHT_FOREGROUND_SLOT => self.foreground,
            BACKGROUND_SLOT => self.background,
            CURSOR_SLOT => self.cursor,
            FIRST_DIM_SLOT..=LAST_DIM_SLOT => dim(self.ansi[slot - FIRST_DIM_SLOT]),
            DIM_FOREGROUND_SLOT => dim(self.foreground),
            _ => u8::try_from(slot).map_or(self.foreground, |index| self.indexed(index)),
        }
    }
}

impl Default for Palette {
    fn default() -> Self {
        Self::dracula()
    }
}

#[cfg(test)]
mod tests {
    use super::{Palette, Rgb};

    fn rgb(r: u8, g: u8, b: u8) -> Rgb {
        Rgb { r, g, b }
    }

    #[test]
    fn the_first_sixteen_entries_are_the_theme() {
        let palette = Palette::dracula();
        assert_eq!(palette.indexed(1), rgb(0xFF, 0x55, 0x55));
        assert_eq!(palette.indexed(15), rgb(0xFF, 0xFF, 0xFF));
    }

    #[test]
    fn the_cube_follows_xterm() {
        let palette = Palette::dracula();
        assert_eq!(palette.indexed(16), rgb(0, 0, 0));
        assert_eq!(palette.indexed(196), rgb(255, 0, 0));
        assert_eq!(palette.indexed(21), rgb(0, 0, 255));
        assert_eq!(palette.indexed(46), rgb(0, 255, 0));
        assert_eq!(palette.indexed(110), rgb(135, 175, 215));
        assert_eq!(palette.indexed(231), rgb(255, 255, 255));
    }

    /// `over` with `color` laid on it at `alpha` out of 255, rounded as xterm.js blends.
    fn blend(over: Rgb, color: Rgb, alpha: u16) -> Rgb {
        let mix = |under: u8, top: u8| {
            let under = i32::from(under);
            let delta = (i32::from(top) - under) * i32::from(alpha);
            let rounded = (delta * 2 + 255).div_euclid(2 * 255);
            u8::try_from(under + rounded).expect("a channel")
        };
        Rgb {
            r: mix(over.r, color.r),
            g: mix(over.g, color.g),
            b: mix(over.b, color.b),
        }
    }

    #[test]
    fn the_selection_is_the_csharp_schemes_laid_over_their_background() {
        // The C# themes' `selectionBackground`, alpha out of 255 as xterm.js parses it; the
        // C# Default scheme takes xterm.js's own, white at 0x4D.
        for (palette, color, alpha) in [
            (Palette::dracula(), rgb(98, 114, 164), 128),
            (Palette::solarized_dark(), rgb(38, 139, 210), 77),
            (Palette::monokai(), rgb(73, 72, 62), 179),
            (Palette::nord(), rgb(67, 76, 94), 179),
            (Palette::standard(), rgb(255, 255, 255), 77),
        ] {
            assert_eq!(
                palette.selection,
                blend(palette.background, color, alpha),
                "{palette:?}"
            );
        }
    }

    #[test]
    fn the_grey_ramp_follows_xterm() {
        let palette = Palette::dracula();
        assert_eq!(palette.indexed(232), rgb(8, 8, 8));
        assert_eq!(palette.indexed(244), rgb(128, 128, 128));
        assert_eq!(palette.indexed(255), rgb(238, 238, 238));
    }
}
