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

//! What the synchronization is told: the repository, the branch and the author of the commits
//! (the C# `CmdLibGitSyncUrl`, `CmdLibGitSyncBranch`, `CmdLibGitSyncAuthorName` and
//! `CmdLibGitSyncAuthorEmail`), and what it remembers of the runbooks between two
//! synchronizations. The local repository is not a setting: it is a folder Heimdall makes in
//! its own data folder (see the parent module).
//!
//! Every value a person typed ends up in the arguments of a `git` process, so each is held to
//! a strict shape before it is used: no leading `-` (git would read it as an option), no
//! control or invisible character, and a URL of an allowed scheme only. The check is
//! [`SyncSettings::validate`]; nothing else builds a command line.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::tools::private_file::write_private;
use crate::tools::runbooks::is_hidden;
use crate::tools::user_commands::MAX_FILE_BYTES;

/// The file the settings are kept in, beside the library.
pub const SETTINGS_FILE: &str = "command-library-sync.json";

/// The branch a new setup starts on, as the C# `DefaultCmdLibGitSyncBranch`.
pub const DEFAULT_BRANCH: &str = "main";

/// The author name a new setup ships with, as the C# `DefaultCmdLibGitSyncAuthorName`. While
/// the setting still holds it, the name of the global Git identity takes its place.
pub const PLACEHOLDER_AUTHOR_NAME: &str = "Heimdall User";

/// The author e-mail address a new setup ships with, as the C#
/// `DefaultCmdLibGitSyncAuthorEmail`.
pub const PLACEHOLDER_AUTHOR_EMAIL: &str = "heimdall@local";

/// The longest URL taken.
pub const MAX_URL_LENGTH: usize = 2048;

/// The longest branch name taken.
pub const MAX_BRANCH_LENGTH: usize = 200;

/// The longest author name taken.
pub const MAX_AUTHOR_NAME_LENGTH: usize = 100;

/// The longest author e-mail address taken.
pub const MAX_AUTHOR_EMAIL_LENGTH: usize = 254;

/// The schemes of a remote URL: nothing else is passed to git (no `http`, no `git`, no `file`
/// as the C# `GitUrlValidator`, and above all no `ext::` or `fd::` transport helper, which run
/// a command).
pub const ALLOWED_SCHEMES: [&str; 2] = ["https", "ssh"];

#[cfg(test)]
thread_local! {
    /// Lets a test of this thread use a `file://` bare repository as the remote.
    static FILE_URLS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Whether `file://` remotes are taken: never, except in the tests that need a bare
/// repository on disk and say so with `allow_file_urls_in_this_test`.
#[must_use]
pub fn file_urls_allowed() -> bool {
    #[cfg(test)]
    {
        FILE_URLS.with(std::cell::Cell::get)
    }
    #[cfg(not(test))]
    {
        false
    }
}

/// Takes `file://` remotes for the rest of this test's thread.
#[cfg(test)]
pub fn allow_file_urls_in_this_test() {
    FILE_URLS.with(|allowed| allowed.set(true));
}

/// What separates the scheme from the rest of a URL.
const SCHEME_SEPARATOR: &str = "://";

/// What starts an option for git.
const OPTION_LEADING: char = '-';

/// What introduces a transport helper (`ext::sh -c ...`).
const HELPER_SEPARATOR: &str = "::";

/// What separates the user from the host in a URL, and the host from the path in the scp-like
/// syntax.
const USER_SEPARATOR: char = '@';
const SCP_PATH_SEPARATOR: char = ':';

/// What replaces the credentials of a URL in a text shown or logged.
const MASK: &str = "***";

/// The characters a branch may hold besides letters and digits.
const BRANCH_PUNCTUATION: [char; 3] = ['.', '_', '-'];
const BRANCH_SEPARATOR: char = '/';

/// What a branch name may not end with.
const BRANCH_LOCK_SUFFIX: &str = ".lock";

/// The shortest host of the scp-like syntax: a single letter is a Windows drive.
const MIN_SCP_HOST_LENGTH: usize = 2;

/// Why a value of the settings is refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsIssue {
    /// No repository URL.
    UrlMissing,
    /// The URL starts with `-`: git would take it for an option.
    UrlOption,
    /// The URL holds a space, a control character or one that shows nothing.
    UrlHidden,
    /// The URL is longer than [`MAX_URL_LENGTH`].
    UrlTooLong,
    /// The URL names a scheme or a transport that is not allowed.
    UrlScheme,
    /// The URL is not one of the forms taken.
    UrlFormat,
    /// An https URL carries a user name or a token: credentials belong to the credential
    /// manager.
    UrlCredentials,
    /// No branch.
    BranchMissing,
    /// The branch is not a name git takes, or starts with `-`.
    BranchFormat,
    /// The author name is empty, too long, or holds what a name may not.
    AuthorName,
    /// The author e-mail address is empty, too long, or is not an address.
    AuthorEmail,
}

