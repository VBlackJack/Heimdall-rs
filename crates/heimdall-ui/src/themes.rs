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

//! The window's themes: the seventeen palettes of `ThemeForge` 2.1.0 the C# Heimdall
//! offers, each tinted with the accent chosen, as the C# `ThemeService` tints it; and high
//! contrast, the Windows "High Contrast Black" scheme, taken while Windows has it on.
//!
//! Dracula reproduces the Dracula Theme palette by Zeno Rocha, under the MIT license, and
//! Drakul is derived from it; `THIRD-PARTY-NOTICES.md` says so.

use std::collections::HashMap;
use std::fmt;
use std::sync::{LazyLock, Mutex};

use heimdall_core::settings::{Accent, AppTheme};
use iced::theme::palette::{Extended, Pair, Palette};
use iced::{Color, Theme};

use crate::i18n::fl;

/// The colours of a theme, by the slots `ThemeForge` names them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ThemeColors {
    /// The window's background.
    pub background: Color,
    /// The surface a row or field stands out on.
    pub current_line: Color,
    /// The C# `SelectionColor`, which `HighlightBrush` reads: what a hovered row, a hovered
    /// tab and a hovered menu entry lie on. Distinct from the current line in 16 of the 18
    /// themes (Dracula and Drakul share it).
    pub selection: Color,
    /// The text.
    pub foreground: Color,
    /// The secondary text.
    pub comment: Color,
    /// The theme's own accent, which the Default accent takes.
    pub accent: Color,
    /// Blue, an accent only.
    pub blue: Color,
    /// Cyan.
    pub cyan: Color,
    /// Green, which success takes.
    pub green: Color,
    /// Orange, which a warning takes.
    pub orange: Color,
    /// Pink.
    pub pink: Color,
    /// Purple.
    pub purple: Color,
    /// Red, which danger takes.
    pub red: Color,
    /// Yellow.
    pub yellow: Color,
    /// A light theme: dark text on a bright background.
    pub light: bool,
    /// High contrast: every control outlined, wider, and the keyboard's field in yellow.
    pub high_contrast: bool,
}

impl ThemeColors {
    /// The colour `accent` picks: the theme's own for the Default one.
    #[must_use]
    pub const fn accent(&self, accent: Accent) -> Color {
        match accent {
            Accent::Default => self.accent,
            Accent::Blue => self.blue,
            Accent::Cyan => self.cyan,
            Accent::Green => self.green,
            Accent::Orange => self.orange,
            Accent::Pink => self.pink,
            Accent::Purple => self.purple,
            Accent::Red => self.red,
            Accent::Yellow => self.yellow,
        }
    }
}

/// The colour `0xRRGGBB`.
const fn hex(value: u32) -> Color {
    let [_, r, g, b] = value.to_be_bytes();
    Color::from_rgb8(r, g, b)
}

/// The canonical Dracula palette, with `ThemeForge`'s blue added.
const DRACULA: ThemeColors = ThemeColors {
    background: hex(0x0028_2A36),
    current_line: hex(0x0044_475A),
    selection: hex(0x0044_475A),
    foreground: hex(0x00F8_F8F2),
    comment: hex(0x0062_72A4),
    accent: hex(0x00BD_93F9),
    blue: hex(0x0070_AEFF),
    cyan: hex(0x008B_E9FD),
    green: hex(0x0050_FA7B),
    orange: hex(0x00FF_B86C),
    pink: hex(0x00FF_79C6),
    purple: hex(0x00BD_93F9),
    red: hex(0x00FF_5555),
    yellow: hex(0x00F1_FA8C),
    light: false,
    high_contrast: false,
};

/// Dracula with its comment lifted to be readable on the background.
const DRAKUL: ThemeColors = ThemeColors {
    comment: hex(0x00B3_BBD6),
    ..DRACULA
};

/// A dark palette of `ThemeForge`: its surfaces and text, over the accents every dark one
/// shares.
const fn dark(
    background: u32,
    current_line: u32,
    selection: u32,
    foreground: u32,
    comment: u32,
) -> ThemeColors {
    ThemeColors {
        background: hex(background),
        current_line: hex(current_line),
        selection: hex(selection),
        foreground: hex(foreground),
        comment: hex(comment),
        accent: hex(0x00C8_85E5),
        blue: hex(0x0063_A6FF),
        cyan: hex(0x002A_B9C8),
        green: hex(0x002A_C450),
        orange: hex(0x00DB_9155),
        pink: hex(0x00E4_7FAC),
        purple: hex(0x00C8_85E5),
        red: hex(0x00E5_8585),
        yellow: hex(0x00AB_AB24),
        light: false,
        high_contrast: false,
    }
}

/// A light palette of `ThemeForge`: its surfaces and text, over the darker accents every
/// light one shares.
const fn light(
    background: u32,
    current_line: u32,
    selection: u32,
    foreground: u32,
    comment: u32,
) -> ThemeColors {
    ThemeColors {
        background: hex(background),
        current_line: hex(current_line),
        selection: hex(selection),
        foreground: hex(foreground),
        comment: hex(comment),
        accent: hex(0x008E_18C1),
        blue: hex(0x0000_5FC2),
        cyan: hex(0x000F_707B),
        green: hex(0x000F_7829),
        orange: hex(0x0095_4D12),
        pink: hex(0x00B1_165C),
        purple: hex(0x008E_18C1),
        red: hex(0x00B8_1717),
        yellow: hex(0x0068_680D),
        light: true,
        high_contrast: false,
    }
}

