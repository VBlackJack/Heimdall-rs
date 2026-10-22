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

mod quoting;

use quoting::{
    POWERSHELL_SINGLE_QUOTES, POWERSHELL_TYPOGRAPHIC_DOUBLE_QUOTES, QuoteContext,
    checked_placeholder_contexts, escape_character,
};

/// The file of the user's commands, beside the profiles file.
pub const LIBRARY_FILE: &str = "command-library.json";

/// The envelope's version, as the C# export writes it.
pub const SCHEMA_VERSION: &str = "1.0";

/// The longest value of a parameter, as the C# `MaxParameterLength`.
pub const MAX_PARAMETER_LENGTH: usize = 1024;

/// The longest generated command, as the C# `MaxCommandLength`.
pub const MAX_COMMAND_LENGTH: usize = 4096;

/// The longest path value, as the C# `MaxPathLength` (the Windows `MAX_PATH`).
pub const MAX_PATH_LENGTH: usize = 260;

/// The most parameters of a template, as the C# `MaxParametersPerTemplate`.
pub const MAX_PARAMETERS: usize = 50;

/// The longest title, as the C# `MaxActionTitleLength`.
pub const MAX_TITLE_LENGTH: usize = 200;

/// The longest description, as the C# `MaxActionDescriptionLength`.
pub const MAX_DESCRIPTION_LENGTH: usize = 2000;

/// The longest category, as the C# `MaxActionCategoryLength`.
pub const MAX_CATEGORY_LENGTH: usize = 100;

/// The longest command pattern, as the C# `MaxTemplateCommandPatternLength`.
pub const MAX_PATTERN_LENGTH: usize = 4000;

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

/// The punctuation, besides ASCII letters and digits, a value substituted bare may hold, as
/// the C# `SafeBareValueRegex` (`CommandGeneratorService.cs`): none of it splits, expands or
/// substitutes in bash or PowerShell. See [`is_safe_bare_value`].
const SAFE_BARE_PUNCTUATION: [char; 10] = ['.', '_', '/', ':', '=', '@', '+', ',', '%', '-'];

/// The characters a number value may hold: digits, a decimal point, an exponent and signs.
/// Stricter than the Rust float parser, which also reads `inf` and `NaN`.
const NUMBER_CHARACTERS: [char; 5] = ['.', 'e', 'E', '+', '-'];

/// What a command line reads as an option when it starts a value.
const OPTION_LEADING: char = '-';

/// What `cmd` and many Windows tools read as a switch when it starts a value.
const WINDOWS_SWITCH_LEADING: char = '/';

/// The character a Windows value may not end with: a native program's argument parser reads
/// it as an escape of the closing quote.
const WINDOWS_BACKSLASH: char = '\\';

/// What PowerShell reads specially at the start of an argument: `-` names a parameter and
/// `@` splats a variable.
const POWERSHELL_LEADING_SPECIAL: [char; 2] = ['-', '@'];

/// What PowerShell reads between words: a comma builds an array out of two arguments.
const POWERSHELL_ARRAY_SEPARATOR: char = ',';

/// What may stand right before a bare value besides letters, digits and white space. Not `-`
/// (a PowerShell parameter name, an option), not `@` (PowerShell splats `@name`), not a
/// comma (it builds an array), not a bracket or a quote: next to those the value would join
/// the syntax (`{{v}}` with `a,b` is a brace expansion, `({v})` with `1+2` is an expression).
const BARE_NEIGHBOUR_BEFORE: [char; 7] = ['.', '_', '/', ':', '=', '+', '%'];

/// What the white-space delimited word around a bare value may not hold (`{pre{v}post,end}`
/// is a brace expansion, `@prefix{v}` a splat, `(1+2)` an expression), the placeholders of
/// other parameters left aside.
const WORD_SYNTAX_FOR_BARE: [char; 19] = [
    '{', '}', '(', ')', '[', ']', ',', ';', '&', '|', '<', '>', '$', '`', '\'', '"', '!', '#', '\\',
];

