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

mod common;

use std::path::{Path, PathBuf};

use heimdall_app::files::{
    Direction, EntryKind, FilesKey, LocalEntry, RemoteEntry, Side, TransferEvent, TransferState,
    plan_transfer,
};
use heimdall_app::{
    App, AppConfig, AttemptId, ConnectionEvent, Effect, FilesMessage, Message as AppMessage, TabId,
    UiError,
};
use heimdall_core::profile::{ProfileId, SshProfile};
use heimdall_core::store::ProfileStore;
use heimdall_files::RemoteSession;
use heimdall_sftp::protocol::{Request, Response, SFTP_VERSION};
use heimdall_sftp::{ClientConfig, RemotePath, SftpClient};
use heimdall_ssh::AgentSource;
use heimdall_term::GridSize;
use heimdall_ui::drop_batch::DropPlace;
use heimdall_ui::floating_view::{PaneTool, tool_id};
use heimdall_ui::shell::{Message, Shell};
use heimdall_ui::terminal_view::FONTS;
use iced::{Settings, Size};
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
        inode: None,
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
    let (core, tab, _) = files_tab_attempt(dir).await;
    (core, tab)
}

/// A Files tab with both panes listed, and the attempt its session came from.
async fn files_tab_attempt(dir: &Path) -> (App, TabId, AttemptId) {
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
            shell: None,
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
    (core, tab, attempt)
}

fn simulator(shell: &Shell) -> common::Drawn<'_> {
    let settings = Settings {
        fonts: FONTS.iter().map(|face| (*face).into()).collect(),
        ..Settings::default()
    };
    common::simulator(settings, WINDOW, shell.view())
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

