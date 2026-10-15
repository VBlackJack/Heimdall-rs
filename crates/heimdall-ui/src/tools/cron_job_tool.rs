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

//! The cron job manager, as the C# `CronJobManagerView` and `CronJobViewModel`: a crontab
//! pasted and parsed into its schedules, each with its next run and what it does, the one
//! selected with its next five runs; or this computer's Windows scheduled tasks, with their
//! status, next run, last run and last result. Either list copied as tab-separated rows.

use heimdall_app::TabId;
use heimdall_core::tools::cron_builder::Minute;
use heimdall_core::tools::cron_job::{
    self, CrontabEntry, NEXT_RUNS_COUNT, ScheduledTask, TasksError,
};
use heimdall_core::tools::date_time::{Instant, WallClock};
use iced::widget::text_editor::{Action, Content};
use iced::widget::{Column, button, column, container, row, scrollable, text};
use iced::{Element, Length, Task};

use super::{CopySlot, ToolMessage, ToolPane};
use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, spacing};

/// What is shown for a schedule with no run, as the C# `"-"`.
const NO_RUN: &str = "-";

/// The height of the crontab box, as the C# paste box.
const PASTE_HEIGHT: f32 = 140.0;

/// How wide each column of the crontab list is, as the C# grid's star widths.
const CRON_COLUMNS: [u16; 4] = [2, 4, 3, 4];

/// How wide each column of the tasks list is.
const TASK_COLUMNS: [u16; 5] = [5, 2, 3, 3, 2];

/// Which list is shown, as the C# `CronJobMode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CronMode {
    /// A crontab pasted.
    #[default]
    Paste,
    /// The Windows scheduled tasks.
    Tasks,
}

/// What the cron job manager is asked.
#[derive(Debug, Clone)]
pub enum CronJobMessage {
    /// The list shown.
    Mode(CronMode),
    /// Something done in the crontab box.
    Input(Action),
    /// Parse the crontab, as the C# `ParseCommand`.
    Parse,
    /// Clear it, as the C# `ClearPasteCommand`.
    Clear,
    /// The schedule selected, by its place.
    Select(usize),
    /// List the tasks again, as the C# `RefreshTasksCommand`.
    Refresh,
    /// The tasks listed, or why not.
    Loaded(Result<Vec<ScheduledTask>, TasksError>),
    /// Copy the list shown, as the C# `CopyAllCommand`.
    Copy,
}

/// What an update asks of its tab.
#[derive(Debug)]
pub enum Outcome {
    /// Nothing more.
    Done,
    /// This text copied.
    Copy(String),
    /// The tasks listed.
    Load,
}

impl Outcome {
    /// What tab `tab` runs for it.
    pub fn task(self, tab: TabId) -> Task<Message> {
        match self {
            Self::Done | Self::Copy(_) => Task::none(),
            Self::Load => Task::perform(
                async {
                    tokio::task::spawn_blocking(cron_job::load_scheduled_tasks)
                        .await
                        .unwrap_or_else(|error| Err(TasksError::Failed(error.to_string())))
                },
                move |loaded| {
                    Message::Tool(tab, ToolMessage::CronJob(CronJobMessage::Loaded(loaded)))
                },
            ),
        }
    }
}

/// Why a crontab gave nothing, as the C# errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PasteError {
    /// Nothing pasted, as `ToolCronJobErrorEmpty`.
    Empty,
    /// No schedule in it, as `ToolCronJobErrorNoParsed`.
    NoEntry,
}

/// What the status line says, as the C# `CronJobStatusKind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Status {
    None,
    Parsed(usize),
    Tasks(usize),
}

/// The cron job manager's state, as the C# view model's.
#[derive(Debug)]
pub struct CronJobPane {
    mode: CronMode,
    input: Content,
    entries: Vec<CrontabEntry>,
    /// The minute the crontab was parsed, which its runs follow, as the C# `_lastParseNow`.
    parsed_at: Option<Minute>,
    selected: Option<usize>,
    paste_error: Option<PasteError>,
    tasks: Vec<ScheduledTask>,
    loading: bool,
    tasks_error: Option<TasksError>,
    status: Status,
    now: fn() -> WallClock,
}

