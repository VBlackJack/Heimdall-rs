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

//! The day of this build, for the About page's "Build date", chosen so that the same source
//! builds the same binary: `SOURCE_DATE_EPOCH` when the build sets it, as reproducible
//! builds do; else the time of the commit checked out, asked of git; else none, and the row
//! is not shown. The clock of the computer building is never read. A release build's date
//! comes from its release tag instead, read by the page itself, as the C# reads it from its
//! version.
//!
//! It also hands the page the versions of the components its "System" card names, read
//! from the workspace's `Cargo.lock`: the versions this binary is built with, offline.

#[path = "src/build_date.rs"]
mod build_date;

#[path = "src/component_versions.rs"]
mod component_versions;

use std::path::{Path, PathBuf};
use std::process::Command;

/// The variable of reproducible builds: the build's time, in seconds since 1970.
const SOURCE_DATE_EPOCH: &str = "SOURCE_DATE_EPOCH";

/// The file holding the versions every package of the workspace is built with.
const LOCK_FILE: &str = "Cargo.lock";

/// The files of the git folder written when the commit checked out changes.
const HEAD_FILES: [&str; 2] = ["HEAD", "logs/HEAD"];

fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo::rerun-if-changed=src/build_date.rs");
    println!("cargo::rerun-if-changed=src/component_versions.rs");
    println!("cargo::rerun-if-env-changed={SOURCE_DATE_EPOCH}");
    let seconds = std::env::var(SOURCE_DATE_EPOCH)
        .ok()
        .and_then(|value| value.trim().parse::<i64>().ok())
        .or_else(commit_time);
    if let Some(date) = seconds.and_then(build_date::civil_date) {
        println!(
            "cargo::rustc-env={}={date}",
            build_date::BUILD_DATE_VARIABLE
        );
    }
    pass_component_versions();
}

/// Sets the variable of each component the lock pins to one version; the others are left
/// unset, and the page leaves their rows out.
fn pass_component_versions() {
    let Some(lock) = lock_file() else {
        return;
    };
    println!("cargo::rerun-if-changed={}", lock.display());
    let Ok(text) = std::fs::read_to_string(&lock) else {
        return;
    };
    for (package, variable) in component_versions::COMPONENTS {
        if let Some(version) = component_versions::locked_version(&text, package) {
            println!("cargo::rustc-env={variable}={version}");
        }
    }
}

/// The `Cargo.lock` of the workspace: the nearest one in this package's folder or above it.
fn lock_file() -> Option<PathBuf> {
    let folder = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR")?);
    folder
        .ancestors()
        .map(|folder| folder.join(LOCK_FILE))
        .find(|lock| lock.is_file())
}

/// The time of the commit checked out, in seconds since 1970; `None` without git or
/// outside a repository. The build runs again when that commit changes.
fn commit_time() -> Option<i64> {
    let git_dir = PathBuf::from(git(&["rev-parse", "--absolute-git-dir"])?);
    for file in HEAD_FILES {
        let path = git_dir.join(file);
        if path.is_file() {
            println!("cargo::rerun-if-changed={}", path.display());
        }
    }
    git(&["log", "-1", "--format=%ct"])?.parse().ok()
}

/// What git says to `arguments`, run in this package's folder; `None` when it fails.
fn git(arguments: &[&str]) -> Option<String> {
    let folder = std::env::var_os("CARGO_MANIFEST_DIR")?;
    let output = Command::new("git")
        .args(arguments)
        .current_dir(Path::new(&folder))
        .output()
        .ok()
        .filter(|output| output.status.success())?;
    let said = String::from_utf8(output.stdout).ok()?;
    let said = said.trim();
    (!said.is_empty()).then(|| said.to_owned())
}
