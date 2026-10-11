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

//! Scheduled connections fired while Heimdall runs, as the C# `TaskSchedulerService`: a
//! look every minute, a round never overlapping another, a task due opened through the same
//! connection as any saved profile. Nothing is registered with the system: a task whose time
//! came while Heimdall was closed is missed, and timed again from the start, never run in a
//! burst.
//!
//! Unattended, a task never asks anything: it uses what is saved for its profile only. A
//! password not saved, a Windows Hello or Credential Guard check that would ask, a server
//! key or certificate not trusted yet: it fails, said in the status bar and the log, and
//! recorded as its last outcome. A task due while the workspace is locked waits for the
//! unlock, as an auto-reconnect does, then runs once. A local shell or program is never
//! opened by a schedule.

use std::path::Path;
use std::time::Duration;

use heimdall_core::profile::ProfileId;
use heimdall_core::scheduled::{
    self, FailureKind, LocalClock, Outcome, ScheduledTask, ScheduledTasks, Unresolved,
};

use super::reconnect::open_message;
use super::tree::ProfileKind;
use super::{App, Effect, Notice, Phase};
use crate::driver::Purpose;
use crate::error::UiError;
use crate::event::ConnectionEvent;
use crate::ids::TabId;

/// How often the tasks are looked at, as the C# `TickIntervalMs`.
pub const SCHEDULED_TICK: Duration = Duration::from_secs(60);

/// What the status bar says of a scheduled connection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScheduledNotice {
    /// A task is opening the profile of this name.
    Triggered(String),
    /// A task for the profile of this name did not open it.
    Failed {
        /// The profile's name, as the task recorded it.
        name: String,
        /// Why.
        failure: FailureKind,
    },
    /// A task names a local shell or program, which a schedule never opens.
    Refused(String),
    /// A task waits for the workspace to be unlocked.
    Deferred(String),
}

/// The scheduled tasks and what is running of them.
#[derive(Debug, Default)]
pub(super) struct Scheduler {
    /// The tasks, and their file.
    tasks: ScheduledTasks,
    /// Why the file was not read, when it was there and was not: the tasks are then kept
    /// for the run only, and the migration does not add to them.
    unread: Option<String>,
    /// The tabs a task opened that have not connected or failed yet, with the task.
    unattended: Vec<(TabId, String)>,
    /// The tasks due and not run yet: while the workspace was locked, or a dialog was open.
    held: Vec<String>,
    /// A round is being made: another one is skipped, as the C# tick guard.
    ticking: bool,
    /// What to say once the event being applied has changed what the tab shows.
    pending: Option<ScheduledNotice>,
}

impl Scheduler {
    /// The tasks kept beside `profiles_file`; none, said in the log, when they cannot be
    /// read.
    pub(super) fn open(profiles_file: &Path) -> Self {
        let (tasks, unread) = ScheduledTasks::open(&scheduled::scheduled_path(profiles_file));
        if let Some(error) = &unread {
            log::warn!("scheduled connections not read: {error}");
        }
        Self {
            tasks,
            unread: unread.map(|error| error.to_string()),
            ..Self::default()
        }
    }
}

/// Why a task matched no profile, for the log, as the C# says it.
fn diagnosis(unresolved: Unresolved) -> &'static str {
    match unresolved {
        Unresolved::IdentifierNotFound => {
            "no profile carries that identifier: it was deleted or re-identified, and \
             another profile of the same name is not assumed to be its replacement"
        }
        Unresolved::NameAmbiguous => {
            "this task records no profile identifier and several profiles carry that name, \
             so which one it means cannot be established"
        }
        Unresolved::NameNotFound => {
            "this task records no profile identifier and no profile carries that name"
        }
    }
}

impl App {
    /// The scheduled connections, in the order they were added.
    #[must_use]
    pub fn scheduled_tasks(&self) -> &[ScheduledTask] {
        self.scheduler.tasks.tasks()
    }

    /// Whether the scheduled connections need their minute tick: one is enabled, or one
    /// is held.
    #[must_use]
    pub fn schedules_connections(&self) -> bool {
        !self.scheduler.held.is_empty() || self.scheduled_tasks().iter().any(|task| task.enabled)
    }

