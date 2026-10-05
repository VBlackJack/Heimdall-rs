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

//! Folders in a Files tab: selecting and opening them, sending them whole, and the new
//! folder, rename and delete operations, from the question asked to what is done on disk.

use std::path::{Path, PathBuf};

use heimdall_app::files::{
    Direction, EntryKind, FileOperation, FilesError, FilesKey, LocalEntry, PlanRequest,
    RemoteEntry, Side, file_operation, plan_transfer, typed_name,
};
use heimdall_app::{
    App, AppConfig, ConnectionEvent, Dialog, Effect, FilesMessage, Message, NameAction, TabId,
};
use heimdall_core::profile::{ProfileId, SshProfile};
use heimdall_core::store::ProfileStore;
use heimdall_files::RemoteSession;
use heimdall_files::conflict::Kind;
use heimdall_sftp::protocol::{Request, Response, SFTP_VERSION, StatusCode};
use heimdall_sftp::{ClientConfig, RemotePath, SftpClient};
use heimdall_ssh::AgentSource;
use heimdall_term::GridSize;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

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

/// A Files tab with `/srv` listed on the server and `dir` on this computer.
async fn tab(dir: &Path) -> (App, TabId) {
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
        session_logging: None,
    }]);
    store.save().expect("save");
    let mut app = App::new(AppConfig {
        profiles_file,
        known_hosts: dir.join("known_hosts"),
        legacy_dir: None,
        agent: AgentSource::Disabled,
        initial_grid: GridSize { cols: 80, rows: 24 },
        files_start: dir.to_owned(),
        system_credentials: heimdall_app::SystemCredentials::memory(),
    });
    let (tab, attempt) = match app
        .update(Message::OpenFiles(ProfileId::new("a")))
        .as_slice()
    {
        [Effect::Connect { tab, attempt, .. }] => (*tab, *attempt),
        other => panic!("{other:?}"),
    };
    app.update(Message::Connection {
        tab,
        attempt,
        event: ConnectionEvent::FilesReady {
            client: idle_client().await,
            shell: None,
        },
    });
    let remote = |name: &str, kind| RemoteEntry {
        name: name.as_bytes().to_vec(),
        label: name.to_owned(),
        kind,
        size: Some(4096),
        modified: None,
        permissions: None,
        owner: None,
        group: None,
    };
    let local = |name: &str, kind| LocalEntry {
        name: name.into(),
        label: name.to_owned(),
        kind,
        size: Some(4096),
        modified: None,
    };
    files(
        &mut app,
        FilesMessage::RemoteListed {
            tab,
            result: Ok((
                RemotePath::from("/srv"),
                vec![
                    remote("logs", EntryKind::Directory),
                    remote("a.txt", EntryKind::File),
                ],
            )),
        },
    );
    files(
        &mut app,
        FilesMessage::LocalListed {
            tab,
            result: Ok((
                dir.to_owned(),
                vec![
                    local("docs", EntryKind::Directory),
                    local("b.txt", EntryKind::File),
                ],
            )),
        },
    );
    (app, tab)
}

fn files(app: &mut App, message: FilesMessage) -> Vec<Effect> {
    app.update(Message::Files(message))
}

/// Stops the transfer being planned, so that the next one asked for is planned at once.
fn stop_planning(app: &mut App, tab: TabId) {
    let id = app
        .tab(tab)
        .and_then(|found| found.files.as_ref())
        .and_then(|files| {
            files
                .transfers
                .iter()
                .find(|t| t.state == heimdall_app::files::TransferState::Preparing)
        })
        .map(|transfer| transfer.id)
        .expect("one being planned");
    assert!(files(app, FilesMessage::Cancel { tab, id }).is_empty());
}

/// The plan a transfer starts with: what it was asked for, before anything is written.
fn plan_request(effects: &[Effect]) -> &PlanRequest {
    match effects {
        [Effect::PlanTransfer { request, .. }] => request,
        other => panic!("expected a plan, got {other:?}"),
    }
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

fn select(app: &mut App, tab: TabId, side: Side, index: usize) -> Vec<Effect> {
    files(app, FilesMessage::Select { tab, side, index })
}

fn operation(effects: &[Effect]) -> FileOperation {
    match effects {
        // A delete or a change of permissions starts as the first step of its run.
        [Effect::FileOperation { operation, .. } | Effect::FileBatchStep { operation, .. }] => {
            (**operation).clone()
        }
        other => panic!("expected an operation, got {other:?}"),
    }
}

fn pane_error(app: &App, tab: TabId, side: Side) -> Option<FilesError> {
    let files = app.tab(tab).and_then(|t| t.files.as_ref()).expect("files");
    match side {
        Side::Remote => files.remote.error.clone(),
        Side::Local => files.local.error.clone(),
    }
}

#[tokio::test]
async fn one_click_selects_a_folder_and_a_second_opens_it() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut app, tab) = tab(dir.path()).await;
    assert!(select(&mut app, tab, Side::Remote, 0).is_empty());
    let files_pane = app.tab(tab).and_then(|t| t.files.as_ref()).expect("files");
    assert_eq!(files_pane.remote.selected, Some(0));
    // A file between the two clicks: the second click on the folder selects it again.
    select(&mut app, tab, Side::Remote, 1);
    assert!(select(&mut app, tab, Side::Remote, 0).is_empty());
    let opened = select(&mut app, tab, Side::Remote, 0);
    assert!(
        matches!(opened.as_slice(), [Effect::ListRemote { path, .. }]
            if path.as_bytes() == b"/srv/logs"),
        "{opened:?}"
    );
    // A second click on a selected file does not start a transfer.
    select(&mut app, tab, Side::Local, 1);
    assert!(select(&mut app, tab, Side::Local, 1).is_empty());
    let local = select(&mut app, tab, Side::Local, 0);
    assert!(local.is_empty());
    let opened = select(&mut app, tab, Side::Local, 0);
    assert!(
        matches!(opened.as_slice(), [Effect::ListLocal { path, .. }]
            if *path == dir.path().join("docs")),
        "{opened:?}"
    );
}

#[tokio::test]
async fn a_selected_folder_is_sent_whole_in_both_directions() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut app, tab) = tab(dir.path()).await;
    select(&mut app, tab, Side::Remote, 0);
    let down = files(
        &mut app,
        FilesMessage::Transfer {
            tab,
            direction: Direction::Download,
        },
    );
    let request = plan_request(&down);
    assert_eq!(request.direction, Direction::Download);
    let [root] = request.roots.as_slice() else {
        panic!("{request:?}")
    };
    assert_eq!(root.root.kind, Kind::Folder);
    assert_eq!(root.root.remote.as_bytes(), b"/srv/logs");
    assert_eq!(root.root.local, dir.path().join("logs"));
    stop_planning(&mut app, tab);
    select(&mut app, tab, Side::Local, 0);
    let up = files(
        &mut app,
        FilesMessage::Transfer {
            tab,
            direction: Direction::Upload,
        },
    );
    let request = plan_request(&up);
    assert_eq!(request.direction, Direction::Upload);
    assert_eq!(request.roots[0].root.kind, Kind::Folder);
    assert_eq!(request.roots[0].root.remote.as_bytes(), b"/srv/docs");
    let files_pane = app.tab(tab).and_then(|t| t.files.as_ref()).expect("files");
    assert!(
        files_pane.transfers.iter().all(|t| t.total.is_none()),
        "a folder's own size is not what its transfer moves"
    );
    stop_planning(&mut app, tab);
    // A file stays a file.
    select(&mut app, tab, Side::Remote, 1);
    let file = files(
        &mut app,
        FilesMessage::Transfer {
            tab,
            direction: Direction::Download,
        },
    );
    assert_eq!(plan_request(&file).roots[0].root.kind, Kind::File);
}

#[tokio::test]
async fn a_new_folder_takes_the_typed_name_in_the_pane_folder() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut app, tab) = tab(dir.path()).await;
    files(
        &mut app,
        FilesMessage::AskNewFolder {
            tab,
            side: Side::Remote,
        },
    );
    assert!(matches!(
        &app.dialog,
        Some(Dialog::AskName { action: NameAction::NewFolder, value, .. }) if value.is_empty()
    ));
    files(&mut app, FilesMessage::NameEdited("fresh".to_owned()));
    match operation(&app.update(Message::ConfirmDialog)) {
        FileOperation::RemoteMakeFolder { path, .. } => assert_eq!(path.as_bytes(), b"/srv/fresh"),
        other => panic!("{other:?}"),
    }
    assert!(app.dialog.is_none());

    files(
        &mut app,
        FilesMessage::AskNewFolder {
            tab,
            side: Side::Local,
        },
    );
    files(&mut app, FilesMessage::NameEdited("mine".to_owned()));
    match operation(&app.update(Message::ConfirmDialog)) {
        FileOperation::LocalMakeFolder { path } => assert_eq!(path, dir.path().join("mine")),
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn a_rename_starts_from_the_current_name_and_stays_in_its_folder() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut app, tab) = tab(dir.path()).await;
    select(&mut app, tab, Side::Remote, 1);
    files(
        &mut app,
        FilesMessage::AskRename {
            tab,
            side: Side::Remote,
        },
    );
    assert!(matches!(
        &app.dialog,
        Some(Dialog::AskName { action: NameAction::Rename, value, .. }) if value == "a.txt"
    ));
    files(&mut app, FilesMessage::NameEdited("c.txt".to_owned()));
    match operation(&app.update(Message::ConfirmDialog)) {
        FileOperation::RemoteRename { from, to, .. } => {
            assert_eq!(from.as_bytes(), b"/srv/a.txt");
            assert_eq!(to.as_bytes(), b"/srv/c.txt");
        }
        other => panic!("{other:?}"),
    }
    select(&mut app, tab, Side::Local, 1);
    files(
        &mut app,
        FilesMessage::AskRename {
            tab,
            side: Side::Local,
        },
    );
    files(&mut app, FilesMessage::NameEdited("d.txt".to_owned()));
    match operation(&app.update(Message::ConfirmDialog)) {
        FileOperation::LocalRename { from, to } => {
            assert_eq!(from, dir.path().join("b.txt"));
            assert_eq!(to, dir.path().join("d.txt"));
        }
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn nothing_to_rename_or_delete_without_a_selection() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut app, tab) = tab(dir.path()).await;
    for side in [Side::Remote, Side::Local] {
        files(&mut app, FilesMessage::AskRename { tab, side });
        files(&mut app, FilesMessage::AskDelete { tab, side });
        assert!(app.dialog.is_none());
        assert!(app.update(Message::ConfirmDialog).is_empty());
    }
}

