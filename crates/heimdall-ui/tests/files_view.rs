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
        vault_entry: None,
        forwards: heimdall_core::profile::Forwards::default(),
        post_connect: heimdall_core::post_connect::PostConnect::default(),
        forward_agent: false,
        compression: false,
        sftp: false,
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
        permissions: None,
        owner: None,
        group: None,
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

#[tokio::test]
async fn the_server_pane_shows_the_csharp_columns_and_a_header_sorts_by_its_column() {
    use heimdall_app::files::SortColumn;
    use std::time::{Duration, UNIX_EPOCH};

    let dir = tempfile::tempdir().expect("dir");
    let (core, tab) = files_tab(dir.path()).await;
    let mut shell = Shell::with_app(core);
    let _ = shell.update(Message::App(AppMessage::Files(
        FilesMessage::RemoteListed {
            tab,
            result: Ok((
                RemotePath::from("/srv"),
                vec![RemoteEntry {
                    name: b"run.sh".to_vec(),
                    label: "run.sh".to_owned(),
                    kind: EntryKind::File,
                    size: Some(10),
                    // 2026-09-27 19:15:03 UTC.
                    modified: Some(UNIX_EPOCH + Duration::from_secs(1_790_536_503)),
                    permissions: Some(0o4755),
                    owner: Some(1000),
                    group: None,
                }],
            )),
        },
    )));
    snapshot(&shell, "files-columns.png");
    {
        // Narrow: the last columns give way, the name stays readable.
        let mut ui = simulator(&shell);
        ui.click("run.sh").expect("the name, clickable");
        assert!(ui.find("Owner").is_err(), "given way");
    }
    {
        let settings = Settings {
            fonts: FONTS.iter().map(|face| (*face).into()).collect(),
            ..Settings::default()
        };
        let mut ui = Simulator::with_size(settings, Size::new(1800.0, 800.0), shell.view());
        for shown in [
            "2026-09-27 19:15",
            "rwsr-xr-x",
            "1000",
            "Permissions",
            "Owner",
        ] {
            ui.find(shown).expect(shown);
        }
        ui.find("Name \u{25b2}")
            .expect("sorted by name, the smallest first");
        ui.click("Permissions").expect("the header");
        let messages = files_messages(ui);
        assert!(
            messages.iter().any(|message| matches!(
                message,
                FilesMessage::SortBy {
                    side: Side::Remote,
                    column: SortColumn::Permissions,
                    ..
                }
            )),
            "{messages:?}"
        );
    }
    let _ = shell.update(Message::App(AppMessage::Files(FilesMessage::SortBy {
        tab,
        side: Side::Remote,
        column: SortColumn::Permissions,
    })));
    let _ = shell.update(Message::App(AppMessage::Files(FilesMessage::SortBy {
        tab,
        side: Side::Remote,
        column: SortColumn::Permissions,
    })));
    let settings = Settings {
        fonts: FONTS.iter().map(|face| (*face).into()).collect(),
        ..Settings::default()
    };
    let mut ui = Simulator::with_size(settings, Size::new(1800.0, 800.0), shell.view());
    ui.find("Permissions \u{25bc}").expect("the other way");
    assert!(
        ui.find("Name \u{25b2}").is_ok(),
        "this computer's pane keeps its own sort"
    );
}

#[tokio::test]
async fn a_right_click_selects_an_entry_and_opens_the_csharp_menu() {
    use heimdall_ui::tree_view::TreeMenu;
    use iced::mouse;

    let dir = tempfile::tempdir().expect("dir");
    let (core, tab) = files_tab(dir.path()).await;
    let mut shell = Shell::with_app(core);
    let entry = TreeMenu::FilesEntry {
        tab,
        side: Side::Remote,
        index: 1,
    };
    {
        let mut ui = simulator(&shell);
        let name = ui.find("backup.tar.gz").expect("the file");
        ui.point_at(name.bounds().center());
        ui.simulate([
            iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)),
            iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Right)),
        ]);
        assert!(
            ui.into_messages().any(
                |message| matches!(message, Message::OpenTreeMenu(ref menu) if *menu == entry)
            )
        );
    }
    let _ = shell.update(Message::OpenTreeMenu(entry.clone()));
    let selected = |shell: &Shell| {
        shell
            .app()
            .tab(tab)
            .expect("tab")
            .files
            .as_ref()
            .expect("files")
            .remote
            .selected
    };
    assert_eq!(selected(&shell), Some(1), "selected, as in the C# tab");
    snapshot(&shell, "files-menu.png");
    // A right click on a folder already selected leaves it closed.
    let _ = shell.update(Message::App(AppMessage::Files(FilesMessage::Select {
        tab,
        side: Side::Remote,
        index: 0,
    })));
    let _ = shell.update(Message::OpenTreeMenu(TreeMenu::FilesEntry {
        tab,
        side: Side::Remote,
        index: 0,
    }));
    assert_eq!(selected(&shell), Some(0));
    let path = shell
        .app()
        .tab(tab)
        .expect("tab")
        .files
        .as_ref()
        .expect("files")
        .remote
        .path
        .display();
    assert_eq!(path, "/home/admin", "not opened");
    // Nor for an entry no longer listed.
    let _ = shell.update(Message::OpenTreeMenu(TreeMenu::FilesEntry {
        tab,
        side: Side::Remote,
        index: 9,
    }));
    assert!(simulator(&shell).find("Copy path").is_err());
}

