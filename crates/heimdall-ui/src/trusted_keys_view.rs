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
//! pages list them: searched, copied, forgotten after a question. The FTPS certificates, which
//! the C# lists nowhere, are listed as the RDP ones.

use heimdall_app::{Message as AppMessage, SettingsMessage, TrustedKey, TrustedKeysMessage};
use heimdall_core::profile::display_address;
use heimdall_rdp::KnownRdpHost;
use iced::widget::{Column, button, column, container, row, text, text_input, tooltip};
use iced::{Alignment, Element, Length};

use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, spacing};

/// Characters of an SSH fingerprint shown before the ellipsis, as the C# list shows it.
const SSH_FINGERPRINT_SHOWN: usize = 16;
/// Characters of an RDP fingerprint shown before the ellipsis.
const RDP_FINGERPRINT_SHOWN: usize = 20;
/// Share of a host key row the copy and remove buttons take.
const ACTIONS_PORTION: u16 = 4;
/// Share of a certificate row its server takes.
const SERVER_PORTION: u16 = 3;
/// Share of a certificate row its fingerprint takes.
const FINGERPRINT_PORTION: u16 = 3;
/// Share of a certificate row its subject, and its issuer, take.
const NAME_PORTION: u16 = 3;
/// Share of a certificate row the time it was trusted takes.
const TRUSTED_PORTION: u16 = 2;
/// Share of a certificate row its forget button takes.
const FORGET_PORTION: u16 = 2;
/// What stands for the rest of a fingerprint cut short.
const ELLIPSIS: &str = "...";

/// Which list a search box filters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrustedList {
    /// SSH host keys.
    HostKeys,
    /// RDP certificates.
    Certificates,
    /// FTPS certificates.
    FtpsCertificates,
    /// VNC certificates.
    VncCertificates,
}

/// What a list of certificates says of itself: its title, its hint, and what stands in it
/// while empty.
struct CertificateTexts {
    /// The card's title.
    title: String,
    /// What the list holds.
    hint: String,
    /// The title of the empty list.
    empty_title: String,
    /// What to do while the list is empty.
    empty_body: String,
}

/// A list of certificates trusted: the protocol's, as read, with its search.
#[derive(Clone, Copy)]
struct CertificateList<'a> {
    /// Which list it is, for its search box.
    list: TrustedList,
    /// The certificates, in the order of their file.
    entries: &'a [KnownRdpHost],
    /// The key a certificate of the list is.
    key: fn(KnownRdpHost) -> TrustedKey,
    /// What is typed in its search box.
    search: &'a str,
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
        text(cut).size(font_size::CAPTION),
        container(text(fingerprint.to_owned()).size(font_size::CAPTION))
            .padding(spacing::XS)
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
            text(title).size(font_size::SUBTITLE),
            text(hint).size(font_size::CAPTION),
            text_input(&placeholder, typed)
                .style(styles::text_input)
                .on_input(move |typed| Message::TrustedSearch(list, typed)),
            body,
        ]
        .spacing(spacing::SM),
    )
    .padding(spacing::MD)
    .style(container::bordered_box)
    .into()
}

/// What stands in an empty list: its title and what to do.
fn empty<'a>(title: String, body: String) -> Element<'a, Message> {
    column![text(title), text(body).size(font_size::CAPTION)]
        .spacing(spacing::XS)
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
        let mut rows = Column::new().spacing(spacing::XS).push(
            row![
                header(fl!("ui-trusted-host-keys-host"), 3),
                header(fl!("ui-trusted-host-keys-algorithm"), 2),
                header(fl!("ui-trusted-host-keys-fingerprint"), 3),
                // Room for both buttons, whatever the language: "Copier l'empreinte" is long.
                header(String::new(), ACTIONS_PORTION),
            ]
            .spacing(spacing::SM),
        );
        for entry in &keys.ssh {
            let address = display_address(&entry.host, entry.port);
            if !matches(&address, search) {
                continue;
            }
            let key = TrustedKey::Ssh(entry.clone());
            rows = rows.push(
                row![
                    cell(text(address).size(font_size::CAPTION).into(), 3),
                    cell(
                        text(entry.algorithm.clone())
                            .size(font_size::CAPTION)
                            .into(),
                        2
                    ),
                    cell(
                        fingerprint_cell(&entry.fingerprint, SSH_FINGERPRINT_SHOWN),
                        3
                    ),
                    cell(
                        row![
                            small_button(
                                fl!("ui-trusted-host-keys-copy"),
                                trusted(TrustedKeysMessage::CopyFingerprint(key.clone())),
                                styles::secondary,
                            ),
                            small_button(
                                fl!("ui-trusted-host-keys-remove"),
                                trusted(TrustedKeysMessage::RequestForget(key)),
                                styles::danger,
                            ),
                        ]
                        .spacing(spacing::XS)
                        .into(),
                        ACTIONS_PORTION,
                    ),
                ]
                .spacing(spacing::SM)
                .align_y(Alignment::Center),
            );
        }
        rows.into()
    };
    // As the C# section: the import beside the list it adds to.
    let body = column![
        row![
            small_button(
                fl!("ui-trusted-host-keys-import"),
                crate::hostkeys_view::app(heimdall_app::HostKeysMessage::Start),
                styles::secondary,
            ),
            small_button(
                fl!("ui-trusted-host-keys-export"),
                trusted(TrustedKeysMessage::Export),
                styles::secondary,
            ),
        ]
        .spacing(spacing::SM),
        body,
    ]
    .spacing(spacing::SM)
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

