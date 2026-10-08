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

//! The folders restricted on Windows to the user, the Administrators and SYSTEM, as the C#
//! `AclEnforcer`, read back from the system: only on Windows.

#![cfg(windows)]

#[path = "support/dacl.rs"]
mod dacl;

use heimdall_core::folder_acl::{
    RestrictError, current_user_sid, icacls_program, is_sid, restrict, restrict_all,
};

/// Flags of an entry set on the folder: inherited by its files and folders.
const SET_HERE: &str = "OICI";

/// Flags of an entry a file takes from its folder.
const INHERITED_BY_A_FILE: &str = "ID";

/// Flags of an entry a folder takes from its parent.
const INHERITED_BY_A_FOLDER: &str = "OICIID";

#[test]
fn the_current_user_is_read_as_a_sid_string() {
    let user = current_user_sid().expect("user");
    assert!(is_sid(&user), "{user}");
    assert!(user.starts_with("S-1-5-"), "{user}");
}

#[test]
fn icacls_is_taken_from_the_system_folder() {
    let program = icacls_program().expect("icacls");
    assert!(program.is_absolute());
    assert!(program.is_file(), "{}", program.display());
    assert!(
        program
            .parent()
            .and_then(|folder| folder.file_name())
            .is_some_and(|name| name.eq_ignore_ascii_case("System32")),
        "{}",
        program.display()
    );
}

#[test]
fn a_folder_keeps_only_the_three_and_what_it_holds_takes_them() {
    let dir = tempfile::tempdir().expect("dir");
    let folder = dir.path().join("config");
    std::fs::create_dir(&folder).expect("folder");
    let before = folder.join("profiles.toml");
    std::fs::write(&before, b"").expect("file");

    restrict(&folder).expect("restricted");
    // Again, as at each start: the same list.
    restrict(&folder).expect("restricted again");

    let found = dacl::read(&folder);
    assert!(found.protected, "{found:?}");
    assert_eq!(found.entries, dacl::restricted(SET_HERE));
    let file = dacl::read(&before);
    assert_eq!(
        file.entries,
        dacl::restricted(INHERITED_BY_A_FILE),
        "already there"
    );
    let after = folder.join("logs");
    std::fs::create_dir(&after).expect("made after");
    let made = dacl::read(&after);
    assert!(!made.protected);
    assert_eq!(
        made.entries,
        dacl::restricted(INHERITED_BY_A_FOLDER),
        "made after"
    );
}

#[test]
fn one_folder_failing_does_not_stop_the_others_and_is_said() {
    let dir = tempfile::tempdir().expect("dir");
    let blocker = dir.path().join("a file");
    std::fs::write(&blocker, b"").expect("file");
    let unmade = blocker.join("config");
    let data = dir.path().join("data");

    let failed = restrict_all(&[unmade.clone(), data.clone()]);

    assert_eq!(failed.len(), 1, "{failed:?}");
    assert_eq!(failed[0].folder, unmade);
    assert!(matches!(failed[0].error, RestrictError::NotMade(_)));
    assert!(failed[0].to_string().contains("not restricted"));
    let found = dacl::read(&data);
    assert!(found.protected);
    assert_eq!(found.entries, dacl::restricted(SET_HERE));
}

#[test]
fn icacls_failing_or_a_relative_path_is_an_error() {
    let dir = tempfile::tempdir().expect("dir");
    let missing = dir.path().join("missing");
    assert!(matches!(
        restrict(&missing),
        Err(RestrictError::Refused(Some(code))) if code != 0
    ));
    assert!(matches!(
        restrict(std::path::Path::new("/q")),
        Err(RestrictError::NotAbsolute)
    ));
}
