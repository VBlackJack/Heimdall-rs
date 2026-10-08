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
//! connects and its folder; a card of what is known of its account; Connect, then Edit and
//! Delete, and their keys.

use heimdall_app::{
    GatewayBadge, Message as AppMessage, ProfileSummary, SavedCredentials, SessionState,
    server_text,
};
use iced::widget::{Column, button, column, container, row, space, text};
use iced::{Alignment, Element, Font, Length, Theme, font};

use crate::i18n::fl;
use crate::shell::Message;

/// Space around the panel, as the C# `SpacingXl`.
const PADDING: f32 = 24.0;
/// Space between parts of a line, and between the buttons.
const SPACING: f32 = 8.0;
/// Size of the session's name, as the C# `FontSizeDisplay`.
const TITLE_SIZE: f32 = 22.0;
/// Size of where it connects, as the C# `FontSizeBodyLarge`.
const ADDRESS_SIZE: f32 = 14.0;
/// Size of the folder's line, as the C# `FontSizeBody`.
const BODY_SIZE: f32 = 13.0;
/// Size of the protocol, the state and the card's lines, as the C# `FontSizeCaption`.
const CAPTION_SIZE: f32 = 12.0;
/// Size of the shortcut hint, as the C# `FontSizeSmallCaption`.
const HINT_SIZE: f32 = 11.0;
/// Corner radius of the protocol's pill and of the card, as the C# `CornerRadiusSm`.
const RADIUS: f32 = 4.0;
/// Diameter of the state's dot, as the C# panel's.
const DOT_SIDE: f32 = 8.0;
/// Width of Connect, as the C# `MinWidth`.
const CONNECT_WIDTH: f32 = 120.0;

/// The window's font, bold, as the C# panel's title.
const BOLD: Font = Font {
    weight: font::Weight::Bold,
    ..Font::with_name(crate::UI_FONT_FAMILY)
};

