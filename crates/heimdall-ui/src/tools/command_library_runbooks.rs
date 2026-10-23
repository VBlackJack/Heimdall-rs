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

//! The runbooks panel of the command library: the list of the user's runbooks with New, Edit,
//! Duplicate and Delete, and the editor that adds, orders and removes steps, as the C#
//! `CommandLibraryViewModel.Runbooks.cs` and `RunbookDialogViewModel.cs`. A runbook is user
//! data only and none ships with Heimdall. A run is manual and step by step: see [`run`].

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use heimdall_app::TabId;
use heimdall_core::tools::command_library::{self, CatalogueEntry};
use heimdall_core::tools::runbooks::{
    self, FailureMode, KeepError, MAX_STEPS, MAX_TAG_LENGTH, MAX_TAGS, MAX_VARIABLES,
    RUNBOOKS_FILE, Runbook, RunbookIssue, RunbookStore, Step, StepIssue,
};
use heimdall_core::tools::user_commands::{
    MAX_COMMAND_LENGTH, MAX_DESCRIPTION_LENGTH, MAX_PARAMETER_LENGTH, MAX_TITLE_LENGTH,
    TemplatePlatform, UserCommand, UserLibrary, UserParameter,
};
use iced::widget::{Column, Row, button, column, container, row, text, text_input};
use iced::{Element, Length};

use self::run::{Run, RunSession};
use super::ToolMessage;
use super::command_library_tool::{CommandLibraryMessage, category_name, language, value_error};
use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, spacing};

/// The width of the labels of the editor's fields.
const LABEL_WIDTH: f32 = 140.0;

/// The width of the name of a variable in the editor.
const VARIABLE_NAME_WIDTH: f32 = 180.0;

/// The most actions the picker offers for one search.
const PICK_RESULTS: usize = 8;

/// What the panel is asked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunbookMessage {
    /// Open or close the panel.
    Toggle,
    /// Start a new runbook.
    New,
    /// Edit the runbook of that identity.
    Edit(String),
    /// Make a copy of the runbook of that identity.
    Duplicate(String),
    /// Ask whether to delete the runbook of that identity.
    AskDelete(String),
    /// Start a run of the runbook of that identity.
    Run(String),
    /// The value typed for variable `.0` of the run.
    RunValue(usize, String),
    /// Confirm the variables of the run and show its steps.
    RunBegin,
    /// Send the current step to the terminal chosen, after the question.
    RunSend,
    /// Type the current step at the prompt of the terminal chosen.
    RunInsert,
    /// Copy the current step.
    RunCopy,
    /// Pass over the current step.
    RunSkip,
    /// Leave the run.
    RunStop,
    /// Delete it.
    ConfirmDelete(String),
    /// Keep it.
    CancelDelete,
    /// The name typed.
    Name(String),
    /// The description typed.
    Description(String),
    /// The failure mode chosen.
    Mode(FailureMode),
    /// Add an empty variable.
    AddVariable,
    /// The name typed for variable `.0`.
    VariableName(usize, String),
    /// The default typed for variable `.0`.
    VariableValue(usize, String),
    /// Remove variable `.0`.
    RemoveVariable(usize),
    /// The search typed to find an action to add.
    PickSearch(String),
    /// Add the library action of that identity as the last step.
    AddAction(String),
    /// Add an empty literal command as the last step.
    AddLiteral,
    /// Move step `.0` one place up.
    MoveUp(usize),
    /// Move step `.0` one place down.
    MoveDown(usize),
    /// Remove step `.0`.
    RemoveStep(usize),
    /// The value typed for parameter `.1` of step `.0`.
    Param(usize, usize, String),
    /// The command typed for the literal step `.0`.
    Command(usize, String),
    /// The platform chosen for the literal step `.0`.
    Platform(usize, TemplatePlatform),
    /// The note typed for step `.0`.
    Note(usize, String),
    /// Keep the runbook of the editor.
    Save,
    /// Leave the editor.
    Cancel,
}

/// What went wrong with the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Failure {
    /// It could not be read: it is left as it is and nothing is changed.
    Unreadable,
    /// The change could not be saved.
    Unsaved,
}

/// Why the editor's runbook was not kept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Problem {
    /// A variable has a name that is not one, or two have the same.
    VariableName(String),
    /// A rule of the runbook is broken.
    Issue(RunbookIssue),
    /// The file holds as many runbooks as it can.
    Full,
    /// The runbook being edited is no longer in the file: another window deleted it.
    DeletedElsewhere,
    /// The file could not be written.
    Unsaved,
}

/// A value of a step, for one parameter of its action.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Field {
    name: String,
    label: String,
    value: String,
}

/// A step in the editor.
#[derive(Debug, Clone, PartialEq, Eq)]
enum EditorStep {
    Action {
        action_id: String,
        title: String,
        /// Whether the library still has the action.
        found: bool,
        fields: Vec<Field>,
        note: String,
    },
    Literal {
        command: String,
        platform: TemplatePlatform,
        note: String,
    },
}

/// The editor: the runbook being made or changed.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Editor {
    /// The identity of the runbook; new for a new one.
    public_id: String,
    /// Whether the runbook is kept already.
    existing: bool,
    name: String,
    description: String,
    mode: FailureMode,
    variables: Vec<(String, String)>,
    steps: Vec<EditorStep>,
    tags: Vec<String>,
    search: String,
    problem: Option<Problem>,
}

