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

//! The user's own commands of the command library, apart from the seeded catalog.
//!
//! The C# keeps them as TwinShell actions (`TwinShell.Core/Models/Action.cs`) in a database,
//! with `IsUserCreated` set (`ActionService.cs:97`), and moves them as a JSON envelope
//! (`CommandLibraryTransferService.cs:46-62`). Here the file is that envelope, so the
//! library written here can be imported by the C# and the C# export can be imported here
//! ([`UserLibrary::import`]), each action keeping the fields this build does not edit.
//! Values put into a command are checked and quoted as the C# generator does
//! (`CommandGeneratorService.cs`), and a value holding a line break is always refused: the
//! command is typed into a terminal.

use std::fs;
use std::io;
use std::net::IpAddr;
use std::path::Path;
use std::str::FromStr as _;

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Map, Value};

use super::date_time::{Instant, round_trip_utc};
use super::private_file::write_private;
use super::uuid_generator::{self, UuidFormat, UuidVersion};

/// The file of the user's commands, beside the profiles file.
pub const LIBRARY_FILE: &str = "command-library.json";

/// The envelope's version, as the C# export writes it.
pub const SCHEMA_VERSION: &str = "1.0";

/// The longest value of a parameter, as the C# `MaxParameterLength`.
pub const MAX_PARAMETER_LENGTH: usize = 256;

/// The longest generated command, as the C# `MaxCommandLength`.
pub const MAX_COMMAND_LENGTH: usize = 1024;

/// The most parameters of a template, as the C# `MaxParametersPerTemplate`.
pub const MAX_PARAMETERS: usize = 50;

/// The longest title, as the C# `MaxActionTitleLength`.
pub const MAX_TITLE_LENGTH: usize = 200;

/// The longest description, as the C# `MaxActionDescriptionLength`.
pub const MAX_DESCRIPTION_LENGTH: usize = 2000;

/// The longest category, as the C# `MaxActionCategoryLength`.
pub const MAX_CATEGORY_LENGTH: usize = 100;

/// The longest command pattern, as the C# `MaxTemplateCommandPatternLength`.
pub const MAX_PATTERN_LENGTH: usize = 1000;

/// The most tags, as the C# `MaxActionTagsCount`.
pub const MAX_TAGS: usize = 20;

/// The largest file read, as the C# `MaxFileSizeBytes`.
pub const MAX_FILE_BYTES: u64 = 10 * 1024 * 1024;

/// The longest hostname, as the C# `IsValidHostname`.
const MAX_HOSTNAME_LENGTH: usize = 255;

/// The longest label of a hostname.
const MAX_HOSTNAME_LABEL_LENGTH: usize = 63;

/// What separates the tags typed in the editor.
const TAG_SEPARATOR: char = ',';

/// The characters a string value may not hold, as the C# `DangerousChars`.
const DANGEROUS_CHARACTERS: [char; 9] = ['&', '|', ';', '`', '$', '(', ')', '<', '>'];

/// The single quotes PowerShell reads as one: each is doubled to carry itself.
const POWERSHELL_QUOTES: [char; 5] = ['\'', '\u{2018}', '\u{2019}', '\u{201A}', '\u{201B}'];

/// The type name of a parameter with a fixed set of values, as the C# `ChoiceTypeName`.
pub const CHOICE_TYPE: &str = "choice";

/// The type of a parameter nothing else is said of, as the C# default.
pub const STRING_TYPE: &str = "string";

/// The operating system a template is written for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UserPlatform {
    /// PowerShell.
    Windows,
    /// A POSIX shell.
    Linux,
    /// Both, one template each.
    #[default]
    Both,
}

impl UserPlatform {
    /// Every platform, in the order the editor offers them.
    pub const ALL: [Self; 3] = [Self::Windows, Self::Linux, Self::Both];

    /// The number the C# writes (`Platform.cs`).
    const fn number(self) -> u8 {
        match self {
            Self::Windows => 0,
            Self::Linux => 1,
            Self::Both => 2,
        }
    }

