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

//! A Citrix application's status tab, as the C# `EmbeddedCitrixView` in its external mode:
//! the launch, then the state of the Citrix client running the session.
//!
//! The C# finds the session's window and its owner process through Win32 calls this crate
//! cannot make. Here the client is found by its process alone: the `wfica32.exe` processes
//! running before the launch are listed, and a process of that name that appears after it
//! is this session's client. When one was already running and none appears, the session
//! may have opened in it, as Citrix shares a client between sessions: it cannot be told
//! apart from the others.
//!
//! The processes are listed by Windows' own `tasklist.exe`, from the system folder, in its
//! CSV form; only the CSV structure, the image name and the process id are read, the rest
//! being in the language of Windows.

use std::collections::BTreeSet;
use std::path::Path;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use crate::citrix::{CitrixLaunch, CitrixRefusal};
use crate::citrix_terminate::{TerminateOffer, TerminateResult, Termination};

/// How often the client is looked for while the tab is open, as the C#
/// `HealthCheckIntervalMs`.
pub const HEALTH_INTERVAL: Duration = Duration::from_secs(3);

/// The program of Citrix Workspace's client, which runs the sessions.
pub const CLIENT_IMAGE: &str = "wfica32.exe";

/// Windows' process lister, in the system folder.
const TASKLIST_PROGRAM: &str = "tasklist.exe";
/// `tasklist.exe`'s CSV output, without its header, of the client's processes alone.
const TASKLIST_FORMAT: [&str; 3] = ["/FO", "CSV", "/NH"];
/// `tasklist.exe`'s filter switch.
const TASKLIST_FILTER: &str = "/FI";
/// The filter's condition, before the client's image name.
const TASKLIST_IMAGE_FILTER: &str = "IMAGENAME eq ";

/// What quotes a CSV field, and stands for itself doubled inside one.
const CSV_QUOTE: char = '"';
/// What separates CSV fields.
const CSV_SEPARATOR: char = ',';

/// How the launch time is shown, as the C# "Started" column of the tunnels.
const LAUNCHED_FORMAT: &str = "%H:%M:%S";

/// How long a listing of the client's processes may take: well above what `tasklist.exe`
/// takes on a loaded computer, under a second, and below what a user would take for a tab
/// stuck. Past it, `tasklist.exe` is killed and the listing given up for that look.
pub const LIST_TIME_LIMIT: Duration = Duration::from_secs(10);

/// How many looks in a row may fail to list the client's processes before the tab says
/// the client is not tracked any more: a passing failure is tried again at the next look.
pub const LIST_FAILURES_TO_UNTRACK: u32 = 3;

/// How often a process waited for within a limit is looked at: often enough to answer at
/// once, seldom enough to cost nothing.
pub const WAIT_STEP: Duration = Duration::from_millis(50);

/// The name of the thread a listing runs on.
const LISTER_THREAD: &str = "citrix-clients";

/// Process ids.
pub type Pids = BTreeSet<u32>;

/// Why the client's processes could not be listed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ListError {
    /// Not on Windows: there is no Citrix Workspace client to look for.
    Unsupported,
    /// The lister failed, and why.
    Failed(String),
    /// The lister did not answer within its time limit.
    TimedOut,
}

/// What lists the client's running processes: `tasklist.exe`, or another in tests.
pub trait ClientLister: Send + Sync {
    /// The ids of the client's running processes.
    ///
    /// # Errors
    ///
    /// [`ListError`] when they cannot be listed.
    fn client_pids(&self) -> Result<Pids, ListError>;
}

/// The client's processes, as Windows' `tasklist.exe` lists them.
#[derive(Debug, Clone, Copy, Default)]
pub struct Tasklist;

impl ClientLister for Tasklist {
    fn client_pids(&self) -> Result<Pids, ListError> {
        list_clients()
    }
}

/// The command listing the client's processes: `tasklist.exe` in `system_dir`, each
/// argument its own, no shell reading them.
#[must_use]
pub fn tasklist_command(system_dir: &Path) -> Command {
    let mut command = Command::new(system_dir.join(TASKLIST_PROGRAM));
    command
        .args(TASKLIST_FORMAT)
        .arg(TASKLIST_FILTER)
        .arg(format!("{TASKLIST_IMAGE_FILTER}{CLIENT_IMAGE}"));
    command
}

/// A process waited for within a time limit: the child started, or another in tests.
pub trait Waitable {
    /// Its exit status once it exited; `None` while it runs. Never waits.
    ///
    /// # Errors
    ///
    /// When its state cannot be read.
    fn try_wait(&mut self) -> std::io::Result<Option<std::process::ExitStatus>>;
    /// Ends it, then waits for it to be gone.
    fn kill_and_wait(&mut self);
}

impl Waitable for std::process::Child {
    fn try_wait(&mut self) -> std::io::Result<Option<std::process::ExitStatus>> {
        std::process::Child::try_wait(self)
    }

    fn kill_and_wait(&mut self) {
        // Already gone, it cannot be killed: waiting for it is what matters.
        let _ = self.kill();
        let _ = self.wait();
    }
}