/// The panel's state.
#[derive(Debug, Default)]
pub struct RunbooksPane {
    open: bool,
    /// Where the runbooks are kept; `None` when the platform has no folder.
    file: Option<PathBuf>,
    store: RunbookStore,
    failure: Option<Failure>,
    editor: Option<Editor>,
    deleting: Option<String>,
    /// The run in progress.
    run: Option<Run>,
    /// What the session the library was opened for gives a run.
    session: RunSession,
    /// How many runs were started, which gives each its identity.
    runs_started: u64,
}

/// The action of an identity: a command of the catalogue, else one of the user's.
fn lookup<'l>(library: &'l UserLibrary) -> impl Fn(&str) -> Option<&'l UserCommand> + 'l {
    move |id| {
        command_library::shipped()
            .get(id)
            .map(CatalogueEntry::command)
            .or_else(|| library.get(id))
    }
}

/// The parameters an action declares, Windows first, a name both declare listed once, as
/// the C# `FillParameters`.
fn declared_parameters(command: &UserCommand) -> Vec<&UserParameter> {
    let mut declared: Vec<&UserParameter> = Vec::new();
    for platform in [TemplatePlatform::Windows, TemplatePlatform::Linux] {
        for parameter in command
            .template(platform)
            .map_or(&[][..], |template| template.parameters.as_slice())
        {
            if !parameter.name.is_empty()
                && !declared.iter().any(|known| known.name == parameter.name)
            {
                declared.push(parameter);
            }
        }
    }
    declared
}

/// The action of an identity as the panel shows it: the catalogue's in the language of the
/// window (its title and the labels of its values, as the library's own cards), else the
/// user's. The lookup the model validates with has no language, so what is shown never comes
/// from it.
fn shown_action<'l>(library: &'l UserLibrary, id: &str) -> Option<Cow<'l, UserCommand>> {
    shown_action_in(library, id, &language())
}

/// [`shown_action`] in `language`.
fn shown_action_in<'l>(
    library: &'l UserLibrary,
    id: &str,
    language: &str,
) -> Option<Cow<'l, UserCommand>> {
    if let Some(entry) = command_library::shipped().get(id) {
        return Some(entry.localized(language));
    }
    library.get(id).map(Cow::Borrowed)
}

/// The fields of a step: one per declared parameter, holding what the step stores or else
/// the default, then what the step stores for a parameter the action no longer declares (kept,
/// so an edit of the action that brings it back finds it again).
fn fields_of(command: Option<&UserCommand>, stored: &BTreeMap<String, String>) -> Vec<Field> {
    let mut fields: Vec<Field> = command
        .map(declared_parameters)
        .unwrap_or_default()
        .into_iter()
        .map(|parameter| Field {
            name: parameter.name.clone(),
            label: if parameter.label.trim().is_empty() {
                parameter.name.clone()
            } else {
                parameter.label.clone()
            },
            value: stored
                .get(&parameter.name)
                .cloned()
                .or_else(|| parameter.default_value.clone())
                .unwrap_or_default(),
        })
        .collect();
    for (name, value) in stored {
        if !fields.iter().any(|field| &field.name == name) {
            fields.push(Field {
                name: name.clone(),
                label: name.clone(),
                value: value.clone(),
            });
        }
    }
    fields
}

impl Editor {
    fn blank() -> Self {
        let fresh = Runbook::new("");
        Self {
            public_id: fresh.public_id,
            existing: false,
            name: String::new(),
            description: String::new(),
            mode: FailureMode::Stop,
            variables: Vec::new(),
            steps: Vec::new(),
            tags: Vec::new(),
            search: String::new(),
            problem: None,
        }
    }

    fn of(runbook: &Runbook, library: &UserLibrary) -> Self {
        Self {
            public_id: runbook.public_id.clone(),
            existing: true,
            name: runbook.name.clone(),
            description: runbook.description.clone().unwrap_or_default(),
            mode: runbook.failure_mode,
            variables: runbook
                .variables
                .iter()
                .map(|(name, value)| (name.clone(), value.clone()))
                .collect(),
            steps: runbook
                .steps
                .iter()
                .map(|step| match step {
                    Step::Action {
                        action_id,
                        action_title,
                        parameters,
                        description,
                    } => {
                        let command = shown_action(library, action_id);
                        EditorStep::Action {
                            action_id: action_id.clone(),
                            title: command
                                .as_ref()
                                .map_or_else(|| action_title.clone(), |each| each.title.clone()),
                            found: command.is_some(),
                            fields: fields_of(command.as_deref(), parameters),
                            note: description.clone().unwrap_or_default(),
                        }
                    }
                    Step::Literal {
                        command,
                        platform,
                        description,
                    } => EditorStep::Literal {
                        command: command.clone(),
                        platform: *platform,
                        note: description.clone().unwrap_or_default(),
                    },
                })
                .collect(),
            tags: runbook.tags.clone(),
            search: String::new(),
            problem: None,
        }
    }

