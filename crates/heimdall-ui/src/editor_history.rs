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

//! Undo and redo for iced's text editor, which has neither.
//!
//! Each edit is recorded as a replacement: where it starts, what it removed, what it
//! inserted, and where what it inserted ends. Undoing selects what was inserted and puts
//! back what was removed; redoing does the reverse. A deletion without a selection first
//! selects what it deletes, so what it removes is always known, line endings included.

use std::sync::Arc;

use iced::widget::text_editor::{Action, Content, Cursor, Edit, Motion, Position};

/// Most edits kept for undo; the oldest go first.
const MAX_CHANGES: usize = 1_000;

/// One edit, as a replacement.
#[derive(Debug, Clone, PartialEq)]
struct Change {
    /// Where it starts.
    start: Position,
    /// What it removed there.
    removed: String,
    /// What it inserted there.
    inserted: String,
    /// Where what it inserted ends.
    end: Position,
    /// Typed: the next letter typed right after it joins it, so undo takes words back.
    typing: bool,
}

/// The edits of one document, to undo and redo, and whether it differs from the text last
/// saved.
#[derive(Debug, Clone, PartialEq)]
pub struct History {
    undo: Vec<Change>,
    redo: Vec<Change>,
    /// How many edits to undo to reach the text saved; `None` once it cannot be reached.
    saved: Option<usize>,
    /// Counts every change to the text, edits, undos and redos alike.
    version: u64,
}

impl Default for History {
    fn default() -> Self {
        Self {
            undo: Vec::new(),
            redo: Vec::new(),
            saved: Some(0),
            version: 0,
        }
    }
}

impl History {
    /// Does `edit` in `content`, recorded to be undone.
    pub fn edit<R: iced::advanced::text::Renderer>(
        &mut self,
        content: &mut Content<R>,
        edit: Edit,
    ) {
        let inserted = match &edit {
            Edit::Insert(character) => character.to_string(),
            Edit::Paste(text) => text.to_string(),
            Edit::Enter => "\n".to_owned(),
            Edit::Backspace | Edit::Delete => String::new(),
            // Never asked: a tab is typed as a character.
            Edit::Indent | Edit::Unindent => return,
        };
        if content.cursor().selection.is_none() {
            match edit {
                Edit::Backspace => content.perform(Action::Select(Motion::Left)),
                Edit::Delete => content.perform(Action::Select(Motion::Right)),
                _ => {}
            }
        }
        let cursor = content.cursor();
        let start = cursor
            .selection
            .map_or(cursor.position, |anchor| earliest(anchor, cursor.position));
        // Read from the lines, not `selection`, which gives every line ending as LF.
        let removed = cursor.selection.map_or_else(String::new, |anchor| {
            span(content, start, latest(anchor, cursor.position))
        });
        if removed.is_empty() && inserted.is_empty() {
            // A deletion at the edge: nothing changes.
            content.move_to(Cursor {
                position: cursor.position,
                selection: None,
            });
            return;
        }
        content.perform(Action::Edit(edit));
        let typing = matches!(inserted.as_str(), letter if letter.chars().count() == 1
            && !letter.contains(['\n', '\r'])
            && removed.is_empty());
        self.record(Change {
            start,
            removed,
            inserted,
            end: content.cursor().position,
            typing,
        });
    }

    /// Takes the last edit back; whether there was one.
    pub fn undo<R: iced::advanced::text::Renderer>(&mut self, content: &mut Content<R>) -> bool {
        let Some(change) = self.undo.pop() else {
            return false;
        };
        let end = replace(content, change.start, change.end, &change.removed);
        self.version += 1;
        self.redo.push(Change {
            start: change.start,
            removed: change.inserted,
            inserted: change.removed,
            end,
            typing: false,
        });
        true
    }

    /// Does again the last edit taken back; whether there was one.
    pub fn redo<R: iced::advanced::text::Renderer>(&mut self, content: &mut Content<R>) -> bool {
        let Some(change) = self.redo.pop() else {
            return false;
        };
        let end = replace(content, change.start, change.end, &change.removed);
        self.version += 1;
        self.undo.push(Change {
            start: change.start,
            removed: change.inserted,
            inserted: change.removed,
            end,
            typing: false,
        });
        true
    }

    /// The text now is the text saved.
    pub fn mark_saved(&mut self) {
        self.saved = Some(self.undo.len());
    }

    /// The text of `version` was saved: the text saved now, unless it changed since; then
    /// the text saved is one no undo reaches.
    pub fn mark_saved_if(&mut self, version: u64) {
        if version == self.version {
            self.mark_saved();
        } else {
            self.saved = None;
        }
    }

    /// Changes with each edit, undo and redo: what a save started from.
    #[must_use]
    pub fn version(&self) -> u64 {
        self.version
    }

    /// Whether the text differs from the text last saved.
    #[must_use]
    pub fn is_dirty(&self) -> bool {
        self.saved != Some(self.undo.len())
    }