/// The Windows "High Contrast Black" scheme: white text on black, hyperlinks in yellow,
/// selected text on cyan, disabled text in green. Its other colours are as bright, for
/// errors and warnings to stay readable on black.
const HIGH_CONTRAST: ThemeColors = ThemeColors {
    background: hex(0x0000_0000),
    current_line: hex(0x0000_0000),
    selection: hex(0x0000_0000),
    foreground: hex(0x00FF_FFFF),
    comment: hex(0x00FF_FFFF),
    accent: hex(0x00FF_FF00),
    blue: hex(0x001A_EBFF),
    cyan: hex(0x001A_EBFF),
    green: hex(0x003F_F23F),
    orange: hex(0x00FF_B000),
    pink: hex(0x00FF_80FF),
    purple: hex(0x00C0_A0FF),
    red: hex(0x00FF_6060),
    yellow: hex(0x00FF_FF00),
    light: false,
    high_contrast: true,
};

/// The colours of `theme`, as `ThemeForge` 2.1.0 defines them.
#[must_use]
#[allow(clippy::too_many_lines)] // One palette per theme, five numbers each.
pub const fn colors(theme: AppTheme) -> ThemeColors {
    match theme {
        AppTheme::Dracula => DRACULA,
        AppTheme::Drakul => DRAKUL,
        AppTheme::Striga => dark(
            0x0028_211F,
            0x003D_312F,
            0x004B_3D3A,
            0x00F3_F2F1,
            0x00B9_A09A,
        ),
        AppTheme::Cinder => dark(
            0x0028_271F,
            0x003D_3A2F,
            0x004B_483A,
            0x00F3_F3F1,
            0x00AD_A688,
        ),
        AppTheme::Bracken => dark(
            0x0021_281F,
            0x0031_3D2F,
            0x003D_4B3A,
            0x00F2_F3F1,
            0x0090_AD8A,
        ),
        AppTheme::Tarn => dark(
            0x001F_2827,
            0x002F_3D3A,
            0x003A_4B48,
            0x00F1_F3F3,
            0x008A_ADA7,
        ),
        AppTheme::Mortis => dark(
            0x001F_2528,
            0x002F_383D,
            0x003A_454B,
            0x00F1_F3F3,
            0x0094_AAB4,
        ),
        AppTheme::Slate => dark(
            0x001F_2128,
            0x002F_323D,
            0x003A_3E4B,
            0x00F1_F2F3,
            0x009E_A6BC,
        ),
        // Its own accent, an indigo.
        AppTheme::Magellan => ThemeColors {
            accent: hex(0x008C_9CFF),
            ..dark(
                0x001B_1F33,
                0x002A_3050,
                0x0035_3C63,
                0x00F1_F3FB,
                0x00A7_AED0,
            )
        },
        AppTheme::Voivode => dark(
            0x0021_1F28,
            0x0032_2F3D,
            0x003E_3A4B,
            0x00F2_F1F3,
            0x00A9_A2BE,
        ),
        AppTheme::Carmilla => dark(
            0x0027_1F28,
            0x003A_2F3D,
            0x0048_3A4B,
            0x00F3_F1F3,
            0x00B6_9EBB,
        ),
        AppTheme::Whitby => dark(
            0x0028_1F25,
            0x003D_2F38,
            0x004B_3A45,
            0x00F3_F1F3,
            0x00BB_9DB1,
        ),
        AppTheme::Vesper => dark(
            0x0028_1F21,
            0x003D_2F32,
            0x004B_3A3E,
            0x00F3_F1F2,
            0x00BC_9FA6,
        ),
        AppTheme::Parchment => light(
            0x00F4_F3F1,
            0x00E4_E2DC,
            0x00D7_D3CB,
            0x001C_1A17,
            0x0062_5842,
        ),
        AppTheme::Folio => light(
            0x00F1_F2F4,
            0x00DC_DFE4,
            0x00CB_CFD7,
            0x0017_191C,
            0x004D_5973,
        ),
        // Green moved to viridian.
        AppTheme::Wormwood => ThemeColors {
            green: hex(0x0013_C282),
            ..dark(
                0x001F_2824,
                0x002F_3D36,
                0x003A_4B42,
                0x00F1_F3F2,
                0x008A_AD9C,
            )
        },
        // Orange moved to amber.
        AppTheme::Sconce => ThemeColors {
            orange: hex(0x00DC_9316),
            ..dark(
                0x0028_231F,
                0x003D_342F,
                0x004B_413A,
                0x00F3_F2F1,
                0x00B5_A395,
            )
        },
        AppTheme::HighContrast => HIGH_CONTRAST,
    }
}

