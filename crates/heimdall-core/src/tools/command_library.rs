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

//! The command library's shipped catalogue: the actions of the C# Heimdall's seed
//! (`data/seed/actions/*.json`), one file each, embedded at build time from
//! `data/command-catalogue/actions` (see the README beside it for the C# commit they were
//! copied from, so a later sync is a copy of the files).
//!
//! An entry is a [`UserCommand`] (the C# `Action`, same JSON) plus its `translations`: the
//! English is the pivot text of the file, the French sits under `translations.fr` and the
//! Spanish under `translations.es` (titles and descriptions only in the shipped catalogue). A
//! part a language has no text for, and every part of a language with no block, shows the
//! pivot, as `ActionLocalizer.cs` does. A user's command that carries `translations` (an
//! imported pack) is shown the same way ([`localize_user`]). A category is a stable key
//! (`active-directory`), its label written by the window in its language.
//!
//! Nothing here runs a command. A file the user hands over (the library's Import) is
//! untrusted: [`validate_import`] holds it to the same checks as the shipped entries and
//! to the generator's, and the commands it keeps are only ever shown, copied or sent after
//! the user's own question.

use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap};
use std::sync::OnceLock;

use serde::Deserialize;
use serde_json::Value;

use super::user_commands::{
    CHOICE_TYPE, GenerateError, MAX_CATEGORY_LENGTH, MAX_DESCRIPTION_LENGTH, MAX_PARAMETERS,
    MAX_PATTERN_LENGTH, MAX_TAGS, MAX_TITLE_LENGTH, TemplatePlatform, UserCommand, UserParameter,
    UserPlatform, UserTemplate, ValueError, generate,
};
use crate::profile::RemoteOs;

/// The files of the catalogue, in the order of their names; written by `build.rs`.
const CATALOGUE_FILES: &[&str] = include!(concat!(env!("OUT_DIR"), "/command_catalogue_files.rs"));

/// The language the pivot text of a file is written in.
pub const PIVOT_LANGUAGE: &str = "en";

/// The length of a language code in a file: `fr`, `FR` and `fr-FR` are one language.
const LANGUAGE_CODE_LENGTH: usize = 2;

/// The field of an action that holds its text in each language.
const TRANSLATIONS_FIELD: &str = "translations";

/// The categories the catalogue uses, as the C# `CommandCategories`, in the order they are
/// listed.
pub const CATEGORY_KEYS: [&str; 13] = [
    "security",
    "active-directory",
    "services-automation",
    "windows-updates",
    "monitoring-logs",
    "files-storage",
    "network-dns",
    "package-management",
    "development",
    "performance",
    "containers-vms",
    "database",
    "windows-debloat",
];

/// The parameter types a command of the library can ask for: the types the generator
/// checks (`generate`), plus `text`, which it reads as a free string.
pub const SUPPORTED_PARAMETER_TYPES: [&str; 10] = [
    "string",
    "text",
    "path",
    "int",
    "integer",
    "number",
    "hostname",
    "ipaddress",
    "driveletter",
    "choice",
];

/// The kept names of the ten commands the first versions of the library had, and the entry
/// of the catalogue that took their place (the same command, as the C# seed writes it).
/// `linux-kill-port` (`fuser -k`) has no entry in the seed: a favorite or a history row
/// that names it stays in the file and finds no command, as for any command gone.
pub const LEGACY_SNIPPET_IDS: [(&str, &str); 9] = [
    ("linux-listening-ports", "ss-sockets"),
    ("linux-journal-follow", "journalctl-follow"),
    ("linux-cert-expiry", "openssl-test-tls"),
    ("windows-test-connection", "unified-network-test-port"),
    ("windows-port-owner", "unified-network-connections"),
    ("windows-restart-service", "unified-service-restart"),
    ("windows-flush-dns", "dns-flush-cache"),
    ("mtr-probe", "network-trace-route"),
    ("nmap-service-scan", "nmap-port-scan"),
];

/// The text of a parameter in a language.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct ParameterText {
    /// What the window calls it.
    pub label: Option<String>,
    /// What it is for.
    pub description: Option<String>,
}

