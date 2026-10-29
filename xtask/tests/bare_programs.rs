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

//! The bare program guard run over this workspace: no application code starts a program by
//! a bare name, looked up on the `PATH` (and the current folder first on Windows), but the
//! exceptions it lists with their reasons; and every source is read.

use std::path::{Path, PathBuf};

use xtask::bare_programs::{
    ALLOWED, Constants, check_source, is_allowed, is_bare, scan, string_constants,
};
use xtask::folder_sources::sources;

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask sits inside the workspace")
        .to_owned()
}

#[test]
fn no_application_code_starts_a_program_by_a_bare_name() {
    let root = workspace_root();
    let result = scan(&root).expect("the sources are readable");
    let report: Vec<String> = result
        .findings
        .iter()
        .filter(|finding| !is_allowed(&root, &finding.file, &finding.program))
        .map(|finding| {
            format!(
                "{}:{}: \"{}\" is looked up on the PATH; start it by its full path \
                 (heimdall_core::tools::system_program), or allow it in ALLOWED with why",
                finding.file.display(),
                finding.line,
                finding.program
            )
        })
        .collect();
    assert!(report.is_empty(), "bare programs:\n{}", report.join("\n"));
}

#[test]
fn the_scan_reads_every_source_folder_and_the_ones_below() {
    // A scan that silently read nothing, or only the top of each `src`, would pass the
    // test above.
    let root = workspace_root();
    let result = scan(&root).expect("the sources are readable");
    let listed = sources(&root).expect("listed");
    assert!(result.scanned.len() > listed.len() / 2, "most sources read");
    assert!(result.scanned.iter().all(|file| listed.contains(file)));
    for file in [
        "crates/heimdall-ui/src/main.rs",
        "crates/heimdall-ui/src/tools/dnssec_tool.rs",
        "crates/heimdall-app/src/external_edit.rs",
        "crates/heimdall-core/src/tools/system_program.rs",
        "crates/heimdall-core/src/tools/library_sync/git.rs",
    ] {
        assert!(result.scanned.contains(&root.join(file)), "{file} not read");
    }
}

#[test]
fn every_allowance_is_still_used_and_says_why() {
    let root = workspace_root();
    let result = scan(&root).expect("the sources are readable");
    for allowed in ALLOWED {
        assert!(!allowed.reason.trim().is_empty());
        assert!(
            result.findings.iter().any(|finding| {
                finding.file == root.join(allowed.file) && finding.program == allowed.program
            }),
            "{} no longer starts {}: its allowance must go",
            allowed.file,
            allowed.program
        );
    }
}

#[test]
fn every_way_of_naming_a_bare_program_is_caught_outside_tests_and_comments() {
    let other_file = "pub(crate) const OPENER: &str = \"xdg-open\";\n\
                      pub const FULL: &str = \"/usr/bin/open\";\n";
    let mut global: Constants = Constants::new();
    for (name, value) in string_constants(other_file) {
        global.entry(name).or_default().push(value);
    }
    let text = "\
const LOOKUP: &str = \"nslookup.exe\";
fn a() { let _ = Command::new(\"nslookup\"); }
fn b() { let _ = std::process::Command::new(LOOKUP); }
fn c() { let _ = tokio::process::Command::new(&crate::opener::OPENER); }
fn d() { let _ = Command::new(\"/bin/sh\"); let _ = Command::new(\"C:\\\\Windows\\\\a.exe\"); }
fn e() { let _ = Command::new(program); let _ = Command::new(folder.join(LOOKUP)); }
fn f() { let _ = Command::new(crate::opener::FULL); }
// Command::new(\"git\") in a comment
#[cfg(test)]
mod tests {
    fn g() { let _ = Command::new(\"sh\"); }
}
fn h() { let _ = Command::new(\"git\").arg(\"x\"); }
";
    let found = check_source(text, &global);
    assert_eq!(
        found,
        [
            (2, "nslookup".to_owned()),
            (3, "nslookup.exe".to_owned()),
            (4, "xdg-open".to_owned()),
            (13, "git".to_owned()),
        ]
    );
}

#[test]
fn a_program_with_a_folder_is_not_bare() {
    assert!(is_bare("nslookup"));
    assert!(is_bare("git.exe"));
    assert!(!is_bare("/usr/bin/nslookup"));
    assert!(!is_bare(r"C:\Windows\System32\nslookup.exe"));
    assert!(!is_bare(""));
}
