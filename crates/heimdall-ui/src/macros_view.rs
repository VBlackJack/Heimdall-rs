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

//! The terminal macros on the Settings page, each with its inputs, Edit and Delete; the
//! macro editor, as the C# one; and what the status bar says of them.

use heimdall_app::macro_player::MacroOutcome;
use heimdall_app::{
    EntryDraft, EntryField, EntryProblem, MacroDraft, MacroEdit, MacroMessage, MacroProblem,
    Message as AppMessage, Notice, server_text,
};
use heimdall_core::macros::{
    EXPECT_TIMEOUT_MAX, EXPECT_TIMEOUT_MIN, InputError, OnTimeout, TerminalMacro,
};
use iced::widget::{
    Column, button, checkbox, column, container, pick_list, row, scrollable, text, text_input,
};
use iced::{Alignment, Element, Length};

use crate::i18n::fl;
use crate::shell::Message;

const SPACING: f32 = 8.0;
const PADDING: f32 = 12.0;
/// Widest the card grows, as the other Settings cards.
const CARD_WIDTH: f32 = 720.0;
const SMALL_SIZE: f32 = 12.0;
const HEADING_SIZE: f32 = 18.0;
/// Width of a number field of the editor.
const NUMBER_WIDTH: f32 = 90.0;
/// Tallest the editor's list of inputs grows before it scrolls.
const ENTRIES_HEIGHT: f32 = 420.0;

/// The macros kept, each with how many inputs it types, Edit and Delete.
pub fn card(macros: &[TerminalMacro]) -> Element<'_, Message> {
    let mut content = Column::new().spacing(SPACING);
    if macros.is_empty() {
        content = content.push(text(fl!("ui-macros-empty")).size(SMALL_SIZE));
    }
    for kept in macros {
        let named = |message: fn(String) -> MacroMessage| {
            Message::App(AppMessage::Macro(message(kept.name.clone())))
        };
        content = content.push(
            row![
                text(server_text(&kept.name)),
                text(fl!("ui-macros-inputs", count = kept.entries.len())).size(SMALL_SIZE),
                iced::widget::space::horizontal(),
                button(text(fl!("ui-macros-edit")).size(SMALL_SIZE))
                    .style(button::secondary)
                    .on_press(named(MacroMessage::Edit)),
                button(text(fl!("ui-macros-delete")).size(SMALL_SIZE))
                    .style(button::danger)
                    .on_press(named(MacroMessage::AskDelete)),
            ]
            .spacing(SPACING)
            .align_y(Alignment::Center),
        );
    }
    container(content)
        .padding(PADDING)
        .max_width(CARD_WIDTH)
        .width(Length::Fill)
        .style(container::bordered_box)
        .into()
}

/// A change in the macro editor, as the window's message.
fn draft(edit: MacroEdit) -> Message {
    Message::App(AppMessage::Macro(MacroMessage::Draft(edit)))
}

/// A field of the input `entry`, as the window's message.
fn field(entry: usize, field: EntryField) -> Message {
    draft(MacroEdit::Field { entry, field })
}

/// What a macro does when the text it waits for does not come, as the list names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TimeoutChoice(OnTimeout);

impl std::fmt::Display for TimeoutChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&match self.0 {
            OnTimeout::Abort => fl!("ui-macro-editor-timeout-abort"),
            OnTimeout::Continue => fl!("ui-macro-editor-timeout-continue"),
        })
    }
}

/// Why the macro edited is not kept, as the C# editor says it.
fn problem_text(problem: &MacroProblem) -> String {
    match problem {
        MacroProblem::NameRequired => fl!("ui-macro-editor-name-required"),
        MacroProblem::Entry { entry, problem } => {
            let reason = match problem {
                EntryProblem::Input(InputError::TrailingEscape) => {
                    fl!("ui-macro-editor-input-trailing")
                }
                EntryProblem::Input(InputError::BadHex) => fl!("ui-macro-editor-input-hex"),
                EntryProblem::Input(InputError::UnknownEscape(c)) => {
                    fl!("ui-macro-editor-input-escape", escape = c.to_string())
                }
                EntryProblem::Delay => fl!("ui-macro-editor-delay-invalid"),
                EntryProblem::Timeout => fl!(
                    "ui-macro-editor-timeout-range",
                    min = EXPECT_TIMEOUT_MIN,
                    max = EXPECT_TIMEOUT_MAX
                ),
                EntryProblem::Regex(detail) => {
                    fl!("ui-macro-editor-regex-invalid", reason = detail.as_str())
                }
            };
            fl!(
                "ui-macro-editor-entry-invalid",
                entry = (*entry),
                reason = reason
            )
        }
    }
}