#[tokio::test]
async fn a_delete_names_what_it_deletes_and_a_dismissed_one_does_nothing() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut app, tab) = tab(dir.path()).await;
    select(&mut app, tab, Side::Remote, 0);
    files(
        &mut app,
        FilesMessage::AskDelete {
            tab,
            side: Side::Remote,
        },
    );
    assert!(matches!(
        &app.dialog,
        Some(Dialog::ConfirmDelete { name, folder: true, .. }) if name == "logs"
    ));
    app.update(Message::DismissDialog);
    assert!(
        app.update(Message::ConfirmDialog).is_empty(),
        "a dismissed delete deletes nothing"
    );
    files(
        &mut app,
        FilesMessage::AskDelete {
            tab,
            side: Side::Remote,
        },
    );
    match operation(&app.update(Message::ConfirmDialog)) {
        FileOperation::RemoteRemove { path, .. } => assert_eq!(path.as_bytes(), b"/srv/logs"),
        other => panic!("{other:?}"),
    }
    // One delete at a time in a tab: this one over first.
    files(
        &mut app,
        FilesMessage::BatchStepDone {
            tab,
            result: Ok(()),
        },
    );
    select(&mut app, tab, Side::Local, 1);
    files(
        &mut app,
        FilesMessage::AskDelete {
            tab,
            side: Side::Local,
        },
    );
    assert!(matches!(
        &app.dialog,
        Some(Dialog::ConfirmDelete { folder: false, .. })
    ));
    match operation(&app.update(Message::ConfirmDialog)) {
        FileOperation::LocalRemove { path } => assert_eq!(path, dir.path().join("b.txt")),
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn an_unusable_typed_name_is_refused_in_the_pane() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut app, tab) = tab(dir.path()).await;
    for (side, typed) in [(Side::Remote, "a/b"), (Side::Local, "..")] {
        files(&mut app, FilesMessage::AskNewFolder { tab, side });
        files(&mut app, FilesMessage::NameEdited(typed.to_owned()));
        assert!(app.update(Message::ConfirmDialog).is_empty(), "{typed}");
        assert_eq!(pane_error(&app, tab, side), Some(FilesError::InvalidName));
    }
}

#[tokio::test]
async fn a_finished_operation_refreshes_its_pane_and_shows_a_failure() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut app, tab) = tab(dir.path()).await;
    let done = files(
        &mut app,
        FilesMessage::OperationDone {
            tab,
            side: Side::Local,
            result: Ok(()),
        },
    );
    assert!(matches!(done.as_slice(), [Effect::ListLocal { .. }]));
    let failed = files(
        &mut app,
        FilesMessage::OperationDone {
            tab,
            side: Side::Remote,
            result: Err(FilesError::Exists),
        },
    );
    assert!(matches!(failed.as_slice(), [Effect::ListRemote { .. }]));
    assert_eq!(
        pane_error(&app, tab, Side::Remote),
        Some(FilesError::Exists),
        "the refresh keeps the failure until it lands"
    );
}

#[test]
fn typed_names_are_checked_for_their_side() {
    for bad in ["", ".", "..", "a/b", "tab\there"] {
        assert_eq!(
            typed_name(Side::Remote, bad).err(),
            Some(FilesError::InvalidName),
            "{bad:?}"
        );
        assert_eq!(
            typed_name(Side::Local, bad).err(),
            Some(FilesError::InvalidName),
            "{bad:?}"
        );
    }
    let name = typed_name(Side::Remote, "report 2026.txt").expect("usable");
    assert_eq!(name.name, "report 2026.txt");
    // A trailing line break from a paste is dropped, not refused.
    assert_eq!(
        typed_name(Side::Local, "notes\n").expect("usable").name,
        "notes"
    );
}

#[tokio::test]
async fn local_operations_create_rename_without_replacing_and_delete() {
    let dir = tempfile::tempdir().expect("dir");
    let folder: PathBuf = dir.path().join("made");
    file_operation(FileOperation::LocalMakeFolder {
        path: folder.clone(),
    })
    .await
    .expect("made");
    assert!(folder.is_dir());
    std::fs::write(folder.join("x"), b"x").expect("file");
    std::fs::write(dir.path().join("taken"), b"keep").expect("taken");
    assert_eq!(
        file_operation(FileOperation::LocalRename {
            from: folder.join("x"),
            to: dir.path().join("taken"),
        })
        .await,
        Err(FilesError::Exists),
        "a rename never replaces"
    );
    assert_eq!(
        std::fs::read(dir.path().join("taken")).expect("kept"),
        b"keep"
    );
    let moved = dir.path().join("moved");
    file_operation(FileOperation::LocalRename {
        from: folder,
        to: moved.clone(),
    })
    .await
    .expect("renamed");
    file_operation(FileOperation::LocalRemove {
        path: moved.clone(),
    })
    .await
    .expect("removed");
    assert!(!moved.exists());
    assert_eq!(
        file_operation(FileOperation::LocalMakeFolder {
            path: dir.path().join("taken"),
        })
        .await,
        Err(FilesError::Exists)
    );
}

#[cfg(unix)]
#[tokio::test]
async fn a_local_delete_removes_links_never_their_targets() {
    let dir = tempfile::tempdir().expect("dir");
    let outside = dir.path().join("outside");
    std::fs::create_dir(&outside).expect("outside");
    std::fs::write(outside.join("precious"), b"keep").expect("precious");
    let doomed = dir.path().join("doomed");
    std::fs::create_dir(&doomed).expect("doomed");
    std::os::unix::fs::symlink(&outside, doomed.join("away")).expect("link inside");
    let link = dir.path().join("link");
    std::os::unix::fs::symlink(&outside, &link).expect("link");
    for path in [doomed.clone(), link.clone()] {
        file_operation(FileOperation::LocalRemove { path })
            .await
            .expect("removed");
    }
    assert!(doomed.symlink_metadata().is_err());
    assert!(link.symlink_metadata().is_err());
    assert_eq!(
        std::fs::read(outside.join("precious")).expect("kept"),
        b"keep"
    );
}

#[cfg(unix)]
#[path = "../../heimdall-sftp/tests/common/mod.rs"]
mod common;

#[cfg(unix)]
#[tokio::test]
async fn remote_operations_against_openssh() {
    let Some((_server, client)) = common::start().await else {
        return;
    };
    let client = RemoteSession::Sftp(client);
    let dir = tempfile::tempdir().expect("dir");
    let made = dir.path().join("made");
    common::step(file_operation(FileOperation::RemoteMakeFolder {
        client: client.clone(),
        path: common::remote(&made),
    }))
    .await
    .expect("made");
    assert!(made.is_dir());
    std::fs::write(made.join("x"), b"x").expect("file");
    std::fs::write(dir.path().join("taken"), b"keep").expect("taken");
    assert!(
        common::step(file_operation(FileOperation::RemoteRename {
            client: client.clone(),
            from: common::remote(&made.join("x")),
            to: common::remote(&dir.path().join("taken")),
        }))
        .await
        .is_err(),
        "a rename never replaces"
    );
    assert_eq!(
        std::fs::read(dir.path().join("taken")).expect("kept"),
        b"keep"
    );
    let moved = dir.path().join("moved");
    common::step(file_operation(FileOperation::RemoteRename {
        client: client.clone(),
        from: common::remote(&made),
        to: common::remote(&moved),
    }))
    .await
    .expect("renamed");
    common::step(file_operation(FileOperation::RemoteRemove {
        client,
        path: common::remote(&moved),
    }))
    .await
    .expect("removed");
    assert!(!moved.exists());
}

fn key(app: &mut App, tab: TabId, key: FilesKey) -> Vec<Effect> {
    files(app, FilesMessage::Key { tab, key })
}

fn selected(app: &App, tab: TabId, side: Side) -> Option<usize> {
    let files = app.tab(tab).and_then(|t| t.files.as_ref()).expect("files");
    match side {
        Side::Remote => files.remote.selected,
        Side::Local => files.local.selected,
    }
}

