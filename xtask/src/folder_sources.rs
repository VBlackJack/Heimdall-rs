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

//! Folder source guard: no code of the application but `heimdall_core::paths` asks the
//! system or the environment where a user's or the program's folder is.
//!
//! Every such folder goes through `paths`, so that the folders Heimdall trusts never follow
//! the environment, and so that a test build roots every one of them in its scratch folder.
//! A folder asked elsewhere would escape both. Tests may ask freely: the test modules at the
//! end of a file (`#[cfg(test)]` then `mod`, closed at the first column) and the `tests`
//! folders are not read, nor are comments.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// What asks where a folder is, other than through `paths`.
pub const REFUSED: [&str; 9] = [
    "env::temp_dir",
    "current_exe",
    "directories::",
    "BaseDirs",
    "ProjectDirs",
    "UserDirs",
    "known_folder",
    "KNOWNFOLDERID",
    "SHGetKnownFolderPath",
];

/// A file allowed to ask, with why.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Allowed {
    /// The file, from the workspace root, with `/`.
    pub file: &'static str,
    /// Why it may.
    pub reason: &'static str,
}

/// Every file allowed to ask, with its reason.
pub const ALLOWED: [Allowed; 1] = [Allowed {
    file: "crates/heimdall-core/src/paths.rs",
    reason: "the one place every folder is resolved: from the system, never the environment, \
             and inside the scratch folder of a test build",
}];

/// The line that opens a test module, at the first column.
const TEST_GATE: &str = "#[cfg(test)]";

/// How the module under it starts, at the first column.
const MODULE_START: &str = "mod ";

/// The line that closes a module opened at the first column.
const MODULE_END: &str = "}";

/// The start of a comment line.
const COMMENT: &str = "//";

/// One place that asks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// The file.
    pub file: PathBuf,
    /// Its line, from 1.
    pub line: usize,
    /// What it asks with.
    pub refused: &'static str,
}

/// Result of a scan: what was found, and which files were read.
#[derive(Debug, Default)]
pub struct Scan {
    /// Places refused.
    pub findings: Vec<Finding>,
    /// Files read.
    pub scanned: Vec<PathBuf>,
}

/// The lines of the Rust source `text` that ask where a folder is, outside its test modules
/// and comments: each line's number and what it asks with.
#[must_use]
pub fn check_source(text: &str) -> Vec<(usize, &'static str)> {
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
        found.extend(
            REFUSED
                .iter()
                .filter(|refused| line.contains(*refused))
                .map(|refused| (index + 1, *refused)),
        );
    }
    found
}

/// The Rust sources under `folder`, every folder under it included.
fn sources_under(folder: &Path, into: &mut Vec<PathBuf>) -> io::Result<()> {
    for entry in fs::read_dir(folder)? {
        let path = entry?.path();
        if path.is_dir() {
            sources_under(&path, into)?;
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            into.push(path);
        }
    }
    Ok(())
}

/// The application's sources of the workspace at `root`: every `src` folder under `crates`.
///
/// # Errors
///
/// A directory that cannot be read.
pub fn sources(root: &Path) -> io::Result<Vec<PathBuf>> {
    let mut found = Vec::new();
    for entry in fs::read_dir(root.join("crates"))? {
        let source = entry?.path().join("src");
        if source.is_dir() {
            sources_under(&source, &mut found)?;
        }
    }
    found.sort();
    Ok(found)
}

/// Whether `file`, under `root`, is in [`ALLOWED`].
#[must_use]
pub fn is_allowed(root: &Path, file: &Path) -> bool {
    ALLOWED
        .iter()
        .any(|allowed| root.join(allowed.file) == file)
}

/// Every application source of the workspace at `root` checked.
///
/// # Errors
///
/// A file or directory that cannot be read.
pub fn scan(root: &Path) -> io::Result<Scan> {
    let mut result = Scan::default();
    for file in sources(root)? {
        if !is_allowed(root, &file) {
            let text = fs::read_to_string(&file)?;
            result
                .findings
                .extend(
                    check_source(&text)
                        .into_iter()
                        .map(|(line, refused)| Finding {
                            file: file.clone(),
                            line,
                            refused,
                        }),
                );
        }
        result.scanned.push(file);
    }
    Ok(result)
}