    /// The platform of a number or a name as the C# reads it; `Both` for anything else, as
    /// `NormalizeEnumDomains` (`CommandLibraryTransferService.cs:156-167`).
    fn from_wire(wire: &Wire) -> Self {
        match wire {
            Wire::Number(0) => Self::Windows,
            Wire::Number(1) => Self::Linux,
            Wire::Text(name) if name.eq_ignore_ascii_case("windows") => Self::Windows,
            Wire::Text(name) if name.eq_ignore_ascii_case("linux") => Self::Linux,
            _ => Self::Both,
        }
    }
}

/// How dangerous running a command is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum Criticality {
    /// Reads and shows.
    #[default]
    Info,
    /// Changes something that can be undone.
    Run,
    /// Can destroy data or stop a machine.
    Dangerous,
}

impl Criticality {
    /// Every level, in the order the editor offers them.
    pub const ALL: [Self; 3] = [Self::Info, Self::Run, Self::Dangerous];

    /// The number the C# writes (`CriticalityLevel.cs`).
    const fn number(self) -> u8 {
        match self {
            Self::Info => 0,
            Self::Run => 1,
            Self::Dangerous => 2,
        }
    }

    /// The level of a number or a name as the C# reads it; `Dangerous` for anything else,
    /// as `NormalizeEnumDomains`: a risk that cannot be read is the highest.
    fn from_wire(wire: &Wire) -> Self {
        match wire {
            Wire::Number(0) => Self::Info,
            Wire::Number(1) => Self::Run,
            Wire::Text(name) if name.eq_ignore_ascii_case("info") => Self::Info,
            Wire::Text(name) if name.eq_ignore_ascii_case("run") => Self::Run,
            _ => Self::Dangerous,
        }
    }
}

/// An enumeration as a file holds it: a number or a name.
#[derive(Deserialize)]
#[serde(untagged)]
enum Wire {
    Number(i64),
    Text(String),
}

impl Serialize for UserPlatform {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u8(self.number())
    }
}

impl<'de> Deserialize<'de> for UserPlatform {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(Self::from_wire(&Wire::deserialize(deserializer)?))
    }
}

impl Serialize for Criticality {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u8(self.number())
    }
}

impl<'de> Deserialize<'de> for Criticality {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(Self::from_wire(&Wire::deserialize(deserializer)?))
    }
}

/// How a value is put into its place in the pattern, as the C# `QuotingMode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Quoting {
    /// The value is quoted whole.
    #[default]
    ShellQuote,
    /// The placeholder is already between single quotes: only the value is escaped.
    InlineInQuotes,
}

impl Serialize for Quoting {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(match self {
            Self::ShellQuote => "ShellQuote",
            Self::InlineInQuotes => "InlineInQuotes",
        })
    }
}

impl<'de> Deserialize<'de> for Quoting {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let name = String::deserialize(deserializer)?;
        if name.eq_ignore_ascii_case("ShellQuote") {
            Ok(Self::ShellQuote)
        } else if name.eq_ignore_ascii_case("InlineInQuotes") {
            Ok(Self::InlineInQuotes)
        } else {
            Err(D::Error::custom("unknown quoting"))
        }
    }
}

/// One value a command asks for, as the C# `TemplateParameter`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserParameter {
    /// The name between braces in the pattern.
    #[serde(default)]
    pub name: String,
    /// What the window calls it.
    #[serde(default)]
    pub label: String,
    /// Its type: `string`, `path`, `int`, `number`, `hostname`, `ipaddress`, `driveletter`,
    /// `choice`.
    #[serde(default = "default_type", rename = "type")]
    pub kind: String,
    /// What it holds until something is typed.
    #[serde(default)]
    pub default_value: Option<String>,
    /// Whether a value must be given.
    #[serde(default)]
    pub required: bool,
    /// What it is for.
    #[serde(default)]
    pub description: Option<String>,
    /// The values offered, for a choice.
    #[serde(default)]
    pub allowed_values: Option<Vec<String>>,
    /// How it is put in; the default when none.
    #[serde(default)]
    pub quoting: Option<Quoting>,
}

fn default_type() -> String {
    STRING_TYPE.to_owned()
}

