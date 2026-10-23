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

//! The command library: the user's own commands, made, changed and deleted here, then the
//! shipped catalogue (the C# Heimdall's actions, read from its JSON files) filtered by
//! platform and by what is typed, in the language of the window. Each command shows its
//! description, the values it asks for and its command with them put in, copied by its own
//! button or sent to a terminal tab, once the user agrees to the question that follows.

use std::borrow::Cow;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use heimdall_app::{CapturedCommand, Delivery, SendTarget, SentCommand, TabId};
use heimdall_core::profile::RemoteOs;
use heimdall_core::tools::command_activity::{
    self, ACTIVITY_FILE, ActivityError, CommandActivity, HistoryEntry, HistoryMode,
};
use heimdall_core::tools::command_library::{
    self, CatalogueEntry, CommandIssue, LEGACY_SNIPPET_IDS,
};
use heimdall_core::tools::library_transfer::{self, RunbookImport};
use heimdall_core::tools::user_commands::{
    self, Criticality, Draft, DraftError, GenerateError, ImportReport, LIBRARY_FILE, LibraryError,
    TemplatePlatform, UserCommand, UserLibrary, UserParameter, UserPlatform, ValueError,
};
use iced::widget::{
    Column, Row, button, checkbox, column, container, row, scrollable, text, text_input,
};
use iced::{Element, Length, Task, window};

use super::command_library_runbooks::{Delivered, Ready, RunTarget, RunbookMessage, RunbooksPane};
use super::{CopySlot, ToolMessage, ToolPane};
use crate::file_dialog::{self, FileDialog};
use crate::i18n::fl;
use crate::shell::{Message, main_window_task};
use crate::styles;
use crate::tokens::{font_size, spacing};

/// Width of a value's label, as the result grids' label column.
const PARAM_LABEL_WIDTH: f32 = 160.0;

/// Width of the editor's field labels.
const EDITOR_LABEL_WIDTH: f32 = 140.0;

/// The most commands sent whose receipt is still awaited: a question left unanswered never
/// gets one, so the oldest go.
const MAX_AWAITED: usize = 8;

/// The separator of several terminal names in one line.
const NAME_SEPARATOR: &str = ", ";

/// What joins the tags of a command in the editor's field.
const TAG_JOINER: &str = ", ";

/// The commands of the catalogue shown at first, and added by each press of "show more": the
/// catalogue is hundreds of commands, and a card with its values is not cheap to draw.
const CATALOG_PAGE_SIZE: usize = 30;

/// The platforms the filter offers, in order; a command for both is under each.
const PLATFORM_FILTERS: [UserPlatform; 3] = [
    UserPlatform::Linux,
    UserPlatform::Windows,
    UserPlatform::Both,
];

/// A command of the library: one of the seeded catalog or one of the user's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandRef {
    /// A command of the catalogue, by identity.
    Catalog(String),
    /// A command of the user, by identity.
    User(String),
}

/// What the command library is asked.
#[derive(Debug, Clone)]
pub enum CommandLibraryMessage {
    /// The platform shown; `None` for every one.
    Platform(Option<UserPlatform>),
    /// The search typed.
    Search(String),
    /// Show more commands of the catalogue.
    ShowMore,
    /// Show more of the user's own commands.
    ShowMoreMine,
    /// Value `.1` of the catalogue's command `.0` typed.
    Param(String, String, String),
    /// The template of the catalogue's command `.0` shown.
    Template(String, TemplatePlatform),
    /// Value `.1` of the user's command `.0` typed.
    UserParam(String, String, String),
    /// The template of the user's command `.0` shown.
    UserTemplate(String, TemplatePlatform),
    /// Copy the command.
    Copy(CommandRef),
    /// Ask to send the command to the terminal chosen.
    Send(CommandRef),
    /// Type the command at the prompt of the terminal chosen, without Enter and without a
    /// question: nothing runs until the user presses Enter there.
    Insert(CommandRef),
    /// Insert the command last used, as the C# Shift+Enter.
    InsertSelected,
    /// The library belongs to a session: its platform starts the filter, its host and its
    /// user fill the parameters that take them, and the history can be limited to its host.
    Session {
        /// The session's host.
        host: Option<String>,
        /// The session's account.
        user: Option<String>,
        /// The platform of its far end; `Auto` when it cannot be told.
        platform: RemoteOs,
        /// Its terminal tab, which commands go to first.
        tab: Option<TabId>,
    },
    /// Limit the history to the session's host, or show every host's.
    HistoryThisHostOnly(bool),
    /// The terminal tab the commands are sent to.
    Target(TabId),
    /// Start a new command of the user.
    New,
    /// Start a new command of the user from a command captured in a terminal: its text is the
    /// pattern for the session's platform, at the level that runs, tagged with the protocol,
    /// and the title is left for the user. Nothing is saved before the user does.
    Capture(CapturedCommand),
    /// Edit the user's command of this identity.
    Edit(String),
    /// A field of the editor changed.
    Field(Field, String),
    /// The platforms of the command edited.
    EditPlatform(UserPlatform),
    /// The level of the command edited.
    EditLevel(Criticality),
    /// Keep the command edited.
    Save,
    /// Leave the editor.
    CancelEdit,
    /// Ask whether to delete the user's command of this identity.
    AskDelete(String),
    /// Delete it.
    ConfirmDelete(String),
    /// Keep it.
    CancelDelete,
    /// Star the command, or unstar it, as the C# `ToggleFavoriteByIdAsync`.
    ToggleFavorite(CommandRef),
    /// Show only the favorites, or all, as the C# `ToggleFavoritesFilter`.
    FavoritesOnly,
    /// Start a new command as a copy of this one, as the C# `DuplicateSelectedAsync`.
    Duplicate(CommandRef),
    /// Duplicate the command last used, as the C# Ctrl+D.
    DuplicateSelected,
    /// Show the history, or hide it.
    ToggleHistory,
    /// Put the history entry at this place back to use: asks to send it, never sends.
    Replay(usize),
    /// Copy the history entry at this place.
    CopyHistory(usize),
    /// Ask whether to clear the history.
    AskClearHistory,
    /// Clear it.
    ConfirmClearHistory,
    /// Keep it.
    CancelClearHistory,
    /// Tick the terminal for the broadcast, or untick it.
    BroadcastTarget(TabId),
    /// Tick every terminal.
    BroadcastAll,
    /// Untick every terminal.
    BroadcastNone,
    /// Ask to send the command to every terminal ticked.
    Broadcast(CommandRef),
    /// Ask for a file of commands to import.
    Import,
    /// The file picked was read; `None` when none was picked.
    Imported(Option<Result<String, TransferError>>),
    /// Open or close the question of what an export holds.
    Export,
    /// Ask where to write an export of `.0`.
    ExportScope(ExportScope),
    /// Something about the runbooks.
    Runbooks(RunbookMessage),
    /// The library was written to the file picked; `None` when none was picked.
    Exported(Option<Result<(), TransferError>>),
}

/// What an export writes, as the C# `CommandLibraryExportScope`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportScope {
    /// What the user created or imported, and their runbooks: the default.
    Mine,
    /// The commands the list shows now, filters and search applied; no runbook.
    CurrentFilter,
    /// The whole catalogue as well as the user's commands and runbooks.
    Everything,
}

/// Why a file of commands could not be imported or exported.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransferError {
    /// Larger than the C# limit.
    TooLarge,
    /// Not an export of the library.
    Format,
    /// More actions than an import takes.
    TooManyActions,
    /// More runbooks than an import takes.
    TooManyRunbooks,
    /// Written by another major version of the format.
    Version(String),
    /// The file's runbooks cannot be kept: nothing was imported.
    RunbooksUnsaved,
    /// Nothing to write for this choice.
    NothingToExport,
    /// What the system said.
    Failed(String),
}

/// A text field of the editor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    /// The title.
    Title,
    /// The description.
    Description,
    /// The category.
    Category,
    /// The tags.
    Tags,
    /// The PowerShell pattern.
    WindowsPattern,
    /// The POSIX shell pattern.
    LinuxPattern,
}

/// What an update asks of the window.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Nothing outside the pane.
    None,
    /// Copy `.1` by the button of `.0`.
    Copy(CopySlot, String),
    /// Ask whether to write `command` into terminal `target`.
    Send {
        /// The terminal tab.
        target: TabId,
        /// The command.
        command: String,
        /// Marked dangerous.
        dangerous: bool,
        /// A command typed as is, which no library action stands for.
        typed_as_is: bool,
    },
    /// Type `command` at the prompt of terminal `target`, no question asked.
    Insert {
        /// The terminal tab.
        target: TabId,
        /// The command.
        command: String,
    },
    /// Ask whether to write `command` into every terminal of `targets`.
    Broadcast {
        /// The terminal tabs.
        targets: Vec<TabId>,
        /// The command.
        command: String,
        /// Marked dangerous.
        dangerous: bool,
    },
    /// Ask for a file of commands and import it.
    Import,
    /// Ask where to write this export of the library.
    Export(String),
}

impl Outcome {
    /// What tab `tab` runs for an outcome that is the pane's own, its dialogs over the
    /// window `main`.
    pub fn task(self, tab: TabId, main: Option<window::Id>) -> Task<Message> {
        match self {
            Self::Import => pick_import(tab, main),
            Self::Export(text) => pick_export(tab, main, text),
            Self::None
            | Self::Copy(..)
            | Self::Send { .. }
            | Self::Insert { .. }
            | Self::Broadcast { .. } => Task::none(),
        }
    }
}

/// What the pane tells under its buttons.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Notice {
    /// A file imported: its actions, and its runbooks when it carried some.
    Imported(ImportReport, Option<RunbookImport>),
    /// The library exported.
    Exported,
    /// A file that could not be used.
    TransferFailed(TransferError),
    /// A broadcast written, to all of its terminals or not.
    Broadcast {
        delivered: usize,
        total: usize,
        unreached: Vec<String>,
    },
    /// A history entry whose command is gone.
    ReplayGone,
    /// A history entry that cannot be sent now: no terminal, or values refused.
    ReplayBlocked,
    /// The favorites and history could not be written.
    ActivityUnsaved,
    /// A command typed at the prompt of this terminal, waiting for the user's Enter.
    Inserted(String),
    /// A capture from a terminal that the editor open with changes in it refused.
    CaptureBlocked,
}

/// A command asked to be written, until the application says it was.
#[derive(Debug, Clone)]
struct Awaited {
    /// The command as it is typed.
    command: String,
    /// What the history keeps of it, but where and when; none for a command no library action
    /// stands for (a literal step of a runbook).
    entry: Option<HistoryEntry>,
    /// The step of the runbook run this command is, when it is one.
    run_step: Option<(u64, usize)>,
    /// The terminals asked, with their names.
    tabs: Vec<(TabId, String)>,
    /// Asked to several terminals at once.
    broadcast: bool,
    /// Whether it is run or only typed at the prompt.
    delivery: Delivery,
}

/// What a press on a step of a run asks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RunStep {
    /// Write it into the terminal and submit it, after the question.
    Send,
    /// Type it at the prompt.
    Insert,
    /// Copy it.
    Copy,
}

/// What the session the library was opened for gives it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Session {
    /// Its host.
    host: Option<String>,
    /// Its account.
    user: Option<String>,
}

/// The editor: the draft and why it was not kept.
#[derive(Debug, Clone, Default)]
struct Editor {
    draft: Draft,
    /// The draft as the editor opened on it, to tell whether the user changed anything.
    original: Draft,
    problem: Option<DraftError>,
}

impl Editor {
    /// An editor opened on `draft`.
    fn on(draft: Draft) -> Self {
        Self {
            original: draft.clone(),
            draft,
            problem: None,
        }
    }

    /// Whether the user changed the draft since the editor opened.
    fn is_changed(&self) -> bool {
        self.draft != self.original
    }
}

/// What went wrong with the user's file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Failure {
    /// It could not be read: it is left as it is and nothing is changed.
    Unreadable,
    /// The change could not be saved.
    Unsaved,
}

/// The command library's state.
#[derive(Debug, Default)]
pub struct CommandLibraryPane {
    platform: Option<UserPlatform>,
    search: String,
    /// The pages of the catalogue shown besides the first.
    more_pages: usize,
    /// The pages of the user's own commands shown besides the first.
    more_mine_pages: usize,
    values: HashMap<(String, String), String>,
    templates: HashMap<String, TemplatePlatform>,
    /// Where the user's commands are kept; `None` when the platform has no folder.
    file: Option<PathBuf>,
    library: UserLibrary,
    failure: Option<Failure>,
    user_values: HashMap<(String, String), String>,
    user_templates: HashMap<String, TemplatePlatform>,
    editor: Option<Editor>,
    deleting: Option<String>,
    targets: Vec<SendTarget>,
    target: Option<TabId>,
    /// Where the favorites and the history are kept.
    activity_file: Option<PathBuf>,
    activity: CommandActivity,
    /// Whether the activity file may be written: not when it could not be read.
    activity_writable: bool,
    favorites_only: bool,
    history_open: bool,
    clearing_history: bool,
    /// The command last used, which Ctrl+D copies.
    selected: Option<CommandRef>,
    /// The terminals ticked for a broadcast.
    broadcast: Vec<TabId>,
    awaited: Vec<Awaited>,
    notice: Option<Notice>,
    /// The session the library was opened for, when it was.
    session: Option<Session>,
    /// The terminal tab of the session the library was opened from.
    session_tab: Option<TabId>,
    /// Whether the history shows only the rows of the session's host: ticked when the
    /// library belongs to a session, as the C# default.
    history_this_host_only: bool,
    /// The runbooks: the panel and the file they are kept in.
    runbooks: RunbooksPane,
    /// Whether the question of what an export holds is open.
    export_choice: bool,
    /// The category of the command last created in this pane, which a captured command starts
    /// in, as the C# `LastCreatedCategory`.
    last_category: Option<String>,
}

impl CommandLibraryPane {
    /// A pane whose user commands are kept beside `profiles_file`.
    #[must_use]
    pub fn new(profiles_file: Option<&Path>) -> Self {
        let file = profiles_file
            .and_then(Path::parent)
            .map(|folder| folder.join(LIBRARY_FILE));
        let activity_file = profiles_file
            .and_then(Path::parent)
            .map(|folder| folder.join(ACTIVITY_FILE));
        let mut pane = Self {
            file,
            activity_file,
            activity_writable: true,
            ..Self::default()
        };
        if let Some(path) = &pane.activity_file {
            match CommandActivity::load(path) {
                Ok(activity) => pane.activity = activity,
                Err(ActivityError::Io(_) | ActivityError::Format) => {
                    pane.activity_writable = false;
                }
            }
        }
        // The ten commands the library had before the shipped catalogue keep their stars
        // and their history under the names of their successors.
        if pane.activity.rename_catalog_keys(&LEGACY_SNIPPET_IDS) {
            pane.keep_activity();
        }
        if let Some(path) = &pane.file {
            match UserLibrary::load(path) {
                Ok(library) => pane.library = library,
                Err(
                    LibraryError::Io(_)
                    | LibraryError::TooLarge
                    | LibraryError::Format
                    | LibraryError::TooManyActions
                    | LibraryError::TooManyRunbooks
                    | LibraryError::Version(_),
                ) => {
                    pane.failure = Some(Failure::Unreadable);
                }
            }
        }
        pane.runbooks = RunbooksPane::new(profiles_file, &pane.library);
        pane
    }

    /// The terminals commands can be sent to, as the application offers them now.
    pub fn set_targets(&mut self, targets: Vec<SendTarget>) {
        self.targets = targets;
    }

    /// The terminal a command goes to: the one chosen while it is still there, else the
    /// session the library was opened from while it is still offered, else the first.
    fn effective_target(&self) -> Option<&SendTarget> {
        let offered = |wanted: Option<TabId>| {
            wanted.and_then(|tab| self.targets.iter().find(|each| each.tab == tab))
        };
        offered(self.target)
            .or_else(|| offered(self.session_tab))
            .or_else(|| self.targets.first())
    }

    /// Applies `message`.
    pub fn update(&mut self, message: CommandLibraryMessage) -> Outcome {
        match message {
            CommandLibraryMessage::Platform(platform) => {
                self.platform = platform;
                self.more_pages = 0;
                self.more_mine_pages = 0;
            }
            CommandLibraryMessage::Search(typed) => {
                self.search = typed;
                self.more_pages = 0;
                self.more_mine_pages = 0;
            }
            CommandLibraryMessage::ShowMore => self.more_pages += 1,
            CommandLibraryMessage::ShowMoreMine => self.more_mine_pages += 1,
            CommandLibraryMessage::Param(id, name, value) => {
                self.values.insert((id, name), value);
            }
            CommandLibraryMessage::Template(id, platform) => {
                self.templates.insert(id, platform);
            }
            CommandLibraryMessage::UserParam(id, name, value) => {
                self.user_values.insert((id, name), value);
            }
            CommandLibraryMessage::UserTemplate(id, platform) => {
                self.user_templates.insert(id, platform);
            }
            CommandLibraryMessage::Copy(command) => {
                self.selected = Some(command.clone());
                return match self.command_text(&command) {
                    Some(text) => {
                        self.record_copy(&command);
                        Outcome::Copy(self.copy_slot(&command), text)
                    }
                    None => Outcome::None,
                };
            }
            CommandLibraryMessage::Send(command) => {
                self.selected = Some(command.clone());
                return self.send(&command);
            }
            CommandLibraryMessage::Insert(command) => {
                self.selected = Some(command.clone());
                return self.insert(&command);
            }
            // The key is the library's own only while no editor has the keyboard.
            CommandLibraryMessage::InsertSelected => {
                if self.editor.is_none()
                    && !self.runbooks.is_open()
                    && let Some(command) = self.selected.clone()
                {
                    return self.insert(&command);
                }
            }
            CommandLibraryMessage::Session {
                host,
                user,
                platform,
                tab,
            } => self.open_for_session(host, user, platform, tab),
            CommandLibraryMessage::HistoryThisHostOnly(on) => self.history_this_host_only = on,
            CommandLibraryMessage::Broadcast(command) => {
                self.selected = Some(command.clone());
                return self.broadcast_outcome(&command);
            }
            CommandLibraryMessage::Target(tab) => {
                self.target = Some(tab);
                self.runbooks.repin(tab);
            }
            CommandLibraryMessage::Runbooks(message) => return self.runbooks_message(message),
            CommandLibraryMessage::Import if self.can_edit() => return Outcome::Import,
            CommandLibraryMessage::Export => self.export_choice = !self.export_choice,
            CommandLibraryMessage::ExportScope(scope) => return self.export(scope),
            CommandLibraryMessage::Replay(index) => return self.replay(index),
            CommandLibraryMessage::CopyHistory(index) => {
                return match self.activity.history().get(index) {
                    Some(entry) => {
                        Outcome::Copy(CopySlot::CmdLibHistory(index), entry.pattern.clone())
                    }
                    None => Outcome::None,
                };
            }
            other => self.edit(other),
        }
        Outcome::None
    }

    /// The messages about the favorites, the history, the broadcast and the files.
    fn activity_message(&mut self, message: CommandLibraryMessage) {
        match message {
            CommandLibraryMessage::ToggleFavorite(command) => {
                self.selected = Some(command.clone());
                let key = self.key_of(&command);
                self.activity.toggle_favorite(&key);
                self.keep_activity();
            }
            CommandLibraryMessage::FavoritesOnly => {
                self.favorites_only = !self.favorites_only;
                self.more_pages = 0;
                self.more_mine_pages = 0;
            }
            CommandLibraryMessage::Duplicate(command) => self.duplicate(&command),
            CommandLibraryMessage::DuplicateSelected => {
                if let Some(command) = self.selected.clone() {
                    self.duplicate(&command);
                }
            }
            CommandLibraryMessage::ToggleHistory => {
                self.history_open = !self.history_open;
                self.clearing_history = false;
                if self.history_open {
                    self.runbooks.close();
                }
            }
            CommandLibraryMessage::AskClearHistory => self.clearing_history = true,
            CommandLibraryMessage::CancelClearHistory => self.clearing_history = false,
            CommandLibraryMessage::ConfirmClearHistory => {
                self.clearing_history = false;
                self.activity.clear_history();
                self.keep_activity();
            }
            CommandLibraryMessage::BroadcastTarget(tab) => {
                if let Some(index) = self.broadcast.iter().position(|each| *each == tab) {
                    self.broadcast.remove(index);
                } else if self.targets.iter().any(|each| each.tab == tab) {
                    self.broadcast.push(tab);
                }
            }
            CommandLibraryMessage::BroadcastAll => {
                self.broadcast = self.targets.iter().map(|each| each.tab).collect();
            }
            CommandLibraryMessage::BroadcastNone => self.broadcast.clear(),
            CommandLibraryMessage::Imported(Some(result)) => self.imported(result),
            CommandLibraryMessage::Exported(Some(result)) => {
                self.notice = Some(match result {
                    Ok(()) => Notice::Exported,
                    Err(error) => Notice::TransferFailed(error),
                });
            }
            _ => {}
        }
    }

    /// The file read is added to the library, as the C# import, and said what it did. The
    /// file is held to its schema and its counts first; its actions are then kept, and its
    /// runbooks against the library as those actions left it, so a runbook may use an action
    /// of the same file.
    fn imported(&mut self, result: Result<String, TransferError>) {
        let text = match result {
            Ok(text) => text,
            Err(error) => {
                self.notice = Some(Notice::TransferFailed(error));
                return;
            }
        };
        if !self.can_edit() {
            return;
        }
        let runbooks = match library_transfer::precheck(&text) {
            Ok(runbooks) => runbooks,
            Err(error) => {
                self.notice = Some(Notice::TransferFailed(transfer_error(error)));
                return;
            }
        };
        // Refused before anything is kept, so a file is never half imported for this reason.
        if !runbooks.is_empty() && !self.runbooks.can_edit() {
            self.notice = Some(Notice::TransferFailed(TransferError::RunbooksUnsaved));
            return;
        }
        let before = self.library.clone();
        match self.library.import(&text) {
            Ok(report) => {
                // A level in a file is its author's claim: a destructive pattern is
                // dangerous whatever the file says.
                self.library.raise_dangerous(&report.touched, |pattern| {
                    heimdall_app::dangerous_command(pattern).is_some()
                });
                if !self.keep(&before) {
                    return;
                }
                let kept = if runbooks.is_empty() {
                    Some(None)
                } else {
                    self.runbooks.import(&runbooks, &self.library).map(Some)
                };
                self.notice = Some(if let Some(runbooks) = kept {
                    Notice::Imported(report, runbooks)
                } else {
                    // The runbooks could not be kept: the actions go back too, so a file is
                    // never half imported.
                    self.library = before.clone();
                    self.keep(&before);
                    Notice::TransferFailed(TransferError::RunbooksUnsaved)
                });
            }
            Err(error) => self.notice = Some(Notice::TransferFailed(transfer_error(error))),
        }
    }