/// The character that makes a word of the pattern a PowerShell splat when it starts it.
const SPLAT_CHARACTER: char = '@';

/// What starts an option or a PowerShell parameter name.
const OPTION_PREFIX: char = '-';

/// What binds a value to the option before it (`--opt=value`, `-Name:value`).
const OPTION_VALUE_BINDERS: [char; 2] = ['=', ':'];

/// What stands for a placeholder of another parameter when a word of the pattern is read.
const PLACEHOLDER_STAND_IN: char = 'P';

/// What may stand right after a bare value besides letters, digits and white space.
const BARE_NEIGHBOUR_AFTER: [char; 9] = ['.', '_', '/', ':', '=', '+', '%', '-', '@'];

/// The characters a value read as plain shell words may not hold besides the metacharacters:
/// globs, brace and tilde expansion, history, comments and the comma of a brace list.
const WORD_SYNTAX_CHARACTERS: [char; 10] = ['*', '?', '[', ']', '{', '}', '~', '!', '#', ','];

/// What a removed placeholder leaves around it.
const WORD_SEPARATOR: char = ' ';

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
/// a blank value is its default. An optional value still blank is left out of the command
/// together with the space it leaves, instead of being quoted into an empty argument.
///
/// The leaving out happens first, and every value is judged against the pattern that is left
/// (`echo ${empty}'{v}'` becomes `echo $'{v}'`, a bash string that reads backslash escapes).
/// Every placeholder is then replaced in one pass over that pattern, so a value that happens
/// to hold the text `{other}` stays that text: replaced one parameter after another it would
/// be read as the placeholder of the next, and the next value would land inside the quotes
/// of the first.
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
    let entries: Vec<(&UserParameter, &str)> = template
        .parameters
        .iter()
        .map(|parameter| {
            let value = value_of(&parameter.name)
                .filter(|typed| !typed.trim().is_empty())
                .or(parameter.default_value.as_deref())
                .unwrap_or("");
            (parameter, value)
        })
        .collect();
    let left_out: Vec<Substitution> = entries
        .iter()
        .filter(|(_, value)| value.trim().is_empty())
        .map(|(parameter, _)| Substitution {
            placeholder: placeholder_text(&parameter.name),
            replacement: None,
        })
        .collect();
    let pattern = substitute(&template.command_pattern, &left_out);
    let mut substitutions: Vec<Substitution> = Vec::new();
    let mut refused = Vec::new();
    for (parameter, value) in entries {
        match check_value(&pattern, parameter, value, platform) {
            Ok(()) => {
                if value.trim().is_empty() {
                    continue;
                }
                let placeholder = placeholder_text(&parameter.name);
                // The first parameter of a name owns its placeholders, as in the C# where
                // the second finds nothing left to replace.
                if substitutions
                    .iter()
                    .any(|known| known.placeholder == placeholder)
                {
                    continue;
                }
                let replacement = quote_value(&pattern, parameter, value, platform);
                substitutions.push(Substitution {
                    placeholder,
                    replacement: Some(replacement),
                });
            }
            Err(error) => refused.push(error),
        }
    }
    if !refused.is_empty() {
        return Err(GenerateError::Values(refused));
    }
    let command = substitute(&pattern, &substitutions);
    if command.chars().count() > MAX_COMMAND_LENGTH {
        return Err(GenerateError::TooLong);
    }
    Ok(command)
}

/// The text of the placeholder of `name` in a pattern.
fn placeholder_text(name: &str) -> String {
    format!("{{{name}}}")
}

/// One placeholder of a pattern and what takes its place.
struct Substitution {
    /// The text looked for, `{name}`.
    placeholder: String,
    /// The value as it goes into the command; `None` leaves the argument out.
    replacement: Option<String>,
}