impl UserParameter {
    /// A parameter of `name`, a string asking for nothing more.
    #[must_use]
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            label: name.to_owned(),
            kind: default_type(),
            default_value: None,
            required: false,
            description: None,
            allowed_values: None,
            quoting: None,
        }
    }

    /// Whether it offers a fixed set, as the C# `IsChoice`.
    #[must_use]
    pub fn is_choice(&self) -> bool {
        self.kind.eq_ignore_ascii_case(CHOICE_TYPE)
            && self.allowed_values.as_ref().is_some_and(|v| !v.is_empty())
    }
}

/// The command of one platform, as the C# `CommandTemplate`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserTemplate {
    /// The platform it is written for.
    #[serde(default)]
    pub platform: UserPlatform,
    /// Its name.
    #[serde(default)]
    pub name: String,
    /// The command, each value at its `{name}`.
    #[serde(default)]
    pub command_pattern: String,
    /// The values it asks for.
    #[serde(default)]
    pub parameters: Vec<UserParameter>,
}

/// Which template of a command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TemplatePlatform {
    /// The PowerShell one.
    Windows,
    /// The POSIX shell one.
    Linux,
}

/// One command of the user, as the C# `Action` the user created.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserCommand {
    /// The identity.
    #[serde(default)]
    pub id: String,
    /// The identity moved between installations.
    #[serde(default)]
    pub public_id: String,
    /// The title.
    #[serde(default)]
    pub title: String,
    /// What it does.
    #[serde(default)]
    pub description: String,
    /// The category the user names.
    #[serde(default)]
    pub category: String,
    /// The platforms it has a template for.
    #[serde(default)]
    pub platform: UserPlatform,
    /// How dangerous it is.
    #[serde(default)]
    pub level: Criticality,
    /// Its tags.
    #[serde(default)]
    pub tags: Vec<String>,
    /// The PowerShell template.
    #[serde(default)]
    pub windows_command_template: Option<UserTemplate>,
    /// The POSIX shell template.
    #[serde(default)]
    pub linux_command_template: Option<UserTemplate>,
    /// When it was made, as the C# writes it.
    #[serde(default)]
    pub created_at: Option<String>,
    /// When it was last changed.
    #[serde(default)]
    pub updated_at: Option<String>,
    /// Always set for the commands of this library.
    #[serde(default)]
    pub is_user_created: bool,
    /// What the C# keeps that this build does not edit (examples, links, notes): kept as it
    /// came, so a command goes back to the C# whole.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl UserCommand {
    /// The template of `platform`.
    #[must_use]
    pub fn template(&self, platform: TemplatePlatform) -> Option<&UserTemplate> {
        match platform {
            TemplatePlatform::Windows => self.windows_command_template.as_ref(),
            TemplatePlatform::Linux => self.linux_command_template.as_ref(),
        }
    }

    /// The platforms it has a usable template for, Windows first.
    #[must_use]
    pub fn template_platforms(&self) -> Vec<TemplatePlatform> {
        [TemplatePlatform::Windows, TemplatePlatform::Linux]
            .into_iter()
            .filter(|platform| {
                self.template(*platform)
                    .is_some_and(|t| !t.command_pattern.trim().is_empty())
            })
            .collect()
    }

    /// The template shown first: this system's, else the other, as the C#
    /// `TemplateHelper.GetActiveTemplate`.
    #[must_use]
    pub fn preferred_platform(&self, windows_host: bool) -> Option<TemplatePlatform> {
        let all = self.template_platforms();
        let first = if windows_host {
            TemplatePlatform::Windows
        } else {
            TemplatePlatform::Linux
        };
        if all.contains(&first) {
            Some(first)
        } else {
            all.first().copied()
        }
    }
}

/// Why a value cannot be put in a command. Each names the parameter by its label.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValueError {
    /// A required value is missing.
    Required(String),
    /// The value is not among those offered.
    NotOffered(String),
    /// Not a whole number.
    NotInteger(String),
    /// Not a number.
    NotNumber(String),
    /// Not a host name.
    Hostname(String),
    /// Not an IP address.
    IpAddress(String),
    /// Not one drive letter.
    DriveLetter(String),
    /// Not a path.
    Path(String),
    /// Longer than [`MAX_PARAMETER_LENGTH`].
    TooLong(String),
    /// Holds a character the shell would read as syntax.
    Dangerous(String),
    /// Holds a line break or another control character: it would run more than the command.
    Control(String),
}

