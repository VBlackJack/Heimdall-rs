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

//! Measures the drift between the command catalogue Heimdall-rs ships and the C# Heimdall's
//! seed it is copied from: each action file, and the optional debloat pack, compared as JSON
//! (so line ends, indentation and the order of fields do not count).

use std::collections::BTreeSet;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde_json::Value;

/// The shipped catalogue's folder, under the root of the Heimdall-rs workspace.
pub const RUST_CATALOGUE_DIR: &str = "crates/heimdall-core/data/command-catalogue/actions";

/// The seed's folder, under the root of a C# Heimdall checkout.
pub const CSHARP_SEED_DIR: &str = "data/seed/actions";

/// The debloat pack, at the same place under both roots.
pub const PACK_FILE: &str = "data/packs/windows-workstation-debloat.json";

/// The extension of an action file.
const ACTION_EXTENSION: &str = "json";

/// A byte order mark some Windows editors write at the start of a file.
const BYTE_ORDER_MARK: char = '\u{feff}';

/// How a file of one side stands against the other.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Drift {
    /// Both sides have it, with a different content.
    Differs,
    /// Only Heimdall-rs has it.
    RustOnly,
    /// Only the C# has it.
    CSharpOnly,
}

impl fmt::Display for Drift {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Differs => "differs",
            Self::RustOnly => "only in Heimdall-rs",
            Self::CSharpOnly => "only in the C#",
        })
    }
}

/// One file that is not the same on both sides.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Difference {
    /// The file, relative to the root of the side that has it (Heimdall-rs when both do).
    pub file: PathBuf,
    /// How it differs.
    pub drift: Drift,
}

/// The outcome of a comparison.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Report {
    /// The files looked at, on either side.
    pub compared: usize,
    /// The files that are not the same, in the order of their names.
    pub differences: Vec<Difference>,
}

/// Compares the catalogue and the pack of the Heimdall-rs workspace at `rust_root` with the
/// C# checkout at `csharp_root`.
///
/// # Errors
///
/// Returns an error when a catalogue folder cannot be listed, or a file cannot be read or is
/// not JSON.
pub fn compare(rust_root: &Path, csharp_root: &Path) -> io::Result<Report> {
    let mut report = Report::default();
    let rust_dir = rust_root.join(RUST_CATALOGUE_DIR);
    let csharp_dir = csharp_root.join(CSHARP_SEED_DIR);
    let rust_names = action_names(&rust_dir)?;
    let csharp_names = action_names(&csharp_dir)?;
    for name in rust_names.union(&csharp_names) {
        let side = |dir: &str| Path::new(dir).join(name);
        compare_file(
            (&rust_dir.join(name), side(RUST_CATALOGUE_DIR)),
            (&csharp_dir.join(name), side(CSHARP_SEED_DIR)),
            &mut report,
        )?;
    }
    compare_file(
        (&rust_root.join(PACK_FILE), PathBuf::from(PACK_FILE)),
        (&csharp_root.join(PACK_FILE), PathBuf::from(PACK_FILE)),
        &mut report,
    )?;
    Ok(report)
}

/// Compares one file of each side, each given as its path on disk and the path it is shown
/// under; a file neither side has is not counted.
fn compare_file(
    (rust, rust_shown): (&Path, PathBuf),
    (csharp, csharp_shown): (&Path, PathBuf),
    report: &mut Report,
) -> io::Result<()> {
    let drift = match (rust.is_file(), csharp.is_file()) {
        (false, false) => return Ok(()),
        (true, false) => Some((Drift::RustOnly, rust_shown)),
        (false, true) => Some((Drift::CSharpOnly, csharp_shown)),
        (true, true) => {
            (read_json(rust)? != read_json(csharp)?).then_some((Drift::Differs, rust_shown))
        }
    };
    report.compared += 1;
    if let Some((drift, file)) = drift {
        report.differences.push(Difference { file, drift });
    }
    Ok(())
}

/// The names of the action files of `dir`.
fn action_names(dir: &Path) -> io::Result<BTreeSet<String>> {
    let mut names = BTreeSet::new();
    let entries = fs::read_dir(dir)
        .map_err(|error| io::Error::new(error.kind(), format!("{}: {error}", dir.display())))?;
    for entry in entries {
        let path = entry?.path();
        let is_action = path.is_file()
            && path
                .extension()
                .is_some_and(|extension| extension == ACTION_EXTENSION);
        if let (true, Some(name)) = (is_action, path.file_name()) {
            names.insert(name.to_string_lossy().into_owned());
        }
    }
    Ok(names)
}

/// The JSON content of the file at `path`.
fn read_json(path: &Path) -> io::Result<Value> {
    let text = fs::read_to_string(path)?;
    serde_json::from_str(text.trim_start_matches(BYTE_ORDER_MARK)).map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{}: {error}", path.display()),
        )
    })
}