/// `pattern` with every placeholder of `substitutions` replaced in one pass.
///
/// A placeholder left out takes with it the run of spaces it leaves, as the C#
/// `RemoveEmptyPlaceholder`: only the spaces next to it are touched (two spaces the author
/// wrote inside a quoted grep expression stay), and one that ends the command takes the
/// space before it along, so `hostname {name}` becomes `hostname`.
fn substitute(pattern: &str, substitutions: &[Substitution]) -> String {
    let mut result = String::with_capacity(pattern.len());
    let mut rest = pattern;
    while let Some(next) = rest.chars().next() {
        let hit = substitutions
            .iter()
            .find(|each| rest.starts_with(each.placeholder.as_str()));
        match hit {
            Some(Substitution {
                placeholder,
                replacement: Some(value),
            }) => {
                result.push_str(value);
                rest = &rest[placeholder.len()..];
            }
            Some(Substitution {
                placeholder,
                replacement: None,
            }) => {
                rest = &rest[placeholder.len()..];
                if result.is_empty() || result.ends_with(WORD_SEPARATOR) {
                    rest = rest.trim_start_matches(WORD_SEPARATOR);
                }
                if rest.is_empty() {
                    let kept = result.trim_end_matches(WORD_SEPARATOR).len();
                    result.truncate(kept);
                }
            }
            None => {
                result.push(next);
                rest = &rest[next.len_utf8()..];
            }
        }
    }
    result
}

/// Checks `value` against `parameter`, as the C# `ValidateParameterValue` and
/// `ValidateParameters` together; a blank value passes unless required. `pattern` and
/// `platform` say where the value will stand, which decides whether the shell characters
/// are refused.
fn check_value(
    pattern: &str,
    parameter: &UserParameter,
    value: &str,
    platform: TemplatePlatform,
) -> Result<(), ValueError> {
    let label = || {
        if parameter.label.trim().is_empty() {
            parameter.name.clone()
        } else {
            parameter.label.clone()
        }
    };
    // A control character is refused for every type, the unknown ones included: a line feed
    // submits the command on the terminal before it is read, and no quoting stops that.
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
    let kind = parameter.kind.to_lowercase();
    match kind.as_str() {
        "int" | "integer" => ensure(
            value.trim().parse::<i32>().is_ok() && !negative_on_linux(value, platform),
            || ValueError::NotInteger(label()),
        )?,
        "number" => ensure(
            value.trim().parse::<f64>().is_ok()
                && value
                    .trim()
                    .chars()
                    .all(|c| c.is_ascii_digit() || NUMBER_CHARACTERS.contains(&c))
                && !negative_on_linux(value, platform),
            || ValueError::NotNumber(label()),
        )?,
        "hostname" => ensure(is_hostname(value), || ValueError::Hostname(label()))?,
        "ipaddress" => ensure(IpAddr::from_str(value.trim()).is_ok(), || {
            ValueError::IpAddress(label())
        })?,
        "driveletter" => ensure(
            value.len() == 1 && value.chars().all(|c| c.is_ascii_alphabetic()),
            || ValueError::DriveLetter(label()),
        )?,
        // Judged by its shape, never by where it points: the command runs on another
        // machine, so resolving the value on this one would say nothing about it.
        "path" => ensure(value.chars().count() <= MAX_PATH_LENGTH, || {
            ValueError::Path(label())
        })?,
        // A string, and any type this build does not know (a name typed by hand into an
        // imported file): quoted like a string, so it gets at least the checks no quoting
        // replaces.
        _ => {}
    }
    // Stricter than the C#: the shape of a free text value, whatever the quoting around it.
    if !parameter.is_choice()
        && !typed_kind(&kind)
        && holds_refused_shape(value, kind.as_str() == "path", platform)
    {
        return Err(ValueError::Dangerous(label()));
    }
    // The place the value stands in is judged for every type, after the type: a valid number
    // or address is still a word that can be a heredoc delimiter, and typed values go in
    // as they are (trimmed), so that is what is judged.
    let typed = typed_kind(&kind);
    let judged = if typed { value.trim() } else { value };
    ensure(
        !holds_refused_characters(pattern, parameter, judged, platform)
            && (!typed || typed_fits_around(pattern, parameter, judged, platform)),
        || ValueError::Dangerous(label()),
    )
}