/// Why a command cannot be made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GenerateError {
    /// The template has no command.
    EmptyPattern,
    /// Too many parameters.
    TooManyParameters,
    /// The values refused.
    Values(Vec<ValueError>),
    /// The command is longer than [`MAX_COMMAND_LENGTH`].
    TooLong,
}

/// The command of `template` with the values `value_of` gives, each put in as its type and
/// quoting say for the shell of `platform`, as the C# `CommandGeneratorService.GenerateCommand`;
/// a blank value is its default.
///
/// # Errors
///
/// [`GenerateError`]: an empty pattern, too many parameters, a value refused, or a command
/// too long.
pub fn generate<'v>(
    template: &UserTemplate,
    platform: TemplatePlatform,
    value_of: impl Fn(&str) -> Option<&'v str>,
) -> Result<String, GenerateError> {
    if template.command_pattern.trim().is_empty() {
        return Err(GenerateError::EmptyPattern);
    }
    if template.parameters.len() > MAX_PARAMETERS {
        return Err(GenerateError::TooManyParameters);
    }
    let mut command = template.command_pattern.clone();
    let mut refused = Vec::new();
    for parameter in &template.parameters {
        let value = value_of(&parameter.name)
            .filter(|typed| !typed.trim().is_empty())
            .or(parameter.default_value.as_deref())
            .unwrap_or("");
        match check_value(parameter, value) {
            Ok(()) => {
                let quoted = quote_value(parameter, value, platform);
                command = command.replace(&format!("{{{}}}", parameter.name), &quoted);
            }
            Err(error) => refused.push(error),
        }
    }
    if !refused.is_empty() {
        return Err(GenerateError::Values(refused));
    }
    if command.chars().count() > MAX_COMMAND_LENGTH {
        return Err(GenerateError::TooLong);
    }
    Ok(command)
}

/// Checks `value` against `parameter`, as the C# `ValidateParameterValue` and
/// `ValidateParameters` together; a blank value passes unless required.
fn check_value(parameter: &UserParameter, value: &str) -> Result<(), ValueError> {
    let label = || {
        if parameter.label.trim().is_empty() {
            parameter.name.clone()
        } else {
            parameter.label.clone()
        }
    };
    if value.chars().any(char::is_control) {
        return Err(ValueError::Control(label()));
    }
    if value.trim().is_empty() {
        return if parameter.required {
            Err(ValueError::Required(label()))
        } else {
            Ok(())
        };
    }
    if value.chars().count() > MAX_PARAMETER_LENGTH {
        return Err(ValueError::TooLong(label()));
    }
    if parameter.is_choice() {
        let offered = parameter.allowed_values.as_deref().unwrap_or_default();
        if !offered
            .iter()
            .any(|each| each.trim().eq_ignore_ascii_case(value.trim()))
        {
            return Err(ValueError::NotOffered(label()));
        }
    }
    match parameter.kind.to_lowercase().as_str() {
        "int" | "integer" if value.trim().parse::<i32>().is_err() => {
            Err(ValueError::NotInteger(label()))
        }
        "number" if value.trim().parse::<f64>().is_err() => Err(ValueError::NotNumber(label())),
        "hostname" if !is_hostname(value) => Err(ValueError::Hostname(label())),
        "ipaddress" if IpAddr::from_str(value.trim()).is_err() => {
            Err(ValueError::IpAddress(label()))
        }
        "driveletter" if !(value.len() == 1 && value.chars().all(|c| c.is_ascii_alphabetic())) => {
            Err(ValueError::DriveLetter(label()))
        }
        "path" if value.trim().is_empty() => Err(ValueError::Path(label())),
        "string" if value.contains(DANGEROUS_CHARACTERS) => Err(ValueError::Dangerous(label())),
        _ => Ok(()),
    }
}

