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

//! The preview of a Heimdall session document picked for "Import Sessions", as the C#
//! shows it in its `RdpImportDialog` with the profile import's words
//! (`RdpImportDialogTextOptions.ProfileImport`): a row for each profile, its clash and the
//! choice for it, then the profiles chosen imported.

use heimdall_app::{
    ImportActions, Message as AppMessage, ProfileImportMessage, ProfileImportPreview,
    ProfileImportRow, server_text,
};
use heimdall_core::import::rdp_file::Conflict;
use iced::widget::{Column, button, checkbox, column, container, pick_list, row, text};
use iced::{Element, Length};

use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, spacing};

/// Tallest the list of profiles grows before it scrolls.
const ROWS_HEIGHT: f32 = 320.0;

/// Widths of the columns: tick, source, name, host, status, conflict.
const COLUMNS: [f32; 6] = [50.0, 150.0, 190.0, 170.0, 240.0, 150.0];

/// Every choice for a clash, as the C# list offers them.
const CONFLICTS: [crate::rdp_view::ConflictChoice; 3] = [
    crate::rdp_view::ConflictChoice(Conflict::Skip),
    crate::rdp_view::ConflictChoice(Conflict::Replace),
    crate::rdp_view::ConflictChoice(Conflict::AutoRename),
];

fn app(message: ProfileImportMessage) -> Message {
    Message::App(AppMessage::ProfileImport(message))
}

fn cell<'a>(value: impl Into<String>, width: f32) -> Element<'a, Message> {
    container(
        text(value.into())
            .size(font_size::BODY)
            .wrapping(text::Wrapping::None),
    )
    .width(width)
    .clip(true)
    .into()
}

/// The preview, as the C# `RdpImportDialog` shows a profile import.
#[must_use]
pub fn preview(preview: &ProfileImportPreview) -> Element<'_, Message> {
    let (chosen, total, conflicts) = preview.counts();
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
            .map(|(index, row)| profile_row(index, row, &preview.source)),
    )
    .spacing(2.0);
    let content = column![
        crate::dialog_parts::list_title(fl!("ui-profile-import-title")),
        crate::dialog_parts::note(fl!("ui-profile-import-subtitle", count = total)),
        crate::dialog_parts::note(fl!(
            "ui-profile-import-summary",
            chosen = chosen,
            total = total,
            conflicts = conflicts
        )),
    ]
    .spacing(spacing::SM);
    let mut choices = row![
        button(text(fl!("ui-rdp-select-all")).size(font_size::BODY))
            .style(styles::secondary)
            .on_press(app(ProfileImportMessage::ChooseAll(true))),
        button(text(fl!("ui-rdp-select-none")).size(font_size::BODY))
            .style(styles::secondary)
            .on_press(app(ProfileImportMessage::ChooseAll(false))),
    ]
    .spacing(spacing::SM)
    .align_y(iced::Alignment::Center);
    if conflicts > 0 {
        choices = choices.push(text(fl!("ui-rdp-apply-all")).size(font_size::BODY));
        for choice in CONFLICTS {
            choices = choices.push(
                button(text(choice.to_string()).size(font_size::BODY))
                    .style(styles::secondary)
                    .on_press(app(ProfileImportMessage::ConflictAll(choice.0))),
            );
        }
    }
    content
        .push(choices)
        .push(header)
        .push(styles::scroll(rows).height(ROWS_HEIGHT))
        .push(crate::dialog_parts::buttons([
            crate::dialog_parts::cancel(),
            crate::dialog_parts::confirm(
                fl!("ui-profile-import-button"),
                preview
                    .can_import()
                    .then_some(Message::App(AppMessage::ConfirmDialog)),
            ),
        ]))
        .width(Length::Shrink)
        .into()
}

fn profile_row<'a>(index: usize, row: &'a ProfileImportRow, file: &str) -> Element<'a, Message> {
    // As the C# source column, the file and the profile's place in it.
    let source = match row.position {
        Some(position) => fl!(
            "ui-profile-import-source",
            file = file,
            position = position.to_string()
        ),
        None => file.to_owned(),
    };
    let host = row
        .endpoint
        .as_ref()
        .map(|(host, port)| format!("{host}:{port}"))
        .unwrap_or_default();
    let tick: Element<'_, Message> = if row.choosable() {
        checkbox(row.chosen)
            .style(styles::checkbox)
            .on_toggle(move |_| app(ProfileImportMessage::Choose(index)))
            .into()
    } else {
        checkbox(false).style(styles::checkbox).into()
    };
    let conflict: Element<'_, Message> = if row.conflict_with.is_some() {
        pick_list(
            CONFLICTS,
            Some(crate::rdp_view::ConflictChoice(row.conflict)),
            move |choice| app(ProfileImportMessage::Conflict(index, choice.0)),
        )
        .style(styles::pick_list)
        .menu_style(styles::menu)
        .text_size(font_size::BODY)
        .into()
    } else {
        text("").into()
    };
    row![
        container(tick).width(COLUMNS[0]),
        cell(source, COLUMNS[1]),
        cell(server_text(&row.name), COLUMNS[2]),
        cell(server_text(&host), COLUMNS[3]),
        container(text(status(row)).size(font_size::BODY)).width(COLUMNS[4]),
        container(conflict).width(COLUMNS[5]),
    ]
    .align_y(iced::Alignment::Center)
    .into()
}

/// What the C# status column says of a profile: why it is refused, or what it clashes with.
fn status(row: &ProfileImportRow) -> String {
    match (&row.refused, &row.conflict_with) {
        (Some(reason), _) => crate::texts::skip_reason(reason),
        (None, Some(with)) => fl!("ui-rdp-status-conflict", name = server_text(with)),
        (None, None) => String::new(),
    }
}

/// What each choice of the preview did, as the C# `StatusImportProfileSummary`.
#[must_use]
pub fn actions_line(actions: ImportActions) -> String {
    fl!(
        "ui-dialog-import-actions",
        imported = actions.imported,
        replaced = actions.replaced,
        renamed = actions.renamed,
        skipped = actions.skipped
    )
}