/// One input of the macro edited, `index` from 0 of `count`: what is typed and the pause
/// before it, moved or taken out; what it waits for first when it does.
fn entry_card(index: usize, count: usize, entry: &EntryDraft) -> Element<'_, Message> {
    // The texts each on a line of their own: beside the buttons, they would have no room.
    let mut content = column![
        row![
            text(fl!("ui-macro-editor-input")).size(SMALL_SIZE),
            text_input("", &entry.input)
                .on_input(move |typed| field(index, EntryField::Input(typed)))
                .width(Length::Fill),
        ]
        .spacing(SPACING)
        .align_y(Alignment::Center),
        row![
            text(fl!("ui-macro-editor-delay")).size(SMALL_SIZE),
            text_input("", &entry.delay)
                .on_input(move |typed| field(index, EntryField::Delay(typed)))
                .width(NUMBER_WIDTH),
            button(text(fl!("ui-macro-editor-move-up")).size(SMALL_SIZE))
                .style(button::secondary)
                .on_press_maybe((index > 0).then(|| draft(MacroEdit::MoveUp(index)))),
            button(text(fl!("ui-macro-editor-move-down")).size(SMALL_SIZE))
                .style(button::secondary)
                .on_press_maybe((index + 1 < count).then(|| draft(MacroEdit::MoveDown(index)))),
            button(text(fl!("ui-macro-editor-delete-entry")).size(SMALL_SIZE))
                .style(button::danger)
                .on_press(draft(MacroEdit::Remove(index))),
        ]
        .spacing(SPACING)
        .align_y(Alignment::Center),
        checkbox(entry.expects)
            .label(fl!("ui-macro-editor-expects"))
            .on_toggle(move |on| field(index, EntryField::Expects(on))),
    ]
    .spacing(SPACING / 2.0);
    if entry.expects {
        content = content.push(
            row![
                text(fl!("ui-macro-editor-pattern")).size(SMALL_SIZE),
                text_input("", &entry.pattern)
                    .on_input(move |typed| field(index, EntryField::Pattern(typed)))
                    .width(Length::Fill),
            ]
            .spacing(SPACING)
            .align_y(Alignment::Center),
        );
        content = content.push(
            row![
                checkbox(entry.regex)
                    .label(fl!("ui-macro-editor-regex"))
                    .on_toggle(move |on| field(index, EntryField::Regex(on))),
                text(fl!("ui-macro-editor-timeout")).size(SMALL_SIZE),
                text_input("", &entry.timeout)
                    .on_input(move |typed| field(index, EntryField::Timeout(typed)))
                    .width(NUMBER_WIDTH),
                pick_list(
                    [
                        TimeoutChoice(OnTimeout::Abort),
                        TimeoutChoice(OnTimeout::Continue)
                    ],
                    Some(TimeoutChoice(entry.on_timeout)),
                    move |choice: TimeoutChoice| field(index, EntryField::OnTimeout(choice.0)),
                ),
            ]
            .spacing(SPACING)
            .align_y(Alignment::Center),
        );
    }
    container(content)
        .padding(PADDING / 2.0)
        .width(Length::Fill)
        .style(container::bordered_box)
        .into()
}

/// The macro editor, as the C# one: the name, the inputs, a step added waiting for text or
/// not, what is wrong when it cannot be kept; Delete macro, Cancel and Save.
pub fn editor(edited: &MacroDraft) -> Element<'_, Message> {
    let count = edited.entries.len();
    let mut entries = Column::new().spacing(SPACING);
    for (index, entry) in edited.entries.iter().enumerate() {
        entries = entries.push(entry_card(index, count, entry));
    }
    let mut page = column![
        text(fl!("ui-macro-editor-title")).size(HEADING_SIZE),
        row![
            text(fl!("ui-macro-editor-name")),
            text_input("", &edited.name)
                .on_input(|typed| draft(MacroEdit::Name(typed)))
                .width(Length::Fill),
        ]
        .spacing(SPACING)
        .align_y(Alignment::Center),
        text(fl!("ui-macro-editor-input-hint")).size(SMALL_SIZE),
        scrollable(entries).height(ENTRIES_HEIGHT),
        row![
            button(text(fl!("ui-macro-editor-add-expect")))
                .style(button::secondary)
                .on_press(draft(MacroEdit::Add { expects: true })),
            button(text(fl!("ui-macro-editor-add-send")))
                .style(button::secondary)
                .on_press(draft(MacroEdit::Add { expects: false })),
        ]
        .spacing(SPACING),
    ]
    .spacing(SPACING);
    if let Some(problem) = &edited.problem {
        page = page.push(text(problem_text(problem)).style(text::danger));
    }
    let mut actions = row![].spacing(SPACING);
    if let Some(original) = &edited.original {
        actions = actions.push(
            button(text(fl!("ui-macro-editor-delete-macro")))
                .style(button::danger)
                .on_press(Message::App(AppMessage::Macro(MacroMessage::AskDelete(
                    original.clone(),
                )))),
        );
    }
    page.push(
        actions
            .push(iced::widget::space::horizontal())
            .push(
                button(text(fl!("ui-dialog-cancel-button")))
                    .style(button::secondary)
                    .on_press(Message::App(AppMessage::DismissDialog)),
            )
            .push(
                button(text(fl!("ui-dialog-save-macro-confirm")))
                    .on_press(Message::App(AppMessage::ConfirmDialog)),
            ),
    )
    .into()
}

/// What the status bar says of a macro.
pub fn notice(notice: &Notice) -> String {
    match notice {
        Notice::MacroNothingRecorded => fl!("ui-status-macro-nothing"),
        Notice::MacroSaved(name) => fl!("ui-status-macro-saved", name = server_text(name)),
        Notice::MacroDeleted(name) => fl!("ui-status-macro-deleted", name = server_text(name)),
        Notice::MacroEnded { name, outcome } => {
            let name = server_text(name);
            match outcome {
                MacroOutcome::Completed => fl!("ui-status-macro-completed", name = name),
                MacroOutcome::Stopped => fl!("ui-status-macro-stopped", name = name),
                MacroOutcome::TimedOut { entry } => {
                    fl!("ui-status-macro-timed-out", name = name, entry = (*entry))
                }
                MacroOutcome::Closed => fl!("ui-status-macro-closed", name = name),
            }
        }
        _ => String::new(),
    }
}
