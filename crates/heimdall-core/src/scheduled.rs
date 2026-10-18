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

//! Scheduled connections, as the C# `ScheduledTaskDto` and `TaskSchedulerService` keep and
//! time them: a saved profile opened every day at a time of day, or every so many minutes,
//! while Heimdall runs. Nothing is registered with the system: a task whose time comes while
//! Heimdall is closed is missed.
//!
//! The tasks live in a small file of their own beside the profiles, never in the settings
//! export. The rules here are pure: when a task runs next, from a moment and this computer's
//! clock, and which profile it runs against, from the profiles held.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::profile::{ProfileId, RdpProfile, SshProfile, WinRmProfile};
use crate::store::{StoreError, write_atomic};

/// Name of the file, beside the profiles.
pub const SCHEDULED_FILE_NAME: &str = "scheduled.toml";

/// Version of the file this build writes and reads.
const SCHEDULED_VERSION: u32 = 1;

/// The time of day a daily task takes when none is given, or the one given does not read,
/// as the C# fallback.
pub const DEFAULT_TIME_OF_DAY: &str = "08:00";

/// The interval a new task is offered, in minutes, as the C# dialog.
pub const DEFAULT_INTERVAL_MINUTES: u32 = 30;

/// The interval an interval task below one minute takes, as the C# `ComputeNextRun`.
pub const FALLBACK_INTERVAL_MINUTES: u32 = 60;

/// Seconds in a minute.
const MINUTE_SECONDS: i64 = 60;

/// Seconds in an hour.
const HOUR_SECONDS: i64 = 3_600;

/// Seconds in a day.
const DAY_SECONDS: i64 = 86_400;

/// Hours in a day.
const DAY_HOURS: u8 = 24;

/// Minutes in an hour.
const HOUR_MINUTES: u8 = 60;

/// How close to now a daily time may be and still run today, as the C# "within 1 minute":
/// nearer than that, it runs tomorrow.
const DAILY_LEAD_SECONDS: i64 = MINUTE_SECONDS;

/// Random bytes in a task's identifier.
const ID_BYTES: usize = 16;

/// This computer's clock: its offset from UTC at a moment, and when its wall clock shows a
/// time. Given, so the timing is computed without reading the system, and tested.
pub trait LocalClock {
    /// The offset east of UTC, in seconds, of the wall clock at `unix_seconds`.
    fn offset_at(&self, unix_seconds: i64) -> i32;

    /// The offset east of UTC, in seconds, of the wall clock when it shows `wall_seconds`,
    /// seconds since 1970-01-01 on the wall: the earlier of two when the clock goes back,
    /// the one before a gap when it skips ahead.
    fn offset_of_wall_clock(&self, wall_seconds: i64) -> i32;
}

/// A clock a fixed offset from UTC, which never changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FixedOffset(pub i32);

impl LocalClock for FixedOffset {
    fn offset_at(&self, _unix_seconds: i64) -> i32 {
        self.0
    }

    fn offset_of_wall_clock(&self, _wall_seconds: i64) -> i32 {
        self.0
    }
}

/// A time of day, to the minute.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct TimeOfDay {
    /// Hour, 0 to 23.
    pub hour: u8,
    /// Minute, 0 to 59.
    pub minute: u8,
}

impl TimeOfDay {
    /// `text` as `HH:mm`, the hour on one or two digits, seconds allowed and ignored, as
    /// the C# reads it; `None` for anything else.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let mut fields = text.trim().split(':');
        let hour = fields.next()?;
        let minute = fields.next()?;
        let second = fields.next();
        if fields.next().is_some()
            || !(1..=2).contains(&hour.len())
            || minute.len() != 2
            || second.is_some_and(|second| second.len() != 2)
        {
            return None;
        }
        let digits = |field: &str| {
            field
                .bytes()
                .all(|byte| byte.is_ascii_digit())
                .then(|| field.parse::<u8>().ok())
                .flatten()
        };
        let (hour, minute) = (digits(hour)?, digits(minute)?);
        if hour >= DAY_HOURS || minute >= HOUR_MINUTES {
            return None;
        }
        if let Some(second) = second {
            digits(second).filter(|second| *second < HOUR_MINUTES)?;
        }
        Some(Self { hour, minute })
    }

    /// As `HH:mm`.
    #[must_use]
    pub fn text(self) -> String {
        format!("{:02}:{:02}", self.hour, self.minute)
    }

    /// Seconds from midnight.
    fn seconds(self) -> i64 {
        i64::from(self.hour) * HOUR_SECONDS + i64::from(self.minute) * MINUTE_SECONDS
    }
}

