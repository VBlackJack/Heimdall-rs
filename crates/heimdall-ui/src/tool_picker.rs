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

//! The picker of "Add > Tool", as the C# `ToolPickerDialog`: a search box over the tools, the
//! list of those that match, and, for a tool that reaches a host, the host to open it on.

use heimdall_app::tools::ToolId;
use heimdall_app::{Message as AppMessage, ToolEntryEdit, ToolEntryForm, ToolsMessage};
use iced::widget::{button, column, row, text, text_input};
use iced::{Element, Length};

use crate::dialog_parts;
use crate::i18n::fl;
use crate::shell::{Message, confirm_buttons, name_field_id};
use crate::styles;
use crate::tokens::{font_size, spacing};
use crate::tools;

/// Height of the list of tools, a few rows tall, scrolled.
const LIST_HEIGHT: f32 = 260.0;

/// Whether `query` finds `tool`, as the sidebar's filter does: every word of it in the tool's
/// name, its identifier or one of its command words, whatever the case.
#[must_use]
pub fn finds(tool: ToolId, query: &str) -> bool {
    let name = tools::label(tool).to_lowercase();
    let code = tool.code().to_lowercase();
    query.split_whitespace().all(|word| {
        let word = word.to_lowercase();
        name.contains(&word)
            || code.contains(&word)
            || tool
                .prefixes()
                .iter()
                .any(|prefix| prefix.to_lowercase().contains(&word))
    })
}

/// The tools `query` finds, in the registry's order.
#[must_use]
pub fn matching(query: &str) -> Vec<ToolId> {
    ToolId::ALL
        .into_iter()
        .filter(|tool| finds(*tool, query))
        .collect()
}

fn edit(edit: ToolEntryEdit) -> Message {
    Message::App(AppMessage::Tools(ToolsMessage::EditEntry(edit)))
}

/// The dialog for `form`.
#[must_use]
pub fn dialog(form: &ToolEntryForm) -> Element<'_, Message> {
    let chosen = form.selected.as_ref().map(|(tool, _)| *tool);
    let mut list = column![].spacing(spacing::XS);
    for tool in matching(&form.query) {
        let label = tools::label(tool);
        list = list.push(
            button(text(label.clone()).size(font_size::BODY_LARGE))
                .width(Length::Fill)
                .style(styles::palette_row(chosen == Some(tool)))
                .on_press(edit(ToolEntryEdit::Pick(tool, label))),
        );
    }
    let mut body = column![
        dialog_parts::prompt(fl!("ui-tool-picker-prompt")),
        text_input(&fl!("ui-tool-picker-search"), &form.query)
            .style(styles::text_input)
            .id(name_field_id())
            .on_input(|value| edit(ToolEntryEdit::Query(value))),
        styles::scroll(list).height(LIST_HEIGHT),
    ]
    .spacing(spacing::SM);
    if chosen.is_some_and(ToolId::is_network) {
        body = body.push(
            row![
                text(fl!("ui-tool-picker-host")).size(font_size::BODY),
                text_input("", &form.host)
                    .style(styles::text_input)
                    .on_input(|value| edit(ToolEntryEdit::Host(value)))
                    .on_submit(Message::App(AppMessage::ConfirmDialog)),
            ]
            .spacing(spacing::SM),
        );
    }
    dialog_parts::form(
        dialog_parts::title(fl!("ui-tool-picker-title")),
        body,
        confirm_buttons(fl!("ui-tool-picker-confirm")),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_search_finds_every_tool() {
        assert_eq!(matching("").len(), ToolId::ALL.len());
    }

    #[test]
    fn a_search_finds_a_tool_by_its_identifier_and_ignores_the_case() {
        assert!(matching("PING").contains(&ToolId::Ping));
        assert!(matching("  pInG ").contains(&ToolId::Ping));
        assert!(!matching("ping").contains(&ToolId::Uuid));
    }

    #[test]
    fn a_search_nothing_matches_finds_no_tool() {
        assert!(matching("zzzzzz-no-such-tool").is_empty());
    }
}
