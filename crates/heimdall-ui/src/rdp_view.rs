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

//! "Import .rdp files" in the window, as the C# one: files picked in the system's open
//! dialog or dropped on the window, then a preview of each, a choice for each name taken,
//! then what was imported.

use std::fmt;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;

use heimdall_app::{
    Dialog, Message as AppMessage, RDP_EXTENSION, RdpMessage, RdpNames, RdpPreview, RdpRow,
};
use heimdall_core::import::rdp_file::{Conflict, MAX_FILE_BYTES, Refusal};
use iced::widget::{Column, button, checkbox, column, container, pick_list, row, scrollable, text};
use iced::{Element, Length};

use crate::i18n::fl;
use crate::shell::Message;

/// Room between the parts of the preview.
const SPACING: f32 = 8.0;

/// Size of the preview's text.
const TEXT_SIZE: f32 = 13.0;

/// Size of its title.
const TITLE_SIZE: f32 = 20.0;

/// Tallest the list of files grows before it scrolls.
const ROWS_HEIGHT: f32 = 320.0;

/// Widths of the columns: tick, source, name, host, status, conflict.
const COLUMNS: [f32; 6] = [50.0, 170.0, 170.0, 170.0, 240.0, 150.0];

/// Every choice for a name taken, as the C# list offers them.
const CONFLICTS: [ConflictChoice; 3] = [
    ConflictChoice(Conflict::Skip),
    ConflictChoice(Conflict::Replace),
    ConflictChoice(Conflict::AutoRename),
];

/// A choice for a name taken, named in the user's language.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ConflictChoice(Conflict);

impl fmt::Display for ConflictChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&conflict_name(self.0))
    }
}

fn conflict_name(conflict: Conflict) -> String {
    match conflict {
        Conflict::Skip => fl!("ui-rdp-conflict-skip"),
        Conflict::Replace => fl!("ui-rdp-conflict-replace"),
        Conflict::AutoRename => fl!("ui-rdp-conflict-rename"),
    }
}

/// The words the import writes into names, in the user's language.
#[must_use]
pub fn names() -> RdpNames {
    RdpNames {
        rename: fl!("ui-rdp-rename", name = "{name}", n = "{n}"),
        fallback: fl!("ui-rdp-fallback-name"),
    }
}

/// The files the user picks in the open dialog, once it closes; none when cancelled.
pub type Pick = Pin<Box<dyn Future<Output = Option<Vec<rfd::FileHandle>>> + Send>>;

/// Opens the system's open dialog for `.rdp` files, several at once, over `parent`.
#[must_use]
pub fn pick(title: String, filter: String, parent: Option<&dyn iced::window::Window>) -> Pick {
    let mut dialog = rfd::AsyncFileDialog::new()
        .set_title(title)
        .add_filter(filter, &[RDP_EXTENSION]);
    if let Some(parent) = parent {
        dialog = dialog.set_parent(&parent);
    }
    Box::pin(dialog.pick_files())
}

/// The picked files read; `None` when none was picked.
pub async fn read_picked(pick: Pick) -> Option<Vec<(PathBuf, Result<String, String>)>> {
    let files = pick.await?;
    let paths: Vec<PathBuf> = files.iter().map(|file| file.path().to_owned()).collect();
    Some(read_all(paths).await)
}

/// Each file's text, or its path and why it could not be read: a file bigger than the C#
/// limit is not read at all.
pub async fn read_all(paths: Vec<PathBuf>) -> Vec<(PathBuf, Result<String, String>)> {
    let mut read = Vec::with_capacity(paths.len());
    for path in paths {
        let text = read_file(&path).await;
        read.push((path, text));
    }
    read
}

async fn read_file(path: &Path) -> Result<String, String> {
    let reason = |error: &dyn fmt::Display| format!("{}: {error}", path.display());
    let size = tokio::fs::metadata(path)
        .await
        .map_err(|error| reason(&error))?
        .len();
    if size > MAX_FILE_BYTES {
        return Err(reason(&format_args!("{size} > {MAX_FILE_BYTES} bytes")));
    }
    tokio::fs::read_to_string(path)
        .await
        .map_err(|error| reason(&error))
}

fn app(message: RdpMessage) -> Message {
    Message::App(AppMessage::Rdp(message))
}

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

