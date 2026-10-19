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

//! The `--data-dir <full path>` option of a normal build: an alternate instance (a hand
//! test, a demo profile) with its own configuration, data and saved passwords, so it shares
//! nothing writable with the user's own, as the C# `DataDirectoryArgument` and
//! `ApplicationDataPathResolver.UseDataRoot` (`App.xaml.cs:153-156`). What it moves, and what
//! stays the user's, is said at [`heimdall_core::paths::use_data_root`].
//!
//! As the C# reads it: `--data-dir <folder>` or `--data-dir=<folder>`, the name in any case,
//! the folder trimmed of spaces and quotes (a shortcut's `"C:\My Demo\"` arrives with a quote
//! at its end), the last occurrence winning, any other argument ignored. Where the C# falls
//! back to the user's own folders, this refuses to start instead, so a mistyped option never
//! opens the real profile: a missing folder, a relative one (the C# makes it absolute against
//! the current folder), a folder meeting one of the user's real folders, and an option whose
//! name only starts like this one (`--data-directory`).
//!
//! Unlike the `test-data-dir` sandbox (a debug-only feature for the automated UI tests), it
//! leaves the home, Documents and temporary folders the user's own. A build never has both:
//! the sandbox replaces this module when its feature is on.
//!
//! Without the option the build starts as always, on the user's own folders. With one, every
//! window's title ends with the folder, so an alternate instance is never taken for the
//! primary one.

use std::ffi::{OsStr, OsString};
use std::fmt;
use std::path::PathBuf;

use heimdall_core::paths::{self, DataRootError};

use crate::i18n::fl;

/// The exit code of a build started with a `--data-dir` folder it could not use.
pub const REFUSED_EXIT_CODE: i32 = 2;

/// The option naming the data root, as the C# spells it.
pub const ARGUMENT: &str = "--data-dir";

/// What joins the option and its value in a single word, as in `--data-dir=C:\demo`.
const VALUE_SEPARATOR: char = '=';

/// What the C# trims from the ends of the folder: a quote left by Windows' argument rules.
const QUOTE: char = '"';

/// Why a `--data-dir` folder did not start the build.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArgumentError {
    /// The option came without its folder.
    NoValue,
    /// The folder given with the option is not text.
    NotText(OsString),
    /// An option whose name starts as this one's but is not it, such as `--data-directory`.
    Unknown(OsString),
    /// The folder was refused.
    Refused(DataRootError),
}

impl fmt::Display for ArgumentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let option = ARGUMENT;
        let text = match self {
            Self::NoValue => fl!("ui-data-dir-error-no-value", option = option),
            Self::NotText(given) => fl!(
                "ui-data-dir-error-not-text",
                option = option,
                folder = given.to_string_lossy().into_owned()
            ),
            Self::Unknown(given) => fl!(
                "ui-data-dir-error-unknown",
                given = given.to_string_lossy().into_owned(),
                option = option
            ),
            Self::Refused(DataRootError::NotAbsolute(folder)) => fl!(
                "ui-data-dir-error-not-absolute",
                folder = folder.display().to_string()
            ),
            Self::Refused(DataRootError::Overlaps(folder, real)) => fl!(
                "ui-data-dir-error-overlaps",
                folder = folder.display().to_string(),
                real = real.display().to_string()
            ),
            Self::Refused(DataRootError::Unusable(detail)) => {
                fl!("ui-data-dir-error-unusable", detail = detail.clone())
            }
            Self::Refused(DataRootError::AlreadySet) => fl!("ui-data-dir-error-already-set"),
            Self::Refused(DataRootError::TestBuild) => {
                fl!("ui-data-dir-error-test-build", option = option)
            }
        };
        f.write_str(&text)
    }
}

impl std::error::Error for ArgumentError {}

/// Whether `bytes` start with the option's name, in any case.
fn starts_with_option(bytes: &[u8]) -> bool {
    bytes
        .get(..ARGUMENT.len())
        .is_some_and(|start| start.eq_ignore_ascii_case(ARGUMENT.as_bytes()))
}

/// `value` as the folder it names, trimmed of spaces and quotes as the C# trims it; `None`
/// when nothing is left. A value that is not text is taken as the system gives it.
fn folder_of(value: &OsStr) -> Option<PathBuf> {
    match value.to_str() {
        Some(text) => {
            let trimmed = text.trim().trim_matches(QUOTE).trim();
            (!trimmed.is_empty()).then(|| PathBuf::from(trimmed))
        }
        None => (!value.is_empty()).then(|| PathBuf::from(value)),
    }
}

/// The data root the command line `arguments` names, the program's name left out, or `None`
/// when the option is absent. The last occurrence wins, as the C# reads it; any other
/// argument is ignored.
///
/// # Errors
///
/// [`ArgumentError`]: the option without its folder, a folder given in one word that is not
/// text, or an option whose name only starts like this one.
pub fn folder_from(
    arguments: impl IntoIterator<Item = OsString>,
) -> Result<Option<PathBuf>, ArgumentError> {
    let mut arguments = arguments.into_iter();
    let mut folder = None;
    while let Some(argument) = arguments.next() {
        if !starts_with_option(argument.as_encoded_bytes()) {
            continue;
        }
        // A word starting with the option's name that is not text cannot be split at its
        // separator without losses: refused rather than guessed.
        let Some(text) = argument.to_str() else {
            return Err(ArgumentError::NotText(argument));
        };
        let (name, inline) = match text.split_once(VALUE_SEPARATOR) {
            Some((name, value)) => (name, Some(value)),
            None => (text, None),
        };
        if !name.eq_ignore_ascii_case(ARGUMENT) {
            return Err(ArgumentError::Unknown(argument));
        }
        let value = match inline {
            Some(value) => OsString::from(value),
            None => arguments.next().ok_or(ArgumentError::NoValue)?,
        };
        folder = Some(folder_of(&value).ok_or(ArgumentError::NoValue)?);
    }
    Ok(folder)
}

/// Roots the application's folders in the data root `arguments` names, when one is named:
/// before anything reads a folder. Does nothing when the option is absent, so the build
/// starts on the user's own folders.
///
/// # Errors
///
/// [`ArgumentError`], as [`folder_from`] and [`paths::use_data_root`] refuse.
pub fn init(arguments: impl IntoIterator<Item = OsString>) -> Result<(), ArgumentError> {
    if let Some(folder) = folder_from(arguments)? {
        paths::use_data_root(&folder).map_err(ArgumentError::Refused)?;
    }
    Ok(())
}

/// Says why the build does not start: on the error output, and, in a release build on
/// Windows or macOS, which has no console, in a message box, so a refused folder never ends the program
/// without a word.
pub fn refuse(error: &ArgumentError) {
    crate::i18n::init();
    let message = error.to_string();
    eprintln!("{message}");
    // Linux has no dialog crate here (the portal goes through zbus) and a console, so the
    // error output above is where the message is read there.
    #[cfg(all(not(debug_assertions), not(target_os = "linux")))]
    {
        let _ = rfd::MessageDialog::new()
            .set_level(rfd::MessageLevel::Error)
            .set_title(fl!("ui-data-dir-error-title"))
            .set_description(message)
            .set_buttons(rfd::MessageButtons::Ok)
            .show();
    }
}

/// `title` with the data root said after it, when one is in use, so an alternate instance is
/// never taken for the primary one; `title` unchanged on the user's own folders.
#[must_use]
pub fn titled(title: String) -> String {
    match paths::data_root() {
        Some(folder) => fl!(
            "ui-window-title-data",
            title = title,
            folder = folder.display().to_string()
        ),
        None => title,
    }
}
