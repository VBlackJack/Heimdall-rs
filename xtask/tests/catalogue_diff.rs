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

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use tempfile::TempDir;
use xtask::catalogue_diff::{
    CSHARP_SEED_DIR, Difference, Drift, PACK_FILE, RUST_CATALOGUE_DIR, Report, compare,
};

/// A Heimdall-rs root and a C# root, each with its (empty) catalogue folder.
struct Sides {
    rust: TempDir,
    csharp: TempDir,
}

impl Sides {
    fn new() -> Self {
        let sides = Self {
            rust: tempfile::tempdir().expect("folder"),
            csharp: tempfile::tempdir().expect("folder"),
        };
        fs::create_dir_all(sides.rust.path().join(RUST_CATALOGUE_DIR)).expect("folder");
        fs::create_dir_all(sides.csharp.path().join(CSHARP_SEED_DIR)).expect("folder");
        sides
    }

    fn rust_action(&self, name: &str, text: &str) {
        write(&self.rust.path().join(RUST_CATALOGUE_DIR).join(name), text);
    }

    fn csharp_action(&self, name: &str, text: &str) {
        write(&self.csharp.path().join(CSHARP_SEED_DIR).join(name), text);
    }

    fn compare(&self) -> io::Result<Report> {
        compare(self.rust.path(), self.csharp.path())
    }
}

fn write(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().expect("a parent")).expect("folder");
    fs::write(path, text).expect("written");
}

fn difference(file: PathBuf, drift: Drift) -> Difference {
    Difference { file, drift }
}

#[test]
fn the_same_json_written_another_way_is_no_difference() {
    let sides = Sides::new();
    sides.rust_action("a.json", "{\n  \"id\": \"a\",\n  \"level\": 1\n}\n");
    sides.csharp_action("a.json", "\u{feff}{\r\n\"level\":1,\r\n\"id\":\"a\"}\r\n");
    write(&sides.rust.path().join(PACK_FILE), "{\"actions\": []}");
    write(
        &sides.csharp.path().join(PACK_FILE),
        "{ \"actions\" : [ ] }",
    );
    let report = sides.compare().expect("compared");
    assert_eq!(
        report,
        Report {
            compared: 2,
            differences: Vec::new()
        }
    );
}

#[test]
fn each_file_that_differs_or_is_on_one_side_only_is_listed() {
    let sides = Sides::new();
    sides.rust_action("same.json", "{\"id\": \"same\"}");
    sides.csharp_action("same.json", "{\"id\": \"same\"}");
    sides.rust_action("changed.json", "{\"id\": \"changed\"}");
    sides.csharp_action(
        "changed.json",
        "{\"id\": \"changed\", \"translations\": {\"es\": {}}}",
    );
    sides.rust_action("rust-only.json", "{}");
    sides.csharp_action("csharp-only.json", "{}");
    // Not an action file: never looked at.
    sides.rust_action("README.md", "# Catalogue");
    write(&sides.rust.path().join(PACK_FILE), "{\"actions\": []}");
    write(
        &sides.csharp.path().join(PACK_FILE),
        "{\"actions\": [{\"id\": \"x\"}]}",
    );
    let report = sides.compare().expect("compared");
    assert_eq!(report.compared, 5);
    assert_eq!(
        report.differences,
        [
            difference(
                Path::new(RUST_CATALOGUE_DIR).join("changed.json"),
                Drift::Differs
            ),
            difference(
                Path::new(CSHARP_SEED_DIR).join("csharp-only.json"),
                Drift::CSharpOnly
            ),
            difference(
                Path::new(RUST_CATALOGUE_DIR).join("rust-only.json"),
                Drift::RustOnly
            ),
            difference(PathBuf::from(PACK_FILE), Drift::Differs),
        ]
    );
}

#[test]
fn a_pack_on_one_side_only_is_listed_and_on_neither_is_not_counted() {
    let sides = Sides::new();
    assert_eq!(sides.compare().expect("compared"), Report::default());
    write(&sides.rust.path().join(PACK_FILE), "{}");
    let report = sides.compare().expect("compared");
    assert_eq!(
        report.differences,
        [difference(PathBuf::from(PACK_FILE), Drift::RustOnly)]
    );
}

#[test]
fn a_file_that_is_not_json_or_a_missing_seed_folder_is_an_error() {
    let sides = Sides::new();
    sides.rust_action("a.json", "{}");
    sides.csharp_action("a.json", "not json");
    let error = sides.compare().expect_err("not JSON");
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    assert!(error.to_string().contains("a.json"), "{error}");

    let elsewhere = tempfile::tempdir().expect("folder");
    let error = compare(sides.rust.path(), elsewhere.path()).expect_err("no seed folder");
    assert_eq!(error.kind(), io::ErrorKind::NotFound);
    assert!(error.to_string().contains(CSHARP_SEED_DIR), "{error}");
}
