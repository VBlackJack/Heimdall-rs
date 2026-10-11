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
//! toolbar over the desktop, as the C# noVNC page's, asks the server for the quality chosen.

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
use heimdall_ui::shell::{Message, SessionButton, Shell, VncScaling, session_button_id};
use heimdall_ui::terminal_view::FONTS;
use heimdall_ui::tree_view::TreeMenu;
use iced::futures::StreamExt as _;
use iced::{Point, Settings, Size, mouse};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::{TcpListener, TcpStream};

const WINDOW: Size = Size::new(1200.0, 720.0);

/// Bound on anything the test waits for.
const WAIT: Duration = Duration::from_secs(10);

/// The Extended Clipboard pseudo-encoding, 0xC0A1E5CE as noVNC's `encodings.js` has it.
const EXTENDED_CLIPBOARD: i32 = 0xC0A1_E5CE_u32.cast_signed();

/// What the client sends once the desktop opens: pixel format, encodings, update request.
const OPENING_REQUESTS: usize = 20 + 4 + 16 * 4 + 10;

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

/// A window tall enough for a tab's whole menu.
const TALL_WINDOW: Size = Size::new(1200.0, 1400.0);

fn settings() -> Settings {
    Settings {
        fonts: FONTS.iter().map(|face| (*face).into()).collect(),
        ..Settings::default()
    }
}

fn tall(shell: &Shell) -> common::Drawn<'_> {
    common::simulator(settings(), TALL_WINDOW, shell.view())
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

/// What the overlay's list `list` of `tab` sends once opened and its line `line` (from 0)
/// clicked. iced draws a pick list's text without showing it to the simulator: its lines,
/// each as tall as the closed list, open below it.
fn pick(shell: &Shell, tab: TabId, list: SessionButton, line: f32) -> Vec<Message> {
    let mut ui = tall(shell);
    let bounds = ui
        .find(session_button_id(tab, list))
        .expect("the overlay's list")
        .bounds();
    let x = bounds.center_x();
    click_at(&mut ui, Point::new(x, bounds.center_y()));
    click_at(
        &mut ui,
        Point::new(x, bounds.y + bounds.height * (1.5 + line)),
    );
    ui.into_messages().collect()
}

/// What the overlay's "Quality", the C# noVNC toolbar's, sends once its line `line` is
/// clicked.
fn choose(shell: &Shell, tab: TabId, line: f32) -> Option<(TabId, VncQuality)> {
    pick(shell, tab, SessionButton::Quality, line)
        .into_iter()
        .find_map(|message| match message {
            Message::App(AppMessage::VncQuality { tab, quality }) => Some((tab, quality)),
            _ => None,
        })
}

/// The tops of the last texts `labels`, an open menu's entries: each below the one before.
fn assert_in_order(shell: &Shell, labels: &[&str]) {
    let mut ui = tall(shell);
    let tops: Vec<f32> = labels
        .iter()
        .map(|label| common::last_bounds(&mut ui, label).y)
        .collect();
    assert!(
        tops.windows(2).all(|pair| pair[0] < pair[1]),
        "{labels:?} at {tops:?}"
    );
}

/// The profile "Lab desk", a VNC server on this computer's `port` taking no password.
fn desk(port: u16) -> VncProfile {
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
    }
}

/// A shell whose "Lab desk" tab is connected to the server on `port`; the tab, and its
/// session's events, kept for the session to go on.
async fn connected(dir: &Path, port: u16) -> (Shell, TabId, impl Sized) {
    let mut core = app_of(dir, desk(port));
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
    (shell, tab, events)
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
    // SetEncodings of 11, then the whole desktop asked anew.
    let server = tokio::spawn(serve(listener, 4 + 16 * 4 + 10));
    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, tab, _events) = connected(dir.path(), port).await;
    // As the C# toolbar: "Performance" until another is chosen.
    let quality = shell
        .app()
        .tab(tab)
        .and_then(|found| found.desktop.as_deref())
        .and_then(DesktopPane::vnc_quality);
    assert_eq!(quality, Some(VncQuality::Performance));
    // The overlay's "Quality", the C# noVNC toolbar's: its four lines, in the C# order,
    // each sent for this tab.
    let chosen: Vec<_> = [0.0, 1.0, 2.0, 3.0]
        .into_iter()
        .map(|line| choose(&shell, tab, line))
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
    // In noVNC's order: the encodings, JPEG quality 3 (-29) and compression 9 (-247), then
    // the pseudo-encodings.
    let mut expected = vec![2, 0, 0, 16];
    for encoding in [
        1_i32,
        7,
        -260,
        16,
        5,
        2,
        0,
        -29,
        -247,
        -223,
        -224,
        -258,
        -308,
        -307,
        EXTENDED_CLIPBOARD,
        -239,
    ] {
        expected.extend_from_slice(&encoding.to_be_bytes());
    }
    expected.extend_from_slice(&[3, 0, 0, 0, 0, 0, 0, 4, 0, 2]);
    assert_eq!(sent, expected, "the levels, then the whole desktop");
}