    /// What an export of `scope` writes, asked to be saved where the user says; says so when
    /// there is nothing to write.
    fn export(&mut self, scope: ExportScope) -> Outcome {
        self.export_choice = false;
        let user: Vec<&UserCommand> = match scope {
            ExportScope::CurrentFilter => self.shown_user(),
            ExportScope::Mine | ExportScope::Everything => self.library.commands().iter().collect(),
        };
        // The shipped actions go out whole, their translations too, as the C# `Action`.
        let shipped: Vec<_> = match scope {
            ExportScope::CurrentFilter => self
                .shown()
                .into_iter()
                .map(library_transfer::shipped_value)
                .collect(),
            ExportScope::Everything => command_library::shipped()
                .entries()
                .iter()
                .map(library_transfer::shipped_value)
                .collect(),
            ExportScope::Mine => Vec::new(),
        };
        let runbooks = match scope {
            ExportScope::CurrentFilter => Vec::new(),
            ExportScope::Mine | ExportScope::Everything => self.runbooks.exportable(&self.library),
        };
        let actions: Vec<_> = shipped
            .into_iter()
            .chain(user.into_iter().map(library_transfer::action_value))
            .collect();
        let text = (!actions.is_empty() || !runbooks.is_empty())
            .then(|| library_transfer::export_text(&actions, &runbooks));
        match text {
            Some(text) => {
                self.notice = None;
                Outcome::Export(text)
            }
            None => {
                self.notice = Some(Notice::TransferFailed(TransferError::NothingToExport));
                Outcome::None
            }
        }
    }

    /// The name a command is kept under in the favorites and the history.
    fn key_of(&self, command: &CommandRef) -> String {
        match command {
            CommandRef::Catalog(id) => command_activity::catalog_key(id),
            CommandRef::User(id) => command_activity::user_key(id),
        }
    }

    /// The command named `key` in the favorites or the history, when it is still here.
    fn command_of_key(&self, key: &str) -> Option<CommandRef> {
        if let Some(name) = key.strip_prefix(command_activity::CATALOG_KEY_PREFIX) {
            return command_library::shipped()
                .get(name)
                .map(|entry| CommandRef::Catalog(entry.command().id.clone()));
        }
        let id = key.strip_prefix(command_activity::USER_KEY_PREFIX)?;
        self.library
            .get(id)
            .map(|_| CommandRef::User(id.to_owned()))
    }

    /// Whether `command` is a favorite.
    fn is_favorite(&self, command: &CommandRef) -> bool {
        self.activity.is_favorite(&self.key_of(command))
    }

    /// Writes the favorites and the history; says so when that fails. Nothing is written
    /// over a file that could not be read.
    fn keep_activity(&mut self) {
        let Some(path) = &self.activity_file else {
            return;
        };
        if !self.activity_writable {
            self.notice = Some(Notice::ActivityUnsaved);
            return;
        }
        if self.activity.save(path).is_err() {
            self.notice = Some(Notice::ActivityUnsaved);
        }
    }

    /// Opens the editor on a new command that starts as a copy of `command`.
    fn duplicate(&mut self, command: &CommandRef) {
        if !self.can_edit() || self.editor.is_some() {
            return;
        }
        let draft = match command {
            CommandRef::User(id) => self.library.get(id).map(|each| {
                Draft::copy_of(
                    each,
                    fl!(
                        "ui-tool-cmdlib-duplicate-title",
                        title = each.title.as_str()
                    ),
                )
            }),
            CommandRef::Catalog(id) => {
                catalogue_entry(id).map(|entry| catalogue_draft(&entry.localized(&language())))
            }
        };
        if let Some(draft) = draft {
            self.editor = Some(Editor::on(draft));
            self.history_open = false;
        }
    }

    /// Records the commands the application wrote, in the history, and says what a broadcast
    /// reached. Those not asked by this pane are not its own.
    pub fn record_sent(&mut self, receipts: &[SentCommand]) {
        if self.awaited.is_empty() || receipts.is_empty() {
            return;
        }
        for awaited in std::mem::take(&mut self.awaited) {
            let reached: Vec<(TabId, String)> = awaited
                .tabs
                .iter()
                .filter(|(tab, _)| {
                    receipts.iter().any(|each| {
                        each.tab == *tab
                            && each.command == awaited.command
                            && each.delivery == awaited.delivery
                    })
                })
                .cloned()
                .collect();
            if reached.is_empty() {
                self.awaited.push(awaited);
                continue;
            }
            if let Some(mut entry) = awaited.entry.clone() {
                entry.target = reached
                    .iter()
                    .map(|(_, name)| name.as_str())
                    .collect::<Vec<_>>()
                    .join(NAME_SEPARATOR);
                entry.at = now_unix();
                entry.mode = match awaited.delivery {
                    Delivery::Run => HistoryMode::Sent,
                    Delivery::Inserted => HistoryMode::Inserted,
                };
                self.activity.record(entry);
                self.keep_activity();
            }
            // The run goes on only once the terminal has the step: a question answered no
            // leaves it where it is.
            if let Some((run_id, index)) = awaited.run_step {
                self.runbooks.step_delivered(
                    run_id,
                    index,
                    match awaited.delivery {
                        Delivery::Run => Delivered::Sent,
                        Delivery::Inserted => Delivered::Inserted,
                    },
                );
            }
            if awaited.delivery == Delivery::Inserted {
                self.notice = Some(Notice::Inserted(
                    reached
                        .iter()
                        .map(|(_, name)| name.as_str())
                        .collect::<Vec<_>>()
                        .join(NAME_SEPARATOR),
                ));
            }
            if awaited.broadcast {
                let unreached: Vec<&(TabId, String)> = awaited
                    .tabs
                    .iter()
                    .filter(|each| !reached.contains(each))
                    .collect();
                // As the C#: only what did not receive stays ticked, so a second press never
                // runs the command again where it already ran.
                self.broadcast
                    .retain(|tab| unreached.iter().any(|(each, _)| each == tab));
                self.notice = Some(Notice::Broadcast {
                    delivered: reached.len(),
                    total: awaited.tabs.len(),
                    unreached: unreached.iter().map(|(_, name)| name.clone()).collect(),
                });
            }
        }
    }

    /// Starts the library on the session it was opened for, as the C# `AutoSelectPlatform` and
    /// `InitializeAsync` do: the platform filter on the session's platform (every platform when
    /// it cannot be told), its host and user kept to fill the parameters, and the history
    /// limited to its host.
    fn open_for_session(
        &mut self,
        host: Option<String>,
        user: Option<String>,
        platform: RemoteOs,
        tab: Option<TabId>,
    ) {
        self.session_tab = tab;
        let given = |value: Option<String>| {
            value
                .map(|text| text.trim().to_owned())
                .filter(|text| !text.is_empty())
        };
        let session = Session {
            host: given(host),
            user: given(user),
        };
        self.runbooks.session_changed();
        self.runbooks
            .set_session(session.host.clone(), session.user.clone(), platform);
        self.history_this_host_only = session.host.is_some();
        self.session = (session != Session::default()).then_some(session);
        self.platform = command_library::session_platform_filter(platform);
        self.more_pages = 0;
        self.more_mine_pages = 0;
    }

    /// The host of the session the library was opened for.
    fn session_host(&self) -> Option<&str> {
        self.session
            .as_ref()
            .and_then(|session| session.host.as_deref())
    }

    /// The host the history is limited to: the session's, while "This host only" is ticked.
    fn history_host_filter(&self) -> Option<&str> {
        self.session_host().filter(|_| self.history_this_host_only)
    }

    /// The rows of the history the panel shows, each with its place in the whole history: all
    /// of them, or the session's host's alone while "This host only" is ticked. A row without a
    /// host is hidden under the filter: nothing says it went to this host.
    fn history_rows(&self) -> Vec<(usize, &HistoryEntry)> {
        let host_filter = self.history_host_filter();
        self.activity
            .history()
            .iter()
            .enumerate()
            .filter(|(_, entry)| host_filter.is_none_or(|host| entry.is_for_host(host)))
            .collect()
    }

    /// Records a copy in the history, against the session's host, as the C# does after a
    /// copy: it went to the clipboard, not to a terminal, so no terminal is named.
    fn record_copy(&mut self, command: &CommandRef) {
        let Some(mut entry) = self.history_entry(command) else {
            return;
        };
        entry.mode = HistoryMode::Copied;
        entry.at = now_unix();
        self.activity.record(entry);
        self.keep_activity();
    }

    /// The messages about the user's commands.
    fn edit(&mut self, message: CommandLibraryMessage) {
        match message {
            CommandLibraryMessage::New if self.can_edit() => {
                self.editor = Some(Editor::default());
                self.history_open = false;
            }
            // An editor with changes in it is never replaced by a capture: said, nothing lost.
            CommandLibraryMessage::Capture(_)
                if self.editor.as_ref().is_some_and(Editor::is_changed) =>
            {
                self.notice = Some(Notice::CaptureBlocked);
            }
            CommandLibraryMessage::Capture(capture) if self.can_edit() => {
                self.notice = None;
                self.history_open = false;
                self.editor = Some(Editor::on(captured_draft(
                    &capture,
                    self.last_category.as_deref(),
                )));
            }
            CommandLibraryMessage::Edit(id) if self.can_edit() => {
                if let Some(command) = self.library.get(&id) {
                    self.editor = Some(Editor::on(Draft::of(command)));
                    self.history_open = false;
                }
            }
            CommandLibraryMessage::Field(field, value) => {
                if let Some(editor) = &mut self.editor {
                    editor.problem = None;
                    let draft = &mut editor.draft;
                    match field {
                        Field::Title => draft.title = value,
                        Field::Description => draft.description = value,
                        Field::Category => draft.category = value,
                        Field::Tags => draft.tags = value,
                        Field::WindowsPattern => draft.windows_pattern = value,
                        Field::LinuxPattern => draft.linux_pattern = value,
                    }
                }
            }
            CommandLibraryMessage::EditPlatform(platform) => {
                if let Some(editor) = &mut self.editor {
                    editor.problem = None;
                    editor.draft.platform = platform;
                }
            }
            CommandLibraryMessage::EditLevel(level) => {
                if let Some(editor) = &mut self.editor {
                    editor.problem = None;
                    editor.draft.level = level;
                }
            }
            CommandLibraryMessage::Save => self.save_editor(),
            CommandLibraryMessage::CancelEdit => self.editor = None,
            CommandLibraryMessage::AskDelete(id) if self.can_edit() => self.deleting = Some(id),
            CommandLibraryMessage::ConfirmDelete(id) if self.can_edit() => {
                self.deleting = None;
                let before = self.library.clone();
                if self.library.delete(&id) {
                    self.keep(&before);
                    let key = command_activity::user_key(&id);
                    if self.activity.is_favorite(&key) {
                        self.activity.toggle_favorite(&key);
                        self.keep_activity();
                    }
                }
            }
            CommandLibraryMessage::CancelDelete => self.deleting = None,
            other => self.activity_message(other),
        }
    }

    /// Whether the user's commands can be changed: their file was read, and has a place.
    fn can_edit(&self) -> bool {
        self.failure != Some(Failure::Unreadable) && self.file.is_some()
    }

    /// Keeps the editor's draft: saved to the file, or said why not.
    fn save_editor(&mut self) {
        let Some(editor) = self.editor.as_mut() else {
            return;
        };
        let before = self.library.clone();
        let created = editor
            .draft
            .id
            .is_none()
            .then(|| editor.draft.category.trim().to_owned());
        match self.library.upsert(&editor.draft) {
            Ok(_) => {
                if self.keep(&before) {
                    self.editor = None;
                    if let Some(category) = created.filter(|category| !category.is_empty()) {
                        self.last_category = Some(category);
                    }
                }
            }
            Err(problem) => editor.problem = Some(problem),
        }
    }

    /// Writes the library to its file; when that fails the library goes back to `before`
    /// and the failure is said. Whether it was written.
    fn keep(&mut self, before: &UserLibrary) -> bool {
        let Some(path) = &self.file else {
            return false;
        };
        match self.library.save(path) {
            Ok(()) => {
                self.failure = None;
                true
            }
            Err(_) => {
                self.library = before.clone();
                self.failure = Some(Failure::Unsaved);
                false
            }
        }
    }

    /// The button that copies `command`.
    fn copy_slot(&self, command: &CommandRef) -> CopySlot {
        match command {
            CommandRef::Catalog(id) => {
                CopySlot::CmdLibCommand(command_library::shipped().position(id).unwrap_or_default())
            }
            CommandRef::User(id) => CopySlot::CmdLibUserCommand(
                self.library
                    .commands()
                    .iter()
                    .position(|each| &each.id == id)
                    .unwrap_or_default(),
            ),
        }
    }

    /// The command as it is copied or sent; `None` when a user's command cannot be made
    /// from the values typed.
    fn command_text(&self, command: &CommandRef) -> Option<String> {
        match command {
            CommandRef::Catalog(id) => catalogue_entry(id)
                .and_then(|entry| self.render(command, &entry.localized(&language())).ok()),
            CommandRef::User(id) => self
                .library
                .get(id)
                .and_then(|each| self.render(command, each).ok()),
        }
    }

    /// Whether `command` is marked dangerous, as the C# `DangerousCommandGuard` asks of it.
    fn is_dangerous(&self, command: &CommandRef, text: &str) -> bool {
        // The level a file or a user gave is not the only judge: the paste guard's list of
        // destructive commands asks too, as for any text about to reach a shell.
        heimdall_app::dangerous_command(text).is_some() || self.marked_dangerous(command)
    }

    /// Whether the level kept for `command` is the highest.
    fn marked_dangerous(&self, command: &CommandRef) -> bool {
        match command {
            CommandRef::Catalog(id) => catalogue_entry(id)
                .is_some_and(|entry| entry.command().level == Criticality::Dangerous),
            CommandRef::User(id) => self
                .library
                .get(id)
                .is_some_and(|each| each.level == Criticality::Dangerous),
        }
    }

    /// What the history keeps of `command` but where and when: its title, its pattern as
    /// written (each value a `{name}`, never what was typed) and the names of its values.
    fn history_entry(&self, command: &CommandRef) -> Option<HistoryEntry> {
        let (title, pattern, parameters) = match command {
            CommandRef::Catalog(id) => {
                let entry = catalogue_entry(id)?;
                let each = entry.localized(&language());
                let template = each.template(self.template_shown(command, &each)?)?;
                (
                    each.title.clone(),
                    template.command_pattern.clone(),
                    template.parameters.iter().map(|p| p.name.clone()).collect(),
                )
            }
            CommandRef::User(id) => {
                let each = self.library.get(id)?;
                let template = each.template(self.template_shown(command, each)?)?;
                (
                    each.title.clone(),
                    template.command_pattern.clone(),
                    template.parameters.iter().map(|p| p.name.clone()).collect(),
                )
            }
        };
        Some(HistoryEntry {
            command: self.key_of(command),
            title,
            pattern,
            parameters,
            target: String::new(),
            host: self.session_host().unwrap_or_default().to_owned(),
            mode: HistoryMode::Sent,
            at: 0,
        })
    }

    /// Keeps what a question about `command` written into `tabs` will have to record.
    fn await_receipt(
        &mut self,
        command: &CommandRef,
        text: &str,
        tabs: Vec<(TabId, String)>,
        broadcast: bool,
        delivery: Delivery,
    ) {
        let Some(entry) = self.history_entry(command) else {
            return;
        };
        // A command asked from a card supersedes the question of a run step still open: only
        // one question is ever on screen, so the run's was closed without sending, and its
        // text must not be taken for this one's receipt.
        self.awaited.retain(|each| each.run_step.is_none());
        if self.awaited.len() >= MAX_AWAITED {
            self.awaited.remove(0);
        }
        self.awaited.push(Awaited {
            command: text.to_owned(),
            entry: Some(entry),
            run_step: None,
            tabs,
            broadcast,
            delivery,
        });
    }

    /// Applies a message of the runbooks panel. The steps of a run reach a terminal through
    /// the very path a command of the library takes: the same refusal of a control
    /// character, the same paste encoding, the same targets (the session's own terminal
    /// first, never a remote desktop, a file transfer or a web view). Nothing here sends
    /// without the press of a button, and nothing sends twice for one.
    fn runbooks_message(&mut self, message: RunbookMessage) -> Outcome {
        match message {
            RunbookMessage::RunSend => self.run_step(RunStep::Send),
            RunbookMessage::RunInsert => self.run_step(RunStep::Insert),
            RunbookMessage::RunCopy => self.run_step(RunStep::Copy),
            other => {
                // What an earlier send said is not about the run, and the history is not the
                // view any more.
                if matches!(other, RunbookMessage::Toggle | RunbookMessage::Run(_)) {
                    self.notice = None;
                }
                if matches!(other, RunbookMessage::Toggle) {
                    self.history_open = false;
                }
                if matches!(other, RunbookMessage::RunStop | RunbookMessage::RunSkip) {
                    self.awaited.retain(|each| each.run_step.is_none());
                }
                self.runbooks.update(other, &self.library);
                // The terminal is pinned when the run starts.
                let tab = self.effective_target().map(|target| target.tab);
                self.runbooks.pin(tab);
                Outcome::None
            }
        }
    }

    /// What to tell about where the run's steps go, when they cannot go anywhere.
    fn run_warning(&self) -> Option<String> {
        match self.run_target() {
            RunTarget::Gone => Some(fl!("ui-tool-cmdlib-runbooks-run-target-gone")),
            RunTarget::Changed => Some(fl!("ui-tool-cmdlib-runbooks-run-session-changed")),
            // Terminals are open but none was there when the run began: it has to be told.
            RunTarget::NoTerminal if !self.targets.is_empty() => {
                Some(fl!("ui-tool-cmdlib-runbooks-run-pick-terminal"))
            }
            RunTarget::Ready(_) | RunTarget::NoTerminal => None,
        }
    }

    /// Where the steps of the run go: its pinned terminal while it is offered.
    fn run_target(&self) -> RunTarget {
        if self.runbooks.is_stale() {
            return RunTarget::Changed;
        }
        match self.runbooks.pinned() {
            Some(tab) if self.targets.iter().any(|each| each.tab == tab) => RunTarget::Ready(tab),
            Some(_) => RunTarget::Gone,
            None => RunTarget::NoTerminal,
        }
    }

    /// Sends, inserts or copies the current step of the run. The step is made and judged
    /// again now, from the variables and the library as they are: a step the generator refuses
    /// gives nothing.
    fn run_step(&mut self, how: RunStep) -> Outcome {
        // A press just after a delivery is a double click, not a wish to send the next step.
        if self.runbooks.is_guarded() {
            return Outcome::None;
        }
        let Some(ready) = self.runbooks.ready_step(&self.library) else {
            return Outcome::None;
        };
        let text = ready.step.command.clone();
        let host = self.session_host().unwrap_or_default().to_owned();
        let entry = |mode: HistoryMode, ready: &Ready| {
            ready.action.as_ref().map(|action| HistoryEntry {
                command: if command_library::shipped().get(&action.id).is_some() {
                    command_activity::catalog_key(&action.id)
                } else {
                    command_activity::user_key(&action.id)
                },
                title: action.title.clone(),
                pattern: action.pattern.clone(),
                parameters: action.parameters.clone(),
                target: String::new(),
                host: host.clone(),
                mode,
                at: now_unix(),
            })
        };
        if how == RunStep::Copy {
            if let Some(copied) = entry(HistoryMode::Copied, &ready) {
                self.activity.record(copied);
                self.keep_activity();
            }
            self.runbooks
                .step_delivered(ready.run_id, ready.index, Delivered::Copied);
            return Outcome::Copy(CopySlot::CmdLibRunbookStep, text);
        }
        // Always the terminal the run was pinned to, never the first one offered.
        let RunTarget::Ready(pinned) = self.run_target() else {
            return Outcome::None;
        };
        let Some(target) = self.targets.iter().find(|each| each.tab == pinned).cloned() else {
            return Outcome::None;
        };
        let delivery = if how == RunStep::Send {
            Delivery::Run
        } else {
            Delivery::Inserted
        };
        // One step is waited for at a time: an earlier question left unanswered is dropped.
        self.awaited.retain(|each| each.run_step.is_none());
        if self.awaited.len() >= MAX_AWAITED {
            self.awaited.remove(0);
        }
        let tab = target.tab;
        self.awaited.push(Awaited {
            command: text.clone(),
            entry: entry(HistoryMode::Sent, &ready),
            run_step: Some((ready.run_id, ready.index)),
            tabs: vec![(target.tab, target.name)],
            broadcast: false,
            delivery,
        });
        if how == RunStep::Send {
            Outcome::Send {
                target: tab,
                command: text,
                dangerous: ready.is_dangerous(),
                typed_as_is: ready.step.literal,
            }
        } else {
            Outcome::Insert {
                target: tab,
                command: text,
            }
        }
    }

    /// Asks the application to question the user about sending `command`.
    fn send(&mut self, command: &CommandRef) -> Outcome {
        let (Some(target), Some(text)) =
            (self.effective_target().cloned(), self.command_text(command))
        else {
            return Outcome::None;
        };
        let dangerous = self.is_dangerous(command, &text);
        self.await_receipt(
            command,
            &text,
            vec![(target.tab, target.name)],
            false,
            Delivery::Run,
        );
        Outcome::Send {
            target: target.tab,
            command: text,
            dangerous,
            typed_as_is: false,
        }
    }

    /// Asks the application to type `command` at the prompt of the terminal chosen. No
    /// question and no danger check: nothing runs, the user's own Enter is the confirmation, as
    /// the C# `Insert`. The text is the one Send would write, made by the same generator and
    /// refused for the same control characters.
    fn insert(&mut self, command: &CommandRef) -> Outcome {
        let (Some(target), Some(text)) =
            (self.effective_target().cloned(), self.command_text(command))
        else {
            return Outcome::None;
        };
        self.await_receipt(
            command,
            &text,
            vec![(target.tab, target.name)],
            false,
            Delivery::Inserted,
        );
        Outcome::Insert {
            target: target.tab,
            command: text,
        }
    }

    /// Asks the application to question the user about sending `command` to every terminal
    /// ticked and still offered, in one question.
    fn broadcast_outcome(&mut self, command: &CommandRef) -> Outcome {
        let chosen: Vec<(TabId, String)> = self
            .targets
            .iter()
            .filter(|each| self.broadcast.contains(&each.tab))
            .map(|each| (each.tab, each.name.clone()))
            .collect();
        let Some(text) = self.command_text(command).filter(|_| !chosen.is_empty()) else {
            return Outcome::None;
        };
        let dangerous = self.is_dangerous(command, &text);
        let targets = chosen.iter().map(|(tab, _)| *tab).collect();
        self.await_receipt(command, &text, chosen, true, Delivery::Run);
        Outcome::Broadcast {
            targets,
            command: text,
            dangerous,
        }
    }

