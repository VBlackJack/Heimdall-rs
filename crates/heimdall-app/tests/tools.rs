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

//! The Tools area of the core: the registry, a tool opened in a tab as the C#
//! `OpenToolTabAsync`, the tools pinned and used lately as the C# `FavoriteToolSet` and
//! `RecentToolList`, and the sidebar's tab and folded groups kept across runs.

use std::path::Path;

use heimdall_app::tools::{
    MAX_RECENT_TOOLS, RecentTools, SidebarTab, ToolCategory, ToolGroup, ToolId, toggle_favorite,
};
use heimdall_app::{App, AppConfig, Message, Phase, TabProfile, ToolsMessage};
use heimdall_ssh::AgentSource;
use heimdall_term::GridSize;

fn app(dir: &Path) -> App {
    App::new(AppConfig {
        profiles_file: dir.join("profiles.toml"),
        known_hosts: dir.join("known_hosts"),
        legacy_dir: None,
        agent: AgentSource::Disabled,
        initial_grid: GridSize { cols: 80, rows: 24 },
        files_start: dir.to_owned(),
        system_credentials: heimdall_app::SystemCredentials::memory(),
    })
}

fn open(app: &mut App, tool: ToolId) {
    let _ = app.update(Message::Tools(ToolsMessage::Open {
        tool,
        title: tool.code().to_owned(),
    }));
}

#[test]
fn the_registry_holds_the_tools_ported_with_their_csharp_entries() {
    assert_eq!(
        ToolId::ALL,
        [ToolId::Base64, ToolId::UrlEncoder, ToolId::Uuid]
    );
    assert_eq!(ToolId::Base64.code(), "BASE64");
    assert_eq!(ToolId::UrlEncoder.prefixes(), ["url", "urlencode"]);
    assert_eq!(ToolId::Uuid.prefixes(), ["uuid", "guid"]);
    assert_eq!(ToolId::Base64.category(), ToolCategory::Encoding);
    assert_eq!(ToolId::Uuid.category(), ToolCategory::System);
    assert!(ToolId::ALL.iter().all(|tool| !tool.is_network()));
    // Looked up as the C# registry looks up, case aside and its prefix taken off.
    assert_eq!(ToolId::from_code("urlenc"), Some(ToolId::UrlEncoder));
    assert_eq!(ToolId::from_code("TOOL:uuid"), Some(ToolId::Uuid));
    assert_eq!(ToolId::from_code("PING"), None, "not ported");
}

#[test]
fn pinning_follows_the_csharp_favorite_tool_set() {
    let none: Vec<String> = Vec::new();
    let pinned = toggle_favorite(&none, ToolId::Uuid);
    assert!(pinned.added);
    assert_eq!(pinned.favorites, ["UUID"]);
    // Membership case aside; every spelling taken off; the others' order kept.
    let held = vec![
        "JSON".to_owned(),
        "uuid".to_owned(),
        "PING".to_owned(),
        "Uuid".to_owned(),
    ];
    let unpinned = toggle_favorite(&held, ToolId::Uuid);
    assert!(!unpinned.added);
    assert_eq!(unpinned.favorites, ["JSON", "PING"]);
    assert_eq!(held.len(), 4, "the caller's list is not changed");
}

#[test]
fn the_tools_used_lately_are_five_at_most_the_newest_first() {
    let mut recent = RecentTools::default();
    recent.track(ToolId::Base64);
    recent.track(ToolId::Uuid);
    recent.track(ToolId::Base64);
    assert_eq!(recent.ids(), [ToolId::Base64, ToolId::Uuid]);
    for _ in 0..MAX_RECENT_TOOLS * 2 {
        recent.track(ToolId::UrlEncoder);
    }
    assert!(recent.ids().len() <= MAX_RECENT_TOOLS);
    assert_eq!(recent.ids()[0], ToolId::UrlEncoder);
}

