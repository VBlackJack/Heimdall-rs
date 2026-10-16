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

//! The notes, as the C# `NotesToolView` keeps them through its `NotesStorageService`: one
//! Markdown file per note under `<data>/config/notes`, listed newest first with the title of
//! its first heading, its tags and the date of its file; the note selected edited and saved
//! 850 ms after the last change, as the C# `_saveTimer`, and when another is selected or the
//! tab closes; copied as Markdown or as HTML. Changes that cannot be written are never let
//! go: the note stays shown, and the tab open, until they are saved or discarded.

use std::path::{Path, PathBuf};
use std::time::Duration;

use heimdall_app::TabId;
use heimdall_core::tools::date_time::Instant;
use heimdall_core::tools::notes_storage::{self, Note};
use iced::widget::text_editor::{Action, Content};
use iced::widget::{Column, button, column, container, row, scrollable, text, text_input};
use iced::{Element, Length, Task};

use super::{CopySlot, ToolMessage, ToolPane};
use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, spacing};

/// How long after the last change a note is saved, as the C# `_saveTimer` interval.
pub const SAVE_DELAY: Duration = Duration::from_millis(850);

/// Width of the list of notes, as the C# list column.
const LIST_WIDTH: f32 = 280.0;

/// What separates the tags shown under a note's title, as the C# `DisplayTags`.
const TAG_GAP: &str = "   ";

/// What the notes tool is asked.
#[derive(Debug, Clone)]
pub enum NotesMessage {
    /// The search typed.
    Search(String),
    /// The note in this file selected.
    Select(PathBuf),
    /// A new note, as the C# New button.
    New,
    /// Something done in the editor.
    Edit(Action),
    /// Change `.0` was the last for [`SAVE_DELAY`]: time to save.
    SaveDue(u64),
    /// Ask to delete the note selected, as the C# Delete button.
    Delete,
    /// The delete confirmed.
    ConfirmDelete,
    /// The delete called off.
    CancelDelete,
    /// Copy the note as Markdown.
    CopyMarkdown,
    /// Copy the note as an HTML document.
    CopyHtml,
    /// Try again the save that kept another note from being shown or made.
    RetrySave,
    /// Drop the changes that could not be saved, then go on to what was asked.
    DiscardChanges,
    /// Stay on the note whose changes could not be saved.
    KeepEditing,
}

/// What an update asks of its tab.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Nothing more.
    Done,
    /// This text copied by the copy button `.0`.
    Copy(CopySlot, String),
    /// Change `.0` saved after [`SAVE_DELAY`] unless another follows.
    SaveLater(u64),
}

impl Outcome {
    /// What tab `tab` runs for it.
    pub fn task(self, tab: TabId) -> Task<Message> {
        match self {
            Self::Done | Self::Copy(..) => Task::none(),
            Self::SaveLater(change) => Task::perform(super::wait(SAVE_DELAY), move |()| {
                Message::Tool(tab, ToolMessage::Notes(NotesMessage::SaveDue(change)))
            }),
        }
    }
}

/// What the status line says.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Status {
    /// Nothing.
    None,
    /// The notes' folder could not be read: what the system said.
    ListFailed(String),
    /// The notes have no folder on this platform.
    NoFolder,
    /// A note could not be saved, made or deleted: what the system said.
    WriteFailed(String),
}

/// What waits for the changes of the note shown to be written: they could not be, and going
/// on would lose them.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Leave {
    /// Show the note in this file.
    Select(PathBuf),
    /// Make a new note.
    New,
}

/// The notes tool's state.
#[derive(Debug)]
pub struct NotesPane {
    /// The notes' folder; `None` where the platform has no data folder.
    dir: Option<PathBuf>,
    notes: Vec<Note>,
    search: String,
    /// The note shown: its file, never reused by another note.
    selected: Option<PathBuf>,
    editor: Content,
    /// The last change, numbered; a save waits for its own number to still be the last.
    changes: u64,
    /// Changes not yet on disk.
    dirty: bool,
    confirm_delete: bool,
    /// Where the user asked to go while the note shown could not be saved.
    blocked: Option<Leave>,
    status: Status,
}

