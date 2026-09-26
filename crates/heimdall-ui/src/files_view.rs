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

//! The Files tab: this computer on the left, the server on the right, transfers below.
//!
//! A folder opens when clicked; a file is selected, and the buttons between the panes send
//! the selection to the other side.

use heimdall_app::files::{
    Direction, EntryKind, FilesError, FilesPane, Side, Transfer, TransferState,
};
use heimdall_app::{FilesMessage, Message as AppMessage, TabId};
use iced::widget::{Column, button, column, container, row, scrollable, text};
use iced::{Alignment, Element, Length};

use crate::i18n::fl;
use crate::shell::Message;
use crate::texts;

/// Gap between elements, in logical pixels.
const SPACING: f32 = 8.0;

/// Padding inside panes, in logical pixels.
const PADDING: f32 = 8.0;

/// Size of secondary text, in logical pixels.
const SMALL_SIZE: f32 = 12.0;

/// Size of pane titles, in logical pixels.
const TITLE_SIZE: f32 = 16.0;

/// Width of the size column, in logical pixels.
const SIZE_WIDTH: f32 = 90.0;

/// Tallest the transfer list grows before it scrolls, in logical pixels.
const TRANSFERS_HEIGHT: f32 = 160.0;

/// Marks a folder after its name: language-neutral, like a path.
const FOLDER_MARK: &str = "/";

/// Marks a link after its name.
const LINK_MARK: &str = " ->";

fn files(message: FilesMessage) -> Message {
    Message::App(AppMessage::Files(message))
}

/// One row of a pane.
struct Row<'a> {
    label: &'a str,
    kind: EntryKind,
    size: Option<u64>,
}

fn entry_row<'a>(entry: &Row<'a>, selected: bool, on_press: Message) -> Element<'a, Message> {
    let mark = match entry.kind {
        EntryKind::Directory => FOLDER_MARK,
        EntryKind::Link => LINK_MARK,
        EntryKind::File | EntryKind::Other => "",
    };
    let size = match (entry.kind, entry.size) {
        (EntryKind::File, Some(bytes)) => texts::size(bytes),
        _ => String::new(),
    };
    button(
        row![
            text(format!("{}{mark}", entry.label)).width(Length::Fill),
            text(size)
                .size(SMALL_SIZE)
                .width(SIZE_WIDTH)
                .align_x(iced::alignment::Horizontal::Right),
        ]
        .spacing(SPACING),
    )
    .width(Length::Fill)
    .style(if selected {
        button::primary
    } else {
        button::text
    })
    .on_press(on_press)
    .into()
}

#[allow(
    clippy::too_many_arguments,
    reason = "a pane is drawn from its parts, shared by both sides"
)]
fn pane<'a>(
    tab: TabId,
    side: Side,
    title: String,
    location: String,
    rows: &[Row<'a>],
    selected: Option<usize>,
    loading: bool,
    error: Option<&FilesError>,
) -> Element<'a, Message> {
    let header = row![
        button(text(fl!("ui-files-up-button")).size(SMALL_SIZE))
            .style(button::secondary)
            .on_press(files(FilesMessage::Up { tab, side })),
        text(location).size(SMALL_SIZE).width(Length::Fill),
        button(text(fl!("ui-files-refresh-button")).size(SMALL_SIZE))
            .style(button::secondary)
            .on_press(files(FilesMessage::Refresh { tab, side })),
    ]
    .spacing(SPACING)
    .align_y(Alignment::Center);
    let mut list = Column::new().spacing(2.0);
    if loading {
        list = list.push(text(fl!("ui-files-loading")).size(SMALL_SIZE));
    } else if rows.is_empty() && error.is_none() {
        list = list.push(text(fl!("ui-files-empty")).size(SMALL_SIZE));
    }
    for (index, entry) in rows.iter().enumerate() {
        let on_press = if entry.kind == EntryKind::Directory {
            files(FilesMessage::Open { tab, side, index })
        } else {
            files(FilesMessage::Select { tab, side, index })
        };
        list = list.push(entry_row(entry, selected == Some(index), on_press));
    }
    let mut content = column![text(title).size(TITLE_SIZE), header].spacing(SPACING);
    if let Some(error) = error {
        content = content.push(text(texts::files_error(error)).size(SMALL_SIZE));
    }
    container(content.push(scrollable(list).height(Length::Fill)))
        .padding(PADDING)
        .width(Length::FillPortion(1))
        .height(Length::Fill)
        .style(container::bordered_box)
        .into()
}

