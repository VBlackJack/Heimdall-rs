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
//! rounded, outlined in one pixel and lit in the accent where the C# lights them. In high
//! contrast every control is outlined, wider, and the field typed in is lit in yellow.
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
use iced::widget::scrollable::{
    Direction, Rail, Scrollable, Scrollbar, Scroller, Status as ScrollStatus, Style as ScrollStyle,
};
use iced::widget::text_editor::{Status as EditorStatus, Style as EditorStyle};
use iced::widget::text_input::{Status as FieldStatus, Style as FieldStyle};
use iced::{Background, Border, Color, Element, Font, Shadow, Theme, font};

use crate::tokens::{
    ACCENT_SHIFT, BORDER_WIDTH, FOCUS_BORDER_WIDTH, HIGH_CONTRAST_BORDER_WIDTH, OPACITY_DISABLED,
    radius,
};

/// Width of a scrollbar's track, as the C# `ThemedScrollBarStyle`'s.
pub const SCROLLBAR_WIDTH: f32 = 12.0;

/// Width of its thumb: the track less the C# thumb's margin of 2 on each side.
pub const SCROLLER_WIDTH: f32 = 8.0;

/// Opacity of a toolbar's separator, as the C# `ToolbarVerticalSeparatorStyle`'s.
const SEPARATOR_OPACITY: f32 = 0.65;

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
    /// `WarningBrush`.
    warning: Color,
    /// `BorderBrush`: a card's edge, as iced's bordered box draws it.
    edge: Color,
    /// The edge of a control drawn without one: none, but in high contrast.
    quiet_edge: Color,
    /// The field the keyboard is in: the accent, yellow in high contrast.
    focus: Color,
    /// Width of every control's border: wider in high contrast.
    width: f32,
}

impl Brushes {
    /// The brushes of `theme`.
    fn of(theme: &Theme) -> Self {
        let extended: &Extended = theme.extended_palette();
        let accent = extended.primary.base.color;
        let high_contrast = crate::themes::is_high_contrast(theme);
        let secondary = extended.secondary.base.color;
        Self {
            accent,
            accent_hover: palette::lighten(accent, ACCENT_SHIFT),
            accent_pressed: palette::darken(accent, ACCENT_SHIFT),
            on_accent: extended.background.base.color,
            card: extended.background.weak.color,
            surface: extended.background.base.color,
            text: extended.background.base.text,
            secondary,
            danger: extended.danger.base.color,
            warning: extended.warning.base.color,
            edge: extended.background.strong.color,
            quiet_edge: if high_contrast {
                secondary
            } else {
                Color::TRANSPARENT
            },
            focus: if high_contrast {
                crate::themes::colors_of(theme).yellow
            } else {
                accent
            },
            width: if high_contrast {
                HIGH_CONTRAST_BORDER_WIDTH
            } else {
                BORDER_WIDTH
            },
        }
    }

    /// A border of `color` as wide as the theme's, rounded by `corner`.
    fn outline(&self, color: Color, corner: f32) -> Border {
        Border {
            color,
            width: self.width,
            radius: corner.into(),
        }
    }

    /// The border of the field the keyboard is in, rounded by `corner`: wider than the
    /// rest in every theme, so it is seen at a glance.
    fn focus_ring(&self, corner: f32) -> Border {
        Border {
            color: self.focus,
            width: self.width.max(FOCUS_BORDER_WIDTH),
            radius: corner.into(),
        }
    }