/// Whether `value` is a host name of RFC 1123.
fn is_hostname(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_HOSTNAME_LENGTH
        && value.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= MAX_HOSTNAME_LABEL_LENGTH
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        })
}

/// `value` as it goes into the command: typed values as they are, strings and paths quoted
/// or escaped for `platform`'s shell, as the C# `EscapeParameterValue`.
fn quote_value(parameter: &UserParameter, value: &str, platform: TemplatePlatform) -> String {
    match parameter.kind.to_lowercase().as_str() {
        "hostname" | "ipaddress" | "driveletter" | "int" | "integer" | "number" => {
            value.trim().to_owned()
        }
        _ if parameter.quoting == Some(Quoting::InlineInQuotes) => escape_content(value, platform),
        _ => format!("'{}'", escape_content(value, platform)),
    }
}

/// `value` escaped to sit between single quotes of `platform`'s shell.
fn escape_content(value: &str, platform: TemplatePlatform) -> String {
    match platform {
        TemplatePlatform::Windows => value
            .chars()
            .flat_map(|c| {
                let doubled = POWERSHELL_QUOTES.contains(&c);
                std::iter::once(c).chain(doubled.then_some(c))
            })
            .collect(),
        TemplatePlatform::Linux => value.replace('\'', "'\\''"),
    }
}

/// Whether `{name}` in `pattern` sits between single quotes, as the C#
/// `CommandPatternQuoting.IsPlaceholderInsideSingleQuotedSpan`: an `InlineInQuotes` value
/// anywhere else would be put in with no quote at all.
#[must_use]
pub fn placeholder_in_single_quotes(pattern: &str, name: &str) -> bool {
    let placeholder = format!("{{{name}}}");
    let mut inside = false;
    let mut rest = pattern;
    while let Some(next) = rest.find(['\'', '{']) {
        let tail = &rest[next..];
        if tail.starts_with('\'') {
            inside = !inside;
            rest = &tail[1..];
        } else if tail.starts_with(&placeholder) {
            if inside {
                return true;
            }
            rest = &tail[placeholder.len()..];
        } else {
            rest = &tail[1..];
        }
    }
    false
}

/// The names between braces in `pattern`, each once, in order of appearance.
#[must_use]
pub fn placeholders(pattern: &str) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    let mut rest = pattern;
    while let Some(open) = rest.find('{') {
        let after = &rest[open + 1..];
        let Some(close) = after.find('}') else {
            break;
        };
        let name = &after[..close];
        if !name.is_empty()
            && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
            && !names.iter().any(|known| known == name)
        {
            names.push(name.to_owned());
        }
        rest = &after[close + 1..];
    }
    names
}

/// What the editor holds of a command.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Draft {
    /// The command edited; `None` for a new one.
    pub id: Option<String>,
    /// The title.
    pub title: String,
    /// What it does.
    pub description: String,
    /// The category.
    pub category: String,
    /// The platforms.
    pub platform: UserPlatform,
    /// How dangerous.
    pub level: Criticality,
    /// The PowerShell pattern.
    pub windows_pattern: String,
    /// The POSIX shell pattern.
    pub linux_pattern: String,
    /// The tags, separated by commas.
    pub tags: String,
}

impl Draft {
    /// The draft of an existing command.
    #[must_use]
    pub fn of(command: &UserCommand) -> Self {
        let pattern = |platform| {
            command
                .template(platform)
                .map(|t| t.command_pattern.clone())
                .unwrap_or_default()
        };
        Self {
            id: Some(command.id.clone()),
            title: command.title.clone(),
            description: command.description.clone(),
            category: command.category.clone(),
            platform: command.platform,
            level: command.level,
            windows_pattern: pattern(TemplatePlatform::Windows),
            linux_pattern: pattern(TemplatePlatform::Linux),
            tags: command.tags.join(", "),
        }
    }

