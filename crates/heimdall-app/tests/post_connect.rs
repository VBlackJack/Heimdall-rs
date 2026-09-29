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

//! A shell's post-connect steps in the application: asked about until approved, approved as
//! shown, counted on the tab while they run, stopped from it.

use std::path::Path;

use heimdall_app::{
    App, AppConfig, ConnectionEvent, Dialog, Effect, Message, PostConnectProgress, Purpose,
    StepStatus, TabId,
};
use heimdall_core::post_connect::{PostConnect, PostConnectStep};
use heimdall_core::profile::{ProfileId, SshProfile};
use heimdall_core::store::ProfileStore;
use heimdall_ssh::AgentSource;
use heimdall_term::GridSize;
use tokio_util::sync::CancellationToken;

const GRID: GridSize = GridSize { cols: 80, rows: 24 };

fn steps() -> Vec<PostConnectStep> {
    vec![
        PostConnectStep::new("sudo -i"),
        PostConnectStep::new("cd /srv"),
    ]
}

fn profile(id: &str, post_connect: PostConnect) -> SshProfile {
    SshProfile {
        id: ProfileId::new(id),
        name: format!("server {id}"),
        group: None,
        host: format!("{id}.lab"),
        port: 22,
        username: Some("admin".to_owned()),
        key_path: None,
        gateway: None,
        vault_entry: None,
        forwards: heimdall_core::profile::Forwards::default(),
        post_connect,
    }
}

/// `imported` has steps nobody approved; `approved` has them approved; `plain` has none.
fn config(dir: &Path) -> AppConfig {
    let profiles_file = dir.join("profiles.toml");
    let mut store = ProfileStore::open(&profiles_file).expect("store");
    store.merge([
        profile(
            "imported",
            PostConnect {
                steps: steps(),
                approved: None,
            },
        ),
        profile("approved", PostConnect::default()),
        profile("plain", PostConnect::default()),
    ]);
    // The store takes no approval from what it merges: given in its own right.
    store.merge([profile(
        "approved",
        PostConnect {
            steps: steps(),
            approved: None,
        },
    )]);
    assert!(store.approve_post_connect(&ProfileId::new("approved"), &steps()));
    store.save().expect("save");
    AppConfig {
        profiles_file,
        known_hosts: dir.join("known_hosts"),
        legacy_dir: None,
        agent: AgentSource::Disabled,
        initial_grid: GRID,
        files_start: dir.to_owned(),
        system_credentials: heimdall_app::SystemCredentials::memory(),
    }
}

fn connect_request(effects: &[Effect]) -> &heimdall_app::ConnectRequest {
    match effects {
        [Effect::Connect { request, .. }] => request,
        other => panic!("expected one Connect, got {other:?}"),
    }
}

#[test]
fn steps_not_approved_are_shown_and_asked_about_before_the_shell_opens() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = App::new(config(dir.path()));
    let effects = app.update(Message::OpenProfile(ProfileId::new("imported")));
    assert!(effects.is_empty(), "nothing opens yet: {effects:?}");
    let Some(Dialog::ConfirmPostConnect(confirmation)) = &app.dialog else {
        panic!("{:?}", app.dialog);
    };
    assert_eq!(confirmation.name, "server imported");
    assert_eq!(confirmation.commands, ["sudo -i", "cd /srv"]);
    assert!(app.tabs.is_empty());
}

#[test]
fn run_and_remember_approves_the_steps_shown_and_types_them() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = App::new(config(dir.path()));
    app.update(Message::OpenProfile(ProfileId::new("imported")));
    let effects = app.update(Message::ConfirmDialog);
    let request = connect_request(&effects);
    assert_eq!(request.profile.post_connect.to_run(), steps());
    // Remembered: saved, and the next shell opens without asking.
    let saved = ProfileStore::open(dir.path().join("profiles.toml")).expect("store");
    let imported = saved
        .ssh_profiles()
        .iter()
        .find(|profile| profile.id.as_str() == "imported")
        .expect("saved");
    assert!(imported.post_connect.is_approved());
    let effects = app.update(Message::OpenProfile(ProfileId::new("imported")));
    assert_eq!(
        connect_request(&effects).profile.post_connect.to_run(),
        steps()
    );
}

#[test]
fn connect_without_them_opens_the_shell_typing_nothing_and_asks_again_next_time() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = App::new(config(dir.path()));
    app.update(Message::OpenProfile(ProfileId::new("imported")));
    let effects = app.update(Message::SkipPostConnect);
    assert!(
        connect_request(&effects)
            .profile
            .post_connect
            .to_run()
            .is_empty()
    );
    assert!(app.dialog.is_none());
    app.update(Message::OpenProfile(ProfileId::new("imported")));
    assert!(matches!(app.dialog, Some(Dialog::ConfirmPostConnect(_))));
}

#[test]
fn approved_steps_and_a_files_tab_open_without_asking() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = App::new(config(dir.path()));
    let effects = app.update(Message::OpenProfile(ProfileId::new("approved")));
    assert_eq!(
        connect_request(&effects).profile.post_connect.to_run(),
        steps()
    );
    let effects = app.update(Message::OpenProfile(ProfileId::new("plain")));
    assert!(
        connect_request(&effects)
            .profile
            .post_connect
            .to_run()
            .is_empty()
    );
    // Files types nothing: no question.
    let effects = app.update(Message::OpenFiles(ProfileId::new("imported")));
    assert_eq!(connect_request(&effects).purpose, Purpose::Files);
    assert!(app.dialog.is_none());
}

#[test]
fn the_tab_counts_the_steps_and_a_click_stops_them() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = App::new(config(dir.path()));
    let effects = app.update(Message::OpenProfile(ProfileId::new("approved")));
    let [Effect::Connect { tab, attempt, .. }] = effects.as_slice() else {
        panic!("{effects:?}");
    };
    let (tab, attempt): (TabId, _) = (*tab, *attempt);
    let stop = CancellationToken::new();
    app.update(Message::Connection {
        tab,
        attempt,
        event: ConnectionEvent::PostConnect(PostConnectProgress {
            step: 1,
            total: 2,
            command: "sudo -i".to_owned(),
            status: StepStatus::Running,
            stop: stop.clone(),
        }),
    });
    let shown = app.tab(tab).and_then(|tab| tab.post_connect.clone());
    assert_eq!(
        shown.map(|progress| (progress.step, progress.total)),
        Some((1, 2))
    );
    app.update(Message::StopPostConnect(tab));
    assert!(stop.is_cancelled());
    app.update(Message::Connection {
        tab,
        attempt,
        event: ConnectionEvent::PostConnectDone,
    });
    assert!(app.tab(tab).expect("tab").post_connect.is_none());
}
