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

//! The shared session sources of the Settings page, as the C# card of its "Shared session
//! sources" (`SettingsViewModel.SharedSessionSources.cs`): one row per source with its name,
//! its file (typed or picked), whether it is read, what became of its last read, its import
//! report and Remove, then Add.
//!
//! Every choice applies at once on this page, where the C# waits for Save: a list in which
//! every row passes the checks of the C# rows and of its settings import (a name, not used
//! twice in any case; an absolute path; an identifier of its own) is applied as it is edited;
//! one that does not stays edited here, each row saying why, until it does.

use heimdall_core::shared_sessions::{
    IdProblem, NameProblem, PathProblem, SharedSource, SourceProblems, new_source_id,
    source_problems,
};
use iced::widget::{Column, button, checkbox, column, container, row, text};
use iced::{Alignment, Element, Length};

use crate::browse::{BrowseTarget, browse_button};
use crate::i18n::fl;
use crate::shared_view::source_notes;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, spacing};

/// Widest the card grows, as the other Settings cards.
const CARD_WIDTH: f32 = 720.0;

/// A change in the editor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourcesEdit {
    /// Add an empty source under a new identifier, as the C# "Add".
    Add,
    /// Remove the source at this place.
    Remove(usize),
    /// The name of the source at this place typed.
    Name(usize, String),
    /// The path of the source at this place typed or picked.
    Path(usize, String),
    /// The source at this place turned on or off.
    Enabled(usize, bool),
    /// Show the import report of the source at this place.
    Report(usize),
}

/// The sources as edited, and what they last applied.
#[derive(Debug, Clone, Default)]
pub struct SharedSourcesEditor {
    /// The rows as edited, in the order shown.
    rows: Vec<SharedSource>,
    /// The sources the settings had when last read or applied.
    seen: Vec<SharedSource>,
}

impl SharedSourcesEditor {
    /// The editor showing `sources`.
    #[must_use]
    pub fn new(sources: &[SharedSource]) -> Self {
        Self {
            rows: sources.to_vec(),
            seen: sources.to_vec(),
        }
    }

    /// Shows `sources` again when the settings changed them from elsewhere (an import, or a
    /// change that could not be saved); what is edited otherwise stays.
    pub fn sync(&mut self, sources: &[SharedSource]) {
        if self.seen != sources {
            *self = Self::new(sources);
        }
    }

    /// The rows as edited.
    #[must_use]
    pub fn rows(&self) -> &[SharedSource] {
        &self.rows
    }

    /// What is wrong with each row, in row order.
    #[must_use]
    pub fn problems(&self) -> Vec<SourceProblems> {
        source_problems(&self.rows)
    }

    /// The identifier of the source at `at`, for its report.
    #[must_use]
    pub fn id_at(&self, at: usize) -> Option<&str> {
        self.rows.get(at).map(|source| source.id.as_str())
    }

    /// Applies `edit`. The sources to apply when every row passes the checks and they differ
    /// from the ones applied; `None` otherwise. [`SourcesEdit::Report`] changes nothing here.
    pub fn edit(&mut self, edit: SourcesEdit) -> Option<Vec<SharedSource>> {
        match edit {
            SourcesEdit::Add => {
                // A source with no random identifier cannot be told from another: not added.
                let id = new_source_id().ok()?;
                self.rows.push(SharedSource {
                    id,
                    name: String::new(),
                    path: String::new(),
                    enabled: true,
                });
            }
            SourcesEdit::Remove(at) => {
                if at >= self.rows.len() {
                    return None;
                }
                self.rows.remove(at);
            }
            SourcesEdit::Name(at, name) => self.rows.get_mut(at)?.name = name,
            SourcesEdit::Path(at, path) => self.rows.get_mut(at)?.path = path,
            SourcesEdit::Enabled(at, on) => self.rows.get_mut(at)?.enabled = on,
            SourcesEdit::Report(_) => return None,
        }
        self.checked()
    }

    /// The rows as the settings keep them, trimmed as the C# Save trims them.
    fn saved(&self) -> Vec<SharedSource> {
        self.rows
            .iter()
            .map(|source| SharedSource {
                id: source.id.clone(),
                name: source.name.trim().to_owned(),
                path: source.path.trim().to_owned(),
                enabled: source.enabled,
            })
            .collect()
    }