    /// Whether an edit can be taken back.
    #[must_use]
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    /// Whether an edit taken back can be done again.
    #[must_use]
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    fn record(&mut self, change: Change) {
        self.version += 1;
        // Redo goes with a new edit: the text saved may be among what it held.
        if !self.redo.is_empty() {
            self.redo.clear();
            if self.saved.is_some_and(|saved| saved > self.undo.len()) {
                self.saved = None;
            }
        }
        let at_saved = self.saved == Some(self.undo.len());
        if change.typing
            && !at_saved
            && let Some(last) = self.undo.last_mut()
            && last.typing
            && last.end == change.start
            // A space after a word starts the next word's change.
            && (change.inserted != " " || last.inserted.ends_with(' '))
        {
            last.inserted.push_str(&change.inserted);
            last.end = change.end;
            return;
        }
        self.undo.push(change);
        if self.undo.len() > MAX_CHANGES {
            self.undo.remove(0);
            self.saved = self.saved.and_then(|saved| saved.checked_sub(1));
        }
    }
}

/// Replaces what lies from `start` to `end` with `text`: where `text` ends then.
fn replace<R: iced::advanced::text::Renderer>(
    content: &mut Content<R>,
    start: Position,
    end: Position,
    text: &str,
) -> Position {
    content.move_to(Cursor {
        position: end,
        selection: (start != end).then_some(start),
    });
    if text.is_empty() {
        if start != end {
            content.perform(Action::Edit(Edit::Delete));
        }
    } else {
        content.perform(Action::Edit(Edit::Paste(Arc::new(text.to_owned()))));
    }
    content.cursor().position
}

/// The text from `start` to `end`, each line ending as the document has it.
fn span<R: iced::advanced::text::Renderer>(
    content: &Content<R>,
    start: Position,
    end: Position,
) -> String {
    let mut text = String::new();
    for index in start.line..=end.line {
        let Some(line) = content.line(index) else {
            break;
        };
        let from = if index == start.line { start.column } else { 0 };
        let to = if index == end.line {
            end.column
        } else {
            line.text.len()
        };
        text.push_str(line.text.get(from..to).unwrap_or_default());
        if index != end.line {
            text.push_str(line.ending.as_str());
        }
    }
    text
}

/// The later of two positions.
fn latest(first: Position, second: Position) -> Position {
    if earliest(first, second) == first {
        second
    } else {
        first
    }
}

