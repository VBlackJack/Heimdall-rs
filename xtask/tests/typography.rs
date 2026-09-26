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

use std::path::{Path, PathBuf};

use xtask::typography::{REFUSED, check_text, scan};

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask sits inside the workspace")
        .to_owned()
}

#[test]
fn the_repository_contains_no_refused_character() {
    let result = scan(&workspace_root()).expect("the workspace is readable");
    let report: Vec<String> = result
        .findings
        .iter()
        .map(|f| {
            format!(
                "{}:{} U+{:04X}, write {}",
                f.path.display(),
                f.line,
                u32::from(f.character),
                f.replacement
            )
        })
        .collect();
    assert!(
        report.is_empty(),
        "refused characters:\n{}",
        report.join("\n")
    );
}

#[test]
fn the_scan_reaches_nested_subdirectories() {
    // A scan that silently stopped at the top level would pass the test above while
    // reading nothing that matters. The French Fluent file sits four levels down.
    let root = workspace_root();
    let nested = root
        .join("crates")
        .join("heimdall-ui")
        .join("i18n")
        .join("fr")
        .join("heimdall-ui.ftl");
    let result = scan(&root).expect("the workspace is readable");
    assert!(
        result.scanned.contains(&nested),
        "the scan never read {}",
        nested.display()
    );
}

#[test]
fn every_refused_character_is_detected() {
    for (character, replacement) in REFUSED {
        let findings = check_text(Path::new("probe.md"), &format!("before {character} after"));
        assert_eq!(
            findings.len(),
            1,
            "U+{:04X} not detected",
            u32::from(character)
        );
        assert_eq!(findings[0].replacement, replacement);
    }
}

#[test]
fn french_and_spanish_accents_are_allowed() {
    let text = "réécriture, à, ç, ê, ï, ù, ñ, ¿qué?, ¡olé!";
    assert!(check_text(Path::new("probe.md"), text).is_empty());
}

#[test]
fn vendored_crates_are_left_as_published_but_their_notes_are_read() {
    let root = workspace_root();
    let result = scan(&root).expect("the workspace is readable");
    let notes = root.join("vendor").join("PATCHES.md");
    assert!(
        result.scanned.contains(&notes),
        "{} not read",
        notes.display()
    );
    let vendored = root.join("vendor").join("ironrdp-connector");
    assert!(
        vendored.is_dir(),
        "the positive control needs a vendored crate"
    );
    assert!(
        !result
            .scanned
            .iter()
            .any(|path| path.starts_with(&vendored)),
        "a vendored crate was read"
    );
}
