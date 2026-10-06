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

//! A Files tab's sudo mode: what the toggle, a delete and a change of permissions ask, and,
//! over SSH against the in-process server running this computer's `sh` with a stand-in
//! `sudo`, what a listing, a delete and a change as root do.

#![cfg(unix)]

#[path = "../../heimdall-ssh/tests/common/mod.rs"]
mod ssh;

use std::os::unix::ffi::OsStrExt as _;
use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use heimdall_app::files::{EntryKind, FileOperation, FilesError, RemoteEntry, Side};
use heimdall_app::sudo_edit::SudoPassword;
use heimdall_app::sudo_mode::{SudoAccess, chmod_or_sudo, sudo_list, sudo_remove};
use heimdall_app::{
    App, AppConfig, ConnectionEvent, Dialog, Effect, FilesMessage, Message, SudoAction, TabId,
};
use heimdall_core::profile::{ProfileId, SshProfile};
use heimdall_files::privileged::Sudo;
use heimdall_files::{Refusal, RemoteSession};
use heimdall_sftp::protocol::{Request, Response, SFTP_VERSION, StatusCode};
use heimdall_sftp::{ClientConfig, RemotePath, SftpClient};
use heimdall_ssh::{AgentSource, Connection, establish};
use heimdall_term::GridSize;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio_util::sync::CancellationToken;

const PASSWORD: &str = "s3cret";

/// A stand-in for sudo, beside a `password` file; each run noted in `calls`.
const FAKE_SUDO: &str = r#"#!/bin/sh
dir=$(dirname "$0")
echo run >> "$dir/calls"
n=0
while [ $# -gt 0 ]; do
  case "$1" in
    -n) n=1 ;;
    -S|-k) ;;
    -p) shift ;;
    --) shift; break ;;
    *) break ;;
  esac
  shift
done
if [ "$n" = 1 ]; then echo "sudo: a password is required" >&2; exit 1; fi
IFS= read -r line || { echo "sudo: no password was provided" >&2; exit 1; }
if [ "$line" != "$(cat "$dir/password")" ]; then
  echo "Sorry, try again." >&2
  echo "sudo: 1 incorrect password attempt" >&2
  exit 1
fi
exec "$@"
"#;

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

/// The stand-in sudo in `dir/bin`; its runs are counted in `dir/bin/calls`.
fn fake_sudo(dir: &Path) -> PathBuf {
    let bin = dir.join("bin");
    std::fs::create_dir(&bin).expect("bin");
    let sudo = bin.join("sudo");
    std::fs::write(&sudo, FAKE_SUDO).expect("sudo");
    std::fs::set_permissions(&sudo, std::fs::Permissions::from_mode(0o755)).expect("mode");
    std::fs::write(bin.join("password"), PASSWORD).expect("password");
    sudo
}

fn sudo_runs(sudo: &Path) -> usize {
    std::fs::read_to_string(sudo.with_file_name("calls")).map_or(0, |calls| calls.lines().count())
}

fn remote(path: &Path) -> RemotePath {
    RemotePath::from_bytes(path.as_os_str().as_bytes())
}

/// An SFTP session answering every request after its start with `code`.
async fn answering(code: StatusCode) -> RemoteSession {
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
            let status = Response::Status {
                id,
                code,
                message: Vec::new(),
            };
            server.write_all(&status.encode()).await.expect("status");
        }
    });
    RemoteSession::Sftp(
        SftpClient::start(client_end, ClientConfig::default())
            .await
            .expect("started"),
    )
}

