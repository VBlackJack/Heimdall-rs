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

//! The Files tab drawn headless: both panes, what a click does, the transfer list, and the\r
//! name and delete questions.
//! Setting `HEIMDALL_SNAPSHOT_DIR` writes a PNG of the tab, for a visual pass.

use std::path::{Path, PathBuf};

use heimdall_app::files::{Direction, EntryKind, LocalEntry, RemoteEntry, Side, TransferEvent};
use heimdall_app::{
    App, AppConfig, ConnectionEvent, Effect, FilesMessage, Message as AppMessage, TabId,
};
use heimdall_core::profile::{ProfileId, SshProfile};
use heimdall_core::store::ProfileStore;
use heimdall_files::RemoteSession;
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
        system_credentials: heimdall_app::SystemCredentials::memory(),
    })
}

fn remote(name: &str, kind: EntryKind, size: u64) -> RemoteEntry {
    RemoteEntry {
        name: name.as_bytes().to_vec(),
        label: name.to_owned(),
        kind,
        size: Some(size),
        modified: None,
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
    // iced names the picture after its renderer (`name-wgpu.png`): clear every variant, or
    // an old picture is only compared against and never replaced.
    let stem = name.trim_end_matches(".png");
    if let Ok(entries) = std::fs::read_dir(Path::new(&dir)) {
        for entry in entries.flatten() {
            let file = entry.file_name();
            let file = file.to_string_lossy();
            if file == name || file.starts_with(&format!("{stem}-")) {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
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
async fn the_tree_menu_opens_a_files_tab_through_connect_as_sftp() {
    use heimdall_core::profile::ProfileId;
    use heimdall_ui::tree_view::TreeMenu;

    let dir = tempfile::tempdir().expect("dir");
    let mut shell = Shell::with_app(app(dir.path()));
    let id = ProfileId::new("a");
    // As in the C# tree: right click, "Connect as...", "SFTP".
    let _ = shell.update(Message::OpenTreeMenu(TreeMenu::Profile(id.clone())));
    {
        let mut ui = simulator(&shell);
        ui.click("Connect as...").expect("connect as");
        assert!(ui.into_messages().any(|message| matches!(
            &message,
            Message::OpenTreeMenu(TreeMenu::ConnectAs(asked)) if *asked == id
        )));
    }
    let _ = shell.update(Message::OpenTreeMenu(TreeMenu::ConnectAs(id.clone())));
    let mut ui = simulator(&shell);
    ui.click("SFTP").expect("sftp");
    assert!(ui.into_messages().any(|message| matches!(
        &message,
        Message::MenuChoice(AppMessage::ConnectAs {
            id: opened,
            protocol: heimdall_app::ConnectAs::Sftp,
        }) if *opened == id
    )));
}

#[tokio::test]
async fn both_panes_show_their_folders_and_a_folder_click_selects_it() {
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
            [FilesMessage::Select { tab: t, side: Side::Remote, index: 0 }] if *t == tab
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

#[tokio::test]
async fn the_name_question_takes_typing_and_enter_confirms() {
    let dir = tempfile::tempdir().expect("dir");
    let (core, tab) = files_tab(dir.path()).await;
    let mut shell = Shell::with_app(core);
    let _ = shell.update(Message::App(AppMessage::Files(
        FilesMessage::AskNewFolder {
            tab,
            side: Side::Remote,
        },
    )));
    snapshot(&shell, "files-new-folder.png");
    let mut ui = simulator(&shell);
    ui.find("New folder").expect("title");
    ui.click("Name").expect("name field");
    ui.typewrite("fresh");
    ui.tap_key(iced::keyboard::key::Named::Enter);
    let messages: Vec<Message> = ui.into_messages().collect();
    assert!(
        messages.iter().any(|message| matches!(
            message,
            Message::App(AppMessage::Files(FilesMessage::NameEdited(value))) if value == "f"
        )),
        "{messages:?}"
    );
    assert!(
        messages
            .iter()
            .any(|message| matches!(message, Message::App(AppMessage::ConfirmDialog))),
        "{messages:?}"
    );
}

#[tokio::test]
async fn a_folder_delete_says_everything_in_it_goes() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut core, tab) = files_tab(dir.path()).await;
    core.update(AppMessage::Files(FilesMessage::Select {
        tab,
        side: Side::Remote,
        index: 0,
    }));
    core.update(AppMessage::Files(FilesMessage::AskDelete {
        tab,
        side: Side::Remote,
    }));
    let shell = Shell::with_app(core);
    snapshot(&shell, "files-delete.png");
    let mut ui = simulator(&shell);
    ui.find("The folder logs and everything in it will be deleted. Links inside are removed, never what they point to. This cannot be undone.")
        .expect("body");
    ui.find("Delete?").expect("title");
}

#[tokio::test]
async fn keys_reach_the_files_tab_and_enter_opens_the_selection() {
    let dir = tempfile::tempdir().expect("dir");
    let (core, tab) = files_tab(dir.path()).await;
    let mut shell = Shell::with_app(core);
    let _ = shell.update(Message::FilesKey(heimdall_app::files::FilesKey::Focus(
        Side::Remote,
    )));
    let _ = shell.update(Message::FilesKey(heimdall_app::files::FilesKey::Next));
    let files = shell
        .app()
        .tab(tab)
        .and_then(|t| t.files.as_ref())
        .expect("files");
    assert_eq!(files.remote.selected, Some(0));
    snapshot(&shell, "files-keyboard.png");
    // Enter with no dialog open: the selected folder opens.
    let _ = shell.update(Message::DialogKey { confirm: true });
    let files = shell
        .app()
        .tab(tab)
        .and_then(|t| t.files.as_ref())
        .expect("files");
    assert_eq!(files.remote.path.as_bytes(), b"/home/admin/logs");
    assert!(files.remote.loading, "the folder is being listed");
}

#[tokio::test]
async fn no_widget_of_the_files_tab_takes_its_keys() {
    let dir = tempfile::tempdir().expect("dir");
    let (core, _tab) = files_tab(dir.path()).await;
    let shell = Shell::with_app(core);
    let mut ui = simulator(&shell);
    for key in [
        iced::keyboard::key::Named::ArrowDown,
        iced::keyboard::key::Named::Tab,
        iced::keyboard::key::Named::F2,
        iced::keyboard::key::Named::Delete,
        iced::keyboard::key::Named::Enter,
    ] {
        assert_eq!(
            ui.tap_key(key),
            iced::event::Status::Ignored,
            "{key:?} reaches the window"
        );
    }
}

#[tokio::test]
async fn the_path_bar_is_typed_over_and_enter_goes_there() {
    let dir = tempfile::tempdir().expect("dir");
    let (core, tab) = files_tab(dir.path()).await;
    let mut shell = Shell::with_app(core);
    {
        let mut ui = simulator(&shell);
        ui.click("/home/admin").expect("the remote path bar");
        ui.typewrite("/x");
        let _ = ui.tap_key(iced::keyboard::key::Named::Enter);
        let messages = files_messages(ui);
        assert!(
            messages.iter().any(|message| matches!(
                message,
                FilesMessage::PathEdited { tab: t, side: Side::Remote, text } if *t == tab && text.ends_with("/x")
            )),
            "{messages:?}"
        );
        assert!(
            messages.iter().any(|message| matches!(
                message,
                FilesMessage::GoTo { tab: t, side: Side::Remote } if *t == tab
            )),
            "{messages:?}"
        );
    }
    // What is typed shows until gone to, and Go is offered only then.
    let mut ui = simulator(&shell);
    ui.click("Go").expect("Go");
    assert!(files_messages(ui).is_empty(), "nothing typed: greyed out");
    let _ = shell.update(Message::App(AppMessage::Files(FilesMessage::PathEdited {
        tab,
        side: Side::Local,
        text: "elsewhere".to_owned(),
    })));
    let mut ui = simulator(&shell);
    ui.find("elsewhere").expect("typed, shown");
    ui.click("Go").expect("Go");
    assert!(files_messages(ui).iter().any(|message| matches!(
        message,
        FilesMessage::GoTo {
            side: Side::Local,
            ..
        }
    )));
}