fn files_messages(ui: common::Drawn<'_>) -> Vec<FilesMessage> {
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
        assert!(
            ui.find("Connect with...").is_err(),
            "the RDP modes are an RDP profile's"
        );
        ui.click("Connect as...").expect("connect as");
        assert!(ui.into_messages().any(|message| matches!(
            &message,
            Message::OpenTreeMenu(TreeMenu::ConnectAs(asked)) if *asked == id
        )));
    }
    // Asked for all the same, an SSH profile's is not drawn.
    let _ = shell.update(Message::OpenTreeMenu(TreeMenu::ConnectWith(id.clone())));
    {
        let mut ui = simulator(&shell);
        assert!(ui.find("Connect (embedded)").is_err());
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
async fn show_hidden_is_the_csharp_check_box_ticked_in_a_new_pane_and_unticked_it_hides_dot_names()
{
    let dir = tempfile::tempdir().expect("dir");
    let (mut core, tab) = files_tab(dir.path()).await;
    core.update(AppMessage::Files(FilesMessage::LocalListed {
        tab,
        result: Ok((
            PathBuf::from(dir.path()),
            vec![
                local(".gitconfig", EntryKind::File),
                local("notes.md", EntryKind::File),
            ],
        )),
    }));
    let mut shell = Shell::with_app(core);
    let toggled = {
        let mut ui = simulator(&shell);
        ui.find(".gitconfig")
            .expect("shown: a pane opens with the box ticked, as the C# ShowHidden");
        ui.click(".*").expect("the check box, by its C# label");
        files_messages(ui)
            .into_iter()
            .find(|message| matches!(message, FilesMessage::ToggleHidden { .. }))
            .expect("toggled")
    };
    assert!(matches!(
        toggled,
        FilesMessage::ToggleHidden {
            side: Side::Local,
            ..
        }
    ));
    let _ = shell.update(Message::App(AppMessage::Files(toggled)));
    let mut ui = simulator(&shell);
    assert!(ui.find(".gitconfig").is_err(), "a dot name, hidden");
    ui.find("notes.md").expect("the rest, kept");
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
    ui.find("admin").expect("remote path, as its folders");
    ui.find("5.0 MiB").expect("size");
    ui.click("logs").expect("folder");
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
    let planning = core.update(AppMessage::Files(FilesMessage::Open {
        tab,
        side: Side::Remote,
        index: 1,
    }));
    let [Effect::PlanTransfer { request, .. }] = planning.as_slice() else {
        panic!("{planning:?}")
    };
    let result = plan_transfer((**request).clone()).await.map(Box::new);
    let effects = core.update(AppMessage::Files(FilesMessage::Planned {
        tab,
        request: request.clone(),
        result,
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
        // Its folders first, as the C# breadcrumb: one goes there.
        let mut ui = simulator(&shell);
        assert!(ui.find("/home/admin").is_err(), "the folders, not the path");
        ui.click("home").expect("a folder of the breadcrumb");
        assert!(
            files_messages(ui).iter().any(|message| matches!(
                message,
                FilesMessage::Ascend { tab: t, side: Side::Remote, levels: 1 } if *t == tab
            )),
            "one folder up"
        );
    }
    // A click beside them gives the path to type in; Escape gives the folders back.
    let _ = shell.update(Message::EditPath {
        tab,
        side: Side::Remote,
    });
    simulator(&shell)
        .find("/home/admin")
        .expect("the path, to type in");
    let _ = shell.update(Message::DialogKey { confirm: false });
    assert!(
        simulator(&shell).find("/home/admin").is_err(),
        "the folders again"
    );
    let _ = shell.update(Message::EditPath {
        tab,
        side: Side::Remote,
    });
    {
        let mut ui = simulator(&shell);
        ui.click("/home/admin").expect("the remote path bar");
        // The click puts the cursor where it lands: typing goes after the path from its end.
        let _ = ui.tap_key(iced::keyboard::key::Named::End);
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
    // What is typed shows until gone to, and Go, a chevron as the C#'s, is offered only then.
    let mut ui = simulator(&shell);
    ui.click(tool_id(tab, Side::Local, PaneTool::Go))
        .expect("Go");
    assert!(files_messages(ui).is_empty(), "nothing typed: greyed out");
    let _ = shell.update(Message::App(AppMessage::Files(FilesMessage::PathEdited {
        tab,
        side: Side::Local,
        text: "elsewhere".to_owned(),
    })));
    let mut ui = simulator(&shell);
    ui.find("elsewhere").expect("typed, shown");
    ui.click(tool_id(tab, Side::Local, PaneTool::Go))
        .expect("Go");
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
                    inode: None,
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
        let mut ui = common::simulator(settings, Size::new(1800.0, 800.0), shell.view());
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
    let mut ui = common::simulator(settings, Size::new(1800.0, 800.0), shell.view());
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
        index: Some(1),
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
        index: Some(0),
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
        index: Some(9),
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
    let mut ui = common::simulator(
        settings,
        WINDOW,
        heimdall_ui::tree_view::files_entry_menu(
            (tab, Side::Local),
            Some(a_file(0)),
            sftp_tab(false, false),
        ),
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
        index: Some(0),
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
        ("Cut", format!("{:?}", FilesMessage::Cut { tab })),
        ("Copy", format!("{:?}", FilesMessage::Copy { tab })),
        ("Paste", format!("{:?}", FilesMessage::Paste { tab })),
        (
            "Duplicate",
            format!("{:?}", FilesMessage::Duplicate { tab }),
        ),
        (
            "Open in terminal",
            format!("{:?}", FilesMessage::OpenInTerminal { tab }),
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
        let mut ui = common::simulator(
            settings,
            WINDOW,
            heimdall_ui::tree_view::files_entry_menu(
                (tab, Side::Remote),
                Some(a_file(1)),
                sftp_tab(true, true),
            ),
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
        common::simulator(
            settings(),
            WINDOW,
            heimdall_ui::tree_view::files_entry_menu(
                (tab, side),
                Some(a_file(0)),
                sftp_tab(false, false),
            ),
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
    assert!(local.find("Cut").is_err(), "the server's only");
    assert!(menu(Side::Remote).find("Paste").is_err(), "nothing cut");
    assert!(
        menu(Side::Remote).find("Duplicate").is_err(),
        "a tab that cannot copy on its server"
    );

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
                    inode: None,
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
        ui.find("2 selected (2.0 KiB)")
            .expect("counted, the file's size beside it, as the C#: not the folder's");
        assert!(
            ui.find("Rename").is_err(),
            "in the entries' menu, as the C#'s, not on the toolbar"
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
        assert!(
            ui.find(tool_id(tab, Side::Local, PaneTool::Bookmarks))
                .is_err(),
            "this computer's pane has none"
        );
        // A star, as the C# toolbar's, opening its menu.
        ui.click(tool_id(tab, Side::Remote, PaneTool::Bookmarks))
            .expect("the server's pane only");
        assert!(ui.into_messages().any(|message| matches!(
            message,
            Message::OpenTreeMenu(TreeMenu::FilesBookmarks(opened)) if opened == tab
        )));
    }
    let _ = shell.update(Message::OpenTreeMenu(TreeMenu::FilesBookmarks(tab)));
    {
        let mut ui = simulator(&shell);
        ui.find("No bookmarks saved").expect("none yet");
        ui.click("Bookmark this path")
            .expect("the menu's first entry, as the C# star's");
        assert!(ui.into_messages().any(|message| matches!(
            message,
            Message::MenuChoice(AppMessage::Files(FilesMessage::Bookmark { tab: chosen }))
                if chosen == tab
        )));
    }
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
    let mut ui = common::simulator(
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
    common::simulator(settings, WINDOW, view)
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
        heimdall_ui::tree_view::files_entry_menu(
            (tab, Side::Remote),
            Some(a_file(0)),
            sftp_tab(false, false),
        ),
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
async fn a_narrow_window_wraps_the_servers_tools_instead_of_cutting_them() {
    // At this width one row of the server's tools and its filter runs past the window's
    // edge, as the hand test once saw a button cut in half.
    let dir = tempfile::tempdir().expect("dir");
    let (core, tab) = files_tab(dir.path()).await;
    let shell = Shell::with_app(core);
    let mut ui = simulator(&shell);
    let back = ui
        .find(tool_id(tab, Side::Remote, PaneTool::Back))
        .expect("shown")
        .bounds();
    let star = ui
        .find(tool_id(tab, Side::Remote, PaneTool::Bookmarks))
        .expect("shown")
        .bounds();
    assert!(
        star.x + star.width <= WINDOW.width,
        "whole in the window: {star:?}"
    );
    assert!(star.y > back.y, "on the next line: {back:?} then {star:?}");
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
    ui.find("logs").expect("kept");
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
    let transfers = |shell: &Shell| {
        shell
            .app()
            .tab(tab)
            .and_then(|found| found.files.as_deref())
            .map(|files| {
                files
                    .transfers
                    .iter()
                    .map(|transfer| transfer.state.clone())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    };
    assert!(transfers(&shell).is_empty(), "gathered until the drop ends");
    assert!(
        simulator(&shell).find("Drop files to upload").is_err(),
        "dropped: no longer said"
    );
    let _ = shell.update(Message::DropGathered(DropPlace::Main));
    assert_eq!(
        transfers(&shell),
        [TransferState::Preparing],
        "planned first: nothing is written before the plan says what is in the way"
    );
    // What is no file at all is refused at once: the drop reached the tab.
    let _ = shell.update(Message::FileDropped(dir.path().join("gone")));
    let _ = shell.update(Message::DropGathered(DropPlace::Main));
    assert_eq!(transfers(&shell).len(), 2, "refused, and said so");
    // Its end said again, the drop is not sent twice.
    let _ = shell.update(Message::DropGathered(DropPlace::Main));
    assert_eq!(transfers(&shell).len(), 2);

    // Not while the Settings page shows.
    let _ = shell.update(Message::ShowSettings);
    let _ = shell.update(Message::FilesHovered(true));
    assert!(simulator(&shell).find("Drop files to upload").is_err());
    let _ = shell.update(Message::FileDropped(dir.path().join("gone again")));
    let _ = shell.update(Message::DropGathered(DropPlace::Main));
    assert_eq!(transfers(&shell).len(), 2, "nothing more");
}

#[tokio::test]
async fn files_dropped_together_from_explorer_go_as_one_upload_as_the_csharp() {
    let dir = tempfile::tempdir().expect("dir");
    let (core, tab) = files_tab(dir.path()).await;
    let mut shell = Shell::with_app(core);
    let names = ["a.txt", "b.txt", "c.txt"];
    let _ = shell.update(Message::FilesHovered(true));
    for name in names {
        let path = dir.path().join(name);
        std::fs::write(&path, b"x").expect("written");
        // winit says each file of the drop in turn.
        let _ = shell.update(Message::FileDropped(path));
    }
    let _ = shell.update(Message::DropGathered(DropPlace::Main));
    let files = shell
        .app()
        .tab(tab)
        .and_then(|found| found.files.as_deref())
        .expect("files");
    // One plan for the three, as the C# `UploadEntriesAsync`: a transfer waiting for
    // another would be queued, not prepared, and asked about in a question of its own.
    assert!(
        files
            .transfers
            .iter()
            .map(|transfer| (transfer.label.as_str(), &transfer.state))
            .eq(names.map(|name| (name, &TransferState::Preparing))),
        "{:?}",
        files.transfers
    );
}

#[tokio::test]
async fn letters_typed_over_a_list_select_the_name_they_start_but_no_fields() {
    use iced::event::Status;

    let dir = tempfile::tempdir().expect("dir");
    let (core, tab) = files_tab(dir.path()).await;
    let mut shell = Shell::with_app(core);
    // Over the lists no widget takes a letter: the window has it, for the type-ahead.
    assert_eq!(simulator(&shell).typewrite("n"), Status::Ignored);
    let selected = |shell: &Shell| {
        let files = shell
            .app()
            .tab(tab)
            .and_then(|found| found.files.as_deref())
            .expect("files");
        (files.local.selected, files.remote.selected)
    };
    // The local list has the keyboard first: "N" is notes.md, whatever its case.
    let _ = shell.update(Message::TypeAhead("N".to_owned()));
    assert_eq!(selected(&shell), (Some(1), None));
    // The server's: "b" is backup.tar.gz, the local list left as it was.
    let _ = shell.update(Message::FilesKey(FilesKey::Focus(Side::Remote)));
    let _ = shell.update(Message::TypeAhead("b".to_owned()));
    assert_eq!(selected(&shell), (Some(1), Some(1)));

    // A field typed in keeps its letters: the filter's, the path bar's.
    {
        let mut ui = simulator(&shell);
        ui.click("Filter files...").expect("a filter");
        assert_eq!(ui.typewrite("l"), Status::Captured, "the filter's");
    }
    let _ = shell.update(Message::EditPath {
        tab,
        side: Side::Remote,
    });
    {
        let mut ui = simulator(&shell);
        ui.click("/home/admin").expect("the path bar");
        assert_eq!(ui.typewrite("l"), Status::Captured, "the path bar's");
    }
    let _ = shell.update(Message::DialogKey { confirm: false });
    // A rename's question has its own field: the list under it is not searched.
    let _ = shell.update(Message::FilesKey(FilesKey::Rename));
    assert!(shell.app().dialog.is_some(), "the rename question");
    let _ = shell.update(Message::TypeAhead("l".to_owned()));
    assert_eq!(selected(&shell), (Some(1), Some(1)), "nothing moved");
}

#[tokio::test]
async fn a_transfer_with_something_in_its_way_asks_about_every_destination_at_once() {
    use heimdall_files::conflict::Choice;
    let dir = tempfile::tempdir().expect("dir");
    let (mut core, tab) = files_tab(dir.path()).await;
    std::fs::write(dir.path().join("backup.tar.gz"), b"mine").expect("in the way");
    let planning = core.update(AppMessage::Files(FilesMessage::Open {
        tab,
        side: Side::Remote,
        index: 1,
    }));
    let [Effect::PlanTransfer { request, .. }] = planning.as_slice() else {
        panic!("{planning:?}")
    };
    let result = plan_transfer((**request).clone()).await.map(Box::new);
    let started = core.update(AppMessage::Files(FilesMessage::Planned {
        tab,
        request: request.clone(),
        result,
    }));
    assert!(started.is_empty(), "nothing before the answer");
    let shell = Shell::with_app(core);
    snapshot(&shell, "files-conflicts.png");
    let mut ui = simulator(&shell);
    ui.find("File conflicts").expect("title");
    ui.find("Choose what Heimdall should do before the transfer starts.")
        .expect("hint");
    ui.find("1 conflicting destination")
        .expect("count, singular");
    ui.find("Apply to all:").expect("apply to all");
    ui.find("backup.tar.gz").expect("the destination");
    ui.find("Destination").expect("column");
    ui.click("Replace").expect("replace them all");
    assert!(matches!(
        files_messages(ui).as_slice(),
        [FilesMessage::ConflictAll(Choice::Replace)]
    ));
}

#[tokio::test]
async fn sudo_s_question_gives_the_password_typed_and_keeps_none_of_it() {
    use heimdall_app::{Dialog, SudoAction};

    let dir = tempfile::tempdir().expect("dir");
    let (mut core, tab) = files_tab(dir.path()).await;
    core.dialog = Some(Dialog::SudoPassword {
        tab,
        name: "sshd_config".to_owned(),
        action: SudoAction::Save(dir.path().join("sshd_config")),
    });
    let mut shell = Shell::with_app(core);
    {
        let mut ui = simulator(&shell);
        ui.find("sudo password").expect("asked");
    }
    let _ = shell.update(Message::SudoPasswordEdited("hunter2".to_owned()));
    let _ = shell.update(Message::SudoPasswordConfirm);
    assert!(shell.app().dialog.is_none(), "answered");
    let kept = shell
        .app()
        .tab(tab)
        .and_then(|t| t.files.as_ref())
        .and_then(|files| files.sudo_password.as_ref())
        .map(|password| password.bytes().to_vec());
    assert_eq!(kept.as_deref(), Some(&b"hunter2"[..]), "kept for the tab");
}

#[tokio::test]
async fn the_delete_as_root_question_names_the_entries_then_how_many_more() {
    use heimdall_app::Dialog;

    let dir = tempfile::tempdir().expect("dir");
    let (mut core, tab) = files_tab(dir.path()).await;
    core.dialog = Some(Dialog::ConfirmSudoDelete {
        tab,
        names: vec!["logs".to_owned(), "backup.tar.gz".to_owned()],
        more: 3,
    });
    let shell = Shell::with_app(core);
    let mut ui = simulator(&shell);
    ui.find("Delete as root?").expect("title");
    for named in ["logs", "backup.tar.gz", "and 3 more"] {
        assert!(ui.find(named).is_ok(), "{named}");
    }
    ui.click("Delete as root").expect("the danger action");
    assert!(
        ui.into_messages()
            .any(|message| matches!(message, Message::App(AppMessage::ConfirmDialog)))
    );
}

#[tokio::test]
async fn a_tab_without_ssh_offers_no_sudo_toggle() {
    let dir = tempfile::tempdir().expect("dir");
    let (core, _) = files_tab(dir.path()).await;
    let shell = Shell::with_app(core);
    let mut ui = simulator(&shell);
    ui.find("Server").expect("the server's pane");
    assert!(ui.find("sudo").is_err(), "no shell to run sudo on");
}

#[tokio::test]
async fn an_empty_pane_says_why_and_offers_the_way_out_as_the_csharp() {
    let dir = tempfile::tempdir().expect("dir");
    let (core, tab) = files_tab(dir.path()).await;
    let mut shell = Shell::with_app(core);
    let _ = shell.update(Message::App(AppMessage::Files(FilesMessage::Filter {
        tab,
        side: Side::Remote,
        text: "zzz".to_owned(),
    })));
    {
        let mut ui = simulator(&shell);
        ui.find("No entries match \"zzz\".").expect("said");
        ui.click("Clear filter").expect("offered");
        assert!(files_messages(ui).iter().any(|message| matches!(
            message,
            FilesMessage::Filter { side: Side::Remote, text, .. } if text.is_empty()
        )));
    }
    let _ = shell.update(Message::App(AppMessage::Files(FilesMessage::Filter {
        tab,
        side: Side::Remote,
        text: String::new(),
    })));
    let _ = shell.update(Message::App(AppMessage::Files(
        FilesMessage::ToggleHidden {
            tab,
            side: Side::Remote,
        },
    )));
    let _ = shell.update(Message::App(AppMessage::Files(
        FilesMessage::RemoteListed {
            tab,
            result: Ok((
                RemotePath::from("/home/admin"),
                vec![remote(".profile", EntryKind::File, 10)],
            )),
        },
    )));
    let mut ui = simulator(&shell);
    ui.find("This folder only contains hidden entries.")
        .expect("said");
    ui.click("Show hidden files").expect("offered");
    assert!(files_messages(ui).iter().any(|message| matches!(
        message,
        FilesMessage::ToggleHidden {
            side: Side::Remote,
            ..
        }
    )));
}

#[tokio::test]
async fn an_entry_dragged_onto_the_other_panes_folder_is_sent_into_it() {
    use heimdall_ui::files_drag::Spot;
    use iced::Point;

    let dir = tempfile::tempdir().expect("dir");
    let (core, tab) = files_tab(dir.path()).await;
    let mut shell = Shell::with_app(core);
    let spot = |side, index| Spot { tab, side, index };
    // Pressed on the server's backup, not chosen yet: it alone is dragged.
    let _ = shell.update(Message::FilesHover(spot(Side::Remote, Some(1))));
    let _ = shell.update(Message::PointerPressed);
    let _ = shell.update(Message::FilesDragMoved(Point::new(30.0, 30.0)));
    // Over this computer's Documents folder, then let go.
    let _ = shell.update(Message::FilesHover(spot(Side::Local, None)));
    let _ = shell.update(Message::FilesHover(spot(Side::Local, Some(0))));
    let _ = shell.update(Message::FilesDragEnd);
    let files = shell
        .app()
        .tab(tab)
        .and_then(|tab| tab.files.as_deref())
        .expect("files");
    let [transfer] = files.transfers.as_slice() else {
        panic!("{:?}", files.transfers.len());
    };
    assert_eq!(transfer.direction, Direction::Download);
    let picked = transfer.picked.as_ref().expect("planned");
    assert_eq!(
        picked.root.local,
        dir.path().join("Documents").join("backup.tar.gz"),
        "into the folder dropped on"
    );

    // A click is no drag: nothing more is sent.
    let _ = shell.update(Message::FilesHover(spot(Side::Remote, Some(1))));
    let _ = shell.update(Message::PointerPressed);
    let _ = shell.update(Message::FilesHover(spot(Side::Local, None)));
    let _ = shell.update(Message::FilesDragEnd);
    let files = shell
        .app()
        .tab(tab)
        .and_then(|tab| tab.files.as_deref())
        .expect("files");
    assert_eq!(files.transfers.len(), 1);
}

/// The facts of an SFTP tab: `can_paste` as said, its SSH connection with `over_ssh`, which
/// it copies and edits on.
fn sftp_tab(can_paste: bool, over_ssh: bool) -> heimdall_ui::tree_view::FilesTabFacts {
    heimdall_ui::tree_view::FilesTabFacts {
        can_paste,
        can_copy: over_ssh,
        connected: over_ssh,
        sftp: true,
        over_ssh,
        local_only: false,
    }
}

/// The facts of a menu opened on a regular file chosen alone, listed at `index`.
fn a_file(index: usize) -> heimdall_ui::tree_view::FilesEntryFacts {
    heimdall_ui::tree_view::FilesEntryFacts {
        index,
        single: true,
        one_file: true,
        link: false,
        runs_in_shell: false,
    }
}

/// The remote pane listing `entries` in `/srv`.
fn list_remote(shell: &mut Shell, tab: TabId, entries: Vec<RemoteEntry>) {
    let _ = shell.update(Message::App(AppMessage::Files(
        FilesMessage::RemoteListed {
            tab,
            result: Ok((RemotePath::from("/srv"), entries)),
        },
    )));
}

#[tokio::test]
async fn a_folder_of_hidden_entries_says_so_before_a_filter_matching_nothing() {
    let dir = tempfile::tempdir().expect("dir");
    let (core, tab) = files_tab(dir.path()).await;
    let mut shell = Shell::with_app(core);
    list_remote(
        &mut shell,
        tab,
        vec![remote(".profile", EntryKind::File, 1)],
    );
    // Shown by default, as the C#'s: hidden here.
    let _ = shell.update(Message::App(AppMessage::Files(
        FilesMessage::ToggleHidden {
            tab,
            side: Side::Remote,
        },
    )));
    let _ = shell.update(Message::App(AppMessage::Files(FilesMessage::Filter {
        tab,
        side: Side::Remote,
        text: "zzz".to_owned(),
    })));
    let mut ui = simulator(&shell);
    ui.find("Show hidden files")
        .expect("only hidden entries, as the C# says first");
    assert!(ui.find("Clear filter").is_err());
}

#[tokio::test]
async fn shift_and_the_arrows_extend_the_selection_and_ctrl_space_takes_one_out() {
    let dir = tempfile::tempdir().expect("dir");
    let (core, tab) = files_tab(dir.path()).await;
    let mut shell = Shell::with_app(core);
    list_remote(
        &mut shell,
        tab,
        vec![
            remote("a", EntryKind::File, 1),
            remote("b", EntryKind::File, 1),
            remote("c", EntryKind::File, 1),
        ],
    );
    let chosen = |shell: &Shell| {
        shell
            .app()
            .tab(tab)
            .and_then(|tab| tab.files.as_deref())
            .map(|files| files.remote.chosen())
            .unwrap_or_default()
    };
    let _ = shell.update(Message::App(AppMessage::Files(FilesMessage::Select {
        tab,
        side: Side::Remote,
        index: 0,
    })));
    let key = |shell: &mut Shell, key| {
        let _ = shell.update(Message::App(AppMessage::Files(FilesMessage::Key {
            tab,
            key,
        })));
    };
    key(&mut shell, FilesKey::ExtendNext);
    key(&mut shell, FilesKey::ExtendNext);
    assert_eq!(chosen(&shell), [0, 1, 2]);
    key(&mut shell, FilesKey::ExtendPrevious);
    assert_eq!(chosen(&shell), [0, 1], "the range shrinks back");
    key(&mut shell, FilesKey::ToggleMark);
    assert_eq!(
        chosen(&shell),
        [0],
        "the entry the keyboard is on taken out"
    );
}

#[tokio::test]
async fn the_menu_beside_the_entries_is_the_folders_and_an_entrys_offers_what_it_takes() {
    let dir = tempfile::tempdir().expect("dir");
    let (_core, tab) = files_tab(dir.path()).await;
    let settings = || Settings {
        fonts: FONTS.iter().map(|face| (*face).into()).collect(),
        ..Settings::default()
    };
    // Beside the entries: the folder's actions, none of an entry's.
    let mut ui = common::simulator(
        settings(),
        WINDOW,
        heimdall_ui::tree_view::files_entry_menu((tab, Side::Remote), None, sftp_tab(true, true)),
    );
    for label in ["New Folder", "Upload here...", "Paste", "Refresh"] {
        ui.find(label).expect(label);
    }
    for label in ["Open", "Rename", "Delete", "Copy path"] {
        assert!(ui.find(label).is_err(), "{label}");
    }
    // A folder: no editing.
    let folder = heimdall_ui::tree_view::FilesEntryFacts {
        index: 0,
        single: true,
        one_file: false,
        link: false,
        runs_in_shell: false,
    };
    let mut ui = common::simulator(
        settings(),
        WINDOW,
        heimdall_ui::tree_view::files_entry_menu(
            (tab, Side::Remote),
            Some(folder),
            sftp_tab(false, true),
        ),
    );
    ui.find("Rename").expect("renamed");
    assert!(ui.find("Edit").is_err(), "a folder is not edited");
    // A link over SFTP: no rename, which would rename its target.
    let link = heimdall_ui::tree_view::FilesEntryFacts {
        link: true,
        ..folder
    };
    let mut ui = common::simulator(
        settings(),
        WINDOW,
        heimdall_ui::tree_view::files_entry_menu(
            (tab, Side::Remote),
            Some(link),
            sftp_tab(false, true),
        ),
    );
    assert!(ui.find("Rename").is_err());
    ui.find("Delete").expect("deleted");
}

#[tokio::test]
async fn an_ftp_tabs_entry_menu_edits_cuts_and_copies_as_the_csharp_but_never_runs_on_the_server() {
    let dir = tempfile::tempdir().expect("dir");
    let (_core, tab) = files_tab(dir.path()).await;
    let ftp = heimdall_ui::tree_view::FilesTabFacts {
        can_paste: false,
        can_copy: true,
        connected: true,
        sftp: false,
        over_ssh: false,
        local_only: false,
    };
    let menu = |facts| {
        let settings = Settings {
            fonts: FONTS.iter().map(|face| (*face).into()).collect(),
            ..Settings::default()
        };
        common::simulator(
            settings,
            WINDOW,
            heimdall_ui::tree_view::files_entry_menu((tab, Side::Remote), Some(facts), ftp),
        )
    };
    let mut ui = menu(a_file(0));
    for label in ["Edit", "Edit with external editor", "Rename", "Cut", "Copy"] {
        ui.find(label).expect(label);
    }
    // Permissions over SFTP only; sudo, Duplicate and a terminal over SSH only.
    for label in [
        "Change permissions...",
        "Edit with sudo",
        "Duplicate",
        "Open in terminal",
    ] {
        assert!(ui.find(label).is_err(), "{label}");
    }
    for (label, expected) in [
        (
            "Edit",
            format!("{:?}", FilesMessage::EditIntegrated { tab }),
        ),
        (
            "Edit with external editor",
            format!("{:?}", FilesMessage::EditExternal { tab }),
        ),
        ("Copy", format!("{:?}", FilesMessage::Copy { tab })),
    ] {
        let mut ui = menu(a_file(0));
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
    // A link over FTP is renamed by its name, as the C#; never edited.
    let link = heimdall_ui::tree_view::FilesEntryFacts {
        one_file: false,
        link: true,
        ..a_file(0)
    };
    let mut ui = menu(link);
    ui.find("Rename").expect("renamed");
    ui.find("Copy").expect("copied");
    assert!(ui.find("Edit").is_err(), "a link is not edited");
}

#[tokio::test]
async fn a_pipe_a_socket_and_a_device_are_marked_in_the_list() {
    use heimdall_app::files::Special;

    let dir = tempfile::tempdir().expect("dir");
    let (core, tab) = files_tab(dir.path()).await;
    let mut shell = Shell::with_app(core);
    list_remote(
        &mut shell,
        tab,
        vec![
            remote("fifo", EntryKind::Other(Special::Pipe), 0),
            remote("sock", EntryKind::Other(Special::Socket), 0),
        ],
    );
    let mut ui = simulator(&shell);
    ui.find("fifo (Named pipe (FIFO))").expect("the pipe");
    ui.find("sock (Socket)").expect("the socket");
}

#[tokio::test]
async fn escape_in_a_filter_empties_it_first_then_leaves_it() {
    use iced::keyboard::key::Named;

    let dir = tempfile::tempdir().expect("dir");
    let (core, tab) = files_tab(dir.path()).await;
    let mut shell = Shell::with_app(core);
    let _ = shell.update(Message::App(AppMessage::Files(FilesMessage::Filter {
        tab,
        side: Side::Remote,
        text: "LOG".to_owned(),
    })));
    {
        let mut ui = simulator(&shell);
        ui.click("LOG").expect("the remote filter, holding a text");
        let _ = ui.tap_key(Named::Escape);
        assert!(
            files_messages(ui).iter().any(|message| matches!(
                message,
                FilesMessage::Filter { side: Side::Remote, text, .. } if text.is_empty()
            )),
            "emptied, as the C# first Escape"
        );
    }
    {
        // Empty, Escape is the field's: it gives the keyboard back, nothing filtered.
        let mut ui = simulator(&shell);
        ui.click("Filter files...")
            .expect("the local filter, empty");
        let _ = ui.tap_key(Named::Escape);
        assert!(
            !files_messages(ui)
                .iter()
                .any(|message| matches!(message, FilesMessage::Filter { .. })),
            "nothing to empty"
        );
    }
}

/// Gap between two columns, a separator in its middle, as the Files tab lays them out.
const COLUMN_GAP: f32 = 8.0;

/// Width of a window wide enough for every column of the server's pane.
const WIDE_WINDOW: Size = Size::new(1800.0, 800.0);

/// Least width of the owner column, as the Files tab keeps it.
const OWNER_LEAST: f32 = 30.0;

/// Width of the owner column until resized.
const OWNER_DEFAULT: f32 = 55.0;

/// Least width of the modification time column.
const MODIFIED_LEAST: f32 = 60.0;

/// How close two edges laid out from the same widths are, in logical pixels.
const EDGE_TOLERANCE: f32 = 0.5;

/// Farthest a name starts after its header: its icon and the room after it.
const NAME_AFTER_ICON: f32 = 24.0;

/// Times a double click is tried: iced tells one by the real time between the presses.
const DOUBLE_CLICK_TRIES: usize = 3;

/// A Files tab whose server pane lists one file with every column filled.
async fn columns_shell(dir: &Path) -> Shell {
    use std::time::{Duration, UNIX_EPOCH};

    let (core, tab) = files_tab(dir).await;
    let mut shell = Shell::with_app(core);
    list_remote(
        &mut shell,
        tab,
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
            inode: None,
        }],
    );
    shell
}

/// The window drawn wide enough for every column.
fn wide(shell: &Shell) -> common::Drawn<'_> {
    let settings = Settings {
        fonts: FONTS.iter().map(|face| (*face).into()).collect(),
        ..Settings::default()
    };
    common::simulator(settings, WIDE_WINDOW, shell.view())
}

/// Where the text `label` is drawn.
fn bounds_of(ui: &mut common::Drawn<'_>, label: &str) -> iced::Rectangle {
    ui.find(label).expect(label).bounds()
}

/// The middle of the separator before the header `label`.
fn separator_before(ui: &mut common::Drawn<'_>, label: &str) -> iced::Point {
    let header = bounds_of(ui, label);
    iced::Point::new(header.x - COLUMN_GAP / 2.0, header.center_y())
}

fn press() -> iced::Event {
    iced::Event::Mouse(iced::mouse::Event::ButtonPressed(iced::mouse::Button::Left))
}

fn release() -> iced::Event {
    iced::Event::Mouse(iced::mouse::Event::ButtonReleased(
        iced::mouse::Button::Left,
    ))
}

/// The pointer moved to `to`, as the window says it.
fn moved(ui: &mut common::Drawn<'_>, to: iced::Point) {
    ui.point_at(to);
    let _ = ui.simulate([iced::Event::Mouse(iced::mouse::Event::CursorMoved {
        position: to,
    })]);
}

/// The column widths `messages` resized the server's pane to, from each one sent.
fn resized(messages: &[Message]) -> Vec<Message> {
    messages
        .iter()
        .filter(|message| {
            matches!(
                message,
                Message::FileColumns {
                    side: Side::Remote,
                    ..
                }
            )
        })
        .cloned()
        .collect()
}

/// Whether `messages` sort a pane.
fn sorts(messages: &[Message]) -> bool {
    messages.iter().any(|message| {
        matches!(
            message,
            Message::App(AppMessage::Files(FilesMessage::SortBy { .. }))
        )
    })
}

/// The width of the server's permissions column, between its header and the owner's.
fn permissions_width(ui: &mut common::Drawn<'_>) -> f32 {
    bounds_of(ui, "Owner").x - COLUMN_GAP - bounds_of(ui, "Permissions").x
}

fn assert_near(actual: f32, expected: f32, what: &str) {
    assert!(
        (actual - expected).abs() <= EDGE_TOLERANCE,
        "{what}: {actual} for {expected}"
    );
}

#[tokio::test]
async fn a_separator_dragged_resizes_its_columns_live_and_the_cells_follow_the_headers() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = columns_shell(dir.path()).await;
    let (permissions, owner, width) = {
        let mut ui = wide(&shell);
        let width = permissions_width(&mut ui);
        (
            bounds_of(&mut ui, "Permissions").x,
            bounds_of(&mut ui, "Owner").x,
            width,
        )
    };
    let messages: Vec<Message> = {
        let mut ui = wide(&shell);
        let from = separator_before(&mut ui, "Owner");
        ui.point_at(from);
        let _ = ui.simulate([press()]);
        moved(&mut ui, iced::Point::new(from.x + 10.0, from.y));
        moved(&mut ui, iced::Point::new(from.x + 20.0, from.y));
        let _ = ui.simulate([release()]);
        ui.into_messages().collect()
    };
    assert!(!sorts(&messages), "a drag on a separator sorts nothing");
    let sent = resized(&messages);
    assert_eq!(sent.len(), 2, "each move sent, as it happens: {messages:?}");
    for message in sent {
        let _ = shell.update(message);
    }
    let mut ui = wide(&shell);
    assert_near(
        bounds_of(&mut ui, "Permissions").x,
        permissions,
        "the column dragged starts where it did",
    );
    assert_near(
        bounds_of(&mut ui, "Owner").x,
        owner + 20.0,
        "the separator stays where let go",
    );
    assert_near(
        permissions_width(&mut ui),
        width + 20.0,
        "the left one widened",
    );
    // Each cell under its header; both panes are sorted by name, either found first.
    for (cell, header) in [("rwsr-xr-x", "Permissions"), ("1000", "Owner")] {
        let (cell_x, header_x) = (bounds_of(&mut ui, cell).x, bounds_of(&mut ui, header).x);
        assert_near(cell_x, header_x, cell);
    }
    let name = bounds_of(&mut ui, "Name \u{25b2}").x;
    let (local, remote) = (
        bounds_of(&mut ui, "Documents").x,
        bounds_of(&mut ui, "run.sh").x,
    );
    // After its icon, as the C#'s: the icon starts under the header.
    let under = |at: f32| (0.0..=NAME_AFTER_ICON).contains(&(at - name));
    assert!(
        under(local) || under(remote),
        "a name under its header: {name}, {local}, {remote}"
    );
    // A click on a header still sorts.
    ui.click("Owner").expect("the header");
    let messages: Vec<Message> = ui.into_messages().collect();
    assert!(
        messages.iter().any(|message| matches!(
            message,
            Message::App(AppMessage::Files(FilesMessage::SortBy {
                side: Side::Remote,
                column: heimdall_app::files::SortColumn::Owner,
                ..
            }))
        )),
        "{messages:?}"
    );
}

#[tokio::test]
async fn a_separator_dragged_keeps_both_columns_at_their_least_width() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = columns_shell(dir.path()).await;
    let (owner, width) = {
        let mut ui = wide(&shell);
        let width = permissions_width(&mut ui);
        (bounds_of(&mut ui, "Owner").x, width)
    };
    let messages: Vec<Message> = {
        let mut ui = wide(&shell);
        let from = separator_before(&mut ui, "Owner");
        ui.point_at(from);
        let _ = ui.simulate([press()]);
        moved(&mut ui, iced::Point::new(from.x + 500.0, from.y));
        let _ = ui.simulate([release()]);
        ui.into_messages().collect()
    };
    for message in resized(&messages) {
        let _ = shell.update(message);
    }
    {
        let mut ui = wide(&shell);
        let gained = OWNER_DEFAULT - OWNER_LEAST;
        assert_near(
            bounds_of(&mut ui, "Owner").x,
            owner + gained,
            "the owner kept at its least",
        );
        assert_near(permissions_width(&mut ui), width + gained, "what it gave");
    }
    // The modification time's separator: it gives the permissions its room, down to its
    // least.
    let messages: Vec<Message> = {
        let mut ui = wide(&shell);
        let from = separator_before(&mut ui, "Permissions");
        ui.point_at(from);
        let _ = ui.simulate([press()]);
        moved(&mut ui, iced::Point::new(from.x - 500.0, from.y));
        let _ = ui.simulate([release()]);
        ui.into_messages().collect()
    };
    for message in resized(&messages) {
        let _ = shell.update(message);
    }
    let mut ui = wide(&shell);
    let modified = bounds_of(&mut ui, "2026-09-27 19:15").x;
    assert_near(
        bounds_of(&mut ui, "Permissions").x,
        modified + MODIFIED_LEAST + COLUMN_GAP,
        "the modification time kept at its least",
    );
}

#[tokio::test]
async fn a_double_click_on_a_separator_fits_the_column_on_its_left_to_its_widest_text() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = columns_shell(dir.path()).await;
    let mut fitted = Vec::new();
    for _ in 0..DOUBLE_CLICK_TRIES {
        let mut ui = wide(&shell);
        let at = separator_before(&mut ui, "Owner");
        ui.point_at(at);
        let _ = ui.simulate([press(), release(), press(), release()]);
        let messages: Vec<Message> = ui.into_messages().collect();
        assert!(
            !sorts(&messages),
            "a double click on a separator sorts nothing"
        );
        fitted = resized(&messages);
        if !fitted.is_empty() {
            break;
        }
    }
    let [fitted] = fitted.as_slice() else {
        panic!("one fit: {fitted:?}");
    };
    let _ = shell.update(fitted.clone());
    let mut ui = wide(&shell);
    let widest = bounds_of(&mut ui, "Permissions")
        .width
        .max(bounds_of(&mut ui, "rwsr-xr-x").width)
        .ceil();
    assert_near(
        permissions_width(&mut ui),
        widest,
        "as wide as its header or its widest cell",
    );
}

/// A Files tab's session reports nothing once its listing is ready: no drop reaches its
/// tab, and its listing never goes behind a failure card. Were one to come, the listing
/// would stay in sight, read only, the failure and its ways out under it.
#[tokio::test]
async fn a_dropped_files_tab_keeps_its_listing_in_sight_read_only() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut core, tab, attempt) = files_tab_attempt(dir.path()).await;
    core.update(AppMessage::Connection {
        tab,
        attempt,
        event: ConnectionEvent::Failed(UiError::ConnectionLost),
    });
    let files = core
        .tab(tab)
        .and_then(|found| found.files.as_deref())
        .expect("its pane");
    assert!(
        files.client.is_none() && files.shell.is_none(),
        "nothing it offers reaches the session gone"
    );
    let shell = Shell::with_app(core);
    snapshot(&shell, "files-dropped.png");
    let mut ui = simulator(&shell);
    for entry in ["logs", "backup.tar.gz", "Documents", "notes.md"] {
        ui.find(entry).expect(entry);
    }
    ui.find("Session disconnected unexpectedly.")
        .expect("the failure said");
    assert!(ui.find("The connection failed").is_err(), "no card over it");
    ui.find("Reconnect").expect("its way out");
}
