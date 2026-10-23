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

//! The external tools of the Settings page, as the C# "External Tools" card
//! (`MainWindow.xaml:5283-5390`): the list, Add and Remove, here Move up and Move down too;
//! the tool chosen edited under it, its placeholders listed, its command previewed and
//! "Test"; and what the status bar says of a tool started, or not.
//!
//! Every choice applies at once on this page, where the C# waits for Save: a list that
//! passes the C# Save's checks (each tool named and with a program found, no name twice) is
//! applied as it is edited; one that does not stays edited here, why said under it, until
//! it does.

use heimdall_app::external_tool::ToolFailure;
use heimdall_app::{ExternalToolMessage, Message as AppMessage, Notice, server_text};
use heimdall_core::external_tools::{self, ExternalTool, Placeholder, ToolsProblem};
use iced::widget::{Column, button, checkbox, column, container, row, text};
use iced::{Alignment, Element, Font, Length};

use crate::browse::{BrowseTarget, browse_button};
use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, spacing};

/// Widest the card grows, as the other Settings cards.
const CARD_WIDTH: f32 = 720.0;

/// Width of the labels before the editor's fields, as the C# grid's first column.
const LABEL_WIDTH: f32 = 140.0;

/// A change in the editor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolsEdit {
    /// Choose the tool at this place of the list.
    Select(usize),
    /// Add a tool, as the C# "Add": named, the host as its arguments, chosen.
    Add,
    /// Remove the tool chosen.
    Remove,
    /// Move the tool chosen one place up.
    Up,
    /// Move it one place down.
    Down,
    /// Its name typed.
    Name(String),
    /// Its program typed.
    Program(String),
    /// Its arguments typed.
    Arguments(String),
    /// Its working folder typed.
    Folder(String),
    /// Run as administrator, or not.
    Administrator(bool),
    /// Run hidden, or not.
    Hidden(bool),
    /// Start the tool chosen as it is edited, for no session, as the C# "Test".
    Test,
}

/// The list as edited, and what it last applied.
#[derive(Debug, Clone, Default)]
pub struct ExternalToolsEditor {
    /// The tools as edited.
    rows: Vec<ExternalTool>,
    /// The tools the settings had when last read or applied, as they are saved.
    seen: Vec<ExternalTool>,
    /// The tool chosen, by its place.
    selected: Option<usize>,
    /// Why the list edited is not applied.
    problem: Option<ToolsProblem>,
}

impl ExternalToolsEditor {
    /// The editor showing `tools`, none chosen.
    #[must_use]
    pub fn new(tools: &[ExternalTool]) -> Self {
        Self {
            rows: tools.to_vec(),
            seen: tools.to_vec(),
            selected: None,
            problem: None,
        }
    }

    /// Shows `tools` again when the settings changed them from elsewhere (an import, or a
    /// change that could not be saved); what is edited otherwise stays.
    pub fn sync(&mut self, tools: &[ExternalTool]) {
        if self.seen != tools {
            // The tool chosen stays chosen by its name, wherever the new list puts it.
            let name = self.selected().map(|tool| tool.name.trim().to_lowercase());
            *self = Self::new(tools);
            self.selected = name.and_then(|name| {
                tools
                    .iter()
                    .position(|tool| tool.name.trim().to_lowercase() == name)
            });
        }
    }

    /// The tool chosen, as edited.
    #[must_use]
    pub fn selected(&self) -> Option<&ExternalTool> {
        self.rows.get(self.selected?)
    }

    /// Why the list edited is not applied, when it is not.
    #[must_use]
    pub fn problem(&self) -> Option<&ToolsProblem> {
        self.problem.as_ref()
    }

