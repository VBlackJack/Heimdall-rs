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

//! The look of the window's controls, as the C# styles of `Themes/CommonControls.xaml` draw
//! them: buttons in three weights and a quiet one, fields, drop-downs and check boxes, all
//! rounded, outlined in one pixel and lit in the accent where the C# lights them.
//!
//! Colours are read from the theme, under the names the C# brushes take: the accent is the
//! primary colour, a card the weak background, the secondary text the secondary colour.

use iced::border::Radius;
use iced::overlay::menu::Style as MenuStyle;
use iced::theme::palette::{self, Extended};
use iced::widget::button::{Status as ButtonStatus, Style as ButtonStyle};
use iced::widget::checkbox::{Status as CheckStatus, Style as CheckStyle};
use iced::widget::container::Style as BoxStyle;
use iced::widget::pick_list::{Status as ListStatus, Style as ListStyle};
use iced::widget::text_input::{Status as FieldStatus, Style as FieldStyle};
use iced::{Background, Border, Color, Font, Shadow, Theme, font};

use crate::tokens::{ACCENT_SHIFT, BORDER_WIDTH, OPACITY_DISABLED, radius};

/// The window's font, semi-bold, as the C#'s `FontWeight="SemiBold"`: a selected tab's
/// name, a session's title above it.
pub const SEMIBOLD: Font = Font {
    weight: font::Weight::Semibold,
    ..crate::UI_FONT
};

/// Opacity of the C# `BroadcastActiveBrush`: the red behind broadcast input while it is on.
const BROADCAST_ALPHA: f32 = 0.27;

/// Opacity of a tab's close button at rest, as the C# tab's: full on the tab selected or
/// under the pointer.
pub const CLOSE_REST_OPACITY: f32 = 0.3;

/// Radius of a tab's top corners, as the C# `ThemedTabItemStyle`'s `8,8,0,0`.
const TAB_RADIUS: Radius = Radius {
    top_left: radius::MD,
    top_right: radius::MD,
    bottom_right: 0.0,
    bottom_left: 0.0,
};

/// Radius of the bar under a selected tab, as the C#'s.
const UNDERLINE_RADIUS: f32 = 1.0;

/// The C# brushes a control is drawn with, read from the theme.
struct Brushes {
    /// `AccentBrush`.
    accent: Color,
    /// `AccentHoverBrush`: the accent, lighter.
    accent_hover: Color,
    /// `AccentPressedBrush`: the accent, darker.
    accent_pressed: Color,
    /// `TextOnAccentBrush`: the window's background, on the accent.
    on_accent: Color,
    /// `CardBrush`, which `HighlightBrush` shares.
    card: Color,
    /// `SurfaceBrush`: the window's background.
    surface: Color,
    /// `TextPrimaryBrush`.
    text: Color,
    /// `TextSecondaryBrush`, which `InputBorderBrush` shares.
    secondary: Color,
    /// `ErrorBrush`.
    danger: Color,
}

impl Brushes {
    /// The brushes of `theme`.
    fn of(theme: &Theme) -> Self {
        let extended: &Extended = theme.extended_palette();
        let accent = extended.primary.base.color;
        Self {
            accent,
            accent_hover: palette::lighten(accent, ACCENT_SHIFT),
            accent_pressed: palette::darken(accent, ACCENT_SHIFT),
            on_accent: extended.background.base.color,
            card: extended.background.weak.color,
            surface: extended.background.base.color,
            text: extended.background.base.text,
            secondary: extended.secondary.base.color,
            danger: extended.danger.base.color,
        }
    }
}

/// A one-pixel border of `color`, rounded by `corner`.
fn outline(color: Color, corner: f32) -> Border {
    Border {
        color,
        width: BORDER_WIDTH,
        radius: corner.into(),
    }
}

/// A button's look at rest: `background`, `text` and a border of `edge`.
fn button_style(background: Option<Color>, text: Color, edge: Color) -> ButtonStyle {
    ButtonStyle {
        background: background.map(Background::Color),
        text_color: text,
        border: outline(edge, radius::MD),
        shadow: Shadow::default(),
        snap: true,
    }
}