/// The parameters of a translation, by template and by the name of the parameter.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct TranslatedParameters {
    /// The PowerShell template's.
    pub windows: BTreeMap<String, ParameterText>,
    /// The POSIX shell template's.
    pub linux: BTreeMap<String, ParameterText>,
}

/// An action's text in one language, each part optional: a missing one shows the pivot.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct Translation {
    /// The title.
    pub title: Option<String>,
    /// What it does.
    pub description: Option<String>,
    /// The notes.
    pub notes: Option<String>,
    /// The parameters' text.
    pub parameters: TranslatedParameters,
    /// What each example does, by the example's command.
    pub examples: BTreeMap<String, String>,
}

/// Why the catalogue (or a file of it) could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CatalogueError {
    /// The file at this place is not an action.
    Entry(usize),
    /// Two files carry this identity.
    Duplicate(String),
}

/// One action of the catalogue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogueEntry {
    command: UserCommand,
    translations: BTreeMap<String, Translation>,
    /// The `translations` object as the file wrote it, to export the action whole.
    raw_translations: Option<Value>,
}

/// The base of a language code: two lower-case letters, `fr-FR` and `FR` both `fr`.
#[must_use]
pub fn normalize_language(language: &str) -> String {
    language
        .trim()
        .chars()
        .take(LANGUAGE_CODE_LENGTH)
        .flat_map(char::to_lowercase)
        .collect()
}

/// `translated` when it says something, else `pivot`, as the C# `Pick`.
fn pick(translated: Option<&String>, pivot: &str) -> String {
    translated
        .filter(|text| !text.trim().is_empty())
        .map_or_else(|| pivot.to_owned(), Clone::clone)
}

impl CatalogueEntry {
    /// Reads one action file; `None` when `text` is not a JSON object of an action.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let mut value: Value = serde_json::from_str(text).ok()?;
        let object = value.as_object_mut()?;
        let mut translations = BTreeMap::new();
        let raw_translations = object.remove(TRANSLATIONS_FIELD);
        if let Some(Value::Object(languages)) = raw_translations.clone() {
            for (language, each) in languages {
                let translation: Translation = serde_json::from_value(each).ok()?;
                translations.insert(normalize_language(&language), translation);
            }
        }
        let mut command: UserCommand = serde_json::from_value(value).ok()?;
        command.is_user_created = false;
        Some(Self {
            command,
            translations,
            raw_translations,
        })
    }

    /// The action as the C# `Action` is written, its `translations` included, for an export.
    #[must_use]
    pub fn to_value(&self) -> Value {
        let mut value = serde_json::to_value(&self.command).unwrap_or_default();
        if let (Some(object), Some(translations)) = (value.as_object_mut(), &self.raw_translations)
        {
            object.insert(TRANSLATIONS_FIELD.to_owned(), translations.clone());
        }
        value
    }

    /// The action, in its pivot text.
    #[must_use]
    pub fn command(&self) -> &UserCommand {
        &self.command
    }

    /// The languages the entry has a translation for.
    pub fn languages(&self) -> impl Iterator<Item = &str> {
        self.translations.keys().map(String::as_str)
    }

    /// The translation into `language`; `None` for the pivot and for a language the entry
    /// has no text in.
    fn translation(&self, language: &str) -> Option<&Translation> {
        self.translations.get(&translated_language(language)?)
    }

    /// The action in `language`: the translated text where the entry has some and the pivot
    /// everywhere else, as the C# `ActionLocalizer.Localize`; borrowed when there is no
    /// translation to apply.
    #[must_use]
    pub fn localized(&self, language: &str) -> Cow<'_, UserCommand> {
        match self.translation(language) {
            Some(translation) => Cow::Owned(translated(&self.command, translation)),
            None => Cow::Borrowed(&self.command),
        }
    }

    /// The notes in `language`, the pivot's when there are none translated.
    #[must_use]
    pub fn notes(&self, language: &str) -> Option<String> {
        let pivot = self.command.extra.get("notes").and_then(Value::as_str);
        let translated = self
            .translation(language)
            .and_then(|translation| translation.notes.as_deref())
            .filter(|notes| !notes.trim().is_empty());
        translated.or(pivot).map(str::to_owned)
    }
}

