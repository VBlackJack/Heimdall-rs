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

//! The dialog adding or editing a scheduled connection, as the C#
//! `ScheduledTaskDialogViewModel`: the session, daily at a time or every so many minutes,
//! enabled, when it would run next, what is wrong, and whether anything was changed.

use heimdall_core::profile::ProfileId;
use heimdall_core::scheduled::{
    self, DEFAULT_INTERVAL_MINUTES, DEFAULT_TIME_OF_DAY, LocalClock, Schedule, ScheduledTask,
    TimeOfDay,
};

/// Longest text the time and interval fields take, as the C# `MaxLength`.
pub const FIELD_MAX_CHARS: usize = 5;

/// Seconds in a day.
const DAY_SECONDS: i64 = 86_400;

/// How a task repeats, as the C# `ScheduleType` list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScheduleKind {
    /// Every day at a time of day.
    Daily,
    /// Every so many minutes.
    Interval,
}

impl ScheduleKind {
    /// Both, in the C# list's order.
    pub const ALL: [Self; 2] = [Self::Daily, Self::Interval];
}

/// What stops the dialog from saving, as the C# validation says it, one at a time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScheduledProblem {
    /// No session chosen.
    ServerRequired,
    /// The time is no `HH:mm` time of day.
    TimeInvalid,
    /// The interval is no positive number of minutes.
    IntervalInvalid,
}

/// When a task filled as the dialog is would run next, as the C# preview says it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NextRunPreview {
    /// Today or tomorrow at this time.
    Daily {
        /// Today rather than tomorrow.
        today: bool,
        /// The time.
        time: TimeOfDay,
    },
    /// In so many minutes, then every so many.
    Interval(u32),
}

/// The fields of the dialog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fields {
    /// The session chosen.
    pub server: Option<ProfileId>,
    /// Daily or every so many minutes.
    pub kind: ScheduleKind,
    /// The time of day, as typed.
    pub time: String,
    /// The interval in minutes, as typed.
    pub minutes: String,
    /// Whether the task runs.
    pub enabled: bool,
}

/// The dialog's state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduledForm {
    /// The task edited; `None` for a new one.
    pub editing: Option<String>,
    /// The fields as they are.
    pub fields: Fields,
    /// What was refused at the last save, checked again as the fields change, as the C#
    /// re-validates once an error is shown; `None` until a save is refused.
    pub problem: Option<ScheduledProblem>,
    /// The fields as the dialog opened with them.
    initial: Fields,
}

impl ScheduledForm {
    /// A new task's dialog, with the C# defaults: daily at 08:00, every 30 minutes, enabled.
    #[must_use]
    pub fn new() -> Self {
        let fields = Fields {
            server: None,
            kind: ScheduleKind::Daily,
            time: DEFAULT_TIME_OF_DAY.to_owned(),
            minutes: DEFAULT_INTERVAL_MINUTES.to_string(),
            enabled: true,
        };
        Self {
            editing: None,
            initial: fields.clone(),
            fields,
            problem: None,
        }
    }

    /// The dialog editing `task`, its session `server` when it may still be chosen.
    #[must_use]
    pub fn edit(task: &ScheduledTask, server: Option<ProfileId>) -> Self {
        let mut form = Self::new();
        form.editing = Some(task.id.clone());
        form.fields.server = server;
        form.fields.enabled = task.enabled;
        match &task.schedule {
            Schedule::Daily { time } => {
                form.fields.kind = ScheduleKind::Daily;
                form.fields.time.clone_from(time);
            }
            Schedule::Interval { minutes } => {
                form.fields.kind = ScheduleKind::Interval;
                form.fields.minutes = task
                    .schedule
                    .interval_minutes()
                    .unwrap_or(*minutes)
                    .to_string();
            }
        }
        form.initial = form.fields.clone();
        form
    }