/// Whether a number is negative on Linux, where a leading dash is an option to the program
/// (`kill -s KILL -1`); PowerShell reads a negative number as a number.
fn negative_on_linux(value: &str, platform: TemplatePlatform) -> bool {
    platform == TemplatePlatform::Linux && value.trim_start().starts_with(OPTION_LEADING)
}

/// Whether `kind` is one of the typed kinds, whose values always go in bare.
fn typed_kind(kind: &str) -> bool {
    matches!(
        kind,
        "int" | "integer" | "number" | "hostname" | "ipaddress" | "driveletter"
    )
}

/// Whether a free text value (a string, a path, an unknown type) has a shape the generator
/// refuses at any place of the pattern, a deliberate tightening of the C# where the quoting
/// of the pattern decides. A second-stage shell (`ssh host`, `sudo sh -c`, `cmd /c`) reads the
/// value again after the first one, so the quotes protect nothing there:
///
/// - a `-` (both platforms) or, for a string on Windows, a `/` that starts the value or follows
///   white space, which the
///   program reads as an option or a switch (`ssh -oProxyCommand=x`); a path may start with
///   a drive letter or `\\server`, so it is not judged for the `/`;
/// - on Linux a command substitution `$(`, a backtick, or `[` with `$`, which bash arithmetic
///   (`let n={v}`, `[[ ]]`, `$(( ))`) evaluates a second time even from a quoted value;
/// - on Windows a double quote of any kind and a trailing backslash, which a native program's
///   argument parser reads as an escape of the closing quote.
fn holds_refused_shape(value: &str, is_path: bool, platform: TemplatePlatform) -> bool {
    let windows = platform == TemplatePlatform::Windows;
    // The start of the value and every start of a word in it: a second-stage shell splits the
    // value on white space (`ssh h find {dir}` with `/ -exec cmd {} +`).
    let word_starts = || {
        std::iter::once(value.trim_start().chars().next())
            .chain(
                value
                    .chars()
                    .zip(value.chars().skip(1))
                    .map(|(before, after)| before.is_whitespace().then_some(after)),
            )
            .flatten()
    };
    if word_starts().any(|c| c == OPTION_LEADING) {
        return true;
    }
    if windows {
        (!is_path && word_starts().any(|c| c == WINDOWS_SWITCH_LEADING))
            || value.ends_with(WINDOWS_BACKSLASH)
            || value
                .chars()
                .any(|c| c == '"' || POWERSHELL_TYPOGRAPHIC_DOUBLE_QUOTES.contains(&c))
    } else {
        value.contains("$(") || value.contains('`') || (value.contains('[') && value.contains('$'))
    }
}

/// Whether a typed value, which always goes in bare, can stand at the bare places of the
/// pattern without joining the text around it into a switch (`Probe -{v}` with the host name
/// `Force`), a splat (`@prefix{v}`) or another syntax. A negative number is a number to
/// PowerShell, not a parameter name, so the value's own leading dash is not judged here.
fn typed_fits_around(
    pattern: &str,
    parameter: &UserParameter,
    value: &str,
    platform: TemplatePlatform,
) -> bool {
    let contexts = checked_placeholder_contexts(
        pattern,
        &parameter.name,
        platform == TemplatePlatform::Windows,
    );
    // Where the lexer is lost the other checks already hold the value to a plain word.
    contexts.contains(&QuoteContext::Uncertain)
        || bare_fits_around(pattern, parameter, value, platform, false, false)
}

/// `Ok` when `holds`, else the error `otherwise` makes.
fn ensure(holds: bool, otherwise: impl FnOnce() -> ValueError) -> Result<(), ValueError> {
    if holds { Ok(()) } else { Err(otherwise()) }
}

