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

//! The dialog offering the previous run's sessions again, as the C# "Restore previous
//! sessions": each ticked to reopen, a missing one named as such, "Select all", then
//! "Don't restore" or "Restore selected".

use heimdall_app::{Message as AppMessage, RestoreDialog};
use heimdall_core::utc::UtcTime;
use iced::widget::{Column, button, checkbox, column, row, scrollable, text};
use iced::{Element, Length};

use crate::i18n::fl;
use crate::shell::Message;

const SPACING: f32 = 8.0;
/// Size of the dialog's title.
const TITLE_SIZE: f32 = 20.0;
/// Size of a protocol, beside a session's name.
const DETAIL_SIZE: f32 = 12.0;
/// Height of the list before it scrolls.
const LIST_HEIGHT: f32 = 320.0;

fn choose(index: Option<usize>, chosen: bool) -> Message {
    Message::App(AppMessage::RestoreChoose { index, chosen })
}

/// The dialog.
pub fn view(dialog: &RestoreDialog) -> Element<'_, Message> {
    let saved_at = UtcTime::of(dialog.saved_at).iso();
    let reopenable = dialog.rows.iter().filter(|row| row.found.is_some());
    let all = reopenable.clone().all(|row| row.chosen) && reopenable.count() > 0;
    let chosen = dialog.rows.iter().any(|row| row.chosen);
    let rows = dialog.rows.iter().enumerate().map(|(index, row)| {
        let (label, protocol): (String, String) = match &row.found {
            Some((name, kind)) => (
                heimdall_app::server_text(name),
                if row.files {
                    fl!("ui-restore-files", protocol = kind.label())
                } else {
                    kind.label().to_owned()
                },
            ),
            None => (
                fl!("ui-restore-missing", id = row.profile.as_str()),
                String::new(),
            ),
        };
        let mut line = checkbox(row.chosen).label(label);
        if row.found.is_some() {
            line = line.on_toggle(move |on| choose(Some(index), on));
        }
        row![
            line.width(Length::Fill),
            text(protocol).size(DETAIL_SIZE).style(text::secondary),
        ]
        .spacing(SPACING)
        .into()
    });
    column![
        text(fl!("ui-restore-title")).size(TITLE_SIZE),
        text(fl!("ui-restore-message")),
        text(fl!("ui-restore-saved-at", time = saved_at))
            .size(DETAIL_SIZE)
            .style(text::secondary),
        checkbox(all)
            .label(fl!("ui-restore-select-all"))
            .on_toggle(|on| choose(None, on)),
        scrollable(Column::with_children(rows).spacing(SPACING / 2.0))
            .height(Length::Fixed(LIST_HEIGHT)),
        row![
            iced::widget::space::horizontal(),
            button(text(fl!("ui-restore-dont")))
                .style(button::secondary)
                .on_press(Message::App(AppMessage::DismissDialog)),
            button(text(fl!("ui-restore-selected")))
                .on_press_maybe(chosen.then_some(Message::App(AppMessage::ConfirmDialog))),
        ]
        .spacing(SPACING),
    ]
    .spacing(SPACING)
    .into()
}