#[tokio::test]
async fn this_computers_menu_uploads_what_the_servers_downloads() {
    let dir = tempfile::tempdir().expect("dir");
    let (_core, tab) = files_tab(dir.path()).await;
    let settings = Settings {
        fonts: FONTS.iter().map(|face| (*face).into()).collect(),
        ..Settings::default()
    };
    let mut ui = Simulator::with_size(
        settings,
        WINDOW,
        heimdall_ui::tree_view::files_entry_menu(tab, Side::Local, 0),
    );
    assert!(ui.find("Download").is_err());
    ui.click("Upload").expect("Upload");
    assert!(ui.into_messages().any(|message| matches!(
        message,
        Message::MenuChoice(AppMessage::Files(FilesMessage::Transfer {
            direction: Direction::Upload,
            ..
        }))
    )));
}

#[tokio::test]
async fn after_an_entrys_menu_the_arrows_still_move_through_the_files() {
    use heimdall_app::files::FilesKey;
    use heimdall_ui::tree_view::TreeMenu;

    let dir = tempfile::tempdir().expect("dir");
    let (core, tab) = files_tab(dir.path()).await;
    let mut shell = Shell::with_app(core);
    let _ = shell.update(Message::OpenTreeMenu(TreeMenu::FilesEntry {
        tab,
        side: Side::Remote,
        index: 0,
    }));
    let _ = shell.update(Message::CloseTreeMenu);
    let _ = shell.update(Message::FilesKey(FilesKey::Next));
    let files = shell
        .app()
        .tab(tab)
        .expect("tab")
        .files
        .as_ref()
        .expect("files");
    assert_eq!(
        files.remote.selected,
        Some(1),
        "the Files tab's, not the tree's"
    );
}

#[tokio::test]
async fn the_servers_entry_menu_asks_for_what_the_csharp_one_does() {
    let dir = tempfile::tempdir().expect("dir");
    let (_core, tab) = files_tab(dir.path()).await;
    for (label, expected) in [
        (
            "Open",
            format!(
                "{:?}",
                FilesMessage::Open {
                    tab,
                    side: Side::Remote,
                    index: 1
                }
            ),
        ),
        (
            "Download",
            format!(
                "{:?}",
                FilesMessage::Transfer {
                    tab,
                    direction: Direction::Download
                }
            ),
        ),
        (
            "Rename",
            format!(
                "{:?}",
                FilesMessage::AskRename {
                    tab,
                    side: Side::Remote
                }
            ),
        ),
        (
            "Copy path",
            format!(
                "{:?}",
                FilesMessage::CopyPath {
                    tab,
                    side: Side::Remote
                }
            ),
        ),
        (
            "New Folder",
            format!(
                "{:?}",
                FilesMessage::AskNewFolder {
                    tab,
                    side: Side::Remote
                }
            ),
        ),
    ] {
        // The menu alone: the pane has buttons named as some of its entries.
        let settings = Settings {
            fonts: FONTS.iter().map(|face| (*face).into()).collect(),
            ..Settings::default()
        };
        let mut ui = Simulator::with_size(
            settings,
            WINDOW,
            heimdall_ui::tree_view::files_entry_menu(tab, Side::Remote, 1),
        );
        ui.click(label).expect(label);
        let chosen: Vec<String> = ui
            .into_messages()
            .filter_map(|message| match message {
                Message::MenuChoice(AppMessage::Files(files)) => Some(format!("{files:?}")),
                _ => None,
            })
            .collect();
        assert_eq!(chosen, [expected], "{label}");
    }
}