/// When a task runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Schedule {
    /// Every day at a time of day, `HH:mm` on this computer's clock; a time that does not
    /// read runs at [`DEFAULT_TIME_OF_DAY`], as the C#.
    Daily {
        /// The time, as written.
        time: String,
    },
    /// Every so many minutes; below one, [`FALLBACK_INTERVAL_MINUTES`].
    Interval {
        /// The interval, in minutes.
        minutes: u32,
    },
}

impl Schedule {
    /// The time of day a daily task runs at: the one written, or [`DEFAULT_TIME_OF_DAY`]
    /// when it does not read; `None` for an interval.
    #[must_use]
    pub fn time_of_day(&self) -> Option<TimeOfDay> {
        match self {
            Self::Daily { time } => Some(TimeOfDay::parse(time).unwrap_or_else(default_time)),
            Self::Interval { .. } => None,
        }
    }

    /// The interval an interval task runs at, in minutes, [`FALLBACK_INTERVAL_MINUTES`]
    /// below one; `None` for a daily task.
    #[must_use]
    pub fn interval_minutes(&self) -> Option<u32> {
        match self {
            Self::Interval { minutes } => Some(effective_minutes(*minutes)),
            Self::Daily { .. } => None,
        }
    }
}

/// [`DEFAULT_TIME_OF_DAY`], read.
fn default_time() -> TimeOfDay {
    TimeOfDay { hour: 8, minute: 0 }
}

/// `minutes`, or [`FALLBACK_INTERVAL_MINUTES`] below one.
fn effective_minutes(minutes: u32) -> u32 {
    if minutes == 0 {
        FALLBACK_INTERVAL_MINUTES
    } else {
        minutes
    }
}

/// Why a task's connection failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureKind {
    /// The task names a profile identifier none holds.
    ProfileNotFound,
    /// The task names no identifier, and no profile carries its name.
    NameNotFound,
    /// The task names no identifier, and several profiles carry its name.
    NameAmbiguous,
    /// The profile's credentials are not stored, or could not be read without asking.
    CredentialUnavailable,
    /// Windows Hello or Credential Guard would have had to be asked.
    VerificationRequired,
    /// The server's key or certificate is not trusted yet: the user decides it, never a
    /// schedule.
    ConfirmationRequired,
    /// The most sessions the settings allowed were open. No longer recorded: the setting is
    /// a threshold that refuses nothing, as the C#; kept so a file that recorded it still
    /// reads, and says why.
    SessionLimit,
    /// The connection did not open.
    ConnectionFailed,
}

/// What became of a task the last time it was due.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "reason", rename_all = "snake_case")]
pub enum Outcome {
    /// It opened its connection.
    Ran,
    /// Its time came while Heimdall was closed: it was skipped.
    Missed,
    /// It did not open its connection.
    Failed(FailureKind),
    /// Its time came while the workspace was locked: it runs once unlocked.
    Deferred,
    /// Its profile is one a schedule never opens, for this reason.
    Refused(RefusalKind),
}

/// Why a schedule never opens a profile: opening it would start a program on this computer
/// or ask the user something, and a schedule runs unattended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RefusalKind {
    /// A local shell or program.
    LocalProgram,
    /// Opened in an external client, `PuTTY` or Remote Desktop Connection.
    ExternalClient,
    /// An RDP profile through a Remote Desktop Gateway, which opens in Remote Desktop
    /// Connection.
    RdGateway,
    /// A Citrix application, launched by the Citrix Workspace client.
    Citrix,
    /// A web console, opened in the system's browser.
    WebConsole,
    /// A `WinRM` profile logging in as the account running Heimdall, or without an
    /// account: only a stored account and password are used unattended.
    WinRmNoAccount,
    /// A `WinRM` profile whose password is not stored.
    WinRmPasswordNotStored,
    /// An SSH profile forwarding X11, which starts the local X server.
    X11Forwarding,
    /// An SSH profile whose post-connect steps are not approved as they are: approving
    /// them asks the user.
    StepsNotApproved,
    /// An SFTP profile while the file browser is turned off.
    FilesBrowserOff,
}