    /// Whether a field was changed since the dialog opened, as the C# `IsDirty`.
    #[must_use]
    pub fn dirty(&self) -> bool {
        self.fields != self.initial
    }

    /// The fields changed by `change`; checked again when a save was refused.
    pub fn change(&mut self, change: impl FnOnce(&mut Fields)) {
        change(&mut self.fields);
        self.fields.time.truncate(char_boundary(&self.fields.time));
        self.fields
            .minutes
            .truncate(char_boundary(&self.fields.minutes));
        if self.problem.is_some() {
            self.problem = self.check().err();
        }
    }

    /// The session and the schedule the fields say.
    ///
    /// # Errors
    ///
    /// [`ScheduledProblem`]: the first thing wrong, as the C# checks them.
    pub fn check(&self) -> Result<(ProfileId, Schedule), ScheduledProblem> {
        let server = self
            .fields
            .server
            .clone()
            .ok_or(ScheduledProblem::ServerRequired)?;
        let schedule = match self.fields.kind {
            ScheduleKind::Daily => Schedule::Daily {
                time: TimeOfDay::parse(&self.fields.time)
                    .ok_or(ScheduledProblem::TimeInvalid)?
                    .text(),
            },
            ScheduleKind::Interval => Schedule::Interval {
                minutes: self
                    .fields
                    .minutes
                    .trim()
                    .parse::<u32>()
                    .ok()
                    .filter(|minutes| *minutes > 0)
                    .ok_or(ScheduledProblem::IntervalInvalid)?,
            },
        };
        Ok((server, schedule))
    }

    /// When the task would run next at `now` on `clock`, as the C# preview; `None` while
    /// the time or the interval does not read.
    #[must_use]
    pub fn preview(&self, now: i64, clock: &dyn LocalClock) -> Option<NextRunPreview> {
        match self.fields.kind {
            ScheduleKind::Daily => {
                let time = TimeOfDay::parse(&self.fields.time)?;
                let schedule = Schedule::Daily { time: time.text() };
                let next = scheduled::next_run(&schedule, now, clock);
                let day = |at: i64| (at + i64::from(clock.offset_at(at))).div_euclid(DAY_SECONDS);
                Some(NextRunPreview::Daily {
                    today: day(next) == day(now),
                    time,
                })
            }
            ScheduleKind::Interval => self
                .fields
                .minutes
                .trim()
                .parse::<u32>()
                .ok()
                .filter(|minutes| *minutes > 0)
                .map(NextRunPreview::Interval),
        }
    }
}

impl Default for ScheduledForm {
    fn default() -> Self {
        Self::new()
    }
}

/// Where `text` is cut to [`FIELD_MAX_CHARS`] characters.
fn char_boundary(text: &str) -> usize {
    text.char_indices()
        .nth(FIELD_MAX_CHARS)
        .map_or(text.len(), |(at, _)| at)
}

#[cfg(test)]
mod tests {
    use heimdall_core::scheduled::FixedOffset;

    use super::*;

    const UTC: FixedOffset = FixedOffset(0);

    /// 2026-10-13 10:00 UTC.
    const NOW: i64 = 1_791_885_600;

    fn chosen() -> ScheduledForm {
        let mut form = ScheduledForm::new();
        form.change(|fields| fields.server = Some(ProfileId::new("web")));
        form
    }

    #[test]
    fn a_new_task_starts_with_the_csharp_defaults() {
        let form = ScheduledForm::new();
        assert_eq!(form.fields.kind, ScheduleKind::Daily);
        assert_eq!(form.fields.time, "08:00");
        assert_eq!(form.fields.minutes, "30");
        assert!(form.fields.enabled);
        assert!(!form.dirty());
        assert_eq!(form.check(), Err(ScheduledProblem::ServerRequired));
    }