impl Default for CronJobPane {
    fn default() -> Self {
        Self::with_clock(local_now)
    }
}

/// This computer's wall clock now.
fn local_now() -> WallClock {
    let now = Instant::now();
    now.wall_clock(heimdall_app::time_zone::local_offset_at(now))
}

impl CronJobPane {
    /// Its state, its runs counted from what `now` says.
    #[must_use]
    pub fn with_clock(now: fn() -> WallClock) -> Self {
        Self {
            mode: CronMode::default(),
            input: Content::new(),
            entries: Vec::new(),
            parsed_at: None,
            selected: None,
            paste_error: None,
            tasks: Vec::new(),
            loading: false,
            tasks_error: None,
            status: Status::None,
            now,
        }
    }

    /// Applies `message`; what is then to be done.
    pub fn update(&mut self, message: CronJobMessage) -> Outcome {
        match message {
            CronJobMessage::Mode(mode) => self.mode = mode,
            CronJobMessage::Input(action) => self.input.perform(action),
            CronJobMessage::Parse => self.parse(),
            CronJobMessage::Clear => {
                self.input = Content::new();
                self.entries.clear();
                self.selected = None;
                self.paste_error = None;
                self.status = Status::None;
            }
            CronJobMessage::Select(index) => {
                self.selected = (index < self.entries.len()).then_some(index);
            }
            CronJobMessage::Refresh => {
                if !self.loading {
                    self.loading = true;
                    self.tasks_error = None;
                    return Outcome::Load;
                }
            }
            CronJobMessage::Loaded(loaded) => {
                self.loading = false;
                match loaded {
                    Ok(tasks) => {
                        self.status = Status::Tasks(tasks.len());
                        self.tasks = tasks;
                    }
                    Err(error) => self.tasks_error = Some(error),
                }
            }
            CronJobMessage::Copy => return Outcome::Copy(self.clipboard_text()),
        }
        Outcome::Done
    }

    /// The crontab pasted parsed, as the C# `Parse`.
    fn parse(&mut self) {
        self.selected = None;
        self.entries.clear();
        self.paste_error = None;
        let content = self.input.text();
        if content.trim().is_empty() {
            self.paste_error = Some(PasteError::Empty);
            return;
        }
        let entries = cron_job::parse_crontab(&content);
        if entries.is_empty() {
            self.paste_error = Some(PasteError::NoEntry);
            return;
        }
        self.parsed_at = Some(Minute::of(&(self.now)()));
        self.status = Status::Parsed(entries.len());
        self.entries = entries;
    }

    /// The next `count` runs of `entry` after the parse.
    fn runs(&self, entry: &CrontabEntry, count: usize) -> Vec<String> {
        self.parsed_at
            .map(|now| entry.next_runs(now, count))
            .unwrap_or_default()
    }

    /// The next run of `entry`, or `-`.
    fn next_run(&self, entry: &CrontabEntry) -> String {
        self.runs(entry, 1)
            .into_iter()
            .next()
            .unwrap_or_else(|| NO_RUN.to_owned())
    }

    /// The list shown as the C# `BuildClipboardText` copies it.
    fn clipboard_text(&self) -> String {
        match self.mode {
            CronMode::Paste => self
                .entries
                .iter()
                .map(|entry| {
                    cron_job::clipboard_row(&[
                        &entry.schedule(),
                        &entry.command,
                        &self.next_run(entry),
                        &super::crontab_tool::describe(&entry.description()),
                    ])
                })
                .collect(),
            CronMode::Tasks => cron_job::tasks_clipboard_text(&self.tasks),
        }
    }

    /// What the status line says.
    fn status_text(&self) -> Option<String> {
        match self.status {
            Status::None => None,
            Status::Parsed(count) => Some(fl!("ui-tool-cronjob-status-parsed", count = count)),
            Status::Tasks(count) => Some(fl!("ui-tool-cronjob-status-tasks", count = count)),
        }
    }

