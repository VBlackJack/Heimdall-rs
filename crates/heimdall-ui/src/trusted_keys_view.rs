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
//! pages list them: searched, copied, forgotten after a question; an SSH host key sorted by
//! any of its columns and shown whole in its details. The FTPS certificates, which the C#
//! lists nowhere, are listed as the RDP ones.

use std::cmp::Ordering;
use std::time::SystemTime;

use heimdall_app::{
    Message as AppMessage, SettingsMessage, TrustedKey, TrustedKeysMessage, local_date_time,
};
use heimdall_core::profile::display_address;
use heimdall_rdp::KnownRdpHost;
use heimdall_ssh::{HostKeySource, KnownHostEntry};
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
/// Share of a host key row the details, copy and remove buttons take.
const ACTIONS_PORTION: u16 = 5;
/// Share of a host key row its server takes, and its fingerprint.
const HOST_PORTION: u16 = 3;
/// Share of a host key row its algorithm, its source, and each of its dates take.
const DETAIL_PORTION: u16 = 2;
/// Width of the labels of the details of a host key.
const DETAILS_LABEL_WIDTH: f32 = 140.0;
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

/// A column of the trusted SSH host keys, as the C# `TrustedHostKeySortColumn`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostKeyColumn {
    /// The server, `host:port`.
    HostPort,
    /// The key's algorithm.
    Algorithm,
    /// Where the key came from.
    Source,
    /// When it was first trusted.
    FirstSeen,
    /// When a server last presented it.
    LastSeen,
    /// Its fingerprint.
    Fingerprint,
}

impl HostKeyColumn {
    /// Every column, in the order the list shows them.
    const ALL: [Self; 6] = [
        Self::HostPort,
        Self::Algorithm,
        Self::Source,
        Self::FirstSeen,
        Self::LastSeen,
        Self::Fingerprint,
    ];

    /// Its header.
    fn title(self) -> String {
        match self {
            Self::HostPort => fl!("ui-trusted-host-keys-host"),
            Self::Algorithm => fl!("ui-trusted-host-keys-algorithm"),
            Self::Source => fl!("ui-trusted-host-keys-source"),
            Self::FirstSeen => fl!("ui-trusted-host-keys-first-seen"),
            Self::LastSeen => fl!("ui-trusted-host-keys-last-seen"),
            Self::Fingerprint => fl!("ui-trusted-host-keys-fingerprint"),
        }
    }

    /// Its share of a row.
    fn portion(self) -> u16 {
        match self {
            Self::HostPort | Self::Fingerprint => HOST_PORTION,
            Self::Algorithm | Self::Source | Self::FirstSeen | Self::LastSeen => DETAIL_PORTION,
        }
    }
}

/// How the trusted SSH host keys are sorted: by one column, one way. Held by the window and
/// never saved, as the C# list holds it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HostKeySort {
    /// The column sorted by.
    pub column: HostKeyColumn,
    /// Smallest first.
    pub ascending: bool,
}

impl Default for HostKeySort {
    /// The C# default: the key last seen first.
    fn default() -> Self {
        Self {
            column: HostKeyColumn::LastSeen,
            ascending: false,
        }
    }
}

impl HostKeySort {
    /// The sort after a click on the header of `column`, as the C# `SortBy`: the other way
    /// on the column sorted by; another column ascending for the server, the algorithm and
    /// the source, descending for the dates and the fingerprint.
    #[must_use]
    pub fn clicked(self, column: HostKeyColumn) -> Self {
        if self.column == column {
            return Self {
                column,
                ascending: !self.ascending,
            };
        }
        Self {
            column,
            ascending: matches!(
                column,
                HostKeyColumn::HostPort | HostKeyColumn::Algorithm | HostKeyColumn::Source
            ),
        }
    }
}

/// The keys of `keys` whose server, `host:port`, holds `search`, whatever the case, as the C#
/// search; in the order `sort` gives, keys that tie in the order of their file.
#[must_use]
pub fn sorted<'a>(
    keys: &'a [KnownHostEntry],
    search: &str,
    sort: HostKeySort,
) -> Vec<&'a KnownHostEntry> {
    let mut found: Vec<&KnownHostEntry> = keys
        .iter()
        .filter(|entry| matches(&display_address(&entry.host, entry.port), search))
        .collect();
    // As the C# `StringComparer.OrdinalIgnoreCase`: compared in upper case.
    let folded = |text: &str| text.to_uppercase();
    // The words shown for each source, read once for the whole sort.
    let sources = [
        HostKeySource::User,
        HostKeySource::Imported,
        HostKeySource::Unknown,
    ]
    .map(|source| (source, folded(&source_name(source))));
    let source_word = |source: HostKeySource| {
        sources
            .iter()
            .find(|(known, _)| *known == source)
            .map(|(_, word)| word.as_str())
    };
    let order = |a: &KnownHostEntry, b: &KnownHostEntry| -> Ordering {
        match sort.column {
            HostKeyColumn::HostPort => folded(&display_address(&a.host, a.port))
                .cmp(&folded(&display_address(&b.host, b.port))),
            HostKeyColumn::Algorithm => folded(&a.algorithm).cmp(&folded(&b.algorithm)),
            HostKeyColumn::Source => {
                source_word(a.details.source).cmp(&source_word(b.details.source))
            }
            // A date not known is the oldest, as the C# `DateTimeOffset.MinValue`.
            HostKeyColumn::FirstSeen => a.details.first_seen.cmp(&b.details.first_seen),
            HostKeyColumn::LastSeen => a.details.last_seen.cmp(&b.details.last_seen),
            HostKeyColumn::Fingerprint => a.fingerprint.cmp(&b.fingerprint),
        }
    };
    found.sort_by(|a, b| {
        let ordering = order(a, b);
        if sort.ascending {
            ordering
        } else {
            ordering.reverse()
        }
    });
    found
}

