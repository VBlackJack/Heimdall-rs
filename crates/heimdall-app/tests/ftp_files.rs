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

//! A Files tab over FTP, against FTP servers run in the test, as the C# one: its files
//! edited in the integrated editor and with an external editor, never with sudo, and its
//! entries copied to be pasted on another server.

use std::net::{Ipv4Addr, TcpListener};
use std::path::Path;
use std::time::Duration;

use heimdall_app::files::{EntryKind, RemoteEntry, Side, copy_across};
use heimdall_app::integrated_edit::{open, save};
use heimdall_app::{App, AppConfig, ConnectionEvent, Effect, FilesMessage, Message, Notice, TabId};
use heimdall_core::profile::{FtpProfile, ProfileId};
use heimdall_files::{FtpClient, FtpSecurity, FtpTarget, RemotePath, RemoteSession};
use heimdall_ssh::AgentSource;
use heimdall_term::GridSize;
use unftp_sbe_fs::Filesystem;

/// Longest wait for the test server, or for one operation.
const STEP: Duration = Duration::from_secs(20);

/// Serves `root` over FTP on a free local port; returns the port.
async fn serve(root: &Path) -> u16 {
    let port = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .expect("free port")
        .local_addr()
        .expect("address")
        .port();
    let home = root.to_owned();
    let server = libunftp::ServerBuilder::new(Box::new(move || {
        Filesystem::new(home.clone()).expect("root")
    }))
    .build()
    .expect("server");
    tokio::spawn(server.listen(format!("127.0.0.1:{port}")));
    tokio::time::timeout(STEP, async {
        while tokio::net::TcpStream::connect((Ipv4Addr::LOCALHOST, port))
            .await
            .is_err()
        {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("started");
    port
}

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

/// An application knowing `profiles`.
fn app(dir: &Path, profiles: Vec<FtpProfile>) -> App {
    let profiles_file = dir.join("profiles.toml");
    let mut store = heimdall_core::store::ProfileStore::open(&profiles_file).expect("store");
    store.merge_ftp(profiles);
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
    app.set_edit_dir(dir.join("edits"));
    app
}

/// The FTP tab of profile `id`, connected with `client`, its server's `/` listing `entries`.
fn ftp_tab(app: &mut App, id: &str, client: RemoteSession, entries: Vec<RemoteEntry>) -> TabId {
    let effects = app.update(Message::ConnectProfile(ProfileId::new(id)));
    let [Effect::ConnectFtp { tab, attempt, .. }] = effects.as_slice() else {
        panic!("{effects:?}");
    };
    let (tab, attempt) = (*tab, *attempt);
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
            result: Ok((RemotePath::from("/"), entries)),
        },
    );
    tab
}

fn files(app: &mut App, message: FilesMessage) -> Vec<Effect> {
    app.update(Message::Files(message))
}

fn file(name: &str, size: u64) -> RemoteEntry {
    RemoteEntry {
        name: name.as_bytes().to_vec(),
        label: name.to_owned(),
        kind: EntryKind::File,
        size: Some(size),
        modified: None,
        permissions: None,
        owner: None,
        group: None,
        inode: None,
    }
}

fn select(app: &mut App, tab: TabId, index: usize) {
    files(
        app,
        FilesMessage::Select {
            tab,
            side: Side::Remote,
            index,
        },
    );
}

/// Saves over `hosts`, `remote` on the server: refused over what was `opened` once changed,
/// written over what was `saved`, and over whatever is there when asked to.
async fn saves_only_over_what_was_read(
    client: &RemoteSession,
    remote: &RemotePath,
    (opened, saved): (heimdall_files::Fingerprint, heimdall_files::Fingerprint),
    hosts: &Path,
) {
    // Saved again over what was first opened: changed since, left as it is.
    assert_eq!(
        save(
            client.clone(),
            remote.clone(),
            b"stale\n".to_vec(),
            Some(opened)
        )
        .await,
        Err(heimdall_app::files::FilesError::ChangedOnServer)
    );
    // Over what was saved, or over whatever is there when asked to.
    save(
        client.clone(),
        remote.clone(),
        b"again\n".to_vec(),
        Some(saved),
    )
    .await
    .expect("saved over what was saved");
    std::fs::write(hosts, b"changed by someone\n").expect("changed");
    save(client.clone(), remote.clone(), b"mine\n".to_vec(), None)
        .await
        .expect("written over, as asked");
    assert_eq!(std::fs::read(hosts).expect("read"), b"mine\n");
}

#[tokio::test]
async fn an_ftp_file_opens_in_the_integrated_editor_and_its_save_is_written_back() {
    let root = tempfile::tempdir().expect("root");
    let hosts = root.path().join("hosts");
    std::fs::write(&hosts, b"127.0.0.1 a\n").expect("file");
    let port = serve(root.path()).await;
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path(), vec![profile("ftp", port)]);
    let tab = ftp_tab(
        &mut app,
        "ftp",
        session(port).await,
        vec![file("hosts", 12)],
    );

    // Open, as a double click: the integrated editor, as over SFTP.
    let opening = files(
        &mut app,
        FilesMessage::Open {
            tab,
            side: Side::Remote,
            index: 0,
        },
    );
    let [
        Effect::OpenEditor {
            id, client, remote, ..
        },
    ] = opening.as_slice()
    else {
        panic!("{opening:?}");
    };
    let (id, client, remote) = (*id, client.clone(), remote.clone());
    assert_eq!(remote, RemotePath::from("/hosts"));
    let opened = tokio::time::timeout(STEP, open(client.clone(), remote.clone()))
        .await
        .expect("in time")
        .expect("opened");
    assert_eq!(opened.text, "127.0.0.1 a\n");
    files(
        &mut app,
        FilesMessage::EditorOpened {
            tab,
            id,
            result: Ok((opened.encoding, opened.fingerprint)),
        },
    );

    let saving = files(
        &mut app,
        FilesMessage::EditorSave {
            tab,
            id,
            text: "127.0.0.1 a\n10.0.0.1 b\n".to_owned(),
            overwrite: false,
        },
    );
    let [
        Effect::SaveEditor {
            client,
            remote,
            bytes,
            expected,
            ..
        },
    ] = saving.as_slice()
    else {
        panic!("{saving:?}");
    };
    let saved = tokio::time::timeout(
        STEP,
        save(client.clone(), remote.clone(), bytes.clone(), *expected),
    )
    .await
    .expect("in time")
    .expect("saved");
    assert_eq!(
        std::fs::read(&hosts).expect("read"),
        b"127.0.0.1 a\n10.0.0.1 b\n"
    );

    saves_only_over_what_was_read(client, remote, (opened.fingerprint, saved), &hosts).await;
    let left: Vec<_> = std::fs::read_dir(root.path())
        .expect("listed")
        .map(|entry| entry.expect("entry").file_name())
        .collect();
    assert_eq!(left.len(), 1, "no temporary file nor backup left: {left:?}");
}