/// The entries of a connected VNC tab's menu, as the C# `SessionTabContextMenuFactory` builds
/// it for a saved profile not split: no Resolution, which is RDP's.
const VNC_TAB_MENU: [&str; 15] = [
    "Disconnect",
    "Rename tab",
    "Reset title",
    "Pin tab",
    "Fullscreen (F11)",
    "Reconnect Session",
    "Duplicate Session",
    "Edit",
    "Copy hostname",
    "Copy username",
    "Reveal in tree",
    "Detach to Window",
    "Close others",
    "Close to the right",
    "Split...",
];

#[tokio::test]
async fn a_vnc_tab_has_the_csharp_header_menu_and_toolbar_over_its_desktop() {
    use heimdall_app::TabMenuMessage;

    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let port = listener.local_addr().expect("address").port();
    // Up to an open desktop; nothing more is read.
    let _server = tokio::spawn(serve(listener, 0));
    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, tab, _events) = connected(dir.path(), port).await;
    // A name the tree does not show: its first text is the tab's in the strip.
    let _ = shell.update(Message::App(AppMessage::TabMenu(TabMenuMessage::Rename(
        tab,
    ))));
    let _ = shell.update(Message::App(AppMessage::TabMenu(
        TabMenuMessage::NameEdited("vnc tab".to_owned()),
    )));
    let _ = shell.update(Message::App(AppMessage::ConfirmDialog));
    {
        let mut ui = tall(&shell);
        // The C# VNC header: name, state, Split and Disconnect; none of the RDP header's
        // buttons, and no bar under it.
        assert_eq!(
            common::all_bounds(&mut ui, "vnc tab").len(),
            2,
            "strip and header"
        );
        ui.find("Connected").expect("the state");
        ui.find("Not encrypted: this desktop and what you type cross the network in clear.")
            .expect("the clear connection said");
        for absent in [
            "Send keys",
            "Fit to Window",
            "Fullscreen (F11)",
            "Send clipboard",
        ] {
            assert!(ui.find(absent).is_err(), "{absent} is on screen");
        }
        assert!(
            ui.find(session_button_id(tab, SessionButton::SendKeys))
                .is_err()
        );
        ui.click("Disconnect").expect("the header's button");
        assert!(ui.into_messages().any(|message| matches!(
            message,
            Message::App(AppMessage::DisconnectDesktop(closed)) if closed == tab
        )));
    }
    // The C# noVNC toolbar, over the desktop's top right corner: Clipboard, Scaling mode,
    // Quality, each sending what the bar sent.
    {
        let mut ui = tall(&shell);
        let clipboard = ui.find("Clipboard").expect("the overlay's button").bounds();
        let scaling = ui
            .find(session_button_id(tab, SessionButton::Scaling))
            .expect("the scaling mode")
            .bounds();
        let quality = ui
            .find(session_button_id(tab, SessionButton::Quality))
            .expect("the quality")
            .bounds();
        assert!(clipboard.x < scaling.x && scaling.x < quality.x);
        ui.click("Clipboard").expect("the overlay's button");
        assert!(ui.into_messages().any(|message| matches!(
            message,
            Message::App(AppMessage::SendClipboard(sent)) if sent == tab
        )));
    }
    // A server that cannot resize its desktop offers Scale and No Scaling, as noVNC.
    let scaled: Vec<VncScaling> = [0.0, 1.0]
        .into_iter()
        .filter_map(|line| {
            pick(&shell, tab, SessionButton::Scaling, line)
                .into_iter()
                .find_map(|message| match message {
                    Message::VncScaling { tab: to, scaling } if to == tab => Some(scaling),
                    _ => None,
                })
        })
        .collect();
    let can_resize = shell
        .app()
        .tab(tab)
        .and_then(|found| found.desktop.as_deref())
        .and_then(DesktopPane::vnc_remote_resize)
        .is_some();
    let expected = if can_resize {
        vec![VncScaling::RemoteResize, VncScaling::Scale]
    } else {
        vec![VncScaling::Scale, VncScaling::NoScaling]
    };
    assert_eq!(scaled, expected);
    // The tab menu, as the C# builds it.
    {
        let mut ui = tall(&shell);
        common::right_click(&mut ui, "vnc tab");
        assert!(ui.into_messages().any(|message| matches!(
            message,
            Message::OpenTreeMenu(TreeMenu::Tab(opened)) if opened == tab
        )));
    }
    let _ = shell.update(Message::OpenTreeMenu(TreeMenu::Tab(tab)));
    assert_in_order(&shell, &VNC_TAB_MENU);
    let mut ui = tall(&shell);
    for absent in ["Resolution", "Send keys", "Quality", "Send clipboard"] {
        assert!(ui.find(absent).is_err(), "{absent} in the tab menu");
    }
}
