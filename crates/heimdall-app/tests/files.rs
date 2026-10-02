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
    plan_transfer,
};
use heimdall_app::{
    App, AppConfig, AttemptId, ConnectionEvent, Dialog, Effect, FilesMessage, Message, Phase,
    Purpose, TabId,
};
use heimdall_core::profile::{ProfileId, SshProfile};
use heimdall_core::store::ProfileStore;
use heimdall_files::RemoteSession;
use heimdall_files::conflict::Choice;
use heimdall_sftp::protocol::{Request, Response, SFTP_VERSION, StatusCode};
use heimdall_sftp::{ClientConfig, RemotePath, SftpClient};
use heimdall_ssh::AgentSource;
use heimdall_term::GridSize;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

/// A client whose server answers the version exchange, then refuses everything: enough for
/// effects to carry, and for a plan to find nothing in its way.
async fn idle_client() -> RemoteSession {
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
        // Nothing is there: every request after the first is refused, so that a transfer's
        // plan finds its destination empty. The pipe stays open for the life of the test.
        loop {
            if server.read_exact(&mut length).await.is_err() {
                std::future::pending::<()>().await;
            }
            let mut body = vec![0; u32::from_be_bytes(length) as usize];
            server.read_exact(&mut body).await.expect("request");
            let id = body
                .get(1..5)
                .and_then(|id| <[u8; 4]>::try_from(id).ok())
                .map_or(0, u32::from_be_bytes);
            let refused = Response::Status {
                id,
                code: StatusCode::NoSuchFile,
                message: Vec::new(),
            };
            server.write_all(&refused.encode()).await.expect("status");
        }
    });
    RemoteSession::Sftp(
        SftpClient::start(client_end, ClientConfig::default())
            .await
            .expect("started"),
    )
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
        vault_entry: None,
        forwards: heimdall_core::profile::Forwards::default(),
        post_connect: heimdall_core::post_connect::PostConnect::default(),
        forward_agent: false,
        compression: false,
        sftp: false,
        legacy_algorithms: false,
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

fn files(app: &mut App, message: FilesMessage) -> Vec<Effect> {
    app.update(Message::Files(message))
}

/// Runs the plans among `effects` as the window does and gives them back to the app; the
/// other effects are kept, with what the plans started.
async fn planned(app: &mut App, effects: Vec<Effect>) -> Vec<Effect> {
    let mut out = Vec::new();
    for effect in effects {
        match effect {
            Effect::PlanTransfer { tab, request } => {
                let result = plan_transfer((*request).clone()).await.map(Box::new);
                out.extend(files(
                    app,
                    FilesMessage::Planned {
                        tab,
                        request,
                        result,
                    },
                ));
            }
            other => out.push(other),
        }
    }
    out
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
        owner: None,
        group: None,
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
async fn a_link_to_a_folder_is_entered_and_one_to_anything_else_goes_back_and_says_so() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (tab, _) = opened(&mut app).await;
    let home = || {
        vec![
            remote_entry(b"www", EntryKind::Link, 0),
            remote_entry(b"motd", EntryKind::Link, 0),
        ]
    };
    // By its label: the pane sorts what it shows.
    let open = |app: &mut App, label: &str| {
        let index = app
            .tab(tab)
            .and_then(|found| found.files.as_ref())
            .and_then(|files| {
                files
                    .remote
                    .entries
                    .iter()
                    .position(|entry| entry.label == label)
            })
            .expect("listed");
        files(
            app,
            FilesMessage::Open {
                tab,
                side: Side::Remote,
                index,
            },
        )
    };
    listed_remote(&mut app, tab, "/home/admin", home());
    let entered = open(&mut app, "www");
    assert!(
        matches!(
            entered.as_slice(),
            [Effect::ListRemote { path, .. }] if path.as_bytes() == b"/home/admin/www"
        ),
        "listed, never downloaded: {entered:?}"
    );
    // The server lists it: a folder.
    listed_remote(&mut app, tab, "/var/www", Vec::new());
    let shown = app.tab(tab).expect("tab").files.as_ref().expect("files");
    assert_eq!(shown.remote.path.display(), "/var/www");
    assert_eq!(app.notice(), None);

    listed_remote(&mut app, tab, "/home/admin", home());
    let tried = open(&mut app, "motd");
    assert!(
        matches!(
            tried.as_slice(),
            [Effect::ListRemote { path, .. }] if path.as_bytes() == b"/home/admin/motd"
        ),
        "{tried:?}"
    );
    // The server cannot list it: it points at no folder.
    files(
        &mut app,
        FilesMessage::RemoteListed {
            tab,
            result: Err(FilesError::SessionClosed),
        },
    );
    let shown = app.tab(tab).expect("tab").files.as_ref().expect("files");
    assert_eq!(
        shown.remote.path.display(),
        "/home/admin",
        "back where it was"
    );
    assert_eq!(shown.remote.error, None, "the folder shown did not fail");
    assert_eq!(
        app.notice(),
        Some(&heimdall_app::Notice::LinkNotAFolder("motd".to_owned()))
    );
}

