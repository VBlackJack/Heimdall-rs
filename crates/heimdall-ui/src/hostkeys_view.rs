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

//! "Trusted SSH hosts..." in the window, as the C# one: a `known_hosts` file picked in the
//! system's open dialog, from `~/.ssh`, then its preview, a row per key to tick, what was
//! left out below, then what was imported.

use std::path::Path;

use heimdall_app::{
    Dialog, HostKeyRow, HostKeysMessage, HostKeysPreview, Message as AppMessage, SettingsMessage,
    TrustedKeysMessage,
};
use heimdall_core::profile::display_address;
use heimdall_ssh::known_hosts_import::{
    HostKeyDiagnostic, HostKeyNote, HostKeyStatus, MAX_FILE_BYTES, Malformed,
};
use iced::widget::{Column, button, checkbox, column, container, row, scrollable, text};
use iced::{Element, Length, Theme};

use crate::i18n::fl;
use crate::sessions_view::{Pick, SSH_FOLDER};
use crate::shell::Message;

/// The file OpenSSH trusts its servers' keys in, offered first.
const KNOWN_HOSTS_FILE_NAME: &str = "known_hosts";

/// Room between the parts of the preview.
const SPACING: f32 = 8.0;

/// Size of the preview's text.
const TEXT_SIZE: f32 = 13.0;

/// Size of its title.
const TITLE_SIZE: f32 = 20.0;

/// Tallest the list of keys grows before it scrolls.
const ROWS_HEIGHT: f32 = 300.0;

/// Tallest the diagnostics grow before they scroll.
const DIAGNOSTICS_HEIGHT: f32 = 130.0;

/// Widths of the columns: tick, host, type, fingerprint, status; the notes take the rest.
/// A fingerprint, `SHA256:` and 43 characters, is shown whole.
const COLUMNS: [f32; 5] = [40.0, 150.0, 150.0, 370.0, 110.0];

/// The application's message a change to the import is.
#[must_use]
pub fn app_message(message: HostKeysMessage) -> AppMessage {
    AppMessage::Settings(SettingsMessage::TrustedKeys(TrustedKeysMessage::Import(
        message,
    )))
}

/// The window's message a change to the import is.
#[must_use]
pub fn app(message: HostKeysMessage) -> Message {
    Message::App(app_message(message))
}

/// Opens the system's open dialog in `~/.ssh` offering `known_hosts`, over `parent` when
/// there is one, as the C# `OpenFileDialog`.
#[must_use]
pub fn pick(parent: Option<&dyn iced::window::Window>) -> Pick {
    let mut dialog = rfd::AsyncFileDialog::new()
        .set_title(fl!("ui-hostkeys-pick-title"))
        .set_file_name(KNOWN_HOSTS_FILE_NAME);
    if let Some(folder) = std::env::home_dir()
        .map(|home| home.join(SSH_FOLDER))
        .filter(|folder| folder.is_dir())
    {
        dialog = dialog.set_directory(folder);
    }
    if let Some(parent) = parent {
        dialog = dialog.set_parent(&parent);
    }
    Box::pin(dialog.pick_file())
}

/// The picked file's text, or why it could not be read; `None` when none was picked.
pub async fn read(pick: Pick) -> Option<Result<String, String>> {
    let file = pick.await?;
    Some(read_file(file.path()).await)
}

/// The text of `path`, or why it could not be read: a file larger than the C# limit is
/// refused before it is read.
///
/// # Errors
///
/// Why, when the file is too large or cannot be read as UTF-8 text.
pub async fn read_file(path: &Path) -> Result<String, String> {
    let size = tokio::fs::metadata(path)
        .await
        .map_err(|error| format!("{}: {error}", path.display()))?
        .len();
    if size > MAX_FILE_BYTES {
        return Err(fl!("ui-hostkeys-too-large", size = size.to_string()));
    }
    crate::sessions_view::read_file(path).await
}

/// A cell of `width`, its text on one line and cut at its edge.
fn cell<'a>(value: impl Into<String>, width: f32) -> Element<'a, Message> {
    container(
        text(value.into())
            .size(TEXT_SIZE)
            .wrapping(text::Wrapping::None),
    )
    .width(width)
    .clip(true)
    .into()
}

/// The preview, as the C# `ImportKnownHostsDialog`.
#[must_use]
pub fn preview(preview: &HostKeysPreview) -> Element<'_, Message> {
    let (total, new, existing, conflicts) = preview.counts();
    let header = row![
        cell("", COLUMNS[0]),
        cell(fl!("ui-hostkeys-column-host"), COLUMNS[1]),
        cell(fl!("ui-hostkeys-column-type"), COLUMNS[2]),
        cell(fl!("ui-hostkeys-column-fingerprint"), COLUMNS[3]),
        cell(fl!("ui-openssh-column-status"), COLUMNS[4]),
        text(fl!("ui-hostkeys-column-notes")).size(TEXT_SIZE),
    ];
    let rows = Column::with_children(
        preview
            .rows
            .iter()
            .enumerate()
            .map(|(index, row)| key_row(index, row, preview)),
    )
    .spacing(2.0);
    let mut content = column![
        text(fl!("ui-hostkeys-title")).size(TITLE_SIZE),
        text(fl!(
            "ui-hostkeys-summary",
            total = total,
            new = new,
            existing = existing,
            conflicts = conflicts
        ))
        .size(TEXT_SIZE),
        checkbox(preview.all_chosen())
            .label(fl!("ui-openssh-choose-all"))
            .text_size(TEXT_SIZE)
            .on_toggle(|on| app(HostKeysMessage::ChooseAll(on))),
        header,
        scrollable(rows).height(Length::Shrink).height(ROWS_HEIGHT),
    ]
    .spacing(SPACING);
    if !preview.diagnostics.is_empty() {
        content = content
            .push(
                text(fl!(
                    "ui-openssh-diagnostics",
                    count = preview.diagnostics.len()
                ))
                .size(TEXT_SIZE),
            )
            .push(
                scrollable(Column::with_children(
                    preview.diagnostics.iter().map(diagnostic_line),
                ))
                .height(DIAGNOSTICS_HEIGHT),
            );
    }
    content
        .push(
            row![
                button(text(fl!("ui-dialog-cancel-button")))
                    .style(button::secondary)
                    .on_press(Message::App(AppMessage::DismissDialog)),
                button(text(fl!("ui-openssh-import-button"))).on_press_maybe(
                    preview
                        .can_import()
                        .then_some(Message::App(AppMessage::ConfirmDialog))
                ),
            ]
            .spacing(SPACING),
        )
        .into()
}

