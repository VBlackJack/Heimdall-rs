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

//! What the hash, HMAC, JWT and TOTP tools draw alike, as the C# tool views do: the empty
//! state of a page that scrolls, a line said in a colour, a box of a colour of its own.

use iced::widget::text_editor::{self, Content};
use iced::widget::{container, text};
use iced::{Background, Border, Color, Element, Length, Theme};

use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, radius};

/// How strongly a status colour fills its box, as the C# JWT expiry's 40 of 255.
pub const OVERLAY_ALPHA: f32 = 40.0 / 255.0;

/// A line said, and how: drawn in the secondary text, the success or the error colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    /// The secondary text.
    Quiet,
    /// The success colour.
    Success,
    /// The error colour.
    Error,
}

/// What a tool shows before it has a result, as the C# `ToolEmptyStateStyle`: its hint,
/// centred across. These tools' pages scroll, which [`super::empty_state`], filling the
/// height left, cannot be part of.
pub fn empty_state<'a>(hint: String) -> Element<'a, Message> {
    container(
        text(hint)
            .size(font_size::BODY_LARGE)
            .style(text::secondary),
    )
    .padding(super::EMPTY_STATE_PADDING)
    .center_x(Length::Fill)
    .into()
}

/// `said` in its tone, at `size`, semibold when `strong`, as the C# result lines.
pub fn said<'a>(said: String, tone: Tone, size: f32, strong: bool) -> Element<'a, Message> {
    let line = text(said).size(size).style(match tone {
        Tone::Quiet => text::secondary,
        Tone::Success => text::success,
        Tone::Error => text::danger,
    });
    if strong {
        line.font(styles::SEMIBOLD).into()
    } else {
        line.into()
    }
}

/// A read-only box of `content` whose text is the colour `color` picks in the theme, as the
/// C# output boxes drawn in the accent or a JWT part's colour.
pub fn tinted_box(
    content: &Content,
    color: fn(&Theme) -> Color,
) -> text_editor::TextEditor<'_, iced::advanced::text::highlighter::PlainText, Message> {
    super::text_box(content, None).style(move |theme, status| text_editor::Style {
        value: color(theme),
        ..styles::text_box(theme, status)
    })
}

/// A read-only box of `content` drawn without its own background nor edge, its text the
/// colour `color` picks, as the C# JWT parts' borderless, transparent `TextBox`.
pub fn bare_box(
    content: &Content,
    color: fn(&Theme) -> Color,
) -> text_editor::TextEditor<'_, iced::advanced::text::highlighter::PlainText, Message> {
    super::text_box(content, None).style(move |theme, status| {
        let style = styles::text_box(theme, status);
        text_editor::Style {
            value: color(theme),
            background: Background::Color(Color::TRANSPARENT),
            border: Border {
                width: 0.0,
                ..style.border
            },
            ..style
        }
    })
}

/// The accent of `theme`, as the C# `AccentBrush`.
pub fn accent(theme: &Theme) -> Color {
    theme.palette().primary
}

/// A box of the card's colour edged with the colour `edge` picks in the theme, as the C# JWT
/// parts' borders.
pub fn edged(edge: fn(&Theme) -> Color) -> impl Fn(&Theme) -> container::Style {
    move |theme| container::Style {
        background: Some(Background::Color(styles::card_color(theme))),
        border: Border {
            color: edge(theme),
            width: crate::tokens::BORDER_WIDTH,
            radius: radius::SM.into(),
        },
        ..container::Style::default()
    }
}

/// A box filled faintly with the colour `color` picks in the theme, edged and written with
/// it, as the C# JWT expiry's border.
pub fn tinted(color: fn(&Theme) -> Color) -> impl Fn(&Theme) -> container::Style {
    move |theme| {
        let color = color(theme);
        container::Style {
            background: Some(Background::Color(color.scale_alpha(OVERLAY_ALPHA))),
            border: Border {
                color,
                width: crate::tokens::BORDER_WIDTH,
                radius: radius::SM.into(),
            },
            text_color: Some(color),
            ..container::Style::default()
        }
    }
}
