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

//! The design tokens of the window: text sizes, corner radii, spacing and borders, each
//! the value of the C# key it is named after in `Themes/CommonControls.xaml`. A view reads
//! its sizes here, so the two applications keep one scale.
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

/// Width of a control's border: a button's, a field's, a card's, as every C# style sets it.
pub const BORDER_WIDTH: f32 = 1.0;

/// Width of a control's border in high contrast: every control outlined, wider.
pub const HIGH_CONTRAST_BORDER_WIDTH: f32 = 2.0;

/// Width of the border of the field the keyboard is in, in every theme: seen at a glance.
pub const FOCUS_BORDER_WIDTH: f32 = 2.0;

/// Opacity of a control that cannot be used, as the C# `OpacityDisabled`.
pub const OPACITY_DISABLED: f32 = 0.6;

/// How much lighter the accent is under the pointer, and darker pressed, as `ThemeForge`'s
/// accent tint moves its lightness.
pub const ACCENT_SHIFT: f32 = 0.08;
