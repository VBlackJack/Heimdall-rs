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

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use heimdall_app::{SendTarget, SentCommand, TabId};
use heimdall_core::tools::command_activity::{
    self, ACTIVITY_FILE, ActivityError, CommandActivity, HistoryEntry,
};
use heimdall_core::tools::command_library::{
    self, CatalogueEntry, CommandIssue, LEGACY_SNIPPET_IDS,
};
use heimdall_core::tools::user_commands::{
    self, Criticality, Draft, DraftError, GenerateError, ImportReport, LIBRARY_FILE, LibraryError,
    TemplatePlatform, UserCommand, UserLibrary, UserPlatform, ValueError,
};
use iced::widget::{
    Column, Row, button, checkbox, column, container, row, scrollable, text, text_input,
};
use iced::{Element, Length, Task, window};

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

/// The one control character a command may hold.
const TAB: char = '\t';

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
    /// The terminal tab the commands are sent to.
    Target(TabId),
    /// Start a new command of the user.
    New,
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
    /// Ask where to export the library.
    Export,
    /// The library was written to the file picked; `None` when none was picked.
    Exported(Option<Result<(), TransferError>>),
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
            Self::None | Self::Copy(..) | Self::Send { .. } | Self::Broadcast { .. } => {
                Task::none()
            }
        }
    }
}

/// What the pane tells under its buttons.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Notice {
    /// A file imported.
    Imported(ImportReport),
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
}

/// A command asked to be written, until the application says it was.
#[derive(Debug, Clone)]
struct Awaited {
    /// The command as it is typed.
    command: String,
    /// What the history keeps of it, but where and when.
    entry: HistoryEntry,
    /// The terminals asked, with their names.
    tabs: Vec<(TabId, String)>,
    /// Asked to several terminals at once.
    broadcast: bool,
}