/// The settings as the person left them: any text, not yet checked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SyncSettings {
    /// The repository: `https://`, `ssh://` or `file://`, or the scp-like `user@host:path`.
    pub remote_url: String,
    /// The branch synchronized.
    pub branch: String,
    /// The name commits are signed with.
    pub author_name: String,
    /// The e-mail address commits are signed with.
    pub author_email: String,
}

impl Default for SyncSettings {
    fn default() -> Self {
        Self {
            remote_url: String::new(),
            branch: DEFAULT_BRANCH.to_owned(),
            author_name: PLACEHOLDER_AUTHOR_NAME.to_owned(),
            author_email: PLACEHOLDER_AUTHOR_EMAIL.to_owned(),
        }
    }
}

/// Settings that passed [`SyncSettings::validate`]: the only form the operations take.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidSettings {
    /// The repository URL, trimmed.
    pub remote_url: String,
    /// The branch.
    pub branch: String,
    /// The author name.
    pub author_name: String,
    /// The author e-mail address.
    pub author_email: String,
}

impl SyncSettings {
    /// Whether the repository is given, as the C# `IsConfigured`.
    #[must_use]
    pub fn is_configured(&self) -> bool {
        !self.remote_url.trim().is_empty()
    }

    /// Holds every value to its shape.
    ///
    /// # Errors
    ///
    /// The first [`SettingsIssue`], in the order the settings are shown.
    pub fn validate(&self) -> Result<ValidSettings, SettingsIssue> {
        let remote_url = self.remote_url.trim();
        url_issue(remote_url).map_or(Ok(()), Err)?;
        let branch = self.branch.trim();
        branch_issue(branch).map_or(Ok(()), Err)?;
        let author_name = self.author_name.trim();
        if !name_is_valid(author_name) {
            return Err(SettingsIssue::AuthorName);
        }
        let author_email = self.author_email.trim();
        if !email_is_valid(author_email) {
            return Err(SettingsIssue::AuthorEmail);
        }
        Ok(ValidSettings {
            remote_url: remote_url.to_owned(),
            branch: branch.to_owned(),
            author_name: author_name.to_owned(),
            author_email: author_email.to_owned(),
        })
    }

    /// Fills the author name and the e-mail address from the identity Git has on this
    /// computer, each only while it still holds the shipped placeholder, so a value the person
    /// chose, or typed meanwhile, is never replaced (`SettingsViewModel.cs`, commit 337fca3b).
    /// Whether anything changed.
    pub fn take_identity(&mut self, name: Option<&str>, email: Option<&str>) -> bool {
        let mut changed = false;
        if self.author_name == PLACEHOLDER_AUTHOR_NAME
            && let Some(name) = name.map(str::trim).filter(|name| name_is_valid(name))
        {
            name.clone_into(&mut self.author_name);
            changed = true;
        }
        if self.author_email == PLACEHOLDER_AUTHOR_EMAIL
            && let Some(email) = email.map(str::trim).filter(|email| email_is_valid(email))
        {
            email.clone_into(&mut self.author_email);
            changed = true;
        }
        changed
    }

