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

//! Git synchronization in the command library: the settings panel, the three buttons (Pull,
//! Push, Sync) and what they tell, as the C# `CommandLibraryViewModel.Commands.cs` sync flow.
//!
//! The work is `heimdall_core::tools::library_sync`; this module runs it off the interface's
//! thread, merges what it brings into the library and the runbooks (the file is untrusted and
//! goes through the checks of an import), and says what happened: how many were added,
//! replaced or left out, and which actions and runbooks were kept because the copy here is
//! newer than the repository's.

use std::path::{Path, PathBuf};

use heimdall_app::TabId;
use heimdall_core::tools::library_sync::{
    self, GitIdentity, PullOutput, PushOutput, RunbookMerge, SETTINGS_FILE, SettingsIssue,
    SyncFailure, SyncSettings, SyncState, bases_of_runbooks,
};
use heimdall_core::tools::library_transfer::{self, RunbookImport};
use heimdall_core::tools::user_commands::ImportReport;
use iced::widget::{button, column, row, text, text_input};
use iced::{Element, Task};

use super::{CommandLibraryMessage, CommandLibraryPane, Notice, Outcome, TransferError};
use super::{import_text, transfer_error};
use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, spacing};
use crate::tools::ToolMessage;

/// The most titles of kept actions named in a result; the rest are counted.
const MAX_LISTED_TITLES: usize = 10;

/// What separates titles in a list.
const TITLE_SEPARATOR: &str = ", ";

/// A field of the settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncField {
    /// The repository URL.
    Url,
    /// The branch.
    Branch,
    /// The author name.
    AuthorName,
    /// The author e-mail address.
    AuthorEmail,
}

/// What the synchronization is asked.
#[derive(Debug, Clone)]
pub enum SyncMessage {
    /// Open the panel, or close it.
    Toggle,
    /// A field typed.
    Field(SyncField, String),
    /// Keep the settings.
    Save,
    /// Bring the repository's library in.
    Pull,
    /// Send the library.
    Push,
    /// Pull, merge, then push.
    Sync,
    /// The identity Git has on this computer was read.
    Identity(Option<GitIdentity>),
    /// A pull ended; whether a push follows.
    Pulled(Result<PullOutput, SyncFailure>, bool),
    /// A push ended.
    Pushed(Result<PushOutput, SyncFailure>),
}

/// What the window runs for the synchronization.
#[derive(Debug, PartialEq, Eq)]
pub enum SyncJob {
    /// Read the identity Git has.
    Identity {
        /// The folder the settings are kept in.
        folder: PathBuf,
    },
    /// Pull; a push follows when `then_push`.
    Pull {
        /// The folder the settings are kept in.
        folder: PathBuf,
        /// The settings.
        settings: SyncSettings,
        /// Whether a push follows the merge.
        then_push: bool,
    },
    /// Push `text`, the export of the library.
    Push {
        /// The folder the settings are kept in.
        folder: PathBuf,
        /// The settings.
        settings: SyncSettings,
        /// The export.
        text: String,
    },
}

impl SyncJob {
    /// What tab `tab` runs: Git is called on a thread of its own, never the interface's.
    pub fn task(self, tab: TabId) -> Task<Message> {
        let to = move |message| {
            Message::Tool(
                tab,
                ToolMessage::CmdLib(CommandLibraryMessage::Sync(message)),
            )
        };
        match self {
            Self::Identity { folder } => Task::perform(
                async move {
                    tokio::task::spawn_blocking(move || library_sync::read_global_identity(&folder))
                        .await
                        .unwrap_or(None)
                },
                move |identity| to(SyncMessage::Identity(identity)),
            ),
            Self::Pull {
                folder,
                settings,
                then_push,
            } => Task::perform(
                async move {
                    tokio::task::spawn_blocking(move || library_sync::pull(&folder, &settings))
                        .await
                        .unwrap_or_else(|error| Err(SyncFailure::Io(error.to_string())))
                },
                move |result| to(SyncMessage::Pulled(result, then_push)),
            ),
            Self::Push {
                folder,
                settings,
                text,
            } => Task::perform(
                async move {
                    tokio::task::spawn_blocking(move || {
                        library_sync::push(&folder, &settings, &text)
                    })
                    .await
                    .unwrap_or_else(|error| Err(SyncFailure::Io(error.to_string())))
                },
                move |result| to(SyncMessage::Pushed(result)),
            ),
        }
    }
}