    /// The tasks whose time came while Heimdall was closed, at the start: skipped, never
    /// run in a burst, each timed again from `now` and shown as missed. A task never timed
    /// is timed.
    pub(super) fn skip_missed(&mut self, now: i64, clock: &dyn LocalClock) {
        let mut changed = false;
        for task in self.scheduler.tasks.tasks_mut() {
            match task.next_run {
                Some(next) if next > now => continue,
                Some(_) if task.enabled => {
                    log::info!(
                        "scheduled task '{}' (id={}) was due while Heimdall was closed: missed",
                        task.server_name,
                        task.id
                    );
                    task.last_outcome = Some(Outcome::Missed);
                }
                _ => {}
            }
            task.next_run = Some(scheduled::next_run(&task.schedule, now, clock));
            changed = true;
        }
        if changed {
            self.save_scheduled();
        }
    }

    /// The minute's look at the tasks at `now` on `clock`: each enabled task due is timed
    /// again and run, after those held; a round already being made is not made again.
    pub fn scheduled_tick_at(&mut self, now: i64, clock: &dyn LocalClock) -> Vec<Effect> {
        if std::mem::replace(&mut self.scheduler.ticking, true) {
            return Vec::new();
        }
        let effects = self.scheduled_round(now, clock);
        self.scheduler.ticking = false;
        effects
    }

    fn scheduled_round(&mut self, now: i64, clock: &dyn LocalClock) -> Vec<Effect> {
        let open: Vec<TabId> = self.tabs.iter().map(|tab| tab.id).collect();
        self.scheduler
            .unattended
            .retain(|(tab, _)| open.contains(tab));
        // Those held run first, once nothing holds them any longer.
        let mut due = if self.is_locked() || self.dialog.is_some() {
            Vec::new()
        } else {
            std::mem::take(&mut self.scheduler.held)
        };
        let mut changed = false;
        for task in self.scheduler.tasks.tasks_mut() {
            if !task.enabled {
                continue;
            }
            match task.next_run {
                Some(next) if now < next => continue,
                Some(next) => {
                    log::info!(
                        "scheduled task '{}' (id={}) is due at {next}",
                        task.server_name,
                        task.id
                    );
                    if !due.contains(&task.id) {
                        due.push(task.id.clone());
                    }
                }
                None => {}
            }
            task.next_run = Some(scheduled::next_run(&task.schedule, now, clock));
            changed = true;
        }
        if changed {
            self.save_scheduled();
        }
        let mut effects = Vec::new();
        for id in due {
            effects.extend(self.run_scheduled(&id, now));
        }
        effects
    }

    /// The tasks held while the workspace was locked, run once it is unlocked.
    pub(super) fn resume_scheduled(&mut self) -> Vec<Effect> {
        let held = std::mem::take(&mut self.scheduler.held);
        let now = crate::time_zone::unix_now();
        held.into_iter()
            .flat_map(|id| self.run_scheduled(&id, now))
            .collect()
    }

