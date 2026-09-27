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

//! What the application decides in a Files tab, checked through `update` and its effects.

use std::path::Path;

use heimdall_app::files::{
    Direction, EntryKind, FilesError, LocalEntry, RemoteEntry, Side, TransferEvent, TransferState,
};
use heimdall_app::{
    App, AppConfig, AttemptId, ConnectionEvent, Dialog, Effect, FilesMessage, Message, Phase,
    Purpose, TabId,
};
use heimdall_core::profile::{ProfileId, SshProfile};
use heimdall_core::store::ProfileStore;
use heimdall_sftp::protocol::{Request, Response, SFTP_VERSION};
use heimdall_sftp::{ClientConfig, RemotePath, SftpClient};
use heimdall_ssh::AgentSource;
use heimdall_term::GridSize;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

/// A client whose server only answers the version exchange: enough for effects to carry.
async fn idle_client() -> SftpClient {
    let (client_end, mut server) = tokio::io::duplex(4096);
    tokio::spawn(async move {
        let mut length = [0; 4];
        server.read_exact(&mut length).await.expect("init length");
        let mut body = vec![0; u32::from_be_bytes(length) as usize];
        server.read_exact(&mut body).await.expect("init");
        assert!(matches!(Request::decode(&body), Ok(Request::Init { .. })));
        let version = Response::Version {
            version: SFTP_VERSION,
            extensions: Vec::new(),
        };
        server.write_all(&version.encode()).await.expect("version");
        // Keep the pipe open for the life of the test.
        std::future::pending::<()>().await;
    });
    SftpClient::start(client_end, ClientConfig::default())
        .await
        .expect("started")
}

fn app(dir: &Path) -> App {
    let profiles_file = dir.join("profiles.toml");
    let mut store = ProfileStore::open(&profiles_file).expect("store");
    store.merge([SshProfile {
        id: ProfileId::new("a"),
        name: "server a".to_owned(),
        group: None,
        host: "a.lab".to_owned(),
        port: 22,
        username: Some("admin".to_owned()),
        key_path: None,
        gateway: None,
    }]);
    store.save().expect("save");
    App::new(AppConfig {
        profiles_file,
        known_hosts: dir.join("known_hosts"),
        legacy_dir: None,
        agent: AgentSource::Disabled,
        initial_grid: GridSize { cols: 80, rows: 24 },
        files_start: dir.to_owned(),
    })
}

fn files(app: &mut App, message: FilesMessage) -> Vec<Effect> {
    app.update(Message::Files(message))
}

/// A Files tab whose session is open, and the effects of its opening.
async fn opened(app: &mut App) -> (TabId, Vec<Effect>) {
    let (tab, attempt) = match app
        .update(Message::OpenFiles(ProfileId::new("a")))
        .as_slice()
    {
        [
            Effect::Connect {
                tab,
                attempt,
                request,
            },
        ] => {
            assert_eq!(request.purpose, Purpose::Files);
            (*tab, *attempt)
        }
        other => panic!("expected a connect, got {other:?}"),
    };
    let effects = ready(app, tab, attempt).await;
    (tab, effects)
}

async fn ready(app: &mut App, tab: TabId, attempt: AttemptId) -> Vec<Effect> {
    app.update(Message::Connection {
        tab,
        attempt,
        event: ConnectionEvent::FilesReady {
            client: idle_client().await,
        },
    })
}

fn remote_entry(name: &[u8], kind: EntryKind, size: u64) -> RemoteEntry {
    RemoteEntry {
        name: name.to_vec(),
        label: String::from_utf8_lossy(name).into_owned(),
        kind,
        size: Some(size),
        modified: None,
        permissions: None,
    }
}

fn local_entry(name: &str, kind: EntryKind) -> LocalEntry {
    LocalEntry {
        name: name.into(),
        label: name.to_owned(),
        kind,
        size: Some(1),
        modified: None,
    }
}

fn listed_remote(app: &mut App, tab: TabId, path: &str, entries: Vec<RemoteEntry>) {
    files(
        app,
        FilesMessage::RemoteListed {
            tab,
            result: Ok((RemotePath::from(path), entries)),
        },
    );
}