    /// Puts the history entry at `index` back to use, as the C# `ReplayHistoryEntry`: the
    /// command it came from, with the template it ran on, is asked to be sent as it is
    /// now written. Only asked: the question that follows is the user's to answer.
    fn replay(&mut self, index: usize) -> Outcome {
        let Some(entry) = self.activity.history().get(index).cloned() else {
            return Outcome::None;
        };
        let Some(command) = self.command_of_key(&entry.command) else {
            self.notice = Some(Notice::ReplayGone);
            return Outcome::None;
        };
        self.selected = Some(command.clone());
        match &command {
            CommandRef::User(id) => {
                if let Some(found) = self.library.get(id)
                    && let Some(platform) = pattern_platform(found, &entry.pattern)
                {
                    self.user_templates.insert(id.clone(), platform);
                }
            }
            CommandRef::Catalog(id) => {
                if let Some(found) = catalogue_entry(id)
                    && let Some(platform) = pattern_platform(found.command(), &entry.pattern)
                {
                    self.templates.insert(id.clone(), platform);
                }
            }
        }
        self.history_open = false;
        let outcome = self.send(&command);
        if outcome == Outcome::None {
            self.notice = Some(Notice::ReplayBlocked);
        }
        outcome
    }

    /// The template of `command`, written `shown`, that is shown: the one chosen, else this
    /// system's.
    fn template_shown(
        &self,
        reference: &CommandRef,
        shown: &UserCommand,
    ) -> Option<TemplatePlatform> {
        let chosen = match reference {
            CommandRef::Catalog(id) => self.templates.get(id),
            CommandRef::User(id) => self.user_templates.get(id),
        }
        .copied()
        .filter(|platform| shown.template_platforms().contains(platform));
        chosen.or_else(|| shown.preferred_platform(cfg!(windows)))
    }

    /// The value typed for `name` of `reference`.
    fn typed(&self, reference: &CommandRef, name: &str) -> Option<&str> {
        let (values, id) = match reference {
            CommandRef::Catalog(id) => (&self.values, id),
            CommandRef::User(id) => (&self.user_values, id),
        };
        values
            .get(&(id.clone(), name.to_owned()))
            .map(String::as_str)
    }

    /// The value `parameter` of `reference` holds now: what was typed, even when emptied;
    /// else what the session the library was opened for gives it (its host for a host, its
    /// user for an account), as the C# `GetInitialParameterValue`. The value goes through the
    /// generator's checks and quoting like one typed.
    fn value(&self, reference: &CommandRef, parameter: &UserParameter) -> Option<Cow<'_, str>> {
        if let Some(typed) = self.typed(reference, &parameter.name) {
            return Some(Cow::Borrowed(typed));
        }
        let session = self.session.as_ref()?;
        command_library::session_value(parameter, session.host.as_deref(), session.user.as_deref())
            .map(Cow::Owned)
    }

    /// The command `shown`, of `reference`, with the values typed or given by the session.
    fn render(&self, reference: &CommandRef, shown: &UserCommand) -> Result<String, GenerateError> {
        let platform = self
            .template_shown(reference, shown)
            .ok_or(GenerateError::EmptyPattern)?;
        let template = shown
            .template(platform)
            .ok_or(GenerateError::EmptyPattern)?;
        let values: HashMap<&str, Cow<'_, str>> = template
            .parameters
            .iter()
            .filter_map(|parameter| {
                self.value(reference, parameter)
                    .map(|value| (parameter.name.as_str(), value))
            })
            .collect();
        let command = user_commands::generate(template, platform, |name| {
            values.get(name).map(|value| &**value)
        })?;
        // A line break would submit the first line before the rest is read, whatever the
        // pattern's author meant: no copy, no send, no receipt for such a command.
        if command.chars().any(|c| c.is_control()) {
            return Err(GenerateError::Multiline);
        }
        Ok(command)
    }

    /// The commands of the catalogue the platform, the favorites and the search keep.
    fn shown(&self) -> Vec<&'static CatalogueEntry> {
        let language = language();
        command_library::shipped()
            .entries()
            .iter()
            .filter(|entry| command_library::on_platform(entry.command().platform, self.platform))
            .filter(|entry| {
                !self.favorites_only
                    || self.is_favorite(&CommandRef::Catalog(entry.command().id.clone()))
            })
            .filter(|entry| {
                let query = self.search.trim();
                query.is_empty()
                    || command_library::matches_query(
                        &entry.localized(&language),
                        &category_name(&entry.command().category),
                        query,
                    )
                    // The English text is searched in any language, as the C# keeps it
                    // beside the translation.
                    || command_library::matches_query(
                        entry.command(),
                        &category_name(&entry.command().category),
                        query,
                    )
            })
            .collect()
    }

    /// The commands of the user the platform and the search keep.
    fn shown_user(&self) -> Vec<&UserCommand> {
        self.library
            .commands()
            .iter()
            .filter(|command| {
                !self.favorites_only || self.is_favorite(&CommandRef::User(command.id.clone()))
            })
            .filter(|command| command_library::on_platform(command.platform, self.platform))
            .filter(|command| {
                command_library::matches_query(command, &command.category, &self.search)
            })
            .collect()
    }

    /// The tool's page: the platforms and the search, the terminal commands go to, the
    /// editor when open, then the user's commands and the catalog.
    pub fn view<'a>(&'a self, tab: TabId, state: &ToolPane) -> Element<'a, Message> {
        let send = move |message| Message::Tool(tab, ToolMessage::CmdLib(message));
        let platform = |label: String, choice: Option<UserPlatform>| {
            button(text(label).size(font_size::BODY))
                .padding(super::COPY_PADDING)
                .style(if self.platform == choice {
                    styles::primary
                } else {
                    styles::secondary
                })
                .on_press(send(CommandLibraryMessage::Platform(choice)))
                .into()
        };
        let platforms = Row::with_children(
            std::iter::once(platform(fl!("ui-tool-cmdlib-platform-all"), None)).chain(
                PLATFORM_FILTERS
                    .into_iter()
                    .map(|choice| platform(user_platform_name(choice), Some(choice))),
            ),
        )
        .spacing(spacing::SM);
        let search = text_input(&fl!("ui-tool-cmdlib-search-placeholder"), &self.search)
            .size(font_size::BODY)
            .padding(super::INPUT_PADDING)
            .style(styles::text_input)
            .on_input(move |typed| send(CommandLibraryMessage::Search(typed)));
        let new = button(text(fl!("ui-tool-cmdlib-new")).size(font_size::BODY))
            .padding(super::COPY_PADDING)
            .style(styles::primary)
            .on_press_maybe(
                (self.can_edit() && self.editor.is_none())
                    .then(|| send(CommandLibraryMessage::New)),
            );
        let plain = |label: String, on: bool, message: Option<CommandLibraryMessage>| {
            button(text(label).size(font_size::BODY))
                .padding(super::COPY_PADDING)
                .style(if on {
                    styles::primary
                } else {
                    styles::secondary
                })
                .on_press_maybe(message.map(send))
        };
        let tools = row![
            new,
            plain(
                fl!("ui-tool-cmdlib-import"),
                false,
                self.can_edit().then_some(CommandLibraryMessage::Import)
            ),
            plain(
                fl!("ui-tool-cmdlib-export"),
                self.export_choice,
                Some(CommandLibraryMessage::Export)
            ),
            plain(
                fl!("ui-tool-cmdlib-favorites-only"),
                self.favorites_only,
                Some(CommandLibraryMessage::FavoritesOnly)
            ),
            plain(
                fl!("ui-tool-cmdlib-history"),
                self.history_open,
                Some(CommandLibraryMessage::ToggleHistory)
            ),
            plain(
                fl!("ui-tool-cmdlib-runbooks"),
                self.runbooks.is_open(),
                Some(CommandLibraryMessage::Runbooks(RunbookMessage::Toggle))
            ),
        ]
        .spacing(spacing::SM);
        // The runbooks have the whole view, as the C#: a panel squeezed in above the list of
        // commands left room for a few lines of it.
        if self.runbooks.is_open() {
            let mut page = column![tools].spacing(spacing::SM);
            if self.export_choice {
                page = page.push(export_choice_view(tab));
            }
            if let Some(notice) = &self.notice {
                page = page.push(notice_view(notice));
            }
            page = page.push(self.target_row(tab));
            let run_target = self.run_target();
            let warning = self.run_warning();
            let can_send = if self.runbooks.is_bound() {
                matches!(run_target, RunTarget::Ready(_))
            } else {
                self.effective_target().is_some()
            };
            let panel = self.runbooks.view(tab, &self.library, can_send, warning);
            return super::tool_body(
                page.push(
                    scrollable(panel.unwrap_or_else(|| column![].into())).height(Length::Fill),
                ),
            );
        }
        let shown = self.shown();
        let mine = self.shown_user();
        let count = shown.len() + mine.len();
        let mut page = column![
            row![platforms, search]
                .spacing(spacing::MD)
                .align_y(iced::Alignment::Center),
            tools,
            text(fl!("ui-tool-cmdlib-count", count = count))
                .size(font_size::CAPTION)
                .style(text::secondary),
        ]
        .spacing(spacing::SM);
        if self.export_choice {
            page = page.push(export_choice_view(tab));
        }
        if let Some(host) = self.session_host() {
            page = page.push(
                text(fl!("ui-tool-cmdlib-session", host = host))
                    .size(font_size::CAPTION)
                    .style(text::secondary),
            );
        }
        if let Some(failure) = self.failure {
            page = page.push(
                text(match failure {
                    Failure::Unreadable => fl!("ui-tool-cmdlib-library-unreadable"),
                    Failure::Unsaved => fl!("ui-tool-cmdlib-library-unsaved"),
                })
                .size(font_size::BODY)
                .style(styles::error_text),
            );
        }
        if let Some(notice) = &self.notice {
            page = page.push(notice_view(notice));
        }
        page = page.push(self.target_row(tab));
        if let Some(row) = self.broadcast_row(tab) {
            page = page.push(row);
        }
        let mut cards: Column<'a, Message> = Column::new().spacing(spacing::MD);
        if self.history_open {
            cards = cards.push(self.history_view(tab, state));
        }
        if let Some(editor) = &self.editor {
            cards = cards.push(editor_view(tab, editor));
        }
        if count == 0 && self.editor.is_none() && !self.history_open {
            return super::tool_body(page.push(super::empty_state(fl!("ui-tool-cmdlib-empty"))));
        }
        if !mine.is_empty() {
            cards = cards.push(
                text(fl!("ui-tool-cmdlib-section-mine"))
                    .size(font_size::BODY_LARGE)
                    .font(styles::SEMIBOLD),
            );
            let visible_mine =
                CATALOG_PAGE_SIZE.saturating_mul(self.more_mine_pages.saturating_add(1));
            let remaining_mine = mine.len().saturating_sub(visible_mine);
            for command in mine.into_iter().take(visible_mine) {
                cards = cards.push(self.user_card(tab, state, command));
            }
            if remaining_mine > 0 {
                cards = cards.push(
                    button(
                        text(fl!("ui-tool-cmdlib-show-more", count = remaining_mine))
                            .size(font_size::BODY),
                    )
                    .padding(super::COPY_PADDING)
                    .style(styles::secondary)
                    .on_press(send(CommandLibraryMessage::ShowMoreMine)),
                );
            }
            if !shown.is_empty() {
                cards = cards.push(
                    text(fl!("ui-tool-cmdlib-section-catalog"))
                        .size(font_size::BODY_LARGE)
                        .font(styles::SEMIBOLD),
                );
            }
        }
        let visible = CATALOG_PAGE_SIZE.saturating_mul(self.more_pages.saturating_add(1));
        let remaining = shown.len().saturating_sub(visible);
        for entry in shown.into_iter().take(visible) {
            cards = cards.push(self.catalogue_card(tab, state, entry));
        }
        if remaining > 0 {
            cards = cards.push(
                button(
                    text(fl!("ui-tool-cmdlib-show-more", count = remaining)).size(font_size::BODY),
                )
                .padding(super::COPY_PADDING)
                .style(styles::secondary)
                .on_press(send(CommandLibraryMessage::ShowMore)),
            );
        }
        super::tool_body(page.push(scrollable(cards).height(Length::Fill)))
    }

    /// Where commands are sent: the terminals offered, the one chosen marked; a hint when
    /// there is none.
    fn target_row<'a>(&'a self, tab: TabId) -> Element<'a, Message> {
        let send = move |message| Message::Tool(tab, ToolMessage::CmdLib(message));
        let Some(chosen) = self.effective_target() else {
            return text(fl!("ui-tool-cmdlib-send-none"))
                .size(font_size::CAPTION)
                .style(text::secondary)
                .into();
        };
        let mut targets = Row::new()
            .spacing(spacing::SM)
            .align_y(iced::Alignment::Center)
            .push(
                text(fl!("ui-tool-cmdlib-send-to"))
                    .size(font_size::CAPTION)
                    .style(text::secondary),
            );
        for target in &self.targets {
            targets = targets.push(
                button(text(target.name.clone()).size(font_size::BODY))
                    .padding(super::COPY_PADDING)
                    .style(if target.tab == chosen.tab {
                        styles::primary
                    } else {
                        styles::secondary
                    })
                    .on_press(send(CommandLibraryMessage::Target(target.tab))),
            );
        }
        targets.into()
    }

    /// The terminals a command can be broadcast to, each ticked or not, with the buttons that
    /// tick all or none; `None` when there are fewer than two terminals, one being sent to
    /// as it is.
    fn broadcast_row<'a>(&'a self, tab: TabId) -> Option<Element<'a, Message>> {
        if self.targets.len() < 2 {
            return None;
        }
        let send = move |message| Message::Tool(tab, ToolMessage::CmdLib(message));
        let mut targets = Row::new()
            .spacing(spacing::SM)
            .align_y(iced::Alignment::Center)
            .push(
                text(fl!("ui-tool-cmdlib-broadcast-to"))
                    .size(font_size::CAPTION)
                    .style(text::secondary),
            );
        for target in &self.targets {
            let id = target.tab;
            targets = targets.push(
                checkbox(self.broadcast.contains(&id))
                    .label(target.name.clone())
                    .on_toggle(move |_| send(CommandLibraryMessage::BroadcastTarget(id)))
                    .style(styles::checkbox)
                    .text_size(font_size::BODY),
            );
        }
        targets = targets
            .push(
                button(text(fl!("ui-tool-cmdlib-broadcast-all")).size(font_size::BODY))
                    .padding(super::COPY_PADDING)
                    .style(styles::secondary)
                    .on_press(send(CommandLibraryMessage::BroadcastAll)),
            )
            .push(
                button(text(fl!("ui-tool-cmdlib-broadcast-none")).size(font_size::BODY))
                    .padding(super::COPY_PADDING)
                    .style(styles::secondary)
                    .on_press(send(CommandLibraryMessage::BroadcastNone)),
            );
        Some(targets.into())
    }

    /// The history, the latest first: each row says what was sent, where and when, and is
    /// replayed or copied by its own buttons; a button clears it, once asked.
    fn history_view<'a>(&'a self, tab: TabId, state: &ToolPane) -> Element<'a, Message> {
        let send = move |message| Message::Tool(tab, ToolMessage::CmdLib(message));
        let mut body = Column::new().spacing(spacing::SM);
        body = body.push(
            text(fl!("ui-tool-cmdlib-history"))
                .size(font_size::BODY_LARGE)
                .font(styles::SEMIBOLD),
        );
        body = body.push(
            text(fl!("ui-tool-cmdlib-history-note"))
                .size(font_size::CAPTION)
                .style(text::secondary),
        );
        // The box exists only for a library that belongs to a session: its host is what the
        // rows are compared with.
        if self.session_host().is_some() {
            body = body.push(
                checkbox(self.history_this_host_only)
                    .label(fl!("ui-tool-cmdlib-history-this-host"))
                    .on_toggle(move |on| send(CommandLibraryMessage::HistoryThisHostOnly(on)))
                    .style(styles::checkbox)
                    .text_size(font_size::BODY),
            );
        }
        let rows = self.history_rows();
        if self.activity.history().is_empty() {
            body = body.push(text(fl!("ui-tool-cmdlib-history-empty")).size(font_size::BODY));
        } else if rows.is_empty() {
            body = body.push(text(fl!("ui-tool-cmdlib-history-empty-host")).size(font_size::BODY));
        }
        for (index, entry) in rows {
            let when = heimdall_app::local_time_text(entry.at);
            let status = match entry.mode {
                HistoryMode::Sent => fl!(
                    "ui-tool-cmdlib-history-sent",
                    target = entry.target.as_str(),
                    when = when
                ),
                HistoryMode::Inserted => fl!(
                    "ui-tool-cmdlib-history-inserted",
                    target = entry.target.as_str(),
                    when = when
                ),
                HistoryMode::Copied => fl!("ui-tool-cmdlib-history-copied", when = when),
            };
            let mut details = column![
                text(entry.title.as_str())
                    .size(font_size::BODY)
                    .font(styles::SEMIBOLD),
                text(status).size(font_size::CAPTION).style(text::secondary),
            ]
            .spacing(spacing::XS)
            .width(Length::Fill);
            if !entry.host.is_empty() {
                details = details.push(
                    text(fl!(
                        "ui-tool-cmdlib-history-host",
                        host = entry.host.as_str()
                    ))
                    .size(font_size::CAPTION)
                    .style(text::secondary),
                );
            }
            let line = row![
                details.push(
                    text(entry.pattern.as_str())
                        .font(super::BOX_FONT)
                        .size(font_size::BODY),
                ),
                button(text(fl!("ui-tool-cmdlib-history-replay")).size(font_size::BODY))
                    .padding(super::COPY_PADDING)
                    .style(styles::secondary)
                    .on_press(send(CommandLibraryMessage::Replay(index))),
                super::copy_button(
                    fl!("ui-tool-cmdlib-copy"),
                    state.copied(CopySlot::CmdLibHistory(index)),
                    send(CommandLibraryMessage::CopyHistory(index)),
                    super::COPY_PADDING,
                ),
            ]
            .spacing(spacing::SM)
            .align_y(iced::Alignment::Center);
            body = body.push(line);
        }
        if !self.activity.history().is_empty() {
            body = body.push(if self.clearing_history {
                row![
                    text(fl!("ui-tool-cmdlib-history-clear-question")).size(font_size::BODY),
                    button(text(fl!("ui-tool-cmdlib-history-clear-confirm")).size(font_size::BODY))
                        .padding(super::COPY_PADDING)
                        .style(styles::danger)
                        .on_press(send(CommandLibraryMessage::ConfirmClearHistory)),
                    button(text(fl!("ui-tool-cmdlib-delete-keep")).size(font_size::BODY))
                        .padding(super::COPY_PADDING)
                        .style(styles::secondary)
                        .on_press(send(CommandLibraryMessage::CancelClearHistory)),
                ]
                .spacing(spacing::SM)
                .align_y(iced::Alignment::Center)
            } else {
                row![
                    button(text(fl!("ui-tool-cmdlib-history-clear")).size(font_size::BODY))
                        .padding(super::COPY_PADDING)
                        .style(styles::secondary)
                        .on_press(send(CommandLibraryMessage::AskClearHistory))
                ]
            });
        }
        container(body)
            .padding(spacing::MD)
            .width(Length::Fill)
            .style(styles::card)
            .into()
    }

    /// The favorite, duplicate and broadcast buttons of `command`, `rendered` when its
    /// text can be made.
    fn extras<'a>(&self, tab: TabId, command: &CommandRef, rendered: bool) -> Row<'a, Message> {
        let send = move |message| Message::Tool(tab, ToolMessage::CmdLib(message));
        let favorite = self.is_favorite(command);
        let can_broadcast = rendered
            && self
                .targets
                .iter()
                .any(|each| self.broadcast.contains(&each.tab));
        row![
            button(
                text(if favorite {
                    fl!("ui-tool-cmdlib-unfavorite")
                } else {
                    fl!("ui-tool-cmdlib-favorite")
                })
                .size(font_size::BODY)
            )
            .padding(super::COPY_PADDING)
            .style(if favorite {
                styles::primary
            } else {
                styles::secondary
            })
            .on_press(send(CommandLibraryMessage::ToggleFavorite(command.clone()))),
            button(text(fl!("ui-tool-cmdlib-duplicate")).size(font_size::BODY))
                .padding(super::COPY_PADDING)
                .style(styles::secondary)
                .on_press_maybe(
                    (self.can_edit() && self.editor.is_none())
                        .then(|| send(CommandLibraryMessage::Duplicate(command.clone())))
                ),
            button(text(fl!("ui-tool-cmdlib-broadcast")).size(font_size::BODY))
                .padding(super::COPY_PADDING)
                .style(styles::secondary)
                .on_press_maybe(
                    can_broadcast.then(|| send(CommandLibraryMessage::Broadcast(command.clone())))
                ),
        ]
        .spacing(spacing::SM)
    }

    /// The copy and send buttons of `command`, with `rendered` its text or `None` when it
    /// cannot be made yet.
    fn actions<'a>(
        &self,
        tab: TabId,
        state: &ToolPane,
        command: &CommandRef,
        rendered: bool,
    ) -> Row<'a, Message> {
        let send = move |message| Message::Tool(tab, ToolMessage::CmdLib(message));
        let can_send = rendered && self.effective_target().is_some();
        row![
            super::copy_button(
                fl!("ui-tool-cmdlib-copy"),
                state.copied(self.copy_slot(command)),
                send(CommandLibraryMessage::Copy(command.clone())),
                super::COPY_PADDING,
            ),
            button(text(fl!("ui-tool-cmdlib-insert")).size(font_size::BODY))
                .padding(super::COPY_PADDING)
                .style(styles::secondary)
                .on_press_maybe(
                    can_send.then(|| send(CommandLibraryMessage::Insert(command.clone())))
                ),
            button(text(fl!("ui-tool-cmdlib-send")).size(font_size::BODY))
                .padding(super::COPY_PADDING)
                .style(styles::secondary)
                .on_press_maybe(
                    can_send.then(|| send(CommandLibraryMessage::Send(command.clone())))
                ),
        ]
        .spacing(spacing::SM)
    }

    /// One command of the catalogue, in the language of the window.
    fn catalogue_card<'a>(
        &'a self,
        tab: TabId,
        state: &ToolPane,
        entry: &'static CatalogueEntry,
    ) -> Element<'a, Message> {
        let shown = entry.localized(&language());
        let reference = CommandRef::Catalog(shown.id.clone());
        self.command_card(
            tab,
            state,
            &reference,
            &shown,
            category_name(&shown.category),
            entry.notes(&language()),
            None,
        )
    }

    /// One command: its title, platform, category and level, its description, the template
    /// to use when it has two, its values, its command with its buttons or why it cannot be
    /// made yet, and `manage` (a user's command's edit and delete buttons) under them.
    fn command_card<'a>(
        &'a self,
        tab: TabId,
        state: &ToolPane,
        reference: &CommandRef,
        command: &UserCommand,
        category: String,
        notes: Option<String>,
        manage: Option<Element<'a, Message>>,
    ) -> Element<'a, Message> {
        let send = move |message| Message::Tool(tab, ToolMessage::CmdLib(message));
        let shown = self.template_shown(reference, command);
        let header = row![
            text(command.title.clone())
                .size(font_size::BODY_LARGE)
                .font(styles::SEMIBOLD),
            text(user_platform_name(command.platform))
                .size(font_size::CAPTION)
                .style(text::secondary),
            text(category)
                .size(font_size::CAPTION)
                .style(text::secondary),
            text(level_name(command.level))
                .size(font_size::CAPTION)
                .style(if command.level == Criticality::Dangerous {
                    styles::error_text
                } else {
                    text::secondary
                }),
        ]
        .spacing(spacing::SM)
        .align_y(iced::Alignment::Center);
        let mut body = column![header].spacing(spacing::SM);
        if !command.description.is_empty() {
            body = body.push(
                text(command.description.clone())
                    .size(font_size::BODY)
                    .style(text::secondary),
            );
        }
        if let Some(notes) = notes.filter(|notes| !notes.trim().is_empty()) {
            body = body.push(text(notes).size(font_size::CAPTION).style(text::secondary));
        }
        // The command itself under its title, for the template shown, on one line, as the C#
        // row's `CommandPreview`.
        if let Some(preview) = shown
            .and_then(|platform| command.template(platform))
            .map(|template| command_library::command_preview(&template.command_pattern))
            .filter(|preview| !preview.is_empty())
        {
            body = body.push(
                text(preview)
                    .font(super::BOX_FONT)
                    .size(font_size::CAPTION)
                    .style(text::secondary),
            );
        }
        let platforms = command.template_platforms();
        if platforms.len() > 1 {
            body = body.push(Row::with_children(platforms.into_iter().map(|choice| {
                let id = command.id.clone();
                button(text(template_name(choice)).size(font_size::BODY))
                    .padding(super::COPY_PADDING)
                    .style(if shown == Some(choice) {
                        styles::primary
                    } else {
                        styles::secondary
                    })
                    .on_press(send(match reference {
                        CommandRef::Catalog(_) => CommandLibraryMessage::Template(id, choice),
                        CommandRef::User(_) => CommandLibraryMessage::UserTemplate(id, choice),
                    }))
                    .into()
            })));
        }
        if let Some(template) = shown.and_then(|platform| command.template(platform)) {
            body = body.push(
                Column::with_children(template.parameters.iter().map(|parameter| {
                    let id = command.id.clone();
                    let name = parameter.name.clone();
                    let value = self.value(reference, parameter).unwrap_or_default();
                    let catalog = matches!(reference, CommandRef::Catalog(_));
                    row![
                        text(if parameter.label.is_empty() {
                            parameter.name.clone()
                        } else {
                            parameter.label.clone()
                        })
                        .size(font_size::BODY)
                        .width(PARAM_LABEL_WIDTH),
                        text_input(parameter.default_value.as_deref().unwrap_or(""), &value)
                            .size(font_size::BODY)
                            .padding(super::INPUT_PADDING)
                            .style(styles::text_input)
                            .on_input(move |typed| {
                                send(if catalog {
                                    CommandLibraryMessage::Param(id.clone(), name.clone(), typed)
                                } else {
                                    CommandLibraryMessage::UserParam(
                                        id.clone(),
                                        name.clone(),
                                        typed,
                                    )
                                })
                            }),
                    ]
                    .spacing(spacing::SM)
                    .align_y(iced::Alignment::Center)
                    .into()
                }))
                .spacing(spacing::XS),
            );
        }
        let made = self.render(reference, command);
        body = body.push(self.extras(tab, reference, made.is_ok()));
        match made {
            Ok(rendered) => {
                body = body.push(command_box(
                    rendered,
                    self.actions(tab, state, reference, true),
                ));
            }
            Err(error) => {
                // A shipped command refused with nothing typed is refused for its own
                // defaults: say so, instead of blaming a value nobody wrote.
                let nothing_typed = command
                    .template(shown.unwrap_or(TemplatePlatform::Linux))
                    .is_none_or(|template| {
                        template.parameters.iter().all(|parameter| {
                            self.typed(reference, &parameter.name)
                                .is_none_or(|typed| typed.trim().is_empty())
                        })
                    });
                let unbuildable = matches!(reference, CommandRef::Catalog(_))
                    && nothing_typed
                    && matches!(&error, GenerateError::Values(values)
                        if values.iter().any(|each| matches!(each, ValueError::Dangerous(_))));
                body = body.push(
                    text(if unbuildable {
                        fl!("ui-tool-cmdlib-catalogue-unbuildable")
                    } else {
                        generate_error(&error)
                    })
                    .size(font_size::BODY)
                    .style(styles::error_text),
                );
            }
        }
        if let Some(manage) = manage {
            body = body.push(manage);
        }
        container(body)
            .padding(spacing::MD)
            .width(Length::Fill)
            .style(styles::card)
            .into()
    }

    /// One command of the user, with its edit and delete buttons.
    fn user_card<'a>(
        &'a self,
        tab: TabId,
        state: &ToolPane,
        command: &'a UserCommand,
    ) -> Element<'a, Message> {
        let send = move |message| Message::Tool(tab, ToolMessage::CmdLib(message));
        let manage: Element<'a, Message> = if self.deleting.as_deref() == Some(&command.id) {
            row![
                text(fl!(
                    "ui-tool-cmdlib-delete-question",
                    title = command.title.as_str()
                ))
                .size(font_size::BODY),
                button(text(fl!("ui-tool-cmdlib-delete")).size(font_size::BODY))
                    .padding(super::COPY_PADDING)
                    .style(styles::danger)
                    .on_press(send(CommandLibraryMessage::ConfirmDelete(
                        command.id.clone()
                    ))),
                button(text(fl!("ui-tool-cmdlib-delete-keep")).size(font_size::BODY))
                    .padding(super::COPY_PADDING)
                    .style(styles::secondary)
                    .on_press(send(CommandLibraryMessage::CancelDelete)),
            ]
            .spacing(spacing::SM)
            .align_y(iced::Alignment::Center)
            .into()
        } else {
            row![
                button(text(fl!("ui-tool-cmdlib-edit")).size(font_size::BODY))
                    .padding(super::COPY_PADDING)
                    .style(styles::secondary)
                    .on_press_maybe(
                        (self.editor.is_none())
                            .then(|| send(CommandLibraryMessage::Edit(command.id.clone())))
                    ),
                button(text(fl!("ui-tool-cmdlib-delete")).size(font_size::BODY))
                    .padding(super::COPY_PADDING)
                    .style(styles::secondary)
                    .on_press(send(CommandLibraryMessage::AskDelete(command.id.clone()))),
            ]
            .spacing(spacing::SM)
            .into()
        };
        self.command_card(
            tab,
            state,
            &CommandRef::User(command.id.clone()),
            command,
            command.category.clone(),
            None,
            Some(manage),
        )
    }
}

