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

//! The test data folder guard run over this workspace: no manifest, workflow or build
//! script turns the `test-data-dir` feature on, and every one of them is read.

use std::fs;
use std::path::{Path, PathBuf};

use xtask::test_data_dir::{FEATURE, check_manifest, check_script, manifests, scan, scripts};

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask sits inside the workspace")
        .to_owned()
}

#[test]
fn no_manifest_workflow_or_build_script_turns_the_test_data_folder_on() {
    let result = scan(&workspace_root()).expect("the files are readable");
    let report: Vec<String> = result
        .findings
        .iter()
        .map(|finding| format!("{}: {}", finding.file.display(), finding.what))
        .collect();
    assert!(
        report.is_empty(),
        "{FEATURE} turned on outside a test build:\n{}",
        report.join("\n")
    );
}

#[test]
fn the_scan_reads_every_manifest_the_ci_workflow_and_the_run_scripts() {
    // A scan that silently read nothing would pass the test above: every crate's manifest,
    // the root's and the xtask's are read, and so are the workflow under .github/workflows,
    // a folder below the root, and the scripts at the root.
    let root = workspace_root();
    let result = scan(&root).expect("the files are readable");
    let mut expected: Vec<PathBuf> = fs::read_dir(root.join("crates"))
        .expect("crates/")
        .map(|entry| entry.expect("entry").path().join("Cargo.toml"))
        .filter(|manifest| manifest.is_file())
        .collect();
    expected.push(root.join("Cargo.toml"));
    expected.push(root.join("xtask").join("Cargo.toml"));
    expected.sort();
    assert_eq!(manifests(&root).expect("listed"), expected);
    let scripts = scripts(&root).expect("listed");
    for file in [
        root.join(".github").join("workflows").join("ci.yml"),
        root.join("run-debug.bat"),
        root.join("run-release.bat"),
    ] {
        assert!(scripts.contains(&file), "{} not read", file.display());
        assert!(
            result.scanned.contains(&file),
            "{} not scanned",
            file.display()
        );
    }
    for manifest in &expected {
        assert!(
            result.scanned.contains(manifest),
            "{} not scanned",
            manifest.display()
        );
    }
}

#[test]
fn the_feature_exists_off_by_default_where_it_is_passed_down() {
    // The positive control: the feature is declared, by name, in the crates that have it,
    // and the guard lets the declarations that only pass it down through.
    let root = workspace_root();
    for name in ["heimdall-core", "heimdall-app", "heimdall-ui"] {
        let manifest = root.join("crates").join(name).join("Cargo.toml");
        let text = fs::read_to_string(&manifest).expect("a manifest");
        let document: toml::Table = text.parse().expect("TOML");
        let features = document
            .get("features")
            .and_then(toml::Value::as_table)
            .unwrap_or_else(|| panic!("{name} has no features"));
        assert!(features.contains_key(FEATURE), "{name} lacks {FEATURE}");
        assert!(
            !features.contains_key("default"),
            "{name} has default features"
        );
        assert_eq!(check_manifest(&text).expect("TOML"), Vec::<String>::new());
    }
}

#[test]
fn every_way_a_manifest_turns_the_feature_on_is_caught() {
    let text = r#"
[package]
name = "probe"

[features]
default = ["test-data-dir"]
audio = ["heimdall-core/test-data-dir"]
quiet = ["heimdall-app?/test-data-dir"]
test-data-dir = ["heimdall-core/test-data-dir"]
other = ["heimdall-rdp/audio"]

[dependencies]
heimdall-core = { workspace = true, features = ["test-data-dir"] }
serde = { version = "1", features = ["derive"] }

[dev-dependencies]
heimdall-app = { path = "../heimdall-app", features = ["heimdall-core/test-data-dir"] }

[target.'cfg(windows)'.dependencies]
heimdall-ui = { path = "../heimdall-ui", features = ["test-data-dir"] }

[workspace.dependencies]
heimdall-core = { path = "crates/heimdall-core", features = ["test-data-dir"] }
"#;
    let mut found = check_manifest(text).expect("TOML");
    found.sort();
    assert_eq!(
        found,
        [
            "dependency `heimdall-app` is declared with `heimdall-core/test-data-dir`",
            "dependency `heimdall-core` is declared with `test-data-dir`",
            "dependency `heimdall-core` is declared with `test-data-dir`",
            "dependency `heimdall-ui` is declared with `test-data-dir`",
            "feature `audio` turns on `heimdall-core/test-data-dir`",
            "feature `default` turns on `test-data-dir`",
            "feature `quiet` turns on `heimdall-app?/test-data-dir`",
        ]
    );
}

#[test]
fn a_script_naming_the_feature_or_every_feature_is_caught() {
    let text = "\
cargo build --release --locked -p heimdall-ui
cargo build --release --features heimdall-ui/test-data-dir
cargo clippy --workspace --all-features
cargo test --workspace --locked
";
    assert_eq!(
        check_script(text),
        [
            "line 2: cargo build --release --features heimdall-ui/test-data-dir",
            "line 3: cargo clippy --workspace --all-features",
        ]
    );
}
