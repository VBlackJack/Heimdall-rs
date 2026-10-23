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

//! Git synchronization of the command library, as the C# `GitSyncService` does it for the
//! library of the Command Library tool: a pull brings the shared file in, a push writes the
//! library out, a sync is a pull, a merge and a push.
//!
//! The C# links LibGit2Sharp; here the installed `git` program is called, as a child process
//! with an argument array and no shell (see [`git`]). The repository holds one file, the
//! library's own export (`command-library.json`, the envelope of [`super::library_transfer`]:
//! the actions of the user and their runbooks), so a pulled file goes through exactly the
//! checks of an import before anything is kept. The folder is only a mirror of the remote:
//! the library the person works with is never inside it, so a push that failed, or a remote
//! that moved, never costs them an edit.
//!
//! This module only moves the file. Merging it is [`super::user_commands::UserLibrary::sync_merge`]
//! for the actions and [`merge::merge_runbooks`] for the runbooks.
//!
//! Nothing pulled is ever run: the file is read from git's object database, not checked out,
//! and its commands are only kept as text.

use std::fs;
use std::io;
use std::path::Path;
use std::sync::{Mutex, TryLockError};

use super::user_commands::{LIBRARY_FILE, MAX_FILE_BYTES, now_text};

pub mod git;
pub mod merge;
pub mod settings;

pub use merge::{RunbookMerge, bases_of, bases_of_runbooks, digest, merge_runbooks};
pub use settings::{
    SETTINGS_FILE, SettingsIssue, StateError, SyncSettings, SyncState, ValidSettings,
    mask_credentials,
};

/// The file the library is kept in inside the repository.
pub const REPOSITORY_FILE: &str = LIBRARY_FILE;

/// The folder, inside the one the settings are kept in, that holds no hook: git is told to look
/// for its hooks there.
const NO_HOOKS_FOLDER: &str = "command-library-sync-no-hooks";

/// What every commit message starts with.
const COMMIT_MESSAGE: &str = "Heimdall command library sync";

/// The configuration keys of the global identity.
const IDENTITY_NAME_KEY: &str = "user.name";
const IDENTITY_EMAIL_KEY: &str = "user.email";

/// The exit code of `git ls-remote --exit-code` when no ref matches.
const NO_MATCHING_REF: i32 = 2;

/// The exit code of `git diff --quiet` when the files differ.
const DIFFERENCES: i32 = 1;

/// The exit code of `git merge-base --is-ancestor` when it is not.
const NOT_AN_ANCESTOR: i32 = 1;

/// One operation at a time on the whole machine: two panes of the library must not both
/// write the same folder.
static OPERATION: Mutex<()> = Mutex::new(());

/// Why an operation did not complete.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncFailure {
    /// Another synchronization is running.
    Busy,
    /// No `git` program on the path.
    GitMissing,
    /// A value of the settings is refused.
    Settings(SettingsIssue),
    /// The folder holds files and no repository: nothing is written into it.
    FolderNotEmpty,
    /// The folder is a repository of another remote.
    OtherRepository,
    /// The folder's repository is on another branch than the one set.
    OtherBranch,
    /// The folder or a file in it could not be used.
    Io(String),
    /// A call of git did not end in time.
    TimedOut,
    /// The remote refused the credentials, or asked for some that cannot be given here.
    Authentication(String),
    /// The remote is not there.
    RemoteNotFound(String),
    /// The remote could not be reached.
    Network(String),
    /// The remote has commits that the folder lacks: pull first.
    PushRejected,
    /// The file is larger than a library file may be.
    FileTooLarge,
    /// Git failed otherwise; what it said, credentials masked.
    Git(String),
}

/// What a pull brought.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PullOutput {
    /// The library file of the remote, as it is; `None` when the remote has no such branch or
    /// the branch has no such file yet.
    pub text: Option<String>,
    /// How many commits of the remote were new to the folder.
    pub commits: usize,
}

/// What a push did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PushOutput {
    /// Whether anything was sent; false when the remote already held this library.
    pub pushed: bool,
}