/// The hover and pressed colours `ThemeForge` writes for the accent of its own themes
/// (`Themes/*.xaml`, `AccentHoverColor` and `AccentPressedColor`), by that accent: four
/// distinct pairs across the 17 themes. Not the Oklab shift it computes for a tinted accent
/// (`ThemeService.AccentTint.cs:42`), which clips differently.
fn own_accent_states(accent: Color) -> Option<(Color, Color)> {
    // Compared by 8-bit channels, as the XAML writes them.
    let [r, g, b, _] = accent.into_rgba8();
    match (r, g, b) {
        // Dracula and Drakul.
        (0xBD, 0x93, 0xF9) => Some((hex(0x00D5_BEFC), hex(0x00A1_70E6))),
        // The dark themes.
        (0xC8, 0x85, 0xE5) => Some((hex(0x00D3_9DEA), hex(0x0096_64AC))),
        // The light themes.
        (0x8E, 0x18, 0xC1) => Some((hex(0x00A5_46CD), hex(0x006A_1291))),
        // Magellan.
        (0x8C, 0x9C, 0xFF) => Some((hex(0x00A6_B2FF), hex(0x005C_6BD6))),
        _ => None,
    }
}

/// The window's theme `theme`, tinted with `accent`: the accent is the primary colour,
/// green success, orange a warning and red danger, as the C# semantic colours; the weak
/// background is the theme's current line, and the secondary colour its comment, as the C#
/// `TextSecondaryBrush` that captions, placeholders and field outlines take.
fn build(theme: AppTheme, accent: Accent) -> Theme {
    let colors = colors(theme);
    let palette = Palette {
        background: colors.background,
        text: colors.foreground,
        primary: colors.accent(accent),
        success: colors.green,
        warning: colors.orange,
        danger: colors.red,
    };
    Theme::custom_with_fn(theme.name(), palette, move |palette| {
        let mut extended = Extended::generate(palette);
        extended.background.weak = Pair::new(colors.current_line, colors.foreground);
        // iced has no selection slot: the C# `HighlightBrush` rides the weaker background,
        // which nothing else reads, for `styles` to find from a bare `Theme`.
        extended.background.weaker = Pair::new(colors.selection, colors.foreground);
        // High contrast's secondary is its text: what stands on it is the background.
        let on_secondary = if colors.high_contrast {
            colors.background
        } else {
            colors.foreground
        };
        extended.secondary.base = Pair::new(colors.comment, on_secondary);
        // Every edge in the text colour, not a shade of black.
        if colors.high_contrast {
            extended.background.strong = Pair::new(colors.foreground, colors.background);
        }
        // iced has no hover slot for the primary colour: the accent's hover and pressed
        // ride the secondary's weak and strong shades, which nothing else reads, for
        // `accent_states_of` to find. Explicit for the theme's own accent, as the C#'s.
        let own = if accent == Accent::Default {
            own_accent_states(colors.accent)
        } else {
            None
        };
        let (hover, pressed) = own.unwrap_or_else(|| {
            (
                iced::theme::palette::lighten(colors.accent(accent), crate::tokens::ACCENT_SHIFT),
                iced::theme::palette::darken(colors.accent(accent), crate::tokens::ACCENT_SHIFT),
            )
        });
        extended.secondary.weak = Pair::new(hover, colors.background);
        extended.secondary.strong = Pair::new(pressed, colors.background);
        extended.is_dark = !colors.light;
        extended
    })
}

/// Every theme with every accent, built once: the window asks for its theme each frame.
static THEMES: LazyLock<Vec<Theme>> = LazyLock::new(|| {
    AppTheme::ALL
        .into_iter()
        .flat_map(|theme| Accent::ALL.map(|accent| build(theme, accent)))
        .collect()
});

/// The window's theme `theme`, tinted with `accent`.
#[must_use]
pub fn theme(theme: AppTheme, accent: Accent) -> Theme {
    let row = AppTheme::ALL.iter().position(|each| *each == theme);
    let column = Accent::ALL.iter().position(|each| *each == accent);
    let index = row.unwrap_or_default() * Accent::ALL.len() + column.unwrap_or_default();
    THEMES
        .get(index)
        .cloned()
        .unwrap_or_else(|| build(theme, accent))
}

/// The colours of the window's theme `theme`, found by the name [`build`] gives it: a view
/// drawing in a colour the palette does not hold, a protocol's, reads it here. A theme of
/// iced's own takes its palette's.
#[must_use]
pub fn colors_of(theme: &Theme) -> ThemeColors {
    let name = theme.to_string();
    AppTheme::ALL
        .into_iter()
        .find(|each| each.name() == name)
        .map_or_else(|| palette_colors(theme), colors)
}

/// The C# `SelectionColor` of the window's theme `theme`, which `HighlightBrush` reads:
/// read from the weaker background [`build`] stores it in, with no lookup by name.
#[must_use]
pub fn selection_of(theme: &Theme) -> Color {
    theme.extended_palette().background.weaker.color
}

