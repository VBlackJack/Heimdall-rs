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

//! Copy, Paste and Duplicate on the server: what the Files tab asks, and, against this
//! computer's own `sh` and `sftp-server`, what a copy does.

#[path = "../../heimdall-ssh/tests/common/mod.rs"]
mod ssh;

use std::path::Path;
use std::sync::Arc;

use heimdall_app::files::{CopySource, EntryKind, FilesError, RemoteEntry, Side, copy_remote};
use heimdall_app::{App, AppConfig, ConnectionEvent, Effect, FilesMessage, Message, Notice, TabId};
use heimdall_core::profile::{ProfileId, SshProfile};
use heimdall_files::RemoteSession;
use heimdall_sftp::protocol::{Request, Response, SFTP_VERSION, StatusCode};
use heimdall_sftp::{ClientConfig, RemotePath, SftpClient};
use heimdall_ssh::{AgentSource, Connection, establish};
use heimdall_term::GridSize;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio_util::sync::CancellationToken;

/// A connection to the in-process SSH server, to run commands on.
async fn shell(server: &ssh::TestServer, dir: &Path) -> Connection {
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

/// An SFTP session refusing every request after its start.
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

fn entry(name: &str, kind: EntryKind) -> RemoteEntry {
    RemoteEntry {
        name: name.as_bytes().to_vec(),
        label: name.to_owned(),
        kind,
        size: Some(1),
        modified: None,
        permissions: None,
        owner: None,
        group: None,
    }
}

/// A Files tab with `/srv` listed, and `shell` as its SSH connection.
fn tab(dir: &Path, client: RemoteSession, shell: Option<Connection>) -> (App, TabId) {
    let profiles_file = dir.join("profiles.toml");
    let mut store = heimdall_core::store::ProfileStore::open(&profiles_file).expect("store");
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
        event: ConnectionEvent::FilesReady { client, shell },
    });
    files(
        &mut app,
        FilesMessage::RemoteListed {
            tab,
            result: Ok((
                RemotePath::from("/srv"),
                vec![
                    entry("logs", EntryKind::Directory),
                    entry("a.txt", EntryKind::File),
                    entry("link", EntryKind::Link),
                ],
            )),
        },
    );
    (app, tab)
}

fn files(app: &mut App, message: FilesMessage) -> Vec<Effect> {
    app.update(Message::Files(message))
}

fn select_all(app: &mut App, tab: TabId) {
    files(
        app,
        FilesMessage::Select {
            tab,
            side: Side::Remote,
            index: 0,
        },
    );
    for index in 1..3 {
        files(
            app,
            FilesMessage::Toggle {
                tab,
                side: Side::Remote,
                index,
            },
        );
    }
}

fn copy_of(effects: &[Effect]) -> (Vec<CopySource>, RemotePath, bool) {
    match effects {
        [
            Effect::CopyRemote {
                sources,
                folder,
                duplicate,
                ..
            },
        ] => (sources.clone(), folder.clone(), *duplicate),
        other => panic!("expected a copy, got {other:?}"),
    }
}

fn source(path: &str, folder: bool) -> CopySource {
    CopySource {
        path: RemotePath::from(path),
        folder,
    }
}

#[tokio::test]
async fn copied_entries_are_pasted_by_a_copy_and_stay_to_be_pasted_again() {
    let server = ssh::start(ssh::Spec::default()).await;
    let dir = tempfile::tempdir().expect("dir");
    let connection = shell(&server, dir.path()).await;
    let (mut app, tab) = tab(dir.path(), idle_client().await, Some(connection));
    assert!(app.can_copy(tab));
    select_all(&mut app, tab);
    assert!(files(&mut app, FilesMessage::Copy { tab }).is_empty());
    assert_eq!(
        app.notice(),
        Some(&Notice::FilesCopied(2)),
        "the link is left"
    );

    let pasted = files(&mut app, FilesMessage::Paste { tab });
    let (sources, folder, duplicate) = copy_of(&pasted);
    assert_eq!(
        sources,
        [source("/srv/logs", true), source("/srv/a.txt", false)]
    );
    assert_eq!(folder, RemotePath::from("/srv"), "a copy beside it is fine");
    assert!(!duplicate);
    assert!(
        files(&mut app, FilesMessage::Paste { tab }).is_empty(),
        "one copy at a time"
    );

    let done = files(
        &mut app,
        FilesMessage::Copied {
            tab,
            results: vec![(
                RemotePath::from("/srv/a.txt"),
                Ok(RemotePath::from("/srv/a (copy).txt")),
            )],
            duplicate: false,
        },
    );
    assert!(matches!(done.as_slice(), [Effect::ListRemote { .. }]));
    assert_eq!(app.notice(), Some(&Notice::FilesPasted));
    assert!(app.can_paste(tab), "a copy stays to be pasted again");
    copy_of(&files(&mut app, FilesMessage::Paste { tab }));
    files(
        &mut app,
        FilesMessage::Copied {
            tab,
            results: vec![(RemotePath::from("/srv/logs"), Err(FilesError::CopyRefused))],
            duplicate: false,
        },
    );
    let pane = app.tab(tab).and_then(|t| t.files.as_ref()).expect("files");
    assert_eq!(pane.remote.error, Some(FilesError::CopyRefused));

    let duplicated = files(&mut app, FilesMessage::Duplicate { tab });
    let (sources, _, duplicate) = copy_of(&duplicated);
    assert_eq!(sources.len(), 2);
    assert!(duplicate);
    files(
        &mut app,
        FilesMessage::Copied {
            tab,
            results: Vec::new(),
            duplicate: true,
        },
    );
    assert_eq!(app.notice(), Some(&Notice::FilesDuplicated));
}

