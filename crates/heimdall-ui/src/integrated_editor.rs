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

//! The integrated editor of a Files tab, as the C# one: the text of a server's file,
//! drawn here with its undo history, in place of the two lists. What it is and what it
//! says is the core's ([`heimdall_app::integrated_edit`]).

use std::collections::HashMap;
use std::sync::Arc;

use heimdall_app::files::FilesError;
use heimdall_app::integrated_edit::{EditorNotice, IntegratedEdit, Opened};
use heimdall_app::text_codec::TextEncoding;
use heimdall_app::{App, Effect, FilesMessage, Message as AppMessage};
use heimdall_app::{EditorId, TabId};
use iced::keyboard::{Key, key::Named};
use iced::widget::text_editor::{self, Action, Binding, Edit, KeyPress, LineEnding};
use iced::widget::{Space, button, column, container, row, text};
use iced::{Element, Font, Length, Task};

use crate::code_editor::{Content, code_editor};
use crate::i18n::fl;
use crate::shell::Message;

/// Size of the editor's notes and status.
const SMALL_SIZE: f32 = 12.0;

/// Space between parts.
const SPACING: f32 = 8.0;

/// What the integrated editor's keys and the window ask of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorKey {
    /// Save, only over the file opened.
    Save,
    /// Save over the server's file, which changed since it was opened.
    Overwrite,
    /// Close.
    Close,
    /// Take the last edit back.
    Undo,
    /// Do again the last edit taken back.
    Redo,
}

/// What reaches the integrated editors.
#[derive(Clone)]
pub enum EditorMessage {
    /// The file was read, or why not.
    Loaded {
        /// Tab.
        tab: TabId,
        /// The editor.
        id: EditorId,
        /// The text, how it is stored and the server's file as read.
        result: Result<Arc<Opened>, FilesError>,
    },
    /// The text editor did something: an edit, a move, a selection.
    Action {
        /// Tab.
        tab: TabId,
        /// The editor.
        id: EditorId,
        /// What it did.
        action: Action,
    },
    /// A key or a button of the editor.
    Key {
        /// Tab.
        tab: TabId,
        /// The editor.
        id: EditorId,
        /// What it asks.
        key: EditorKey,
    },
    /// The text was saved, or why not.
    Saved {
        /// Tab.
        tab: TabId,
        /// The editor.
        id: EditorId,
        /// The server's file as saved, or why not.
        result: Result<heimdall_files::Fingerprint, FilesError>,
    },
}

// The file's text is never written to a log.
impl std::fmt::Debug for EditorMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Loaded { tab, result, .. } => {
                write!(f, "Loaded({}, {})", tab.value(), result.is_ok())
            }
            Self::Action { tab, action, .. } => {
                write!(f, "Action({}, edit: {})", tab.value(), action.is_edit())
            }
            Self::Key { tab, key, .. } => write!(f, "Key({}, {key:?})", tab.value()),
            Self::Saved { tab, result, .. } => {
                write!(f, "Saved({}, {})", tab.value(), result.is_ok())
            }
        }
    }
}

/// A file's text, with its undo history.
pub struct EditorBuffer {
    content: Content,
    history: crate::editor_history::History,
    /// The version of the text a save under way started from.
    saving: Option<u64>,
    /// How the file is coloured, and the name of its language.
    syntax: crate::editor_syntax::Syntax,
}

/// The texts of the integrated editors open, by editor.
#[derive(Default)]
pub struct Editors {
    buffers: HashMap<EditorId, EditorBuffer>,
}

impl Editors {
    /// The text of the editor `id`, once read.
    #[must_use]
    pub fn get(&self, id: EditorId) -> Option<&EditorBuffer> {
        self.buffers.get(&id)
    }

    /// Lets go of the texts of editors no longer open.
    pub fn prune(&mut self, app: &App) {
        if self.buffers.is_empty() {
            return;
        }
        let open: Vec<EditorId> = app
            .tabs
            .iter()
            .filter_map(|tab| tab.files.as_deref()?.editor.as_ref().map(|edit| edit.id))
            .collect();
        self.buffers.retain(|id, _| open.contains(id));
    }

