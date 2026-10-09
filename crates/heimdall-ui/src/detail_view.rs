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
//! at the top left, its name, its protocol in the protocol's colour and its state; where it
//! connects, as "host : port", and its folder; a card of what is known of its account, a
//! favourite marked with the star; Connect, then Edit and Delete, and their keys.

use heimdall_app::{
    GatewayBadge, Message as AppMessage, ProfileKind, ProfileSummary, SavedCredentials,
    SessionState, server_text,
};
use iced::widget::{Column, button, column, container, row, space, text};
use iced::{Alignment, Element, Font, Length, Theme, font};

use crate::i18n::fl;
use crate::icons::{self, Icon, Tint};
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
/// Space above and below the protocol's name, as the C# pill's padding.
const PILL_PADDING_Y: f32 = 2.0;
/// Space on each side of Connect's label, as the C# primary button's padding.
const CONNECT_PADDING_X: f32 = 16.0;
/// Side of the favourite's star, as the C# glyph at `FontSizeBody`.
const FAVORITE_SIDE: f32 = font_size::BODY;
/// Height of a line of text for its size, as iced lays a line out by default: the star is
/// centred on a line of the card.
const LINE_HEIGHT: f32 = 1.3;

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

/// The credentials kept, as the C# line lists them.
fn credentials(saved: &SavedCredentials) -> String {
    let mut kept = Vec::new();
    if saved.password {
        kept.push(fl!("ui-detail-saved-password"));
    }
    if let Some(key) = &saved.key_file {
        kept.push(fl!("ui-detail-saved-key", name = server_text(key)));
    }
    if saved.passphrase {
        kept.push(fl!("ui-detail-saved-passphrase"));
    }
    kept.join(", ")
}

/// Protocol `kind`, its name on its colour, as the C# badge.
pub(crate) fn pill<'a>(kind: ProfileKind) -> Element<'a, Message> {
    container(text(kind.label()).size(font_size::CAPTION).font(SEMIBOLD))
        .padding([PILL_PADDING_Y, SMALL_GAP])
        .style(move |theme: &Theme| container::Style {
            // The C# `BadgeTextBrush`: the background's colour on the protocol's.
            text_color: Some(theme.palette().background),
            background: Some(crate::icons::protocol_color(theme, kind).into()),
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

/// A line under the address: its label, then what it says, in the secondary colour.
fn line<'a>(label: String, value: String) -> Element<'a, Message> {
    row![
        text(label).size(font_size::BODY).style(text::secondary),
        text(value).size(font_size::BODY).style(text::secondary),
    ]
    .spacing(spacing::XS)
    .into()
}

/// What a line of the card says: a text, in the warning colour when `warning`, or the
/// favourite's star.
enum Value {
    /// A text.
    Text { said: String, warning: bool },
    /// The C# `FavoriteStarFill` glyph, in the warning colour.
    Star,
}

impl Value {
    /// A text in the window's colour.
    fn plain(said: String) -> Self {
        Self::Text {
            said,
            warning: false,
        }
    }

    /// It drawn, as the C# card's second column.
    fn view<'a>(self) -> Element<'a, Message> {
        match self {
            Self::Text { said, warning } => text(said)
                .size(font_size::CAPTION)
                .style(if warning {
                    text::warning
                } else {
                    text::default
                })
                .into(),
            // As the C# `FontSizeBody` glyph in `WarningTextBrush`, centred on its line.
            Self::Star => container(icons::icon(
                Icon::FavoriteStarFill,
                Tint::Warning,
                FAVORITE_SIDE,
            ))
            .center_y(font_size::CAPTION * LINE_HEIGHT)
            .into(),
        }
    }
}

/// What is known of the account, labelled, as the C# card: username, gateway, credentials
/// kept, tags, and the favourite's star. `None` when nothing is.
fn card<'a>(
    profile: &ProfileSummary,
    saved: Option<&SavedCredentials>,
) -> Option<Element<'a, Message>> {
    let mut fields: Vec<(String, Value)> = Vec::new();
    if let Some(user) = profile.username.as_deref().filter(|user| !user.is_empty()) {
        fields.push((fl!("ui-detail-username"), Value::plain(server_text(user))));
    }
    if let Some(gateway) = &profile.gateway {
        let (said, warning) = match gateway {
            GatewayBadge::Via(name) => (server_text(name), false),
            GatewayBadge::Missing => (fl!("ui-tree-gateway-missing"), true),
        };
        fields.push((fl!("ui-detail-gateway"), Value::Text { said, warning }));
    }
    if let Some(saved) = saved.filter(|saved| !saved.is_empty()) {
        fields.push((
            fl!("ui-detail-credentials"),
            Value::plain(credentials(saved)),
        ));
    }
    if !profile.metadata.tags.is_empty() {
        fields.push((
            fl!("ui-detail-tags"),
            Value::plain(server_text(&profile.metadata.tags)),
        ));
    }
    if profile.favorite {
        fields.push((fl!("ui-detail-favorite"), Value::Star));
    }
    if fields.is_empty() {
        return None;
    }
    // Two columns, as the C# grid: the labels as wide as the widest.
    let mut labels = Column::new().spacing(spacing::XS);
    let mut values = Column::new().spacing(spacing::XS).width(Length::Fill);
    for (label, value) in fields {
        labels = labels.push(text(label).size(font_size::CAPTION).style(text::secondary));
        values = values.push(value.view());
    }
    Some(
        container(row![labels, values].spacing(spacing::MD))
            .padding([spacing::SM, spacing::MD])
            .width(Length::Fill)
            .style(|theme: &Theme| {
                let palette = theme.extended_palette();
                container::Style {
                    background: Some(palette.background.weak.color.into()),
                    border: iced::Border {
                        color: palette.background.strong.color,
                        width: BORDER_WIDTH,
                        radius: radius::SM.into(),
                    },
                    ..container::Style::default()
                }
            })
            .into(),
    )
}

/// The panel of `profile`; `saved`, the credentials kept for it once read; `editable`, it
/// can be edited; `state`, the state of its session's tab, when one is open.
pub fn view<'a>(
    profile: &ProfileSummary,
    saved: Option<&SavedCredentials>,
    editable: bool,
    state: Option<SessionState>,
) -> Element<'a, Message> {
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
    if let Some((host, port)) = &profile.endpoint {
        panel = panel.push(gap(spacing::MD)).push(
            text(fl!(
                "ui-detail-host-port",
                host = server_text(host),
                port = port.to_string()
            ))
            .size(font_size::BODY_LARGE)
            .style(text::secondary),
        );
    }
    if let Some(group) = &profile.group {
        panel = panel
            .push(gap(spacing::XS))
            .push(line(fl!("ui-detail-folder"), server_text(group)));
    }
    if let Some(environment) = profile.metadata.environment {
        panel = panel.push(gap(spacing::XS)).push(line(
            fl!("ui-detail-environment"),
            crate::texts::environment_name(Some(environment)),
        ));
    }
    if let Some(card) = card(profile, saved) {
        panel = panel.push(gap(spacing::MD)).push(card);
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