#[tokio::test]
async fn a_folder_is_listed_as_root_with_the_password_and_each_entry_its_inode() {
    let server = ssh::start(ssh::Spec::default()).await;
    let keys = tempfile::tempdir().expect("keys");
    let connection = shell(&server, keys.path()).await;
    let dir = tempfile::tempdir().expect("dir");
    let sudo_path = fake_sudo(dir.path());
    let sudo = Sudo::Unchecked(sudo_path.to_str().expect("utf-8"));
    let folder = dir.path().join("secret");
    std::fs::create_dir(&folder).expect("folder");
    std::fs::create_dir(folder.join("logs")).expect("logs");
    std::fs::write(folder.join("it's a\nname"), b"12345").expect("file");
    std::fs::set_permissions(
        folder.join("it's a\nname"),
        std::fs::Permissions::from_mode(0o640),
    )
    .expect("mode");
    std::os::unix::fs::symlink(&folder, dir.path().join("link")).expect("link");

    assert_eq!(
        sudo_list(&connection, &remote(&folder), None, sudo).await,
        Err(FilesError::SudoPasswordNeeded)
    );
    assert_eq!(
        sudo_list(&connection, &remote(&folder), Some(b"wrong"), sudo).await,
        Err(FilesError::SudoPasswordRejected)
    );
    let (listed, entries) = sudo_list(
        &connection,
        &remote(&dir.path().join("link")),
        Some(PASSWORD.as_bytes()),
        sudo,
    )
    .await
    .expect("listed");
    let canonical = folder.canonicalize().expect("canonical");
    assert_eq!(listed, remote(&canonical), "the folder the link leads to");
    let [logs, file] = entries.as_slice() else {
        panic!("{entries:?}");
    };
    assert_eq!(
        (logs.label.as_str(), logs.kind),
        ("logs", EntryKind::Directory)
    );
    assert_eq!(file.name, b"it's a\nname", "the exact bytes");
    assert_eq!(
        (file.kind, file.size, file.permissions),
        (EntryKind::File, Some(5), Some(0o640))
    );
    assert_eq!(
        file.inode,
        Some(
            std::fs::metadata(folder.join("it's a\nname"))
                .expect("there")
                .ino()
        )
    );
    assert_eq!(
        sudo_list(
            &connection,
            &remote(&folder.join("logs/none")),
            Some(PASSWORD.as_bytes()),
            sudo
        )
        .await,
        Err(FilesError::Server {
            refusal: Refusal::NoSuchFile,
            message: String::new()
        })
    );
}

#[tokio::test]
async fn a_delete_as_root_removes_only_what_was_confirmed_and_never_a_protected_path() {
    let server = ssh::start(ssh::Spec::default()).await;
    let keys = tempfile::tempdir().expect("keys");
    let connection = shell(&server, keys.path()).await;
    let dir = tempfile::tempdir().expect("dir");
    let sudo_path = fake_sudo(dir.path());
    let sudo = Sudo::Unchecked(sudo_path.to_str().expect("utf-8"));
    let access = SudoAccess {
        shell: connection,
        password: Some(SudoPassword::new(PASSWORD)),
    };
    let inode = |path: &Path| std::fs::symlink_metadata(path).expect("there").ino();

    // Replaced between the confirmation and the delete: left as it is.
    let file = dir.path().join("a.conf");
    std::fs::write(&file, b"first").expect("file");
    let confirmed = inode(&file);
    std::fs::write(dir.path().join("b.conf"), b"second").expect("other");
    std::fs::rename(dir.path().join("b.conf"), &file).expect("replaced");
    let removed = |path: PathBuf, kind, inode| {
        let access = access.clone();
        async move { sudo_remove(&access, &remote(&path), (kind, inode), None, sudo).await }
    };
    assert_eq!(
        removed(file.clone(), EntryKind::File, Some(confirmed)).await,
        Err(FilesError::ChangedSinceConfirmed)
    );
    assert_eq!(std::fs::read(&file).expect("kept"), b"second");
    assert_eq!(
        removed(file.clone(), EntryKind::Directory, Some(inode(&file))).await,
        Err(FilesError::ChangedSinceConfirmed),
        "another kind"
    );
    assert_eq!(
        removed(file.clone(), EntryKind::File, None).await,
        Err(FilesError::ChangedSinceConfirmed),
        "not listed as root: nothing to check"
    );
    removed(file.clone(), EntryKind::File, Some(inode(&file)))
        .await
        .expect("removed");
    assert!(!file.exists());
    assert_eq!(
        removed(file.clone(), EntryKind::File, Some(confirmed)).await,
        Err(FilesError::ChangedSinceConfirmed),
        "gone"
    );

    // A folder goes with everything in it; a link to a folder alone.
    let kept = dir.path().join("kept");
    std::fs::create_dir(&kept).expect("kept");
    std::fs::write(kept.join("inside"), b"x").expect("inside");
    let folder = dir.path().join("old");
    std::fs::create_dir_all(folder.join("deep/er")).expect("folder");
    std::fs::write(folder.join("deep/er/file"), b"x").expect("file");
    let link = dir.path().join("link");
    std::os::unix::fs::symlink(&kept, &link).expect("link");
    removed(link.clone(), EntryKind::Link, Some(inode(&link)))
        .await
        .expect("link removed");
    assert!(kept.join("inside").exists(), "never what a link points to");
    removed(folder.clone(), EntryKind::Directory, Some(inode(&folder)))
        .await
        .expect("folder removed");
    assert!(!folder.exists());
}

