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

//! The Scheduled page, as the C# `ScheduledTasksViewModel`: the tasks listed, one selected,
//! added and edited in a dialog, deleted after a question, and turned on or off from the
//! list, saved at once. A session a schedule never opens is not offered.

use heimdall_core::profile::ProfileId;
use heimdall_core::scheduled::{self, ScheduledTask};

use super::scheduled::ScheduledNotice;
use super::tree::ProfileKind;
use super::{App, Dialog, Notice};
use crate::scheduled_form::{Fields, ScheduledForm};

/// What the Scheduled page and its dialog do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScheduledMessage {
    /// Opens the dialog for a new task.
    Add,
    /// Opens the dialog for the task selected.
    EditSelected,
    /// Opens the dialog for this task, as a double click on its row.
    Edit(String),
    /// Asks before deleting the task selected.
    DeleteSelected,
    /// Selects this task's row.
    Select(String),
    /// Turns this task on or off, saved at once.
    Toggle(String, bool),
    /// A field of the dialog changed.
    Field(Fields),
}

/// A session the dialog offers: "Name (Type)", as the C# `ServerOption`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduledChoice {
    /// The profile.
    pub id: ProfileId,
    /// Its name.
    pub name: String,
    /// Its protocol.
    pub kind: ProfileKind,
}

/// `unix` on this computer's clock, as the C# grid writes it: `yyyy-MM-dd HH:mm`.
#[must_use]
pub fn local_time_text(unix: i64) -> String {
    use heimdall_core::scheduled::LocalClock as _;
    let offset = crate::time_zone::SystemClock.offset_at(unix);
    let wall = heimdall_core::tools::date_time::Instant::from_unix_seconds(unix).wall_clock(offset);
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}",
        wall.year, wall.month, wall.day, wall.hour, wall.minute
    )
}

impl App {
    /// The task selected on the Scheduled page.
    #[must_use]
    pub fn selected_scheduled(&self) -> Option<&str> {
        self.scheduler.selected.as_deref()
    }

    /// The sessions the dialog offers: every profile a schedule may open, in the tree's
    /// order.
    #[must_use]
    pub fn scheduled_choices(&self) -> Vec<ScheduledChoice> {
        self.profile_summaries()
            .into_iter()
            .filter(|profile| self.scheduled_refusal(&profile.id, profile.kind).is_none())
            .map(|profile| ScheduledChoice {
                id: profile.id,
                name: profile.name,
                kind: profile.kind,
            })
            .collect()
    }

    /// Applies a message of the Scheduled page or its dialog.
    pub(super) fn scheduled_message(&mut self, message: ScheduledMessage) {
        match message {
            ScheduledMessage::Add => {
                if self.dialog.is_none() {
                    self.dialog = Some(Dialog::ScheduledTask(Box::default()));
                }
            }
            ScheduledMessage::EditSelected => {
                if let Some(id) = self.scheduler.selected.clone() {
                    self.edit_scheduled(&id);
                }
            }
            ScheduledMessage::Edit(id) => {
                self.scheduler.selected = Some(id.clone());
                self.edit_scheduled(&id);
            }
            ScheduledMessage::DeleteSelected => {
                let task = self
                    .scheduler
                    .selected
                    .as_deref()
                    .and_then(|id| self.scheduler.tasks.get(id));
                if let Some(task) = task
                    && self.dialog.is_none()
                {
                    self.dialog = Some(Dialog::ConfirmDeleteScheduled {
                        id: task.id.clone(),
                        name: task.server_name.clone(),
                    });
                }
            }
            ScheduledMessage::Select(id) => {
                if self.scheduler.tasks.get(&id).is_some() {
                    self.scheduler.selected = Some(id);
                }
            }
            ScheduledMessage::Toggle(id, enabled) => self.toggle_scheduled(&id, enabled),
            ScheduledMessage::Field(fields) => {
                if let Some(Dialog::ScheduledTask(form)) = self.dialog.as_mut() {
                    form.change(|now| *now = fields);
                }
            }
        }
    }

    /// Opens the dialog for task `id`, its session chosen when a schedule may still open it.
    fn edit_scheduled(&mut self, id: &str) {
        if self.dialog.is_some() {
            return;
        }
        let Some(task) = self.scheduler.tasks.get(id).cloned() else {
            return;
        };
        let server = task
            .server_id
            .clone()
            .filter(|server| self.scheduled_choices().iter().any(|c| &c.id == server));
        self.dialog = Some(Dialog::ScheduledTask(Box::new(ScheduledForm::edit(
            &task, server,
        ))));
    }

