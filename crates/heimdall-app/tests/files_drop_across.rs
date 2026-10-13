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

//! Entries dragged from the file list of one tab onto the file list of another, against FTP
//! servers run in the test, as the C# lists take them: files of this computer uploaded to a
//! server, a server's entries downloaded to this computer, through the transfers a "Paste"
//! or a "Download" goes through; a server's entries onto another server, and files of this
//! computer onto another local list, refused.

#[path = "support/ftp_server.rs"]
mod ftp_server;

use std::path::Path;
use std::time::Duration;

use heimdall_app::files::{
    Direction, EntryKind, LocalEntry, RemoteEntry, Side, TransferState, plan_transfer,
    transfer_events,
};
use heimdall_app::{App, AppConfig, ConnectionEvent, Dialog, Effect, FilesMessage, Message, TabId};
use heimdall_core::profile::{FtpProfile, ProfileId};
use heimdall_files::{FtpClient, FtpSecurity, FtpTarget, RemotePath, RemoteSession};
use heimdall_ssh::AgentSource;
use heimdall_term::GridSize;
use tokio_stream::StreamExt as _;

/// Longest wait for the test server, or for one operation.
const STEP: Duration = Duration::from_secs(20);

/// The profile whose tab is never connected.
const OFFLINE: &str = "offline";

/// An anonymous FTP session to `port`.
async fn session(port: u16) -> RemoteSession {
    let client = FtpClient::connect(&FtpTarget {
        host: "127.0.0.1".to_owned(),
        port,
        username: None,
        password: String::new(),
        passive: true,
        security: FtpSecurity::Plain,
        timeout: STEP,
    })
    .await
    .expect("connected");
    RemoteSession::Ftp(client)
}

fn profile(id: &str, port: u16) -> FtpProfile {
    FtpProfile {
        id: ProfileId::new(id),
        name: id.to_owned(),
        group: None,
        host: "127.0.0.1".to_owned(),
        port,
        username: None,
        passive: true,
        tls: false,
        vault_entry: None,
    }
}

/// An application knowing `profiles`, its Files tabs starting in `start`.
fn app(dir: &Path, start: &Path, profiles: Vec<FtpProfile>) -> App {
    let profiles_file = dir.join("profiles.toml");
    let mut store = heimdall_core::store::ProfileStore::open(&profiles_file).expect("store");
    store.merge_ftp(profiles);
    store.save().expect("save");
    App::new(AppConfig {
        profiles_file,
        known_hosts: dir.join("known_hosts"),
        legacy_dir: None,
        agent: AgentSource::Disabled,
        initial_grid: GridSize { cols: 80, rows: 24 },
        files_start: start.to_owned(),
        system_credentials: heimdall_app::SystemCredentials::memory(),
    })
}

fn files(app: &mut App, message: FilesMessage) -> Vec<Effect> {
    app.update(Message::Files(message))
}

/// The FTP tab of profile `id`, connected with `client` unless `None`, its server's `/`
/// listing `remote` and this computer's folder `local`.
fn ftp_tab(
    app: &mut App,
    id: &str,
    client: Option<RemoteSession>,
    (remote, local): (Vec<RemoteEntry>, Vec<LocalEntry>),
) -> TabId {
    let effects = app.update(Message::ConnectProfile(ProfileId::new(id)));
    let [Effect::ConnectFtp { tab, attempt, .. }] = effects.as_slice() else {
        panic!("{effects:?}");
    };
    let (tab, attempt) = (*tab, *attempt);
    if let Some(client) = client {
        app.update(Message::Connection {
            tab,
            attempt,
            event: ConnectionEvent::FilesReady {
                client,
                shell: None,
            },
        });
        files(
            app,
            FilesMessage::RemoteListed {
                tab,
                result: Ok((RemotePath::from("/"), remote)),
            },
        );
    }
    let folder = app
        .tab(tab)
        .and_then(|found| found.files.as_deref())
        .map(|pane| pane.local.path.clone())
        .expect("files");
    files(
        app,
        FilesMessage::LocalListed {
            tab,
            result: Ok((folder, local)),
        },
    );
    tab
}

