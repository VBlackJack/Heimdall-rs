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

//! The Settings page's Gateways tab: the C# Gateways list and its Overview in one place.
//! Each gateway with its address, its parent and the sessions going through it, edited or
//! deleted here; then the references to gateways that are not configured, reassigned to
//! one that is, or cleared.

use std::collections::BTreeMap;

use heimdall_app::{
    GatewayEntry, GatewayOverview, GatewaysMessage, Message as AppMessage, MissingGateway,
    RoutedSession, server_text,
};
use heimdall_core::profile::{ProfileId, SshGateway};
use iced::widget::{Column, button, column, container, pick_list, row, text};
use iced::{Alignment, Element, Length};

use crate::i18n::fl;
use crate::shell::Message;

const SPACING: f32 = 8.0;
const PADDING: f32 = 12.0;
/// Widest a card grows, as the other Settings cards.
const CARD_WIDTH: f32 = 720.0;
const BODY_SIZE: f32 = 16.0;
const SMALL_SIZE: f32 = 12.0;

/// A configured gateway in the reassignment list: its name and address.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Choice {
    id: ProfileId,
    label: String,
}

impl std::fmt::Display for Choice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label)
    }
}

/// `host:port`, the host made safe to show.
fn address(host: &str, port: u16) -> String {
    format!("{}:{port}", server_text(host))
}

/// The tab: what it holds in a line, then the gateways, then what refers to a gateway
/// that is not configured. `picked`, the gateway chosen for each missing one.
pub fn view<'a>(
    overview: GatewayOverview,
    gateways: &[SshGateway],
    picked: &BTreeMap<ProfileId, ProfileId>,
) -> Column<'a, Message> {
    let choices: Vec<Choice> = gateways
        .iter()
        .map(|gateway| Choice {
            id: gateway.id.clone(),
            label: format!(
                "{} ({})",
                server_text(&gateway.name),
                address(&gateway.host, gateway.port)
            ),
        })
        .collect();
    let mut page = column![
        text(fl!("ui-gateways-title")).size(BODY_SIZE),
        text(fl!("ui-gateways-description")).size(SMALL_SIZE),
        text(fl!(
            "ui-gateways-summary",
            gateways = overview.gateways.len(),
            routed = overview.routed(),
            unresolved = overview.unresolved()
        ))
        .size(SMALL_SIZE),
        container(
            row![
                text(fl!("ui-gateways-configured")).size(BODY_SIZE),
                iced::widget::space::horizontal(),
                button(text(fl!("ui-gateway-add"))).on_press(Message::App(AppMessage::NewGateway)),
            ]
            .align_y(Alignment::Center)
        )
        .max_width(CARD_WIDTH),
    ]
    .spacing(SPACING);
    if overview.gateways.is_empty() {
        page = page.push(text(fl!("ui-gateways-empty")));
    }
    for entry in overview.gateways {
        page = page.push(gateway_card(&entry));
    }
    if !overview.missing.is_empty() {
        page = page
            .push(text(fl!("ui-gateways-unresolved")).size(BODY_SIZE))
            .push(text(fl!("ui-gateways-missing-description")).size(SMALL_SIZE));
        for missing in overview.missing {
            let chosen = picked
                .get(&missing.id)
                .and_then(|id| choices.iter().find(|choice| choice.id == *id).cloned());
            page = page.push(missing_card(missing, choices.clone(), chosen));
        }
    }
    page
}

/// A card of the tab.
fn card(content: Column<'_, Message>) -> Element<'_, Message> {
    container(content.spacing(SPACING / 2.0))
        .padding(PADDING)
        .max_width(CARD_WIDTH)
        .width(Length::Fill)
        .style(container::bordered_box)
        .into()
}

/// The sessions of a gateway, one a line, with their protocol.
fn sessions<'a>(
    mut content: Column<'a, Message>,
    sessions: &[RoutedSession],
) -> Column<'a, Message> {
    for session in sessions {
        content = content.push(
            text(format!(
                "{}  {}",
                session.kind.label(),
                server_text(&session.name)
            ))
            .size(SMALL_SIZE),
        );
    }
    content
}

/// A configured gateway: its name, address and parent, its sessions, Edit and Delete.
fn gateway_card<'a>(entry: &GatewayEntry) -> Element<'a, Message> {
    let mut content = column![
        row![
            text(server_text(&entry.name)).size(BODY_SIZE),
            text(address(&entry.host, entry.port)).size(SMALL_SIZE),
            iced::widget::space::horizontal(),
            text(fl!("ui-gateways-sessions", count = entry.sessions.len())).size(SMALL_SIZE),
            button(text(fl!("ui-gateways-edit")))
                .style(button::secondary)
                .on_press(Message::App(AppMessage::EditGateway(entry.id.clone()))),
            button(text(fl!("ui-gateways-delete")))
                .style(button::danger)
                .on_press(Message::App(AppMessage::Gateways(
                    GatewaysMessage::AskDelete(entry.id.clone())
                ))),
        ]
        .spacing(SPACING)
        .align_y(Alignment::Center),
    ];
    if let Some(parent) = &entry.parent {
        content = content
            .push(text(fl!("ui-gateways-parent", name = server_text(parent))).size(SMALL_SIZE));
    }
    if entry.sessions.is_empty() {
        content = content.push(text(fl!("ui-gateways-no-sessions")).size(SMALL_SIZE));
    }
    card(sessions(content, &entry.sessions))
}

/// References to a gateway that is not configured: the sessions, reassigned to the gateway
/// picked or cleared; the child gateways, each edited on its own.
fn missing_card<'a>(
    missing: MissingGateway,
    choices: Vec<Choice>,
    chosen: Option<Choice>,
) -> Element<'a, Message> {
    let id = missing.id.clone();
    let mut content = column![
        text(fl!(
            "ui-gateways-missing-header",
            id = server_text(missing.id.as_str())
        ))
        .size(BODY_SIZE),
    ];
    if !missing.sessions.is_empty() {
        content = content.push(
            text(fl!("ui-gateways-sessions", count = missing.sessions.len())).size(SMALL_SIZE),
        );
    }
    content = sessions(content, &missing.sessions);
    for (child, name) in missing.gateways {
        content = content.push(
            row![
                text(fl!("ui-gateways-child", name = server_text(&name))).size(SMALL_SIZE),
                button(text(fl!("ui-gateways-edit")).size(SMALL_SIZE))
                    .style(button::secondary)
                    .on_press(Message::App(AppMessage::EditGateway(child))),
            ]
            .spacing(SPACING)
            .align_y(Alignment::Center),
        );
    }
    if !missing.sessions.is_empty() {
        let reassign = chosen.as_ref().map(|choice| {
            Message::App(AppMessage::Gateways(GatewaysMessage::Reassign {
                missing: id.clone(),
                to: choice.id.clone(),
            }))
        });
        let picking = id.clone();
        let mut actions = row![].spacing(SPACING).align_y(Alignment::Center);
        if !choices.is_empty() {
            actions = actions
                .push(
                    pick_list(choices, chosen, move |choice: Choice| {
                        Message::GatewayReassignPicked {
                            missing: picking.clone(),
                            to: choice.id,
                        }
                    })
                    .placeholder(fl!("ui-gateways-reassign-to")),
                )
                .push(button(text(fl!("ui-gateways-reassign"))).on_press_maybe(reassign));
        }
        actions = actions.push(
            button(text(fl!("ui-gateways-clear")))
                .style(button::secondary)
                .on_press(Message::App(AppMessage::Gateways(GatewaysMessage::Clear(
                    id,
                )))),
        );
        content = content.push(actions);
    }
    card(content)
}