    /// The runbook the editor describes.
    ///
    /// # Errors
    ///
    /// [`Problem::VariableName`] for a name that is not a name or is taken twice.
    fn to_runbook(&self) -> Result<Runbook, Problem> {
        let mut variables = BTreeMap::new();
        for (name, value) in &self.variables {
            let name = name.trim();
            if name.is_empty() && value.is_empty() {
                continue;
            }
            if !runbooks::is_valid_variable_name(name) || variables.contains_key(name) {
                return Err(Problem::VariableName(name.to_owned()));
            }
            variables.insert(name.to_owned(), value.clone());
        }
        let note = |text: &str| {
            let trimmed = text.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_owned())
        };
        Ok(Runbook {
            public_id: self.public_id.clone(),
            name: self.name.trim().to_owned(),
            description: note(&self.description),
            failure_mode: self.mode,
            variables,
            tags: self.tags.clone(),
            steps: self
                .steps
                .iter()
                .map(|step| match step {
                    EditorStep::Action {
                        action_id,
                        title,
                        fields,
                        note: text,
                        ..
                    } => Step::Action {
                        action_id: action_id.clone(),
                        action_title: title.clone(),
                        parameters: fields
                            .iter()
                            .map(|field| (field.name.clone(), field.value.clone()))
                            .collect(),
                        description: note(text),
                    },
                    EditorStep::Literal {
                        command,
                        platform,
                        note: text,
                    } => Step::Literal {
                        command: command.trim().to_owned(),
                        platform: *platform,
                        description: note(text),
                    },
                })
                .collect(),
        })
    }

    /// The variables the steps refer to and the editor does not declare.
    fn undeclared(&self) -> Vec<String> {
        let mut names: Vec<String> = Vec::new();
        for step in &self.steps {
            if let EditorStep::Action { fields, .. } = step {
                for field in fields {
                    for name in runbooks::variable_references(&field.value) {
                        let declared = self.variables.iter().any(|(each, _)| each.trim() == name);
                        if !declared && !names.contains(&name) {
                            names.push(name);
                        }
                    }
                }
            }
        }
        names
    }
}

impl RunbooksPane {
    /// The panel over the runbooks kept beside `profiles_file`, read now. `library` is where
    /// the actions of the user are found.
    #[must_use]
    pub fn new(profiles_file: Option<&Path>, library: &UserLibrary) -> Self {
        let file = profiles_file
            .and_then(Path::parent)
            .map(|folder| folder.join(RUNBOOKS_FILE));
        let mut pane = Self {
            file,
            ..Self::default()
        };
        if let Some(path) = &pane.file {
            let find = lookup(library);
            match RunbookStore::load(path, &find) {
                Ok(store) => pane.store = store,
                Err(_) => pane.failure = Some(Failure::Unreadable),
            }
        }
        if pane.store.reidentified() {
            pane.keep_identities();
        }
        pane
    }

    /// The identity of the first runbook.
    #[cfg(test)]
    pub(in crate::tools) fn first_id(&self) -> Option<String> {
        self.store
            .runbooks()
            .first()
            .map(|runbook| runbook.public_id.clone())
    }

    /// Closes the panel.
    pub fn close(&mut self) {
        self.open = false;
    }

    /// Whether the panel is open.
    #[must_use]
    pub const fn is_open(&self) -> bool {
        self.open
    }

    /// Whether the runbooks can be changed: their file was read, and has a place.
    fn can_edit(&self) -> bool {
        self.failure != Some(Failure::Unreadable) && self.file.is_some()
    }

    /// Applies `message`; `library` is where the actions of the user are found.
    pub fn update(&mut self, message: RunbookMessage, library: &UserLibrary) {
        match message {
            RunbookMessage::Toggle => self.open = !self.open,
            RunbookMessage::New if self.can_edit() => self.editor = Some(Editor::blank()),
            RunbookMessage::Edit(id) if self.can_edit() => {
                if let Some(runbook) = self.store.get(&id) {
                    self.editor = Some(Editor::of(runbook, library));
                }
            }
            RunbookMessage::Duplicate(id) if self.can_edit() => {
                let name = self.store.get(&id).map(|runbook| {
                    fl!(
                        "ui-tool-cmdlib-runbooks-copy-name",
                        name = runbook.name.as_str()
                    )
                });
                if let Some(name) = name
                    && let Some(before) = self.refresh(library)
                    && self.store.duplicate(&id, &name).is_some()
                {
                    self.persist(before);
                }
            }
            RunbookMessage::AskDelete(id) if self.can_edit() => self.deleting = Some(id),
            RunbookMessage::ConfirmDelete(id) if self.can_edit() => {
                self.deleting = None;
                if let Some(before) = self.refresh(library)
                    && self.store.delete(&id)
                {
                    self.persist(before);
                }
            }
            RunbookMessage::CancelDelete => self.deleting = None,
            RunbookMessage::Run(id) => self.start_run(&id),
            RunbookMessage::RunValue(..)
            | RunbookMessage::RunBegin
            | RunbookMessage::RunSkip
            | RunbookMessage::RunStop => self.run_message(message),
            // Asked of a terminal: the library pane answers, it knows the terminals.
            RunbookMessage::RunSend | RunbookMessage::RunInsert | RunbookMessage::RunCopy => {}
            RunbookMessage::Save => self.save(library),
            RunbookMessage::Cancel => self.editor = None,
            other => {
                if let Some(editor) = self.editor.as_mut() {
                    edit(editor, other, library);
                }
            }
        }
    }

