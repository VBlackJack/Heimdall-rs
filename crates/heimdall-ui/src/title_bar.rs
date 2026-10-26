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

//! The windows' title bars in the theme's colours, as the C# `WindowThemeHelper` gives every
//! window: the dark flag of a dark theme, the caption in the colour of the window's
//! background and the title in the primary text colour. The window manager call itself is
//! in `heimdall-titlebar`; here a theme becomes its style and a window's handle is found.

use heimdall_core::settings::AppTheme;
use heimdall_titlebar::{Rgb, Style};
use iced::window::raw_window_handle::RawWindowHandle;
use iced::{Color, Task, window};

/// The title bar of the theme `theme`: its background for the caption (`SurfaceBrush`), its
/// text for the title (`TextPrimaryBrush`).
#[must_use]
pub fn style_of(theme: AppTheme) -> Style {
    let colors = crate::themes::colors(theme);
    heimdall_titlebar::style(rgb(colors.background), rgb(colors.foreground))
}

/// A colour as bytes.
fn rgb(color: Color) -> Rgb {
    let [red, green, blue, _] = color.into_rgba8();
    [red, green, blue]
}

/// The title bar of the window `id` set to `style`. A window that is not open, or is no Win32
/// one, is left as it is.
pub fn apply<T: Send + 'static>(id: window::Id, style: Style) -> Task<T> {
    window::run(id, move |window| {
        if let Ok(RawWindowHandle::Win32(handle)) =
            window.window_handle().map(|handle| handle.as_raw())
        {
            heimdall_titlebar::apply(handle.hwnd.get(), style);
        }
    })
    .discard()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dracula_s_bar_is_dark_and_in_its_background() {
        let style = style_of(AppTheme::Dracula);
        assert!(style.dark);
        // #282A36 as 0x00BBGGRR.
        assert_eq!(style.caption, 0x0036_2A28);
        // #F8F8F2.
        assert_eq!(style.text, 0x00F2_F8F8);
    }

    #[test]
    fn the_dark_flag_follows_each_theme_s_background() {
        for theme in AppTheme::ALL {
            let colors = crate::themes::colors(theme);
            let [red, green, blue] = rgb(colors.background);
            let style = style_of(theme);
            assert_eq!(style.dark, colors_are_dark(red, green, blue), "{theme:?}");
            assert_eq!(
                style.caption,
                heimdall_titlebar::color_ref([red, green, blue])
            );
        }
    }

    fn colors_are_dark(red: u8, green: u8, blue: u8) -> bool {
        heimdall_titlebar::is_dark([red, green, blue])
    }
}