/// The accent's colours under the pointer and pressed, read from the secondary's weak and
/// strong shades [`build`] stores them in. A theme not of `ThemeForge` shifts its primary
/// colour as the C# does a tinted accent.
#[must_use]
pub fn accent_states_of(theme: &Theme) -> (Color, Color) {
    let name = theme.to_string();
    if AppTheme::ALL.into_iter().any(|each| each.name() == name) {
        let secondary = &theme.extended_palette().secondary;
        (secondary.weak.color, secondary.strong.color)
    } else {
        let accent = theme.palette().primary;
        (
            iced::theme::palette::lighten(accent, crate::tokens::ACCENT_SHIFT),
            iced::theme::palette::darken(accent, crate::tokens::ACCENT_SHIFT),
        )
    }
}

/// Whether `theme` is high contrast, which [`crate::styles`] outlines wider.
#[must_use]
pub fn is_high_contrast(theme: &Theme) -> bool {
    theme.to_string() == AppTheme::HighContrast.name()
}

/// The colours of a theme not of `ThemeForge`, taken from its palette.
fn palette_colors(theme: &Theme) -> ThemeColors {
    let palette = theme.palette();
    let extended = theme.extended_palette();
    ThemeColors {
        background: palette.background,
        current_line: extended.background.weak.color,
        selection: extended.background.weaker.color,
        foreground: palette.text,
        comment: extended.secondary.base.color,
        accent: palette.primary,
        blue: palette.primary,
        cyan: palette.primary,
        green: palette.success,
        orange: palette.warning,
        pink: palette.danger,
        purple: palette.primary,
        red: palette.danger,
        yellow: palette.warning,
        light: !extended.is_dark,
        high_contrast: false,
    }
}

/// The least contrast WCAG 1.4.3 asks of small text.
pub const MIN_TEXT_CONTRAST: f64 = 4.5;

/// The least contrast WCAG 1.4.11 asks of a line that marks out a region.
pub const MIN_BOUNDARY_CONTRAST: f64 = 3.0;

/// How far the lightness moves per step, on the 0 to 1 HSL scale, as the C#
/// `ThemeTextColours.LightnessStep`.
const LIGHTNESS_STEP: f64 = 0.01;

/// Enough steps to walk the whole lightness scale from either end.
const MAX_LIGHTNESS_STEPS: u32 = 100;

/// Perceived (Rec. 601) luminance below which a background counts as dark, as the C#
/// `ThemeLuminance.DarkThreshold`.
const DARK_THRESHOLD: f64 = 0.5;

/// A colour derived from the theme's own, to read on both grounds of the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Derived {
    /// Error text (`ErrorTextBrush`), from the danger colour.
    Error,
    /// Warning text (`WarningTextBrush`).
    Warning,
    /// Success text (`SuccessTextBrush`).
    Success,
    /// Information text (`InfoTextBrush`), from the theme's cyan, which `ThemeForge` names
    /// `InfoColor`.
    Info,
    /// Accent text (`AccentTextBrush`), from the accent chosen.
    Accent,
    /// A line that marks out a region (`BorderStrongBrush`), from the border colour.
    BorderStrong,
    /// Muted text (the C# `TextTertiaryBrush`), from the theme's comment colour: a group's
    /// name on the Settings rail.
    Muted,
}

impl Derived {
    /// Every derived colour.
    pub const ALL: [Self; 7] = [
        Self::Error,
        Self::Warning,
        Self::Success,
        Self::Info,
        Self::Accent,
        Self::BorderStrong,
        Self::Muted,
    ];

    /// The contrast the colour must reach on the window background and on a card: 4.5 for
    /// text, 3 for a boundary line, as the C# `ThemeTextColours.Pairs`.
    #[must_use]
    pub const fn target(self) -> f64 {
        match self {
            Self::BorderStrong => MIN_BOUNDARY_CONTRAST,
            _ => MIN_TEXT_CONTRAST,
        }
    }
}

/// What a derivation was asked: the colour, both grounds (as 8-bit RGBA) and the contrast.
type Question = ([u8; 4], [u8; 4], [u8; 4], u64);

/// Every answer given so far. A window draws with a handful of themes, so the table holds a
/// few dozen entries; a style call is a lookup, not a lightness walk.
static ANSWERS: LazyLock<Mutex<HashMap<Question, Color>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// The colour `role` takes in `theme`: the theme's own where it already reaches the
/// contrast the role asks of the window background and of a card, the nearest one that does
/// otherwise. No allocation on the way, but for the information colour, which is looked up
/// by the theme's name.
#[must_use]
pub fn derived(theme: &Theme, role: Derived) -> Color {
    let extended = theme.extended_palette();
    let source = match role {
        Derived::Error => extended.danger.base.color,
        Derived::Warning => extended.warning.base.color,
        Derived::Success => extended.success.base.color,
        Derived::Info => colors_of(theme).cyan,
        Derived::Accent => theme.palette().primary,
        Derived::Muted => colors_of(theme).comment,
        // The C# `BorderColor`: the current line, the text in high contrast, where the
        // current line is black ([`build`] puts the text in the strong shade there).
        Derived::BorderStrong
            if extended.background.strong.color == extended.background.base.text =>
        {
            extended.background.base.text
        }
        Derived::BorderStrong => extended.background.weak.color,
    };
    let (background, card) = (theme.palette().background, extended.background.weak.color);
    let question = (
        source.into_rgba8(),
        background.into_rgba8(),
        card.into_rgba8(),
        role.target().to_bits(),
    );
    if let Ok(answers) = ANSWERS.lock()
        && let Some(answer) = answers.get(&question)
    {
        return *answer;
    }
    let answer = derive(source, background, card, role.target());
    if let Ok(mut answers) = ANSWERS.lock() {
        answers.insert(question, answer);
    }
    answer
}

