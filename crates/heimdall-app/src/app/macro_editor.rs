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

//! The macro editor, as the C# one: a macro's name and its inputs, each with its pause and
//! what it waits for first, added, moved, removed, then checked and kept.

use heimdall_core::macros::{
    EXPECT_TIMEOUT_DEFAULT, EXPECT_TIMEOUT_MAX, EXPECT_TIMEOUT_MIN, Expect, InputError, MacroEntry,
    OnTimeout, TerminalMacro, read_input, written_input,
};

use super::{App, Dialog, Notice};

/// An input of the macro being edited, as typed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryDraft {
    /// What is typed, control characters written as `\r`, `\n`, `\t`, `\xNN`.
    pub input: String,
    /// The pause before it, in milliseconds.
    pub delay: String,
    /// It waits for text first.
    pub expects: bool,
    /// The text, or a regular expression.
    pub pattern: String,
    /// `pattern` is a regular expression.
    pub regex: bool,
    /// How long it waits, in milliseconds.
    pub timeout: String,
    /// What then.
    pub on_timeout: OnTimeout,
}

impl EntryDraft {
    fn of(entry: &MacroEntry) -> Self {
        let expect = entry.expect.as_ref();
        Self {
            input: written_input(&entry.input),
            delay: entry.delay_ms.to_string(),
            expects: expect.is_some(),
            pattern: expect
                .map(|expect| expect.pattern.clone())
                .unwrap_or_default(),
            regex: expect.is_some_and(|expect| expect.regex),
            timeout: expect
                .map_or(EXPECT_TIMEOUT_DEFAULT, |expect| expect.timeout_ms)
                .to_string(),
            on_timeout: expect.map(|expect| expect.on_timeout).unwrap_or_default(),
        }
    }

    fn blank(expects: bool) -> Self {
        Self {
            input: String::new(),
            delay: "0".to_owned(),
            expects,
            pattern: String::new(),
            regex: false,
            timeout: EXPECT_TIMEOUT_DEFAULT.to_string(),
            on_timeout: OnTimeout::Abort,
        }
    }

    /// The input it says, or why not.
    fn entry(&self) -> Result<MacroEntry, EntryProblem> {
        let input = read_input(&self.input).map_err(EntryProblem::Input)?;
        let delay_ms = self.delay.trim().parse().map_err(|_| EntryProblem::Delay)?;
        let expect = if self.expects {
            let timeout_ms = self
                .timeout
                .trim()
                .parse()
                .ok()
                .filter(|timeout| (EXPECT_TIMEOUT_MIN..=EXPECT_TIMEOUT_MAX).contains(timeout))
                .ok_or(EntryProblem::Timeout)?;
            if self.regex {
                regex::Regex::new(&self.pattern)
                    .map_err(|error| EntryProblem::Regex(error.to_string()))?;
            }
            Some(Expect {
                pattern: self.pattern.clone(),
                regex: self.regex,
                timeout_ms,
                on_timeout: self.on_timeout,
            })
        } else {
            None
        };
        Ok(MacroEntry {
            input,
            delay_ms,
            expect,
        })
    }
}

/// What is wrong with an input of the macro being edited.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntryProblem {
    /// Its input cannot be read.
    Input(InputError),
    /// Its pause is not a number of milliseconds.
    Delay,
    /// Its wait is out of the C# range.
    Timeout,
    /// Its regular expression does not compile, for this reason.
    Regex(String),
}

/// Why the macro being edited is not kept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MacroProblem {
    /// It has no name.
    NameRequired,
    /// Its input `entry`, from 1, is wrong.
    Entry {
        /// The input, from 1.
        entry: usize,
        /// What is wrong.
        problem: EntryProblem,
    },
}

/// The macro being edited.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MacroDraft {
    /// The name it is kept under; `None` for a new one.
    pub original: Option<String>,
    /// Its name.
    pub name: String,
    /// Its inputs.
    pub entries: Vec<EntryDraft>,
    /// Why it was not kept, last time.
    pub problem: Option<MacroProblem>,
}

impl MacroDraft {
    /// `kept`, to edit.
    fn of(kept: &TerminalMacro) -> Self {
        Self {
            original: Some(kept.name.clone()),
            name: kept.name.clone(),
            entries: kept.entries.iter().map(EntryDraft::of).collect(),
            problem: None,
        }
    }

