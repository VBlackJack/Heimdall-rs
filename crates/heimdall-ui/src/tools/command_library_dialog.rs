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

//! The editors of the command library as dialogs, over the window, as the C#
//! `CommandActionDialog` (620 wide, up to 780 high) and `RunbookDialog` (680 by 720): a title,
//! the form in a scrolling body, what blocks or warns, then Cancel and Save at the bottom
//! right. The window draws the frame over a veil and routes Escape (Cancel) and Enter (Save,
//! the C# `IsDefault`) to it; the state stays in the library's pane.

use heimdall_app::TabId;
use heimdall_core::tools::user_commands::{
    Criticality, Draft, DraftError, OFFERED_PARAMETER_TYPES, ParameterDraft, TemplatePlatform,
    UserPlatform,
};
use iced::widget::{Column, checkbox, column, container, pick_list, row, space, text_input};
use iced::{Alignment, Element, Length};

use super::ToolMessage;
use super::command_library_tool::{
    CommandLibraryMessage, Field, IconState, ParamField, draft_error, icon_button, level_name,
    user_platform_name,
};
use crate::dialog_parts;
use crate::i18n::fl;
use crate::icons::Icon;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, spacing};

/// The command dialog's width, as the C# `CommandActionDialog.xaml:14`.
pub(super) const COMMAND_DIALOG_WIDTH: f32 = 620.0;

/// The most the command dialog grows to, as the C# `MaxHeight` (`xaml:14`).
pub(super) const COMMAND_DIALOG_HEIGHT: f32 = 780.0;

/// The runbook dialog's width and height, as the C# `RunbookDialog.xaml:15`.
pub(super) const RUNBOOK_DIALOG_WIDTH: f32 = 680.0;
pub(super) const RUNBOOK_DIALOG_HEIGHT: f32 = 720.0;

/// The width of a parameter's type drop-down, as the C# column (`xaml:144`).
const PARAMETER_TYPE_WIDTH: f32 = 120.0;

/// Room kept right of the form so the scroll bar does not cover the fields.
const SCROLLBAR_ROOM: f32 = 12.0;

/// One entry of a drop-down: a value and its label.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Pick<T> {
    /// What choosing it means.
    pub value: T,
    /// What it is called.
    pub label: String,
}

impl<T> std::fmt::Display for Pick<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label)
    }
}

/// A drop-down over `options` with `current` chosen: the C# combo boxes of the dialogs.
pub(super) fn pick<'a, T: Clone + PartialEq + 'a>(
    options: Vec<Pick<T>>,
    current: &T,
    on_choose: impl Fn(T) -> Message + 'a,
) -> Element<'a, Message> {
    let chosen = options.iter().find(|each| &each.value == current).cloned();
    pick_list(options, chosen, move |choice: Pick<T>| {
        on_choose(choice.value)
    })
    .width(Length::Fill)
    .text_size(font_size::BODY)
    .style(styles::pick_list)
    .menu_style(styles::menu)
    .into()
}

/// The frame of a dialog: `title`, `body` scrolling, the `notes` under it (a refusal in the
/// error colour, a warning), then Cancel and Save, each sending its message. `size` is the
/// C# dialog's width and the height the window leaves, never more than the C# dialog's own.
pub(super) fn frame<'a>(
    title: String,
    body: Element<'a, Message>,
    notes: Vec<Element<'a, Message>>,
    save: Message,
    cancel: Message,
    size: (f32, f32),
) -> Element<'a, Message> {
    let mut page: Column<'a, Message> = column![
        dialog_parts::title(title),
        styles::scroll(container(body).padding(iced::Padding {
            right: SCROLLBAR_ROOM,
            ..iced::Padding::ZERO
        }))
        .height(Length::Fill),
    ]
    .spacing(spacing::MD);
    for note in notes {
        page = page.push(note);
    }
    page = page.push(dialog_parts::buttons([
        dialog_parts::action(fl!("ui-tool-cmdlib-cancel"), styles::secondary).on_press(cancel),
        dialog_parts::confirm(fl!("ui-tool-cmdlib-save"), Some(save)),
    ]));
    container(page)
        .padding(spacing::LG)
        .width(size.0)
        .height(size.1)
        .style(styles::dialog)
        .into()
}