/// `style` faded, as a disabled C# control is.
fn faded(style: &ButtonStyle) -> ButtonStyle {
    faded_to(style, OPACITY_DISABLED)
}

/// `style` at `opacity`.
fn faded_to(style: &ButtonStyle, opacity: f32) -> ButtonStyle {
    ButtonStyle {
        background: style
            .background
            .map(|background| background.scale_alpha(opacity)),
        text_color: style.text_color.scale_alpha(opacity),
        border: Border {
            color: style.border.color.scale_alpha(opacity),
            ..style.border
        },
        ..*style
    }
}

/// The main action, filled with the accent, as the C# `PrimaryButtonStyle`: Connect, Save,
/// OK.
pub fn primary(theme: &Theme, status: ButtonStatus) -> ButtonStyle {
    let brushes = Brushes::of(theme);
    let filled = |color| button_style(Some(color), brushes.on_accent, color);
    match status {
        ButtonStatus::Active => filled(brushes.accent),
        ButtonStatus::Hovered => filled(brushes.accent_hover),
        ButtonStatus::Pressed => filled(brushes.accent_pressed),
        ButtonStatus::Disabled => faded(&filled(brushes.accent)),
    }
}

/// A complementary action, a card outlined in the secondary text, as the C#
/// `SecondaryButtonStyle`: Edit, Cancel, Later. The accent outlines it under the pointer.
pub fn secondary(theme: &Theme, status: ButtonStatus) -> ButtonStyle {
    let brushes = Brushes::of(theme);
    let rest = button_style(Some(brushes.card), brushes.text, brushes.secondary);
    match status {
        ButtonStatus::Active => rest,
        ButtonStatus::Hovered => ButtonStyle {
            border: outline(brushes.accent, radius::MD),
            ..rest
        },
        ButtonStatus::Pressed => ButtonStyle {
            background: Some(Background::Color(brushes.surface)),
            ..rest
        },
        ButtonStatus::Disabled => faded(&rest),
    }
}

/// An action that destroys, a secondary button whose text is red, as the C# Delete; the red
/// outlines it under the pointer.
pub fn danger(theme: &Theme, status: ButtonStatus) -> ButtonStyle {
    let brushes = Brushes::of(theme);
    let rest = button_style(Some(brushes.card), brushes.danger, brushes.secondary);
    match status {
        ButtonStatus::Active => rest,
        ButtonStatus::Hovered => ButtonStyle {
            border: outline(brushes.danger, radius::MD),
            ..rest
        },
        ButtonStatus::Pressed => ButtonStyle {
            background: Some(Background::Color(brushes.surface)),
            ..rest
        },
        ButtonStatus::Disabled => faded(&rest),
    }
}

/// An action that keeps quiet, text alone until the pointer is on it, as the C#
/// `GhostButtonStyle`: a link, a close mark, a toolbar's entry.
pub fn subtle(theme: &Theme, status: ButtonStatus) -> ButtonStyle {
    let brushes = Brushes::of(theme);
    let rest = button_style(None, brushes.text, Color::TRANSPARENT);
    match status {
        ButtonStatus::Active => rest,
        ButtonStatus::Hovered => ButtonStyle {
            background: Some(Background::Color(brushes.card)),
            ..rest
        },
        ButtonStatus::Pressed => ButtonStyle {
            background: Some(Background::Color(brushes.card)),
            border: outline(brushes.secondary, radius::MD),
            ..rest
        },
        ButtonStatus::Disabled => faded(&rest),
    }
}

/// A tab's close button, as the C# tab's: a secondary button, faded while `quiet`, on a
/// tab neither selected nor under the pointer.
pub fn close_mark(quiet: bool) -> impl Fn(&Theme, ButtonStatus) -> ButtonStyle {
    move |theme, status| {
        let style = secondary(theme, status);
        if quiet && status == ButtonStatus::Active {
            faded_to(&style, CLOSE_REST_OPACITY)
        } else {
            style
        }
    }
}