#[tokio::test]
async fn arrows_walk_the_focused_pane_and_stop_at_its_ends() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut app, tab) = tab(dir.path()).await;
    // The local pane has the focus first; nothing is selected.
    key(&mut app, tab, FilesKey::Next);
    assert_eq!(selected(&app, tab, Side::Local), Some(0));
    key(&mut app, tab, FilesKey::Next);
    key(&mut app, tab, FilesKey::Next);
    assert_eq!(
        selected(&app, tab, Side::Local),
        Some(1),
        "stops at the last"
    );
    key(&mut app, tab, FilesKey::Previous);
    key(&mut app, tab, FilesKey::Previous);
    assert_eq!(
        selected(&app, tab, Side::Local),
        Some(0),
        "stops at the first"
    );
    key(&mut app, tab, FilesKey::Last);
    assert_eq!(selected(&app, tab, Side::Local), Some(1));
    key(&mut app, tab, FilesKey::First);
    assert_eq!(selected(&app, tab, Side::Local), Some(0));
    assert_eq!(
        selected(&app, tab, Side::Remote),
        None,
        "the other pane is untouched"
    );

    // Up with nothing selected starts from the bottom.
    key(&mut app, tab, FilesKey::SwitchPane);
    key(&mut app, tab, FilesKey::Previous);
    assert_eq!(selected(&app, tab, Side::Remote), Some(1));
    key(&mut app, tab, FilesKey::Focus(Side::Local));
    key(&mut app, tab, FilesKey::Next);
    assert_eq!(selected(&app, tab, Side::Local), Some(1));
}

#[tokio::test]
async fn a_click_gives_its_pane_the_focus() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut app, tab) = tab(dir.path()).await;
    select(&mut app, tab, Side::Remote, 1);
    key(&mut app, tab, FilesKey::Previous);
    assert_eq!(selected(&app, tab, Side::Remote), Some(0));
    assert_eq!(selected(&app, tab, Side::Local), None);
}

#[tokio::test]
async fn enter_opens_a_folder_and_sends_a_file() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut app, tab) = tab(dir.path()).await;
    assert!(
        key(&mut app, tab, FilesKey::Open).is_empty(),
        "nothing selected"
    );
    key(&mut app, tab, FilesKey::Focus(Side::Remote));
    key(&mut app, tab, FilesKey::Next);
    let opened = key(&mut app, tab, FilesKey::Open);
    assert!(
        matches!(opened.as_slice(), [Effect::ListRemote { path, .. }]
            if path.as_bytes() == b"/srv/logs"),
        "{opened:?}"
    );
    key(&mut app, tab, FilesKey::Focus(Side::Local));
    key(&mut app, tab, FilesKey::Last);
    let sent = key(&mut app, tab, FilesKey::Open);
    assert_eq!(plan_request(&sent).direction, Direction::Upload);
}

#[tokio::test]
async fn backspace_refresh_rename_and_delete_act_on_the_focused_pane() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut app, tab) = tab(dir.path()).await;
    key(&mut app, tab, FilesKey::Focus(Side::Remote));
    let up = key(&mut app, tab, FilesKey::Parent);
    assert!(
        matches!(up.as_slice(), [Effect::ListRemote { path, .. }] if path.as_bytes() == b"/"),
        "{up:?}"
    );
    let refreshed = key(&mut app, tab, FilesKey::Refresh);
    assert!(matches!(refreshed.as_slice(), [Effect::ListRemote { .. }]));

    key(&mut app, tab, FilesKey::Focus(Side::Local));
    key(&mut app, tab, FilesKey::Last);
    key(&mut app, tab, FilesKey::Rename);
    assert!(matches!(
        &app.dialog,
        Some(Dialog::AskName { side: Side::Local, value, .. }) if value == "b.txt"
    ));
    // Keys never act behind a question.
    assert!(key(&mut app, tab, FilesKey::Delete).is_empty());
    assert!(matches!(&app.dialog, Some(Dialog::AskName { .. })));
    app.update(Message::DismissDialog);
    key(&mut app, tab, FilesKey::Delete);
    assert!(matches!(
        &app.dialog,
        Some(Dialog::ConfirmDelete { side: Side::Local, name, .. }) if name == "b.txt"
    ));
}

#[tokio::test]
async fn a_pane_button_gives_its_pane_the_focus() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut app, tab) = tab(dir.path()).await;
    files(
        &mut app,
        FilesMessage::Refresh {
            tab,
            side: Side::Remote,
        },
    );
    key(&mut app, tab, FilesKey::Next);
    assert_eq!(selected(&app, tab, Side::Remote), Some(0));
    files(
        &mut app,
        FilesMessage::AskNewFolder {
            tab,
            side: Side::Local,
        },
    );
    app.update(Message::DismissDialog);
    key(&mut app, tab, FilesKey::Next);
    assert_eq!(selected(&app, tab, Side::Local), Some(0));
}

/// A transfer's last count is its whole size whatever the throttle let through, and a cancel
/// ends it as cancelled, not failed: a download cancelled can be started again and resumes.
#[cfg(unix)]
#[tokio::test]
async fn a_transfer_ends_on_its_full_count_and_a_cancel_is_not_a_failure() {
    use heimdall_app::files::{TransferEvent, TransferRequest, TransferState, transfer_events};
    use tokio_stream::StreamExt as _;
    use tokio_util::sync::CancellationToken;

    let Some((_server, client)) = common::start().await else {
        return;
    };
    let client = RemoteSession::Sftp(client);
    let dir = tempfile::tempdir().expect("dir");
    let source = dir.path().join("large");
    // Many chunks: the throttle drops most counts, the last among them.
    let size = 8 * 1024 * 1024;
    std::fs::write(&source, common::pattern(size)).expect("source");
    let request = |target: &str, cancel: CancellationToken| TransferRequest {
        client: client.clone(),
        direction: Direction::Download,
        remote: common::remote(&source),
        local: dir.path().join(target),
        replace: false,
        folder: false,
        steps: Vec::new(),
        left_out: 0,
        cancel,
    };

    let events: Vec<TransferEvent> =
        common::step(transfer_events(request("copy", CancellationToken::new())).collect()).await;
    let (last, rest) = events.split_last().expect("events");
    assert_eq!(*last, TransferEvent::Finished(TransferState::Done));
    assert_eq!(
        rest.last(),
        Some(&TransferEvent::Progress(u64::try_from(size).expect("size")))
    );

    let cancelled = CancellationToken::new();
    cancelled.cancel();
    let events: Vec<TransferEvent> =
        common::step(transfer_events(request("again", cancelled)).collect()).await;
    assert_eq!(
        events.last(),
        Some(&TransferEvent::Finished(TransferState::Cancelled))
    );
}

/// A folder sent with an entry left out ends incomplete, with the count, not done.
#[cfg(unix)]
#[tokio::test]
async fn a_folder_with_a_link_ends_incomplete() {
    use heimdall_app::files::{TransferEvent, TransferRequest, TransferState, transfer_events};
    use tokio_stream::StreamExt as _;
    use tokio_util::sync::CancellationToken;

    let Some((_server, client)) = common::start().await else {
        return;
    };
    let dir = tempfile::tempdir().expect("dir");
    let folder = dir.path().join("folder");
    std::fs::create_dir(&folder).expect("folder");
    std::fs::write(folder.join("kept"), b"kept").expect("file");
    std::os::unix::fs::symlink("kept", folder.join("link")).expect("link");
    let client = RemoteSession::Sftp(client);
    let root = heimdall_files::Root {
        remote: common::remote(&folder),
        local: dir.path().join("copy"),
        kind: Kind::Folder,
        stamp: heimdall_files::Stamp::default(),
    };
    let plan =
        common::step(client.plan_download(std::slice::from_ref(&root), &CancellationToken::new()))
            .await
            .expect("planned");
    let steps = plan.resolve(&[]).expect("resolved").remove(0);
    let request = TransferRequest {
        client,
        direction: Direction::Download,
        remote: root.remote,
        local: root.local,
        replace: false,
        folder: true,
        steps,
        left_out: plan.left_out(0),
        cancel: CancellationToken::new(),
    };
    let events: Vec<TransferEvent> = common::step(transfer_events(request).collect()).await;
    assert_eq!(
        events.last(),
        Some(&TransferEvent::Finished(TransferState::Incomplete {
            skipped: 1
        }))
    );
    assert_eq!(
        std::fs::read(dir.path().join("copy").join("kept")).expect("copied"),
        b"kept"
    );
}

#[tokio::test]
async fn a_servers_entry_gets_new_permissions_typed_in_octal() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut app, tab) = tab(dir.path()).await;
    files(
        &mut app,
        FilesMessage::RemoteListed {
            tab,
            result: Ok((
                RemotePath::from("/srv"),
                vec![RemoteEntry {
                    name: b"run.sh".to_vec(),
                    label: "run.sh".to_owned(),
                    kind: EntryKind::File,
                    size: Some(1),
                    modified: None,
                    permissions: Some(0o755),
                    owner: Some(1000),
                    group: Some(50),
                }],
            )),
        },
    );
    let ask = |app: &mut App, side| files(app, FilesMessage::AskPermissions { tab, side });
    assert!(ask(&mut app, Side::Remote).is_empty());
    assert_eq!(app.dialog, None, "nothing selected");
    select(&mut app, tab, Side::Remote, 0);
    ask(&mut app, Side::Remote);
    assert!(matches!(
        &app.dialog,
        Some(Dialog::AskName { action: NameAction::Permissions, value, .. }) if value == "755"
    ));
    files(&mut app, FilesMessage::NameEdited(" 4750 ".to_owned()));
    match operation(&app.update(Message::ConfirmDialog)) {
        FileOperation::RemoteSetPermissions { path, mode, .. } => {
            assert_eq!(path.as_bytes(), b"/srv/run.sh");
            assert_eq!(mode, 0o4750);
        }
        other => panic!("{other:?}"),
    }

    ask(&mut app, Side::Remote);
    files(&mut app, FilesMessage::NameEdited("rwx".to_owned()));
    assert!(app.update(Message::ConfirmDialog).is_empty());
    assert_eq!(
        pane_error(&app, tab, Side::Remote),
        Some(FilesError::InvalidPermissions)
    );

    // Not on this computer's side, as in the C# tab.
    select(&mut app, tab, Side::Local, 1);
    ask(&mut app, Side::Local);
    assert_eq!(app.dialog, None);
}