    /// Applies `message`; what the core then asks.
    pub fn update(&mut self, message: EditorMessage, app: &mut App) -> Vec<Effect> {
        match message {
            EditorMessage::Loaded { tab, id, result } => {
                let name = app
                    .tab(tab)
                    .and_then(|tab| tab.files.as_deref()?.editor.as_ref())
                    .filter(|edit| edit.id == id)
                    .map(|edit| edit.name.clone())
                    .unwrap_or_default();
                let result = result.map(|opened| {
                    self.buffers.insert(
                        id,
                        EditorBuffer {
                            content: Content::with_text(&opened.text),
                            history: crate::editor_history::History::default(),
                            saving: None,
                            syntax: crate::editor_syntax::of(&name),
                        },
                    );
                    (opened.encoding, opened.fingerprint)
                });
                app.update(files(FilesMessage::EditorOpened { tab, id, result }))
            }
            EditorMessage::Action { tab, id, action } => {
                // Under a question, the text is not the user's to change.
                if app.dialog.is_some() {
                    return Vec::new();
                }
                let Some(buffer) = self.buffers.get_mut(&id) else {
                    return Vec::new();
                };
                let edit = match action {
                    Action::Edit(edit) => edit,
                    // Plain selections, whose bounds the undo history can read.
                    Action::SelectWord => {
                        crate::editor_history::select_word(&mut buffer.content);
                        return Vec::new();
                    }
                    Action::SelectLine => {
                        crate::editor_history::select_line(&mut buffer.content);
                        return Vec::new();
                    }
                    action => {
                        buffer.content.perform(action);
                        return Vec::new();
                    }
                };
                let was = buffer.history.is_dirty();
                let edit = file_ending(&buffer.content, edit);
                buffer.history.edit(&mut buffer.content, edit);
                changed(app, tab, id, was, buffer.history.is_dirty())
            }
            EditorMessage::Key { tab, id, key } => {
                if app.dialog.is_some() {
                    return Vec::new();
                }
                self.key(app, tab, id, key)
            }
            EditorMessage::Saved { tab, id, result } => {
                let Some(buffer) = self.buffers.get_mut(&id) else {
                    return Vec::new();
                };
                let started = buffer.saving.take();
                if let (Ok(_), Some(version)) = (&result, started) {
                    buffer.history.mark_saved_if(version);
                }
                let dirty = buffer.history.is_dirty();
                app.update(files(FilesMessage::EditorSaved {
                    tab,
                    id,
                    result,
                    dirty,
                }))
            }
        }
    }

    fn key(&mut self, app: &mut App, tab: TabId, id: EditorId, key: EditorKey) -> Vec<Effect> {
        let Some(buffer) = self.buffers.get_mut(&id) else {
            // Not read yet: only closing means something.
            return if key == EditorKey::Close {
                app.update(files(FilesMessage::EditorClose { tab, id }))
            } else {
                Vec::new()
            };
        };
        match key {
            EditorKey::Save | EditorKey::Overwrite => {
                // A save under way keeps the version it started from; the core says it
                // runs. The text is the real one all the same: never an empty file.
                if buffer.saving.is_some() {
                    return app.update(files(FilesMessage::EditorSave {
                        tab,
                        id,
                        text: buffer.content.text(),
                        overwrite: key == EditorKey::Overwrite,
                    }));
                }
                buffer.saving = Some(buffer.history.version());
                let effects = app.update(files(FilesMessage::EditorSave {
                    tab,
                    id,
                    text: buffer.content.text(),
                    overwrite: key == EditorKey::Overwrite,
                }));
                if !effects
                    .iter()
                    .any(|effect| matches!(effect, Effect::SaveEditor { .. }))
                {
                    buffer.saving = None;
                }
                effects
            }
            EditorKey::Close => app.update(files(FilesMessage::EditorClose { tab, id })),
            EditorKey::Undo | EditorKey::Redo => {
                let was = buffer.history.is_dirty();
                if key == EditorKey::Undo {
                    buffer.history.undo(&mut buffer.content);
                } else {
                    buffer.history.redo(&mut buffer.content);
                }
                changed(app, tab, id, was, buffer.history.is_dirty())
            }
        }
    }
}

fn files(message: FilesMessage) -> AppMessage {
    AppMessage::Files(message)
}

/// Tells the core when the text starts or stops differing from the text saved.
fn changed(app: &mut App, tab: TabId, id: EditorId, was: bool, dirty: bool) -> Vec<Effect> {
    if was == dirty {
        return Vec::new();
    }
    app.update(files(FilesMessage::EditorChanged { tab, id, dirty }))
}