/// The preview, as the C# `RdpImportDialog`.
#[must_use]
pub fn preview(preview: &RdpPreview) -> Element<'_, Message> {
    let (chosen, files, conflicts, passwords) = preview.counts();
    let header = row![
        cell("", COLUMNS[0]),
        cell(fl!("ui-rdp-column-source"), COLUMNS[1]),
        cell(fl!("ui-rdp-column-name"), COLUMNS[2]),
        cell(fl!("ui-rdp-column-host"), COLUMNS[3]),
        cell(fl!("ui-rdp-column-status"), COLUMNS[4]),
        cell(fl!("ui-rdp-column-conflict"), COLUMNS[5]),
    ];
    let rows = Column::with_children(
        preview
            .rows
            .iter()
            .enumerate()
            .map(|(index, row)| file_row(index, row)),
    )
    .spacing(2.0);
    let mut content = column![
        text(fl!("ui-rdp-title")).size(TITLE_SIZE),
        text(fl!(
            "ui-rdp-summary",
            chosen = chosen,
            files = files,
            conflicts = conflicts,
            passwords = passwords
        ))
        .size(TEXT_SIZE),
    ]
    .spacing(SPACING);
    if !preview.unreadable.is_empty() {
        content = content
            .push(text(fl!("ui-rdp-unreadable", count = preview.unreadable.len())).size(TEXT_SIZE));
    }
    let mut choices = row![
        button(text(fl!("ui-rdp-select-all")).size(TEXT_SIZE))
            .style(button::secondary)
            .on_press(app(RdpMessage::ChooseAll(true))),
        button(text(fl!("ui-rdp-select-none")).size(TEXT_SIZE))
            .style(button::secondary)
            .on_press(app(RdpMessage::ChooseAll(false))),
    ]
    .spacing(SPACING)
    .align_y(iced::Alignment::Center);
    if conflicts > 0 {
        choices = choices.push(text(fl!("ui-rdp-apply-all")).size(TEXT_SIZE));
        for choice in CONFLICTS {
            choices = choices.push(
                button(text(choice.to_string()).size(TEXT_SIZE))
                    .style(button::secondary)
                    .on_press(app(RdpMessage::ConflictAll(choice.0))),
            );
        }
    }
    content
        .push(choices)
        .push(header)
        .push(scrollable(rows).height(ROWS_HEIGHT))
        .push(
            row![
                button(text(fl!("ui-dialog-cancel-button")))
                    .style(button::secondary)
                    .on_press(Message::App(AppMessage::DismissDialog)),
                button(text(fl!("ui-rdp-import-button"))).on_press_maybe(
                    preview
                        .can_import()
                        .then_some(Message::App(AppMessage::ConfirmDialog))
                ),
            ]
            .spacing(SPACING),
        )
        .width(Length::Shrink)
        .into()
}

fn file_row(index: usize, row: &RdpRow) -> Element<'_, Message> {
    let source = row
        .source
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let host = row
        .patch
        .as_ref()
        .map(|patch| format!("{}:{}", patch.host, patch.port))
        .unwrap_or_default();
    let tick: Element<'_, Message> = if row.patch.is_ok() {
        checkbox(row.chosen)
            .on_toggle(move |_| app(RdpMessage::Choose(index)))
            .into()
    } else {
        checkbox(false).into()
    };
    let conflict: Element<'_, Message> = if row.conflict_with.is_some() {
        pick_list(
            CONFLICTS,
            Some(ConflictChoice(row.conflict)),
            move |choice| app(RdpMessage::Conflict(index, choice.0)),
        )
        .text_size(TEXT_SIZE)
        .into()
    } else {
        text("").into()
    };
    row![
        container(tick).width(COLUMNS[0]),
        cell(source, COLUMNS[1]),
        cell(row.name.clone(), COLUMNS[2]),
        cell(host, COLUMNS[3]),
        // Several things can be said of a file: they wrap rather than be cut.
        container(text(status(row)).size(TEXT_SIZE)).width(COLUMNS[4]),
        container(conflict).width(COLUMNS[5]),
    ]
    .align_y(iced::Alignment::Center)
    .into()
}

/// What the C# status column says of a file.
fn status(row: &RdpRow) -> String {
    let mut said = Vec::new();
    match &row.patch {
        Err(Refusal::InvalidAddress) => said.push(fl!("ui-rdp-status-invalid-address")),
        Err(Refusal::NeedsRdGateway) => said.push(fl!("ui-rdp-status-rd-gateway")),
        Ok(_) => {}
    }
    if let Some(with) = &row.conflict_with {
        said.push(fl!("ui-rdp-status-conflict", name = with.as_str()));
    }
    if row.password {
        said.push(fl!("ui-rdp-status-password"));
    }
    if row.partial {
        said.push(fl!("ui-rdp-status-partial"));
    }
    if row.unknown > 0 {
        said.push(fl!("ui-rdp-status-unknown", count = row.unknown));
    }
    said.join(", ")
}

/// What the import said when it ended, or that it found nothing: the lines under the title.
#[must_use]
pub fn report_lines(dialog: &Dialog) -> Option<Vec<String>> {
    match dialog {
        Dialog::RdpDone(done) => Some(vec![fl!(
            "ui-rdp-done",
            imported = done.imported,
            replaced = done.replaced,
            renamed = done.renamed,
            skipped = done.skipped,
            passwords = done.passwords
        )]),
        Dialog::RdpNothing { unreadable } => {
            let mut lines = vec![fl!("ui-rdp-nothing")];
            if !unreadable.is_empty() {
                lines.push(fl!("ui-rdp-unreadable", count = unreadable.len()));
            }
            Some(lines)
        }
        _ => None,
    }
}