#[tokio::test]
async fn a_servers_entry_shows_its_properties() {
    use heimdall_app::files::FileProperties;

    let dir = tempfile::tempdir().expect("dir");
    let (mut app, tab) = tab(dir.path()).await;
    let show = |app: &mut App, side| files(app, FilesMessage::ShowProperties { tab, side });
    show(&mut app, Side::Remote);
    assert_eq!(app.dialog, None, "nothing selected");
    select(&mut app, tab, Side::Remote, 1);
    show(&mut app, Side::Remote);
    assert_eq!(
        app.dialog,
        Some(Dialog::FileProperties(Box::new(FileProperties {
            name: "a.txt".to_owned(),
            kind: EntryKind::File,
            size: Some(4096),
            modified: None,
            permissions: None,
            owner: None,
            group: None,
            path: "/srv/a.txt".to_owned(),
        })))
    );
    app.update(Message::ConfirmDialog);
    assert_eq!(app.dialog, None, "Enter closes it");
    select(&mut app, tab, Side::Local, 1);
    show(&mut app, Side::Local);
    assert_eq!(app.dialog, None, "the server's entries only");
}

#[test]
fn octal_permissions_are_one_to_four_octal_digits() {
    use heimdall_app::files::octal_mode;

    assert_eq!(octal_mode("755"), Ok(0o755));
    assert_eq!(octal_mode(" 0644 "), Ok(0o644));
    assert_eq!(octal_mode("7"), Ok(0o7));
    assert_eq!(octal_mode("1777"), Ok(0o1777));
    for refused in [
        "", "  ", "8", "759", "12345", "rwx", "-755", "+755", "0x1ff",
    ] {
        assert_eq!(
            octal_mode(refused),
            Err(FilesError::InvalidPermissions),
            "{refused:?}"
        );
    }
}

fn toggle(app: &mut App, tab: TabId, side: Side, index: usize) {
    files(app, FilesMessage::Toggle { tab, side, index });
}

#[tokio::test]
async fn entries_selected_together_go_together() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut app, tab) = tab(dir.path()).await;
    // Server: logs/, a.txt.
    toggle(&mut app, tab, Side::Remote, 0);
    toggle(&mut app, tab, Side::Remote, 1);
    let effects = files(
        &mut app,
        FilesMessage::Transfer {
            tab,
            direction: Direction::Download,
        },
    );
    let sent: Vec<&[u8]> = plan_request(&effects)
        .roots
        .iter()
        .map(|root| root.root.remote.as_bytes())
        .collect();
    assert_eq!(sent, [&b"/srv/logs"[..], b"/srv/a.txt"]);

    // Renaming is for one entry.
    files(
        &mut app,
        FilesMessage::AskRename {
            tab,
            side: Side::Remote,
        },
    );
    assert_eq!(app.dialog, None);

    files(
        &mut app,
        FilesMessage::AskDelete {
            tab,
            side: Side::Remote,
        },
    );
    assert!(matches!(
        &app.dialog,
        Some(Dialog::ConfirmDelete { count: 2, folder: true, name, .. }) if name == "logs"
    ));
    // One after the other, as the C# deletes them.
    let removed = run_batch(&mut app, tab, app_confirm());
    assert_eq!(removed, [b"/srv/logs".to_vec(), b"/srv/a.txt".to_vec()]);

    files(
        &mut app,
        FilesMessage::AskPermissions {
            tab,
            side: Side::Remote,
        },
    );
    files(&mut app, FilesMessage::NameEdited("700".to_owned()));
    let changed = run_batch(&mut app, tab, app_confirm());
    assert_eq!(changed.len(), 2, "both, the same bits");

    // A plain click leaves one selected.
    select(&mut app, tab, Side::Remote, 1);
    files(
        &mut app,
        FilesMessage::AskDelete {
            tab,
            side: Side::Remote,
        },
    );
    assert!(matches!(
        &app.dialog,
        Some(Dialog::ConfirmDelete {
            count: 1,
            folder: false,
            ..
        })
    ));
}

#[tokio::test]
async fn transfers_in_the_way_are_asked_one_after_the_other_and_cancel_drops_only_one() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut app, tab) = tab(dir.path()).await;
    // The server's a.txt is here too.
    std::fs::write(dir.path().join("a.txt"), b"mine").expect("local file");
    let asked = |app: &App| match &app.dialog {
        Some(Dialog::FileConflicts { rows, .. }) => rows.first().map(|row| row.target.clone()),
        _ => None,
    };
    select(&mut app, tab, Side::Remote, 1);
    let download = |app: &mut App| {
        files(
            app,
            FilesMessage::Transfer {
                tab,
                direction: Direction::Download,
            },
        )
    };
    let first = download(&mut app);
    assert!(planned(&mut app, first).await.is_empty(), "asked first");
    let second = download(&mut app);
    assert!(
        second.is_empty(),
        "waits its turn: planned once the first is answered"
    );
    assert_eq!(asked(&app).as_deref(), Some("a.txt"));
    // Cancelled: that one goes nowhere, the next is planned and asked.
    let next = app.update(Message::DismissDialog);
    assert!(planned(&mut app, next).await.is_empty());
    assert_eq!(asked(&app).as_deref(), Some("a.txt"), "then the next");
    let started = app.update(Message::ConfirmDialog);
    assert!(
        matches!(started.as_slice(), [Effect::Transfer { request, .. }] if !request.replace),
        "{started:?}"
    );
    assert_eq!(asked(&app), None, "nothing left waiting");
    assert!(app.update(Message::ConfirmDialog).is_empty());
}

#[tokio::test]
async fn what_explorer_drops_goes_to_the_servers_folder_shown() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut app, tab) = tab(dir.path()).await;
    let outside = tempfile::tempdir().expect("dir");
    let file = outside.path().join("report.pdf");
    std::fs::write(&file, b"12345").expect("written");
    let folder = outside.path().join("photos");
    std::fs::create_dir(&folder).expect("folder");
    let drop = |app: &mut App, path: &Path| {
        files(
            app,
            FilesMessage::Dropped {
                tab,
                path: path.to_owned(),
            },
        )
    };
    let dropped = drop(&mut app, &file);
    let sent = planned(&mut app, dropped).await;
    assert!(
        matches!(sent.as_slice(), [Effect::Transfer { request, .. }]
            if request.remote.as_bytes() == b"/srv/report.pdf"
                && request.local == file
                && !request.folder
                && !request.replace),
        "{sent:?}"
    );
    let files_pane = app.tab(tab).expect("tab").files.as_ref().expect("files");
    assert_eq!(files_pane.transfers.last().map(|t| t.total), Some(Some(5)));
    // Ended before the next starts: one runs at a time.
    if let [Effect::Transfer { id, .. }] = sent.as_slice() {
        files(
            &mut app,
            FilesMessage::TransferEvent {
                tab,
                id: *id,
                event: heimdall_app::files::TransferEvent::Finished(
                    heimdall_app::files::TransferState::Done,
                ),
            },
        );
    }

    let dropped = drop(&mut app, &folder);
    let sent = planned(&mut app, dropped).await;
    assert!(
        matches!(sent.as_slice(), [Effect::Transfer { request, .. }] if request.folder),
        "{sent:?}"
    );
    assert!(
        drop(&mut app, &outside.path().join("gone")).is_empty(),
        "nothing there"
    );
    let files_pane = app.tab(tab).expect("tab").files.as_ref().expect("files");
    assert!(matches!(
        files_pane.transfers.last().map(|t| &t.state),
        Some(heimdall_app::files::TransferState::Failed(
            FilesError::NotAFile
        ))
    ));
}

#[tokio::test]
async fn cut_entries_are_pasted_by_a_move_into_the_folder_shown() {
    use heimdall_app::Notice;

    let dir = tempfile::tempdir().expect("dir");
    let (mut app, tab) = tab(dir.path()).await;
    assert!(files(&mut app, FilesMessage::Cut { tab }).is_empty());
    assert!(!app.can_paste(tab), "nothing selected, nothing cut");
    select(&mut app, tab, Side::Remote, 1);
    assert!(files(&mut app, FilesMessage::Cut { tab }).is_empty());
    assert_eq!(app.notice(), Some(&Notice::FilesCut(1)));
    assert!(app.can_paste(tab));

    // Pasted where it is already: nothing moves, and the paste is done.
    assert!(files(&mut app, FilesMessage::Paste { tab }).is_empty());
    assert_eq!(app.notice(), Some(&Notice::FilesPasted));
    assert!(!app.can_paste(tab));

    select(&mut app, tab, Side::Remote, 1);
    files(&mut app, FilesMessage::Cut { tab });
    files(
        &mut app,
        FilesMessage::RemoteListed {
            tab,
            result: Ok((RemotePath::from("/srv/logs"), Vec::new())),
        },
    );
    let effects = files(&mut app, FilesMessage::Paste { tab });
    let moves = match effects.as_slice() {
        [Effect::MoveRemote { moves, .. }] => moves.clone(),
        other => panic!("expected a move, got {other:?}"),
    };
    assert_eq!(
        moves,
        vec![(
            RemotePath::from("/srv/a.txt"),
            RemotePath::from("/srv/logs/a.txt")
        )]
    );

    // A failure keeps the entry to paste again, and says why on the server's pane.
    let failed = files(
        &mut app,
        FilesMessage::Moved {
            tab,
            results: vec![(RemotePath::from("/srv/a.txt"), Err(FilesError::IsLink))],
        },
    );
    assert!(matches!(failed.as_slice(), [Effect::ListRemote { .. }]));
    assert_eq!(
        pane_error(&app, tab, Side::Remote),
        Some(FilesError::IsLink)
    );
    assert!(app.can_paste(tab));

    let done = files(
        &mut app,
        FilesMessage::Moved {
            tab,
            results: vec![(RemotePath::from("/srv/a.txt"), Ok(()))],
        },
    );
    assert!(matches!(done.as_slice(), [Effect::ListRemote { .. }]));
    assert_eq!(app.notice(), Some(&Notice::FilesPasted));
    assert!(!app.can_paste(tab), "everything cut was moved");
}