/// The command of the catalogue of this identity.
fn catalogue_entry(id: &str) -> Option<&'static CatalogueEntry> {
    command_library::shipped().get(id)
}

/// The language of the window, as a catalogue file names it.
pub(super) fn language() -> String {
    crate::i18n::current().code().to_owned()
}

/// The template of `command` whose pattern is `pattern`: the one a history row ran on.
fn pattern_platform(command: &UserCommand, pattern: &str) -> Option<TemplatePlatform> {
    command.template_platforms().into_iter().find(|platform| {
        command
            .template(*platform)
            .is_some_and(|template| template.command_pattern == pattern)
    })
}

/// The editor's draft for a copy of a command of the catalogue, shown in the language of the
/// window: its patterns keep their platforms, and a dangerous one stays so.
fn catalogue_draft(command: &UserCommand) -> Draft {
    let pattern_of = |platform| {
        command
            .template(platform)
            .map(|template| template.command_pattern.clone())
            .unwrap_or_default()
    };
    Draft {
        id: None,
        title: fl!(
            "ui-tool-cmdlib-duplicate-title",
            title = command.title.as_str()
        ),
        description: command.description.clone(),
        category: category_name(&command.category),
        platform: command.platform,
        level: command.level,
        windows_pattern: pattern_of(TemplatePlatform::Windows),
        linux_pattern: pattern_of(TemplatePlatform::Linux),
        tags: command.tags.join(TAG_JOINER),
    }
}

/// The editor's draft for a command captured from a terminal: its text in the pattern of the
/// session's platform and only there, at the level that runs, the protocol as a tag, no title
/// (the user names it) and the category of the last command made, if any.
fn captured_draft(capture: &CapturedCommand, category: Option<&str>) -> Draft {
    let text = capture.text.clone();
    let (platform, windows_pattern, linux_pattern) = match capture.platform {
        TemplatePlatform::Windows => (UserPlatform::Windows, text, String::new()),
        TemplatePlatform::Linux => (UserPlatform::Linux, String::new(), text),
    };
    Draft {
        id: None,
        category: category.unwrap_or_default().to_owned(),
        platform,
        level: Criticality::Run,
        windows_pattern,
        linux_pattern,
        tags: capture.tag.clone().unwrap_or_default(),
        ..Draft::default()
    }
}

/// Now, in seconds since 1970.
fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| {
            i64::try_from(since.as_secs()).unwrap_or(i64::MAX)
        })
}

/// What the pane tells, in the window's language.
fn notice_view<'a>(notice: &Notice) -> Element<'a, Message> {
    let (message, failed) = match notice {
        Notice::Imported(report, runbooks) => {
            let mut done = fl!(
                "ui-tool-cmdlib-import-done",
                imported = report.imported,
                updated = report.updated,
                skipped = report.skipped
            );
            if let Some(runbooks) = runbooks {
                done = fl!(
                    "ui-tool-cmdlib-import-runbooks",
                    done = done,
                    imported = runbooks.imported,
                    updated = runbooks.updated,
                    skipped = runbooks.skipped
                );
            }
            if let Some(name) = runbooks
                .as_ref()
                .and_then(|each| each.first_updated.as_ref())
            {
                done = fl!(
                    "ui-tool-cmdlib-import-replaced",
                    done = done,
                    name = name.as_str()
                );
            }
            // The first reason: the action's, else the runbook's.
            let first = report
                .first_skipped
                .as_ref()
                .map(|(title, issue)| (title.as_str(), issue_text(issue)))
                .or_else(|| {
                    runbooks.as_ref().and_then(|each| {
                        each.first_skipped
                            .as_ref()
                            .map(|(name, why)| (name.as_str(), refusal_text(why)))
                    })
                });
            match first {
                Some((title, reason)) => (
                    fl!(
                        "ui-tool-cmdlib-import-skipped",
                        done = done,
                        title = title,
                        reason = reason
                    ),
                    false,
                ),
                None => (done, false),
            }
        }
        Notice::Exported => (fl!("ui-tool-cmdlib-export-done"), false),
        Notice::TransferFailed(error) => (
            match error {
                TransferError::TooLarge => fl!("ui-tool-cmdlib-transfer-too-large"),
                TransferError::Format => fl!("ui-tool-cmdlib-transfer-format"),
                TransferError::TooManyActions => fl!(
                    "ui-tool-cmdlib-transfer-too-many",
                    max = user_commands::MAX_IMPORT_ACTIONS
                ),
                TransferError::TooManyRunbooks => fl!(
                    "ui-tool-cmdlib-transfer-too-many-runbooks",
                    max = heimdall_core::tools::runbooks::MAX_RUNBOOKS
                ),
                TransferError::Version(version) => fl!(
                    "ui-tool-cmdlib-transfer-version",
                    version = version.as_str()
                ),
                TransferError::RunbooksUnsaved => fl!("ui-tool-cmdlib-transfer-runbooks-unsaved"),
                TransferError::NothingToExport => fl!("ui-tool-cmdlib-export-nothing"),
                TransferError::Failed(detail) => {
                    fl!("ui-tool-cmdlib-transfer-failed", detail = detail.as_str())
                }
            },
            true,
        ),
        Notice::Broadcast {
            delivered,
            total,
            unreached,
        } => {
            let (delivered, total): (usize, usize) = (*delivered, *total);
            if unreached.is_empty() {
                (
                    fl!("ui-tool-cmdlib-broadcast-sent", count = delivered),
                    false,
                )
            } else {
                (
                    fl!(
                        "ui-tool-cmdlib-broadcast-partial",
                        delivered = delivered,
                        total = total,
                        names = unreached.join(NAME_SEPARATOR)
                    ),
                    true,
                )
            }
        }
        Notice::ReplayGone => (fl!("ui-tool-cmdlib-replay-gone"), true),
        Notice::ReplayBlocked => (fl!("ui-tool-cmdlib-replay-blocked"), true),
        Notice::CaptureBlocked => (fl!("ui-tool-cmdlib-capture-blocked"), true),
        Notice::ActivityUnsaved => (fl!("ui-tool-cmdlib-activity-unsaved"), true),
        Notice::Inserted(target) => (
            fl!("ui-tool-cmdlib-inserted", target = target.as_str()),
            false,
        ),
    };
    text(message)
        .size(font_size::BODY)
        .style(if failed {
            styles::error_text
        } else {
            text::secondary
        })
        .into()
}

/// The open dialog over the window `main`, then the file picked read and imported.
fn pick_import(tab: TabId, main: Option<window::Id>) -> Task<Message> {
    let title = fl!("ui-tool-cmdlib-import-title");
    let done = move |opened| {
        Message::Tool(
            tab,
            ToolMessage::CmdLib(CommandLibraryMessage::Imported(opened)),
        )
    };
    main_window_task(main).then(move |id| {
        let title = title.clone();
        let pick = match id {
            Some(id) => window::run(id, move |window| {
                FileDialog::new()
                    .set_title(title.clone())
                    .set_parent(window)
                    .pick_file()
            }),
            None => Task::done(FileDialog::new().set_title(title).pick_file()),
        };
        pick.then(move |pick: file_dialog::Pick<PathBuf>| {
            Task::perform(
                async move {
                    match pick.await {
                        Ok(Some(path)) => Some(read_import(path).await),
                        Ok(None) => None,
                        Err(unavailable) => Some(Err(TransferError::Failed(unavailable.message()))),
                    }
                },
                done,
            )
        })
    })
}

/// The text of the file at `path`, refused past the C# limit.
async fn read_import(path: PathBuf) -> Result<String, TransferError> {
    let failed = |error: std::io::Error| TransferError::Failed(error.to_string());
    let metadata = tokio::fs::metadata(&path).await.map_err(failed)?;
    if metadata.len() > user_commands::MAX_FILE_BYTES {
        return Err(TransferError::TooLarge);
    }
    let bytes = tokio::fs::read(&path).await.map_err(failed)?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// The save dialog over the window `main`, then `export` written to the file picked.
fn pick_export(tab: TabId, main: Option<window::Id>, export: String) -> Task<Message> {
    let title = fl!("ui-tool-cmdlib-export-title");
    let done = move |saved| {
        Message::Tool(
            tab,
            ToolMessage::CmdLib(CommandLibraryMessage::Exported(saved)),
        )
    };
    main_window_task(main).then(move |id| {
        let dialog = FileDialog::new()
            .set_title(title.clone())
            .set_file_name(LIBRARY_FILE);
        let export = export.clone();
        let pick = match id {
            Some(id) => window::run(id, move |window| dialog.set_parent(window).save_file()),
            None => Task::done(dialog.save_file()),
        };
        pick.then(move |pick: file_dialog::Pick<PathBuf>| {
            let export = export.clone();
            Task::perform(
                async move {
                    match pick.await {
                        Ok(Some(path)) => Some(
                            super::diagram_tool::write_file(path, export)
                                .await
                                .map(|_| ())
                                .map_err(|error| match error {
                                    super::diagram_tool::FileError::TooLarge => {
                                        TransferError::TooLarge
                                    }
                                    super::diagram_tool::FileError::Failed(detail) => {
                                        TransferError::Failed(detail)
                                    }
                                }),
                        ),
                        Ok(None) => None,
                        Err(unavailable) => Some(Err(TransferError::Failed(unavailable.message()))),
                    }
                },
                done,
            )
        })
    })
}

/// A command in its own box with its buttons at the right.
fn command_box<'a>(command: String, buttons: Row<'a, Message>) -> Element<'a, Message> {
    container(
        row![
            text(command)
                .font(super::BOX_FONT)
                .size(font_size::BODY)
                .width(Length::Fill),
            buttons,
        ]
        .spacing(spacing::SM)
        .align_y(iced::Alignment::Center),
    )
    .padding(spacing::SM)
    .width(Length::Fill)
    .style(styles::field_box)
    .into()
}

/// A labelled row of choice buttons in the editor.
fn choice_row<'a>(label: String, buttons: Vec<Element<'a, Message>>) -> Row<'a, Message> {
    row![
        text(label).size(font_size::BODY).width(EDITOR_LABEL_WIDTH),
        Row::with_children(buttons).spacing(spacing::SM),
    ]
    .spacing(spacing::SM)
    .align_y(iced::Alignment::Center)
}

/// The editor of a command of the user.
fn editor_view(tab: TabId, editor: &Editor) -> Element<'_, Message> {
    let send = move |message| Message::Tool(tab, ToolMessage::CmdLib(message));
    let draft = &editor.draft;
    let field = |label: String, value: &str, which: Field| {
        row![
            text(label).size(font_size::BODY).width(EDITOR_LABEL_WIDTH),
            text_input("", value)
                .size(font_size::BODY)
                .padding(super::INPUT_PADDING)
                .style(styles::text_input)
                .on_input(move |typed| send(CommandLibraryMessage::Field(which, typed))),
        ]
        .spacing(spacing::SM)
        .align_y(iced::Alignment::Center)
    };
    let choice = |label: String, on: bool, message: CommandLibraryMessage| {
        button(text(label).size(font_size::BODY))
            .padding(super::COPY_PADDING)
            .style(if on {
                styles::primary
            } else {
                styles::secondary
            })
            .on_press(send(message))
            .into()
    };
    let mut form = column![
        text(if draft.id.is_some() {
            fl!("ui-tool-cmdlib-editor-edit")
        } else {
            fl!("ui-tool-cmdlib-editor-new")
        })
        .size(font_size::BODY_LARGE)
        .font(styles::SEMIBOLD),
        field(
            fl!("ui-tool-cmdlib-field-title"),
            &draft.title,
            Field::Title
        ),
        field(
            fl!("ui-tool-cmdlib-field-description"),
            &draft.description,
            Field::Description
        ),
        field(
            fl!("ui-tool-cmdlib-field-category"),
            &draft.category,
            Field::Category
        ),
        field(fl!("ui-tool-cmdlib-field-tags"), &draft.tags, Field::Tags),
        choice_row(
            fl!("ui-tool-cmdlib-field-platform"),
            UserPlatform::ALL
                .into_iter()
                .map(|platform| choice(
                    user_platform_name(platform),
                    draft.platform == platform,
                    CommandLibraryMessage::EditPlatform(platform)
                ))
                .collect()
        ),
        choice_row(
            fl!("ui-tool-cmdlib-field-level"),
            Criticality::ALL
                .into_iter()
                .map(|level| choice(
                    level_name(level),
                    draft.level == level,
                    CommandLibraryMessage::EditLevel(level)
                ))
                .collect()
        ),
    ]
    .spacing(spacing::SM);
    if draft.platform != UserPlatform::Linux {
        form = form.push(field(
            fl!("ui-tool-cmdlib-field-windows-pattern"),
            &draft.windows_pattern,
            Field::WindowsPattern,
        ));
    }
    if draft.platform != UserPlatform::Windows {
        form = form.push(field(
            fl!("ui-tool-cmdlib-field-linux-pattern"),
            &draft.linux_pattern,
            Field::LinuxPattern,
        ));
    }
    form = form.push(
        text(fl!("ui-tool-cmdlib-field-pattern-hint"))
            .size(font_size::CAPTION)
            .style(text::secondary),
    );
    if let Some(problem) = editor.problem {
        form = form.push(
            text(draft_error(problem))
                .size(font_size::BODY)
                .style(styles::error_text),
        );
    }
    form = form.push(
        row![
            button(text(fl!("ui-tool-cmdlib-save")).size(font_size::BODY))
                .padding(super::PRIMARY_PADDING)
                .style(styles::primary)
                .on_press(send(CommandLibraryMessage::Save)),
            button(text(fl!("ui-tool-cmdlib-cancel")).size(font_size::BODY))
                .padding(super::PRIMARY_PADDING)
                .style(styles::secondary)
                .on_press(send(CommandLibraryMessage::CancelEdit)),
        ]
        .spacing(spacing::SM),
    );
    container(form)
        .padding(spacing::MD)
        .width(Length::Fill)
        .style(styles::card)
        .into()
}

/// The name of the platforms of a command of the user.
fn user_platform_name(platform: UserPlatform) -> String {
    match platform {
        UserPlatform::Windows => fl!("ui-tool-cmdlib-platform-windows"),
        UserPlatform::Linux => fl!("ui-tool-cmdlib-platform-linux"),
        UserPlatform::Both => fl!("ui-tool-cmdlib-platform-both"),
    }
}

/// The name of the template of `platform`: the shell it is written for.
fn template_name(platform: TemplatePlatform) -> String {
    match platform {
        TemplatePlatform::Windows => fl!("ui-tool-cmdlib-template-windows"),
        TemplatePlatform::Linux => fl!("ui-tool-cmdlib-template-linux"),
    }
}

/// The name of `level`.
fn level_name(level: Criticality) -> String {
    match level {
        Criticality::Info => fl!("ui-tool-cmdlib-level-info"),
        Criticality::Run => fl!("ui-tool-cmdlib-level-run"),
        Criticality::Dangerous => fl!("ui-tool-cmdlib-level-dangerous"),
    }
}

/// The name of the category `key`, in the window's language; a category the user typed is
/// shown as typed.
pub(super) fn category_name(key: &str) -> String {
    match key {
        "security" => fl!("ui-tool-cmdlib-category-security"),
        "active-directory" => fl!("ui-tool-cmdlib-category-active-directory"),
        "services-automation" => fl!("ui-tool-cmdlib-category-services-automation"),
        "windows-updates" => fl!("ui-tool-cmdlib-category-windows-updates"),
        "monitoring-logs" => fl!("ui-tool-cmdlib-category-monitoring-logs"),
        "files-storage" => fl!("ui-tool-cmdlib-category-files-storage"),
        "network-dns" => fl!("ui-tool-cmdlib-category-network-dns"),
        "package-management" => fl!("ui-tool-cmdlib-category-package-management"),
        "development" => fl!("ui-tool-cmdlib-category-development"),
        "performance" => fl!("ui-tool-cmdlib-category-performance"),
        "containers-vms" => fl!("ui-tool-cmdlib-category-containers-vms"),
        "database" => fl!("ui-tool-cmdlib-category-database"),
        "windows-debloat" => fl!("ui-tool-cmdlib-category-windows-debloat"),
        other => other.to_owned(),
    }
}

/// Why a draft was not kept, in the window's language.
fn draft_error(problem: DraftError) -> String {
    match problem {
        DraftError::TitleMissing => fl!("ui-tool-cmdlib-error-title-missing"),
        DraftError::TitleTooLong => fl!("ui-tool-cmdlib-error-title-long"),
        DraftError::CategoryMissing => fl!("ui-tool-cmdlib-error-category-missing"),
        DraftError::CategoryTooLong => fl!("ui-tool-cmdlib-error-category-long"),
        DraftError::DescriptionTooLong => fl!("ui-tool-cmdlib-error-description-long"),
        DraftError::PatternMissing => fl!("ui-tool-cmdlib-error-pattern-missing"),
        DraftError::PatternTooLong => fl!(
            "ui-tool-cmdlib-error-pattern-long",
            max = user_commands::MAX_PATTERN_LENGTH
        ),
        DraftError::PatternMultiline => fl!("ui-tool-cmdlib-error-pattern-multiline"),
        DraftError::TooManyParameters => fl!("ui-tool-cmdlib-error-too-many-parameters"),
        DraftError::TooManyTags => fl!("ui-tool-cmdlib-error-too-many-tags"),
        DraftError::Gone => fl!("ui-tool-cmdlib-error-gone"),
    }
}