/// What a schedule is asked to open, with what decides whether it may.
#[derive(Debug, Clone, Copy)]
pub enum Target<'a> {
    /// An SSH or SFTP profile.
    Ssh(&'a SshProfile),
    /// An RDP profile.
    Rdp(&'a RdpProfile),
    /// A `WinRM` profile, and whether a password is stored for its account.
    WinRm {
        /// The profile.
        profile: &'a WinRmProfile,
        /// A password is stored for its host, port and account.
        password_stored: bool,
    },
    /// A Telnet profile.
    Telnet,
    /// A VNC profile.
    Vnc,
    /// An FTP profile.
    Ftp,
    /// A Citrix profile.
    Citrix,
    /// A web console, opened in the system's browser.
    Web,
    /// A local shell or program profile.
    Local,
}

/// Why a schedule never opens `target`; `None` when it may, its SFTP profiles only while
/// `files_browser` is on. A profile opens unattended only in a tab of the application, with
/// what is stored for it: never in an external program, never asking anything.
#[must_use]
pub fn refusal(target: Target<'_>, files_browser: bool) -> Option<RefusalKind> {
    match target {
        Target::Local => Some(RefusalKind::LocalProgram),
        Target::Citrix => Some(RefusalKind::Citrix),
        // The system's browser is a program of this computer, as an external client is.
        Target::Web => Some(RefusalKind::WebConsole),
        Target::Ssh(profile) if profile.sftp => {
            (!files_browser).then_some(RefusalKind::FilesBrowserOff)
        }
        Target::Ssh(profile) if !profile.ssh_mode.is_embedded() => {
            Some(RefusalKind::ExternalClient)
        }
        Target::Ssh(profile) if profile.x11_forwarding => Some(RefusalKind::X11Forwarding),
        Target::Ssh(profile) if profile.post_connect.needs_approval() => {
            Some(RefusalKind::StepsNotApproved)
        }
        Target::Rdp(profile) if profile.extras.external => Some(RefusalKind::ExternalClient),
        Target::Rdp(profile) if profile.extras.rd_gateway().is_some() => {
            Some(RefusalKind::RdGateway)
        }
        Target::WinRm { profile, .. }
            if profile
                .username
                .as_deref()
                .is_none_or(|user| user.trim().is_empty()) =>
        {
            Some(RefusalKind::WinRmNoAccount)
        }
        Target::WinRm {
            password_stored: false,
            ..
        } => Some(RefusalKind::WinRmPasswordNotStored),
        Target::Ssh(_)
        | Target::Rdp(_)
        | Target::WinRm { .. }
        | Target::Telnet
        | Target::Vnc
        | Target::Ftp => None,
    }
}

/// A scheduled connection, as the C# `ScheduledTaskDto`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScheduledTask {
    /// Its own identifier.
    pub id: String,
    /// The profile it opens; `None` for a task of an older C# file, which named it only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_id: Option<ProfileId>,
    /// The profile's name when the task was saved.
    pub server_name: String,
    /// The profile's connection type when the task was saved, as the C# shows it.
    pub connection_type: String,
    /// Whether it runs.
    pub enabled: bool,
    /// When it runs.
    pub schedule: Schedule,
    /// When it last ran, in seconds since the Unix epoch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_run: Option<i64>,
    /// When it runs next, in seconds since the Unix epoch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_run: Option<i64>,
    /// What became of it the last time it was due.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_outcome: Option<Outcome>,
    /// Its failures and refusals in a row, since it last ran: at
    /// [`MAX_CONSECUTIVE_FAILURES`] it is turned off.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub failures: u32,
}

/// Failures and refusals in a row after which a task is turned off, so that a task that
/// cannot run does not fail again every time its schedule comes round.
pub const MAX_CONSECUTIVE_FAILURES: u32 = 3;

/// Whether `count` is zero, for a count written only when there is one.
#[expect(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde's skip_serializing_if passes a reference"
)]
fn is_zero(count: &u32) -> bool {
    *count == 0
}

impl ScheduledTask {
    /// Whether it was turned off after [`MAX_CONSECUTIVE_FAILURES`] failures in a row.
    #[must_use]
    pub fn suspended(&self) -> bool {
        !self.enabled && self.failures >= MAX_CONSECUTIVE_FAILURES
    }

    /// Records `outcome`: a run clears the failures, a failure or a refusal counts one, and
    /// the task is turned off at [`MAX_CONSECUTIVE_FAILURES`]; whether this turned it off.
    pub fn record(&mut self, outcome: Outcome) -> bool {
        self.last_outcome = Some(outcome);
        match outcome {
            Outcome::Ran => self.failures = 0,
            Outcome::Failed(_) | Outcome::Refused(_) => {
                self.failures = self.failures.saturating_add(1);
                if self.enabled && self.failures >= MAX_CONSECUTIVE_FAILURES {
                    self.enabled = false;
                    return true;
                }
            }
            Outcome::Missed | Outcome::Deferred => {}
        }
        false
    }
}