#[tokio::test]
async fn protected_paths_are_refused_before_anything_runs_as_root() {
    let server = ssh::start(ssh::Spec::default()).await;
    let dir = tempfile::tempdir().expect("dir");
    let connection = shell(&server, dir.path()).await;
    let sudo_path = fake_sudo(dir.path());
    let sudo = Sudo::Unchecked(sudo_path.to_str().expect("utf-8"));
    let access = SudoAccess {
        shell: connection,
        password: Some(SudoPassword::new(PASSWORD)),
    };
    let home = dir.path().join("home");
    std::fs::create_dir(&home).expect("home");
    let inode = Some(std::fs::metadata(&home).expect("there").ino());
    let refused = |path: RemotePath, given: Option<RemotePath>| {
        let access = access.clone();
        async move {
            sudo_remove(
                &access,
                &path,
                (EntryKind::Directory, inode),
                given.as_ref(),
                sudo,
            )
            .await
        }
    };
    for path in [
        "/",
        "/etc",
        "/etc/",
        "/home/user",
        "relative",
        "/tmp/../etc",
    ] {
        assert_eq!(
            refused(RemotePath::from(path), Some(remote(&home))).await,
            Err(FilesError::SudoProtected),
            "{path}"
        );
    }
    assert_eq!(
        refused(remote(&home), Some(remote(&home))).await,
        Err(FilesError::SudoProtected),
        "the account's home folder itself"
    );
    assert_eq!(sudo_runs(&sudo_path), 0, "sudo never ran for them");
    assert!(home.exists());
    refused(remote(&home), None)
        .await
        .expect("the same folder, not given as the home: deleted");
    assert!(!home.exists());
}

#[tokio::test]
async fn permissions_refused_to_the_account_are_given_as_root_only_while_the_mode_is_on() {
    let server = ssh::start(ssh::Spec::default()).await;
    let keys = tempfile::tempdir().expect("keys");
    let connection = shell(&server, keys.path()).await;
    let dir = tempfile::tempdir().expect("dir");
    let sudo_path = fake_sudo(dir.path());
    let sudo = Sudo::Unchecked(sudo_path.to_str().expect("utf-8"));
    let access = SudoAccess {
        shell: connection,
        password: Some(SudoPassword::new(PASSWORD)),
    };
    let file = dir.path().join("script.sh");
    std::fs::write(&file, b"#!/bin/sh\n").expect("file");
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).expect("mode");
    let mode = || {
        std::fs::metadata(&file)
            .expect("there")
            .permissions()
            .mode()
            & 0o7777
    };
    let denied = answering(StatusCode::PermissionDenied).await;

    assert!(
        matches!(
            chmod_or_sudo(&denied, None, &remote(&file), 0o755, sudo).await,
            Err(FilesError::Server {
                refusal: Refusal::PermissionDenied,
                ..
            })
        ),
        "the sudo mode off: refused"
    );
    assert_eq!(sudo_runs(&sudo_path), 0);
    assert_eq!(mode(), 0o600);
    chmod_or_sudo(&denied, Some(&access), &remote(&file), 0o4755, sudo)
        .await
        .expect("given as root");
    assert_eq!(mode(), 0o4755);

    let missing = answering(StatusCode::NoSuchFile).await;
    let runs = sudo_runs(&sudo_path);
    assert!(
        matches!(
            chmod_or_sudo(&missing, Some(&access), &remote(&file), 0o700, sudo).await,
            Err(FilesError::Server {
                refusal: Refusal::NoSuchFile,
                ..
            })
        ),
        "refused for another reason: not as root"
    );
    assert_eq!(sudo_runs(&sudo_path), runs);

    let link = dir.path().join("link");
    std::os::unix::fs::symlink(&file, &link).expect("link");
    assert_eq!(
        chmod_or_sudo(&denied, Some(&access), &remote(&link), 0o777, sudo).await,
        Err(FilesError::IsLink),
        "never through a link"
    );
    assert_eq!(mode(), 0o4755);
}