/// What a pull merged into the library.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Pulled {
    /// How many commits were new.
    commits: usize,
    /// The repository has no library on this branch yet.
    empty: bool,
    /// What happened to the actions.
    actions: ImportReport,
    /// What happened to the runbooks, when the file carried some.
    runbooks: Option<RunbookImport>,
    /// The titles of the actions and the names of the runbooks kept because the copy here is
    /// newer.
    kept_local: Vec<String>,
}

/// What a synchronization tells.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncNotice {
    /// A value of the settings is refused.
    Settings(SettingsIssue),
    /// The settings are kept.
    Saved,
    /// The settings file could not be read, so nothing is written over it.
    StateUnreadable,
    /// The settings could not be written.
    StateUnsaved,
    /// The library of the user cannot be read or written.
    LibraryUnavailable,
    /// The pulled library could not be written; nothing was changed.
    LibraryUnsaved,
    /// The library has nothing to send.
    NothingToPush,
    /// The pulled file cannot be used.
    File(TransferError),
    /// An operation failed.
    Failed(SyncFailure),
    /// A pull, a push or both completed.
    Done {
        /// What the pull merged, when there was one.
        pulled: Option<Pulled>,
        /// Whether the push sent anything, when there was one.
        pushed: Option<bool>,
    },
}

/// The synchronization's state in a pane of the library.
#[derive(Debug, Default)]
pub struct SyncPane {
    open: bool,
    /// The file the settings are kept in; `None` when the platform has no folder.
    file: Option<PathBuf>,
    state: SyncState,
    /// The settings file could not be read: it is not written over.
    unreadable: bool,
    /// An operation is running.
    busy: bool,
    /// Whether the identity Git has was asked for.
    identity_asked: bool,
    /// What a sync's pull merged, until its push ends.
    pulled: Option<Pulled>,
    /// The digests of the runbooks a running push sends, which the repository then agrees with.
    pending_bases: Option<std::collections::BTreeMap<String, u64>>,
}

impl SyncPane {
    /// The synchronization of the library kept beside `profiles_file`, its settings read now.
    #[must_use]
    pub fn new(profiles_file: Option<&Path>) -> Self {
        let file = profiles_file
            .and_then(Path::parent)
            .map(|folder| folder.join(SETTINGS_FILE));
        let mut pane = Self {
            file,
            ..Self::default()
        };
        if let Some(path) = &pane.file {
            match SyncState::load(path) {
                Ok(state) => pane.state = state,
                Err(_) => pane.unreadable = true,
            }
        }
        pane
    }

    /// Whether the panel is open.
    #[must_use]
    pub const fn is_open(&self) -> bool {
        self.open
    }

    /// The folder the settings are kept in, where the synchronization keeps its own files.
    fn folder(&self) -> Option<PathBuf> {
        self.file
            .as_deref()
            .and_then(Path::parent)
            .map(Path::to_path_buf)
    }

    /// Writes the settings; whether it was done.
    fn persist(&self) -> bool {
        !self.unreadable
            && self
                .file
                .as_ref()
                .is_some_and(|path| self.state.save(path).is_ok())
    }
}

