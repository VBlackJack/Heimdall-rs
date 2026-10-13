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

//! The Scheduled page and its dialog, as the C# ones: the title with Add, Edit and Delete,
//! the tasks' grid (on, session, type, schedule, last run, next run, and what became of the
//! last run), a double click editing a task, the empty state with its own Add; the dialog
//! with its session, its schedule, its next run, what is wrong, and its questions before
//! deleting a task or losing what was changed.

use heimdall_app::scheduled_form::{
    Fields, NextRunPreview, ScheduleKind, ScheduledForm, ScheduledProblem,
};
use heimdall_app::{
    App, Message as AppMessage, ScheduledChoice, ScheduledMessage, local_time_text, server_text,
};
use heimdall_core::scheduled::{Schedule, ScheduledTask};
use iced::widget::{
    Column, button, checkbox, column, container, mouse_area, pick_list, row, rule, space, text,
    text_input,
};
use iced::{Alignment, Element, Length};

use crate::dialog_parts::{self, Severity};
use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, spacing};
use crate::tree_row::{Mark, RowChrome};

/// Room around the page.
const PAGE_PADDING: f32 = 16.0;

/// Widths of the grid's fixed columns, as the C# ones: On, Type, Last run, Next run.
const ENABLED_WIDTH: f32 = 40.0;
const TYPE_WIDTH: f32 = 70.0;
const TIME_WIDTH: f32 = 130.0;

/// Widths of the dialog's time and interval fields, as the C# ones.
const TIME_FIELD_WIDTH: f32 = 120.0;
const INTERVAL_FIELD_WIDTH: f32 = 100.0;

fn scheduled(message: ScheduledMessage) -> Message {
    Message::App(AppMessage::Scheduled(message))
}

/// A task's schedule, in the window's language: "Daily 08:00", "Every 30 min".
#[must_use]
pub fn schedule_text(schedule: &Schedule) -> String {
    match (schedule.time_of_day(), schedule.interval_minutes()) {
        (Some(time), _) => fl!("ui-scheduled-schedule-daily", time = time.text()),
        (None, Some(minutes)) => fl!("ui-scheduled-schedule-interval", minutes = minutes),
        (None, None) => String::new(),
    }
}

/// The Scheduled page of `app`, as the C# one.
#[must_use]
pub fn page(app: &App) -> Element<'_, Message> {
    let selected = app.selected_scheduled();
    let header = row![
        text(fl!("ui-scheduled-title")).size(font_size::TITLE),
        space::horizontal(),
        button(text(fl!("ui-scheduled-add")))
            .style(styles::primary)
            .on_press(scheduled(ScheduledMessage::Add)),
        button(text(fl!("ui-scheduled-edit")))
            .style(styles::secondary)
            .on_press_maybe(selected.map(|_| scheduled(ScheduledMessage::EditSelected))),
        button(text(fl!("ui-scheduled-delete")))
            .style(styles::secondary)
            .on_press_maybe(selected.map(|_| scheduled(ScheduledMessage::DeleteSelected))),
    ]
    .spacing(spacing::SM)
    .align_y(Alignment::Center);
    let tasks = app.scheduled_tasks();
    let body: Element<'_, Message> = if tasks.is_empty() {
        // As the C# empty state: a sentence and its own Add.
        container(
            column![
                text(fl!("ui-scheduled-empty"))
                    .size(font_size::LARGE)
                    .style(text::secondary),
                button(text(fl!("ui-scheduled-empty-add")))
                    .style(styles::primary)
                    .on_press(scheduled(ScheduledMessage::Add)),
            ]
            .spacing(spacing::SM)
            .align_x(Alignment::Center),
        )
        .center(Length::Fill)
        .into()
    } else {
        let rows = Column::new()
            .spacing(spacing::XS)
            .extend(tasks.iter().map(|task| task_row(task, selected)));
        column![
            columns(),
            rule::horizontal(1),
            styles::scroll(rows).height(Length::Fill)
        ]
        .spacing(spacing::SM)
        .height(Length::Fill)
        .into()
    };
    container(
        column![header, body]
            .spacing(spacing::SM)
            .width(Length::Fill)
            .height(Length::Fill),
    )
    .padding(PAGE_PADDING)
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

