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
use iced::theme::palette::Extended;
use iced::widget::TextInput;
use iced::widget::button::{Status as ButtonStatus, Style as ButtonStyle};
use iced::widget::checkbox::{Status as CheckStatus, Style as CheckStyle};
use iced::widget::container::Style as BoxStyle;
use iced::widget::pick_list::{Status as ListStatus, Style as ListStyle};
use iced::widget::progress_bar::Style as ProgressStyle;
use iced::widget::radio::{Status as RadioStatus, Style as RadioStyle};
use iced::widget::scrollable::{
    Direction, Rail, Scrollable, Scrollbar, Scroller, Status as ScrollStatus, Style as ScrollStyle,
};
use iced::widget::slider::{
    Handle, Rail as SliderRail, Status as SliderStatus, Style as SliderStyle,
};
use iced::widget::text::LineHeight;
use iced::widget::text::Style as TextStyle;
use iced::widget::text_editor::{Status as EditorStatus, Style as EditorStyle};
use iced::widget::text_input::{Status as FieldStatus, Style as FieldStyle};
use iced::{Background, Border, Color, Element, Font, Pixels, Shadow, Theme, font};

use crate::tokens::{
    BORDER_WIDTH, DIALOG_SHADOW_ALPHA, DIALOG_SHADOW_BLUR, DIALOG_SHADOW_DEPTH, FOCUS_BORDER_WIDTH,
    HIGH_CONTRAST_BORDER_WIDTH, INPUT_LINE_HEIGHT, INPUT_PADDING, MENU_SHADOW_ALPHA,
    MENU_SHADOW_BLUR, MENU_SHADOW_DEPTH, OPACITY_DISABLED, SELECTION_ALPHA, SELECTION_ALPHA_MIN,
    SELECTION_ALPHA_STEP, radius,
};

// The C# `AppFocusVisualStyle` (`CommonControls.xaml:240-253`) rings a focused button in a
// 2 px accent, rounded 4. iced 0.14 buttons take no keyboard focus and have no `Focused`
// status to style, so no ring is drawn on them; the fields and the check boxes show their
// focus (`FOCUS_BORDER_WIDTH`, the owner-approved 2 px border). Audit P3-5.

/// Width of a scrollbar's track, as the C# `ThemedScrollBarStyle`'s.
pub const SCROLLBAR_WIDTH: f32 = 12.0;

/// Width of its thumb: the track less the C# thumb's margin of 2 on each side.
pub const SCROLLER_WIDTH: f32 = 8.0;

/// Opacity of the scrim behind a dialog, as the C# `OverlayBackground` (the window's
/// background at 0.70, `2758070d`): a light theme dims to a light veil.
const SCRIM_ALPHA: f32 = 0.7;

/// Opacity of a toolbar's separator, as the C# `ToolbarVerticalSeparatorStyle`'s.
const SEPARATOR_OPACITY: f32 = 0.65;

/// The window's font, semi-bold, as the C#'s `FontWeight="SemiBold"`: a selected tab's
/// name, a session's title above it.
pub const SEMIBOLD: Font = Font {
    weight: font::Weight::Semibold,
    ..crate::UI_FONT
};

/// The window's font, bold, as the C#'s `FontWeight="Bold"`: the application's name in its
/// toolbar.
pub const BOLD: Font = Font {
    weight: font::Weight::Bold,
    ..crate::UI_FONT
};

/// Opacity of the C# `BroadcastActiveBrush`: the red behind broadcast input while it is on.
const BROADCAST_ALPHA: f32 = 0.27;