impl CommandLibraryPane {
    /// Applies `message` to the synchronization.
    pub(super) fn sync_message(&mut self, message: SyncMessage) -> Outcome {
        match message {
            SyncMessage::Toggle => {
                self.sync.open = !self.sync.open;
                // The author starts from the identity Git has, once, while it is still the
                // shipped placeholder (commit 337fca3b of the C#).
                if self.sync.open
                    && !self.sync.identity_asked
                    && self.sync.state.settings.author_is_placeholder()
                    && let Some(folder) = self.sync.folder()
                {
                    self.sync.identity_asked = true;
                    return Outcome::Sync(SyncJob::Identity { folder });
                }
            }
            SyncMessage::Field(field, value) => {
                let settings = &mut self.sync.state.settings;
                match field {
                    SyncField::Url => settings.remote_url = value,
                    SyncField::Branch => settings.branch = value,
                    SyncField::AuthorName => settings.author_name = value,
                    SyncField::AuthorEmail => settings.author_email = value,
                }
            }
            SyncMessage::Save => self.sync_save(),
            SyncMessage::Pull => return self.sync_start(false, true),
            SyncMessage::Push => return self.sync_start(true, false),
            SyncMessage::Sync => return self.sync_start(true, true),
            SyncMessage::Identity(identity) => {
                // Only a field that still holds the placeholder is filled, so what was typed
                // meanwhile stays. The fields count as edited: nothing is written until the
                // person saves or synchronizes.
                if let Some(identity) = identity {
                    self.sync
                        .state
                        .settings
                        .take_identity(identity.name.as_deref(), identity.email.as_deref());
                }
            }
            SyncMessage::Pulled(result, then_push) => return self.sync_pulled(result, then_push),
            SyncMessage::Pushed(result) => self.sync_pushed(result),
        }
        Outcome::None
    }

    /// Says `notice`.
    fn sync_tell(&mut self, notice: SyncNotice) {
        self.notice = Some(Notice::Sync(notice));
    }

    /// Keeps the settings when they are valid.
    fn sync_save(&mut self) {
        if let Err(issue) = self.sync.state.settings.validate() {
            self.sync_tell(SyncNotice::Settings(issue));
        } else if self.sync.unreadable {
            self.sync_tell(SyncNotice::StateUnreadable);
        } else if self.sync.persist() {
            self.sync_tell(SyncNotice::Saved);
        } else {
            self.sync_tell(SyncNotice::StateUnsaved);
        }
    }

    /// Starts a pull (when `pull`) or, when only `push`, a push; a sync is both.
    fn sync_start(&mut self, push: bool, pull: bool) -> Outcome {
        if self.sync.busy {
            return Outcome::None;
        }
        if !self.can_edit() {
            self.sync_tell(SyncNotice::LibraryUnavailable);
            return Outcome::None;
        }
        if self.sync.unreadable {
            self.sync_tell(SyncNotice::StateUnreadable);
            return Outcome::None;
        }
        if let Err(issue) = self.sync.state.settings.validate() {
            self.sync_tell(SyncNotice::Settings(issue));
            return Outcome::None;
        }
        // What runs is what is saved.
        let Some(folder) = self.sync.folder().filter(|_| self.sync.persist()) else {
            self.sync_tell(SyncNotice::StateUnsaved);
            return Outcome::None;
        };
        self.sync.pulled = None;
        self.notice = None;
        if pull {
            self.sync.busy = true;
            return Outcome::Sync(SyncJob::Pull {
                folder,
                settings: self.sync.state.settings.clone(),
                then_push: push,
            });
        }
        match self.sync_push_job(folder) {
            Some(job) => {
                self.sync.busy = true;
                Outcome::Sync(job)
            }
            None => {
                self.sync_tell(SyncNotice::NothingToPush);
                Outcome::None
            }
        }
    }

    /// The push of the library as it is now: its actions and their runbooks. `None` when there
    /// is nothing of the user's to send, which would empty the repository's library.
    fn sync_push_job(&mut self, folder: PathBuf) -> Option<SyncJob> {
        let commands: Vec<_> = self
            .library
            .commands()
            .iter()
            .map(library_transfer::action_value)
            .collect();
        let runbooks = self.runbooks.exportable(&self.library);
        if commands.is_empty() && runbooks.is_empty() {
            return None;
        }
        self.sync.pending_bases = Some(bases_of_runbooks(&runbooks));
        Some(SyncJob::Push {
            folder,
            settings: self.sync.state.settings.clone(),
            text: library_transfer::export_text(&commands, &runbooks),
        })
    }

