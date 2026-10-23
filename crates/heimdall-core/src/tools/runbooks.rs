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

//! Runbooks of the command library: ordered steps the user chains instead of sending them
//! one by one on every incident.
//!
//! The C# keeps a runbook as a `CommandBatch` and moves it as `RunbookTransferModel`
//! (`RunbookTransferModel.cs`, the optional `runbooks` array of the library envelope). The
//! file written here holds that same array under the same keys, so the export and import
//! added later can hand runbooks to the C# and read theirs. A step is either a library
//! action (catalogue or user, by identity) with a value for each parameter, or a literal
//! command for one platform. A value written `{{name}}` names a run variable, asked once
//! when the run starts (`RunbookVariables.cs`).
//!
//! Nothing here runs a command. A runbook file is untrusted when it is read: every text is
//! held to the rules of the command library (no control character, none that shows nothing,
//! lengths and counts bounded) and every value of an action step goes through the same
//! generator checks as a value typed in the library. A runbook that breaks a rule is not
//! used and not lost: it is kept as it was read and written back unchanged.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::ops::Range;
use std::path::Path;

use serde_json::{Map, Value};

use super::private_file::write_private;
use super::user_commands::{
    self, Criticality, GenerateError, MAX_COMMAND_LENGTH, MAX_DESCRIPTION_LENGTH, MAX_FILE_BYTES,
    MAX_PARAMETER_LENGTH, MAX_PARAMETERS, MAX_TITLE_LENGTH, TemplatePlatform, UserCommand,
    ValueError,
};
use super::uuid_generator::{self, UuidFormat, UuidVersion};
use crate::invisible::is_invisible;
use crate::profile::RemoteOs;

/// The file of the runbooks, beside the file of the user's commands.
pub const RUNBOOKS_FILE: &str = "runbooks.json";

/// The version of the file, as the `schemaVersion` of the C# envelope. A file of another
/// major version is left alone.
pub const SCHEMA_VERSION: &str = "1.0";

/// The most runbooks a file may hold, those kept unread included: a file of more is refused.
pub const MAX_RUNBOOKS: usize = 200;

/// The most steps of a runbook.
pub const MAX_STEPS: usize = 100;

/// The most variables of a runbook.
pub const MAX_VARIABLES: usize = 50;

/// The longest name of a variable.
pub const MAX_VARIABLE_NAME_LENGTH: usize = 64;

/// The longest identity of a library action.
pub const MAX_ACTION_ID_LENGTH: usize = 200;

/// The most tags of a runbook.
pub const MAX_TAGS: usize = 20;

/// The longest tag.
pub const MAX_TAG_LENGTH: usize = 100;

/// The longest parameter name of a step.
pub const MAX_PARAMETER_NAME_LENGTH: usize = 100;

/// The longest identity of a runbook, as a GUID with its hyphens.
const MAX_PUBLIC_ID_LENGTH: usize = 36;

/// The length of a GUID written without hyphens.
const GUID_PLAIN_LENGTH: usize = 32;

/// The lengths of the hyphen-separated groups of a GUID.
const GUID_GROUPS: [usize; 5] = [8, 4, 4, 4, 12];

/// The major version of [`SCHEMA_VERSION`], the one this build reads.
const SUPPORTED_MAJOR: &str = "1";

/// What separates the major version from the rest in `schemaVersion`.
const VERSION_SEPARATOR: char = '.';

/// What opens a reference to a variable.
const REFERENCE_OPEN: &str = "{{";

/// What closes a reference to a variable.
const REFERENCE_CLOSE: &str = "}}";

/// The wire names, as the C# enums write them.
const MODE_STOP: &str = "StopOnError";
const MODE_CONTINUE: &str = "ContinueOnError";
const PLATFORM_WINDOWS: &str = "Windows";
const PLATFORM_LINUX: &str = "Linux";
const PLATFORM_BOTH: &str = "Both";

/// The keys of the envelope and of a runbook, as `RunbookTransferModel` names them.
mod key {
    pub const SCHEMA_VERSION: &str = "schemaVersion";
    pub const RUNBOOKS: &str = "runbooks";
    pub const PUBLIC_ID: &str = "publicId";
    pub const NAME: &str = "name";
    pub const DESCRIPTION: &str = "description";
    pub const EXECUTION_MODE: &str = "executionMode";
    pub const VARIABLES: &str = "variables";
    pub const TAGS: &str = "tags";
    pub const STEPS: &str = "steps";
    pub const ACTION_ID: &str = "actionId";
    pub const ACTION_TITLE: &str = "actionTitle";
    pub const COMMAND: &str = "command";
    pub const PLATFORM: &str = "platform";
    pub const PARAMETERS: &str = "parameters";
}

/// What the library looks an action up by: a catalogue command or a command of the user.
pub type ActionLookup<'c> = dyn Fn(&str) -> Option<&'c UserCommand> + 'c;

/// What a run does when a step fails. Stored and shown; the run itself is manual.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FailureMode {
    /// Stop the run.
    #[default]
    Stop,
    /// Carry on with the next step.
    Continue,
}

