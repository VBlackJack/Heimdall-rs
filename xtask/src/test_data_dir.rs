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
//! the features that pass it down to the crate below may name it, and only one CI job,
//! [`ALLOWED_JOB`], may build with it: in debug, uploading nothing.
//!
//! A release build cannot compile it (`heimdall-core` refuses it without debug assertions),
//! so no release profile may turn debug assertions on, nor any profile inheriting it, and no
//! script may set them.

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

/// The name of the one CI job that builds with the feature: lints and tests, in debug.
pub const ALLOWED_JOB: &str = "test-data-dir (debug)";

/// What that job may never do: build for release, upload anything, turn every feature on.
pub const REFUSED_IN_ALLOWED_JOB: [&str; 3] = ["--release", "upload", ALL_FEATURES];

/// What turns debug assertions on, in a manifest's or cargo's configuration or in the
/// environment: never written in a workflow or build script.
pub const DEBUG_ASSERTIONS: [&str; 2] = ["debug-assertions", "DEBUG_ASSERTIONS"];

/// The key of debug assertions in a profile.
const DEBUG_ASSERTIONS_KEY: &str = "debug-assertions";

/// The profile a release is built with.
const RELEASE_PROFILE: &str = "release";

/// The key naming the profile a profile inherits from.
const INHERITS_KEY: &str = "inherits";

/// The line opening the jobs of a workflow.
const JOBS_LINE: &str = "jobs:";

/// The indentation of a job's key under `jobs:`.
const JOB_INDENT: usize = 2;

/// The key of a job's displayed name.
const NAME_KEY: &str = "name:";

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
    /// CI jobs named [`ALLOWED_JOB`] found.
    pub allowed_jobs: usize,
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
    if let Some(profiles) = document.get("profile").and_then(toml::Value::as_table) {
        for (name, profile) in profiles {
            let inherits_release = profile
                .get(INHERITS_KEY)
                .and_then(toml::Value::as_str)
                .is_some_and(|inherited| inherited == RELEASE_PROFILE);
            if (name == RELEASE_PROFILE || inherits_release) && sets_debug_assertions(profile) {
                found.push(format!(
                    "profile `{name}` builds a release with debug assertions"
                ));
            }
        }
    }
    Ok(found)
}

/// Whether `value`, a profile or a table inside one (a package's, the build scripts'), turns
/// debug assertions on anywhere.
fn sets_debug_assertions(value: &toml::Value) -> bool {
    value.as_table().is_some_and(|table| {
        table.iter().any(|(key, inner)| {
            (key == DEBUG_ASSERTIONS_KEY && inner.as_bool() == Some(true))
                || sets_debug_assertions(inner)
        })
    })
}

/// The dependency tables of `table`.
fn collect_dependency_tables<'a>(table: &'a toml::Table, into: &mut Vec<&'a toml::Table>) {
    for name in ["dependencies", "dev-dependencies", "build-dependencies"] {
        if let Some(found) = table.get(name).and_then(toml::Value::as_table) {
            into.push(found);
        }
    }
}

/// Whether `line` of a workflow or build script turns the feature or debug assertions on.
fn refused_line(line: &str) -> bool {
    line.contains(FEATURE)
        || line.contains(ALL_FEATURES)
        || DEBUG_ASSERTIONS.iter().any(|marker| line.contains(marker))
}

/// A refused line, said with its number.
fn said(index: usize, line: &str) -> String {
    format!("line {}: {}", index + 1, line.trim())
}

/// What in the build script `text` turns the feature on: a line naming it, asking for every
/// feature, or turning debug assertions on.
#[must_use]
pub fn check_script(text: &str) -> Vec<String> {
    text.lines()
        .enumerate()
        .filter(|(_, line)| refused_line(line))
        .map(|(index, line)| said(index, line))
        .collect()
}

/// What in the workflow `text` turns the feature on, as [`check_script`] says it, outside
/// the job named [`ALLOWED_JOB`]; inside it, what it may never do
/// ([`REFUSED_IN_ALLOWED_JOB`], debug assertions). Comments are not read. With how many jobs
/// have that name.
#[must_use]
pub fn check_workflow(text: &str) -> (Vec<String>, usize) {
    let lines: Vec<&str> = text.lines().collect();
    // Each line's job, by the index of the line opening it.
    let mut job_of: Vec<Option<usize>> = Vec::with_capacity(lines.len());
    let mut in_jobs = false;
    let mut job = None;
    for (index, line) in lines.iter().enumerate() {
        let indent = line.len() - line.trim_start().len();
        let content = line.trim();
        let significant = !content.is_empty() && !content.starts_with('#');
        if significant && indent == 0 {
            in_jobs = content == JOBS_LINE;
            job = None;
        } else if in_jobs && significant && indent == JOB_INDENT && content.ends_with(':') {
            job = Some(index);
        }
        job_of.push(job);
    }
    let allowed: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(index, line)| {
            job_of[*index].is_some()
                && line
                    .trim()
                    .strip_prefix(NAME_KEY)
                    .is_some_and(|name| name.trim().trim_matches(['"', '\'']) == ALLOWED_JOB)
        })
        .filter_map(|(index, _)| job_of[index])
        .collect();
    let found = lines
        .iter()
        .enumerate()
        // A comment runs nothing.
        .filter(|(_, line)| !line.trim_start().starts_with('#'))
        .filter(|(index, line)| {
            if job_of[*index].is_some_and(|start| allowed.contains(&start)) {
                REFUSED_IN_ALLOWED_JOB
                    .iter()
                    .chain(DEBUG_ASSERTIONS.iter())
                    .any(|refused| line.contains(refused))
            } else {
                refused_line(line)
            }
        })
        .map(|(index, line)| said(index, line))
        .collect();
    (found, allowed.len())
}

/// Whether `path` is a workflow, read by [`check_workflow`].
fn is_workflow(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| WORKFLOW_EXTENSIONS.contains(&extension))
}

/// The extensions of a workflow.
const WORKFLOW_EXTENSIONS: [&str; 2] = ["yml", "yaml"];

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
        let found = if is_workflow(&script) {
            let (found, allowed) = check_workflow(&text);
            result.allowed_jobs += allowed;
            found
        } else {
            check_script(&text)
        };
        result
            .findings
            .extend(found.into_iter().map(|what| Finding {
                file: script.clone(),
                what,
            }));
        result.scanned.push(script);
    }
    if result.allowed_jobs > 1 {
        result.findings.push(Finding {
            file: root.join(SCRIPT_FOLDERS[0]),
            what: format!("{} jobs are named `{ALLOWED_JOB}`", result.allowed_jobs),
        });
    }
    Ok(result)
}