    /// Reads the runbooks again from their file, to change them on what is there now (another
    /// pane of the library may have written since this one read). The store as it was before,
    /// to go back to if the write fails; `None`, with the failure said, when the file cannot be
    /// read now: nothing is then changed.
    fn refresh(&mut self, library: &UserLibrary) -> Option<RunbookStore> {
        let path = self.file.clone()?;
        let find = lookup(library);
        match RunbookStore::load(&path, &find) {
            Ok(fresh) => {
                let before = std::mem::replace(&mut self.store, fresh);
                if self.store.reidentified() && !self.keep_identities() {
                    self.store = before;
                    return None;
                }
                Some(before)
            }
            Err(_) => {
                self.failure = Some(Failure::Unsaved);
                None
            }
        }
    }

    /// Writes the store back after a read that gave runbooks new identities, which are
    /// random: written once, they are the same at every later read. Whether it was written.
    fn keep_identities(&mut self) -> bool {
        let written = self
            .file
            .as_ref()
            .is_some_and(|path| self.store.save(path).is_ok());
        if !written {
            self.failure = Some(Failure::Unsaved);
        }
        written
    }

    /// Writes the store; when that fails it goes back to `before` and the failure is said.
    /// Whether it was written.
    fn persist(&mut self, before: RunbookStore) -> bool {
        let Some(path) = &self.file else {
            self.store = before;
            return false;
        };
        match self.store.save(path) {
            Ok(()) => {
                self.failure = None;
                true
            }
            Err(_) => {
                self.store = before;
                self.failure = Some(Failure::Unsaved);
                false
            }
        }
    }

    /// Keeps the runbook of the editor, or says why not.
    fn save(&mut self, library: &UserLibrary) {
        let Some(editor) = self.editor.as_ref() else {
            return;
        };
        let runbook = match editor.to_runbook() {
            Ok(runbook) => runbook,
            Err(problem) => {
                self.say(problem);
                return;
            }
        };
        // Another pane may have changed the file since this one read it: the change is made
        // on what is on disk now, so it cannot erase theirs.
        let Some(before) = self.refresh(library) else {
            self.say(Problem::Unsaved);
            return;
        };
        if self.editor.as_ref().is_some_and(|editor| editor.existing)
            && self.store.get(&runbook.public_id).is_none()
        {
            // Deleted from another window: not brought back unasked. The editor stays open
            // as a new runbook, and a second Save keeps it.
            if let Some(editor) = self.editor.as_mut() {
                editor.existing = false;
            }
            self.say(Problem::DeletedElsewhere);
            return;
        }
        let find = lookup(library);
        let kept = self.store.keep(runbook, &find);
        match kept {
            Ok(()) => {
                if self.persist(before) {
                    self.editor = None;
                } else {
                    self.say(Problem::Unsaved);
                }
            }
            Err(KeepError::Refused(issue)) => self.say(Problem::Issue(issue)),
            Err(KeepError::TooMany) => self.say(Problem::Full),
        }
    }

    fn say(&mut self, problem: Problem) {
        if let Some(editor) = self.editor.as_mut() {
            editor.problem = Some(problem);
        }
    }

    /// The panel, or nothing while it is closed; `has_terminal` says whether a terminal is
    /// offered to send a step to.
    pub fn view<'a>(
        &'a self,
        tab: TabId,
        library: &'a UserLibrary,
        has_terminal: bool,
        warning: Option<String>,
    ) -> Option<Element<'a, Message>> {
        if !self.open {
            return None;
        }
        if let Some(run) = self.run_view(tab, library, has_terminal, warning) {
            return Some(run);
        }
        Some(match &self.editor {
            Some(editor) => editor_view(tab, editor, library),
            None => self.list_view(tab),
        })
    }

    fn list_view<'a>(&'a self, tab: TabId) -> Element<'a, Message> {
        let send = move |message| send(tab, message);
        let mut panel: Column<'a, Message> = column![
            row![
                text(fl!("ui-tool-cmdlib-runbooks-title"))
                    .size(font_size::BODY_LARGE)
                    .font(styles::SEMIBOLD)
                    .width(Length::Fill),
                action(
                    fl!("ui-tool-cmdlib-runbooks-new"),
                    true,
                    self.can_edit().then(|| send(RunbookMessage::New)),
                ),
            ]
            .spacing(spacing::SM)
            .align_y(iced::Alignment::Center),
        ]
        .spacing(spacing::SM);
        if let Some(failure) = self.failure {
            panel = panel.push(
                text(match failure {
                    Failure::Unreadable => fl!("ui-tool-cmdlib-runbooks-unreadable"),
                    Failure::Unsaved => fl!("ui-tool-cmdlib-runbooks-unsaved"),
                })
                .size(font_size::BODY)
                .style(styles::error_text),
            );
        }
        if self.store.unreadable_count() > 0 {
            panel = panel.push(
                text(fl!(
                    "ui-tool-cmdlib-runbooks-skipped",
                    count = self.store.unreadable_count()
                ))
                .size(font_size::CAPTION)
                .style(text::secondary),
            );
        }
        if self.store.runbooks().is_empty() {
            panel = panel.push(
                text(fl!("ui-tool-cmdlib-runbooks-empty"))
                    .size(font_size::BODY)
                    .style(text::secondary),
            );
        }
        for runbook in self.store.runbooks() {
            panel = panel.push(self.entry_view(tab, runbook));
        }
        container(panel)
            .padding(spacing::MD)
            .width(Length::Fill)
            .style(styles::card)
            .into()
    }

    fn entry_view<'a>(&'a self, tab: TabId, runbook: &'a Runbook) -> Element<'a, Message> {
        let send = move |message| send(tab, message);
        let id = &runbook.public_id;
        let manage: Row<'a, Message> = if self.deleting.as_deref() == Some(id.as_str()) {
            row![
                text(fl!(
                    "ui-tool-cmdlib-runbooks-delete-question",
                    name = runbook.name.as_str()
                ))
                .size(font_size::BODY),
                button(text(fl!("ui-tool-cmdlib-delete")).size(font_size::BODY))
                    .padding(super::COPY_PADDING)
                    .style(styles::danger)
                    .on_press(send(RunbookMessage::ConfirmDelete(id.clone()))),
                action(
                    fl!("ui-tool-cmdlib-delete-keep"),
                    false,
                    Some(send(RunbookMessage::CancelDelete)),
                ),
            ]
        } else {
            row![
                action(
                    fl!("ui-tool-cmdlib-runbooks-run"),
                    true,
                    Some(send(RunbookMessage::Run(id.clone()))),
                ),
                action(
                    fl!("ui-tool-cmdlib-edit"),
                    false,
                    Some(send(RunbookMessage::Edit(id.clone()))),
                ),
                action(
                    fl!("ui-tool-cmdlib-runbooks-duplicate"),
                    false,
                    Some(send(RunbookMessage::Duplicate(id.clone()))),
                ),
                action(
                    fl!("ui-tool-cmdlib-delete"),
                    false,
                    Some(send(RunbookMessage::AskDelete(id.clone()))),
                ),
            ]
        }
        .spacing(spacing::SM)
        .align_y(iced::Alignment::Center);
        let mut about = column![
            text(runbook.name.as_str())
                .size(font_size::BODY_LARGE)
                .font(styles::SEMIBOLD),
            text(fl!(
                "ui-tool-cmdlib-runbooks-steps",
                count = runbook.steps.len()
            ))
            .size(font_size::CAPTION)
            .style(text::secondary),
        ]
        .spacing(spacing::XS)
        .width(Length::Fill);
        if let Some(description) = &runbook.description {
            about = about.push(text(description.as_str()).size(font_size::BODY));
        }
        container(
            row![about, manage]
                .spacing(spacing::MD)
                .align_y(iced::Alignment::Center),
        )
        .padding(spacing::SM)
        .width(Length::Fill)
        .style(styles::field_box)
        .into()
    }
}