#[tokio::test]
async fn the_servers_menu_offers_permissions_and_properties_and_their_dialogs_show() {
    let dir = tempfile::tempdir().expect("dir");
    let (core, tab) = files_tab(dir.path()).await;
    let settings = || Settings {
        fonts: FONTS.iter().map(|face| (*face).into()).collect(),
        ..Settings::default()
    };
    let menu = |side| {
        Simulator::with_size(
            settings(),
            WINDOW,
            heimdall_ui::tree_view::files_entry_menu(tab, side, 0),
        )
    };
    let mut remote = menu(Side::Remote);
    remote.click("Change permissions...").expect("permissions");
    assert!(remote.into_messages().any(|message| matches!(
        message,
        Message::MenuChoice(AppMessage::Files(FilesMessage::AskPermissions {
            side: Side::Remote,
            ..
        }))
    )));
    let mut remote = menu(Side::Remote);
    remote.click("Properties").expect("properties");
    assert!(remote.into_messages().any(|message| matches!(
        message,
        Message::MenuChoice(AppMessage::Files(FilesMessage::ShowProperties { .. }))
    )));
    let mut local = menu(Side::Local);
    assert!(
        local.find("Change permissions...").is_err(),
        "the server's only"
    );
    assert!(local.find("Properties").is_err());

    let mut shell = Shell::with_app(core);
    let _ = shell.update(Message::App(AppMessage::Files(
        FilesMessage::RemoteListed {
            tab,
            result: Ok((
                RemotePath::from("/srv"),
                vec![RemoteEntry {
                    name: b"run.sh".to_vec(),
                    label: "run.sh".to_owned(),
                    kind: EntryKind::File,
                    size: Some(10),
                    modified: None,
                    permissions: Some(0o755),
                    owner: Some(1000),
                    group: Some(50),
                }],
            )),
        },
    )));
    let _ = shell.update(Message::App(AppMessage::Files(FilesMessage::Select {
        tab,
        side: Side::Remote,
        index: 0,
    })));
    let _ = shell.update(Message::App(AppMessage::Files(
        FilesMessage::ShowProperties {
            tab,
            side: Side::Remote,
        },
    )));
    snapshot(&shell, "files-properties.png");
    {
        let mut ui = simulator(&shell);
        for shown in [
            "Properties - run.sh",
            "rwxr-xr-x (755)",
            "/srv/run.sh",
            "50",
            "File",
        ] {
            ui.find(shown).expect(shown);
        }
    }
    let _ = shell.update(Message::App(AppMessage::DismissDialog));
    let _ = shell.update(Message::App(AppMessage::Files(
        FilesMessage::AskPermissions {
            tab,
            side: Side::Remote,
        },
    )));
    let mut ui = simulator(&shell);
    ui.find("Change Permissions").expect("its title");
    ui.find("Permissions (octal, e.g. 755):")
        .expect("its label");
}

#[tokio::test]
async fn ctrl_and_shift_clicks_select_several_entries_and_the_pane_says_how_many() {
    use iced::keyboard::Modifiers;

    let dir = tempfile::tempdir().expect("dir");
    let (core, tab) = files_tab(dir.path()).await;
    let mut shell = Shell::with_app(core);
    let click = |shell: &mut Shell, index| {
        let _ = shell.update(Message::App(AppMessage::Files(FilesMessage::Select {
            tab,
            side: Side::Local,
            index,
        })));
    };
    let chosen = |shell: &Shell| {
        shell
            .app()
            .tab(tab)
            .expect("tab")
            .files
            .as_ref()
            .expect("files")
            .local
            .chosen()
    };
    click(&mut shell, 1);
    let _ = shell.update(Message::Modifiers(Modifiers::CTRL));
    click(&mut shell, 0);
    assert_eq!(chosen(&shell), [0, 1], "Ctrl adds");
    snapshot(&shell, "files-several.png");
    {
        let mut ui = simulator(&shell);
        ui.find("2 selected").expect("counted");
        ui.click("Rename").expect("the button");
        assert!(
            files_messages(ui).is_empty(),
            "renaming is for one entry: greyed out"
        );
    }
    let _ = shell.update(Message::Modifiers(Modifiers::empty()));
    click(&mut shell, 1);
    assert_eq!(chosen(&shell), [1], "a plain click, one again");
    {
        let mut ui = simulator(&shell);
        assert!(ui.find("2 selected").is_err());
        assert!(ui.find("1 selected").is_err(), "one is not counted");
    }
    let _ = shell.update(Message::Modifiers(Modifiers::SHIFT));
    click(&mut shell, 0);
    assert_eq!(chosen(&shell), [0, 1], "Shift takes the range");
}