#[test]
fn a_tool_opens_in_a_tab_of_its_own_once_ready_and_is_shown_again_when_open() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    open(&mut app, ToolId::Uuid);
    assert_eq!(app.tabs.len(), 1);
    let tab = &app.tabs[0];
    assert!(matches!(tab.profile, TabProfile::Tool(ToolId::Uuid)));
    assert_eq!(tab.tool(), Some(ToolId::Uuid));
    assert_eq!(tab.phase, Phase::Connected);
    assert_eq!(tab.display_title(), "UUID");
    assert!(!tab.is_live(), "no session is lost when it closes");
    let uuid = tab.id;
    assert_eq!(app.recent_tools(), [ToolId::Uuid]);
    open(&mut app, ToolId::Base64);
    assert_eq!(app.tabs.len(), 2);
    // As the C# singleton tools: shown again, no second tab.
    open(&mut app, ToolId::Uuid);
    assert_eq!(app.tabs.len(), 2);
    assert_eq!(app.active, Some(uuid));
    assert_eq!(app.recent_tools(), [ToolId::Uuid, ToolId::Base64]);
}

#[test]
fn a_tool_tab_closes_without_asking_is_neither_detached_nor_split_nor_reopened() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    open(&mut app, ToolId::Base64);
    open(&mut app, ToolId::Uuid);
    let (base64, uuid) = (app.tabs[0].id, app.tabs[1].id);
    assert!(!app.can_detach(&app.tabs[0]));
    assert!(app.merge_candidates(base64).is_empty());
    assert!(!app.can_reopen(&app.tabs[0]));
    assert!(!app.can_restart(&app.tabs[0]));
    let _ = app.update(Message::RequestCloseTab(uuid));
    assert!(app.dialog.is_none(), "{:?}", app.dialog);
    assert_eq!(app.tabs.len(), 1);
}

#[test]
fn pins_the_sidebar_tab_and_folded_groups_are_kept_across_runs() {
    let dir = tempfile::tempdir().expect("dir");
    let mut first = app(dir.path());
    assert_eq!(first.sidebar_tab(), SidebarTab::Sessions);
    assert!(first.tool_group_open(ToolGroup::Favorites));
    let _ = first.update(Message::Tools(ToolsMessage::ToggleFavorite(ToolId::Uuid)));
    let _ = first.update(Message::Tools(ToolsMessage::ToggleFavorite(ToolId::Base64)));
    let _ = first.update(Message::Tools(ToolsMessage::ToggleSidebarTab));
    let _ = first.update(Message::Tools(ToolsMessage::ToggleGroup(
        ToolGroup::Category(ToolCategory::Encoding),
    )));
    assert_eq!(first.favorite_tools(), [ToolId::Uuid, ToolId::Base64]);
    assert!(first.is_favorite_tool(ToolId::Uuid));
    open(&mut first, ToolId::Uuid);
    drop(first);

    let mut second = app(dir.path());
    assert_eq!(second.favorite_tools(), [ToolId::Uuid, ToolId::Base64]);
    assert_eq!(second.sidebar_tab(), SidebarTab::Tools);
    assert!(!second.tool_group_open(ToolGroup::Category(ToolCategory::Encoding)));
    assert!(second.tool_group_open(ToolGroup::Category(ToolCategory::System)));
    assert!(
        second.recent_tools().is_empty(),
        "the C# keeps them for the run only"
    );
    let _ = second.update(Message::Tools(ToolsMessage::ShowSidebarTab(
        SidebarTab::Sessions,
    )));
    let _ = second.update(Message::Tools(ToolsMessage::ToggleFavorite(ToolId::Uuid)));
    assert_eq!(second.sidebar_tab(), SidebarTab::Sessions);
    assert_eq!(second.favorite_tools(), [ToolId::Base64]);
    assert!(second.tabs.is_empty(), "a tool is not restored");
}

#[test]
fn a_network_tool_would_inherit_the_selected_session_host() {
    use heimdall_core::profile::{ProfileId, SshProfile};
    let dir = tempfile::tempdir().expect("dir");
    let mut store =
        heimdall_core::store::ProfileStore::open(dir.path().join("profiles.toml")).expect("store");
    store.merge([SshProfile {
        id: ProfileId::new("web"),
        name: "web".to_owned(),
        group: None,
        host: "  web.lab ".to_owned(),
        port: 22,
        username: None,
        key_path: None,
        gateway: None,
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
    }]);
    store.save().expect("save");
    let mut app = app(dir.path());
    assert_eq!(app.tool_target_host(), None, "none selected");
    app.selected_profile = Some(ProfileId::new("web"));
    assert_eq!(app.tool_target_host().as_deref(), Some("web.lab"));
}