/// Waits for `process` to exit within `limit`, looking every [`WAIT_STEP`]: its exit
/// status, or `None` once the limit is past, the process then killed and waited for.
///
/// # Errors
///
/// When its state cannot be read.
pub fn wait_within(
    process: &mut impl Waitable,
    limit: Duration,
) -> std::io::Result<Option<std::process::ExitStatus>> {
    let deadline = Instant::now() + limit;
    loop {
        if let Some(status) = process.try_wait()? {
            return Ok(Some(status));
        }
        if Instant::now() >= deadline {
            process.kill_and_wait();
            return Ok(None);
        }
        std::thread::sleep(WAIT_STEP);
    }
}

/// The client's processes, listed by `tasklist.exe` from the system folder Windows names,
/// within [`LIST_TIME_LIMIT`]: past it, `tasklist.exe` is killed.
#[cfg(windows)]
fn list_clients() -> Result<Pids, ListError> {
    use std::io::Read as _;
    use std::os::windows::process::CommandExt;
    use std::process::Stdio;
    let folder = heimdall_core::paths::system_dir()
        .ok_or_else(|| ListError::Failed("the system folder is unknown".to_owned()))?;
    let mut child = tasklist_command(&folder)
        .creation_flags(crate::citrix::CREATE_NO_WINDOW)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| ListError::Failed(error.to_string()))?;
    // Read as it is written, so a full pipe never holds it: the reading ends with it.
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| ListError::Failed("no output".to_owned()))?;
    let reader = std::thread::spawn(move || {
        let mut output = Vec::new();
        stdout.read_to_end(&mut output).map(|_| output)
    });
    let status = wait_within(&mut child, LIST_TIME_LIMIT);
    let output = reader.join();
    match status.map_err(|error| ListError::Failed(error.to_string()))? {
        None => Err(ListError::TimedOut),
        Some(status) if !status.success() => Err(ListError::Failed(format!(
            "tasklist ended with {:?}",
            status.code()
        ))),
        Some(_) => match output {
            Ok(Ok(output)) => Ok(parse_tasklist(&String::from_utf8_lossy(&output))),
            Ok(Err(error)) => Err(ListError::Failed(error.to_string())),
            Err(_) => Err(ListError::Failed("the output was not read".to_owned())),
        },
    }
}

/// The client's processes, listed by `lister` on a thread of its own and waited for
/// within `limit`: past it, the listing is given up for this time, said once in the log,
/// and the thread left to end by itself. `tasklist.exe` kills its own process at the same
/// limit.
///
/// # Errors
///
/// [`ListError::TimedOut`] past `limit`, else what `lister` answered.
pub fn list_within(lister: Arc<dyn ClientLister>, limit: Duration) -> Result<Pids, ListError> {
    let (sender, receiver) = std::sync::mpsc::channel();
    let spawned = std::thread::Builder::new()
        .name(LISTER_THREAD.to_owned())
        .spawn(move || {
            // Given up on already: nobody to tell.
            let _ = sender.send(lister.client_pids());
        });
    if let Err(error) = spawned {
        return Err(ListError::Failed(error.to_string()));
    }
    let answer = match receiver.recv_timeout(limit) {
        Ok(answer) => answer,
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => Err(ListError::TimedOut),
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
            Err(ListError::Failed("the lister stopped".to_owned()))
        }
    };
    if answer == Err(ListError::TimedOut) {
        log::warn!(
            "the Citrix clients were not listed within {} ms",
            limit.as_millis()
        );
    }
    answer
}

/// Citrix Workspace's client is a Windows program.
#[cfg(not(windows))]
fn list_clients() -> Result<Pids, ListError> {
    Err(ListError::Unsupported)
}

/// The client's process ids in `tasklist.exe`'s CSV output. A line that is not a CSV row
/// of the client with a process id is no process: the line saying no task matches, in the
/// language of Windows, among them.
#[must_use]
pub fn parse_tasklist(output: &str) -> Pids {
    output
        .lines()
        .filter_map(csv_fields)
        .filter_map(|fields| client_pid(&fields))
        .collect()
}

/// The fields of a CSV row whose fields are all quoted, as `tasklist.exe` writes them; a
/// doubled quote inside one stands for itself. `None` when `line` is not one.
fn csv_fields(line: &str) -> Option<Vec<String>> {
    let mut chars = line.trim().chars().peekable();
    let mut fields = Vec::new();
    loop {
        if chars.next() != Some(CSV_QUOTE) {
            return None;
        }
        let mut field = String::new();
        loop {
            match chars.next()? {
                CSV_QUOTE if chars.peek() == Some(&CSV_QUOTE) => {
                    chars.next();
                    field.push(CSV_QUOTE);
                }
                CSV_QUOTE => break,
                other => field.push(other),
            }
        }
        fields.push(field);
        match chars.next() {
            None => return Some(fields),
            Some(CSV_SEPARATOR) => {}
            Some(_) => return None,
        }
    }
}

/// The process id of a row of the client: its image name, then its id in digits.
fn client_pid(fields: &[String]) -> Option<u32> {
    let [image, pid, ..] = fields else {
        return None;
    };
    if !image.trim().eq_ignore_ascii_case(CLIENT_IMAGE)
        || pid.is_empty()
        || !pid.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    pid.parse().ok()
}