impl FailureMode {
    /// Both modes, in the order the editor offers them.
    pub const ALL: [Self; 2] = [Self::Stop, Self::Continue];

    const fn wire(self) -> &'static str {
        match self {
            Self::Stop => MODE_STOP,
            Self::Continue => MODE_CONTINUE,
        }
    }

    /// The mode of a name as the C# reads it; stop for anything else.
    fn from_wire(name: Option<&str>) -> Self {
        match name {
            Some(name) if name.trim().eq_ignore_ascii_case(MODE_CONTINUE) => Self::Continue,
            _ => Self::Stop,
        }
    }
}

/// One step of a runbook.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// A library action with a value for each of its parameters.
    Action {
        /// The identity of the action in the catalogue or in the user's commands.
        action_id: String,
        /// Its title when the step was made, shown when the action is gone.
        action_title: String,
        /// The value of each parameter, by name; a value may hold `{{name}}` references.
        parameters: BTreeMap<String, String>,
        /// A note on the step.
        description: Option<String>,
    },
    /// A command typed as it is, for one platform.
    Literal {
        /// The command, one line.
        command: String,
        /// The shell it is written for.
        platform: TemplatePlatform,
        /// A note on the step.
        description: Option<String>,
    },
}

impl Step {
    /// A step for the action `action_id` with `parameters`.
    #[must_use]
    pub fn action(
        action_id: &str,
        action_title: &str,
        parameters: BTreeMap<String, String>,
    ) -> Self {
        Self::Action {
            action_id: action_id.to_owned(),
            action_title: action_title.to_owned(),
            parameters,
            description: None,
        }
    }

    /// A literal step.
    #[must_use]
    pub fn literal(command: &str, platform: TemplatePlatform) -> Self {
        Self::Literal {
            command: command.to_owned(),
            platform,
            description: None,
        }
    }

    /// The note on the step.
    #[must_use]
    pub fn description(&self) -> Option<&str> {
        match self {
            Self::Action { description, .. } | Self::Literal { description, .. } => {
                description.as_deref()
            }
        }
    }
}

/// A runbook.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Runbook {
    /// Its identity, a GUID; what an import merges by.
    pub public_id: String,
    /// Its name.
    pub name: String,
    /// What it is for.
    pub description: Option<String>,
    /// What a run does when a step fails.
    pub failure_mode: FailureMode,
    /// The variables with their default values.
    pub variables: BTreeMap<String, String>,
    /// Its tags.
    pub tags: Vec<String>,
    /// Its steps, in run order.
    pub steps: Vec<Step>,
}

/// Why a step is refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StepIssue {
    /// The action has no identity.
    ActionMissing,
    /// The identity of the action is longer than [`MAX_ACTION_ID_LENGTH`].
    ActionTooLong,
    /// The title of the action is longer than [`MAX_TITLE_LENGTH`].
    TitleTooLong,
    /// More parameters than [`MAX_PARAMETERS`].
    TooManyParameters,
    /// A parameter has no valid name.
    ParameterName(String),
    /// A value is longer than [`MAX_PARAMETER_LENGTH`].
    ValueTooLong(String),
    /// A text holds a control character or one that shows nothing.
    Hidden,
    /// The literal command is empty.
    CommandMissing,
    /// The literal command is longer than [`MAX_COMMAND_LENGTH`].
    CommandTooLong,
    /// The note is longer than [`MAX_DESCRIPTION_LENGTH`].
    DescriptionTooLong,
    /// The generator refuses the values for every template of the action.
    Values(Vec<ValueError>),
}

/// Why a runbook is refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunbookIssue {
    /// It is not an object.
    Shape,
    /// It has no name.
    NameMissing,
    /// The name is longer than [`MAX_TITLE_LENGTH`].
    NameTooLong,
    /// The description is longer than [`MAX_DESCRIPTION_LENGTH`].
    DescriptionTooLong,
    /// A text holds a control character or one that shows nothing.
    Hidden,
    /// More tags than [`MAX_TAGS`].
    TooManyTags,
    /// A tag is longer than [`MAX_TAG_LENGTH`].
    TagTooLong,
    /// More variables than [`MAX_VARIABLES`].
    TooManyVariables,
    /// A default value is longer than [`MAX_PARAMETER_LENGTH`].
    VariableValueTooLong(String),
    /// More steps than [`MAX_STEPS`].
    TooManySteps,
    /// The step of that number (from 1) is refused.
    Step(usize, StepIssue),
}

/// A character a command must not hold: a control character, which would submit the line or
/// move the cursor, or one that shows nothing, which would make the command read as
/// something else than it is.
#[must_use]
pub fn is_hidden(c: char) -> bool {
    c.is_control() || is_invisible(c)
}

/// Whether `text` holds a [`is_hidden`] character.
fn holds_hidden(text: &str) -> bool {
    text.chars().any(is_hidden)
}

/// Like [`holds_hidden`] for a note, which may hold line breaks and tabs.
fn note_holds_hidden(text: &str) -> bool {
    text.chars()
        .any(|c| is_hidden(c) && !matches!(c, '\n' | '\r' | '\t'))
}