    /// Applies `edit`, a new tool named `new_name`; `found` says whether a program written
    /// is there. The list to apply when it passes the C# Save's checks and differs from the
    /// one applied; `None` otherwise. "Test" changes nothing here.
    pub fn edit(
        &mut self,
        edit: ToolsEdit,
        new_name: &str,
        found: impl Fn(&str) -> bool,
    ) -> Option<Vec<ExternalTool>> {
        match edit {
            ToolsEdit::Select(at) => {
                self.selected = (at < self.rows.len()).then_some(at);
                return None;
            }
            ToolsEdit::Test => return None,
            ToolsEdit::Add => {
                self.rows.push(ExternalTool::new(new_name.to_owned()));
                self.selected = Some(self.rows.len() - 1);
            }
            ToolsEdit::Remove => {
                let at = self.selected.take().filter(|at| *at < self.rows.len())?;
                self.rows.remove(at);
            }
            ToolsEdit::Up => {
                let at = self
                    .selected
                    .filter(|at| *at > 0 && *at < self.rows.len())?;
                self.rows.swap(at, at - 1);
                self.selected = Some(at - 1);
            }
            ToolsEdit::Down => {
                let at = self.selected.filter(|at| at + 1 < self.rows.len())?;
                self.rows.swap(at, at + 1);
                self.selected = Some(at + 1);
            }
            edit => {
                let tool = self.rows.get_mut(self.selected?)?;
                match edit {
                    ToolsEdit::Name(name) => tool.name = name,
                    ToolsEdit::Program(program) => tool.executable_path = program,
                    ToolsEdit::Arguments(arguments) => tool.arguments = arguments,
                    ToolsEdit::Folder(folder) => tool.working_directory = folder,
                    ToolsEdit::Administrator(on) => tool.run_as_administrator = on,
                    ToolsEdit::Hidden(on) => tool.run_hidden = on,
                    _ => {}
                }
            }
        }
        self.checked(found)
    }

    /// The path picked in a "Browse..." dialog for the tool chosen: its program, or its
    /// working folder when `folder`.
    pub fn picked(
        &mut self,
        folder: bool,
        path: String,
        found: impl Fn(&str) -> bool,
    ) -> Option<Vec<ExternalTool>> {
        let edit = if folder {
            ToolsEdit::Folder(path)
        } else {
            ToolsEdit::Program(path)
        };
        self.edit(edit, "", found)
    }

    /// The list checked as the C# Save checks it: the list to apply when it passes and is
    /// not the one applied.
    fn checked(&mut self, found: impl Fn(&str) -> bool) -> Option<Vec<ExternalTool>> {
        self.problem = external_tools::problem(&self.rows, found);
        if self.problem.is_some() {
            return None;
        }
        let saved = external_tools::saved(&self.rows);
        if saved == self.seen {
            return None;
        }
        self.seen.clone_from(&saved);
        Some(saved)
    }

    /// The card: the list with its buttons, then the tool chosen edited, with `preview`, the
    /// command it runs, and whether it can run as administrator here, `elevation`.
    pub fn view(
        &self,
        preview: Option<Result<String, ToolFailure>>,
        elevation: bool,
    ) -> Element<'_, Message> {
        let chosen = self.selected.filter(|at| *at < self.rows.len());
        let mut content = Column::new().spacing(spacing::SM);
        content = content.push(
            text(fl!("ui-external-tools-description"))
                .size(font_size::CAPTION)
                .style(text::secondary),
        );
        if self.rows.is_empty() {
            content = content.push(text(fl!("ui-external-tools-empty")).size(font_size::CAPTION));
        }
        for (at, tool) in self.rows.iter().enumerate() {
            let line = fl!(
                "ui-external-tools-row",
                name = server_text(tool.name.trim()),
                program = server_text(tool.executable_path.trim())
            );
            content = content.push(
                button(text(line))
                    .width(Length::Fill)
                    .style(if chosen == Some(at) {
                        styles::primary
                    } else {
                        styles::subtle
                    })
                    .on_press(send(ToolsEdit::Select(at))),
            );
        }
        let movable = |allowed: bool| chosen.is_some() && allowed;
        content = content.push(
            row![
                button(text(fl!("ui-external-tools-add")))
                    .padding(crate::tokens::BUTTON_PADDING)
                    .style(styles::secondary)
                    .on_press(send(ToolsEdit::Add)),
                button(text(fl!("ui-external-tools-move-up")))
                    .padding(crate::tokens::BUTTON_PADDING)
                    .style(styles::secondary)
                    .on_press_maybe(
                        movable(chosen.is_some_and(|at| at > 0)).then(|| send(ToolsEdit::Up))
                    ),
                button(text(fl!("ui-external-tools-move-down")))
                    .padding(crate::tokens::BUTTON_PADDING)
                    .style(styles::secondary)
                    .on_press_maybe(
                        movable(chosen.is_some_and(|at| at + 1 < self.rows.len()))
                            .then(|| send(ToolsEdit::Down))
                    ),
                button(text(fl!("ui-external-tools-remove")))
                    .padding(crate::tokens::BUTTON_PADDING)
                    .style(styles::danger)
                    .on_press_maybe(chosen.map(|_| send(ToolsEdit::Remove))),
            ]
            .spacing(spacing::SM),
        );
        if let Some(tool) = self.selected() {
            content = content.push(tool_editor(tool, preview, elevation));
        }
        if let Some(problem) = &self.problem {
            content = content.push(
                text(problem_text(problem))
                    .size(font_size::CAPTION)
                    .style(styles::error_text),
            );
        }
        container(content)
            .padding(spacing::MD)
            .max_width(CARD_WIDTH)
            .width(Length::Fill)
            .style(styles::panel)
            .into()
    }
}

