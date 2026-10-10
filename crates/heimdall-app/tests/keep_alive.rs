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

//! Keeping SSH sessions up, as the C# Heimdall's settings: the keep-alive interval given to
//! every connection, and the `TMOUT` reset of idle SSH shells, never of a shell being typed
//! into or of a local one.

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use heimdall_app::local_driver::LocalShell;
use heimdall_app::{
    App, AppConfig, AttemptId, ConnectionEvent, Effect, InputSink, KeyInput, Message,
    SettingsMessage, TabId,
};
use heimdall_core::profile::{ProfileId, SshProfile};
use heimdall_core::store::ProfileStore;
use heimdall_ssh::{AgentSource, SessionClosed, TerminalSize};
use heimdall_term::local::LocalArguments;
use heimdall_term::{GridSize, Key, KeyLocation, Modifiers};

/// Records what the application sends to a session.
#[derive(Debug, Default)]
struct RecordingSink {
    written: Mutex<Vec<u8>>,
}

impl RecordingSink {
    /// What was written since the last call.
    fn take(&self) -> Vec<u8> {
        std::mem::take(&mut *self.written.lock().expect("written"))
    }
}

impl InputSink for RecordingSink {
    fn write(&self, bytes: Vec<u8>) -> Result<(), SessionClosed> {
        self.written.lock().expect("written").extend(bytes);
        Ok(())
    }
    fn resize(&self, _size: TerminalSize) -> Result<(), SessionClosed> {
        Ok(())
    }
    fn close(&self) {}
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

/// A connected shell of "web", and what reaches its session.
fn connected(app: &mut App) -> (TabId, Arc<RecordingSink>) {
    let effects = app.update(Message::OpenProfile(ProfileId::new("web")));
    let [Effect::Connect { tab, attempt, .. }] = effects.as_slice() else {
        panic!("{effects:?}");
    };
    let (tab, attempt): (TabId, AttemptId) = (*tab, *attempt);
    let sink = Arc::new(RecordingSink::default());
    app.update(Message::Connection {
        tab,
        attempt,
        event: ConnectionEvent::Connected {
            input: Arc::clone(&sink) as Arc<dyn InputSink>,
        },
    });
    (tab, sink)
}

#[test]
fn every_ssh_connection_is_given_the_keep_alive_interval_of_the_settings() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let interval = |app: &mut App| {
        let effects = app.update(Message::OpenProfile(ProfileId::new("web")));
        let [Effect::Connect { request, .. }] = effects.as_slice() else {
            panic!("{effects:?}");
        };
        request.options.keepalive_interval
    };
    assert_eq!(
        interval(&mut app),
        Duration::from_secs(30),
        "the C# default"
    );
    app.update(Message::Settings(SettingsMessage::SshKeepAliveInterval(45)));
    assert_eq!(interval(&mut app), Duration::from_secs(45));
    // Out of the C# range: refused, the setting kept.
    app.update(Message::Settings(SettingsMessage::SshKeepAliveInterval(4)));
    assert_eq!(interval(&mut app), Duration::from_secs(45));
}

#[test]
fn an_idle_ssh_shell_gets_enter_and_one_being_typed_into_does_not() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    assert_eq!(app.tmout_reset_interval(), None, "no shell open");
    let (tab, sink) = connected(&mut app);
    assert_eq!(
        app.tmout_reset_interval(),
        Some(Duration::from_secs(240)),
        "the C# default"
    );

    // Nothing typed yet: idle.
    app.update(Message::TmoutResetTick);
    assert_eq!(sink.take(), b"\r");

    // Typed into within the interval: the Enter would land in the user's line.
    app.update(Message::Key {
        tab,
        input: KeyInput {
            key: Key::Character('l'),
            text: Some("l".to_owned()),
            physical_digit: None,
            location: KeyLocation::Standard,
            modifiers: Modifiers::default(),
        },
    });
    assert_eq!(sink.take(), b"l");
    app.update(Message::TmoutResetTick);
    assert!(sink.take().is_empty(), "the user's own input reset TMOUT");

    // Turned off: nothing, and no timer.
    app.update(Message::Settings(SettingsMessage::SshTmoutResetInterval(0)));
    assert_eq!(app.tmout_reset_interval(), None);
    app.update(Message::TmoutResetTick);
    assert!(sink.take().is_empty());
}

#[test]
fn a_local_shell_has_no_remote_tmout_and_gets_no_enter() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let effects = app.update(Message::OpenLocal(LocalShell {
        name: "shell".to_owned(),
        program: None,
        arguments: LocalArguments::List(Vec::new()),
        working_directory: None,
        environment: Vec::new(),
    }));
    let [Effect::ConnectLocal { tab, attempt, .. }] = effects.as_slice() else {
        panic!("{effects:?}");
    };
    let sink = Arc::new(RecordingSink::default());
    app.update(Message::Connection {
        tab: *tab,
        attempt: *attempt,
        event: ConnectionEvent::Connected {
            input: Arc::clone(&sink) as Arc<dyn InputSink>,
        },
    });
    assert_eq!(app.tmout_reset_interval(), None);
    app.update(Message::TmoutResetTick);
    assert!(sink.take().is_empty());
}
