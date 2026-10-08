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

//! A local shell profile's form, as the C# cards: "Local shell", the executable (typed, or
//! taken from the common shells) and its arguments, then "Advanced shell options", the
//! folder it starts in and "Run as administrator". The C# elevation modes come down to that
//! one box: a shell run as administrator opens in a window of its own, never in a tab.

use heimdall_app::Message as AppMessage;
use heimdall_app::elevated_shell;
use heimdall_app::local_draft::SHELL_PRESETS;
use heimdall_app::profile_draft::{ProfileDraft, ProfileField, ProfileToggle};
use iced::Element;
use iced::widget::{checkbox, column, pick_list, text};

use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, spacing};

/// The two cards; `field` draws a field of the form as the other cards do.
pub fn view<'a>(
    draft: &'a ProfileDraft,
    field: impl Fn(ProfileField) -> Element<'a, Message>,
) -> Element<'a, Message> {
    let chosen = SHELL_PRESETS
        .iter()
        .find(|preset| **preset == draft.local_program.trim())
        .copied();
    column![
        text(fl!("ui-profile-local-title")),
        text(fl!("ui-profile-local-desc")).size(font_size::CAPTION),
        field(ProfileField::LocalProgram),
        pick_list(SHELL_PRESETS, chosen, |preset: &str| {
            Message::App(AppMessage::ProfileField {
                field: ProfileField::LocalProgram,
                value: preset.to_owned(),
            })
        })
        .style(styles::pick_list)
        .menu_style(styles::menu)
        .placeholder(fl!("ui-profile-local-presets")),
        field(ProfileField::LocalArguments),
        text(fl!("ui-profile-local-advanced-title")),
        text(fl!("ui-profile-local-advanced-desc")).size(font_size::CAPTION),
        field(ProfileField::WorkingDirectory),
    ]
    .push(run_as_administrator(draft))
    .spacing(spacing::SM)
    .into()
}

/// The "Run as administrator" box and what it does, on Windows; elsewhere only while ticked,
/// to be cleared, with why it opens nothing there.
fn run_as_administrator<'a>(draft: &ProfileDraft) -> Option<Element<'a, Message>> {
    let toggle = ProfileToggle::RunAsAdministrator;
    if !draft.shows_toggle(toggle) {
        return None;
    }
    let hint = if elevated_shell::SUPPORTED {
        text(fl!("ui-profile-local-run-as-admin-hint")).size(font_size::CAPTION)
    } else {
        text(fl!("ui-profile-local-run-as-admin-windows-only"))
            .size(font_size::CAPTION)
            .style(text::danger)
    };
    Some(
        column![
            checkbox(draft.is_on(toggle))
                .style(styles::checkbox)
                .label(fl!("ui-profile-local-run-as-admin"))
                .on_toggle(move |on| Message::App(AppMessage::ProfileToggle { toggle, on })),
            hint,
        ]
        .spacing(spacing::XS)
        .into(),
    )
}