/// Whether `name` can be declared as a variable: a letter or an underscore, then letters,
/// digits or underscores, as the C# `RunbookVariables.IsValidName`.
#[must_use]
pub fn is_valid_variable_name(name: &str) -> bool {
    name.len() <= MAX_VARIABLE_NAME_LENGTH && !name.is_empty() && name_length(name) == name.len()
}

/// The text a reference to the variable `name` is written as.
#[must_use]
pub fn format_reference(name: &str) -> String {
    format!("{REFERENCE_OPEN}{name}{REFERENCE_CLOSE}")
}

/// Every reference `{{name}}` of `text`, as its byte range and the name it holds, left to
/// right. Nothing else is read: no expression, no default, no escape.
fn references(text: &str) -> Vec<(Range<usize>, &str)> {
    let mut found = Vec::new();
    let mut at = 0;
    while let Some(offset) = text[at..].find(REFERENCE_OPEN) {
        let start = at + offset;
        let name_start = start + REFERENCE_OPEN.len();
        let rest = &text[name_start..];
        let name_len = name_length(rest);
        if name_len > 0 && rest[name_len..].starts_with(REFERENCE_CLOSE) {
            let end = name_start + name_len + REFERENCE_CLOSE.len();
            found.push((start..end, &rest[..name_len]));
            at = end;
        } else {
            at = start + 1;
        }
    }
    found
}

/// The length in bytes of the name a text starts with; 0 when it starts with none.
fn name_length(text: &str) -> usize {
    let mut length = 0;
    for (index, c) in text.char_indices() {
        let allowed = if index == 0 {
            c.is_ascii_alphabetic() || c == '_'
        } else {
            c.is_ascii_alphanumeric() || c == '_'
        };
        if !allowed {
            break;
        }
        length = index + c.len_utf8();
    }
    length
}

/// The variables `text` refers to, each once, in the order they first appear.
#[must_use]
pub fn variable_references(text: &str) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for (_, name) in references(text) {
        if !names.iter().any(|known| known == name) {
            names.push(name.to_owned());
        }
    }
    names
}

/// `text` with every reference replaced by the value of the variable of that name; a
/// reference to a variable `variables` does not hold is left as written and its name added
/// to `unknown`, so the step shows an error instead of sending text nobody meant.
#[must_use]
pub fn substitute(
    text: &str,
    variables: &BTreeMap<String, String>,
    unknown: &mut Vec<String>,
) -> String {
    let mut result = String::with_capacity(text.len());
    let mut copied = 0;
    for (range, name) in references(text) {
        result.push_str(&text[copied..range.start]);
        if let Some(value) = variables.get(name) {
            result.push_str(value);
        } else {
            result.push_str(&text[range.clone()]);
            if !unknown.iter().any(|known| known == name) {
                unknown.push(name.to_owned());
            }
        }
        copied = range.end;
    }
    result.push_str(&text[copied..]);
    result
}

impl Runbook {
    /// A new runbook of `name` with a new identity.
    #[must_use]
    pub fn new(name: &str) -> Self {
        Self {
            public_id: new_identity(),
            name: name.trim().to_owned(),
            ..Self::default()
        }
    }

    /// The variables the steps refer to, each once, in the order they first appear.
    #[must_use]
    pub fn referenced_variables(&self) -> Vec<String> {
        let mut names: Vec<String> = Vec::new();
        for step in &self.steps {
            if let Step::Action { parameters, .. } = step {
                for value in parameters.values() {
                    for name in variable_references(value) {
                        if !names.contains(&name) {
                            names.push(name);
                        }
                    }
                }
            }
        }
        names
    }

    /// The variables the steps refer to and the runbook does not declare.
    #[must_use]
    pub fn undeclared_variables(&self) -> Vec<String> {
        self.referenced_variables()
            .into_iter()
            .filter(|name| !self.variables.contains_key(name))
            .collect()
    }