#[tokio::test]
async fn back_returns_through_the_folders_left_and_home_to_the_first_one_as_the_csharp() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (tab, _) = opened(&mut app).await;
    let remote = |app: &App| {
        let files = app.tab(tab).expect("tab").files.as_ref().expect("files");
        (
            files.remote.path.display(),
            files
                .remote
                .history
                .iter()
                .map(RemotePath::display)
                .collect::<Vec<_>>(),
        )
    };
    let asked = |effects: &[Effect]| match effects {
        [Effect::ListRemote { path, .. }] => path.display(),
        other => panic!("{other:?}"),
    };
    let message = |app: &mut App, message: FilesMessage| files(app, message);
    listed_remote(
        &mut app,
        tab,
        "/home/admin",
        vec![remote_entry(b"logs", EntryKind::Directory, 0)],
    );
    assert_eq!(remote(&app), ("/home/admin".to_owned(), Vec::new()));
    let back = FilesMessage::Back {
        tab,
        side: Side::Remote,
    };
    assert!(
        message(&mut app, back.clone()).is_empty(),
        "nowhere to go back to"
    );

    // Into a folder, then up: each folder left is kept.
    let into = message(
        &mut app,
        FilesMessage::Open {
            tab,
            side: Side::Remote,
            index: 0,
        },
    );
    assert_eq!(asked(&into), "/home/admin/logs");
    listed_remote(&mut app, tab, "/home/admin/logs", Vec::new());
    let up = message(
        &mut app,
        FilesMessage::Up {
            tab,
            side: Side::Remote,
        },
    );
    assert_eq!(asked(&up), "/home/admin");
    listed_remote(&mut app, tab, "/home/admin", Vec::new());
    assert_eq!(
        remote(&app).1,
        ["/home/admin", "/home/admin/logs"],
        "most recent last"
    );

    // Back: to the last folder left, taken out once shown.
    assert_eq!(asked(&message(&mut app, back.clone())), "/home/admin/logs");
    listed_remote(&mut app, tab, "/home/admin/logs", Vec::new());
    assert_eq!(remote(&app).1, ["/home/admin"]);

    // A folder that cannot be listed is not one that was left.
    let typed = FilesMessage::PathEdited {
        tab,
        side: Side::Remote,
        text: "/root".to_owned(),
    };
    message(&mut app, typed);
    message(
        &mut app,
        FilesMessage::GoTo {
            tab,
            side: Side::Remote,
        },
    );
    files(
        &mut app,
        FilesMessage::RemoteListed {
            tab,
            result: Err(FilesError::SessionClosed),
        },
    );
    assert_eq!(
        remote(&app).1,
        ["/home/admin"],
        "the refused listing kept nothing"
    );

    // Home: the first folder shown, and the one left kept for Back.
    let home = message(
        &mut app,
        FilesMessage::Home {
            tab,
            side: Side::Remote,
        },
    );
    assert_eq!(asked(&home), "/home/admin");
    listed_remote(&mut app, tab, "/home/admin", Vec::new());
    assert_eq!(remote(&app).1, ["/home/admin", "/home/admin/logs"]);
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
    let opened = files(
        &mut app,
        FilesMessage::Open {
            tab,
            side: Side::Remote,
            index: 0,
        },
    );
    let effects = planned(&mut app, opened).await;
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
async fn an_existing_local_file_is_replaced_only_once_answered_so() {
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
    let transfer = |app: &mut App| {
        files(
            app,
            FilesMessage::Transfer {
                tab,
                direction: Direction::Download,
            },
        )
    };
    let started = transfer(&mut app);
    let asked = planned(&mut app, started).await;
    assert!(asked.is_empty(), "nothing starts before the answer");
    let Some(Dialog::FileConflicts { rows, .. }) = &app.dialog else {
        panic!("{:?}", app.dialog);
    };
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].target, "report.txt");
    assert_eq!(
        rows[0].choice,
        Choice::AutoRename,
        "the answer that loses nothing"
    );
    app.update(Message::DismissDialog);
    assert!(
        app.update(Message::ConfirmDialog).is_empty(),
        "a dismissed question starts nothing"
    );

    // Kept by default: the download takes a free name.
    let started = transfer(&mut app);
    planned(&mut app, started).await;
    let renamed = app.update(Message::ConfirmDialog);
    assert!(
        matches!(renamed.as_slice(), [Effect::Transfer { request, .. }]
            if !request.replace && request.local == dir.path().join("report (copy).txt")),
        "{renamed:?}"
    );

    // Replaced once answered so.
    let started = transfer(&mut app);
    planned(&mut app, started).await;
    files(
        &mut app,
        FilesMessage::ConflictChosen {
            row: 0,
            choice: Choice::Replace,
        },
    );
    let replaced = app.update(Message::ConfirmDialog);
    assert!(
        matches!(replaced.as_slice(), [Effect::Transfer { request, .. }]
            if request.replace && request.local == dir.path().join("report.txt")),
        "{replaced:?}"
    );
}

