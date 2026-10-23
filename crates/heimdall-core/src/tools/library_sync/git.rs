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

//! The installed `git` program, run as a child process.
//!
//! Nothing here goes through a shell: a command is a program path and an array of arguments
//! (`std::process::Command`), so no value is ever parsed as shell syntax. Every value a person
//! typed (the URL, the branch) comes after a `--` where git has one, and is checked before
//! (see `settings`): a value that starts with `-` is refused, never passed.
//!
//! What git is allowed to do is narrowed on every call: only the `https`, `ssh` and `file`
//! transports (the `ext::` and `fd::` helpers run a command), no hooks, no prompt that waits
//! for a person who is not looking at a terminal, no editor, a time limit on each call, and
//! objects received checked for corruption.

use std::ffi::{OsStr, OsString};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use super::SyncFailure;
use super::settings::mask_credentials;

/// How long a call that touches only this computer may take.
pub const LOCAL_TIMEOUT: Duration = Duration::from_secs(30);

/// How long a call that talks to the remote may take: a clone of a large repository over a
/// slow link is the longest.
pub const NETWORK_TIMEOUT: Duration = Duration::from_secs(120);

/// How long the global identity is waited for (`GitConfigIdentityReader.ReadTimeout`).
pub const IDENTITY_TIMEOUT: Duration = Duration::from_secs(2);

/// How often a running call is looked at.
const POLL_INTERVAL: Duration = Duration::from_millis(20);

/// How long the output of a call that has ended is waited for: a process it started may still
/// hold the pipe.
const OUTPUT_GRACE: Duration = Duration::from_secs(2);

/// The most output kept of a call that is not asked for a file.
pub const SMALL_OUTPUT: usize = 64 * 1024;

/// The most of git's complaint kept, and shown.
const MAX_DETAIL_CHARS: usize = 400;

/// The bytes read from a pipe at a time.
const READ_CHUNK: usize = 8 * 1024;

/// A process created without a console window of its own.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// The transports git may use.
pub const ALLOWED_PROTOCOLS: &str = "https:ssh:file";

/// The ssh command used unless the person has set their own: never asks anything (a pass
/// phrase, an unknown host key) on a terminal nobody sees.
const DEFAULT_SSH_COMMAND: &str = "ssh -o BatchMode=yes";

/// The variable that names it.
const SSH_COMMAND_VARIABLE: &str = "GIT_SSH_COMMAND";

/// The variables set for every call.
const ENVIRONMENT: [(&str, &str); 8] = [
    // No username or password asked on a terminal.
    ("GIT_TERMINAL_PROMPT", "0"),
    // Nor by the credential manager's window.
    ("GCM_INTERACTIVE", "never"),
    ("GIT_ALLOW_PROTOCOL", ALLOWED_PROTOCOLS),
    // No editor: a message is always given, and a merge is never left to an editor.
    ("GIT_EDITOR", "true"),
    ("GIT_SEQUENCE_EDITOR", "true"),
    ("GIT_OPTIONAL_LOCKS", "0"),
    // The same words whatever the language of the computer, which the failures are read by.
    ("LC_ALL", "C"),
    ("LANGUAGE", "C"),
];

/// The variables of the caller that would point git at another repository than the folder
/// it is run in.
const REMOVED_ENVIRONMENT: [&str; 9] = [
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_OBJECT_DIRECTORY",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_COMMON_DIR",
    "GIT_NAMESPACE",
    "GIT_CONFIG_PARAMETERS",
    "GIT_CONFIG_COUNT",
];

/// The configuration passed on every call (`git -c key=value`).
const FIXED_CONFIGURATION: [&str; 8] = [
    // The helpers of the `ext::` transport run a command.
    "protocol.ext.allow=never",
    // A configured file system monitor is a program git starts.
    "core.fsmonitor=false",
    // The bytes of the library are the bytes of the file.
    "core.autocrlf=false",
    "core.symlinks=false",
    // What is received is checked before it is kept.
    "transfer.fsckObjects=true",
    // A signing key asks for a pass phrase.
    "commit.gpgsign=false",
    "submodule.recurse=false",
    "fetch.recurseSubmodules=false",
];

/// The `git` found on the path, looked for once.
///
/// The directories of `PATH` are searched here, and only the absolute ones: on Windows a bare
/// `Command::new("git")` looks in the current directory first, where a planted `git.exe`
/// would run.
#[must_use]
pub fn program() -> Option<&'static Path> {
    static FOUND: OnceLock<Option<PathBuf>> = OnceLock::new();
    FOUND
        .get_or_init(|| {
            let path = std::env::var_os("PATH")?;
            resolve_in(&path, cfg!(windows))
        })
        .as_deref()
}

