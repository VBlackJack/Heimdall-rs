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

//! VNC in the window, drawn headless: the profile in the sidebar opens a VNC tab, whose
//! toolbar asks the server for the quality chosen.

mod common;

use std::path::Path;
use std::time::Duration;

use heimdall_app::vnc_driver::vnc_events;
use heimdall_app::{
    AnswerRegistry, App, AppConfig, ConnectionEvent, DesktopPane, Effect, Message as AppMessage,
    TabId, VncQuality,
};
use heimdall_core::profile::{ProfileId, VncProfile};
use heimdall_core::store::ProfileStore;
use heimdall_ssh::AgentSource;
use heimdall_term::GridSize;
use heimdall_ui::shell::{Message, Shell};
use heimdall_ui::terminal_view::FONTS;
use iced::futures::StreamExt as _;
use iced::{Point, Settings, Size, mouse};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::{TcpListener, TcpStream};

const WINDOW: Size = Size::new(1200.0, 720.0);

/// Bound on anything the test waits for.
const WAIT: Duration = Duration::from_secs(10);

/// What the client sends once the desktop opens: pixel format, encodings, update request.
const OPENING_REQUESTS: usize = 20 + 44 + 10;

fn app(dir: &Path) -> App {
    app_of(
        dir,
        VncProfile {
            id: ProfileId::new("kiosk"),
            name: "Lobby kiosk".to_owned(),
            group: Some("Floor".to_owned()),
            host: "kiosk.lab".to_owned(),
            port: 5901,
            view_only: true,
            allow_no_password: false,
            require_tls: false,
            username: None,
            vault_entry: None,
        },
    )
}

fn app_of(dir: &Path, profile: VncProfile) -> App {
    let profiles_file = dir.join("profiles.toml");
    let mut store = ProfileStore::open(&profiles_file).expect("store");
    store.merge_vnc([profile]);
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

#[test]
fn a_vnc_profile_is_listed_and_opens_a_vnc_tab() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = Shell::with_app(app(dir.path()));
    let settings = Settings {
        fonts: FONTS.iter().map(|face| (*face).into()).collect(),
        ..Settings::default()
    };
    // As in the C# tree: a click selects, a double click connects.
    let messages = common::double_click_messages(
        || common::simulator(settings.clone(), WINDOW, shell.view()),
        "Lobby kiosk",
    );
    assert!(messages.iter().any(|message| matches!(
        message,
        Message::TreeClick(id) if id.as_str() == "kiosk"
    )));
    assert!(
        messages.iter().any(|message| matches!(
            message,
            Message::App(AppMessage::ConnectProfile(id)) if id.as_str() == "kiosk"
        )),
        "a double click connects"
    );
    // The tree shows the protocol as its icon; the details of the session selected name it.
    let _ = shell.update(Message::TreeClick(ProfileId::new("kiosk")));
    common::simulator(settings, WINDOW, shell.view())
        .find("VNC")
        .expect("protocol");
}

/// Vertical padding of a pick list, iced's button padding: a line of its menu is as tall as
/// the closed list, its text's line and this.
const PICK_LIST_PADDING_Y: f32 = 10.0;

/// From the end of the "Send clipboard" text to inside the menu after it: the button's own
/// padding, the bar's spacing, and some way into the menu.
const INTO_THE_MENU: f32 = 40.0;

fn settings() -> Settings {
    Settings {
        fonts: FONTS.iter().map(|face| (*face).into()).collect(),
        ..Settings::default()
    }
}

/// Clicks at `point`, as a user does.
fn click_at(ui: &mut common::Drawn<'_>, point: Point) {
    ui.point_at(point);
    let _ = ui.simulate([
        iced::Event::Mouse(mouse::Event::CursorMoved { position: point }),
        iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
    ]);
}

/// What the quality menu sends once opened and its line `line` (from 0) clicked. iced draws
/// a pick list's text without showing it to the simulator: the menu is found after the
/// "Send clipboard" button, as in the C# toolbar, and its lines open below it.
fn choose(shell: &Shell, line: f32) -> Option<(TabId, VncQuality)> {
    let mut ui = common::simulator(settings(), WINDOW, shell.view());
    let clipboard = ui
        .find("Send clipboard")
        .expect("the button before the menu")
        .bounds();
    let height = clipboard.height + PICK_LIST_PADDING_Y;
    let menu = Point::new(
        clipboard.x + clipboard.width + INTO_THE_MENU,
        clipboard.center_y(),
    );
    click_at(&mut ui, menu);
    let top = clipboard.center_y() - height / 2.0;
    click_at(&mut ui, Point::new(menu.x, top + height * (1.5 + line)));
    ui.into_messages().find_map(|message| match message {
        Message::App(AppMessage::VncQuality { tab, quality }) => Some((tab, quality)),
        _ => None,
    })
}