/// How far the generator's own quoting protects a value at one place of the pattern.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Protection {
    /// The value ends up inside a literal single-quoted span: nothing in it can end the span
    /// but a single quote, which both escapers double or close and reopen.
    Literal,
    /// The value is inside a double-quoted string: the shell still expands `$(...)` and
    /// backticks there, and a double quote or an escape character ends or bends the string.
    DoubleQuoted,
    /// The value is read as shell words (a quote the pattern did not open, an ANSI-C string,
    /// a dollar sign): any character the shell gives a meaning is refused.
    Words,
}

/// The protection of a value of `parameter` at a place the shell reads as `context`; `None`
/// where the lexer does not follow the pattern, and only a plain value put in bare is safe.
fn protection_of(context: QuoteContext, parameter: &UserParameter) -> Option<Protection> {
    let inline = parameter.quoting == Some(Quoting::InlineInQuotes);
    match (context, inline) {
        // `ShellQuote` wraps the value in single quotes itself, literal when the placeholder
        // stands bare; `InlineInQuotes` only escapes it and relies on the pattern's quotes.
        (QuoteContext::Bare, false) | (QuoteContext::SingleQuoted, true) => {
            Some(Protection::Literal)
        }
        (QuoteContext::DoubleQuoted, _) => Some(Protection::DoubleQuoted),
        (QuoteContext::Uncertain | QuoteContext::Document, _) => None,
        _ => Some(Protection::Words),
    }
}

/// Whether `c` cannot be put in a value that is `protection` protected.
fn refused_at(protection: Protection, c: char, powershell: bool, windows_path: bool) -> bool {
    // Parentheses are common in Windows paths (`C:\Program Files (x86)`) and cmd is the only
    // second-stage shell that reads them, inside quotes they stay literal.
    let metacharacter = DANGEROUS_CHARACTERS.contains(&c)
        && !(windows_path && protection == Protection::Literal && matches!(c, '(' | ')'));
    let ends_double = c == '"'
        || c == escape_character(powershell)
        || (powershell && POWERSHELL_TYPOGRAPHIC_DOUBLE_QUOTES.contains(&c));
    match protection {
        // Stricter than the C#: a second-stage shell reads the value again (see
        // `holds_refused_shape`), so the metacharacters stay refused inside single quotes.
        Protection::Literal => metacharacter,
        Protection::DoubleQuoted => metacharacter || ends_double || (!powershell && c == '!'),
        Protection::Words => {
            metacharacter
                || ends_double
                || c == '\\'
                || c.is_whitespace()
                || POWERSHELL_SINGLE_QUOTES.contains(&c)
                || WORD_SYNTAX_CHARACTERS.contains(&c)
        }
    }
}

