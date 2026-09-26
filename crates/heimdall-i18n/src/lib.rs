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
//! languages, the fallback, and [`find_gaps`], which each of them runs in a test so that
//! no language ships with a key missing or left over.

use std::collections::BTreeSet;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use fluent_syntax::ast::Entry;
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

/// Compares every supported language of `domain` against the fallback language.
///
/// An empty result means every language declares exactly the same keys.
///
/// # Errors
///
/// Returns [`CheckError`] when a file is missing, unreadable or does not parse.
pub fn find_gaps(crate_root: &Path, domain: &str) -> Result<Vec<Gap>, CheckError> {
    let reference = load_ids(&ftl_path(crate_root, FALLBACK_LANGUAGE, domain))?;
    let mut gaps = Vec::new();
    for language in SUPPORTED_LANGUAGES
        .iter()
        .filter(|language| **language != FALLBACK_LANGUAGE)
    {
        let ids = load_ids(&ftl_path(crate_root, language, domain))?;
        let gap = |key: &String, kind: GapKind| Gap {
            language: (*language).to_owned(),
            key: key.clone(),
            kind,
        };
        gaps.extend(
            reference
                .difference(&ids)
                .map(|key| gap(key, GapKind::Missing)),
        );
        gaps.extend(
            ids.difference(&reference)
                .map(|key| gap(key, GapKind::Extra)),
        );
    }
    Ok(gaps)
}

fn load_ids(path: &Path) -> Result<BTreeSet<String>, CheckError> {
    let source = fs::read_to_string(path).map_err(|source| CheckError::Io {
        path: path.to_owned(),
        source,
    })?;
    message_ids(&source).map_err(|errors| CheckError::Parse {
        path: path.to_owned(),
        errors,
    })
}
