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

//! The SFTP pane docked beside an SSH shell, drawn headless: the C# "SFTP browser" card of
//! the Settings page, and the pane showing the server's files alone until its toggle shows
//! this computer's again.

mod common;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use heimdall_app::files::{EntryKind, LocalEntry};
use heimdall_app::{
    App, AppConfig, ConnectionEvent, Effect, FilesMessage, InputSink, Message as AppMessage,
    SettingsMessage, TabId,
};
use heimdall_core::profile::{ProfileId, SshProfile};
use heimdall_core::settings::SftpBrowser;
use heimdall_core::store::ProfileStore;
use heimdall_files::RemoteSession;
use heimdall_sftp::protocol::{Request, Response, SFTP_VERSION};
use heimdall_sftp::{ClientConfig, RemotePath, SftpClient};
use heimdall_ssh::{AgentSource, SessionClosed, TerminalSize};
use heimdall_term::GridSize;
use heimdall_ui::shell::{Message, SettingsTab, Shell};
use heimdall_ui::terminal_view::FONTS;
use iced::{Settings, Size};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

/// Size of the simulated window, in logical pixels: wide enough for two panes.
const WINDOW: Size = Size::new(1400.0, 720.0);

/// Size of the simulated Settings page: tall enough for the whole SSH tab.
const SETTINGS_WINDOW: Size = Size::new(1100.0, 2400.0);

#[derive(Debug, Default)]
struct Sink;

impl InputSink for Sink {
    fn write(&self, _bytes: Vec<u8>) -> Result<(), SessionClosed> {
        Ok(())
    }
    fn resize(&self, _size: TerminalSize) -> Result<(), SessionClosed> {
        Ok(())
    }
    fn close(&self) {}
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
        legacy_algorithms: false,
        session_logging: None,
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

/// A shell of profile `a` connected, the SFTP pane docked beside it connected and listed.
async fn docked(dir: &Path) -> (App, TabId) {
    let mut core = app(dir);
    let (shell, attempt) = match core
        .update(AppMessage::OpenProfile(ProfileId::new("a")))
        .as_slice()
    {
        [Effect::Connect { tab, attempt, .. }] => (*tab, *attempt),
        other => panic!("{other:?}"),
    };
    let effects = core.update(AppMessage::Connection {
        tab: shell,
        attempt,
        event: ConnectionEvent::Connected {
            input: Arc::new(Sink),
        },
    });
    let [Effect::Connect { tab, attempt, .. }] = effects.as_slice() else {
        panic!("the pane's connection: {effects:?}");
    };
    let (pane, attempt) = (*tab, *attempt);
    core.update(AppMessage::Connection {
        tab: pane,
        attempt,
        event: ConnectionEvent::FilesReady {
            client: idle_client().await,
            shell: None,
        },
    });
    core.update(AppMessage::Files(FilesMessage::RemoteListed {
        tab: pane,
        result: Ok((RemotePath::from("/home/admin"), Vec::new())),
    }));
    core.update(AppMessage::Files(FilesMessage::LocalListed {
        tab: pane,
        result: Ok((
            PathBuf::from(dir),
            vec![LocalEntry {
                name: "notes.md".into(),
                label: "notes.md".to_owned(),
                kind: EntryKind::File,
                size: Some(2048),
                modified: None,
            }],
        )),
    }));
    assert_eq!(core.active, Some(shell), "the keyboard on the shell");
    (core, pane)
}

fn simulator(shell: &Shell, size: Size) -> common::Drawn<'_> {
    let settings = Settings {
        fonts: FONTS.iter().map(|face| (*face).into()).collect(),
        ..Settings::default()
    };
    common::simulator(settings, size, shell.view())
}

/// The settings each click on the card asks for.
fn clicked(shell: &Shell, label: &str) -> Vec<SftpBrowser> {
    let mut ui = simulator(shell, SETTINGS_WINDOW);
    ui.click(label).expect(label);
    ui.into_messages()
        .filter_map(|message| match message {
            Message::App(AppMessage::Settings(SettingsMessage::SftpBrowser(sftp))) => Some(sftp),
            _ => None,
        })
        .collect()
}

#[test]
fn the_sftp_browser_card_toggles_the_settings_with_the_csharp_wording() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = Shell::with_app(app(dir.path()));
    let _ = shell.update(Message::ShowSettings);
    let _ = shell.update(Message::SettingsTab(SettingsTab::Ssh));
    {
        let mut ui = simulator(&shell, SETTINGS_WINDOW);
        ui.find("SFTP browser").expect("the C# section");
        assert!(
            ui.find("SFTP follows SSH working directory").is_err(),
            "not offered while it is not applied"
        );
    }
    let defaults = SftpBrowser::default();
    let auto_open_off = SftpBrowser {
        auto_open_on_ssh: false,
        ..defaults
    };
    assert_eq!(
        clicked(&shell, "Auto-open SFTP panel on SSH connection"),
        [auto_open_off]
    );
    let browser_off = SftpBrowser {
        enabled: false,
        ..defaults
    };
    let asked = clicked(&shell, "Enable integrated SFTP browser");
    assert_eq!(asked, [browser_off]);
    for sftp in asked {
        let _ = shell.update(Message::App(AppMessage::Settings(
            SettingsMessage::SftpBrowser(sftp),
        )));
    }
    assert_eq!(shell.app().settings().sftp_browser, browser_off);
    assert!(
        clicked(&shell, "Auto-open SFTP panel on SSH connection").is_empty(),
        "under the browser, as the C# checkbox it enables"
    );
}

#[tokio::test]
async fn a_docked_sftp_pane_shows_the_server_files_alone_until_asked() {
    let dir = tempfile::tempdir().expect("dir");
    let (core, pane) = docked(dir.path()).await;
    let mut shell = Shell::with_app(core);
    {
        let mut ui = simulator(&shell, WINDOW);
        ui.find("Server").expect("the server's pane");
        assert!(
            ui.find("This computer").is_err(),
            "half a tab wide: the server's files alone, as the C# pane"
        );
        ui.click("Local files").expect("its toggle");
        assert!(ui.into_messages().any(|message| matches!(
            message,
            Message::App(AppMessage::Files(FilesMessage::ToggleLocal { tab })) if tab == pane
        )));
    }
    let _ = shell.update(Message::App(AppMessage::Files(FilesMessage::ToggleLocal {
        tab: pane,
    })));
    let mut ui = simulator(&shell, WINDOW);
    ui.find("This computer").expect("shown again");
    ui.find("notes.md").expect("its files");
}
