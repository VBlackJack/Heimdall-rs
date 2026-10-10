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

//! The SFTP pane docked beside an SSH shell following the working folder the shell reports
//! (OSC 7), as the C# `FollowSftpToCurrentDirectory`: only while its "cwd" toggle is on,
//! seeded from the setting, the toggle never changing the setting; the host left aside;
//! the same folder never listed twice in a row; a folder that cannot be listed said in the
//! pane, which stays where it was; nothing done while it lists or asks a question, the next
//! report followed then; never an FTP pane, and nothing ever typed into the shell.

#[path = "support/ftp_server.rs"]
mod ftp_server;

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use heimdall_app::files::Side;
use heimdall_app::split::{Axis, Placement, SplitMessage};
use heimdall_app::{
    App, AppConfig, AttemptId, ConnectionEvent, Effect, FilesMessage, InputSink, Message,
    SettingsMessage, TabId,
};
use heimdall_core::profile::{FtpProfile, ProfileId, SshProfile};
use heimdall_core::settings::SftpBrowser;
use heimdall_core::store::ProfileStore;
use heimdall_files::{FtpClient, FtpSecurity, FtpTarget, Refusal, RemotePath, RemoteSession};
use heimdall_sftp::protocol::{Request, Response, SFTP_VERSION};
use heimdall_sftp::{ClientConfig, SftpClient};
use heimdall_ssh::{AgentSource, SessionClosed, TerminalSize};
use heimdall_term::GridSize;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

/// The grid every tab opens at until the window says otherwise.
const GRID: GridSize = GridSize { cols: 80, rows: 24 };

/// The folder the docked pane shows first.
const HOME: &str = "/home/admin";

/// Longest wait for the test FTP server.
const STEP: Duration = Duration::from_secs(20);

/// The port of the FTP profile in the tests that open none: the standard one.
const UNSERVED_FTP_PORT: u16 = 21;

/// What the shell was sent: nothing, ever, by following.
#[derive(Debug, Default)]
struct Sink(Mutex<Vec<u8>>);

impl InputSink for Sink {
    fn write(&self, bytes: Vec<u8>) -> Result<(), SessionClosed> {
        self.0.lock().expect("sink").extend(bytes);
        Ok(())
    }
    fn resize(&self, _size: TerminalSize) -> Result<(), SessionClosed> {
        Ok(())
    }
    fn close(&self) {}
}

/// Profile `a`, an SSH server; `files`, an FTP server on `ftp_port`.
fn app(dir: &Path, ftp_port: u16) -> App {
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
    store.merge_ftp([FtpProfile {
        id: ProfileId::new("files"),
        name: "files".to_owned(),
        group: None,
        host: "127.0.0.1".to_owned(),
        port: ftp_port,
        username: None,
        passive: true,
        tls: false,
        vault_entry: None,
    }]);
    store.save().expect("save");
    App::new(AppConfig {
        profiles_file,
        known_hosts: dir.join("known_hosts"),
        legacy_dir: None,
        agent: AgentSource::Disabled,
        initial_grid: GRID,
        files_start: dir.to_owned(),
        system_credentials: heimdall_app::SystemCredentials::memory(),
    })
}

fn settings(app: &mut App, sftp: SftpBrowser) {
    app.update(Message::Settings(SettingsMessage::SftpBrowser(sftp)));
    assert_eq!(app.settings().sftp_browser, sftp);
}

/// The settings with following on or off, the browser and its auto-open on.
fn following(follow_ssh_directory: bool) -> SftpBrowser {
    SftpBrowser {
        follow_ssh_directory,
        ..SftpBrowser::default()
    }
}

/// An SFTP server that answers the start, then nothing.
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
        std::future::pending::<()>().await;
    });
    RemoteSession::Sftp(
        SftpClient::start(client_end, ClientConfig::default())
            .await
            .expect("started"),
    )
}