#[tokio::test]
async fn the_servers_pane_bookmarks_its_folder_and_lists_the_bookmarks() {
    use heimdall_ui::tree_view::TreeMenu;

    let dir = tempfile::tempdir().expect("dir");
    let (core, tab) = files_tab(dir.path()).await;
    let mut shell = Shell::with_app(core);
    {
        let mut ui = simulator(&shell);
        let found = ui.find("Bookmark this path").expect("shown");
        assert!(
            found.bounds().x > WINDOW.width / 2.0,
            "the first found is the server's pane's, on the right: this computer's has none"
        );
        ui.click("Bookmark this path")
            .expect("the server's pane only");
        assert!(
            files_messages(ui)
                .iter()
                .any(|message| matches!(message, FilesMessage::Bookmark { .. }))
        );
    }
    {
        let mut ui = simulator(&shell);
        ui.click("Bookmarks").expect("its list");
        assert!(ui.into_messages().any(|message| matches!(
            message,
            Message::OpenTreeMenu(TreeMenu::FilesBookmarks(opened)) if opened == tab
        )));
    }
    let _ = shell.update(Message::OpenTreeMenu(TreeMenu::FilesBookmarks(tab)));
    simulator(&shell)
        .find("No bookmarks saved")
        .expect("none yet");
    let _ = shell.update(Message::CloseTreeMenu);
    let _ = shell.update(Message::App(AppMessage::Files(FilesMessage::Bookmark {
        tab,
    })));
    simulator(&shell)
        .find("Bookmark added: /home/admin")
        .expect("said");
    // The menu alone: the path bar holds the same text.
    let settings = Settings {
        fonts: FONTS.iter().map(|face| (*face).into()).collect(),
        ..Settings::default()
    };
    let mut ui = Simulator::with_size(
        settings,
        WINDOW,
        heimdall_ui::tree_view::files_bookmarks_menu(tab, &["/home/admin".to_owned()]),
    );
    assert!(ui.find("No bookmarks saved").is_err());
    ui.click("/home/admin").expect("the bookmark");
    let chosen: Vec<Message> = ui
        .into_messages()
        .filter(|message| matches!(message, Message::MenuChoice(_)))
        .collect();
    assert!(
        matches!(
            chosen.as_slice(),
            [Message::MenuChoice(AppMessage::Files(
                FilesMessage::OpenBookmark { index: 0, .. }
            ))]
        ),
        "{chosen:?}"
    );
}

/// The RGBA pixel at logical `(x, y)` of `view` drawn alone in the dark theme.
fn pixel_of(view: iced::Element<'_, Message>, x: u32, y: u32) -> [u8; 4] {
    /// Physical pixels per logical pixel in a simulator snapshot.
    const SNAPSHOT_SCALE: u32 = 2;

    let dir = tempfile::tempdir().expect("dir");
    let settings = Settings {
        fonts: FONTS.iter().map(|face| (*face).into()).collect(),
        ..Settings::default()
    };
    Simulator::with_size(settings, WINDOW, view)
        .snapshot(&iced::Theme::Dark)
        .expect("drawn")
        .matches_image(dir.path().join("menu.png"))
        .expect("written");
    // iced names the file after its renderer.
    let entry = std::fs::read_dir(dir.path())
        .expect("listed")
        .flatten()
        .next()
        .expect("one snapshot");
    let decoder = png::Decoder::new(std::io::BufReader::new(
        std::fs::File::open(entry.path()).expect("opened"),
    ));
    let mut reader = decoder.read_info().expect("header");
    let mut bytes = vec![0; reader.output_buffer_size().expect("size")];
    let info = reader.next_frame(&mut bytes).expect("frame");
    let at = usize::try_from((y * SNAPSHOT_SCALE) * info.width * 4 + (x * SNAPSHOT_SCALE) * 4)
        .expect("index");
    [bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]
}