    /// The sources to apply when every row passes and they are not the ones applied.
    fn checked(&mut self) -> Option<Vec<SharedSource>> {
        if self.problems().iter().any(|problems| !problems.is_ok()) {
            return None;
        }
        let saved = self.saved();
        if saved == self.seen {
            return None;
        }
        self.seen.clone_from(&saved);
        Some(saved)
    }

    /// The card: the hint, the rows with their errors and what became of their file, and Add.
    pub fn view<'a>(&'a self, app: &'a heimdall_app::App) -> Element<'a, Message> {
        let problems = self.problems();
        let states = app.shared_sessions().sources();
        let mut content = Column::new().spacing(spacing::SM);
        content = content.push(
            text(fl!("ui-shared-sources-description"))
                .size(font_size::CAPTION)
                .style(text::secondary),
        );
        for (at, source) in self.rows.iter().enumerate() {
            content = content.push(
                row![
                    styles::field(&fl!("ui-shared-sources-name"), &source.name)
                        .style(styles::text_input)
                        .width(Length::FillPortion(1))
                        .on_input(move |name| send(SourcesEdit::Name(at, name))),
                    styles::field(&fl!("ui-shared-sources-path"), &source.path)
                        .style(styles::text_input)
                        .width(Length::FillPortion(2))
                        .on_input(move |path| send(SourcesEdit::Path(at, path))),
                    browse_button(BrowseTarget::SharedSourcePath(at)),
                ]
                .spacing(spacing::SM)
                .align_y(Alignment::Center),
            );
            // A source not yet applied has no state to report on.
            let state = states.iter().find(|state| state.id == source.id);
            let mut actions = row![
                checkbox(source.enabled)
                    .style(styles::checkbox)
                    .label(fl!("ui-shared-sources-enabled"))
                    .on_toggle(move |on| send(SourcesEdit::Enabled(at, on))),
                iced::widget::space::horizontal(),
            ]
            .spacing(spacing::SM)
            .align_y(Alignment::Center);
            if state.is_some_and(|state| state.sessions > 0) {
                actions = actions.push(
                    button(text(fl!("ui-shared-sources-report")))
                        .padding(crate::tokens::BUTTON_PADDING)
                        .style(styles::secondary)
                        .on_press(send(SourcesEdit::Report(at))),
                );
            }
            actions = actions.push(
                button(text(fl!("ui-shared-sources-remove")))
                    .padding(crate::tokens::BUTTON_PADDING)
                    .style(styles::danger)
                    .on_press(send(SourcesEdit::Remove(at))),
            );
            content = content.push(actions);
            if let Some(state) = state {
                content = content.push(
                    text(fl!("ui-shared-sources-status", count = state.sessions))
                        .size(font_size::CAPTION)
                        .style(text::secondary),
                );
                for (note, warning) in source_notes(state) {
                    content = content.push(text(note).size(font_size::CAPTION).style(if warning {
                        styles::warning_text
                    } else {
                        text::secondary
                    }));
                }
            }
            for message in problems
                .get(at)
                .copied()
                .map(problem_texts)
                .unwrap_or_default()
            {
                content = content.push(
                    text(message)
                        .size(font_size::CAPTION)
                        .style(styles::error_text),
                );
            }
        }
        content = content.push(column![
            button(text(fl!("ui-shared-sources-add")))
                .padding(crate::tokens::BUTTON_PADDING)
                .style(styles::secondary)
                .on_press(send(SourcesEdit::Add))
        ]);
        container(content)
            .padding(spacing::MD)
            .max_width(CARD_WIDTH)
            .width(Length::Fill)
            .style(styles::panel)
            .into()
    }
}

