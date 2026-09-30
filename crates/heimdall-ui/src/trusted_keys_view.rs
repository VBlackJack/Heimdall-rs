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

//! The keys trusted for servers on the Settings page, as the C# Host keys and Certificates
//! pages list them: searched, copied, forgotten after a question.

use heimdall_app::{Message as AppMessage, SettingsMessage, TrustedKey, TrustedKeysMessage};
use heimdall_core::profile::display_address;
use iced::widget::{Column, button, column, container, row, text, text_input, tooltip};
use iced::{Alignment, Element, Length};

use crate::i18n::fl;
use crate::shell::Message;

/// Space between elements.
const SPACING: f32 = 8.0;
/// Inner margin of a card.
const PADDING: f32 = 12.0;
/// Size of hints and cells.
const SMALL_SIZE: f32 = 12.0;
/// Size of a card's title.
const TITLE_SIZE: f32 = 16.0;
/// Size of a question's title.
const HEADING_SIZE: f32 = 20.0;
/// Characters of an SSH fingerprint shown before the ellipsis, as the C# list shows it.
const SSH_FINGERPRINT_SHOWN: usize = 16;
/// Characters of an RDP fingerprint shown before the ellipsis.
const RDP_FINGERPRINT_SHOWN: usize = 20;
/// What stands for the rest of a fingerprint cut short.
const ELLIPSIS: &str = "...";

/// Which list a search box filters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrustedList {
    /// SSH host keys.
    HostKeys,
    /// RDP certificates.
    Certificates,
}

fn trusted(message: TrustedKeysMessage) -> Message {
    Message::App(AppMessage::Settings(SettingsMessage::TrustedKeys(message)))
}

/// `fingerprint` cut after `shown` characters, the whole of it in a tooltip.
fn fingerprint_cell<'a>(fingerprint: &str, shown: usize) -> Element<'a, Message> {
    let cut: String = if fingerprint.chars().count() > shown {
        fingerprint
            .chars()
            .take(shown)
            .chain(ELLIPSIS.chars())
            .collect()
    } else {
        fingerprint.to_owned()
    };
    tooltip(
        text(cut).size(SMALL_SIZE),
        container(text(fingerprint.to_owned()).size(SMALL_SIZE))
            .padding(SPACING / 2.0)
            .style(container::rounded_box),
        tooltip::Position::Top,
    )
    .into()
}

/// A card: its title, its hint, its search box, then `body`.
fn card<'a>(
    title: String,
    hint: String,
    search: (TrustedList, &str, String),
    body: Element<'a, Message>,
) -> Element<'a, Message> {
    let (list, typed, placeholder) = search;
    container(
        column![
            text(title).size(TITLE_SIZE),
            text(hint).size(SMALL_SIZE),
            text_input(&placeholder, typed)
                .on_input(move |typed| Message::TrustedSearch(list, typed)),
            body,
        ]
        .spacing(SPACING),
    )
    .padding(PADDING)
    .style(container::bordered_box)
    .into()
}

/// What stands in an empty list: its title and what to do.
fn empty<'a>(title: String, body: String) -> Element<'a, Message> {
    column![text(title), text(body).size(SMALL_SIZE)]
        .spacing(SPACING / 2.0)
        .into()
}

/// Whether `candidate` holds `typed`, whatever the case.
fn matches(candidate: &str, typed: &str) -> bool {
    candidate
        .to_lowercase()
        .contains(&typed.trim().to_lowercase())
}

/// The trusted SSH host keys, `search` typed: host and port, algorithm, fingerprint, and a
/// copy and a remove button per key.
pub fn host_keys<'a>(keys: &'a heimdall_app::TrustedKeys, search: &'a str) -> Element<'a, Message> {
    let body: Element<'a, Message> = if keys.ssh.is_empty() {
        empty(
            fl!("ui-trusted-host-keys-empty-title"),
            fl!("ui-trusted-host-keys-empty-body"),
        )
    } else {
        let mut rows = Column::new().spacing(SPACING / 2.0).push(
            row![
                header(fl!("ui-trusted-host-keys-host"), 3),
                header(fl!("ui-trusted-host-keys-algorithm"), 2),
                header(fl!("ui-trusted-host-keys-fingerprint"), 3),
                header(String::new(), 3),
            ]
            .spacing(SPACING),
        );
        for entry in &keys.ssh {
            let address = display_address(&entry.host, entry.port);
            if !matches(&address, search) {
                continue;
            }
            let key = TrustedKey::Ssh(entry.clone());
            rows = rows.push(
                row![
                    cell(text(address).size(SMALL_SIZE).into(), 3),
                    cell(text(entry.algorithm.clone()).size(SMALL_SIZE).into(), 2),
                    cell(
                        fingerprint_cell(&entry.fingerprint, SSH_FINGERPRINT_SHOWN),
                        3
                    ),
                    cell(
                        row![
                            small_button(
                                fl!("ui-trusted-host-keys-copy"),
                                trusted(TrustedKeysMessage::CopyFingerprint(key.clone())),
                                button::secondary,
                            ),
                            small_button(
                                fl!("ui-trusted-host-keys-remove"),
                                trusted(TrustedKeysMessage::RequestForget(key)),
                                button::danger,
                            ),
                        ]
                        .spacing(SPACING / 2.0)
                        .into(),
                        3,
                    ),
                ]
                .spacing(SPACING)
                .align_y(Alignment::Center),
            );
        }
        rows.into()
    };
    // As the C# section: the import beside the list it adds to.
    let body = column![
        small_button(
            fl!("ui-trusted-host-keys-import"),
            crate::hostkeys_view::app(heimdall_app::HostKeysMessage::Start),
            button::secondary,
        ),
        body,
    ]
    .spacing(SPACING)
    .into();
    card(
        fl!("ui-trusted-host-keys-title"),
        fl!("ui-trusted-host-keys-hint"),
        (
            TrustedList::HostKeys,
            search,
            fl!("ui-trusted-host-keys-search"),
        ),
        body,
    )
}

