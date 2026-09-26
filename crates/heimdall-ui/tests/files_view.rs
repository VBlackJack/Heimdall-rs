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

//! The Files tab drawn headless: both panes, what a click does, and the transfer list.
//! Setting `HEIMDALL_SNAPSHOT_DIR` writes a PNG of the tab, for a visual pass.

use std::path::{Path, PathBuf};

use heimdall_app::files::{Direction, EntryKind, LocalEntry, RemoteEntry, Side, TransferEvent};
use heimdall_app::{
    App, AppConfig, ConnectionEvent, Effect, FilesMessage, Message as AppMessage, TabId,
};
use heimdall_core::profile::{ProfileId, SshProfile};
use heimdall_core::store::ProfileStore;
use heimdall_sftp::protocol::{Request, Response, SFTP_VERSION};
use heimdall_sftp::{ClientConfig, RemotePath, SftpClient};
use heimdall_ssh::AgentSource;
use heimdall_term::GridSize;
use heimdall_ui::shell::{Message, Shell};
use heimdall_ui::terminal_view::FONTS;
use iced::{Settings, Size};
use iced_test::simulator::Simulator;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

/// Size of the simulated window, in logical pixels.
const WINDOW: Size = Size::new(1200.0, 720.0);

/// Environment variable naming a directory for PNG snapshots.
const SNAPSHOT_VARIABLE: &str = "HEIMDALL_SNAPSHOT_DIR";

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

fn remote(name: &str, kind: EntryKind, size: u64) -> RemoteEntry {
    RemoteEntry {
        name: name.as_bytes().to_vec(),
        label: name.to_owned(),
        kind,
        size: Some(size),
        modified: None,
        permissions: None,
    }
}

fn local(name: &str, kind: EntryKind) -> LocalEntry {
    LocalEntry {
        name: name.into(),
        label: name.to_owned(),
        kind,
        size: Some(2048),
        modified: None,
    }
}

/// A Files tab with both panes listed.
async fn files_tab(dir: &Path) -> (App, TabId) {
    let mut core = app(dir);
    let (tab, attempt) = match core
        .update(AppMessage::OpenFiles(ProfileId::new("a")))
        .as_slice()
    {
        [Effect::Connect { tab, attempt, .. }] => (*tab, *attempt),
        other => panic!("{other:?}"),
    };
    core.update(AppMessage::Connection {
        tab,
        attempt,
        event: ConnectionEvent::FilesReady {
            client: idle_client().await,
        },
    });
    core.update(AppMessage::Files(FilesMessage::RemoteListed {
        tab,
        result: Ok((
            RemotePath::from("/home/admin"),
            vec![
                remote("logs", EntryKind::Directory, 0),
                remote("backup.tar.gz", EntryKind::File, 5 * 1024 * 1024 + 99),
            ],
        )),
    }));
    core.update(AppMessage::Files(FilesMessage::LocalListed {
        tab,
        result: Ok((
            PathBuf::from(dir),
            vec![
                local("Documents", EntryKind::Directory),
                local("notes.md", EntryKind::File),
            ],
        )),
    }));
    (core, tab)
}

fn simulator(shell: &Shell) -> Simulator<'_, Message> {
    let settings = Settings {
        fonts: FONTS.iter().map(|face| (*face).into()).collect(),
        ..Settings::default()
    };
    Simulator::with_size(settings, WINDOW, shell.view())
}

fn snapshot(shell: &Shell, name: &str) {
    let Some(dir) = std::env::var_os(SNAPSHOT_VARIABLE) else {
        return;
    };
    let path = Path::new(&dir).join(name);
    let _ = std::fs::remove_file(&path);
    simulator(shell)
        .snapshot(&shell.theme())
        .expect("drawn")
        .matches_image(path)
        .expect("written");
}

fn files_messages(ui: Simulator<'_, Message>) -> Vec<FilesMessage> {
    ui.into_messages()
        .filter_map(|message| match message {
            Message::App(AppMessage::Files(files)) => Some(files),
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn the_sidebar_opens_a_files_tab() {
    let dir = tempfile::tempdir().expect("dir");
    let shell = Shell::with_app(app(dir.path()));
    let mut ui = simulator(&shell);
    ui.click("Files").expect("files button");
    assert!(ui.into_messages().any(
        |message| matches!(message, Message::App(AppMessage::OpenFiles(id)) if id.to_string() == "a")
    ));
}

#[tokio::test]
async fn both_panes_show_their_folders_and_a_folder_click_opens_it() {
    let dir = tempfile::tempdir().expect("dir");
    let (core, tab) = files_tab(dir.path()).await;
    let shell = Shell::with_app(core);
    snapshot(&shell, "files.png");
    let mut ui = simulator(&shell);
    ui.find("server a (files)").expect("tab title");
    ui.find("This computer").expect("local pane");
    ui.find("Server").expect("remote pane");
    ui.find("/home/admin").expect("remote path");
    ui.find("5.0 MiB").expect("size");
    ui.click("logs/").expect("folder");
    let messages = files_messages(ui);
    assert!(
        matches!(
            messages.as_slice(),
            [FilesMessage::Open { tab: t, side: Side::Remote, index: 0 }] if *t == tab
        ),
        "{messages:?}"
    );
}

#[tokio::test]
async fn a_file_click_selects_and_the_button_sends_it() {
    let dir = tempfile::tempdir().expect("dir");
    let (core, tab) = files_tab(dir.path()).await;
    let mut shell = Shell::with_app(core);
    {
        let mut ui = simulator(&shell);
        ui.click("Download").ok();
        ui.click("backup.tar.gz").expect("file");
        let messages = files_messages(ui);
        assert!(
            matches!(
                messages.as_slice(),
                [FilesMessage::Select {
                    side: Side::Remote,
                    index: 1,
                    ..
                }]
            ),
            "the button does nothing without a selection: {messages:?}"
        );
    }
    let _ = shell.update(Message::App(AppMessage::Files(FilesMessage::Select {
        tab,
        side: Side::Remote,
        index: 1,
    })));
    let mut ui = simulator(&shell);
    ui.click("Download").expect("download button");
    assert!(matches!(
        files_messages(ui).as_slice(),
        [FilesMessage::Transfer {
            direction: Direction::Download,
            ..
        }]
    ));
}

#[tokio::test]
async fn a_running_transfer_shows_its_progress_and_can_be_cancelled() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut core, tab) = files_tab(dir.path()).await;
    let effects = core.update(AppMessage::Files(FilesMessage::Open {
        tab,
        side: Side::Remote,
        index: 1,
    }));
    let [Effect::Transfer { id, .. }] = effects.as_slice() else {
        panic!("{effects:?}")
    };
    let id = *id;
    core.update(AppMessage::Files(FilesMessage::TransferEvent {
        tab,
        id,
        event: TransferEvent::Progress(1024 * 1024),
    }));
    let shell = Shell::with_app(core);
    snapshot(&shell, "files-transfer.png");
    let mut ui = simulator(&shell);
    ui.find("Download of backup.tar.gz").expect("transfer row");
    ui.find("1.0 MiB of 5.0 MiB").expect("progress");
    ui.click("Cancel").expect("cancel button");
    assert!(matches!(
        files_messages(ui).as_slice(),
        [FilesMessage::Cancel { id: cancelled, .. }] if *cancelled == id
    ));
}