    /// The runbook of a value read from a file, as the C# `ToRunbook` makes it: an unreadable
    /// failure mode becomes stop, a variable whose name is not a name is dropped (no step
    /// could refer to it), an unreadable identity is replaced. The result is not yet checked:
    /// see [`Runbook::validate`].
    ///
    /// # Errors
    ///
    /// [`RunbookIssue::Shape`] when `value` is not an object, and the step that names no
    /// platform for a literal command.
    pub fn from_value(value: &Value) -> Result<Self, RunbookIssue> {
        let object = value.as_object().ok_or(RunbookIssue::Shape)?;
        let text = |field: &str| object.get(field).and_then(Value::as_str);
        let public_id = text(key::PUBLIC_ID)
            .map(str::trim)
            .filter(|id| is_identity(id))
            .map_or_else(new_identity, str::to_ascii_lowercase);
        let variables = object
            .get(key::VARIABLES)
            .and_then(Value::as_object)
            .map(|entries| {
                entries
                    .iter()
                    .filter(|(name, _)| is_valid_variable_name(name))
                    .map(|(name, value)| {
                        (name.clone(), value.as_str().unwrap_or_default().to_owned())
                    })
                    .collect()
            })
            .unwrap_or_default();
        let tags = object
            .get(key::TAGS)
            .and_then(Value::as_array)
            .map(|tags| {
                tags.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default();
        let mut steps = Vec::new();
        if let Some(listed) = object.get(key::STEPS).and_then(Value::as_array) {
            for step in listed.iter().filter(|each| !each.is_null()) {
                steps.push(step_from_value(step, steps.len() + 1)?);
            }
        }
        Ok(Self {
            public_id,
            name: text(key::NAME).unwrap_or_default().trim().to_owned(),
            description: note_of(text(key::DESCRIPTION)),
            failure_mode: FailureMode::from_wire(text(key::EXECUTION_MODE)),
            variables,
            tags,
            steps,
        })
    }

    /// The runbook as `RunbookTransferModel` writes it.
    #[must_use]
    pub fn to_value(&self) -> Value {
        let mut object = Map::new();
        object.insert(
            key::PUBLIC_ID.to_owned(),
            Value::from(self.public_id.clone()),
        );
        object.insert(key::NAME.to_owned(), Value::from(self.name.clone()));
        object.insert(
            key::DESCRIPTION.to_owned(),
            self.description.clone().map_or(Value::Null, Value::from),
        );
        object.insert(
            key::EXECUTION_MODE.to_owned(),
            Value::from(self.failure_mode.wire()),
        );
        object.insert(key::VARIABLES.to_owned(), map_value(&self.variables));
        object.insert(key::TAGS.to_owned(), Value::from(self.tags.clone()));
        object.insert(
            key::STEPS.to_owned(),
            Value::Array(self.steps.iter().map(step_to_value).collect()),
        );
        Value::Object(object)
    }

    /// Checks the runbook as an untrusted file is checked: counts and lengths, no control or
    /// hidden character in any text, a command on every literal step, and for every action
    /// step the values the generator would refuse for the action, when `lookup` finds it. A
    /// value that refers to a variable is judged when the run substitutes it, not here.
    ///
    /// # Errors
    ///
    /// The first rule broken, as a [`RunbookIssue`].
    pub fn validate(&self, lookup: &ActionLookup<'_>) -> Result<(), RunbookIssue> {
        self.validate_header()?;
        if self.steps.len() > MAX_STEPS {
            return Err(RunbookIssue::TooManySteps);
        }
        for (index, step) in self.steps.iter().enumerate() {
            validate_step(step, lookup).map_err(|issue| RunbookIssue::Step(index + 1, issue))?;
        }
        Ok(())
    }

    /// The name, the description, the tags and the variables.
    fn validate_header(&self) -> Result<(), RunbookIssue> {
        if self.name.trim().is_empty() {
            return Err(RunbookIssue::NameMissing);
        }
        if self.name.chars().count() > MAX_TITLE_LENGTH {
            return Err(RunbookIssue::NameTooLong);
        }
        if holds_hidden(&self.name) {
            return Err(RunbookIssue::Hidden);
        }
        if let Some(description) = &self.description {
            if description.chars().count() > MAX_DESCRIPTION_LENGTH {
                return Err(RunbookIssue::DescriptionTooLong);
            }
            if note_holds_hidden(description) {
                return Err(RunbookIssue::Hidden);
            }
        }
        if self.tags.len() > MAX_TAGS {
            return Err(RunbookIssue::TooManyTags);
        }
        for tag in &self.tags {
            if tag.chars().count() > MAX_TAG_LENGTH {
                return Err(RunbookIssue::TagTooLong);
            }
            if holds_hidden(tag) {
                return Err(RunbookIssue::Hidden);
            }
        }
        if self.variables.len() > MAX_VARIABLES {
            return Err(RunbookIssue::TooManyVariables);
        }
        for (name, value) in &self.variables {
            if value.chars().count() > MAX_PARAMETER_LENGTH {
                return Err(RunbookIssue::VariableValueTooLong(name.clone()));
            }
            if holds_hidden(value) {
                return Err(RunbookIssue::Hidden);
            }
        }
        Ok(())
    }
}

/// A step of a value read from a file; `number` counts from 1.
fn step_from_value(value: &Value, number: usize) -> Result<Step, RunbookIssue> {
    let Some(object) = value.as_object() else {
        return Err(RunbookIssue::Step(number, StepIssue::CommandMissing));
    };
    let text = |field: &str| object.get(field).and_then(Value::as_str);
    let description = note_of(text(key::DESCRIPTION));
    if let Some(action_id) = text(key::ACTION_ID)
        .map(str::trim)
        .filter(|id| !id.is_empty())
    {
        let parameters = object
            .get(key::PARAMETERS)
            .and_then(Value::as_object)
            .map(|entries| {
                entries
                    .iter()
                    .map(|(name, value)| {
                        (name.clone(), value.as_str().unwrap_or_default().to_owned())
                    })
                    .collect()
            })
            .unwrap_or_default();
        return Ok(Step::Action {
            action_id: action_id.to_owned(),
            action_title: text(key::ACTION_TITLE).unwrap_or_default().to_owned(),
            parameters,
            description,
        });
    }
    // A literal step written for both platforms, or for none that is readable, reads as
    // Linux, as the C# editor shows it (`IsWindows` is false unless the platform is Windows).
    let platform = match text(key::PLATFORM).map(str::trim) {
        Some(name) if name.eq_ignore_ascii_case(PLATFORM_WINDOWS) || name == "0" => {
            TemplatePlatform::Windows
        }
        _ => TemplatePlatform::Linux,
    };
    Ok(Step::Literal {
        command: text(key::COMMAND).unwrap_or_default().trim().to_owned(),
        platform,
        description,
    })
}

/// A step as the transfer model writes it.
fn step_to_value(step: &Step) -> Value {
    let mut object = Map::new();
    match step {
        Step::Action {
            action_id,
            action_title,
            parameters,
            description,
        } => {
            object.insert(key::ACTION_ID.to_owned(), Value::from(action_id.clone()));
            object.insert(
                key::ACTION_TITLE.to_owned(),
                Value::from(action_title.clone()),
            );
            // The C# keeps the text last generated here; this build generates it again at
            // each run, so there is none to keep.
            object.insert(key::COMMAND.to_owned(), Value::from(String::new()));
            object.insert(key::PLATFORM.to_owned(), Value::from(PLATFORM_BOTH));
            object.insert(
                key::DESCRIPTION.to_owned(),
                description.clone().map_or(Value::Null, Value::from),
            );
            object.insert(key::PARAMETERS.to_owned(), map_value(parameters));
        }
        Step::Literal {
            command,
            platform,
            description,
        } => {
            object.insert(key::ACTION_ID.to_owned(), Value::Null);
            object.insert(key::ACTION_TITLE.to_owned(), Value::from(String::new()));
            object.insert(key::COMMAND.to_owned(), Value::from(command.clone()));
            object.insert(
                key::PLATFORM.to_owned(),
                Value::from(match platform {
                    TemplatePlatform::Windows => PLATFORM_WINDOWS,
                    TemplatePlatform::Linux => PLATFORM_LINUX,
                }),
            );
            object.insert(
                key::DESCRIPTION.to_owned(),
                description.clone().map_or(Value::Null, Value::from),
            );
            object.insert(key::PARAMETERS.to_owned(), Value::Object(Map::new()));
        }
    }
    Value::Object(object)
}

/// A map of texts as a JSON object.
fn map_value(map: &BTreeMap<String, String>) -> Value {
    Value::Object(
        map.iter()
            .map(|(name, value)| (name.clone(), Value::from(value.clone())))
            .collect(),
    )
}

/// A note, trimmed, or none when it is blank.
fn note_of(text: Option<&str>) -> Option<String> {
    text.map(str::trim)
        .filter(|note| !note.is_empty())
        .map(str::to_owned)
}

/// Whether `id` reads as a GUID: 32 hexadecimal digits, or 8-4-4-4-12 with hyphens.
fn is_identity(id: &str) -> bool {
    let hex = |part: &str| part.chars().all(|c| c.is_ascii_hexdigit());
    if id.len() == GUID_PLAIN_LENGTH {
        return hex(id);
    }
    let parts: Vec<&str> = id.split('-').collect();
    id.len() == MAX_PUBLIC_ID_LENGTH
        && parts.len() == GUID_GROUPS.len()
        && parts
            .iter()
            .zip(GUID_GROUPS)
            .all(|(part, length)| part.len() == length && hex(part))
}

/// A new identity, a version 4 UUID as the C# `Guid.NewGuid`.
fn new_identity() -> String {
    uuid_generator::generate(UuidVersion::V4).map_or_else(
        |_| {
            format!(
                "{:032x}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |since| since.as_nanos())
            )
        },
        |uuid| uuid_generator::format(uuid, UuidFormat::default()),
    )
}

/// Checks one step; see [`Runbook::validate`].
fn validate_step(step: &Step, lookup: &ActionLookup<'_>) -> Result<(), StepIssue> {
    if let Some(note) = step.description() {
        if note.chars().count() > MAX_DESCRIPTION_LENGTH {
            return Err(StepIssue::DescriptionTooLong);
        }
        if note_holds_hidden(note) {
            return Err(StepIssue::Hidden);
        }
    }
    match step {
        Step::Literal { command, .. } => {
            if command.trim().is_empty() {
                return Err(StepIssue::CommandMissing);
            }
            if command.chars().count() > MAX_COMMAND_LENGTH {
                return Err(StepIssue::CommandTooLong);
            }
            if holds_hidden(command) {
                return Err(StepIssue::Hidden);
            }
            Ok(())
        }
        Step::Action {
            action_id,
            action_title,
            parameters,
            ..
        } => {
            if action_id.trim().is_empty() {
                return Err(StepIssue::ActionMissing);
            }
            if action_id.chars().count() > MAX_ACTION_ID_LENGTH {
                return Err(StepIssue::ActionTooLong);
            }
            if action_title.chars().count() > MAX_TITLE_LENGTH {
                return Err(StepIssue::TitleTooLong);
            }
            if holds_hidden(action_id) || holds_hidden(action_title) {
                return Err(StepIssue::Hidden);
            }
            if parameters.len() > MAX_PARAMETERS {
                return Err(StepIssue::TooManyParameters);
            }
            for (name, value) in parameters {
                if name.is_empty()
                    || name.chars().count() > MAX_PARAMETER_NAME_LENGTH
                    || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                {
                    return Err(StepIssue::ParameterName(name.clone()));
                }
                if value.chars().count() > MAX_PARAMETER_LENGTH {
                    return Err(StepIssue::ValueTooLong(name.clone()));
                }
                if holds_hidden(value) {
                    return Err(StepIssue::Hidden);
                }
            }
            match lookup(action_id) {
                Some(command) => check_values(command, parameters),
                None => Ok(()),
            }
        }
    }
}

/// Puts the values of a step through the generator against each template of its action,
/// as a value typed in the library goes through it. The values that refer to a variable are
/// left out, to be judged once the run has substituted them; what the generator says of a
/// parameter then missing is not held against them. The step is refused when the generator
/// refuses its values for every template: a value fit for one shell is fit for a run there.
fn check_values(
    command: &UserCommand,
    parameters: &BTreeMap<String, String>,
) -> Result<(), StepIssue> {
    let mut refused: Option<StepIssue> = None;
    for platform in command.template_platforms() {
        let Some(template) = command.template(platform) else {
            continue;
        };
        let deferred: Vec<&str> = template
            .parameters
            .iter()
            .filter(|parameter| {
                parameters
                    .get(&parameter.name)
                    .is_some_and(|value| !variable_references(value).is_empty())
            })
            .map(|parameter| parameter.name.as_str())
            .collect();
        let deferred_labels: Vec<&str> = template
            .parameters
            .iter()
            .filter(|parameter| deferred.contains(&parameter.name.as_str()))
            .map(|parameter| {
                if parameter.label.trim().is_empty() {
                    parameter.name.as_str()
                } else {
                    parameter.label.as_str()
                }
            })
            .collect();
        let outcome = user_commands::generate(template, platform, |name| {
            if deferred.contains(&name) {
                None
            } else {
                parameters.get(name).map(String::as_str)
            }
        });
        match outcome {
            Ok(_) => return Ok(()),
            Err(GenerateError::Values(errors)) => {
                let errors: Vec<ValueError> = errors
                    .into_iter()
                    .filter(|error| match error {
                        ValueError::Required(label) => !deferred_labels.contains(&label.as_str()),
                        _ => true,
                    })
                    .collect();
                if errors.is_empty() {
                    return Ok(());
                }
                refused.get_or_insert(StepIssue::Values(errors));
            }
            // A command that is too long once the variables are in is judged at the run.
            Err(GenerateError::TooLong) if !deferred.is_empty() => return Ok(()),
            Err(GenerateError::TooLong) => {
                refused.get_or_insert(StepIssue::CommandTooLong);
            }
            // The pattern of the action is the action's own business.
            Err(
                GenerateError::EmptyPattern
                | GenerateError::TooManyParameters
                | GenerateError::Multiline,
            ) => {}
        }
    }
    refused.map_or(Ok(()), Err)
}

/// Why the file of the runbooks could not be used.
#[derive(Debug)]
pub enum StoreError {
    /// It could not be read or written.
    Io(io::Error),
    /// It is larger than [`MAX_FILE_BYTES`].
    TooLarge,
    /// It is not an envelope of runbooks.
    Format,
    /// It is written by another major version of the format: left alone.
    Version(String),
    /// It holds more than [`MAX_RUNBOOKS`] runbooks.
    TooMany,
}

/// Why a runbook could not be kept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeepError {
    /// The runbook breaks a rule.
    Refused(RunbookIssue),
    /// The file would hold more than [`MAX_RUNBOOKS`] runbooks.
    TooMany,
}

/// The runbooks of the user.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RunbookStore {
    runbooks: Vec<Runbook>,
    /// What was read and broke a rule, as it was read, written back unchanged.
    unreadable: Vec<Value>,
    /// Whether a runbook was given a new identity when the file was read.
    reidentified: bool,
}