/// The `git` of the first absolute directory of `path_variable` that holds one.
#[must_use]
pub fn resolve_in(path_variable: &OsStr, windows: bool) -> Option<PathBuf> {
    let name = if windows { "git.exe" } else { "git" };
    std::env::split_paths(path_variable)
        .filter(|directory| directory.is_absolute())
        .map(|directory| directory.join(name))
        .find(|candidate| candidate.is_file())
}

/// What a call said.
#[derive(Debug)]
pub struct Output {
    /// Its exit code; `None` when a signal ended it.
    pub code: Option<i32>,
    /// What it wrote, cut at the cap it was run with.
    pub stdout: Vec<u8>,
    /// Whether `stdout` is cut.
    pub truncated: bool,
    /// What it complained of.
    pub stderr: String,
}

impl Output {
    /// Whether it succeeded.
    #[must_use]
    pub fn ok(&self) -> bool {
        self.code == Some(0)
    }

    /// The first line of its output, trimmed.
    #[must_use]
    pub fn first_line(&self) -> String {
        String::from_utf8_lossy(&self.stdout)
            .lines()
            .next()
            .unwrap_or_default()
            .trim()
            .to_owned()
    }
}

/// The configuration of every call: `-c key=value` for each, then the author when given.
#[must_use]
pub fn global_arguments(hooks: &Path, identity: Option<(&str, &str)>) -> Vec<OsString> {
    let mut arguments: Vec<OsString> = Vec::new();
    let mut set = |setting: OsString| {
        arguments.push(OsString::from("-c"));
        arguments.push(setting);
    };
    // An empty folder: no hook of the repository (or of the user's global setting) runs.
    let mut hooks_setting = OsString::from("core.hooksPath=");
    hooks_setting.push(hooks);
    set(hooks_setting);
    for setting in FIXED_CONFIGURATION {
        set(OsString::from(setting));
    }
    if let Some((name, email)) = identity {
        set(OsString::from(format!("user.name={name}")));
        set(OsString::from(format!("user.email={email}")));
    }
    arguments
}

/// What is set and removed in the environment of every call; `ssh_command_given` says whether
/// the person already has a `GIT_SSH_COMMAND`, which is then left alone.
#[must_use]
pub fn environment(
    ssh_command_given: bool,
) -> (Vec<(&'static str, &'static str)>, Vec<&'static str>) {
    let mut set: Vec<(&'static str, &'static str)> = ENVIRONMENT.to_vec();
    if !ssh_command_given {
        set.push((SSH_COMMAND_VARIABLE, DEFAULT_SSH_COMMAND));
    }
    (set, REMOVED_ENVIRONMENT.to_vec())
}

/// The arguments of each call, built here and nowhere else. A value a person typed is always
/// after a `--`; what comes before one is fixed text.
pub mod args {
    use std::ffi::OsString;

    use super::super::REPOSITORY_FILE;

    /// The remote's name.
    pub const REMOTE: &str = "origin";

    /// What the last fetch is called.
    pub const FETCHED: &str = "FETCH_HEAD";

    /// The prefix of a branch's full name.
    pub const BRANCH_PREFIX: &str = "refs/heads/";

    fn of(parts: &[&str]) -> Vec<OsString> {
        parts.iter().map(OsString::from).collect()
    }

    /// The full name of `branch`.
    #[must_use]
    pub fn branch_ref(branch: &str) -> String {
        format!("{BRANCH_PREFIX}{branch}")
    }

    /// A new repository in the folder.
    #[must_use]
    pub fn init() -> Vec<OsString> {
        of(&["init", "--quiet"])
    }

    /// The branch the repository starts on, whatever git's default is.
    #[must_use]
    pub fn point_head(branch: &str) -> Vec<OsString> {
        let mut arguments = of(&["symbolic-ref", "HEAD"]);
        arguments.push(branch_ref(branch).into());
        arguments
    }

    /// The remote `origin` at `url`.
    #[must_use]
    pub fn add_origin(url: &str) -> Vec<OsString> {
        let mut arguments = of(&["remote", "add", "--", REMOTE]);
        arguments.push(url.into());
        arguments
    }

    /// The URL `origin` was added with, as written (not as an `insteadOf` rule rewrites it).
    #[must_use]
    pub fn origin_url() -> Vec<OsString> {
        of(&["config", "--get", "remote.origin.url"])
    }

    /// The branch the folder is on.
    #[must_use]
    pub fn current_branch() -> Vec<OsString> {
        of(&["symbolic-ref", "--short", "-q", "HEAD"])
    }