/// The message to the window for a message of the panel.
fn send(tab: TabId, message: RunbookMessage) -> Message {
    Message::Tool(
        tab,
        ToolMessage::CmdLib(CommandLibraryMessage::Runbooks(message)),
    )
}

/// Applies a message about the editor's content.
fn edit(editor: &mut Editor, message: RunbookMessage, library: &UserLibrary) {
    // What was refused is said until the user changes something: the field fixed, the reason
    // goes with it instead of staying under a form that is now right.
    editor.problem = None;
    match message {
        RunbookMessage::Name(value) => editor.name = value,
        RunbookMessage::Description(value) => editor.description = value,
        RunbookMessage::Mode(mode) => editor.mode = mode,
        RunbookMessage::AddVariable if editor.variables.len() < MAX_VARIABLES => {
            editor.variables.push((String::new(), String::new()));
        }
        RunbookMessage::VariableName(index, value) => {
            if let Some(entry) = editor.variables.get_mut(index) {
                entry.0 = value;
            }
        }
        RunbookMessage::VariableValue(index, value) => {
            if let Some(entry) = editor.variables.get_mut(index) {
                entry.1 = value;
            }
        }
        RunbookMessage::RemoveVariable(index) if index < editor.variables.len() => {
            editor.variables.remove(index);
        }
        RunbookMessage::PickSearch(value) => editor.search = value,
        RunbookMessage::AddAction(id) if editor.steps.len() < MAX_STEPS => {
            if let Some(command) = shown_action(library, &id) {
                let step = EditorStep::Action {
                    title: command.title.clone(),
                    found: true,
                    fields: fields_of(Some(&command), &BTreeMap::new()),
                    note: String::new(),
                    action_id: command.id.clone(),
                };
                editor.steps.push(step);
            }
        }
        RunbookMessage::AddLiteral if editor.steps.len() < MAX_STEPS => {
            editor.steps.push(EditorStep::Literal {
                command: String::new(),
                platform: if cfg!(windows) {
                    TemplatePlatform::Windows
                } else {
                    TemplatePlatform::Linux
                },
                note: String::new(),
            });
        }
        RunbookMessage::MoveUp(index) if index > 0 && index < editor.steps.len() => {
            editor.steps.swap(index, index - 1);
        }
        RunbookMessage::MoveDown(index) if index + 1 < editor.steps.len() => {
            editor.steps.swap(index, index + 1);
        }
        RunbookMessage::RemoveStep(index) if index < editor.steps.len() => {
            editor.steps.remove(index);
        }
        RunbookMessage::Param(step, field, value) => {
            if let Some(EditorStep::Action { fields, .. }) = editor.steps.get_mut(step)
                && let Some(field) = fields.get_mut(field)
            {
                field.value = value;
            }
        }
        RunbookMessage::Command(step, value) => {
            if let Some(EditorStep::Literal { command, .. }) = editor.steps.get_mut(step) {
                *command = value;
            }
        }
        RunbookMessage::Platform(step, chosen) => {
            if let Some(EditorStep::Literal { platform, .. }) = editor.steps.get_mut(step) {
                *platform = chosen;
            }
        }
        RunbookMessage::Note(step, value) => match editor.steps.get_mut(step) {
            Some(EditorStep::Action { note, .. } | EditorStep::Literal { note, .. }) => {
                *note = value;
            }
            None => {}
        },
        _ => {}
    }
}