#[tokio::test]
async fn ctrl_c_copies_the_path_of_the_focused_panes_entry() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut app, tab) = tab(dir.path()).await;
    assert!(
        key(&mut app, tab, FilesKey::CopyPath).is_empty(),
        "nothing selected"
    );
    select(&mut app, tab, Side::Remote, 1);
    let effects = key(&mut app, tab, FilesKey::CopyPath);
    assert!(
        matches!(effects.as_slice(), [Effect::WriteClipboard(text)] if text == "/srv/a.txt"),
        "{effects:?}"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn a_move_goes_on_past_one_that_fails_and_never_replaces() {
    use heimdall_app::files::move_remote;

    let Some((_server, client)) = common::start().await else {
        return;
    };
    let client = RemoteSession::Sftp(client);
    let dir = tempfile::tempdir().expect("dir");
    let into = dir.path().join("into");
    std::fs::create_dir(&into).expect("folder");
    std::fs::write(dir.path().join("one"), b"one").expect("one");
    std::fs::write(dir.path().join("two"), b"two").expect("two");
    std::fs::write(into.join("one"), b"keep").expect("taken");
    let results = common::step(move_remote(
        client,
        vec![
            (
                common::remote(&dir.path().join("one")),
                common::remote(&into.join("one")),
            ),
            (
                common::remote(&dir.path().join("two")),
                common::remote(&into.join("two")),
            ),
        ],
    ))
    .await;
    assert!(results[0].1.is_err(), "a move never replaces");
    assert!(results[1].1.is_ok());
    assert_eq!(std::fs::read(into.join("one")).expect("kept"), b"keep");
    assert_eq!(std::fs::read(into.join("two")).expect("moved"), b"two");
    assert!(!dir.path().join("two").exists());
}

#[tokio::test]
async fn what_is_cut_is_pasted_only_on_the_same_server_behind_the_same_gateway() {
    let dir = tempfile::tempdir().expect("dir");
    let profiles_file = dir.path().join("profiles.toml");
    let mut store = ProfileStore::open(&profiles_file).expect("store");
    let profile = |id: &str, gateway: Option<&str>| SshProfile {
        id: ProfileId::new(id),
        name: format!("server {id}"),
        group: None,
        host: "a.lab".to_owned(),
        port: 22,
        username: Some("admin".to_owned()),
        key_path: None,
        gateway: gateway.map(ProfileId::new),
        vault_entry: None,
        forwards: heimdall_core::profile::Forwards::default(),
        post_connect: heimdall_core::post_connect::PostConnect::default(),
        forward_agent: false,
        compression: false,
        sftp: false,
        legacy_algorithms: false,
        session_logging: None,
    };
    store.merge([
        profile("direct", None),
        profile("again", None),
        profile("behind", Some("gw")),
    ]);
    store.merge_gateways([heimdall_core::profile::SshGateway {
        id: ProfileId::new("gw"),
        name: "GW".to_owned(),
        host: "gw.lab".to_owned(),
        port: 22,
        username: Some("ops".to_owned()),
        key_path: None,
        parent: None,
    }]);
    store.save().expect("save");
    let mut app = App::new(AppConfig {
        profiles_file,
        known_hosts: dir.path().join("known_hosts"),
        legacy_dir: None,
        agent: AgentSource::Disabled,
        initial_grid: GridSize { cols: 80, rows: 24 },
        files_start: dir.path().to_owned(),
        system_credentials: heimdall_app::SystemCredentials::memory(),
    });
    let open = async |app: &mut App, id: &str| {
        let (tab, attempt) = match app
            .update(Message::OpenFiles(ProfileId::new(id)))
            .as_slice()
        {
            [Effect::Connect { tab, attempt, .. }, ..] => (*tab, *attempt),
            other => panic!("{other:?}"),
        };
        app.update(Message::Connection {
            tab,
            attempt,
            event: ConnectionEvent::FilesReady {
                client: idle_client().await,
                shell: None,
            },
        });
        files(
            app,
            FilesMessage::RemoteListed {
                tab,
                result: Ok((
                    RemotePath::from("/srv"),
                    vec![RemoteEntry {
                        name: b"a.txt".to_vec(),
                        label: "a.txt".to_owned(),
                        kind: EntryKind::File,
                        size: Some(1),
                        modified: None,
                        permissions: None,
                        owner: None,
                        group: None,
                    }],
                )),
            },
        );
        tab
    };
    let direct = open(&mut app, "direct").await;
    let again = open(&mut app, "again").await;
    let behind = open(&mut app, "behind").await;
    select(&mut app, direct, Side::Remote, 0);
    files(&mut app, FilesMessage::Cut { tab: direct });
    assert!(app.can_paste(again), "the same server, another tab");
    assert!(
        !app.can_paste(behind),
        "the same address behind a gateway can be another machine"
    );
    assert!(files(&mut app, FilesMessage::Paste { tab: behind }).is_empty());
}

/// A Files tab editing the server's "/srv/a.txt", opened as `EditExternal` asks: the app, the
/// tab, the local copy and the fingerprint it was opened with.
async fn editing(dir: &Path) -> (App, TabId, PathBuf, heimdall_files::Fingerprint) {
    use heimdall_app::Notice;
    use heimdall_app::external_edit::EditSession;

    let (mut app, tab) = tab(dir).await;
    app.set_edit_dir(dir.join("edits"));
    select(&mut app, tab, Side::Remote, 0);
    assert!(
        files(&mut app, FilesMessage::EditExternal { tab }).is_empty(),
        "a folder is not edited"
    );
    select(&mut app, tab, Side::Remote, 1);
    let started = files(&mut app, FilesMessage::EditExternal { tab });
    assert!(
        matches!(started.as_slice(), [Effect::StartEdit { remote, .. }] if remote.as_bytes() == b"/srv/a.txt"),
        "{started:?}"
    );
    let local = dir.join("edits").join("a.txt");
    let fingerprint = heimdall_files::Fingerprint {
        size: Some(1),
        modified: Some(1),
        permissions: Some(0o100_644),
        uid_gid: Some((1000, 1000)),
    };
    files(
        &mut app,
        FilesMessage::EditStarted {
            tab,
            result: Ok(Box::new(EditSession {
                remote: RemotePath::from("/srv/a.txt"),
                name: "a.txt".to_owned(),
                local: local.clone(),
                sent: [0; 32],
                fingerprint,
                seen: None,
                candidate: None,
                refused: None,
                privileged: false,
            })),
        },
    );
    assert_eq!(
        app.notice(),
        Some(&Notice::FilesEditing("a.txt".to_owned()))
    );
    assert!(app.has_edits());
    (app, tab, local, fingerprint)
}

#[tokio::test]
async fn a_reconnected_tab_goes_on_watching_its_files_in_an_external_editor() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut app, tab, local, _) = editing(dir.path()).await;
    app.update(Message::ReconnectTab(tab));
    let reopened = app.active.expect("a tab shown");
    assert_ne!(reopened, tab, "a new connection");
    let watched: Vec<_> = app
        .tab(reopened)
        .and_then(|found| found.files.as_deref())
        .map(|files| files.edits.iter().map(|edit| edit.local.clone()).collect())
        .unwrap_or_default();
    assert_eq!(watched, [local], "its saves are still sent");
    assert!(app.has_edits());
}

#[tokio::test]
async fn a_file_edited_externally_is_watched_one_look_at_a_time_and_its_saves_said() {
    use heimdall_app::Notice;
    use heimdall_app::external_edit::EditCheck;

    let dir = tempfile::tempdir().expect("dir");
    let (mut app, tab, local, fingerprint) = editing(dir.path()).await;
    let tick = |app: &mut App| files(app, FilesMessage::EditTick);
    assert!(
        matches!(tick(&mut app).as_slice(), [Effect::CheckEdits { edits, .. }] if edits.len() == 1)
    );
    assert!(tick(&mut app).is_empty(), "one look at a time");
    let now = std::time::SystemTime::now();
    let sent = files(
        &mut app,
        FilesMessage::EditsChecked {
            tab,
            results: vec![(
                local,
                EditCheck::Sent {
                    modified: now,
                    sent: [1; 32],
                    fingerprint,
                },
            )],
        },
    );
    assert!(
        matches!(sent.as_slice(), [Effect::ListRemote { .. }]),
        "listed again"
    );
    assert_eq!(
        app.notice(),
        Some(&Notice::FilesAutoUploaded("a.txt".to_owned()))
    );
}