#[tokio::test]
async fn an_ftp_tab_edits_with_an_external_editor_and_never_with_sudo() {
    let root = tempfile::tempdir().expect("root");
    std::fs::write(root.path().join("a.conf"), b"x\n").expect("file");
    let port = serve(root.path()).await;
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path(), vec![profile("ftp", port)]);
    let tab = ftp_tab(
        &mut app,
        "ftp",
        session(port).await,
        vec![file("a.conf", 2)],
    );
    select(&mut app, tab, 0);

    let editing = files(&mut app, FilesMessage::EditExternal { tab });
    assert!(
        matches!(editing.as_slice(), [Effect::StartEdit { remote, .. }] if *remote == RemotePath::from("/a.conf")),
        "{editing:?}"
    );
    // No SSH connection to run sudo: nothing asked, nothing said.
    assert!(files(&mut app, FilesMessage::EditWithSudo { tab }).is_empty());
    let pane = app.tab(tab).and_then(|t| t.files.as_ref()).expect("files");
    assert_eq!(pane.remote.error, None);
    // Nor permissions: SFTP only, as the C#.
    files(
        &mut app,
        FilesMessage::AskPermissions {
            tab,
            side: Side::Remote,
        },
    );
    assert!(app.dialog.is_none(), "{:?}", app.dialog);
}