    /// The tool's page, as the C# `CronJobManagerView.xaml`: the two modes, then the list
    /// of the one shown, the status and the copy button.
    pub fn view<'a>(&'a self, tab: TabId, state: &ToolPane) -> Element<'a, Message> {
        let send = move |message| Message::Tool(tab, ToolMessage::CronJob(message));
        let mode = |label: String, mode: CronMode| {
            button(text(label).size(font_size::BODY))
                .padding(super::COPY_PADDING)
                .style(if self.mode == mode {
                    styles::primary
                } else {
                    styles::secondary
                })
                .on_press(send(CronJobMessage::Mode(mode)))
        };
        let can_copy = match self.mode {
            CronMode::Paste => !self.entries.is_empty(),
            CronMode::Tasks => !self.tasks.is_empty(),
        };
        let header = row![
            mode(fl!("ui-tool-cronjob-tab-paste"), CronMode::Paste),
            mode(fl!("ui-tool-cronjob-tab-tasks"), CronMode::Tasks),
            iced::widget::space().width(Length::Fill),
        ]
        .push(can_copy.then(|| {
            super::copy_button(
                fl!("ui-tool-cronjob-copy"),
                state.copied(CopySlot::CronJobExport),
                send(CronJobMessage::Copy),
                super::COPY_PADDING,
            )
        }))
        .spacing(spacing::SM)
        .align_y(iced::Alignment::Center);
        let body = match self.mode {
            CronMode::Paste => self.paste_view(tab),
            CronMode::Tasks => self.tasks_view(tab),
        };
        super::tool_body(
            column![header, body]
                .push(super::status_line(
                    self.status_text().map(|status| (status, false)),
                ))
                .spacing(spacing::SM),
        )
    }

    /// The crontab box, its list and the selected schedule's details.
    fn paste_view(&self, tab: TabId) -> Element<'_, Message> {
        let send = move |message| Message::Tool(tab, ToolMessage::CronJob(message));
        let input = super::text_box(&self.input, Some(fl!("ui-tool-cronjob-paste-placeholder")))
            .height(PASTE_HEIGHT)
            .on_action(move |action| send(CronJobMessage::Input(action)));
        let buttons = row![
            super::action_button(
                fl!("ui-tool-cronjob-parse"),
                true,
                Some(send(CronJobMessage::Parse))
            ),
            super::action_button(
                fl!("ui-tool-cronjob-clear"),
                false,
                Some(send(CronJobMessage::Clear))
            ),
        ]
        .spacing(spacing::SM);
        let error = self.paste_error.map(|error| match error {
            PasteError::Empty => fl!("ui-tool-cronjob-error-empty"),
            PasteError::NoEntry => fl!("ui-tool-cronjob-error-no-entry"),
        });
        let list: Element<'_, Message> = if self.entries.is_empty() {
            super::empty_state(fl!("ui-tool-cronjob-empty-state"))
        } else {
            let heading = table_row(
                &[
                    fl!("ui-tool-cronjob-col-schedule"),
                    fl!("ui-tool-cronjob-col-command"),
                    fl!("ui-tool-cronjob-col-next-run"),
                    fl!("ui-tool-cronjob-col-description"),
                ],
                &CRON_COLUMNS,
                true,
            );
            let rows = self.entries.iter().enumerate().map(|(index, entry)| {
                button(table_row(
                    &[
                        entry.schedule(),
                        entry.command.clone(),
                        self.next_run(entry),
                        super::crontab_tool::describe(&entry.description()),
                    ],
                    &CRON_COLUMNS,
                    false,
                ))
                .padding(0)
                .width(Length::Fill)
                .style(if self.selected == Some(index) {
                    styles::primary
                } else {
                    styles::subtle
                })
                .on_press(send(CronJobMessage::Select(index)))
                .into()
            });
            column![
                heading,
                scrollable(Column::with_children(rows).spacing(spacing::XS)).height(Length::Fill)
            ]
            .push(self.detail())
            .spacing(spacing::XS)
            .height(Length::Fill)
            .into()
        };
        column![
            text(fl!("ui-tool-cronjob-paste-instructions"))
                .size(font_size::BODY)
                .style(text::secondary),
            input,
            buttons
        ]
        .push(error.map(super::error_line))
        .push(list)
        .spacing(spacing::SM)
        .height(Length::Fill)
        .into()
    }

    /// The selected schedule, its command and its next runs, as the C# detail panel.
    fn detail(&self) -> Option<Element<'_, Message>> {
        let entry = self.entries.get(self.selected?)?;
        let runs = self.runs(entry, NEXT_RUNS_COUNT);
        let runs = if runs.is_empty() {
            NO_RUN.to_owned()
        } else {
            runs.join("\n")
        };
        let schedule = entry.schedule();
        Some(
            container(
                column![
                    text(fl!("ui-tool-cronjob-detail-header"))
                        .size(font_size::BODY)
                        .font(styles::SEMIBOLD),
                    text(fl!(
                        "ui-tool-cronjob-detail-schedule",
                        schedule = schedule.as_str()
                    ))
                    .size(font_size::BODY),
                    text(fl!(
                        "ui-tool-cronjob-detail-command",
                        command = entry.command.as_str()
                    ))
                    .size(font_size::BODY),
                    text(fl!("ui-tool-cronjob-detail-next-runs"))
                        .size(font_size::BODY)
                        .style(text::secondary),
                    text(runs).font(super::BOX_FONT).size(font_size::BODY),
                ]
                .spacing(spacing::XS),
            )
            .padding(spacing::MD)
            .width(Length::Fill)
            .style(styles::card)
            .into(),
        )
    }

    /// The Refresh button and the tasks.
    fn tasks_view(&self, tab: TabId) -> Element<'_, Message> {
        let send = move |message| Message::Tool(tab, ToolMessage::CronJob(message));
        let refresh = row![super::action_button(
            fl!("ui-tool-cronjob-refresh"),
            true,
            (!self.loading).then(|| send(CronJobMessage::Refresh))
        )]
        .push(self.loading.then(|| {
            text(fl!("ui-tool-cronjob-loading"))
                .size(font_size::BODY)
                .style(text::secondary)
        }))
        .spacing(spacing::SM)
        .align_y(iced::Alignment::Center);
        let error = self.tasks_error.as_ref().map(|error| match error {
            TasksError::Unsupported => fl!("ui-tool-cronjob-error-unsupported"),
            TasksError::NoSystemFolder => fl!("ui-tool-cronjob-error-no-system-folder"),
            TasksError::TimedOut => fl!("ui-tool-cronjob-error-timeout"),
            TasksError::Failed(reason) => {
                fl!("ui-tool-cronjob-error-failed", error = reason.as_str())
            }
        });
        let list: Element<'_, Message> = if self.tasks.is_empty() {
            super::empty_state(fl!("ui-tool-cronjob-tasks-empty"))
        } else {
            let heading = table_row(
                &[
                    fl!("ui-tool-cronjob-col-task-name"),
                    fl!("ui-tool-cronjob-col-status"),
                    fl!("ui-tool-cronjob-col-next-run"),
                    fl!("ui-tool-cronjob-col-last-run"),
                    fl!("ui-tool-cronjob-col-last-result"),
                ],
                &TASK_COLUMNS,
                true,
            );
            let rows = self.tasks.iter().map(|task| {
                table_row(
                    &[
                        task.name.clone(),
                        task.status.clone(),
                        task.next_run.clone(),
                        task.last_run.clone(),
                        task.last_result.clone(),
                    ],
                    &TASK_COLUMNS,
                    false,
                )
            });
            column![
                heading,
                scrollable(Column::with_children(rows).spacing(spacing::XS)).height(Length::Fill)
            ]
            .spacing(spacing::XS)
            .height(Length::Fill)
            .into()
        };
        column![refresh]
            .push(error.map(super::error_line))
            .push(list)
            .spacing(spacing::SM)
            .height(Length::Fill)
            .into()
    }
}

