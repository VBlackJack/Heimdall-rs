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

//! The file browser docked beside a local shell following the working folder the shell
//! reports (OSC 7): only while its "cwd" toggle is on, seeded from the setting, the toggle
//! never changing the setting; the host left aside; the same folder never listed twice in
//! a row; a folder that cannot be listed said in the browser, which stays where it was;
//! nothing done while it lists or asks a question, the next report followed then; a path
//! naming no folder of this computer left alone. `ConEmu`'s OSC 9;9, as Windows prompts
//! send it, moves the browser on Windows as OSC 7 does, and nothing elsewhere. An SSH
//! shell's report still drives its SFTP pane alone, never a local browser, and only an
//! OSC 7 one; nothing is ever typed into a shell.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use heimdall_app::files::{FilesError, FilesPane, Side};
use heimdall_app::local_driver::LocalShell;
use heimdall_app::split::{Axis, Placement, SplitMessage};
use heimdall_app::{
    App, AppConfig, AttemptId, ConnectionEvent, Effect, FilesMessage, InputSink, Message,
    SettingsMessage, TabId,
};
use heimdall_core::profile::{ProfileId, SshProfile};
use heimdall_core::settings::SftpBrowser;
use heimdall_core::store::ProfileStore;
use heimdall_files::{RemotePath, RemoteSession};
use heimdall_sftp::protocol::{Request, Response, SFTP_VERSION};
use heimdall_sftp::{ClientConfig, SftpClient};
use heimdall_ssh::{AgentSource, SessionClosed, TerminalSize};
use heimdall_term::GridSize;
use heimdall_term::local::LocalArguments;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

/// The grid every tab opens at until the window says otherwise.
const GRID: GridSize = GridSize { cols: 80, rows: 24 };

/// The folder the SFTP pane shows first.
const REMOTE_HOME: &str = "/home/admin";

/// The host a local shell names in its reports: left aside.
const HOST: &str = "workstation";

/// What a shell was sent: nothing, ever, by following.
#[derive(Debug, Default)]
struct Sink(Mutex<Vec<u8>>);

impl Sink {
    /// Everything written so far.
    fn written(&self) -> Vec<u8> {
        self.0.lock().expect("sink").clone()
    }
}

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

/// Profile `a`, an SSH server; the home folder `dir`.
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
        session_logging: None,
        ssh_mode: heimdall_core::profile::SshMode::Embedded,
        x11_forwarding: false,
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

/// The settings changed to `sftp`.
fn settings(app: &mut App, sftp: SftpBrowser) {
    app.update(Message::Settings(SettingsMessage::SftpBrowser(sftp)));
    assert_eq!(app.settings().sftp_browser, sftp);
}

/// The settings with the local browser following its shell or not, the rest as default.
fn following(follow_local_directory: bool) -> SftpBrowser {
    SftpBrowser {
        follow_local_directory,
        ..SftpBrowser::default()
    }
}

/// The file URL a shell of this computer reports for `folder`, as a file URL writes a
/// path: `/C:/Users/x` on Windows, the path as it is elsewhere, spaces escaped.
fn url(folder: &Path) -> String {
    let path = folder.to_str().expect("UTF-8").replace('\\', "/");
    let path = if path.starts_with('/') {
        path
    } else {
        format!("/{path}")
    };
    format!("file://{HOST}{}", path.replace(' ', "%20"))
}