#[tokio::test]
async fn an_upload_with_nothing_in_its_way_starts_without_a_question() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (tab, _) = opened(&mut app).await;
    listed_remote(&mut app, tab, "/srv", Vec::new());
    files(
        &mut app,
        FilesMessage::LocalListed {
            tab,
            result: Ok((
                dir.path().to_owned(),
                vec![local_entry("fresh.md", EntryKind::File)],
            )),
        },
    );
    let opened = files(
        &mut app,
        FilesMessage::Open {
            tab,
            side: Side::Local,
            index: 0,
        },
    );
    let fresh = planned(&mut app, opened).await;
    assert!(
        matches!(fresh.as_slice(), [Effect::Transfer { request, .. }]
            if request.direction == Direction::Upload && !request.replace
                && request.remote.as_bytes() == b"/srv/fresh.md"),
        "{fresh:?}"
    );
    assert!(app.dialog.is_none());
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
    let first = planned(&mut app, first).await;
    let second = files(
        &mut app,
        FilesMessage::Open {
            tab,
            side: Side::Remote,
            index: 1,
        },
    );
    let second = planned(&mut app, second).await;
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

    // A transfer still running: closing says it cancels it, as the C# "Transfer In Progress".
    app.update(Message::RequestCloseTab(tab));
    let name = app.tab(tab).expect("tab").display_title().to_owned();
    assert_eq!(
        app.dialog,
        Some(Dialog::ConfirmCloseTransfers { tab, name }),
        "not the plain close question"
    );
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

#[tokio::test]
async fn a_folder_typed_in_the_path_bar_is_listed_from_the_one_shown() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (tab, _) = opened(&mut app).await;
    listed_remote(&mut app, tab, "/home/admin", Vec::new());
    let go = |app: &mut App, side: Side, text: &str| {
        files(
            app,
            FilesMessage::PathEdited {
                tab,
                side,
                text: text.to_owned(),
            },
        );
        files(app, FilesMessage::GoTo { tab, side })
    };
    let remote_path = |effects: &[Effect]| match effects {
        [Effect::ListRemote { path, .. }] => path.display(),
        other => panic!("{other:?}"),
    };
    assert_eq!(
        remote_path(&go(&mut app, Side::Remote, " /var/log ")),
        "/var/log"
    );
    assert_eq!(
        app.tab(tab)
            .expect("tab")
            .files
            .as_ref()
            .expect("files")
            .remote
            .path
            .display(),
        "/home/admin",
        "the folder shown stays until the listing comes back"
    );
    assert_eq!(
        remote_path(&go(&mut app, Side::Remote, "logs")),
        "/home/admin/logs",
        "relative to the folder shown"
    );
    assert!(
        files(
            &mut app,
            FilesMessage::GoTo {
                tab,
                side: Side::Remote
            }
        )
        .is_empty(),
        "gone to already: nothing typed since"
    );
    assert!(
        go(&mut app, Side::Remote, "   ").is_empty(),
        "nothing typed"
    );
    assert!(go(&mut app, Side::Local, "  ").is_empty(), "nor here");

    let local = go(&mut app, Side::Local, "sub");
    assert!(
        matches!(local.as_slice(), [Effect::ListLocal { path, .. }] if *path == dir.path().join("sub")),
        "{local:?}"
    );
    let elsewhere = tempfile::tempdir().expect("dir");
    let absolute = go(
        &mut app,
        Side::Local,
        &elsewhere.path().display().to_string(),
    );
    assert!(
        matches!(absolute.as_slice(), [Effect::ListLocal { path, .. }] if path == elsewhere.path()),
        "{absolute:?}"
    );
}