    /// Whether the author is still the shipped placeholder, in whole or in part.
    #[must_use]
    pub fn author_is_placeholder(&self) -> bool {
        self.author_name == PLACEHOLDER_AUTHOR_NAME || self.author_email == PLACEHOLDER_AUTHOR_EMAIL
    }
}

/// Whether `text` holds a space, a control character or one that shows nothing.
fn holds_hidden_or_space(text: &str) -> bool {
    text.chars().any(|c| is_hidden(c) || c.is_whitespace())
}

/// Why `url` is refused, when it is.
fn url_issue(url: &str) -> Option<SettingsIssue> {
    if url.is_empty() {
        return Some(SettingsIssue::UrlMissing);
    }
    if url.starts_with(OPTION_LEADING) {
        return Some(SettingsIssue::UrlOption);
    }
    if url.chars().count() > MAX_URL_LENGTH {
        return Some(SettingsIssue::UrlTooLong);
    }
    if holds_hidden_or_space(url) {
        return Some(SettingsIssue::UrlHidden);
    }
    if let Some((scheme, rest)) = url.split_once(SCHEME_SEPARATOR) {
        let scheme = scheme.to_ascii_lowercase();
        let is_file = scheme == "file" && file_urls_allowed();
        if !ALLOWED_SCHEMES.contains(&scheme.as_str()) && !is_file {
            return Some(SettingsIssue::UrlScheme);
        }
        if rest.is_empty() {
            return Some(SettingsIssue::UrlFormat);
        }
        if is_file {
            return None;
        }
        let authority = rest.split('/').next().unwrap_or_default();
        let (user, host) = match authority.rfind(USER_SEPARATOR) {
            Some(index) => (Some(&authority[..index]), &authority[index + 1..]),
            None => (None, authority),
        };
        if host.is_empty() {
            return Some(SettingsIssue::UrlFormat);
        }
        if scheme == "https" && user.is_some() {
            return Some(SettingsIssue::UrlCredentials);
        }
        // ssh would read a user or a host that starts with a dash as an option.
        if user.is_some_and(|user| user.starts_with(OPTION_LEADING))
            || host.starts_with(OPTION_LEADING)
        {
            return Some(SettingsIssue::UrlOption);
        }
        return None;
    }
    // A transport helper (`ext::sh -c ...`, `fd::17`): it runs a command.
    if url.contains(HELPER_SEPARATOR) {
        return Some(SettingsIssue::UrlScheme);
    }
    // The scp-like syntax, `user@host:path`: ssh. A single letter before the colon is a
    // Windows drive, and a path without a scheme is not one of the forms taken.
    if let Some((host_part, path)) = url.split_once(SCP_PATH_SEPARATOR) {
        let (user, host) = match host_part.rfind(USER_SEPARATOR) {
            Some(index) => (Some(&host_part[..index]), &host_part[index + 1..]),
            None => (None, host_part),
        };
        if user.is_some_and(|user| user.starts_with(OPTION_LEADING))
            || host.starts_with(OPTION_LEADING)
        {
            return Some(SettingsIssue::UrlOption);
        }
        let plausible = host.chars().count() >= MIN_SCP_HOST_LENGTH
            && !host.contains('/')
            && !host.contains('\\')
            && !path.is_empty();
        return if plausible {
            None
        } else {
            Some(SettingsIssue::UrlFormat)
        };
    }
    Some(SettingsIssue::UrlFormat)
}

/// Why the branch `name` is refused, when it is: the rules of `git check-ref-format`, kept to
/// a narrower alphabet.
fn branch_issue(name: &str) -> Option<SettingsIssue> {
    if name.is_empty() {
        return Some(SettingsIssue::BranchMissing);
    }
    let allowed = |c: char| {
        c.is_ascii_alphanumeric() || BRANCH_PUNCTUATION.contains(&c) || c == BRANCH_SEPARATOR
    };
    let bad = name.chars().count() > MAX_BRANCH_LENGTH
        || !name.chars().all(allowed)
        || name.starts_with(OPTION_LEADING)
        || name.starts_with(['.', BRANCH_SEPARATOR])
        || name.ends_with(['.', BRANCH_SEPARATOR])
        || name.ends_with(BRANCH_LOCK_SUFFIX)
        || name.contains("..")
        || name.contains("//")
        || name.contains("/.")
        || name.contains(".lock/");
    bad.then_some(SettingsIssue::BranchFormat)
}

