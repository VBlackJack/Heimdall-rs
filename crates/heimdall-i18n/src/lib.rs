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

//! Localisation infrastructure shared by the Heimdall crates that show text to the user.
//!
//! The `fl!` macro of `i18n-embed-fl` resolves a key against the `i18n.toml` of the crate
//! that calls it, and uses that crate's package name as the Fluent domain. Every crate
//! with user-facing text therefore owns its files, at
//! `i18n/<language>/<package-name>.ftl`. This crate holds what they share: the supported
//! languages, the fallback, and [`find_gaps`], which each of them runs in a test. New text
//! is written in the fallback language first and translated later, so a key missing from
//! another language is allowed (shown in the fallback meanwhile); a key the fallback lacks
//! is refused, and so is a translation whose variables are not the fallback's: Fluent
//! would show a variable it is never given as an error in the text, and drop one it is
//! given and not asked for.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use fluent_syntax::ast::{
    Entry, Expression, InlineExpression, Pattern, PatternElement, VariantKey,
};
use fluent_syntax::parser;

/// Language used when a message is missing in the requested one.
pub const FALLBACK_LANGUAGE: &str = "en";

/// Languages every user-facing crate must ship, fallback first.
pub const SUPPORTED_LANGUAGES: [&str; 3] = ["en", "fr", "es"];

/// Directory, relative to a crate root, that holds its Fluent files.
pub const ASSETS_DIR: &str = "i18n";

/// Extension of a Fluent resource file.
const FTL_EXTENSION: &str = "ftl";

/// How a language differs from the fallback language for one key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GapKind {
    /// The key exists in the fallback language and not in this one.
    Missing,
    /// The key exists in this language and not in the fallback one.
    Extra,
    /// The key exists in both, and this language's text names other variables.
    Variables,
}

/// One key that is not present in every supported language.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Gap {
    /// Language the gap was found in.
    pub language: String,
    /// Fluent message identifier.
    pub key: String,
    /// Direction of the difference.
    pub kind: GapKind,
}

/// Reasons [`find_gaps`] could not compare the languages.
#[derive(Debug)]
pub enum CheckError {
    /// A Fluent file could not be read.
    Io {
        /// File that failed.
        path: PathBuf,
        /// Underlying error.
        source: std::io::Error,
    },
    /// A Fluent file did not parse.
    Parse {
        /// File that failed.
        path: PathBuf,
        /// Number of syntax errors reported by the parser.
        errors: usize,
    },
}

impl fmt::Display for CheckError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => write!(f, "{}: {source}", path.display()),
            Self::Parse { path, errors } => {
                write!(f, "{}: {errors} Fluent syntax error(s)", path.display())
            }
        }
    }
}

impl std::error::Error for CheckError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Parse { .. } => None,
        }
    }
}

/// Path of the Fluent file of `domain` in `language`, under the crate at `crate_root`.
#[must_use]
pub fn ftl_path(crate_root: &Path, language: &str, domain: &str) -> PathBuf {
    crate_root
        .join(ASSETS_DIR)
        .join(language)
        .join(domain)
        .with_extension(FTL_EXTENSION)
}

/// Message identifiers declared in a Fluent source.
///
/// # Errors
///
/// Returns the number of syntax errors when the source does not parse cleanly.
pub fn message_ids(source: &str) -> Result<BTreeSet<String>, usize> {
    let resource = parser::parse(source).map_err(|(_, errors)| errors.len())?;
    Ok(resource
        .body
        .iter()
        .filter_map(|entry| match entry {
            Entry::Message(message) => Some(message.id.name.to_owned()),
            _ => None,
        })
        .collect())
}

/// Variables each message of a Fluent source names, in its value, its variants and its
/// attributes, by message identifier.
///
/// # Errors
///
/// Returns the number of syntax errors when the source does not parse cleanly.
pub fn message_variables(source: &str) -> Result<BTreeMap<String, BTreeSet<String>>, usize> {
    let resource = parser::parse(source).map_err(|(_, errors)| errors.len())?;
    Ok(resource
        .body
        .iter()
        .filter_map(|entry| match entry {
            Entry::Message(message) => {
                let mut names = BTreeSet::new();
                message
                    .value
                    .iter()
                    .chain(message.attributes.iter().map(|attribute| &attribute.value))
                    .for_each(|pattern| pattern_variables(pattern, &mut names));
                Some((message.id.name.to_owned(), names))
            }
            _ => None,
        })
        .collect())
}