/// The language `language` names when it is not the pivot: `None` for the pivot and for no
/// language, whose text is the file's own.
fn translated_language(language: &str) -> Option<String> {
    let language = normalize_language(language);
    (!language.is_empty() && language != PIVOT_LANGUAGE).then_some(language)
}

/// `command` with the texts of `translation`: each part it says something for, the pivot's
/// everywhere else, as the C# `ActionLocalizer.Localize`.
fn translated(command: &UserCommand, translation: &Translation) -> UserCommand {
    let mut copy = command.clone();
    copy.title = pick(translation.title.as_ref(), &copy.title);
    copy.description = pick(translation.description.as_ref(), &copy.description);
    for (template, texts) in [
        (
            copy.windows_command_template.as_mut(),
            &translation.parameters.windows,
        ),
        (
            copy.linux_command_template.as_mut(),
            &translation.parameters.linux,
        ),
    ] {
        let Some(template) = template else {
            continue;
        };
        for parameter in &mut template.parameters {
            if let Some(text) = texts.get(&parameter.name) {
                parameter.label = pick(text.label.as_ref(), &parameter.label);
                if let Some(description) = text.description.as_ref().filter(|d| !d.is_empty()) {
                    parameter.description = Some(description.clone());
                }
            }
        }
    }
    copy
}

/// A user's command in `language`, as the C# `CommandLibraryViewModel` shows every action
/// through `ActionLocalizer`: the `translations` an imported action carries (the debloat pack's
/// French and Spanish) give its text, with the same rules as the shipped catalogue; borrowed
/// when it has none for that language, or none that reads.
#[must_use]
pub fn localize_user<'c>(command: &'c UserCommand, language: &str) -> Cow<'c, UserCommand> {
    let found = translated_language(language).and_then(|language| {
        let Some(Value::Object(languages)) = command.extra.get(TRANSLATIONS_FIELD) else {
            return None;
        };
        languages
            .iter()
            .find(|(each, _)| normalize_language(each) == language)
            .and_then(|(_, text)| Translation::deserialize(text).ok())
    });
    match found {
        Some(translation) => Cow::Owned(translated(command, &translation)),
        None => Cow::Borrowed(command),
    }
}

/// The shipped actions.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Catalogue {
    entries: Vec<CatalogueEntry>,
    positions: HashMap<String, usize>,
}

impl Catalogue {
    /// The catalogue of the files `sources`, each the text of one action.
    ///
    /// # Errors
    ///
    /// [`CatalogueError`] for a file that is not an action, or an identity held twice.
    pub fn from_sources(sources: &[&str]) -> Result<Self, CatalogueError> {
        let mut catalogue = Self::default();
        for (index, source) in sources.iter().enumerate() {
            let entry = CatalogueEntry::parse(source).ok_or(CatalogueError::Entry(index))?;
            let id = entry.command.id.clone();
            if id.is_empty() {
                return Err(CatalogueError::Entry(index));
            }
            if catalogue
                .positions
                .insert(id.clone(), catalogue.entries.len())
                .is_some()
            {
                return Err(CatalogueError::Duplicate(id));
            }
            catalogue.entries.push(entry);
        }
        Ok(catalogue)
    }

    /// The actions, in the order of their file names.
    #[must_use]
    pub fn entries(&self) -> &[CatalogueEntry] {
        &self.entries
    }

    /// The place of the action of identity `id`.
    #[must_use]
    pub fn position(&self, id: &str) -> Option<usize> {
        self.positions.get(id).copied()
    }

    /// The action of identity `id`.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&CatalogueEntry> {
        self.position(id).and_then(|index| self.entries.get(index))
    }
}