impl RunbookStore {
    /// The runbooks, the one changed last first.
    #[must_use]
    pub fn runbooks(&self) -> &[Runbook] {
        &self.runbooks
    }

    /// Whether reading the file gave a runbook a new identity (it had none that was a GUID, or
    /// repeated another's). The new identities are random: the store must be written back
    /// before anything is changed on it, or the next read would draw others.
    #[must_use]
    pub const fn reidentified(&self) -> bool {
        self.reidentified
    }

    /// The runbook of `public_id`.
    #[must_use]
    pub fn get(&self, public_id: &str) -> Option<&Runbook> {
        self.runbooks
            .iter()
            .find(|each| each.public_id == public_id)
    }

    /// How many runbooks of the file were left out for breaking a rule. They are kept as
    /// they are and written back with the others.
    #[must_use]
    pub fn unreadable_count(&self) -> usize {
        self.unreadable.len()
    }

    /// The runbooks kept at `path`; an empty store when there is no file. The file is
    /// untrusted: a runbook that breaks a rule of [`Runbook::validate`], or whose identity
    /// another has, is not offered; see [`RunbookStore::unreadable_count`].
    ///
    /// # Errors
    ///
    /// [`StoreError`] when the file cannot be read, is too large, is not an envelope or is
    /// of another major version: the caller must then not save over it.
    pub fn load(path: &Path, lookup: &ActionLookup<'_>) -> Result<Self, StoreError> {
        let length = match fs::metadata(path) {
            Ok(meta) => meta.len(),
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(error) => return Err(StoreError::Io(error)),
        };
        if length > MAX_FILE_BYTES {
            return Err(StoreError::TooLarge);
        }
        let text = fs::read_to_string(path).map_err(StoreError::Io)?;
        Self::parse(&text, lookup)
    }