/// A local shell started in `folder`, writing into `sink`, its browser docked and showing
/// `folder`: the shell and the browser.
fn local_shell(app: &mut App, folder: &Path, sink: &Arc<Sink>) -> ((TabId, AttemptId), TabId) {
    let shell = match app
        .update(Message::OpenLocal(LocalShell {
            name: "Shell".to_owned(),
            program: None,
            arguments: LocalArguments::List(Vec::new()),
            working_directory: Some(folder.to_owned()),
            environment: Vec::new(),
        }))
        .as_slice()
    {
        [Effect::ConnectLocal { tab, attempt, .. }] => (*tab, *attempt),
        other => panic!("expected one ConnectLocal, got {other:?}"),
    };
    let effects = app.update(Message::Connection {
        tab: shell.0,
        attempt: shell.1,
        event: ConnectionEvent::Connected {
            input: Arc::clone(sink) as Arc<dyn InputSink>,
        },
    });
    let [Effect::ListLocal { tab: pane, path }] = effects.as_slice() else {
        panic!("the browser's listing: {effects:?}");
    };
    assert_eq!(path, folder, "the shell's folder");
    let pane = *pane;
    listed(app, pane, folder);
    (shell, pane)
}

/// The browser's listing of `folder` arrived.
fn listed(app: &mut App, pane: TabId, folder: &Path) {
    app.update(Message::Files(FilesMessage::LocalListed {
        tab: pane,
        result: Ok((folder.to_owned(), Vec::new())),
    }));
}

/// The shell's output: its prompt, reporting `url` as its working folder.
fn report(app: &mut App, (tab, attempt): (TabId, AttemptId), url: &str) -> Vec<Effect> {
    app.update(Message::Connection {
        tab,
        attempt,
        event: ConnectionEvent::Output(format!("\x1b]7;{url}\x1b\\> ").into_bytes()),
    })
}

/// The shell's output: its prompt, reporting `path` as its working folder with `ConEmu`'s
/// OSC 9;9, quoted, as Windows Terminal's shell integration sends it.
fn conemu_report(app: &mut App, (tab, attempt): (TabId, AttemptId), path: &str) -> Vec<Effect> {
    app.update(Message::Connection {
        tab,
        attempt,
        event: ConnectionEvent::Output(format!("\x1b]9;9;\"{path}\"\x1b\\PS> ").into_bytes()),
    })
}

/// The folders `effects` list on this computer for `pane`.
fn lists(effects: &[Effect], pane: TabId) -> Vec<PathBuf> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::ListLocal { tab, path } if *tab == pane => Some(path.clone()),
            _ => None,
        })
        .collect()
}

/// The folders `effects` list on a server, for any pane.
fn remote_lists(effects: &[Effect]) -> Vec<String> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::ListRemote { path, .. } => Some(path.display()),
            _ => None,
        })
        .collect()
}

/// The files of `pane`.
fn files(app: &App, pane: TabId) -> &FilesPane {
    app.tab(pane)
        .and_then(|tab| tab.files.as_deref())
        .expect("files")
}

/// The "cwd" toggle of `pane`, on or off; none when it has none.
fn follows(app: &App, pane: TabId) -> Option<bool> {
    files(app, pane).follow.as_ref().map(|follow| follow.on)
}

#[test]
fn a_reported_folder_is_gone_to_while_the_toggle_is_on_the_host_left_aside() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let sink = Arc::new(Sink::default());
    let (shell, pane) = local_shell(&mut app, dir.path(), &sink);
    let sub = dir.path().join("my projects");
    let effects = report(&mut app, shell, &url(&sub));
    assert_eq!(lists(&effects, pane), std::slice::from_ref(&sub));
    assert_eq!(effects.len(), 1, "nothing else: {effects:?}");
    assert!(files(&app, pane).local.loading);
    listed(&mut app, pane, &sub);
    let local = &files(&app, pane).local;
    assert_eq!(local.path, sub);
    assert_eq!(
        local.history,
        [dir.path().to_owned()],
        "as a path typed in its bar: Back goes back"
    );
    assert!(
        app.tab(shell.0)
            .and_then(|tab| tab.working_directory.as_deref())
            .is_some(),
        "kept on the shell's tab"
    );
    // Another host named: the path still read on this computer.
    let other = dir.path().join("other");
    let elsewhere = url(&other).replacen(HOST, "elsewhere", 1);
    assert_eq!(lists(&report(&mut app, shell, &elsewhere), pane), [other]);
    assert!(sink.written().is_empty(), "nothing typed into the shell");
}