#[tokio::test]
async fn a_refused_save_is_said_on_the_pane_and_the_editor_reopens_without_a_new_copy() {
    use heimdall_app::external_edit::EditCheck;
    use heimdall_app::{Notice, SettingsMessage};

    let dir = tempfile::tempdir().expect("dir");
    let (mut app, tab, local, _) = editing(dir.path()).await;
    let tick = |app: &mut App| files(app, FilesMessage::EditTick);
    let edit = |app: &mut App| files(app, FilesMessage::EditExternal { tab });
    let now = std::time::SystemTime::now();
    assert_eq!(tick(&mut app).len(), 1, "looked at");
    files(
        &mut app,
        FilesMessage::EditsChecked {
            tab,
            results: vec![(
                local,
                EditCheck::Refused {
                    modified: now,
                    error: FilesError::ChangedOnServer,
                },
            )],
        },
    );
    assert_eq!(
        pane_error(&app, tab, Side::Remote),
        Some(FilesError::ChangedOnServer)
    );
    assert!(matches!(
        app.notice(),
        Some(Notice::FilesAutoUploadRefused {
            error: FilesError::ChangedOnServer,
            ..
        })
    ));

    // Edited already: the editor opens it again, nothing copied anew.
    assert!(matches!(
        edit(&mut app).as_slice(),
        [Effect::LaunchEditor { .. }]
    ));

    // A shell set as the editor is refused before anything runs.
    if cfg!(unix) {
        app.update(Message::Settings(SettingsMessage::ExternalEditor(
            "/bin/sh".to_owned(),
        )));
        assert!(edit(&mut app).is_empty());
        assert_eq!(
            pane_error(&app, tab, Side::Remote),
            Some(FilesError::EditorRunsFiles)
        );
    }
}

#[tokio::test]
async fn open_in_terminal_opens_a_shell_of_the_profile_in_the_folder_chosen() {
    use heimdall_app::Purpose;

    let dir = tempfile::tempdir().expect("dir");
    let (mut app, tab) = tab(dir.path()).await;
    let typed = |effects: &[Effect]| match effects {
        [Effect::Connect { request, .. }] => {
            assert_eq!(request.purpose, Purpose::Shell);
            request
                .profile
                .post_connect
                .to_run()
                .into_iter()
                .map(|step| step.input)
                .collect::<Vec<_>>()
        }
        other => panic!("expected a shell, got {other:?}"),
    };
    let open = |app: &mut App| files(app, FilesMessage::OpenInTerminal { tab });

    assert_eq!(typed(&open(&mut app)), ["cd -- '/srv'"], "the folder shown");
    assert_eq!(
        app.active,
        app.tabs.last().map(|t| t.id),
        "the shell is shown"
    );
    select(&mut app, tab, Side::Remote, 0);
    assert_eq!(
        typed(&open(&mut app)),
        ["cd -- '/srv/logs'"],
        "the folder selected"
    );
    select(&mut app, tab, Side::Remote, 1);
    assert_eq!(
        typed(&open(&mut app)),
        ["cd -- '/srv'"],
        "a file: its folder"
    );

    files(
        &mut app,
        FilesMessage::RemoteListed {
            tab,
            result: Ok((RemotePath::from("/srv/it's"), Vec::new())),
        },
    );
    assert_eq!(
        typed(&open(&mut app)),
        [r"cd -- '/srv/it'\''s'"],
        "quoted for sh"
    );
    files(
        &mut app,
        FilesMessage::RemoteListed {
            tab,
            result: Ok((RemotePath::from("/srv/a\nrm -rf ~"), Vec::new())),
        },
    );
    assert!(
        open(&mut app).is_empty(),
        "a line a shell cannot carry as it is"
    );
}

#[tokio::test]
async fn closing_a_tab_with_files_in_an_external_editor_asks_first() {
    use heimdall_app::Dialog;

    let dir = tempfile::tempdir().expect("dir");
    let (mut app, tab, _, _) = editing(dir.path()).await;
    app.update(Message::RequestCloseTab(tab));
    assert!(
        matches!(&app.dialog, Some(Dialog::ConfirmCloseEdits { tab: asked, .. }) if *asked == tab),
        "{:?}",
        app.dialog
    );
    assert!(app.tab(tab).is_some(), "still open until answered");
}

#[tokio::test]
async fn a_refused_save_can_be_sent_anyway_and_an_edit_stopped() {
    use heimdall_app::external_edit::EditCheck;

    let dir = tempfile::tempdir().expect("dir");
    let (mut app, tab, local, fingerprint) = editing(dir.path()).await;
    let now = std::time::SystemTime::now();
    let refused = EditCheck::Refused {
        modified: now,
        error: FilesError::ChangedOnServer,
    };
    files(&mut app, FilesMessage::EditTick);
    files(
        &mut app,
        FilesMessage::EditsChecked {
            tab,
            results: vec![(local.clone(), refused)],
        },
    );
    let edit = |app: &App| {
        app.tab(tab)
            .and_then(|t| t.files.as_ref())
            .and_then(|files| files.edits.first().cloned())
    };
    assert_eq!(
        edit(&app).and_then(|edit| edit.refused),
        Some(FilesError::ChangedOnServer),
        "the refusal is kept to be shown"
    );

    let sending = files(
        &mut app,
        FilesMessage::EditSendAnyway {
            tab,
            local: local.clone(),
        },
    );
    assert!(matches!(
        sending.as_slice(),
        [Effect::SendEditAnyway { .. }]
    ));
    files(
        &mut app,
        FilesMessage::EditSentAnyway {
            tab,
            local: local.clone(),
            check: EditCheck::Sent {
                modified: now,
                sent: [2; 32],
                fingerprint,
            },
        },
    );
    assert_eq!(edit(&app).and_then(|edit| edit.refused), None, "sent");

    let opening = files(
        &mut app,
        FilesMessage::EditOpenFolder {
            tab,
            local: local.clone(),
        },
    );
    assert!(
        matches!(opening.as_slice(), [Effect::OpenFolder { folder, .. }] if Some(folder.as_path()) == local.parent()),
        "{opening:?}"
    );

    files(&mut app, FilesMessage::EditStop { tab, local });
    assert!(!app.has_edits(), "no longer watched");
    app.update(Message::RequestCloseTab(tab));
    assert!(
        !matches!(
            app.dialog,
            Some(heimdall_app::Dialog::ConfirmCloseEdits { .. })
        ),
        "nothing left to ask about"
    );
}

#[tokio::test]
async fn upload_here_picks_files_and_sends_them_together_into_the_folder_shown() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut app, tab) = tab(dir.path()).await;
    assert!(matches!(
        files(&mut app, FilesMessage::UploadHere { tab }).as_slice(),
        [Effect::PickUploads { tab: asked }] if *asked == tab
    ));

    let first = dir.path().join("one.txt");
    let second = dir.path().join("two.txt");
    std::fs::write(&first, b"1").expect("one");
    std::fs::write(&second, b"22").expect("two");
    // Selected in the server's pane, a folder is not where they go: the folder shown is.
    select(&mut app, tab, Side::Remote, 0);
    let planned = files(
        &mut app,
        FilesMessage::UploadPicked {
            tab,
            paths: vec![first, second, dir.path().join("gone")],
        },
    );
    let request = plan_request(&planned);
    let targets: Vec<&[u8]> = request
        .roots
        .iter()
        .map(|root| root.root.remote.as_bytes())
        .collect();
    assert_eq!(targets, [&b"/srv/one.txt"[..], b"/srv/two.txt"]);
    assert_eq!(request.direction, Direction::Upload);
    let files_pane = app.tab(tab).and_then(|t| t.files.as_ref()).expect("files");
    assert!(
        files_pane.transfers.iter().any(|transfer| matches!(
            transfer.state,
            heimdall_app::files::TransferState::Failed(FilesError::NotAFile)
        )),
        "what is not there is said, the others still go"
    );
    assert!(
        files(
            &mut app,
            FilesMessage::UploadPicked {
                tab,
                paths: Vec::new()
            }
        )
        .is_empty(),
        "nothing picked"
    );
}

#[tokio::test]
async fn paste_from_explorer_uploads_the_files_copied_or_says_there_are_none() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut app, tab) = tab(dir.path()).await;
    assert!(matches!(
        files(&mut app, FilesMessage::PasteFromExplorer { tab }).as_slice(),
        [Effect::ReadExplorerFiles { tab: asked }] if *asked == tab
    ));
    assert!(
        files(
            &mut app,
            FilesMessage::ExplorerFilesRead {
                tab,
                paths: Vec::new()
            }
        )
        .is_empty()
    );
    assert_eq!(
        app.notice(),
        Some(&heimdall_app::Notice::ExplorerHoldsNoFiles)
    );

    let copied = dir.path().join("report.pdf");
    std::fs::write(&copied, b"%PDF").expect("copied");
    let planned = files(
        &mut app,
        FilesMessage::ExplorerFilesRead {
            tab,
            paths: vec![copied],
        },
    );
    let request = plan_request(&planned);
    assert_eq!(request.roots.len(), 1);
    assert_eq!(request.roots[0].root.remote.as_bytes(), b"/srv/report.pdf");
}

/// The fingerprint a test's server file has.
fn fingerprint(size: u64) -> heimdall_files::Fingerprint {
    heimdall_files::Fingerprint {
        size: Some(size),
        modified: Some(1),
        permissions: Some(0o100_644),
        uid_gid: Some((1000, 1000)),
    }
}

/// The integrated editor of `tab`, as the core holds it.
fn open_editor(app: &App, tab: TabId) -> Option<heimdall_app::integrated_edit::IntegratedEdit> {
    app.tab(tab)
        .and_then(|found| found.files.as_ref())
        .and_then(|files| files.editor.clone())
}

