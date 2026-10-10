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

//! What "Run in Shell" of the local file browser runs, and for which files.
//!
//! The C# `LocalFileBrowserView` offers it for one file whose extension is `.ps1`, `.bat`,
//! `.cmd` or `.sh`, on every platform, and types the script's path into the shell beside the
//! browser, quoted for the shell it guesses from the program's name
//! (`TerminalCommandFormatter.FormatRun`). Heimdall-rs never types into a shell already
//! running: the script runs in a new tab of its own, started by its interpreter, and only
//! what this platform runs is offered. On Windows, a `PowerShell` script by the `PowerShell`
//! local shells start, which stays open once it ends, and a batch script by `cmd.exe`, which
//! stays open too; on Unix, a shell script by `/bin/sh`, the tab ending with it.
//!
//! `cmd.exe` reads its command line with rules of its own: a path holding a quote, `%` or
//! `!` cannot be handed to it as text whatever the quoting, and is refused rather than
//! quoted in a way that would depend on its settings.

use std::io;
use std::path::{Path, PathBuf};

use heimdall_term::local::LocalArguments;

use crate::local_open;

/// The extensions of a `PowerShell` script, compared whatever their case, as the C#
/// `RunnableExtensions`.
pub const POWERSHELL_EXTENSIONS: [&str; 1] = [".ps1"];

/// The extensions of a batch script, compared whatever their case, as the C#
/// `RunnableExtensions`.
pub const BATCH_EXTENSIONS: [&str; 2] = [".bat", ".cmd"];

/// The extensions of a POSIX shell script, compared whatever their case, as the C#
/// `RunnableExtensions`.
pub const POSIX_EXTENSIONS: [&str; 1] = [".sh"];

/// `PowerShell`'s flag keeping it open once the script ends, as the C# shell beside the
/// browser stays.
pub const POWERSHELL_NO_EXIT: &str = "-NoExit";

/// `PowerShell`'s flag naming the script it runs: the path, read as a path, never as code.
pub const POWERSHELL_FILE: &str = "-File";

/// `cmd.exe`'s switches: the first and last quotes of what follows `/k` are taken off and
/// nothing else (`/s`), the rest run with the shell kept open once it ends (`/k`).
pub const CMD_SWITCHES: &str = "/s /k";

/// `cmd.exe`, in the system folder.
#[cfg(windows)]
const CMD_PROGRAM: &str = "cmd.exe";

/// The characters `cmd.exe` reads in a quoted path as more than text: the quote, which ends
/// it; `%`, which expands a variable; `!`, which expands one when delayed expansion is on.
pub const CMD_REFUSED: [char; 3] = ['"', '%', '!'];

/// The kind of script "Run in Shell" runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScriptKind {
    /// A `PowerShell` script, run by `PowerShell`.
    PowerShell,
    /// A batch script, run by `cmd.exe`.
    Batch,
    /// A POSIX shell script, run by `/bin/sh`.
    Posix,
}

impl ScriptKind {
    /// The kind of script file `name` is, by its extension compared whatever its case,
    /// whatever the platform.
    #[must_use]
    pub fn of(name: &str) -> Option<Self> {
        let extension = local_open::extension(name)?;
        let listed = |extensions: &[&str]| {
            extensions
                .iter()
                .any(|listed| extension.eq_ignore_ascii_case(listed))
        };
        if listed(&POWERSHELL_EXTENSIONS) {
            Some(Self::PowerShell)
        } else if listed(&BATCH_EXTENSIONS) {
            Some(Self::Batch)
        } else if listed(&POSIX_EXTENSIONS) {
            Some(Self::Posix)
        } else {
            None
        }
    }

    /// Whether this platform runs it: `PowerShell` and batch scripts on Windows, POSIX
    /// shell scripts on Unix.
    #[must_use]
    pub fn runs_here(self) -> bool {
        match self {
            Self::PowerShell | Self::Batch => cfg!(windows),
            Self::Posix => cfg!(unix),
        }
    }
}

/// The kind of script file `name` is when this platform runs it: what "Run in Shell" is
/// offered for.
#[must_use]
pub fn runnable_here(name: &str) -> Option<ScriptKind> {
    ScriptKind::of(name).filter(|kind| kind.runs_here())
}

/// Why a script is not run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScriptRefusal {
    /// Its path holds this character, which its interpreter's command line would read as
    /// more than part of the path.
    Character(char),
    /// Its path is not Unicode text: it cannot be handed on as it is.
    NotText,
}

/// The arguments the interpreter of a `kind` script at `script`, a full path, is given:
/// `-NoExit -File` and the path for `PowerShell`, each quoted as the C runtime reads it
/// back; `/s /k` and the path inside two pairs of quotes for `cmd.exe`, written as they
/// are, the outer pair taken off by `/s` and the inner one keeping `&`, `^`, `|`, `<`, `>`
/// and parentheses as text; the path alone for `/bin/sh`.
///
/// # Errors
///
/// [`ScriptRefusal`]: a path that is not text; for `PowerShell`, a quote or a control
/// character; for `cmd.exe`, one of [`CMD_REFUSED`] or a control character.
pub fn arguments(kind: ScriptKind, script: &Path) -> Result<LocalArguments, ScriptRefusal> {
    let path = script.to_str().ok_or(ScriptRefusal::NotText)?;
    let refused = |more: &[char]| {
        path.chars()
            .find(|c| c.is_control() || more.contains(c))
            .map_or(Ok(()), |c| Err(ScriptRefusal::Character(c)))
    };
    match kind {
        ScriptKind::PowerShell => {
            refused(&['"'])?;
            Ok(LocalArguments::List(vec![
                POWERSHELL_NO_EXIT.to_owned(),
                POWERSHELL_FILE.to_owned(),
                path.to_owned(),
            ]))
        }
        ScriptKind::Batch => {
            refused(&CMD_REFUSED)?;
            Ok(LocalArguments::WindowsLine(format!(
                "{CMD_SWITCHES} \"\"{path}\"\""
            )))
        }
        // Handed over as an argument of its own: nothing in it is read by a shell.
        ScriptKind::Posix => Ok(LocalArguments::List(vec![path.to_owned()])),
    }
}

/// The interpreter of a `kind` script, by its full path: on Windows, the `PowerShell`
/// local shells start and `cmd.exe` in the system folder Windows says.
///
/// # Errors
///
/// The interpreter could not be found, or does not run on this platform.
pub fn interpreter(kind: ScriptKind) -> io::Result<PathBuf> {
    #[cfg(windows)]
    return match kind {
        ScriptKind::PowerShell => heimdall_term::local::program_path(None),
        ScriptKind::Batch => heimdall_core::paths::system_program(CMD_PROGRAM).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                heimdall_core::paths::SYSTEM_FOLDER_UNKNOWN,
            )
        }),
        ScriptKind::Posix => Err(not_here()),
    };
    #[cfg(unix)]
    return match kind {
        ScriptKind::Posix => Ok(PathBuf::from(heimdall_term::local::program::FALLBACK_SHELL)),
        ScriptKind::PowerShell | ScriptKind::Batch => Err(not_here()),
    };
}

/// A script this platform does not run.
fn not_here() -> io::Error {
    io::Error::new(
        io::ErrorKind::Unsupported,
        "this kind of script does not run on this platform",
    )
}
