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

//! Bare program guard: no application code starts a program by a bare name.
//!
//! `Command::new("nslookup")` is looked up on the `PATH`, and on Windows in the current
//! folder first: a program of that name planted there runs in its place. Every system
//! program goes by its full path instead (`heimdall_core::tools::system_program`, the system
//! folder, fixed folders elsewhere). The guard reads every `Command::new(...)` whose program
//! is written as a string, or as a string constant (`XDG_OPEN_PROGRAM`, local or named by
//! its path), and refuses a name without a folder; a program computed at run time is the
//! code's own business and is not judged. The exceptions are [`ALLOWED`], each with why.
//!
//! Tests may start what they like: the test modules at the end of a file (`#[cfg(test)]`
//! then `mod`, closed at the first column), the test modules in files of their own
//! (`tests.rs`, `*_tests.rs`) and the `tests` folders are not read, nor are comments.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::folder_sources;

/// String constants by name, each with every value a constant of that name holds.
pub type Constants = BTreeMap<String, Vec<String>>;

/// A program allowed to start by its bare name in one file, with why.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Allowed {
    /// The file, from the workspace root, with `/`.
    pub file: &'static str,
    /// The program, as the code names it.
    pub program: &'static str,
    /// Why it may.
    pub reason: &'static str,
}

/// Why `xdg-open` is started by its name: it is the desktop's own, wherever the
/// distribution put it, and the user's session decides which one runs.
const XDG_OPEN_REASON: &str = "Linux only: xdg-open is the desktop's own opener, installed \
     where each distribution puts it, and resolved on the session's PATH by design";

/// Every program allowed to start by its bare name, with its file and reason.
pub const ALLOWED: [Allowed; 3] = [
    Allowed {
        file: "crates/heimdall-app/src/citrix.rs",
        program: "xdg-open",
        reason: XDG_OPEN_REASON,
    },
    Allowed {
        file: "crates/heimdall-app/src/external_edit.rs",
        program: "xdg-open",
        reason: XDG_OPEN_REASON,
    },
    Allowed {
        file: "crates/heimdall-app/src/external_url.rs",
        program: "xdg-open",
        reason: XDG_OPEN_REASON,
    },
];

/// What starts a program.
const LAUNCH: &str = "Command::new(";

/// What a string constant's declaration holds between its name and its value.
const STRING_CONSTANT: &str = ": &str = \"";

/// The keyword of a constant, after its visibility.
const CONSTANT_KEYWORD: &str = "const ";

/// The line that opens a test module, at the first column.
const TEST_GATE: &str = "#[cfg(test)]";

/// How the module under it starts, at the first column.
const MODULE_START: &str = "mod ";

/// The line that closes a module opened at the first column.
const MODULE_END: &str = "}";

/// The start of a comment line.
const COMMENT: &str = "//";

/// The name of a test module kept in a file of its own.
const TEST_FILE_STEM: &str = "tests";

/// How the name of a test file of its own ends.
const TEST_FILE_SUFFIX: &str = "_tests";

/// One program started by its bare name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// The file.
    pub file: PathBuf,
    /// Its line, from 1.
    pub line: usize,
    /// The program, as named.
    pub program: String,
}

/// Result of a scan: what was found, and which files were read.
#[derive(Debug, Default)]
pub struct Scan {
    /// Programs started by a bare name, allowed ones included.
    pub findings: Vec<Finding>,
    /// Files read.
    pub scanned: Vec<PathBuf>,
}

/// The string constants `text` declares on one line, as name and value.
#[must_use]
pub fn string_constants(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim_start();
            let declaration = &line[line.find(CONSTANT_KEYWORD)?..];
            let rest = declaration.strip_prefix(CONSTANT_KEYWORD)?;
            let (name, value) = rest.split_once(STRING_CONSTANT)?;
            let (value, _) = value.split_once('"')?;
            let name = name.trim();
            is_constant_name(name).then(|| (name.to_owned(), value.to_owned()))
        })
        .collect()
}

/// Whether `name` is written as a constant: capitals, digits and `_`, one capital at least.
fn is_constant_name(name: &str) -> bool {
    name.chars().any(|c| c.is_ascii_uppercase())
        && name
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}

