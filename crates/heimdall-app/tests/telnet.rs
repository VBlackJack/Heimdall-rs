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

//! A Telnet tab: opened from a profile, fed by a session over TCP to a fake server on this
//! machine, and shown in its terminal.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use heimdall_app::telnet_driver::{TelnetRequest, telnet_events};
use heimdall_app::{
    App, AppConfig, AttemptId, ConnectionEvent, Effect, InputSink, Message, Phase, Purpose, TabId,
    TabProfile, UiError,
};
use heimdall_core::profile::{ProfileId, TelnetProfile};
use heimdall_core::store::ProfileStore;
use heimdall_ssh::AgentSource;
use heimdall_term::GridSize;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::TcpListener;
use tokio_stream::StreamExt as _;

const IAC: u8 = 255;
const DO: u8 = 253;
const WILL: u8 = 251;
const SB: u8 = 250;
const SE: u8 = 240;
const NAWS: u8 = 31;

/// Bound on anything the test waits for.
const WAIT: Duration = Duration::from_secs(10);

fn app(dir: &Path, port: u16) -> App {
    let profiles_file = dir.join("profiles.toml");
    let mut store = ProfileStore::open(&profiles_file).expect("store");
    store.merge_telnet([TelnetProfile {
        id: ProfileId::new("sw"),
        name: "Core switch".to_owned(),
        group: Some("Network".to_owned()),
        host: "127.0.0.1".to_owned(),
        port,
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

/// A Telnet tab, its attempt and the request its opening made.
fn open(app: &mut App) -> (TabId, AttemptId, TelnetRequest) {
    let effects = app.update(Message::OpenTelnet(ProfileId::new("sw")));
    let Some(Effect::ConnectTelnet {
        tab,
        attempt,
        request,
    }) = effects.into_iter().next()
    else {
        panic!("no Telnet attempt");
    };
    (tab, attempt, *request)
}

#[test]
fn opening_a_telnet_profile_starts_a_terminal_tab() {
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path(), 2323);
    assert_eq!(app.telnet_profiles().len(), 1);
    let (tab, _, request) = open(&mut app);
    assert_eq!(request.profile.host, "127.0.0.1");
    assert_eq!(request.profile.port, 2323);
    assert_eq!((request.size.cols, request.size.rows), (80, 24));
    let tab = app.tab(tab).expect("tab");
    assert!(matches!(tab.profile, TabProfile::Telnet(_)));
    assert_eq!(tab.purpose, Purpose::Shell);
    assert_eq!(tab.phase, Phase::Connecting);
    assert_eq!(tab.profile.username(), None, "Telnet asks in the session");
}

/// Feeds every event of the attempt to the application. `reply` sees each piece of output
/// with the session's input, as someone watching the terminal would.
async fn follow(
    app: &mut App,
    tab: TabId,
    attempt: AttemptId,
    request: TelnetRequest,
    mut reply: impl FnMut(&[u8], &dyn InputSink),
) {
    let mut events = telnet_events(request);
    let mut input: Option<Arc<dyn InputSink>> = None;
    while let Some(event) = tokio::time::timeout(WAIT, events.next())
        .await
        .expect("in time")
    {
        match &event {
            ConnectionEvent::Connected { input: sink } => input = Some(Arc::clone(sink)),
            ConnectionEvent::Output(bytes) => {
                if let Some(sink) = &input {
                    reply(bytes, sink.as_ref());
                }
            }
            _ => {}
        }
        let _ = app.update(Message::Connection {
            tab,
            attempt,
            event,
        });
    }
}

#[tokio::test]
async fn a_telnet_session_shows_in_its_tab_and_takes_its_input() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let port = listener.local_addr().expect("address").port();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("accepted");
        stream.write_all(&[IAC, DO, NAWS]).await.expect("offer");
        let mut answer = [0; 3 + 9];
        stream.read_exact(&mut answer).await.expect("answer");
        stream.write_all(b"login: ").await.expect("prompt");
        let mut typed = [0; 6];
        stream.read_exact(&mut typed).await.expect("typed");
        stream.write_all(b"welcome").await.expect("welcome");
        (answer, typed)
    });

    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path(), port);
    let (tab, attempt, request) = open(&mut app);
    follow(&mut app, tab, attempt, request, |output, input| {
        if output.ends_with(b"login: ") {
            input.write(b"root\r".to_vec()).expect("typed");
        }
    })
    .await;
    let (answer, typed) = server.await.expect("server");

    // The size the tab was opened with.
    assert_eq!(
        answer,
        [IAC, WILL, NAWS, IAC, SB, NAWS, 0, 80, 0, 24, IAC, SE]
    );
    assert_eq!(&typed, b"root\r\0");
    let tab = app.tab(tab).expect("tab");
    assert_eq!(tab.phase, Phase::Closed { exit_status: None });
    assert_eq!(
        tab.terminal.snapshot().row_text(0).trim_end(),
        "login: welcome"
    );
}

#[tokio::test]
async fn a_closed_port_fails_the_tab_with_a_network_error() {
    // Bound but not listening: the port stays this test's, and refuses. A listener
    // dropped instead frees it, for a test running beside this one to take.
    let socket = tokio::net::TcpSocket::new_v4().expect("socket");
    socket
        .bind("127.0.0.1:0".parse().expect("address"))
        .expect("bind");
    let port = socket.local_addr().expect("address").port();
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path(), port);
    let (tab, attempt, request) = open(&mut app);
    follow(&mut app, tab, attempt, request, |_, _| panic!("output")).await;
    assert!(
        matches!(
            app.tab(tab).expect("tab").phase,
            Phase::Failed(UiError::Network { .. })
        ),
        "{:?}",
        app.tab(tab).expect("tab").phase
    );
}