#[tokio::test]
async fn a_tab_without_its_ssh_connection_neither_copies_nor_pastes_a_copy() {
    let server = ssh::start(ssh::Spec::default()).await;
    let dir = tempfile::tempdir().expect("dir");
    let connection = shell(&server, dir.path()).await;
    let (mut app, with) = tab(dir.path(), idle_client().await, Some(connection));
    select_all(&mut app, with);
    files(&mut app, FilesMessage::Copy { tab: with });

    let other = tempfile::tempdir().expect("dir");
    let (mut bare, without) = tab(other.path(), idle_client().await, None);
    assert!(!bare.can_copy(without));
    select_all(&mut bare, without);
    assert!(files(&mut bare, FilesMessage::Copy { tab: without }).is_empty());
    assert_eq!(bare.notice(), None);
    assert!(files(&mut bare, FilesMessage::Duplicate { tab: without }).is_empty());
    assert!(!bare.can_paste(without));
}

#[tokio::test]
async fn without_a_connection_to_run_on_nothing_is_copied_any_other_way() {
    let results = copy_remote(
        idle_client().await,
        None,
        vec![source("/srv/a.txt", false), source("/srv/b.txt", false)],
        RemotePath::from("/srv"),
        CancellationToken::new(),
    )
    .await;
    assert_eq!(
        results,
        [(RemotePath::from("/srv/a.txt"), Err(FilesError::CopyRefused))],
        "the first failure stops the rest"
    );
}

#[cfg(unix)]
#[path = "../../heimdall-sftp/tests/common/mod.rs"]
mod sftp;

#[cfg(unix)]
#[tokio::test]
async fn a_copy_on_the_server_keeps_modes_takes_a_free_name_and_never_copies_into_itself() {
    use std::os::unix::fs::PermissionsExt as _;

    let Some((_sftp_server, client)) = sftp::start().await else {
        return;
    };
    let client = RemoteSession::Sftp(client);
    let server = ssh::start(ssh::Spec::default()).await;
    let keys = tempfile::tempdir().expect("dir");
    let connection = shell(&server, keys.path()).await;
    let dir = tempfile::tempdir().expect("dir");
    let file = dir.path().join("a.txt");
    std::fs::write(&file, b"content").expect("file");
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o640)).expect("mode");
    let folder = dir.path().join("dir");
    std::fs::create_dir_all(folder.join("inner")).expect("tree");
    std::fs::write(folder.join("inner/f"), b"deep").expect("deep");

    let results = sftp::step(copy_remote(
        client.clone(),
        Some(connection.clone()),
        vec![
            CopySource {
                path: sftp::remote(&file),
                folder: false,
            },
            CopySource {
                path: sftp::remote(&folder),
                folder: true,
            },
        ],
        sftp::remote(dir.path()),
        CancellationToken::new(),
    ))
    .await;
    assert_eq!(
        results,
        [
            (
                sftp::remote(&file),
                Ok(sftp::remote(&dir.path().join("a (copy).txt")))
            ),
            (
                sftp::remote(&folder),
                Ok(sftp::remote(&dir.path().join("dir (copy)")))
            ),
        ]
    );
    let copy = dir.path().join("a (copy).txt");
    assert_eq!(std::fs::read(&copy).expect("copied"), b"content");
    assert_eq!(
        std::fs::metadata(&copy)
            .expect("there")
            .permissions()
            .mode()
            & 0o777,
        0o640
    );
    assert_eq!(
        std::fs::read(dir.path().join("dir (copy)/inner/f")).expect("copied"),
        b"deep"
    );

    let into_itself = sftp::step(copy_remote(
        client,
        Some(connection),
        vec![
            CopySource {
                path: sftp::remote(&folder),
                folder: true,
            },
            CopySource {
                path: sftp::remote(&file),
                folder: false,
            },
        ],
        sftp::remote(&folder.join("inner")),
        CancellationToken::new(),
    ))
    .await;
    assert_eq!(
        into_itself,
        [(
            sftp::remote(&folder),
            Err(FilesError::PasteIntoItself {
                name: "dir".to_owned()
            })
        )],
        "refused, and nothing after it"
    );
    assert!(!folder.join("inner/a.txt").exists());
}