/// A client process running now that was not before the launch, nor is another tab's: the
/// lowest, when several are.
#[must_use]
pub fn new_client(before: &Pids, now: &Pids, claimed: &Pids) -> Option<u32> {
    now.iter()
        .find(|pid| !before.contains(pid) && !claimed.contains(pid))
        .copied()
}

/// Where the launcher stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LauncherStatus {
    /// Still running.
    Running,
    /// Exited, with its exit code when it has one.
    Exited(Option<i32>),
    /// Cannot be said.
    Unknown,
}

/// What says where the launcher stands: the process started, or another in tests.
pub trait LauncherWatch: Send + Sync {
    /// Where the launcher stands now; never waits.
    fn status(&self) -> LauncherStatus;
}

/// The launcher process started.
pub struct ChildWatch(Mutex<std::process::Child>);

impl ChildWatch {
    /// Watches `child`, shared with the probes.
    #[must_use]
    pub fn shared(child: std::process::Child) -> Arc<dyn LauncherWatch> {
        Arc::new(Self(Mutex::new(child)))
    }
}

impl LauncherWatch for ChildWatch {
    fn status(&self) -> LauncherStatus {
        let Ok(mut child) = self.0.lock() else {
            return LauncherStatus::Unknown;
        };
        match child.try_wait() {
            Ok(Some(status)) => LauncherStatus::Exited(status.code()),
            Ok(None) => LauncherStatus::Running,
            Err(_) => LauncherStatus::Unknown,
        }
    }
}

/// How the application was launched, as the C# info panel's "Mode" says it. The cache
/// launch line itself, a secret, is not kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaunchMethod {
    /// From Citrix Workspace's cache.
    CacheLine,
    /// From an ICA file.
    IcaFile,
    /// Asked of its `StoreFront`.
    StoreFront,
}

impl LaunchMethod {
    /// How `launch` launches.
    #[must_use]
    pub fn of(launch: &CitrixLaunch) -> Self {
        match launch {
            CitrixLaunch::CacheLine(_) => Self::CacheLine,
            CitrixLaunch::IcaFile(_) => Self::IcaFile,
            CitrixLaunch::StoreFront { .. } => Self::StoreFront,
        }
    }
}

/// Why the client is not tracked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Untracked {
    /// Not on Windows.
    WindowsOnly,
    /// Its processes could not be listed.
    Unavailable,
    /// Its processes were not listed within the time limit.
    TimedOut,
}

impl Untracked {
    /// Why listing failed with `error`.
    fn of(error: &ListError) -> Self {
        match error {
            ListError::Unsupported => Self::WindowsOnly,
            ListError::Failed(_) => Self::Unavailable,
            ListError::TimedOut => Self::TimedOut,
        }
    }
}

/// The state of the session's client.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientState {
    /// The launch is under way, or its client not looked for yet.
    Launching,
    /// The client of this session runs, with this process id.
    Running(u32),
    /// No client of this session is seen yet.
    NotFoundYet,
    /// The client of this session, with this process id, ended.
    Ended(u32),
    /// A client was running before the launch and none appeared: the session may run in
    /// it, among others, and cannot be told apart.
    Shared,
    /// The launcher exited with this error code, and no client appeared.
    LauncherFailed(i32),
    /// Nothing was launched, for this reason.
    NotStarted(CitrixRefusal),
    /// The client is not looked for.
    Untracked(Untracked),
}

impl ClientState {
    /// The client process of this session, once found.
    #[must_use]
    pub fn client(&self) -> Option<u32> {
        match self {
            Self::Running(pid) | Self::Ended(pid) => Some(*pid),
            _ => None,
        }
    }
}

/// What a probe saw.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Probe {
    /// Where the launcher stands.
    pub launcher: LauncherStatus,
    /// The client's processes running, or why they were not listed.
    pub clients: Result<Pids, ListError>,
}

/// Looks at `launcher`, and lists the client's processes with `lister` when given one,
/// within `limit`; never waits on the launcher.
#[must_use]
pub fn probe(
    lister: Option<Arc<dyn ClientLister>>,
    launcher: &dyn LauncherWatch,
    limit: Duration,
) -> Probe {
    Probe {
        launcher: launcher.status(),
        clients: lister.map_or(Err(ListError::Unsupported), |lister| {
            list_within(lister, limit)
        }),
    }
}

/// A launch done: the client's processes before it, the launcher, and when.
#[derive(Clone)]
pub struct Launched {
    /// The client's processes running before the launch, or why they were not listed.
    pub baseline: Result<Pids, ListError>,
    /// The launcher started.
    pub launcher: Arc<dyn LauncherWatch>,
    /// When it started.
    pub at: SystemTime,
}

impl std::fmt::Debug for Launched {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Launched")
            .field("baseline", &self.baseline)
            .finish_non_exhaustive()
    }
}