#[test]
fn nothing_is_gone_to_while_the_toggle_is_off_and_turned_on_the_next_report_is() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    settings(&mut app, following(false));
    let sink = Arc::new(Sink::default());
    let (shell, pane) = local_shell(&mut app, dir.path(), &sink);
    assert_eq!(follows(&app, pane), Some(false), "seeded off");
    let sub = dir.path().join("sub");
    let effects = report(&mut app, shell, &url(&sub));
    assert!(lists(&effects, pane).is_empty(), "{effects:?}");
    assert_eq!(files(&app, pane).local.path, dir.path());
    let effects = app.update(Message::Files(FilesMessage::ToggleFollow { tab: pane }));
    assert!(effects.is_empty(), "as the SFTP toggle: {effects:?}");
    assert_eq!(follows(&app, pane), Some(true));
    assert_eq!(
        lists(&report(&mut app, shell, &url(&sub)), pane),
        [sub],
        "the next report, even of the same folder"
    );
    assert!(sink.written().is_empty());
}

#[test]
fn the_setting_seeds_the_toggle_which_never_changes_it() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let sink = Arc::new(Sink::default());
    let (_, pane) = local_shell(&mut app, dir.path(), &sink);
    assert_eq!(follows(&app, pane), Some(true), "seeded on, the default");
    app.update(Message::Files(FilesMessage::ToggleFollow { tab: pane }));
    assert_eq!(follows(&app, pane), Some(false));
    assert_eq!(
        app.settings().sftp_browser,
        SftpBrowser::default(),
        "the setting stays"
    );
    // A browser docked after the setting changed is seeded from it.
    settings(&mut app, following(false));
    let (_, later) = local_shell(&mut app, dir.path(), &sink);
    assert_eq!(follows(&app, later), Some(false));
    assert_eq!(follows(&app, pane), Some(false), "the first left as it was");
}

#[test]
fn the_same_folder_reported_again_is_not_listed_again() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let sink = Arc::new(Sink::default());
    let (shell, pane) = local_shell(&mut app, dir.path(), &sink);
    let src = dir.path().join("src");
    assert_eq!(
        lists(&report(&mut app, shell, &url(&src)), pane),
        std::slice::from_ref(&src)
    );
    listed(&mut app, pane, &src);
    assert!(
        lists(&report(&mut app, shell, &url(&src)), pane).is_empty(),
        "the next prompt, the same folder"
    );
    // The user browses elsewhere: the next prompt in the same folder leaves it there.
    app.update(Message::Files(FilesMessage::Up {
        tab: pane,
        side: Side::Local,
    }));
    listed(&mut app, pane, dir.path());
    assert!(lists(&report(&mut app, shell, &url(&src)), pane).is_empty());
    let target = dir.path().join("target");
    assert_eq!(
        lists(&report(&mut app, shell, &url(&target)), pane),
        [target],
        "the shell moved"
    );
}

#[test]
fn a_folder_that_cannot_be_listed_is_said_and_the_browser_stays_where_it_was() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let sink = Arc::new(Sink::default());
    let (shell, pane) = local_shell(&mut app, dir.path(), &sink);
    let private = dir.path().join("private");
    assert_eq!(
        lists(&report(&mut app, shell, &url(&private)), pane),
        std::slice::from_ref(&private)
    );
    let refused = FilesError::Local {
        detail: "access denied".to_owned(),
    };
    let effects = app.update(Message::Files(FilesMessage::LocalListed {
        tab: pane,
        result: Err(refused.clone()),
    }));
    assert!(
        effects.is_empty(),
        "not the home folder instead: {effects:?}"
    );
    let local = &files(&app, pane).local;
    assert_eq!(local.path, dir.path(), "the folder shown stays");
    assert_eq!(local.error.as_ref(), Some(&refused), "said in the browser");
    assert!(local.history.is_empty(), "nowhere left");
    assert!(!local.loading);
    assert!(
        lists(&report(&mut app, shell, &url(&private)), pane).is_empty(),
        "not tried again at each prompt"
    );
}