/// Where a key came from, in the C# words.
fn source_name(source: HostKeySource) -> String {
    match source {
        HostKeySource::User => fl!("ui-trusted-host-keys-source-user"),
        HostKeySource::Imported => fl!("ui-trusted-host-keys-source-imported"),
        HostKeySource::Unknown => fl!("ui-trusted-host-keys-source-unknown"),
    }
}

/// A date of a key, in this computer's time, or the C# word for one not known.
fn date_text(time: Option<SystemTime>) -> String {
    time.map_or_else(|| fl!("ui-trusted-host-keys-date-unknown"), local_date_time)
}

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

/// The trusted SSH host keys, `search` typed and in the order of `sort`, in the C# columns:
/// server, algorithm, source, first and last seen, fingerprint; each header sorts by its
/// column; a details, a copy and a remove button per key.
pub fn host_keys<'a>(
    keys: &'a heimdall_app::TrustedKeys,
    search: &'a str,
    sort: HostKeySort,
) -> Element<'a, Message> {
    let body: Element<'a, Message> = if keys.ssh.is_empty() {
        empty(
            fl!("ui-trusted-host-keys-empty-title"),
            fl!("ui-trusted-host-keys-empty-body"),
        )
    } else {
        let mut headers = row![].spacing(spacing::SM);
        for column in HostKeyColumn::ALL {
            headers = headers.push(sort_header(column, sort));
        }
        // Room for the three buttons, whatever the language: "Copier l'empreinte" is long.
        let headers = headers.push(header(String::new(), ACTIONS_PORTION));
        let mut rows = Column::new().spacing(spacing::XS).push(headers);
        for entry in sorted(&keys.ssh, search, sort) {
            let address = display_address(&entry.host, entry.port);
            let key = TrustedKey::Ssh(entry.clone());
            let caption = |value: String| -> Element<'a, Message> {
                text(value).size(font_size::CAPTION).into()
            };
            rows = rows.push(
                row![
                    cell(caption(address), HOST_PORTION),
                    cell(caption(entry.algorithm.clone()), DETAIL_PORTION),
                    cell(caption(source_name(entry.details.source)), DETAIL_PORTION),
                    cell(caption(date_text(entry.details.first_seen)), DETAIL_PORTION),
                    cell(caption(date_text(entry.details.last_seen)), DETAIL_PORTION),
                    cell(
                        fingerprint_cell(&entry.fingerprint, SSH_FINGERPRINT_SHOWN),
                        HOST_PORTION
                    ),
                    cell(
                        row![
                            small_button(
                                fl!("ui-trusted-host-keys-details"),
                                trusted(TrustedKeysMessage::ShowDetails(entry.clone())),
                                styles::secondary,
                            ),
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

/// The header of `column`, a button that sorts by it, marked when the list is sorted by it.
fn sort_header<'a>(column: HostKeyColumn, sort: HostKeySort) -> Element<'a, Message> {
    let title = column.title();
    let title = match (sort.column == column, sort.ascending) {
        (false, _) => title,
        (true, true) => fl!("ui-files-sorted-ascending", column = title),
        (true, false) => fl!("ui-files-sorted-descending", column = title),
    };
    container(
        button(text(title).size(font_size::CAPTION).style(text::secondary))
            .style(styles::subtle)
            .padding(0)
            .on_press(Message::HostKeySort(column)),
    )
    .width(Length::FillPortion(column.portion()))
    .into()
}

/// All that is known of the SSH host key `entry`, as the C# "Trusted host key details"
/// dialog shows it: its server, algorithm, source, dates, whole fingerprint and the base64
/// of its key, then Close.
pub fn details(entry: &KnownHostEntry) -> Element<'_, Message> {
    let public_key = entry
        .public_key
        .clone()
        .unwrap_or_else(|| fl!("ui-trusted-host-key-details-public-key-unavailable"));
    let lines = [
        (
            HostKeyColumn::HostPort.title(),
            display_address(&entry.host, entry.port),
        ),
        (HostKeyColumn::Algorithm.title(), entry.algorithm.clone()),
        (
            HostKeyColumn::Source.title(),
            source_name(entry.details.source),
        ),
        (
            HostKeyColumn::FirstSeen.title(),
            date_text(entry.details.first_seen),
        ),
        (
            HostKeyColumn::LastSeen.title(),
            date_text(entry.details.last_seen),
        ),
        (
            HostKeyColumn::Fingerprint.title(),
            entry.fingerprint.clone(),
        ),
        (fl!("ui-trusted-host-key-details-public-key"), public_key),
    ];
    let mut content =
        column![text(fl!("ui-trusted-host-key-details-title")).size(font_size::SUBTITLE)]
            .spacing(spacing::SM);
    for (label, value) in lines {
        content = content.push(
            row![
                text(label)
                    .size(font_size::CAPTION)
                    .width(DETAILS_LABEL_WIDTH),
                text(value).width(Length::Fill)
            ]
            .spacing(spacing::SM),
        );
    }
    let close =
        crate::dialog_parts::action(fl!("ui-trusted-host-key-details-close"), styles::primary)
            .on_press(Message::App(AppMessage::DismissDialog));
    content.push(crate::dialog_parts::buttons([close])).into()
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