/// The messages of a Fluent source with a `one` variant that does not name the number it
/// is chosen by. French puts 0 in `one`: a variant reading "1 session" would say it of none.
///
/// # Errors
///
/// Returns the number of syntax errors when the source does not parse cleanly.
pub fn fixed_one_variants(source: &str) -> Result<Vec<String>, usize> {
    let resource = parser::parse(source).map_err(|(_, errors)| errors.len())?;
    Ok(resource
        .body
        .iter()
        .filter_map(|entry| match entry {
            Entry::Message(message) => {
                let fixed = message
                    .value
                    .iter()
                    .chain(message.attributes.iter().map(|attribute| &attribute.value))
                    .any(pattern_has_fixed_one);
                fixed.then(|| message.id.name.to_owned())
            }
            _ => None,
        })
        .collect())
}

fn pattern_has_fixed_one(pattern: &Pattern<&str>) -> bool {
    pattern.elements.iter().any(|element| match element {
        PatternElement::Placeable {
            expression: Expression::Select { selector, variants },
        } => variants.iter().any(|variant| {
            let fixed = matches!(variant.key, VariantKey::Identifier { name: "one" })
                && matches!(selector, InlineExpression::VariableReference { id } if {
                    let mut names = BTreeSet::new();
                    pattern_variables(&variant.value, &mut names);
                    !names.contains(id.name)
                });
            fixed || pattern_has_fixed_one(&variant.value)
        }),
        _ => false,
    })
}

fn pattern_variables(pattern: &Pattern<&str>, names: &mut BTreeSet<String>) {
    for element in &pattern.elements {
        if let PatternElement::Placeable { expression } = element {
            expression_variables(expression, names);
        }
    }
}

fn expression_variables(expression: &Expression<&str>, names: &mut BTreeSet<String>) {
    match expression {
        Expression::Select { selector, variants } => {
            inline_variables(selector, names);
            for variant in variants {
                pattern_variables(&variant.value, names);
            }
        }
        Expression::Inline(inline) => inline_variables(inline, names),
    }
}

fn inline_variables(inline: &InlineExpression<&str>, names: &mut BTreeSet<String>) {
    let arguments = match inline {
        InlineExpression::VariableReference { id } => {
            names.insert(id.name.to_owned());
            None
        }
        InlineExpression::Placeable { expression } => {
            expression_variables(expression, names);
            None
        }
        InlineExpression::FunctionReference { arguments, .. } => Some(arguments),
        InlineExpression::TermReference { arguments, .. } => arguments.as_ref(),
        InlineExpression::StringLiteral { .. }
        | InlineExpression::NumberLiteral { .. }
        | InlineExpression::MessageReference { .. } => None,
    };
    if let Some(arguments) = arguments {
        for argument in arguments
            .positional
            .iter()
            .chain(arguments.named.iter().map(|named| &named.value))
        {
            inline_variables(argument, names);
        }
    }
}

/// Compares every supported language of `domain` against the fallback language.
///
/// An empty result means every language declares exactly the same keys, each naming the
/// same variables as in the fallback language.
///
/// # Errors
///
/// Returns [`CheckError`] when a file is missing, unreadable or does not parse.
pub fn find_gaps(crate_root: &Path, domain: &str) -> Result<Vec<Gap>, CheckError> {
    let reference = load_messages(&ftl_path(crate_root, FALLBACK_LANGUAGE, domain))?;
    let mut gaps = Vec::new();
    for language in SUPPORTED_LANGUAGES
        .iter()
        .filter(|language| **language != FALLBACK_LANGUAGE)
    {
        let messages = load_messages(&ftl_path(crate_root, language, domain))?;
        let gap = |key: &String, kind: GapKind| Gap {
            language: (*language).to_owned(),
            key: key.clone(),
            kind,
        };
        for (key, variables) in &reference {
            gaps.extend(match messages.get(key) {
                None => Some(gap(key, GapKind::Missing)),
                Some(translated) if translated != variables => Some(gap(key, GapKind::Variables)),
                Some(_) => None,
            });
        }
        gaps.extend(
            messages
                .keys()
                .filter(|key| !reference.contains_key(*key))
                .map(|key| gap(key, GapKind::Extra)),
        );
    }
    Ok(gaps)
}

fn load_messages(path: &Path) -> Result<BTreeMap<String, BTreeSet<String>>, CheckError> {
    let source = fs::read_to_string(path).map_err(|source| CheckError::Io {
        path: path.to_owned(),
        source,
    })?;
    message_variables(&source).map_err(|errors| CheckError::Parse {
        path: path.to_owned(),
        errors,
    })
}
