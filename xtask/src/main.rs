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

//! Entry point of `cargo xtask`.
//!
//! Commands:
//!
//! - `legacy-lookup <key>`: prints the value of a C# Heimdall locale key in every supported
//!   language. The C# checkout is named by the `HEIMDALL_CS_REPO` environment variable.
//! - `catalogue-diff <csharp-repo>`: compares the shipped command catalogue and the debloat
//!   pack with the C# checkout's seed, prints each file that is not the same on both sides,
//!   and fails when there is one.

use std::env;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use xtask::catalogue_diff::compare;
use xtask::legacy_locales::{LEGACY_REPO_VARIABLE, lookup};

const LEGACY_LOOKUP: &str = "legacy-lookup";

const CATALOGUE_DIFF: &str = "catalogue-diff";

/// Printed in place of a value when a language lacks the key.
const ABSENT: &str = "<absent>";

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    match args.as_slice() {
        [command, key] if command == LEGACY_LOOKUP => legacy_lookup(key),
        [command, repo] if command == CATALOGUE_DIFF => catalogue_diff(Path::new(repo)),
        _ => {
            eprintln!("usage: cargo xtask {LEGACY_LOOKUP} <key>");
            eprintln!("       cargo xtask {CATALOGUE_DIFF} <csharp-repo>");
            ExitCode::FAILURE
        }
    }
}

fn legacy_lookup(key: &str) -> ExitCode {
    let Some(repo) = env::var_os(LEGACY_REPO_VARIABLE).map(PathBuf::from) else {
        eprintln!("{LEGACY_REPO_VARIABLE} is not set");
        return ExitCode::FAILURE;
    };
    match lookup(&repo, key) {
        Ok(translations) => {
            for translation in translations {
                let value = translation.value.as_deref().unwrap_or(ABSENT);
                println!("{} = {value}", translation.language);
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn catalogue_diff(csharp_repo: &Path) -> ExitCode {
    // xtask sits one folder below the root of the workspace.
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")));
    match compare(workspace, csharp_repo) {
        Ok(report) => {
            for difference in &report.differences {
                println!("{}: {}", difference.drift, difference.file.display());
            }
            println!(
                "{} of {} files differ",
                report.differences.len(),
                report.compared
            );
            if report.differences.is_empty() {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            }
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