/// A row of a list: its cells in columns `portions` wide; the heading in semibold.
fn table_row<'a>(cells: &[String], portions: &[u16], heading: bool) -> Element<'a, Message> {
    let cells = cells.iter().zip(portions).map(|(cell, portion)| {
        let cell = text(cell.clone()).size(font_size::BODY);
        let cell = if heading {
            cell.font(styles::SEMIBOLD).style(text::secondary)
        } else {
            cell
        };
        container(cell)
            .width(Length::FillPortion(*portion))
            .padding([spacing::XS, spacing::SM])
            .into()
    });
    iced::widget::Row::with_children(cells)
        .width(Length::Fill)
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn thursday_noon() -> WallClock {
        // 2026-10-15 12:07 UTC, a Thursday.
        Instant::from_unix_seconds(1_792_066_020).wall_clock(0)
    }

    fn pasted(pane: &mut CronJobPane, crontab: &str) {
        pane.update(CronJobMessage::Input(Action::Edit(
            iced::widget::text_editor::Edit::Paste(std::sync::Arc::new(crontab.to_owned())),
        )));
        pane.update(CronJobMessage::Parse);
    }

    #[test]
    fn a_pasted_crontab_lists_its_schedules_with_real_next_runs() {
        let mut pane = CronJobPane::with_clock(thursday_noon);
        pasted(
            &mut pane,
            "MAILTO=root\n*/5 * * * * /bin/check\n0 0 * * 7 /bin/weekly\n",
        );
        assert_eq!(pane.entries.len(), 2);
        assert_eq!(pane.status, Status::Parsed(2));
        assert_eq!(pane.next_run(&pane.entries[0]), "2026-10-15 12:10 (Thu)");
        assert_eq!(pane.next_run(&pane.entries[1]), "2026-10-18 00:00 (Sun)");
        pane.update(CronJobMessage::Select(1));
        assert_eq!(pane.selected, Some(1));
        assert_eq!(
            pane.runs(&pane.entries[1], NEXT_RUNS_COUNT).len(),
            NEXT_RUNS_COUNT
        );
        let copied = pane.clipboard_text();
        assert!(copied.starts_with("*/5 * * * *\t/bin/check\t2026-10-15 12:10 (Thu)\t"));
        assert_eq!(copied.lines().count(), 2);
    }

    #[test]
    fn an_empty_or_scheduleless_paste_is_said() {
        let mut pane = CronJobPane::with_clock(thursday_noon);
        pane.update(CronJobMessage::Parse);
        assert_eq!(pane.paste_error, Some(PasteError::Empty));
        pasted(&mut pane, "# only a comment\nPATH=/bin\n");
        assert_eq!(pane.paste_error, Some(PasteError::NoEntry));
        pane.update(CronJobMessage::Clear);
        assert_eq!(pane.paste_error, None);
        assert_eq!(pane.status, Status::None);
    }

    #[test]
    fn tasks_load_once_at_a_time_and_their_errors_are_kept() {
        let mut pane = CronJobPane::with_clock(thursday_noon);
        assert!(matches!(
            pane.update(CronJobMessage::Refresh),
            Outcome::Load
        ));
        assert!(matches!(
            pane.update(CronJobMessage::Refresh),
            Outcome::Done
        ));
        pane.update(CronJobMessage::Loaded(Err(TasksError::TimedOut)));
        assert_eq!(pane.tasks_error, Some(TasksError::TimedOut));
        assert!(matches!(
            pane.update(CronJobMessage::Refresh),
            Outcome::Load
        ));
        assert_eq!(pane.tasks_error, None);
        pane.update(CronJobMessage::Loaded(Ok(vec![ScheduledTask {
            name: "\\Backup".to_owned(),
            status: "Ready".to_owned(),
            next_run: "N/A".to_owned(),
            last_run: "N/A".to_owned(),
            last_result: "0".to_owned(),
        }])));
        assert_eq!(pane.status, Status::Tasks(1));
        pane.update(CronJobMessage::Mode(CronMode::Tasks));
        assert_eq!(pane.clipboard_text(), "\\Backup\tReady\tN/A\tN/A\t0\n");
    }
}
