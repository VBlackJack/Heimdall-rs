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

//! A VNC tab: opened from a profile, its password asked through a question, its desktop
//! input reaching a fake server on this machine in RFB terms.

use std::path::Path;
use std::time::Duration;

use heimdall_app::vnc_driver::{VncRequest, vnc_events};
use heimdall_app::{
    Answer, AnswerRegistry, App, AppConfig, AttemptId, ConnectionEvent, DesktopInput, Effect,
    Message, Phase, PointerButton, Purpose, QuestionKind, TabId,
};
use heimdall_core::profile::{ProfileId, VncProfile};
use heimdall_core::store::ProfileStore;
use heimdall_ssh::{AgentSource, Secret};
use heimdall_term::GridSize;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::{TcpListener, TcpStream};
use tokio_stream::StreamExt as _;

/// Bound on anything the test waits for.
const WAIT: Duration = Duration::from_secs(10);

const CHALLENGE: [u8; 16] = [
    0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff,
];
/// The response for "Secret12", computed with openssl.
const RESPONSE: [u8; 16] = [
    0xee, 0xe9, 0x81, 0xe2, 0x74, 0x19, 0x66, 0x45, 0xd7, 0x10, 0xe9, 0xd9, 0x1f, 0xf5, 0xf5, 0xe8,
];
/// Client messages between the server's init and the session: pixel format, encodings,
/// the first update request.
const OPENING_REQUESTS: usize = 20 + 44 + 10;