    /// Runs task `id` at `now`: held while locked or while a dialog is open, refused for a
    /// local profile, failed when its profile is not found or a check would ask; else its
    /// profile opens as any saved profile does, unattended.
    fn run_scheduled(&mut self, id: &str, now: i64) -> Vec<Effect> {
        let Some(task) = self
            .scheduler
            .tasks
            .get(id)
            .filter(|task| task.enabled)
            .cloned()
        else {
            return Vec::new();
        };
        if self.is_locked() || self.dialog.is_some() {
            if !self.scheduler.held.iter().any(|held| held == id) {
                self.scheduler.held.push(task.id.clone());
                // Locked: said, and recorded; a dialog open: it simply waits for it.
                if self.is_locked() {
                    log::info!(
                        "scheduled task '{}' (id={}) deferred until the workspace is unlocked",
                        task.server_name,
                        task.id
                    );
                    self.tell(Notice::Scheduled(ScheduledNotice::Deferred(
                        task.server_name.clone(),
                    )));
                    self.record_outcome(&task.id, Outcome::Deferred, None);
                }
            }
            return Vec::new();
        }
        let summaries = self.profile_summaries();
        let resolved = scheduled::resolve(
            task.server_id.as_ref().map(ProfileId::as_str),
            &task.server_name,
            &summaries,
            |profile| profile.id.as_str(),
            |profile| profile.name.as_str(),
        );
        let profile = match resolved {
            Ok(profile) => profile.clone(),
            Err(unresolved) => {
                log::warn!(
                    "scheduled task '{}' (serverId={}) connected nothing: {}. Re-point the \
                     task at the profile you mean.",
                    task.server_name,
                    task.server_id.as_ref().map_or("", ProfileId::as_str),
                    diagnosis(unresolved)
                );
                self.scheduled_failed(&task, unresolved.failure(), Some(now));
                return Vec::new();
            }
        };
        if profile.kind == ProfileKind::Local {
            log::warn!(
                "scheduled task '{}' (id={}) refused: a local shell or program is never opened \
                 by a schedule",
                task.server_name,
                task.id
            );
            self.tell(Notice::Scheduled(ScheduledNotice::Refused(
                task.server_name.clone(),
            )));
            self.record_outcome(&task.id, Outcome::Refused, Some(now));
            return Vec::new();
        }
        self.open_scheduled(&task, profile.id, profile.kind, now)
    }

    /// Opens `id` of `kind` for `task` at `now`, as any saved profile opens, unless a check
    /// would ask first; the tab it opens is watched until it connects or fails.
    fn open_scheduled(
        &mut self,
        task: &ScheduledTask,
        id: ProfileId,
        kind: ProfileKind,
        now: i64,
    ) -> Vec<Effect> {
        let message = open_message(id, kind, Purpose::Shell);
        if self.waits_for_credential_guard(&message) || self.waits_for_hello(&message) {
            log::warn!(
                "scheduled task '{}' (id={}) not run: Windows Hello or Credential Guard would \
                 have had to be asked",
                task.server_name,
                task.id
            );
            self.scheduled_failed(task, FailureKind::VerificationRequired, Some(now));
            return Vec::new();
        }
        log::info!(
            "executing scheduled task '{}' (serverId={}, type={})",
            task.server_name,
            task.server_id.as_ref().map_or("", ProfileId::as_str),
            task.connection_type
        );
        let before = self.tabs.len();
        let effects = self.update(message);
        if self.tabs.len() > before
            && let Some(tab) = self.tabs.last()
        {
            let tab = tab.id;
            self.scheduler.unattended.push((tab, task.id.clone()));
            self.record_last_run(&task.id, now);
            // Told once its tab is shown, which a notice is said for.
            self.tell(Notice::Scheduled(ScheduledNotice::Triggered(
                task.server_name.clone(),
            )));
        } else if matches!(self.notice, Some((Notice::SessionLimitReached(_), _))) {
            self.scheduled_failed(task, FailureKind::SessionLimit, Some(now));
        } else {
            // Opened outside a tab, in an external program, or past the steps' approval.
            self.record_outcome(&task.id, Outcome::Ran, Some(now));
        }
        effects
    }