/// Enter as the file ends its lines, not always LF.
fn file_ending(content: &Content, edit: Edit) -> Edit {
    let ending = document_ending(content);
    match edit {
        Edit::Enter if ending != LineEnding::Lf => {
            Edit::Paste(Arc::new(ending.as_str().to_owned()))
        }
        // Text pasted keeps the file's line endings, whatever the clipboard had.
        Edit::Paste(text) if text.contains(['\r', '\n']) => {
            let unified = text.replace("\r\n", "\n").replace('\r', "\n");
            Edit::Paste(Arc::new(if ending == LineEnding::Lf {
                unified
            } else {
                unified.replace('\n', ending.as_str())
            }))
        }
        edit => edit,
    }
}

/// How the file ends its lines where the cursor is: the cursor's line, else the first's,
/// else LF.
fn document_ending(content: &Content) -> LineEnding {
    let line = content.cursor().position.line;
    [content.line(line), content.line(0)]
        .into_iter()
        .flatten()
        .map(|line| line.ending)
        .find(|ending| *ending != LineEnding::None)
        .unwrap_or(LineEnding::Lf)
}

/// Runs the integrated editor's effects: the file read, the text saved.
pub fn task(effect: Effect) -> Task<Message> {
    match effect {
        Effect::OpenEditor {
            tab,
            id,
            client,
            remote,
        } => Task::perform(
            heimdall_app::integrated_edit::open(client, remote),
            move |result| {
                Message::Editor(EditorMessage::Loaded {
                    tab,
                    id,
                    result: result.map(Arc::new),
                })
            },
        ),
        Effect::SaveEditor {
            tab,
            id,
            client,
            remote,
            bytes,
            expected,
        } => Task::perform(
            heimdall_app::integrated_edit::save(client, remote, bytes, expected),
            move |result| Message::Editor(EditorMessage::Saved { tab, id, result }),
        ),
        _ => Task::none(),
    }
}

/// What the editor's own keys ask, as the C# one's: Ctrl+S saves, Ctrl+W closes, Ctrl+Z
/// and Ctrl+Y (or Ctrl+Shift+Z) undo and redo.
fn editor_key(press: &KeyPress) -> Option<EditorKey> {
    let modifiers = press.modifiers;
    if !modifiers.command() || modifiers.alt() {
        return None;
    }
    let Key::Character(letter) = press.key.as_ref() else {
        return None;
    };
    match (letter.to_lowercase().as_str(), modifiers.shift()) {
        ("s", false) => Some(EditorKey::Save),
        ("w", false) => Some(EditorKey::Close),
        ("z", false) => Some(EditorKey::Undo),
        ("z", true) | ("y", false) => Some(EditorKey::Redo),
        _ => None,
    }
}

/// The editor's keys, only while it has the keyboard: its own, Tab typing a tab, and the
/// rest as any text field. `ask` makes the message of one of its own.
fn binding<M: Clone>(press: KeyPress, ask: impl Fn(EditorKey) -> M) -> Option<Binding<M>> {
    if !matches!(press.status, text_editor::Status::Focused { .. }) {
        return None;
    }
    if let Some(key) = editor_key(&press) {
        return Some(Binding::Custom(ask(key)));
    }
    if press.key == Key::Named(Named::Tab) && press.modifiers.is_empty() {
        return Some(Binding::Insert('\t'));
    }
    Binding::from_key_press(press)
}

/// The editor of a Files tab, in place of its lists: its file's name, Save and Close, what
/// it says, the text, and where the cursor is. `connected` while the session is up; the
/// code coloured with `syntax`, the window theme's.
pub fn view<'a>(
    tab: TabId,
    edit: &'a IntegratedEdit,
    buffer: Option<&'a EditorBuffer>,
    connected: bool,
    syntax: iced::highlighter::Theme,
) -> Element<'a, Message> {
    let id = edit.id;
    let key = move |key| Message::Editor(EditorMessage::Key { tab, id, key });
    let title = if edit.dirty {
        format!("{} *", edit.name)
    } else {
        edit.name.clone()
    };
    let can_save = connected && buffer.is_some() && !edit.saving;
    let header = row![
        text(title),
        Space::new().width(Length::Fill),
        button(text(fl!("ui-editor-save")).size(SMALL_SIZE))
            .on_press_maybe(can_save.then(|| key(EditorKey::Save))),
        button(text(fl!("ui-editor-close")).size(SMALL_SIZE))
            .style(button::secondary)
            .on_press(key(EditorKey::Close)),
    ]
    .spacing(SPACING)
    .align_y(iced::Alignment::Center);
    let mut page = column![header].spacing(SPACING);
    if let Some(note) = notice(edit, connected) {
        let mut line = row![text(note).size(SMALL_SIZE).style(text::warning)]
            .spacing(SPACING)
            .align_y(iced::Alignment::Center);
        if edit.notice == Some(EditorNotice::ChangedOnServer) && can_save {
            line = line.push(
                button(text(fl!("ui-editor-overwrite")).size(SMALL_SIZE))
                    .style(button::danger)
                    .on_press(key(EditorKey::Overwrite)),
            );
        }
        page = page.push(line);
    }
    let Some(buffer) = buffer else {
        return page
            .push(
                container(text(fl!("ui-editor-opening", name = edit.name.as_str())))
                    .center(Length::Fill),
            )
            .into();
    };
    // Numbered lines, never wrapped, as the C# editor's.
    let body = code_editor(&buffer.content)
        .on_action(move |action| Message::Editor(EditorMessage::Action { tab, id, action }))
        .key_binding(move |press| {
            binding(press, |key| {
                Message::Editor(EditorMessage::Key { tab, id, key })
            })
        })
        .font(Font::MONOSPACE)
        .highlight(&buffer.syntax.token, syntax);
    page.push(body)
        .push(status(edit, buffer))
        .padding(SPACING)
        .into()
}