/// When a task of `schedule` runs next after `now`, in seconds since the Unix epoch, as the
/// C# `ComputeNextRun`: a daily task at its time today, or tomorrow when that is past or
/// within a minute; an interval task `minutes` from now. Days are counted on `clock`'s wall
/// clock, so a daily task keeps its time across a change of the clock.
#[must_use]
pub fn next_run(schedule: &Schedule, now: i64, clock: &dyn LocalClock) -> i64 {
    match schedule {
        Schedule::Interval { minutes } => {
            now + i64::from(effective_minutes(*minutes)) * MINUTE_SECONDS
        }
        Schedule::Daily { time } => {
            let wall_now = now + i64::from(clock.offset_at(now));
            let today = wall_now.div_euclid(DAY_SECONDS) * DAY_SECONDS;
            let at = |wall: i64| wall - i64::from(clock.offset_of_wall_clock(wall));
            match TimeOfDay::parse(time) {
                Some(time) => {
                    let candidate = at(today + time.seconds());
                    if candidate <= now + DAILY_LEAD_SECONDS {
                        at(today + DAY_SECONDS + time.seconds())
                    } else {
                        candidate
                    }
                }
                // A time that does not read: tomorrow at the default, as the C#.
                None => at(today + DAY_SECONDS + default_time().seconds()),
            }
        }
    }
}

/// Why a task was matched to no profile, as the C# `ScheduledTaskResolution`: three ways to
/// answer nothing, which the log says apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unresolved {
    /// The task names an identifier and no profile carries it.
    IdentifierNotFound,
    /// The task names no identifier, and no profile carries its name.
    NameNotFound,
    /// The task names no identifier, and several profiles carry its name.
    NameAmbiguous,
}

impl Unresolved {
    /// The failure it records.
    #[must_use]
    pub fn failure(self) -> FailureKind {
        match self {
            Self::IdentifierNotFound => FailureKind::ProfileNotFound,
            Self::NameNotFound => FailureKind::NameNotFound,
            Self::NameAmbiguous => FailureKind::NameAmbiguous,
        }
    }
}

/// The profile among `servers` a task naming `server_id` and `server_name` runs against, as
/// the C# `ScheduledTaskServerResolver`: a task that names an identifier is answered by that
/// identifier, compared exactly, or not at all, since a name left unique by the deletion of
/// the task's own profile names another machine; a task naming none is answered by its name,
/// without case, only when exactly one profile carries it. `servers` is every profile held,
/// never a filtered view.
///
/// # Errors
///
/// [`Unresolved`], saying which of the three ways nothing was found.
pub fn resolve<'a, T>(
    server_id: Option<&str>,
    server_name: &str,
    servers: impl IntoIterator<Item = &'a T>,
    id_of: impl Fn(&T) -> &str,
    name_of: impl Fn(&T) -> &str,
) -> Result<&'a T, Unresolved>
where
    T: 'a + ?Sized,
{
    if let Some(id) = server_id.filter(|id| !id.is_empty()) {
        return servers
            .into_iter()
            .find(|server| id_of(server) == id)
            .ok_or(Unresolved::IdentifierNotFound);
    }
    let mut named = servers
        .into_iter()
        .filter(|server| same_name(name_of(server), server_name));
    match (named.next(), named.next()) {
        (Some(server), None) => Ok(server),
        (None, _) => Err(Unresolved::NameNotFound),
        (Some(_), Some(_)) => Err(Unresolved::NameAmbiguous),
    }
}

/// Whether `a` and `b` are the same name without case, as .NET's `OrdinalIgnoreCase`.
fn same_name(a: &str, b: &str) -> bool {
    a.chars()
        .flat_map(char::to_uppercase)
        .eq(b.chars().flat_map(char::to_uppercase))
}

/// A new task identifier: random, as hexadecimal.
///
/// # Errors
///
/// [`sealvault::Error`] when the system gives no random bytes.
pub fn new_task_id() -> Result<String, sealvault::Error> {
    let bytes: [u8; ID_BYTES] = sealvault::random::array()?;
    Ok(data_encoding::HEXLOWER.encode(&bytes))
}

/// On-disk shape of the file.
#[derive(Debug, Default, Serialize, Deserialize)]
struct ScheduledFile {
    #[serde(default)]
    version: u32,
    #[serde(default, rename = "task")]
    tasks: Vec<ScheduledTask>,
}

/// The version of the file, read alone.
#[derive(Debug, Deserialize)]
struct FileVersion {
    #[serde(default)]
    version: u32,
}

/// What [`ScheduledTasks::merge`] did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MergeCount {
    /// Tasks whose identifier was not held.
    pub added: usize,
    /// Tasks whose identifier was held, replaced.
    pub updated: usize,
}