/// The trusted RDP certificates, `search` typed, as the C# columns: server, fingerprint,
/// subject, issuer, when trusted, and a forget button. A certificate recorded without its
/// details leaves their cells empty.
pub fn certificates<'a>(
    keys: &'a heimdall_app::TrustedKeys,
    search: &'a str,
) -> Element<'a, Message> {
    certificate_card(
        CertificateList {
            list: TrustedList::Certificates,
            entries: &keys.rdp,
            key: TrustedKey::Rdp,
            search,
        },
        CertificateTexts {
            title: fl!("ui-trusted-certificates-title"),
            hint: fl!("ui-trusted-certificates-hint"),
            empty_title: fl!("ui-trusted-certificates-empty-title"),
            empty_body: fl!("ui-trusted-certificates-empty-body"),
        },
    )
}

/// The trusted FTPS certificates, `search` typed, in the columns of the RDP ones.
pub fn ftps_certificates<'a>(
    keys: &'a heimdall_app::TrustedKeys,
    search: &'a str,
) -> Element<'a, Message> {
    certificate_card(
        CertificateList {
            list: TrustedList::FtpsCertificates,
            entries: &keys.ftps,
            key: TrustedKey::Ftps,
            search,
        },
        CertificateTexts {
            title: fl!("ui-trusted-ftps-certificates-title"),
            hint: fl!("ui-trusted-ftps-certificates-hint"),
            empty_title: fl!("ui-trusted-ftps-certificates-empty-title"),
            empty_body: fl!("ui-trusted-ftps-certificates-empty-body"),
        },
    )
}

/// The trusted VNC certificates, `search` typed, in the columns of the RDP ones.
pub fn vnc_certificates<'a>(
    keys: &'a heimdall_app::TrustedKeys,
    search: &'a str,
) -> Element<'a, Message> {
    certificate_card(
        CertificateList {
            list: TrustedList::VncCertificates,
            entries: &keys.vnc,
            key: TrustedKey::Vnc,
            search,
        },
        CertificateTexts {
            title: fl!("ui-trusted-vnc-certificates-title"),
            hint: fl!("ui-trusted-vnc-certificates-hint"),
            empty_title: fl!("ui-trusted-vnc-certificates-empty-title"),
            empty_body: fl!("ui-trusted-vnc-certificates-empty-body"),
        },
    )
}