/// What the editor says: its notice, or that the session ended.
fn notice(edit: &IntegratedEdit, connected: bool) -> Option<String> {
    if !connected {
        return Some(fl!("ui-editor-notice-session-ended"));
    }
    Some(match edit.notice.as_ref()? {
        EditorNotice::Latin1 => fl!("ui-editor-notice-latin1"),
        EditorNotice::Saved => fl!("ui-editor-notice-saved"),
        EditorNotice::ChangedOnServer => fl!("ui-editor-notice-changed"),
        EditorNotice::Unencodable(at) => fl!(
            "ui-editor-notice-unencodable",
            line = at.line,
            column = at.column
        ),
        EditorNotice::SaveRunning => fl!("ui-editor-notice-save-running"),
        EditorNotice::SessionEnded => fl!("ui-editor-notice-session-ended"),
        EditorNotice::Failed(error) => fl!(
            "ui-editor-notice-failed",
            reason = crate::texts::files_error(error)
        ),
    })
}

/// Where the cursor is, in characters, how many lines, the language the text is coloured
/// as, how the file is stored and how its lines end.
fn status<'a>(edit: &IntegratedEdit, buffer: &EditorBuffer) -> Element<'a, Message> {
    let cursor = buffer.content.cursor().position;
    let column = buffer
        .content
        .line(cursor.line)
        .and_then(|line| {
            line.text
                .get(..cursor.column)
                .map(|before| before.chars().count())
        })
        .unwrap_or(cursor.column);
    let encoding = edit.opened.map(|(encoding, _)| encoding_label(encoding));
    let ending = match buffer.content.line_ending() {
        Some(LineEnding::CrLf) => fl!("ui-editor-ending-crlf"),
        Some(LineEnding::Cr) => fl!("ui-editor-ending-cr"),
        _ => fl!("ui-editor-ending-lf"),
    };
    let mut parts = row![
        text(fl!(
            "ui-editor-position",
            line = (cursor.line + 1),
            column = (column + 1)
        ))
        .size(SMALL_SIZE),
        text(fl!("ui-editor-lines", count = buffer.content.line_count())).size(SMALL_SIZE),
        text(
            buffer
                .syntax
                .name
                .clone()
                .unwrap_or_else(|| fl!("ui-editor-plain-text"))
        )
        .size(SMALL_SIZE),
    ]
    .spacing(SPACING * 2.0);
    if let Some(encoding) = encoding {
        parts = parts.push(text(encoding).size(SMALL_SIZE));
    }
    parts.push(text(ending).size(SMALL_SIZE)).into()
}

fn encoding_label(encoding: TextEncoding) -> String {
    match encoding {
        TextEncoding::Utf8 { bom: false } => fl!("ui-editor-encoding-utf8"),
        TextEncoding::Utf8 { bom: true } => fl!("ui-editor-encoding-utf8-bom"),
        TextEncoding::Utf16Le => fl!("ui-editor-encoding-utf16le"),
        TextEncoding::Utf16Be => fl!("ui-editor-encoding-utf16be"),
        TextEncoding::Utf32Le => fl!("ui-editor-encoding-utf32le"),
        TextEncoding::Utf32Be => fl!("ui-editor-encoding-utf32be"),
        TextEncoding::Latin1 => fl!("ui-editor-encoding-latin1"),
    }
}