    /// A pull ended: merge what it brought, then push when asked.
    fn sync_pulled(&mut self, result: Result<PullOutput, SyncFailure>, then_push: bool) -> Outcome {
        let pulled = match result {
            Ok(output) => self.sync_merge(output),
            Err(failure) => Err(SyncNotice::Failed(failure)),
        };
        let pulled = match pulled {
            Ok(pulled) => pulled,
            Err(notice) => {
                self.sync.busy = false;
                self.sync_tell(notice);
                return Outcome::None;
            }
        };
        if then_push && let Some(folder) = self.sync.folder() {
            if let Some(job) = self.sync_push_job(folder) {
                self.sync.pulled = Some(pulled);
                return Outcome::Sync(job);
            }
        }
        self.sync.busy = false;
        self.sync_tell(SyncNotice::Done {
            pulled: Some(pulled),
            pushed: None,
        });
        Outcome::None
    }

    /// A push ended.
    fn sync_pushed(&mut self, result: Result<PushOutput, SyncFailure>) {
        self.sync.busy = false;
        let pulled = self.sync.pulled.take();
        match result {
            Ok(output) => {
                // The repository now holds every runbook as it is here.
                self.sync.state.runbook_bases = self.sync.pending_bases.take().unwrap_or_default();
                let notice = if self.sync.persist() {
                    SyncNotice::Done {
                        pulled,
                        pushed: Some(output.pushed),
                    }
                } else {
                    SyncNotice::StateUnsaved
                };
                self.sync_tell(notice);
            }
            Err(failure) => {
                self.sync.pending_bases = None;
                // A sync whose push failed still pulled: the failure is what is told first.
                self.sync_tell(SyncNotice::Failed(failure));
            }
        }
    }

    /// Merges what a pull brought into the library and the runbooks, the file being untrusted:
    /// it is held to the checks of an import, and nothing in it is run.
    fn sync_merge(&mut self, output: PullOutput) -> Result<Pulled, SyncNotice> {
        let Some(text) = output.text else {
            return Ok(Pulled {
                commits: output.commits,
                empty: true,
                ..Pulled::default()
            });
        };
        let carried = library_transfer::precheck(&text)
            .map_err(|error| SyncNotice::File(transfer_error(error)))?;
        if !carried.is_empty() && !self.runbooks.can_edit() {
            return Err(SyncNotice::File(TransferError::RunbooksUnsaved));
        }
        let before = self.library.clone();
        let report = self
            .library
            .sync_merge(&text)
            .map_err(|error| SyncNotice::File(transfer_error(error)))?;
        // A level in a file is its author's claim: a destructive pattern is dangerous
        // whatever the file says.
        self.library.raise_dangerous(&report.touched, |pattern| {
            heimdall_app::dangerous_command(pattern).is_some()
        });
        if (report.imported + report.updated > 0) && !self.keep(&before) {
            return Err(SyncNotice::LibraryUnsaved);
        }
        let merged: Option<RunbookMerge> = if carried.is_empty() {
            None
        } else {
            let merged =
                self.runbooks
                    .sync(&carried, &self.library, &mut self.sync.state.runbook_bases);
            if merged.is_none() {
                // The runbooks could not be kept: the actions go back too, so a file is never
                // half merged.
                self.library = before.clone();
                self.keep(&before);
                return Err(SyncNotice::File(TransferError::RunbooksUnsaved));
            }
            merged
        };
        // The digests the runbooks now agree with are kept; losing them only makes the next
        // sync keep a differing runbook instead of replacing it.
        self.sync.persist();
        let mut kept_local = report.kept_local.clone();
        if let Some(merged) = &merged {
            kept_local.extend(merged.kept_local.iter().cloned());
        }
        Ok(Pulled {
            commits: output.commits,
            empty: false,
            actions: ImportReport {
                imported: report.imported,
                updated: report.updated,
                skipped: report.skipped,
                first_skipped: report.first_skipped,
                touched: report.touched,
            },
            runbooks: merged.map(|merged| RunbookImport {
                imported: merged.imported,
                updated: merged.updated,
                skipped: merged.skipped,
                first_updated: None,
                first_skipped: merged.first_skipped,
            }),
            kept_local,
        })
    }