/// The global Git identity of the person.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GitIdentity {
    /// `user.name`.
    pub name: Option<String>,
    /// `user.email`.
    pub email: Option<String>,
}

/// The `user.name` and `user.email` Git has for the person, to start the author of the commits
/// from (`GitConfigIdentityReader`, commit 337fca3b). `None` when there is no `git` or it says
/// nothing; never an error. It runs git, so it is called off the interface's thread.
#[must_use]
pub fn read_global_identity() -> Option<GitIdentity> {
    let program = git::program()?;
    // `git config --global` reads no repository: any folder will do, and no hook can run.
    let folder = Path::new(".");
    let hooks = Path::new(NO_HOOKS_FOLDER);
    let runner = git::Runner::new(program, folder, hooks, None);
    let read = |key: &str| {
        runner
            .run(
                &git::args::global_value(key),
                git::IDENTITY_TIMEOUT,
                git::SMALL_OUTPUT,
            )
            .ok()
            .filter(git::Output::ok)
            .map(|output| output.first_line())
            .filter(|value| !value.is_empty())
    };
    let identity = GitIdentity {
        name: read(IDENTITY_NAME_KEY),
        email: read(IDENTITY_EMAIL_KEY),
    };
    (identity.name.is_some() || identity.email.is_some()).then_some(identity)
}

/// Brings the remote's library file in. The folder is made, or its repository brought up to
/// date, but the file is not checked out: it is read from git's objects.
///
/// # Errors
///
/// [`SyncFailure`], told in the person's language by the caller.
pub fn pull(state_folder: &Path, settings: &SyncSettings) -> Result<PullOutput, SyncFailure> {
    let _only = acquire()?;
    let valid = settings.validate().map_err(SyncFailure::Settings)?;
    let program = git::program().ok_or(SyncFailure::GitMissing)?;
    let hooks = no_hooks(state_folder)?;
    prepare_folder(program, &hooks, &valid)?;
    let runner = git::Runner::new(
        program,
        &valid.local_path,
        &hooks,
        Some((&valid.author_name, &valid.author_email)),
    );
    if !remote_has_branch(&runner, &valid.branch)? {
        return Ok(PullOutput {
            text: None,
            commits: 0,
        });
    }
    fetch(&runner, &valid.branch)?;
    let head = head_exists(&runner)?;
    let commits = count(
        &runner,
        if head {
            git::args::behind_count()
        } else {
            git::args::fetched_count()
        },
    )?;
    let text = fetched_text(&runner)?;
    advance(&runner)?;
    Ok(PullOutput { text, commits })
}