/// `edit` as the window's message.
fn send(edit: ToolsEdit) -> Message {
    Message::ExternalToolsEdit(edit)
}

/// `label` before `field`, as the C# editor's grid.
fn field_row<'a>(
    label: String,
    field: impl Into<Element<'a, Message>>,
) -> iced::widget::Row<'a, Message> {
    row![text(label).width(LABEL_WIDTH), field.into()]
        .spacing(spacing::SM)
        .align_y(Alignment::Center)
}

/// The fields of `tool`, its placeholders, its command previewed and "Test".
fn tool_editor(
    tool: &ExternalTool,
    preview: Option<Result<String, ToolFailure>>,
    elevation: bool,
) -> Element<'_, Message> {
    let mut editor = column![
        field_row(
            fl!("ui-external-tools-name"),
            styles::field("", &tool.name)
                .style(styles::text_input)
                .on_input(|name| send(ToolsEdit::Name(name))),
        ),
        field_row(
            fl!("ui-external-tools-program"),
            row![
                styles::field("", &tool.executable_path)
                    .style(styles::text_input)
                    .on_input(|program| send(ToolsEdit::Program(program))),
                browse_button(BrowseTarget::ExternalToolProgram),
            ]
            .spacing(spacing::SM)
            .align_y(Alignment::Center),
        ),
        field_row(
            fl!("ui-external-tools-arguments"),
            styles::field("", &tool.arguments)
                .style(styles::text_input)
                .font(Font::MONOSPACE)
                .on_input(|arguments| send(ToolsEdit::Arguments(arguments))),
        ),
        field_row(
            fl!("ui-external-tools-working-directory"),
            row![
                styles::field("", &tool.working_directory)
                    .style(styles::text_input)
                    .on_input(|folder| send(ToolsEdit::Folder(folder))),
                browse_button(BrowseTarget::ExternalToolFolder),
            ]
            .spacing(spacing::SM)
            .align_y(Alignment::Center),
        ),
        row![
            checkbox(tool.run_as_administrator)
                .style(styles::checkbox)
                .label(fl!("ui-external-tools-run-as-administrator"))
                // Where there is no elevation prompt, it can only be cleared.
                .on_toggle_maybe(
                    (elevation || tool.run_as_administrator)
                        .then_some(|on| send(ToolsEdit::Administrator(on)))
                ),
            checkbox(tool.run_hidden)
                .style(styles::checkbox)
                .label(fl!("ui-external-tools-run-hidden"))
                .on_toggle(|on| send(ToolsEdit::Hidden(on))),
        ]
        .spacing(spacing::LG),
    ]
    .spacing(spacing::SM);
    let mut legend = Column::new().spacing(spacing::XS);
    for placeholder in Placeholder::ALL {
        legend = legend.push(
            row![
                text(placeholder.token())
                    .font(Font::MONOSPACE)
                    .size(font_size::CAPTION),
                text(placeholder_text(placeholder))
                    .size(font_size::CAPTION)
                    .style(text::secondary),
            ]
            .spacing(spacing::SM),
        );
    }
    editor = editor.push(legend);
    let shown = match preview {
        Some(Ok(command)) => text(server_text(&command))
            .font(Font::MONOSPACE)
            .size(font_size::CAPTION),
        Some(Err(failure)) => text(failure_text(&failure))
            .size(font_size::CAPTION)
            .style(styles::error_text),
        None => text(String::new()),
    };
    editor
        .push(
            row![
                container(shown).width(Length::Fill),
                button(text(fl!("ui-external-tools-test")))
                    .padding(crate::tokens::BUTTON_PADDING)
                    .style(styles::secondary)
                    .on_press(send(ToolsEdit::Test)),
            ]
            .spacing(spacing::SM)
            .align_y(Alignment::Center),
        )
        .into()
}

