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

//! Test data folder guard: the `test-data-dir` feature, which roots every per-user folder in
//! a scratch folder for automated UI tests, is never turned on by a build that is not one.
//!
//! No manifest turns it on by default, through another feature, or in a dependency's
//! features; no workflow or build script names it, or asks for every feature at once. Only
//! the features that pass it down to the crate below may name it. A release build could not
//! compile it anyway (`heimdall-core` refuses it without debug assertions): this guard keeps
//! it out of the commands that build what is shipped.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// The feature guarded.
pub const FEATURE: &str = "test-data-dir";

/// The cargo flag that turns every feature on, this one with them.
pub const ALL_FEATURES: &str = "--all-features";

/// The extensions of the build scripts and workflows read.
pub const SCRIPT_EXTENSIONS: [&str; 6] = ["yml", "yaml", "bat", "cmd", "ps1", "sh"];

/// The folders whose build scripts and workflows are read, under the workspace root; the
/// root's own are read too.
pub const SCRIPT_FOLDERS: [&str; 3] = [".github/workflows", "scripts", ".cargo"];

/// Cargo's own configuration, which can name features through an alias.
pub const CARGO_CONFIG: [&str; 2] = [".cargo/config.toml", ".cargo/config"];

/// One place that turns the feature on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// The file.
    pub file: PathBuf,
    /// What in it turns the feature on.
    pub what: String,
}

/// Result of a scan: what was found, and which files were read.
#[derive(Debug, Default)]
pub struct Scan {
    /// Places refused.
    pub findings: Vec<Finding>,
    /// Files read.
    pub scanned: Vec<PathBuf>,
}

/// Whether the feature `entry` of a feature list or a dependency's features names this
/// feature, of its own crate (`test-data-dir`) or of another (`crate/test-data-dir`,
/// `crate?/test-data-dir`).
fn names_feature(entry: &str) -> bool {
    entry == FEATURE || entry.rsplit('/').next() == Some(FEATURE)
}

/// The strings of the array `value`, when it is one.
fn strings(value: &toml::Value) -> Vec<&str> {
    value
        .as_array()
        .map(|items| items.iter().filter_map(toml::Value::as_str).collect())
        .unwrap_or_default()
}

/// What in the manifest `text` turns the feature on, each said in words: a default or
/// another feature naming it, a dependency declared with it. Only a feature of the same name
/// may name it, to pass it down.
///
/// # Errors
///
/// A manifest that is not TOML.
pub fn check_manifest(text: &str) -> io::Result<Vec<String>> {
    let document: toml::Table = text.parse().map_err(io::Error::other)?;
    let mut found = Vec::new();
    if let Some(features) = document.get("features").and_then(toml::Value::as_table) {
        for (name, list) in features {
            if name == FEATURE {
                continue;
            }
            for entry in strings(list)
                .into_iter()
                .filter(|entry| names_feature(entry))
            {
                found.push(format!("feature `{name}` turns on `{entry}`"));
            }
        }
    }
    let mut tables: Vec<&toml::Table> = Vec::new();
    collect_dependency_tables(&document, &mut tables);
    if let Some(targets) = document.get("target").and_then(toml::Value::as_table) {
        for target in targets.values().filter_map(toml::Value::as_table) {
            collect_dependency_tables(target, &mut tables);
        }
    }
    if let Some(workspace) = document.get("workspace").and_then(toml::Value::as_table) {
        collect_dependency_tables(workspace, &mut tables);
    }
    for table in tables {
        for (dependency, declaration) in table {
            let Some(features) = declaration.get("features") else {
                continue;
            };
            for entry in strings(features)
                .into_iter()
                .filter(|entry| names_feature(entry))
            {
                found.push(format!(
                    "dependency `{dependency}` is declared with `{entry}`"
                ));
            }
        }
    }
    Ok(found)
}

/// The dependency tables of `table`.
fn collect_dependency_tables<'a>(table: &'a toml::Table, into: &mut Vec<&'a toml::Table>) {
    for name in ["dependencies", "dev-dependencies", "build-dependencies"] {
        if let Some(found) = table.get(name).and_then(toml::Value::as_table) {
            into.push(found);
        }
    }
}

/// What in the workflow or build script `text` turns the feature on: a line naming it, or
/// asking for every feature.
#[must_use]
pub fn check_script(text: &str) -> Vec<String> {
    text.lines()
        .enumerate()
        .filter(|(_, line)| line.contains(FEATURE) || line.contains(ALL_FEATURES))
        .map(|(index, line)| format!("line {}: {}", index + 1, line.trim()))
        .collect()
}

/// The manifests of the workspace at `root`: the root's, one per directory under `crates`,
/// and the `xtask` one.
///
/// # Errors
///
/// A directory that cannot be read.
pub fn manifests(root: &Path) -> io::Result<Vec<PathBuf>> {
    let mut found = vec![
        root.join("Cargo.toml"),
        root.join("xtask").join("Cargo.toml"),
    ];
    for entry in fs::read_dir(root.join("crates"))? {
        let manifest = entry?.path().join("Cargo.toml");
        if manifest.is_file() {
            found.push(manifest);
        }
    }
    found.sort();
    Ok(found)
}

/// Whether `path` is a workflow or build script, by its extension.
fn is_script(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| SCRIPT_EXTENSIONS.contains(&extension))
}

/// The scripts of `folder` and every folder under it; nothing when it is not there.
fn scripts_under(folder: &Path, into: &mut Vec<PathBuf>) -> io::Result<()> {
    let entries = match fs::read_dir(folder) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    for entry in entries {
        let path = entry?.path();
        if path.is_dir() {
            scripts_under(&path, into)?;
        } else if is_script(&path) {
            into.push(path);
        }
    }
    Ok(())
}

/// The workflows and build scripts of the workspace at `root`: those at its root, those
/// under [`SCRIPT_FOLDERS`], and cargo's configuration.
///
/// # Errors
///
/// A directory that cannot be read.
pub fn scripts(root: &Path) -> io::Result<Vec<PathBuf>> {
    let mut found = Vec::new();
    for entry in fs::read_dir(root)? {
        let path = entry?.path();
        if path.is_file() && is_script(&path) {
            found.push(path);
        }
    }
    for folder in SCRIPT_FOLDERS {
        scripts_under(&root.join(folder), &mut found)?;
    }
    found.extend(
        CARGO_CONFIG
            .iter()
            .map(|config| root.join(config))
            .filter(|config| config.is_file()),
    );
    found.sort();
    found.dedup();
    Ok(found)
}

/// Every manifest, workflow and build script of the workspace at `root` checked.
///
/// # Errors
///
/// A file that cannot be read, or a manifest that is not TOML.
pub fn scan(root: &Path) -> io::Result<Scan> {
    let mut result = Scan::default();
    for manifest in manifests(root)? {
        let text = fs::read_to_string(&manifest)?;
        for what in check_manifest(&text)? {
            result.findings.push(Finding {
                file: manifest.clone(),
                what,
            });
        }
        result.scanned.push(manifest);
    }
    for script in scripts(root)? {
        let text = fs::read_to_string(&script)?;
        for what in check_script(&text) {
            result.findings.push(Finding {
                file: script.clone(),
                what,
            });
        }
        result.scanned.push(script);
    }
    Ok(result)
}
