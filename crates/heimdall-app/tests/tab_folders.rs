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

//! The tab strip gathered by folder, as the C# `GroupSessionTabsByFolder`: off until chosen
//! from a tab's menu, the tabs of a folder side by side, pinned tabs first, the choice kept
//! across runs.

use std::path::Path;

use heimdall_app::{
    App, AppConfig, Effect, Message, SettingsMessage, TabId, TabMenuMessage, folder_label,
};
use heimdall_core::profile::{ProfileId, SshProfile};
use heimdall_core::store::ProfileStore;
use heimdall_ssh::AgentSource;
use heimdall_term::GridSize;

fn profile(id: &str, group: Option<&str>) -> SshProfile {
    SshProfile {
        id: ProfileId::new(id),
        name: format!("server {id}"),
        group: group.map(str::to_owned),
        host: format!("{id}.lab"),
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

fn app(dir: &Path) -> App {
    let profiles_file = dir.join("profiles.toml");
    let mut store = ProfileStore::open(&profiles_file).expect("store");
    store.merge([
        profile("a", Some("Lab/Web")),
        profile("b", Some("Prod")),
        profile("c", None),
    ]);
    store.save().expect("save");
    let mut app = App::new(AppConfig {
        profiles_file,
        known_hosts: dir.join("known_hosts"),
        legacy_dir: None,
        agent: AgentSource::Disabled,
        initial_grid: GridSize { cols: 80, rows: 24 },
        files_start: dir.to_owned(),
        system_credentials: heimdall_app::SystemCredentials::memory(),
    });
    app.update(Message::Settings(SettingsMessage::SftpBrowser(
        heimdall_core::settings::SftpBrowser {
            auto_open_on_ssh: false,
            ..heimdall_core::settings::SftpBrowser::default()
        },
    )));
    app
}

fn open(app: &mut App, id: &str) -> TabId {
    match app
        .update(Message::OpenProfile(ProfileId::new(id)))
        .as_slice()
    {
        [Effect::Connect { tab, .. }] => *tab,
        other => panic!("expected one Connect, got {other:?}"),
    }
}

fn strip(app: &App) -> Vec<TabId> {
    app.strip().iter().map(|tab| tab.id).collect()
}

fn group_by_folder(app: &mut App) {
    app.update(Message::TabMenu(TabMenuMessage::ToggleGroupByFolder));
}

#[test]
fn the_strip_keeps_the_order_of_opening_until_the_tabs_are_gathered_by_folder() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (a1, b1, a2, c1) = (
        open(&mut app, "a"),
        open(&mut app, "b"),
        open(&mut app, "a"),
        open(&mut app, "c"),
    );
    assert!(!app.groups_tabs_by_folder(), "off, as the C# default");
    assert_eq!(strip(&app), [a1, b1, a2, c1]);
    assert!(app.tab_folder(app.tab(a1).expect("tab")).is_none());

    group_by_folder(&mut app);
    assert!(app.groups_tabs_by_folder());
    // The second tab of "Lab/Web" joins the first; the tab in no folder keeps its place.
    assert_eq!(strip(&app), [a1, a2, b1, c1]);
    let folder = app.tab_folder(app.tab(a2).expect("tab"));
    assert_eq!(folder.as_deref(), Some("Lab/Web"));
    assert_eq!(folder_label("Lab/Web"), "Web");
    assert!(app.tab_folder(app.tab(c1).expect("tab")).is_none());

    group_by_folder(&mut app);
    assert_eq!(
        strip(&app),
        [a1, b1, a2, c1],
        "back in the order of opening"
    );
}

#[test]
fn a_pinned_tab_leads_the_strip_and_belongs_to_no_folder() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (a1, b1, a2) = (
        open(&mut app, "a"),
        open(&mut app, "b"),
        open(&mut app, "a"),
    );
    group_by_folder(&mut app);
    app.update(Message::TabMenu(TabMenuMessage::Pin(a2)));
    assert_eq!(strip(&app), [a2, a1, b1]);
    assert!(app.tab_folder(app.tab(a2).expect("tab")).is_none());
    assert!(app.tab_folder(app.tab(a1).expect("tab")).is_some());
}

#[test]
fn the_choice_is_kept_across_runs_as_the_csharp_setting() {
    let dir = tempfile::tempdir().expect("dir");
    let mut first = app(dir.path());
    group_by_folder(&mut first);
    drop(first);
    let settings = heimdall_core::settings::settings_path(&dir.path().join("profiles.toml"));
    let text = std::fs::read_to_string(settings).expect("saved");
    assert!(text.contains("group_tabs_by_folder = true"), "{text}");
    assert!(
        app(dir.path()).groups_tabs_by_folder(),
        "on, as it was left"
    );
}