/// The window's font, semi-bold, as the C# protocol pill's.
const SEMIBOLD: Font = Font {
    weight: font::Weight::Semibold,
    ..Font::with_name(crate::UI_FONT_FAMILY)
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

/// The protocol of `profile`, its name on its colour, as the C# badge.
fn pill<'a>(profile: &ProfileSummary) -> Element<'a, Message> {
    let kind = profile.kind;
    container(text(kind.label()).size(CAPTION_SIZE).font(SEMIBOLD))
        .padding([2.0, 6.0])
        .style(move |theme: &Theme| container::Style {
            // The C# `BadgeTextBrush`: the background's colour on the protocol's.
            text_color: Some(theme.palette().background),
            background: Some(crate::icons::protocol_color(theme, kind).into()),
            border: iced::Border {
                radius: RADIUS.into(),
                ..iced::Border::default()
            },
            ..container::Style::default()
        })
        .into()
}

/// The state of the session's tab, as the C# panel words it: "Disconnected" when none is
/// open.
fn status<'a>(state: Option<SessionState>) -> Element<'a, Message> {
    let said = state.map_or_else(
        || fl!("ui-status-disconnected"),
        crate::floating_view::state_text,
    );
    row![
        crate::tree_view::sized_state_dot(state, DOT_SIDE),
        text(said).size(CAPTION_SIZE).style(text::secondary),
    ]
    .spacing(SPACING * 0.75)
    .align_y(Alignment::Center)
    .into()
}

/// A line under the address: its label, then what it says, in the secondary colour.
fn line<'a>(label: String, value: String) -> Element<'a, Message> {
    row![
        text(label).size(BODY_SIZE).style(text::secondary),
        text(value).size(BODY_SIZE).style(text::secondary),
    ]
    .spacing(SPACING / 2.0)
    .into()
}

/// What is known of the account, labelled, as the C# card: username, gateway, credentials
/// kept, tags, favourite. `None` when nothing is.
fn card<'a>(
    profile: &ProfileSummary,
    saved: Option<&SavedCredentials>,
) -> Option<Element<'a, Message>> {
    let mut fields: Vec<(String, String, bool)> = Vec::new();
    if let Some(user) = profile.username.as_deref().filter(|user| !user.is_empty()) {
        fields.push((fl!("ui-detail-username"), server_text(user), false));
    }
    if let Some(gateway) = &profile.gateway {
        let (said, missing) = match gateway {
            GatewayBadge::Via(name) => (server_text(name), false),
            GatewayBadge::Missing => (fl!("ui-tree-gateway-missing"), true),
        };
        fields.push((fl!("ui-detail-gateway"), said, missing));
    }
    if let Some(saved) = saved.filter(|saved| !saved.is_empty()) {
        fields.push((fl!("ui-detail-credentials"), credentials(saved), false));
    }
    if !profile.metadata.tags.is_empty() {
        fields.push((
            fl!("ui-detail-tags"),
            server_text(&profile.metadata.tags),
            false,
        ));
    }
    if profile.favorite {
        fields.push((fl!("ui-detail-favorite"), fl!("ui-detail-yes"), false));
    }
    if fields.is_empty() {
        return None;
    }
    // Two columns, as the C# grid: the labels as wide as the widest.
    let mut labels = Column::new().spacing(SPACING / 2.0);
    let mut values = Column::new().spacing(SPACING / 2.0).width(Length::Fill);
    for (label, value, warning) in fields {
        labels = labels.push(text(label).size(CAPTION_SIZE).style(text::secondary));
        values = values.push(text(value).size(CAPTION_SIZE).style(if warning {
            text::warning
        } else {
            text::default
        }));
    }
    Some(
        container(row![labels, values].spacing(SPACING * 1.5))
            .padding([SPACING, SPACING * 1.5])
            .width(Length::Fill)
            .style(|theme: &Theme| {
                let palette = theme.extended_palette();
                container::Style {
                    background: Some(palette.background.weak.color.into()),
                    border: iced::Border {
                        color: palette.background.strong.color,
                        width: 1.0,
                        radius: RADIUS.into(),
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
        text(server_text(&profile.name)).size(TITLE_SIZE).font(BOLD),
        gap(SPACING),
        row![pill(profile), status(state)]
            .spacing(SPACING)
            .align_y(Alignment::Center),
    ];
    if let Some((host, port)) = &profile.endpoint {
        panel = panel.push(gap(SPACING * 1.5)).push(
            text(format!("{}:{port}", server_text(host)))
                .size(ADDRESS_SIZE)
                .style(text::secondary),
        );
    }
    if let Some(group) = &profile.group {
        panel = panel
            .push(gap(SPACING / 2.0))
            .push(line(fl!("ui-detail-folder"), server_text(group)));
    }
    if let Some(environment) = profile.metadata.environment {
        panel = panel.push(gap(SPACING / 2.0)).push(line(
            fl!("ui-detail-environment"),
            crate::texts::environment_name(Some(environment)),
        ));
    }
    if let Some(card) = card(profile, saved) {
        panel = panel.push(gap(SPACING * 1.5)).push(card);
    }
    panel = panel
        .push(gap(SPACING * 2.5))
        .push(
            // Never narrower than the C# minimum, wider when its label is.
            button(
                column![
                    text(fl!("ui-detail-connect")).wrapping(text::Wrapping::None),
                    space().width(CONNECT_WIDTH - SPACING * 4.0),
                ]
                .align_x(Alignment::Center),
            )
            .padding([SPACING, SPACING * 2.0])
            .on_press(Message::App(AppMessage::ConnectProfile(id.clone()))),
        )
        .push(gap(SPACING))
        .push(
            row![
                button(text(fl!("ui-detail-edit")))
                    .style(button::secondary)
                    .padding([SPACING * 0.75, SPACING * 1.5])
                    .on_press_maybe(
                        editable.then(|| Message::App(AppMessage::EditProfile(id.clone())))
                    ),
                button(text(fl!("ui-tree-delete")))
                    .style(button::danger)
                    .padding([SPACING * 0.75, SPACING * 1.5])
                    .on_press(Message::App(AppMessage::RequestDeleteProfile(id))),
            ]
            .spacing(SPACING),
        )
        .push(gap(SPACING))
        .push(
            text(fl!("ui-detail-hints"))
                .size(HINT_SIZE)
                .style(text::secondary),
        );
    container(panel)
        .padding(PADDING)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}