/// Whether `program` names no folder: looked up on the `PATH`.
#[must_use]
pub fn is_bare(program: &str) -> bool {
    !program.is_empty() && !program.contains(['/', '\\'])
}

/// The argument of the launch that starts at `text`, up to its closing parenthesis or the
/// end of the line.
fn argument(text: &str) -> &str {
    let mut depth = 0_usize;
    for (index, c) in text.char_indices() {
        match c {
            '(' => depth += 1,
            ')' if depth == 0 => return &text[..index],
            ')' => depth -= 1,
            _ => {}
        }
    }
    text
}

/// The program `argument` names, when it is written as a string or a string constant;
/// `None` when it is computed. `local` are the file's constants, `global` every source's.
fn named_program(argument: &str, local: &Constants, global: &Constants) -> Vec<String> {
    let argument = argument.trim().trim_start_matches('&').trim();
    if let Some(literal) = argument.strip_prefix('"') {
        return literal
            .split_once('"')
            .map(|(value, _)| vec![value.to_owned()])
            .unwrap_or_default();
    }
    if !argument
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == ':')
    {
        return Vec::new();
    }
    let name = argument.rsplit("::").next().unwrap_or(argument);
    if !is_constant_name(name) {
        return Vec::new();
    }
    let qualified = argument.contains("::");
    let from_file = (!qualified).then(|| local.get(name)).flatten();
    from_file
        .or_else(|| global.get(name))
        .cloned()
        .unwrap_or_default()
}

/// The lines of the Rust source `text` that start a program by its bare name, outside its
/// test modules and comments: each line's number and the program. `global` are the string
/// constants of every source, for a constant named by its path.
#[must_use]
pub fn check_source(text: &str, global: &Constants) -> Vec<(usize, String)> {
    let mut local: Constants = Constants::new();
    for (name, value) in string_constants(text) {
        local.entry(name).or_default().push(value);
    }
    let mut found = Vec::new();
    let mut lines = text.lines().enumerate().peekable();
    while let Some((index, line)) = lines.next() {
        if line == TEST_GATE
            && lines
                .peek()
                .is_some_and(|(_, next)| next.starts_with(MODULE_START))
        {
            for (_, inside) in lines.by_ref() {
                if inside == MODULE_END {
                    break;
                }
            }
            continue;
        }
        if line.trim_start().starts_with(COMMENT) {
            continue;
        }
        let mut rest = line;
        while let Some(at) = rest.find(LAUNCH) {
            rest = &rest[at + LAUNCH.len()..];
            found.extend(
                named_program(argument(rest), &local, global)
                    .into_iter()
                    .filter(|program| is_bare(program))
                    .map(|program| (index + 1, program)),
            );
        }
    }
    found
}

/// Whether `file` is a test module kept in a file of its own.
fn is_test_file(file: &Path) -> bool {
    file.file_stem()
        .and_then(|stem| stem.to_str())
        .is_some_and(|stem| stem == TEST_FILE_STEM || stem.ends_with(TEST_FILE_SUFFIX))
}

/// Whether the finding `program` in `file`, under `root`, is in [`ALLOWED`].
#[must_use]
pub fn is_allowed(root: &Path, file: &Path, program: &str) -> bool {
    ALLOWED
        .iter()
        .any(|allowed| root.join(allowed.file) == file && allowed.program == program)
}

/// Every application source of the workspace at `root` checked.
///
/// # Errors
///
/// A file or directory that cannot be read.
pub fn scan(root: &Path) -> io::Result<Scan> {
    let mut texts = Vec::new();
    for file in folder_sources::sources(root)? {
        if !is_test_file(&file) {
            let text = fs::read_to_string(&file)?;
            texts.push((file, text));
        }
    }
    let mut global: Constants = Constants::new();
    for (_, text) in &texts {
        for (name, value) in string_constants(text) {
            global.entry(name).or_default().push(value);
        }
    }
    let mut result = Scan::default();
    for (file, text) in texts {
        result.findings.extend(
            check_source(&text, &global)
                .into_iter()
                .map(|(line, program)| Finding {
                    file: file.clone(),
                    line,
                    program,
                }),
        );
        result.scanned.push(file);
    }
    Ok(result)
}
