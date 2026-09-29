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

//! Folders in the profile file, as the C# Heimdall keeps them: empty ones kept, a folder
//! renamed or moved with all it holds, deleted with its profiles going to no folder.

use heimdall_core::folder::FolderError;
use heimdall_core::paths::PROFILES_FILE_NAME;
use heimdall_core::profile::{ProfileId, RdpProfile, SshProfile};
use heimdall_core::store::ProfileStore;

fn ssh(id: &str, group: Option<&str>) -> SshProfile {
    SshProfile {
        id: ProfileId::new(id),
        name: id.to_owned(),
        group: group.map(str::to_owned),
        host: "h".to_owned(),
        port: 22,
        username: None,
        key_path: None,
        gateway: None,
        vault_entry: None,
    }
}

fn rdp(id: &str, group: Option<&str>) -> RdpProfile {
    RdpProfile {
        id: ProfileId::new(id),
        name: id.to_owned(),
        group: group.map(str::to_owned),
        host: "h".to_owned(),
        port: 3389,
        username: None,
        domain: None,
        allow_tls_only: false,
        gateway: None,
        redirect_clipboard: true,
        redirect_drives: false,
        options: heimdall_core::profile::RdpOptions::default(),
        vault_entry: None,
    }
}

fn store(dir: &std::path::Path) -> ProfileStore {
    let mut store = ProfileStore::open(dir.join(PROFILES_FILE_NAME)).expect("opens");
    store.merge([
        ssh("web", Some("Prod/Web")),
        ssh("db", Some(" Prod ")),
        ssh("production", Some("Production")),
        ssh("loose", None),
    ]);
    store.merge_rdp([rdp("dc", Some("Prod/Web/Front"))]);
    store
}

fn groups(store: &ProfileStore) -> Vec<(String, Option<String>)> {
    let mut groups: Vec<(String, Option<String>)> = store
        .ssh_profiles()
        .iter()
        .map(|p| (p.id.to_string(), p.group.clone()))
        .chain(
            store
                .rdp_profiles()
                .iter()
                .map(|p| (p.id.to_string(), p.group.clone())),
        )
        .collect();
    groups.sort();
    groups
}

fn group(store: &ProfileStore, id: &str) -> Option<String> {
    groups(store)
        .into_iter()
        .find(|(found, _)| found == id)
        .and_then(|(_, group)| group)
}

#[test]
fn every_folder_on_the_way_is_known_and_an_empty_one_is_kept_across_a_save() {
    let dir = tempfile::tempdir().expect("dir");
    let mut store = store(dir.path());
    assert_eq!(
        store.folder_paths(),
        ["Prod", "Prod/Web", "Prod/Web/Front", "Production"]
    );
    assert_eq!(
        store.add_folder("Prod", " Archive "),
        Ok("Prod/Archive".to_owned())
    );
    assert_eq!(
        store.add_folder("", "prod"),
        Err(FolderError::Collision),
        "whatever the case"
    );
    assert_eq!(
        store.add_folder("Prod", "a/b"),
        Err(FolderError::InvalidName)
    );
    assert_eq!(store.add_folder("Prod", " "), Err(FolderError::InvalidName));
    store.save().expect("saves");
    let text = std::fs::read_to_string(dir.path().join(PROFILES_FILE_NAME)).expect("reads");
    assert!(text.contains("folder = [\"Prod/Archive\"]"), "{text}");
    let reopened = ProfileStore::open(dir.path().join(PROFILES_FILE_NAME)).expect("reopens");
    assert_eq!(reopened.folders(), ["Prod/Archive"]);
    assert!(reopened.folder_paths().contains(&"Prod/Archive".to_owned()));
}

#[test]
fn a_renamed_folder_takes_its_profiles_and_folders_along_and_no_other() {
    let dir = tempfile::tempdir().expect("dir");
    let mut store = store(dir.path());
    store.add_folder("Prod/Web", "Old").expect("folder");
    assert_eq!(store.rename_folder("Prod", " Live "), Ok("Live".to_owned()));
    assert_eq!(group(&store, "web").as_deref(), Some("Live/Web"));
    assert_eq!(group(&store, "db").as_deref(), Some("Live"));
    assert_eq!(
        group(&store, "dc").as_deref(),
        Some("Live/Web/Front"),
        "another protocol too"
    );
    assert_eq!(
        group(&store, "production").as_deref(),
        Some("Production"),
        "not a sub-folder"
    );
    assert_eq!(group(&store, "loose"), None);
    assert_eq!(store.folders(), ["Live/Web/Old"]);

    assert_eq!(
        store.rename_folder("Live", "production"),
        Err(FolderError::Collision)
    );
    assert_eq!(
        store.rename_folder("Live", "LIVE"),
        Ok("LIVE".to_owned()),
        "its own case"
    );
    assert_eq!(
        store.rename_folder("Nowhere", "x"),
        Err(FolderError::Missing)
    );
    assert_eq!(
        store.rename_folder("LIVE", ""),
        Err(FolderError::InvalidName)
    );
}

#[test]
fn a_moved_folder_goes_under_another_or_to_the_top_but_never_into_itself() {
    let dir = tempfile::tempdir().expect("dir");
    let mut store = store(dir.path());
    assert_eq!(
        store.move_folder("Prod/Web", "Production"),
        Ok("Production/Web".to_owned())
    );
    assert_eq!(group(&store, "web").as_deref(), Some("Production/Web"));
    assert_eq!(group(&store, "dc").as_deref(), Some("Production/Web/Front"));
    assert_eq!(
        group(&store, "db").as_deref(),
        Some(" Prod "),
        "untouched, as written"
    );
    assert_eq!(
        store.move_folder("Production/Web", ""),
        Ok("Web".to_owned())
    );
    assert_eq!(group(&store, "web").as_deref(), Some("Web"));
    assert_eq!(
        store.move_folder("Web", "Web/Front"),
        Err(FolderError::IntoItself)
    );
    assert_eq!(
        store.move_folder("Web", "Web"),
        Err(FolderError::IntoItself)
    );
    store.add_folder("Prod", "web").expect("folder");
    assert_eq!(
        store.move_folder("Web", "Prod"),
        Err(FolderError::Collision)
    );
}

#[test]
fn a_deleted_folder_sends_its_profiles_to_no_folder() {
    let dir = tempfile::tempdir().expect("dir");
    let mut store = store(dir.path());
    store.add_folder("Prod/Web", "Old").expect("folder");
    store.add_folder("", "Kept").expect("folder");
    assert_eq!(store.delete_folder("Prod"), 3, "db, web and dc");
    assert_eq!(group(&store, "web"), None);
    assert_eq!(group(&store, "dc"), None);
    assert_eq!(group(&store, "production").as_deref(), Some("Production"));
    assert_eq!(store.folders(), ["Kept"]);
    assert_eq!(store.folder_paths(), ["Kept", "Production"]);
}

#[test]
fn folders_written_by_hand_are_read_the_one_way() {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join(PROFILES_FILE_NAME);
    std::fs::write(
        &path,
        "version = 8\nfolder = [\" A / B \", \"\", \" / \"]\n",
    )
    .expect("writes");
    let store = ProfileStore::open(&path).expect("opens");
    assert_eq!(store.folders(), ["A/B"], "blank ones dropped");
    assert_eq!(store.folder_paths(), ["A", "A/B"]);
}
