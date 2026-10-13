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
//! look every minute, a task due opened through the same connection as any saved profile.
//! Nothing is registered with the system: a task whose time came while Heimdall was closed
//! is missed, and timed again from the start, never run in a burst. A round is made within
//! one message, which the application applies one at a time: rounds never overlap, so no
//! guard is kept for it, unlike the C# timer's.
//!
//! Unattended, a task never asks anything and never starts a program on this computer: a
//! profile that would ([`scheduled::refusal`]) is refused. What it opens uses what is saved
//! for its profile only, in a tab opened behind the one shown: a password not saved, a
//! Windows Hello or Credential Guard check that would ask, a server key or certificate not
//! trusted yet fail it, said in the status bar and the log and recorded as its last
//! outcome; a tab that fails before connecting is closed, and a task failing
//! [`scheduled::MAX_CONSECUTIVE_FAILURES`] times in a row is turned off. A session that
//! connected and dropped or ended later is no failure of the task, which ran: it counts
//! toward nothing, and its tab, left for the user to see, is closed when the task runs
//! again. Its tabs keep their
//! origin for their whole life: they never reconnect by themselves. A task due while the
//! workspace is locked waits for the unlock, as an auto-reconnect does, then runs once; one
//! due while a tab it opened is still open is skipped.

use std::path::Path;
use std::time::Duration;

use heimdall_core::profile::ProfileId;
use heimdall_core::scheduled::{
    self, FailureKind, LocalClock, Outcome, RefusalKind, ScheduledTask, ScheduledTasks, Target,
    Unresolved,
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
    /// A task names a profile a schedule never opens.
    Refused {
        /// The profile's name, as the task recorded it.
        name: String,
        /// Why.
        refusal: RefusalKind,
    },
    /// A task waits for the workspace to be unlocked.
    Deferred(String),
    /// A task for the profile of this name was created.
    Added(String),
    /// A task for the profile of this name was changed.
    Updated(String),
    /// A task for the profile of this name was deleted.
    Deleted(String),
    /// A task was turned off after failing this many times in a row.
    Suspended {
        /// The profile's name, as the task recorded it.
        name: String,
        /// The failures in a row.
        failures: u32,
    },
}

/// Where a tab a task opened stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage {
    /// Connecting; its outcome is to be recorded.
    Opening,
    /// Its failure is recorded: closed once its attempt ends.
    Failing,
    /// Connected.
    Connected,
}

/// A tab a task opened, for its whole life.
#[derive(Debug, Clone)]
struct Origin {
    tab: TabId,
    task: String,
    /// The SFTP pane docked beside the task's shell: unattended too, its outcome not the
    /// task's.
    companion: bool,
    stage: Stage,
}