/// The colour closest to `colour`, same hue and saturation, that reaches `target` against
/// both grounds; `colour` itself when it already does. Its lightness moves one percent at a
/// time, away from the grounds, as the C# `ThemeTextColours.Derive`.
#[must_use]
pub fn derive(colour: Color, background: Color, card: Color, target: f64) -> Color {
    let readable =
        |c: Color| contrast_ratio(c, background) >= target && contrast_ratio(c, card) >= target;
    if readable(colour) {
        return colour;
    }
    let lighten = perceived_luminance(background) < DARK_THRESHOLD;
    let [r, g, b, _] = colour.into_rgba8();
    let (hue, saturation, lightness) = to_hsl(r, g, b);
    let mut candidate = colour;
    for step in 1..=MAX_LIGHTNESS_STEPS {
        let moved = f64::from(step) * LIGHTNESS_STEP;
        let lightness = if lighten {
            (lightness + moved).min(1.0)
        } else {
            (lightness - moved).max(0.0)
        };
        let (r, g, b) = from_hsl(hue, saturation, lightness);
        candidate = Color {
            a: colour.a,
            ..Color::from_rgb8(r, g, b)
        };
        if readable(candidate) {
            return candidate;
        }
    }
    candidate
}

/// Black or white, whichever reads better on `fill`: the text of a badge on a raw status
/// colour. At least 4.5:1 on any fill, the worst case being a mid-bright one.
#[must_use]
pub fn text_on(fill: Color) -> Color {
    if contrast_ratio(Color::BLACK, fill) >= contrast_ratio(Color::WHITE, fill) {
        Color::BLACK
    } else {
        Color::WHITE
    }
}

/// WCAG 2 contrast ratio of two opaque colours, from 1 to 21, in double precision as the
/// C# `ThemeTextColours.ContrastRatio`.
#[must_use]
pub fn contrast_ratio(a: Color, b: Color) -> f64 {
    let (a, b) = (relative_luminance(a), relative_luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

/// Relative luminance of `color`, as WCAG 2, from its 8-bit channels.
fn relative_luminance(color: Color) -> f64 {
    let linear = |channel: u8| {
        let c = f64::from(channel) / 255.0;
        if c <= 0.03928 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    let [r, g, b, _] = color.into_rgba8();
    0.2126 * linear(r) + 0.7152 * linear(g) + 0.0722 * linear(b)
}

/// Rec. 601 luma of `color`, from 0 to 1.
fn perceived_luminance(color: Color) -> f64 {
    let [r, g, b, _] = color.into_rgba8();
    (0.299 * f64::from(r) + 0.587 * f64::from(g) + 0.114 * f64::from(b)) / 255.0
}

/// Hue, saturation and lightness, each from 0 to 1, of an 8-bit colour.
#[allow(clippy::float_cmp)] // `max` is one of the three channels, exactly.
fn to_hsl(r: u8, g: u8, b: u8) -> (f64, f64, f64) {
    let (r, g, b) = (
        f64::from(r) / 255.0,
        f64::from(g) / 255.0,
        f64::from(b) / 255.0,
    );
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let lightness = max.midpoint(min);
    let delta = max - min;
    if delta == 0.0 {
        return (0.0, 0.0, lightness);
    }
    let saturation = if lightness > 0.5 {
        delta / (2.0 - max - min)
    } else {
        delta / (max + min)
    };
    let hue = if max == r {
        (g - b) / delta + if g < b { 6.0 } else { 0.0 }
    } else if max == g {
        (b - r) / delta + 2.0
    } else {
        (r - g) / delta + 4.0
    };
    (hue / 6.0, saturation, lightness)
}

/// The 8-bit colour of a hue, saturation and lightness.
#[allow(
    clippy::float_cmp,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)] // A grey has a saturation of exactly 0; a channel is clamped to 0..=255 first.
fn from_hsl(hue: f64, saturation: f64, lightness: f64) -> (u8, u8, u8) {
    // Half to even, as .NET's `Math.Round`.
    let byte = |unit: f64| (unit.clamp(0.0, 1.0) * 255.0).round_ties_even() as u8;
    if saturation == 0.0 {
        let grey = byte(lightness);
        return (grey, grey, grey);
    }
    let q = if lightness < 0.5 {
        lightness * (1.0 + saturation)
    } else {
        lightness + saturation - lightness * saturation
    };
    let p = 2.0 * lightness - q;
    let channel = |mut t: f64| {
        if t < 0.0 {
            t += 1.0;
        }
        if t > 1.0 {
            t -= 1.0;
        }
        let unit = if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 0.5 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        };
        byte(unit)
    };
    (
        channel(hue + 1.0 / 3.0),
        channel(hue),
        channel(hue - 1.0 / 3.0),
    )
}

