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

//! The `MobaXterm` import variables of the Settings page, as the C# card of the `MobaXterm` import
//! variables (`SettingsViewModel.MobaXtermVariables.cs`): one row per variable with its
//! name, its value and what the value resolves to, Add and Remove.
//!
//! Every choice applies at once on this page, where the C# waits for Save: a list in which
//! every row passes the C# row checks (a name the import recognises, not used twice in any
//! case, and a value) is applied as it is edited; one that does not stays edited here, each
//! row saying why, until it does.

use std::collections::{BTreeMap, HashMap};

use heimdall_core::import::mobaxterm::{is_valid_variable_name, resolve_value};
use iced::widget::{Column, button, column, container, row, text};
use iced::{Alignment, Element, Length};

use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, spacing};

/// Widest the card grows, as the other Settings cards.
const CARD_WIDTH: f32 = 720.0;

/// Width of the "Resolves to" column.
const RESOLVED_WIDTH: f32 = 180.0;

/// A change in the editor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VariablesEdit {
    /// Add an empty row, as the C# "Add".
    Add,
    /// Remove the row at this place.
    Remove(usize),
    /// The name of the row at this place typed.
    Name(usize, String),
    /// The value of the row at this place typed.
    Value(usize, String),
}

/// Why a row cannot be saved, as the C# `MobaXtermVariableErrorKeys`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowProblem {
    /// No name.
    NameRequired,
    /// A name the import would never recognise.
    NameInvalid,
    /// Another row has this name, in any case.
    NameDuplicate,
    /// No value.
    ValueRequired,
}

impl RowProblem {
    fn text(self) -> String {
        match self {
            Self::NameRequired => fl!("ui-mobaxterm-variables-name-required"),
            Self::NameInvalid => fl!("ui-mobaxterm-variables-name-invalid"),
            Self::NameDuplicate => fl!("ui-mobaxterm-variables-name-duplicate"),
            Self::ValueRequired => fl!("ui-mobaxterm-variables-value-required"),
        }
    }
}

/// The variables as edited, and what they last applied.
#[derive(Debug, Clone, Default)]
pub struct MobaxtermVariablesEditor {
    /// The rows as edited: name and value, in the order shown.
    rows: Vec<(String, String)>,
    /// The variables the settings had when last read or applied.
    seen: BTreeMap<String, String>,
}

impl MobaxtermVariablesEditor {
    /// The editor showing `variables`.
    #[must_use]
    pub fn new(variables: &BTreeMap<String, String>) -> Self {
        Self {
            rows: variables
                .iter()
                .map(|(name, value)| (name.clone(), value.clone()))
                .collect(),
            seen: variables.clone(),
        }
    }

    /// Shows `variables` again when the settings changed them from elsewhere (an import, or
    /// a change that could not be saved); what is edited otherwise stays.
    pub fn sync(&mut self, variables: &BTreeMap<String, String>) {
        if &self.seen != variables {
            *self = Self::new(variables);
        }
    }

    /// The rows as edited.
    #[must_use]
    pub fn rows(&self) -> &[(String, String)] {
        &self.rows
    }

    /// Why each row cannot be saved, in row order; `None` for a row that can.
    #[must_use]
    pub fn problems(&self) -> Vec<Option<RowProblem>> {
        let mut counts: HashMap<String, usize> = HashMap::new();
        for (name, _) in &self.rows {
            let name = name.trim();
            if !name.is_empty() {
                *counts.entry(name.to_lowercase()).or_default() += 1;
            }
        }
        self.rows
            .iter()
            .map(|(name, value)| {
                let name = name.trim();
                if name.is_empty() {
                    Some(RowProblem::NameRequired)
                } else if !is_valid_variable_name(name) {
                    Some(RowProblem::NameInvalid)
                } else if counts.get(&name.to_lowercase()).is_some_and(|n| *n > 1) {
                    Some(RowProblem::NameDuplicate)
                } else if value.trim().is_empty() {
                    Some(RowProblem::ValueRequired)
                } else {
                    None
                }
            })
            .collect()
    }

    /// Applies `edit`. The variables to apply when every row passes the checks and they differ
    /// from the ones applied; `None` otherwise.
    pub fn edit(&mut self, edit: VariablesEdit) -> Option<BTreeMap<String, String>> {
        match edit {
            VariablesEdit::Add => self.rows.push((String::new(), String::new())),
            VariablesEdit::Remove(at) => {
                if at >= self.rows.len() {
                    return None;
                }
                self.rows.remove(at);
            }
            VariablesEdit::Name(at, name) => self.rows.get_mut(at)?.0 = name,
            VariablesEdit::Value(at, value) => self.rows.get_mut(at)?.1 = value,
        }
        self.checked()
    }

    /// The rows as the settings keep them, trimmed as the C# Save trims them.
    fn saved(&self) -> BTreeMap<String, String> {
        self.rows
            .iter()
            .map(|(name, value)| (name.trim().to_owned(), value.trim().to_owned()))
            .collect()
    }

    /// The variables to apply when every row passes and they are not the ones applied.
    fn checked(&mut self) -> Option<BTreeMap<String, String>> {
        if self.problems().iter().any(Option::is_some) {
            return None;
        }
        let saved = self.saved();
        if saved == self.seen {
            return None;
        }
        self.seen.clone_from(&saved);
        Some(saved)
    }