/// The earlier of two positions.
fn earliest(first: Position, second: Position) -> Position {
    if (first.line, first.column) <= (second.line, second.column) {
        first
    } else {
        second
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use iced::widget::text_editor::{Action, Content, Cursor, Edit, Motion, Position};

    use super::History;

    type Text = Content<iced::Renderer>;

    fn at(content: &mut Text, line: usize, column: usize) {
        content.move_to(Cursor {
            position: Position { line, column },
            selection: None,
        });
    }

    fn typed(history: &mut History, content: &mut Text, text: &str) {
        for character in text.chars() {
            history.edit(content, Edit::Insert(character));
        }
    }

    #[test]
    fn the_text_given_is_the_text_read_back_line_endings_included() {
        for text in [
            "",
            "one",
            "one\n",
            "a\r\nb\r\n",
            "a\rb",
            "a\n\rb",
            "mixed\r\nendings\nhere\r",
            "no ending at the end\r\nlast",
        ] {
            assert_eq!(Text::with_text(text).text(), text, "{text:?}");
        }
    }

    #[test]
    fn typing_is_taken_back_word_by_word_and_done_again() {
        let mut content = Text::with_text("hello\r\n");
        let mut history = History::default();
        at(&mut content, 0, 5);
        typed(&mut history, &mut content, " big world");
        assert_eq!(content.text(), "hello big world\r\n");
        assert!(history.is_dirty());

        assert!(history.undo(&mut content));
        assert_eq!(content.text(), "hello big\r\n", "the last word");
        assert!(history.undo(&mut content));
        assert_eq!(content.text(), "hello\r\n");
        assert!(!history.is_dirty(), "back to the text saved");
        assert!(!history.undo(&mut content));

        assert!(history.redo(&mut content));
        assert!(history.redo(&mut content));
        assert_eq!(content.text(), "hello big world\r\n");
        assert!(!history.redo(&mut content));
    }

    #[test]
    fn a_line_ending_deleted_comes_back_as_it_was() {
        let mut content = Text::with_text("one\r\ntwo\r\n");
        let mut history = History::default();
        at(&mut content, 1, 0);
        history.edit(&mut content, Edit::Backspace);
        assert_eq!(content.text(), "onetwo\r\n");
        assert!(history.undo(&mut content));
        assert_eq!(content.text(), "one\r\ntwo\r\n", "CRLF, not LF");

        at(&mut content, 0, 3);
        history.edit(&mut content, Edit::Delete);
        assert_eq!(content.text(), "onetwo\r\n");
        assert!(history.undo(&mut content));
        assert_eq!(content.text(), "one\r\ntwo\r\n");
    }

    #[test]
    fn a_selection_replaced_by_a_paste_comes_back() {
        let mut content = Text::with_text("alpha\r\nbeta\r\ngamma\r\n");
        let mut history = History::default();
        content.move_to(Cursor {
            position: Position { line: 2, column: 2 },
            selection: Some(Position { line: 0, column: 2 }),
        });
        history.edit(&mut content, Edit::Paste(Arc::new("X\r\nY".to_owned())));
        assert_eq!(content.text(), "alX\r\nYmma\r\n");
        assert!(history.undo(&mut content));
        assert_eq!(content.text(), "alpha\r\nbeta\r\ngamma\r\n");
        assert!(history.redo(&mut content));
        assert_eq!(content.text(), "alX\r\nYmma\r\n");
    }

    #[test]
    fn letters_of_more_than_one_byte_come_back_whole() {
        let mut content = Text::with_text(
            "héllo wörld
next",
        );
        let mut history = History::default();
        content.move_to(Cursor {
            position: Position { line: 1, column: 2 },
            selection: Some(Position { line: 0, column: 3 }),
        });
        history.edit(&mut content, Edit::Delete);
        assert_eq!(content.text(), "héxt");
        assert!(history.undo(&mut content));
        assert_eq!(
            content.text(),
            "héllo wörld
next"
        );
        at(&mut content, 0, "héllo wö".len());
        history.edit(&mut content, Edit::Backspace);
        assert_eq!(
            content.text(),
            "héllo wrld
next"
        );
        assert!(history.undo(&mut content));
        assert_eq!(
            content.text(),
            "héllo wörld
next"
        );
    }

    #[test]
    fn a_deletion_at_the_edge_records_nothing() {
        let mut content = Text::with_text("x");
        let mut history = History::default();
        at(&mut content, 0, 0);
        history.edit(&mut content, Edit::Backspace);
        assert!(!history.can_undo() && !history.is_dirty());
        at(&mut content, 0, 1);
        history.edit(&mut content, Edit::Delete);
        assert!(!history.can_undo());
        assert_eq!(content.text(), "x");
    }

    #[test]
    fn the_text_saved_stays_known_through_undo_and_is_lost_by_a_new_edit_after_it() {
        let mut content = Text::with_text("a");
        let mut history = History::default();
        at(&mut content, 0, 1);
        history.edit(&mut content, Edit::Enter);
        history.mark_saved();
        assert!(!history.is_dirty());
        history.edit(&mut content, Edit::Insert('b'));
        assert!(history.is_dirty());
        assert!(history.undo(&mut content));
        assert!(!history.is_dirty(), "undone back to the save");
        assert!(history.undo(&mut content));
        assert!(history.is_dirty(), "before the save");
        // A new edit here: the text saved can no longer be reached.
        history.edit(&mut content, Edit::Insert('c'));
        assert!(history.is_dirty());
        assert!(!history.can_redo());
        while history.undo(&mut content) {}
        assert!(history.is_dirty());
    }

    #[test]
    fn a_letter_typed_at_the_save_starts_a_new_change() {
        let mut content = Text::with_text("");
        let mut history = History::default();
        typed(&mut history, &mut content, "ab");
        history.mark_saved();
        typed(&mut history, &mut content, "c");
        assert!(history.undo(&mut content));
        assert_eq!(content.text(), "ab");
        assert!(!history.is_dirty());
    }

    #[test]
    fn a_save_marks_the_text_saved_only_when_nothing_changed_meanwhile() {
        let mut content = Text::with_text("");
        let mut history = History::default();
        typed(&mut history, &mut content, "a");
        let started = history.version();
        history.mark_saved_if(started);
        assert!(!history.is_dirty());

        typed(&mut history, &mut content, "b");
        let started = history.version();
        typed(&mut history, &mut content, "c");
        history.mark_saved_if(started);
        assert!(history.is_dirty(), "edited while it saved");
        while history.undo(&mut content) {}
        assert!(
            history.is_dirty(),
            "what the server has is out of undo's reach"
        );
    }

    #[test]
    fn moving_the_cursor_is_not_an_edit() {
        let mut content = Text::with_text("ab");
        let mut history = History::default();
        content.perform(Action::Move(Motion::DocumentEnd));
        assert!(!history.can_undo());
        history.edit(&mut content, Edit::Insert('c'));
        content.perform(Action::Move(Motion::DocumentStart));
        history.edit(&mut content, Edit::Insert('z'));
        assert_eq!(content.text(), "zabc");
        assert!(history.undo(&mut content));
        assert_eq!(content.text(), "abc", "not joined: typed elsewhere");
    }
}