    /// Whether `branch` exists on the remote: exit code 0 when it does, 2 when it does not.
    #[must_use]
    pub fn remote_branch(branch: &str) -> Vec<OsString> {
        let mut arguments = of(&["ls-remote", "--exit-code", "--heads", "--", REMOTE]);
        arguments.push(branch_ref(branch).into());
        arguments
    }

    /// Fetches `branch` into `FETCH_HEAD`, nothing else: no tags, no submodules.
    #[must_use]
    pub fn fetch(branch: &str) -> Vec<OsString> {
        let mut arguments = of(&[
            "fetch",
            "--quiet",
            "--no-tags",
            "--no-recurse-submodules",
            "--",
            REMOTE,
        ]);
        arguments.push(branch_ref(branch).into());
        arguments
    }

    /// Whether the folder has a first commit yet.
    #[must_use]
    pub fn head_exists() -> Vec<OsString> {
        of(&["rev-parse", "--verify", "--quiet", "HEAD"])
    }

    /// How many commits the remote has that the folder lacks.
    #[must_use]
    pub fn behind_count() -> Vec<OsString> {
        of(&["rev-list", "--count", "HEAD..FETCH_HEAD", "--"])
    }

    /// How many commits the remote has, for a folder without one.
    #[must_use]
    pub fn fetched_count() -> Vec<OsString> {
        of(&["rev-list", "--count", "FETCH_HEAD", "--"])
    }

    /// How many commits the folder has that the remote lacks.
    #[must_use]
    pub fn ahead_count() -> Vec<OsString> {
        of(&["rev-list", "--count", "FETCH_HEAD..HEAD", "--"])
    }

    /// Whether everything the remote has is in the folder: exit code 0 when it is.
    #[must_use]
    pub fn contains_fetched() -> Vec<OsString> {
        of(&["merge-base", "--is-ancestor", "FETCH_HEAD", "HEAD"])
    }

    fn fetched_file() -> String {
        format!("{FETCHED}:{REPOSITORY_FILE}")
    }

    /// The size of the library file in the last fetch; fails when it has none.
    #[must_use]
    pub fn fetched_size() -> Vec<OsString> {
        let mut arguments = of(&["cat-file", "-s"]);
        arguments.push(fetched_file().into());
        arguments
    }

    /// The library file in the last fetch, read from the object database: nothing is checked
    /// out, so nothing it holds can act on the folder.
    #[must_use]
    pub fn fetched_content() -> Vec<OsString> {
        let mut arguments = of(&["cat-file", "blob"]);
        arguments.push(fetched_file().into());
        arguments
    }

    /// Brings the folder to the last fetch when that is a straight line forward.
    #[must_use]
    pub fn fast_forward() -> Vec<OsString> {
        of(&["merge", "--ff-only", "--quiet", "FETCH_HEAD"])
    }

    /// Brings the branch to the last fetch when the folder has a commit the remote lacks (a
    /// push that failed): the commit only held an export of the library, which is still here.
    /// The files of the folder are not touched.
    #[must_use]
    pub fn move_to_fetched() -> Vec<OsString> {
        of(&["reset", "--quiet", "--mixed", "FETCH_HEAD", "--"])
    }

    /// Stages the library file.
    #[must_use]
    pub fn stage() -> Vec<OsString> {
        of(&["add", "--", REPOSITORY_FILE])
    }

    /// Whether the library file differs from the last commit: exit code 1 when it does.
    #[must_use]
    pub fn staged_changes() -> Vec<OsString> {
        of(&["diff", "--cached", "--quiet", "--", REPOSITORY_FILE])
    }

    /// Commits the library file with `message`.
    #[must_use]
    pub fn commit(message: &str) -> Vec<OsString> {
        let mut arguments = of(&["commit", "--quiet", "-m"]);
        arguments.push(message.into());
        arguments.extend(of(&["--", REPOSITORY_FILE]));
        arguments
    }

    /// Pushes the folder's commit to `branch` of the remote, never forced.
    #[must_use]
    pub fn push(branch: &str) -> Vec<OsString> {
        let mut arguments = of(&["push", "--quiet", "--no-verify", "--", REMOTE]);
        arguments.push(format!("HEAD:{}", branch_ref(branch)).into());
        arguments
    }

    /// A value of the global configuration.
    #[must_use]
    pub fn global_value(key: &str) -> Vec<OsString> {
        let mut arguments = of(&["config", "--global", "--get"]);
        arguments.push(key.into());
        arguments
    }
}

/// A folder `git` is run in, with the configuration every call carries.
pub struct Runner<'a> {
    program: &'a Path,
    folder: &'a Path,
    global: Vec<OsString>,
}