    /// The runbooks of an envelope in `text`; see [`RunbookStore::load`].
    ///
    /// # Errors
    ///
    /// [`StoreError`], as [`RunbookStore::load`].
    pub fn parse(text: &str, lookup: &ActionLookup<'_>) -> Result<Self, StoreError> {
        let root: Value = serde_json::from_str(text).map_err(|_| StoreError::Format)?;
        let envelope = root.as_object().ok_or(StoreError::Format)?;
        if let Some(version) = envelope.get(key::SCHEMA_VERSION) {
            let version = version.as_str().ok_or(StoreError::Format)?;
            let major = version.split(VERSION_SEPARATOR).next().unwrap_or_default();
            if major != SUPPORTED_MAJOR {
                return Err(StoreError::Version(version.to_owned()));
            }
        }
        let listed = match envelope.get(key::RUNBOOKS) {
            None | Some(Value::Null) => &[][..],
            Some(Value::Array(listed)) => listed.as_slice(),
            Some(_) => return Err(StoreError::Format),
        };
        if listed.len() > MAX_RUNBOOKS {
            return Err(StoreError::TooMany);
        }
        let mut store = Self::default();
        for value in listed {
            let runbook = Runbook::from_value(value)
                .and_then(|runbook| runbook.validate(lookup).map(|()| runbook));
            match runbook {
                Ok(mut runbook) => {
                    // Two runbooks of one identity (a copied file): the later one is a runbook
                    // of its own, not a loss.
                    if store.get(&runbook.public_id).is_some() {
                        runbook.public_id = new_identity();
                    }
                    let written = value
                        .get(key::PUBLIC_ID)
                        .and_then(Value::as_str)
                        .map(|id| id.trim().to_ascii_lowercase());
                    store.reidentified |= written.as_deref() != Some(runbook.public_id.as_str());
                    store.runbooks.push(runbook);
                }
                Err(_) => store.unreadable.push(value.clone()),
            }
        }
        Ok(store)
    }