#[test]
fn a_report_is_let_go_while_the_browser_lists_or_asks_and_the_next_one_followed() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let sink = Arc::new(Sink::default());
    let (shell, pane) = local_shell(&mut app, dir.path(), &sink);
    // Listing: let go.
    app.update(Message::Files(FilesMessage::Refresh {
        tab: pane,
        side: Side::Local,
    }));
    let var = dir.path().join("var");
    assert!(lists(&report(&mut app, shell, &url(&var)), pane).is_empty());
    listed(&mut app, pane, dir.path());
    assert_eq!(
        lists(&report(&mut app, shell, &url(&var)), pane),
        std::slice::from_ref(&var),
        "the next prompt"
    );
    listed(&mut app, pane, &var);
    // Asking for a new folder's name: the folder stays the one it was asked in.
    app.update(Message::Files(FilesMessage::AskNewFolder {
        tab: pane,
        side: Side::Local,
    }));
    let usr = dir.path().join("usr");
    assert!(lists(&report(&mut app, shell, &url(&usr)), pane).is_empty());
    assert_eq!(files(&app, pane).local.path, var);
    app.update(Message::DismissDialog);
    assert_eq!(lists(&report(&mut app, shell, &url(&usr)), pane), [usr]);
}

#[cfg(windows)]
#[test]
fn a_report_naming_no_folder_of_this_computer_is_left_alone() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let sink = Arc::new(Sink::default());
    let (shell, pane) = local_shell(&mut app, dir.path(), &sink);
    // A WSL shell's Linux folder, and a share named by its host alone.
    for elsewhere in ["file://wsl/home/admin", "file://server/share/x"] {
        assert!(
            report(&mut app, shell, elsewhere).is_empty(),
            "{elsewhere}: no folder here"
        );
    }
    assert_eq!(files(&app, pane).local.path, dir.path());
    let sub = dir.path().join("sub");
    assert_eq!(
        lists(&report(&mut app, shell, &url(&sub)), pane),
        [sub],
        "the next folder of this computer followed"
    );
}

#[cfg(windows)]
#[test]
fn a_conemu_report_moves_the_browser_as_osc_7_does() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let sink = Arc::new(Sink::default());
    let (shell, pane) = local_shell(&mut app, dir.path(), &sink);
    let sub = dir.path().join("my sub");
    let shown = sub.to_str().expect("UTF-8");
    let effects = conemu_report(&mut app, shell, shown);
    assert_eq!(lists(&effects, pane), std::slice::from_ref(&sub));
    assert_eq!(effects.len(), 1, "nothing else: {effects:?}");
    listed(&mut app, pane, &sub);
    assert_eq!(files(&app, pane).local.history, [dir.path().to_owned()]);
    // The same folder with forward slashes: the same folder, not listed again.
    let forward = shown.replace('\\', "/");
    assert!(lists(&conemu_report(&mut app, shell, &forward), pane).is_empty());
    assert!(sink.written().is_empty(), "nothing typed into the shell");
}

#[cfg(not(windows))]
#[test]
fn a_conemu_report_moves_nothing_on_a_unix_computer() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let sink = Arc::new(Sink::default());
    let (shell, pane) = local_shell(&mut app, dir.path(), &sink);
    let effects = conemu_report(&mut app, shell, "C:\\Users\\admin");
    assert!(effects.is_empty(), "a Windows path: {effects:?}");
    assert_eq!(files(&app, pane).local.path, dir.path());
}

#[test]
fn a_browser_taken_out_of_the_split_follows_no_shell() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    let sink = Arc::new(Sink::default());
    let (shell, pane) = local_shell(&mut app, dir.path(), &sink);
    app.update(Message::Split(SplitMessage::Unsplit(shell.0)));
    assert_eq!(app.panes_of(shell.0), [shell.0]);
    let effects = report(&mut app, shell, &url(&dir.path().join("sub")));
    assert!(lists(&effects, pane).is_empty(), "{effects:?}");
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