/// The grid's column titles, as the C# ones, and the last run's outcome.
fn columns<'a>() -> Element<'a, Message> {
    let title = |label: String| text(label).size(font_size::CAPTION).style(text::secondary);
    row![
        title(fl!("ui-scheduled-column-enabled")).width(ENABLED_WIDTH),
        title(fl!("ui-scheduled-column-server")).width(Length::Fill),
        title(fl!("ui-scheduled-column-type")).width(TYPE_WIDTH),
        title(fl!("ui-scheduled-column-schedule")).width(Length::Fill),
        title(fl!("ui-scheduled-column-last-run")).width(TIME_WIDTH),
        title(fl!("ui-scheduled-column-next-run")).width(TIME_WIDTH),
        title(fl!("ui-scheduled-column-outcome")).width(Length::Fill),
    ]
    .spacing(spacing::SM)
    .into()
}

/// A task's row: its switch, saved at once, then what it opens and when; a click selects
/// it, a double click edits it.
fn task_row<'a>(task: &'a ScheduledTask, selected: Option<&str>) -> Element<'a, Message> {
    let cell = |value: String| text(value).size(font_size::CAPTION);
    let id = task.id.clone();
    let line = row![
        container(
            checkbox(task.enabled)
                .style(styles::checkbox)
                .on_toggle(move |on| scheduled(ScheduledMessage::Toggle(id.clone(), on)))
        )
        .width(ENABLED_WIDTH),
        cell(server_text(&task.server_name)).width(Length::Fill),
        cell(server_text(&task.connection_type)).width(TYPE_WIDTH),
        cell(schedule_text(&task.schedule)).width(Length::Fill),
        cell(task.last_run.map(local_time_text).unwrap_or_default()).width(TIME_WIDTH),
        cell(
            task.next_run
                .filter(|_| task.enabled)
                .map(local_time_text)
                .unwrap_or_default()
        )
        .width(TIME_WIDTH),
        cell(crate::texts::scheduled_outcome(task)).width(Length::Fill),
    ]
    .spacing(spacing::SM)
    .align_y(Alignment::Center);
    let mark = if selected == Some(task.id.as_str()) {
        Mark::Selected
    } else {
        Mark::None
    };
    mouse_area(RowChrome::new(line, mark))
        .on_press(scheduled(ScheduledMessage::Select(task.id.clone())))
        .on_double_click(scheduled(ScheduledMessage::Edit(task.id.clone())))
        .interaction(iced::mouse::Interaction::Pointer)
        .into()
}

/// A session in the dialog's list: "Name (Type)", as the C# `DisplayText`.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Choice(ScheduledChoice);

impl std::fmt::Display for Choice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", server_text(&self.0.name), self.0.kind.label())
    }
}

/// A schedule kind in the dialog's list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Kind(ScheduleKind);

impl std::fmt::Display for Kind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&match self.0 {
            ScheduleKind::Daily => fl!("ui-scheduled-kind-daily"),
            ScheduleKind::Interval => fl!("ui-scheduled-kind-interval"),
        })
    }
}

/// The sentence for `problem`, as the C# validation says it.
#[must_use]
pub fn problem_text(problem: ScheduledProblem) -> String {
    match problem {
        ScheduledProblem::ServerRequired => fl!("ui-scheduled-problem-server"),
        ScheduledProblem::TimeInvalid => fl!("ui-scheduled-problem-time"),
        ScheduledProblem::IntervalInvalid => fl!("ui-scheduled-problem-interval"),
    }
}

/// The next run the dialog's preview says.
#[must_use]
pub fn preview_text(preview: NextRunPreview) -> String {
    match preview {
        NextRunPreview::Daily { today, time } => fl!(
            "ui-scheduled-next-run-daily",
            day = if today {
                fl!("ui-scheduled-next-run-today")
            } else {
                fl!("ui-scheduled-next-run-tomorrow")
            },
            time = time.text()
        ),
        NextRunPreview::Interval(minutes) => {
            fl!("ui-scheduled-next-run-interval", minutes = minutes)
        }
    }
}

/// A message changing the dialog's fields as `change` does to `fields`.
fn changed(fields: &Fields, change: impl FnOnce(&mut Fields)) -> Message {
    let mut fields = fields.clone();
    change(&mut fields);
    scheduled(ScheduledMessage::Field(fields))
}