/// The question of what an export holds, with the three answers and a word about secrets.
fn export_choice_view<'a>(tab: TabId) -> Element<'a, Message> {
    let send = move |message| Message::Tool(tab, ToolMessage::CmdLib(message));
    let choice = |label: String, scope: ExportScope| {
        button(text(label).size(font_size::BODY))
            .padding(super::COPY_PADDING)
            .style(if scope == ExportScope::Mine {
                styles::primary
            } else {
                styles::secondary
            })
            .on_press(send(CommandLibraryMessage::ExportScope(scope)))
    };
    column![
        text(fl!("ui-tool-cmdlib-export-scope-title")).size(font_size::BODY),
        row![
            choice(fl!("ui-tool-cmdlib-export-scope-mine"), ExportScope::Mine),
            choice(
                fl!("ui-tool-cmdlib-export-scope-filtered"),
                ExportScope::CurrentFilter
            ),
            choice(
                fl!("ui-tool-cmdlib-export-scope-all"),
                ExportScope::Everything
            ),
            button(text(fl!("ui-tool-cmdlib-cancel")).size(font_size::BODY))
                .padding(super::COPY_PADDING)
                .style(styles::secondary)
                .on_press(send(CommandLibraryMessage::Export)),
        ]
        .spacing(spacing::SM),
        text(fl!("ui-tool-cmdlib-export-scope-hint"))
            .size(font_size::CAPTION)
            .style(text::secondary),
        text(fl!("ui-tool-cmdlib-export-secrets-note"))
            .size(font_size::CAPTION)
            .style(text::secondary),
    ]
    .spacing(spacing::XS)
    .into()
}

/// What a refused file is told as.
fn transfer_error(error: LibraryError) -> TransferError {
    match error {
        LibraryError::TooLarge => TransferError::TooLarge,
        LibraryError::Io(error) => TransferError::Failed(error.to_string()),
        LibraryError::Format => TransferError::Format,
        LibraryError::TooManyActions => TransferError::TooManyActions,
        LibraryError::TooManyRunbooks => TransferError::TooManyRunbooks,
        LibraryError::Version(version) => TransferError::Version(version),
    }
}

/// Why an imported runbook was left out, in the window's language.
fn refusal_text(why: &library_transfer::RunbookRefusal) -> String {
    super::command_library_runbooks::refusal_text(why)
}

/// Why an imported command was left out, in the window's language.
fn issue_text(issue: &CommandIssue) -> String {
    match issue {
        CommandIssue::MissingTitle => fl!("ui-tool-cmdlib-issue-title-missing"),
        CommandIssue::TitleTooLong => fl!("ui-tool-cmdlib-error-title-long"),
        CommandIssue::MissingCategory => fl!("ui-tool-cmdlib-error-category-missing"),
        CommandIssue::CategoryTooLong => fl!("ui-tool-cmdlib-error-category-long"),
        CommandIssue::DescriptionTooLong => fl!("ui-tool-cmdlib-error-description-long"),
        CommandIssue::TooManyTags => fl!("ui-tool-cmdlib-error-too-many-tags"),
        CommandIssue::NoTemplate => fl!("ui-tool-cmdlib-error-pattern-missing"),
        CommandIssue::PatternTooLong(_) => fl!(
            "ui-tool-cmdlib-error-pattern-long",
            max = user_commands::MAX_PATTERN_LENGTH
        ),
        CommandIssue::PatternMultiline(_) => fl!("ui-tool-cmdlib-error-pattern-multiline"),
        CommandIssue::TooManyParameters(_) => fl!("ui-tool-cmdlib-error-too-many-parameters"),
        CommandIssue::UnnamedParameter(_) => fl!("ui-tool-cmdlib-issue-parameter-unnamed"),
        CommandIssue::DuplicateParameter(_, name) => {
            fl!(
                "ui-tool-cmdlib-issue-parameter-duplicate",
                name = name.as_str()
            )
        }
        CommandIssue::UnknownParameterType(_, kind) => {
            fl!("ui-tool-cmdlib-issue-parameter-type", kind = kind.as_str())
        }
        CommandIssue::ChoiceWithoutValues(_, name) => {
            fl!("ui-tool-cmdlib-issue-choice-empty", name = name.as_str())
        }
        CommandIssue::RefusedDefault(_) => fl!("ui-tool-cmdlib-issue-default-refused"),
        CommandIssue::InCatalogue => fl!("ui-tool-cmdlib-issue-in-catalogue"),
    }
}

/// Why a command of the user cannot be made from the values typed.
pub(super) fn generate_error(error: &GenerateError) -> String {
    match error {
        GenerateError::EmptyPattern => fl!("ui-tool-cmdlib-error-empty-pattern"),
        GenerateError::TooManyParameters => fl!("ui-tool-cmdlib-error-too-many-parameters"),
        GenerateError::Multiline => fl!("ui-tool-cmdlib-error-pattern-multiline"),
        GenerateError::TooLong => fl!(
            "ui-tool-cmdlib-error-command-long",
            max = user_commands::MAX_COMMAND_LENGTH
        ),
        GenerateError::Values(values) => {
            values.iter().map(value_error).collect::<Vec<_>>().join(" ")
        }
    }
}