/// The shipped catalogue, read once. A file that cannot be read is left out and logged: the
/// tests hold every shipped file to being read, so this only protects a damaged build.
#[must_use]
pub fn shipped() -> &'static Catalogue {
    static SHIPPED: OnceLock<Catalogue> = OnceLock::new();
    SHIPPED.get_or_init(|| {
        let mut catalogue = Catalogue::default();
        for source in CATALOGUE_FILES {
            match CatalogueEntry::parse(source) {
                Some(entry)
                    if !entry.command.id.is_empty()
                        && !catalogue.positions.contains_key(&entry.command.id) =>
                {
                    catalogue
                        .positions
                        .insert(entry.command.id.clone(), catalogue.entries.len());
                    catalogue.entries.push(entry);
                }
                _ => log::warn!("a file of the shipped command catalogue was left out"),
            }
        }
        catalogue
    })
}

/// The text of every file of the shipped catalogue.
#[must_use]
pub fn shipped_sources() -> &'static [&'static str] {
    CATALOGUE_FILES
}

/// The entry that replaced the command named `old` in the first versions of the library.
#[must_use]
pub fn legacy_snippet(old: &str) -> Option<&'static str> {
    LEGACY_SNIPPET_IDS
        .iter()
        .find(|(known, _)| *known == old)
        .map(|(_, id)| *id)
}

/// Whether the category `key` is one the catalogue knows.
#[must_use]
pub fn is_known_category(key: &str) -> bool {
    CATEGORY_KEYS.contains(&key)
}

/// Whether a command of `platform` is shown under the filter `chosen`, `None` being every
/// platform: a command with a template for both is shown under each, and under its own.
#[must_use]
pub fn on_platform(platform: UserPlatform, chosen: Option<UserPlatform>) -> bool {
    chosen.is_none_or(|chosen| match chosen {
        UserPlatform::Windows => platform != UserPlatform::Linux,
        UserPlatform::Linux => platform != UserPlatform::Windows,
        UserPlatform::Both => platform == UserPlatform::Both,
    })
}

/// The platform filter a session starts the library on, as the C# `AutoSelectPlatform`: the
/// platform its far end runs, `None` (every platform) when it cannot be told.
#[must_use]
pub fn session_platform_filter(platform: RemoteOs) -> Option<UserPlatform> {
    match platform {
        RemoteOs::Linux => Some(UserPlatform::Linux),
        RemoteOs::Windows => Some(UserPlatform::Windows),
        RemoteOs::Auto => None,
    }
}

/// The names that make a parameter take a host, as the C# `IsHostParameter`.
const HOST_PARAMETER_NAMES: [&str; 10] = [
    "host",
    "hostname",
    "target",
    "targethost",
    "computername",
    "server",
    "ip",
    "ipaddress",
    "remotehost",
    "remotecomputer",
];

/// The names that make a parameter take the account a command acts as, as the C#
/// `IsUserParameter`.
const USER_PARAMETER_NAMES: [&str; 5] = ["user", "username", "login", "account", "samaccountname"];

/// The types that take a host whatever their name is called.
const HOST_PARAMETER_TYPES: [&str; 2] = ["hostname", "ipaddress"];

/// Whether `parameter` is one a session's host can fill: of a host type, or named as a host.
/// An `ipaddress` one is only filled by an address, never a name (the C# `CanPrefillTargetHost`).
#[must_use]
pub fn is_host_parameter(parameter: &UserParameter) -> bool {
    let kind = parameter.kind.trim().to_lowercase();
    let name = parameter.name.trim().to_lowercase();
    HOST_PARAMETER_TYPES.contains(&kind.as_str()) || HOST_PARAMETER_NAMES.contains(&name.as_str())
}

/// Whether `parameter` is one a session's user name can fill, by its name alone: no
/// parameter type says "user" (the C# `IsUserParameter`).
#[must_use]
pub fn is_user_parameter(parameter: &UserParameter) -> bool {
    USER_PARAMETER_NAMES.contains(&parameter.name.trim().to_lowercase().as_str())
}

