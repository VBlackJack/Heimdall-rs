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

//! A window's title bar drawn in the theme's colours, as the C# `WindowThemeHelper`
//! (`Theming/WindowThemeHelper.cs`) does: the immersive dark mode flag (attribute 20, and 19
//! of the older builds) for a dark theme, the caption in the colour of the window's
//! background (`SurfaceBrush`, attribute 35) and its text in the primary text colour
//! (`TextPrimaryBrush`, attribute 36). No border colour is set, as in the C#.
//!
//! On Windows the desktop window manager does it, through one function declared by hand in
//! one module, the only unsafe code here. Elsewhere there is no such title bar and applying
//! one does nothing. A Windows that does not know an attribute ignores it and keeps its
//! default bar, as the C# ignores the result.

#[cfg(windows)]
#[expect(
    unsafe_code,
    reason = "the window manager's attribute call is a foreign function"
)]
mod dwm;

/// A window handle (`HWND`) as the number the window system gives it.
pub type Hwnd = isize;

/// A colour as red, green and blue, each from 0 to 255.
pub type Rgb = [u8; 3];

/// `DWMWA_USE_IMMERSIVE_DARK_MODE`.
#[cfg(any(windows, test))]
const ATTR_DARK_MODE: u32 = 20;
/// The same flag before Windows 10 build 18985.
#[cfg(any(windows, test))]
const ATTR_DARK_MODE_LEGACY: u32 = 19;
/// `DWMWA_CAPTION_COLOR`.
#[cfg(any(windows, test))]
const ATTR_CAPTION_COLOR: u32 = 35;
/// `DWMWA_TEXT_COLOR`.
#[cfg(any(windows, test))]
const ATTR_TEXT_COLOR: u32 = 36;

/// Perceived luminance, from 0 to 1, below which a background counts as dark: the C#
/// `ThemeLuminance.DarkThreshold`.
const DARK_THRESHOLD: f64 = 0.5;
/// Rec. 601 weight of red, as the C# `ThemeLuminance.IsDark`.
const LUMA_RED: f64 = 0.299;
/// Rec. 601 weight of green.
const LUMA_GREEN: f64 = 0.587;
/// Rec. 601 weight of blue.
const LUMA_BLUE: f64 = 0.114;
/// Highest value of a colour channel.
const CHANNEL_MAX: f64 = 255.0;
/// Bits a green channel is shifted by in a `COLORREF`.
const GREEN_SHIFT: u32 = 8;
/// Bits a blue channel is shifted by in a `COLORREF`.
const BLUE_SHIFT: u32 = 16;

/// What a title bar is set to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Style {
    /// Whether the bar uses the dark mode of the system's own controls.
    pub dark: bool,
    /// The caption's colour, as a `COLORREF` (`0x00BBGGRR`).
    pub caption: u32,
    /// The title's colour, as a `COLORREF`.
    pub text: u32,
}

/// Whether `background` is a dark colour, by its perceived (Rec. 601) luminance.
#[must_use]
pub fn is_dark(background: Rgb) -> bool {
    let [red, green, blue] = background;
    let luma =
        LUMA_RED * f64::from(red) + LUMA_GREEN * f64::from(green) + LUMA_BLUE * f64::from(blue);
    luma / CHANNEL_MAX < DARK_THRESHOLD
}

/// A colour as the `COLORREF` the window manager expects: `0x00BBGGRR`.
#[must_use]
pub fn color_ref(color: Rgb) -> u32 {
    let [red, green, blue] = color;
    u32::from(red) | (u32::from(green) << GREEN_SHIFT) | (u32::from(blue) << BLUE_SHIFT)
}

/// The title bar of a theme whose window background is `surface` and whose text is `text`:
/// dark when the background is, the caption the background's colour, the title the text's.
#[must_use]
pub fn style(surface: Rgb, text: Rgb) -> Style {
    Style {
        dark: is_dark(surface),
        caption: color_ref(surface),
        text: color_ref(text),
    }
}

/// Whether this system draws a title bar in the theme's colours.
pub const SUPPORTED: bool = cfg!(windows);