/// Whether `value` holds a character the generator will not put in for `parameter`: a shell
/// metacharacter, where the quoting cannot neutralize it; as the C#
/// `ContainsRefusedCharacters` with `RequiresCharacterBlacklist`.
///
/// `ShellQuote` wraps the value in single quotes itself, which is literal only when the
/// placeholder stands bare in the pattern: inside a double-quoted string the shell still
/// expands `$(...)` between those quotes. `InlineInQuotes` only escapes the value, so it is
/// literal only when every occurrence of the placeholder sits in a single-quoted span that
/// no double-quoted string encloses. In both literal cases the only character that could
/// end the span is a single quote, which both escapers double or close and reopen; that is
/// why the quoting alone is enough there. Stricter than the C#, the metacharacters are still
/// refused in those places (LDAP filters, grep alternations and `$env:TEMP` do not pass),
/// because a second-stage shell reads the value again; only the parentheses of a Windows
/// path are let through. Where the lexer does not follow the pattern ([`QuoteContext::Uncertain`])
/// only a value of plain characters is accepted, and it goes in bare.
fn holds_refused_characters(
    pattern: &str,
    parameter: &UserParameter,
    value: &str,
    platform: TemplatePlatform,
) -> bool {
    let powershell = platform == TemplatePlatform::Windows;
    let windows_path = powershell && parameter.kind.eq_ignore_ascii_case("path");
    let contexts = checked_placeholder_contexts(pattern, &parameter.name, powershell);
    // No substitution is trusted inside a document the lexer does not follow.
    if contexts.contains(&QuoteContext::Document) {
        return true;
    }
    if contexts.contains(&QuoteContext::Uncertain) {
        return !bare_fits(pattern, parameter, value, platform, true);
    }
    // `InlineInQuotes` brings no quote of its own: an occurrence outside a single-quoted
    // span (or a double-quoted one, which has its own checks) is a bare word, and the value
    // there must be one that is safe bare, glued to nothing (`Write-Output @args` splats,
    // `-NoEnumerate` is a switch).
    let outside = contexts.iter().any(|context| {
        matches!(
            context,
            QuoteContext::Bare | QuoteContext::AnsiC | QuoteContext::Expanding
        )
    });
    if parameter.quoting == Some(Quoting::InlineInQuotes)
        && outside
        && !bare_fits(pattern, parameter, value, platform, false)
    {
        return true;
    }
    contexts
        .into_iter()
        .filter_map(|context| protection_of(context, parameter))
        .any(|protection| {
            value
                .chars()
                .any(|c| refused_at(protection, c, powershell, windows_path))
        })
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

/// `value` as it goes into the command: typed values as they are, strings and paths bare
/// where nothing in them is readable by the shell, else quoted or escaped for `platform`'s
/// shell, as the C# `EscapeParameterValue`.
fn quote_value(
    pattern: &str,
    parameter: &UserParameter,
    value: &str,
    platform: TemplatePlatform,
) -> String {
    let uncertain = checked_placeholder_contexts(
        pattern,
        &parameter.name,
        platform == TemplatePlatform::Windows,
    )
    .contains(&QuoteContext::Uncertain);
    match parameter.kind.to_lowercase().as_str() {
        "hostname" | "ipaddress" | "driveletter" | "int" | "integer" | "number" => {
            value.trim().to_owned()
        }
        // Where the pattern is not followed, no quote is added and none is escaped: the value
        // was accepted only because it holds plain characters.
        _ if uncertain => value.to_owned(),
        // The quotes are the pattern's: only the content is escaped.
        _ if parameter.quoting == Some(Quoting::InlineInQuotes) => escape_content(value, platform),
        _ if bare_fits(pattern, parameter, value, platform, false) => value.to_owned(),
        _ => format!("'{}'", escape_content(value, platform)),
    }
}

/// Whether `value` can be substituted bare at every occurrence of its placeholder.
///
/// It has to be a safe value ([`is_safe_bare_value`]) and every occurrence has to stand
/// between text that cannot join it into syntax: `{{v}}` with `a,b` would be the bash brace
/// expansion `{a,b}`, `({v})` with `1+2` the PowerShell expression `(1+2)`, `@{v}` a
/// PowerShell splat, and a value holding `=` at the start of a command an assignment.
/// Unless `unfollowed` (the lexer cannot say where the occurrences stand and a value will be
/// put in bare whatever they are), every occurrence also has to stand outside quotes, after
/// no dollar sign and no escape; in that mode a quote next to the value is fine, it only
/// closes or opens a string around plain text.
fn bare_fits(
    pattern: &str,
    parameter: &UserParameter,
    value: &str,
    platform: TemplatePlatform,
    unfollowed: bool,
) -> bool {
    is_safe_bare_value(value, platform)
        && bare_fits_around(pattern, parameter, value, platform, unfollowed, true)
}

/// The part of [`bare_fits`] about the places of the placeholder, not the value's characters.
/// With `every_place` false the occurrences inside quotes are left alone (a typed value is
/// made of plain characters that no quote span can be broken by) and only the bare ones are
/// judged.
fn bare_fits_around(
    pattern: &str,
    parameter: &UserParameter,
    value: &str,
    platform: TemplatePlatform,
    unfollowed: bool,
    every_place: bool,
) -> bool {
    let placeholder = placeholder_text(&parameter.name);
    let contexts = checked_placeholder_contexts(
        pattern,
        &parameter.name,
        platform == TemplatePlatform::Windows,
    );
    let occurrences: Vec<(usize, &str)> = pattern.match_indices(&placeholder).collect();
    if occurrences.len() != contexts.len() {
        return false;
    }
    // A heredoc ends on a line holding only its delimiter: a value alone on its line could be
    // that line, and the lexer does not follow scripts.
    if unfollowed && pattern.lines().any(|line| line.trim() == placeholder) {
        return false;
    }
    // Where the lexer cannot say what surrounds a value, it goes in bare only as a word of its
    // own: next to white space or a quote, never glued to text that could join it into a
    // delimiter (`END_{v}` ends a heredoc called `END_CONFIG`), a switch or a splat.
    let standalone = |c: char| c.is_whitespace() || matches!(c, '\'' | '"');
    let neighbour_ok = |c: char, allowed: &[char]| {
        c.is_whitespace() || c.is_ascii_alphanumeric() || allowed.contains(&c)
    };
    occurrences
        .iter()
        .zip(&contexts)
        .all(|((at, text), context)| {
            if !every_place && !unfollowed && *context != QuoteContext::Bare {
                return true;
            }
            let before = pattern[..*at].chars().next_back();
            let after = pattern[at + text.len()..].chars().next();
            let around_ok = if unfollowed {
                before.is_none_or(standalone) && after.is_none_or(standalone)
            } else {
                *context == QuoteContext::Bare
                    && word_keeps_bare_value_plain(pattern, *at, text.len())
                    && before.is_none_or(|c| neighbour_ok(c, &BARE_NEIGHBOUR_BEFORE))
                    && after.is_none_or(|c| neighbour_ok(c, &BARE_NEIGHBOUR_AFTER))
            };
            around_ok && !(value.contains('=') && before.is_none_or(char::is_whitespace))
        })
}

/// Whether the white-space delimited word of `pattern` around the placeholder at `at`
/// (`len` bytes long) holds nothing a bare value could join into syntax: no bracket, comma,
/// quote or dollar sign (`{pre{v}post,end}` is a bash brace expansion, `(1+2)` a PowerShell
/// expression), and no leading `@` (`@prefix{v}` is a PowerShell splat). The placeholders of
/// other parameters in the word are read as a letter.
fn word_keeps_bare_value_plain(pattern: &str, at: usize, len: usize) -> bool {
    let start = pattern[..at]
        .char_indices()
        .rev()
        .find(|(_, c)| c.is_whitespace())
        .map_or(0, |(index, c)| index + c.len_utf8());
    let after = at + len;
    let end = pattern[after..]
        .find(char::is_whitespace)
        .map_or(pattern.len(), |offset| after + offset);
    let word = &pattern[start..end];
    let mut plain = String::with_capacity(word.len());
    let mut rest = word;
    while let Some(c) = rest.chars().next() {
        let other_placeholder = c == '{'
            && rest.find('}').is_some_and(|close| {
                let name = &rest[1..close];
                name.chars()
                    .next()
                    .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
                    && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
            });
        if other_placeholder {
            plain.push(PLACEHOLDER_STAND_IN);
            rest = &rest[rest.find('}').map_or(1, |close| close + 1)..];
        } else {
            plain.push(c);
            rest = &rest[c.len_utf8()..];
        }
    }
    // A word that starts with a dash is an option or a PowerShell parameter name: glued to its
    // letters (`-Fo{v}` with `rce`) the value would complete the name, so only a value after
    // the `=` or `:` that binds it is a value.
    let completes_a_name =
        word.starts_with(OPTION_PREFIX) && !pattern[start..at].contains(OPTION_VALUE_BINDERS);
    !completes_a_name
        && !plain.starts_with(SPLAT_CHARACTER)
        && !plain.contains(WORD_SYNTAX_FOR_BARE)
}

/// Whether `value` means the same to the shell with or without quotes, so it can be
/// substituted bare (`--add-port=8080/tcp` rather than `--add-port='8080'/'tcp'`), as the
/// C# `IsSafeBareValue`.
///
/// The class is ASCII letters and digits plus [`SAFE_BARE_PUNCTUATION`]: no space, no quote,
/// no glob character, no `$`, no backslash and no tilde, so no shell splits, expands or
/// substitutes anything in such a value. PowerShell reads three of those characters
/// differently at the start of an argument or between words, so on Windows those values
/// keep their quotes: a leading `-` makes a parameter name of the value, a leading `@`
/// splats a variable, and a comma builds an array of two arguments out of one. Bash gives
/// none of them a meaning.
fn is_safe_bare_value(value: &str, platform: TemplatePlatform) -> bool {
    let in_class = !value.is_empty()
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || SAFE_BARE_PUNCTUATION.contains(&c));
    if !in_class {
        return false;
    }
    platform != TemplatePlatform::Windows
        || (!value.starts_with(POWERSHELL_LEADING_SPECIAL)
            && !value.contains(POWERSHELL_ARRAY_SEPARATOR))
}