impl Default for NotesPane {
    /// A new tab's state: the notes of the data folder.
    fn default() -> Self {
        Self::new(notes_storage::notes_dir())
    }
}

impl NotesPane {
    /// Its state over the notes of `dir`, listed now, the newest selected.
    #[must_use]
    pub fn new(dir: Option<PathBuf>) -> Self {
        let mut pane = Self {
            dir,
            notes: Vec::new(),
            search: String::new(),
            selected: None,
            editor: Content::new(),
            changes: 0,
            dirty: false,
            confirm_delete: false,
            blocked: None,
            status: Status::None,
        };
        pane.reload();
        if let Some(newest) = pane.notes.first().map(|note| note.path.clone()) {
            pane.show(&newest);
        }
        pane
    }

    /// The notes read again from their folder.
    fn reload(&mut self) {
        let Some(dir) = &self.dir else {
            self.status = Status::NoFolder;
            return;
        };
        match notes_storage::list_notes(dir) {
            Ok(notes) => self.notes = notes,
            Err(error) => self.status = Status::ListFailed(error.to_string()),
        }
    }

    /// Applies `message`; what is then to be done.
    pub fn update(&mut self, message: NotesMessage) -> Outcome {
        match message {
            NotesMessage::Search(typed) => self.search = typed,
            NotesMessage::Select(path) => {
                if self.selected.as_ref() != Some(&path) {
                    self.leave(Leave::Select(path));
                }
            }
            NotesMessage::New => self.leave(Leave::New),
            NotesMessage::RetrySave => {
                if self.flush()
                    && let Some(leave) = self.blocked.take()
                {
                    self.go(leave);
                }
            }
            NotesMessage::DiscardChanges => {
                self.discard();
                if let Some(leave) = self.blocked.take() {
                    self.go(leave);
                }
            }
            NotesMessage::KeepEditing => self.blocked = None,
            NotesMessage::Edit(action) => {
                let edit = action.is_edit();
                self.editor.perform(action);
                if edit && self.selected.is_some() {
                    self.dirty = true;
                    self.changes += 1;
                    return Outcome::SaveLater(self.changes);
                }
            }
            NotesMessage::SaveDue(change) => {
                // Failed, it is said, and the changes wait for the next save.
                if change == self.changes {
                    self.flush();
                }
            }
            NotesMessage::Delete => self.confirm_delete = self.selected.is_some(),
            NotesMessage::CancelDelete => self.confirm_delete = false,
            NotesMessage::ConfirmDelete => self.delete(),
            NotesMessage::CopyMarkdown => {
                return Outcome::Copy(CopySlot::NotesMarkdown, self.editor_text());
            }
            NotesMessage::CopyHtml => {
                if let Some(note) = self.current() {
                    return Outcome::Copy(CopySlot::NotesHtml, notes_storage::to_html(&note));
                }
            }
        }
        Outcome::Done
    }

    /// The tab closing: the note shown saved, as the C# `TrySaveSynchronously`.
    pub fn closing(&mut self) {
        self.flush();
    }

    /// The changes of the note shown written now, as the C# `CanClose` does before its tab
    /// closes (`NotesToolView.xaml.cs:107`); whether nothing is left unsaved.
    pub fn save_now(&mut self) -> bool {
        self.flush()
    }

    /// The title of the note shown when its changes are not on disk, which closing the tab
    /// would lose; `None` when nothing would be lost.
    #[must_use]
    pub fn unsaved(&self) -> Option<String> {
        if !self.dirty {
            return None;
        }
        self.current().map(|note| note.title())
    }

    /// The note shown left for `leave` once its changes are on disk; kept, its changes with
    /// it, while they cannot be written, until they are or the user discards them.
    fn leave(&mut self, leave: Leave) {
        if self.flush() {
            self.blocked = None;
            self.go(leave);
        } else {
            self.confirm_delete = false;
            self.blocked = Some(leave);
        }
    }

    /// `leave` done, nothing left unsaved.
    fn go(&mut self, leave: Leave) {
        match leave {
            Leave::Select(path) => self.show(&path),
            Leave::New => self.create(),
        }
    }