    #[test]
    fn each_field_is_checked_as_the_csharp_checks_it() {
        let mut form = chosen();
        assert_eq!(
            form.check(),
            Ok((
                ProfileId::new("web"),
                Schedule::Daily {
                    time: "08:00".to_owned()
                }
            ))
        );
        form.change(|fields| fields.time = "24:00".to_owned());
        assert_eq!(form.check(), Err(ScheduledProblem::TimeInvalid));
        form.change(|fields| fields.time = "7:5".to_owned());
        assert_eq!(form.check(), Err(ScheduledProblem::TimeInvalid));
        form.change(|fields| fields.time = "7:05".to_owned());
        assert_eq!(
            form.check().map(|(_, schedule)| schedule),
            Ok(Schedule::Daily {
                time: "07:05".to_owned()
            }),
            "written as HH:mm"
        );
        form.change(|fields| fields.kind = ScheduleKind::Interval);
        for refused in ["0", "-5", "abc", ""] {
            form.change(|fields| fields.minutes = refused.to_owned());
            assert_eq!(
                form.check(),
                Err(ScheduledProblem::IntervalInvalid),
                "{refused}"
            );
        }
        form.change(|fields| fields.minutes = "123456".to_owned());
        assert_eq!(form.fields.minutes, "12345", "five characters at most");
        assert_eq!(
            form.check().map(|(_, schedule)| schedule),
            Ok(Schedule::Interval { minutes: 12_345 })
        );
    }

    #[test]
    fn a_refused_save_is_checked_again_as_the_fields_change() {
        let mut form = ScheduledForm::new();
        form.problem = form.check().err();
        assert_eq!(form.problem, Some(ScheduledProblem::ServerRequired));
        form.change(|fields| fields.server = Some(ProfileId::new("web")));
        assert_eq!(form.problem, None, "fixed: no longer said");
        form.problem = None;
        form.change(|fields| fields.time = "nope".to_owned());
        assert_eq!(form.problem, None, "not said before a save is refused");
    }

    #[test]
    fn a_change_marks_the_dialog_and_undoing_it_unmarks_it() {
        let mut form = ScheduledForm::new();
        form.change(|fields| fields.enabled = false);
        assert!(form.dirty());
        form.change(|fields| fields.enabled = true);
        assert!(!form.dirty());
    }

    #[test]
    fn an_edited_task_opens_with_its_own_fields_unchanged() {
        let task = ScheduledTask {
            id: "t".to_owned(),
            server_id: Some(ProfileId::new("web")),
            server_name: "web".to_owned(),
            connection_type: "SSH".to_owned(),
            enabled: false,
            schedule: Schedule::Interval { minutes: 0 },
            last_run: None,
            next_run: None,
            last_outcome: None,
            failures: 0,
        };
        let form = ScheduledForm::edit(&task, Some(ProfileId::new("web")));
        assert_eq!(form.editing.as_deref(), Some("t"));
        assert_eq!(form.fields.kind, ScheduleKind::Interval);
        assert_eq!(form.fields.minutes, "60", "the interval it runs at");
        assert!(!form.fields.enabled);
        assert!(!form.dirty());
    }

    #[test]
    fn the_preview_says_today_or_tomorrow_and_the_interval() {
        let mut form = chosen();
        form.change(|fields| fields.time = "11:00".to_owned());
        assert_eq!(
            form.preview(NOW, &UTC),
            Some(NextRunPreview::Daily {
                today: true,
                time: TimeOfDay {
                    hour: 11,
                    minute: 0
                }
            })
        );
        form.change(|fields| fields.time = "10:00".to_owned());
        assert!(matches!(
            form.preview(NOW, &UTC),
            Some(NextRunPreview::Daily { today: false, .. })
        ));
        form.change(|fields| fields.time = "nope".to_owned());
        assert_eq!(form.preview(NOW, &UTC), None);
        form.change(|fields| {
            fields.kind = ScheduleKind::Interval;
            fields.minutes = "15".to_owned();
        });
        assert_eq!(form.preview(NOW, &UTC), Some(NextRunPreview::Interval(15)));
    }
}
