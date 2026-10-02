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

/// Room between the parts of the question.
const SPACING: f32 = 8.0;

/// Size of its text.
const TEXT_SIZE: f32 = 13.0;

/// Size of the notes under a destination and of the count.
const CAPTION_SIZE: f32 = 12.0;

/// Size of its title.
const TITLE_SIZE: f32 = 20.0;

/// Tallest the list grows before it scrolls.
const ROWS_HEIGHT: f32 = 300.0;

/// Widths of the columns: destination, action (the C# one is 150).
const COLUMNS: [f32; 2] = [520.0, 150.0];

/// Every answer, in the C# order.
const CHOICES: [Choice; 3] = [Choice::Skip, Choice::Replace, Choice::AutoRename];

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
    }
}

fn files(message: FilesMessage) -> Message {
    Message::App(AppMessage::Files(message))
}

/// The question, for `rows`.
#[must_use]
pub fn view(rows: &[ConflictRow]) -> Element<'_, Message> {
    let mut all = row![text(fl!("ui-files-conflict-apply-all")).size(TEXT_SIZE)]
        .spacing(SPACING)
        .align_y(iced::Alignment::Center);
    for choice in CHOICES {
        all = all.push(
            button(text(choice_name(choice)).size(TEXT_SIZE))
                .style(button::text)
                .on_press(files(FilesMessage::ConflictAll(choice))),
        );
    }
    let header = row![
        heading(fl!("ui-files-conflict-destination"), COLUMNS[0]),
        heading(fl!("ui-files-conflict-action"), COLUMNS[1]),
    ];
    let list = Column::with_children(rows.iter().enumerate().map(|(index, row)| line(index, row)))
        .spacing(SPACING);
    column![
        text(fl!("ui-files-conflict-title")).size(TITLE_SIZE),
        text(fl!("ui-files-conflict-hint"))
            .size(TEXT_SIZE)
            .style(text::secondary),
        text(fl!("ui-files-conflict-summary", count = rows.len()))
            .size(CAPTION_SIZE)
            .style(text::secondary),
        all,
        container(column![header, scrollable(list).height(ROWS_HEIGHT)].spacing(SPACING))
            .padding(SPACING)
            .style(container::bordered_box),
        row![
            button(text(fl!("ui-dialog-cancel-button")))
                .style(button::secondary)
                .on_press(Message::App(AppMessage::DismissDialog)),
            button(text(fl!("ui-files-conflict-apply")))
                .on_press(Message::App(AppMessage::ConfirmDialog)),
        ]
        .spacing(SPACING),
    ]
    .spacing(SPACING)
    .width(Length::Shrink)
    .into()
}

fn heading<'a>(label: String, width: f32) -> Element<'a, Message> {
    container(text(label).size(TEXT_SIZE).style(text::secondary))
        .width(width)
        .into()
}

fn line(index: usize, row: &ConflictRow) -> Element<'_, Message> {
    let mut target = column![
        container(
            text(row.target.as_str())
                .size(TEXT_SIZE)
                .wrapping(text::Wrapping::None)
        )
        .clip(true)
    ];
    // A folder that can only be skipped takes everything planned inside with it.
    if row.folder && !row.allowed.replace && !row.allowed.rename {
        target = target.push(
            text(fl!("ui-files-conflict-folder-skip"))
                .size(CAPTION_SIZE)
                .style(text::warning),
        );
    }
    let allowed: Vec<Named> = CHOICES
        .into_iter()
        .filter(|choice| row.allowed.allows(*choice))
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
            .text_size(TEXT_SIZE)
        )
        .width(COLUMNS[1]),
    ]
    .align_y(iced::Alignment::Center)
    .into()
}