/// What `placeholder` stands for, as the C# legend says it.
fn placeholder_text(placeholder: Placeholder) -> String {
    match placeholder {
        Placeholder::Host => fl!("ui-external-tools-var-host"),
        Placeholder::Port => fl!("ui-external-tools-var-port"),
        Placeholder::User => fl!("ui-external-tools-var-user"),
        Placeholder::ServerName => fl!("ui-external-tools-var-server-name"),
        Placeholder::Protocol => fl!("ui-external-tools-var-protocol"),
        Placeholder::KeyFile => fl!("ui-external-tools-var-key-file"),
        Placeholder::Project => fl!("ui-external-tools-var-project"),
        Placeholder::Gateway => fl!("ui-external-tools-var-gateway"),
    }
}

/// Why the list is not applied, as the C# validation says it.
#[must_use]
pub fn problem_text(problem: &ToolsProblem) -> String {
    match problem {
        ToolsProblem::Incomplete => fl!("ui-external-tools-incomplete"),
        ToolsProblem::Duplicate(name) => {
            fl!("ui-external-tools-duplicate", name = server_text(name))
        }
        ToolsProblem::NotFound { name, program } => fl!(
            "ui-external-tools-not-found",
            name = server_text(name),
            program = server_text(program)
        ),
    }
}

/// Why a tool did not start, as the C# `ExternalToolLaunchError` details it.
#[must_use]
pub fn failure_text(failure: &ToolFailure) -> String {
    match failure {
        ToolFailure::ShellTarget => fl!("ui-external-tools-shell-blocked"),
        ToolFailure::NotFound(program) => fl!(
            "ui-external-tools-program-missing",
            program = server_text(program)
        ),
        ToolFailure::InvalidProgram(program) => fl!(
            "ui-external-tools-program-invalid",
            program = server_text(program)
        ),
        ToolFailure::OptionLike(placeholder) => fl!(
            "ui-external-tools-option-like",
            placeholder = placeholder.token()
        ),
        ToolFailure::WorkingDirectory(folder) => fl!(
            "ui-external-tools-folder-missing",
            folder = server_text(folder)
        ),
        ToolFailure::InvalidArguments => fl!("ui-external-tools-arguments-invalid"),
        ToolFailure::ElevationUnsupported => fl!("ui-external-tools-elevation-unsupported"),
        ToolFailure::NotStarted(reason) => server_text(reason),
    }
}

/// What the status bar says of an external tool started, or not.
#[must_use]
pub fn notice(notice: &Notice) -> String {
    match notice {
        Notice::ExternalToolStarted(name) => {
            fl!("ui-status-external-tool-started", name = server_text(name))
        }
        Notice::ExternalToolFailed { name, failure } => fl!(
            "ui-status-external-tool-failed",
            name = server_text(name),
            reason = failure_text(failure)
        ),
        _ => String::new(),
    }
}

