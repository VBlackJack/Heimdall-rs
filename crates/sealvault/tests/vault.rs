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

//! A vault on disk: what goes in comes out with the password, and nothing else opens it.

use std::fs;
use std::path::{Path, PathBuf};

use sealvault::{Vault, VaultError, backup_path};

const PASSWORD: &[u8] = b"correct horse battery staple";

/// Where each field starts in the file, as the format lays it out.
const FIELDS: [(&str, usize); 11] = [
    ("magic", 0),
    ("version", 4),
    ("derivation", 6),
    ("memory", 7),
    ("iterations", 11),
    ("lanes", 15),
    ("salt", 19),
    ("wrapping method", 35),
    ("wrapping nonce", 36),
    ("wrapped key", 48),
    ("body nonce", 96),
];

/// Where the sealed body starts.
const BODY: usize = 108;

fn filled(dir: &Path) -> PathBuf {
    let path = dir.join("secrets.svlt");
    let mut vault = Vault::create(&path, PASSWORD).expect("created");
    vault.set("ssh/web", b"hunter2".to_vec());
    vault.set("rdp/dc", b"P@ss w0rd".to_vec());
    vault.save().expect("saved");
    path
}

fn unreadable(path: &Path, password: &[u8]) -> bool {
    matches!(Vault::open(path, password), Err(VaultError::Unreadable))
}

#[test]
fn what_is_saved_opens_with_the_password() {
    let dir = tempfile::tempdir().expect("dir");
    let path = filled(dir.path());
    let mut vault = Vault::open(&path, PASSWORD).expect("opens");
    assert_eq!(vault.get("ssh/web"), Some(&b"hunter2"[..]));
    assert_eq!(vault.get("rdp/dc"), Some(&b"P@ss w0rd"[..]));
    assert_eq!(vault.get("nothing"), None);
    assert_eq!(vault.names().collect::<Vec<_>>(), ["rdp/dc", "ssh/web"]);
    assert!(vault.remove("rdp/dc"));
    assert!(!vault.remove("rdp/dc"));
    vault.save().expect("saved");
    let vault = Vault::open(&path, PASSWORD).expect("opens again");
    assert_eq!(vault.names().collect::<Vec<_>>(), ["ssh/web"]);
}

#[test]
fn the_secrets_are_not_in_the_file() {
    let dir = tempfile::tempdir().expect("dir");
    let path = filled(dir.path());
    let bytes = fs::read(&path).expect("read");
    for plain in [&b"hunter2"[..], b"ssh/web", b"rdp/dc"] {
        assert!(
            !bytes.windows(plain.len()).any(|window| window == plain),
            "{}",
            String::from_utf8_lossy(plain)
        );
    }
}

#[test]
fn a_wrong_password_is_unreadable() {
    let dir = tempfile::tempdir().expect("dir");
    let path = filled(dir.path());
    assert!(unreadable(&path, b"correct horse battery stapl"));
    assert!(unreadable(&path, b""));
}

#[test]
fn a_change_to_any_field_or_to_the_body_is_unreadable() {
    let dir = tempfile::tempdir().expect("dir");
    let path = filled(dir.path());
    let good = fs::read(&path).expect("read");
    let last = good.len() - 1;
    let places = FIELDS
        .iter()
        .copied()
        .chain([("body", BODY), ("tag", last)]);
    for (field, offset) in places {
        let mut changed = good.clone();
        changed[offset] ^= 1;
        fs::write(&path, &changed).expect("written");
        assert!(unreadable(&path, PASSWORD), "{field} at {offset}");
    }
    fs::write(&path, &good).expect("restored");
    assert!(
        Vault::open(&path, PASSWORD).is_ok(),
        "the unchanged file opens"
    );
}

#[test]
fn a_file_cut_short_or_not_a_vault_is_unreadable() {
    let dir = tempfile::tempdir().expect("dir");
    let path = filled(dir.path());
    let good = fs::read(&path).expect("read");
    for length in [0, 4, BODY - 1, BODY, BODY + 15, good.len() - 1] {
        fs::write(&path, &good[..length]).expect("written");
        assert!(unreadable(&path, PASSWORD), "cut to {length}");
    }
    fs::write(&path, b"version = 5\n").expect("written");
    assert!(unreadable(&path, PASSWORD));
}