#[tokio::test]
async fn a_header_click_sorts_its_own_pane_only() {
    use heimdall_app::files::SortColumn;

    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (tab, _) = opened(&mut app).await;
    listed_remote(
        &mut app,
        tab,
        "/srv",
        vec![
            remote_entry(b"big", EntryKind::File, 90),
            remote_entry(b"small", EntryKind::File, 1),
        ],
    );
    let sort = |app: &mut App, side, column| {
        files(app, FilesMessage::SortBy { tab, side, column });
    };
    let remote = |app: &App| {
        let files = app.tab(tab).expect("tab").files.as_ref().expect("files");
        files
            .remote
            .entries
            .iter()
            .map(|entry| entry.label.clone())
            .collect::<Vec<_>>()
    };
    sort(&mut app, Side::Remote, SortColumn::Size);
    assert_eq!(remote(&app), ["small", "big"]);
    sort(&mut app, Side::Remote, SortColumn::Size);
    assert_eq!(remote(&app), ["big", "small"], "the other way");
    sort(&mut app, Side::Local, SortColumn::Size);
    assert_eq!(
        remote(&app),
        ["big", "small"],
        "the other pane's sort is its own"
    );
    let files_pane = app.tab(tab).expect("tab").files.as_ref().expect("files");
    assert_eq!(files_pane.local.sort.column, SortColumn::Size);
    assert!(files_pane.remote.sort.descending);
}

#[tokio::test]
async fn copy_path_copies_the_selected_entry_whole_and_says_so() {
    use heimdall_app::Notice;

    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (tab, _) = opened(&mut app).await;
    listed_remote(
        &mut app,
        tab,
        "/srv",
        vec![remote_entry(b"run.sh", EntryKind::File, 1)],
    );
    let copy = |app: &mut App, side| files(app, FilesMessage::CopyPath { tab, side });
    assert!(copy(&mut app, Side::Remote).is_empty(), "nothing selected");
    files(
        &mut app,
        FilesMessage::Select {
            tab,
            side: Side::Remote,
            index: 0,
        },
    );
    let effects = copy(&mut app, Side::Remote);
    assert!(
        matches!(effects.as_slice(), [Effect::WriteClipboard(text)] if text == "/srv/run.sh"),
        "{effects:?}"
    );
    assert_eq!(
        app.notice(),
        Some(&Notice::Copied("/srv/run.sh".to_owned()))
    );

    files(
        &mut app,
        FilesMessage::LocalListed {
            tab,
            result: Ok((
                dir.path().to_owned(),
                vec![local_entry("notes.md", EntryKind::File)],
            )),
        },
    );
    files(
        &mut app,
        FilesMessage::Select {
            tab,
            side: Side::Local,
            index: 0,
        },
    );
    let local = dir.path().join("notes.md").display().to_string();
    let effects = copy(&mut app, Side::Local);
    assert!(
        matches!(effects.as_slice(), [Effect::WriteClipboard(text)] if *text == local),
        "{effects:?}"
    );
}

#[tokio::test]
async fn the_servers_folder_is_bookmarked_once_and_gone_back_to() {
    use heimdall_app::Notice;

    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let (tab, _) = opened(&mut app).await;
    let bookmarks = |app: &App| {
        app.tab(tab)
            .expect("tab")
            .files
            .as_ref()
            .expect("files")
            .bookmarks
            .iter()
            .map(RemotePath::display)
            .collect::<Vec<_>>()
    };
    assert!(files(&mut app, FilesMessage::OpenBookmark { tab, index: 0 }).is_empty());
    listed_remote(&mut app, tab, "/var/log", Vec::new());
    files(&mut app, FilesMessage::Bookmark { tab });
    assert_eq!(
        app.notice(),
        Some(&Notice::Bookmarked("/var/log".to_owned()))
    );
    files(&mut app, FilesMessage::Bookmark { tab });
    listed_remote(&mut app, tab, "/etc", Vec::new());
    files(&mut app, FilesMessage::Bookmark { tab });
    assert_eq!(bookmarks(&app), ["/var/log", "/etc"], "once each, in order");
    files(
        &mut app,
        FilesMessage::Refresh {
            tab,
            side: Side::Local,
        },
    );
    let back = files(&mut app, FilesMessage::OpenBookmark { tab, index: 0 });
    let shown = app.tab(tab).expect("tab").files.as_ref().expect("files");
    assert_eq!(
        shown.remote.path.display(),
        "/etc",
        "until the listing comes back"
    );
    assert_eq!(
        shown.focus,
        Side::Remote,
        "the keys go to the server's pane"
    );
    assert!(
        matches!(back.as_slice(), [Effect::ListRemote { path, .. }] if path.as_bytes() == b"/var/log"),
        "{back:?}"
    );
    assert!(files(&mut app, FilesMessage::OpenBookmark { tab, index: 2 }).is_empty());
}
