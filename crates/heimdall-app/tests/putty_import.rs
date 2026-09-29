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

//! "Import `PuTTY` sessions", as the C# Heimdall's: `PuTTY`'s store read, the SSH sessions
//! previewed with the OpenSSH preview, a session without a host listed invalid.

use std::path::Path;

use heimdall_app::{
    App, AppConfig, Dialog, Effect, Message, SessionsCounts, SessionsMessage, SessionsSource,
};
use heimdall_core::import::openssh::Status;
use heimdall_core::import::putty::{RawSession, Value};
use heimdall_core::store::ProfileStore;
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

fn session(name: &str, host: Option<&str>) -> RawSession {
    let mut values = vec![("Protocol".to_owned(), Value::Text("ssh".to_owned()))];
    if let Some(host) = host {
        values.push(("HostName".to_owned(), Value::Text(host.to_owned())));
    }
    values.push(("PortNumber".to_owned(), Value::Number(2222)));
    RawSession::new(name.to_owned(), values)
}

fn read(app: &mut App, sessions: Vec<RawSession>) {
    app.update(Message::Sessions(SessionsMessage::PuttyRead(Ok(sessions))));
}

#[test]
fn the_menu_entry_reads_the_store() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    assert!(matches!(
        app.update(Message::Sessions(SessionsMessage::Putty))
            .as_slice(),
        [Effect::ReadPuttySessions]
    ));
}

#[test]
fn a_session_without_a_host_is_listed_invalid_and_cannot_be_chosen() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    read(
        &mut app,
        vec![session("web", Some("web.lab")), session("broken", None)],
    );
    let Some(Dialog::SessionsPreview(preview)) = &app.dialog else {
        panic!("{:?}", app.dialog);
    };
    assert_eq!(preview.source, SessionsSource::Putty);
    let rows: Vec<(Status, bool)> = preview
        .rows
        .iter()
        .map(|row| (row.assessment.status, row.chosen))
        .collect();
    assert_eq!(rows, [(Status::New, true), (Status::Invalid, false)]);
    assert_eq!(preview.counts(), (2, 1, 0, 1));
    assert!(preview.all_chosen(), "every row that can be chosen is");
    assert_eq!(preview.putty_diagnostics.len(), 1, "the missing host said");

    app.update(Message::Sessions(SessionsMessage::Choose(1)));
    app.update(Message::Sessions(SessionsMessage::ChooseAll(true)));
    let Some(Dialog::SessionsPreview(preview)) = &app.dialog else {
        panic!();
    };
    assert!(!preview.rows[1].chosen, "never chosen");

    app.update(Message::ConfirmDialog);
    assert_eq!(
        app.dialog,
        Some(Dialog::SessionsDone {
            source: SessionsSource::Putty,
            counts: SessionsCounts {
                imported: 1,
                ..SessionsCounts::default()
            },
        })
    );
    let store = ProfileStore::open(dir.path().join("profiles.toml")).expect("saved");
    assert_eq!(store.ssh_profiles().len(), 1);
    assert_eq!(
        (
            store.ssh_profiles()[0].host.as_str(),
            store.ssh_profiles()[0].port
        ),
        ("web.lab", 2222)
    );
}

#[test]
fn no_session_or_an_unreadable_store_is_said_as_putty() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    read(&mut app, Vec::new());
    assert_eq!(
        app.dialog,
        Some(Dialog::SessionsEmpty {
            source: SessionsSource::Putty
        })
    );
    app.update(Message::Sessions(SessionsMessage::PuttyRead(Err(
        "denied".to_owned()
    ))));
    assert_eq!(
        app.dialog,
        Some(Dialog::SessionsUnreadable {
            source: SessionsSource::Putty,
            detail: "denied".to_owned()
        })
    );
}

#[test]
fn the_sessions_are_never_shown_in_a_log() {
    let message = Message::Sessions(SessionsMessage::PuttyRead(Ok(vec![session(
        "secret-server",
        Some("h"),
    )])));
    assert!(!format!("{message:?}").contains("secret-server"));
}

#[test]
fn session_files_are_read_from_their_folder_and_no_folder_is_no_session() {
    let dir = tempfile::tempdir().expect("dir");
    let folder = dir.path().join("sessions");
    assert_eq!(
        heimdall_app::putty_store::read_folder(&folder),
        Ok(Vec::new())
    );
    std::fs::create_dir(&folder).expect("folder");
    std::fs::write(folder.join("lab%20web"), "HostName=web.lab\nProtocol=ssh\n").expect("file");
    std::fs::create_dir(folder.join("not-a-session")).expect("sub-folder");
    let sessions = heimdall_app::putty_store::read_folder(&folder).expect("read");
    assert_eq!(sessions.len(), 1, "a folder is not a session");
    assert_eq!(sessions[0].encoded_name, "lab%20web");
    assert_eq!(
        sessions[0].values.get("hostname"),
        Some(&Value::Text("web.lab".to_owned()))
    );
}