/// The colours the integrated editor highlights code with: dark ones on a dark theme,
/// light ones on a light theme.
#[must_use]
pub const fn syntax(theme: AppTheme) -> iced::highlighter::Theme {
    if colors(theme).light {
        iced::highlighter::Theme::InspiredGitHub
    } else {
        iced::highlighter::Theme::Base16Ocean
    }
}

/// A theme in the Settings page's list, named as the C# names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ThemeChoice(pub AppTheme);

impl fmt::Display for ThemeChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&match self.0 {
            AppTheme::Dracula => fl!("ui-theme-dracula"),
            AppTheme::Drakul => fl!("ui-theme-drakul"),
            AppTheme::Striga => fl!("ui-theme-striga"),
            AppTheme::Cinder => fl!("ui-theme-cinder"),
            AppTheme::Bracken => fl!("ui-theme-bracken"),
            AppTheme::Tarn => fl!("ui-theme-tarn"),
            AppTheme::Mortis => fl!("ui-theme-mortis"),
            AppTheme::Slate => fl!("ui-theme-slate"),
            AppTheme::Magellan => fl!("ui-theme-magellan"),
            AppTheme::Voivode => fl!("ui-theme-voivode"),
            AppTheme::Carmilla => fl!("ui-theme-carmilla"),
            AppTheme::Whitby => fl!("ui-theme-whitby"),
            AppTheme::Vesper => fl!("ui-theme-vesper"),
            AppTheme::Parchment => fl!("ui-theme-parchment"),
            AppTheme::Folio => fl!("ui-theme-folio"),
            AppTheme::Wormwood => fl!("ui-theme-wormwood"),
            AppTheme::Sconce => fl!("ui-theme-sconce"),
            AppTheme::HighContrast => fl!("ui-theme-high-contrast"),
        })
    }
}

/// An accent in the Settings page's list, named as the C# names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AccentChoice(pub Accent);

impl fmt::Display for AccentChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&match self.0 {
            Accent::Default => fl!("ui-accent-default"),
            Accent::Blue => fl!("ui-accent-blue"),
            Accent::Cyan => fl!("ui-accent-cyan"),
            Accent::Green => fl!("ui-accent-green"),
            Accent::Orange => fl!("ui-accent-orange"),
            Accent::Pink => fl!("ui-accent-pink"),
            Accent::Purple => fl!("ui-accent-purple"),
            Accent::Red => fl!("ui-accent-red"),
            Accent::Yellow => fl!("ui-accent-yellow"),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The light themes of `ThemeForge`: its Light family.
    const LIGHT: [AppTheme; 2] = [AppTheme::Parchment, AppTheme::Folio];

    #[test]
    fn the_default_accent_takes_the_explicit_hover_and_pressed_of_themeforge() {
        // Dracula `AccentHoverColor` / `AccentPressedColor`, `Themes/Dracula.xaml:38-39`.
        let dracula = theme(AppTheme::Dracula, Accent::Default);
        assert_eq!(
            accent_states_of(&dracula),
            (hex(0x00D5_BEFC), hex(0x00A1_70E6))
        );
        // Folio, a light theme (`Themes/Folio.xaml`), and Magellan's indigo.
        assert_eq!(
            accent_states_of(&theme(AppTheme::Folio, Accent::Default)),
            (hex(0x00A5_46CD), hex(0x006A_1291))
        );
        assert_eq!(
            accent_states_of(&theme(AppTheme::Magellan, Accent::Default)),
            (hex(0x00A6_B2FF), hex(0x005C_6BD6))
        );
    }

    #[test]
    fn a_tinted_accent_is_shifted_as_themeforge_does() {
        let tinted = theme(AppTheme::Dracula, Accent::Blue);
        let blue = colors(AppTheme::Dracula).blue;
        assert_eq!(
            accent_states_of(&tinted),
            (
                iced::theme::palette::lighten(blue, crate::tokens::ACCENT_SHIFT),
                iced::theme::palette::darken(blue, crate::tokens::ACCENT_SHIFT)
            )
        );
    }

    #[test]
    fn every_theme_builds_dark_or_light_as_its_family() {
        for each in AppTheme::ALL {
            let built = theme(each, Accent::Default);
            let extended = built.extended_palette();
            assert_eq!(extended.is_dark, !LIGHT.contains(&each), "{each:?}");
            assert_eq!(
                extended.is_dark,
                iced::theme::palette::is_dark(colors(each).background),
                "{each:?}: its background agrees"
            );
            assert_eq!(built.palette().background, colors(each).background);
            assert_eq!(built.palette().text, colors(each).foreground);
            assert_eq!(
                extended.background.weak.color,
                colors(each).current_line,
                "{each:?}"
            );
            assert_eq!(
                extended.secondary.base.color,
                colors(each).comment,
                "{each:?}: the secondary text"
            );
        }
    }

    #[test]
    fn the_semantic_colours_are_the_themes_green_orange_and_red() {
        for each in AppTheme::ALL {
            let palette = theme(each, Accent::Default).palette();
            let colors = colors(each);
            assert_eq!(palette.success, colors.green, "{each:?}");
            assert_eq!(palette.warning, colors.orange, "{each:?}");
            assert_eq!(palette.danger, colors.red, "{each:?}");
        }
    }