/// The trusted RDP certificates, `search` typed: server, fingerprint, and a forget button.
pub fn certificates<'a>(
    keys: &'a heimdall_app::TrustedKeys,
    search: &'a str,
) -> Element<'a, Message> {
    let body: Element<'a, Message> = if keys.rdp.is_empty() {
        empty(
            fl!("ui-trusted-certificates-empty-title"),
            fl!("ui-trusted-certificates-empty-body"),
        )
    } else {
        let mut rows = Column::new().spacing(SPACING / 2.0).push(
            row![
                header(fl!("ui-trusted-certificates-server"), 3),
                header(fl!("ui-trusted-certificates-fingerprint"), 4),
                header(String::new(), 2),
            ]
            .spacing(SPACING),
        );
        for entry in &keys.rdp {
            let address = display_address(&entry.host, entry.port);
            let fingerprint = entry.fingerprint.to_string();
            if !matches(&address, search) && !matches(&fingerprint, search) {
                continue;
            }
            rows = rows.push(
                row![
                    cell(text(address).size(SMALL_SIZE).into(), 3),
                    cell(fingerprint_cell(&fingerprint, RDP_FINGERPRINT_SHOWN), 4),
                    cell(
                        small_button(
                            fl!("ui-trusted-certificates-forget"),
                            trusted(TrustedKeysMessage::RequestForget(TrustedKey::Rdp(
                                entry.clone()
                            ))),
                            button::danger,
                        ),
                        2,
                    ),
                ]
                .spacing(SPACING)
                .align_y(Alignment::Center),
            );
        }
        rows.into()
    };
    card(
        fl!("ui-trusted-certificates-title"),
        fl!("ui-trusted-certificates-hint"),
        (
            TrustedList::Certificates,
            search,
            fl!("ui-trusted-certificates-search"),
        ),
        body,
    )
}

/// Why the lists could not be read, when they could not.
pub fn unreadable<'a>(keys: &heimdall_app::TrustedKeys) -> Option<Element<'a, Message>> {
    let detail = keys.unreadable.as_deref()?;
    Some(
        text(fl!("ui-trusted-keys-unreadable", detail = detail))
            .size(SMALL_SIZE)
            .style(text::danger)
            .into(),
    )
}

fn header<'a>(label: String, portion: u16) -> Element<'a, Message> {
    container(text(label).size(SMALL_SIZE))
        .width(Length::FillPortion(portion))
        .into()
}

fn cell(content: Element<'_, Message>, portion: u16) -> Element<'_, Message> {
    container(content)
        .width(Length::FillPortion(portion))
        .into()
}

fn small_button<'a>(
    label: String,
    message: Message,
    style: impl Fn(&iced::Theme, button::Status) -> button::Style + 'a,
) -> Element<'a, Message> {
    button(text(label).size(SMALL_SIZE))
        .style(style)
        .on_press(message)
        .into()
}

/// The question before forgetting `key`, in the C# words.
pub fn forget_question(key: &TrustedKey) -> Element<'_, Message> {
    let server = key.address();
    let fingerprint = key.fingerprint();
    let (title, body, keep, forget) = match key {
        TrustedKey::Ssh(_) => (
            fl!("ui-dialog-forget-host-key-title"),
            [
                fl!("ui-dialog-forget-host-key-body", server = server.as_str()),
                fl!(
                    "ui-dialog-forget-host-key-fingerprint",
                    fingerprint = fingerprint.as_str()
                ),
                fl!(
                    "ui-dialog-forget-host-key-consequence",
                    server = server.as_str()
                ),
            ]
            .join("\n\n"),
            fl!("ui-dialog-cancel-button"),
            fl!("ui-dialog-forget-host-key-confirm"),
        ),
        TrustedKey::Rdp(_) => (
            fl!("ui-dialog-forget-certificate-title"),
            fl!(
                "ui-dialog-forget-certificate-body",
                server = server.as_str(),
                fingerprint = fingerprint.as_str()
            ),
            fl!("ui-dialog-forget-certificate-keep"),
            fl!("ui-dialog-forget-certificate-confirm"),
        ),
    };
    column![
        text(title).size(HEADING_SIZE),
        text(body),
        row![
            button(text(keep))
                .style(button::secondary)
                .on_press(Message::App(AppMessage::DismissDialog)),
            button(text(forget))
                .style(button::danger)
                .on_press(Message::App(AppMessage::ConfirmDialog)),
        ]
        .spacing(SPACING),
    ]
    .spacing(SPACING)
    .into()
}