fn remote_file(name: &str, kind: EntryKind) -> RemoteEntry {
    RemoteEntry {
        name: name.as_bytes().to_vec(),
        label: name.to_owned(),
        kind,
        size: Some(10),
        modified: None,
        permissions: None,
        owner: None,
        owner_name: None,
        group: None,
        inode: None,
    }
}

fn local_file(name: &str, kind: EntryKind) -> LocalEntry {
    LocalEntry {
        name: name.into(),
        label: name.to_owned(),
        kind,
        size: Some(10),
        modified: None,
    }
}

fn select(app: &mut App, tab: TabId, side: Side, index: usize) {
    files(app, FilesMessage::Select { tab, side, index });
}

fn drop_across(
    app: &mut App,
    (from_tab, from): (TabId, Side),
    (tab, onto): (TabId, Side),
    into: Option<usize>,
) -> Vec<Effect> {
    files(
        app,
        FilesMessage::DropAcross {
            from_tab,
            from,
            tab,
            onto,
            into,
        },
    )
}

/// Runs `effects` as the window does, plans and transfers, until none is left; the other
/// effects are given back.
async fn run(app: &mut App, effects: Vec<Effect>) -> Vec<Effect> {
    let mut waiting = effects;
    let mut rest = Vec::new();
    while let Some(effect) = waiting.pop() {
        match effect {
            Effect::PlanTransfer { tab, request } => {
                let result = tokio::time::timeout(STEP, plan_transfer((*request).clone()))
                    .await
                    .expect("planned in time")
                    .map(Box::new);
                waiting.extend(files(
                    app,
                    FilesMessage::Planned {
                        tab,
                        request,
                        result,
                    },
                ));
            }
            Effect::Transfer { tab, id, request } => {
                let mut events = std::pin::pin!(transfer_events(*request));
                while let Some(event) = tokio::time::timeout(STEP, events.next())
                    .await
                    .expect("in time")
                {
                    waiting.extend(files(app, FilesMessage::TransferEvent { tab, id, event }));
                }
            }
            other => rest.push(other),
        }
    }
    rest
}

fn transfers(app: &App, tab: TabId) -> Vec<(Direction, TransferState)> {
    app.tab(tab)
        .and_then(|found| found.files.as_deref())
        .map(|pane| {
            pane.transfers
                .iter()
                .map(|transfer| (transfer.direction, transfer.state.clone()))
                .collect()
        })
        .unwrap_or_default()
}

/// Two FTP servers and this computer's folder, with an FTP tab on each server.
struct Bench {
    dir: tempfile::TempDir,
    local: tempfile::TempDir,
    first: tempfile::TempDir,
    second: tempfile::TempDir,
    app: App,
    source: TabId,
    target: TabId,
}

/// The source tab lists `report.txt` and the folder `logs` on its server and on this
/// computer; the target tab's server holds the folder `inbox` alone.
async fn bench() -> Bench {
    let dir = tempfile::tempdir().expect("dir");
    let local = tempfile::tempdir().expect("local");
    std::fs::write(local.path().join("notes.txt"), b"mine\n").expect("local file");
    std::fs::create_dir(local.path().join("drop")).expect("local folder");
    let first = tempfile::tempdir().expect("first");
    std::fs::write(first.path().join("report.txt"), b"quarterly\n").expect("file");
    std::fs::create_dir(first.path().join("logs")).expect("folder");
    std::fs::write(first.path().join("logs").join("a.log"), b"line\n").expect("log");
    let second = tempfile::tempdir().expect("second");
    std::fs::create_dir(second.path().join("inbox")).expect("inbox");
    let (from_port, to_port) = (
        ftp_server::serve(first.path(), None).await,
        ftp_server::serve(second.path(), None).await,
    );
    let mut app = app(
        dir.path(),
        local.path(),
        vec![
            profile("from", from_port),
            profile("to", to_port),
            profile(OFFLINE, from_port),
        ],
    );
    let listed_local = vec![
        local_file("drop", EntryKind::Directory),
        local_file("notes.txt", EntryKind::File),
    ];
    let source = ftp_tab(
        &mut app,
        "from",
        Some(session(from_port).await),
        (
            vec![
                remote_file("logs", EntryKind::Directory),
                remote_file("report.txt", EntryKind::File),
            ],
            listed_local.clone(),
        ),
    );
    let target = ftp_tab(
        &mut app,
        "to",
        Some(session(to_port).await),
        (
            vec![remote_file("inbox", EntryKind::Directory)],
            listed_local,
        ),
    );
    Bench {
        dir,
        local,
        first,
        second,
        app,
        source,
        target,
    }
}