#[test]
fn every_failure_reads_the_same() {
    let dir = tempfile::tempdir().expect("dir");
    let path = filled(dir.path());
    let wrong = Vault::open(&path, b"nope").expect_err("wrong").to_string();
    let good = fs::read(&path).expect("read");
    fs::write(&path, &good[..good.len() - 1]).expect("cut");
    let cut = Vault::open(&path, PASSWORD).expect_err("cut").to_string();
    assert_eq!(wrong, cut);
}

#[test]
fn creating_over_a_file_is_refused() {
    let dir = tempfile::tempdir().expect("dir");
    let path = filled(dir.path());
    assert!(matches!(
        Vault::create(&path, b"other"),
        Err(VaultError::AlreadyExists(_))
    ));
    assert!(Vault::open(&path, PASSWORD).is_ok(), "left as it was");
}

#[test]
fn each_save_seals_under_a_new_nonce_and_each_vault_has_its_own_salt() {
    let dir = tempfile::tempdir().expect("dir");
    let path = filled(dir.path());
    let vault = Vault::open(&path, PASSWORD).expect("opens");
    let first = fs::read(&path).expect("read");
    vault.save().expect("saved again");
    let second = fs::read(&path).expect("read");
    assert_ne!(first[96..BODY], second[96..BODY], "body nonce");
    assert_ne!(first[BODY..], second[BODY..], "sealed body");

    let other = dir.path().join("other.svlt");
    Vault::create(&other, PASSWORD).expect("created");
    let other = fs::read(&other).expect("read");
    assert_ne!(first[19..35], other[19..35], "salt");
    assert_ne!(first[48..96], other[48..96], "wrapped key");
}

#[test]
fn a_new_password_opens_it_and_the_old_one_no_longer_does() {
    let dir = tempfile::tempdir().expect("dir");
    let path = filled(dir.path());
    let before = fs::read(&path).expect("read");
    let mut vault = Vault::open(&path, PASSWORD).expect("opens");
    vault.change_password(b"a new one").expect("changed");
    let after = fs::read(&path).expect("read");
    assert_ne!(before[19..35], after[19..35], "a new salt");
    assert!(unreadable(&path, PASSWORD));
    let vault = Vault::open(&path, b"a new one").expect("opens with the new one");
    assert_eq!(vault.get("ssh/web"), Some(&b"hunter2"[..]));
}

#[test]
fn an_old_body_grafted_under_a_new_header_is_unreadable() {
    let dir = tempfile::tempdir().expect("dir");
    let path = filled(dir.path());
    let old = fs::read(&path).expect("read");
    let mut vault = Vault::open(&path, PASSWORD).expect("opens");
    vault.set("ssh/web", b"rotated".to_vec());
    vault.change_password(b"a new one").expect("changed");
    let new = fs::read(&path).expect("read");
    // The new header with the old body and the nonce it was sealed under: the same data key
    // seals both, so only the header's place in the body's seal tells them apart.
    let grafted = [&new[..96], &old[96..BODY], &old[BODY..]].concat();
    fs::write(&path, &grafted).expect("written");
    assert!(unreadable(&path, b"a new one"));
}

#[test]
fn the_previous_file_is_kept_beside_it() {
    let dir = tempfile::tempdir().expect("dir");
    let path = filled(dir.path());
    let before = fs::read(&path).expect("read");
    let mut vault = Vault::open(&path, PASSWORD).expect("opens");
    vault.set("new", b"x".to_vec());
    vault.save().expect("saved");
    assert_eq!(fs::read(backup_path(&path)).expect("backup"), before);
    assert!(
        Vault::open(backup_path(&path), PASSWORD)
            .expect("the backup opens")
            .get("new")
            .is_none()
    );
}

#[test]
fn debug_shows_no_name_and_no_secret() {
    let dir = tempfile::tempdir().expect("dir");
    let path = filled(dir.path());
    let vault = Vault::open(&path, PASSWORD).expect("opens");
    let shown = format!("{vault:?}");
    for hidden in ["hunter2", "ssh/web", "rdp/dc"] {
        assert!(!shown.contains(hidden), "{shown}");
    }
    assert!(shown.contains("entries: 2"), "{shown}");
}