/// The scheduled tasks, read once and written on each change.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScheduledTasks {
    /// The file they are kept in; `None` keeps them for the run only.
    path: Option<PathBuf>,
    /// In the order they were added.
    tasks: Vec<ScheduledTask>,
}

impl ScheduledTasks {
    /// The tasks kept at `path`; none when there is no file.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the file is there and cannot be read, does not parse, or
    /// was written by a newer version.
    pub fn load(path: &Path) -> Result<Self, StoreError> {
        let text = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(Self::kept_at(path));
            }
            Err(source) => {
                return Err(StoreError::Io {
                    path: path.to_owned(),
                    source,
                });
            }
        };
        let parse_error = |source| StoreError::Parse {
            path: path.to_owned(),
            source,
        };
        // The version first: a newer file need not have a shape this build reads.
        let version = toml::from_str::<FileVersion>(&text)
            .map_err(parse_error)?
            .version;
        if version > SCHEDULED_VERSION {
            return Err(StoreError::UnsupportedVersion {
                path: path.to_owned(),
                found: version,
                expected: SCHEDULED_VERSION,
            });
        }
        let file: ScheduledFile = toml::from_str(&text).map_err(parse_error)?;
        Ok(Self {
            path: Some(path.to_owned()),
            tasks: file.tasks,
        })
    }

    /// The tasks kept at `path`, and why none were read when a file was there. A file that
    /// does not read is never written over, so no task is lost to a typing mistake in it:
    /// what is changed then lasts as long as the run.
    #[must_use]
    pub fn open(path: &Path) -> (Self, Option<StoreError>) {
        match Self::load(path) {
            Ok(tasks) => (tasks, None),
            Err(error) => (Self::default(), Some(error)),
        }
    }

    /// No task yet, kept at `path`.
    fn kept_at(path: &Path) -> Self {
        Self {
            path: Some(path.to_owned()),
            tasks: Vec::new(),
        }
    }

    /// The file they are kept in; `None` while they last as long as the run.
    #[must_use]
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// The tasks, in the order they were added.
    #[must_use]
    pub fn tasks(&self) -> &[ScheduledTask] {
        &self.tasks
    }

    /// The task of identifier `id`.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&ScheduledTask> {
        self.tasks.iter().find(|task| task.id == id)
    }

    /// The tasks to change; [`Self::save`] writes them.
    pub fn tasks_mut(&mut self) -> &mut Vec<ScheduledTask> {
        &mut self.tasks
    }

    /// Puts `task` in place of the one of its identifier, or last when none has it, and
    /// writes the file.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the file cannot be written; the change is kept for the
    /// run all the same.
    pub fn upsert(&mut self, task: ScheduledTask) -> Result<(), StoreError> {
        self.put(task);
        self.save()
    }

    /// Removes the task of identifier `id`, and writes the file when there was one.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the file cannot be written.
    pub fn remove(&mut self, id: &str) -> Result<bool, StoreError> {
        let before = self.tasks.len();
        self.tasks.retain(|task| task.id != id);
        if self.tasks.len() == before {
            return Ok(false);
        }
        self.save().map(|()| true)
    }

    /// Puts each of `tasks` in place of the one of its identifier, or last, and writes the
    /// file once.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the file cannot be written; the tasks are kept for the
    /// run all the same.
    pub fn merge(
        &mut self,
        tasks: impl IntoIterator<Item = ScheduledTask>,
    ) -> Result<MergeCount, StoreError> {
        let mut count = MergeCount::default();
        for task in tasks {
            if self.put(task) {
                count.updated += 1;
            } else {
                count.added += 1;
            }
        }
        self.save().map(|()| count)
    }

    /// Puts `task` in place of the one of its identifier, or last; whether one had it.
    fn put(&mut self, task: ScheduledTask) -> bool {
        if let Some(held) = self.tasks.iter_mut().find(|held| held.id == task.id) {
            *held = task;
            true
        } else {
            self.tasks.push(task);
            false
        }
    }

    /// Writes the file, when they are kept in one.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the file cannot be written.
    pub fn save(&self) -> Result<(), StoreError> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        let text = toml::to_string_pretty(&ScheduledFile {
            version: SCHEDULED_VERSION,
            tasks: self.tasks.clone(),
        })?;
        write_atomic(path, &text)
    }
}

/// The file beside `profiles_file`, in the configuration folder.
#[must_use]
pub fn scheduled_path(profiles_file: &Path) -> PathBuf {
    profiles_file.with_file_name(SCHEDULED_FILE_NAME)
}

#[cfg(test)]
#[path = "scheduled_tests.rs"]
mod tests;