#[tokio::test]
async fn the_files_menus_are_drawn_on_a_card_that_hides_what_is_under_them() {
    let dir = tempfile::tempdir().expect("dir");
    let (_core, tab) = files_tab(dir.path()).await;
    let window = pixel_of(iced::widget::space().into(), 2, 20);
    // In the card's margin, left of the entries: the card's colour, not the window's, or
    // the listing under the menu shows through its entries.
    let entry_menu = pixel_of(
        heimdall_ui::tree_view::files_entry_menu(tab, Side::Remote, 0),
        2,
        20,
    );
    assert_ne!(entry_menu, window, "the entry menu has no card");
    let bookmarks = pixel_of(
        heimdall_ui::tree_view::files_bookmarks_menu(tab, &["/home/admin".to_owned()]),
        2,
        20,
    );
    assert_ne!(bookmarks, window, "the bookmarks menu has no card");
}

#[tokio::test]
async fn a_narrow_window_wraps_the_servers_buttons_instead_of_cutting_them() {
    // At this width one row of the server's buttons runs past the window's edge, as the
    // hand test saw "Bookmarks" cut in half.
    let dir = tempfile::tempdir().expect("dir");
    let (core, _) = files_tab(dir.path()).await;
    let shell = Shell::with_app(core);
    let mut ui = simulator(&shell);
    let bookmark = ui.find("Bookmark this path").expect("shown").bounds();
    let bookmarks = ui.find("Bookmarks").expect("shown").bounds();
    assert!(
        bookmarks.x + bookmarks.width <= WINDOW.width,
        "whole in the window: {bookmarks:?}"
    );
    assert!(
        bookmarks.y > bookmark.y,
        "on the next line: {bookmark:?} then {bookmarks:?}"
    );
}

#[tokio::test]
async fn a_pane_filters_its_entries_hides_dot_names_and_counts_them() {
    let dir = tempfile::tempdir().expect("dir");
    let (core, tab) = files_tab(dir.path()).await;
    let mut shell = Shell::with_app(core);
    {
        let mut ui = simulator(&shell);
        ui.find("2 items").expect("counted");
        ui.click("Filter files...").expect("the filter");
        ui.typewrite("x");
        assert!(files_messages(ui).iter().any(|message| matches!(
            message,
            FilesMessage::Filter { side: Side::Local, text, .. } if text == "x"
        )));
    }
    {
        let mut ui = simulator(&shell);
        ui.click(".*").expect("the toggle");
        assert!(files_messages(ui).iter().any(|message| matches!(
            message,
            FilesMessage::ToggleHidden {
                side: Side::Local,
                ..
            }
        )));
    }
    let _ = shell.update(Message::App(AppMessage::Files(FilesMessage::Filter {
        tab,
        side: Side::Remote,
        text: "LOG".to_owned(),
    })));
    let mut ui = simulator(&shell);
    ui.find("1/2 items").expect("shown of listed");
    assert!(ui.find("backup.tar.gz").is_err(), "filtered out");
    ui.find("logs/").expect("kept");
}

#[tokio::test]
async fn files_dropped_on_a_files_tab_are_uploaded_and_said_while_dragged() {
    let dir = tempfile::tempdir().expect("dir");
    let (core, tab) = files_tab(dir.path()).await;
    let mut shell = Shell::with_app(core);
    assert!(simulator(&shell).find("Drop files to upload").is_err());
    let _ = shell.update(Message::FilesHovered(true));
    snapshot(&shell, "files-drop.png");
    simulator(&shell)
        .find("Drop files to upload")
        .expect("said while dragged");
    let _ = shell.update(Message::FilesHovered(false));
    assert!(simulator(&shell).find("Drop files to upload").is_err());

    let dropped = dir.path().join("dropped.txt");
    std::fs::write(&dropped, b"x").expect("written");
    let _ = shell.update(Message::FilesHovered(true));
    let _ = shell.update(Message::FileDropped(dropped));
    let files = shell
        .app()
        .tab(tab)
        .expect("tab")
        .files
        .as_ref()
        .expect("files");
    assert_eq!(files.transfers.len(), 1, "sent");
    assert!(
        simulator(&shell).find("Drop files to upload").is_err(),
        "dropped: no longer said"
    );

    // Not while the Settings page shows.
    let _ = shell.update(Message::ShowSettings);
    let _ = shell.update(Message::FilesHovered(true));
    assert!(simulator(&shell).find("Drop files to upload").is_err());
    let again = dir.path().join("again.txt");
    std::fs::write(&again, b"x").expect("written");
    let _ = shell.update(Message::FileDropped(again));
    let files = shell
        .app()
        .tab(tab)
        .expect("tab")
        .files
        .as_ref()
        .expect("files");
    assert_eq!(files.transfers.len(), 1, "nothing more");
}