#[tokio::test]
async fn a_file_is_opened_with_sudo_asking_its_password_once_and_forgetting_a_refused_one() {
    use heimdall_app::sudo_edit::SudoPassword;
    use heimdall_app::{Dialog, SudoAction};

    let server = ssh::start(ssh::Spec::default()).await;
    let dir = tempfile::tempdir().expect("dir");
    let connection = shell(&server, dir.path()).await;
    let (mut app, tab) = tab(dir.path(), idle_client().await, Some(connection));
    app.set_edit_dir(dir.path().join("edits"));
    files(
        &mut app,
        FilesMessage::Select {
            tab,
            side: Side::Remote,
            index: 1,
        },
    );
    let remote = RemotePath::from("/srv/a.txt");
    let opening = files(&mut app, FilesMessage::EditWithSudo { tab });
    assert!(
        matches!(opening.as_slice(), [Effect::SudoOpen { remote: asked, password: None, .. }] if *asked == remote),
        "{opening:?}"
    );
    let opened = |app: &mut App, result| {
        files(
            app,
            FilesMessage::SudoOpened {
                tab,
                remote: remote.clone(),
                result,
            },
        )
    };
    let password = |app: &App| {
        app.tab(tab)
            .and_then(|t| t.files.as_ref())
            .is_some_and(|files| files.sudo_password.is_some())
    };

    opened(&mut app, Err(FilesError::SudoPasswordNeeded));
    assert!(matches!(
        &app.dialog,
        Some(Dialog::SudoPassword { action: SudoAction::Open(asked), .. }) if *asked == remote
    ));
    // A bare confirm, Enter elsewhere, never answers it: only the password typed does.
    app.update(Message::ConfirmDialog);
    assert!(matches!(&app.dialog, Some(Dialog::SudoPassword { .. })));
    let again = files(
        &mut app,
        FilesMessage::SudoPasswordGiven {
            tab,
            password: SudoPassword::new("wrong"),
        },
    );
    assert!(matches!(
        again.as_slice(),
        [Effect::SudoOpen {
            password: Some(_),
            ..
        }]
    ));
    assert!(app.dialog.is_none() && password(&app), "kept for the tab");
    assert_eq!(
        format!("{:?}", SudoPassword::new("hunter2")),
        "SudoPassword(..)",
        "never shown"
    );

    opened(&mut app, Err(FilesError::SudoPasswordRejected));
    assert!(!password(&app), "a refused one is forgotten");
    assert!(
        matches!(&app.dialog, Some(Dialog::SudoPassword { .. })),
        "and asked again"
    );
    files(
        &mut app,
        FilesMessage::SudoPasswordGiven {
            tab,
            password: SudoPassword::new("right"),
        },
    );
}

#[tokio::test]
async fn a_file_opened_with_sudo_is_watched_with_the_password_and_its_save_said() {
    use heimdall_app::external_edit::{EditCheck, EditSession};
    use heimdall_app::sudo_edit::SudoPassword;

    let server = ssh::start(ssh::Spec::default()).await;
    let dir = tempfile::tempdir().expect("dir");
    let connection = shell(&server, dir.path()).await;
    let (mut app, tab) = tab(dir.path(), idle_client().await, Some(connection));
    app.set_edit_dir(dir.path().join("edits"));
    let remote = RemotePath::from("/srv/a.txt");
    let opened = |app: &mut App, result| {
        files(
            app,
            FilesMessage::SudoOpened {
                tab,
                remote: remote.clone(),
                result,
            },
        )
    };
    opened(&mut app, Err(FilesError::SudoPasswordNeeded));
    files(
        &mut app,
        FilesMessage::SudoPasswordGiven {
            tab,
            password: SudoPassword::new("right"),
        },
    );

    let local = dir.path().join("edits").join("a.txt");
    let session = EditSession {
        remote: remote.clone(),
        name: "a.txt".to_owned(),
        local: local.clone(),
        sent: [0; 32],
        fingerprint: heimdall_files::Fingerprint {
            size: None,
            modified: None,
            permissions: None,
            uid_gid: None,
        },
        seen: None,
        candidate: None,
        refused: None,
        privileged: true,
    };
    opened(&mut app, Ok(Box::new(session)));
    assert!(app.has_edits());
    let looked = files(&mut app, FilesMessage::EditTick);
    assert!(
        matches!(
            looked.as_slice(),
            [Effect::CheckEdits {
                shell: Some(_),
                password: Some(_),
                ..
            }]
        ),
        "a privileged edit is looked at with the connection and the password"
    );
    files(
        &mut app,
        FilesMessage::EditsChecked {
            tab,
            results: Vec::new(),
        },
    );

    // Saved with sudo from the row: said so.
    let saving = files(
        &mut app,
        FilesMessage::EditSaveWithSudo {
            tab,
            local: local.clone(),
        },
    );
    assert!(matches!(saving.as_slice(), [Effect::SudoSave { .. }]));
    files(
        &mut app,
        FilesMessage::SudoSaved {
            tab,
            local,
            check: EditCheck::Sent {
                modified: std::time::SystemTime::now(),
                sent: [1; 32],
                fingerprint: heimdall_files::Fingerprint {
                    size: None,
                    modified: None,
                    permissions: None,
                    uid_gid: None,
                },
            },
        },
    );
    assert_eq!(
        app.notice(),
        Some(&Notice::FilesSavedWithSudo("a.txt".to_owned()))
    );
}