/// A button of the panel.
fn action<'a>(label: String, primary: bool, message: Option<Message>) -> Element<'a, Message> {
    button(text(label).size(font_size::BODY))
        .padding(super::COPY_PADDING)
        .style(if primary {
            styles::primary
        } else {
            styles::secondary
        })
        .on_press_maybe(message)
        .into()
}

/// A labelled text field.
fn field<'a>(
    label: String,
    value: &str,
    on_input: impl Fn(String) -> Message + 'a,
) -> Row<'a, Message> {
    row![
        text(label).size(font_size::BODY).width(LABEL_WIDTH),
        text_input("", value)
            .size(font_size::BODY)
            .padding(super::INPUT_PADDING)
            .style(styles::text_input)
            .on_input(on_input),
    ]
    .spacing(spacing::SM)
    .align_y(iced::Alignment::Center)
}

/// The name of the shell of a platform.
fn shell_name(platform: TemplatePlatform) -> String {
    match platform {
        TemplatePlatform::Windows => fl!("ui-tool-cmdlib-template-windows"),
        TemplatePlatform::Linux => fl!("ui-tool-cmdlib-template-linux"),
    }
}

/// The name of a failure mode.
fn mode_name(mode: FailureMode) -> String {
    match mode {
        FailureMode::Stop => fl!("ui-tool-cmdlib-runbooks-mode-stop"),
        FailureMode::Continue => fl!("ui-tool-cmdlib-runbooks-mode-continue"),
    }
}

/// The actions whose title, description, tags or command hold `query`, the user's first.
fn matches<'a>(library: &'a UserLibrary, query: &str) -> Vec<(String, String)> {
    if query.trim().is_empty() {
        return Vec::new();
    }
    let language = language();
    let mine = library
        .commands()
        .iter()
        .filter(|command| {
            command_library::matches_query(command, &category_name(&command.category), query)
        })
        .map(|command| (command.id.clone(), command.title.clone()));
    let shipped = command_library::shipped()
        .entries()
        .iter()
        .filter_map(|entry| {
            let shown = entry.localized(&language);
            command_library::matches_query(&shown, &category_name(&shown.category), query)
                .then(|| (shown.id.clone(), shown.title.clone()))
        });
    mine.chain(shipped).take(PICK_RESULTS).collect()
}

/// Why the runbook was not kept, in the language of the window.
fn problem_text(problem: &Problem) -> String {
    match problem {
        Problem::VariableName(name) => {
            fl!(
                "ui-tool-cmdlib-runbooks-err-variable-name",
                name = name.as_str()
            )
        }
        Problem::DeletedElsewhere => fl!("ui-tool-cmdlib-runbooks-err-deleted"),
        Problem::Full => fl!(
            "ui-tool-cmdlib-runbooks-err-full",
            max = runbooks::MAX_RUNBOOKS
        ),
        Problem::Unsaved => fl!("ui-tool-cmdlib-runbooks-unsaved"),
        Problem::Issue(issue) => match issue {
            RunbookIssue::NameMissing => fl!("ui-tool-cmdlib-runbooks-err-name"),
            RunbookIssue::NameTooLong => {
                fl!(
                    "ui-tool-cmdlib-runbooks-err-name-long",
                    max = MAX_TITLE_LENGTH
                )
            }
            RunbookIssue::TooManyVariables => {
                fl!(
                    "ui-tool-cmdlib-runbooks-err-variables-many",
                    max = MAX_VARIABLES
                )
            }
            RunbookIssue::VariableValueTooLong(name) => fl!(
                "ui-tool-cmdlib-runbooks-err-variable-long",
                name = name.as_str(),
                max = MAX_PARAMETER_LENGTH
            ),
            RunbookIssue::TooManySteps => {
                fl!("ui-tool-cmdlib-runbooks-err-steps-many", max = MAX_STEPS)
            }
            RunbookIssue::Step(number, step) => step_problem_text(*number, step),
            RunbookIssue::DescriptionTooLong => fl!(
                "ui-tool-cmdlib-runbooks-err-description-long",
                max = MAX_DESCRIPTION_LENGTH
            ),
            RunbookIssue::TooManyTags => {
                fl!("ui-tool-cmdlib-runbooks-err-tags-many", max = MAX_TAGS)
            }
            RunbookIssue::TagTooLong => {
                fl!("ui-tool-cmdlib-runbooks-err-tag-long", max = MAX_TAG_LENGTH)
            }
            RunbookIssue::Shape | RunbookIssue::Hidden => {
                fl!("ui-tool-cmdlib-runbooks-err-text")
            }
        },
    }
}