/// The scheduled tasks and what is running of them.
#[derive(Debug, Default)]
pub(super) struct Scheduler {
    /// The tasks, and their file.
    pub(super) tasks: ScheduledTasks,
    /// Why the file was not read, when it was there and was not: the tasks are then kept
    /// for the run only, and the migration does not add to them.
    unread: Option<String>,
    /// The tabs tasks opened.
    origins: Vec<Origin>,
    /// The tasks due and not run yet: while the workspace was locked, or a dialog was open.
    held: Vec<String>,
    /// The task selected on the Scheduled page.
    pub(super) selected: Option<String>,
    /// What to say once the event being applied has changed what the tab shows.
    pending: Option<ScheduledNotice>,
    /// The tabs to close once the event being applied is done with them.
    closing: Vec<TabId>,
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

/// `text` for a log line: its control characters and its invisible format characters
/// (bidirectional embeddings, overrides and isolates, zero-width characters and marks, the
/// byte order mark) shown as `?`, so that a name read from a file can neither forge or
/// break the line nor make it read other than it is.
fn logged(text: &str) -> String {
    text.chars()
        .map(|c| {
            if c.is_control() || is_format(c) {
                '?'
            } else {
                c
            }
        })
        .collect()
}

/// Whether `c` is an invisible character that changes how a line reads.
fn is_format(c: char) -> bool {
    matches!(
        c,
        '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}' | '\u{FEFF}'
    )
}

/// The name of `dialog`'s kind, for the log: never what it holds.
fn dialog_kind(dialog: &super::Dialog) -> String {
    format!("{dialog:?}")
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect()
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

    /// Whether tab `tab_id` was opened by a scheduled task, or docked beside one that was.
    #[must_use]
    pub fn is_scheduled_tab(&self, tab_id: TabId) -> bool {
        self.scheduler
            .origins
            .iter()
            .any(|origin| origin.tab == tab_id)
    }

    /// Why a schedule never opens saved profile `id` of `kind`; `None` when it may.
    #[must_use]
    pub fn scheduled_refusal(&self, id: &ProfileId, kind: ProfileKind) -> Option<RefusalKind> {
        let target = match kind {
            ProfileKind::Ssh | ProfileKind::Sftp => {
                Target::Ssh(self.profiles().iter().find(|profile| &profile.id == id)?)
            }
            ProfileKind::Rdp => Target::Rdp(
                self.rdp_profiles()
                    .iter()
                    .find(|profile| &profile.id == id)?,
            ),
            ProfileKind::WinRm => {
                let profile = self
                    .winrm_profiles()
                    .iter()
                    .find(|profile| &profile.id == id)?;
                Target::WinRm {
                    profile,
                    password_stored: self.winrm_stored_password(profile).is_some(),
                }
            }
            ProfileKind::Telnet => Target::Telnet,
            ProfileKind::Vnc => Target::Vnc,
            ProfileKind::Ftp => Target::Ftp,
            ProfileKind::Citrix => Target::Citrix,
            ProfileKind::Local => Target::Local,
        };
        scheduled::refusal(target, self.settings.sftp_browser.enabled)
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
                        logged(&task.server_name),
                        logged(&task.id)
                    );
                    task.record(Outcome::Missed);
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
    /// again and run, after those held once nothing holds them.
    pub fn scheduled_tick_at(&mut self, now: i64, clock: &dyn LocalClock) -> Vec<Effect> {
        let open: Vec<TabId> = self.tabs.iter().map(|tab| tab.id).collect();
        self.scheduler
            .origins
            .retain(|origin| open.contains(&origin.tab));
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
                        logged(&task.server_name),
                        logged(&task.id)
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

    /// Holds task `task` until nothing holds it: said and recorded when the workspace is
    /// locked; a dialog open, it simply waits for it.
    fn hold_scheduled(&mut self, task: &ScheduledTask) {
        if self.scheduler.held.contains(&task.id) {
            return;
        }
        self.scheduler.held.push(task.id.clone());
        if self.is_locked() {
            log::info!(
                "scheduled task '{}' (id={}) deferred until the workspace is unlocked",
                logged(&task.server_name),
                logged(&task.id)
            );
            self.tell(Notice::Scheduled(ScheduledNotice::Deferred(
                task.server_name.clone(),
            )));
            self.record_outcome(&task.id, Outcome::Deferred, None);
        }
    }

    /// Whether a tab task `id` opened is still connecting or connected.
    fn still_open(&self, id: &str) -> bool {
        self.scheduler.origins.iter().any(|origin| {
            origin.task == id
                && !origin.companion
                && origin.stage != Stage::Failing
                && self.tab(origin.tab).is_some_and(|tab| {
                    !matches!(tab.phase, Phase::Failed(_) | Phase::Closed { .. })
                })
        })
    }

    /// Closes the panes task `id` opened whose session ended or failed, its SFTP pane among
    /// them, before it runs again: a task on a server that ends its sessions keeps one tab,
    /// never one more each run. Only those panes: a pane the user put in their split stays.
    fn close_finished(&mut self, id: &str) {
        let finished: Vec<TabId> = self
            .scheduler
            .origins
            .iter()
            .filter(|origin| origin.task == id)
            .map(|origin| origin.tab)
            .filter(|tab| {
                self.tab(*tab)
                    .is_some_and(|tab| matches!(tab.phase, Phase::Failed(_) | Phase::Closed { .. }))
            })
            .collect();
        for tab in finished {
            self.close_pane(tab);
        }
        let open: Vec<TabId> = self.tabs.iter().map(|tab| tab.id).collect();
        self.scheduler
            .origins
            .retain(|origin| open.contains(&origin.tab));
    }

    /// Runs task `id` at `now`: held while locked or while a dialog is open, skipped while a
    /// tab it opened is open, failed when its profile is not found, refused when a schedule
    /// never opens it; else its profile opens as any saved profile does, unattended.
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
            self.hold_scheduled(&task);
            return Vec::new();
        }
        if self.still_open(id) {
            // Its last outcome stays: the run before is still going.
            log::info!(
                "scheduled task '{}' (id={}) skipped: the session it opened is still open",
                logged(&task.server_name),
                logged(&task.id)
            );
            return Vec::new();
        }
        self.close_finished(id);
        let summaries = self.profile_summaries();
        let resolved = scheduled::resolve(
            task.server_id.as_ref().map(ProfileId::as_str),
            &task.server_name,
            &summaries,
            |profile| profile.id.as_str(),
            |profile| profile.name.as_str(),
        );
        let (profile, kind) = match resolved {
            Ok(profile) => (profile.id.clone(), profile.kind),
            Err(unresolved) => {
                log::warn!(
                    "scheduled task '{}' (serverId={}) connected nothing: {}. Re-point the \
                     task at the profile you mean.",
                    logged(&task.server_name),
                    logged(task.server_id.as_ref().map_or("", ProfileId::as_str)),
                    diagnosis(unresolved)
                );
                self.scheduled_failed(&task, unresolved.failure(), Some(now));
                return Vec::new();
            }
        };
        if let Some(refusal) = self.scheduled_refusal(&profile, kind) {
            log::warn!(
                "scheduled task '{}' (id={}) refused: {refusal:?}",
                logged(&task.server_name),
                logged(&task.id)
            );
            self.tell(Notice::Scheduled(ScheduledNotice::Refused {
                name: task.server_name.clone(),
                refusal,
            }));
            self.record_outcome(&task.id, Outcome::Refused(refusal), Some(now));
            return Vec::new();
        }
        self.open_scheduled(&task, profile, kind, now)
    }