    /// Keeps the store at `path`, readable by its owner alone, replacing the file in one
    /// rename.
    ///
    /// # Errors
    ///
    /// [`StoreError::TooLarge`] when the file would pass [`MAX_FILE_BYTES`], else the error
    /// of writing; what was there is left as it was.
    pub fn save(&self, path: &Path) -> Result<(), StoreError> {
        let text = self.to_text();
        if u64::try_from(text.len()).map_or(true, |size| size > MAX_FILE_BYTES) {
            return Err(StoreError::TooLarge);
        }
        if let Some(folder) = path.parent() {
            fs::create_dir_all(folder).map_err(StoreError::Io)?;
        }
        write_private(path, text.as_bytes()).map_err(StoreError::Io)
    }

    /// The store as the file holds it.
    #[must_use]
    pub fn to_text(&self) -> String {
        let mut envelope = Map::new();
        envelope.insert(key::SCHEMA_VERSION.to_owned(), Value::from(SCHEMA_VERSION));
        envelope.insert(
            key::RUNBOOKS.to_owned(),
            Value::Array(
                self.runbooks
                    .iter()
                    .map(Runbook::to_value)
                    .chain(self.unreadable.iter().cloned())
                    .collect(),
            ),
        );
        serde_json::to_string_pretty(&Value::Object(envelope)).unwrap_or_default()
    }

    /// Keeps `runbook`, a new one or a new version of the one of its identity, and puts it
    /// first. Nothing changes when it is refused.
    ///
    /// # Errors
    ///
    /// [`KeepError`]: a rule broken, or too many runbooks.
    pub fn keep(&mut self, runbook: Runbook, lookup: &ActionLookup<'_>) -> Result<(), KeepError> {
        let runbook = Runbook {
            name: runbook.name.trim().to_owned(),
            ..runbook
        };
        runbook.validate(lookup).map_err(KeepError::Refused)?;
        let known = self
            .runbooks
            .iter()
            .position(|each| each.public_id == runbook.public_id);
        if known.is_none() && self.runbooks.len() + self.unreadable.len() >= MAX_RUNBOOKS {
            return Err(KeepError::TooMany);
        }
        if let Some(index) = known {
            self.runbooks.remove(index);
        }
        self.runbooks.insert(0, runbook);
        Ok(())
    }