    /// The changes of the note shown dropped, as the user asked: the note as last saved.
    fn discard(&mut self) {
        // A save waiting for them would write them after all: it is let go.
        self.changes += 1;
        self.dirty = false;
        if let Some(content) = self.current_saved().map(|note| note.content.clone()) {
            self.editor = Content::with_text(&content);
        }
        if matches!(self.status, Status::WriteFailed(_)) {
            self.status = Status::None;
        }
    }

    /// The note shown as it was last read or saved.
    fn current_saved(&self) -> Option<&Note> {
        let path = self.selected.as_ref()?;
        self.notes.iter().find(|note| &note.path == path)
    }

    /// The note in `path` shown in the editor.
    fn show(&mut self, path: &Path) {
        self.confirm_delete = false;
        let Some(note) = self.notes.iter().find(|note| note.path == path) else {
            return;
        };
        self.editor = Content::with_text(&note.content);
        self.selected = Some(note.path.clone());
        self.dirty = false;
    }

    /// The editor's text, its line endings as they were read.
    fn editor_text(&self) -> String {
        self.editor.text()
    }

    /// The note shown as it is now in the editor.
    fn current(&self) -> Option<Note> {
        let path = self.selected.as_ref()?;
        let note = self.notes.iter().find(|note| &note.path == path)?;
        Some(Note {
            content: self.editor_text(),
            ..note.clone()
        })
    }

    /// The changes of the note shown written to its file, now; whether nothing is left
    /// unsaved. A failure is said, and the changes kept.
    fn flush(&mut self) -> bool {
        if !self.dirty {
            return true;
        }
        let Some(path) = self.selected.clone() else {
            return true;
        };
        let content = self.editor_text();
        match notes_storage::save_note(&path, &content) {
            Ok(modified) => {
                self.dirty = false;
                if let Some(note) = self.notes.iter_mut().find(|note| note.path == path) {
                    note.content = content;
                    note.modified = modified;
                }
                // Newest first again, as the C# list after a save.
                self.notes.sort_by(|a, b| {
                    b.modified
                        .cmp(&a.modified)
                        .then_with(|| a.path.cmp(&b.path))
                });
                if matches!(self.status, Status::WriteFailed(_)) {
                    self.status = Status::None;
                }
                true
            }
            Err(error) => {
                self.status = Status::WriteFailed(error.to_string());
                false
            }
        }
    }

    /// A new note made, listed first and shown, as the C# blank note; the note shown was
    /// saved before.
    fn create(&mut self) {
        let Some(dir) = self.dir.clone() else {
            self.status = Status::NoFolder;
            return;
        };
        let now = Instant::now();
        let clock = now.wall_clock(heimdall_app::time_zone::local_offset_at(now));
        match notes_storage::create_note(&dir, &fl!("ui-tool-notes-new-title"), &clock) {
            Ok(note) => {
                let path = note.path.clone();
                self.notes.insert(0, note);
                self.search.clear();
                self.show(&path);
            }
            Err(error) => self.status = Status::WriteFailed(error.to_string()),
        }
    }

    /// The note shown deleted, its pending save dropped, as the C# `DeleteCurrentNoteAsync`.
    fn delete(&mut self) {
        self.confirm_delete = false;
        self.blocked = None;
        let Some(path) = self.selected.take() else {
            return;
        };
        // A save waiting for this note would write it back: it is let go.
        self.changes += 1;
        self.dirty = false;
        self.editor = Content::new();
        match notes_storage::delete_note(&path) {
            Ok(()) => self.notes.retain(|note| note.path != path),
            Err(error) => {
                self.status = Status::WriteFailed(error.to_string());
                self.reload();
            }
        }
    }

    /// The notes the search keeps; the one shown as it is in the editor.
    fn shown_notes(&self) -> Vec<Note> {
        let current = self.current();
        self.notes
            .iter()
            .map(|note| match &current {
                Some(current) if current.path == note.path => current.clone(),
                _ => note.clone(),
            })
            .filter(|note| note.matches(&self.search))
            .collect()
    }