/// Why a value is refused, naming its parameter.
pub(super) fn value_error(error: &ValueError) -> String {
    match error {
        ValueError::Required(label) => fl!("ui-tool-cmdlib-value-required", label = label.as_str()),
        ValueError::NotOffered(label) => {
            fl!("ui-tool-cmdlib-value-not-offered", label = label.as_str())
        }
        ValueError::NotInteger(label) => {
            fl!("ui-tool-cmdlib-value-not-integer", label = label.as_str())
        }
        ValueError::NotNumber(label) => {
            fl!("ui-tool-cmdlib-value-not-number", label = label.as_str())
        }
        ValueError::Hostname(label) => fl!("ui-tool-cmdlib-value-hostname", label = label.as_str()),
        ValueError::IpAddress(label) => fl!("ui-tool-cmdlib-value-ip", label = label.as_str()),
        ValueError::DriveLetter(label) => fl!("ui-tool-cmdlib-value-drive", label = label.as_str()),
        ValueError::Path(label) => fl!(
            "ui-tool-cmdlib-value-path",
            label = label.as_str(),
            max = user_commands::MAX_PATH_LENGTH
        ),
        ValueError::TooLong(label) => fl!(
            "ui-tool-cmdlib-value-long",
            label = label.as_str(),
            max = user_commands::MAX_PARAMETER_LENGTH
        ),
        ValueError::Dangerous(label) => {
            fl!("ui-tool-cmdlib-value-dangerous", label = label.as_str())
        }
        ValueError::Control(label) => fl!("ui-tool-cmdlib-value-control", label = label.as_str()),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use heimdall_core::tools::runbooks::{RUNBOOKS_FILE, Runbook, RunbookStore, Step};

    use super::super::command_library_runbooks::{RunTarget, StepStatus};
    use super::*;

    /// Three tab identifiers, made by opening three tools in an application.
    fn tab_ids() -> [TabId; 3] {
        use heimdall_app::tools::{ToolContext, ToolId};
        use heimdall_app::{App, AppConfig, Message as AppMessage, ToolsMessage};
        let folder = tempfile::tempdir().expect("folder");
        let mut app = App::new(AppConfig {
            profiles_file: folder.path().join("profiles.toml"),
            known_hosts: folder.path().join("known_hosts"),
            legacy_dir: None,
            agent: heimdall_ssh::AgentSource::Disabled,
            initial_grid: heimdall_term::GridSize { cols: 80, rows: 24 },
            files_start: folder.path().to_owned(),
            system_credentials: heimdall_app::SystemCredentials::memory(),
        });
        for word in ["a", "b", "c"] {
            let _ = app.update(AppMessage::Tools(ToolsMessage::Open {
                tool: ToolId::Hash,
                title: word.to_owned(),
                context: ToolContext::with_argument(word),
            }));
        }
        [app.tabs[0].id, app.tabs[1].id, app.tabs[2].id]
    }

    fn target(tab: TabId, name: &str) -> SendTarget {
        SendTarget {
            tab,
            name: name.to_owned(),
        }
    }

    /// A pane keeping its commands in a folder of its own.
    fn pane(folder: &Path) -> CommandLibraryPane {
        CommandLibraryPane::new(Some(&folder.join("profiles.toml")))
    }

    /// The route tracer of the catalogue, with a value to ask for and a template for each
    /// shell.
    const TRACE: &str = "network-trace-route";

    /// The tracer of the catalogue, its Linux template shown (this host's shell decides
    /// otherwise), and `target` typed.
    fn trace(pane: &mut CommandLibraryPane, target: &str) -> CommandRef {
        pane.update(CommandLibraryMessage::Template(
            TRACE.to_owned(),
            TemplatePlatform::Linux,
        ));
        pane.update(CommandLibraryMessage::Param(
            TRACE.to_owned(),
            "target".to_owned(),
            target.to_owned(),
        ));
        CommandRef::Catalog(TRACE.to_owned())
    }

    /// A command of the catalogue for Windows only, with no value to ask for.
    fn fsmo() -> CommandRef {
        CommandRef::Catalog("ad-check-fsmo-roles".to_owned())
    }

    /// A command of the catalogue for Linux only, marked dangerous, whose values have defaults.
    fn fsck() -> CommandRef {
        CommandRef::Catalog("fsck-check-filesystem".to_owned())
    }

    /// A command of the catalogue for Linux only, not dangerous, whose value has a default.
    fn follow() -> CommandRef {
        CommandRef::Catalog("journalctl-follow".to_owned())
    }

    fn type_into(pane: &mut CommandLibraryPane, field: Field, value: &str) {
        pane.update(CommandLibraryMessage::Field(field, value.to_owned()));
    }

    /// A user command "Ping" made through the editor; its identity.
    fn make_ping(pane: &mut CommandLibraryPane) -> String {
        pane.update(CommandLibraryMessage::New);
        pane.update(CommandLibraryMessage::EditPlatform(UserPlatform::Linux));
        type_into(pane, Field::Title, "Ping");
        type_into(pane, Field::Category, "Network");
        type_into(pane, Field::LinuxPattern, "ping -c 3 {host}");
        pane.update(CommandLibraryMessage::Save);
        assert!(pane.editor.is_none(), "kept");
        pane.library.commands()[0].id.clone()
    }

    #[test]
    fn a_catalogue_command_copies_with_the_values_typed_by_its_own_button() {
        let mut pane = CommandLibraryPane::default();
        let trace = trace(&mut pane, "10.0.0.1");
        let position = command_library::shipped().position(TRACE).expect("shipped");
        assert_eq!(
            pane.update(CommandLibraryMessage::Copy(trace)),
            Outcome::Copy(
                CopySlot::CmdLibCommand(position),
                "traceroute 10.0.0.1".to_owned()
            )
        );
        let position = command_library::shipped()
            .position("ad-check-fsmo-roles")
            .expect("shipped");
        assert_eq!(
            pane.update(CommandLibraryMessage::Copy(fsmo())),
            Outcome::Copy(
                CopySlot::CmdLibCommand(position),
                "netdom query fsmo".to_owned()
            )
        );
    }

    #[test]
    fn a_value_refused_leaves_nothing_to_copy_and_a_missing_one_is_asked_for() {
        let mut pane = CommandLibraryPane::default();
        let trace = trace(&mut pane, "");
        assert_eq!(
            pane.update(CommandLibraryMessage::Copy(trace.clone())),
            Outcome::None,
            "a required value not given"
        );
        trace_value(&mut pane, "a; rm -rf x");
        assert_eq!(
            pane.update(CommandLibraryMessage::Copy(trace)),
            Outcome::None,
            "shell syntax"
        );
    }

    fn trace_value(pane: &mut CommandLibraryPane, target: &str) {
        pane.update(CommandLibraryMessage::Param(
            TRACE.to_owned(),
            "target".to_owned(),
            target.to_owned(),
        ));
    }

    #[test]
    fn the_platform_and_the_search_filter_the_catalogue() {
        let mut pane = CommandLibraryPane::default();
        let all = command_library::shipped().entries().len();
        assert_eq!(pane.shown().len(), all);
        pane.update(CommandLibraryMessage::Platform(Some(UserPlatform::Windows)));
        let windows = pane.shown();
        assert!(windows.len() < all);
        assert!(
            windows
                .iter()
                .all(|entry| entry.command().platform != UserPlatform::Linux)
        );
        pane.update(CommandLibraryMessage::Search(
            "netdom query fsmo".to_owned(),
        ));
        let shown: Vec<&str> = pane
            .shown()
            .iter()
            .map(|entry| entry.command().id.as_str())
            .collect();
        assert!(shown.contains(&"ad-check-fsmo-roles"), "{shown:?}");
        assert!(shown.len() < 5, "{shown:?}");
    }

    #[test]
    fn more_of_the_catalogue_is_shown_page_by_page_and_a_new_filter_starts_over() {
        let mut pane = CommandLibraryPane::default();
        pane.update(CommandLibraryMessage::ShowMore);
        pane.update(CommandLibraryMessage::ShowMore);
        assert_eq!(pane.more_pages, 2);
        pane.update(CommandLibraryMessage::Search("x".to_owned()));
        assert_eq!(pane.more_pages, 0);
        pane.update(CommandLibraryMessage::ShowMore);
        pane.update(CommandLibraryMessage::Platform(None));
        assert_eq!(pane.more_pages, 0);
        pane.update(CommandLibraryMessage::ShowMore);
        pane.update(CommandLibraryMessage::FavoritesOnly);
        assert_eq!(pane.more_pages, 0);
    }

    #[test]
    fn a_send_asks_for_the_command_with_its_values_and_never_without_a_terminal() {
        let mut pane = CommandLibraryPane::default();
        let mtr = trace(&mut pane, "10.0.0.1");
        assert_eq!(
            pane.update(CommandLibraryMessage::Send(mtr.clone())),
            Outcome::None,
            "no terminal: nothing asked"
        );
        let [first, second, _] = tab_ids();
        pane.set_targets(vec![target(first, "A"), target(second, "B")]);
        let Outcome::Send {
            target, command, ..
        } = pane.update(CommandLibraryMessage::Send(mtr.clone()))
        else {
            panic!("asked");
        };
        assert_eq!(
            (target, command.as_str()),
            (first, "traceroute 10.0.0.1"),
            "the first terminal until one is chosen"
        );
        pane.update(CommandLibraryMessage::Target(second));
        assert!(matches!(
            pane.update(CommandLibraryMessage::Send(mtr.clone())),
            Outcome::Send { target, .. } if target == second
        ));
        // The terminal chosen is closed: the first again.
        pane.set_targets(vec![target_of(first)]);
        assert!(matches!(
            pane.update(CommandLibraryMessage::Send(mtr)),
            Outcome::Send { target, .. } if target == first
        ));
    }

    fn target_of(tab: TabId) -> SendTarget {
        target(tab, "A")
    }

    #[test]
    fn a_dangerous_command_is_marked_so_to_the_question() {
        let mut pane = CommandLibraryPane::default();
        pane.set_targets(vec![target(tab_ids()[0], "A")]);
        assert!(matches!(
            pane.update(CommandLibraryMessage::Send(fsck())),
            Outcome::Send {
                dangerous: true,
                ..
            }
        ));
        assert!(matches!(
            pane.update(CommandLibraryMessage::Send(follow())),
            Outcome::Send {
                dangerous: false,
                ..
            }
        ));
    }

    #[test]
    fn the_user_makes_changes_and_deletes_commands_kept_in_their_own_file() {
        let folder = tempfile::tempdir().expect("folder");
        let mut pane = pane(folder.path());
        let id = make_ping(&mut pane);
        assert!(folder.path().join(LIBRARY_FILE).is_file());

        // Another pane over the same folder sees it: kept, apart from the catalog.
        let again = self::pane(folder.path());
        assert_eq!(again.library.commands().len(), 1);
        assert_eq!(
            again.shown().len(),
            command_library::shipped().entries().len()
        );

        pane.update(CommandLibraryMessage::Edit(id.clone()));
        type_into(&mut pane, Field::Title, "Ping three times");
        pane.update(CommandLibraryMessage::Save);
        assert_eq!(pane.library.commands()[0].title, "Ping three times");
        assert_eq!(
            self::pane(folder.path()).library.commands()[0].title,
            "Ping three times"
        );

        pane.update(CommandLibraryMessage::AskDelete(id.clone()));
        assert_eq!(pane.library.commands().len(), 1, "asked first");
        pane.update(CommandLibraryMessage::CancelDelete);
        assert_eq!(pane.library.commands().len(), 1, "kept");
        pane.update(CommandLibraryMessage::AskDelete(id.clone()));
        pane.update(CommandLibraryMessage::ConfirmDelete(id));
        assert!(pane.library.commands().is_empty());
        assert!(self::pane(folder.path()).library.commands().is_empty());
    }

    #[test]
    fn a_draft_not_kept_stays_in_the_editor_with_its_reason() {
        let folder = tempfile::tempdir().expect("folder");
        let mut pane = pane(folder.path());
        pane.update(CommandLibraryMessage::New);
        pane.update(CommandLibraryMessage::Save);
        assert_eq!(
            pane.editor.as_ref().and_then(|editor| editor.problem),
            Some(DraftError::TitleMissing)
        );
        pane.update(CommandLibraryMessage::CancelEdit);
        assert!(pane.editor.is_none());
        assert!(pane.library.commands().is_empty());
    }

    /// A command captured in a terminal of the given platform.
    fn captured(text: &str, platform: TemplatePlatform) -> CapturedCommand {
        CapturedCommand {
            text: text.to_owned(),
            platform,
            tag: Some("ssh".to_owned()),
        }
    }

    #[test]
    fn a_captured_linux_command_opens_the_editor_on_a_new_action_with_the_title_left_to_the_user() {
        let folder = tempfile::tempdir().expect("folder");
        let mut pane = pane(folder.path());
        pane.update(CommandLibraryMessage::Capture(captured(
            "df -h && free -m",
            TemplatePlatform::Linux,
        )));
        let draft = &pane.editor.as_ref().expect("the editor is open").draft;
        assert_eq!(draft.id, None, "a new action, never an edit");
        assert_eq!(draft.linux_pattern, "df -h && free -m");
        assert_eq!(draft.windows_pattern, "", "one shell only");
        assert_eq!(draft.platform, UserPlatform::Linux);
        assert_eq!(draft.level, Criticality::Run);
        assert_eq!(draft.tags, "ssh");
        assert_eq!(draft.title, "", "the user names it");
        assert!(
            pane.library.commands().is_empty(),
            "nothing is kept before the user saves"
        );
    }

    #[test]
    fn a_captured_windows_command_goes_to_the_powershell_pattern() {
        let folder = tempfile::tempdir().expect("folder");
        let mut pane = pane(folder.path());
        pane.update(CommandLibraryMessage::Capture(captured(
            "cd C:\\logs ; Get-ChildItem",
            TemplatePlatform::Windows,
        )));
        let draft = &pane.editor.as_ref().expect("the editor is open").draft;
        assert_eq!(draft.windows_pattern, "cd C:\\logs ; Get-ChildItem");
        assert_eq!(draft.linux_pattern, "");
        assert_eq!(draft.platform, UserPlatform::Windows);
    }

    #[test]
    fn a_captured_command_is_saved_through_the_checks_of_the_editor_as_the_users_own() {
        let folder = tempfile::tempdir().expect("folder");
        let mut pane = pane(folder.path());
        pane.update(CommandLibraryMessage::Capture(captured(
            "uptime",
            TemplatePlatform::Linux,
        )));
        // No title, no category: refused with the usual reason, the draft kept.
        pane.update(CommandLibraryMessage::Save);
        assert_eq!(
            pane.editor.as_ref().and_then(|editor| editor.problem),
            Some(DraftError::TitleMissing)
        );
        type_into(&mut pane, Field::Title, "Load");
        type_into(&mut pane, Field::Category, "Network");
        pane.update(CommandLibraryMessage::Save);
        assert!(pane.editor.is_none(), "kept");
        let kept = pane.library.commands();
        assert_eq!(kept.len(), 1, "a command of the user, in their file");
        assert_eq!(kept[0].tags, ["ssh"]);
        assert_eq!(kept[0].level, Criticality::Run);

        // The next capture starts in the category last created.
        pane.update(CommandLibraryMessage::Capture(captured(
            "df -h",
            TemplatePlatform::Linux,
        )));
        assert_eq!(
            pane.editor
                .as_ref()
                .map(|editor| editor.draft.category.as_str()),
            Some("Network")
        );
    }

    #[test]
    fn a_capture_never_replaces_an_editor_with_changes_and_says_so() {
        let folder = tempfile::tempdir().expect("folder");
        let mut pane = pane(folder.path());
        pane.update(CommandLibraryMessage::New);
        type_into(&mut pane, Field::Title, "Mine");
        pane.update(CommandLibraryMessage::Capture(captured(
            "uptime",
            TemplatePlatform::Linux,
        )));
        let editor = pane.editor.as_ref().expect("still open");
        assert_eq!(editor.draft.title, "Mine", "nothing lost");
        assert_eq!(editor.draft.linux_pattern, "");
        assert_eq!(pane.notice, Some(Notice::CaptureBlocked));

        // An editor nothing was typed in gives way.
        pane.update(CommandLibraryMessage::CancelEdit);
        pane.update(CommandLibraryMessage::New);
        pane.update(CommandLibraryMessage::Capture(captured(
            "uptime",
            TemplatePlatform::Linux,
        )));
        assert_eq!(
            pane.editor
                .as_ref()
                .map(|editor| editor.draft.linux_pattern.as_str()),
            Some("uptime")
        );
        assert_eq!(pane.notice, None);
    }

    #[test]
    fn a_captured_command_holding_a_line_break_is_refused_at_save() {
        let folder = tempfile::tempdir().expect("folder");
        let mut pane = pane(folder.path());
        pane.update(CommandLibraryMessage::Capture(captured(
            "ls\nrm -rf x",
            TemplatePlatform::Linux,
        )));
        type_into(&mut pane, Field::Title, "Bad");
        type_into(&mut pane, Field::Category, "Network");
        pane.update(CommandLibraryMessage::Save);
        assert_eq!(
            pane.editor.as_ref().and_then(|editor| editor.problem),
            Some(DraftError::PatternMultiline)
        );
        assert!(pane.library.commands().is_empty());
    }

    #[test]
    fn a_capture_with_no_file_to_keep_it_in_opens_nothing() {
        let mut pane = CommandLibraryPane::default();
        pane.update(CommandLibraryMessage::Capture(captured(
            "uptime",
            TemplatePlatform::Linux,
        )));
        assert!(pane.editor.is_none());
    }

    #[test]
    fn a_user_command_is_copied_and_sent_with_its_values_shaped_for_the_shell() {
        let folder = tempfile::tempdir().expect("folder");
        let mut pane = pane(folder.path());
        let id = make_ping(&mut pane);
        let ping = CommandRef::User(id.clone());
        pane.update(CommandLibraryMessage::UserParam(
            id.clone(),
            "host".to_owned(),
            "web.lab".to_owned(),
        ));
        assert_eq!(
            pane.update(CommandLibraryMessage::Copy(ping.clone())),
            Outcome::Copy(
                CopySlot::CmdLibUserCommand(0),
                "ping -c 3 web.lab".to_owned()
            )
        );
        let terminal = tab_ids()[0];
        pane.set_targets(vec![target(terminal, "A")]);
        assert_eq!(
            pane.update(CommandLibraryMessage::Send(ping.clone())),
            Outcome::Send {
                target: terminal,
                command: "ping -c 3 web.lab".to_owned(),
                dangerous: false,
                typed_as_is: false,
            }
        );
        // A value that would run more is refused: nothing to copy, nothing to send.
        pane.update(CommandLibraryMessage::UserParam(
            id,
            "host".to_owned(),
            "web.lab\nrm -rf x".to_owned(),
        ));
        assert_eq!(
            pane.update(CommandLibraryMessage::Copy(ping.clone())),
            Outcome::None
        );
        assert_eq!(
            pane.update(CommandLibraryMessage::Send(ping)),
            Outcome::None
        );
    }

    #[test]
    fn a_file_that_cannot_be_read_is_left_alone_and_nothing_is_changed() {
        let folder = tempfile::tempdir().expect("folder");
        let path = folder.path().join(LIBRARY_FILE);
        std::fs::write(&path, "not json").expect("written");
        let mut pane = pane(folder.path());
        assert_eq!(pane.failure, Some(Failure::Unreadable));
        pane.update(CommandLibraryMessage::New);
        assert!(pane.editor.is_none(), "no editor over a file not read");
        assert_eq!(std::fs::read_to_string(&path).expect("read"), "not json");
    }

    fn receipt(tab: TabId, name: &str, command: &str) -> SentCommand {
        SentCommand {
            tab,
            target: name.to_owned(),
            command: command.to_owned(),
            delivery: Delivery::Run,
        }
    }

    #[test]
    fn a_file_of_commands_is_imported_and_said_what_it_did() {
        let source = tempfile::tempdir().expect("folder");
        let mut origin = pane(source.path());
        make_ping(&mut origin);
        assert_eq!(origin.update(CommandLibraryMessage::Export), Outcome::None);
        assert!(origin.export_choice, "the question is asked first");
        let Outcome::Export(exported) =
            origin.update(CommandLibraryMessage::ExportScope(ExportScope::Mine))
        else {
            panic!("export asked");
        };
        assert!(!origin.export_choice, "answered");
        assert!(exported.contains("schemaVersion"), "the C# envelope");

        let folder = tempfile::tempdir().expect("folder");
        let mut pane = pane(folder.path());
        assert_eq!(pane.update(CommandLibraryMessage::Import), Outcome::Import);
        pane.update(CommandLibraryMessage::Imported(Some(Ok(exported.clone()))));
        assert_eq!(pane.library.commands().len(), 1);
        assert!(matches!(
            &pane.notice,
            Some(Notice::Imported(report, None))
                if (report.imported, report.updated, report.skipped) == (1, 0, 0)
        ));
        let again = self::pane(folder.path());
        assert_eq!(again.library.commands().len(), 1, "kept in the file");

        // The same file again replaces the command, as the C# import.
        pane.update(CommandLibraryMessage::Imported(Some(Ok(exported))));
        assert_eq!(pane.library.commands().len(), 1);
        assert!(matches!(
            pane.notice,
            Some(Notice::Imported(ImportReport { updated: 1, .. }, None))
        ));

        // Something that is not an export changes nothing and is said.
        pane.update(CommandLibraryMessage::Imported(Some(Ok("nope".to_owned()))));
        assert_eq!(
            pane.notice,
            Some(Notice::TransferFailed(TransferError::Format))
        );
        assert_eq!(pane.library.commands().len(), 1);
        pane.update(CommandLibraryMessage::Imported(Some(Err(
            TransferError::TooLarge,
        ))));
        assert_eq!(
            pane.notice,
            Some(Notice::TransferFailed(TransferError::TooLarge))
        );
    }

    #[test]
    fn nothing_is_imported_without_a_place_to_keep_it_and_nothing_exported_when_empty() {
        let mut pane = CommandLibraryPane::default();
        assert_eq!(pane.update(CommandLibraryMessage::Import), Outcome::None);
        assert_eq!(pane.update(CommandLibraryMessage::Export), Outcome::None);
        let folder = tempfile::tempdir().expect("folder");
        let mut kept = self::pane(folder.path());
        assert_eq!(kept.update(CommandLibraryMessage::Export), Outcome::None);
        assert_eq!(
            kept.update(CommandLibraryMessage::ExportScope(ExportScope::Mine)),
            Outcome::None
        );
        assert_eq!(
            kept.notice,
            Some(Notice::TransferFailed(TransferError::NothingToExport))
        );
    }

    /// The `totalActions` and the runbooks of an export.
    fn exported_counts(text: &str) -> (usize, usize) {
        let total = text
            .split("\"totalActions\":")
            .nth(1)
            .map(|rest| {
                rest.trim_start()
                    .chars()
                    .take_while(char::is_ascii_digit)
                    .collect::<String>()
            })
            .and_then(|count| count.parse().ok())
            .expect("a count");
        (total, text.matches("\"executionMode\"").count())
    }

    #[test]
    fn export_asks_what_to_include_and_a_file_with_runbooks_round_trips() {
        let source = tempfile::tempdir().expect("folder");
        let mut origin = run_pane(source.path(), vec![three_step_runbook()]);
        origin.update(CommandLibraryMessage::Imported(Some(Ok(
            TWO_ACTIONS.to_owned()
        ))));
        let export = |pane: &mut CommandLibraryPane, scope| {
            pane.update(CommandLibraryMessage::Export);
            match pane.update(CommandLibraryMessage::ExportScope(scope)) {
                Outcome::Export(text) => text,
                other => panic!("an export, not {other:?}"),
            }
        };

        // My actions: what the user made or imported, and the runbooks.
        let mine = export(&mut origin, ExportScope::Mine);
        assert_eq!(exported_counts(&mine), (2, 1));
        // The current filter: the actions listed, no runbook.
        origin.update(CommandLibraryMessage::Search("Calm".to_owned()));
        let filtered = export(&mut origin, ExportScope::CurrentFilter);
        // The list shows the catalogue commands the search keeps as well as the user one.
        assert_eq!(
            exported_counts(&filtered),
            (origin.shown().len() + origin.shown_user().len(), 0)
        );
        assert_eq!(origin.shown_user().len(), 1);
        origin.update(CommandLibraryMessage::Search(String::new()));
        // Everything: the shipped catalogue too.
        let all = export(&mut origin, ExportScope::Everything);
        let (count, runbooks) = exported_counts(&all);
        assert_eq!(count, command_library::shipped().entries().len() + 2);
        assert_eq!(runbooks, 1);

        // Into another installation: the actions and the runbook come, and the summary says so.
        let folder = tempfile::tempdir().expect("folder");
        let mut pane = pane(folder.path());
        pane.update(CommandLibraryMessage::Imported(Some(Ok(mine.clone()))));
        assert_eq!(pane.library.commands().len(), 2);
        assert_eq!(pane.runbooks.exportable(&pane.library).len(), 1);
        assert!(matches!(
            &pane.notice,
            Some(Notice::Imported(actions, Some(runbooks)))
                if actions.imported == 2 && runbooks.imported == 1 && runbooks.skipped == 0
        ));
        assert_eq!(
            self::pane(folder.path())
                .runbooks
                .exportable(&pane.library)
                .len(),
            1,
            "kept in the file"
        );
        // The same file again replaces the runbook in place instead of copying it.
        pane.update(CommandLibraryMessage::Imported(Some(Ok(mine))));
        assert_eq!(pane.runbooks.exportable(&pane.library).len(), 1);
        assert!(matches!(
            &pane.notice,
            Some(Notice::Imported(_, Some(runbooks))) if runbooks.updated == 1
        ));
        // An export of everything does not copy the catalogue back into the user commands.
        pane.update(CommandLibraryMessage::Imported(Some(Ok(all))));
        assert_eq!(pane.library.commands().len(), 2);
        assert!(matches!(
            &pane.notice,
            Some(Notice::Imported(actions, _))
                if actions.skipped == command_library::shipped().entries().len()
                    && actions.first_skipped.as_ref().is_some_and(
                        |(_, why)| *why == CommandIssue::InCatalogue
                    )
        ));
    }

    #[test]
    fn a_hostile_file_is_refused_or_trimmed_and_nothing_is_run() {
        let folder = tempfile::tempdir().expect("folder");
        let mut pane = pane(folder.path());
        for (file, expected) in [
            (
                r#"{"schemaVersion":"2.0","actions":[]}"#.to_owned(),
                TransferError::Version("2.0".to_owned()),
            ),
            (
                format!(
                    "{{\"runbooks\":[{}]}}",
                    vec!["{}"; heimdall_core::tools::runbooks::MAX_RUNBOOKS + 1].join(",")
                ),
                TransferError::TooManyRunbooks,
            ),
            (r#"{"runbooks":"x"}"#.to_owned(), TransferError::Format),
        ] {
            pane.update(CommandLibraryMessage::Imported(Some(Ok(file))));
            assert_eq!(pane.notice, Some(Notice::TransferFailed(expected)));
        }
        assert!(pane.library.commands().is_empty());

        // One good runbook, one with a control character, one with a refused value.
        let file = r#"{"actions":[{"id":"calm","title":"Calm","category":"mine","platform":1,
            "linuxCommandTemplate":{"platform":1,"commandPattern":"echo {w}",
            "parameters":[{"name":"w","label":"W","type":"string"}]}}],
          "runbooks":[
            {"name":"Good","steps":[{"command":"uptime","platform":"Both"}]},
            {"name":"Bell\u0007","steps":[{"command":"uptime"}]},
            {"name":"Injected","steps":[{"actionId":"calm","parameters":{"w":"a; rm -rf /"}}]}]}"#;
        pane.update(CommandLibraryMessage::Imported(Some(Ok(file.to_owned()))));
        let Some(Notice::Imported(_, Some(runbooks))) = &pane.notice else {
            panic!("said: {:?}", pane.notice);
        };
        assert_eq!((runbooks.imported, runbooks.skipped), (1, 2));
        assert!(
            matches!(&runbooks.first_skipped, Some((name, _)) if name == "Bell"),
            "the first left out is named, without its control character"
        );
        let kept = pane.runbooks.exportable(&pane.library);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].name, "Good");
        // Both reads as Linux.
        assert_eq!(
            kept[0].steps,
            [Step::literal("uptime", TemplatePlatform::Linux)]
        );
    }

    #[test]
    fn a_runbook_save_that_fails_takes_the_imported_actions_back() {
        let folder = tempfile::tempdir().expect("folder");
        let mut pane = pane(folder.path());
        // The runbooks file becomes unreadable after the pane opened: the save fails late.
        std::fs::create_dir(folder.path().join(RUNBOOKS_FILE)).expect("folder");
        let file = r#"{"actions":[{"id":"calm","title":"Calm","category":"mine","platform":1,
            "linuxCommandTemplate":{"platform":1,"commandPattern":"echo calm","parameters":[]}}],
            "runbooks":[{"name":"R","steps":[{"command":"id"}]}]}"#;
        pane.update(CommandLibraryMessage::Imported(Some(Ok(file.to_owned()))));
        assert_eq!(
            pane.notice,
            Some(Notice::TransferFailed(TransferError::RunbooksUnsaved))
        );
        assert!(pane.library.commands().is_empty(), "nothing half imported");
        assert!(
            self::pane(folder.path()).library.commands().is_empty(),
            "nor in the file"
        );
    }

    #[test]
    fn a_file_with_runbooks_is_refused_whole_when_the_runbooks_file_cannot_be_used() {
        let folder = tempfile::tempdir().expect("folder");
        std::fs::write(folder.path().join(RUNBOOKS_FILE), "not json").expect("written");
        let mut pane = pane(folder.path());
        let file = r#"{"actions":[{"id":"calm","title":"Calm","category":"mine","platform":1,
            "linuxCommandTemplate":{"platform":1,"commandPattern":"echo calm","parameters":[]}}],
            "runbooks":[{"name":"R","steps":[{"command":"id"}]}]}"#;
        pane.update(CommandLibraryMessage::Imported(Some(Ok(file.to_owned()))));
        assert_eq!(
            pane.notice,
            Some(Notice::TransferFailed(TransferError::RunbooksUnsaved))
        );
        assert!(pane.library.commands().is_empty(), "nothing half imported");
    }

    #[test]
    fn a_favorite_is_kept_filters_the_list_and_goes_with_its_command() {
        let folder = tempfile::tempdir().expect("folder");
        let mut pane = pane(folder.path());
        let id = make_ping(&mut pane);
        let mtr = CommandRef::Catalog(TRACE.to_owned());
        let ping = CommandRef::User(id.clone());
        pane.update(CommandLibraryMessage::ToggleFavorite(mtr.clone()));
        assert!(pane.is_favorite(&mtr));
        assert!(!pane.is_favorite(&ping));
        assert!(
            folder.path().join(ACTIVITY_FILE).is_file(),
            "kept beside the commands"
        );
        assert!(
            self::pane(folder.path()).is_favorite(&mtr),
            "after a restart"
        );

        pane.update(CommandLibraryMessage::FavoritesOnly);
        let shown: Vec<&str> = pane
            .shown()
            .iter()
            .map(|each| each.command().id.as_str())
            .collect();
        assert_eq!(shown, [TRACE]);
        assert!(pane.shown_user().is_empty());
        pane.update(CommandLibraryMessage::ToggleFavorite(ping.clone()));
        assert_eq!(pane.shown_user().len(), 1);

        // Deleting the command lets go of its star.
        pane.update(CommandLibraryMessage::AskDelete(id.clone()));
        pane.update(CommandLibraryMessage::ConfirmDelete(id));
        assert!(
            pane.activity
                .favorites()
                .iter()
                .all(|each| each.starts_with("catalog:"))
        );

        pane.update(CommandLibraryMessage::ToggleFavorite(mtr.clone()));
        assert!(!pane.is_favorite(&mtr), "toggled off");
    }

    #[test]
    fn a_duplicate_opens_a_copy_that_is_saved_as_a_command_of_its_own() {
        let folder = tempfile::tempdir().expect("folder");
        let mut pane = pane(folder.path());
        let id = make_ping(&mut pane);
        pane.update(CommandLibraryMessage::Duplicate(CommandRef::User(
            id.clone(),
        )));
        let editor = pane.editor.as_ref().expect("editor open");
        assert_eq!(editor.draft.id, None, "a new command");
        assert!(editor.draft.title.contains("Ping"));
        assert_eq!(editor.draft.linux_pattern, "ping -c 3 {host}");
        pane.update(CommandLibraryMessage::Save);
        assert_eq!(pane.library.commands().len(), 2);
        assert_ne!(pane.library.commands()[1].id, id);

        // A snippet of the catalog is copied too: its pattern is kept for its platform.
        pane.update(CommandLibraryMessage::Duplicate(fsmo()));
        let draft = &pane.editor.as_ref().expect("editor open").draft;
        assert_eq!(draft.platform, UserPlatform::Windows);
        assert_eq!(draft.windows_pattern, "netdom query fsmo");
        assert!(draft.linux_pattern.is_empty());
        assert_eq!(draft.level, Criticality::Info);
        // The editor is open: a second duplicate changes nothing.
        pane.update(CommandLibraryMessage::Duplicate(follow()));
        assert_eq!(
            pane.editor.as_ref().expect("still open").draft.platform,
            UserPlatform::Windows
        );
    }

    #[test]
    fn ctrl_d_duplicates_the_command_last_used_and_nothing_before() {
        let folder = tempfile::tempdir().expect("folder");
        let mut pane = pane(folder.path());
        pane.update(CommandLibraryMessage::DuplicateSelected);
        assert!(pane.editor.is_none(), "nothing used yet");
        pane.update(CommandLibraryMessage::Copy(follow()));
        pane.update(CommandLibraryMessage::DuplicateSelected);
        let editor = pane.editor.as_ref().expect("editor open");
        assert_eq!(editor.draft.linux_pattern, "journalctl -f -n {lines}");
    }

    #[test]
    fn a_command_sent_is_recorded_as_its_pattern_with_no_value_and_not_before_it_is_written() {
        let folder = tempfile::tempdir().expect("folder");
        let mut pane = pane(folder.path());
        let [first, ..] = tab_ids();
        pane.set_targets(vec![target(first, "web-1")]);
        let mtr = trace(&mut pane, "secret-host.internal");
        let Outcome::Send { command, .. } = pane.update(CommandLibraryMessage::Send(mtr)) else {
            panic!("asked");
        };
        assert!(command.contains("secret-host.internal"));
        assert!(pane.activity.history().is_empty(), "asked, not sent");

        // A receipt for another command, or another terminal, is not this one's.
        pane.record_sent(&[receipt(first, "web-1", "uptime")]);
        assert!(pane.activity.history().is_empty());

        pane.record_sent(&[receipt(first, "web-1", &command)]);
        let [entry] = pane.activity.history() else {
            panic!("one entry");
        };
        assert_eq!(entry.target, "web-1");
        assert_eq!(entry.pattern, "traceroute {target}");
        assert_eq!(entry.parameters, ["target"]);
        assert!(entry.at > 0);
        let kept = std::fs::read_to_string(folder.path().join(ACTIVITY_FILE)).expect("read");
        assert!(
            !kept.contains("secret-host"),
            "what was typed is never kept"
        );
        assert_eq!(
            self::pane(folder.path()).activity.history().len(),
            1,
            "after a restart"
        );
    }

    #[test]
    fn a_replay_asks_to_send_and_never_sends_and_a_gone_command_is_said() {
        let folder = tempfile::tempdir().expect("folder");
        let mut pane = pane(folder.path());
        let id = make_ping(&mut pane);
        let [first, ..] = tab_ids();
        pane.set_targets(vec![target(first, "web-1")]);
        pane.update(CommandLibraryMessage::UserParam(
            id.clone(),
            "host".to_owned(),
            "web.lab".to_owned(),
        ));
        let ping = CommandRef::User(id.clone());
        let Outcome::Send { command, .. } = pane.update(CommandLibraryMessage::Send(ping)) else {
            panic!("asked");
        };
        pane.record_sent(&[receipt(first, "web-1", &command)]);
        pane.update(CommandLibraryMessage::ToggleHistory);
        assert!(pane.history_open);

        // Replayed: only the question is asked, with the command as written now.
        let Outcome::Send {
            target, command, ..
        } = pane.update(CommandLibraryMessage::Replay(0))
        else {
            panic!("a question");
        };
        assert_eq!((target, command.as_str()), (first, "ping -c 3 web.lab"));
        assert!(!pane.history_open, "the history closes on a replay");

        // Its copy is the pattern.
        assert_eq!(
            pane.update(CommandLibraryMessage::CopyHistory(0)),
            Outcome::Copy(CopySlot::CmdLibHistory(0), "ping -c 3 {host}".to_owned())
        );

        // The command is deleted: said, nothing asked.
        pane.update(CommandLibraryMessage::AskDelete(id.clone()));
        pane.update(CommandLibraryMessage::ConfirmDelete(id));
        assert_eq!(pane.update(CommandLibraryMessage::Replay(0)), Outcome::None);
        assert_eq!(pane.notice, Some(Notice::ReplayGone));
        // An entry that is not there asks nothing.
        assert_eq!(pane.update(CommandLibraryMessage::Replay(9)), Outcome::None);
    }

    #[test]
    fn a_replay_without_a_terminal_is_said_and_asks_nothing() {
        let mut pane = CommandLibraryPane::default();
        pane.activity.record(HistoryEntry {
            command: command_activity::catalog_key(TRACE),
            title: "Trace the network route".to_owned(),
            pattern: "traceroute {target}".to_owned(),
            parameters: vec!["target".to_owned()],
            target: "web-1".to_owned(),
            host: String::new(),
            mode: HistoryMode::Sent,
            at: now_unix(),
        });
        assert_eq!(pane.update(CommandLibraryMessage::Replay(0)), Outcome::None);
        assert_eq!(pane.notice, Some(Notice::ReplayBlocked));
    }

    #[test]
    fn the_history_is_cleared_once_asked_and_the_favorites_stay() {
        let folder = tempfile::tempdir().expect("folder");
        let mut pane = pane(folder.path());
        let mtr = trace(&mut pane, "10.0.0.1");
        pane.update(CommandLibraryMessage::ToggleFavorite(mtr.clone()));
        let [first, ..] = tab_ids();
        pane.set_targets(vec![target(first, "web-1")]);
        let Outcome::Send { command, .. } = pane.update(CommandLibraryMessage::Send(mtr.clone()))
        else {
            panic!("asked");
        };
        pane.record_sent(&[receipt(first, "web-1", &command)]);
        assert_eq!(pane.activity.history().len(), 1);
        pane.update(CommandLibraryMessage::AskClearHistory);
        pane.update(CommandLibraryMessage::CancelClearHistory);
        assert_eq!(pane.activity.history().len(), 1, "kept");
        pane.update(CommandLibraryMessage::AskClearHistory);
        pane.update(CommandLibraryMessage::ConfirmClearHistory);
        assert!(pane.activity.history().is_empty());
        let again = self::pane(folder.path());
        assert!(again.activity.history().is_empty());
        assert!(again.is_favorite(&mtr));
    }

    #[test]
    fn a_broadcast_asks_once_for_every_terminal_ticked_and_none_otherwise() {
        let mut pane = CommandLibraryPane::default();
        let [first, second, third] = tab_ids();
        pane.set_targets(vec![
            target(first, "A"),
            target(second, "B"),
            target(third, "C"),
        ]);
        let flush = fsmo();
        assert_eq!(
            pane.update(CommandLibraryMessage::Broadcast(flush.clone())),
            Outcome::None,
            "no terminal ticked"
        );
        pane.update(CommandLibraryMessage::BroadcastTarget(first));
        pane.update(CommandLibraryMessage::BroadcastTarget(third));
        let Outcome::Broadcast {
            targets,
            command,
            dangerous,
        } = pane.update(CommandLibraryMessage::Broadcast(flush.clone()))
        else {
            panic!("a question");
        };
        assert_eq!(targets, [first, third], "in the order of the strip");
        assert_eq!(command, "netdom query fsmo");
        assert!(!dangerous);
        pane.update(CommandLibraryMessage::BroadcastTarget(third));
        assert_eq!(pane.broadcast, [first], "unticked");
        pane.update(CommandLibraryMessage::BroadcastAll);
        assert_eq!(pane.broadcast.len(), 3);
        pane.update(CommandLibraryMessage::BroadcastNone);
        assert!(pane.broadcast.is_empty());
        // A dangerous command is marked so for the one question.
        pane.update(CommandLibraryMessage::BroadcastAll);
        assert!(matches!(
            pane.update(CommandLibraryMessage::Broadcast(fsck())),
            Outcome::Broadcast {
                dangerous: true,
                ..
            }
        ));
    }

    #[test]
    fn a_partial_broadcast_is_said_and_keeps_only_the_terminals_not_reached_ticked() {
        let mut pane = CommandLibraryPane::default();
        let [first, second, third] = tab_ids();
        pane.set_targets(vec![
            target(first, "A"),
            target(second, "B"),
            target(third, "C"),
        ]);
        pane.update(CommandLibraryMessage::BroadcastAll);
        let flush = fsmo();
        pane.update(CommandLibraryMessage::Broadcast(flush));
        // The application wrote into two of the three: the third closed meanwhile.
        pane.record_sent(&[
            receipt(first, "A", "netdom query fsmo"),
            receipt(second, "B", "netdom query fsmo"),
        ]);
        assert_eq!(
            pane.notice,
            Some(Notice::Broadcast {
                delivered: 2,
                total: 3,
                unreached: vec!["C".to_owned()]
            })
        );
        assert_eq!(pane.broadcast, [third], "only what did not receive");
        let [entry] = pane.activity.history() else {
            panic!("one entry for one command");
        };
        assert_eq!(entry.target, "A, B");
    }

    #[test]
    fn a_target_closed_leaves_the_ticked_terminals_it_was() {
        let mut pane = CommandLibraryPane::default();
        let [first, second, _] = tab_ids();
        pane.set_targets(vec![target(first, "A"), target(second, "B")]);
        pane.update(CommandLibraryMessage::BroadcastTarget(first));
        pane.update(CommandLibraryMessage::BroadcastTarget(second));
        pane.set_targets(vec![target(second, "B")]);
        let Outcome::Broadcast { targets, .. } =
            pane.update(CommandLibraryMessage::Broadcast(fsmo()))
        else {
            panic!("a question");
        };
        assert_eq!(targets, [second], "never a terminal that is gone");
        // A terminal that is not offered cannot be ticked.
        let third = tab_ids()[2];
        pane.update(CommandLibraryMessage::BroadcastNone);
        pane.update(CommandLibraryMessage::BroadcastTarget(third));
        assert!(pane.broadcast.is_empty());
    }

    #[test]
    fn the_favorites_and_the_history_of_the_first_ten_commands_follow_them_to_the_catalogue() {
        let folder = tempfile::tempdir().expect("folder");
        let path = folder.path().join(ACTIVITY_FILE);
        let mut old = CommandActivity::default();
        old.toggle_favorite(&command_activity::catalog_key("mtr-probe"));
        old.toggle_favorite(&command_activity::catalog_key("linux-kill-port"));
        old.record(HistoryEntry {
            command: command_activity::catalog_key("mtr-probe"),
            title: "Network path quality".to_owned(),
            pattern: "mtr -rw -c 50 {host}".to_owned(),
            parameters: vec!["host".to_owned()],
            target: "web-1".to_owned(),
            host: String::new(),
            mode: HistoryMode::Sent,
            at: now_unix(),
        });
        old.save(&path).expect("written");

        let mut pane = pane(folder.path());
        let trace = CommandRef::Catalog(TRACE.to_owned());
        assert!(pane.is_favorite(&trace), "starred under its new name");
        assert_eq!(
            pane.command_of_key(&pane.activity.history()[0].command),
            Some(trace),
            "the history finds it"
        );
        // Written back under the new names, so the next start has nothing to rename.
        let kept = CommandActivity::load(&path).expect("read");
        assert!(kept.is_favorite(&command_activity::catalog_key(TRACE)));
        assert!(!kept.is_favorite(&command_activity::catalog_key("mtr-probe")));
        // The command with no successor is kept in the file and finds no command.
        assert!(kept.is_favorite(&command_activity::catalog_key("linux-kill-port")));
        pane.update(CommandLibraryMessage::Replay(0));
        assert_ne!(pane.notice, Some(Notice::ReplayGone));
    }

    #[test]
    fn a_file_imported_that_breaks_a_rule_is_left_out_and_counted() {
        let folder = tempfile::tempdir().expect("folder");
        let mut pane = pane(folder.path());
        let file = r#"{
            "schemaVersion": "1.0",
            "actions": [
                {"id": "good", "title": "Good", "category": "mine", "platform": 1,
                 "linuxCommandTemplate": {"platform": 1, "commandPattern": "uptime", "parameters": []}},
                {"id": "odd", "title": "Odd", "category": "mine", "platform": 1,
                 "linuxCommandTemplate": {"platform": 1, "commandPattern": "run {x}",
                  "parameters": [{"name": "x", "label": "X", "type": "magic"}]}},
                {"id": "lines", "title": "Lines", "category": "mine", "platform": 1,
                 "linuxCommandTemplate": {"platform": 1, "commandPattern": "id\nwhoami", "parameters": []}}
            ]
        }"#;
        pane.update(CommandLibraryMessage::Imported(Some(Ok(file.to_owned()))));
        let Some(Notice::Imported(report, _)) = &pane.notice else {
            panic!("said");
        };
        assert_eq!((report.imported, report.updated, report.skipped), (1, 0, 2));
        assert!(
            matches!(&report.first_skipped, Some((title, _)) if title == "Odd"),
            "the first left out is named"
        );
        assert_eq!(pane.library.commands().len(), 1);
        assert_eq!(pane.library.commands()[0].title, "Good");
    }

    #[test]
    fn a_catalogue_command_with_a_line_break_is_not_copied_sent_or_awaited() {
        let mut pane = CommandLibraryPane::default();
        let [first, second, _] = tab_ids();
        pane.set_targets(vec![target(first, "A"), target(second, "B")]);
        pane.update(CommandLibraryMessage::BroadcastAll);
        let memory = CommandRef::Catalog("perf-memory-analysis".to_owned());
        for platform in [TemplatePlatform::Windows, TemplatePlatform::Linux] {
            pane.update(CommandLibraryMessage::Template(
                "perf-memory-analysis".to_owned(),
                platform,
            ));
            assert_eq!(
                pane.update(CommandLibraryMessage::Copy(memory.clone())),
                Outcome::None
            );
            assert_eq!(
                pane.update(CommandLibraryMessage::Send(memory.clone())),
                Outcome::None
            );
            assert_eq!(
                pane.update(CommandLibraryMessage::Broadcast(memory.clone())),
                Outcome::None
            );
        }
        assert!(pane.awaited.is_empty(), "no receipt for a refused command");
    }

    #[test]
    fn an_imported_destructive_command_asks_for_the_danger_confirmation_at_any_level() {
        let folder = tempfile::tempdir().expect("folder");
        let mut pane = pane(folder.path());
        let file = r#"{"actions": [
            {"id": "wipe", "title": "Wipe", "category": "mine", "platform": 1, "level": 0,
             "linuxCommandTemplate": {"platform": 1, "commandPattern": "rm -rf ~", "parameters": []}}
        ]}"#;
        pane.update(CommandLibraryMessage::Imported(Some(Ok(file.to_owned()))));
        let id = pane.library.commands()[0].id.clone();
        assert_eq!(pane.library.commands()[0].level, Criticality::Dangerous);
        pane.set_targets(vec![target(tab_ids()[0], "A")]);
        assert!(matches!(
            pane.update(CommandLibraryMessage::Send(CommandRef::User(id))),
            Outcome::Send {
                dangerous: true,
                ..
            }
        ));
        // Even a level kept as Info is judged by the text that reaches the shell.
        let mut other = CommandLibraryPane::default();
        assert!(other.is_dangerous(&fsmo(), "shutdown -h now"));
        assert!(!other.is_dangerous(&fsmo(), "netdom query fsmo"));
        other.update(CommandLibraryMessage::ShowMoreMine);
        assert_eq!(other.more_mine_pages, 1);
        other.update(CommandLibraryMessage::Search("x".to_owned()));
        assert_eq!(other.more_mine_pages, 0);
    }

    /// A session told to the pane, as the window tells it when the tab opens.
    fn open_for(pane: &mut CommandLibraryPane, host: &str, user: Option<&str>, os: RemoteOs) {
        pane.update(CommandLibraryMessage::Session {
            host: Some(host.to_owned()),
            user: user.map(str::to_owned),
            platform: os,
            tab: None,
        });
    }

    /// A user command with `pattern` for Linux, made through the editor; its identity.
    fn make_command(pane: &mut CommandLibraryPane, title: &str, pattern: &str) -> String {
        pane.update(CommandLibraryMessage::New);
        pane.update(CommandLibraryMessage::EditPlatform(UserPlatform::Linux));
        type_into(pane, Field::Title, title);
        type_into(pane, Field::Category, "Network");
        type_into(pane, Field::LinuxPattern, pattern);
        pane.update(CommandLibraryMessage::Save);
        assert!(pane.editor.is_none(), "kept");
        pane.library
            .commands()
            .iter()
            .find(|each| each.title == title)
            .expect("made")
            .id
            .clone()
    }

    #[test]
    fn a_library_opened_from_a_session_starts_on_the_platform_of_its_far_end() {
        let mut pane = CommandLibraryPane::default();
        assert_eq!(pane.platform, None, "opened from no session");
        open_for(&mut pane, "win.lab", None, RemoteOs::Windows);
        assert_eq!(pane.platform, Some(UserPlatform::Windows));
        assert!(
            pane.shown()
                .iter()
                .all(|entry| entry.command().platform != UserPlatform::Linux)
        );
        open_for(&mut pane, "web.lab", None, RemoteOs::Linux);
        assert_eq!(pane.platform, Some(UserPlatform::Linux));
        assert!(
            pane.shown()
                .iter()
                .all(|entry| entry.command().platform != UserPlatform::Windows)
        );
        // Undetermined leaves every platform shown, and the filter stays the user's.
        open_for(&mut pane, "switch.lab", None, RemoteOs::Auto);
        assert_eq!(pane.platform, None);
        pane.update(CommandLibraryMessage::Platform(Some(UserPlatform::Linux)));
        assert_eq!(pane.platform, Some(UserPlatform::Linux));
    }

    #[test]
    fn a_session_with_nothing_to_give_changes_nothing() {
        let mut pane = CommandLibraryPane::default();
        pane.update(CommandLibraryMessage::Session {
            host: Some("  ".to_owned()),
            user: None,
            platform: RemoteOs::Auto,
            tab: None,
        });
        assert_eq!(pane.session, None);
        assert_eq!(pane.platform, None);
        assert!(!pane.history_this_host_only);
        assert_eq!(pane.history_host_filter(), None);
    }

    #[test]
    fn the_session_host_and_user_fill_the_parameters_that_take_them() {
        let folder = tempfile::tempdir().expect("folder");
        let mut pane = pane(folder.path());
        let ping = CommandRef::User(make_command(&mut pane, "Ping", "ping -c 3 {host}"));
        let who = CommandRef::User(make_command(&mut pane, "Who", "id {user}"));
        let other = CommandRef::User(make_command(&mut pane, "Other", "echo {word}"));
        open_for(&mut pane, " web.lab ", Some("ops"), RemoteOs::Linux);
        let copied = |pane: &mut CommandLibraryPane, command: &CommandRef| match pane
            .update(CommandLibraryMessage::Copy(command.clone()))
        {
            Outcome::Copy(_, text) => Some(text),
            _ => None,
        };
        assert_eq!(
            copied(&mut pane, &ping).as_deref(),
            Some("ping -c 3 web.lab"),
            "the host, with nothing typed"
        );
        assert_eq!(copied(&mut pane, &who).as_deref(), Some("id ops"));
        let left = copied(&mut pane, &other).unwrap_or_default();
        assert!(
            !left.contains("web.lab") && !left.contains("ops"),
            "a parameter of another name is left to its user: {left:?}"
        );

        // What is typed wins, and so does emptying the box: the session does not come back.
        let CommandRef::User(id) = &ping else {
            panic!("a user command");
        };
        pane.update(CommandLibraryMessage::UserParam(
            id.clone(),
            "host".to_owned(),
            "db.lab".to_owned(),
        ));
        assert_eq!(
            copied(&mut pane, &ping).as_deref(),
            Some("ping -c 3 db.lab")
        );
        pane.update(CommandLibraryMessage::UserParam(
            id.clone(),
            "host".to_owned(),
            String::new(),
        ));
        assert_ne!(
            copied(&mut pane, &ping).as_deref(),
            Some("ping -c 3 web.lab"),
            "emptied by the user"
        );
    }

    #[test]
    fn a_catalogue_command_takes_the_session_host_by_the_name_of_its_parameter() {
        let mut pane = CommandLibraryPane::default();
        open_for(&mut pane, "10.0.0.7", None, RemoteOs::Linux);
        pane.update(CommandLibraryMessage::Template(
            TRACE.to_owned(),
            TemplatePlatform::Linux,
        ));
        let Outcome::Copy(_, text) = pane.update(CommandLibraryMessage::Copy(CommandRef::Catalog(
            TRACE.to_owned(),
        ))) else {
            panic!("copied");
        };
        assert_eq!(text, "traceroute 10.0.0.7");
    }

    #[test]
    fn a_session_value_is_checked_and_quoted_like_one_typed() {
        let folder = tempfile::tempdir().expect("folder");
        let mut pane = pane(folder.path());
        let ping = CommandRef::User(make_command(&mut pane, "Ping", "ping -c 3 {host}"));
        // The window checks a host before it gives it; if one got here, the generator would
        // still refuse what the shell could read.
        open_for(&mut pane, "web.lab; rm -rf /", None, RemoteOs::Linux);
        assert_eq!(
            pane.update(CommandLibraryMessage::Copy(ping.clone())),
            Outcome::None
        );
        pane.set_targets(vec![target(tab_ids()[0], "A")]);
        assert_eq!(
            pane.update(CommandLibraryMessage::Insert(ping.clone())),
            Outcome::None
        );
        assert_eq!(
            pane.update(CommandLibraryMessage::Send(ping)),
            Outcome::None
        );
        assert!(pane.awaited.is_empty());
    }

    #[test]
    fn an_insert_types_the_command_with_no_question_and_is_recorded_as_inserted() {
        let folder = tempfile::tempdir().expect("folder");
        let mut pane = pane(folder.path());
        let ping = CommandRef::User(make_command(&mut pane, "Ping", "ping -c 3 {host}"));
        let terminal = tab_ids()[0];
        // No terminal: nothing to type into.
        open_for(&mut pane, "web.lab", None, RemoteOs::Linux);
        assert_eq!(
            pane.update(CommandLibraryMessage::Insert(ping.clone())),
            Outcome::None
        );
        pane.set_targets(vec![target(terminal, "web-1")]);
        assert_eq!(
            pane.update(CommandLibraryMessage::Insert(ping.clone())),
            Outcome::Insert {
                target: terminal,
                command: "ping -c 3 web.lab".to_owned(),
            }
        );
        assert!(pane.activity.history().is_empty(), "asked, not typed");
        // A receipt of a run is not the insert's.
        pane.record_sent(&[receipt(terminal, "web-1", "ping -c 3 web.lab")]);
        assert!(pane.activity.history().is_empty());
        let mut typed = receipt(terminal, "web-1", "ping -c 3 web.lab");
        typed.delivery = Delivery::Inserted;
        pane.record_sent(&[typed]);
        let [entry] = pane.activity.history() else {
            panic!("one entry");
        };
        assert_eq!(entry.mode, HistoryMode::Inserted);
        assert_eq!(entry.host, "web.lab");
        assert_eq!(entry.target, "web-1");
        assert_eq!(entry.pattern, "ping -c 3 {host}", "never what was typed");
        assert_eq!(pane.notice, Some(Notice::Inserted("web-1".to_owned())));
    }

    #[test]
    fn an_insert_never_asks_the_dangerous_question_a_send_asks() {
        let mut pane = CommandLibraryPane::default();
        pane.set_targets(vec![target(tab_ids()[0], "A")]);
        assert!(matches!(
            pane.update(CommandLibraryMessage::Send(fsck())),
            Outcome::Send {
                dangerous: true,
                ..
            }
        ));
        // The same command, typed and left at the prompt: there is no question to carry.
        assert!(matches!(
            pane.update(CommandLibraryMessage::Insert(fsck())),
            Outcome::Insert { .. }
        ));
    }

    #[test]
    fn an_insert_refuses_a_command_with_a_line_break_as_a_send_does() {
        let folder = tempfile::tempdir().expect("folder");
        let mut pane = pane(folder.path());
        let id = make_command(&mut pane, "Ping", "ping -c 3 {host}");
        let ping = CommandRef::User(id.clone());
        pane.set_targets(vec![target(tab_ids()[0], "A")]);
        for value in ["web.lab\nrm -rf x", "web.lab\r", "web\u{1b}[2J", "a\tb"] {
            pane.update(CommandLibraryMessage::UserParam(
                id.clone(),
                "host".to_owned(),
                value.to_owned(),
            ));
            assert_eq!(
                pane.update(CommandLibraryMessage::Insert(ping.clone())),
                Outcome::None,
                "{value:?}"
            );
            assert_eq!(
                pane.update(CommandLibraryMessage::Send(ping.clone())),
                Outcome::None,
                "{value:?}"
            );
        }
        assert!(pane.awaited.is_empty(), "nothing is awaited");
    }

    #[test]
    fn shift_enter_inserts_the_command_last_used_and_nothing_before_or_over_the_editor() {
        let folder = tempfile::tempdir().expect("folder");
        let mut pane = pane(folder.path());
        let terminal = tab_ids()[0];
        pane.set_targets(vec![target(terminal, "A")]);
        assert_eq!(
            pane.update(CommandLibraryMessage::InsertSelected),
            Outcome::None,
            "no command used yet"
        );
        let id = make_command(&mut pane, "Ping", "ping -c 3 localhost");
        let ping = CommandRef::User(id);
        pane.update(CommandLibraryMessage::Copy(ping));
        assert_eq!(
            pane.update(CommandLibraryMessage::InsertSelected),
            Outcome::Insert {
                target: terminal,
                command: "ping -c 3 localhost".to_owned(),
            }
        );
        // A key pressed in the editor's fields is not the library's.
        pane.update(CommandLibraryMessage::New);
        assert_eq!(
            pane.update(CommandLibraryMessage::InsertSelected),
            Outcome::None
        );
    }

    #[test]
    fn a_copy_is_recorded_as_copied_against_the_session_host_and_names_no_terminal() {
        let folder = tempfile::tempdir().expect("folder");
        let mut pane = pane(folder.path());
        open_for(&mut pane, "web.lab", None, RemoteOs::Linux);
        let trace = trace(&mut pane, "10.0.0.1");
        let Outcome::Copy(..) = pane.update(CommandLibraryMessage::Copy(trace)) else {
            panic!("copied");
        };
        let [entry] = pane.activity.history() else {
            panic!("one entry");
        };
        assert_eq!(entry.mode, HistoryMode::Copied);
        assert_eq!(entry.host, "web.lab");
        assert_eq!(entry.target, "");
        assert_eq!(entry.pattern, "traceroute {target}");
        let kept = std::fs::read_to_string(folder.path().join(ACTIVITY_FILE)).expect("read");
        assert!(!kept.contains("10.0.0.1"), "what was typed is never kept");
        // A command that cannot be made is not copied, so not recorded.
        pane.update(CommandLibraryMessage::Param(
            TRACE.to_owned(),
            "target".to_owned(),
            "a;b".to_owned(),
        ));
        assert_eq!(
            pane.update(CommandLibraryMessage::Copy(CommandRef::Catalog(
                TRACE.to_owned()
            ))),
            Outcome::None
        );
        assert_eq!(pane.activity.history().len(), 1);
    }

    #[test]
    fn a_command_sent_is_recorded_as_sent_against_the_session_host() {
        let mut pane = CommandLibraryPane::default();
        let terminal = tab_ids()[0];
        pane.set_targets(vec![target(terminal, "web-1")]);
        open_for(&mut pane, "web.lab", None, RemoteOs::Linux);
        let mtr = trace(&mut pane, "10.0.0.1");
        let Outcome::Send { command, .. } = pane.update(CommandLibraryMessage::Send(mtr)) else {
            panic!("asked");
        };
        pane.record_sent(&[receipt(terminal, "web-1", &command)]);
        let [entry] = pane.activity.history() else {
            panic!("one entry");
        };
        assert_eq!(
            (entry.mode, entry.host.as_str()),
            (HistoryMode::Sent, "web.lab")
        );
    }

    #[test]
    fn this_host_only_limits_the_history_to_the_session_host_and_starts_ticked() {
        let mut pane = CommandLibraryPane::default();
        let row = |host: &str, mode: HistoryMode, at: i64| HistoryEntry {
            command: command_activity::catalog_key(TRACE),
            title: "Trace".to_owned(),
            pattern: "traceroute {target}".to_owned(),
            parameters: vec!["target".to_owned()],
            target: String::new(),
            host: host.to_owned(),
            mode,
            at,
        };
        let now = now_unix();
        pane.activity
            .record(row("db.lab", HistoryMode::Sent, now - 3));
        pane.activity.record(row("", HistoryMode::Sent, now - 2));
        pane.activity
            .record(row("WEB.lab", HistoryMode::Copied, now - 1));
        pane.activity
            .record(row("web.lab", HistoryMode::Inserted, now));
        // Opened from no session: every row, and no box to untick.
        assert_eq!(pane.history_rows().len(), 4);

        open_for(&mut pane, "web.lab", None, RemoteOs::Linux);
        assert!(
            pane.history_this_host_only,
            "ticked when the library has a session"
        );
        let shown: Vec<_> = pane
            .history_rows()
            .into_iter()
            .map(|(index, entry)| (index, entry.mode))
            .collect();
        assert_eq!(
            shown,
            [(0, HistoryMode::Inserted), (1, HistoryMode::Copied)],
            "this host's rows by their place in the whole history, case aside; the row with \
             no host is not any host's"
        );
        pane.update(CommandLibraryMessage::HistoryThisHostOnly(false));
        assert_eq!(pane.history_rows().len(), 4, "every host's");
        pane.update(CommandLibraryMessage::HistoryThisHostOnly(true));
        assert_eq!(pane.history_rows().len(), 2);
    }

    #[test]
    fn a_replay_from_a_filtered_history_replays_the_row_clicked() {
        let folder = tempfile::tempdir().expect("folder");
        let mut pane = pane(folder.path());
        let terminal = tab_ids()[0];
        pane.set_targets(vec![target(terminal, "web-1")]);
        let id = make_command(&mut pane, "Ping", "ping -c 3 localhost");
        pane.activity.record(HistoryEntry {
            command: command_activity::user_key(&id),
            title: "Ping".to_owned(),
            pattern: "ping -c 3 localhost".to_owned(),
            parameters: Vec::new(),
            target: "web-1".to_owned(),
            host: "db.lab".to_owned(),
            mode: HistoryMode::Sent,
            at: now_unix(),
        });
        pane.activity.record(HistoryEntry {
            command: command_activity::user_key(&id),
            title: "Ping".to_owned(),
            pattern: "ping -c 3 localhost".to_owned(),
            parameters: Vec::new(),
            target: "web-1".to_owned(),
            host: "web.lab".to_owned(),
            mode: HistoryMode::Sent,
            at: now_unix(),
        });
        open_for(&mut pane, "web.lab", None, RemoteOs::Linux);
        let [(index, _)] = pane.history_rows()[..] else {
            panic!("this host's row alone");
        };
        assert_eq!(index, 0, "the latest is first in the whole history");
        let Outcome::Send { command, .. } = pane.update(CommandLibraryMessage::Replay(index))
        else {
            panic!("asked");
        };
        assert_eq!(command, "ping -c 3 localhost");
    }

    #[test]
    fn commands_go_first_to_the_terminal_of_the_session_the_library_was_opened_from() {
        let mut pane = CommandLibraryPane::default();
        let [first, second, third] = tab_ids();
        pane.set_targets(vec![
            target(first, "A"),
            target(second, "B"),
            target(third, "C"),
        ]);
        pane.update(CommandLibraryMessage::Session {
            host: Some("web.lab".to_owned()),
            user: None,
            platform: RemoteOs::Linux,
            tab: Some(second),
        });
        let ping = trace(&mut pane, "10.0.0.1");
        assert!(matches!(
            pane.update(CommandLibraryMessage::Insert(ping.clone())),
            Outcome::Insert { target, .. } if target == second
        ));
        // The user's own choice wins, and a session terminal gone leaves the first.
        pane.update(CommandLibraryMessage::Target(third));
        assert!(matches!(
            pane.update(CommandLibraryMessage::Insert(ping.clone())),
            Outcome::Insert { target, .. } if target == third
        ));
        pane.target = None;
        pane.set_targets(vec![target(first, "A"), target(third, "C")]);
        assert!(matches!(
            pane.update(CommandLibraryMessage::Insert(ping)),
            Outcome::Insert { target, .. } if target == first
        ));
    }

    /// A pane over a folder holding `runbooks`.
    fn run_pane(folder: &Path, runbooks: Vec<Runbook>) -> CommandLibraryPane {
        let mut store = RunbookStore::default();
        for runbook in runbooks {
            store.keep(runbook, &|_: &str| None).expect("kept");
        }
        store.save(&folder.join(RUNBOOKS_FILE)).expect("saved");
        pane(folder)
    }

    /// Two actions of the user: one that runs, one marked dangerous.
    const TWO_ACTIONS: &str = r#"{"actions": [
        {"id": "calm", "title": "Calm", "category": "mine", "platform": 1, "level": 0,
         "linuxCommandTemplate": {"platform": 1, "commandPattern": "echo calm", "parameters": []}},
        {"id": "grave", "title": "Grave", "category": "mine", "platform": 1, "level": 2,
         "linuxCommandTemplate": {"platform": 1, "commandPattern": "echo grave", "parameters": []}}
    ]}"#;

    fn action_step(id: &str) -> Step {
        Step::action(id, id, BTreeMap::new())
    }

    /// An action, a literal command and a dangerous action.
    fn three_step_runbook() -> Runbook {
        let mut runbook = Runbook::new("Three");
        runbook.steps = vec![
            action_step("calm"),
            Step::literal("uptime", TemplatePlatform::Linux),
            action_step("grave"),
        ];
        runbook
    }

    /// Imports the two actions and starts a run of the first runbook of the pane.
    fn start_run(pane: &mut CommandLibraryPane) {
        pane.update(CommandLibraryMessage::Imported(Some(Ok(
            TWO_ACTIONS.to_owned()
        ))));
        let id = pane.runbooks.first_id().expect("a runbook");
        pane.update(CommandLibraryMessage::Runbooks(RunbookMessage::Toggle));
        pane.update(CommandLibraryMessage::Runbooks(RunbookMessage::Run(id)));
        // A press right after a delivery is ignored for a while; the tests that press twice
        // on purpose say so.
        pane.runbooks.set_guard(std::time::Duration::ZERO);
    }

    fn run_message(pane: &mut CommandLibraryPane, message: RunbookMessage) -> Outcome {
        pane.update(CommandLibraryMessage::Runbooks(message))
    }

    #[test]
    fn a_run_sends_nothing_by_itself_and_a_step_moves_on_only_when_the_terminal_has_it() {
        let folder = tempfile::tempdir().expect("folder");
        let mut pane = run_pane(folder.path(), vec![three_step_runbook()]);
        let terminal = tab_ids()[0];
        pane.set_targets(vec![target(terminal, "web-1")]);
        open_for(&mut pane, "web.lab", None, RemoteOs::Linux);
        start_run(&mut pane);

        // Starting, looking and waiting write nothing and ask nothing.
        assert!(pane.awaited.is_empty());
        assert!(pane.activity.history().is_empty());
        assert_eq!(pane.runbooks.status_of(0), Some(StepStatus::Pending));

        // The press asks; the question is still to be answered, so the step stays where it is.
        let Outcome::Send {
            target: asked,
            command,
            dangerous,
            typed_as_is,
        } = run_message(&mut pane, RunbookMessage::RunSend)
        else {
            panic!("asked");
        };
        assert_eq!(
            (asked, command.as_str(), dangerous, typed_as_is),
            (terminal, "echo calm", false, false)
        );
        assert_eq!(pane.runbooks.current_index(), Some(0));
        assert!(
            pane.activity.history().is_empty(),
            "not recorded before it is written"
        );

        // The terminal has it: recorded like a send of the library, and the run goes on to
        // the next step, which is not sent.
        pane.record_sent(&[receipt(terminal, "web-1", &command)]);
        assert_eq!(pane.runbooks.status_of(0), Some(StepStatus::Sent));
        assert_eq!(pane.runbooks.current_index(), Some(1));
        let [entry] = pane.activity.history() else {
            panic!("one row");
        };
        assert_eq!(
            (entry.mode, entry.host.as_str(), entry.target.as_str()),
            (HistoryMode::Sent, "web.lab", "web-1")
        );
        assert_eq!(entry.pattern, "echo calm");
        assert_eq!(entry.command, command_activity::user_key("calm"));
        assert!(pane.awaited.is_empty(), "nothing waits for the next step");
    }

    #[test]
    fn one_step_is_waited_for_at_a_time_and_a_stop_forgets_it() {
        let folder = tempfile::tempdir().expect("folder");
        let mut pane = run_pane(folder.path(), vec![three_step_runbook()]);
        let terminal = tab_ids()[0];
        pane.set_targets(vec![target(terminal, "web-1")]);
        start_run(&mut pane);

        // A question answered no leaves the step to be sent again.
        for _ in 0..3 {
            assert!(matches!(
                run_message(&mut pane, RunbookMessage::RunSend),
                Outcome::Send { .. }
            ));
        }
        assert_eq!(pane.awaited.len(), 1);
        assert_eq!(pane.runbooks.current_index(), Some(0));

        // A late receipt after a stop moves nothing and records nothing.
        run_message(&mut pane, RunbookMessage::RunStop);
        assert!(pane.awaited.is_empty());
        pane.record_sent(&[receipt(terminal, "web-1", "echo calm")]);
        assert!(pane.activity.history().is_empty());
    }

    #[test]
    fn the_dangerous_question_is_asked_for_a_dangerous_action_and_always_for_a_literal_step() {
        let folder = tempfile::tempdir().expect("folder");
        let mut pane = run_pane(folder.path(), vec![three_step_runbook()]);
        let terminal = tab_ids()[0];
        pane.set_targets(vec![target(terminal, "web-1")]);
        start_run(&mut pane);

        let mut asked = Vec::new();
        for _ in 0..3 {
            let Outcome::Send {
                command,
                dangerous,
                typed_as_is,
                ..
            } = run_message(&mut pane, RunbookMessage::RunSend)
            else {
                panic!("asked");
            };
            asked.push((command.clone(), dangerous, typed_as_is));
            pane.record_sent(&[receipt(terminal, "web-1", &command)]);
        }
        assert_eq!(
            asked,
            [
                ("echo calm".to_owned(), false, false),
                // Always asked, and said to be typed as is, not called dangerous for that.
                ("uptime".to_owned(), false, true),
                ("echo grave".to_owned(), true, false),
            ]
        );
        assert!(pane.runbooks.is_finished());
        // A literal step leaves no row of the history, as the C#: a row belongs to an action.
        let titles: Vec<&str> = pane
            .activity
            .history()
            .iter()
            .map(|entry| entry.title.as_str())
            .collect();
        assert_eq!(titles, ["Grave", "Calm"]);
    }

    #[test]
    fn an_insert_types_the_step_with_no_question_and_is_recorded_as_inserted() {
        let folder = tempfile::tempdir().expect("folder");
        let mut pane = run_pane(folder.path(), vec![three_step_runbook()]);
        let terminal = tab_ids()[0];
        pane.set_targets(vec![target(terminal, "web-1")]);
        start_run(&mut pane);
        run_message(&mut pane, RunbookMessage::RunSkip);
        run_message(&mut pane, RunbookMessage::RunSkip);

        // The last step is the dangerous one: Insert still asks nothing, as the library's.
        let Outcome::Insert {
            target: typed,
            command,
        } = run_message(&mut pane, RunbookMessage::RunInsert)
        else {
            panic!("typed");
        };
        assert_eq!((typed, command.as_str()), (terminal, "echo grave"));
        let mut inserted = receipt(terminal, "web-1", &command);
        inserted.delivery = Delivery::Inserted;
        pane.record_sent(&[inserted]);
        assert_eq!(pane.runbooks.status_of(2), Some(StepStatus::Inserted));
        let [entry] = pane.activity.history() else {
            panic!("one row");
        };
        assert_eq!(entry.mode, HistoryMode::Inserted);
    }

    #[test]
    fn a_copy_marks_the_step_copied_and_is_recorded_for_an_action() {
        let folder = tempfile::tempdir().expect("folder");
        let mut pane = run_pane(folder.path(), vec![three_step_runbook()]);
        start_run(&mut pane);
        // No terminal needed to copy.
        assert_eq!(
            run_message(&mut pane, RunbookMessage::RunCopy),
            Outcome::Copy(CopySlot::CmdLibRunbookStep, "echo calm".to_owned())
        );
        assert_eq!(pane.runbooks.status_of(0), Some(StepStatus::Copied));
        assert_eq!(pane.activity.history()[0].mode, HistoryMode::Copied);
        // The literal step: copied, no row.
        assert!(matches!(
            run_message(&mut pane, RunbookMessage::RunCopy),
            Outcome::Copy(CopySlot::CmdLibRunbookStep, text) if text == "uptime"
        ));
        assert_eq!(pane.activity.history().len(), 1);
    }

    #[test]
    fn skip_and_stop_send_nothing_and_leave_no_row() {
        let folder = tempfile::tempdir().expect("folder");
        let mut pane = run_pane(folder.path(), vec![three_step_runbook()]);
        let terminal = tab_ids()[0];
        pane.set_targets(vec![target(terminal, "web-1")]);
        start_run(&mut pane);
        assert_eq!(
            run_message(&mut pane, RunbookMessage::RunSkip),
            Outcome::None
        );
        assert_eq!(pane.runbooks.status_of(0), Some(StepStatus::Skipped));
        assert_eq!(pane.runbooks.current_index(), Some(1));
        assert_eq!(
            run_message(&mut pane, RunbookMessage::RunStop),
            Outcome::None
        );
        assert!(!pane.runbooks.is_running());
        assert!(pane.awaited.is_empty());
        assert!(pane.activity.history().is_empty());
        // Nothing is sent by a stopped run.
        assert_eq!(
            run_message(&mut pane, RunbookMessage::RunSend),
            Outcome::None
        );
    }

    #[test]
    fn a_step_goes_to_the_terminal_of_the_session_first_and_to_none_without_one() {
        let folder = tempfile::tempdir().expect("folder");
        let mut pane = run_pane(folder.path(), vec![three_step_runbook()]);
        let [first, second, _] = tab_ids();
        start_run(&mut pane);
        assert_eq!(
            run_message(&mut pane, RunbookMessage::RunSend),
            Outcome::None,
            "no terminal: nothing asked, nothing awaited"
        );
        assert!(pane.awaited.is_empty());
        assert_eq!(pane.runbooks.current_index(), Some(0));
        pane.runbooks.stop_for_test();

        pane.set_targets(vec![target(first, "A"), target(second, "B")]);
        pane.update(CommandLibraryMessage::Session {
            host: None,
            user: None,
            platform: RemoteOs::Linux,
            tab: Some(second),
        });
        start_run(&mut pane);
        assert!(matches!(
            run_message(&mut pane, RunbookMessage::RunSend),
            Outcome::Send { target, .. } if target == second
        ));
        // The user's own pick moves the run, and nothing else does.
        pane.update(CommandLibraryMessage::Target(first));
        assert!(matches!(
            run_message(&mut pane, RunbookMessage::RunInsert),
            Outcome::Insert { target, .. } if target == first
        ));
    }

    #[test]
    fn a_run_stays_on_its_terminal_and_waits_when_it_is_gone_or_the_session_changes() {
        let folder = tempfile::tempdir().expect("folder");
        let mut pane = run_pane(folder.path(), vec![three_step_runbook()]);
        let [first, second, _] = tab_ids();
        pane.set_targets(vec![target(first, "A"), target(second, "B")]);
        start_run(&mut pane);
        assert!(matches!(
            run_message(&mut pane, RunbookMessage::RunSend),
            Outcome::Send { target, .. } if target == first
        ));
        run_message(&mut pane, RunbookMessage::RunSkip);

        // The pinned terminal closes: never the first one left.
        pane.set_targets(vec![target(second, "B")]);
        assert_eq!(pane.run_target(), RunTarget::Gone);
        for message in [RunbookMessage::RunSend, RunbookMessage::RunInsert] {
            assert_eq!(run_message(&mut pane, message), Outcome::None);
        }
        assert!(pane.awaited.is_empty());
        // Picking a terminal is the user's decision.
        pane.update(CommandLibraryMessage::Target(second));
        assert!(matches!(
            run_message(&mut pane, RunbookMessage::RunSend),
            Outcome::Send { target, .. } if target == second
        ));

        // A session that changes mid-run blocks it the same way.
        run_message(&mut pane, RunbookMessage::RunStop);
        pane.set_targets(vec![target(first, "A"), target(second, "B")]);
        start_run(&mut pane);
        open_for(&mut pane, "other.lab", None, RemoteOs::Windows);
        assert_eq!(pane.run_target(), RunTarget::Changed);
        assert_eq!(
            run_message(&mut pane, RunbookMessage::RunSend),
            Outcome::None
        );
        pane.update(CommandLibraryMessage::Target(first));
        assert!(matches!(
            run_message(&mut pane, RunbookMessage::RunSend),
            Outcome::Send { target, .. } if target == first
        ));
    }

    #[test]
    fn a_double_press_cannot_deliver_the_step_after() {
        let folder = tempfile::tempdir().expect("folder");
        let mut pane = run_pane(folder.path(), vec![three_step_runbook()]);
        let terminal = tab_ids()[0];
        pane.set_targets(vec![target(terminal, "A")]);
        start_run(&mut pane);
        pane.runbooks
            .set_guard(std::time::Duration::from_secs(3600));
        assert!(matches!(
            run_message(&mut pane, RunbookMessage::RunCopy),
            Outcome::Copy(..)
        ));
        for message in [
            RunbookMessage::RunCopy,
            RunbookMessage::RunSend,
            RunbookMessage::RunInsert,
        ] {
            assert_eq!(run_message(&mut pane, message), Outcome::None);
        }
        assert_eq!(pane.runbooks.status_of(1), Some(StepStatus::Pending));
        assert!(pane.awaited.is_empty());
    }

    #[test]
    fn a_text_sent_from_a_card_after_a_question_closed_is_not_the_run_step() {
        let folder = tempfile::tempdir().expect("folder");
        let mut pane = run_pane(folder.path(), vec![three_step_runbook()]);
        let terminal = tab_ids()[0];
        pane.set_targets(vec![target(terminal, "A")]);
        start_run(&mut pane);
        // The question of the step is closed without sending.
        assert!(matches!(
            run_message(&mut pane, RunbookMessage::RunSend),
            Outcome::Send { .. }
        ));
        // The same text from the card of the action.
        assert!(matches!(
            pane.update(CommandLibraryMessage::Send(CommandRef::User(
                "calm".to_owned()
            ))),
            Outcome::Send { .. }
        ));
        pane.record_sent(&[receipt(terminal, "A", "echo calm")]);
        assert_eq!(
            pane.runbooks.current_index(),
            Some(0),
            "the run did not move"
        );
        assert_eq!(pane.activity.history().len(), 1, "one row");

        // A skip forgets a step still waited for.
        run_message(&mut pane, RunbookMessage::RunSend);
        run_message(&mut pane, RunbookMessage::RunSkip);
        assert!(pane.awaited.is_empty());
    }

    #[test]
    fn the_key_that_inserts_the_command_last_used_is_not_the_runbooks() {
        let folder = tempfile::tempdir().expect("folder");
        let mut pane = run_pane(folder.path(), vec![three_step_runbook()]);
        let terminal = tab_ids()[0];
        pane.set_targets(vec![target(terminal, "A")]);
        let mtr = trace(&mut pane, "10.0.0.1");
        pane.update(CommandLibraryMessage::Copy(mtr));
        assert!(matches!(
            pane.update(CommandLibraryMessage::InsertSelected),
            Outcome::Insert { .. }
        ));
        pane.update(CommandLibraryMessage::Runbooks(RunbookMessage::Toggle));
        assert_eq!(
            pane.update(CommandLibraryMessage::InsertSelected),
            Outcome::None
        );
    }

    #[test]
    fn a_step_the_generator_refuses_at_the_run_gives_nothing_to_send_insert_or_copy() {
        let folder = tempfile::tempdir().expect("folder");
        let mut runbook = Runbook::new("Trace");
        runbook
            .variables
            .insert("target".to_owned(), "web.lab".to_owned());
        runbook.steps = vec![Step::action(
            TRACE,
            "Trace",
            BTreeMap::from([("target".to_owned(), "{{target}}".to_owned())]),
        )];
        let mut pane = run_pane(folder.path(), vec![runbook]);
        let terminal = tab_ids()[0];
        pane.set_targets(vec![target(terminal, "web-1")]);
        // The session host fills the variable, and the generator judges it at the run.
        open_for(&mut pane, "web.lab; reboot", None, RemoteOs::Linux);
        let id = pane.runbooks.first_id().expect("a runbook");
        pane.update(CommandLibraryMessage::Runbooks(RunbookMessage::Toggle));
        run_message(&mut pane, RunbookMessage::Run(id));
        run_message(&mut pane, RunbookMessage::RunBegin);
        for message in [
            RunbookMessage::RunSend,
            RunbookMessage::RunInsert,
            RunbookMessage::RunCopy,
        ] {
            assert_eq!(run_message(&mut pane, message), Outcome::None);
        }
        assert!(pane.awaited.is_empty());
        assert!(pane.activity.history().is_empty());
        assert_eq!(pane.runbooks.status_of(0), Some(StepStatus::Pending));
    }

    #[test]
    fn a_session_opened_between_the_start_and_the_variables_being_confirmed_blocks_the_run() {
        let folder = tempfile::tempdir().expect("folder");
        let mut runbook = Runbook::new("Trace");
        runbook
            .variables
            .insert("target".to_owned(), "web.lab".to_owned());
        runbook.steps = vec![Step::action(
            TRACE,
            "Trace",
            BTreeMap::from([("target".to_owned(), "{{target}}".to_owned())]),
        )];
        let mut pane = run_pane(folder.path(), vec![runbook]);
        let [first, second, _] = tab_ids();
        pane.set_targets(vec![target(first, "A"), target(second, "B")]);
        open_for(&mut pane, "first.lab", None, RemoteOs::Linux);
        let id = pane.runbooks.first_id().expect("a runbook");
        run_message(&mut pane, RunbookMessage::Run(id));
        pane.runbooks.set_guard(std::time::Duration::ZERO);

        // A new session while the values are being confirmed.
        open_for(&mut pane, "second.lab", None, RemoteOs::Windows);
        run_message(&mut pane, RunbookMessage::RunBegin);
        assert_eq!(pane.run_target(), RunTarget::Changed);
        assert!(pane.run_warning().is_some());
        assert_eq!(
            run_message(&mut pane, RunbookMessage::RunSend),
            Outcome::None
        );

        // Picking a terminal takes the system of the session now open, not the first one's.
        pane.update(CommandLibraryMessage::Target(second));
        let Outcome::Send { command, .. } = run_message(&mut pane, RunbookMessage::RunSend) else {
            panic!("asked");
        };
        assert_eq!(
            command, "tracert first.lab",
            "first session's value, new system"
        );
    }

    #[test]
    fn terminals_opened_after_a_run_began_without_one_are_to_be_picked() {
        let folder = tempfile::tempdir().expect("folder");
        let mut pane = run_pane(folder.path(), vec![three_step_runbook()]);
        start_run(&mut pane);
        assert_eq!(
            pane.run_warning(),
            None,
            "no terminal at all: the usual hint"
        );
        pane.set_targets(vec![target(tab_ids()[0], "A")]);
        assert_eq!(pane.run_target(), RunTarget::NoTerminal);
        assert!(pane.run_warning().is_some());
        assert_eq!(
            run_message(&mut pane, RunbookMessage::RunSend),
            Outcome::None
        );
    }

    #[test]
    fn opening_the_runbooks_clears_an_old_notice_and_leaves_the_history_and_the_editor_alone() {
        let folder = tempfile::tempdir().expect("folder");
        let mut pane = run_pane(folder.path(), vec![three_step_runbook()]);
        pane.notice = Some(Notice::Inserted("A".to_owned()));
        pane.update(CommandLibraryMessage::ToggleHistory);
        assert!(pane.history_open);
        pane.update(CommandLibraryMessage::Runbooks(RunbookMessage::Toggle));
        assert_eq!(pane.notice, None);
        assert!(!pane.history_open, "the history is not the view any more");

        // And the other way round: the history takes the view back.
        pane.update(CommandLibraryMessage::ToggleHistory);
        assert!(pane.history_open);
        assert!(!pane.runbooks.is_open());

        // The editor takes the view from the history too.
        pane.update(CommandLibraryMessage::New);
        assert!(pane.editor.is_some());
        assert!(!pane.history_open);
    }

    #[test]
    fn a_run_started_clears_the_notice_of_an_insert() {
        let folder = tempfile::tempdir().expect("folder");
        let mut pane = run_pane(folder.path(), vec![three_step_runbook()]);
        start_run(&mut pane);
        pane.notice = Some(Notice::Inserted("A".to_owned()));
        let id = pane.runbooks.first_id().expect("a runbook");
        run_message(&mut pane, RunbookMessage::RunStop);
        run_message(&mut pane, RunbookMessage::Run(id));
        assert_eq!(pane.notice, None);
    }
}