#[tokio::test]
async fn files_of_this_computer_dropped_on_another_tabs_server_are_uploaded_into_the_folder_dropped_on()
 {
    let mut bench = bench().await;
    let app = &mut bench.app;
    let (source, target) = (bench.source, bench.target);
    assert!(app.takes_drop_across((source, Side::Local), (target, Side::Remote)));
    select(app, source, Side::Local, 1);

    let effects = drop_across(app, (source, Side::Local), (target, Side::Remote), Some(0));
    let left = run(app, effects).await;
    assert!(
        left.iter()
            .any(|effect| matches!(effect, Effect::ListRemote { tab, .. } if *tab == target)),
        "the server's list dropped on listed again: {left:?}"
    );
    assert_eq!(
        std::fs::read(bench.second.path().join("inbox").join("notes.txt")).expect("uploaded"),
        b"mine\n",
        "into the folder dropped on"
    );
    assert_eq!(
        transfers(app, target),
        vec![(Direction::Upload, TransferState::Done)],
        "on the tab dropped on, as its own upload"
    );
    assert!(
        transfers(app, source).is_empty(),
        "nothing on the tab dragged from"
    );
    assert!(
        bench.local.path().join("notes.txt").exists(),
        "a drop copies"
    );
}

#[tokio::test]
async fn a_drop_from_another_tab_uploads_as_a_drop_from_explorer_does() {
    let mut bench = bench().await;
    let app = &mut bench.app;
    let (source, target) = (bench.source, bench.target);
    select(app, source, Side::Local, 1);
    let dragged = drop_across(app, (source, Side::Local), (target, Side::Remote), None);
    let dropped = files(
        app,
        FilesMessage::Dropped {
            tab: target,
            paths: vec![bench.local.path().join("notes.txt")],
        },
    );
    assert!(
        matches!(dragged.as_slice(), [Effect::PlanTransfer { tab, .. }] if *tab == target),
        "planned on the tab dropped on: {dragged:?}"
    );
    assert!(dropped.is_empty(), "the second waits its turn: {dropped:?}");
    let queued: Vec<_> = app
        .tab(target)
        .and_then(|found| found.files.as_deref())
        .expect("files")
        .transfers
        .iter()
        .map(|transfer| {
            let picked = transfer.picked.as_ref().expect("picked");
            (
                transfer.direction,
                picked.label.clone(),
                picked.total,
                picked.root.remote.clone(),
                picked.root.local.clone(),
            )
        })
        .collect();
    let one = (
        Direction::Upload,
        "notes.txt".to_owned(),
        Some(5),
        RemotePath::from("/notes.txt"),
        bench.local.path().join("notes.txt"),
    );
    assert_eq!(
        queued,
        vec![one.clone(), one],
        "the same upload, into the folder shown"
    );
}

#[tokio::test]
async fn a_name_in_the_way_is_asked_about_as_a_paste_asks() {
    let mut bench = bench().await;
    std::fs::write(bench.second.path().join("notes.txt"), b"theirs\n").expect("in the way");
    let app = &mut bench.app;
    let (source, target) = (bench.source, bench.target);
    select(app, source, Side::Local, 1);
    let effects = drop_across(app, (source, Side::Local), (target, Side::Remote), None);
    let started = run(app, effects).await;
    assert!(
        started.is_empty(),
        "nothing written before the answer: {started:?}"
    );
    let Some(Dialog::FileConflicts { rows, .. }) = &app.dialog else {
        panic!("{:?}", app.dialog);
    };
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].target, "notes.txt");
    assert_eq!(
        std::fs::read(bench.second.path().join("notes.txt")).expect("kept"),
        b"theirs\n"
    );
}

