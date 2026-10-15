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

//! The Cron Job Manager tool view: crontab expression natural language explanation,
//! next execution schedule calculation, and system scheduled tasks inspection.

use heimdall_app::TabId;
use iced::widget::{button, column, container, row, scrollable, text, text_input};
use iced::{Alignment, Element, Length, Task, Theme};

use heimdall_core::tools::cron_job::{self, CrontabParser, TaskEntry};

use super::{CopySlot, ToolPane};
use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, spacing};

/// Mode for the Cron Job Manager view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CronMode {
    /// Crontab expression parser and next runs.
    #[default]
    Crontab,
    /// System scheduled tasks viewer.
    ScheduledTasks,
}

/// Messages emitted by the Cron Job Manager view.
#[derive(Debug, Clone)]
pub enum CronJobMessage {
    /// Mode changed.
    ModeChanged(CronMode),
    /// Crontab expression input changed.
    ExpressionChanged(String),
    /// Apply preset expression.
    ApplyPreset(String),
    /// Refresh scheduled tasks list.
    RefreshTasks,
    /// Scheduled tasks loaded.
    TasksLoaded(Result<Vec<TaskEntry>, String>),
    /// Filter query text changed for scheduled tasks.
    TaskFilterChanged(String),
    /// Copy CSV export of scheduled tasks.
    CopyCsv,
    /// Copy crontab description.
    CopySchedule,
}

/// The Cron Job Manager tool pane state.
#[derive(Debug)]
pub struct CronJobPane {
    mode: CronMode,
    expression: String,
    tasks_loading: bool,
    tasks_error: Option<String>,
    tasks: Vec<TaskEntry>,
    task_filter: String,
}

impl Default for CronJobPane {
    fn default() -> Self {
        Self {
            mode: CronMode::Crontab,
            expression: "*/15 * * * *".to_string(),
            tasks_loading: false,
            tasks_error: None,
            tasks: Vec::new(),
            task_filter: String::new(),
        }
    }
}

/// Outcome of updating the Cron Job tool pane.
#[derive(Debug)]
pub enum Outcome {
    None,
    Task(Task<Message>),
    Copy(CopySlot, String),
}

impl CronJobPane {
    /// Handles tool messages.
    pub fn update(&mut self, tab: TabId, message: CronJobMessage) -> Outcome {
        match message {
            CronJobMessage::ModeChanged(mode) => {
                self.mode = mode;
                Outcome::None
            }
            CronJobMessage::ExpressionChanged(expr) => {
                self.expression = expr;
                Outcome::None
            }
            CronJobMessage::ApplyPreset(expr) => {
                self.expression = expr;
                Outcome::None
            }
            CronJobMessage::RefreshTasks => {
                self.tasks_loading = true;
                self.tasks_error = None;

                let task = Task::perform(
                    async move {
                        tokio::task::spawn_blocking(cron_job::run_tasks_command)
                            .await
                            .map_err(|e| e.to_string())
                            .and_then(|r| r)
                    },
                    move |res| {
                        Message::Tool(
                            tab,
                            super::ToolMessage::CronJob(CronJobMessage::TasksLoaded(res)),
                        )
                    },
                );

                Outcome::Task(task)
            }
            CronJobMessage::TasksLoaded(Ok(tasks)) => {
                self.tasks_loading = false;
                self.tasks = tasks;
                Outcome::None
            }
            CronJobMessage::TasksLoaded(Err(err)) => {
                self.tasks_loading = false;
                self.tasks_error = Some(err);
                Outcome::None
            }
            CronJobMessage::TaskFilterChanged(filter) => {
                self.task_filter = filter;
                Outcome::None
            }
            CronJobMessage::CopyCsv => {
                let csv = cron_job::build_tasks_csv(&self.tasks);
                Outcome::Copy(CopySlot::CronJobExport, csv)
            }
            CronJobMessage::CopySchedule => {
                let desc = CrontabParser::describe(&self.expression);
                Outcome::Copy(CopySlot::CronJobExport, desc)
            }
        }
    }