/// A field under its label, as the C# `DialogLabelStyle` above its box.
pub(super) fn labelled<'a>(
    label: String,
    input: impl Into<Element<'a, Message>>,
) -> Column<'a, Message> {
    column![dialog_parts::dialog_label(label), input.into()].spacing(spacing::XS)
}

/// The command dialog over `draft`; `problem` is why the last Save was refused.
pub(super) fn command_dialog(
    tab: TabId,
    draft: &Draft,
    problem: Option<DraftError>,
    height: f32,
) -> Element<'_, Message> {
    let send = move |message| Message::Tool(tab, ToolMessage::CmdLib(message));
    let input = |value: &str, which: Field| {
        text_input("", value)
            .size(font_size::BODY)
            .padding(super::INPUT_PADDING)
            .style(styles::text_input)
            .on_input(move |typed| send(CommandLibraryMessage::Field(which, typed)))
    };
    let platforms = pick(
        UserPlatform::ALL
            .into_iter()
            .map(|value| Pick {
                value,
                label: user_platform_name(value),
            })
            .collect(),
        &draft.platform,
        move |platform| send(CommandLibraryMessage::EditPlatform(platform)),
    );
    // The risk level is a combo box, as the C# `CmbRisk` (`xaml:86-93`), not a row of chips.
    let levels = pick(
        Criticality::ALL
            .into_iter()
            .map(|value| Pick {
                value,
                label: level_name(value),
            })
            .collect(),
        &draft.level,
        move |level| send(CommandLibraryMessage::EditLevel(level)),
    );
    let mut form: Column<'_, Message> = column![
        labelled(
            fl!("ui-tool-cmdlib-field-title"),
            input(&draft.title, Field::Title)
        ),
        labelled(
            fl!("ui-tool-cmdlib-field-category"),
            input(&draft.category, Field::Category)
        ),
        row![
            labelled(fl!("ui-tool-cmdlib-field-platform"), platforms).width(Length::Fill),
            labelled(fl!("ui-tool-cmdlib-field-level"), levels).width(Length::Fill),
        ]
        .spacing(spacing::XL),
        labelled(
            fl!("ui-tool-cmdlib-field-description"),
            input(&draft.description, Field::Description)
        ),
    ]
    .spacing(spacing::MD);
    for platform in [TemplatePlatform::Windows, TemplatePlatform::Linux] {
        if shows(draft.platform, platform) {
            form = form.push(template_section(tab, draft, platform));
        }
    }
    form = form.push(labelled(
        fl!("ui-tool-cmdlib-field-tags"),
        input(&draft.tags, Field::Tags),
    ));
    let mut notes: Vec<Element<'_, Message>> = Vec::new();
    if let Some(problem) = problem {
        notes.push(dialog_parts::error(draft_error(problem)).into());
    }
    let drift = drift_text(draft);
    if !drift.is_empty() {
        notes.push(dialog_parts::warning_card(drift));
    }
    frame(
        if draft.id.is_some() {
            fl!("ui-tool-cmdlib-editor-edit")
        } else {
            fl!("ui-tool-cmdlib-editor-new")
        },
        form.into(),
        notes,
        send(CommandLibraryMessage::Save),
        send(CommandLibraryMessage::CancelEdit),
        (
            COMMAND_DIALOG_WIDTH,
            COMMAND_DIALOG_HEIGHT.min(height).max(0.0),
        ),
    )
}

/// Whether the platform chosen has the template of `template`, as the C# `ShowWindowsSection`
/// and `ShowLinuxSection`.
const fn shows(chosen: UserPlatform, template: TemplatePlatform) -> bool {
    matches!(
        (chosen, template),
        (UserPlatform::Both, _)
            | (UserPlatform::Windows, TemplatePlatform::Windows)
            | (UserPlatform::Linux, TemplatePlatform::Linux)
    )
}