/// Writes `text`, the export of the library, into the repository and sends it. Refused when
/// the remote has commits the folder lacks (pull first), as the C# `NonFastForwardException`.
///
/// # Errors
///
/// [`SyncFailure`], told in the person's language by the caller.
pub fn push(
    state_folder: &Path,
    settings: &SyncSettings,
    text: &str,
) -> Result<PushOutput, SyncFailure> {
    let _only = acquire()?;
    if u64::try_from(text.len()).map_or(true, |size| size > MAX_FILE_BYTES) {
        return Err(SyncFailure::FileTooLarge);
    }
    let valid = settings.validate().map_err(SyncFailure::Settings)?;
    let program = git::program().ok_or(SyncFailure::GitMissing)?;
    let hooks = no_hooks(state_folder)?;
    prepare_folder(program, &hooks, &valid)?;
    let runner = git::Runner::new(
        program,
        &valid.local_path,
        &hooks,
        Some((&valid.author_name, &valid.author_email)),
    );
    let remote_has = remote_has_branch(&runner, &valid.branch)?;
    if remote_has {
        fetch(&runner, &valid.branch)?;
        // The remote moved since the last pull: sending now would erase its edits.
        if !head_exists(&runner)? {
            return Err(SyncFailure::PushRejected);
        }
        let contained = call(&runner, &git::args::contains_fetched(), git::LOCAL_TIMEOUT)?;
        match contained.code {
            Some(0) => {}
            Some(NOT_AN_ANCESTOR) => return Err(SyncFailure::PushRejected),
            _ => return Err(git::classify(&contained.stderr)),
        }
    }
    fs::write(valid.local_path.join(REPOSITORY_FILE), text)
        .map_err(|error| SyncFailure::Io(error.to_string()))?;
    expect(&runner, &git::args::stage(), git::LOCAL_TIMEOUT)?;
    let staged = call(&runner, &git::args::staged_changes(), git::LOCAL_TIMEOUT)?;
    match staged.code {
        Some(0) => {}
        Some(DIFFERENCES) => {
            let message = format!("{COMMIT_MESSAGE}: {}", now_text());
            expect(&runner, &git::args::commit(&message), git::LOCAL_TIMEOUT)?;
        }
        _ => return Err(git::classify(&staged.stderr)),
    }
    let to_send = if remote_has {
        count(&runner, git::args::ahead_count())? > 0
    } else {
        head_exists(&runner)?
    };
    if !to_send {
        return Ok(PushOutput { pushed: false });
    }
    let sent = call(
        &runner,
        &git::args::push(&valid.branch),
        git::NETWORK_TIMEOUT,
    )?;
    if !sent.ok() {
        return Err(git::classify(&sent.stderr));
    }
    Ok(PushOutput { pushed: true })
}

/// Takes the lock of the operations, or says another one is running.
fn acquire() -> Result<std::sync::MutexGuard<'static, ()>, SyncFailure> {
    match OPERATION.try_lock() {
        Ok(guard) => Ok(guard),
        Err(TryLockError::Poisoned(poisoned)) => Ok(poisoned.into_inner()),
        Err(TryLockError::WouldBlock) => Err(SyncFailure::Busy),
    }
}

/// An empty folder to point git's hooks at, made beside the settings.
fn no_hooks(state_folder: &Path) -> Result<std::path::PathBuf, SyncFailure> {
    let hooks = state_folder.join(NO_HOOKS_FOLDER);
    fs::create_dir_all(&hooks).map_err(|error| SyncFailure::Io(error.to_string()))?;
    Ok(hooks)
}

/// Makes the folder a repository of the remote and the branch, or checks that it already is
/// one. A folder with other files in it is refused, as the C# `IsValidRepositoryPath` and its
/// empty-folder rule do.
fn prepare_folder(program: &Path, hooks: &Path, valid: &ValidSettings) -> Result<(), SyncFailure> {
    let folder = &valid.local_path;
    let io = |error: io::Error| SyncFailure::Io(error.to_string());
    if folder.join(".git").exists() {
        let runner = git::Runner::new(program, folder, hooks, None);
        let url = call(&runner, &git::args::origin_url(), git::LOCAL_TIMEOUT)?;
        if !url.ok() {
            return Err(SyncFailure::OtherRepository);
        }
        if url.first_line() != valid.remote_url {
            return Err(SyncFailure::OtherRepository);
        }
        let branch = call(&runner, &git::args::current_branch(), git::LOCAL_TIMEOUT)?;
        // A repository with no commit yet reports no branch name through this call on some
        // versions of git; the first fetch of the folder decides.
        if branch.ok() && branch.first_line() != valid.branch {
            return Err(SyncFailure::OtherBranch);
        }
        return Ok(());
    }
    match fs::read_dir(folder) {
        Ok(mut entries) => {
            if entries.next().is_some() {
                return Err(SyncFailure::FolderNotEmpty);
            }
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            fs::create_dir_all(folder).map_err(io)?;
        }
        Err(error) => return Err(io(error)),
    }
    let runner = git::Runner::new(program, folder, hooks, None);
    expect(&runner, &git::args::init(), git::LOCAL_TIMEOUT)?;
    expect(
        &runner,
        &git::args::point_head(&valid.branch),
        git::LOCAL_TIMEOUT,
    )?;
    expect(
        &runner,
        &git::args::add_origin(&valid.remote_url),
        git::LOCAL_TIMEOUT,
    )?;
    Ok(())
}