fn app(dir: &Path, port: u16, view_only: bool) -> App {
    let profiles_file = dir.join("profiles.toml");
    let mut store = ProfileStore::open(&profiles_file).expect("store");
    store.merge_vnc([VncProfile {
        id: ProfileId::new("kiosk"),
        name: "Kiosk".to_owned(),
        group: None,
        host: "127.0.0.1".to_owned(),
        port,
        view_only,
        allow_no_password: false,
        vault_entry: None,
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

fn open(app: &mut App) -> (TabId, AttemptId, VncRequest) {
    let effects = app.update(Message::OpenVnc(ProfileId::new("kiosk")));
    let Some(Effect::ConnectVnc {
        tab,
        attempt,
        request,
    }) = effects.into_iter().next()
    else {
        panic!("no VNC attempt");
    };
    (tab, attempt, *request)
}

async fn read_exactly(stream: &mut TcpStream, count: usize) -> Vec<u8> {
    let mut bytes = vec![0; count];
    tokio::time::timeout(WAIT, stream.read_exact(&mut bytes))
        .await
        .expect("in time")
        .expect("read");
    bytes
}

/// Bound on waiting to see that nothing comes.
const QUIET: Duration = Duration::from_millis(500);

/// A TigerVNC-like server with VNC Authentication, up to an open 4 by 2 desktop; then the
/// next `expected` bytes the client sends, or, for none expected, whatever comes within
/// [`QUIET`]. It then closes, which ends the session.
async fn serve(listener: TcpListener, expected: usize) -> Vec<u8> {
    let mut stream = opened_desktop(listener).await;
    if expected > 0 {
        return read_exactly(&mut stream, expected).await;
    }
    let mut rest = vec![0; 64];
    match tokio::time::timeout(QUIET, stream.read(&mut rest)).await {
        Ok(Ok(read)) => rest.truncate(read),
        _ => rest.clear(),
    }
    rest
}

/// The server's side up to an open 4 by 2 desktop, the client's first requests read.
async fn opened_desktop(listener: TcpListener) -> TcpStream {
    let (mut stream, _) = listener.accept().await.expect("accepted");
    stream.write_all(b"RFB 003.008\n").await.expect("version");
    let _ = read_exactly(&mut stream, 12).await;
    stream.write_all(&[1, 2]).await.expect("types");
    let _ = read_exactly(&mut stream, 1).await;
    stream.write_all(&CHALLENGE).await.expect("challenge");
    assert_eq!(
        read_exactly(&mut stream, 16).await,
        RESPONSE,
        "the password"
    );
    stream.write_all(&[0, 0, 0, 0]).await.expect("result");
    let _ = read_exactly(&mut stream, 1).await;
    let mut init = vec![0, 4, 0, 2];
    init.extend_from_slice(&[32, 24, 0, 1, 0, 255, 0, 255, 0, 255, 16, 8, 0, 0, 0, 0]);
    // The desktop's name, as the server gives it.
    init.extend_from_slice(&8_u32.to_be_bytes());
    init.extend_from_slice(b"Lab desk");
    stream.write_all(&init).await.expect("init");
    let _ = read_exactly(&mut stream, OPENING_REQUESTS).await;
    stream
}

/// A server that says its screens, one of identifier 7, as `TigerVNC` does; then the size
/// the client asks of it, as `SetDesktopSize` bytes.
async fn serve_screens(listener: TcpListener) -> Vec<u8> {
    let mut stream = opened_desktop(listener).await;
    // A framebuffer update of one rectangle: the extended desktop size, unchanged.
    let mut update = vec![0, 0, 0, 1, 0, 0, 0, 0, 0, 4, 0, 2];
    update.extend_from_slice(&(-308_i32).to_be_bytes());
    update.extend_from_slice(&[1, 0, 0, 0]);
    update.extend_from_slice(&7_u32.to_be_bytes());
    update.extend_from_slice(&[0, 0, 0, 0, 0, 4, 0, 2, 0, 0, 0, 0]);
    stream.write_all(&update).await.expect("update");
    // The next update asked for, then the size: 8 bytes, and 16 for its screen.
    let _ = read_exactly(&mut stream, 10).await;
    read_exactly(&mut stream, 24).await
}

/// Runs the attempt, answers the password question with "Secret12", and once the desktop is
/// open sends `inputs` through the application; the server ends the session.
async fn session(
    app: &mut App,
    tab: TabId,
    attempt: AttemptId,
    request: VncRequest,
    inputs: Vec<DesktopInput>,
) {
    let registry = AnswerRegistry::default();
    let mut events = vnc_events(request, registry.clone());
    while let Some(event) = tokio::time::timeout(WAIT, events.next())
        .await
        .expect("in time")
    {
        let answer = match &event {
            ConnectionEvent::Question {
                question,
                kind: QuestionKind::ServerPassword(asked),
            } => {
                assert_eq!((asked.host.as_str(), asked.port), ("127.0.0.1", asked.port));
                Some(*question)
            }
            _ => None,
        };
        let opened = matches!(event, ConnectionEvent::VncReady { .. });
        let _ = app.update(Message::Connection {
            tab,
            attempt,
            event,
        });
        if let Some(question) = answer {
            assert!(registry.answer(
                question,
                Some(Answer::Secret(Secret::new("Secret12".to_owned())))
            ));
        }
        if opened {
            assert_eq!(app.tab(tab).expect("tab").phase, Phase::Connected);
            assert_eq!(
                app.tab(tab)
                    .and_then(|found| found.desktop.as_ref())
                    .and_then(|desktop| desktop.desktop_name.as_deref()),
                Some("Lab desk"),
                "shown on its bar, as the C# session title"
            );
            let _ = app.update(Message::DesktopInput {
                tab,
                inputs: inputs.clone(),
            });
        }
    }
}

#[tokio::test]
async fn a_vnc_tab_asks_the_password_and_sends_input_in_rfb_terms() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let port = listener.local_addr().expect("address").port();
    let server = tokio::spawn(serve(listener, 6 * 3 + 8));
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path(), port, false);
    let (tab, attempt, request) = open(&mut app);
    assert_eq!(app.tab(tab).expect("tab").purpose, Purpose::Vnc);
    session(
        &mut app,
        tab,
        attempt,
        request,
        vec![
            DesktopInput::Button {
                button: PointerButton::Left,
                pressed: true,
                x: 3,
                y: 1,
            },
            DesktopInput::Wheel {
                vertical: true,
                units: 120,
            },
            DesktopInput::Key {
                scancode: None,
                keysym: Some(0x21),
                pressed: true,
            },
        ],
    )
    .await;
    let sent = server.await.expect("server");
    assert_eq!(
        sent,
        [
            5, 1, 0, 3, 0, 1, // left held at 3,1
            5, 9, 0, 3, 0, 1, // the wheel up: button 4 with the left still held
            5, 1, 0, 3, 0, 1, // and released
            4, 1, 0, 0, 0, 0, 0, 0x21, // '!' pressed
        ]
    );
}

#[tokio::test]
async fn a_view_only_tab_sends_nothing() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let port = listener.local_addr().expect("address").port();
    let server = tokio::spawn(serve(listener, 0));
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path(), port, true);
    let (tab, attempt, request) = open(&mut app);
    session(
        &mut app,
        tab,
        attempt,
        request,
        vec![
            DesktopInput::Move { x: 1, y: 1 },
            DesktopInput::Key {
                scancode: None,
                keysym: Some(0x61),
                pressed: true,
            },
        ],
    )
    .await;
    assert!(server.await.expect("server").is_empty());
}

