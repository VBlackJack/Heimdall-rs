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
use std::path::PathBuf;

use heimdall_core::paths::PROFILES_FILE_NAME;
use heimdall_core::profile::{ProfileId, RdpProfile, SshProfile, TelnetProfile};
use heimdall_core::store::{MergeReport, PROFILE_FILE_VERSION, ProfileStore, StoreError};

fn profile(id: &str, host: &str) -> SshProfile {
    SshProfile {
        id: ProfileId::new(id),
        name: id.to_uppercase(),
        group: None,
        host: host.to_owned(),
        port: 22,
        username: Some("admin".to_owned()),
        key_path: Some(PathBuf::from("/keys/admin")),
    }
}

#[test]
fn a_missing_file_is_an_empty_store() {
    let dir = tempfile::tempdir().expect("temp dir");
    let store = ProfileStore::open(dir.path().join(PROFILES_FILE_NAME)).expect("opens");
    assert!(store.ssh_profiles().is_empty());
}

#[test]
fn saved_profiles_read_back_identically() {
    let dir = tempfile::tempdir().expect("temp dir");
    // A directory that does not exist yet: save must create it.
    let path = dir.path().join("nested").join(PROFILES_FILE_NAME);
    let mut store = ProfileStore::open(&path).expect("opens");
    store.merge([profile("a", "h1"), profile("b", "h2")]);
    store.save().expect("saves");

    let reopened = ProfileStore::open(&path).expect("reopens");
    assert_eq!(reopened.ssh_profiles(), store.ssh_profiles());
}

#[test]
fn save_leaves_no_temporary_file_behind() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join(PROFILES_FILE_NAME);
    let mut store = ProfileStore::open(&path).expect("opens");
    store.merge([profile("a", "h1")]);
    store.save().expect("saves once");
    store.save().expect("saves over the existing file");
    let names: Vec<_> = fs::read_dir(dir.path())
        .expect("readable")
        .map(|entry| entry.expect("entry").file_name())
        .collect();
    assert_eq!(names, vec![PROFILES_FILE_NAME]);
}

#[test]
fn merging_twice_updates_instead_of_duplicating() {
    let dir = tempfile::tempdir().expect("temp dir");
    let mut store = ProfileStore::open(dir.path().join(PROFILES_FILE_NAME)).expect("opens");

    let first = store.merge([profile("a", "h1"), profile("b", "h2")]);
    assert_eq!(
        first,
        MergeReport {
            added: 2,
            updated: 0,
            unchanged: 0
        }
    );

    let second = store.merge([
        profile("a", "h1"),
        profile("b", "changed"),
        profile("c", "h3"),
    ]);
    assert_eq!(
        second,
        MergeReport {
            added: 1,
            updated: 1,
            unchanged: 1
        }
    );

    let hosts: Vec<_> = store
        .ssh_profiles()
        .iter()
        .map(|p| p.host.as_str())
        .collect();
    assert_eq!(hosts, vec!["h1", "changed", "h3"]);
}

#[test]
fn a_file_of_another_version_is_refused() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join(PROFILES_FILE_NAME);
    fs::write(&path, format!("version = {}\n", PROFILE_FILE_VERSION + 1)).expect("writes");
    assert!(matches!(
        ProfileStore::open(&path),
        Err(StoreError::UnsupportedVersion { .. })
    ));
}

#[test]
fn a_corrupt_file_is_an_error_not_an_empty_store() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join(PROFILES_FILE_NAME);
    fs::write(&path, "version = [").expect("writes");
    assert!(matches!(
        ProfileStore::open(&path),
        Err(StoreError::Parse { .. })
    ));
}

fn rdp(id: &str) -> RdpProfile {
    RdpProfile {
        id: ProfileId::new(id),
        name: id.to_uppercase(),
        group: Some("Windows".to_owned()),
        host: "dc.lab".to_owned(),
        port: 3389,
        username: Some("admin".to_owned()),
        domain: Some("LAB".to_owned()),
        allow_tls_only: false,
    }
}

#[test]
fn a_version_1_file_still_opens_and_is_saved_as_the_current_version() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join(PROFILES_FILE_NAME);
    fs::write(
        &path,
        "version = 1\n\n[[ssh]]\nid = \"a\"\nname = \"A\"\nhost = \"h\"\nport = 22\n",
    )
    .expect("writes");
    let mut store = ProfileStore::open(&path).expect("a version 1 file opens");
    assert_eq!(store.ssh_profiles().len(), 1);
    assert!(store.rdp_profiles().is_empty());
    store.merge_rdp([rdp("r")]);
    store.save().expect("saves");
    let text = fs::read_to_string(&path).expect("reads");
    assert!(
        text.starts_with(&format!("version = {PROFILE_FILE_VERSION}\n")),
        "{text}"
    );
    assert_eq!(PROFILE_FILE_VERSION, 3);
}

#[test]
fn rdp_profiles_read_back_and_are_removed_like_the_others() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join(PROFILES_FILE_NAME);
    let mut store = ProfileStore::open(&path).expect("opens");
    store.merge([profile("s", "h")]);
    let report = store.merge_rdp([rdp("r")]);
    assert_eq!(report.added, 1);
    store.save().expect("saves");
    let mut reopened = ProfileStore::open(&path).expect("reopens");
    assert_eq!(reopened.rdp_profiles(), store.rdp_profiles());
    assert!(reopened.remove(&ProfileId::new("r")));
    assert!(reopened.rdp_profiles().is_empty());
    assert_eq!(reopened.ssh_profiles().len(), 1, "the SSH profile stays");
    assert!(!reopened.remove(&ProfileId::new("r")), "already gone");
}

fn telnet(id: &str) -> TelnetProfile {
    TelnetProfile {
        id: ProfileId::new(id),
        name: id.to_uppercase(),
        group: Some("Network".to_owned()),
        host: "switch.lab".to_owned(),
        port: 23,
    }
}

#[test]
fn a_version_2_file_with_rdp_profiles_still_opens() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join(PROFILES_FILE_NAME);
    fs::write(
        &path,
        "version = 2\n\n[[rdp]]\nid = \"r\"\nname = \"R\"\nhost = \"h\"\nport = 3389\n",
    )
    .expect("writes");
    let store = ProfileStore::open(&path).expect("a version 2 file opens");
    assert_eq!(store.rdp_profiles().len(), 1);
    assert!(store.telnet_profiles().is_empty());
}

#[test]
fn telnet_profiles_read_back_and_are_removed_like_the_others() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join(PROFILES_FILE_NAME);
    let mut store = ProfileStore::open(&path).expect("opens");
    store.merge([profile("s", "h")]);
    store.merge_rdp([rdp("r")]);
    let report = store.merge_telnet([telnet("t")]);
    assert_eq!(report.added, 1);
    assert_eq!(store.merge_telnet([telnet("t")]).unchanged, 1);
    store.save().expect("saves");
    let text = fs::read_to_string(&path).expect("reads");
    assert!(text.contains("[[telnet]]"), "{text}");
    let mut reopened = ProfileStore::open(&path).expect("reopens");
    assert_eq!(reopened.telnet_profiles(), store.telnet_profiles());
    assert!(reopened.remove(&ProfileId::new("t")));
    assert!(reopened.telnet_profiles().is_empty());
    assert_eq!(reopened.ssh_profiles().len(), 1, "the SSH profile stays");
    assert_eq!(reopened.rdp_profiles().len(), 1, "the RDP profile stays");
    assert!(!reopened.remove(&ProfileId::new("t")), "already gone");
}