#[cfg(unix)]
#[tokio::test]
async fn an_external_editors_save_is_uploaded_over_ftp_only_while_the_file_is_unchanged() {
    use std::path::PathBuf;

    use heimdall_app::external_edit::{EditCheck, EditSession, Editor, check_edit, start_edit};
    use heimdall_app::files::FilesError;
    use tokio_util::sync::CancellationToken;

    let root = tempfile::tempdir().expect("root");
    let file = root.path().join("nginx.conf");
    std::fs::write(&file, b"listen 80;\n").expect("file");
    let client = session(serve(root.path()).await).await;
    let dir = tempfile::tempdir().expect("dir");
    let quiet = Editor {
        program: PathBuf::from("true"),
        arguments: Vec::new(),
    };
    let mut session = tokio::time::timeout(
        STEP,
        start_edit(
            client.clone(),
            RemotePath::from("/nginx.conf"),
            quiet,
            (dir.path().join("edits"), Vec::new()),
            CancellationToken::new(),
        ),
    )
    .await
    .expect("in time")
    .expect("opened");
    assert_eq!(
        std::fs::read(&session.local).expect("copy"),
        b"listen 80;\n"
    );
    let look = async |session: &mut EditSession| {
        let check = tokio::time::timeout(STEP, check_edit(&client, session, None))
            .await
            .expect("in time");
        session.apply(&check);
        check
    };
    assert_eq!(look(&mut session).await, EditCheck::Unchanged);

    // Saved: seen once, sent when seen again unchanged.
    std::thread::sleep(Duration::from_millis(20));
    std::fs::write(&session.local, b"listen 443;\n").expect("saved");
    assert!(matches!(look(&mut session).await, EditCheck::Saving(_)));
    assert!(matches!(look(&mut session).await, EditCheck::Sent { .. }));
    assert_eq!(std::fs::read(&file).expect("sent"), b"listen 443;\n");

    // Changed on the server meanwhile: the save is kept, the server's file left as it is.
    std::fs::write(&file, b"listen 8080; # by someone else\n").expect("changed");
    std::thread::sleep(Duration::from_millis(20));
    std::fs::write(&session.local, b"listen 8443;\n").expect("saved");
    assert!(matches!(look(&mut session).await, EditCheck::Saving(_)));
    assert!(matches!(
        look(&mut session).await,
        EditCheck::Refused {
            error: FilesError::ChangedOnServer,
            ..
        }
    ));
    assert_eq!(
        std::fs::read(&file).expect("kept"),
        b"listen 8080; # by someone else\n"
    );
}

#[tokio::test]
async fn an_ftp_tabs_entries_are_copied_and_pasted_on_another_server() {
    let first = tempfile::tempdir().expect("first");
    std::fs::write(first.path().join("report.txt"), b"quarterly\n").expect("file");
    std::fs::create_dir(first.path().join("logs")).expect("folder");
    std::fs::write(first.path().join("logs").join("a.log"), b"line\n").expect("log");
    let second = tempfile::tempdir().expect("second");
    let (from_port, to_port) = (serve(first.path()).await, serve(second.path()).await);
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(
        dir.path(),
        vec![profile("from", from_port), profile("to", to_port)],
    );
    let folder = RemoteEntry {
        kind: EntryKind::Directory,
        ..file("logs", 0)
    };
    let source = ftp_tab(
        &mut app,
        "from",
        session(from_port).await,
        vec![file("report.txt", 10), folder],
    );
    let target = ftp_tab(&mut app, "to", session(to_port).await, Vec::new());

    // Copy is offered, as the C#; permissions and a copy on the server are not.
    assert!(app.can_hold_copy(source));
    assert!(!app.can_copy(source), "no copy on an FTP server");
    assert!(!app.files_over_sftp(source));
    assert!(app.files_connected(source));
    select(&mut app, source, 0);
    files(
        &mut app,
        FilesMessage::Toggle {
            tab: source,
            side: Side::Remote,
            index: 1,
        },
    );
    assert!(files(&mut app, FilesMessage::Copy { tab: source }).is_empty());
    assert_eq!(app.notice(), Some(&Notice::FilesCopied(2)));
    assert!(
        !app.can_paste(source),
        "FTP has no copy on its own server, as the C# refuses it"
    );
    assert!(app.can_paste(target), "another server");

    let pasted = files(&mut app, FilesMessage::Paste { tab: target });
    let [
        Effect::CopyAcross {
            from,
            to,
            sources,
            folder,
            staging,
            cancel,
            ..
        },
    ] = pasted.as_slice()
    else {
        panic!("{pasted:?}");
    };
    let results = tokio::time::timeout(
        STEP,
        copy_across(
            from.clone(),
            to.clone(),
            sources.clone(),
            folder.clone(),
            staging.clone(),
            cancel.clone(),
        ),
    )
    .await
    .expect("in time");
    assert!(
        results.iter().all(|(_, result)| result.is_ok()),
        "{results:?}"
    );
    assert_eq!(
        std::fs::read(second.path().join("report.txt")).expect("copied"),
        b"quarterly\n"
    );
    assert_eq!(
        std::fs::read(second.path().join("logs").join("a.log")).expect("copied"),
        b"line\n"
    );
    assert_eq!(
        std::fs::read(first.path().join("report.txt")).expect("kept"),
        b"quarterly\n",
        "a copy leaves the source"
    );
}