/// Runs the attempt as [`session`] does; once the desktop is open, `opened` acts through the
/// application. Returns what the application asked of the window for the server's events.
async fn session_with(
    app: &mut App,
    tab: TabId,
    attempt: AttemptId,
    request: VncRequest,
    opened: impl FnOnce(&mut App),
) -> Vec<Effect> {
    let registry = AnswerRegistry::default();
    let mut events = vnc_events(request, registry.clone());
    let mut opened = Some(opened);
    let mut effects = Vec::new();
    while let Some(event) = tokio::time::timeout(WAIT, events.next())
        .await
        .expect("in time")
    {
        let question = match &event {
            ConnectionEvent::Question { question, .. } => Some(*question),
            _ => None,
        };
        let ready = matches!(event, ConnectionEvent::VncReady { .. });
        effects.extend(app.update(Message::Connection {
            tab,
            attempt,
            event,
        }));
        if let Some(question) = question {
            assert!(registry.answer(
                question,
                Some(Answer::Secret(Secret::new("Secret12".to_owned())))
            ));
        }
        if ready && let Some(opened) = opened.take() {
            opened(app);
        }
    }
    effects
}

#[tokio::test]
async fn the_clipboard_goes_to_the_server_on_a_click_only_in_latin_1() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let port = listener.local_addr().expect("address").port();
    // ClientCutText: type, three padding bytes, the length, the Latin-1 text.
    let server = tokio::spawn(serve(listener, 8 + 7));
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path(), port, false);
    let (tab, attempt, request) = open(&mut app);
    session_with(&mut app, tab, attempt, request, |app| {
        // Shown, it sends nothing by itself: VNC carries the text in clear.
        assert!(app.update(Message::SelectTab(tab)).is_empty());
        assert!(matches!(
            app.update(Message::SendClipboard(tab)).as_slice(),
            [Effect::ReadClipboard { tab: asked }] if *asked == tab
        ));
        app.update(Message::ClipboardText {
            tab,
            text: Some("h\u{e9}llo \u{20ac}".to_owned()),
        });
    })
    .await;
    assert_eq!(
        server.await.expect("server"),
        [
            6, 0, 0, 0, 0, 0, 0, 7, b'h', 0xE9, b'l', b'l', b'o', b' ', b'?'
        ],
        "the euro sign is not Latin-1"
    );
}

#[tokio::test]
async fn a_view_only_tab_sends_no_clipboard() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let port = listener.local_addr().expect("address").port();
    let server = tokio::spawn(serve(listener, 0));
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path(), port, true);
    let (tab, attempt, request) = open(&mut app);
    session_with(&mut app, tab, attempt, request, |app| {
        assert!(app.update(Message::SendClipboard(tab)).is_empty());
        app.update(Message::ClipboardText {
            tab,
            text: Some("secret".to_owned()),
        });
    })
    .await;
    assert!(server.await.expect("server").is_empty());
}