/// The card of the certificates of `certificates`, which says itself in `texts`.
fn certificate_card<'a>(
    certificates: CertificateList<'a>,
    texts: CertificateTexts,
) -> Element<'a, Message> {
    let CertificateList {
        list,
        entries,
        key,
        search,
    } = certificates;
    let body: Element<'a, Message> = if entries.is_empty() {
        empty(texts.empty_title, texts.empty_body)
    } else {
        let mut rows = Column::new().spacing(spacing::XS).push(
            row![
                header(fl!("ui-trusted-certificates-server"), SERVER_PORTION),
                header(
                    fl!("ui-trusted-certificates-fingerprint"),
                    FINGERPRINT_PORTION
                ),
                header(fl!("ui-trusted-certificates-subject"), NAME_PORTION),
                header(fl!("ui-trusted-certificates-issuer"), NAME_PORTION),
                header(fl!("ui-trusted-certificates-trusted"), TRUSTED_PORTION),
                header(String::new(), FORGET_PORTION),
            ]
            .spacing(spacing::SM),
        );
        for entry in entries {
            let address = display_address(&entry.host, entry.port);
            let fingerprint = entry.fingerprint.to_string();
            // As the C# search: the server, the key, and the names of the certificate.
            let found = [Some(&address), Some(&fingerprint)]
                .into_iter()
                .chain([entry.subject.as_ref(), entry.issuer.as_ref()])
                .flatten()
                .any(|candidate| matches(candidate, search));
            if found {
                // A server trusted with more than one certificate can be forgotten whole.
                let shared = entries
                    .iter()
                    .filter(|other| other.host == entry.host && other.port == entry.port)
                    .nth(1)
                    .is_some();
                rows = rows.push(certificate_row(
                    key(entry.clone()),
                    entry,
                    (address, &fingerprint),
                    shared,
                ));
            }
        }
        rows.into()
    };
    card(
        texts.title,
        texts.hint,
        (list, search, fl!("ui-trusted-certificates-search")),
        body,
    )
}

/// One trusted certificate's row, `key` being `entry`, its address and fingerprint as
/// `shown`. A certificate whose server is `shared` with another can forget the server too.
fn certificate_row<'a>(
    key: TrustedKey,
    entry: &KnownRdpHost,
    shown: (String, &str),
    shared: bool,
) -> Element<'a, Message> {
    let (address, fingerprint) = shown;
    let detail = |value: Option<String>| -> Element<'a, Message> {
        text(value.unwrap_or_default())
            .size(font_size::CAPTION)
            .into()
    };
    let since = key.trusted_since();
    let mut forget = Column::new().spacing(spacing::XS);
    if shared {
        forget = forget.push(small_button(
            fl!("ui-trusted-certificates-forget-server"),
            trusted(TrustedKeysMessage::RequestForgetServer(key.clone())),
            styles::danger,
        ));
    }
    forget = forget.push(small_button(
        fl!("ui-trusted-certificates-forget"),
        trusted(TrustedKeysMessage::RequestForget(key)),
        styles::danger,
    ));
    row![
        cell(
            text(address).size(font_size::CAPTION).into(),
            SERVER_PORTION
        ),
        cell(
            fingerprint_cell(fingerprint, RDP_FINGERPRINT_SHOWN),
            FINGERPRINT_PORTION
        ),
        cell(detail(entry.subject.clone()), NAME_PORTION),
        cell(detail(entry.issuer.clone()), NAME_PORTION),
        cell(detail(since), TRUSTED_PORTION),
        cell(forget.into(), FORGET_PORTION),
    ]
    .spacing(spacing::SM)
    .align_y(Alignment::Center)
    .into()
}

/// Why the lists could not be read, when they could not.
pub fn unreadable<'a>(keys: &heimdall_app::TrustedKeys) -> Option<Element<'a, Message>> {
    let detail = keys.unreadable.as_deref()?;
    Some(
        text(fl!("ui-trusted-keys-unreadable", detail = detail))
            .size(font_size::CAPTION)
            .style(text::danger)
            .into(),
    )
}

fn header<'a>(label: String, portion: u16) -> Element<'a, Message> {
    container(text(label).size(font_size::CAPTION))
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
    button(text(label).size(font_size::CAPTION))
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
        TrustedKey::Rdp(_) | TrustedKey::Ftps(_) | TrustedKey::Vnc(_) => (
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
    question(title, body, keep, forget)
}

/// The question before forgetting the `count` certificates trusted for the server of `key`.
pub fn forget_server_question<'a>(key: &TrustedKey, count: usize) -> Element<'a, Message> {
    let server = key.address();
    question(
        fl!("ui-dialog-forget-server-certificates-title"),
        fl!(
            "ui-dialog-forget-server-certificates-body",
            server = server.as_str(),
            count = count
        ),
        fl!("ui-dialog-forget-certificate-keep"),
        fl!("ui-dialog-forget-certificate-confirm"),
    )
}

/// A question before forgetting: its `title` and `body`, then the answers `keep` and
/// `forget`.
fn question<'a>(title: String, body: String, keep: String, forget: String) -> Element<'a, Message> {
    crate::dialog_parts::choice(
        crate::dialog_parts::Severity::Danger,
        title,
        crate::dialog_parts::body(body),
        keep,
        forget,
    )
}