    /// Opens `id` of `kind` for `task` at `now`, as any saved profile opens, behind the tab
    /// shown, unless a check would ask first or the session limit is reached; the tab it
    /// opens is watched until it connects or fails.
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
                logged(&task.server_name),
                logged(&task.id)
            );
            self.scheduled_failed(task, FailureKind::VerificationRequired, Some(now));
            return Vec::new();
        }
        if self.session_limit_hit().is_some() {
            log::warn!(
                "scheduled task '{}' (id={}) not run: the session limit is reached",
                logged(&task.server_name),
                logged(&task.id)
            );
            self.scheduled_failed(task, FailureKind::SessionLimit, Some(now));
            return Vec::new();
        }
        log::info!(
            "executing scheduled task '{}' (serverId={}, type={})",
            logged(&task.server_name),
            logged(task.server_id.as_ref().map_or("", ProfileId::as_str)),
            logged(&task.connection_type)
        );
        let shown = self.active;
        let before = self.tabs.len();
        let effects = self.update(message);
        let opened = (self.tabs.len() > before)
            .then(|| self.tabs.last().map(|tab| tab.id))
            .flatten();
        // Behind the tab shown: a schedule never takes the keyboard.
        if shown.is_some() && self.active != shown {
            self.active = shown;
            self.sync_focus();
        }
        if self.dialog.is_some() {
            // Never expected of a profile not refused; a dialog a schedule raised would wait
            // for the user, and hold every task behind it: closed, the session with it.
            let kind = self.dialog.as_ref().map(dialog_kind).unwrap_or_default();
            log::warn!(
                "scheduled task '{}' (id={}) raised a dialog ({kind}): closed, and the session \
                 with it",
                logged(&task.server_name),
                logged(&task.id)
            );
            self.dialog = None;
            if let Some(tab) = opened {
                self.close_pane(tab);
            }
            self.scheduled_failed(task, FailureKind::ConfirmationRequired, Some(now));
            return effects;
        }
        let Some(tab) = opened else {
            log::warn!(
                "scheduled task '{}' (id={}) opened no session",
                logged(&task.server_name),
                logged(&task.id)
            );
            self.scheduled_failed(task, FailureKind::ConnectionFailed, Some(now));
            return effects;
        };
        self.scheduler.origins.push(Origin {
            tab,
            task: task.id.clone(),
            companion: false,
            stage: Stage::Opening,
        });
        self.record_last_run(&task.id, now);
        self.tell(Notice::Scheduled(ScheduledNotice::Triggered(
            task.server_name.clone(),
        )));
        effects
    }

    /// The SFTP pane docked beside scheduled shell `shell` since there were `count` tabs:
    /// unattended too.
    pub(super) fn adopt_companion(&mut self, shell: TabId, count: usize) {
        let Some(task) = self
            .scheduler
            .origins
            .iter()
            .find(|origin| origin.tab == shell)
            .map(|origin| origin.task.clone())
        else {
            return;
        };
        let added: Vec<TabId> = self.tabs.iter().skip(count).map(|tab| tab.id).collect();
        for tab in added {
            self.scheduler.origins.push(Origin {
                tab,
                task: task.clone(),
                companion: true,
                stage: Stage::Opening,
            });
        }
    }

    /// An event of tab `tab_id`'s connection, looked at first when a task opened it: what
    /// would ask is refused, and the effects returned in place of the usual ones; its
    /// connection or its failure recorded as the task's outcome, and a tab failing before
    /// it connected closed.
    pub(super) fn unattended_event(
        &mut self,
        tab_id: TabId,
        event: &ConnectionEvent,
    ) -> Option<Vec<Effect>> {
        let index = self
            .scheduler
            .origins
            .iter()
            .position(|origin| origin.tab == tab_id)?;
        let origin = self.scheduler.origins[index].clone();
        let failure = match event {
            ConnectionEvent::Question { .. } => FailureKind::CredentialUnavailable,
            ConnectionEvent::UnknownHostKey { .. }
            | ConnectionEvent::UnknownRdpCertificate { .. } => FailureKind::ConfirmationRequired,
            ConnectionEvent::Failed(_) => {
                if origin.stage != Stage::Connected {
                    self.scheduler.closing.push(tab_id);
                }
                if origin.stage == Stage::Opening && !origin.companion {
                    self.scheduled_tab_failed(&origin.task, FailureKind::ConnectionFailed);
                }
                self.scheduler.origins[index].stage = Stage::Failing;
                return None;
            }
            ConnectionEvent::Connected { .. }
            | ConnectionEvent::RdpReady { .. }
            | ConnectionEvent::VncReady { .. }
            | ConnectionEvent::FilesReady { .. } => {
                if origin.stage == Stage::Opening && !origin.companion {
                    self.record_outcome(&origin.task, Outcome::Ran, None);
                }
                self.scheduler.origins[index].stage = Stage::Connected;
                return None;
            }
            _ => return None,
        };
        self.scheduler.origins[index].stage = Stage::Failing;
        if !origin.companion {
            let locked = self.is_locked() && matches!(event, ConnectionEvent::Question { .. });
            match self.scheduler.tasks.get(&origin.task).cloned() {
                // Locked while it connected: its saved password could not be read. It runs
                // again once unlocked, as a task due while locked.
                Some(task) if locked => self.hold_scheduled(&task),
                Some(_) if origin.stage == Stage::Opening => {
                    self.scheduled_tab_failed(&origin.task, failure);
                }
                _ => {}
            }
        }
        // Declined: the attempt fails, as a question the user cancels, and its tab closes
        // then.
        if let ConnectionEvent::Question { question, .. } = event {
            return Some(vec![Effect::Answer {
                question: *question,
                answer: None,
            }]);
        }
        // The attempt stopped to ask: its tab closes.
        if let Some(tab) = self.tab_mut(tab_id) {
            tab.retry = None;
            tab.prompts.clear();
            tab.phase = Phase::Failed(UiError::Cancelled);
        }
        self.scheduler.closing.push(tab_id);
        Some(Vec::new())
    }

    /// Says what a scheduled tab's event left to say, once its tab shows it, and closes the
    /// scheduled tabs that failed.
    pub(super) fn after_scheduled_event(&mut self) {
        for tab in std::mem::take(&mut self.scheduler.closing) {
            if self.tab(tab).is_some() {
                self.close_pane(tab);
            }
        }
        if let Some(notice) = self.scheduler.pending.take() {
            self.tell(Notice::Scheduled(notice));
        }
    }

    /// Records that the tab of task `id` failed for `failure`, said once the event is
    /// applied.
    fn scheduled_tab_failed(&mut self, id: &str, failure: FailureKind) {
        let Some(task) = self.scheduler.tasks.get(id).cloned() else {
            return;
        };
        log::warn!(
            "scheduled task '{}' (id={}) did not connect: {failure:?}",
            logged(&task.server_name),
            logged(&task.id)
        );
        self.scheduler.pending = Some(ScheduledNotice::Failed {
            name: task.server_name.clone(),
            failure,
        });
        if let Some(suspended) = self.record_outcome(id, Outcome::Failed(failure), None) {
            self.scheduler.pending = Some(suspended);
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
    /// A task turned off by it is said, and logged; what is said of it, then.
    fn record_outcome(
        &mut self,
        id: &str,
        outcome: Outcome,
        ran: Option<i64>,
    ) -> Option<ScheduledNotice> {
        let task = self
            .scheduler
            .tasks
            .tasks_mut()
            .iter_mut()
            .find(|task| task.id == id)?;
        if let Some(ran) = ran {
            task.last_run = Some(ran);
        }
        let suspended = task.record(outcome).then(|| ScheduledNotice::Suspended {
            name: task.server_name.clone(),
            failures: task.failures,
        });
        if suspended.is_some() {
            log::warn!(
                "scheduled task '{}' (id={}) turned off after {} failures in a row",
                logged(&task.server_name),
                logged(&task.id),
                task.failures
            );
        }
        self.save_scheduled();
        if let Some(notice) = &suspended {
            self.tell(Notice::Scheduled(notice.clone()));
        }
        suspended
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
    pub(super) fn save_scheduled(&self) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_cannot_hide_how_a_log_line_reads() {
        for invisible in [
            '\u{200B}', '\u{200D}', '\u{200F}', '\u{202A}', '\u{202E}', '\u{2066}', '\u{2069}',
            '\u{FEFF}',
        ] {
            assert_eq!(logged(&format!("a{invisible}b")), "a?b", "{invisible:?}");
        }
        assert_eq!(
            logged("Serveur \u{2010}"),
            "Serveur \u{2010}",
            "a visible hyphen stays"
        );
    }

    #[test]
    fn a_dialog_is_logged_by_its_kind_alone() {
        let dialog = super::super::Dialog::ExportDone { count: 3 };
        assert_eq!(dialog_kind(&dialog), "ExportDone");
    }

    #[test]
    fn a_name_cannot_forge_or_break_a_log_line() {
        assert_eq!(
            logged("web\n[ERROR] forged\r\u{1b}[2J"),
            "web?[ERROR] forged??[2J"
        );
        assert_eq!(
            logged("Serveur Été"),
            "Serveur Été",
            "only control characters"
        );
    }
}
