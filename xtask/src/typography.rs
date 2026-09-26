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

//! Typography guard: the characters that are only typographic versions of an ASCII one.
//!
//! Each of them has a plain ASCII replacement that survives a Windows terminal, a diff, a
//! console code page and a CI log. Accents are not in the list: French and Spanish text
//! keeps them. The list is written with escapes so that this file passes its own guard.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Refused characters, each with the ASCII text to write instead.
pub const REFUSED: [(char, &str); 22] = [
    ('\u{2014}', "-"),
    ('\u{2013}', "-"),
    ('\u{2010}', "-"),
    ('\u{2011}', "-"),
    ('\u{2212}', "-"),
    ('\u{201C}', "\""),
    ('\u{201D}', "\""),
    ('\u{201E}', "\""),
    ('\u{00AB}', "\""),
    ('\u{00BB}', "\""),
    ('\u{2018}', "'"),
    ('\u{2019}', "'"),
    ('\u{2026}', "..."),
    ('\u{00A0}', "a plain space"),
    ('\u{202F}', "a plain space"),
    ('\u{2009}', "a plain space"),
    ('\u{200B}', "nothing"),
    ('\u{FEFF}', "nothing"),
    ('\u{0152}', "OE"),
    ('\u{0153}', "oe"),
    ('\u{00C6}', "AE"),
    ('\u{00E6}', "ae"),
];

/// Extensions of the text files the guard reads.
pub const SCANNED_EXTENSIONS: [&str; 7] = ["md", "rs", "toml", "ftl", "yml", "yaml", "txt"];

/// Directory names the guard never enters.
pub const SKIPPED_DIRECTORIES: [&str; 3] = [".git", "target", "node_modules"];

/// One refused character found in a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// File containing the character.
    pub path: PathBuf,
    /// One-based line number.
    pub line: usize,
    /// The refused character.
    pub character: char,
    /// What to write instead.
    pub replacement: &'static str,
}

/// Result of a scan: what was found, and which files were read.
#[derive(Debug, Default)]
pub struct Scan {
    /// Refused characters found.
    pub findings: Vec<Finding>,
    /// Every file read, so a caller can prove the scan reached where it should.
    pub scanned: Vec<PathBuf>,
}

/// Scans `root` recursively.
///
/// # Errors
///
/// Returns the first I/O error met while walking or reading.
pub fn scan(root: &Path) -> io::Result<Scan> {
    let mut result = Scan::default();
    walk(root, &mut result)?;
    Ok(result)
}

/// Refused characters in `text`, attributed to `path`.
#[must_use]
pub fn check_text(path: &Path, text: &str) -> Vec<Finding> {
    text.lines()
        .enumerate()
        .flat_map(|(index, line)| {
            line.chars().filter_map(move |character| {
                REFUSED
                    .iter()
                    .find(|(refused, _)| *refused == character)
                    .map(|(_, replacement)| Finding {
                        path: path.to_owned(),
                        line: index + 1,
                        character,
                        replacement,
                    })
            })
        })
        .collect()
}

fn walk(dir: &Path, result: &mut Scan) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            let skipped = path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| SKIPPED_DIRECTORIES.contains(&name));
            if !skipped {
                walk(&path, result)?;
            }
        } else if is_scanned(&path) {
            let text = fs::read_to_string(&path)?;
            result.findings.extend(check_text(&path, &text));
            result.scanned.push(path);
        }
    }
    Ok(())
}

fn is_scanned(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| SCANNED_EXTENSIONS.contains(&extension))
}
