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

//! A local shell profile's form, as the C# cards: "Local shell" on the General tab, the
//! executable (one list that can be typed in, the common shells offered) and its arguments,
//! then "Advanced
//! shell options" on the Options tab, the folder it starts in and "Run as administrator".
//! The C# elevation modes come down to that one box: a shell run as administrator opens in
//! a window of its own, never in a tab.

use heimdall_app::Message as AppMessage;
use heimdall_app::elevated_shell;
use heimdall_app::profile_draft::{ProfileDraft, ProfileField, ProfileToggle};
use iced::Element;
use iced::widget::{checkbox, column, combo_box, text};

use crate::dialog_parts;
use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{INPUT_LINE_HEIGHT, INPUT_PADDING, font_size, spacing};

/// The "Local shell" card of the General tab: the executable in one list that can be typed
/// in, as the C# editable combo (the common shells offered, any path allowed), `shells`
/// holding what is typed; `field` draws the other fields of the form as the other cards do.
pub fn basics<'a>(
    draft: &'a ProfileDraft,
    shells: &'a combo_box::State<String>,
    field: impl Fn(ProfileField) -> Element<'a, Message>,
) -> Element<'a, Message> {
    let set = |value: String| {
        Message::App(AppMessage::ProfileField {
            field: ProfileField::LocalProgram,
            value,
        })
    };
    column![
        dialog_parts::section(
            fl!("ui-profile-local-title"),
            Some(fl!("ui-profile-local-desc"))
        ),
        column![
            dialog_parts::label(fl!("ui-profile-local-executable")),
            combo_box(
                shells,
                &fl!("ui-profile-local-default-shell"),
                Some(&draft.local_program),
                set
            )
            .on_input(set)
            .input_style(styles::text_input)
            .menu_style(styles::menu)
            .padding(INPUT_PADDING)
            .line_height(iced::widget::text::LineHeight::Absolute(iced::Pixels(
                INPUT_LINE_HEIGHT
            )))
            .width(iced::Length::Fill),
        ]
        .spacing(spacing::XS),
        field(ProfileField::LocalArguments),
    ]
    .spacing(spacing::MD)
    .into()
}

/// The "Advanced shell options" card of the Options tab: the folder the shell starts in,
/// then "Run as administrator".
pub fn advanced<'a>(
    draft: &'a ProfileDraft,
    field: impl Fn(ProfileField) -> Element<'a, Message>,
) -> Element<'a, Message> {
    column![
        dialog_parts::section(
            fl!("ui-profile-local-advanced-title"),
            Some(fl!("ui-profile-local-advanced-desc"))
        ),
        field(ProfileField::WorkingDirectory),
    ]
    .push(run_as_administrator(draft))
    .spacing(spacing::MD)
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
        dialog_parts::hint(fl!("ui-profile-local-run-as-admin-hint"))
    } else {
        text(fl!("ui-profile-local-run-as-admin-windows-only"))
            .size(font_size::CAPTION)
            .style(styles::error_text)
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