/// Sets the title bar of the window `hwnd` to `style`; failures are ignored. Does nothing
/// outside Windows, nor for a null handle.
pub fn apply(hwnd: Hwnd, style: Style) {
    #[cfg(windows)]
    if hwnd != 0 {
        dwm::apply(hwnd, style);
    }
    #[cfg(not(windows))]
    let _ = (hwnd, style);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dark_background_gets_the_dark_bar_and_a_light_one_does_not() {
        assert!(is_dark([0x28, 0x2A, 0x36]));
        assert!(is_dark([0, 0, 0]));
        assert!(!is_dark([0xFF, 0xFF, 0xFF]));
        assert!(!is_dark([0xF5, 0xF5, 0xF5]));
    }

    #[test]
    fn the_threshold_is_perceived_luminance_not_the_mean() {
        // Pure green is 58.7 % luminous: light. Pure blue is 11.4 %: dark.
        assert!(!is_dark([0, 0xFF, 0]));
        assert!(is_dark([0, 0, 0xFF]));
        // Either side of one half: 127 gives 0.4980, 128 gives 0.5020.
        assert!(is_dark([127, 127, 127]));
        assert!(!is_dark([128, 128, 128]));
    }

    #[test]
    fn a_colour_ref_is_blue_green_red() {
        assert_eq!(color_ref([0x11, 0x22, 0x33]), 0x0033_2211);
        assert_eq!(color_ref([0xFF, 0, 0]), 0x0000_00FF);
        assert_eq!(color_ref([0, 0, 0xFF]), 0x00FF_0000);
    }

    #[test]
    fn the_style_takes_the_surface_for_the_caption_and_the_text_for_the_title() {
        // Dracula: background #282A36, text #F8F8F2.
        assert_eq!(
            style([0x28, 0x2A, 0x36], [0xF8, 0xF8, 0xF2]),
            Style {
                dark: true,
                caption: 0x0036_2A28,
                text: 0x00F2_F8F8,
            }
        );
        // A light theme: no dark flag.
        let light = style([0xFF, 0xFF, 0xFF], [0x20, 0x20, 0x20]);
        assert!(!light.dark);
        assert_eq!(light.caption, 0x00FF_FFFF);
    }

    #[test]
    fn the_attributes_are_the_c_ones() {
        assert_eq!(
            [
                ATTR_DARK_MODE,
                ATTR_DARK_MODE_LEGACY,
                ATTR_CAPTION_COLOR,
                ATTR_TEXT_COLOR
            ],
            [20, 19, 35, 36]
        );
    }

    #[test]
    fn a_null_handle_is_left_alone() {
        apply(0, style([0, 0, 0], [255, 255, 255]));
    }

    /// The `[lints]` table of a manifest; the workspace's own when `workspace`.
    fn lints(manifest: &str, workspace: bool) -> toml::Table {
        let table: toml::Table = manifest.parse().expect("a manifest");
        let holder = if workspace {
            table["workspace"].as_table().expect("a workspace").clone()
        } else {
            table
        };
        holder["lints"].as_table().expect("a lints table").clone()
    }

    #[test]
    fn the_lints_are_the_workspace_s_but_for_unsafe_code() {
        let workspace = lints(include_str!("../../../Cargo.toml"), true);
        let mut own = lints(include_str!("../Cargo.toml"), false);
        let mut expected = workspace;
        // Denied rather than forbidden, so that the window manager's module can allow it.
        assert_eq!(
            expected["rust"]["unsafe_code"].as_str(),
            Some("forbid"),
            "the workspace forbids unsafe code"
        );
        expected["rust"]
            .as_table_mut()
            .expect("rust lints")
            .insert("unsafe_code".to_owned(), "deny".into());
        // Every unsafe block explained.
        let added = own["clippy"]
            .as_table_mut()
            .expect("clippy lints")
            .remove("undocumented_unsafe_blocks");
        assert_eq!(added.as_ref().and_then(toml::Value::as_str), Some("deny"));
        assert_eq!(
            own, expected,
            "a lint added to the workspace is added here too"
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn elsewhere_than_windows_nothing_is_drawn() {
        const { assert!(!SUPPORTED) };
        apply(1, style([0, 0, 0], [255, 255, 255]));
    }
}