/// A shell of profile `a` connected, writing to `sink`.
fn shell(app: &mut App, sink: &Arc<Sink>) -> (TabId, AttemptId, Vec<Effect>) {
    let (tab, attempt) = match app
        .update(Message::OpenProfile(ProfileId::new("a")))
        .as_slice()
    {
        [Effect::Connect { tab, attempt, .. }] => (*tab, *attempt),
        other => panic!("expected one Connect, got {other:?}"),
    };
    let effects = app.update(Message::Connection {
        tab,
        attempt,
        event: ConnectionEvent::Connected {
            input: Arc::clone(sink) as Arc<dyn InputSink>,
        },
    });
    (tab, attempt, effects)
}

/// A connected shell and the SFTP pane docked beside it, connected and showing [`HOME`],
/// following the shell as `sftp` says.
struct Docked {
    app: App,
    shell: (TabId, AttemptId),
    pane: TabId,
    sink: Arc<Sink>,
}

async fn docked(dir: &Path, sftp: SftpBrowser) -> Docked {
    let mut app = app(dir, UNSERVED_FTP_PORT);
    settings(&mut app, sftp);
    let sink = Arc::new(Sink::default());
    let (tab, attempt, effects) = shell(&mut app, &sink);
    let [
        Effect::Connect {
            tab: pane,
            attempt: pane_attempt,
            ..
        },
    ] = effects.as_slice()
    else {
        panic!("the pane's connection: {effects:?}");
    };
    let (pane, pane_attempt) = (*pane, *pane_attempt);
    app.update(Message::Connection {
        tab: pane,
        attempt: pane_attempt,
        event: ConnectionEvent::FilesReady {
            client: idle_client().await,
            shell: None,
        },
    });
    listed(&mut app, pane, HOME);
    Docked {
        app,
        shell: (tab, attempt),
        pane,
        sink,
    }
}

/// The pane's listing of `path` arrived.
fn listed(app: &mut App, pane: TabId, path: &str) {
    app.update(Message::Files(FilesMessage::RemoteListed {
        tab: pane,
        result: Ok((RemotePath::from(path), Vec::new())),
    }));
}

/// The shell's output: its prompt, reporting `url` as its working folder.
fn report(app: &mut App, (tab, attempt): (TabId, AttemptId), url: &str) -> Vec<Effect> {
    app.update(Message::Connection {
        tab,
        attempt,
        event: ConnectionEvent::Output(format!("\x1b]7;{url}\x1b\\admin@a:~$ ").into_bytes()),
    })
}

/// The folders `effects` list on `pane`'s server.
fn lists(effects: &[Effect], pane: TabId) -> Vec<String> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::ListRemote { tab, path, .. } if *tab == pane => Some(path.display()),
            _ => None,
        })
        .collect()
}

fn files(app: &App, pane: TabId) -> &heimdall_app::files::FilesPane {
    app.tab(pane)
        .and_then(|tab| tab.files.as_deref())
        .expect("files")
}

fn follows(app: &App, pane: TabId) -> Option<bool> {
    files(app, pane).follow.as_ref().map(|follow| follow.on)
}

#[tokio::test]
async fn a_reported_folder_is_gone_to_while_the_toggle_is_on_the_host_left_aside() {
    let dir = tempfile::tempdir().expect("dir");
    let Docked {
        mut app,
        shell,
        pane,
        sink,
    } = docked(dir.path(), following(true)).await;
    let effects = report(&mut app, shell, "file://a.lab/var/log");
    assert_eq!(lists(&effects, pane), ["/var/log"]);
    assert_eq!(effects.len(), 1, "nothing else: {effects:?}");
    assert!(files(&app, pane).remote.loading);
    listed(&mut app, pane, "/var/log");
    let remote = &files(&app, pane).remote;
    assert_eq!(remote.path.display(), "/var/log");
    assert_eq!(
        remote.history,
        [RemotePath::from(HOME)],
        "as a path typed in its bar: Back goes back"
    );
    assert_eq!(
        app.tab(shell.0)
            .and_then(|tab| tab.working_directory.as_deref()),
        Some("/var/log"),
        "kept on the shell's tab"
    );
    // A nested ssh to another server: its path, on the pane's own server, as the C#.
    let effects = report(&mut app, shell, "file://elsewhere.lab/srv/my%20site");
    assert_eq!(lists(&effects, pane), ["/srv/my site"]);
    assert!(
        sink.0.lock().expect("sink").is_empty(),
        "nothing typed into the shell"
    );
}