    /// The macro it says, or why not.
    fn checked(&self) -> Result<TerminalMacro, MacroProblem> {
        let name = self.name.trim();
        if name.is_empty() {
            return Err(MacroProblem::NameRequired);
        }
        let entries = self
            .entries
            .iter()
            .enumerate()
            .map(|(index, entry)| {
                entry.entry().map_err(|problem| MacroProblem::Entry {
                    entry: index + 1,
                    problem,
                })
            })
            .collect::<Result<_, _>>()?;
        Ok(TerminalMacro {
            name: name.to_owned(),
            entries,
        })
    }
}

/// A field of an input of the macro being edited.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntryField {
    /// What is typed.
    Input(String),
    /// The pause before it.
    Delay(String),
    /// It waits for text first.
    Expects(bool),
    /// The text waited for.
    Pattern(String),
    /// That text is a regular expression.
    Regex(bool),
    /// How long it waits.
    Timeout(String),
    /// What then.
    OnTimeout(OnTimeout),
}

/// A change of the macro being edited.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MacroEdit {
    /// Its name.
    Name(String),
    /// A field of its input `entry`, from 0.
    Field {
        /// The input.
        entry: usize,
        /// The field.
        field: EntryField,
    },
    /// An input added at the end, waiting for text first or not, as the C# "Add expect
    /// step" and "Add send step".
    Add {
        /// It waits for text first.
        expects: bool,
    },
    /// The input `entry` moved one up.
    MoveUp(usize),
    /// The input `entry` moved one down.
    MoveDown(usize),
    /// The input `entry` taken out.
    Remove(usize),
}

impl App {
    /// The editor of the macro `name`.
    pub(super) fn edit_macro(&mut self, name: &str) {
        if let Some(kept) = self.macros.get(name) {
            self.dialog = Some(Dialog::EditMacro(Box::new(MacroDraft::of(kept))));
        }
    }

    /// A change of the macro being edited.
    pub(super) fn macro_edit(&mut self, edit: MacroEdit) {
        let Some(Dialog::EditMacro(draft)) = self.dialog.as_mut() else {
            return;
        };
        draft.problem = None;
        let entries = &mut draft.entries;
        match edit {
            MacroEdit::Name(name) => draft.name = name,
            MacroEdit::Field { entry, field } => {
                if let Some(entry) = entries.get_mut(entry) {
                    match field {
                        EntryField::Input(input) => entry.input = input,
                        EntryField::Delay(delay) => entry.delay = delay,
                        EntryField::Expects(expects) => entry.expects = expects,
                        EntryField::Pattern(pattern) => entry.pattern = pattern,
                        EntryField::Regex(regex) => entry.regex = regex,
                        EntryField::Timeout(timeout) => entry.timeout = timeout,
                        EntryField::OnTimeout(on_timeout) => entry.on_timeout = on_timeout,
                    }
                }
            }
            MacroEdit::Add { expects } => entries.push(EntryDraft::blank(expects)),
            MacroEdit::MoveUp(index) if index > 0 && index < entries.len() => {
                entries.swap(index - 1, index);
            }
            MacroEdit::MoveDown(index) if index + 1 < entries.len() => {
                entries.swap(index, index + 1);
            }
            MacroEdit::Remove(index) if index < entries.len() => {
                entries.remove(index);
            }
            MacroEdit::MoveUp(_) | MacroEdit::MoveDown(_) | MacroEdit::Remove(_) => {}
        }
    }

    /// The macro edited, kept as checked, in place of what it was; asked again, with what is
    /// wrong, when it cannot be.
    pub(super) fn save_edited_macro(&mut self, mut draft: MacroDraft) {
        match draft.checked() {
            Err(problem) => {
                draft.problem = Some(problem);
                self.dialog = Some(Dialog::EditMacro(Box::new(draft)));
            }
            Ok(edited) => {
                let mut kept = self.macros.clone();
                if let Some(original) = &draft.original {
                    kept.remove(original);
                }
                let name = edited.name.clone();
                kept.put(edited);
                self.keep_macros(kept, Notice::MacroSaved(name));
            }
        }
    }
}