/// The editor: the draft and why it was not kept.
#[derive(Debug, Clone, Default)]
struct Editor {
    draft: Draft,
    problem: Option<DraftError>,
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
                    | LibraryError::TooManyActions,
                ) => {
                    pane.failure = Some(Failure::Unreadable);
                }
            }
        }
        pane
    }

    /// The terminals commands can be sent to, as the application offers them now.
    pub fn set_targets(&mut self, targets: Vec<SendTarget>) {
        self.targets = targets;
    }

    /// The terminal a command goes to: the one chosen while it is still there, else the
    /// first.
    fn effective_target(&self) -> Option<&SendTarget> {
        self.target
            .and_then(|chosen| self.targets.iter().find(|each| each.tab == chosen))
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
                    Some(text) => Outcome::Copy(self.copy_slot(&command), text),
                    None => Outcome::None,
                };
            }
            CommandLibraryMessage::Send(command) => {
                self.selected = Some(command.clone());
                return self.send(&command);
            }
            CommandLibraryMessage::Broadcast(command) => {
                self.selected = Some(command.clone());
                return self.broadcast_outcome(&command);
            }
            CommandLibraryMessage::Target(tab) => self.target = Some(tab),
            CommandLibraryMessage::Import if self.can_edit() => return Outcome::Import,
            CommandLibraryMessage::Export if !self.library.commands().is_empty() => {
                return Outcome::Export(self.library.export());
            }
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

    /// The file read is added to the library, as the C# import, and said what it did.
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
        let before = self.library.clone();
        match self.library.import(&text) {
            Ok(report) => {
                // A level in a file is its author's claim: a destructive pattern is
                // dangerous whatever the file says.
                self.library
                    .raise_dangerous(|pattern| heimdall_app::dangerous_command(pattern).is_some());
                if self.keep(&before) {
                    self.notice = Some(Notice::Imported(report));
                }
            }
            Err(LibraryError::TooLarge) => {
                self.notice = Some(Notice::TransferFailed(TransferError::TooLarge));
            }
            Err(LibraryError::Io(error)) => {
                self.notice = Some(Notice::TransferFailed(TransferError::Failed(
                    error.to_string(),
                )));
            }
            Err(LibraryError::Format) => {
                self.notice = Some(Notice::TransferFailed(TransferError::Format));
            }
            Err(LibraryError::TooManyActions) => {
                self.notice = Some(Notice::TransferFailed(TransferError::TooManyActions));
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
            self.editor = Some(Editor {
                draft,
                problem: None,
            });
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
                    receipts
                        .iter()
                        .any(|each| each.tab == *tab && each.command == awaited.command)
                })
                .cloned()
                .collect();
            if reached.is_empty() {
                self.awaited.push(awaited);
                continue;
            }
            let mut entry = awaited.entry.clone();
            entry.target = reached
                .iter()
                .map(|(_, name)| name.as_str())
                .collect::<Vec<_>>()
                .join(NAME_SEPARATOR);
            entry.at = now_unix();
            self.activity.record(entry);
            self.keep_activity();
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

    /// The messages about the user's commands.
    fn edit(&mut self, message: CommandLibraryMessage) {
        match message {
            CommandLibraryMessage::New if self.can_edit() => {
                self.editor = Some(Editor::default());
            }
            CommandLibraryMessage::Edit(id) if self.can_edit() => {
                if let Some(command) = self.library.get(&id) {
                    self.editor = Some(Editor {
                        draft: Draft::of(command),
                        problem: None,
                    });
                }
            }
            CommandLibraryMessage::Field(field, value) => {
                if let Some(editor) = &mut self.editor {
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
                    editor.draft.platform = platform;
                }
            }
            CommandLibraryMessage::EditLevel(level) => {
                if let Some(editor) = &mut self.editor {
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
        match self.library.upsert(&editor.draft) {
            Ok(_) => {
                if self.keep(&before) {
                    self.editor = None;
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
    ) {
        let Some(entry) = self.history_entry(command) else {
            return;
        };
        if self.awaited.len() >= MAX_AWAITED {
            self.awaited.remove(0);
        }
        self.awaited.push(Awaited {
            command: text.to_owned(),
            entry,
            tabs,
            broadcast,
        });
    }

    /// Asks the application to question the user about sending `command`.
    fn send(&mut self, command: &CommandRef) -> Outcome {
        let (Some(target), Some(text)) =
            (self.effective_target().cloned(), self.command_text(command))
        else {
            return Outcome::None;
        };
        let dangerous = self.is_dangerous(command, &text);
        self.await_receipt(command, &text, vec![(target.tab, target.name)], false);
        Outcome::Send {
            target: target.tab,
            command: text,
            dangerous,
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
        self.await_receipt(command, &text, chosen, true);
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

    /// The command `shown`, of `reference`, with the values typed.
    fn render(&self, reference: &CommandRef, shown: &UserCommand) -> Result<String, GenerateError> {
        let platform = self
            .template_shown(reference, shown)
            .ok_or(GenerateError::EmptyPattern)?;
        let template = shown
            .template(platform)
            .ok_or(GenerateError::EmptyPattern)?;
        let command =
            user_commands::generate(template, platform, |name| self.typed(reference, name))?;
        // A line break would submit the first line before the rest is read, whatever the
        // pattern's author meant: no copy, no send, no receipt for such a command.
        if command.chars().any(|c| c != TAB && c.is_control()) {
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
                false,
                (!self.library.commands().is_empty()).then_some(CommandLibraryMessage::Export)
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
        ]
        .spacing(spacing::SM);
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
        if let Some(failure) = self.failure {
            page = page.push(
                text(match failure {
                    Failure::Unreadable => fl!("ui-tool-cmdlib-library-unreadable"),
                    Failure::Unsaved => fl!("ui-tool-cmdlib-library-unsaved"),
                })
                .size(font_size::BODY)
                .style(text::danger),
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
        if self.activity.history().is_empty() {
            body = body.push(text(fl!("ui-tool-cmdlib-history-empty")).size(font_size::BODY));
        }
        for (index, entry) in self.activity.history().iter().enumerate() {
            let when = heimdall_app::local_time_text(entry.at);
            let line = row![
                column![
                    text(entry.title.as_str())
                        .size(font_size::BODY)
                        .font(styles::SEMIBOLD),
                    text(fl!(
                        "ui-tool-cmdlib-history-sent",
                        target = entry.target.as_str(),
                        when = when
                    ))
                    .size(font_size::CAPTION)
                    .style(text::secondary),
                    text(entry.pattern.as_str())
                        .font(super::BOX_FONT)
                        .size(font_size::BODY),
                ]
                .spacing(spacing::XS)
                .width(Length::Fill),
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
                    text::danger
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
                    let value = self.typed(reference, &name).unwrap_or("");
                    let catalog = matches!(reference, CommandRef::Catalog(_));
                    row![
                        text(if parameter.label.is_empty() {
                            parameter.name.clone()
                        } else {
                            parameter.label.clone()
                        })
                        .size(font_size::BODY)
                        .width(PARAM_LABEL_WIDTH),
                        text_input(parameter.default_value.as_deref().unwrap_or(""), value)
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
                    .style(text::danger),
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
fn language() -> String {
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
        Notice::Imported(report) => {
            let done = fl!(
                "ui-tool-cmdlib-import-done",
                imported = report.imported,
                updated = report.updated,
                skipped = report.skipped
            );
            match &report.first_skipped {
                Some((title, issue)) => (
                    fl!(
                        "ui-tool-cmdlib-import-skipped",
                        done = done,
                        title = title.as_str(),
                        reason = issue_text(issue)
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
        Notice::ActivityUnsaved => (fl!("ui-tool-cmdlib-activity-unsaved"), true),
    };
    text(message)
        .size(font_size::BODY)
        .style(if failed {
            text::danger
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
                .style(text::danger),
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
fn category_name(key: &str) -> String {
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
    }
}

/// Why a command of the user cannot be made from the values typed.
fn generate_error(error: &GenerateError) -> String {
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
fn value_error(error: &ValueError) -> String {
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
        }
    }

    #[test]
    fn a_file_of_commands_is_imported_and_said_what_it_did() {
        let source = tempfile::tempdir().expect("folder");
        let mut origin = pane(source.path());
        make_ping(&mut origin);
        let Outcome::Export(exported) = origin.update(CommandLibraryMessage::Export) else {
            panic!("export asked");
        };
        assert!(exported.contains("schemaVersion"), "the C# envelope");

        let folder = tempfile::tempdir().expect("folder");
        let mut pane = pane(folder.path());
        assert_eq!(pane.update(CommandLibraryMessage::Import), Outcome::Import);
        pane.update(CommandLibraryMessage::Imported(Some(Ok(exported.clone()))));
        assert_eq!(pane.library.commands().len(), 1);
        assert!(matches!(
            &pane.notice,
            Some(Notice::Imported(report))
                if (report.imported, report.updated, report.skipped) == (1, 0, 0)
        ));
        let again = self::pane(folder.path());
        assert_eq!(again.library.commands().len(), 1, "kept in the file");

        // The same file again replaces the command, as the C# import.
        pane.update(CommandLibraryMessage::Imported(Some(Ok(exported))));
        assert_eq!(pane.library.commands().len(), 1);
        assert!(matches!(
            pane.notice,
            Some(Notice::Imported(ImportReport { updated: 1, .. }))
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
        let Some(Notice::Imported(report)) = &pane.notice else {
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
}