    #[test]
    fn the_accent_is_the_primary_colour_and_default_the_themes_own() {
        assert_eq!(
            theme(AppTheme::Drakul, Accent::Default).palette().primary,
            hex(0x00BD_93F9),
            "Dracula's purple"
        );
        assert_eq!(
            theme(AppTheme::Magellan, Accent::Default).palette().primary,
            hex(0x008C_9CFF),
            "Magellan's own indigo"
        );
        for each in AppTheme::ALL {
            for accent in Accent::ALL {
                assert_eq!(
                    theme(each, accent).palette().primary,
                    colors(each).accent(accent),
                    "{each:?} {accent:?}"
                );
            }
            assert_ne!(
                theme(each, Accent::Cyan).palette().primary,
                theme(each, Accent::Red).palette().primary,
                "{each:?}"
            );
        }
    }

    #[test]
    fn the_signature_accents_are_the_ones_themeforge_breaks() {
        let dark = colors(AppTheme::Slate);
        assert_eq!(colors(AppTheme::Wormwood).green, hex(0x0013_C282));
        assert_eq!(colors(AppTheme::Sconce).orange, hex(0x00DC_9316));
        assert_eq!(colors(AppTheme::Wormwood).orange, dark.orange);
        assert_eq!(colors(AppTheme::Sconce).green, dark.green);
        assert_eq!(
            colors(AppTheme::Dracula),
            ThemeColors {
                comment: hex(0x0062_72A4),
                ..colors(AppTheme::Drakul)
            },
            "Drakul is Dracula with its comment lifted"
        );
    }

    #[test]
    fn a_window_theme_gives_back_its_colours() {
        for each in AppTheme::ALL {
            assert_eq!(
                colors_of(&theme(each, Accent::Red)),
                colors(each),
                "{each:?}"
            );
        }
        let other = colors_of(&Theme::Light);
        assert_eq!(other.blue, Theme::Light.palette().primary, "its palette's");
    }

    #[test]
    fn high_contrast_is_white_on_black_with_yellow_and_cyan_and_alone_so() {
        let built = theme(AppTheme::HighContrast, Accent::Default);
        assert_eq!(built.palette().background, Color::BLACK);
        assert_eq!(built.palette().text, Color::WHITE);
        assert_eq!(
            built.palette().primary,
            hex(0x00FF_FF00),
            "yellow, its own accent"
        );
        assert_eq!(
            theme(AppTheme::HighContrast, Accent::Cyan)
                .palette()
                .primary,
            hex(0x001A_EBFF)
        );
        let extended = built.extended_palette();
        assert_eq!(extended.secondary.base.color, Color::WHITE);
        assert_eq!(
            extended.secondary.base.text,
            Color::BLACK,
            "readable on its secondary"
        );
        assert_eq!(
            extended.background.strong.color,
            Color::WHITE,
            "white edges"
        );
        assert!(is_high_contrast(&built));
        for each in AppTheme::ALL {
            assert_eq!(
                is_high_contrast(&theme(each, Accent::Default)),
                each == AppTheme::HighContrast,
                "{each:?}"
            );
        }
        assert!(!is_high_contrast(&Theme::Dark), "iced's own");
    }

    #[test]
    fn a_light_theme_highlights_code_in_light_colours() {
        for each in AppTheme::ALL {
            assert_eq!(syntax(each).is_dark(), !LIGHT.contains(&each), "{each:?}");
        }
    }

    #[test]
    fn the_selection_is_the_themeforge_selection_colour_and_the_window_theme_carries_it() {
        // `SelectionColor` of `ThemeForge` 2.1.0 `Themes/*.xaml`, line 28 (38 for the
        // Magellan family): the C# `HighlightBrush`.
        let expected = [
            (AppTheme::Striga, 0x004B_3D3A),
            (AppTheme::Cinder, 0x004B_483A),
            (AppTheme::Bracken, 0x003D_4B3A),
            (AppTheme::Tarn, 0x003A_4B48),
            (AppTheme::Mortis, 0x003A_454B),
            (AppTheme::Slate, 0x003A_3E4B),
            (AppTheme::Magellan, 0x0035_3C63),
            (AppTheme::Voivode, 0x003E_3A4B),
            (AppTheme::Carmilla, 0x0048_3A4B),
            (AppTheme::Whitby, 0x004B_3A45),
            (AppTheme::Vesper, 0x004B_3A3E),
            (AppTheme::Parchment, 0x00D7_D3CB),
            (AppTheme::Folio, 0x00CB_CFD7),
            (AppTheme::Wormwood, 0x003A_4B42),
            (AppTheme::Sconce, 0x004B_413A),
        ];
        for (each, value) in expected {
            assert_eq!(colors(each).selection, hex(value), "{each:?}");
            assert_ne!(
                colors(each).selection,
                colors(each).current_line,
                "{each:?}: apart from the card, as the C#"
            );
        }
        for each in AppTheme::ALL {
            for accent in Accent::ALL {
                assert_eq!(
                    selection_of(&theme(each, accent)),
                    colors(each).selection,
                    "{each:?} {accent:?}"
                );
            }
        }
    }