/// Whether `name` can sign a commit.
fn name_is_valid(name: &str) -> bool {
    !name.is_empty()
        && name.chars().count() <= MAX_AUTHOR_NAME_LENGTH
        && !name.starts_with(OPTION_LEADING)
        && !name.chars().any(|c| is_hidden(c) || c == '<' || c == '>')
}

/// Whether `email` can sign a commit.
fn email_is_valid(email: &str) -> bool {
    let Some((user, host)) = email.split_once(USER_SEPARATOR) else {
        return false;
    };
    !user.is_empty()
        && !host.is_empty()
        && email.chars().count() <= MAX_AUTHOR_EMAIL_LENGTH
        && !email.starts_with(OPTION_LEADING)
        && !email
            .chars()
            .any(|c| is_hidden(c) || c.is_whitespace() || c == '<' || c == '>')
}

/// `text` with the credentials of every URL in it replaced by [`MASK`]: what git says of a
/// failure can quote the URL it was given, and a URL may carry a token.
#[must_use]
pub fn mask_credentials(text: &str) -> String {
    let mut masked = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find(SCHEME_SEPARATOR) {
        let (before, after) = rest.split_at(at + SCHEME_SEPARATOR.len());
        masked.push_str(before);
        // The authority ends at the first `/`, space or quote; credentials are what precedes
        // the last `@` of it.
        let end = after
            .find(|c: char| c == '/' || c.is_whitespace() || c == '\'' || c == '"')
            .unwrap_or(after.len());
        let (authority, tail) = after.split_at(end);
        match authority.rfind(USER_SEPARATOR) {
            Some(index) => {
                masked.push_str(MASK);
                masked.push_str(&authority[index..]);
            }
            None => masked.push_str(authority),
        }
        rest = tail;
    }
    masked.push_str(rest);
    masked
}

/// Why the file of the settings could not be used.
#[derive(Debug)]
pub enum StateError {
    /// It could not be read or written.
    Io(io::Error),
    /// It is larger than [`MAX_FILE_BYTES`] or is not what this build writes.
    Format,
}

/// The settings and what the synchronization remembers between two runs.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SyncState {
    /// What the person set.
    pub settings: SyncSettings,
    /// For each runbook, a digest of its content as it was when it last agreed with the
    /// repository (after a pull took it, or a push wrote it). A runbook has no timestamp to
    /// tell a local edit by (the C# compares `UpdatedAt`), so it is compared with this: a run
    /// or an import changes nothing in the runbook and so never looks like an edit.
    pub runbook_bases: BTreeMap<String, u64>,
}

impl SyncState {
    /// The state kept at `path`; the default one when there is no file.
    ///
    /// # Errors
    ///
    /// [`StateError`] when the file cannot be read or is not a state: the caller must then
    /// not save over it.
    pub fn load(path: &Path) -> Result<Self, StateError> {
        let length = match fs::metadata(path) {
            Ok(meta) => meta.len(),
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(error) => return Err(StateError::Io(error)),
        };
        if length > MAX_FILE_BYTES {
            return Err(StateError::Format);
        }
        let text = fs::read_to_string(path).map_err(StateError::Io)?;
        serde_json::from_str(&text).map_err(|_| StateError::Format)
    }

    /// Keeps the state at `path`, readable by its owner alone: the URL may carry a token.
    ///
    /// # Errors
    ///
    /// The error of writing the file; what was there is left as it was.
    pub fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(folder) = path.parent() {
            fs::create_dir_all(folder)?;
        }
        let text = serde_json::to_string_pretty(self).map_err(io::Error::other)?;
        write_private(path, text.as_bytes())
    }
}
