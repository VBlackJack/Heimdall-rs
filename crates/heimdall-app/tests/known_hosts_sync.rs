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

//! The user's OpenSSH `known_hosts` imported at startup, as the C# `KnownHostsStartupSync`:
//! only when chosen, once, in the background, from the file under the home folder.

use std::path::Path;

use heimdall_app::{App, AppConfig, Effect, Message, SettingsMessage, SystemCredentials};
use heimdall_ssh::known_hosts_import::HostKeysSynced;
use heimdall_ssh::{AgentSource, HostKeySource, KnownHosts};
use heimdall_term::GridSize;

/// A host key fixture's `type base64` part.
fn key(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../heimdall-ssh/tests/fixtures/hostkeys")
        .join(format!("{name}.pub"));
    let text = std::fs::read_to_string(path).expect("fixture");
    let mut fields = text.split_whitespace();
    format!(
        "{} {}",
        fields.next().expect("type"),
        fields.next().expect("key")
    )
}

fn app(dir: &Path) -> App {
    App::new(AppConfig {
        profiles_file: dir.join("profiles.toml"),
        known_hosts: dir.join("known_hosts"),
        legacy_dir: None,
        agent: AgentSource::Disabled,
        initial_grid: GridSize { cols: 80, rows: 24 },
        files_start: dir.to_owned(),
        system_credentials: SystemCredentials::memory(),
    })
}

#[test]
fn off_by_default_nothing_runs_and_turned_on_it_waits_for_the_next_start() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    assert!(!app.settings().sync_known_hosts_at_startup);
    assert!(app.sync_known_hosts_at_startup().is_empty(), "off: nothing");

    // Turned on in Settings: nothing runs in this run, as the C# reads it at start.
    let effects = app.update(Message::Settings(SettingsMessage::SyncKnownHostsAtStartup(
        true,
    )));
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, Effect::SyncKnownHosts { .. })),
        "{effects:?}"
    );
    assert!(app.sync_known_hosts_at_startup().is_empty(), "once a run");

    // The next start, the setting saved on: once, in the background.
    let mut again = self::app(dir.path());
    assert!(again.settings().sync_known_hosts_at_startup);
    let effects = again.sync_known_hosts_at_startup();
    let [Effect::SyncKnownHosts { source, store }] = effects.as_slice() else {
        panic!("one import: {effects:?}");
    };
    assert_eq!(store, &dir.path().join("known_hosts"), "into its own store");
    assert!(
        source.ends_with(Path::new(".ssh").join("known_hosts")),
        "the user's OpenSSH file: {}",
        source.display()
    );
    assert_eq!(
        Some(source.clone()),
        heimdall_core::paths::openssh_known_hosts(),
        "under the home folder the platform says"
    );
    assert!(
        again.sync_known_hosts_at_startup().is_empty(),
        "never twice"
    );
}

#[test]
fn a_run_learns_new_servers_and_leaves_what_contradicts_the_trust_given() {
    let dir = tempfile::tempdir().expect("dir");
    let store = KnownHosts::new(dir.path().join("config").join("known_hosts"));
    store
        .learn(
            "known.lab",
            22,
            &heimdall_ssh::PublicKey::from_openssh(&key("host-ed25519")).expect("key"),
        )
        .expect("learn");
    let source = dir.path().join("known_hosts");
    std::fs::write(
        &source,
        format!(
            "\u{FEFF}new.lab {}\nnew.lab {}\nknown.lab {}\nknown.lab {}\n|1|c2FsdA==|aGFzaA== {}\n",
            key("host-ed25519"),
            key("host-ecdsa"),
            key("host-ed25519"),
            key("host-ecdsa"),
            key("host-ed25519"),
        ),
    )
    .expect("written");

    let done = heimdall_app::known_hosts_sync::run(&source, &store).expect("ran");
    assert_eq!((done.imported, done.matched), (2, 1));
    assert_eq!(done.conflicts.len(), 1, "the ECDSA key of known.lab");
    assert_eq!(done.conflicts[0].host, "known.lab");
    assert_eq!(store.recorded("new.lab", 22).expect("read").len(), 2);
    assert_eq!(store.recorded("known.lab", 22).expect("read").len(), 1);
    let listed = store.entries().expect("listed");
    assert!(
        listed
            .iter()
            .filter(|entry| entry.host == "new.lab")
            .all(|entry| entry.details.source == HostKeySource::Imported)
    );

    // Run again: every key matches, none is added.
    let again = heimdall_app::known_hosts_sync::run(&source, &store).expect("ran");
    assert_eq!((again.imported, again.matched), (0, 3));
}

#[test]
fn a_missing_file_or_a_folder_in_its_place_imports_nothing() {
    let dir = tempfile::tempdir().expect("dir");
    let store = KnownHosts::new(dir.path().join("config").join("known_hosts"));
    let missing = heimdall_app::known_hosts_sync::run(&dir.path().join("absent"), &store);
    assert_eq!(missing.expect("ran"), HostKeysSynced::default());
    let folder = heimdall_app::known_hosts_sync::run(dir.path(), &store);
    assert_eq!(folder.expect("ran"), HostKeysSynced::default());
    assert!(!store.path().exists(), "nothing written");
}
