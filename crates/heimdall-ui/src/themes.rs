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

use std::fmt;
use std::sync::LazyLock;

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
const fn dark(background: u32, current_line: u32, foreground: u32, comment: u32) -> ThemeColors {
    ThemeColors {
        background: hex(background),
        current_line: hex(current_line),
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
const fn light(background: u32, current_line: u32, foreground: u32, comment: u32) -> ThemeColors {
    ThemeColors {
        background: hex(background),
        current_line: hex(current_line),
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
pub const fn colors(theme: AppTheme) -> ThemeColors {
    match theme {
        AppTheme::Dracula => DRACULA,
        AppTheme::Drakul => DRAKUL,
        AppTheme::Striga => dark(0x0028_211F, 0x003D_312F, 0x00F3_F2F1, 0x00B9_A09A),
        AppTheme::Cinder => dark(0x0028_271F, 0x003D_3A2F, 0x00F3_F3F1, 0x00AD_A688),
        AppTheme::Bracken => dark(0x0021_281F, 0x0031_3D2F, 0x00F2_F3F1, 0x0090_AD8A),
        AppTheme::Tarn => dark(0x001F_2827, 0x002F_3D3A, 0x00F1_F3F3, 0x008A_ADA7),
        AppTheme::Mortis => dark(0x001F_2528, 0x002F_383D, 0x00F1_F3F3, 0x0094_AAB4),
        AppTheme::Slate => dark(0x001F_2128, 0x002F_323D, 0x00F1_F2F3, 0x009E_A6BC),
        // Its own accent, an indigo.
        AppTheme::Magellan => ThemeColors {
            accent: hex(0x008C_9CFF),
            ..dark(0x001B_1F33, 0x002A_3050, 0x00F1_F3FB, 0x00A7_AED0)
        },
        AppTheme::Voivode => dark(0x0021_1F28, 0x0032_2F3D, 0x00F2_F1F3, 0x00A9_A2BE),
        AppTheme::Carmilla => dark(0x0027_1F28, 0x003A_2F3D, 0x00F3_F1F3, 0x00B6_9EBB),
        AppTheme::Whitby => dark(0x0028_1F25, 0x003D_2F38, 0x00F3_F1F3, 0x00BB_9DB1),
        AppTheme::Vesper => dark(0x0028_1F21, 0x003D_2F32, 0x00F3_F1F2, 0x00BC_9FA6),
        AppTheme::Parchment => light(0x00F4_F3F1, 0x00E4_E2DC, 0x001C_1A17, 0x0062_5842),
        AppTheme::Folio => light(0x00F1_F2F4, 0x00DC_DFE4, 0x0017_191C, 0x004D_5973),
        // Green moved to viridian.
        AppTheme::Wormwood => ThemeColors {
            green: hex(0x0013_C282),
            ..dark(0x001F_2824, 0x002F_3D36, 0x00F1_F3F2, 0x008A_AD9C)
        },
        // Orange moved to amber.
        AppTheme::Sconce => ThemeColors {
            orange: hex(0x00DC_9316),
            ..dark(0x0028_231F, 0x003D_342F, 0x00F3_F2F1, 0x00B5_A395)
        },
        AppTheme::HighContrast => HIGH_CONTRAST,
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
}