#[tokio::test]
async fn nothing_is_gone_to_while_the_toggle_is_off_but_the_folder_is_kept() {
    let dir = tempfile::tempdir().expect("dir");
    let Docked {
        mut app,
        shell,
        pane,
        ..
    } = docked(dir.path(), following(false)).await;
    assert_eq!(follows(&app, pane), Some(false));
    let effects = report(&mut app, shell, "file://a.lab/etc");
    assert!(lists(&effects, pane).is_empty(), "{effects:?}");
    assert_eq!(files(&app, pane).remote.path.display(), HOME);
    assert_eq!(
        app.tab(shell.0)
            .and_then(|tab| tab.working_directory.as_deref()),
        Some("/etc")
    );
}

#[tokio::test]
async fn the_same_folder_reported_again_is_not_listed_again() {
    let dir = tempfile::tempdir().expect("dir");
    let Docked {
        mut app,
        shell,
        pane,
        ..
    } = docked(dir.path(), following(true)).await;
    assert_eq!(
        lists(&report(&mut app, shell, "file://a.lab/srv"), pane),
        ["/srv"]
    );
    listed(&mut app, pane, "/srv");
    assert!(
        lists(&report(&mut app, shell, "file://a.lab/srv"), pane).is_empty(),
        "the next prompt, the same folder"
    );
    // The user browses elsewhere: the next prompt in the same folder leaves the pane there.
    app.update(Message::Files(FilesMessage::Up {
        tab: pane,
        side: Side::Remote,
    }));
    listed(&mut app, pane, "/");
    assert!(lists(&report(&mut app, shell, "file://a.lab/srv"), pane).is_empty());
    assert_eq!(
        lists(&report(&mut app, shell, "file://a.lab/tmp"), pane),
        ["/tmp"],
        "the shell moved"
    );
}

#[tokio::test]
async fn the_setting_seeds_the_toggle_which_never_changes_it() {
    let dir = tempfile::tempdir().expect("dir");
    let Docked { mut app, pane, .. } = docked(dir.path(), following(true)).await;
    assert_eq!(follows(&app, pane), Some(true), "seeded on");
    app.update(Message::Files(FilesMessage::ToggleFollow { tab: pane }));
    assert_eq!(follows(&app, pane), Some(false));
    assert_eq!(
        app.settings().sftp_browser,
        following(true),
        "the setting stays, as the C# toggle leaves it"
    );
    let dir = tempfile::tempdir().expect("dir");
    let Docked { app, pane, .. } = docked(dir.path(), following(false)).await;
    assert_eq!(follows(&app, pane), Some(false), "seeded off, the default");
}

#[tokio::test]
async fn the_toggle_turned_on_goes_nowhere_until_the_shell_reports_again() {
    let dir = tempfile::tempdir().expect("dir");
    let Docked {
        mut app,
        shell,
        pane,
        ..
    } = docked(dir.path(), following(false)).await;
    assert!(lists(&report(&mut app, shell, "file://a.lab/opt"), pane).is_empty());
    let effects = app.update(Message::Files(FilesMessage::ToggleFollow { tab: pane }));
    assert!(effects.is_empty(), "as the C# toggle: {effects:?}");
    assert_eq!(follows(&app, pane), Some(true));
    assert_eq!(
        lists(&report(&mut app, shell, "file://a.lab/opt"), pane),
        ["/opt"],
        "the next report, even of the same folder"
    );
    listed(&mut app, pane, "/opt");
    // Off and on again: the folder followed is forgotten.
    app.update(Message::Files(FilesMessage::ToggleFollow { tab: pane }));
    app.update(Message::Files(FilesMessage::ToggleFollow { tab: pane }));
    assert_eq!(
        lists(&report(&mut app, shell, "file://a.lab/opt"), pane),
        ["/opt"]
    );
}