impl<'a> Runner<'a> {
    /// A runner of `program` in `folder`.
    #[must_use]
    pub fn new(
        program: &'a Path,
        folder: &'a Path,
        hooks: &Path,
        identity: Option<(&str, &str)>,
    ) -> Self {
        Self {
            program,
            folder,
            global: global_arguments(hooks, identity),
        }
    }

    /// Runs git with `arguments`, waiting at most `timeout`, keeping at most `cap` bytes of
    /// what it writes.
    ///
    /// # Errors
    ///
    /// [`SyncFailure::Io`] when it cannot be started, [`SyncFailure::TimedOut`] when it does
    /// not end in time (it is then killed).
    pub fn run(
        &self,
        arguments: &[OsString],
        timeout: Duration,
        cap: usize,
    ) -> Result<Output, SyncFailure> {
        let mut command = Command::new(self.program);
        command
            .args(&self.global)
            .args(arguments)
            .current_dir(self.folder)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let (set, removed) = environment(std::env::var_os(SSH_COMMAND_VARIABLE).is_some());
        for name in removed {
            command.env_remove(name);
        }
        for (name, value) in set {
            command.env(name, value);
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(CREATE_NO_WINDOW);
        }
        let mut child = command
            .spawn()
            .map_err(|error| SyncFailure::Io(error.to_string()))?;
        let stdout = spawn_reader(child.stdout.take(), cap);
        let stderr = spawn_reader(child.stderr.take(), SMALL_OUTPUT);
        let deadline = Instant::now() + timeout;
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) if Instant::now() >= deadline => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(SyncFailure::TimedOut);
                }
                Ok(None) => thread::sleep(POLL_INTERVAL),
                Err(error) => {
                    let _ = child.kill();
                    return Err(SyncFailure::Io(error.to_string()));
                }
            }
        };
        let (stdout, truncated) = stdout.recv_timeout(OUTPUT_GRACE).unwrap_or_default();
        let (stderr, _) = stderr.recv_timeout(OUTPUT_GRACE).unwrap_or_default();
        Ok(Output {
            code: status.code(),
            stdout,
            truncated,
            stderr: String::from_utf8_lossy(&stderr).into_owned(),
        })
    }
}

/// A thread that reads `pipe` to its end, keeping `cap` bytes; the bytes kept and whether more
/// were dropped arrive on the channel returned.
fn spawn_reader<R: Read + Send + 'static>(
    pipe: Option<R>,
    cap: usize,
) -> mpsc::Receiver<(Vec<u8>, bool)> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let mut kept = Vec::new();
        let mut truncated = false;
        if let Some(mut pipe) = pipe {
            let mut chunk = [0_u8; READ_CHUNK];
            // Read on past the cap so the process never blocks on a full pipe.
            while let Ok(read) = pipe.read(&mut chunk) {
                if read == 0 {
                    break;
                }
                let room = cap.saturating_sub(kept.len());
                kept.extend_from_slice(&chunk[..read.min(room)]);
                truncated |= read > room;
            }
        }
        let _ = sender.send((kept, truncated));
    });
    receiver
}

/// How a failed call is told, from the words of git (`LC_ALL=C` makes them the same on every
/// computer).
#[must_use]
pub fn classify(stderr: &str) -> SyncFailure {
    let detail = detail_of(stderr);
    let said = stderr.to_ascii_lowercase();
    let has = |needles: &[&str]| needles.iter().any(|needle| said.contains(needle));
    if has(&[
        "authentication failed",
        "permission denied",
        "could not read username",
        "could not read password",
        "terminal prompts disabled",
        "host key verification failed",
        "invalid credentials",
        "access denied",
        "returned error: 401",
        "returned error: 403",
    ]) {
        SyncFailure::Authentication(detail)
    } else if has(&[
        "repository not found",
        "does not appear to be a git repository",
        "returned error: 404",
        "not found",
    ]) {
        SyncFailure::RemoteNotFound(detail)
    } else if has(&[
        "could not resolve host",
        "connection refused",
        "connection timed out",
        "network is unreachable",
        "failed to connect",
        "unable to access",
        "ssl",
        "tls",
    ]) {
        SyncFailure::Network(detail)
    } else if has(&[
        "non-fast-forward",
        "fetch first",
        "failed to push some refs",
    ]) {
        SyncFailure::PushRejected
    } else {
        SyncFailure::Git(detail)
    }
}

/// What git complained of, made safe to show: credentials masked, no control character,
/// one line, cut.
#[must_use]
pub fn detail_of(stderr: &str) -> String {
    let masked = mask_credentials(stderr);
    let one_line: String = masked
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .filter(|c| !c.is_control())
        .collect();
    one_line.chars().take(MAX_DETAIL_CHARS).collect()
}
