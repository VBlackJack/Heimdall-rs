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

//! A plain FTP tab says it is in clear, as the C# browser's badge and status line: beside
//! the server pane's name for as long as the tab is open, and in the status bar once it
//! connects. An explicit FTPS tab says nothing of the kind.

mod common;

use std::path::Path;

use heimdall_app::{
    App, AppConfig, ConnectionEvent, Effect, Message as AppMessage, SystemCredentials, TabId,
};
use heimdall_core::profile::{FtpProfile, ProfileId};
use heimdall_core::store::ProfileStore;
use heimdall_files::RemoteSession;
use heimdall_sftp::protocol::{Request, Response, SFTP_VERSION};
use heimdall_sftp::{ClientConfig, SftpClient};
use heimdall_ssh::AgentSource;
use heimdall_term::GridSize;
use heimdall_ui::shell::Shell;
use heimdall_ui::terminal_view::FONTS;
use iced::{Settings, Size};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

const WINDOW: Size = Size::new(1200.0, 720.0);

/// The badge beside the server pane's name, in the C# words.
const BADGE: &str = "Sent in clear text (no TLS)";

/// A session that answers its start and nothing else: the tab is drawn from its profile.
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

fn profile(id: &str, tls: bool) -> FtpProfile {
    FtpProfile {
        id: ProfileId::new(id),
        name: id.to_owned(),
        group: None,
        host: "files.lab".to_owned(),
        port: 21,
        username: None,
        passive: true,
        tls,
        vault_entry: None,
    }
}

fn app(dir: &Path) -> App {
    let profiles_file = dir.join("profiles.toml");
    let mut store = ProfileStore::open(&profiles_file).expect("store");
    store.merge_ftp(vec![profile("plain", false), profile("secure", true)]);
    store.save().expect("save");
    App::new(AppConfig {
        profiles_file,
        known_hosts: dir.join("known_hosts"),
        legacy_dir: None,
        agent: AgentSource::Disabled,
        initial_grid: GridSize { cols: 80, rows: 24 },
        files_start: dir.to_owned(),
        system_credentials: SystemCredentials::memory(),
    })
}

/// The tab of profile `id`, connected.
async fn connected(core: &mut App, id: &str) -> TabId {
    let effects = core.update(AppMessage::ConnectProfile(ProfileId::new(id)));
    let [Effect::ConnectFtp { tab, attempt, .. }] = effects.as_slice() else {
        panic!("{effects:?}");
    };
    let (tab, attempt) = (*tab, *attempt);
    core.update(AppMessage::Connection {
        tab,
        attempt,
        event: ConnectionEvent::FilesReady {
            client: idle_client().await,
            shell: None,
        },
    });
    tab
}

fn simulator(shell: &Shell) -> common::Drawn<'_> {
    let settings = Settings {
        fonts: FONTS.iter().map(|face| (*face).into()).collect(),
        ..Settings::default()
    };
    common::simulator(settings, WINDOW, shell.view())
}

#[tokio::test]
async fn a_plain_ftp_tab_says_it_is_in_clear_beside_the_server_pane_and_in_the_status_bar() {
    let dir = tempfile::tempdir().expect("dir");
    let mut core = app(dir.path());
    connected(&mut core, "plain").await;
    let shell = Shell::with_app(core);
    let mut ui = simulator(&shell);
    ui.find("Server").expect("the server pane");
    ui.find(BADGE).expect("the C# badge");
    ui.find(
        "FTP session to files.lab:21 is using a cleartext channel. Credentials and file \
         contents are transmitted unencrypted. Prefer SFTP or FTPS when available.",
    )
    .expect("the C# warning, in the status bar");
}

#[tokio::test]
async fn an_ftps_tab_says_nothing_of_the_kind() {
    let dir = tempfile::tempdir().expect("dir");
    let mut core = app(dir.path());
    connected(&mut core, "secure").await;
    let shell = Shell::with_app(core);
    let mut ui = simulator(&shell);
    ui.find("Server").expect("the server pane");
    assert!(ui.find(BADGE).is_err(), "encrypted");
    assert!(
        ui.find(
            "FTP session to files.lab:21 is using a cleartext channel. Credentials and file              contents are transmitted unencrypted. Prefer SFTP or FTPS when available.",
        )
        .is_err()
    );
}
