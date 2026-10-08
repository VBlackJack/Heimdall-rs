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

//! The question asked before a transfer writes where something already is, as the C#
//! `FileConflictDialog`: every destination in the way at once, an answer for each, and the
//! same answer for all in one press.

use std::fmt;

use heimdall_app::{ConflictRow, FilesMessage, Message as AppMessage};
use heimdall_files::conflict::Choice;
use iced::widget::{Column, button, column, container, pick_list, row, scrollable, text};
use iced::{Element, Length};

use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, spacing};

/// Tallest the list grows before it scrolls.
const ROWS_HEIGHT: f32 = 300.0;

/// Widths of the columns: destination, action (the C# one is 150).
const COLUMNS: [f32; 2] = [520.0, 150.0];

/// Every answer, in the C# order.
const CHOICES: [Choice; 4] = [
    Choice::Skip,
    Choice::Replace,
    Choice::AutoRename,
    Choice::ReplaceIfNewer,
];

/// An answer, named in the user's language.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Named(Choice);

impl fmt::Display for Named {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&choice_name(self.0))
    }
}

fn choice_name(choice: Choice) -> String {
    match choice {
        Choice::Skip => fl!("ui-files-conflict-skip"),
        Choice::Replace => fl!("ui-files-conflict-replace"),
        Choice::AutoRename => fl!("ui-files-conflict-rename"),
        Choice::ReplaceIfNewer => fl!("ui-files-conflict-replace-if-newer"),
    }
}

/// A copy's size and time, as the C# dialog writes them.
fn stamp_text(stamp: &heimdall_files::Stamp) -> (String, String) {
    (
        stamp
            .size
            .map_or_else(|| fl!("ui-files-conflict-unknown"), crate::texts::size),
        stamp.modified.map_or_else(
            || fl!("ui-files-conflict-unknown"),
            crate::files_view::modified_text,
        ),
    )
}

/// What a file in the way is told by: both copies' size and time, and which is newer.
fn details<'a>(row: &ConflictRow) -> Column<'a, Message> {
    let note = |line: String| text(line).size(font_size::CAPTION).style(text::secondary);
    let (size, modified) = stamp_text(&row.incoming);
    let mut lines = column![note(fl!(
        "ui-files-conflict-incoming",
        size = size,
        modified = modified
    ))];
    if let Some(existing) = &row.existing {
        let (size, modified) = stamp_text(existing);
        lines = lines.push(note(fl!(
            "ui-files-conflict-existing",
            size = size,
            modified = modified
        )));
        let compared = match row.incoming.compare_time(existing) {
            Some(std::cmp::Ordering::Greater) => Some(fl!("ui-files-conflict-newer")),
            Some(std::cmp::Ordering::Less) => Some(fl!("ui-files-conflict-older")),
            Some(std::cmp::Ordering::Equal) => Some(fl!("ui-files-conflict-same-time")),
            None => None,
        };
        if let Some(compared) = compared {
            lines = lines.push(note(compared));
        }
    }
    lines
}

fn files(message: FilesMessage) -> Message {
    Message::App(AppMessage::Files(message))
}

/// The question, for `rows`.
#[must_use]
pub fn view(rows: &[ConflictRow]) -> Element<'_, Message> {
    let mut all = row![text(fl!("ui-files-conflict-apply-all")).size(font_size::BODY)]
        .spacing(spacing::SM)
        .align_y(iced::Alignment::Center);
    for choice in CHOICES {
        all = all.push(
            button(text(choice_name(choice)).size(font_size::BODY))
                .style(styles::subtle)
                .on_press(files(FilesMessage::ConflictAll(choice))),
        );
    }
    let header = row![
        heading(fl!("ui-files-conflict-destination"), COLUMNS[0]),
        heading(fl!("ui-files-conflict-action"), COLUMNS[1]),
    ];
    let list = Column::with_children(rows.iter().enumerate().map(|(index, row)| line(index, row)))
        .spacing(spacing::SM);
    column![
        text(fl!("ui-files-conflict-title")).size(font_size::TITLE),
        text(fl!("ui-files-conflict-hint"))
            .size(font_size::BODY)
            .style(text::secondary),
        text(fl!("ui-files-conflict-summary", count = rows.len()))
            .size(font_size::CAPTION)
            .style(text::secondary),
        all,
        container(column![header, scrollable(list).height(ROWS_HEIGHT)].spacing(spacing::SM))
            .padding(spacing::SM)
            .style(container::bordered_box),
        row![
            button(text(fl!("ui-dialog-cancel-button")))
                .style(styles::secondary)
                .on_press(Message::App(AppMessage::DismissDialog)),
            button(text(fl!("ui-files-conflict-apply")))
                .style(styles::primary)
                .on_press(Message::App(AppMessage::ConfirmDialog)),
        ]
        .spacing(spacing::SM),
    ]
    .spacing(spacing::SM)
    .width(Length::Shrink)
    .into()
}

fn heading<'a>(label: String, width: f32) -> Element<'a, Message> {
    container(text(label).size(font_size::BODY).style(text::secondary))
        .width(width)
        .into()
}

fn line(index: usize, row: &ConflictRow) -> Element<'_, Message> {
    let mut target = column![
        container(
            text(row.target.as_str())
                .size(font_size::BODY)
                .wrapping(text::Wrapping::None)
        )
        .clip(true)
    ];
    if !row.folder {
        target = target.push(details(row));
    }
    // A folder that can only be skipped takes everything planned inside with it.
    if row.folder && !row.allowed.replace && !row.allowed.rename {
        target = target.push(
            text(fl!("ui-files-conflict-folder-skip"))
                .size(font_size::CAPTION)
                .style(text::warning),
        );
    }
    let allowed: Vec<Named> = CHOICES
        .into_iter()
        .filter(|choice| row.allowed.allows(*choice))
        // "Replace if newer" is a file's answer.
        .filter(|choice| *choice != Choice::ReplaceIfNewer || !row.folder)
        .map(Named)
        .collect();
    row![
        container(target).width(COLUMNS[0]),
        container(
            pick_list(allowed, Some(Named(row.choice)), move |choice: Named| {
                files(FilesMessage::ConflictChosen {
                    row: index,
                    choice: choice.0,
                })
            })
            .style(styles::pick_list)
            .menu_style(styles::menu)
            .text_size(font_size::BODY)
        )
        .width(COLUMNS[1]),
    ]
    .align_y(iced::Alignment::Center)
    .into()
}