/// An SSH shell of profile `a` connected, writing into `sink`, and what that set off.
fn ssh_shell(app: &mut App, sink: &Arc<Sink>) -> ((TabId, AttemptId), Vec<Effect>) {
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
    ((tab, attempt), effects)
}

#[tokio::test]
async fn an_ssh_shells_report_drives_its_sftp_pane_alone_and_a_local_one_its_browser() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    settings(
        &mut app,
        SftpBrowser {
            follow_ssh_directory: true,
            ..SftpBrowser::default()
        },
    );
    let ssh_sink = Arc::new(Sink::default());
    let (ssh, effects) = ssh_shell(&mut app, &ssh_sink);
    let [
        Effect::Connect {
            tab: sftp,
            attempt: sftp_attempt,
            ..
        },
    ] = effects.as_slice()
    else {
        panic!("the SFTP pane's connection: {effects:?}");
    };
    let (sftp, sftp_attempt) = (*sftp, *sftp_attempt);
    app.update(Message::Connection {
        tab: sftp,
        attempt: sftp_attempt,
        event: ConnectionEvent::FilesReady {
            client: idle_client().await,
            shell: None,
        },
    });
    app.update(Message::Files(FilesMessage::RemoteListed {
        tab: sftp,
        result: Ok((RemotePath::from(REMOTE_HOME), Vec::new())),
    }));
    app.update(Message::Files(FilesMessage::LocalListed {
        tab: sftp,
        result: Ok((dir.path().to_owned(), Vec::new())),
    }));
    let sink = Arc::new(Sink::default());
    let (shell, pane) = local_shell(&mut app, dir.path(), &sink);
    let effects = report(&mut app, ssh, "file://a.lab/srv");
    assert_eq!(remote_lists(&effects), ["/srv"], "its SFTP pane");
    assert!(lists(&effects, pane).is_empty(), "{effects:?}");
    assert!(
        lists(&effects, sftp).is_empty(),
        "not the SFTP pane's local side"
    );
    assert_eq!(effects.len(), 1, "{effects:?}");
    let sub = dir.path().join("sub");
    let effects = report(&mut app, shell, &url(&sub));
    assert_eq!(lists(&effects, pane), [sub]);
    assert!(remote_lists(&effects).is_empty(), "{effects:?}");
    assert_eq!(effects.len(), 1, "{effects:?}");
    // A Windows path names no folder of the server: no SFTP pane follows OSC 9;9.
    let effects = conemu_report(&mut app, ssh, "C:/srv");
    assert!(effects.is_empty(), "{effects:?}");
    assert!(ssh_sink.written().is_empty() && sink.written().is_empty());
}

#[test]
fn a_local_browser_split_with_an_ssh_shell_never_follows_it() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path());
    settings(
        &mut app,
        SftpBrowser {
            auto_open_on_ssh: false,
            follow_ssh_directory: true,
            ..SftpBrowser::default()
        },
    );
    let ssh_sink = Arc::new(Sink::default());
    let (ssh, effects) = ssh_shell(&mut app, &ssh_sink);
    assert!(effects.is_empty(), "no SFTP pane: {effects:?}");
    let sink = Arc::new(Sink::default());
    let (shell, pane) = local_shell(&mut app, dir.path(), &sink);
    app.update(Message::Split(SplitMessage::Unsplit(shell.0)));
    app.update(Message::Split(SplitMessage::Merge {
        host: ssh.0,
        tab: pane,
        axis: Axis::SideBySide,
        placement: Placement::Second,
    }));
    assert_eq!(
        app.panes_of(ssh.0),
        [ssh.0, pane],
        "split with the SSH shell"
    );
    assert_eq!(follows(&app, pane), Some(true));
    let effects = report(&mut app, ssh, "file://a.lab/home");
    assert!(effects.is_empty(), "a server's path: {effects:?}");
    assert_eq!(files(&app, pane).local.path, dir.path());
    assert!(ssh_sink.written().is_empty());
}
