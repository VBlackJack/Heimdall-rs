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

//! The profile tree as the C# Heimdall draws it: nested folders, opened and closed, sub-
//! folders before profiles, "(No Folder)" last; a search opens what it finds.

use std::path::Path;

use heimdall_app::{App, AppConfig, Message, NO_FOLDER, TreeRow};
use heimdall_core::profile::{ProfileId, SshProfile};
use heimdall_core::store::ProfileStore;
use heimdall_ssh::AgentSource;
use heimdall_term::GridSize;

fn profile(id: &str, name: &str, group: Option<&str>) -> SshProfile {
    SshProfile {
        id: ProfileId::new(id),
        name: name.to_owned(),
        group: group.map(str::to_owned),
        host: format!("{id}.lab"),
        port: 22,
        username: None,
        key_path: None,
        gateway: None,
    }
}

fn app(dir: &Path) -> App {
    let profiles_file = dir.join("profiles.toml");
    let mut store = ProfileStore::open(&profiles_file).expect("store");
    store.merge([
        profile("web", "web", Some(" Prod / Web ")),
        profile("api", "Api", Some("Prod/Web")),
        profile("db", "db", Some("Prod")),
        profile("dev", "dev", Some("dev")),
        profile("loose", "loose", None),
        profile("empty", "empty", Some("")),
    ]);
    store.save().expect("save");
    App::new(AppConfig {
        profiles_file,
        known_hosts: dir.join("known_hosts"),
        legacy_dir: None,
        agent: AgentSource::Disabled,
        initial_grid: GridSize { cols: 80, rows: 24 },
        files_start: dir.to_owned(),
        system_credentials: heimdall_app::SystemCredentials::memory(),
    })
}

/// The rows, written short: `+name@depth` for an open folder, `-name@depth` a closed one,
/// `name@depth` a profile.
fn outline(rows: &[TreeRow]) -> Vec<String> {
    rows.iter()
        .map(|row| match row {
            TreeRow::Folder {
                path,
                name,
                depth,
                open,
            } => {
                let name = if path == NO_FOLDER { "(none)" } else { name };
                format!("{}{name}@{depth}", if *open { '+' } else { '-' })
            }
            TreeRow::Profile { profile, depth } => format!("{}@{depth}", profile.id),
        })
        .collect()
}

#[test]
fn folders_nest_by_their_path_sub_folders_first_and_no_folder_last() {
    let dir = tempfile::tempdir().expect("dir");
    let app = app(dir.path());
    assert_eq!(
        outline(&app.tree_rows("")),
        [
            "+dev@0",
            "dev@1", // By name, whatever the case: "dev" before "Prod".
            "+Prod@0",
            "+Web@1",
            "api@2",
            "web@2",
            "db@1",
            "+(none)@0",
            "empty@1",
            "loose@1",
        ]
    );
}

#[test]
fn a_folder_closes_and_opens_again_and_keeps_its_sub_folders_state() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    app.update(Message::ToggleFolder("Prod/Web".to_owned()));
    assert_eq!(
        outline(&app.tree_rows("")),
        [
            "+dev@0",
            "dev@1",
            "+Prod@0",
            "-Web@1",
            "db@1",
            "+(none)@0",
            "empty@1",
            "loose@1",
        ]
    );
    app.update(Message::ToggleFolder(" Prod ".to_owned()));
    assert_eq!(
        outline(&app.tree_rows("")),
        [
            "+dev@0",
            "dev@1",
            "-Prod@0",
            "+(none)@0",
            "empty@1",
            "loose@1"
        ],
        "a path is read whatever its spaces"
    );
    app.update(Message::ToggleFolder("Prod".to_owned()));
    assert!(
        outline(&app.tree_rows("")).contains(&"-Web@1".to_owned()),
        "Web stays closed"
    );
    app.update(Message::ToggleFolder(NO_FOLDER.to_owned()));
    assert_eq!(
        outline(&app.tree_rows("")).last(),
        Some(&"-(none)@0".to_owned())
    );
}

#[test]
fn a_search_shows_what_it_finds_every_folder_on_the_way_open() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    app.update(Message::ToggleFolder("Prod".to_owned()));
    app.update(Message::ToggleFolder("Prod/Web".to_owned()));
    assert_eq!(
        outline(&app.tree_rows("api")),
        ["+Prod@0", "+Web@1", "api@2"]
    );
    assert_eq!(outline(&app.tree_rows("loose")), ["+(none)@0", "loose@1"]);
    assert!(app.tree_rows("nothing like it").is_empty());
    // Emptied, the folders are as they were.
    assert_eq!(
        outline(&app.tree_rows(" ")).get(2),
        Some(&"-Prod@0".to_owned())
    );
}