/// Why step `number` was refused.
fn step_problem_text(number: usize, issue: &StepIssue) -> String {
    match issue {
        StepIssue::CommandMissing => {
            fl!("ui-tool-cmdlib-runbooks-err-step-empty", number = number)
        }
        StepIssue::CommandTooLong => fl!(
            "ui-tool-cmdlib-runbooks-err-step-long",
            number = number,
            max = MAX_COMMAND_LENGTH
        ),
        StepIssue::Hidden => fl!("ui-tool-cmdlib-runbooks-err-step-hidden", number = number),
        StepIssue::ValueTooLong(name) => fl!(
            "ui-tool-cmdlib-runbooks-err-step-value-long",
            number = number,
            name = name.as_str(),
            max = MAX_PARAMETER_LENGTH
        ),
        StepIssue::Values(errors) => fl!(
            "ui-tool-cmdlib-runbooks-err-step-values",
            number = number,
            reason = errors.first().map(value_error).unwrap_or_default()
        ),
        StepIssue::ActionMissing
        | StepIssue::ActionTooLong
        | StepIssue::TitleTooLong
        | StepIssue::TooManyParameters
        | StepIssue::ParameterName(_) => {
            fl!("ui-tool-cmdlib-runbooks-err-step-other", number = number)
        }
        StepIssue::DescriptionTooLong => fl!(
            "ui-tool-cmdlib-runbooks-err-step-note-long",
            number = number,
            max = MAX_DESCRIPTION_LENGTH
        ),
    }
}

/// The editor of a runbook.
fn editor_view<'a>(
    tab: TabId,
    editor: &'a Editor,
    library: &'a UserLibrary,
) -> Element<'a, Message> {
    let send = move |message| send(tab, message);
    let mut form: Column<'a, Message> = column![
        text(if editor.existing {
            fl!("ui-tool-cmdlib-runbooks-editor-edit")
        } else {
            fl!("ui-tool-cmdlib-runbooks-editor-new")
        })
        .size(font_size::BODY_LARGE)
        .font(styles::SEMIBOLD),
        field(
            fl!("ui-tool-cmdlib-runbooks-field-name"),
            &editor.name,
            move |typed| { send(RunbookMessage::Name(typed)) }
        ),
        field(
            fl!("ui-tool-cmdlib-runbooks-field-description"),
            &editor.description,
            move |typed| send(RunbookMessage::Description(typed)),
        ),
        row![
            text(fl!("ui-tool-cmdlib-runbooks-field-mode"))
                .size(font_size::BODY)
                .width(LABEL_WIDTH),
            Row::with_children(FailureMode::ALL.into_iter().map(|mode| action(
                mode_name(mode),
                editor.mode == mode,
                Some(send(RunbookMessage::Mode(mode))),
            )))
            .spacing(spacing::SM),
        ]
        .spacing(spacing::SM)
        .align_y(iced::Alignment::Center),
    ]
    .spacing(spacing::SM);

    form = form.push(
        text(fl!("ui-tool-cmdlib-runbooks-variables"))
            .size(font_size::BODY)
            .font(styles::SEMIBOLD),
    );
    form = form.push(
        text(fl!("ui-tool-cmdlib-runbooks-variables-hint"))
            .size(font_size::CAPTION)
            .style(text::secondary),
    );
    form = form.push(
        text(fl!("ui-tool-cmdlib-runbooks-secrets-hint"))
            .size(font_size::CAPTION)
            .style(text::secondary),
    );
    for (index, (name, value)) in editor.variables.iter().enumerate() {
        form = form.push(
            row![
                text_input(&fl!("ui-tool-cmdlib-runbooks-variable-name"), name)
                    .size(font_size::BODY)
                    .padding(super::INPUT_PADDING)
                    .style(styles::text_input)
                    .width(VARIABLE_NAME_WIDTH)
                    .on_input(move |typed| send(RunbookMessage::VariableName(index, typed))),
                text_input(&fl!("ui-tool-cmdlib-runbooks-variable-default"), value)
                    .size(font_size::BODY)
                    .padding(super::INPUT_PADDING)
                    .style(styles::text_input)
                    .on_input(move |typed| send(RunbookMessage::VariableValue(index, typed))),
                action(
                    fl!("ui-tool-cmdlib-runbooks-remove"),
                    false,
                    Some(send(RunbookMessage::RemoveVariable(index))),
                ),
            ]
            .spacing(spacing::SM)
            .align_y(iced::Alignment::Center),
        );
    }
    form = form.push(action(
        fl!("ui-tool-cmdlib-runbooks-variable-add"),
        false,
        (editor.variables.len() < MAX_VARIABLES).then(|| send(RunbookMessage::AddVariable)),
    ));

    form = form.push(
        text(fl!("ui-tool-cmdlib-runbooks-steps-title"))
            .size(font_size::BODY)
            .font(styles::SEMIBOLD),
    );
    if editor.steps.is_empty() {
        form = form.push(
            text(fl!("ui-tool-cmdlib-runbooks-steps-empty"))
                .size(font_size::BODY)
                .style(text::secondary),
        );
    }
    for (index, step) in editor.steps.iter().enumerate() {
        form = form.push(step_view(tab, index, editor.steps.len(), step));
    }

    form = form.push(
        text_input(
            &fl!("ui-tool-cmdlib-runbooks-pick-placeholder"),
            &editor.search,
        )
        .size(font_size::BODY)
        .padding(super::INPUT_PADDING)
        .style(styles::text_input)
        .on_input(move |typed| send(RunbookMessage::PickSearch(typed))),
    );
    let found = matches(library, &editor.search);
    if found.is_empty() && !editor.search.trim().is_empty() {
        form = form.push(
            text(fl!("ui-tool-cmdlib-runbooks-pick-none"))
                .size(font_size::CAPTION)
                .style(text::secondary),
        );
    }
    let can_add = editor.steps.len() < MAX_STEPS;
    for (id, title) in found {
        form = form.push(action(
            fl!("ui-tool-cmdlib-runbooks-pick-add", title = title),
            false,
            can_add.then(|| send(RunbookMessage::AddAction(id))),
        ));
    }
    form = form.push(action(
        fl!("ui-tool-cmdlib-runbooks-literal-add"),
        false,
        can_add.then(|| send(RunbookMessage::AddLiteral)),
    ));

    let undeclared = editor.undeclared();
    if !undeclared.is_empty() {
        form = form.push(
            text(fl!(
                "ui-tool-cmdlib-runbooks-undeclared",
                names = undeclared.join(", ")
            ))
            .size(font_size::CAPTION)
            .style(text::secondary),
        );
    }
    if let Some(problem) = &editor.problem {
        form = form.push(
            text(problem_text(problem))
                .size(font_size::BODY)
                .style(styles::error_text),
        );
    }
    form = form.push(
        row![
            button(text(fl!("ui-tool-cmdlib-save")).size(font_size::BODY))
                .padding(super::PRIMARY_PADDING)
                .style(styles::primary)
                .on_press(send(RunbookMessage::Save)),
            button(text(fl!("ui-tool-cmdlib-cancel")).size(font_size::BODY))
                .padding(super::PRIMARY_PADDING)
                .style(styles::secondary)
                .on_press(send(RunbookMessage::Cancel)),
        ]
        .spacing(spacing::SM),
    );
    container(form)
        .padding(spacing::MD)
        .width(Length::Fill)
        .style(styles::card)
        .into()
}

