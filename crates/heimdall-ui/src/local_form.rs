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
//! folder it starts in. The C# elevation choice is left out: Heimdall-rs never runs a shell
//! elevated.

use heimdall_app::Message as AppMessage;
use heimdall_app::local_draft::SHELL_PRESETS;
use heimdall_app::profile_draft::{ProfileDraft, ProfileField};
use iced::Element;
use iced::widget::{column, pick_list, text};

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
    .spacing(spacing::SM)
    .into()
}
