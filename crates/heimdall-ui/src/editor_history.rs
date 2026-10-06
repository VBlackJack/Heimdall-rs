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

//! Undo and redo for the integrated editor's text widget, which, as iced's text editor it
//! derives from, has neither.
//!
//! Each edit is recorded as a replacement: where it starts, what it removed, what it
//! inserted, and where what it inserted ends. Undoing selects what was inserted and puts
//! back what was removed; redoing does the reverse.
//!
//! What an edit removes must be known exactly, so every selection is a plain one whose
//! bounds the cursor reports: a deletion without a selection first selects the character
//! it deletes, in the order of the text, and a word or a line is selected here
//! ([`select_word`], [`select_line`]) instead of by iced, whose own word and line
//! selections the cursor does not report.

use std::sync::Arc;

use iced::widget::text_editor::{Action, Cursor, Edit, Position};

use crate::code_editor::Content;

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
        if content.cursor().selection.is_none() && matches!(edit, Edit::Backspace | Edit::Delete) {
            // A deletion at the edge: nothing changes.
            let Some((start, end)) = deleted(content, matches!(edit, Edit::Delete)) else {
                return;
            };
            content.move_to(Cursor {
                position: end,
                selection: Some(start),
            });
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
    // Always a selection, empty or not: one the user made is never what gets replaced.
    content.move_to(Cursor {
        position: end,
        selection: Some(start),
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

/// What Backspace (or Delete, `forward`) removes from the cursor: the character before
/// it (after it) in the order of the text, whatever the direction it is shown in, or the
/// line ending it stands at. `None` at the start (end) of the text.
fn deleted<R: iced::advanced::text::Renderer>(
    content: &Content<R>,
    forward: bool,
) -> Option<(Position, Position)> {
    let at = content.cursor().position;
    let text = content.line(at.line)?.text.into_owned();
    let column = at.column.min(text.len());
    if forward {
        if let Some(next) = text.get(column..)?.chars().next() {
            let end = Position {
                line: at.line,
                column: column + next.len_utf8(),
            };
            return Some((at, end));
        }
        content.line(at.line + 1)?;
        return Some((
            at,
            Position {
                line: at.line + 1,
                column: 0,
            },
        ));
    }
    if let Some(previous) = text.get(..column)?.chars().next_back() {
        let start = Position {
            line: at.line,
            column: column - previous.len_utf8(),
        };
        return Some((start, at));
    }
    let above = at.line.checked_sub(1)?;
    let length = content.line(above)?.text.len();
    Some((
        Position {
            line: above,
            column: length,
        },
        at,
    ))
}

/// Selects the word at the cursor, as a double click does: letters, digits and `_` around
/// it, else the one character there. A plain selection, whose bounds the cursor reports.
pub fn select_word<R: iced::advanced::text::Renderer>(content: &mut Content<R>) {
    let at = content.cursor().position;
    let Some(text) = content.line(at.line).map(|line| line.text.into_owned()) else {
        return;
    };
    let column = at.column.min(text.len());
    let word = |character: char| character.is_alphanumeric() || character == '_';
    let after = text.get(column..).unwrap_or_default();
    let before = text.get(..column).unwrap_or_default();
    let (start, end) = match after.chars().next() {
        Some(here) if word(here) => (
            column
                - before
                    .chars()
                    .rev()
                    .take_while(|c| word(*c))
                    .map(char::len_utf8)
                    .sum::<usize>(),
            column
                + after
                    .chars()
                    .take_while(|c| word(*c))
                    .map(char::len_utf8)
                    .sum::<usize>(),
        ),
        Some(here) => (column, column + here.len_utf8()),
        None => (column, column),
    };
    content.move_to(Cursor {
        position: Position {
            line: at.line,
            column: end,
        },
        selection: Some(Position {
            line: at.line,
            column: start,
        }),
    });
}

/// Selects the line at the cursor, as a triple click does, its ending left out. A plain
/// selection, whose bounds the cursor reports.
pub fn select_line<R: iced::advanced::text::Renderer>(content: &mut Content<R>) {
    let at = content.cursor().position;
    let Some(length) = content.line(at.line).map(|line| line.text.len()) else {
        return;
    };
    content.move_to(Cursor {
        position: Position {
            line: at.line,
            column: length,
        },
        selection: Some(Position {
            line: at.line,
            column: 0,
        }),
    });
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

    use iced::widget::text_editor::{Action, Cursor, Edit, Motion, Position};

    use super::{History, select_line, select_word};
    use crate::code_editor::Content;

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
    fn a_word_or_a_line_selected_then_replaced_comes_back() {
        let mut content = Text::with_text("hello world\nnext line\n");
        let mut history = History::default();
        at(&mut content, 0, 9);
        select_word(&mut content);
        assert_eq!(content.selection().as_deref(), Some("world"));
        history.edit(&mut content, Edit::Insert('X'));
        assert_eq!(content.text(), "hello X\nnext line\n");
        assert!(history.undo(&mut content));
        assert_eq!(content.text(), "hello world\nnext line\n");

        at(&mut content, 1, 2);
        select_line(&mut content);
        assert_eq!(content.selection().as_deref(), Some("next line"));
        // Cut: what was selected is deleted, and comes back.
        history.edit(&mut content, Edit::Delete);
        assert_eq!(content.text(), "hello world\n\n");
        assert!(history.undo(&mut content));
        assert_eq!(content.text(), "hello world\nnext line\n");
    }

    #[test]
    fn undo_never_takes_the_users_selection_with_it() {
        let mut content = Text::with_text("abc def");
        let mut history = History::default();
        at(&mut content, 0, 3);
        history.edit(&mut content, Edit::Backspace);
        assert_eq!(content.text(), "ab def");
        content.move_to(Cursor {
            position: Position { line: 0, column: 6 },
            selection: Some(Position { line: 0, column: 2 }),
        });
        assert!(history.undo(&mut content));
        assert_eq!(content.text(), "abc def", "the selection kept");
        content.move_to(Cursor {
            position: Position { line: 0, column: 7 },
            selection: Some(Position { line: 0, column: 3 }),
        });
        assert!(history.redo(&mut content));
        assert_eq!(content.text(), "ab def");
    }

    #[test]
    fn backspace_takes_the_character_before_in_the_text_whatever_its_direction() {
        let hebrew = "\u{5e9}\u{5dc}\u{5d5}\u{5dd}";
        let mut content = Text::with_text(hebrew);
        let mut history = History::default();
        at(&mut content, 0, "\u{5e9}\u{5dc}".len());
        history.edit(&mut content, Edit::Backspace);
        assert_eq!(content.text(), "\u{5e9}\u{5d5}\u{5dd}");
        history.edit(&mut content, Edit::Delete);
        assert_eq!(content.text(), "\u{5e9}\u{5dd}");
        while history.undo(&mut content) {}
        assert_eq!(content.text(), hebrew);
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