/// As [`serve`], then sends the server's clipboard, `text` in Latin-1, and closes.
async fn serve_cut_text(listener: TcpListener, text: &[u8]) {
    let (mut stream, _) = listener.accept().await.expect("accepted");
    stream.write_all(b"RFB 003.008\n").await.expect("version");
    let _ = read_exactly(&mut stream, 12).await;
    stream.write_all(&[1, 2]).await.expect("types");
    let _ = read_exactly(&mut stream, 1).await;
    stream.write_all(&CHALLENGE).await.expect("challenge");
    let _ = read_exactly(&mut stream, 16).await;
    stream.write_all(&[0, 0, 0, 0]).await.expect("result");
    let _ = read_exactly(&mut stream, 1).await;
    let mut init = vec![0, 4, 0, 2];
    init.extend_from_slice(&[32, 24, 0, 1, 0, 255, 0, 255, 0, 255, 16, 8, 0, 0, 0, 0]);
    init.extend_from_slice(&[0, 0, 0, 0]);
    stream.write_all(&init).await.expect("init");
    let _ = read_exactly(&mut stream, OPENING_REQUESTS).await;
    let mut cut = vec![3, 0, 0, 0];
    cut.extend_from_slice(&u32::try_from(text.len()).expect("short").to_be_bytes());
    cut.extend_from_slice(text);
    stream.write_all(&cut).await.expect("cut text");
    tokio::time::sleep(QUIET).await;
}

#[tokio::test]
async fn what_the_server_copies_reaches_this_sides_clipboard() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let port = listener.local_addr().expect("address").port();
    let server = tokio::spawn(serve_cut_text(listener, b"caf\xe9"));
    let dir = tempfile::tempdir().expect("dir");
    // Watched only, it still receives: nothing is sent to the server.
    let mut app = app(dir.path(), port, true);
    let (tab, attempt, request) = open(&mut app);
    let effects = session_with(&mut app, tab, attempt, request, |_| {}).await;
    server.await.expect("server");
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, Effect::WriteClipboard(text) if text == "caf\u{e9}")),
        "{effects:?}"
    );
}

#[tokio::test]
async fn a_vnc_desktop_is_resized_to_its_tab_once_asked_and_the_server_takes_sizes() {
    use heimdall_app::TabMenuMessage;

    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let port = listener.local_addr().expect("address").port();
    let server = tokio::spawn(serve_screens(listener));
    let dir = tempfile::tempdir().expect("dir");
    let mut app = app(dir.path(), port, false);
    let (tab, attempt, request) = open(&mut app);
    let registry = AnswerRegistry::default();
    let mut events = vnc_events(request, registry.clone());
    let resizable = |app: &App| {
        app.tab(tab)
            .and_then(|found| found.desktop.as_ref())
            .and_then(|desktop| desktop.vnc_remote_resize())
    };
    let mut asked = false;
    loop {
        tokio::select! {
            event = events.next() => {
                let Some(event) = event else { break };
                if let ConnectionEvent::Question { question, .. } = &event {
                    let question = *question;
                    let _ = app.update(Message::Connection { tab, attempt, event });
                    assert!(registry.answer(
                        question,
                        Some(Answer::Secret(Secret::new("Secret12".to_owned())))
                    ));
                    continue;
                }
                let _ = app.update(Message::Connection { tab, attempt, event });
            }
            () = tokio::time::sleep(Duration::from_millis(20)), if !asked => {
                // Once the server said its screens: the tab's size known, then asked.
                if resizable(&app) == Some(false) {
                    let _ = app.update(Message::DesktopShown { tab, width: 640, height: 480 });
                    let _ = app.update(Message::TabMenu(TabMenuMessage::VncRemoteResize(tab)));
                    assert_eq!(resizable(&app), Some(true));
                    asked = true;
                }
            }
        }
    }
    let sent = tokio::time::timeout(WAIT, server)
        .await
        .expect("in time")
        .expect("server");
    let mut expected = vec![251, 0, 2, 128, 1, 224, 1, 0];
    expected.extend_from_slice(&7_u32.to_be_bytes());
    expected.extend_from_slice(&[0, 0, 0, 0, 2, 128, 1, 224, 0, 0, 0, 0]);
    assert_eq!(sent, expected, "640 by 480, its one screen");
}
