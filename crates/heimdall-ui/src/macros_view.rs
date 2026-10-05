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

//! The terminal macros on the Settings page, each with its inputs and a way to forget it,
//! and what the status bar says of them.

use heimdall_app::macro_player::MacroOutcome;
use heimdall_app::{MacroMessage, Message as AppMessage, Notice, server_text};
use heimdall_core::macros::TerminalMacro;
use iced::widget::{Column, button, container, row, text};
use iced::{Alignment, Element, Length};

use crate::i18n::fl;
use crate::shell::Message;

const SPACING: f32 = 8.0;
const PADDING: f32 = 12.0;
/// Widest the card grows, as the other Settings cards.
const CARD_WIDTH: f32 = 720.0;
const SMALL_SIZE: f32 = 12.0;

/// The macros kept, each with how many inputs it types and Delete.
pub fn card(macros: &[TerminalMacro]) -> Element<'_, Message> {
    let mut content = Column::new().spacing(SPACING);
    if macros.is_empty() {
        content = content.push(text(fl!("ui-macros-empty")).size(SMALL_SIZE));
    }
    for kept in macros {
        content = content.push(
            row![
                text(server_text(&kept.name)),
                text(fl!("ui-macros-inputs", count = kept.entries.len())).size(SMALL_SIZE),
                iced::widget::space::horizontal(),
                button(text(fl!("ui-macros-delete")).size(SMALL_SIZE))
                    .style(button::danger)
                    .on_press(Message::App(AppMessage::Macro(MacroMessage::Delete(
                        kept.name.clone()
                    )))),
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
