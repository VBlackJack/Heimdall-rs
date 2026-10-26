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

//! Lists the files of the shipped command catalogue (`data/command-catalogue/actions`) so
//! the library embeds each one with `include_str!`: a later sync of the catalogue with the
//! C# Heimdall is a copy of its files, with no Rust list to keep in step. Standard library
//! only; no network, nothing is read outside the crate's own folder.

use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;

/// The catalogue's folder, under the crate.
const CATALOGUE_FOLDER: &str = "data/command-catalogue/actions";

/// The extension of an action file.
const ACTION_EXTENSION: &str = "json";

/// The file written into the build folder.
const OUTPUT_FILE: &str = "command_catalogue_files.rs";

fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo::rerun-if-changed={CATALOGUE_FOLDER}");
    let manifest = PathBuf::from(
        std::env::var_os("CARGO_MANIFEST_DIR").expect("cargo sets the manifest folder"),
    );
    let out = PathBuf::from(std::env::var_os("OUT_DIR").expect("cargo sets the output folder"));
    let folder = manifest.join(CATALOGUE_FOLDER);
    let mut names: Vec<String> = fs::read_dir(&folder)
        .expect("the catalogue folder is part of the crate")
        .filter_map(Result::ok)
        // Regular files only: a link or a folder named like an action is not one.
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == ACTION_EXTENSION)
        })
        .filter_map(|path| {
            path.file_name()
                .map(|name| name.to_string_lossy().into_owned())
        })
        .collect();
    // A fixed order: the same source builds the same binary.
    names.sort();
    let mut source = String::from("&[\n");
    // Paths stay relative to the crate and are resolved by the compiler from the current
    // manifest folder: the generated file is identical in every checkout, so a build folder
    // shared between worktrees never points at a folder that no longer exists.
    for name in &names {
        writeln!(
            source,
            "    include_str!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/{CATALOGUE_FOLDER}/{name}\")),"
        )
        .expect("writing to a string cannot fail");
    }
    source.push_str("]\n");
    fs::write(out.join(OUTPUT_FILE), source).expect("the build folder is writable");
}