    /// The panel: the settings and the three buttons.
    pub(super) fn sync_view<'a>(&'a self, tab: TabId) -> Element<'a, Message> {
        let send = move |message| {
            Message::Tool(
                tab,
                ToolMessage::CmdLib(CommandLibraryMessage::Sync(message)),
            )
        };
        let settings = &self.sync.state.settings;
        let field = |label: String, value: &str, which: SyncField| {
            row![
                text(label)
                    .size(font_size::BODY)
                    .width(super::EDITOR_LABEL_WIDTH),
                text_input("", value)
                    .size(font_size::BODY)
                    .padding(crate::tools::INPUT_PADDING)
                    .style(styles::text_input)
                    .on_input(move |typed| send(SyncMessage::Field(which, typed))),
            ]
            .spacing(spacing::SM)
            .align_y(iced::Alignment::Center)
        };
        let ready = !self.sync.busy;
        let action = |label: String, message: SyncMessage, primary: bool| {
            button(text(label).size(font_size::BODY))
                .padding(crate::tools::COPY_PADDING)
                .style(if primary {
                    styles::primary
                } else {
                    styles::secondary
                })
                .on_press_maybe(ready.then(|| send(message)))
        };
        let mut panel = column![
            text(fl!("ui-tool-cmdlib-sync-title"))
                .size(font_size::BODY_LARGE)
                .font(styles::SEMIBOLD),
            text(fl!("ui-tool-cmdlib-sync-hint"))
                .size(font_size::CAPTION)
                .style(text::secondary),
            field(
                fl!("ui-tool-cmdlib-sync-url"),
                &settings.remote_url,
                SyncField::Url
            ),
            field(
                fl!("ui-tool-cmdlib-sync-branch"),
                &settings.branch,
                SyncField::Branch
            ),
            field(
                fl!("ui-tool-cmdlib-sync-author-name"),
                &settings.author_name,
                SyncField::AuthorName
            ),
            field(
                fl!("ui-tool-cmdlib-sync-author-email"),
                &settings.author_email,
                SyncField::AuthorEmail
            ),
            row![
                action(fl!("ui-tool-cmdlib-sync-save"), SyncMessage::Save, false),
                action(fl!("ui-tool-cmdlib-sync-pull"), SyncMessage::Pull, false),
                action(fl!("ui-tool-cmdlib-sync-push"), SyncMessage::Push, false),
                action(fl!("ui-tool-cmdlib-sync-sync"), SyncMessage::Sync, true),
            ]
            .spacing(spacing::SM),
        ]
        .spacing(spacing::XS);
        if self.sync.busy {
            panel = panel.push(
                text(fl!("ui-tool-cmdlib-sync-working"))
                    .size(font_size::CAPTION)
                    .style(text::secondary),
            );
        }
        panel.into()
    }
}

/// What a notice says, and whether it is a failure.
pub(super) fn notice_text(notice: &SyncNotice) -> (String, bool) {
    match notice {
        SyncNotice::Settings(issue) => (settings_issue_text(*issue), true),
        SyncNotice::Saved => (fl!("ui-tool-cmdlib-sync-saved"), false),
        SyncNotice::StateUnreadable => (fl!("ui-tool-cmdlib-sync-state-unreadable"), true),
        SyncNotice::StateUnsaved => (fl!("ui-tool-cmdlib-sync-state-unsaved"), true),
        SyncNotice::LibraryUnavailable => (fl!("ui-tool-cmdlib-library-unreadable"), true),
        SyncNotice::LibraryUnsaved => (fl!("ui-tool-cmdlib-sync-library-unsaved"), true),
        SyncNotice::NothingToPush => (fl!("ui-tool-cmdlib-sync-nothing-to-push"), true),
        SyncNotice::File(error) => (super::transfer_error_text(error), true),
        SyncNotice::Failed(failure) => (failure_text(failure), true),
        SyncNotice::Done { pulled, pushed } => (done_text(pulled.as_ref(), *pushed), false),
    }
}

/// What a completed synchronization says: the pull, the push, then what was kept.
fn done_text(pulled: Option<&Pulled>, pushed: Option<bool>) -> String {
    let mut lines: Vec<String> = Vec::new();
    if let Some(pulled) = pulled {
        lines.push(if pulled.empty {
            fl!("ui-tool-cmdlib-sync-pulled-empty")
        } else {
            fl!(
                "ui-tool-cmdlib-sync-pulled",
                commits = pulled.commits,
                done = import_text(&pulled.actions, pulled.runbooks.as_ref())
            )
        });
    }
    match pushed {
        Some(true) => lines.push(fl!("ui-tool-cmdlib-sync-pushed")),
        Some(false) => lines.push(fl!("ui-tool-cmdlib-sync-pushed-nothing")),
        None => {}
    }
    if let Some(pulled) = pulled.filter(|pulled| !pulled.kept_local.is_empty()) {
        lines.push(kept_local_text(&pulled.kept_local));
    }
    lines.join("\n")
}