// ---- what the tab asks -------------------------------------------------------------------

fn entry(name: &str, kind: EntryKind, inode: u64) -> RemoteEntry {
    RemoteEntry {
        name: name.as_bytes().to_vec(),
        label: name.to_owned(),
        kind,
        size: Some(1),
        modified: None,
        permissions: Some(0o644),
        owner: None,
        group: None,
        inode: Some(inode),
    }
}

/// A Files tab with `/srv` listed, and `shell` as its SSH connection, if any.
async fn tab(dir: &Path, shell: Option<Connection>) -> (App, TabId) {
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
        event: ConnectionEvent::FilesReady {
            client: answering(StatusCode::NoSuchFile).await,
            shell,
        },
    });
    files(
        &mut app,
        FilesMessage::RemoteListed {
            tab,
            result: Ok((
                RemotePath::from("/srv"),
                vec![entry("logs", EntryKind::Directory, 0)],
            )),
        },
    );
    (app, tab)
}

fn files(app: &mut App, message: FilesMessage) -> Vec<Effect> {
    app.update(Message::Files(message))
}

fn pane(app: &App, tab: TabId) -> &heimdall_app::files::FilesPane {
    app.tab(tab)
        .and_then(|tab| tab.files.as_deref())
        .expect("a files tab")
}

/// What `tab` lists as root, as a listing on its way would bring it.
fn listed_as_root(app: &mut App, tab: TabId, path: &str, entries: Vec<RemoteEntry>) {
    files(
        app,
        FilesMessage::SudoListed {
            tab,
            path: RemotePath::from(path),
            result: Ok((RemotePath::from(path), entries)),
        },
    );
}