    /// Removes the runbook of `public_id`. Whether there was one.
    pub fn delete(&mut self, public_id: &str) -> bool {
        let before = self.runbooks.len();
        self.runbooks.retain(|each| each.public_id != public_id);
        self.runbooks.len() != before
    }

    /// A copy of the runbook of `public_id` under `name` and a new identity, kept first.
    /// The identity of the copy; `None` when there is no such runbook, or the store is full.
    pub fn duplicate(&mut self, public_id: &str, name: &str) -> Option<String> {
        if self.runbooks.len() + self.unreadable.len() >= MAX_RUNBOOKS {
            return None;
        }
        let mut copy = self.get(public_id)?.clone();
        copy.public_id = new_identity();
        copy.name = name.trim().chars().take(MAX_TITLE_LENGTH).collect();
        let id = copy.public_id.clone();
        self.runbooks.insert(0, copy);
        Some(id)
    }
}

/// Why a step of a run cannot be sent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunProblem {
    /// The literal command is empty.
    CommandMissing,
    /// The literal command is longer than [`MAX_COMMAND_LENGTH`].
    CommandTooLong,
    /// The literal command holds a control character or one that shows nothing.
    Hidden,
    /// The library no longer has the action.
    ActionMissing,
    /// The action has no usable template.
    NoTemplate,
    /// The step refers to variables the run does not hold.
    UnknownVariables(Vec<String>),
    /// The generator refuses the values.
    Values(Vec<ValueError>),
    /// The generator cannot make the command (too long, no pattern, too many parameters).
    Generate(GenerateError),
}

/// A step ready to be sent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedStep {
    /// The command, one line, as it is typed.
    pub command: String,
    /// The template it was made from; the step's own platform for a literal step.
    pub platform: TemplatePlatform,
    /// How dangerous it is: the action's stored level, and the highest for a literal step,
    /// which nobody has judged.
    pub level: Criticality,
    /// Whether it is a literal step.
    pub literal: bool,
}

/// The template an action step runs on for a session whose far end is `remote`, as the C#
/// `SelectRunbookTemplate`: the matching one, the other when the action has only one, and
/// Windows first when the platform cannot be told.
#[must_use]
pub fn select_template(command: &UserCommand, remote: RemoteOs) -> Option<TemplatePlatform> {
    let usable = command.template_platforms();
    let (first, second) = if remote == RemoteOs::Linux {
        (TemplatePlatform::Linux, TemplatePlatform::Windows)
    } else {
        (TemplatePlatform::Windows, TemplatePlatform::Linux)
    };
    [first, second]
        .into_iter()
        .find(|platform| usable.contains(platform))
}

/// The command a step sends once the run holds `variables`, judged now as a command typed in
/// the library is: every value of an action step goes through [`user_commands::generate`]
/// after the variables are put in (the C# `ResolveAction` and `ValidateParameters`), and a
/// literal step is refused for a control or invisible character. `action` is the library's
/// action of an action step, `None` when it is gone.
///
/// # Errors
///
/// The reason the step cannot be sent, as a [`RunProblem`].
pub fn resolve_step(
    step: &Step,
    variables: &BTreeMap<String, String>,
    action: Option<&UserCommand>,
    remote: RemoteOs,
) -> Result<ResolvedStep, RunProblem> {
    match step {
        Step::Literal {
            command, platform, ..
        } => {
            let command = command.trim();
            if command.is_empty() {
                return Err(RunProblem::CommandMissing);
            }
            if command.chars().count() > MAX_COMMAND_LENGTH {
                return Err(RunProblem::CommandTooLong);
            }
            if holds_hidden(command) {
                return Err(RunProblem::Hidden);
            }
            Ok(ResolvedStep {
                command: command.to_owned(),
                platform: *platform,
                level: Criticality::Dangerous,
                literal: true,
            })
        }
        Step::Action { parameters, .. } => {
            let action = action.ok_or(RunProblem::ActionMissing)?;
            let platform = select_template(action, remote).ok_or(RunProblem::NoTemplate)?;
            let template = action.template(platform).ok_or(RunProblem::NoTemplate)?;
            let mut unknown = Vec::new();
            let values: BTreeMap<&str, String> = parameters
                .iter()
                .map(|(name, value)| (name.as_str(), substitute(value, variables, &mut unknown)))
                .collect();
            if !unknown.is_empty() {
                return Err(RunProblem::UnknownVariables(unknown));
            }
            let command = user_commands::generate(template, platform, |name| {
                values.get(name).map(String::as_str)
            })
            .map_err(|error| match error {
                GenerateError::Values(errors) => RunProblem::Values(errors),
                other => RunProblem::Generate(other),
            })?;
            // A line break would submit the first line before the rest is read, whatever the
            // pattern's author meant, as the library's own send refuses it.
            if command.chars().any(is_hidden) {
                return Err(RunProblem::Generate(GenerateError::Multiline));
            }
            Ok(ResolvedStep {
                command,
                platform,
                level: action.level,
                literal: false,
            })
        }
    }
}

#[cfg(test)]
mod tests;