#[tokio::test]
async fn a_folder_that_cannot_be_listed_is_said_and_the_pane_stays_where_it_was() {
    let dir = tempfile::tempdir().expect("dir");
    let Docked {
        mut app,
        shell,
        pane,
        ..
    } = docked(dir.path(), following(true)).await;
    assert_eq!(
        lists(&report(&mut app, shell, "file://a.lab/root/private"), pane),
        ["/root/private"]
    );
    let refused = heimdall_app::files::FilesError::Server {
        refusal: Refusal::PermissionDenied,
        message: String::new(),
    };
    app.update(Message::Files(FilesMessage::RemoteListed {
        tab: pane,
        result: Err(refused.clone()),
    }));
    let remote = &files(&app, pane).remote;
    assert_eq!(remote.path.display(), HOME, "the folder shown stays");
    assert_eq!(remote.error.as_ref(), Some(&refused), "said in the pane");
    assert!(remote.history.is_empty(), "nowhere left");
    assert!(
        lists(&report(&mut app, shell, "file://a.lab/root/private"), pane).is_empty(),
        "not tried again at each prompt"
    );
}

#[tokio::test]
async fn a_report_is_let_go_while_the_pane_lists_or_asks_and_the_next_one_followed() {
    let dir = tempfile::tempdir().expect("dir");
    let Docked {
        mut app,
        shell,
        pane,
        ..
    } = docked(dir.path(), following(true)).await;
    // Listing: let go, as the C# load gate lets it go.
    app.update(Message::Files(FilesMessage::Refresh {
        tab: pane,
        side: Side::Remote,
    }));
    assert!(lists(&report(&mut app, shell, "file://a.lab/var"), pane).is_empty());
    listed(&mut app, pane, HOME);
    assert_eq!(
        lists(&report(&mut app, shell, "file://a.lab/var"), pane),
        ["/var"],
        "the next prompt"
    );
    listed(&mut app, pane, "/var");
    // Asking for a new folder's name: the folder stays the one it was asked in.
    app.update(Message::Files(FilesMessage::AskNewFolder {
        tab: pane,
        side: Side::Remote,
    }));
    assert!(lists(&report(&mut app, shell, "file://a.lab/usr"), pane).is_empty());
    app.update(Message::DismissDialog);
    assert_eq!(
        lists(&report(&mut app, shell, "file://a.lab/usr"), pane),
        ["/usr"]
    );
}

/// Serves `root` over FTP on a free local port; returns the port.
async fn serve(root: &Path) -> u16 {
    ftp_server::serve(root, None).await
}

#[tokio::test]
async fn an_ftp_pane_beside_the_shell_never_follows_it() {
    let dir = tempfile::tempdir().expect("dir");
    let served = tempfile::tempdir().expect("served");
    let port = serve(served.path()).await;
    let mut app = app(dir.path(), port);
    settings(
        &mut app,
        SftpBrowser {
            auto_open_on_ssh: false,
            ..following(true)
        },
    );
    let sink = Arc::new(Sink::default());
    let (tab, attempt, _) = shell(&mut app, &sink);
    let effects = app.update(Message::ConnectProfile(ProfileId::new("files")));
    let [
        Effect::ConnectFtp {
            tab: ftp,
            attempt: ftp_attempt,
            ..
        },
    ] = effects.as_slice()
    else {
        panic!("{effects:?}");
    };
    let (ftp, ftp_attempt) = (*ftp, *ftp_attempt);
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
    app.update(Message::Connection {
        tab: ftp,
        attempt: ftp_attempt,
        event: ConnectionEvent::FilesReady {
            client: RemoteSession::Ftp(client),
            shell: None,
        },
    });
    listed(&mut app, ftp, "/");
    app.update(Message::Split(SplitMessage::Merge {
        host: tab,
        tab: ftp,
        axis: Axis::SideBySide,
        placement: Placement::Second,
    }));
    assert_eq!(app.panes_of(tab), [tab, ftp], "split with the shell");
    assert_eq!(follows(&app, ftp), None, "no toggle over FTP, as the C#");
    let effects = report(&mut app, (tab, attempt), "file://a.lab/pub");
    assert!(lists(&effects, ftp).is_empty(), "{effects:?}");
    assert_eq!(files(&app, ftp).remote.path.display(), "/");
}