/// What to say of each problem of a row.
fn problem_texts(problems: SourceProblems) -> Vec<String> {
    let mut texts = Vec::new();
    match problems.name {
        Some(NameProblem::Required) => texts.push(fl!("ui-shared-sources-name-required")),
        Some(NameProblem::Duplicate) => texts.push(fl!("ui-shared-sources-name-duplicate")),
        None => {}
    }
    match problems.path {
        Some(PathProblem::Required) => texts.push(fl!("ui-shared-sources-path-required")),
        Some(PathProblem::Invalid) => texts.push(fl!("ui-shared-sources-path-invalid")),
        None => {}
    }
    match problems.id {
        Some(IdProblem::Invalid) => texts.push(fl!("ui-shared-sources-id-invalid")),
        Some(IdProblem::Duplicate) => texts.push(fl!("ui-shared-sources-id-duplicate")),
        None => {}
    }
    texts
}

/// `edit` as the window's message.
fn send(edit: SourcesEdit) -> Message {
    Message::SharedSourcesEdit(edit)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(id: &str, name: &str, path: &str) -> SharedSource {
        SharedSource {
            id: id.to_owned(),
            name: name.to_owned(),
            path: path.to_owned(),
            enabled: true,
        }
    }

    #[test]
    fn a_new_row_is_not_applied_until_it_has_a_name_and_a_path() {
        let mut editor = SharedSourcesEditor::new(&[]);
        assert_eq!(editor.edit(SourcesEdit::Add), None);
        assert_eq!(editor.rows().len(), 1);
        let problems = editor.problems();
        assert_eq!(problems[0].name, Some(NameProblem::Required));
        assert_eq!(problems[0].path, Some(PathProblem::Required));
        assert_eq!(problems[0].id, None, "it has an identifier of its own");
        assert_eq!(editor.edit(SourcesEdit::Name(0, "Team".to_owned())), None);
        assert_eq!(
            editor.edit(SourcesEdit::Path(0, "relative.mxtsessions".to_owned())),
            None,
            "a relative path is refused as the import refuses it"
        );
        let applied = editor
            .edit(SourcesEdit::Path(0, " /srv/team.mxtsessions ".to_owned()))
            .expect("applied");
        assert_eq!(applied.len(), 1);
        assert_eq!(applied[0].name, "Team");
        assert_eq!(applied[0].path, "/srv/team.mxtsessions", "trimmed");
        assert!(applied[0].enabled);
    }

    #[test]
    fn two_rows_may_not_share_a_name_in_any_case() {
        let mut editor = SharedSourcesEditor::new(&[source("one", "Team", "/srv/a")]);
        assert_eq!(editor.edit(SourcesEdit::Add), None);
        assert_eq!(editor.edit(SourcesEdit::Path(1, "/srv/b".to_owned())), None);
        assert_eq!(editor.edit(SourcesEdit::Name(1, "TEAM".to_owned())), None);
        assert_eq!(
            editor.problems()[1].name,
            Some(NameProblem::Duplicate),
            "the C# refuses the second"
        );
        let applied = editor
            .edit(SourcesEdit::Name(1, "Lab".to_owned()))
            .expect("applied");
        assert_eq!(applied.len(), 2);
    }

    #[test]
    fn turning_a_source_off_and_removing_it_apply_at_once() {
        let mut editor = SharedSourcesEditor::new(&[
            source("one", "Team", "/srv/a"),
            source("two", "Lab", "/srv/b"),
        ]);
        let off = editor
            .edit(SourcesEdit::Enabled(0, false))
            .expect("applied");
        assert!(!off[0].enabled);
        let removed = editor.edit(SourcesEdit::Remove(0)).expect("applied");
        assert_eq!(removed.len(), 1);
        assert_eq!(removed[0].id, "two");
        assert_eq!(editor.edit(SourcesEdit::Remove(9)), None);
        assert_eq!(editor.edit(SourcesEdit::Report(0)), None);
        assert_eq!(editor.id_at(0), Some("two"));
    }

    #[test]
    fn the_editor_follows_the_settings_when_they_change_elsewhere() {
        let mut editor = SharedSourcesEditor::new(&[source("one", "Team", "/srv/a")]);
        editor.edit(SourcesEdit::Add);
        // What is being edited stays while the settings are as they were.
        editor.sync(&[source("one", "Team", "/srv/a")]);
        assert_eq!(editor.rows().len(), 2);
        editor.sync(&[]);
        assert!(editor.rows().is_empty());
    }
}