fn key_row<'a>(
    index: usize,
    row: &'a HostKeyRow,
    preview: &HostKeysPreview,
) -> Element<'a, Message> {
    let candidate = &row.candidate;
    let (status, note) = match row.status {
        HostKeyStatus::New => (fl!("ui-hostkeys-status-new"), String::new()),
        HostKeyStatus::Existing => (
            fl!("ui-hostkeys-status-existing"),
            fl!("ui-hostkeys-note-existing"),
        ),
        HostKeyStatus::Conflict => (
            fl!("ui-hostkeys-status-conflict"),
            if conflicts_in_file(row, preview) {
                fl!("ui-hostkeys-note-conflict-file")
            } else {
                fl!("ui-hostkeys-note-conflict-store")
            },
        ),
    };
    // As the C# preview: only a new key can be ticked.
    let tick = checkbox(row.chosen).on_toggle_maybe(
        (row.status == HostKeyStatus::New).then_some(move |_| app(HostKeysMessage::Choose(index))),
    );
    row![
        container(tick).width(COLUMNS[0]),
        cell(display_address(&candidate.host, candidate.port), COLUMNS[1]),
        cell(candidate.key.algorithm().to_string(), COLUMNS[2]),
        cell(row.fingerprint.clone(), COLUMNS[3]),
        cell(status, COLUMNS[4]),
        // The notes wrap rather than being cut: they are the reason a key is left out.
        text(note).size(TEXT_SIZE).width(Length::Fill),
    ]
    .align_y(iced::Alignment::Center)
    .into()
}

/// Whether the file gives another key of `row`'s kind for its server.
fn conflicts_in_file(row: &HostKeyRow, preview: &HostKeysPreview) -> bool {
    let key = &row.candidate;
    preview.rows.iter().any(|other| {
        let other = &other.candidate;
        other.host == key.host
            && other.port == key.port
            && other.key.algorithm() == key.key.algorithm()
            && other.key.key_data() != key.key.key_data()
    })
}

fn diagnostic_line(diagnostic: &HostKeyDiagnostic) -> Element<'_, Message> {
    let line = diagnostic.line;
    let said = match &diagnostic.note {
        HostKeyNote::HashedHost => fl!("ui-hostkeys-diag-hashed", line = line),
        HostKeyNote::CertAuthority => fl!("ui-hostkeys-diag-cert-authority", line = line),
        HostKeyNote::Revoked => fl!("ui-hostkeys-diag-revoked", line = line),
        HostKeyNote::HostPattern(value) => {
            fl!(
                "ui-hostkeys-diag-pattern",
                line = line,
                value = value.as_str()
            )
        }
        HostKeyNote::UnsupportedKey(value) => {
            fl!(
                "ui-hostkeys-diag-key-type",
                line = line,
                value = value.as_str()
            )
        }
        HostKeyNote::Malformed(why) => {
            let value = match why {
                Malformed::TooLong => fl!("ui-hostkeys-malformed-too-long"),
                Malformed::Fields(count) => {
                    fl!("ui-hostkeys-malformed-fields", count = count.to_owned())
                }
                Malformed::BadKey => fl!("ui-hostkeys-malformed-bad-key"),
                Malformed::Marker(marker) => {
                    fl!("ui-hostkeys-malformed-marker", marker = marker.as_str())
                }
            };
            fl!("ui-hostkeys-diag-malformed", line = line, value = value)
        }
    };
    let warning = diagnostic.note.is_warning();
    text(said)
        .size(TEXT_SIZE)
        .style(move |theme: &Theme| text::Style {
            color: warning.then(|| theme.extended_palette().danger.base.color),
        })
        .into()
}

/// What the import said when it ended, or why it did not start: the title and the lines
/// under it.
#[must_use]
pub fn report_lines(dialog: &Dialog) -> Option<(String, Vec<String>)> {
    let line = match dialog {
        Dialog::HostKeysDone { done, warnings } => fl!(
            "ui-hostkeys-done",
            imported = done.imported,
            existing = done.existing,
            conflicts = done.conflicts,
            warnings = warnings.to_owned()
        ),
        Dialog::HostKeysUnreadable { detail } => {
            fl!("ui-hostkeys-unreadable", detail = detail.as_str())
        }
        Dialog::HostKeysEmpty => fl!("ui-hostkeys-empty"),
        _ => return None,
    };
    Some((fl!("ui-hostkeys-title"), vec![line]))
}
