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

use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

use xtask::legacy_locales::{LEGACY_REPO_VARIABLE, lookup};

const LEGACY_LOOKUP: &str = "legacy-lookup";

/// Printed in place of a value when a language lacks the key.
const ABSENT: &str = "<absent>";

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    match args.as_slice() {
        [command, key] if command == LEGACY_LOOKUP => legacy_lookup(key),
        _ => {
            eprintln!("usage: cargo xtask {LEGACY_LOOKUP} <key>");
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