/// Lists the client's processes with `lister` within `limit`, then starts the launcher
/// with `start`: the processes before the launch, for the new one to be told apart.
///
/// # Errors
///
/// [`CitrixRefusal`] when `start` started nothing.
pub fn launch_tracked(
    lister: Arc<dyn ClientLister>,
    limit: Duration,
    start: impl FnOnce() -> Result<Arc<dyn LauncherWatch>, CitrixRefusal>,
) -> Result<Launched, CitrixRefusal> {
    let baseline = list_within(lister, limit);
    let launcher = start()?;
    Ok(Launched {
        baseline,
        launcher,
        at: SystemTime::now(),
    })
}

/// The client of a session followed from its launch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientTracker {
    /// The client's processes before the launch, once launched with them listed.
    baseline: Option<Pids>,
    /// The client's state.
    state: ClientState,
    /// Where the launcher stands.
    launcher: LauncherStatus,
    /// How many looks in a row failed to list the client's processes.
    failures: u32,
}

impl Default for ClientTracker {
    fn default() -> Self {
        Self {
            baseline: None,
            state: ClientState::Launching,
            launcher: LauncherStatus::Running,
            failures: 0,
        }
    }
}

impl ClientTracker {
    /// The client's state.
    #[must_use]
    pub fn state(&self) -> &ClientState {
        &self.state
    }

    /// Where the launcher stands.
    #[must_use]
    pub fn launcher(&self) -> LauncherStatus {
        self.launcher
    }

    /// The launcher's exit code, once it exited with one.
    #[must_use]
    pub fn exit_code(&self) -> Option<i32> {
        match self.launcher {
            LauncherStatus::Exited(code) => code,
            LauncherStatus::Running | LauncherStatus::Unknown => None,
        }
    }

    /// The launch was done, the client's processes before it being `baseline`; whether
    /// the state changed.
    pub fn launched(&mut self, baseline: Result<Pids, ListError>) -> bool {
        match baseline {
            Ok(pids) => {
                self.baseline = Some(pids);
                false
            }
            // No baseline, no new client to tell apart: not tracked at all.
            Err(error) => self.set(ClientState::Untracked(Untracked::of(&error))),
        }
    }

    /// Nothing was launched, for `refusal`; whether the state changed.
    pub fn refused(&mut self, refusal: CitrixRefusal) -> bool {
        self.launcher = LauncherStatus::Unknown;
        self.set(ClientState::NotStarted(refusal))
    }

    /// Whether a probe has anything to tell: the client may still change, or the launcher
    /// still runs.
    #[must_use]
    pub fn wants_probe(&self) -> bool {
        match self.state {
            ClientState::NotStarted(_) => false,
            ClientState::Launching => self.baseline.is_some(),
            ClientState::Ended(_) | ClientState::Untracked(_) => {
                self.launcher == LauncherStatus::Running
            }
            ClientState::Running(_)
            | ClientState::NotFoundYet
            | ClientState::Shared
            | ClientState::LauncherFailed(_) => true,
        }
    }

    /// What `probe` saw, `claimed` being the clients of the other tabs: the launcher
    /// once exited stays so, the client once found is followed until it ends, which is
    /// final. A listing that failed changes nothing, until [`LIST_FAILURES_TO_UNTRACK`]
    /// failed in a row: the client is then not tracked any more, which is final too.
    /// Whether the state changed.
    pub fn observe(&mut self, probe: &Probe, claimed: &Pids) -> bool {
        if self.launcher == LauncherStatus::Running {
            self.launcher = probe.launcher;
        }
        if self.baseline.is_none()
            || matches!(
                self.state,
                ClientState::Ended(_) | ClientState::Untracked(_) | ClientState::NotStarted(_)
            )
        {
            return false;
        }
        let clients = match &probe.clients {
            Ok(clients) => {
                self.failures = 0;
                clients
            }
            Err(ListError::Unsupported) => return false,
            Err(error) => return self.listing_failed(error),
        };
        let Some(baseline) = &self.baseline else {
            return false;
        };
        let next = match self.state {
            ClientState::Running(pid) if clients.contains(&pid) => ClientState::Running(pid),
            ClientState::Running(pid) => ClientState::Ended(pid),
            _ => match new_client(baseline, clients, claimed) {
                Some(pid) => ClientState::Running(pid),
                None => match self.launcher {
                    LauncherStatus::Exited(Some(code)) if code != 0 => {
                        ClientState::LauncherFailed(code)
                    }
                    _ if clients.iter().any(|pid| baseline.contains(pid)) => ClientState::Shared,
                    _ => ClientState::NotFoundYet,
                },
            },
        };
        self.set(next)
    }

    /// A look failed to list the client's processes with `error`: tried again at the
    /// next, until too many failed in a row. Whether the state changed.
    fn listing_failed(&mut self, error: &ListError) -> bool {
        self.failures = self.failures.saturating_add(1);
        if self.failures < LIST_FAILURES_TO_UNTRACK {
            return false;
        }
        self.set(ClientState::Untracked(Untracked::of(error)))
    }

    /// Sets the state; whether it changed.
    fn set(&mut self, state: ClientState) -> bool {
        let changed = self.state != state;
        self.state = state;
        changed
    }
}