    /// What the status line says.
    fn status_text(&self) -> Option<(String, bool)> {
        match &self.status {
            Status::None => None,
            Status::NoFolder => Some((fl!("ui-tool-notes-error-no-folder"), true)),
            Status::ListFailed(error) => Some((
                fl!("ui-tool-notes-error-list", error = error.as_str()),
                true,
            )),
            Status::WriteFailed(error) => Some((
                fl!("ui-tool-notes-error-save", error = error.as_str()),
                true,
            )),
        }
    }

    /// The tool's page: the search, New and the notes on the left; the note selected, its
    /// actions and its editor on the right.
    pub fn view<'a>(&'a self, tab: TabId, state: &ToolPane) -> Element<'a, Message> {
        let send = move |message| Message::Tool(tab, ToolMessage::Notes(message));
        let shown = self.shown_notes();
        let count = shown.len();
        let items = shown.into_iter().map(|note| {
            let selected = self.selected.as_ref() == Some(&note.path);
            let tags = note.tags();
            let date = note
                .modified
                .map(super::log_viewer_tool::local_time)
                .and_then(|time| time.get(..DATE_MINUTE_LENGTH).map(str::to_owned))
                .unwrap_or_default();
            let mut item = column![
                text(note.title())
                    .size(font_size::BODY)
                    .font(styles::SEMIBOLD),
                text(date).size(font_size::CAPTION).style(text::secondary),
            ];
            if !tags.is_empty() {
                let tags: Vec<String> = tags.iter().map(|tag| format!("#{tag}")).collect();
                item = item.push(
                    text(tags.join(TAG_GAP))
                        .size(font_size::CAPTION)
                        .style(text::secondary),
                );
            }
            button(item.spacing(spacing::XS))
                .padding(spacing::SM)
                .width(Length::Fill)
                .style(if selected {
                    styles::primary
                } else {
                    styles::subtle
                })
                .on_press(send(NotesMessage::Select(note.path)))
                .into()
        });
        let list = column![
            row![
                text_input(&fl!("ui-tool-notes-search-placeholder"), &self.search)
                    .size(font_size::BODY)
                    .padding(super::INPUT_PADDING)
                    .style(styles::text_input)
                    .on_input(move |typed| send(NotesMessage::Search(typed))),
                super::action_button(
                    fl!("ui-tool-notes-new"),
                    true,
                    self.dir.is_some().then(|| send(NotesMessage::New))
                ),
            ]
            .spacing(spacing::SM)
            .align_y(iced::Alignment::Center),
            text(fl!("ui-tool-notes-count", count = count))
                .size(font_size::CAPTION)
                .style(text::secondary),
            scrollable(Column::with_children(items).spacing(spacing::XS)).height(Length::Fill),
        ]
        .spacing(spacing::SM)
        .width(LIST_WIDTH)
        .height(Length::Fill);
        let note: Element<'a, Message> = if self.selected.is_some() {
            let actions = if self.blocked.is_some() {
                row![
                    text(fl!("ui-tool-notes-unsaved-blocked"))
                        .size(font_size::BODY)
                        .style(text::danger),
                    super::action_button(
                        fl!("ui-tool-notes-retry"),
                        true,
                        Some(send(NotesMessage::RetrySave))
                    ),
                    super::action_button(
                        fl!("ui-tool-notes-discard"),
                        false,
                        Some(send(NotesMessage::DiscardChanges))
                    ),
                    super::action_button(
                        fl!("ui-tool-notes-cancel"),
                        false,
                        Some(send(NotesMessage::KeepEditing))
                    ),
                ]
            } else if self.confirm_delete {
                let title = self.current().map(|note| note.title()).unwrap_or_default();
                row![
                    text(fl!("ui-tool-notes-delete-confirm", title = title))
                        .size(font_size::BODY)
                        .style(text::danger),
                    super::action_button(
                        fl!("ui-tool-notes-delete"),
                        true,
                        Some(send(NotesMessage::ConfirmDelete))
                    ),
                    super::action_button(
                        fl!("ui-tool-notes-cancel"),
                        false,
                        Some(send(NotesMessage::CancelDelete))
                    ),
                ]
            } else {
                row![
                    super::copy_button(
                        fl!("ui-tool-notes-copy-markdown"),
                        state.copied(CopySlot::NotesMarkdown),
                        send(NotesMessage::CopyMarkdown),
                        super::COPY_PADDING,
                    ),
                    super::copy_button(
                        fl!("ui-tool-notes-copy-html"),
                        state.copied(CopySlot::NotesHtml),
                        send(NotesMessage::CopyHtml),
                        super::COPY_PADDING,
                    ),
                    iced::widget::space().width(Length::Fill),
                    super::action_button(
                        fl!("ui-tool-notes-delete"),
                        false,
                        Some(send(NotesMessage::Delete))
                    ),
                ]
            };
            column![
                actions
                    .spacing(spacing::SM)
                    .align_y(iced::Alignment::Center),
                super::text_box(&self.editor, Some(fl!("ui-tool-notes-content-placeholder")))
                    .height(Length::Fill)
                    .on_action(move |action| send(NotesMessage::Edit(action))),
            ]
            .spacing(spacing::SM)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
        } else {
            container(
                column![
                    text(fl!("ui-tool-notes-empty-title")).size(font_size::BODY_LARGE),
                    text(fl!("ui-tool-notes-empty-body"))
                        .size(font_size::BODY)
                        .style(text::secondary),
                ]
                .spacing(spacing::SM),
            )
            .center(Length::Fill)
            .into()
        };
        super::tool_body(
            column![row![list, note].spacing(spacing::LG).height(Length::Fill),]
                .push(super::status_line(self.status_text()))
                .spacing(spacing::SM),
        )
    }
}