/// Runs a call, returning what it said, success or not.
fn call(
    runner: &git::Runner<'_>,
    arguments: &[std::ffi::OsString],
    timeout: std::time::Duration,
) -> Result<git::Output, SyncFailure> {
    runner.run(arguments, timeout, git::SMALL_OUTPUT)
}

/// Runs a call that must succeed.
fn expect(
    runner: &git::Runner<'_>,
    arguments: &[std::ffi::OsString],
    timeout: std::time::Duration,
) -> Result<git::Output, SyncFailure> {
    let output = call(runner, arguments, timeout)?;
    if output.ok() {
        Ok(output)
    } else {
        Err(git::classify(&output.stderr))
    }
}

/// Whether `branch` exists on the remote.
fn remote_has_branch(runner: &git::Runner<'_>, branch: &str) -> Result<bool, SyncFailure> {
    let output = call(
        runner,
        &git::args::remote_branch(branch),
        git::NETWORK_TIMEOUT,
    )?;
    match output.code {
        Some(0) => Ok(true),
        Some(NO_MATCHING_REF) => Ok(false),
        _ => Err(git::classify(&output.stderr)),
    }
}

/// Fetches `branch` of the remote.
fn fetch(runner: &git::Runner<'_>, branch: &str) -> Result<(), SyncFailure> {
    expect(runner, &git::args::fetch(branch), git::NETWORK_TIMEOUT).map(|_| ())
}

/// Whether the folder has a first commit.
fn head_exists(runner: &git::Runner<'_>) -> Result<bool, SyncFailure> {
    let output = call(runner, &git::args::head_exists(), git::LOCAL_TIMEOUT)?;
    match output.code {
        Some(0) => Ok(true),
        Some(1) => Ok(false),
        _ => Err(git::classify(&output.stderr)),
    }
}

/// The number a counting call printed.
fn count(
    runner: &git::Runner<'_>,
    arguments: Vec<std::ffi::OsString>,
) -> Result<usize, SyncFailure> {
    let output = expect(runner, &arguments, git::LOCAL_TIMEOUT)?;
    output
        .first_line()
        .parse()
        .map_err(|_| SyncFailure::Git(output.first_line()))
}

/// The library file of the last fetch; `None` when the branch has none.
fn fetched_text(runner: &git::Runner<'_>) -> Result<Option<String>, SyncFailure> {
    let size = call(runner, &git::args::fetched_size(), git::LOCAL_TIMEOUT)?;
    if !size.ok() {
        return Ok(None);
    }
    let bytes: u64 = size
        .first_line()
        .parse()
        .map_err(|_| SyncFailure::Git(size.first_line()))?;
    if bytes > MAX_FILE_BYTES {
        return Err(SyncFailure::FileTooLarge);
    }
    let cap = usize::try_from(MAX_FILE_BYTES).map_or(usize::MAX, |cap| cap.saturating_add(1));
    let content = runner.run(&git::args::fetched_content(), git::LOCAL_TIMEOUT, cap)?;
    if !content.ok() {
        return Err(git::classify(&content.stderr));
    }
    if content.truncated {
        return Err(SyncFailure::FileTooLarge);
    }
    let text = String::from_utf8_lossy(&content.stdout);
    Ok(Some(text.trim_start_matches('\u{feff}').to_owned()))
}

/// Brings the folder to the last fetch: forward when it can, else (a commit of its own that a
/// push never sent) by moving the branch, which leaves every file as it is.
fn advance(runner: &git::Runner<'_>) -> Result<(), SyncFailure> {
    let forward = call(runner, &git::args::fast_forward(), git::LOCAL_TIMEOUT)?;
    if forward.ok() {
        return Ok(());
    }
    expect(runner, &git::args::move_to_fetched(), git::LOCAL_TIMEOUT).map(|_| ())
}

#[cfg(test)]
mod tests;
