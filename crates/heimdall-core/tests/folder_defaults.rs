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

//! What a folder gives the sessions put in it, as the C# `GroupDefaultsDto` does: nearest
//! folder wins per field, set fields are never touched, and the table follows the folders.

use std::path::{Path, PathBuf};

use heimdall_core::folder_defaults::{FolderDefaults, InheritedField};
use heimdall_core::metadata::{Environment, ProfileMetadata};
use heimdall_core::paths::PROFILES_FILE_NAME;
use heimdall_core::profile::{ProfileId, SshGateway, SshProfile};
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
        local_tunnel_port: None,
        vault_entry: None,
        forwards: heimdall_core::profile::Forwards::default(),
        post_connect: heimdall_core::post_connect::PostConnect::default(),
        forward_agent: false,
        compression: false,
        sftp: false,
        legacy_algorithms: false,
        session_logging: None,
        ssh_mode: heimdall_core::profile::SshMode::Embedded,
        x11_forwarding: false,
    }
}

fn gateway(id: &str) -> SshGateway {
    SshGateway {
        id: ProfileId::new(id),
        name: id.to_uppercase(),
        host: format!("{id}.lab"),
        port: 22,
        username: None,
        key_path: None,
        parent: None,
    }
}

fn profile<'a>(store: &'a ProfileStore, id: &str) -> &'a SshProfile {
    store
        .ssh_profiles()
        .iter()
        .find(|p| p.id.as_str() == id)
        .expect("kept")
}

/// A store with `PROD` and `PROD/Linux` that set defaults, and the gateways `edge` and
/// `inner`.
fn store(dir: &Path) -> ProfileStore {
    let mut store = ProfileStore::open(dir.join(PROFILES_FILE_NAME)).expect("opens");
    store.merge_gateways([gateway("edge"), gateway("inner")]);
    store.merge([
        ssh("seed-a", Some("PROD/Linux")),
        ssh("seed-b", Some("PROD")),
    ]);
    store
        .set_folder_defaults(
            "PROD",
            FolderDefaults {
                gateway: Some(ProfileId::new("edge")),
                username: Some("ops".to_owned()),
                key_path: Some(PathBuf::from("/keys/prod")),
                port: Some(2200),
                environment: Some(Environment::Production),
            },
        )
        .expect("folder exists");
    store
        .set_folder_defaults(
            "PROD/Linux",
            FolderDefaults {
                username: Some("root".to_owned()),
                ..FolderDefaults::default()
            },
        )
        .expect("folder exists");
    store
}

#[test]
fn the_nearest_folder_wins_per_field_and_says_where_each_value_comes_from() {
    let dir = tempfile::tempdir().expect("dir");
    let store = store(dir.path());
    let resolved = store.resolve_folder_defaults("PROD/Linux/Web");
    let username = resolved.username.expect("a user");
    assert_eq!(
        (username.value.as_str(), username.folder.as_str()),
        ("root", "PROD/Linux")
    );
    let port = resolved.port.expect("a port");
    assert_eq!((port.value, port.folder.as_str()), (2200, "PROD"));
    assert_eq!(resolved.gateway.expect("a gateway").folder, "PROD");
    assert!(store.resolve_folder_defaults("Other").is_empty());
}

#[test]
fn a_session_moved_into_a_folder_gets_only_what_it_leaves_unset() {
    let dir = tempfile::tempdir().expect("dir");
    let mut store = store(dir.path());
    let mut own = ssh("own", None);
    own.username = Some("me".to_owned());
    store.merge([own, ssh("bare", None)]);

    let filled = store
        .move_into_folder(&ProfileId::new("own"), Some("PROD/Linux".to_owned()))
        .expect("a profile");
    assert!(!filled.contains(&InheritedField::Username), "{filled:?}");
    assert!(filled.contains(&InheritedField::KeyPath));
    let own = profile(&store, "own");
    assert_eq!(own.username.as_deref(), Some("me"));
    assert_eq!(own.key_path.as_deref(), Some(Path::new("/keys/prod")));
    assert_eq!(own.gateway, Some(ProfileId::new("edge")));
    assert_eq!(own.port, 22, "the port is never replaced on a move");

    store.move_into_folder(&ProfileId::new("bare"), Some("PROD/Linux".to_owned()));
    assert_eq!(profile(&store, "bare").username.as_deref(), Some("root"));
    assert_eq!(
        store
            .metadata(&ProfileId::new("bare"))
            .and_then(|m| m.environment),
        Some(Environment::Production)
    );
}

