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
    App, AppConfig, Effect, Message, SettingsMessage, TabGroup, TabId, TabMenuMessage, folder_label,
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
        remote_os: heimdall_core::profile::RemoteOs::Auto,
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

fn menu(app: &mut App, message: TabMenuMessage) {
    app.update(Message::TabMenu(message));
}

fn shown(app: &App) -> TabId {
    app.shown_tab().expect("a tab is shown").id
}

fn folded(app: &App, tab: TabId) -> bool {
    app.folder_folded(app.tab(tab).expect("tab"))
}

#[test]
fn folding_a_folder_takes_its_tabs_off_the_strip_and_unfolding_brings_them_back() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (a1, b1, a2, c1) = (
        open(&mut app, "a"),
        open(&mut app, "b"),
        open(&mut app, "a"),
        open(&mut app, "c"),
    );
    group_by_folder(&mut app);
    // The tab in no folder is shown: folding Lab/Web moves nothing.
    assert_eq!(shown(&app), c1);
    menu(&mut app, TabMenuMessage::ToggleFolderFold(a1));
    assert!(folded(&app, a1) && folded(&app, a2));
    assert!(!folded(&app, b1) && !folded(&app, c1));
    assert_eq!(
        app.reachable_tabs(),
        [b1, c1],
        "a key skips the folded folder"
    );
    assert_eq!(
        strip(&app),
        [a1, a2, b1, c1],
        "the tab list still shows every tab"
    );
    assert_eq!(shown(&app), c1);

    menu(&mut app, TabMenuMessage::ToggleFolderFold(a2));
    assert!(!folded(&app, a1));
    assert_eq!(app.reachable_tabs(), [a1, a2, b1, c1]);
}

#[test]
fn folding_the_folder_of_the_shown_tab_shows_the_nearest_tab_outside_it() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (a1, a2, b1, c1) = (
        open(&mut app, "a"),
        open(&mut app, "a"),
        open(&mut app, "b"),
        open(&mut app, "c"),
    );
    group_by_folder(&mut app);
    app.update(Message::SelectTab(a1));
    menu(&mut app, TabMenuMessage::ToggleFolderFold(a2));
    assert_eq!(shown(&app), b1, "the first tab after the folder");
    assert!(folded(&app, a1));

    // Folding the next folder: after it only a tab in no folder, which is taken.
    app.update(Message::SelectTab(b1));
    menu(&mut app, TabMenuMessage::ToggleFolderFold(b1));
    assert_eq!(shown(&app), c1);
    assert!(folded(&app, b1));
}

#[test]
fn the_last_tab_of_the_strip_falls_back_to_the_one_before_its_folder() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (c1, b1) = (open(&mut app, "c"), open(&mut app, "b"));
    group_by_folder(&mut app);
    assert_eq!(shown(&app), b1);
    menu(&mut app, TabMenuMessage::ToggleFolderFold(b1));
    assert_eq!(shown(&app), c1, "nothing after: the one before");
}

#[test]
fn a_folder_holding_every_reachable_tab_stays_open() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (a1, a2) = (open(&mut app, "a"), open(&mut app, "a"));
    group_by_folder(&mut app);
    assert!(!app.can_fold_folder(a1), "the chip says it cannot fold");
    menu(&mut app, TabMenuMessage::ToggleFolderFold(a1));
    assert!(!folded(&app, a1));
    assert_eq!(shown(&app), a2);

    // Another tab, in another folder: now it can.
    let b1 = open(&mut app, "b");
    app.update(Message::SelectTab(a1));
    assert!(app.can_fold_folder(a1));
    menu(&mut app, TabMenuMessage::ToggleFolderFold(a1));
    assert!(folded(&app, a1));
    assert_eq!(shown(&app), b1);
}

#[test]
fn showing_a_tab_of_a_folded_folder_unfolds_it_and_a_new_tab_of_it_arrives_open() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (a1, _b1, c1) = (
        open(&mut app, "a"),
        open(&mut app, "b"),
        open(&mut app, "c"),
    );
    group_by_folder(&mut app);
    menu(&mut app, TabMenuMessage::ToggleFolderFold(a1));
    assert!(folded(&app, a1));
    assert_eq!(shown(&app), c1);
    app.update(Message::SelectTab(a1));
    assert!(!folded(&app, a1));

    app.update(Message::SelectTab(c1));
    menu(&mut app, TabMenuMessage::ToggleFolderFold(a1));
    assert!(folded(&app, a1));
    let a2 = open(&mut app, "a");
    assert!(!folded(&app, a2), "opening a session shows it");
    assert!(!folded(&app, a1));
}