    /// A button's look at rest: `background`, `text` and a border of `edge`.
    fn button(&self, background: Option<Color>, text: Color, edge: Color) -> ButtonStyle {
        ButtonStyle {
            background: background.map(Background::Color),
            text_color: text,
            border: self.outline(edge, radius::MD),
            shadow: Shadow::default(),
            snap: true,
        }
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
    let filled = |color| brushes.button(Some(color), brushes.on_accent, color);
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
    let rest = brushes.button(Some(brushes.card), brushes.text, brushes.secondary);
    match status {
        ButtonStatus::Active => rest,
        ButtonStatus::Hovered => ButtonStyle {
            border: brushes.outline(brushes.accent, radius::MD),
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
    let rest = brushes.button(Some(brushes.card), brushes.danger, brushes.secondary);
    match status {
        ButtonStatus::Active => rest,
        ButtonStatus::Hovered => ButtonStyle {
            border: brushes.outline(brushes.danger, radius::MD),
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
/// `GhostButtonStyle`: a link, a close mark, a toolbar's entry. Outlined in high contrast.
pub fn subtle(theme: &Theme, status: ButtonStatus) -> ButtonStyle {
    let brushes = Brushes::of(theme);
    let rest = brushes.button(None, brushes.text, brushes.quiet_edge);
    match status {
        ButtonStatus::Active => rest,
        ButtonStatus::Hovered => ButtonStyle {
            background: Some(Background::Color(brushes.card)),
            ..rest
        },
        ButtonStatus::Pressed => ButtonStyle {
            background: Some(Background::Color(brushes.card)),
            border: brushes.outline(brushes.secondary, radius::MD),
            ..rest
        },
        ButtonStatus::Disabled => faded(&rest),
    }
}

/// A line of Quick Connect's results, as the C# palette's `ListBoxItem`: nothing behind it
/// but the highlight under the pointer; once `chosen`, a card outlined in the accent, as the
/// C# selected item with the keyboard on it.
pub fn palette_row(chosen: bool) -> impl Fn(&Theme, ButtonStatus) -> ButtonStyle {
    move |theme, status| {
        let brushes = Brushes::of(theme);
        if chosen {
            let lit = brushes.button(Some(brushes.card), brushes.text, brushes.focus);
            return match status {
                ButtonStatus::Disabled => faded(&lit),
                _ => lit,
            };
        }
        subtle(theme, status)
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
    let rest = brushes.button(
        Some(brushes.danger.scale_alpha(BROADCAST_ALPHA)),
        brushes.danger,
        brushes.quiet_edge,
    );
    match status {
        ButtonStatus::Active | ButtonStatus::Hovered => rest,
        ButtonStatus::Pressed => ButtonStyle {
            border: brushes.outline(brushes.danger, radius::MD),
            ..rest
        },
        ButtonStatus::Disabled => faded(&rest),
    }
}

/// A session's tab, as the C# `ThemedTabItemStyle`: nothing behind it but under the
/// pointer, a card once `selected`, rounded at the top; its text secondary until then.
/// Outlined in high contrast.
pub fn tab(selected: bool) -> impl Fn(&Theme, ButtonStatus) -> ButtonStyle {
    move |theme, status| {
        let brushes = Brushes::of(theme);
        let lit = selected || matches!(status, ButtonStatus::Hovered | ButtonStatus::Pressed);
        ButtonStyle {
            background: lit.then_some(Background::Color(brushes.card)),
            text_color: if lit { brushes.text } else { brushes.secondary },
            border: Border {
                color: brushes.quiet_edge,
                width: brushes.width,
                radius: TAB_RADIUS,
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

/// A segment of the C# connection phase stepper: the accent once its phase is reached, the
/// disabled text before it.
pub fn phase_segment(theme: &Theme, lit: bool) -> BoxStyle {
    let brushes = Brushes::of(theme);
    let colour = if lit {
        brushes.accent
    } else {
        Color {
            a: brushes.secondary.a * crate::tokens::OPACITY_DISABLED,
            ..brushes.secondary
        }
    };
    BoxStyle {
        background: Some(Background::Color(colour)),
        border: Border {
            radius: radius::XS.into(),
            ..Border::default()
        },
        ..BoxStyle::default()
    }
}

/// The C# letterbox hint's badge over a desktop: the border colour, small corners.
pub fn letterbox_hint(theme: &Theme) -> BoxStyle {
    BoxStyle {
        background: Some(Background::Color(Brushes::of(theme).edge)),
        border: Border {
            radius: radius::SM.into(),
            ..Border::default()
        },
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
/// the text colour under the pointer and in the accent while typed in, wider then; yellow
/// in high contrast.
pub fn text_input(theme: &Theme, status: FieldStatus) -> FieldStyle {
    let brushes = Brushes::of(theme);
    let selection = theme.extended_palette().primary.weak.color;
    let rest = FieldStyle {
        background: Background::Color(brushes.card),
        border: brushes.outline(brushes.secondary, radius::MD),
        icon: brushes.secondary,
        placeholder: brushes.secondary,
        value: brushes.text,
        selection,
    };
    match status {
        FieldStatus::Active => rest,
        FieldStatus::Hovered => FieldStyle {
            border: brushes.outline(brushes.text, radius::MD),
            ..rest
        },
        FieldStatus::Focused { .. } => FieldStyle {
            border: brushes.focus_ring(radius::MD),
            ..rest
        },
        FieldStatus::Disabled => FieldStyle {
            background: Background::Color(brushes.card.scale_alpha(OPACITY_DISABLED)),
            border: brushes.outline(brushes.secondary.scale_alpha(OPACITY_DISABLED), radius::MD),
            value: brushes.text.scale_alpha(OPACITY_DISABLED),
            ..rest
        },
    }
}

/// A box of several lines, as the C# `ThemedTextBoxStyle` on a box that takes line breaks:
/// drawn as [`text_input`].
pub fn text_box(theme: &Theme, status: EditorStatus) -> EditorStyle {
    let field = text_input(
        theme,
        match status {
            EditorStatus::Active => FieldStatus::Active,
            EditorStatus::Hovered => FieldStatus::Hovered,
            EditorStatus::Focused { is_hovered } => FieldStatus::Focused { is_hovered },
            EditorStatus::Disabled => FieldStatus::Disabled,
        },
    );
    EditorStyle {
        background: field.background,
        border: field.border,
        placeholder: field.placeholder,
        value: field.value,
        selection: field.selection,
    }
}

/// The colour of a card, as the C# `CardBrush`: a badge's background.
#[must_use]
pub fn card_color(theme: &Theme) -> Color {
    Brushes::of(theme).card
}

/// A mark filled with `color`, rounded by `corner`: a category's dot or bar, a card's icon
/// square, a count's badge.
#[must_use]
pub fn filled(color: Color, corner: f32) -> BoxStyle {
    BoxStyle {
        background: Some(Background::Color(color)),
        border: Border {
            radius: corner.into(),
            ..Border::default()
        },
        ..BoxStyle::default()
    }
}

/// A tool's card on the Tools page, as the C# `CreateToolsTabCard`: a card outlined in the
/// border colour, rounded as `CornerRadiusMd`; under the pointer or pressed, outlined in the
/// accent on the highlight, which the card colour is.
pub fn tool_card(theme: &Theme, status: ButtonStatus) -> ButtonStyle {
    let brushes = Brushes::of(theme);
    let rest = brushes.button(Some(brushes.card), brushes.text, brushes.edge);
    match status {
        ButtonStatus::Active => rest,
        ButtonStatus::Hovered | ButtonStatus::Pressed => ButtonStyle {
            border: brushes.outline(brushes.accent, radius::MD),
            ..rest
        },
        ButtonStatus::Disabled => faded(&rest),
    }
}

/// A tab of the sidebar's "Sessions | Tools", as the C# `SidebarTabStyle`: nothing behind it
/// but the highlight under the pointer; its text secondary until `selected`, then the text's
/// own colour. The accent's line under it is its caller's.
pub fn sidebar_tab(selected: bool) -> impl Fn(&Theme, ButtonStatus) -> ButtonStyle {
    move |theme, status| {
        let brushes = Brushes::of(theme);
        let lit = matches!(status, ButtonStatus::Hovered | ButtonStatus::Pressed);
        ButtonStyle {
            background: lit.then_some(Background::Color(brushes.card)),
            text_color: if selected {
                brushes.text
            } else {
                brushes.secondary
            },
            border: Border {
                color: brushes.quiet_edge,
                width: brushes.width,
                radius: 0.0.into(),
            },
            shadow: Shadow::default(),
            snap: true,
        }
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
        border: brushes.outline(brushes.secondary, radius::MD),
    };
    match status {
        ListStatus::Active => rest,
        ListStatus::Hovered => ListStyle {
            border: brushes.outline(brushes.text, radius::MD),
            ..rest
        },
        ListStatus::Opened { .. } => ListStyle {
            border: brushes.focus_ring(radius::MD),
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
        border: brushes.outline(brushes.accent, radius::LG),
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
        border: brushes.outline(edge.scale_alpha(fade), radius::SM),
        text_color: None,
    }
}

/// A count of errors on a tab's header, as the C# server dialog's badge: the window's
/// background on the error colour, rounded as `CornerRadiusMd`.
pub fn error_badge(theme: &Theme) -> BoxStyle {
    let brushes = Brushes::of(theme);
    BoxStyle {
        text_color: Some(brushes.on_accent),
        background: Some(Background::Color(brushes.danger)),
        border: Border {
            radius: radius::MD.into(),
            ..Border::default()
        },
        ..BoxStyle::default()
    }
}

/// [`checkbox`] labelled in the secondary text, as the C# file browser's "Show hidden" one
/// beside its filter.
pub fn quiet_checkbox(theme: &Theme, status: CheckStatus) -> CheckStyle {
    CheckStyle {
        text_color: Some(Brushes::of(theme).secondary),
        ..checkbox(theme, status)
    }
}

/// A row of a list that draws its own background, as a file list's row over the C#
/// `FileBrowserRowStyle`: the button itself shows nothing but its text.
pub fn bare(theme: &Theme, status: ButtonStatus) -> ButtonStyle {
    let brushes = Brushes::of(theme);
    let rest = brushes.button(None, brushes.text, Color::TRANSPARENT);
    match status {
        ButtonStatus::Disabled => faded(&rest),
        _ => rest,
    }
}

/// A toolbar's toggle, as the C# `ToolbarToggleButtonStyle`: outlined in the secondary text,
/// a card under the pointer; once `on`, a card outlined in the warning colour.
pub fn toggle(on: bool) -> impl Fn(&Theme, ButtonStatus) -> ButtonStyle {
    move |theme, status| {
        let brushes = Brushes::of(theme);
        let rest = if on {
            brushes.button(Some(brushes.card), brushes.text, brushes.warning)
        } else {
            brushes.button(None, brushes.text, brushes.secondary)
        };
        match status {
            ButtonStatus::Active => rest,
            ButtonStatus::Hovered => ButtonStyle {
                background: Some(Background::Color(brushes.card)),
                ..rest
            },
            ButtonStatus::Pressed => ButtonStyle {
                background: Some(Background::Color(brushes.surface)),
                ..rest
            },
            ButtonStatus::Disabled => faded(&rest),
        }
    }
}

/// A card, as the C# Settings cards: the card colour, rounded, its edge the border colour.
pub fn card(theme: &Theme) -> BoxStyle {
    let brushes = Brushes::of(theme);
    BoxStyle {
        background: Some(Background::Color(brushes.card)),
        border: brushes.outline(brushes.edge, radius::LG),
        ..BoxStyle::default()
    }
}

/// A card rounded as `CornerRadiusXl`, as the C# About page's cards
/// (`MainWindow.xaml:4690-4694`): the card colour, its edge the border colour.
pub fn large_card(theme: &Theme) -> BoxStyle {
    let brushes = Brushes::of(theme);
    BoxStyle {
        background: Some(Background::Color(brushes.card)),
        border: brushes.outline(brushes.edge, radius::XL),
        ..BoxStyle::default()
    }
}

/// A line across a card, in the border colour, as the C# About page's under the version
/// (`MainWindow.xaml:4714`).
pub fn rule(theme: &Theme) -> BoxStyle {
    BoxStyle {
        background: Some(Background::Color(Brushes::of(theme).edge)),
        ..BoxStyle::default()
    }
}

/// A dialog, as the C# dialog windows: the window's background, rounded as `CornerRadiusXl`,
/// its edge the border colour.
pub fn dialog(theme: &Theme) -> BoxStyle {
    let brushes = Brushes::of(theme);
    BoxStyle {
        background: Some(Background::Color(brushes.surface)),
        border: brushes.outline(brushes.edge, radius::XL),
        ..BoxStyle::default()
    }
}

/// The C# `ThemedTabControlStyle` frame: a card rounded as `CornerRadiusXl`, outlined in the
/// border colour, its headers on the window's background above it.
pub fn tab_frame(theme: &Theme) -> BoxStyle {
    let brushes = Brushes::of(theme);
    BoxStyle {
        background: Some(Background::Color(brushes.card)),
        border: brushes.outline(brushes.edge, radius::XL),
        ..BoxStyle::default()
    }
}

/// The header strip of a [`tab_frame`]: the window's background, rounded at the top.
pub fn tab_headers(theme: &Theme) -> BoxStyle {
    BoxStyle {
        background: Some(Background::Color(Brushes::of(theme).surface)),
        border: Border {
            radius: Radius {
                top_left: radius::XL,
                top_right: radius::XL,
                bottom_right: 0.0,
                bottom_left: 0.0,
            },
            ..Border::default()
        },
        ..BoxStyle::default()
    }
}

/// A box drawn as a field, as the C# breadcrumb takes the path box's look: a card outlined
/// in the secondary text.
pub fn field_box(theme: &Theme) -> BoxStyle {
    let brushes = Brushes::of(theme);
    BoxStyle {
        background: Some(Background::Color(brushes.card)),
        border: brushes.outline(brushes.secondary, radius::MD),
        ..BoxStyle::default()
    }
}

/// The C# "Modified" badge of a setting: the window's background outlined in the accent.
pub fn badge(theme: &Theme) -> BoxStyle {
    let brushes = Brushes::of(theme);
    BoxStyle {
        background: Some(Background::Color(brushes.surface)),
        border: brushes.outline(brushes.accent, radius::SM),
        ..BoxStyle::default()
    }
}

/// The C# security notice of a file browser (`EmbeddedSftpView.xaml:161-186`): the
/// window's background outlined in the warning colour.
pub fn warning_badge(theme: &Theme) -> BoxStyle {
    let brushes = Brushes::of(theme);
    BoxStyle {
        background: Some(Background::Color(brushes.surface)),
        border: brushes.outline(brushes.warning, radius::SM),
        ..BoxStyle::default()
    }
}

/// A line between two parts, as the C# `BorderBrush` edges of a toolbar; faded as the C#
/// toolbar's separator.
pub fn divider(theme: &Theme) -> BoxStyle {
    BoxStyle {
        background: Some(Background::Color(
            Brushes::of(theme).edge.scale_alpha(SEPARATOR_OPACITY),
        )),
        ..BoxStyle::default()
    }
}

/// A scrollbar as the C#'s: a track as wide as [`SCROLLBAR_WIDTH`], its thumb
/// [`SCROLLER_WIDTH`] in the middle of it.
#[must_use]
pub fn scrollbar() -> Scrollbar {
    Scrollbar::new()
        .width(SCROLLBAR_WIDTH)
        .scroller_width(SCROLLER_WIDTH)
}

/// `content` scrolled down, its scrollbar as the C#'s: what every list and page of the
/// window scrolls in.
pub fn scroll<'a, Message: 'a>(
    content: impl Into<Element<'a, Message>>,
) -> Scrollable<'a, Message> {
    iced::widget::scrollable(content)
        .direction(Direction::Vertical(scrollbar()))
        .style(scrollbars)
}

/// The scrollbars of a scrolled view, as the C# `ThemedScrollBarStyle`: the track in the
/// window's background, the thumb rounded in the secondary text, in the accent while dragged.
pub fn scrollbars(theme: &Theme, status: ScrollStatus) -> ScrollStyle {
    let brushes = Brushes::of(theme);
    let (vertical, horizontal) = match status {
        ScrollStatus::Dragged {
            is_vertical_scrollbar_dragged,
            is_horizontal_scrollbar_dragged,
            ..
        } => (
            is_vertical_scrollbar_dragged,
            is_horizontal_scrollbar_dragged,
        ),
        _ => (false, false),
    };
    let rail = |dragged: bool| Rail {
        background: Some(Background::Color(brushes.surface)),
        border: Border {
            radius: radius::SM.into(),
            ..Border::default()
        },
        scroller: Scroller {
            background: Background::Color(if dragged {
                brushes.accent
            } else {
                brushes.secondary
            }),
            border: Border {
                radius: radius::SM.into(),
                ..Border::default()
            },
        },
    };
    ScrollStyle {
        vertical_rail: rail(vertical),
        horizontal_rail: rail(horizontal),
        gap: None,
        ..iced::widget::scrollable::default(theme, status)
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
    fn an_about_card_is_a_card_rounded_as_the_csharp_xl_and_its_rule_the_card_edge() {
        let theme = magellan();
        let card = card(&theme);
        let large = large_card(&theme);
        assert_eq!(large.background, card.background);
        assert_eq!(large.border.color, card.border.color);
        assert!((large.border.radius.top_left - radius::XL).abs() < f32::EPSILON);
        assert_eq!(
            rule(&theme).background,
            Some(Background::Color(card.border.color)),
            "the C# BorderBrush, unfaded"
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
    fn the_field_typed_in_is_outlined_wider_in_every_theme_and_yellow_in_high_contrast() {
        for each in AppTheme::ALL {
            let theme = themes::theme(each, Accent::Blue);
            let rest = text_input(&theme, FieldStatus::Active);
            let typed = text_input(&theme, FieldStatus::Focused { is_hovered: false });
            assert!(
                (typed.border.width - FOCUS_BORDER_WIDTH).abs() < f32::EPSILON,
                "{each:?}"
            );
            assert!(
                typed.border.width > BORDER_WIDTH,
                "{each:?}: seen at a glance"
            );
            assert_ne!(typed.border.color, rest.border.color, "{each:?}");
            let expected = if each == AppTheme::HighContrast {
                themes::colors(each).yellow
            } else {
                themes::colors(each).accent(Accent::Blue)
            };
            assert_eq!(typed.border.color, expected, "{each:?}");
        }
    }

    #[test]
    fn high_contrast_outlines_every_control_wider() {
        let theme = themes::theme(AppTheme::HighContrast, Accent::Default);
        let white = themes::colors(AppTheme::HighContrast).foreground;
        let borders = [
            primary(&theme, ButtonStatus::Active).border,
            secondary(&theme, ButtonStatus::Active).border,
            subtle(&theme, ButtonStatus::Active).border,
            tab(false)(&theme, ButtonStatus::Active).border,
            text_input(&theme, FieldStatus::Active).border,
            pick_list(&theme, ListStatus::Active).border,
            checkbox(&theme, CheckStatus::Active { is_checked: false }).border,
            card(&theme).border,
        ];
        for border in borders {
            assert!(
                (border.width - HIGH_CONTRAST_BORDER_WIDTH).abs() < f32::EPSILON,
                "{border:?}"
            );
            assert!(border.color.a > 0.0, "drawn: {border:?}");
        }
        assert_eq!(subtle(&theme, ButtonStatus::Active).border.color, white);
        assert_eq!(card(&theme).border.color, white, "white edges");
        let other = magellan();
        assert_eq!(
            subtle(&other, ButtonStatus::Active).border.color,
            Color::TRANSPARENT,
            "elsewhere a quiet button has no edge"
        );
        assert!(
            (secondary(&other, ButtonStatus::Active).border.width - BORDER_WIDTH).abs()
                < f32::EPSILON
        );
    }

    #[test]
    fn a_scrollbar_thumb_is_the_secondary_text_and_the_accent_while_dragged() {
        let theme = magellan();
        let colors = themes::colors(AppTheme::Magellan);
        let rest = scrollbars(
            &theme,
            ScrollStatus::Active {
                is_horizontal_scrollbar_disabled: false,
                is_vertical_scrollbar_disabled: false,
            },
        );
        assert_eq!(
            rest.vertical_rail.scroller.background,
            Background::Color(colors.comment)
        );
        assert_eq!(
            rest.vertical_rail.background,
            Some(Background::Color(colors.background)),
            "the track, the window's background"
        );
        let dragged = scrollbars(
            &theme,
            ScrollStatus::Dragged {
                is_horizontal_scrollbar_dragged: false,
                is_vertical_scrollbar_dragged: true,
                is_horizontal_scrollbar_disabled: false,
                is_vertical_scrollbar_disabled: false,
            },
        );
        assert_eq!(
            dragged.vertical_rail.scroller.background,
            Background::Color(colors.accent(Accent::Blue))
        );
        assert_eq!(
            dragged.horizontal_rail.scroller.background,
            Background::Color(colors.comment),
            "the other one at rest"
        );
        const { assert!(SCROLLER_WIDTH < SCROLLBAR_WIDTH, "thin, inside its track") };
    }

    #[test]
    fn a_toolbar_toggle_on_is_outlined_in_the_warning_colour() {
        let theme = magellan();
        let colors = themes::colors(AppTheme::Magellan);
        assert_eq!(
            toggle(true)(&theme, ButtonStatus::Active).border.color,
            colors.orange
        );
        let off = toggle(false)(&theme, ButtonStatus::Active);
        assert_eq!(off.border.color, colors.comment);
        assert_eq!(off.background, None, "nothing behind it at rest");
    }

    #[test]
    fn a_disabled_button_is_faded() {
        let theme = magellan();
        let style = secondary(&theme, ButtonStatus::Disabled);
        assert!(style.text_color.a < 1.0);
    }
}