/// How many actions were kept because the copy here is newer, and which.
fn kept_local_text(titles: &[String]) -> String {
    let listed = titles
        .iter()
        .take(MAX_LISTED_TITLES)
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join(TITLE_SEPARATOR);
    let names = if titles.len() > MAX_LISTED_TITLES {
        let unlisted = titles.len() - MAX_LISTED_TITLES;
        fl!(
            "ui-tool-cmdlib-sync-kept-local-more",
            titles = listed,
            count = unlisted
        )
    } else {
        listed
    };
    fl!(
        "ui-tool-cmdlib-sync-kept-local",
        count = titles.len(),
        titles = names
    )
}

/// Why a value of the settings is refused, in the window's language.
fn settings_issue_text(issue: SettingsIssue) -> String {
    match issue {
        SettingsIssue::UrlMissing => fl!("ui-tool-cmdlib-sync-issue-url-missing"),
        SettingsIssue::UrlOption => fl!("ui-tool-cmdlib-sync-issue-url-option"),
        SettingsIssue::UrlHidden => fl!("ui-tool-cmdlib-sync-issue-url-hidden"),
        SettingsIssue::UrlTooLong => fl!("ui-tool-cmdlib-sync-issue-url-long"),
        SettingsIssue::UrlScheme => fl!("ui-tool-cmdlib-sync-issue-url-scheme"),
        SettingsIssue::UrlFormat => fl!("ui-tool-cmdlib-sync-issue-url-format"),
        SettingsIssue::UrlCredentials => fl!("ui-tool-cmdlib-sync-issue-url-credentials"),
        SettingsIssue::BranchMissing => fl!("ui-tool-cmdlib-sync-issue-branch-missing"),
        SettingsIssue::BranchFormat => fl!("ui-tool-cmdlib-sync-issue-branch-format"),
        SettingsIssue::AuthorName => fl!("ui-tool-cmdlib-sync-issue-author-name"),
        SettingsIssue::AuthorEmail => fl!("ui-tool-cmdlib-sync-issue-author-email"),
    }
}

/// Why an operation failed, in the window's language. What git said is shown after the
/// sentence, its credentials already masked.
fn failure_text(failure: &SyncFailure) -> String {
    match failure {
        SyncFailure::Busy => fl!("ui-tool-cmdlib-sync-failed-busy"),
        SyncFailure::GitMissing => fl!("ui-tool-cmdlib-sync-failed-git-missing"),
        SyncFailure::Settings(issue) => settings_issue_text(*issue),
        SyncFailure::FolderNotOurs => fl!("ui-tool-cmdlib-sync-failed-folder-not-ours"),
        SyncFailure::Io(detail) => {
            fl!("ui-tool-cmdlib-sync-failed-io", detail = detail.as_str())
        }
        SyncFailure::TimedOut => fl!("ui-tool-cmdlib-sync-failed-timeout"),
        SyncFailure::Authentication(detail) => {
            fl!("ui-tool-cmdlib-sync-failed-auth", detail = detail.as_str())
        }
        SyncFailure::RemoteNotFound(detail) => fl!(
            "ui-tool-cmdlib-sync-failed-not-found",
            detail = detail.as_str()
        ),
        SyncFailure::Network(detail) => {
            fl!(
                "ui-tool-cmdlib-sync-failed-network",
                detail = detail.as_str()
            )
        }
        SyncFailure::PushRejected => fl!("ui-tool-cmdlib-sync-failed-rejected"),
        SyncFailure::FileTooLarge => fl!("ui-tool-cmdlib-sync-failed-too-large"),
        SyncFailure::Git(detail) => {
            fl!("ui-tool-cmdlib-sync-failed-git", detail = detail.as_str())
        }
    }
}

#[cfg(test)]
mod tests;
