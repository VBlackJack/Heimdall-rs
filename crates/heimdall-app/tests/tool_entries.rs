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

//! A tool saved as a tree entry, as the C# "Add > Tool": the picker writes it, the tree shows
//! it, and opening it opens the tool's tab on the host and port it holds.

use std::path::Path;

use heimdall_app::tools::{ToolContext, ToolId};
use heimdall_app::{
    App, AppConfig, Dialog, Message, Notice, ProfileKind, ToolEntryEdit, ToolsMessage,
};
use heimdall_core::profile::{ProfileId, ToolProfile};
use heimdall_core::store::ProfileStore;
use heimdall_ssh::AgentSource;
use heimdall_term::GridSize;

fn entry(id: &str, tool: &str, host: Option<&str>, port: Option<u16>) -> ToolProfile {
    ToolProfile {
        id: ProfileId::new(id),
        name: format!("Entry {id}"),
        group: Some("Lab".to_owned()),
        tool: tool.to_owned(),
        host: host.map(str::to_owned),
        port,
    }
}

fn app_with(dir: &Path, entries: impl IntoIterator<Item = ToolProfile>) -> App {
    let file = dir.join("profiles.toml");
    let mut store = ProfileStore::open(&file).expect("store");
    store.merge_tools(entries);
    store.save().expect("saves");
    App::new(AppConfig {
        profiles_file: file,
        known_hosts: dir.join("known_hosts"),
        legacy_dir: None,
        agent: AgentSource::Disabled,
        initial_grid: GridSize { cols: 80, rows: 24 },
        files_start: dir.to_owned(),
        system_credentials: heimdall_app::SystemCredentials::memory(),
    })
}

#[test]
fn a_tool_entry_is_listed_in_the_tree_as_a_tool_with_its_host() {
    let dir = tempfile::tempdir().expect("dir");
    let app = app_with(
        dir.path(),
        [
            entry("ping", "PING", Some("core.lab"), Some(443)),
            entry("uuid", "UUID", None, None),
        ],
    );
    let summaries = app.profile_summaries();
    let ping = summaries
        .iter()
        .find(|p| p.id.as_str() == "ping")
        .expect("ping");
    assert_eq!(ping.kind, ProfileKind::Tool);
    assert_eq!(ping.endpoint, Some(("core.lab".to_owned(), 443)));
    assert_eq!(ping.group.as_deref(), Some("Lab"));
    let uuid = summaries
        .iter()
        .find(|p| p.id.as_str() == "uuid")
        .expect("uuid");
    assert_eq!(uuid.endpoint, None);
}

#[test]
fn opening_a_tool_entry_opens_the_tool_tab_on_its_saved_host_and_port() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app_with(
        dir.path(),
        [entry("ping", "PING", Some(" core.lab "), Some(443))],
    );
    let _ = app.update(Message::Tools(ToolsMessage::OpenEntry(ProfileId::new(
        "ping",
    ))));
    assert_eq!(app.tabs.len(), 1);
    assert_eq!(app.tabs[0].tool(), Some(ToolId::Ping));
    assert_eq!(app.tabs[0].title, "Entry ping");
    let context = app.tabs[0].tool_context();
    assert_eq!(context.host.as_deref(), Some("core.lab"));
    assert_eq!(context.port, Some(443));
}

#[test]
fn double_clicking_a_tool_entry_goes_through_the_same_open() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app_with(dir.path(), [entry("ping", "ping", Some("core.lab"), None)]);
    let _ = app.update(Message::ConnectProfile(ProfileId::new("ping")));
    assert_eq!(
        app.tabs.len(),
        1,
        "a tool entry opens a tab, nothing connects"
    );
    assert_eq!(
        app.tabs[0].tool(),
        Some(ToolId::Ping),
        "the code's case does not matter"
    );
}

#[test]
fn a_tool_that_reaches_no_host_ignores_the_host_and_a_second_open_shows_the_first_tab() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app_with(
        dir.path(),
        [entry("uuid", "UUID", Some("core.lab"), Some(9))],
    );
    let _ = app.update(Message::Tools(ToolsMessage::OpenEntry(ProfileId::new(
        "uuid",
    ))));
    assert_eq!(app.tabs[0].tool(), Some(ToolId::Uuid));
    assert_eq!(app.tabs[0].tool_context(), &ToolContext::default());
    let _ = app.update(Message::Tools(ToolsMessage::OpenEntry(ProfileId::new(
        "uuid",
    ))));
    assert_eq!(app.tabs.len(), 1, "a tool reaching no host is shown again");
}

#[test]
fn an_entry_naming_a_tool_this_build_lacks_opens_nothing_and_says_so() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app_with(dir.path(), [entry("x", "PRIVLAUNCH", None, None)]);
    let _ = app.update(Message::Tools(ToolsMessage::OpenEntry(ProfileId::new("x"))));
    assert!(app.tabs.is_empty());
    assert!(matches!(app.notice(), Some(Notice::UnknownTool(tool)) if tool == "PRIVLAUNCH"));
}

#[test]
fn the_picker_writes_the_entry_in_its_folder_and_needs_a_tool_chosen() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app_with(dir.path(), []);
    let _ = app.update(Message::Tools(ToolsMessage::NewEntry {
        folder: Some("Lab".to_owned()),
    }));
    assert!(matches!(app.dialog, Some(Dialog::AddTool(_))));
    let _ = app.update(Message::ConfirmDialog);
    assert!(
        matches!(app.dialog, Some(Dialog::AddTool(_))),
        "nothing chosen: the picker stays"
    );
    let edit = |edit| Message::Tools(ToolsMessage::EditEntry(edit));
    let _ = app.update(edit(ToolEntryEdit::Pick(ToolId::Ping, "Ping".to_owned())));
    let _ = app.update(edit(ToolEntryEdit::Host(" core.lab ".to_owned())));
    let _ = app.update(Message::ConfirmDialog);
    assert!(app.dialog.is_none());
    let summaries = app.profile_summaries();
    assert_eq!(summaries.len(), 1);
    assert_eq!(summaries[0].kind, ProfileKind::Tool);
    assert_eq!(summaries[0].name, "Ping");
    assert_eq!(summaries[0].group.as_deref(), Some("Lab"));
    assert_eq!(summaries[0].endpoint, Some(("core.lab".to_owned(), 0)));
    // A tool reaching no host drops the host typed before it was chosen.
    let _ = app.update(Message::Tools(ToolsMessage::NewEntry { folder: None }));
    let _ = app.update(edit(ToolEntryEdit::Host("ignored.lab".to_owned())));
    let _ = app.update(edit(ToolEntryEdit::Pick(ToolId::Uuid, "UUID".to_owned())));
    let _ = app.update(Message::ConfirmDialog);
    let uuid = app
        .profile_summaries()
        .into_iter()
        .find(|p| p.name == "UUID")
        .expect("uuid entry");
    assert_eq!(uuid.endpoint, None);
    assert_eq!(uuid.group, None);
}
