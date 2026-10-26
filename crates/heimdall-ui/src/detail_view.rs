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

//! The session selected, in place of a session when none is open, as the C# detail panel:
//! at the top left, its name, then the same card the properties panel shows (the C# panel
//! hosts the same control twice): its protocol in the protocol's colour and its state, where
//! it connects as "host:port", and what is known of it, authentication first; Connect, then
//! Edit and Delete, and their keys.

use heimdall_app::{Message as AppMessage, ProfileKind, SessionState, server_text};
use iced::widget::{button, column, container, row, space, text};
use iced::{Alignment, Element, Font, Length, Theme, font};

use crate::i18n::fl;
use crate::properties_panel::{self, Facts};
use crate::shell::Message;
use crate::styles;
use crate::tokens::{BORDER_WIDTH, font_size, radius, spacing};

/// Diameter of the state's dot, as the C# panel's.
pub(crate) const DOT_SIDE: f32 = 8.0;
/// Width of Connect, as the C# `MinWidth`.
const CONNECT_WIDTH: f32 = 120.0;
/// Space the C# panel leaves of 6 pixels: after the state's dot, above and below Edit's and
/// Delete's labels, and on each side of the protocol's name.
pub(crate) const SMALL_GAP: f32 = 6.0;
/// Space above the card of the session's properties, as the C# card's top margin.
const CARD_GAP: f32 = 10.0;
/// Space above and below the protocol's name, as the C# pill's padding.
const PILL_PADDING_Y: f32 = 2.0;
/// Space on each side of Connect's label, as the C# primary button's padding.
const CONNECT_PADDING_X: f32 = 16.0;

/// The window's font, bold, as the C# panel's title.
pub(crate) const BOLD: Font = Font {
    weight: font::Weight::Bold,
    ..crate::UI_FONT
};

/// The window's font, semi-bold, as the C# protocol pill's.
const SEMIBOLD: Font = Font {
    weight: font::Weight::Semibold,
    ..crate::UI_FONT
};

/// Space of `height` between two parts of the panel, as the C# margins.
fn gap<'a>(height: f32) -> Element<'a, Message> {
    space().height(height).into()
}

/// The text and the fill of the pill of protocol `kind` in `theme`.
pub(crate) fn pill_colors(theme: &Theme, kind: ProfileKind) -> (iced::Color, iced::Color) {
    let fill = crate::icons::protocol_color(theme, kind);
    // The C# `BadgeTextBrush`: the background's colour on the protocol's; black or white
    // where that leaves the name under 4.5:1.
    (
        crate::themes::label_on(theme.palette().background, fill),
        fill,
    )
}

/// Protocol `kind`, its name on its colour, as the C# badge.
pub(crate) fn pill<'a>(kind: ProfileKind) -> Element<'a, Message> {
    container(text(kind.label()).size(font_size::CAPTION).font(SEMIBOLD))
        .padding([PILL_PADDING_Y, SMALL_GAP])
        .style(move |theme: &Theme| container::Style {
            text_color: Some(pill_colors(theme, kind).0),
            background: Some(pill_colors(theme, kind).1.into()),
            border: iced::Border {
                radius: radius::SM.into(),
                ..iced::Border::default()
            },
            ..container::Style::default()
        })
        .into()
}

/// The state of the session's tab, as the C# panel words it: "Disconnected" when none is
/// open.
pub(crate) fn status<'a>(state: Option<SessionState>) -> Element<'a, Message> {
    let said = state.map_or_else(
        || fl!("ui-status-disconnected"),
        crate::floating_view::state_text,
    );
    row![
        crate::tree_view::sized_state_dot(state, DOT_SIDE),
        text(said).size(font_size::CAPTION).style(text::secondary),
    ]
    .spacing(SMALL_GAP)
    .align_y(Alignment::Center)
    .into()
}

/// The card's look, as the C# `CardBrush` in `BorderStrongBrush`, rounded as `CornerRadiusSm`:
/// the detail panel's and the properties panel's.
pub(crate) fn card_style(theme: &Theme) -> container::Style {
    let palette = theme.extended_palette();
    container::Style {
        background: Some(palette.background.weak.color.into()),
        border: iced::Border {
            color: crate::themes::derived(theme, crate::themes::Derived::BorderStrong),
            width: BORDER_WIDTH,
            radius: radius::SM.into(),
        },
        ..container::Style::default()
    }
}

/// The panel of the session `facts` describe; `state`, the state of its session's tab, when
/// one is open; `editable`, it can be edited.
pub fn view<'a>(
    facts: Facts<'_>,
    state: Option<SessionState>,
    editable: bool,
) -> Element<'a, Message> {
    let profile = facts.profile;
    let id = profile.id.clone();
    let mut panel = column![
        text(server_text(&profile.name))
            .size(font_size::DISPLAY)
            .font(BOLD),
        gap(spacing::SM),
        row![pill(profile.kind), status(state)]
            .spacing(spacing::SM)
            .align_y(Alignment::Center),
    ];
    if let Some(endpoint) = properties_panel::endpoint(profile) {
        panel = panel
            .push(gap(spacing::SM))
            .push(properties_panel::address(endpoint));
    }
    let lines = properties_panel::lines(facts);
    if !lines.is_empty() {
        panel = panel
            .push(gap(CARD_GAP))
            .push(properties_panel::card(&lines));
    }
    panel = panel
        .push(gap(spacing::LG))
        .push(
            // Never narrower than the C# minimum, wider when its label is.
            button(
                column![
                    text(fl!("ui-detail-connect")).wrapping(text::Wrapping::None),
                    space().width(CONNECT_WIDTH - CONNECT_PADDING_X * 2.0),
                ]
                .align_x(Alignment::Center),
            )
            .style(styles::primary)
            .padding([spacing::SM, CONNECT_PADDING_X])
            .on_press(Message::App(AppMessage::ConnectProfile(id.clone()))),
        )
        .push(gap(spacing::SM))
        .push(
            row![
                button(text(fl!("ui-detail-edit")))
                    .style(styles::secondary)
                    .padding([SMALL_GAP, spacing::MD])
                    .on_press_maybe(
                        editable.then(|| Message::App(AppMessage::EditProfile(id.clone())))
                    ),
                button(text(fl!("ui-tree-delete")))
                    .style(styles::danger)
                    .padding([SMALL_GAP, spacing::MD])
                    .on_press(Message::App(AppMessage::RequestDeleteProfile(id))),
            ]
            .spacing(spacing::SM),
        )
        .push(gap(spacing::SM))
        .push(
            text(fl!("ui-detail-hints"))
                .size(font_size::SMALL_CAPTION)
                .style(text::secondary),
        );
    container(panel)
        .padding(spacing::XL)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}