/// A Citrix tab's status: how it launched, when, and its client.
pub struct CitrixPane {
    /// How it launched.
    pub method: LaunchMethod,
    /// When the launcher started, once it did.
    pub launched_at: Option<SystemTime>,
    /// Its client.
    pub tracker: ClientTracker,
    /// The launcher, once started.
    launcher: Option<Arc<dyn LauncherWatch>>,
    /// A probe is under way: none other is started before it answers.
    probing: bool,
    /// The last request to end its client, once the user asked one.
    termination: Option<Termination>,
}

impl std::fmt::Debug for CitrixPane {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CitrixPane")
            .field("method", &self.method)
            .field("tracker", &self.tracker)
            .field("termination", &self.termination)
            .finish_non_exhaustive()
    }
}

impl CitrixPane {
    /// A launch by `method`, under way.
    #[must_use]
    pub fn new(method: LaunchMethod) -> Self {
        Self {
            method,
            launched_at: None,
            tracker: ClientTracker::default(),
            launcher: None,
            probing: false,
            termination: None,
        }
    }

    /// The launch was done; whether the state changed.
    pub fn launched(&mut self, launched: Launched) -> bool {
        self.launched_at = Some(launched.at);
        self.launcher = Some(launched.launcher);
        self.tracker.launched(launched.baseline)
    }

    /// When the launcher started, in this computer's time.
    #[must_use]
    pub fn launched_clock(&self) -> Option<String> {
        self.launched_at.map(|at| {
            chrono::DateTime::<chrono::Local>::from(at)
                .format(LAUNCHED_FORMAT)
                .to_string()
        })
    }

    /// Whether a probe would be started now.
    #[must_use]
    pub fn polls(&self) -> bool {
        !self.probing && self.launcher.is_some() && self.tracker.wants_probe()
    }

    /// Starts a probe, when one is due: the launcher to look at, and whether the client's
    /// processes are listed.
    pub fn start_probe(&mut self) -> Option<(Arc<dyn LauncherWatch>, bool)> {
        if !self.polls() {
            return None;
        }
        let launcher = Arc::clone(self.launcher.as_ref()?);
        self.probing = true;
        let lists = !matches!(self.tracker.state(), ClientState::Untracked(_));
        Some((launcher, lists))
    }

    /// A probe answered; whether the client's state changed.
    pub fn probed(&mut self, probe: &Probe, claimed: &Pids) -> bool {
        self.probing = false;
        self.tracker.observe(probe, claimed)
    }

    /// What is offered at `now` to end the session, `claimed` being the clients of the
    /// other tabs: only for a client this tab found itself and still running, never a
    /// shared one, another tab's, or one not tracked.
    #[must_use]
    pub fn terminate_offer(&self, claimed: &Pids, now: Instant) -> TerminateOffer {
        let ClientState::Running(pid) = *self.tracker.state() else {
            return TerminateOffer::Nothing;
        };
        if claimed.contains(&pid) {
            return TerminateOffer::Nothing;
        }
        match &self.termination {
            Some(termination) if termination.pid() == pid => termination.offer(now),
            _ => TerminateOffer::Terminate,
        }
    }

    /// The user confirmed ending client `pid`, forced when `force`, at `now`.
    pub fn terminate_started(&mut self, pid: u32, force: bool, now: Instant) {
        self.termination = Some(Termination::started(pid, force, now));
    }