#[tokio::test]
async fn an_open_session_lists_both_panes() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (tab, effects) = opened(&mut app).await;
    assert_eq!(app.tab(tab).expect("tab").phase, Phase::Connected);
    assert!(matches!(
        effects.as_slice(),
        [Effect::ListRemote { path, .. }, Effect::ListLocal { path: local, .. }]
            if path.as_bytes() == b"." && local == dir.path()
    ));
}

#[tokio::test]
async fn opening_a_folder_lists_it_and_up_goes_back() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (tab, _) = opened(&mut app).await;
    listed_remote(
        &mut app,
        tab,
        "/home/admin",
        vec![remote_entry(b"logs", EntryKind::Directory, 0)],
    );
    let opened_folder = files(
        &mut app,
        FilesMessage::Open {
            tab,
            side: Side::Remote,
            index: 0,
        },
    );
    assert!(matches!(
        opened_folder.as_slice(),
        [Effect::ListRemote { path, .. }] if path.as_bytes() == b"/home/admin/logs"
    ));
    let up = files(
        &mut app,
        FilesMessage::Up {
            tab,
            side: Side::Remote,
        },
    );
    assert!(matches!(
        up.as_slice(),
        [Effect::ListRemote { path, .. }] if path.as_bytes() == b"/home/admin"
    ));
}

#[tokio::test]
async fn opening_a_remote_file_downloads_it_into_the_local_folder() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (tab, _) = opened(&mut app).await;
    listed_remote(
        &mut app,
        tab,
        "/srv",
        vec![remote_entry(b"report.txt", EntryKind::File, 42)],
    );
    let effects = files(
        &mut app,
        FilesMessage::Open {
            tab,
            side: Side::Remote,
            index: 0,
        },
    );
    let [Effect::Transfer { request, .. }] = effects.as_slice() else {
        panic!("expected a transfer, got {effects:?}");
    };
    assert_eq!(request.direction, Direction::Download);
    assert_eq!(request.remote.as_bytes(), b"/srv/report.txt");
    assert_eq!(request.local, dir.path().join("report.txt"));
    let pane = app.tab(tab).and_then(|t| t.files.as_ref()).expect("files");
    assert_eq!(pane.transfers.len(), 1);
    assert_eq!(pane.transfers[0].total, Some(42));
}

#[tokio::test]
async fn an_existing_local_file_is_replaced_only_once_confirmed() {
    let dir = tempfile::tempdir().expect("dir");
    std::fs::write(dir.path().join("report.txt"), b"mine").expect("existing");
    let mut app = app(dir.path());
    let (tab, _) = opened(&mut app).await;
    listed_remote(
        &mut app,
        tab,
        "/srv",
        vec![remote_entry(b"report.txt", EntryKind::File, 42)],
    );
    files(
        &mut app,
        FilesMessage::Select {
            tab,
            side: Side::Remote,
            index: 0,
        },
    );
    let asked = files(
        &mut app,
        FilesMessage::Transfer {
            tab,
            direction: Direction::Download,
        },
    );
    assert!(asked.is_empty(), "nothing starts before the answer");
    assert!(matches!(app.dialog, Some(Dialog::ConfirmOverwrite { .. })));
    app.update(Message::DismissDialog);
    assert!(
        app.update(Message::ConfirmDialog).is_empty(),
        "a dismissed question starts nothing"
    );

    files(
        &mut app,
        FilesMessage::Transfer {
            tab,
            direction: Direction::Download,
        },
    );
    let confirmed = app.update(Message::ConfirmDialog);
    assert!(matches!(confirmed.as_slice(), [Effect::Transfer { .. }]));
}

