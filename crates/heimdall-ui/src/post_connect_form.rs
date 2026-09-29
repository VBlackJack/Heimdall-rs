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

//! The post-connect steps of an SSH profile's form, as the C# "Post-connect sequence" card:
//! a row per step, on or off, its command, its delay and what a failure does; a click on a
//! row selects it for Remove and the moves.

use heimdall_app::Message as AppMessage;
use heimdall_app::steps_draft::{StepEdit, StepsDraft};
use heimdall_core::post_connect::{OnFailure, PostConnectStep};
use iced::widget::{
    button, checkbox, column, container, mouse_area, pick_list, row, text, text_input,
};
use iced::{Alignment, Element, Length};

use crate::i18n::fl;
use crate::shell::Message;

/// Space between the parts of a row, and between the rows.
const SPACING: f32 = 8.0;
/// Size of the captions and hints.
const CAPTION_SIZE: f32 = 12.0;
/// Width of the delay field.
const DELAY_WIDTH: f32 = 90.0;
/// Padding inside a row.
const ROW_PADDING: f32 = 6.0;

/// What a failure does, as the list names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FailureChoice(OnFailure);

impl std::fmt::Display for FailureChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&match self.0 {
            OnFailure::Continue => fl!("ui-post-connect-failure-continue"),
            OnFailure::Stop => fl!("ui-post-connect-failure-stop"),
        })
    }
}

fn edit(edit: StepEdit) -> Message {
    Message::App(AppMessage::PostConnectEdit(edit))
}

/// The card: its title and hint, the rows or the empty text, then the buttons.
#[must_use]
pub fn view(draft: &StepsDraft) -> Element<'_, Message> {
    let mut card = column![
        text(fl!("ui-post-connect-title")),
        text(fl!("ui-post-connect-hint")).size(CAPTION_SIZE),
    ]
    .spacing(SPACING);
    if draft.steps.is_empty() {
        card = card.push(text(fl!("ui-post-connect-empty")).size(CAPTION_SIZE));
    }
    for (index, step) in draft.steps.iter().enumerate() {
        card = card.push(step_row(index, step, draft.selected == Some(index)));
    }
    let selected = draft.selected.is_some();
    card.push(text(fl!("ui-post-connect-order-hint")).size(CAPTION_SIZE))
        .push(
            row![
                button(text(fl!("ui-post-connect-add")))
                    .style(button::secondary)
                    .on_press(edit(StepEdit::Add)),
                button(text(fl!("ui-post-connect-remove")))
                    .style(button::secondary)
                    .on_press_maybe(selected.then(|| edit(StepEdit::Remove))),
                button(text(fl!("ui-post-connect-move-up")))
                    .style(button::secondary)
                    .on_press_maybe(draft.can_move_up().then(|| edit(StepEdit::MoveUp))),
                button(text(fl!("ui-post-connect-move-down")))
                    .style(button::secondary)
                    .on_press_maybe(draft.can_move_down().then(|| edit(StepEdit::MoveDown))),
            ]
            .spacing(SPACING),
        )
        .into()
}

/// One step: on or off, its command, its delay, what a failure does; framed when selected.
fn step_row(index: usize, step: &PostConnectStep, selected: bool) -> Element<'_, Message> {
    let caption = |label: String| text(label).size(CAPTION_SIZE);
    let fields = row![
        checkbox(step.enabled).on_toggle(move |on| edit(StepEdit::Enabled(index, on))),
        column![
            caption(fl!("ui-post-connect-command")),
            text_input(&fl!("ui-post-connect-command-placeholder"), &step.input)
                .on_input(move |input| edit(StepEdit::Input(index, input))),
        ]
        .width(Length::Fill),
        column![
            caption(fl!("ui-post-connect-delay")),
            text_input("", &step.delay_ms.to_string())
                .on_input(move |typed| edit(StepEdit::Delay(index, typed)))
                .width(DELAY_WIDTH),
        ],
        column![
            caption(fl!("ui-post-connect-on-failure")),
            pick_list(
                [
                    FailureChoice(OnFailure::Continue),
                    FailureChoice(OnFailure::Stop)
                ],
                Some(FailureChoice(step.on_failure)),
                move |choice: FailureChoice| edit(StepEdit::OnFailure(index, choice.0)),
            ),
        ],
    ]
    .spacing(SPACING)
    .align_y(Alignment::End);
    let framed = container(fields).padding(ROW_PADDING).width(Length::Fill);
    let framed = if selected {
        framed.style(container::bordered_box)
    } else {
        framed
    };
    mouse_area(framed)
        .on_press(edit(StepEdit::Select(index)))
        .into()
}