#[test]
fn a_folder_with_no_tab_left_forgets_it_was_folded() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (a1, b1) = (open(&mut app, "a"), open(&mut app, "b"));
    group_by_folder(&mut app);
    app.update(Message::SelectTab(b1));
    menu(&mut app, TabMenuMessage::ToggleFolderFold(a1));
    assert!(folded(&app, a1));
    app.update(Message::RequestCloseTab(a1));
    if app.dialog.is_some() {
        app.update(Message::ConfirmDialog);
    }
    assert!(app.tab(a1).is_none());
    app.update(Message::SelectTab(b1));
    let a2 = open(&mut app, "a");
    app.update(Message::SelectTab(b1));
    assert!(!folded(&app, a2), "the next tab of that folder is open");
}

#[test]
fn collapse_the_other_folders_keeps_this_one_open_and_moves_the_selection_in() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (a1, b1, c1) = (
        open(&mut app, "a"),
        open(&mut app, "b"),
        open(&mut app, "c"),
    );
    group_by_folder(&mut app);
    app.update(Message::SelectTab(b1));
    menu(&mut app, TabMenuMessage::FoldOtherFolders(a1));
    assert_eq!(shown(&app), a1, "the selection moved into the folder");
    assert!(!folded(&app, a1));
    assert!(folded(&app, b1));
    assert!(!folded(&app, c1), "a tab in no folder stays");
    assert_eq!(app.reachable_tabs(), [a1, c1]);

    // This folder folded before is opened again; the selection inside stays put.
    app.update(Message::SelectTab(c1));
    menu(&mut app, TabMenuMessage::ToggleFolderFold(a1));
    assert!(folded(&app, a1));
    menu(&mut app, TabMenuMessage::FoldOtherFolders(a1));
    assert!(!folded(&app, a1));
    assert!(folded(&app, b1));
    assert_eq!(shown(&app), a1);
}

#[test]
fn collapse_the_other_folders_is_offered_only_with_another_folder_on_the_strip() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (a1, c1) = (open(&mut app, "a"), open(&mut app, "c"));
    group_by_folder(&mut app);
    assert!(!app.has_other_folder(a1));
    assert!(!app.has_other_folder(c1), "a tab in no folder has none");
    open(&mut app, "b");
    assert!(app.has_other_folder(a1));
    assert!(!app.has_other_folder(c1));
}

#[test]
fn close_this_folders_tabs_closes_the_folder_group_only() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (a1, b1, a2, c1) = (
        open(&mut app, "a"),
        open(&mut app, "b"),
        open(&mut app, "a"),
        open(&mut app, "c"),
    );
    assert!(
        app.tab_group(a1, TabGroup::Folder).is_empty(),
        "not offered while the tabs are not arranged"
    );
    group_by_folder(&mut app);
    assert_eq!(
        app.tab_group(a2, TabGroup::Folder),
        [a1, a2],
        "this tab too"
    );
    assert!(app.tab_group(c1, TabGroup::Folder).is_empty());
    menu(
        &mut app,
        TabMenuMessage::Close {
            tab: a2,
            group: TabGroup::Folder,
        },
    );
    // Asked once when a session is live, as the other bulk closes are.
    if app.dialog.is_some() {
        app.update(Message::ConfirmDialog);
    }
    assert_eq!(strip(&app), [b1, c1]);
}

#[test]
fn close_others_and_close_to_the_right_spare_a_folded_folder() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (a1, b1, c1) = (
        open(&mut app, "a"),
        open(&mut app, "b"),
        open(&mut app, "c"),
    );
    group_by_folder(&mut app);
    app.update(Message::SelectTab(c1));
    menu(&mut app, TabMenuMessage::ToggleFolderFold(a1));
    assert_eq!(app.tab_group(c1, TabGroup::Others), [b1]);
    assert_eq!(app.tab_group(b1, TabGroup::Right), [c1]);
    assert!(app.tab_group(c1, TabGroup::Right).is_empty());
    assert_eq!(
        app.tab_group(c1, TabGroup::All),
        [a1, b1, c1],
        "close all names every tab, as the C# one"
    );
}
