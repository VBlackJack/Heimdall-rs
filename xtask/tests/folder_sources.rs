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

//! The folder source guard run over this workspace: no application code but
//! `heimdall_core::paths` asks where a folder is, and every source is read.

use std::fs;
use std::path::{Path, PathBuf};

use xtask::folder_sources::{ALLOWED, REFUSED, check_source, scan, sources};

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask sits inside the workspace")
        .to_owned()
}

#[test]
fn no_application_code_but_paths_asks_where_a_folder_is() {
    let result = scan(&workspace_root()).expect("the sources are readable");
    let report: Vec<String> = result
        .findings
        .iter()
        .map(|finding| {
            format!(
                "{}:{}: {}; ask heimdall_core::paths instead",
                finding.file.display(),
                finding.line,
                finding.refused
            )
        })
        .collect();
    assert!(report.is_empty(), "folders asked:\n{}", report.join("\n"));
}

#[test]
fn the_scan_reads_every_source_folder_and_the_ones_below() {
    // A scan that silently read nothing, or only the top of each `src`, would pass the
    // test above.
    let root = workspace_root();
    let result = scan(&root).expect("the sources are readable");
    assert_eq!(result.scanned, sources(&root).expect("listed"));
    for file in [
        "crates/heimdall-ui/src/main.rs",
        "crates/heimdall-app/src/rdp_external.rs",
        "crates/heimdall-app/src/app/files_clipboard.rs",
        "crates/heimdall-core/src/import/citrix_cache.rs",
        "crates/heimdall-core/src/paths.rs",
    ] {
        assert!(result.scanned.contains(&root.join(file)), "{file} not read");
    }
}

#[test]
fn every_allowance_is_still_used_and_says_why() {
    let root = workspace_root();
    for allowed in ALLOWED {
        assert!(!allowed.reason.trim().is_empty());
        let text = fs::read_to_string(root.join(allowed.file)).expect("an allowed file");
        assert!(
            !check_source(&text).is_empty(),
            "{} no longer asks: its allowance must go",
            allowed.file
        );
    }
}

#[test]
fn every_way_of_asking_is_caught_outside_tests_and_comments() {
    let text = "\
fn a() { let _ = std::env::temp_dir(); }
fn b() { let _ = std::env::current_exe(); }
use directories::BaseDirs;
fn c() { let _ = ProjectDirs::from(\"\", \"\", \"x\"); let _ = UserDirs::new(); }
fn d() { known_folder(&KNOWNFOLDERID::Profile); SHGetKnownFolderPath(); }
fn e() { let _ = heimdall_core::paths::temp_dir(); }
// std::env::temp_dir() in a comment
#[cfg(test)]
mod tests {
    fn f() { let _ = std::env::temp_dir(); }
}
fn g() { let _ = std::env::current_exe(); }
";
    let found = check_source(text);
    let lines: Vec<usize> = found.iter().map(|(line, _)| *line).collect();
    assert_eq!(lines, [1, 2, 3, 3, 4, 4, 5, 5, 5, 12]);
    let mut kinds: Vec<&str> = found.iter().map(|(_, refused)| *refused).collect();
    kinds.sort_unstable();
    kinds.dedup();
    let mut every = REFUSED.to_vec();
    every.sort_unstable();
    assert_eq!(kinds, every);
}