async fn read_exactly(stream: &mut TcpStream, count: usize) -> Vec<u8> {
    let mut bytes = vec![0; count];
    tokio::time::timeout(WAIT, stream.read_exact(&mut bytes))
        .await
        .expect("in time")
        .expect("read");
    bytes
}

/// A server without a password up to an open 4 by 2 desktop; then the next `expected` bytes
/// the client sends.
async fn serve(listener: TcpListener, expected: usize) -> Vec<u8> {
    let (mut stream, _) = listener.accept().await.expect("accepted");
    stream.write_all(b"RFB 003.008\n").await.expect("version");
    let _ = read_exactly(&mut stream, 12).await;
    stream.write_all(&[1, 1]).await.expect("types");
    let _ = read_exactly(&mut stream, 1).await;
    stream.write_all(&[0, 0, 0, 0]).await.expect("result");
    let _ = read_exactly(&mut stream, 1).await;
    let mut init = vec![0, 4, 0, 2];
    init.extend_from_slice(&[32, 24, 0, 1, 0, 255, 0, 255, 0, 255, 16, 8, 0, 0, 0, 0]);
    init.extend_from_slice(&0_u32.to_be_bytes());
    stream.write_all(&init).await.expect("init");
    let _ = read_exactly(&mut stream, OPENING_REQUESTS).await;
    read_exactly(&mut stream, expected).await
}

#[tokio::test]
async fn the_quality_menu_offers_the_csharp_four_from_performance_and_asks_the_server() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let port = listener.local_addr().expect("address").port();
    // SetEncodings of 10, then the whole desktop asked anew.
    let server = tokio::spawn(serve(listener, 4 + 40 + 10));
    let dir = tempfile::tempdir().expect("dir");
    let mut core = app_of(
        dir.path(),
        VncProfile {
            id: ProfileId::new("desk"),
            name: "Lab desk".to_owned(),
            group: None,
            host: "127.0.0.1".to_owned(),
            port,
            view_only: false,
            allow_no_password: true,
            require_tls: false,
            username: None,
            vault_entry: None,
        },
    );
    let Some(Effect::ConnectVnc {
        tab,
        attempt,
        request,
    }) = core
        .update(AppMessage::OpenVnc(ProfileId::new("desk")))
        .into_iter()
        .next()
    else {
        panic!("no VNC attempt");
    };
    let mut shell = Shell::with_app(core);
    let mut events = vnc_events(*request, AnswerRegistry::default());
    loop {
        let event = tokio::time::timeout(WAIT, events.next())
            .await
            .expect("in time")
            .expect("an event");
        let ready = matches!(event, ConnectionEvent::VncReady { .. });
        let _ = shell.update(Message::App(AppMessage::Connection {
            tab,
            attempt,
            event,
        }));
        if ready {
            break;
        }
    }
    // As the C# toolbar: "Performance" until another is chosen.
    let quality = shell
        .app()
        .tab(tab)
        .and_then(|found| found.desktop.as_deref())
        .and_then(DesktopPane::vnc_quality);
    assert_eq!(quality, Some(VncQuality::Performance));
    // The menu's four lines, in the C# order, each sent for this tab.
    let chosen: Vec<_> = [0.0, 1.0, 2.0, 3.0]
        .into_iter()
        .map(|line| choose(&shell, line))
        .collect();
    assert_eq!(
        chosen,
        VncQuality::ALL.map(|quality| Some((tab, quality))),
        "the menu's lines"
    );
    let _ = shell.update(Message::App(AppMessage::VncQuality {
        tab,
        quality: VncQuality::LowBandwidth,
    }));
    let sent = tokio::time::timeout(WAIT, server)
        .await
        .expect("in time")
        .expect("server");
    // Tight first, the others, then compression 9 (-247) and JPEG quality 3 (-29).
    let mut expected = vec![2, 0, 0, 10];
    for encoding in [7_i32, 16, 1, 0, -223, -224, -308, -307, -247, -29] {
        expected.extend_from_slice(&encoding.to_be_bytes());
    }
    expected.extend_from_slice(&[3, 0, 0, 0, 0, 0, 0, 4, 0, 2]);
    assert_eq!(sent, expected, "the levels, then the whole desktop");
}
