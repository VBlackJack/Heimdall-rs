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

//! The design tokens of the window: text sizes, corner radii, spacing, icon sizes and
//! borders, each the value of the C# key it is named after in `Themes/CommonControls.xaml`.
//! A view reads its sizes here, so the two applications keep one scale.
//!
//! Sizes are in logical pixels, as WPF's device-independent pixels are.

/// Text sizes, as the C# `FontSize*` keys.
pub mod font_size {
    /// `FontSizeBadge`: a count or a tag in a badge.
    pub const BADGE: f32 = 10.0;
    /// `FontSizeSmallCaption`: the smallest caption, a protocol tag or a note.
    pub const SMALL_CAPTION: f32 = 11.0;
    /// `FontSizeCaption`: secondary text under or beside a value.
    pub const CAPTION: f32 = 12.0;
    /// `FontSizeBody`: the window's text, its default size.
    pub const BODY: f32 = 13.0;
    /// `FontSizeBodyLarge`: text set slightly apart, a menu's entries.
    pub const BODY_LARGE: f32 = 14.0;
    /// `FontSizeSubtitle`: a section's or a group's title.
    pub const SUBTITLE: f32 = 15.0;
    /// `FontSizeLarge`: a panel's title, the window's name in its bar.
    pub const LARGE: f32 = 17.0;
    /// `FontSizeTitle`: a page's or a dialog's heading.
    pub const TITLE: f32 = 20.0;
    /// `FontSizeDisplay`: the name a panel is about.
    pub const DISPLAY: f32 = 22.0;
    /// `FontSizeHeadline`: the largest heading.
    pub const HEADLINE: f32 = 24.0;
}

/// Corner radii, as the C# `CornerRadius*` keys.
pub mod radius {
    /// `CornerRadiusXs`: a swatch, a small mark.
    pub const XS: f32 = 2.0;
    /// `CornerRadiusSm`: a badge, a check box, a highlight outline.
    pub const SM: f32 = 4.0;
    /// `CornerRadiusMd`: a button, a field, a list, a card.
    pub const MD: f32 = 8.0;
    /// `CornerRadiusLg`: a drop-down's open list.
    pub const LG: f32 = 10.0;
    /// `CornerRadiusXl`: a dialog.
    pub const XL: f32 = 12.0;
}

/// Spacing between and around elements, as the C# `Spacing*` keys.
pub mod spacing {
    /// `SpacingXs`: between the parts of one element.
    pub const XS: f32 = 4.0;
    /// `SpacingSm`, and `SpacingRowGap`: between stacked elements and rows.
    pub const SM: f32 = 8.0;
    /// `SpacingMd`: inside a panel or a card.
    pub const MD: f32 = 12.0;
    /// `SpacingLg`: around a page's content.
    pub const LG: f32 = 20.0;
    /// `SpacingXl`: around an empty state.
    pub const XL: f32 = 24.0;
}

/// Icon sizes, as the C# `IconSize*` keys.
pub mod icon_size {
    /// `IconSizeHero`: the icon a page is about, the About page's application icon.
    pub const HERO: f32 = 48.0;
}

/// Width of a control's border: a button's, a field's, a card's, as every C# style sets it.
pub const BORDER_WIDTH: f32 = 1.0;

/// Width of a control's border in high contrast: every control outlined, wider.
pub const HIGH_CONTRAST_BORDER_WIDTH: f32 = 2.0;

/// Width of the border of the field the keyboard is in, in every theme: seen at a glance.
pub const FOCUS_BORDER_WIDTH: f32 = 2.0;

/// Blur of a dialog's drop shadow, as the C# popups' `DropShadowEffect` (`MainWindow.xaml:6107`).
pub const DIALOG_SHADOW_BLUR: f32 = 24.0;

/// How far a dialog's shadow falls, straight down, as the C#'s `ShadowDepth`.
pub const DIALOG_SHADOW_DEPTH: f32 = 8.0;

/// Opacity of a dialog's black shadow, as the C#'s.
pub const DIALOG_SHADOW_ALPHA: f32 = 0.4;

/// Opacity of a control that cannot be used, as the C# `OpacityDisabled`.
pub const OPACITY_DISABLED: f32 = 0.6;

/// How much lighter the accent is under the pointer, and darker pressed, as `ThemeForge`'s
/// accent tint moves its lightness.
pub const ACCENT_SHIFT: f32 = 0.08;

/// Padding inside a text field, vertical then horizontal, as the C# `ThemedTextBoxStyle`'s
/// `10,6`: with the 13 px text, a field about 33 px high instead of iced's default 5 px
/// all round.
pub const INPUT_PADDING: [f32; 2] = [6.0, 10.0];

/// Padding of a field in a tool's page, as the C# `PaddingInput` token (`8,6`).
pub const TOOL_INPUT_PADDING: [f32; 2] = [6.0, 8.0];

/// Padding of a button that carries an action, as the C# `PrimaryButtonStyle`,
/// `SecondaryButtonStyle` and `QuietButtonStyle`: `16,8`.
pub const BUTTON_PADDING: [f32; 2] = [8.0, 16.0];

/// Padding of a small action beside a field or in a dense row, as the C# `PaddingButtonCopy`
/// token (`10,4`): the caption-sized buttons of a toolbar or an editor.
pub const BUTTON_COMPACT_PADDING: [f32; 2] = [4.0, 10.0];

/// Height of a text field, as measured in the C# window (`ThemedTextBoxStyle`, 13 px text):
/// about 33 px.
pub const INPUT_HEIGHT: f32 = 33.0;

/// Height of the line of text inside a field: the field's height less the vertical
/// [`INPUT_PADDING`] twice. iced's default line (1.3 times the text) left a field 29 px high.
pub const INPUT_LINE_HEIGHT: f32 = INPUT_HEIGHT - 2.0 * INPUT_PADDING[0];

/// Padding inside a status bar pill, vertical then horizontal, as the C#
/// `StatusBarPillButtonStyle`'s `8,2`.
pub const STATUS_PILL_PADDING: [f32; 2] = [2.0, 8.0];

/// Height of a status bar pill, as the C# `SessionHeaderMinTarget` it takes for its
/// `MinHeight`: the 24 px target.
pub const STATUS_PILL_HEIGHT: f32 = 24.0;

/// Room above and below a status bar pill, as the C# pills' `3` margins.
pub const STATUS_PILL_MARGIN: f32 = 3.0;

/// Room before a status bar pill, as the C# pills' `6` right margin.
pub const STATUS_PILL_GAP: f32 = 6.0;