/// One step of the editor, number `index` of `count`.
fn step_view<'a>(
    tab: TabId,
    index: usize,
    count: usize,
    step: &'a EditorStep,
) -> Element<'a, Message> {
    let send = move |message| send(tab, message);
    let controls = row![
        action(
            fl!("ui-tool-cmdlib-runbooks-move-up"),
            false,
            (index > 0).then(|| send(RunbookMessage::MoveUp(index))),
        ),
        action(
            fl!("ui-tool-cmdlib-runbooks-move-down"),
            false,
            (index + 1 < count).then(|| send(RunbookMessage::MoveDown(index))),
        ),
        action(
            fl!("ui-tool-cmdlib-runbooks-remove"),
            false,
            Some(send(RunbookMessage::RemoveStep(index))),
        ),
    ]
    .spacing(spacing::SM);
    let position = index + 1;
    let number = text(fl!(
        "ui-tool-cmdlib-runbooks-step-number",
        number = position
    ))
    .size(font_size::BODY)
    .font(styles::SEMIBOLD);
    let mut body: Column<'a, Message> = Column::new().spacing(spacing::SM);
    let note = match step {
        EditorStep::Action {
            title,
            found,
            fields,
            note,
            ..
        } => {
            body = body.push(text(title.as_str()).size(font_size::BODY_LARGE));
            if !found {
                body = body.push(
                    text(fl!("ui-tool-cmdlib-runbooks-step-missing"))
                        .size(font_size::CAPTION)
                        .style(styles::error_text),
                );
            } else if fields.is_empty() {
                body = body.push(
                    text(fl!("ui-tool-cmdlib-runbooks-step-no-values"))
                        .size(font_size::CAPTION)
                        .style(text::secondary),
                );
            }
            for (position, parameter) in fields.iter().enumerate() {
                body = body.push(field(
                    parameter.label.clone(),
                    &parameter.value,
                    move |typed| send(RunbookMessage::Param(index, position, typed)),
                ));
            }
            note
        }
        EditorStep::Literal {
            command,
            platform,
            note,
        } => {
            body = body.push(
                row![
                    text(fl!("ui-tool-cmdlib-runbooks-step-literal"))
                        .size(font_size::BODY)
                        .width(LABEL_WIDTH),
                    Row::with_children(
                        [TemplatePlatform::Windows, TemplatePlatform::Linux]
                            .into_iter()
                            .map(|choice| action(
                                shell_name(choice),
                                *platform == choice,
                                Some(send(RunbookMessage::Platform(index, choice))),
                            )),
                    )
                    .spacing(spacing::SM),
                ]
                .spacing(spacing::SM)
                .align_y(iced::Alignment::Center),
            );
            body = body.push(field(
                fl!("ui-tool-cmdlib-runbooks-step-command"),
                command,
                move |typed| send(RunbookMessage::Command(index, typed)),
            ));
            note
        }
    };
    body = body.push(field(
        fl!("ui-tool-cmdlib-runbooks-step-note"),
        note,
        move |typed| send(RunbookMessage::Note(index, typed)),
    ));
    container(
        column![
            row![number.width(Length::Fill), controls]
                .spacing(spacing::SM)
                .align_y(iced::Alignment::Center),
            body,
        ]
        .spacing(spacing::SM),
    )
    .padding(spacing::SM)
    .width(Length::Fill)
    .style(styles::field_box)
    .into()
}

mod run;

#[cfg(test)]
pub(in crate::tools) use self::run::StepStatus;
pub use self::run::{Delivered, Ready, RunTarget};

#[cfg(test)]
mod tests;
