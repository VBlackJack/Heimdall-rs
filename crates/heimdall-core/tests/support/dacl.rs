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

//! A folder's access list read back on Windows, for a test to check: `icacls /save` writes
//! it as SDDL, which is never localised, in UTF-16.

#![allow(dead_code, reason = "each test binary uses the part it needs")]

use std::collections::BTreeSet;
use std::path::Path;
use std::process::Command;

use heimdall_core::folder_acl::{current_user_sid, icacls_program};

/// What starts the access list in SDDL.
const DACL_START: &str = "D:";

/// The flag of a protected access list, among those before its first entry.
const PROTECTED_FLAG: char = 'P';

/// The accounts SDDL writes by an alias rather than by their SID: the built-in
/// Administrator (RID 500, the account the Windows CI runs as) and Guest (RID 501).
const ACCOUNT_ALIASES: [(&str, &str); 2] = [("-500", "LA"), ("-501", "LG")];

/// A folder's access list: whether it is protected, and its entries.
#[derive(Debug)]
pub struct Dacl {
    pub protected: bool,
    pub entries: BTreeSet<String>,
}

/// The access list of `path`.
pub fn read(path: &Path) -> Dacl {
    let dir = tempfile::tempdir().expect("dir");
    let saved = dir.path().join("saved");
    let status = Command::new(icacls_program().expect("icacls"))
        .arg(path)
        .arg("/save")
        .arg(&saved)
        .arg("/q")
        .status()
        .expect("icacls ran");
    assert!(status.success(), "icacls /save failed: {status}");
    let bytes = std::fs::read(&saved).expect("saved");
    let units: Vec<u16> = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u16::from_le_bytes(*pair))
        .collect();
    let text = String::from_utf16(&units).expect("UTF-16");
    let sddl = text
        .lines()
        .find_map(|line| line.trim().strip_prefix(DACL_START))
        .expect("an access list");
    let (flags, entries) = sddl.split_once('(').unwrap_or((sddl, ""));
    Dacl {
        protected: flags.contains(PROTECTED_FLAG),
        entries: entries
            .trim_end_matches(')')
            .split(")(")
            .filter(|entry| !entry.is_empty())
            .map(str::to_owned)
            .collect(),
    }
}

/// The three entries a restricted folder has, each with `flags`: full control to the
/// current user, the Administrators and SYSTEM.
pub fn restricted(flags: &str) -> BTreeSet<String> {
    let user = sddl_account(&current_user_sid().expect("user"));
    [user.as_str(), "BA", "SY"]
        .into_iter()
        .map(|account| format!("A;{flags};FA;;;{account}"))
        .collect()
}

/// How SDDL writes the account `sid`: its alias when it has one, else the SID itself.
fn sddl_account(sid: &str) -> String {
    ACCOUNT_ALIASES
        .iter()
        .find(|(rid, _)| sid.ends_with(rid))
        .map_or_else(|| sid.to_owned(), |(_, alias)| (*alias).to_owned())
}