/// The message starting the tool `name` of the list for the profile `profile`.
#[must_use]
pub fn launch(name: &str, profile: heimdall_core::profile::ProfileId) -> AppMessage {
    AppMessage::ExternalTool(ExternalToolMessage::Launch {
        name: name.to_owned(),
        profile: Some(profile),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tool(name: &str, program: &str) -> ExternalTool {
        ExternalTool {
            name: name.to_owned(),
            executable_path: program.to_owned(),
            ..ExternalTool::default()
        }
    }

    const fn everywhere(_: &str) -> bool {
        true
    }

    #[test]
    fn a_tool_added_is_applied_once_it_has_a_program() {
        let mut editor = ExternalToolsEditor::new(&[]);
        assert_eq!(editor.edit(ToolsEdit::Add, "New Tool", everywhere), None);
        assert_eq!(editor.problem(), Some(&ToolsProblem::Incomplete));
        assert_eq!(
            editor.selected().map(|t| t.arguments.as_str()),
            Some("{Host}")
        );
        let applied = editor.edit(
            ToolsEdit::Program(" winscp.exe ".to_owned()),
            "",
            everywhere,
        );
        let applied = applied.expect("applied");
        assert_eq!(applied[0].name, "New Tool");
        assert_eq!(applied[0].executable_path, "winscp.exe", "trimmed");
        assert_eq!(editor.problem(), None);
        // The settings now hold it: nothing shown again, what is typed stays.
        editor.sync(&applied);
        assert_eq!(
            editor.selected().map(|t| t.executable_path.as_str()),
            Some(" winscp.exe ")
        );
    }

    #[test]
    fn a_name_twice_or_a_program_missing_stays_edited_and_said() {
        let mut editor = ExternalToolsEditor::new(&[tool("WinSCP", "winscp.exe")]);
        let found = |program: &str| program != "gone.exe";
        editor.edit(ToolsEdit::Add, "winscp", found);
        assert_eq!(
            editor.edit(ToolsEdit::Program("x.exe".to_owned()), "", found),
            None
        );
        assert_eq!(
            editor.problem(),
            Some(&ToolsProblem::Duplicate("winscp".to_owned()))
        );
        editor.edit(ToolsEdit::Name("Putty".to_owned()), "", found);
        assert_eq!(
            editor.edit(ToolsEdit::Program("gone.exe".to_owned()), "", found),
            None
        );
        assert!(matches!(
            editor.problem(),
            Some(ToolsProblem::NotFound { .. })
        ));
    }

    #[test]
    fn the_tools_are_reordered_and_removed_the_chosen_one_following() {
        let tools = [tool("A", "a"), tool("B", "b"), tool("C", "c")];
        let mut editor = ExternalToolsEditor::new(&tools);
        assert_eq!(
            editor.edit(ToolsEdit::Up, "", everywhere),
            None,
            "none chosen"
        );
        editor.edit(ToolsEdit::Select(2), "", everywhere);
        let names = |tools: Option<Vec<ExternalTool>>| -> Vec<String> {
            tools
                .expect("applied")
                .into_iter()
                .map(|tool| tool.name)
                .collect()
        };
        assert_eq!(
            names(editor.edit(ToolsEdit::Up, "", everywhere)),
            ["A", "C", "B"]
        );
        assert_eq!(
            names(editor.edit(ToolsEdit::Up, "", everywhere)),
            ["C", "A", "B"]
        );
        assert_eq!(
            editor.edit(ToolsEdit::Up, "", everywhere),
            None,
            "already first"
        );
        assert_eq!(
            names(editor.edit(ToolsEdit::Down, "", everywhere)),
            ["A", "C", "B"]
        );
        assert_eq!(
            names(editor.edit(ToolsEdit::Remove, "", everywhere)),
            ["A", "B"]
        );
        assert_eq!(editor.selected(), None);
    }

    #[test]
    fn a_change_from_elsewhere_shows_again() {
        let mut editor = ExternalToolsEditor::new(&[tool("A", "a")]);
        editor.edit(ToolsEdit::Select(0), "", everywhere);
        editor.edit(ToolsEdit::Program(" ".to_owned()), "", everywhere);
        assert_eq!(editor.problem(), Some(&ToolsProblem::Incomplete));
        let imported = [tool("B", "b"), tool("A", "a")];
        editor.sync(&imported);
        assert_eq!(editor.problem(), None);
        assert_eq!(editor.selected(), Some(&imported[1]), "by its name");
        editor.sync(&[tool("C", "c")]);
        assert_eq!(editor.selected(), None, "gone");
    }

    #[test]
    fn a_path_picked_fills_the_program_or_the_folder_of_the_tool_chosen() {
        let mut editor = ExternalToolsEditor::new(&[tool("A", "a")]);
        editor.edit(ToolsEdit::Select(0), "", everywhere);
        let applied = editor
            .picked(true, "/tmp".to_owned(), everywhere)
            .expect("applied");
        assert_eq!(applied[0].working_directory, "/tmp");
        let applied = editor
            .picked(false, "/usr/bin/b".to_owned(), everywhere)
            .expect("applied");
        assert_eq!(applied[0].executable_path, "/usr/bin/b");
    }

    #[test]
    fn every_failure_and_problem_is_said() {
        for failure in [
            ToolFailure::ShellTarget,
            ToolFailure::NotFound("x".to_owned()),
            ToolFailure::InvalidProgram("x".to_owned()),
            ToolFailure::OptionLike(Placeholder::Host),
            ToolFailure::WorkingDirectory("x".to_owned()),
            ToolFailure::InvalidArguments,
            ToolFailure::ElevationUnsupported,
            ToolFailure::NotStarted("denied".to_owned()),
        ] {
            assert!(!failure_text(&failure).is_empty(), "{failure:?}");
        }
        for problem in [
            ToolsProblem::Incomplete,
            ToolsProblem::Duplicate("x".to_owned()),
            ToolsProblem::NotFound {
                name: "x".to_owned(),
                program: "y".to_owned(),
            },
        ] {
            assert!(!problem_text(&problem).is_empty(), "{problem:?}");
        }
        for placeholder in Placeholder::ALL {
            assert!(!placeholder_text(placeholder).is_empty());
        }
    }
}