/// Broadcast input's button while it is on, as the C# status bar's: the red faint behind
/// it, its text red.
pub fn broadcasting(theme: &Theme, status: ButtonStatus) -> ButtonStyle {
    let brushes = Brushes::of(theme);
    let rest = button_style(
        Some(brushes.danger.scale_alpha(BROADCAST_ALPHA)),
        brushes.danger,
        Color::TRANSPARENT,
    );
    match status {
        ButtonStatus::Active | ButtonStatus::Hovered => rest,
        ButtonStatus::Pressed => ButtonStyle {
            border: outline(brushes.danger, radius::MD),
            ..rest
        },
        ButtonStatus::Disabled => faded(&rest),
    }
}

/// A session's tab, as the C# `ThemedTabItemStyle`: nothing behind it but under the
/// pointer, a card once `selected`, rounded at the top; its text secondary until then.
pub fn tab(selected: bool) -> impl Fn(&Theme, ButtonStatus) -> ButtonStyle {
    move |theme, status| {
        let brushes = Brushes::of(theme);
        let lit = selected || matches!(status, ButtonStatus::Hovered | ButtonStatus::Pressed);
        ButtonStyle {
            background: lit.then_some(Background::Color(brushes.card)),
            text_color: if lit { brushes.text } else { brushes.secondary },
            border: Border {
                radius: TAB_RADIUS,
                ..Border::default()
            },
            shadow: Shadow::default(),
            snap: true,
        }
    }
}

/// The bar under a selected tab, in the accent, as the C#'s; `lit` false, it keeps its
/// place unseen.
pub fn underline(lit: bool) -> impl Fn(&Theme) -> BoxStyle {
    move |theme| BoxStyle {
        background: lit.then(|| Background::Color(Brushes::of(theme).accent)),
        border: Border {
            radius: UNDERLINE_RADIUS.into(),
            ..Border::default()
        },
        ..BoxStyle::default()
    }
}

/// A strip of a session's chrome, as the C# pane header: a card from edge to edge.
pub fn strip(theme: &Theme) -> BoxStyle {
    BoxStyle {
        background: Some(Background::Color(Brushes::of(theme).card)),
        ..BoxStyle::default()
    }
}

/// A small round mark in the accent, as the C# dot on the filter button while a filter
/// leaves sessions out.
pub fn accent_dot(theme: &Theme) -> BoxStyle {
    BoxStyle {
        background: Some(Background::Color(Brushes::of(theme).accent)),
        border: Border {
            radius: radius::LG.into(),
            ..Border::default()
        },
        ..BoxStyle::default()
    }
}

/// A text field, as the C# `ThemedTextBoxStyle`: a card outlined in the secondary text, in
/// the text colour under the pointer and in the accent while typed in.
pub fn text_input(theme: &Theme, status: FieldStatus) -> FieldStyle {
    let brushes = Brushes::of(theme);
    let selection = theme.extended_palette().primary.weak.color;
    let rest = FieldStyle {
        background: Background::Color(brushes.card),
        border: outline(brushes.secondary, radius::MD),
        icon: brushes.secondary,
        placeholder: brushes.secondary,
        value: brushes.text,
        selection,
    };
    match status {
        FieldStatus::Active => rest,
        FieldStatus::Hovered => FieldStyle {
            border: outline(brushes.text, radius::MD),
            ..rest
        },
        FieldStatus::Focused { .. } => FieldStyle {
            border: outline(brushes.accent, radius::MD),
            ..rest
        },
        FieldStatus::Disabled => FieldStyle {
            background: Background::Color(brushes.card.scale_alpha(OPACITY_DISABLED)),
            border: outline(brushes.secondary.scale_alpha(OPACITY_DISABLED), radius::MD),
            value: brushes.text.scale_alpha(OPACITY_DISABLED),
            ..rest
        },
    }
}

/// A drop-down, as the C# `ThemedComboBoxStyle`: outlined as a field, in the accent while
/// its list is open.
pub fn pick_list(theme: &Theme, status: ListStatus) -> ListStyle {
    let brushes = Brushes::of(theme);
    let rest = ListStyle {
        text_color: brushes.text,
        placeholder_color: brushes.secondary,
        handle_color: brushes.text,
        background: Background::Color(brushes.card),
        border: outline(brushes.secondary, radius::MD),
    };
    match status {
        ListStatus::Active => rest,
        ListStatus::Hovered => ListStyle {
            border: outline(brushes.text, radius::MD),
            ..rest
        },
        ListStatus::Opened { .. } => ListStyle {
            border: outline(brushes.accent, radius::MD),
            ..rest
        },
    }
}