/// Opacity of a tab's close button at rest, as the C# `TabCloseButtonIdleOpacity`: full on
/// the tab selected or under the pointer. 0.3 measured under the 3:1 a control needs.
pub const CLOSE_REST_OPACITY: f32 = 0.5;

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
    /// The accent as a line or a fill that carries meaning, 3:1 on every surface: the
    /// accent itself where it already is.
    line: Color,
    /// `AccentHoverBrush`: the accent, lighter.
    accent_hover: Color,
    /// `AccentPressedBrush`: the accent, darker.
    accent_pressed: Color,
    /// `TextOnAccentBrush`: the window's background, on the accent.
    on_accent: Color,
    /// `CardBrush`.
    card: Color,
    /// `HighlightBrush`: the theme's `SelectionColor`, what a hovered row, tab or menu
    /// entry lies on. Apart from the card in every theme but Dracula's.
    highlight: Color,
    /// `SurfaceBrush`: the window's background.
    surface: Color,
    /// `TextPrimaryBrush`.
    text: Color,
    /// `InputBorderBrush`, the comment colour as a line: lifted to 3:1 where it is short.
    secondary: Color,
    /// `TextSecondaryBrush`: the comment colour as text, 4.5:1 on every surface.
    secondary_text: Color,
    /// The placeholder of a field.
    placeholder: Color,
    /// `ErrorBrush`.
    danger: Color,
    /// `WarningBrush`.
    warning: Color,
    /// `BorderBrush`: a card's, a bar's or a popup's edge, the C# `BorderColor`.
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
        let secondary = crate::themes::derived(theme, crate::themes::Derived::CommentLine);
        Self {
            accent,
            line: crate::themes::derived(theme, crate::themes::Derived::AccentLine),
            accent_hover: crate::themes::accent_states_of(theme).0,
            accent_pressed: crate::themes::accent_states_of(theme).1,
            on_accent: extended.background.base.color,
            card: extended.background.weak.color,
            highlight: crate::themes::selection_of(theme),
            surface: extended.background.base.color,
            text: extended.background.base.text,
            secondary,
            secondary_text: extended.secondary.base.color,
            placeholder: crate::themes::derived(theme, crate::themes::Derived::Placeholder),
            danger: extended.danger.base.color,
            warning: extended.warning.base.color,
            // `BorderColor`, the current line in every ThemeForge theme, which the card
            // takes too; the text colour in high contrast, whose current line is black.
            edge: if high_contrast {
                extended.background.base.text
            } else {
                extended.background.weak.color
            },
            quiet_edge: if high_contrast {
                secondary
            } else {
                Color::TRANSPARENT
            },
            focus: if high_contrast {
                crate::themes::colors_of(theme).yellow
            } else {
                crate::themes::derived(theme, crate::themes::Derived::AccentLine)
            },
            width: if high_contrast {
                HIGH_CONTRAST_BORDER_WIDTH
            } else {
                BORDER_WIDTH
            },
        }
    }

    /// The label drawn on `fill`: the window's background where it reads, black or white
    /// otherwise.
    fn label_on(&self, fill: Color) -> Color {
        crate::themes::label_on(self.on_accent, fill)
    }

    /// `color` as a mark: itself where it is seen at 3:1 on the window and on a card, the
    /// nearest colour that is otherwise.
    fn seen(&self, color: Color) -> Color {
        crate::themes::legible_on(
            color,
            &[self.surface, self.card],
            crate::themes::MIN_BOUNDARY_CONTRAST,
        )
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
pub(crate) fn faded_to(style: &ButtonStyle, opacity: f32) -> ButtonStyle {
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
    // The window's background on the accent, as the C#; black or white where the accent
    // hovered or pressed leaves it under 4.5:1.
    let filled = |color| brushes.button(Some(color), brushes.label_on(color), color);
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
            background: Some(Background::Color(brushes.highlight)),
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
    // The label is the derived error text, 4.5:1 on the card; the outline under the pointer
    // is a line, the theme's own red.
    let rest = brushes.button(
        Some(brushes.card),
        crate::themes::derived(theme, crate::themes::Derived::Error),
        brushes.secondary,
    );
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
            background: Some(Background::Color(brushes.highlight)),
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

/// A button of the status bar, a pill as the C# `StatusBarPillButtonStyle`: the card
/// colour, outlined in the border colour, rounded as `CornerRadiusSm`; the highlight and
/// the accent outline under the pointer, the window's background pressed.
pub fn status_pill(theme: &Theme, status: ButtonStatus) -> ButtonStyle {
    let brushes = Brushes::of(theme);
    let rest = ButtonStyle {
        border: brushes.outline(brushes.edge, radius::SM),
        ..brushes.button(Some(brushes.card), brushes.text, brushes.edge)
    };
    match status {
        ButtonStatus::Active => rest,
        ButtonStatus::Hovered => ButtonStyle {
            background: Some(Background::Color(brushes.highlight)),
            border: brushes.outline(brushes.accent, radius::SM),
            ..rest
        },
        ButtonStatus::Pressed => ButtonStyle {
            background: Some(Background::Color(brushes.surface)),
            ..rest
        },
        ButtonStatus::Disabled => faded(&rest),
    }
}

/// A pill of the status bar while its function is on, as the C# broadcast toggle's trigger:
/// the red faint behind it, its border and text red.
pub fn status_pill_active(theme: &Theme, status: ButtonStatus) -> ButtonStyle {
    let brushes = Brushes::of(theme);
    let fill = brushes.danger.scale_alpha(BROADCAST_ALPHA);
    // The error text, lifted until it reads on the red it is drawn over as well.
    let label = crate::themes::legible_on(
        crate::themes::derived(theme, crate::themes::Derived::Error),
        &[crate::themes::over(fill, brushes.surface)],
        crate::themes::MIN_TEXT_CONTRAST,
    );
    let rest = ButtonStyle {
        border: brushes.outline(brushes.danger, radius::SM),
        ..brushes.button(Some(fill), label, brushes.danger)
    };
    match status {
        ButtonStatus::Active | ButtonStatus::Hovered => rest,
        ButtonStatus::Pressed => ButtonStyle {
            background: Some(Background::Color(brushes.surface)),
            ..rest
        },
        ButtonStatus::Disabled => faded(&rest),
    }
}

/// The status bar's counter, a pill that is not a button, as the C#
/// `StatusBarPillBorderStyle`: the card colour, outlined in the border colour.
pub fn status_pill_box(theme: &Theme) -> BoxStyle {
    let brushes = Brushes::of(theme);
    BoxStyle {
        background: Some(Background::Color(brushes.card)),
        text_color: Some(brushes.text),
        border: brushes.outline(brushes.edge, radius::SM),
        ..BoxStyle::default()
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
            background: if selected {
                Some(Background::Color(brushes.card))
            } else if lit {
                Some(Background::Color(brushes.highlight))
            } else {
                None
            },
            text_color: if lit {
                brushes.text
            } else {
                brushes.secondary_text
            },
            // `BorderThickness="1,1,1,0"` (`CommonControls.xaml:1218`): transparent at
            // rest, the border colour once hovered (`:1237`). Selected, the strong border
            // as the C# `fix(tabs)` (v2026.102201): the plain one is the card colour on
            // every shipped theme, which left the selected tab without an outline.
            border: Border {
                color: if selected {
                    crate::themes::derived(theme, crate::themes::Derived::BorderStrong)
                } else if lit {
                    brushes.edge
                } else {
                    brushes.quiet_edge
                },
                width: brushes.width,
                radius: TAB_RADIUS,
            },
            shadow: Shadow::default(),
            snap: true,
        }
    }
}

/// The chip naming a folder before its tabs, as the C# group chip: a card edged quietly,
/// its text secondary.
pub fn folder_chip(theme: &Theme) -> BoxStyle {
    let brushes = Brushes::of(theme);
    BoxStyle {
        background: Some(Background::Color(brushes.card)),
        text_color: Some(brushes.secondary_text),
        border: Border {
            color: brushes.quiet_edge,
            width: brushes.width,
            radius: radius::SM.into(),
        },
        ..BoxStyle::default()
    }
}

/// The bar under a selected tab, in the accent, as the C#'s; `lit` false, it keeps its
/// place unseen.
pub fn underline(lit: bool) -> impl Fn(&Theme) -> BoxStyle {
    move |theme| BoxStyle {
        background: lit.then(|| Background::Color(Brushes::of(theme).line)),
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
        background: Some(Background::Color(Brushes::of(theme).line)),
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
    let selection = selection_fill(&brushes);
    let rest = FieldStyle {
        background: Background::Color(brushes.card),
        border: brushes.outline(brushes.secondary, radius::MD),
        icon: brushes.secondary,
        placeholder: brushes.placeholder,
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

/// The fill behind the text selected in a field: the accent over the field, as faint as it
/// takes for the text to read on it at 4.5:1.
fn selection_fill(brushes: &Brushes) -> Color {
    let key = (
        brushes.accent.into_rgba8(),
        brushes.card.into_rgba8(),
        brushes.text.into_rgba8(),
    );
    if let Ok(fills) = SELECTIONS.lock()
        && let Some(fill) = fills.get(&key)
    {
        return *fill;
    }
    let fill = lowered_selection(brushes);
    if let Ok(mut fills) = SELECTIONS.lock() {
        fills.insert(key, fill);
    }
    fill
}

/// What [`selection_fill`] asked: the accent, the card and the text, as 8-bit RGBA.
type SelectionKey = ([u8; 4], [u8; 4], [u8; 4]);

/// The selection fills found so far: a field asks for its look every frame.
static SELECTIONS: std::sync::LazyLock<
    std::sync::Mutex<std::collections::HashMap<SelectionKey, Color>>,
> = std::sync::LazyLock::new(|| std::sync::Mutex::new(std::collections::HashMap::new()));

/// The walk of [`selection_fill`]: the opacity lowered a step at a time.
fn lowered_selection(brushes: &Brushes) -> Color {
    let mut alpha = SELECTION_ALPHA;
    loop {
        let fill = crate::themes::over(brushes.accent.scale_alpha(alpha), brushes.card);
        if alpha - SELECTION_ALPHA_STEP < SELECTION_ALPHA_MIN
            || crate::themes::contrast_ratio(brushes.text, fill) >= crate::themes::MIN_TEXT_CONTRAST
        {
            return fill;
        }
        alpha -= SELECTION_ALPHA_STEP;
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
/// accent on the highlight.
pub fn tool_card(theme: &Theme, status: ButtonStatus) -> ButtonStyle {
    let brushes = Brushes::of(theme);
    let rest = brushes.button(Some(brushes.card), brushes.text, brushes.edge);
    match status {
        ButtonStatus::Active => rest,
        ButtonStatus::Hovered | ButtonStatus::Pressed => ButtonStyle {
            background: Some(Background::Color(brushes.highlight)),
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
            background: lit.then_some(Background::Color(brushes.highlight)),
            text_color: if selected {
                brushes.text
            } else {
                brushes.secondary_text
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

/// A section on the Settings rail, as the C# `SettingsRailTabItemStyle`: its text secondary
/// until `selected` or under the pointer, then the text's own colour; the rail sits straight
/// on the window's background, so the open section is lit by the card's colour and the one
/// under the pointer by the highlight. The accent's bar at its left is its caller's.
pub fn rail_item(selected: bool) -> impl Fn(&Theme, ButtonStatus) -> ButtonStyle {
    move |theme, status| {
        let brushes = Brushes::of(theme);
        let hovered = matches!(status, ButtonStatus::Hovered | ButtonStatus::Pressed);
        ButtonStyle {
            background: if hovered {
                Some(Background::Color(brushes.highlight))
            } else {
                selected.then_some(Background::Color(brushes.card))
            },
            text_color: if selected || hovered {
                brushes.text
            } else {
                brushes.secondary_text
            },
            border: Border {
                color: brushes.quiet_edge,
                width: brushes.width,
                radius: radius::SM.into(),
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
        placeholder_color: brushes.placeholder,
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

/// The fill and the text of the entry of a menu under the pointer: the highlight; in high
/// contrast, whose card is its background, the accent, to be seen.
pub(crate) fn menu_entry_lit(theme: &Theme) -> (Color, Color) {
    let brushes = Brushes::of(theme);
    if crate::themes::is_high_contrast(theme) {
        (brushes.accent, brushes.label_on(brushes.accent))
    } else {
        (brushes.highlight, brushes.text)
    }
}

/// A drop-down's open list, as the C# combo box's popup: the window's background outlined in
/// the border colour, the entry under the pointer on the highlight, as `HighlightBrush`.
pub fn menu(theme: &Theme) -> MenuStyle {
    let brushes = Brushes::of(theme);
    let (lit, on_lit) = menu_entry_lit(theme);
    MenuStyle {
        background: Background::Color(brushes.surface),
        border: brushes.outline(brushes.edge, radius::LG),
        text_color: brushes.text,
        selected_text_color: on_lit,
        selected_background: Background::Color(lit),
        shadow: Shadow::default(),
    }
}

/// A check box, as the C# `ThemedCheckBoxStyle`: a card outlined in the secondary text,
/// filled with the accent once checked, outlined in the accent under the pointer.
pub fn checkbox(theme: &Theme, status: CheckStatus) -> CheckStyle {
    let brushes = Brushes::of(theme);
    let (is_checked, edge, fade) = match status {
        CheckStatus::Active { is_checked } => (is_checked, brushes.secondary, 1.0),
        CheckStatus::Hovered { is_checked } => (is_checked, brushes.line, 1.0),
        CheckStatus::Disabled { is_checked } => (is_checked, brushes.secondary, OPACITY_DISABLED),
    };
    let (fill, edge) = if is_checked {
        (brushes.line, brushes.line)
    } else {
        (brushes.card, edge)
    };
    CheckStyle {
        background: Background::Color(fill.scale_alpha(fade)),
        icon_color: brushes.label_on(fill),
        border: brushes.outline(edge.scale_alpha(fade), radius::SM),
        text_color: None,
    }
}

/// A count of errors on a tab's header, as the C# server dialog's badge: the window's
/// background on the error colour, rounded as `CornerRadiusMd`.
pub fn error_badge(theme: &Theme) -> BoxStyle {
    let brushes = Brushes::of(theme);
    BoxStyle {
        text_color: Some(brushes.label_on(brushes.danger)),
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
        text_color: Some(Brushes::of(theme).secondary_text),
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

/// A choice among several, as the C# `ThemedRadioButtonStyle`: a card circle outlined in the
/// secondary text, the accent under the pointer, filled with the accent and a dot of the
/// window's background when chosen.
pub fn radio(theme: &Theme, status: RadioStatus) -> RadioStyle {
    let brushes = Brushes::of(theme);
    let (chosen, hovered) = match status {
        RadioStatus::Active { is_selected } => (is_selected, false),
        RadioStatus::Hovered { is_selected } => (is_selected, true),
    };
    let (fill, edge) = match (chosen, hovered) {
        (true, _) => (brushes.line, brushes.line),
        (false, true) => (brushes.card, brushes.line),
        (false, false) => (brushes.card, brushes.secondary),
    };
    RadioStyle {
        background: Background::Color(fill),
        dot_color: brushes.label_on(fill),
        border_width: brushes.width,
        border_color: edge,
        text_color: None,
    }
}

/// A slider, as the C# `ThemedSliderStyle`: the accent up to the thumb, the card colour past
/// it, an accent thumb edged in the window's background, lighter under the pointer and
/// darker dragged.
pub fn slider(theme: &Theme, status: SliderStatus) -> SliderStyle {
    let brushes = Brushes::of(theme);
    let thumb = match status {
        SliderStatus::Active => brushes.line,
        SliderStatus::Hovered => brushes.seen(brushes.accent_hover),
        SliderStatus::Dragged => brushes.seen(brushes.accent_pressed),
    };
    let base = iced::widget::slider::default(theme, status);
    SliderStyle {
        rail: SliderRail {
            backgrounds: (
                Background::Color(brushes.line),
                Background::Color(brushes.card),
            ),
            border: Border {
                radius: radius::XS.into(),
                ..Border::default()
            },
            ..base.rail
        },
        handle: Handle {
            background: Background::Color(thumb),
            border_width: BORDER_WIDTH,
            border_color: brushes.surface,
            ..base.handle
        },
    }
}

/// A bar of progress, as the C# `ThemedProgressBarStyle`: the accent on a card, outlined in
/// the border colour.
pub fn progress(theme: &Theme) -> ProgressStyle {
    let brushes = Brushes::of(theme);
    ProgressStyle {
        background: Background::Color(brushes.card),
        bar: Background::Color(brushes.line),
        border: brushes.outline(brushes.edge, radius::MD),
    }
}

/// Room above and below a tooltip's text, as the C# `ThemedToolTipStyle`'s `Padding="8,6"`
/// (`CommonControls.xaml:1984`). iced pads a tooltip evenly, so the 2 more at each side
/// come from [`tip`].
pub const TOOLTIP_PADDING: f32 = 6.0;

/// Room at each side of a tooltip's text, as the same `Padding="8,6"`.
pub const TOOLTIP_PADDING_X: f32 = 8.0;

/// `content` with `text` as its tooltip at `position`: iced's tooltip, its text given the
/// 2 px at each side that make the C#'s `8,6` of the even [`TOOLTIP_PADDING`].
pub fn tip<'a, Message: 'a>(
    content: impl Into<Element<'a, Message>>,
    text: impl Into<Element<'a, Message>>,
    position: iced::widget::tooltip::Position,
) -> iced::widget::Tooltip<'a, Message> {
    iced::widget::tooltip(
        content,
        iced::widget::container(text).padding(iced::Padding {
            top: 0.0,
            right: TOOLTIP_PADDING_X - TOOLTIP_PADDING,
            bottom: 0.0,
            left: TOOLTIP_PADDING_X - TOOLTIP_PADDING,
        }),
        position,
    )
}

/// The count on a tree folder, as the C# folder row's badge (`MainWindow.xaml:882-896`): a
/// card outlined in the border colour, rounded as `SessionTreeBadgeCornerRadius`. Its count is
/// in the primary text, not the C#'s `TextSecondaryBrush`: that is under 4.5:1 on the card
/// in Dracula, too faint for small text.
pub fn count_badge(theme: &Theme) -> BoxStyle {
    let brushes = Brushes::of(theme);
    BoxStyle {
        background: Some(Background::Color(brushes.card)),
        text_color: Some(brushes.text),
        border: brushes.outline(brushes.edge, radius::BADGE),
        ..BoxStyle::default()
    }
}

/// The three dots on the sidebar's splitter, as the C# splitter's glyph (`MainWindow.xaml:1566`):
/// the secondary text, round.
pub fn splitter_dot(theme: &Theme) -> BoxStyle {
    filled(Brushes::of(theme).secondary, f32::MAX)
}

/// A tooltip, as the C# `ThemedToolTipStyle`: the window's background, outlined in the
/// border colour and rounded as `CornerRadiusMd`; outlined in white in high contrast.
pub fn tooltip(theme: &Theme) -> BoxStyle {
    let brushes = Brushes::of(theme);
    BoxStyle {
        background: Some(Background::Color(brushes.surface)),
        text_color: Some(brushes.text),
        border: brushes.outline(brushes.edge, radius::MD),
        ..BoxStyle::default()
    }
}

/// A panel set over or beside the content, as the C# popups and list boxes: the window's
/// background, outlined in the border colour and rounded as `CornerRadiusLg`.
pub fn panel(theme: &Theme) -> BoxStyle {
    let brushes = Brushes::of(theme);
    BoxStyle {
        background: Some(Background::Color(brushes.surface)),
        text_color: Some(brushes.text),
        border: brushes.outline(brushes.edge, radius::LG),
        ..BoxStyle::default()
    }
}

/// The shadow under a context menu, as the C# `ContextMenu`'s `HasDropShadow`: a soft one,
/// much lighter than a dialog's.
pub fn menu_shadow() -> Shadow {
    Shadow {
        color: Color {
            a: MENU_SHADOW_ALPHA,
            ..Color::BLACK
        },
        offset: iced::Vector::new(0.0, MENU_SHADOW_DEPTH),
        blur_radius: MENU_SHADOW_BLUR,
    }
}

/// A context menu's card, as the C# `ThemedContextMenuStyle`: the window's background,
/// outlined in the border colour, rounded as `CornerRadiusLg`, with a drop shadow.
pub fn menu_card(theme: &Theme) -> BoxStyle {
    let brushes = Brushes::of(theme);
    BoxStyle {
        background: Some(Background::Color(brushes.surface)),
        text_color: Some(brushes.text),
        border: brushes.outline(brushes.edge, radius::LG),
        shadow: menu_shadow(),
        ..BoxStyle::default()
    }
}

/// A bar along an edge of the window, as the C# toolbar and status bar: the window's
/// background, square, no outline; a [`rule`] beside it draws the C# edge on its one side.
pub fn bar(theme: &Theme) -> BoxStyle {
    let brushes = Brushes::of(theme);
    BoxStyle {
        background: Some(Background::Color(brushes.surface)),
        text_color: Some(brushes.text),
        ..BoxStyle::default()
    }
}

/// The one-sided edge of a [`bar`]: a line across, as thick as a border, in the border
/// colour.
pub fn edge_line<'a, Message: 'a>() -> Element<'a, Message> {
    iced::widget::container(iced::widget::space())
        .width(iced::Length::Fill)
        .height(BORDER_WIDTH)
        .style(strong_rule)
        .into()
}

/// The one-sided edge of a [`bar`] on its right: a line down, as thick as a border, in the
/// border colour, as the C# `BorderThickness="0,0,1,0"` of the sessions panel.
pub fn vertical_edge_line<'a, Message: 'a>() -> Element<'a, Message> {
    iced::widget::container(iced::widget::space())
        .width(BORDER_WIDTH)
        .height(iced::Length::Fill)
        .style(strong_rule)
        .into()
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

/// The round badge of the C# Launchpad card (`MainWindow.xaml:2131-2137`, `:2188-2194`): the
/// window's background outlined round, in the accent for the welcome and in the border
/// colour for the "select a session" card. `side` is the badge's side, which `radius` halves.
pub fn launchpad_badge(accented: bool, side: f32) -> impl Fn(&Theme) -> BoxStyle {
    move |theme: &Theme| {
        let brushes = Brushes::of(theme);
        let edge = if accented {
            brushes.accent
        } else {
            brushes.edge
        };
        BoxStyle {
            background: Some(Background::Color(brushes.surface)),
            border: brushes.outline(edge, side / 2.0),
            ..BoxStyle::default()
        }
    }
}

/// The shortcuts pill under the C# Launchpad's buttons (`MainWindow.xaml:2166-2172`): the
/// window's background outlined in the border colour, rounded as `CornerRadiusMd`.
pub fn launchpad_pill(theme: &Theme) -> BoxStyle {
    let brushes = Brushes::of(theme);
    BoxStyle {
        background: Some(Background::Color(brushes.surface)),
        border: brushes.outline(brushes.edge, radius::MD),
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

/// A line that marks out a region, in the strong border colour (3:1 on the window and on a
/// card), as the C# `BorderStrongBrush` of the sidebar divider and of the bars' and tool
/// headers' rules (`f2cf2263`).
pub fn strong_rule(theme: &Theme) -> BoxStyle {
    BoxStyle {
        background: Some(Background::Color(crate::themes::derived(
            theme,
            crate::themes::Derived::BorderStrong,
        ))),
        ..BoxStyle::default()
    }
}

/// A dot of the welcome tour's step indicator, as the C# ellipses
/// (`MainWindow.xaml:5697-5713`): the accent for the step shown, the disabled text colour for
/// the others.
pub fn step_dot(shown: bool) -> impl Fn(&Theme) -> BoxStyle {
    move |theme| {
        let brushes = Brushes::of(theme);
        let color = if shown {
            brushes.accent
        } else {
            Color {
                a: brushes.secondary.a * OPACITY_DISABLED,
                ..brushes.secondary
            }
        };
        BoxStyle {
            background: Some(Background::Color(color)),
            border: Border {
                radius: Radius::from(radius::SM),
                ..Border::default()
            },
            ..BoxStyle::default()
        }
    }
}

/// The drop shadow of a dialog card, as the C# popups' `DropShadowEffect`
/// (`MainWindow.xaml:6107`: blur 24, depth 8 straight down, opacity 0.4, black).
pub fn dialog_shadow() -> Shadow {
    Shadow {
        color: Color {
            a: DIALOG_SHADOW_ALPHA,
            ..Color::BLACK
        },
        offset: iced::Vector::new(0.0, DIALOG_SHADOW_DEPTH),
        blur_radius: DIALOG_SHADOW_BLUR,
    }
}

/// A dialog, as the C# dialog windows: the window's background, rounded as `CornerRadiusXl`,
/// its edge the border colour, set above the window by the popups' [`dialog_shadow`].
pub fn dialog(theme: &Theme) -> BoxStyle {
    let brushes = Brushes::of(theme);
    BoxStyle {
        background: Some(Background::Color(brushes.surface)),
        border: brushes.outline(brushes.edge, radius::XL),
        shadow: dialog_shadow(),
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

/// A section of the session form, as the C# `DialogSectionCardStyle`
/// (`DialogCommonStyles.xaml:52`): the card colour, rounded as `CornerRadiusXl`, outlined in
/// the strong border.
pub fn section_card(theme: &Theme) -> BoxStyle {
    let brushes = Brushes::of(theme);
    BoxStyle {
        background: Some(Background::Color(brushes.card)),
        border: Border {
            color: crate::themes::derived(theme, crate::themes::Derived::BorderStrong),
            width: brushes.width,
            radius: radius::XL.into(),
        },
        ..BoxStyle::default()
    }
}

/// A node of the "Connection path" diagram, as the C# `DiagramNodeStyle`
/// (`ServerDialog.xaml:140`): filled with the window's background, outlined in the border
/// colour, rounded as `CornerRadiusLg`.
pub fn diagram_node(theme: &Theme) -> BoxStyle {
    let brushes = Brushes::of(theme);
    BoxStyle {
        background: Some(Background::Color(brushes.surface)),
        border: brushes.outline(brushes.edge, radius::LG),
        ..BoxStyle::default()
    }
}

/// The bar holding a form's pill sub-tabs, as the header of the C# `SegmentedTabControlStyle`
/// (`CommonControls.xaml:1309`): the window's background, outlined in the border colour,
/// rounded as `CornerRadiusMd`.
pub fn segment_bar(theme: &Theme) -> BoxStyle {
    let brushes = Brushes::of(theme);
    BoxStyle {
        background: Some(Background::Color(brushes.surface)),
        border: brushes.outline(brushes.edge, radius::MD),
        ..BoxStyle::default()
    }
}

/// A pill sub-tab, as the C# `SegmentedTabItemStyle` (`CommonControls.xaml:1340`): nothing
/// behind it but under the pointer, the accent with the label on it once `selected`; its
/// text secondary until the pointer is over it.
pub fn segment(selected: bool) -> impl Fn(&Theme, ButtonStatus) -> ButtonStyle {
    move |theme, status| {
        let brushes = Brushes::of(theme);
        let hovered = matches!(status, ButtonStatus::Hovered | ButtonStatus::Pressed);
        let (background, text_color, edge) = if selected {
            (
                Some(brushes.accent),
                brushes.label_on(brushes.accent),
                brushes.accent,
            )
        } else if hovered {
            (Some(brushes.highlight), brushes.text, brushes.edge)
        } else {
            (None, brushes.secondary_text, brushes.quiet_edge)
        };
        let look = ButtonStyle {
            background: background.map(Background::Color),
            text_color,
            border: brushes.outline(edge, radius::SM),
            shadow: Shadow::default(),
            snap: true,
        };
        if status == ButtonStatus::Disabled {
            faded(&look)
        } else {
            look
        }
    }
}

/// A protocol's card in the picker, as the C# `ProtocolCardButtonStyle`
/// (`ServerDialog.xaml:230`): the card colour with no border at rest, the window's
/// background outlined in the accent under the pointer.
pub fn protocol_card(theme: &Theme, status: ButtonStatus) -> ButtonStyle {
    let brushes = Brushes::of(theme);
    let rest = ButtonStyle {
        background: Some(Background::Color(brushes.card)),
        text_color: brushes.text,
        border: brushes.outline(brushes.quiet_edge, radius::XL),
        shadow: Shadow::default(),
        snap: true,
    };
    match status {
        ButtonStatus::Active | ButtonStatus::Disabled => rest,
        ButtonStatus::Hovered | ButtonStatus::Pressed => ButtonStyle {
            background: Some(Background::Color(brushes.surface)),
            border: brushes.outline(brushes.line, radius::XL),
            ..rest
        },
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

/// How loud a level badge is: the C# `RiskBrushKey`, `TextSecondaryBrush` for what only
/// reads, `WarningBrush` for what changes something, `ErrorBrush` for what can destroy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    /// What only reads and shows.
    Quiet,
    /// What changes something that can be undone.
    Warning,
    /// What can destroy data or stop a machine.
    Danger,
}

/// The level badge of a command's row, as the C# list row's risk `Border`: filled in the
/// tone's colour, rounded as `CornerRadiusSm`, its label in whatever reads on that fill.
pub fn level_badge(tone: Tone) -> impl Fn(&Theme) -> BoxStyle {
    move |theme| {
        let brushes = Brushes::of(theme);
        let fill = match tone {
            Tone::Quiet => brushes.secondary,
            Tone::Warning => brushes.warning,
            Tone::Danger => brushes.danger,
        };
        BoxStyle {
            background: Some(Background::Color(fill)),
            text_color: Some(brushes.label_on(fill)),
            border: Border {
                radius: radius::SM.into(),
                ..Border::default()
            },
            ..BoxStyle::default()
        }
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

/// Error text, derived to 4.5:1 on the window and on a card, as the C# `ErrorTextBrush`
/// (`92ccb004`): an inline validation error, a failure line.
pub fn error_text(theme: &Theme) -> TextStyle {
    colored(error_text_color(theme))
}

/// Warning text, derived to 4.5:1 on the window and on a card (`WarningTextBrush`).
pub fn warning_text(theme: &Theme) -> TextStyle {
    colored(warning_text_color(theme))
}

/// Success text, derived to 4.5:1 on the window and on a card (`SuccessTextBrush`).
pub fn success_text(theme: &Theme) -> TextStyle {
    colored(success_text_color(theme))
}

/// Text in the accent, derived to 4.5:1 on the window and on a card, as the C#
/// `AccentTextBrush` (`c5858a38`). The accent as a fill, a border or a glyph stays raw.
pub fn accent_text(theme: &Theme) -> TextStyle {
    colored(accent_text_color(theme))
}

/// `hue` as text: lifted to 4.5:1 on every surface text is drawn on, itself where it reads.
/// For a tool's colour-coded words (a JWT part, an HTTP status class).
#[must_use]
pub fn hue_text(theme: &Theme, hue: Color) -> Color {
    crate::themes::legible_on(
        hue,
        &crate::themes::grounds_of(theme, crate::themes::Derived::Muted),
        crate::themes::MIN_TEXT_CONTRAST,
    )
}

/// `text` lifted to 4.5:1 on `tint` at `alpha` over the window and over a card: the words of
/// a coloured notice.
#[must_use]
pub fn text_on_tint(theme: &Theme, text: Color, tint: Color, alpha: f32) -> Color {
    let fill = tint.scale_alpha(alpha);
    crate::themes::legible_on(
        text,
        &[
            crate::themes::over(fill, theme.palette().background),
            crate::themes::over(fill, card_color(theme)),
        ],
        crate::themes::MIN_TEXT_CONTRAST,
    )
}

/// The colour of [`error_text`], for a widget that takes a colour rather than a style.
#[must_use]
pub fn error_text_color(theme: &Theme) -> Color {
    crate::themes::derived(theme, crate::themes::Derived::Error)
}

/// The colour of [`warning_text`].
#[must_use]
pub fn warning_text_color(theme: &Theme) -> Color {
    crate::themes::derived(theme, crate::themes::Derived::Warning)
}

/// The colour of [`success_text`].
#[must_use]
pub fn success_text_color(theme: &Theme) -> Color {
    crate::themes::derived(theme, crate::themes::Derived::Success)
}

/// The colour of [`accent_text`].
#[must_use]
pub fn accent_text_color(theme: &Theme) -> Color {
    crate::themes::derived(theme, crate::themes::Derived::Accent)
}

fn colored(color: Color) -> TextStyle {
    TextStyle { color: Some(color) }
}

/// The scrim behind a dialog or an overlay: the window's background at [`SCRIM_ALPHA`], as
/// the C# `OverlayBackground`, so a light theme dims to a light veil and the secondary text
/// on it stays readable.
#[must_use]
pub fn scrim(theme: &Theme) -> Color {
    Color {
        a: SCRIM_ALPHA,
        ..theme.palette().background
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

/// A text field as the C# `ThemedTextBoxStyle` draws it: padded `10,6`, a line tall enough
/// for the C# field's height, and styled as [`text_input`]. Every field is made here, so one
/// cannot fall back to iced's default 5 px padding, which left it 27 px high with its text
/// against the edge.
pub fn field<'a, Message: Clone + 'a>(placeholder: &str, value: &str) -> TextInput<'a, Message> {
    iced::widget::text_input(placeholder, value)
        .padding(INPUT_PADDING)
        .line_height(LineHeight::Absolute(Pixels(INPUT_LINE_HEIGHT)))
        .style(text_input)
}

#[cfg(test)]
mod tests {
    use super::*;
    use heimdall_core::settings::{Accent, AppTheme};

    use crate::themes;

    use crate::themes::contrast_ratio as contrast;

    /// The least contrast WCAG asks of small text.
    const SMALL_TEXT_CONTRAST: f64 = crate::themes::MIN_TEXT_CONTRAST;

    #[test]
    fn the_selected_tab_is_outlined_at_3_to_1_on_the_card_and_the_window_in_every_theme() {
        for each in AppTheme::ALL {
            for accent in Accent::ALL {
                let theme = themes::theme(each, accent);
                let style = tab(true)(&theme, ButtonStatus::Active);
                let Some(Background::Color(card)) = style.background else {
                    panic!("{each:?}: the selected tab is a card");
                };
                assert!(style.border.width > 0.0, "{each:?}: no outline width");
                // The tab sits on the card strip of a session and on the window's
                // background in the headers of the Settings page and the server dialog.
                for (what, ground) in [("card", card), ("window", theme.palette().background)] {
                    let ratio = contrast(style.border.color, ground);
                    assert!(
                        ratio >= 3.0,
                        "{each:?}/{accent:?}: the outline is {ratio:.2}:1 on the {what}"
                    );
                }
                assert_ne!(style.border.color, card, "{each:?}: outline is the card");
            }
        }
    }

    #[test]
    fn the_quick_connect_label_and_the_counter_read_at_4_5_to_1_in_every_theme() {
        for each in AppTheme::ALL {
            for accent in Accent::ALL {
                let theme = themes::theme(each, accent);
                // Quick Connect's label is the secondary button's text on its card; the
                // counter's words are the pill's, on the card too.
                let button = secondary(&theme, ButtonStatus::Active);
                let Some(Background::Color(card)) = button.background else {
                    panic!("{each:?}: the button has a card");
                };
                let pill = status_pill_box(&theme);
                assert_eq!(pill.background, Some(Background::Color(card)));
                for (what, color) in [
                    ("Quick Connect", button.text_color),
                    ("counter", pill.text_color.expect("the pill sets its text")),
                ] {
                    let ratio = contrast(color, card);
                    assert!(
                        ratio >= SMALL_TEXT_CONTRAST,
                        "{each:?}/{accent:?}: the {what} is {ratio:.2}:1"
                    );
                }
            }
        }
    }

    #[test]
    fn the_folder_count_reads_at_4_5_to_1_on_its_badge_in_every_theme() {
        for each in AppTheme::ALL {
            for accent in Accent::ALL {
                let style = count_badge(&themes::theme(each, accent));
                let Some(Background::Color(card)) = style.background else {
                    panic!("{each:?}: the badge has a card");
                };
                let ratio = contrast(style.text_color.expect("the badge sets its text"), card);
                assert!(
                    ratio >= SMALL_TEXT_CONTRAST,
                    "{each:?}/{accent:?}: the count is {ratio:.2}:1"
                );
            }
        }
    }

    /// Magellan tinted blue, the theme the C# screens were taken in.
    fn magellan() -> Theme {
        themes::theme(AppTheme::Magellan, Accent::Blue)
    }

    #[test]
    fn a_dialog_has_the_csharp_popup_shadow() {
        let style = dialog(&magellan());
        assert!((style.shadow.blur_radius - 24.0).abs() < f32::EPSILON);
        assert!((style.shadow.offset.y - 8.0).abs() < f32::EPSILON);
        assert!(style.shadow.offset.x.abs() < f32::EPSILON);
        assert!((style.shadow.color.a - 0.4).abs() < f32::EPSILON);
    }

    #[test]
    fn a_context_menu_is_a_bordered_rounded_card_with_a_light_shadow() {
        let style = menu_card(&magellan());
        assert!((style.border.width - BORDER_WIDTH).abs() < f32::EPSILON);
        assert!((style.border.radius.top_left - radius::LG).abs() < f32::EPSILON);
        assert!(style.shadow.blur_radius > 0.0);
        assert!(
            style.shadow.blur_radius < dialog(&magellan()).shadow.blur_radius,
            "lighter than a dialog's"
        );
    }

    #[test]
    fn a_tab_is_edged_once_hovered_or_selected_as_the_csharp_tab_item() {
        let theme = magellan();
        let colors = themes::colors(AppTheme::Magellan);
        let rest = tab(false)(&theme, ButtonStatus::Active);
        assert_eq!(rest.border.color, Color::TRANSPARENT);
        let hovered = tab(false)(&theme, ButtonStatus::Hovered);
        assert_eq!(hovered.border.color, colors.current_line);
        assert!((hovered.border.width - BORDER_WIDTH).abs() < f32::EPSILON);
        // Selected, the strong border (v2026.102201): the plain one is the card colour.
        let selected = tab(true)(&theme, ButtonStatus::Active);
        assert_eq!(
            selected.border.color,
            themes::derived(&theme, themes::Derived::BorderStrong)
        );
        assert!((selected.border.width - BORDER_WIDTH).abs() < f32::EPSILON);
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
            assert_eq!(
                style.border.color,
                themes::colors(AppTheme::Magellan).comment
            );
            assert!((style.border.width - BORDER_WIDTH).abs() < f32::EPSILON);
        }
        assert_eq!(
            danger(&theme, ButtonStatus::Active).text_color,
            themes::derived(&theme, themes::Derived::Error),
            "Delete is written in red, lifted to read on every surface"
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
        let typed = text_input(&theme, FieldStatus::Focused { is_hovered: false });
        assert_eq!(
            typed.border.color,
            themes::derived(&theme, themes::Derived::AccentLine)
        );
        let rest = text_input(&theme, FieldStatus::Active);
        assert_eq!(
            rest.border.color,
            themes::colors(AppTheme::Magellan).comment
        );
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
                themes::derived(&theme, themes::Derived::AccentLine)
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
            Background::Color(themes::colors(AppTheme::Magellan).comment)
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
            Background::Color(themes::colors(AppTheme::Magellan).comment),
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
        assert_eq!(off.border.color, themes::colors(AppTheme::Magellan).comment);
        assert_eq!(off.background, None, "nothing behind it at rest");
    }

    #[test]
    fn a_disabled_button_is_faded() {
        let theme = magellan();
        let style = secondary(&theme, ButtonStatus::Disabled);
        assert!(style.text_color.a < 1.0);
    }

    #[test]
    fn a_hovered_row_tab_and_menu_entry_lie_on_the_selection_not_the_card() {
        for each in [AppTheme::Folio, AppTheme::Parchment, AppTheme::Slate] {
            let theme = themes::theme(each, Accent::Blue);
            let colors = themes::colors(each);
            let on = |color| Some(Background::Color(color));
            assert_eq!(themes::selection_of(&theme), colors.selection, "{each:?}");
            assert_ne!(themes::selection_of(&theme), card_color(&theme), "{each:?}");
            assert_eq!(
                tab(false)(&theme, ButtonStatus::Hovered).background,
                on(colors.selection),
                "{each:?}: a tab under the pointer"
            );
            assert_eq!(
                tab(true)(&theme, ButtonStatus::Active).background,
                on(colors.current_line),
                "{each:?}: the selected tab stays a card"
            );
            assert_eq!(
                sidebar_tab(false)(&theme, ButtonStatus::Hovered).background,
                on(colors.selection),
                "{each:?}"
            );
            assert_eq!(
                secondary(&theme, ButtonStatus::Hovered).background,
                on(colors.selection),
                "{each:?}"
            );
            assert_eq!(
                subtle(&theme, ButtonStatus::Hovered).background,
                on(colors.current_line),
                "{each:?}: the C# ghost button hovers on the card"
            );
            assert_eq!(
                menu(&theme).selected_background,
                Background::Color(colors.selection),
                "{each:?}"
            );
        }
    }

    #[test]
    fn dracula_hovers_as_it_did_the_selection_being_its_card() {
        let theme = themes::theme(AppTheme::Dracula, Accent::Default);
        assert_eq!(themes::selection_of(&theme), card_color(&theme));
    }

    #[test]
    fn the_sessions_panel_is_the_window_background_not_a_card() {
        for each in AppTheme::ALL {
            let theme = themes::theme(each, Accent::Default);
            let panel = bar(&theme);
            assert_eq!(
                panel.background,
                Some(Background::Color(themes::colors(each).background)),
                "{each:?}"
            );
            assert!(
                panel.border.width.abs() < f32::EPSILON,
                "{each:?}: no border all round"
            );
        }
    }

    /// Whether two paddings are the same, to a float's precision.
    fn same(left: [f32; 2], right: [f32; 2]) -> bool {
        left.iter()
            .zip(right)
            .all(|(left, right)| (left - right).abs() < f32::EPSILON)
    }

    #[test]
    fn the_padding_tokens_are_the_csharp_values() {
        use crate::tokens::{
            BUTTON_COMPACT_PADDING, BUTTON_PADDING, INPUT_HEIGHT, TOOL_INPUT_PADDING,
        };
        // `ThemedTextBoxStyle` Padding="10,6": vertical then horizontal here.
        assert!(same(INPUT_PADDING, [6.0, 10.0]));
        // `PaddingInput` 8,6.
        assert!(same(TOOL_INPUT_PADDING, [6.0, 8.0]));
        // `PrimaryButtonStyle` and `SecondaryButtonStyle` Padding="16,8".
        assert!(same(BUTTON_PADDING, [8.0, 16.0]));
        // `PaddingButtonCopy` 10,4.
        assert!(same(BUTTON_COMPACT_PADDING, [4.0, 10.0]));
        // The measured C# field, 33 px: the line fills what the padding leaves.
        assert!((INPUT_LINE_HEIGHT + 2.0 * INPUT_PADDING[0] - INPUT_HEIGHT).abs() < f32::EPSILON);
    }

    #[test]
    fn the_scrim_is_the_window_background_at_seventy_percent_and_light_on_a_light_theme() {
        for each in AppTheme::ALL {
            let theme = themes::theme(each, Accent::Default);
            let veil = scrim(&theme);
            let background = theme.palette().background;
            assert!((veil.a - 0.7).abs() < f32::EPSILON, "{each:?}");
            assert_eq!(
                (veil.r, veil.g, veil.b),
                (background.r, background.g, background.b),
                "{each:?}"
            );
        }
        let light = scrim(&themes::theme(AppTheme::Parchment, Accent::Default));
        assert!(
            light.r > 0.5 && light.g > 0.5 && light.b > 0.5,
            "a light veil"
        );
    }

    #[test]
    fn the_text_on_a_status_fill_reads_at_4_5_to_1_in_every_theme() {
        let mut checked = 0;
        for each in AppTheme::ALL {
            let theme = themes::theme(each, Accent::Default);
            let extended = theme.extended_palette();
            for fill in [
                extended.success.base.color,
                extended.warning.base.color,
                extended.danger.base.color,
            ] {
                let ratio = contrast(themes::text_on(fill), fill);
                assert!(ratio >= SMALL_TEXT_CONTRAST, "{each:?}: {ratio:.2}:1");
                checked += 1;
            }
        }
        assert_eq!(checked, AppTheme::ALL.len() * 3);
    }
}
