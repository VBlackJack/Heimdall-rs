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
//! its name and protocol, where it connects, what it says of its server, the credentials
//! kept for it; Connect, Edit and Delete, and their keys.

use heimdall_app::{
    GatewayBadge, Message as AppMessage, ProfileSummary, SavedCredentials, server_text,
};
use iced::widget::{Column, button, column, container, row, text};
use iced::{Alignment, Element, Length};

use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, spacing};

/// Widest the panel grows.
const PANEL_WIDTH: f32 = 560.0;
/// Width of a line's label.
const LABEL_WIDTH: f32 = 180.0;

/// A line of the panel: its label, then what it says.
fn line<'a>(label: String, value: String) -> Element<'a, Message> {
    row![
        text(label).size(font_size::CAPTION).width(LABEL_WIDTH),
        text(value).width(Length::Fill),
    ]
    .spacing(spacing::SM)
    .align_y(Alignment::Center)
    .into()
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

/// The panel of `profile`; `saved`, the credentials kept for it once read; `editable`, it
/// can be edited.
pub fn view<'a>(
    profile: &ProfileSummary,
    saved: Option<&SavedCredentials>,
    editable: bool,
) -> Element<'a, Message> {
    let id = profile.id.clone();
    let mut lines = Column::new().spacing(spacing::XS);
    if let Some((host, port)) = &profile.endpoint {
        lines = lines.push(text(format!("{}:{port}", server_text(host))));
    }
    if let Some(group) = &profile.group {
        lines = lines.push(line(fl!("ui-detail-folder"), server_text(group)));
    }
    if let Some(environment) = profile.metadata.environment {
        lines = lines.push(line(
            fl!("ui-detail-environment"),
            crate::texts::environment_name(Some(environment)),
        ));
    }
    if let Some(user) = profile.username.as_deref().filter(|user| !user.is_empty()) {
        lines = lines.push(line(fl!("ui-detail-username"), server_text(user)));
    }
    if let Some(gateway) = &profile.gateway {
        lines = lines.push(line(
            fl!("ui-detail-gateway"),
            match gateway {
                GatewayBadge::Via(name) => server_text(name),
                GatewayBadge::Missing => fl!("ui-tree-gateway-missing"),
            },
        ));
    }
    if let Some(saved) = saved.filter(|saved| !saved.is_empty()) {
        lines = lines.push(line(fl!("ui-detail-credentials"), credentials(saved)));
    }
    if !profile.metadata.tags.is_empty() {
        lines = lines.push(line(
            fl!("ui-detail-tags"),
            server_text(&profile.metadata.tags),
        ));
    }
    if profile.favorite {
        lines = lines.push(line(fl!("ui-detail-favorite"), fl!("ui-detail-yes")));
    }
    let actions = row![
        button(text(fl!("ui-detail-connect")))
            .style(styles::primary)
            .on_press(Message::App(AppMessage::ConnectProfile(id.clone()))),
        button(text(fl!("ui-detail-edit")))
            .style(styles::secondary)
            .on_press_maybe(editable.then(|| Message::App(AppMessage::EditProfile(id.clone())))),
        button(text(fl!("ui-tree-delete")))
            .style(styles::danger)
            .on_press(Message::App(AppMessage::RequestDeleteProfile(id))),
    ]
    .spacing(spacing::SM);
    container(
        column![
            row![
                text(server_text(&profile.name)).size(font_size::DISPLAY),
                text(profile.kind.label())
                    .size(font_size::CAPTION)
                    .style(text::secondary),
            ]
            .spacing(spacing::SM)
            .align_y(Alignment::Center),
            lines,
            actions,
            text(fl!("ui-detail-hints"))
                .size(font_size::CAPTION)
                .style(text::secondary),
        ]
        .spacing(spacing::SM * 2.0),
    )
    .padding(spacing::LG)
    .max_width(PANEL_WIDTH)
    .style(container::bordered_box)
    .into()
}