/// What the patterns and their parameter tables disagree about, one line each, a template
/// named when the command has both (the C# `RefreshConsistencyWarning`).
fn drift_text(draft: &Draft) -> String {
    let has_both = !draft.windows_pattern.trim().is_empty()
        && !draft.linux_pattern.trim().is_empty()
        && draft.platform == UserPlatform::Both;
    let mut lines: Vec<String> = Vec::new();
    for platform in [TemplatePlatform::Windows, TemplatePlatform::Linux] {
        if !shows(draft.platform, platform) {
            continue;
        }
        let (undeclared, unused) = draft.drift(platform);
        let prefix = |line: String| {
            if has_both {
                format!("{}: {line}", template_label(platform))
            } else {
                line
            }
        };
        if !undeclared.is_empty() {
            lines.push(prefix(fl!(
                "ui-tool-cmdlib-warn-undeclared",
                names = undeclared.join(", ")
            )));
        }
        if !unused.is_empty() {
            lines.push(prefix(fl!(
                "ui-tool-cmdlib-warn-unused",
                names = unused.join(", ")
            )));
        }
    }
    lines.join("\n")
}

/// The name of the operating system a template is for, as the C# `ToolCmdLibPlatform*`.
fn template_label(platform: TemplatePlatform) -> String {
    match platform {
        TemplatePlatform::Windows => fl!("ui-tool-cmdlib-platform-windows"),
        TemplatePlatform::Linux => fl!("ui-tool-cmdlib-platform-linux"),
    }
}

/// The section of one template: its section title, command pattern, name and parameter table
/// (`xaml:107-226`).
fn template_section(
    tab: TabId,
    draft: &Draft,
    platform: TemplatePlatform,
) -> Column<'_, Message> {
    let send = move |message| Message::Tool(tab, ToolMessage::CmdLib(message));
    let (pattern, pattern_field, name, name_field) = match platform {
        TemplatePlatform::Windows => (
            &draft.windows_pattern,
            Field::WindowsPattern,
            &draft.windows_name,
            Field::WindowsName,
        ),
        TemplatePlatform::Linux => (
            &draft.linux_pattern,
            Field::LinuxPattern,
            &draft.linux_name,
            Field::LinuxName,
        ),
    };
    let mut section: Column<'_, Message> = column![
        dialog_parts::section(template_section_title(platform), None),
        labelled(
            fl!("ui-tool-cmdlib-field-pattern"),
            text_input("", pattern)
                .size(font_size::BODY)
                .font(super::BOX_FONT)
                .padding(super::INPUT_PADDING)
                .style(styles::text_input)
                .on_input(move |typed| send(CommandLibraryMessage::Field(pattern_field, typed))),
        ),
        labelled(
            fl!("ui-tool-cmdlib-field-template-name"),
            text_input("", name)
                .size(font_size::BODY)
                .padding(super::INPUT_PADDING)
                .style(styles::text_input)
                .on_input(move |typed| send(CommandLibraryMessage::Field(name_field, typed))),
        ),
    ]
    .spacing(spacing::MD);
    let rows = draft.parameters(platform).map_or(&[][..], Vec::as_slice);
    if !rows.is_empty() {
        // The C# column headers over the first line of every row (`xaml:127-135`).
        section = section.push(
            row![
                dialog_parts::hint(fl!("ui-tool-cmdlib-param-name")).width(Length::FillPortion(1)),
                dialog_parts::hint(fl!("ui-tool-cmdlib-param-label")).width(Length::FillPortion(1)),
                dialog_parts::hint(fl!("ui-tool-cmdlib-param-type")).width(PARAMETER_TYPE_WIDTH),
                space::horizontal().width(crate::icons::BUTTON_SIDE),
            ]
            .spacing(spacing::SM),
        );
    }
    for (index, parameter) in rows.iter().enumerate() {
        section = section.push(parameter_row(tab, platform, index, parameter));
    }
    section
        .push(
            dialog_parts::action(fl!("ui-tool-cmdlib-param-add"), styles::secondary)
                .on_press(send(CommandLibraryMessage::AddParam(platform))),
        )
        .spacing(spacing::SM)
}