    /// `taskkill.exe` answered `result` for client `pid`, at `now`; whether it was the
    /// request under way.
    pub fn terminate_answered(&mut self, pid: u32, result: TerminateResult, now: Instant) -> bool {
        match &mut self.termination {
            Some(termination) if termination.pid() == pid => {
                termination.answered(result, now);
                true
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pids(list: &[u32]) -> Pids {
        list.iter().copied().collect()
    }

    #[test]
    fn the_tasklist_rows_give_the_client_process_ids() {
        let output = "\"wfica32.exe\",\"4242\",\"Console\",\"1\",\"52 344 K\"\r\n\
                      \"WFICA32.EXE\",\"17\",\"Services\",\"0\",\"1,024 K\"\r\n";
        assert_eq!(parse_tasklist(output), pids(&[17, 4242]));
        // A doubled quote inside a field is the field's own.
        let quoted = "\"wfica32.exe\",\"99\",\"Con\"\"sole\",\"1\",\"9 K\"";
        assert_eq!(parse_tasklist(quoted), pids(&[99]));
    }

    #[test]
    fn the_line_saying_no_task_matches_is_no_process_in_any_language() {
        for line in [
            "INFO: No tasks are running which match the specified criteria.",
            "Information : aucune t\u{e2}che en service ne correspond aux crit\u{e8}res sp\u{e9}cifi\u{e9}s.",
            "",
        ] {
            assert!(parse_tasklist(line).is_empty(), "{line}");
        }
    }

    #[test]
    fn garbage_is_no_process() {
        for line in [
            "\"wfica32.exe\",\"12a\",\"Console\"",
            "\"wfica32.exe\",\"\",\"Console\"",
            "\"wfica32.exe\",\"-1\"",
            "\"wfica32.exe\",\"99999999999\"",
            "\"other.exe\",\"12\",\"Console\"",
            "\"wfica32.exe\"",
            "\"wfica32.exe\",\"12",
            "\"wfica32.exe\"x,\"12\"",
            "wfica32.exe,12,Console",
            "\u{fffd}\u{fffd},\"12\"",
        ] {
            assert!(parse_tasklist(line).is_empty(), "{line}");
        }
    }

    #[test]
    fn a_new_client_is_one_not_running_before_nor_another_tabs() {
        let before = pids(&[10, 20]);
        assert_eq!(new_client(&before, &pids(&[10, 20]), &Pids::new()), None);
        assert_eq!(
            new_client(&before, &pids(&[10, 30, 25]), &Pids::new()),
            Some(25)
        );
        assert_eq!(
            new_client(&before, &pids(&[10, 30, 25]), &pids(&[25])),
            Some(30)
        );
        assert_eq!(new_client(&Pids::new(), &Pids::new(), &Pids::new()), None);
    }

    #[test]
    fn the_command_is_tasklist_from_the_system_folder_with_its_own_arguments() {
        let folder = Path::new("/windows/system32");
        let command = tasklist_command(folder);
        assert_eq!(
            Path::new(command.get_program()),
            folder.join("tasklist.exe").as_path()
        );
        let arguments: Vec<_> = command
            .get_args()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            arguments,
            ["/FO", "CSV", "/NH", "/FI", "IMAGENAME eq wfica32.exe"]
        );
    }

    struct Fake(Result<Pids, ListError>);

    impl ClientLister for Fake {
        fn client_pids(&self) -> Result<Pids, ListError> {
            self.0.clone()
        }
    }

    struct Watch(LauncherStatus);

    impl LauncherWatch for Watch {
        fn status(&self) -> LauncherStatus {
            self.0
        }
    }

    /// A limit short enough for the tests to wait it out.
    const SHORT_LIMIT: Duration = Duration::from_millis(30);
    /// Longer than [`SHORT_LIMIT`], for a lister answering too late.
    const LATE: Duration = Duration::from_millis(300);

    /// A lister answering after `LATE`.
    struct Late;

    impl ClientLister for Late {
        fn client_pids(&self) -> Result<Pids, ListError> {
            std::thread::sleep(LATE);
            Ok(pids(&[1]))
        }
    }

    /// A lister that answers only once the test is over, its sender dropped.
    struct Stuck(Mutex<std::sync::mpsc::Receiver<()>>);

    impl ClientLister for Stuck {
        fn client_pids(&self) -> Result<Pids, ListError> {
            let _ = self.0.lock().map(|receiver| receiver.recv());
            Ok(Pids::new())
        }
    }

    #[test]
    fn a_probe_lists_only_when_asked() {
        let launcher = Watch(LauncherStatus::Exited(Some(0)));
        assert_eq!(
            probe(
                Some(Arc::new(Fake(Ok(pids(&[7]))))),
                &launcher,
                LIST_TIME_LIMIT
            ),
            Probe {
                launcher: LauncherStatus::Exited(Some(0)),
                clients: Ok(pids(&[7])),
            }
        );
        assert_eq!(
            probe(None, &launcher, LIST_TIME_LIMIT).clients,
            Err(ListError::Unsupported)
        );
    }

    #[test]
    fn a_listing_past_its_limit_is_given_up() {
        assert_eq!(
            list_within(Arc::new(Late), SHORT_LIMIT),
            Err(ListError::TimedOut)
        );
        let (sender, receiver) = std::sync::mpsc::channel();
        let stuck = Arc::new(Stuck(Mutex::new(receiver)));
        assert_eq!(
            probe(Some(stuck), &Watch(LauncherStatus::Running), SHORT_LIMIT).clients,
            Err(ListError::TimedOut)
        );
        drop(sender);
        assert_eq!(
            list_within(Arc::new(Fake(Ok(pids(&[3])))), LIST_TIME_LIMIT),
            Ok(pids(&[3]))
        );
    }

    /// A process exiting once looked at `runs` times, or never.
    struct Process {
        runs: Option<u32>,
        looked: u32,
        killed: u32,
    }

    impl Waitable for Process {
        fn try_wait(&mut self) -> std::io::Result<Option<std::process::ExitStatus>> {
            self.looked += 1;
            Ok(self
                .runs
                .filter(|runs| self.looked > *runs)
                .map(|_| std::process::ExitStatus::default()))
        }

        fn kill_and_wait(&mut self) {
            self.killed += 1;
        }
    }

    #[test]
    fn a_process_past_its_limit_is_killed_and_waited_for() {
        let mut stuck = Process {
            runs: None,
            looked: 0,
            killed: 0,
        };
        assert!(
            wait_within(&mut stuck, SHORT_LIMIT)
                .expect("read")
                .is_none()
        );
        assert_eq!(stuck.killed, 1);
        assert!(stuck.looked > 1, "looked at in steps, not once");

        let mut quick = Process {
            runs: Some(1),
            looked: 0,
            killed: 0,
        };
        assert!(
            wait_within(&mut quick, LIST_TIME_LIMIT)
                .expect("read")
                .is_some()
        );
        assert_eq!(quick.killed, 0);
    }

    #[test]
    fn the_clients_are_listed_before_the_launcher_starts() {
        struct Listing(Arc<Mutex<Vec<&'static str>>>);
        impl ClientLister for Listing {
            fn client_pids(&self) -> Result<Pids, ListError> {
                self.0.lock().expect("lock").push("list");
                Ok(pids(&[5]))
            }
        }
        let order = Arc::new(Mutex::new(Vec::new()));
        let lister = Arc::new(Listing(Arc::clone(&order)));
        let launched = launch_tracked(lister, LIST_TIME_LIMIT, || {
            order.lock().expect("lock").push("start");
            Ok(Arc::new(Watch(LauncherStatus::Running)) as Arc<dyn LauncherWatch>)
        })
        .expect("launched");
        assert_eq!(*order.lock().expect("lock"), ["list", "start"]);
        assert_eq!(launched.baseline, Ok(pids(&[5])));

        let refused = launch_tracked(Arc::new(Fake(Ok(Pids::new()))), LIST_TIME_LIMIT, || {
            Err(CitrixRefusal::WorkspaceNotFound)
        });
        assert!(matches!(refused, Err(CitrixRefusal::WorkspaceNotFound)));

        // A listing past its limit still launches, untracked.
        let late = launch_tracked(Arc::new(Late), SHORT_LIMIT, || {
            Ok(Arc::new(Watch(LauncherStatus::Running)) as Arc<dyn LauncherWatch>)
        })
        .expect("launched");
        assert_eq!(late.baseline, Err(ListError::TimedOut));
        let mut tracker = ClientTracker::default();
        assert!(tracker.launched(late.baseline));
        assert_eq!(
            tracker.state(),
            &ClientState::Untracked(Untracked::TimedOut)
        );
    }

    #[test]
    fn listings_that_time_out_change_nothing_until_too_many_in_a_row() {
        let timed_out = Probe {
            launcher: LauncherStatus::Running,
            clients: Err(ListError::TimedOut),
        };
        let none = Pids::new();
        let mut tracker = ClientTracker::default();
        tracker.launched(Ok(Pids::new()));
        tracker.observe(&seen(LauncherStatus::Running, &[4]), &none);
        for _ in 1..LIST_FAILURES_TO_UNTRACK {
            assert!(!tracker.observe(&timed_out, &none));
            assert_eq!(tracker.state(), &ClientState::Running(4));
        }
        // A listing that answers starts the count again.
        assert!(!tracker.observe(&seen(LauncherStatus::Running, &[4]), &none));
        for _ in 1..LIST_FAILURES_TO_UNTRACK {
            assert!(!tracker.observe(&timed_out, &none));
        }
        assert!(tracker.observe(&timed_out, &none));
        assert_eq!(
            tracker.state(),
            &ClientState::Untracked(Untracked::TimedOut)
        );
        // Final: a listing answering again changes nothing.
        assert!(!tracker.observe(&seen(LauncherStatus::Running, &[4]), &none));
        assert!(tracker.wants_probe(), "the launcher still runs");

        let failed = Probe {
            launcher: LauncherStatus::Running,
            clients: Err(ListError::Failed("denied".to_owned())),
        };
        let mut other = ClientTracker::default();
        other.launched(Ok(Pids::new()));
        for _ in 0..LIST_FAILURES_TO_UNTRACK {
            other.observe(&failed, &none);
        }
        assert_eq!(
            other.state(),
            &ClientState::Untracked(Untracked::Unavailable)
        );
    }

    fn seen(launcher: LauncherStatus, clients: &[u32]) -> Probe {
        Probe {
            launcher,
            clients: Ok(pids(clients)),
        }
    }

    #[test]
    fn the_client_is_found_followed_and_ends() {
        let mut tracker = ClientTracker::default();
        assert_eq!(tracker.state(), &ClientState::Launching);
        assert!(!tracker.wants_probe(), "nothing launched yet");
        assert!(!tracker.launched(Ok(Pids::new())));
        assert!(tracker.wants_probe());
        let none = Pids::new();

        assert!(tracker.observe(&seen(LauncherStatus::Running, &[]), &none));
        assert_eq!(tracker.state(), &ClientState::NotFoundYet);
        assert!(tracker.observe(&seen(LauncherStatus::Exited(Some(0)), &[42]), &none));
        assert_eq!(tracker.state(), &ClientState::Running(42));
        assert_eq!(tracker.exit_code(), Some(0));
        // Another client appearing changes nothing: this one is followed.
        assert!(!tracker.observe(&seen(LauncherStatus::Running, &[42, 43]), &none));
        assert_eq!(tracker.launcher(), LauncherStatus::Exited(Some(0)));
        assert!(tracker.observe(&seen(LauncherStatus::Running, &[43]), &none));
        assert_eq!(tracker.state(), &ClientState::Ended(42));
        // Ended is final, the same id coming back included.
        assert!(!tracker.observe(&seen(LauncherStatus::Running, &[42]), &none));
        assert_eq!(tracker.state(), &ClientState::Ended(42));
        assert!(
            !tracker.wants_probe(),
            "the launcher exited, the client ended"
        );
    }

    #[test]
    fn a_failed_launcher_with_no_client_is_said() {
        let mut tracker = ClientTracker::default();
        tracker.launched(Ok(Pids::new()));
        assert!(tracker.observe(&seen(LauncherStatus::Exited(Some(3)), &[]), &Pids::new()));
        assert_eq!(tracker.state(), &ClientState::LauncherFailed(3));
        // A client appearing after all is followed.
        assert!(tracker.observe(&seen(LauncherStatus::Running, &[8]), &Pids::new()));
        assert_eq!(tracker.state(), &ClientState::Running(8));
    }

    #[test]
    fn a_client_running_before_the_launch_is_shared() {
        let mut tracker = ClientTracker::default();
        tracker.launched(Ok(pids(&[11])));
        assert!(tracker.observe(&seen(LauncherStatus::Exited(Some(0)), &[11]), &Pids::new()));
        assert_eq!(tracker.state(), &ClientState::Shared);
        // A client of its own appearing later is this session's.
        assert!(tracker.observe(&seen(LauncherStatus::Running, &[11, 12]), &Pids::new()));
        assert_eq!(tracker.state(), &ClientState::Running(12));

        // Another tab's client is never taken.
        let mut other = ClientTracker::default();
        other.launched(Ok(pids(&[11])));
        other.observe(&seen(LauncherStatus::Running, &[11, 12]), &pids(&[12]));
        assert_eq!(other.state(), &ClientState::Shared);
    }

    #[test]
    fn a_launch_untracked_or_refused_says_why() {
        let mut tracker = ClientTracker::default();
        assert!(tracker.launched(Err(ListError::Unsupported)));
        assert_eq!(
            tracker.state(),
            &ClientState::Untracked(Untracked::WindowsOnly)
        );
        assert!(
            tracker.wants_probe(),
            "the launcher's exit code is still read"
        );
        assert!(!tracker.observe(&seen(LauncherStatus::Exited(Some(1)), &[9]), &Pids::new()));
        assert_eq!(tracker.exit_code(), Some(1));
        assert!(!tracker.wants_probe());

        let mut failed = ClientTracker::default();
        assert!(failed.launched(Err(ListError::Failed("denied".to_owned()))));
        assert_eq!(
            failed.state(),
            &ClientState::Untracked(Untracked::Unavailable)
        );

        let mut refused = ClientTracker::default();
        assert!(refused.refused(CitrixRefusal::WorkspaceNotFound));
        assert!(!refused.wants_probe());
    }

    #[test]
    fn a_listing_that_failed_changes_nothing() {
        let mut tracker = ClientTracker::default();
        tracker.launched(Ok(Pids::new()));
        tracker.observe(&seen(LauncherStatus::Running, &[4]), &Pids::new());
        let failed = Probe {
            launcher: LauncherStatus::Running,
            clients: Err(ListError::Failed("busy".to_owned())),
        };
        assert!(!tracker.observe(&failed, &Pids::new()));
        assert_eq!(tracker.state(), &ClientState::Running(4));
    }

    #[test]
    fn a_pane_probes_once_at_a_time() {
        let mut pane = CitrixPane::new(LaunchMethod::StoreFront);
        assert!(!pane.polls(), "not launched");
        assert!(pane.start_probe().is_none());
        pane.launched(Launched {
            baseline: Ok(Pids::new()),
            launcher: Arc::new(Watch(LauncherStatus::Running)),
            at: SystemTime::UNIX_EPOCH,
        });
        assert!(pane.launched_clock().is_some());
        let (_, lists) = pane.start_probe().expect("a probe");
        assert!(lists);
        assert!(pane.start_probe().is_none(), "one under way");
        pane.probed(&seen(LauncherStatus::Running, &[1]), &Pids::new());
        assert!(pane.polls());
    }

    #[test]
    fn terminate_is_never_offered_for_another_tabs_client() {
        let now = Instant::now();
        let mut pane = CitrixPane::new(LaunchMethod::StoreFront);
        assert_eq!(
            pane.terminate_offer(&Pids::new(), now),
            TerminateOffer::Nothing
        );
        pane.launched(Launched {
            baseline: Ok(Pids::new()),
            launcher: Arc::new(Watch(LauncherStatus::Running)),
            at: SystemTime::UNIX_EPOCH,
        });
        pane.probed(&seen(LauncherStatus::Running, &[4]), &Pids::new());
        assert_eq!(
            pane.terminate_offer(&Pids::new(), now),
            TerminateOffer::Terminate
        );
        assert_eq!(
            pane.terminate_offer(&pids(&[4]), now),
            TerminateOffer::Nothing
        );
        pane.terminate_started(4, false, now);
        assert!(!pane.terminate_answered(5, TerminateResult::Requested, now));
        assert!(pane.terminate_answered(4, TerminateResult::Requested, now));
        assert_eq!(
            pane.terminate_offer(&Pids::new(), now),
            TerminateOffer::Asked { force: false }
        );
    }
}