    /// The pattern of `platform` the platform chosen asks for: the other is left out.
    fn pattern_for(&self, platform: TemplatePlatform) -> Option<&str> {
        let wanted = matches!(
            (self.platform, platform),
            (UserPlatform::Both, _)
                | (UserPlatform::Windows, TemplatePlatform::Windows)
                | (UserPlatform::Linux, TemplatePlatform::Linux)
        );
        let pattern = match platform {
            TemplatePlatform::Windows => &self.windows_pattern,
            TemplatePlatform::Linux => &self.linux_pattern,
        };
        (wanted && !pattern.trim().is_empty()).then_some(pattern.trim())
    }
}

/// Why a draft cannot be saved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DraftError {
    /// No title.
    TitleMissing,
    /// A title longer than [`MAX_TITLE_LENGTH`].
    TitleTooLong,
    /// No category.
    CategoryMissing,
    /// A category longer than [`MAX_CATEGORY_LENGTH`].
    CategoryTooLong,
    /// A description longer than [`MAX_DESCRIPTION_LENGTH`].
    DescriptionTooLong,
    /// No command for the platform chosen.
    PatternMissing,
    /// A pattern longer than [`MAX_PATTERN_LENGTH`].
    PatternTooLong,
    /// A pattern with a line break: it would run more than one command.
    PatternMultiline,
    /// A pattern with more than [`MAX_PARAMETERS`] values.
    TooManyParameters,
    /// More than [`MAX_TAGS`] tags.
    TooManyTags,
    /// The command edited is no longer in the library.
    Gone,
}

/// Why the file could not be used.
#[derive(Debug)]
pub enum LibraryError {
    /// It could not be read or written.
    Io(io::Error),
    /// It is larger than [`MAX_FILE_BYTES`].
    TooLarge,
    /// It is not an envelope of actions.
    Format,
}

/// What an import did, as the C# `CommandLibraryImportResult`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ImportReport {
    /// Commands added.
    pub imported: usize,
    /// Commands of the user replaced.
    pub updated: usize,
    /// Commands left out: no title or category, too long.
    pub skipped: usize,
}

/// The user's commands.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UserLibrary {
    commands: Vec<UserCommand>,
}

impl UserLibrary {
    /// The commands, in the order they were made.
    #[must_use]
    pub fn commands(&self) -> &[UserCommand] {
        &self.commands
    }