/// The title of the section of the template of `platform`, as the C# `ToolCmdLibDialogSection*`.
fn template_section_title(platform: TemplatePlatform) -> String {
    match platform {
        TemplatePlatform::Windows => fl!("ui-tool-cmdlib-section-windows"),
        TemplatePlatform::Linux => fl!("ui-tool-cmdlib-section-linux"),
    }
}

/// One row of the parameter table, as the C# `ItemTemplate` (`xaml:138-217`): name, label,
/// type and a remove button; then the default and Required; the values of a choice; the
/// description.
fn parameter_row(
    tab: TabId,
    platform: TemplatePlatform,
    index: usize,
    parameter: &ParameterDraft,
) -> Element<'_, Message> {
    let send = move |message| Message::Tool(tab, ToolMessage::CmdLib(message));
    let text_field = move |placeholder: String, value: &str, which: ParamField| {
        text_input(&placeholder, value)
            .size(font_size::BODY)
            .padding(super::INPUT_PADDING)
            .style(styles::text_input)
            .on_input(move |typed| {
                send(CommandLibraryMessage::ParamText(
                    platform, index, which, typed,
                ))
            })
    };
    let kinds = pick(
        OFFERED_PARAMETER_TYPES
            .into_iter()
            .map(str::to_owned)
            .chain(
                // A type an imported file wrote and the editor does not offer is still shown.
                (!OFFERED_PARAMETER_TYPES.contains(&parameter.kind.as_str())
                    && !parameter.kind.is_empty())
                .then(|| parameter.kind.clone()),
            )
            .map(|kind| Pick {
                label: kind.clone(),
                value: kind,
            })
            .collect(),
        &parameter.kind,
        move |kind| send(CommandLibraryMessage::ParamKind(platform, index, kind)),
    );
    let mut lines: Column<'_, Message> = column![
        row![
            text_field(
                fl!("ui-tool-cmdlib-param-name"),
                &parameter.name,
                ParamField::Name
            )
            .font(super::BOX_FONT)
            .width(Length::FillPortion(1)),
            text_field(
                fl!("ui-tool-cmdlib-param-label"),
                &parameter.label,
                ParamField::Label
            )
            .width(Length::FillPortion(1)),
            container(kinds).width(PARAMETER_TYPE_WIDTH),
            icon_button(
                Icon::Close,
                fl!("ui-tool-cmdlib-delete"),
                IconState::Plain,
                Some(send(CommandLibraryMessage::RemoveParam(platform, index))),
            ),
        ]
        .spacing(spacing::SM)
        .align_y(Alignment::Center),
        row![
            text_field(
                fl!("ui-tool-cmdlib-param-default"),
                &parameter.default_value,
                ParamField::Default
            )
            .width(Length::Fill),
            checkbox(parameter.required)
                .style(styles::checkbox)
                .label(fl!("ui-tool-cmdlib-param-required"))
                .text_size(font_size::CAPTION)
                .on_toggle(move |on| {
                    send(CommandLibraryMessage::ParamRequired(platform, index, on))
                }),
        ]
        .spacing(spacing::SM)
        .align_y(Alignment::Center),
    ]
    .spacing(spacing::XS);
    // Only for a choice: for every other type there is nothing to choose from (`xaml:194`).
    if parameter.is_choice() {
        lines = lines.push(text_field(
            fl!("ui-tool-cmdlib-param-allowed"),
            &parameter.allowed_values,
            ParamField::Allowed,
        ));
    }
    lines = lines.push(text_field(
        fl!("ui-tool-cmdlib-param-description"),
        &parameter.description,
        ParamField::Description,
    ));
    container(lines).padding([spacing::XS, 0.0]).into()
}