#[test]
fn a_session_already_in_the_folder_or_with_a_gateway_of_its_own_is_left_alone() {
    let dir = tempfile::tempdir().expect("dir");
    let mut store = store(dir.path());
    let mut routed = ssh("routed", Some("PROD"));
    routed.gateway = Some(ProfileId::new("inner"));
    store.merge([routed, ssh("there", Some("PROD"))]);
    store.set_metadata(
        &ProfileId::new("routed"),
        ProfileMetadata {
            environment: Some(Environment::Lab),
            ..ProfileMetadata::default()
        },
    );
    // Not moved: same folder.
    let none = store
        .move_into_folder(&ProfileId::new("there"), Some("PROD".to_owned()))
        .expect("a profile");
    assert!(none.is_empty());
    assert_eq!(profile(&store, "there").username, None);

    store.move_into_folder(&ProfileId::new("routed"), Some("PROD/Linux".to_owned()));
    let routed = profile(&store, "routed");
    assert_eq!(routed.gateway, Some(ProfileId::new("inner")));
    assert_eq!(routed.username.as_deref(), Some("root"));
    assert_eq!(
        store
            .metadata(&ProfileId::new("routed"))
            .and_then(|m| m.environment),
        Some(Environment::Lab)
    );
}

#[test]
fn a_session_the_store_does_not_hold_gets_nothing() {
    // A shared session lives outside the store: it never inherits.
    let dir = tempfile::tempdir().expect("dir");
    let mut store = store(dir.path());
    assert_eq!(
        store.move_into_folder(&ProfileId::new("shared-1"), Some("PROD".to_owned())),
        None
    );
    assert!(
        store
            .apply_folder_defaults(&ProfileId::new("shared-1"))
            .is_empty()
    );
}

#[test]
fn the_table_follows_a_folder_renamed_moved_and_deleted() {
    let dir = tempfile::tempdir().expect("dir");
    let mut store = store(dir.path());
    store.rename_folder("PROD", "LIVE").expect("renames");
    assert!(store.own_folder_defaults("PROD").is_none());
    assert_eq!(
        store
            .own_folder_defaults("LIVE")
            .and_then(|d| d.username.as_deref()),
        Some("ops")
    );
    assert_eq!(
        store
            .own_folder_defaults("LIVE/Linux")
            .and_then(|d| d.username.as_deref()),
        Some("root")
    );

    store.add_folder("", "Top").expect("adds");
    store.move_folder("LIVE", "Top").expect("moves");
    assert!(store.own_folder_defaults("Top/LIVE/Linux").is_some());
    assert!(store.own_folder_defaults("LIVE").is_none());

    store.delete_folder("Top/LIVE");
    assert_eq!(store.folder_defaults().count(), 0);
}

#[test]
fn deleting_a_gateway_clears_the_folders_that_name_it_and_drops_the_emptied_ones() {
    let dir = tempfile::tempdir().expect("dir");
    let mut store = store(dir.path());
    store
        .set_folder_defaults(
            "PROD/Linux",
            FolderDefaults {
                gateway: Some(ProfileId::new("edge")),
                ..FolderDefaults::default()
            },
        )
        .expect("folder exists");
    assert_eq!(
        store.folders_using_gateway(&ProfileId::new("edge")),
        ["PROD".to_owned(), "PROD/Linux".to_owned()]
    );
    let removed = store
        .remove_gateway(&ProfileId::new("edge"))
        .expect("a gateway");
    assert_eq!(removed.folders, 2);
    assert!(store.own_folder_defaults("PROD/Linux").is_none());
    let prod = store
        .own_folder_defaults("PROD")
        .expect("still sets a user");
    assert_eq!(
        (prod.gateway.as_ref(), prod.username.as_deref()),
        (None, Some("ops"))
    );
}

#[test]
fn the_table_is_saved_and_a_file_without_it_still_opens() {
    let dir = tempfile::tempdir().expect("dir");
    let store = store(dir.path());
    store.save().expect("saves");
    let reopened = ProfileStore::open(store.path()).expect("reopens");
    assert_eq!(
        reopened.own_folder_defaults("PROD"),
        store.own_folder_defaults("PROD")
    );

    let old = dir.path().join("old.toml");
    std::fs::write(
        &old,
        "version = 14\n\n[[ssh]]\nid = \"a\"\nname = \"a\"\nhost = \"h\"\nport = 22\ngroup = \"PROD\"\n",
    )
    .expect("writes");
    let old = ProfileStore::open(&old).expect("a version 14 file opens");
    assert_eq!(old.ssh_profiles().len(), 1);
    assert_eq!(old.folder_defaults().count(), 0);
}

#[test]
fn a_folder_that_does_not_exist_takes_no_defaults() {
    let dir = tempfile::tempdir().expect("dir");
    let mut store = store(dir.path());
    assert!(
        store
            .set_folder_defaults(
                "Nowhere",
                FolderDefaults {
                    username: Some("x".to_owned()),
                    ..FolderDefaults::default()
                }
            )
            .is_err()
    );
}