#[tokio::test]
async fn a_servers_entries_dropped_on_another_tabs_local_list_are_downloaded_there() {
    let mut bench = bench().await;
    let app = &mut bench.app;
    let (source, target) = (bench.source, bench.target);
    assert!(app.takes_drop_across((source, Side::Remote), (target, Side::Local)));
    select(app, source, Side::Remote, 0);
    files(
        app,
        FilesMessage::Toggle {
            tab: source,
            side: Side::Remote,
            index: 1,
        },
    );

    let effects = drop_across(app, (source, Side::Remote), (target, Side::Local), Some(0));
    let left = run(app, effects).await;
    let drop_folder = bench.local.path().join("drop");
    assert_eq!(
        std::fs::read(drop_folder.join("report.txt")).expect("downloaded"),
        b"quarterly\n"
    );
    assert_eq!(
        std::fs::read(drop_folder.join("logs").join("a.log")).expect("downloaded"),
        b"line\n",
        "a folder with what it holds"
    );
    assert_eq!(
        transfers(app, source),
        vec![
            (Direction::Download, TransferState::Done),
            (Direction::Download, TransferState::Done)
        ],
        "on the server's tab, as its own download"
    );
    assert!(transfers(app, target).is_empty());
    assert!(
        bench.first.path().join("report.txt").exists(),
        "a drop copies"
    );
    assert!(
        !left
            .iter()
            .any(|effect| matches!(effect, Effect::ListLocal { tab, .. } if *tab == target)),
        "the folder shown did not change: {left:?}"
    );

    // Into the folder shown: the local list dropped on is listed again once it is there.
    select(app, source, Side::Remote, 1);
    let effects = drop_across(app, (source, Side::Remote), (target, Side::Local), None);
    let left = run(app, effects).await;
    assert_eq!(
        std::fs::read(bench.local.path().join("report.txt")).expect("downloaded"),
        b"quarterly\n"
    );
    assert!(
        left.iter()
            .any(|effect| matches!(effect, Effect::ListLocal { tab, .. } if *tab == target)),
        "{left:?}"
    );
}

#[tokio::test]
async fn a_servers_entries_dropped_on_another_server_are_refused() {
    let mut bench = bench().await;
    let app = &mut bench.app;
    let (source, target) = (bench.source, bench.target);
    assert!(!app.takes_drop_across((source, Side::Remote), (target, Side::Remote)));
    select(app, source, Side::Remote, 1);
    let effects = drop_across(app, (source, Side::Remote), (target, Side::Remote), Some(0));
    assert!(effects.is_empty(), "{effects:?}");
    assert!(transfers(app, source).is_empty());
    assert!(transfers(app, target).is_empty());
    assert!(
        !bench
            .second
            .path()
            .join("inbox")
            .join("report.txt")
            .exists()
    );
    let _ = &bench.dir;
}

#[tokio::test]
async fn files_of_this_computer_dropped_on_another_local_list_are_refused() {
    let mut bench = bench().await;
    let app = &mut bench.app;
    let (source, target) = (bench.source, bench.target);
    assert!(!app.takes_drop_across((source, Side::Local), (target, Side::Local)));
    select(app, source, Side::Local, 1);
    let effects = drop_across(app, (source, Side::Local), (target, Side::Local), Some(0));
    assert!(effects.is_empty(), "{effects:?}");
    assert!(!bench.local.path().join("drop").join("notes.txt").exists());
}

#[tokio::test]
async fn a_drop_on_its_own_tab_is_no_drop_across() {
    let mut bench = bench().await;
    let app = &mut bench.app;
    let source = bench.source;
    assert!(!app.takes_drop_across((source, Side::Local), (source, Side::Remote)));
    select(app, source, Side::Local, 1);
    let effects = drop_across(app, (source, Side::Local), (source, Side::Remote), None);
    assert!(effects.is_empty(), "{effects:?}");
}

#[tokio::test]
async fn a_server_not_connected_takes_no_drop_and_gives_none() {
    let mut bench = bench().await;
    let app = &mut bench.app;
    let source = bench.source;
    let offline = ftp_tab(app, OFFLINE, None, (Vec::new(), Vec::new()));
    assert!(!app.takes_drop_across((source, Side::Local), (offline, Side::Remote)));
    assert!(!app.takes_drop_across((offline, Side::Remote), (source, Side::Local)));
    select(app, source, Side::Local, 1);
    let effects = drop_across(app, (source, Side::Local), (offline, Side::Remote), None);
    assert!(effects.is_empty(), "{effects:?}");
    assert!(transfers(app, offline).is_empty());
}
