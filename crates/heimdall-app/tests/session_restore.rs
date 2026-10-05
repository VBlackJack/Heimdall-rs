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

//! The previous run's sessions offered again at the next start, as the C# session snapshot.

use std::path::Path;

use heimdall_app::{App, AppConfig, Dialog, Effect, Message, Purpose, QuickResult};
use heimdall_core::profile::{ProfileId, SshProfile};
use heimdall_core::session_snapshot::{load, snapshot_path};
use heimdall_core::store::ProfileStore;
use heimdall_ssh::AgentSource;
use heimdall_term::GridSize;

fn ssh(id: &str) -> SshProfile {
    SshProfile {
        id: ProfileId::new(id),
        name: format!("server {id}"),
        group: None,
        host: format!("{id}.lab"),
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
    }
}

fn config(dir: &Path) -> AppConfig {
    AppConfig {
        profiles_file: dir.join("profiles.toml"),
        known_hosts: dir.join("known_hosts"),
        legacy_dir: None,
        agent: AgentSource::Disabled,
        initial_grid: GridSize { cols: 80, rows: 24 },
        files_start: dir.to_owned(),
        system_credentials: heimdall_app::SystemCredentials::memory(),
    }
}

fn app(dir: &Path) -> App {
    let mut store = ProfileStore::open(dir.join("profiles.toml")).expect("store");
    store.merge(["web", "db"].map(ssh));
    store.save().expect("save");
    App::new(config(dir))
}

/// What the effects of a restore open: each profile, and whether for its files.
fn opened(effects: &[Effect]) -> Vec<(String, Purpose)> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::Connect { request, .. } => {
                Some((request.profile.id.to_string(), request.purpose))
            }
            _ => None,
        })
        .collect()
}

/// A run with web's shell, db's files and a session typed in Quick Connect, then closed.
fn previous_run(dir: &Path) {
    let mut app = app(dir);
    app.update(Message::OpenProfile(ProfileId::new("web")));
    app.update(Message::OpenFiles(ProfileId::new("db")));
    app.update(Message::QuickConnect(QuickResult::Ssh {
        username: None,
        host: "typed.lab".to_owned(),
        port: 22,
    }));
    // Nothing connected yet: closing asks nothing.
    let effects = app.update(Message::WindowCloseRequested);
    assert!(
        matches!(effects.as_slice(), [Effect::Exit]),
        "{effects:?} {:?}",
        app.dialog
    );
}

#[test]
fn the_sessions_of_saved_profiles_are_offered_again_and_those_ticked_reopen() {
    let dir = tempfile::tempdir().expect("dir");
    previous_run(dir.path());
    let path = snapshot_path(&dir.path().join("profiles.toml"));
    assert_eq!(
        load(&path).expect("kept").sessions.len(),
        2,
        "the one saved nowhere is not kept"
    );

    let mut app = App::new(config(dir.path()));
    assert!(app.dialog.is_none(), "offered once the window asks");
    app.offer_restore();
    let Some(Dialog::RestoreSessions(dialog)) = &app.dialog else {
        panic!("{:?}", app.dialog);
    };
    assert_eq!(dialog.rows.len(), 2);
    assert!(dialog.rows.iter().all(|row| row.chosen), "every one ticked");
    assert!(dialog.rows[1].files);

    // Untick web: only db's files reopen, and the snapshot goes.
    app.update(Message::RestoreChoose {
        index: Some(0),
        chosen: false,
    });
    let effects = app.update(Message::ConfirmDialog);
    assert_eq!(opened(&effects), [("db".to_owned(), Purpose::Files)]);
    assert!(load(&path).is_none(), "answered: gone");
    app.offer_restore();
    assert!(app.dialog.is_none(), "offered once");
}

#[test]
fn dont_restore_drops_the_snapshot_and_a_profile_deleted_meanwhile_is_missing() {
    let dir = tempfile::tempdir().expect("dir");
    previous_run(dir.path());
    // web deleted before the next start.
    let mut store = ProfileStore::open(dir.path().join("profiles.toml")).expect("store");
    store.remove(&ProfileId::new("web"));
    store.save().expect("save");

    let mut app = App::new(config(dir.path()));
    app.offer_restore();
    let Some(Dialog::RestoreSessions(dialog)) = &app.dialog else {
        panic!("{:?}", app.dialog);
    };
    assert!(dialog.rows[0].found.is_none(), "missing");
    assert!(!dialog.rows[0].chosen, "and not to be ticked");
    app.update(Message::RestoreChoose {
        index: None,
        chosen: true,
    });
    let Some(Dialog::RestoreSessions(dialog)) = &app.dialog else {
        panic!("{:?}", app.dialog);
    };
    assert!(!dialog.rows[0].chosen, "Select all leaves it");

    assert!(app.update(Message::DismissDialog).is_empty());
    assert!(
        load(&snapshot_path(&dir.path().join("profiles.toml"))).is_none(),
        "Don't restore: gone"
    );
}