fn transfer_row(tab: TabId, transfer: &Transfer) -> Element<'_, Message> {
    let what = match transfer.direction {
        Direction::Download => {
            fl!("ui-files-transfer-download", name = transfer.label.as_str())
        }
        Direction::Upload => fl!("ui-files-transfer-upload", name = transfer.label.as_str()),
    };
    let state = match &transfer.state {
        TransferState::Running => match transfer.total {
            Some(total) => fl!(
                "ui-files-state-running",
                done = texts::size(transfer.bytes),
                total = texts::size(total)
            ),
            None => fl!(
                "ui-files-state-running-unknown",
                done = texts::size(transfer.bytes)
            ),
        },
        TransferState::Done => fl!("ui-files-state-done"),
        TransferState::Cancelled => fl!("ui-files-state-cancelled"),
        TransferState::Failed(error) => {
            fl!("ui-files-state-failed", reason = texts::files_error(error))
        }
    };
    let mut line = row![
        text(what).width(Length::FillPortion(2)),
        text(state).size(SMALL_SIZE).width(Length::FillPortion(3)),
    ]
    .spacing(SPACING)
    .align_y(Alignment::Center);
    if transfer.state == TransferState::Running {
        line = line.push(
            button(text(fl!("ui-files-cancel-button")).size(SMALL_SIZE))
                .style(button::secondary)
                .on_press(files(FilesMessage::Cancel {
                    tab,
                    id: transfer.id,
                })),
        );
    }
    line.into()
}

/// The Files tab.
#[must_use]
pub fn view(tab: TabId, files_pane: &FilesPane) -> Element<'_, Message> {
    let local_rows: Vec<Row<'_>> = files_pane
        .local
        .entries
        .iter()
        .map(|entry| Row {
            label: &entry.label,
            kind: entry.kind,
            size: entry.size,
        })
        .collect();
    let remote_rows: Vec<Row<'_>> = files_pane
        .remote
        .entries
        .iter()
        .map(|entry| Row {
            label: &entry.label,
            kind: entry.kind,
            size: entry.size,
        })
        .collect();
    let local = pane(
        tab,
        Side::Local,
        fl!("ui-files-local-title"),
        files_pane.local.path.display().to_string(),
        &local_rows,
        files_pane.local.selected,
        files_pane.local.loading,
        files_pane.local.error.as_ref(),
    );
    let remote = pane(
        tab,
        Side::Remote,
        fl!("ui-files-remote-title"),
        files_pane.remote.path.display(),
        &remote_rows,
        files_pane.remote.selected,
        files_pane.remote.loading,
        files_pane.remote.error.as_ref(),
    );
    let can_upload = files_pane.local.selected.is_some();
    let can_download = files_pane.remote.selected.is_some();
    let actions = column![
        button(text(fl!("ui-files-upload-button"))).on_press_maybe(can_upload.then(|| files(
            FilesMessage::Transfer {
                tab,
                direction: Direction::Upload,
            }
        ))),
        button(text(fl!("ui-files-download-button"))).on_press_maybe(can_download.then(|| files(
            FilesMessage::Transfer {
                tab,
                direction: Direction::Download,
            }
        ))),
    ]
    .spacing(SPACING)
    .align_x(Alignment::Center);
    let panes = row![local, container(actions).center_y(Length::Fill), remote]
        .spacing(SPACING)
        .height(Length::Fill);
    let mut content = column![panes].spacing(SPACING).padding(PADDING);
    if !files_pane.transfers.is_empty() {
        let list = files_pane
            .transfers
            .iter()
            .rev()
            .fold(Column::new().spacing(4.0), |list, transfer| {
                list.push(transfer_row(tab, transfer))
            });
        content = content
            .push(text(fl!("ui-files-transfers-title")).size(TITLE_SIZE))
            .push(container(scrollable(list)).max_height(TRANSFERS_HEIGHT));
    }
    content.into()
}