    /// The card: the hint, the rows with their errors, and Add.
    pub fn view(&self) -> Element<'_, Message> {
        let problems = self.problems();
        let mut content = Column::new().spacing(spacing::SM);
        content = content.push(
            text(fl!("ui-mobaxterm-variables-description"))
                .size(font_size::CAPTION)
                .style(text::secondary),
        );
        if !self.rows.is_empty() {
            content = content.push(
                row![
                    text(fl!("ui-mobaxterm-variables-name"))
                        .size(font_size::CAPTION)
                        .width(Length::FillPortion(1)),
                    text(fl!("ui-mobaxterm-variables-value"))
                        .size(font_size::CAPTION)
                        .width(Length::FillPortion(2)),
                    text(fl!("ui-mobaxterm-variables-resolved"))
                        .size(font_size::CAPTION)
                        .width(RESOLVED_WIDTH),
                ]
                .spacing(spacing::SM),
            );
        }
        for (at, (name, value)) in self.rows.iter().enumerate() {
            let resolved =
                resolve_value(value).unwrap_or_else(|| fl!("ui-mobaxterm-variables-unresolved"));
            content = content.push(
                row![
                    styles::field("", name)
                        .style(styles::text_input)
                        .width(Length::FillPortion(1))
                        .on_input(move |name| send(VariablesEdit::Name(at, name))),
                    styles::field("", value)
                        .style(styles::text_input)
                        .width(Length::FillPortion(2))
                        .on_input(move |value| send(VariablesEdit::Value(at, value))),
                    text(resolved).width(RESOLVED_WIDTH),
                    button(text(fl!("ui-mobaxterm-variables-remove")))
                        .padding(crate::tokens::BUTTON_PADDING)
                        .style(styles::danger)
                        .on_press(send(VariablesEdit::Remove(at))),
                ]
                .spacing(spacing::SM)
                .align_y(Alignment::Center),
            );
            if let Some(problem) = problems.get(at).copied().flatten() {
                content = content.push(
                    text(problem.text())
                        .size(font_size::CAPTION)
                        .style(styles::error_text),
                );
            }
        }
        content = content.push(column![
            button(text(fl!("ui-mobaxterm-variables-add")))
                .padding(crate::tokens::BUTTON_PADDING)
                .style(styles::secondary)
                .on_press(send(VariablesEdit::Add))
        ]);
        container(content)
            .padding(spacing::MD)
            .max_width(CARD_WIDTH)
            .width(Length::Fill)
            .style(styles::panel)
            .into()
    }
}

/// `edit` as the window's message.
fn send(edit: VariablesEdit) -> Message {
    Message::MobaxtermVariablesEdit(edit)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn editor(pairs: &[(&str, &str)]) -> MobaxtermVariablesEditor {
        MobaxtermVariablesEditor::new(
            &pairs
                .iter()
                .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
                .collect(),
        )
    }

    #[test]
    fn a_row_is_applied_once_it_has_a_valid_name_and_a_value() {
        let mut editor = editor(&[]);
        assert_eq!(editor.edit(VariablesEdit::Add), None, "an empty row waits");
        assert_eq!(editor.problems(), [Some(RowProblem::NameRequired)]);
        assert_eq!(editor.edit(VariablesEdit::Name(0, "TEAM".into())), None);
        assert_eq!(editor.problems(), [Some(RowProblem::ValueRequired)]);
        let applied = editor.edit(VariablesEdit::Value(0, " jb ".into()));
        assert_eq!(
            applied,
            Some(BTreeMap::from([("TEAM".to_owned(), "jb".to_owned())])),
            "trimmed as the C# Save trims"
        );
        assert_eq!(
            editor.edit(VariablesEdit::Value(0, "jb".into())),
            None,
            "nothing changed"
        );
    }

    #[test]
    fn a_name_the_import_would_not_recognise_or_a_repeated_one_is_refused() {
        let mut editor = editor(&[("DAS", "a")]);
        assert_eq!(editor.edit(VariablesEdit::Add), None);
        assert_eq!(editor.edit(VariablesEdit::Name(1, "1x".into())), None);
        assert_eq!(editor.problems()[1], Some(RowProblem::NameInvalid));
        assert_eq!(editor.edit(VariablesEdit::Name(1, "das".into())), None);
        assert_eq!(
            editor.problems(),
            [
                Some(RowProblem::NameDuplicate),
                Some(RowProblem::NameDuplicate)
            ],
            "the same name in any case"
        );
    }

    #[test]
    fn removing_the_last_row_applies_an_empty_list() {
        let mut editor = editor(&[("DAS", "a")]);
        assert_eq!(editor.edit(VariablesEdit::Remove(5)), None);
        assert_eq!(editor.edit(VariablesEdit::Remove(0)), Some(BTreeMap::new()));
    }

    #[test]
    fn what_the_settings_change_from_elsewhere_is_shown_again() {
        let mut editor = editor(&[("DAS", "a")]);
        let _ = editor.edit(VariablesEdit::Add);
        editor.sync(&BTreeMap::from([("X".to_owned(), "y".to_owned())]));
        assert_eq!(editor.rows(), [("X".to_owned(), "y".to_owned())]);
    }
}