/// A drop-down's open list, as the C# combo box's popup: a card outlined in the accent, the
/// entry under the pointer lit.
pub fn menu(theme: &Theme) -> MenuStyle {
    let brushes = Brushes::of(theme);
    let lit = theme.extended_palette().primary.strong;
    MenuStyle {
        background: Background::Color(brushes.card),
        border: outline(brushes.accent, radius::LG),
        text_color: brushes.text,
        selected_text_color: lit.text,
        selected_background: Background::Color(lit.color),
        shadow: Shadow::default(),
    }
}

/// A check box, as the C# `ThemedCheckBoxStyle`: a card outlined in the secondary text,
/// filled with the accent once checked, outlined in the accent under the pointer.
pub fn checkbox(theme: &Theme, status: CheckStatus) -> CheckStyle {
    let brushes = Brushes::of(theme);
    let (is_checked, edge, fade) = match status {
        CheckStatus::Active { is_checked } => (is_checked, brushes.secondary, 1.0),
        CheckStatus::Hovered { is_checked } => (is_checked, brushes.accent, 1.0),
        CheckStatus::Disabled { is_checked } => (is_checked, brushes.secondary, OPACITY_DISABLED),
    };
    let (fill, edge) = if is_checked {
        (brushes.accent, brushes.accent)
    } else {
        (brushes.card, edge)
    };
    CheckStyle {
        background: Background::Color(fill.scale_alpha(fade)),
        icon_color: brushes.on_accent,
        border: outline(edge.scale_alpha(fade), radius::SM),
        text_color: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use heimdall_core::settings::{Accent, AppTheme};

    use crate::themes;

    /// Magellan tinted blue, the theme the C# screens were taken in.
    fn magellan() -> Theme {
        themes::theme(AppTheme::Magellan, Accent::Blue)
    }

    #[test]
    fn a_primary_button_is_the_accent_with_the_background_as_its_text() {
        let theme = magellan();
        let colors = themes::colors(AppTheme::Magellan);
        let style = primary(&theme, ButtonStatus::Active);
        assert_eq!(
            style.background,
            Some(Background::Color(colors.accent(Accent::Blue)))
        );
        assert_eq!(style.text_color, colors.background);
        assert!((style.border.radius.top_left - radius::MD).abs() < f32::EPSILON);
        let hovered = primary(&theme, ButtonStatus::Hovered);
        assert_ne!(
            hovered.background, style.background,
            "lit under the pointer"
        );
    }

    #[test]
    fn secondary_and_danger_are_cards_outlined_in_the_secondary_text() {
        let theme = magellan();
        let colors = themes::colors(AppTheme::Magellan);
        for style in [
            secondary(&theme, ButtonStatus::Active),
            danger(&theme, ButtonStatus::Active),
        ] {
            assert_eq!(
                style.background,
                Some(Background::Color(colors.current_line))
            );
            assert_eq!(style.border.color, colors.comment);
            assert!((style.border.width - BORDER_WIDTH).abs() < f32::EPSILON);
        }
        assert_eq!(
            danger(&theme, ButtonStatus::Active).text_color,
            colors.red,
            "Delete is written in red"
        );
    }

    #[test]
    fn a_field_is_outlined_in_the_accent_while_typed_in() {
        let theme = magellan();
        let colors = themes::colors(AppTheme::Magellan);
        let typed = text_input(&theme, FieldStatus::Focused { is_hovered: false });
        assert_eq!(typed.border.color, colors.accent(Accent::Blue));
        let rest = text_input(&theme, FieldStatus::Active);
        assert_eq!(rest.border.color, colors.comment);
    }

    #[test]
    fn a_disabled_button_is_faded() {
        let theme = magellan();
        let style = secondary(&theme, ButtonStatus::Disabled);
        assert!(style.text_color.a < 1.0);
    }
}