/// "/srv/a.txt" opened in the integrated editor of a Files tab, stored as `encoding`.
async fn editing_inside(
    dir: &Path,
    encoding: heimdall_app::text_codec::TextEncoding,
) -> (App, TabId, heimdall_app::EditorId) {
    let (mut app, tab) = tab(dir).await;
    select(&mut app, tab, Side::Remote, 1);
    let opened = files(&mut app, FilesMessage::EditIntegrated { tab });
    let id = match opened.as_slice() {
        [Effect::OpenEditor { id, remote, .. }] if remote.as_bytes() == b"/srv/a.txt" => *id,
        other => panic!("{other:?}"),
    };
    files(
        &mut app,
        FilesMessage::EditorOpened {
            tab,
            id,
            result: Ok((encoding, fingerprint(4))),
        },
    );
    (app, tab, id)
}

#[tokio::test]
async fn edit_opens_the_selected_file_in_place_of_the_lists() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut app, tab) = tab(dir.path()).await;
    select(&mut app, tab, Side::Remote, 0);
    assert!(files(&mut app, FilesMessage::EditIntegrated { tab }).is_empty());
    assert_eq!(
        pane_error(&app, tab, Side::Remote),
        Some(FilesError::NotAFile),
        "a folder is not edited"
    );

    select(&mut app, tab, Side::Remote, 1);
    let opened = files(&mut app, FilesMessage::EditIntegrated { tab });
    let [Effect::OpenEditor { id, .. }] = opened.as_slice() else {
        panic!("{opened:?}");
    };
    let id = *id;
    let edit = open_editor(&app, tab).expect("an editor");
    assert_eq!(
        (edit.name.as_str(), edit.opened),
        ("a.txt", None),
        "read first"
    );
    assert!(
        files(&mut app, FilesMessage::EditIntegrated { tab }).is_empty(),
        "one editor at a time"
    );
    // The lists are hidden: their keys do nothing.
    assert!(
        files(
            &mut app,
            FilesMessage::Key {
                tab,
                key: FilesKey::Delete
            }
        )
        .is_empty()
    );
    assert!(app.dialog.is_none());

    // Not text: said on the lists, the editor gone.
    files(
        &mut app,
        FilesMessage::EditorOpened {
            tab,
            id,
            result: Err(FilesError::LooksBinary),
        },
    );
    assert!(open_editor(&app, tab).is_none());
    assert_eq!(
        pane_error(&app, tab, Side::Remote),
        Some(FilesError::LooksBinary)
    );
}

#[tokio::test]
async fn a_save_goes_only_over_the_file_opened_and_says_how_it_went() {
    use heimdall_app::integrated_edit::EditorNotice;
    use heimdall_app::text_codec::{TextEncoding, Unencodable};

    let dir = tempfile::tempdir().expect("dir");
    let (mut app, tab, id) = editing_inside(dir.path(), TextEncoding::Latin1).await;
    assert_eq!(
        open_editor(&app, tab).and_then(|edit| edit.notice),
        Some(EditorNotice::Latin1)
    );
    let save = |app: &mut App, text: &str, overwrite: bool| {
        files(
            app,
            FilesMessage::EditorSave {
                tab,
                id,
                text: text.to_owned(),
                overwrite,
            },
        )
    };
    assert!(save(&mut app, "caf\u{e9} \u{20ac}", false).is_empty());
    assert_eq!(
        open_editor(&app, tab).and_then(|edit| edit.notice),
        Some(EditorNotice::Unencodable(Unencodable {
            line: 1,
            column: 6
        })),
        "never written as another character"
    );

    let sent = save(&mut app, "caf\u{e9}", false);
    let [
        Effect::SaveEditor {
            bytes, expected, ..
        },
    ] = sent.as_slice()
    else {
        panic!("{sent:?}");
    };
    assert_eq!(bytes, &[b'c', b'a', b'f', 0xE9]);
    assert_eq!(*expected, Some(fingerprint(4)), "only over the file opened");
    assert!(
        save(&mut app, "caf\u{e9}", false).is_empty(),
        "one save at a time"
    );
    files(&mut app, FilesMessage::EditorClose { tab, id });
    let edit = open_editor(&app, tab).expect("not closed while it saves");
    assert_eq!(edit.notice, Some(EditorNotice::SaveRunning));

    files(
        &mut app,
        FilesMessage::EditorSaved {
            tab,
            id,
            result: Err(FilesError::ChangedOnServer),
            dirty: true,
        },
    );
    let edit = open_editor(&app, tab).expect("kept");
    assert_eq!(edit.notice, Some(EditorNotice::ChangedOnServer));
    assert!(!edit.saving && edit.dirty);
    let overwrite = save(&mut app, "caf\u{e9}", true);
    assert!(
        matches!(
            overwrite.as_slice(),
            [Effect::SaveEditor { expected: None, .. }]
        ),
        "over whatever is there, as asked: {overwrite:?}"
    );
    files(
        &mut app,
        FilesMessage::EditorSaved {
            tab,
            id,
            result: Ok(fingerprint(9)),
            dirty: false,
        },
    );
    let edit = open_editor(&app, tab).expect("kept");
    assert_eq!(edit.notice, Some(EditorNotice::Saved));
    assert_eq!(edit.opened, Some((TextEncoding::Latin1, fingerprint(9))));
    assert!(!edit.dirty);
}

#[tokio::test]
async fn text_not_saved_is_asked_about_before_it_is_lost() {
    use heimdall_app::text_codec::TextEncoding;

    let dir = tempfile::tempdir().expect("dir");
    let (mut app, tab, id) = editing_inside(dir.path(), TextEncoding::Utf8 { bom: false }).await;
    files(&mut app, FilesMessage::EditorClose { tab, id });
    assert!(open_editor(&app, tab).is_none(), "nothing to lose: closed");

    let (mut app, tab, id) = editing_inside(dir.path(), TextEncoding::Utf8 { bom: false }).await;
    files(
        &mut app,
        FilesMessage::EditorChanged {
            tab,
            id,
            dirty: true,
        },
    );
    files(&mut app, FilesMessage::EditorClose { tab, id });
    assert!(matches!(
        &app.dialog,
        Some(Dialog::ConfirmDiscardEditor { name, .. }) if name == "a.txt"
    ));
    app.update(Message::DismissDialog);
    assert!(open_editor(&app, tab).is_some(), "kept");

    app.update(Message::RequestCloseTab(tab));
    assert!(matches!(
        &app.dialog,
        Some(Dialog::ConfirmCloseEditor { tab: asked, .. }) if *asked == tab
    ));
    app.update(Message::DismissDialog);
    app.update(Message::WindowCloseRequested);
    assert_eq!(
        app.dialog,
        Some(Dialog::ConfirmExit {
            live: 1,
            unsaved: 1
        })
    );
    app.update(Message::DismissDialog);

    // A reconnection keeps the text: saved once connected again.
    app.update(Message::ReconnectTab(tab));
    let reopened = app.active.expect("a tab shown");
    assert_eq!(open_editor(&app, reopened).map(|edit| edit.id), Some(id));

    files(&mut app, FilesMessage::EditorClose { tab: reopened, id });
    app.update(Message::ConfirmDialog);
    assert!(open_editor(&app, reopened).is_none(), "discarded as asked");
}

#[tokio::test]
async fn a_file_too_large_for_the_integrated_editor_points_to_the_external_one() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut app, tab) = tab(dir.path()).await;
    let large = RemoteEntry {
        name: b"dump.sql".to_vec(),
        label: "dump.sql".to_owned(),
        kind: EntryKind::File,
        size: Some(heimdall_app::integrated_edit::INTEGRATED_EDIT_LIMIT + 1),
        modified: None,
        permissions: None,
        owner: None,
        group: None,
    };
    files(
        &mut app,
        FilesMessage::RemoteListed {
            tab,
            result: Ok((RemotePath::from("/srv"), vec![large])),
        },
    );
    select(&mut app, tab, Side::Remote, 0);
    assert!(files(&mut app, FilesMessage::EditIntegrated { tab }).is_empty());
    assert_eq!(
        pane_error(&app, tab, Side::Remote),
        Some(FilesError::TooLargeForEditor)
    );
    assert!(
        matches!(
            files(&mut app, FilesMessage::EditExternal { tab }).as_slice(),
            [] | [Effect::StartEdit { .. }]
        ),
        "the external editor still takes it"
    );
}

#[tokio::test]
async fn a_save_answered_after_a_reconnection_reaches_the_editor_where_it_went() {
    use heimdall_app::integrated_edit::EditorNotice;
    use heimdall_app::text_codec::TextEncoding;

    let dir = tempfile::tempdir().expect("dir");
    let (mut app, tab, id) = editing_inside(dir.path(), TextEncoding::Utf8 { bom: false }).await;
    files(
        &mut app,
        FilesMessage::EditorChanged {
            tab,
            id,
            dirty: true,
        },
    );
    let sent = files(
        &mut app,
        FilesMessage::EditorSave {
            tab,
            id,
            text: "new".to_owned(),
            overwrite: false,
        },
    );
    assert!(matches!(sent.as_slice(), [Effect::SaveEditor { .. }]));
    app.update(Message::ReconnectTab(tab));
    let reopened = app.active.expect("a tab shown");
    assert_ne!(reopened, tab);

    // The answer names the tab the save started in: it reaches the editor all the same.
    files(
        &mut app,
        FilesMessage::EditorSaved {
            tab,
            id,
            result: Err(FilesError::SessionClosed),
            dirty: true,
        },
    );
    let edit = open_editor(&app, reopened).expect("moved");
    assert!(!edit.saving, "not stuck saving");
    assert_eq!(
        edit.notice,
        Some(EditorNotice::Failed(FilesError::SessionClosed))
    );
}