/// What the session the library was opened for gives `parameter` to start with, as the C#
/// `GetInitialParameterValue`: its host for a host parameter (an address only, for one of type
/// `ipaddress`), else its user name for a user parameter; nothing otherwise, and nothing for
/// a value that is blank. It only supplies a value: the generator still quotes and checks it as
/// it checks one typed.
#[must_use]
pub fn session_value(
    parameter: &UserParameter,
    host: Option<&str>,
    user: Option<&str>,
) -> Option<String> {
    let given = |value: Option<&str>| {
        value
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    };
    if let Some(host) = given(host)
        && is_host_parameter(parameter)
        && (!parameter.kind.trim().eq_ignore_ascii_case("ipaddress")
            || host.parse::<std::net::IpAddr>().is_ok())
    {
        return Some(host);
    }
    if is_user_parameter(parameter) {
        return given(user);
    }
    None
}

/// The most characters of a command a row shows under its title, as the C#
/// `CommandPreviewText.MaxLength`.
pub const PREVIEW_MAX_CHARS: usize = 96;

/// What a row shows of a command it was cut after.
const PREVIEW_ELLIPSIS: &str = "...";

/// The command on one line for a row, cut at [`PREVIEW_MAX_CHARS`] with an ellipsis when it
/// was longer, as the C# `CommandPreviewText.ToPreview`.
#[must_use]
pub fn command_preview(command: &str) -> String {
    let line = command.replace("\r\n", " ").replace(['\r', '\n'], " ");
    if line.chars().count() > PREVIEW_MAX_CHARS {
        let cut: String = line.chars().take(PREVIEW_MAX_CHARS).collect();
        format!("{cut}{PREVIEW_ELLIPSIS}")
    } else {
        line
    }
}

/// Whether `query` is in the command's title, description, tags, `category` (as the window
/// writes it) or in one of its patterns, ignoring case; an empty query matches all.
#[must_use]
pub fn matches_query(command: &UserCommand, category: &str, query: &str) -> bool {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return true;
    }
    let holds = |text: &str| text.to_lowercase().contains(&query);
    holds(&command.title)
        || holds(&command.description)
        || holds(category)
        || command.tags.iter().any(|tag| holds(tag))
        || [
            &command.windows_command_template,
            &command.linux_command_template,
        ]
        .into_iter()
        .flatten()
        .any(|template| holds(&template.command_pattern))
}

/// Why an action does not hold to the library's rules.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandIssue {
    /// No title.
    MissingTitle,
    /// A title longer than [`MAX_TITLE_LENGTH`].
    TitleTooLong,
    /// No category.
    MissingCategory,
    /// A category longer than [`MAX_CATEGORY_LENGTH`].
    CategoryTooLong,
    /// A description longer than [`MAX_DESCRIPTION_LENGTH`].
    DescriptionTooLong,
    /// More than [`MAX_TAGS`] tags.
    TooManyTags,
    /// No template with a command.
    NoTemplate,
    /// A pattern longer than [`MAX_PATTERN_LENGTH`].
    PatternTooLong(TemplatePlatform),
    /// A pattern with a line break: typed into a terminal it would run more than one line.
    PatternMultiline(TemplatePlatform),
    /// A pattern with a character that shows nothing (a direction override, a zero-width
    /// character): it would not read as what it runs.
    PatternHidden(TemplatePlatform),
    /// More than [`MAX_PARAMETERS`] parameters.
    TooManyParameters(TemplatePlatform),
    /// A parameter without a name.
    UnnamedParameter(TemplatePlatform),
    /// A name held by two parameters of a template.
    DuplicateParameter(TemplatePlatform, String),
    /// A type the library does not know.
    UnknownParameterType(TemplatePlatform, String),
    /// A choice that offers nothing.
    ChoiceWithoutValues(TemplatePlatform, String),
    /// A default the generator refuses.
    RefusedDefault(TemplatePlatform),
    /// An action the catalogue already ships: the shipped one is kept as it is, as the C#
    /// keeps its seeded actions.
    InCatalogue,
}

/// The templates of `command` with the platform each is written for.
fn templates(command: &UserCommand) -> impl Iterator<Item = (TemplatePlatform, &UserTemplate)> {
    [TemplatePlatform::Windows, TemplatePlatform::Linux]
        .into_iter()
        .filter_map(|platform| {
            command
                .template(platform)
                .map(|template| (platform, template))
        })
        .filter(|(_, template)| !template.command_pattern.trim().is_empty())
}