    /// An event of tab `tab_id`'s connection, looked at first when a task opened it: what
    /// would ask is refused, and the effects returned in place of the usual ones; its
    /// connection or its failure recorded as the task's outcome.
    pub(super) fn unattended_event(
        &mut self,
        tab_id: TabId,
        event: &ConnectionEvent,
    ) -> Option<Vec<Effect>> {
        let index = self
            .scheduler
            .unattended
            .iter()
            .position(|(tab, _)| *tab == tab_id)?;
        let failure = match event {
            ConnectionEvent::Question { .. } => FailureKind::CredentialUnavailable,
            ConnectionEvent::UnknownHostKey { .. }
            | ConnectionEvent::UnknownRdpCertificate { .. } => FailureKind::ConfirmationRequired,
            ConnectionEvent::Failed(_) => FailureKind::ConnectionFailed,
            ConnectionEvent::Connected { .. }
            | ConnectionEvent::RdpReady { .. }
            | ConnectionEvent::VncReady { .. }
            | ConnectionEvent::FilesReady { .. } => {
                let (_, id) = self.scheduler.unattended.remove(index);
                self.record_outcome(&id, Outcome::Ran, None);
                return None;
            }
            _ => return None,
        };
        let (_, id) = self.scheduler.unattended.remove(index);
        let task = self.scheduler.tasks.get(&id).cloned();
        if let Some(task) = &task {
            log::warn!(
                "scheduled task '{}' (id={}) did not connect: {failure:?}",
                task.server_name,
                task.id
            );
            self.record_outcome(&task.id, Outcome::Failed(failure), None);
            self.scheduler.pending = Some(ScheduledNotice::Failed {
                name: task.server_name.clone(),
                failure,
            });
        }
        match event {
            // Declined: the attempt fails, as a question the user cancels.
            ConnectionEvent::Question { question, .. } => Some(vec![Effect::Answer {
                question: *question,
                answer: None,
            }]),
            // The attempt stopped to ask: it stays stopped.
            ConnectionEvent::UnknownHostKey { .. }
            | ConnectionEvent::UnknownRdpCertificate { .. } => {
                if let Some(tab) = self.tab_mut(tab_id) {
                    tab.retry = None;
                    tab.prompts.clear();
                    tab.phase = Phase::Failed(UiError::Cancelled);
                }
                Some(Vec::new())
            }
            _ => None,
        }
    }

    /// Says what a scheduled tab's event left to say, once its tab shows it.
    pub(super) fn tell_scheduled(&mut self) {
        if let Some(notice) = self.scheduler.pending.take() {
            self.tell(Notice::Scheduled(notice));
        }
    }

    /// Says and records that `task` failed for `failure`, at `ran` when it ran now.
    fn scheduled_failed(&mut self, task: &ScheduledTask, failure: FailureKind, ran: Option<i64>) {
        self.tell(Notice::Scheduled(ScheduledNotice::Failed {
            name: task.server_name.clone(),
            failure,
        }));
        self.record_outcome(&task.id, Outcome::Failed(failure), ran);
    }

    /// Records `outcome` for task `id`, run at `ran` when it ran now, and writes the file.
    fn record_outcome(&mut self, id: &str, outcome: Outcome, ran: Option<i64>) {
        if let Some(task) = self
            .scheduler
            .tasks
            .tasks_mut()
            .iter_mut()
            .find(|task| task.id == id)
        {
            task.last_outcome = Some(outcome);
            if let Some(ran) = ran {
                task.last_run = Some(ran);
            }
            self.save_scheduled();
        }
    }

    /// Records that task `id` ran at `ran`, its outcome to come, and writes the file.
    fn record_last_run(&mut self, id: &str, ran: i64) {
        if let Some(task) = self
            .scheduler
            .tasks
            .tasks_mut()
            .iter_mut()
            .find(|task| task.id == id)
        {
            task.last_run = Some(ran);
            self.save_scheduled();
        }
    }

    /// Writes the tasks; a failure is logged, and they are kept for the run.
    fn save_scheduled(&self) {
        if let Err(error) = self.scheduler.tasks.save() {
            log::warn!("scheduled connections not saved: {error}");
        }
    }

    /// Carries `tasks` of this computer's C# store over, each in place of the one of its
    /// identifier, each timed from now on this computer's clock; how many, or why none
    /// could be. `None` when there were none. A file that was not read is left as it is.
    pub(super) fn carry_scheduled(
        &mut self,
        tasks: Vec<ScheduledTask>,
    ) -> Option<Result<usize, String>> {
        if tasks.is_empty() {
            return None;
        }
        let now = crate::time_zone::unix_now();
        let clock = crate::time_zone::SystemClock;
        let tasks = tasks.into_iter().map(|mut task| {
            task.next_run = Some(scheduled::next_run(&task.schedule, now, &clock));
            task
        });
        let carried = match &self.scheduler.unread {
            Some(error) => Err(error.clone()),
            None => self
                .scheduler
                .tasks
                .merge(tasks)
                .map(|count| count.added + count.updated)
                .map_err(|error| error.to_string()),
        };
        match &carried {
            Ok(count) => log::info!("scheduled connections carried over: {count}"),
            Err(error) => log::warn!("scheduled connections not carried over: {error}"),
        }
        Some(carried)
    }
}