    /// Turns task `id` on or off and writes it at once, as the C# list should have; turned
    /// on, it is timed from now and its failures forgotten.
    fn toggle_scheduled(&mut self, id: &str, enabled: bool) {
        let now = crate::time_zone::unix_now();
        let Some(task) = self
            .scheduler
            .tasks
            .tasks_mut()
            .iter_mut()
            .find(|task| task.id == id)
        else {
            return;
        };
        if task.enabled == enabled {
            return;
        }
        task.enabled = enabled;
        if enabled {
            task.failures = 0;
            task.next_run = Some(scheduled::next_run(
                &task.schedule,
                now,
                &crate::time_zone::SystemClock,
            ));
        }
        self.save_scheduled();
    }

    /// "Save": the dialog's task written, new or in place of the one edited, timed from
    /// now; refused, what is wrong is said and the dialog stays.
    pub(super) fn save_scheduled_form(&mut self, mut form: ScheduledForm) {
        let (server, schedule) = match form.check() {
            Ok(checked) => checked,
            Err(problem) => {
                form.problem = Some(problem);
                self.dialog = Some(Dialog::ScheduledTask(Box::new(form)));
                return;
            }
        };
        let Some(choice) = self
            .scheduled_choices()
            .into_iter()
            .find(|choice| choice.id == server)
        else {
            form.problem = Some(crate::scheduled_form::ScheduledProblem::ServerRequired);
            self.dialog = Some(Dialog::ScheduledTask(Box::new(form)));
            return;
        };
        let held = form
            .editing
            .as_deref()
            .and_then(|id| self.scheduler.tasks.get(id))
            .cloned();
        let id = match (&held, scheduled::new_task_id()) {
            (Some(task), _) => task.id.clone(),
            (None, Ok(id)) => id,
            (None, Err(error)) => {
                log::warn!("no identifier for a new scheduled task: {error}");
                self.dialog = Some(Dialog::ScheduledTask(Box::new(form)));
                return;
            }
        };
        let now = crate::time_zone::unix_now();
        let next_run = scheduled::next_run(&schedule, now, &crate::time_zone::SystemClock);
        let task = ScheduledTask {
            id,
            server_id: Some(choice.id),
            server_name: choice.name.clone(),
            connection_type: choice.kind.label().to_owned(),
            enabled: form.fields.enabled,
            schedule,
            last_run: held.as_ref().and_then(|task| task.last_run),
            next_run: Some(next_run),
            last_outcome: held.as_ref().and_then(|task| task.last_outcome),
            failures: 0,
        };
        let notice = if held.is_some() {
            ScheduledNotice::Updated(choice.name)
        } else {
            ScheduledNotice::Added(choice.name)
        };
        self.scheduler.selected = Some(task.id.clone());
        if let Err(error) = self.scheduler.tasks.upsert(task) {
            log::warn!("scheduled connections not saved: {error}");
        }
        self.tell(Notice::Scheduled(notice));
    }

    /// The deletion asked about answered: task `id` goes, and is said.
    pub(super) fn delete_scheduled(&mut self, id: &str, name: String) {
        if self.scheduler.selected.as_deref() == Some(id) {
            self.scheduler.selected = None;
        }
        match self.scheduler.tasks.remove(id) {
            Ok(true) => self.tell(Notice::Scheduled(ScheduledNotice::Deleted(name))),
            Ok(false) => {}
            Err(error) => log::warn!("scheduled connections not saved: {error}"),
        }
    }

    /// Esc or Cancel on the dialog: closed when nothing was changed, else asked first, as
    /// the C# unsaved-changes guard; on that question, back to the dialog. `None` for
    /// another dialog.
    pub(super) fn dismiss_scheduled(&mut self) -> Option<Vec<super::Effect>> {
        match self.dialog.take() {
            Some(Dialog::ScheduledTask(form)) if form.dirty() => {
                self.dialog = Some(Dialog::ConfirmDiscardScheduled(form));
                Some(Vec::new())
            }
            Some(Dialog::ConfirmDiscardScheduled(form)) => {
                self.dialog = Some(Dialog::ScheduledTask(form));
                Some(Vec::new())
            }
            other => {
                self.dialog = other;
                None
            }
        }
    }
}