    /// The command of `id`.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&UserCommand> {
        self.commands.iter().find(|command| command.id == id)
    }

    /// The library kept at `path`; an empty one when there is no file.
    ///
    /// # Errors
    ///
    /// [`LibraryError`] when the file cannot be read, is too large or is not an envelope: the
    /// caller must then not save over it.
    pub fn load(path: &Path) -> Result<Self, LibraryError> {
        let length = match fs::metadata(path) {
            Ok(meta) => meta.len(),
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(error) => return Err(LibraryError::Io(error)),
        };
        if length > MAX_FILE_BYTES {
            return Err(LibraryError::TooLarge);
        }
        let text = fs::read_to_string(path).map_err(LibraryError::Io)?;
        let mut library = Self::default();
        library.import(&text)?;
        Ok(library)
    }

    /// Keeps the library at `path`, as the C# envelope, readable by its owner alone.
    ///
    /// # Errors
    ///
    /// The error of writing the file; what was there is left as it was.
    pub fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(folder) = path.parent() {
            fs::create_dir_all(folder)?;
        }
        write_private(path, self.export().as_bytes())
    }

    /// The library as the C# export writes it.
    #[must_use]
    pub fn export(&self) -> String {
        let mut envelope = Map::new();
        envelope.insert("schemaVersion".to_owned(), Value::from(SCHEMA_VERSION));
        envelope.insert("exportDate".to_owned(), Value::from(now_text()));
        envelope.insert("totalActions".to_owned(), Value::from(self.commands.len()));
        envelope.insert(
            "actions".to_owned(),
            serde_json::to_value(&self.commands).unwrap_or_default(),
        );
        serde_json::to_string_pretty(&Value::Object(envelope)).unwrap_or_default()
    }

    /// Adds the commands of `json`, a C# export or a single action, as the C# import does
    /// (`CommandLibraryTransferService.cs:76-135`): one already here is replaced, one
    /// without a title or a category is left out.
    ///
    /// # Errors
    ///
    /// [`LibraryError::Format`] when `json` holds no action.
    pub fn import(&mut self, json: &str) -> Result<ImportReport, LibraryError> {
        let root: Value = serde_json::from_str(json).map_err(|_| LibraryError::Format)?;
        let actions: Vec<UserCommand> = if let Some(Value::Array(list)) = root.get("actions") {
            list.iter()
                .map(|each| serde_json::from_value(each.clone()))
                .collect::<Result<_, _>>()
                .map_err(|_| LibraryError::Format)?
        } else if root.get("id").is_some() && root.get("title").is_some() {
            vec![serde_json::from_value(root).map_err(|_| LibraryError::Format)?]
        } else {
            return Err(LibraryError::Format);
        };
        let mut report = ImportReport::default();
        for mut action in actions {
            if action.title.trim().is_empty()
                || action.category.trim().is_empty()
                || action.title.chars().count() > MAX_TITLE_LENGTH
                || action.category.chars().count() > MAX_CATEGORY_LENGTH
            {
                report.skipped += 1;
                continue;
            }
            normalize_quoting(&mut action);
            action.is_user_created = true;
            let found = self.commands.iter().position(|known| {
                (!action.public_id.is_empty() && known.public_id == action.public_id)
                    || (!action.id.is_empty() && known.id == action.id)
            });
            if action.id.is_empty() {
                action.id = new_identity();
            }
            if action.public_id.is_empty() {
                action.public_id = new_identity();
            }
            if let Some(index) = found {
                action.id.clone_from(&self.commands[index].id);
                action.public_id.clone_from(&self.commands[index].public_id);
                action.updated_at = Some(now_text());
                self.commands[index] = action;
                report.updated += 1;
            } else {
                let now = now_text();
                action.created_at.get_or_insert_with(|| now.clone());
                action.updated_at = Some(now);
                self.commands.push(action);
                report.imported += 1;
            }
        }
        Ok(report)
    }

    /// Makes or changes a command from `draft`; its identity.
    ///
    /// # Errors
    ///
    /// [`DraftError`] when the draft cannot be saved; the library is left as it was.
    pub fn upsert(&mut self, draft: &Draft) -> Result<String, DraftError> {
        let title = draft.title.trim();
        let category = draft.category.trim();
        if title.is_empty() {
            return Err(DraftError::TitleMissing);
        }
        if title.chars().count() > MAX_TITLE_LENGTH {
            return Err(DraftError::TitleTooLong);
        }
        if category.is_empty() {
            return Err(DraftError::CategoryMissing);
        }
        if category.chars().count() > MAX_CATEGORY_LENGTH {
            return Err(DraftError::CategoryTooLong);
        }
        if draft.description.chars().count() > MAX_DESCRIPTION_LENGTH {
            return Err(DraftError::DescriptionTooLong);
        }
        let tags: Vec<String> = draft
            .tags
            .split(TAG_SEPARATOR)
            .map(str::trim)
            .filter(|tag| !tag.is_empty())
            .map(str::to_owned)
            .collect();
        if tags.len() > MAX_TAGS {
            return Err(DraftError::TooManyTags);
        }
        let index = match &draft.id {
            Some(id) => Some(
                self.commands
                    .iter()
                    .position(|command| &command.id == id)
                    .ok_or(DraftError::Gone)?,
            ),
            None => None,
        };
        let before = index.map(|index| &self.commands[index]);
        let windows = template_of(draft, TemplatePlatform::Windows, before)?;
        let linux = template_of(draft, TemplatePlatform::Linux, before)?;
        if windows.is_none() && linux.is_none() {
            return Err(DraftError::PatternMissing);
        }
        if (draft.platform == UserPlatform::Windows && windows.is_none())
            || (draft.platform == UserPlatform::Linux && linux.is_none())
        {
            return Err(DraftError::PatternMissing);
        }
        let now = now_text();
        let mut command = before.cloned().unwrap_or_else(|| UserCommand {
            id: new_identity(),
            public_id: new_identity(),
            title: String::new(),
            description: String::new(),
            category: String::new(),
            platform: UserPlatform::Both,
            level: Criticality::Info,
            tags: Vec::new(),
            windows_command_template: None,
            linux_command_template: None,
            created_at: Some(now.clone()),
            updated_at: None,
            is_user_created: true,
            extra: Map::new(),
        });
        command.title = title.to_owned();
        command.description = draft.description.trim().to_owned();
        command.category = category.to_owned();
        command.platform = draft.platform;
        command.level = draft.level;
        command.tags = tags;
        command.windows_command_template = windows;
        command.linux_command_template = linux;
        command.updated_at = Some(now);
        command.is_user_created = true;
        let id = command.id.clone();
        match index {
            Some(index) => self.commands[index] = command,
            None => self.commands.push(command),
        }
        Ok(id)
    }

    /// Removes the command of `id`; whether there was one.
    pub fn delete(&mut self, id: &str) -> bool {
        let before = self.commands.len();
        self.commands.retain(|command| command.id != id);
        self.commands.len() != before
    }
}