/// The first `count` entries of the server's pane selected.
fn select_first(app: &mut App, tab: TabId, count: usize) {
    files(
        app,
        FilesMessage::Select {
            tab,
            side: Side::Remote,
            index: 0,
        },
    );
    for index in 1..count {
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

#[tokio::test]
async fn the_sudo_toggle_asks_the_password_once_and_turning_it_off_forgets_it() {
    let server = ssh::start(ssh::Spec::default()).await;
    let dir = tempfile::tempdir().expect("dir");
    let connection = shell(&server, dir.path()).await;
    let (mut app, tab) = tab(dir.path(), Some(connection)).await;
    assert!(!pane(&app, tab).sudo_mode, "off by default");
    let srv = RemotePath::from("/srv");

    let on = files(&mut app, FilesMessage::ToggleSudo { tab });
    assert!(
        matches!(on.as_slice(), [Effect::SudoListRemote { path, password: None, .. }] if *path == srv),
        "listed as root: {on:?}"
    );
    assert!(pane(&app, tab).sudo_mode);
    files(
        &mut app,
        FilesMessage::SudoListed {
            tab,
            path: srv.clone(),
            result: Err(FilesError::SudoPasswordNeeded),
        },
    );
    assert!(matches!(
        &app.dialog,
        Some(Dialog::SudoPassword { action: SudoAction::List(path), .. }) if *path == srv
    ));
    let again = files(
        &mut app,
        FilesMessage::SudoPasswordGiven {
            tab,
            password: SudoPassword::new(PASSWORD),
        },
    );
    assert!(matches!(
        again.as_slice(),
        [Effect::SudoListRemote {
            password: Some(_),
            ..
        }]
    ));
    listed_as_root(
        &mut app,
        tab,
        "/srv",
        vec![entry("root-only", EntryKind::File, 7)],
    );
    assert_eq!(pane(&app, tab).remote.entries[0].label, "root-only");

    // Asked once: every listing after it is as root, with the password kept.
    for message in [
        FilesMessage::Refresh {
            tab,
            side: Side::Remote,
        },
        FilesMessage::Up {
            tab,
            side: Side::Remote,
        },
    ] {
        let listing = files(&mut app, message);
        assert!(
            matches!(
                listing.as_slice(),
                [Effect::SudoListRemote {
                    password: Some(_),
                    ..
                }]
            ),
            "{listing:?}"
        );
        assert!(app.dialog.is_none());
    }

    let off = files(&mut app, FilesMessage::ToggleSudo { tab });
    assert!(
        matches!(off.as_slice(), [Effect::ListRemote { .. }]),
        "{off:?}"
    );
    assert!(!pane(&app, tab).sudo_mode);
    assert!(pane(&app, tab).sudo_password.is_none(), "forgotten");
    listed_as_root(&mut app, tab, "/", vec![entry("late", EntryKind::File, 8)]);
    assert_ne!(
        pane(&app, tab)
            .remote
            .entries
            .first()
            .map(|e| e.label.as_str()),
        Some("late"),
        "a listing as root arriving once off is not shown"
    );
}

#[tokio::test]
async fn a_password_not_given_leaves_the_sudo_mode_off() {
    let server = ssh::start(ssh::Spec::default()).await;
    let dir = tempfile::tempdir().expect("dir");
    let connection = shell(&server, dir.path()).await;
    let (mut app, tab) = tab(dir.path(), Some(connection)).await;
    files(&mut app, FilesMessage::ToggleSudo { tab });
    files(
        &mut app,
        FilesMessage::SudoListed {
            tab,
            path: RemotePath::from("/srv"),
            result: Err(FilesError::SudoPasswordNeeded),
        },
    );
    assert!(matches!(&app.dialog, Some(Dialog::SudoPassword { .. })));
    let dismissed = app.update(Message::DismissDialog);
    assert!(
        matches!(dismissed.as_slice(), [Effect::ListRemote { .. }]),
        "listed as the account: {dismissed:?}"
    );
    assert!(!pane(&app, tab).sudo_mode);
}

#[tokio::test]
async fn ftp_tabs_never_offer_the_sudo_toggle() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut app, tab) = tab(dir.path(), None).await;
    assert!(files(&mut app, FilesMessage::ToggleSudo { tab }).is_empty());
    assert!(!pane(&app, tab).sudo_mode);
}