#[tokio::test]
async fn closing_the_editor_never_takes_the_place_of_another_question() {
    use heimdall_app::text_codec::TextEncoding;

    let dir = tempfile::tempdir().expect("dir");
    let (mut app, tab, id) = editing_inside(dir.path(), TextEncoding::Utf8 { bom: false }).await;
    files(
        &mut app,
        FilesMessage::EditorChanged {
            tab,
            id,
            dirty: true,
        },
    );
    app.update(Message::RequestCloseTab(tab));
    let asked = app.dialog.clone();
    assert!(matches!(asked, Some(Dialog::ConfirmCloseEditor { .. })));
    files(&mut app, FilesMessage::EditorClose { tab, id });
    assert_eq!(app.dialog, asked, "the question on screen stays");
}

#[tokio::test]
async fn opening_a_text_file_edits_it_and_a_binary_one_is_offered_for_download() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut app, tab) = tab(dir.path()).await;
    let open = |app: &mut App| {
        files(
            app,
            FilesMessage::Open {
                tab,
                side: Side::Remote,
                index: 1,
            },
        )
    };
    let opened = open(&mut app);
    let [Effect::OpenEditor { id, .. }] = opened.as_slice() else {
        panic!("{opened:?}");
    };
    let id = *id;
    assert!(open_editor(&app, tab).is_some_and(|edit| edit.from_open));

    // Not text: the C# "Binary file" question, its download on yes.
    files(
        &mut app,
        FilesMessage::EditorOpened {
            tab,
            id,
            result: Err(FilesError::LooksBinary),
        },
    );
    assert!(open_editor(&app, tab).is_none());
    assert!(
        matches!(&app.dialog, Some(Dialog::ConfirmDownloadBinary { name, .. }) if name == "a.txt"),
        "{:?}",
        app.dialog
    );
    assert_eq!(
        pane_error(&app, tab, Side::Remote),
        None,
        "asked, not an error"
    );
    let downloaded = app.update(Message::ConfirmDialog);
    assert_eq!(plan_request(&downloaded).direction, Direction::Download);
}

#[tokio::test]
async fn opening_a_file_too_large_to_edit_downloads_it_as_before() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut app, tab) = tab(dir.path()).await;
    let large = RemoteEntry {
        name: b"dump.sql".to_vec(),
        label: "dump.sql".to_owned(),
        kind: EntryKind::File,
        size: Some(heimdall_app::integrated_edit::INTEGRATED_EDIT_LIMIT + 1),
        modified: None,
        permissions: None,
        owner: None,
        group: None,
    };
    files(
        &mut app,
        FilesMessage::RemoteListed {
            tab,
            result: Ok((RemotePath::from("/srv"), vec![large])),
        },
    );
    let opened = files(
        &mut app,
        FilesMessage::Open {
            tab,
            side: Side::Remote,
            index: 0,
        },
    );
    assert_eq!(plan_request(&opened).direction, Direction::Download);
    assert!(open_editor(&app, tab).is_none());
}

#[tokio::test]
async fn the_csharp_file_browser_keys_hold_paste_select_and_transfer() {
    use heimdall_app::Notice;

    let dir = tempfile::tempdir().expect("dir");
    let (mut app, tab) = tab(dir.path()).await;
    // Nothing held: Ctrl+V pastes what Explorer copied.
    assert!(matches!(
        key(&mut app, tab, FilesKey::Paste).as_slice(),
        [Effect::ReadExplorerFiles { tab: asked }] if *asked == tab
    ));

    // Ctrl+A then Ctrl+X on the server's pane: every entry held.
    key(&mut app, tab, FilesKey::Focus(Side::Remote));
    key(&mut app, tab, FilesKey::SelectAll);
    let listed = app
        .tab(tab)
        .and_then(|t| t.files.as_ref())
        .map(|files| (files.remote.chosen().len(), files.remote.entries.len()))
        .expect("files");
    assert_eq!(listed.0, listed.1, "every entry");
    key(&mut app, tab, FilesKey::Cut);
    assert_eq!(app.notice(), Some(&Notice::FilesCut(listed.1)));
    assert!(app.can_paste(tab));
    // Something held: Ctrl+V pastes it, Explorer is not read.
    assert!(
        !matches!(
            key(&mut app, tab, FilesKey::Paste).as_slice(),
            [Effect::ReadExplorerFiles { .. }]
        ),
        "the entries held"
    );

    // This computer's pane holds nothing on Ctrl+C: the clipboard is the server's.
    key(&mut app, tab, FilesKey::Focus(Side::Local));
    select(&mut app, tab, Side::Local, 1);
    key(&mut app, tab, FilesKey::Focus(Side::Local));
    assert!(key(&mut app, tab, FilesKey::Copy).is_empty());

    // F7 asks a new folder's name; Ctrl+Shift+U plans the upload of what is chosen.
    key(&mut app, tab, FilesKey::NewFolder);
    assert!(
        matches!(app.dialog, Some(Dialog::AskName { .. })),
        "{:?}",
        app.dialog
    );
    app.update(Message::DismissDialog);
    assert!(matches!(
        key(&mut app, tab, FilesKey::Upload).as_slice(),
        [Effect::PlanTransfer { .. }]
    ));
}

/// Confirms the open dialog.
fn app_confirm() -> Message {
    Message::ConfirmDialog
}

/// The path the entry of a run being worked on is about, from the step `effects` start.
fn step_path(effects: &[Effect]) -> Option<Vec<u8>> {
    effects.iter().find_map(|effect| match effect {
        Effect::FileBatchStep { operation, .. } => Some(match &**operation {
            FileOperation::RemoteRemove { path, .. }
            | FileOperation::RemoteSetPermissions { path, .. } => path.as_bytes().to_vec(),
            other => panic!("{other:?}"),
        }),
        _ => None,
    })
}

/// Starts a run with `start` and has every entry go: the paths, in the order worked on.
fn run_batch(app: &mut App, tab: TabId, start: Message) -> Vec<Vec<u8>> {
    let mut paths = Vec::new();
    let mut effects = app.update(start);
    while let Some(path) = step_path(&effects) {
        paths.push(path);
        effects = files(
            app,
            FilesMessage::BatchStepDone {
                tab,
                result: Ok(()),
            },
        );
    }
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, Effect::ListRemote { .. })),
        "listed again once over: {effects:?}"
    );
    paths
}

#[tokio::test]
async fn a_run_says_where_it_is_can_be_stopped_and_sums_up_what_failed() {
    use heimdall_app::Notice;
    use heimdall_app::files::{BatchKind, BatchOutcome};

    let dir = tempfile::tempdir().expect("dir");
    let (mut app, tab) = tab(dir.path()).await;
    let batch = |app: &App| {
        app.tab(tab)
            .and_then(|tab| tab.files.as_ref())
            .and_then(|files| files.batch.as_ref())
            .map(|batch| (batch.current.clone(), batch.done, batch.total))
    };
    // Both entries of the server, marked once: the marks stay after a run.
    toggle(&mut app, tab, Side::Remote, 0);
    toggle(&mut app, tab, Side::Remote, 1);
    let ask_delete = |app: &mut App| {
        files(
            app,
            FilesMessage::AskDelete {
                tab,
                side: Side::Remote,
            },
        );
        assert!(
            matches!(app.dialog, Some(Dialog::ConfirmDelete { count: 2, .. })),
            "{:?}",
            app.dialog
        );
    };
    ask_delete(&mut app);
    assert!(step_path(&app.update(Message::ConfirmDialog)).is_some());
    assert_eq!(batch(&app), Some(("logs".to_owned(), 0, 2)));
    // Another delete waits for this one.
    files(
        &mut app,
        FilesMessage::AskDelete {
            tab,
            side: Side::Remote,
        },
    );
    assert_eq!(app.dialog, None);

    // The first fails, the second goes: the failure is summed up, named.
    let next = files(
        &mut app,
        FilesMessage::BatchStepDone {
            tab,
            result: Err(FilesError::Exists),
        },
    );
    assert_eq!(step_path(&next), Some(b"/srv/a.txt".to_vec()));
    assert_eq!(batch(&app), Some(("a.txt".to_owned(), 1, 2)));
    files(
        &mut app,
        FilesMessage::BatchStepDone {
            tab,
            result: Ok(()),
        },
    );
    assert_eq!(batch(&app), None);
    assert_eq!(
        app.notice(),
        Some(&Notice::FilesBatch(BatchOutcome::Failed {
            kind: BatchKind::Delete,
            failed: 1,
            total: 2,
            first: "logs".to_owned(),
            reason: FilesError::Exists,
        }))
    );

    // Stopped: the entry being worked on finishes, no other starts.
    ask_delete(&mut app);
    app.update(Message::ConfirmDialog);
    files(&mut app, FilesMessage::StopBatch { tab });
    let over = files(
        &mut app,
        FilesMessage::BatchStepDone {
            tab,
            result: Ok(()),
        },
    );
    assert!(step_path(&over).is_none(), "nothing more started");
    assert_eq!(
        app.notice(),
        Some(&Notice::FilesBatch(BatchOutcome::Stopped {
            kind: BatchKind::Delete,
            done: 1,
            total: 2,
        }))
    );
}