#[tokio::test]
async fn an_upload_over_a_listed_remote_name_asks_and_then_replaces() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (tab, _) = opened(&mut app).await;
    listed_remote(
        &mut app,
        tab,
        "/srv",
        vec![remote_entry(b"notes.md", EntryKind::File, 3)],
    );
    files(
        &mut app,
        FilesMessage::LocalListed {
            tab,
            result: Ok((
                dir.path().to_owned(),
                vec![
                    local_entry("fresh.md", EntryKind::File),
                    local_entry("notes.md", EntryKind::File),
                ],
            )),
        },
    );
    let fresh = files(
        &mut app,
        FilesMessage::Open {
            tab,
            side: Side::Local,
            index: 0,
        },
    );
    assert!(
        matches!(fresh.as_slice(), [Effect::Transfer { request, .. }]
            if request.direction == Direction::Upload && !request.replace
                && request.remote.as_bytes() == b"/srv/fresh.md"),
        "{fresh:?}"
    );
    let clash = files(
        &mut app,
        FilesMessage::Open {
            tab,
            side: Side::Local,
            index: 1,
        },
    );
    assert!(clash.is_empty());
    let confirmed = app.update(Message::ConfirmDialog);
    assert!(matches!(
        confirmed.as_slice(),
        [Effect::Transfer { request, .. }] if request.replace
    ));
}

#[tokio::test]
async fn a_hostile_remote_name_is_refused_before_any_transfer() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (tab, _) = opened(&mut app).await;
    listed_remote(
        &mut app,
        tab,
        "/srv",
        vec![remote_entry(b"..", EntryKind::File, 1)],
    );
    let effects = files(
        &mut app,
        FilesMessage::Open {
            tab,
            side: Side::Remote,
            index: 0,
        },
    );
    assert!(effects.is_empty(), "{effects:?}");
    let pane = app.tab(tab).and_then(|t| t.files.as_ref()).expect("files");
    assert!(matches!(
        pane.transfers[0].state,
        TransferState::Failed(FilesError::UnsafeName { .. })
    ));
}

#[tokio::test]
async fn a_finished_download_refreshes_the_local_pane_and_closing_cancels_the_rest() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (tab, _) = opened(&mut app).await;
    listed_remote(
        &mut app,
        tab,
        "/srv",
        vec![
            remote_entry(b"one", EntryKind::File, 1),
            remote_entry(b"two", EntryKind::File, 1),
        ],
    );
    let first = files(
        &mut app,
        FilesMessage::Open {
            tab,
            side: Side::Remote,
            index: 0,
        },
    );
    let second = files(
        &mut app,
        FilesMessage::Open {
            tab,
            side: Side::Remote,
            index: 1,
        },
    );
    let [Effect::Transfer { id, .. }] = first.as_slice() else {
        panic!("{first:?}")
    };
    let [
        Effect::Transfer {
            request: running, ..
        },
    ] = second.as_slice()
    else {
        panic!("{second:?}")
    };
    let refreshed = files(
        &mut app,
        FilesMessage::TransferEvent {
            tab,
            id: *id,
            event: TransferEvent::Finished(TransferState::Done),
        },
    );
    assert!(matches!(refreshed.as_slice(), [Effect::ListLocal { .. }]));

    // Connected: closing asks first.
    app.update(Message::RequestCloseTab(tab));
    app.update(Message::ConfirmDialog);
    assert!(app.tab(tab).is_none());
    assert!(
        running.cancel.is_cancelled(),
        "the running transfer stops with its tab"
    );
}

#[tokio::test]
async fn a_host_key_question_keeps_the_tab_a_files_tab() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (tab, attempt) = match app
        .update(Message::OpenFiles(ProfileId::new("a")))
        .as_slice()
    {
        [Effect::Connect { tab, attempt, .. }] => (*tab, *attempt),
        other => panic!("{other:?}"),
    };
    let key = heimdall_ssh::PublicKey::from_openssh(
        "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIGb9wFcIfI8bE9fMhXj1bDO0N2sVX4r3r7cS3nNa2yZf",
    )
    .expect("key");
    app.update(Message::Connection {
        tab,
        attempt,
        event: ConnectionEvent::UnknownHostKey {
            host: "a.lab".to_owned(),
            port: 22,
            fingerprint: "SHA256:x".to_owned(),
            key: std::sync::Arc::new(key),
        },
    });
    let reconnect = app.update(Message::HostKeyDecision { tab, accept: true });
    assert!(
        matches!(reconnect.as_slice(), [Effect::Connect { request, .. }]
            if request.purpose == Purpose::Files),
        "{reconnect:?}"
    );
}
