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

//! The test data folder of a test build: `--data-dir <full path>` on the command line roots
//! every per-user folder there, with [`heimdall_core::paths::use_test_data_dir`], and every
//! window's title says so. Only in a debug build with the `test-data-dir` feature, for
//! automated UI tests: a build without it reads no argument at all.
//!
//! A test build is never started on the user's own folders: without the argument it does
//! not start.

use std::ffi::OsString;
use std::fmt;
use std::path::PathBuf;

use heimdall_core::paths::{self, TestDataDirError, test_data};

use crate::i18n::fl;

/// The exit code of a test build started without a usable test data folder.
pub const REFUSED_EXIT_CODE: i32 = 2;

/// What joins the argument and its value in a single word, as in `--data-dir=C:\scratch`.
const VALUE_SEPARATOR: char = '=';

/// Why a test build did not start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArgumentError {
    /// No test data folder was named.
    Missing,
    /// The argument came without its folder.
    NoValue,
    /// The test data folder was named twice.
    Repeated,
    /// An argument this build does not know.
    Unknown(OsString),
    /// The folder was refused.
    Refused(TestDataDirError),
}

impl fmt::Display for ArgumentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let argument = test_data::ARGUMENT;
        match self {
            Self::Missing => write!(
                f,
                "a test build runs only on a scratch folder: start it with {argument} <full path>"
            ),
            Self::NoValue => write!(f, "{argument} needs a full path after it"),
            Self::Repeated => write!(f, "{argument} is given more than once"),
            Self::Unknown(other) => write!(f, "unknown argument: {}", other.to_string_lossy()),
            Self::Refused(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for ArgumentError {}

/// The test data folder the command line `arguments`, the program's name left out, names.
///
/// # Errors
///
/// [`ArgumentError`]: none named, named twice or without its folder, or another argument.
pub fn folder_from(
    arguments: impl IntoIterator<Item = OsString>,
) -> Result<PathBuf, ArgumentError> {
    let mut arguments = arguments.into_iter();
    let mut folder = None;
    while let Some(argument) = arguments.next() {
        let text = argument.to_string_lossy();
        let value = if text == test_data::ARGUMENT {
            arguments.next().ok_or(ArgumentError::NoValue)?
        } else if let Some(value) = text
            .strip_prefix(test_data::ARGUMENT)
            .and_then(|rest| rest.strip_prefix(VALUE_SEPARATOR))
        {
            OsString::from(value)
        } else {
            return Err(ArgumentError::Unknown(argument));
        };
        if value.is_empty() {
            return Err(ArgumentError::NoValue);
        }
        if folder.replace(PathBuf::from(value)).is_some() {
            return Err(ArgumentError::Repeated);
        }
    }
    folder.ok_or(ArgumentError::Missing)
}

/// Roots every per-user folder in the test data folder `arguments` names: before anything
/// reads a folder.
///
/// # Errors
///
/// [`ArgumentError`], as [`folder_from`] and [`paths::use_test_data_dir`] refuse.
pub fn init(arguments: impl IntoIterator<Item = OsString>) -> Result<(), ArgumentError> {
    let folder = folder_from(arguments)?;
    paths::use_test_data_dir(&folder).map_err(ArgumentError::Refused)
}

/// `title` with the test data folder said after it, when one is in use, so that a test
/// build is never taken for the real application.
#[must_use]
pub fn titled(title: String) -> String {
    match paths::test_data_dir() {
        Some(folder) => fl!(
            "ui-window-title-test-data",
            title = title,
            folder = folder.display().to_string()
        ),
        None => title,
    }
}