#[tokio::test]
async fn a_delete_as_root_names_the_entries_and_checks_each_as_confirmed() {
    let server = ssh::start(ssh::Spec::default()).await;
    let dir = tempfile::tempdir().expect("dir");
    let connection = shell(&server, dir.path()).await;
    let (mut app, tab) = tab(dir.path(), Some(connection)).await;
    files(&mut app, FilesMessage::ToggleSudo { tab });
    let entries: Vec<RemoteEntry> = (0..12)
        .map(|index| entry(&format!("f{index:02}"), EntryKind::File, 100 + index))
        .chain([entry("logs", EntryKind::Directory, 99)])
        .collect();
    listed_as_root(&mut app, tab, "/srv", entries);

    select_first(&mut app, tab, 2);
    files(
        &mut app,
        FilesMessage::AskDelete {
            tab,
            side: Side::Remote,
        },
    );
    assert!(
        matches!(
            &app.dialog,
            Some(Dialog::ConfirmSudoDelete { names, more: 0, .. })
                if names == &["logs".to_owned(), "f00".to_owned()]
        ),
        "{:?}",
        app.dialog
    );
    let deleting = app.update(Message::ConfirmDialog);
    let [Effect::FileBatchStep { operation, .. }] = deleting.as_slice() else {
        panic!("{deleting:?}");
    };
    match operation.as_ref() {
        FileOperation::RemoteSudoRemove {
            path,
            kind,
            inode,
            home,
            ..
        } => {
            assert_eq!(path.as_bytes(), b"/srv/logs");
            assert_eq!((*kind, *inode), (EntryKind::Directory, Some(99)));
            assert_eq!(home.as_ref(), Some(&RemotePath::from("/srv")));
        }
        other => panic!("{other:?}"),
    }
    files(&mut app, FilesMessage::StopBatch { tab });
    files(
        &mut app,
        FilesMessage::BatchStepDone {
            tab,
            result: Ok(()),
        },
    );

    // Thirteen: the first ten named, then how many more.
    listed_as_root(
        &mut app,
        tab,
        "/srv",
        (0..13)
            .map(|index| entry(&format!("f{index:02}"), EntryKind::File, index))
            .collect(),
    );
    select_first(&mut app, tab, 13);
    files(
        &mut app,
        FilesMessage::AskDelete {
            tab,
            side: Side::Remote,
        },
    );
    assert!(
        matches!(&app.dialog, Some(Dialog::ConfirmSudoDelete { names, more: 3, .. }) if names.len() == 10),
        "{:?}",
        app.dialog
    );
    app.update(Message::DismissDialog);
}

#[tokio::test]
async fn a_protected_root_among_the_entries_is_refused_before_asking() {
    let server = ssh::start(ssh::Spec::default()).await;
    let dir = tempfile::tempdir().expect("dir");
    let connection = shell(&server, dir.path()).await;
    let (mut app, tab) = tab(dir.path(), Some(connection)).await;
    files(&mut app, FilesMessage::ToggleSudo { tab });
    listed_as_root(
        &mut app,
        tab,
        "/",
        vec![
            entry("etc", EntryKind::Directory, 2),
            entry("tmp-x", EntryKind::File, 3),
        ],
    );
    select_first(&mut app, tab, 2);
    files(
        &mut app,
        FilesMessage::AskDelete {
            tab,
            side: Side::Remote,
        },
    );
    assert!(app.dialog.is_none(), "nothing asked");
    assert_eq!(
        pane(&app, tab).remote.error,
        Some(FilesError::SudoProtected)
    );
}

#[tokio::test]
async fn permissions_fall_back_to_sudo_only_while_the_mode_is_on() {
    let server = ssh::start(ssh::Spec::default()).await;
    let dir = tempfile::tempdir().expect("dir");
    let connection = shell(&server, dir.path()).await;
    let (mut app, tab) = tab(dir.path(), Some(connection)).await;
    let change = |app: &mut App| {
        files(
            app,
            FilesMessage::Select {
                tab,
                side: Side::Remote,
                index: 0,
            },
        );
        files(
            app,
            FilesMessage::AskPermissions {
                tab,
                side: Side::Remote,
            },
        );
        files(app, FilesMessage::NameEdited("750".to_owned()));
        let effects = app.update(Message::ConfirmDialog);
        let [Effect::FileBatchStep { operation, .. }] = effects.as_slice() else {
            panic!("{effects:?}");
        };
        let FileOperation::RemoteSetPermissions { mode, sudo, .. } = operation.as_ref() else {
            panic!("{operation:?}");
        };
        assert_eq!(*mode, 0o750);
        let with_sudo = sudo.is_some();
        files(
            app,
            FilesMessage::BatchStepDone {
                tab,
                result: Ok(()),
            },
        );
        with_sudo
    };
    assert!(!change(&mut app), "the mode off: as the account only");
    files(&mut app, FilesMessage::ToggleSudo { tab });
    listed_as_root(
        &mut app,
        tab,
        "/srv",
        vec![entry("app.sh", EntryKind::File, 5)],
    );
    assert!(change(&mut app), "the mode on: as root where refused");
}