    /// Renders the Cron Job Manager tool view.
    pub fn view<'a>(
        &'a self,
        tab: TabId,
        state: &ToolPane,
        _theme: &Theme,
    ) -> Element<'a, Message> {
        let send = move |msg| Message::Tool(tab, super::ToolMessage::CronJob(msg));

        let btn_crontab_mode =
            button(text(fl!("ui-tool-cronjob-tab-crontab")).size(font_size::CAPTION))
                .padding([spacing::XS, spacing::SM])
                .style(if self.mode == CronMode::Crontab {
                    styles::primary
                } else {
                    styles::secondary
                })
                .on_press(send(CronJobMessage::ModeChanged(CronMode::Crontab)));

        let btn_tasks_mode =
            button(text(fl!("ui-tool-cronjob-tab-schtasks")).size(font_size::CAPTION))
                .padding([spacing::XS, spacing::SM])
                .style(if self.mode == CronMode::ScheduledTasks {
                    styles::primary
                } else {
                    styles::secondary
                })
                .on_press(send(CronJobMessage::ModeChanged(CronMode::ScheduledTasks)));

        let mode_row = row![btn_crontab_mode, btn_tasks_mode]
            .spacing(spacing::SM)
            .align_y(Alignment::Center);

        let content_body: Element<'a, Message> = match self.mode {
            CronMode::Crontab => {
                let expr_input =
                    text_input(&fl!("ui-tool-cronjob-expr-placeholder"), &self.expression)
                        .on_input(move |s| send(CronJobMessage::ExpressionChanged(s)))
                        .width(Length::Fill);

                let btn_preset_min = button(
                    text(fl!("ui-tool-cronjob-preset-every-min")).size(font_size::SMALL_CAPTION),
                )
                .padding([spacing::XS, spacing::XS])
                .style(styles::secondary)
                .on_press(send(CronJobMessage::ApplyPreset("* * * * *".to_string())));

                let btn_preset_hr = button(
                    text(fl!("ui-tool-cronjob-preset-hourly")).size(font_size::SMALL_CAPTION),
                )
                .padding([spacing::XS, spacing::XS])
                .style(styles::secondary)
                .on_press(send(CronJobMessage::ApplyPreset("0 * * * *".to_string())));

                let btn_preset_daily = button(
                    text(fl!("ui-tool-cronjob-preset-daily")).size(font_size::SMALL_CAPTION),
                )
                .padding([spacing::XS, spacing::XS])
                .style(styles::secondary)
                .on_press(send(CronJobMessage::ApplyPreset("0 0 * * *".to_string())));

                let btn_preset_weekly = button(
                    text(fl!("ui-tool-cronjob-preset-weekly")).size(font_size::SMALL_CAPTION),
                )
                .padding([spacing::XS, spacing::XS])
                .style(styles::secondary)
                .on_press(send(CronJobMessage::ApplyPreset("0 0 * * 1".to_string())));

                let presets_row = row![
                    btn_preset_min,
                    btn_preset_hr,
                    btn_preset_daily,
                    btn_preset_weekly,
                ]
                .spacing(spacing::XS)
                .align_y(Alignment::Center);

                let description_text = CrontabParser::describe(&self.expression);

                let copy_schedule_btn = super::copy_button(
                    fl!("ui-tool-cronjob-schedule-title"),
                    state.copied(CopySlot::CronJobExport),
                    send(CronJobMessage::CopySchedule),
                    super::COPY_PADDING,
                );

                let desc_card = container(
                    column![
                        row![
                            text(fl!("ui-tool-cronjob-schedule-title"))
                                .size(font_size::CAPTION)
                                .style(text::secondary),
                            copy_schedule_btn,
                        ]
                        .spacing(spacing::SM)
                        .align_y(Alignment::Center),
                        text(description_text).size(font_size::SUBTITLE),
                    ]
                    .spacing(spacing::XS),
                )
                .padding(spacing::MD)
                .style(styles::card)
                .width(Length::Fill);

                let next_runs = CrontabParser::next_runs(&self.expression, 5);

                let mut next_runs_col = column![
                    text(fl!("ui-tool-cronjob-next-runs-title"))
                        .size(font_size::CAPTION)
                        .style(text::secondary)
                ]
                .spacing(spacing::XS);

                for run in next_runs {
                    next_runs_col = next_runs_col.push(
                        row![
                            text("•").size(font_size::CAPTION).style(text::secondary),
                            text(run).size(font_size::CAPTION),
                        ]
                        .spacing(spacing::SM),
                    );
                }

                let next_runs_card = container(next_runs_col)
                    .padding(spacing::MD)
                    .style(styles::card)
                    .width(Length::Fill);

                column![expr_input, presets_row, desc_card, next_runs_card]
                    .spacing(spacing::MD)
                    .into()
            }
            CronMode::ScheduledTasks => {
                let btn_refresh =
                    button(text(fl!("ui-tool-cronjob-btn-refresh")).size(font_size::CAPTION))
                        .padding([spacing::XS, spacing::SM])
                        .on_press_maybe(if self.tasks_loading {
                            None
                        } else {
                            Some(send(CronJobMessage::RefreshTasks))
                        });

                let copy_csv_btn = super::copy_button(
                    fl!("ui-tool-cronjob-btn-copy-csv"),
                    state.copied(CopySlot::CronJobExport),
                    send(CronJobMessage::CopyCsv),
                    super::COPY_PADDING,
                );

                let filter_input = text_input(
                    &fl!("ui-tool-cronjob-filter-placeholder"),
                    &self.task_filter,
                )
                .on_input(move |s| send(CronJobMessage::TaskFilterChanged(s)))
                .width(Length::Fill);

                let controls_row = row![btn_refresh, copy_csv_btn, filter_input]
                    .spacing(spacing::SM)
                    .align_y(Alignment::Center);

                let tasks_body: Element<'a, Message> = if let Some(err) = &self.tasks_error {
                    container(text(err).size(font_size::CAPTION).style(text::danger))
                        .padding(spacing::MD)
                        .into()
                } else if self.tasks.is_empty() {
                    container(
                        text(fl!("ui-tool-cronjob-empty-tasks"))
                            .size(font_size::CAPTION)
                            .style(text::secondary),
                    )
                    .padding(spacing::XL)
                    .center_x(Length::Fill)
                    .into()
                } else {
                    let filter_lower = self.task_filter.trim().to_lowercase();
                    let matching_tasks: Vec<&'a TaskEntry> = self
                        .tasks
                        .iter()
                        .filter(|t| {
                            if filter_lower.is_empty() {
                                true
                            } else {
                                t.name.to_lowercase().contains(&filter_lower)
                                    || t.status.to_lowercase().contains(&filter_lower)
                            }
                        })
                        .collect();

                    let header = row![
                        text(fl!("ui-tool-cronjob-col-taskname"))
                            .size(font_size::SMALL_CAPTION)
                            .style(text::secondary)
                            .width(Length::FillPortion(3)),
                        text(fl!("ui-tool-cronjob-col-nextrun"))
                            .size(font_size::SMALL_CAPTION)
                            .style(text::secondary)
                            .width(Length::FillPortion(2)),
                        text(fl!("ui-tool-cronjob-col-status"))
                            .size(font_size::SMALL_CAPTION)
                            .style(text::secondary)
                            .width(Length::FillPortion(1)),
                        text(fl!("ui-tool-cronjob-col-lastrun"))
                            .size(font_size::SMALL_CAPTION)
                            .style(text::secondary)
                            .width(Length::FillPortion(2)),
                        text(fl!("ui-tool-cronjob-col-lastresult"))
                            .size(font_size::SMALL_CAPTION)
                            .style(text::secondary)
                            .width(Length::FillPortion(1)),
                    ]
                    .spacing(spacing::SM);

                    let mut rows_col = column![header].spacing(spacing::XS);

                    for task in matching_tasks {
                        let task_row = row![
                            text(&task.name)
                                .size(font_size::SMALL_CAPTION)
                                .width(Length::FillPortion(3)),
                            text(&task.next_run_time)
                                .size(font_size::SMALL_CAPTION)
                                .width(Length::FillPortion(2)),
                            text(&task.status)
                                .size(font_size::SMALL_CAPTION)
                                .width(Length::FillPortion(1)),
                            text(&task.task_to_run)
                                .size(font_size::SMALL_CAPTION)
                                .width(Length::FillPortion(2)),
                            text(&task.author)
                                .size(font_size::SMALL_CAPTION)
                                .width(Length::FillPortion(1)),
                        ]
                        .spacing(spacing::SM);

                        rows_col = rows_col.push(task_row);
                    }

                    scrollable(
                        container(rows_col)
                            .padding(spacing::SM)
                            .style(styles::card)
                            .width(Length::Fill),
                    )
                    .height(Length::Fill)
                    .into()
                };

                column![controls_row, tasks_body]
                    .spacing(spacing::MD)
                    .into()
            }
        };

        column![mode_row, content_body]
            .spacing(spacing::MD)
            .padding(spacing::MD)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }
}