/// `value` escaped to sit between single quotes of `platform`'s shell.
fn escape_content(value: &str, platform: TemplatePlatform) -> String {
    match platform {
        TemplatePlatform::Windows => value
            .chars()
            .flat_map(|c| {
                let doubled = POWERSHELL_SINGLE_QUOTES.contains(&c);
                std::iter::once(c).chain(doubled.then_some(c))
            })
            .collect(),
        TemplatePlatform::Linux => value.replace('\'', "'\\''"),
    }
}

/// Whether `{name}` is written in `pattern` and every occurrence of it sits in a literal
/// single-quoted span of `platform`'s shell, as the C#
/// `CommandPatternQuoting.IsPlaceholderOnlyInSingleQuotedSpans`: an `InlineInQuotes` value
/// anywhere else would be put in with no quote at all. Read off the same lexer the
/// generator uses, not a count of single quotes: the count called
/// `-Filter "Name -like '*{x}*'"` single-quoted although the shell expands `$(...)` there.
#[must_use]
pub fn placeholder_in_single_quotes(pattern: &str, name: &str, platform: TemplatePlatform) -> bool {
    let contexts =
        checked_placeholder_contexts(pattern, name, platform == TemplatePlatform::Windows);
    !contexts.is_empty()
        && contexts
            .iter()
            .all(|context| *context == QuoteContext::SingleQuoted)
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

    /// The draft of a new command that starts as a copy of `command`, as the C#
    /// `CommandActionDialogViewModel.AsCopyOf`: it has no identity, so saving it makes a
    /// command of its own, and it is called `title`.
    #[must_use]
    pub fn copy_of(command: &UserCommand, title: String) -> Self {
        Self {
            id: None,
            title,
            ..Self::of(command)
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
            && !placeholder_in_single_quotes(pattern, &parameter.name, platform)
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
    for (template, platform) in [
        (
            action.windows_command_template.as_mut(),
            TemplatePlatform::Windows,
        ),
        (
            action.linux_command_template.as_mut(),
            TemplatePlatform::Linux,
        ),
    ] {
        let Some(template) = template else {
            continue;
        };
        let pattern = template.command_pattern.clone();
        for parameter in &mut template.parameters {
            if parameter.quoting == Some(Quoting::InlineInQuotes)
                && !placeholder_in_single_quotes(&pattern, &parameter.name, platform)
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

#[cfg(test)]
mod generator_tests;