#[cfg(test)]
mod tests {
    use iced::keyboard::key::{Code, Physical};
    use iced::keyboard::{Key, Modifiers};
    use iced::widget::text_editor::{Edit, KeyPress, Status};

    use crate::code_editor::Content;

    use iced::widget::text_editor::Binding;

    use super::{EditorKey, binding, editor_key, file_ending};

    fn press(letter: &str, modifiers: Modifiers) -> KeyPress {
        let key = Key::Character(letter.into());
        KeyPress {
            key: key.clone(),
            modified_key: key,
            physical_key: Physical::Code(Code::KeyA),
            modifiers,
            text: None,
            status: Status::Focused { is_hovered: false },
        }
    }

    #[test]
    fn the_editors_keys_are_the_csharps() {
        let ctrl = Modifiers::CTRL;
        assert_eq!(editor_key(&press("s", ctrl)), Some(EditorKey::Save));
        assert_eq!(editor_key(&press("w", ctrl)), Some(EditorKey::Close));
        assert_eq!(editor_key(&press("z", ctrl)), Some(EditorKey::Undo));
        assert_eq!(editor_key(&press("y", ctrl)), Some(EditorKey::Redo));
        assert_eq!(
            editor_key(&press("Z", ctrl | Modifiers::SHIFT)),
            Some(EditorKey::Redo)
        );
        assert_eq!(editor_key(&press("s", Modifiers::empty())), None, "typed");
        assert_eq!(
            editor_key(&press("s", ctrl | Modifiers::ALT)),
            None,
            "AltGr"
        );
    }

    /// How long the editor takes to take a file's text and to give it back, for the
    /// size cap: run by hand, `cargo test --release -- --ignored editor_timing`.
    #[test]
    #[ignore = "a measure, not a check"]
    fn editor_timing() {
        let line = "server_name example.lab www.example.lab; # a line of a server's file
";
        for mib in [1, 4, 16] {
            let text = line.repeat(mib * 1024 * 1024 / line.len());
            let started = std::time::Instant::now();
            let content: Content = Content::with_text(&text);
            let built = started.elapsed();
            let started = std::time::Instant::now();
            let back = content.text();
            println!(
                "{mib} MiB, {} lines: with_text {built:?}, text {:?}",
                content.line_count(),
                started.elapsed()
            );
            assert_eq!(back.len(), text.len());
        }
        let long = "x".repeat(2 * 1024 * 1024);
        let started = std::time::Instant::now();
        let _: Content = Content::with_text(&long);
        println!("one line of 2 MiB: with_text {:?}", started.elapsed());
    }

    #[test]
    fn the_editors_keys_act_only_while_it_has_the_keyboard() {
        let mut tab = press("s", Modifiers::CTRL);
        assert!(matches!(
            binding(tab.clone(), |key| key),
            Some(Binding::Custom(EditorKey::Save))
        ));
        tab.status = Status::Active;
        assert!(
            binding(tab, |key| key).is_none(),
            "under a dialog, in another field"
        );
        let mut typed = press("\t", Modifiers::empty());
        typed.key = Key::Named(iced::keyboard::key::Named::Tab);
        assert!(matches!(
            binding(typed.clone(), |key| key),
            Some(Binding::Insert('\t'))
        ));
        typed.status = Status::Hovered;
        assert!(
            binding(typed, |key| key).is_none(),
            "no tab typed into a hidden text"
        );
    }

    #[test]
    fn text_pasted_ends_its_lines_as_the_file_does() {
        let crlf: Content = Content::with_text("a\r\nb\r\n");
        let pasted = Edit::Paste(std::sync::Arc::new("x\ny\r\nz\r".to_owned()));
        assert!(
            matches!(file_ending(&crlf, pasted.clone()), Edit::Paste(text) if text.as_str() == "x\r\ny\r\nz\r\n")
        );
        let lf: Content = Content::with_text("a\nb\n");
        assert!(
            matches!(file_ending(&lf, pasted), Edit::Paste(text) if text.as_str() == "x\ny\nz\n")
        );
    }

    #[test]
    fn enter_ends_a_line_as_the_file_does() {
        let crlf: Content = Content::with_text("a\r\nb\r\n");
        assert!(
            matches!(file_ending(&crlf, Edit::Enter), Edit::Paste(text) if text.as_str() == "\r\n")
        );
        let lf: Content = Content::with_text("a\nb\n");
        assert!(matches!(file_ending(&lf, Edit::Enter), Edit::Enter));
    }
}