/// The template `draft` asks for on `platform`: its parameters are the pattern's
/// placeholders, each keeping what the command being edited said of it.
fn template_of(
    draft: &Draft,
    platform: TemplatePlatform,
    before: Option<&UserCommand>,
) -> Result<Option<UserTemplate>, DraftError> {
    let Some(pattern) = draft.pattern_for(platform) else {
        return Ok(None);
    };
    if pattern.chars().count() > MAX_PATTERN_LENGTH {
        return Err(DraftError::PatternTooLong);
    }
    if pattern.chars().any(char::is_control) {
        return Err(DraftError::PatternMultiline);
    }
    let names = placeholders(pattern);
    if names.len() > MAX_PARAMETERS {
        return Err(DraftError::TooManyParameters);
    }
    let old = before.and_then(|command| command.template(platform));
    let mut parameters: Vec<UserParameter> = names
        .iter()
        .map(|name| {
            old.and_then(|t| t.parameters.iter().find(|known| &known.name == name))
                .cloned()
                .unwrap_or_else(|| UserParameter::new(name))
        })
        .collect();
    for parameter in &mut parameters {
        if parameter.quoting == Some(Quoting::InlineInQuotes)
            && !placeholder_in_single_quotes(pattern, &parameter.name)
        {
            parameter.quoting = Some(Quoting::ShellQuote);
        }
    }
    Ok(Some(UserTemplate {
        platform: match platform {
            TemplatePlatform::Windows => UserPlatform::Windows,
            TemplatePlatform::Linux => UserPlatform::Linux,
        },
        name: old.map(|t| t.name.clone()).unwrap_or_default(),
        command_pattern: pattern.to_owned(),
        parameters,
    }))
}

/// Resets every `InlineInQuotes` whose placeholder is outside single quotes, as the C#
/// `NormalizeParameterQuoting`: an imported file cannot make a value go in bare.
fn normalize_quoting(action: &mut UserCommand) {
    for template in [
        action.windows_command_template.as_mut(),
        action.linux_command_template.as_mut(),
    ]
    .into_iter()
    .flatten()
    {
        let pattern = template.command_pattern.clone();
        for parameter in &mut template.parameters {
            if parameter.quoting == Some(Quoting::InlineInQuotes)
                && !placeholder_in_single_quotes(&pattern, &parameter.name)
            {
                parameter.quoting = Some(Quoting::ShellQuote);
            }
        }
    }
}

/// A new identity, a version 4 UUID as the C# `Guid.NewGuid`; a time-based text when the
/// system gives no random bytes.
fn new_identity() -> String {
    uuid_generator::generate(UuidVersion::V4).map_or_else(
        |_| format!("user-{}", now_seconds()),
        |uuid| uuid_generator::format(uuid, UuidFormat::default()),
    )
}

/// The seconds since the epoch.
fn now_seconds() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| i64::try_from(since.as_secs()).unwrap_or(0))
}

/// Now in UTC as .NET writes a `DateTime`.
fn now_text() -> String {
    round_trip_utc(Instant::from_unix_seconds(now_seconds()))
}

#[cfg(test)]
mod tests;