/// The first rule `command` breaks about its shape: the texts, the templates and their
/// parameters (its identity is the catalogue's to check, an import gives one). The shipped entries are held to these.
#[must_use]
pub fn structure_issue(command: &UserCommand) -> Option<CommandIssue> {
    if command.title.trim().is_empty() {
        return Some(CommandIssue::MissingTitle);
    }
    if command.title.chars().count() > MAX_TITLE_LENGTH {
        return Some(CommandIssue::TitleTooLong);
    }
    if command.category.trim().is_empty() {
        return Some(CommandIssue::MissingCategory);
    }
    if command.category.chars().count() > MAX_CATEGORY_LENGTH {
        return Some(CommandIssue::CategoryTooLong);
    }
    if command.description.chars().count() > MAX_DESCRIPTION_LENGTH {
        return Some(CommandIssue::DescriptionTooLong);
    }
    if command.tags.len() > MAX_TAGS {
        return Some(CommandIssue::TooManyTags);
    }
    if templates(command).next().is_none() {
        return Some(CommandIssue::NoTemplate);
    }
    for (platform, template) in templates(command) {
        if template.command_pattern.chars().count() > MAX_PATTERN_LENGTH {
            return Some(CommandIssue::PatternTooLong(platform));
        }
        if template.parameters.len() > MAX_PARAMETERS {
            return Some(CommandIssue::TooManyParameters(platform));
        }
        let mut names: Vec<&str> = Vec::new();
        for parameter in &template.parameters {
            if parameter.name.trim().is_empty() {
                return Some(CommandIssue::UnnamedParameter(platform));
            }
            if names.contains(&parameter.name.as_str()) {
                return Some(CommandIssue::DuplicateParameter(
                    platform,
                    parameter.name.clone(),
                ));
            }
            names.push(&parameter.name);
            let kind = parameter.kind.to_lowercase();
            if !SUPPORTED_PARAMETER_TYPES.contains(&kind.as_str()) {
                return Some(CommandIssue::UnknownParameterType(
                    platform,
                    parameter.kind.clone(),
                ));
            }
            if parameter.kind.eq_ignore_ascii_case(CHOICE_TYPE)
                && parameter.allowed_values.as_ref().is_none_or(Vec::is_empty)
            {
                return Some(CommandIssue::ChoiceWithoutValues(
                    platform,
                    parameter.name.clone(),
                ));
            }
        }
    }
    None
}

/// The templates of `command` whose command cannot be made from the defaults because the
/// generator refuses one of them. A required value with no default is not a refusal: it is
/// asked for, and a command made of nothing typed yet is not one to judge.
#[must_use]
pub fn refused_defaults(command: &UserCommand) -> Vec<TemplatePlatform> {
    templates(command)
        .filter(
            |(platform, template)| match generate(template, *platform, |_| None) {
                Ok(_) => false,
                Err(GenerateError::Values(refused)) => refused
                    .iter()
                    .any(|each| !matches!(each, ValueError::Required(_))),
                Err(_) => true,
            },
        )
        .map(|(platform, _)| platform)
        .collect()
}

/// The first rule an action of an imported file breaks: those of the shipped entries, no
/// line break in a pattern, and defaults the generator accepts. Only checks: the command is
/// built as text, never run.
#[must_use]
pub fn validate_import(command: &UserCommand) -> Option<CommandIssue> {
    // An action with no command is useless and harmless, and the C# imports it (it normalizes
    // its enumerations and keeps it): the shipped entries are the ones held to have one.
    if let Some(issue) = structure_issue(command).filter(|issue| *issue != CommandIssue::NoTemplate)
    {
        return Some(issue);
    }
    for (platform, template) in templates(command) {
        if template.command_pattern.chars().any(char::is_control) {
            return Some(CommandIssue::PatternMultiline(platform));
        }
        if template
            .command_pattern
            .chars()
            .any(crate::invisible::is_invisible)
        {
            return Some(CommandIssue::PatternHidden(platform));
        }
    }
    refused_defaults(command)
        .first()
        .map(|platform| CommandIssue::RefusedDefault(*platform))
}

#[cfg(test)]
mod tests;