/// The dialog adding or editing a scheduled connection, as the C# `ScheduledTaskDialog`,
/// its sessions among `choices`, its next run `preview`.
#[must_use]
pub fn dialog<'a>(
    form: &'a ScheduledForm,
    choices: &[ScheduledChoice],
    preview: Option<NextRunPreview>,
) -> Element<'a, Message> {
    let fields = &form.fields;
    let title = dialog_parts::title(if form.editing.is_some() {
        fl!("ui-scheduled-dialog-edit-title")
    } else {
        fl!("ui-scheduled-dialog-add-title")
    });
    let options: Vec<Choice> = choices.iter().cloned().map(Choice).collect();
    let chosen = options
        .iter()
        .find(|choice| Some(&choice.0.id) == fields.server.as_ref())
        .cloned();
    let server_fields = fields.clone();
    let kind_fields = fields.clone();
    let mut content = column![
        dialog_parts::field(
            fl!("ui-scheduled-field-server"),
            pick_list(options, chosen, move |choice: Choice| {
                changed(&server_fields, |fields| fields.server = Some(choice.0.id))
            })
            .placeholder(fl!("ui-scheduled-field-server-placeholder"))
            .style(styles::pick_list)
            .menu_style(styles::menu),
        ),
        dialog_parts::field(
            fl!("ui-scheduled-field-kind"),
            pick_list(
                ScheduleKind::ALL.map(Kind),
                Some(Kind(fields.kind)),
                move |kind: Kind| changed(&kind_fields, |fields| fields.kind = kind.0),
            )
            .style(styles::pick_list)
            .menu_style(styles::menu),
        ),
    ]
    .spacing(spacing::SM);
    content = match fields.kind {
        ScheduleKind::Daily => {
            let time_fields = fields.clone();
            content.push(
                dialog_parts::field(
                    fl!("ui-scheduled-field-time"),
                    text_input("", &fields.time)
                        .style(styles::text_input)
                        .width(TIME_FIELD_WIDTH)
                        .on_input(move |time| changed(&time_fields, |fields| fields.time = time))
                        .on_submit(Message::App(AppMessage::ConfirmDialog)),
                )
                .push(dialog_parts::hint(fl!("ui-scheduled-field-time-hint"))),
            )
        }
        ScheduleKind::Interval => {
            let minutes_fields = fields.clone();
            content.push(dialog_parts::field(
                fl!("ui-scheduled-field-interval"),
                row![
                    text_input("", &fields.minutes)
                        .style(styles::text_input)
                        .width(INTERVAL_FIELD_WIDTH)
                        .on_input(move |minutes| {
                            changed(&minutes_fields, |fields| fields.minutes = minutes)
                        })
                        .on_submit(Message::App(AppMessage::ConfirmDialog)),
                    text(fl!("ui-scheduled-field-interval-suffix")).style(text::secondary),
                ]
                .spacing(spacing::SM)
                .align_y(Alignment::Center),
            ))
        }
    };
    let enabled_fields = fields.clone();
    content = content.push(
        checkbox(fields.enabled)
            .style(styles::checkbox)
            .label(fl!("ui-scheduled-field-enabled"))
            .text_size(font_size::BODY)
            .on_toggle(move |on| changed(&enabled_fields, |fields| fields.enabled = on)),
    );
    if let Some(preview) = preview {
        content = content.push(dialog_parts::hint(preview_text(preview)));
    }
    if let Some(problem) = form.problem {
        content = content.push(dialog_parts::error(problem_text(problem)));
    }
    dialog_parts::form(
        title,
        content,
        dialog_parts::buttons([
            dialog_parts::cancel(),
            dialog_parts::confirm(
                fl!("ui-scheduled-save"),
                Some(Message::App(AppMessage::ConfirmDialog)),
            ),
        ]),
    )
}

/// The question before deleting the task of session `name`, as the C# danger
/// confirmation.
#[must_use]
pub fn delete_question(name: &str) -> Element<'_, Message> {
    dialog_parts::choice(
        Severity::Danger,
        fl!("ui-scheduled-delete-title"),
        dialog_parts::body(fl!("ui-scheduled-delete-body", name = server_text(name))),
        fl!("ui-dialog-cancel-button"),
        fl!("ui-scheduled-delete"),
    )
}

/// The question before closing the dialog and losing what was changed, as the C#
/// unsaved-changes warning.
#[must_use]
pub fn discard_question<'a>() -> Element<'a, Message> {
    dialog_parts::choice(
        Severity::Warning,
        fl!("ui-scheduled-discard-title"),
        dialog_parts::body(fl!("ui-scheduled-discard-body")),
        fl!("ui-scheduled-discard-keep"),
        fl!("ui-scheduled-discard-confirm"),
    )
}
