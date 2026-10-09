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

//! A shell tab's server health panel: shown on the user's word, the server asked over the
//! session's own connection, never keeping it open. The in-process server runs the
//! commands with this computer's `sh`.

#![cfg(unix)]

#[path = "../../heimdall-ssh/tests/common/mod.rs"]
mod ssh;

use std::path::Path;
use std::sync::Arc;

use heimdall_app::server_health::collect;
use heimdall_app::{
    App, AppConfig, AttemptId, ConnectionEvent, Effect, InputSink, Message, TabId, TabMenuMessage,
};
use heimdall_core::profile::{ProfileId, SshProfile};
use heimdall_core::store::ProfileStore;
use heimdall_ssh::{AgentSource, Connection, SessionClosed, TerminalSize, establish};
use heimdall_term::GridSize;
use tokio_util::sync::CancellationToken;

/// A session nobody reads.
#[derive(Debug)]
struct Quiet;

impl InputSink for Quiet {
    fn write(&self, _bytes: Vec<u8>) -> Result<(), SessionClosed> {
        Ok(())
    }
    fn resize(&self, _size: TerminalSize) -> Result<(), SessionClosed> {
        Ok(())
    }
    fn close(&self) {}
}

async fn connection(server: &ssh::TestServer, dir: &Path) -> Connection {
    let options = ssh::options_trusting(dir, server.port, "host-ed25519");
    let prompter = Arc::new(ssh::ScriptedPrompter::passwords(&[ssh::PASSWORD]));
    tokio::time::timeout(
        ssh::STEP_TIMEOUT,
        establish(
            &ssh::profile(server.port, None),
            &options,
            prompter,
            CancellationToken::new(),
        ),
    )
    .await
    .expect("in time")
    .expect("connected")
}

fn app(dir: &Path) -> App {
    let profiles_file = dir.join("profiles.toml");
    let mut store = ProfileStore::open(&profiles_file).expect("store");
    store.merge([SshProfile {
        id: ProfileId::new("web"),
        name: "web".to_owned(),
        group: None,
        host: "web.lab".to_owned(),
        port: 22,
        username: Some("admin".to_owned()),
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
    }]);
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

/// A connected shell of "web", given `connection` as its session's.
fn connected(app: &mut App, connection: &Connection) -> TabId {
    let effects = app.update(Message::OpenProfile(ProfileId::new("web")));
    let [Effect::Connect { tab, attempt, .. }] = effects.as_slice() else {
        panic!("{effects:?}");
    };
    let (tab, attempt): (TabId, AttemptId) = (*tab, *attempt);
    for event in [
        ConnectionEvent::Connected {
            input: Arc::new(Quiet) as Arc<dyn InputSink>,
        },
        ConnectionEvent::SshConnection(connection.downgrade()),
    ] {
        app.update(Message::Connection {
            tab,
            attempt,
            event,
        });
    }
    tab
}

#[tokio::test]
async fn a_linux_server_says_its_cpu_memory_and_disk() {
    let server = ssh::start(ssh::Spec::default()).await;
    let keys = tempfile::tempdir().expect("keys");
    let connection = connection(&server, keys.path()).await;
    let health = tokio::time::timeout(ssh::STEP_TIMEOUT, collect(connection))
        .await
        .expect("in time");
    assert!(health.supported, "{health:?}");
    assert!(health.memory_mb.0 > 0);
    assert!(health.disk.2 <= 100);
    assert!((0.0..=100.0).contains(&health.cpu_percent));
}

#[tokio::test]
async fn the_panel_asks_while_shown_one_question_at_a_time_and_keeps_no_session_open() {
    let server = ssh::start(ssh::Spec::default()).await;
    let keys = tempfile::tempdir().expect("keys");
    let connection = connection(&server, keys.path()).await;
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let tab = connected(&mut app, &connection);
    assert!(!app.polls_health(), "hidden until asked");
    assert!(app.update(Message::HealthTick).is_empty());

    let shown = app.update(Message::TabMenu(TabMenuMessage::ToggleHealth(tab)));
    let [
        Effect::ReadHealth {
            connection: asked, ..
        },
    ] = shown.as_slice()
    else {
        panic!("{shown:?}");
    };
    assert!(app.polls_health());
    assert!(
        app.update(Message::HealthTick).is_empty(),
        "one question at a time"
    );
    let health = tokio::time::timeout(ssh::STEP_TIMEOUT, collect(asked.clone()))
        .await
        .expect("in time");
    app.update(Message::HealthRead {
        tab,
        health: Box::new(health),
    });
    let found = app.tab(tab).expect("tab");
    assert!(
        found
            .health
            .last
            .as_ref()
            .is_some_and(|health| health.supported)
    );
    assert!(matches!(
        app.update(Message::HealthTick).as_slice(),
        [Effect::ReadHealth { .. }]
    ));

    // The session's end: nothing keeps its connection, so nothing is asked any more.
    drop(shown);
    drop(connection);
    app.update(Message::HealthRead {
        tab,
        health: Box::new(heimdall_app::server_health::read("", "", "")),
    });
    assert!(app.update(Message::HealthTick).is_empty());
    assert!(!app.tab(tab).expect("tab").health.available());

    app.update(Message::TabMenu(TabMenuMessage::ToggleHealth(tab)));
    assert!(!app.polls_health(), "hidden");
}