/// The length of `yyyy-MM-dd HH:mm`, the C# list's date.
const DATE_MINUTE_LENGTH: usize = 16;

#[cfg(test)]
mod tests {
    use super::*;

    fn typed(pane: &mut NotesPane, typed: &str) -> Outcome {
        pane.update(NotesMessage::Edit(Action::Edit(
            iced::widget::text_editor::Edit::Paste(std::sync::Arc::new(typed.to_owned())),
        )))
    }

    #[test]
    fn notes_are_read_from_their_folder_newest_selected_and_none_invented() {
        let dir = tempfile::tempdir().expect("temp dir");
        let empty = NotesPane::new(Some(dir.path().to_owned()));
        assert!(empty.notes.is_empty(), "no demo note");
        assert_eq!(empty.selected, None);
        notes_storage::save_note(&dir.path().join("a.md"), "# First\n> tags: ops\n")
            .expect("saved");
        let pane = NotesPane::new(Some(dir.path().to_owned()));
        assert_eq!(pane.notes.len(), 1);
        assert_eq!(pane.selected, Some(dir.path().join("a.md")));
        assert_eq!(pane.editor_text(), "# First\n> tags: ops\n");
    }

    #[test]
    fn an_edit_is_saved_after_the_last_change_only() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("a.md");
        notes_storage::save_note(&path, "# A\n").expect("saved");
        let mut pane = NotesPane::new(Some(dir.path().to_owned()));
        let Outcome::SaveLater(first) = typed(&mut pane, "x") else {
            panic!("a save waited for");
        };
        let Outcome::SaveLater(second) = typed(&mut pane, "y") else {
            panic!("a save waited for");
        };
        pane.update(NotesMessage::SaveDue(first));
        assert_eq!(
            std::fs::read_to_string(&path).expect("read"),
            "# A\n",
            "not the last change"
        );
        pane.update(NotesMessage::SaveDue(second));
        assert_eq!(std::fs::read_to_string(&path).expect("read"), "xy# A\n");
        assert!(!pane.dirty);
    }

    #[test]
    fn a_new_note_is_made_on_disk_and_survives_a_delete_of_another() {
        let dir = tempfile::tempdir().expect("temp dir");
        let mut pane = NotesPane::new(Some(dir.path().to_owned()));
        pane.update(NotesMessage::New);
        let first = pane.selected.clone().expect("the new note shown");
        assert!(first.exists());
        pane.update(NotesMessage::New);
        let second = pane.selected.clone().expect("the new note shown");
        assert_ne!(first, second);
        let Outcome::SaveLater(change) = typed(&mut pane, "draft ") else {
            panic!("a save waited for");
        };
        pane.update(NotesMessage::Select(first.clone()));
        assert!(
            std::fs::read_to_string(&second)
                .expect("read")
                .starts_with("draft "),
            "saved on leaving it"
        );
        pane.update(NotesMessage::Delete);
        assert!(pane.confirm_delete);
        pane.update(NotesMessage::ConfirmDelete);
        assert!(!first.exists());
        pane.update(NotesMessage::SaveDue(change));
        assert!(!first.exists(), "a save waiting never brings it back");
        assert_eq!(pane.notes.len(), 1);
        assert_eq!(pane.notes[0].path, second);
    }

    #[test]
    fn the_tab_closing_saves_the_note_shown() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("a.md");
        notes_storage::save_note(&path, "# A\n").expect("saved");
        let mut pane = NotesPane::new(Some(dir.path().to_owned()));
        let _ = typed(&mut pane, "z");
        pane.closing();
        assert_eq!(std::fs::read_to_string(&path).expect("read"), "z# A\n");
    }

    #[test]
    fn the_html_copied_escapes_the_note_and_its_tags() {
        let dir = tempfile::tempdir().expect("temp dir");
        notes_storage::save_note(&dir.path().join("a.md"), "# <T>\n> tags: <x>\n").expect("saved");
        let mut pane = NotesPane::new(Some(dir.path().to_owned()));
        let Outcome::Copy(CopySlot::NotesHtml, html) = pane.update(NotesMessage::CopyHtml) else {
            panic!("the HTML copied");
        };
        assert!(html.contains("<h1>&lt;T&gt;</h1>"));
        assert!(html.contains("#&lt;x&gt;"));
    }

    /// The note in `path` made impossible to write: a folder stands where its file goes.
    fn unwritable(path: &Path) {
        std::fs::remove_file(path).expect("removed");
        std::fs::create_dir(path).expect("a folder in its place");
    }

    /// Two notes, `a.md` shown, typed into, its file then unwritable.
    fn failing_pane(dir: &Path) -> (NotesPane, PathBuf, PathBuf) {
        let (a, b) = (dir.join("a.md"), dir.join("b.md"));
        notes_storage::save_note(
            &a, "# A
",
        )
        .expect("saved");
        notes_storage::save_note(
            &b, "# B
",
        )
        .expect("saved");
        let mut pane = NotesPane::new(Some(dir.to_owned()));
        pane.update(NotesMessage::Select(a.clone()));
        assert_eq!(pane.selected, Some(a.clone()));
        let _ = typed(&mut pane, "mine ");
        unwritable(&a);
        (pane, a, b)
    }

    #[test]
    fn a_note_that_cannot_be_saved_is_kept_shown_when_another_is_selected() {
        let dir = tempfile::tempdir().expect("temp dir");
        let (mut pane, a, b) = failing_pane(dir.path());
        pane.update(NotesMessage::Select(b.clone()));
        assert_eq!(pane.selected, Some(a.clone()), "not left");
        assert_eq!(
            pane.editor_text(),
            "mine # A
",
            "its changes kept"
        );
        assert!(pane.dirty);
        assert!(matches!(pane.status, Status::WriteFailed(_)));
        assert_eq!(pane.blocked, Some(Leave::Select(b.clone())));
        // Retried while it still fails: still kept.
        pane.update(NotesMessage::RetrySave);
        assert_eq!(pane.selected, Some(a.clone()));
        // Writable again: saved, then the note asked for shown.
        std::fs::remove_dir(&a).expect("folder gone");
        pane.update(NotesMessage::RetrySave);
        assert_eq!(
            std::fs::read_to_string(&a).expect("read"),
            "mine # A
"
        );
        assert_eq!(pane.selected, Some(b));
        assert_eq!(
            pane.editor_text(),
            "# B
"
        );
        assert_eq!(pane.blocked, None);
        assert_eq!(pane.status, Status::None);
    }

    #[test]
    fn changes_that_cannot_be_saved_go_only_when_discarded() {
        let dir = tempfile::tempdir().expect("temp dir");
        let (mut pane, a, b) = failing_pane(dir.path());
        pane.update(NotesMessage::Select(b.clone()));
        pane.update(NotesMessage::KeepEditing);
        assert_eq!(pane.blocked, None);
        assert_eq!(pane.selected, Some(a.clone()), "still the note edited");
        assert_eq!(
            pane.editor_text(),
            "mine # A
"
        );
        let Outcome::SaveLater(change) = typed(&mut pane, "more ") else {
            panic!("a save waited for");
        };
        pane.update(NotesMessage::Select(b.clone()));
        pane.update(NotesMessage::DiscardChanges);
        assert_eq!(pane.selected, Some(b.clone()));
        assert!(!pane.dirty);
        assert_eq!(pane.status, Status::None);
        std::fs::remove_dir(&a).expect("folder gone");
        pane.update(NotesMessage::SaveDue(change));
        assert!(
            !a.exists(),
            "a save waiting never writes what was discarded"
        );
        assert_eq!(
            pane.editor_text(),
            "# B
"
        );
    }

    #[test]
    fn no_new_note_replaces_changes_that_cannot_be_saved() {
        let dir = tempfile::tempdir().expect("temp dir");
        let (mut pane, a, _) = failing_pane(dir.path());
        pane.update(NotesMessage::New);
        assert_eq!(pane.notes.len(), 2, "none made");
        assert_eq!(pane.selected, Some(a.clone()));
        assert_eq!(pane.blocked, Some(Leave::New));
        std::fs::remove_dir(&a).expect("folder gone");
        pane.update(NotesMessage::RetrySave);
        assert_eq!(pane.notes.len(), 3, "made once saved");
        assert_ne!(pane.selected, Some(a.clone()));
        assert_eq!(
            std::fs::read_to_string(&a).expect("read"),
            "mine # A
"
        );
    }

    #[test]
    fn a_note_that_cannot_be_saved_is_said_unsaved_to_its_tab_closing() {
        let dir = tempfile::tempdir().expect("temp dir");
        let (mut pane, a, _) = failing_pane(dir.path());
        assert_eq!(pane.unsaved(), Some("a".to_owned()));
        assert!(!pane.save_now(), "refused, as the C# CanClose");
        assert_eq!(pane.unsaved(), Some("a".to_owned()));
        std::fs::remove_dir(&a).expect("folder gone");
        assert!(pane.save_now());
        assert_eq!(pane.unsaved(), None);
    }

    #[test]
    fn a_save_due_after_another_note_is_shown_writes_nothing_into_it() {
        let dir = tempfile::tempdir().expect("temp dir");
        let (a, b) = (dir.path().join("a.md"), dir.path().join("b.md"));
        notes_storage::save_note(
            &a, "# A
",
        )
        .expect("saved");
        notes_storage::save_note(
            &b, "# B
",
        )
        .expect("saved");
        let mut pane = NotesPane::new(Some(dir.path().to_owned()));
        pane.update(NotesMessage::Select(a.clone()));
        let Outcome::SaveLater(change) = typed(&mut pane, "a ") else {
            panic!("a save waited for");
        };
        pane.update(NotesMessage::Select(b.clone()));
        assert_eq!(
            std::fs::read_to_string(&a).expect("read"),
            "a # A
"
        );
        pane.update(NotesMessage::SaveDue(change));
        assert_eq!(
            std::fs::read_to_string(&b).expect("read"),
            "# B
"
        );
        assert_eq!(
            std::fs::read_to_string(&a).expect("read"),
            "a # A
"
        );
    }

    #[test]
    fn without_a_data_folder_nothing_is_made() {
        let mut pane = NotesPane::new(None);
        assert_eq!(pane.status, Status::NoFolder);
        pane.update(NotesMessage::New);
        assert!(pane.notes.is_empty());
    }
}