    #[test]
    fn dracula_and_drakul_select_on_their_current_line_as_the_csharp() {
        for each in [AppTheme::Dracula, AppTheme::Drakul] {
            assert_eq!(colors(each).selection, hex(0x0044_475A), "{each:?}");
            assert_eq!(colors(each).selection, colors(each).current_line);
        }
        assert_eq!(
            colors(AppTheme::HighContrast).selection,
            colors(AppTheme::HighContrast).background,
            "black on black: high contrast keeps its own lit entries"
        );
    }

    #[test]
    fn a_light_theme_selects_darker_than_its_card_and_a_dark_one_lighter() {
        let luma = |c: Color| 0.2126 * c.r + 0.7152 * c.g + 0.0722 * c.b;
        for each in AppTheme::ALL {
            let colors = colors(each);
            if each == AppTheme::HighContrast
                || each == AppTheme::Dracula
                || each == AppTheme::Drakul
            {
                continue;
            }
            if colors.light {
                assert!(
                    luma(colors.selection) < luma(colors.current_line),
                    "{each:?}"
                );
            } else {
                assert!(
                    luma(colors.selection) > luma(colors.current_line),
                    "{each:?}"
                );
            }
        }
    }

    #[test]
    fn every_derived_colour_reaches_its_contrast_on_the_window_and_on_a_card_in_every_theme() {
        let mut checked = 0;
        for each in AppTheme::ALL {
            for accent in Accent::ALL {
                let built = theme(each, accent);
                let background = built.palette().background;
                let card = built.extended_palette().background.weak.color;
                for role in Derived::ALL {
                    let color = derived(&built, role);
                    for (ground, name) in [(background, "window"), (card, "card")] {
                        let ratio = contrast_ratio(color, ground);
                        assert!(
                            ratio >= role.target(),
                            "{each:?}/{accent:?}: {role:?} is {ratio:.2}:1 on the {name}"
                        );
                    }
                    checked += 1;
                }
            }
        }
        // The loop cannot pass by being empty: 17 themes and high contrast, nine accents,
        // seven colours.
        assert_eq!(AppTheme::ALL.len(), 18);
        assert_eq!(checked, 18 * Accent::ALL.len() * Derived::ALL.len());
    }

    #[test]
    fn a_colour_that_already_reads_is_kept_byte_for_byte() {
        let dark = Color::from_rgb8(0x28, 0x2A, 0x36);
        let card = Color::from_rgb8(0x44, 0x47, 0x5A);
        let white = Color::WHITE;
        assert_eq!(derive(white, dark, card, MIN_TEXT_CONTRAST), white);
        for each in AppTheme::ALL {
            let built = theme(each, Accent::Default);
            let source = built.extended_palette().danger.base.color;
            let ground = built.palette().background;
            let card = built.extended_palette().background.weak.color;
            if contrast_ratio(source, ground) >= MIN_TEXT_CONTRAST
                && contrast_ratio(source, card) >= MIN_TEXT_CONTRAST
            {
                assert_eq!(derived(&built, Derived::Error), source, "{each:?}");
            }
        }
    }

    #[test]
    fn drakul_error_text_is_lifted_as_the_csharp_derives_it() {
        // `92ccb004`: the red #FF5555 is 2.91:1 on the card #44475A and becomes #FF9C9C.
        let built = theme(AppTheme::Drakul, Accent::Default);
        assert_eq!(
            derived(&built, Derived::Error),
            Color::from_rgb8(0xFF, 0x9C, 0x9C)
        );
    }

    #[test]
    fn the_strong_border_is_visible_where_the_theme_border_is_the_card() {
        // `a07a2402`: the border is the card colour, 1.00:1 on a card.
        let built = theme(AppTheme::Drakul, Accent::Default);
        let card = built.extended_palette().background.weak.color;
        assert!(contrast_ratio(derived(&built, Derived::BorderStrong), card) >= 3.0);
        assert!(contrast_ratio(card, card) < 1.01);
    }

    #[test]
    fn the_secondary_text_that_hints_are_drawn_in_reads_at_4_5_to_1_in_every_theme_but_dracula() {
        // Dracula keeps the canonical comment colour of the Dracula Theme palette
        // (#6272A4), 3.03:1 on its window; Drakul is the same theme with the comment lifted
        // to read, and the C# keeps both as they are. Every other theme reads.
        let mut short = Vec::new();
        let mut checked = 0;
        for each in AppTheme::ALL {
            let built = theme(each, Accent::Default);
            let text = built.extended_palette().secondary.base.color;
            let card = built.extended_palette().background.weak.color;
            if [built.palette().background, card]
                .into_iter()
                .any(|ground| contrast_ratio(text, ground) < MIN_TEXT_CONTRAST)
            {
                short.push(each);
            }
            checked += 1;
        }
        assert_eq!(checked, AppTheme::ALL.len());
        assert_eq!(short, [AppTheme::Dracula]);
    }
}
