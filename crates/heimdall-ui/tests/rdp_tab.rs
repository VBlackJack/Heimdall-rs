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

//! RDP in the window, drawn headless: the profile in the sidebar, the way out of a changed
//! key, and the desktop taking the mouse.

mod common;

use std::path::Path;

use heimdall_app::{
    App, AppConfig, AttemptId, ConnectionEvent, DesktopInput, Effect, Message as AppMessage,
    PointerButton, TabId, UiError,
};
use heimdall_core::profile::{ProfileId, RdpProfile};
use heimdall_core::store::ProfileStore;
use heimdall_rdp::{Ending, Framebuffer};
use heimdall_ssh::AgentSource;
use heimdall_term::GridSize;
use heimdall_ui::shell::{Message, Shell};
use heimdall_ui::terminal_view::FONTS;
use iced::{Point, Settings, Size, Theme, mouse};

const WINDOW: Size = Size::new(1200.0, 720.0);

/// Physical pixels per logical pixel in a simulator snapshot.
const SNAPSHOT_SCALE: u32 = 2;

fn app(dir: &Path) -> App {
    app_with(dir, heimdall_core::profile::RdpOptions::default())
}

/// The application with one RDP profile, given `options`.
fn app_with(dir: &Path, options: heimdall_core::profile::RdpOptions) -> App {
    app_of(dir, profile(options))
}

/// The RDP profile "dc", given `options`.
fn profile(options: heimdall_core::profile::RdpOptions) -> RdpProfile {
    RdpProfile {
        extras: heimdall_core::profile::RdpExtras::default(),
        id: ProfileId::new("dc"),
        name: "Domain controller".to_owned(),
        group: Some("Windows".to_owned()),
        host: "dc.lab".to_owned(),
        port: 3389,
        username: Some("admin".to_owned()),
        domain: None,
        allow_tls_only: false,
        gateway: None,
        local_tunnel_port: None,
        redirect_clipboard: true,
        redirect_drives: false,
        options,
        vault_entry: None,
        forwards: heimdall_core::profile::Forwards::default(),
        follow_defaults: false,
        several_servers: false,
        anti_idle: false,
        auto_reconnect: true,
    }
}

/// The application with `profile`, its only one.
fn app_of(dir: &Path, profile: RdpProfile) -> App {
    let profiles_file = dir.join("profiles.toml");
    let mut store = ProfileStore::open(&profiles_file).expect("store");
    store.merge_rdp([profile]);
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

fn simulator(shell: &Shell) -> common::Drawn<'_> {
    common::simulator(settings(), WINDOW, shell.view())
}

fn settings() -> Settings {
    Settings {
        fonts: FONTS.iter().map(|face| (*face).into()).collect(),
        ..Settings::default()
    }
}

/// A shell with the RDP tab open; the tab and its attempt.
fn opened(dir: &Path) -> (Shell, TabId, AttemptId) {
    opened_with(dir, heimdall_core::profile::RdpOptions::default())
}

/// A shell with the RDP tab of a profile given `options` open; the tab and its attempt.
fn opened_with(
    dir: &Path,
    options: heimdall_core::profile::RdpOptions,
) -> (Shell, TabId, AttemptId) {
    let mut core = app_with(dir, options);
    let (tab, attempt) = match core
        .update(AppMessage::OpenRdp(ProfileId::new("dc")))
        .as_slice()
    {
        [Effect::ConnectRdp { tab, attempt, .. }] => (*tab, *attempt),
        other => panic!("{other:?}"),
    };
    (Shell::with_app(core), tab, attempt)
}

fn connection(shell: &mut Shell, tab: TabId, attempt: AttemptId, event: ConnectionEvent) {
    let _ = shell.update(Message::App(AppMessage::Connection {
        tab,
        attempt,
        event,
    }));
}

#[test]
fn an_rdp_profile_is_listed_and_opens_an_rdp_tab() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = Shell::with_app(app(dir.path()));
    // As in the C# tree: a click selects, a double click connects.
    let messages = {
        let _turn = common::render_turn();
        common::double_click_messages(|| simulator(&shell), "Domain controller")
    };
    assert!(messages.iter().any(|message| matches!(
        message,
        Message::TreeClick(id) if id.as_str() == "dc"
    )));
    assert!(
        messages.iter().any(|message| matches!(
            message,
            Message::App(AppMessage::ConnectProfile(id)) if id.as_str() == "dc"
        )),
        "a double click connects"
    );
    // The tree shows the protocol as its icon; the details of the session selected name it.
    let _ = shell.update(Message::TreeClick(ProfileId::new("dc")));
    simulator(&shell).find("RDP").expect("protocol");
}

#[test]
fn a_changed_key_offers_to_forget_the_server() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, tab, attempt) = opened(dir.path());
    connection(
        &mut shell,
        tab,
        attempt,
        ConnectionEvent::Failed(UiError::HostKeyChanged {
            target: None,
            recorded: "SHA256:old".to_owned(),
            offered: "SHA256:new".to_owned(),
        }),
    );
    let mut ui = simulator(&shell);
    ui.click("Forget this server").expect("button");
    assert!(ui.into_messages().any(|message| matches!(
        message,
        Message::App(AppMessage::ForgetServer(forgotten)) if forgotten == tab
    )));
}

#[test]
fn another_failure_offers_no_way_to_forget() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, tab, attempt) = opened(dir.path());
    connection(
        &mut shell,
        tab,
        attempt,
        ConnectionEvent::Failed(UiError::Timeout),
    );
    let mut ui = simulator(&shell);
    assert!(ui.find("Forget this server").is_err());
}

#[test]
fn a_click_on_the_desktop_moves_and_presses_there() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, tab, attempt) = opened(dir.path());
    let (input, _received) = tokio::sync::mpsc::unbounded_channel();
    connection(
        &mut shell,
        tab,
        attempt,
        ConnectionEvent::RdpReady {
            framebuffer: Framebuffer::new(1280, 800),
            input,
            size: tokio::sync::watch::channel(None).0,
            clipboard: None,
        },
    );
    let mut ui = simulator(&shell);
    // Far enough right and down to be on the desktop, whatever the sidebar and tab bar take.
    ui.point_at(Point::new(700.0, 400.0));
    let _ = ui.simulate([
        iced::Event::Mouse(mouse::Event::CursorMoved {
            position: Point::new(700.0, 400.0),
        }),
        iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
    ]);
    let inputs: Vec<DesktopInput> = ui
        .into_messages()
        .filter_map(|message| match message {
            Message::App(AppMessage::DesktopInput { inputs, .. }) => Some(inputs),
            _ => None,
        })
        .flatten()
        .collect();
    let pressed = inputs
        .iter()
        .find_map(|input| match *input {
            DesktopInput::Button {
                button: PointerButton::Left,
                pressed: true,
                x,
                y,
            } => Some((x, y)),
            _ => None,
        })
        .expect("a press");
    // Desktop coordinates, not window ones: the view starts right of the sidebar.
    assert!(pressed.0 < 700 && pressed.1 < 400, "{pressed:?}");
}

/// Height of a point near the desktop's top left corner, below the tab strip as tall as the
/// C#'s and the session bar, on one line or two.
const DESKTOP_CORNER_Y: u32 = 175;

/// The RGBA pixel at logical `(x, y)` of a snapshot of `shell`, and the renderer that drew it.
fn pixel_at(shell: &Shell, x: u32, y: u32) -> ([u8; 4], String) {
    let dir = tempfile::tempdir().expect("dir");
    simulator(shell)
        .snapshot(&Theme::Dark)
        .expect("drawn")
        .matches_image(dir.path().join("frame.png"))
        .expect("written");
    // iced names the file after its renderer: `frame-wgpu.png`, `frame-tiny-skia.png`.
    let entry = std::fs::read_dir(dir.path())
        .expect("listed")
        .flatten()
        .next()
        .expect("one snapshot");
    let name = entry.file_name().to_string_lossy().into_owned();
    let renderer = name
        .trim_start_matches("frame-")
        .trim_end_matches(".png")
        .to_owned();
    let decoder = png::Decoder::new(std::io::BufReader::new(
        std::fs::File::open(entry.path()).expect("opened"),
    ));
    let mut reader = decoder.read_info().expect("header");
    let mut bytes = vec![0; reader.output_buffer_size().expect("size")];
    let info = reader.next_frame(&mut bytes).expect("frame");
    let at = usize::try_from((y * SNAPSHOT_SCALE) * info.width * 4 + (x * SNAPSHOT_SCALE) * 4)
        .expect("index");
    let pixel = [bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]];
    (pixel, renderer)
}

#[test]
fn a_connected_desktop_is_drawn_from_its_first_frame() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, tab, attempt) = opened(dir.path());
    let (input, _received) = tokio::sync::mpsc::unbounded_channel();
    connection(
        &mut shell,
        tab,
        attempt,
        ConnectionEvent::RdpReady {
            framebuffer: Framebuffer::new(1280, 800),
            input,
            size: tokio::sync::watch::channel(None).0,
            clipboard: None,
        },
    );
    let (pixel, renderer) = pixel_at(&shell, 700, 400);
    // The GPU renderer keeps the desktop in a texture, drawn in the frame that shows it.
    // As an image, it would be uploaded in the background and missing from that frame,
    // which is what made the desktop flicker. The software renderer has no such delay and
    // draws the image instead; the check is about the GPU path.
    if renderer != "wgpu" {
        eprintln!("drawn by {renderer}, not the GPU renderer; skipped");
        return;
    }
    // A new desktop is black, and opaque whatever alpha the decoder left.
    assert_eq!(pixel, [0, 0, 0, 255], "drawn by {renderer}");
}

/// A key event as a keyboard sends it: `typed` is what the key types with the modifiers held.
fn key_event(typed: &str, code: iced::keyboard::key::Code, pressed: bool) -> iced::Event {
    use iced::keyboard::{Event, Key, Location, Modifiers};
    let physical_key = iced::keyboard::key::Physical::Code(code);
    let key = Key::Character(typed.into());
    iced::Event::Keyboard(if pressed {
        Event::KeyPressed {
            key: key.clone(),
            modified_key: key,
            physical_key,
            location: Location::Standard,
            modifiers: Modifiers::default(),
            text: Some(typed.into()),
            repeat: false,
        }
    } else {
        Event::KeyReleased {
            key: key.clone(),
            modified_key: key,
            physical_key,
            location: Location::Standard,
            modifiers: Modifiers::default(),
        }
    })
}

#[test]
fn ctrl_k_on_a_desktop_is_left_to_the_window_and_never_reaches_the_server() {
    use iced::event::Status;
    use iced::keyboard::key::{Code, Named, Physical};
    use iced::keyboard::{Event, Key, Location, Modifiers};

    let dir = tempfile::tempdir().expect("dir");
    let (shell, _, _received) = connected(dir.path());
    let ctrl = |pressed: bool, modifiers: Modifiers| {
        let key = Key::Named(Named::Control);
        let physical_key = Physical::Code(Code::ControlLeft);
        iced::Event::Keyboard(if pressed {
            Event::KeyPressed {
                key: key.clone(),
                modified_key: key,
                physical_key,
                location: Location::Left,
                modifiers,
                text: None,
                repeat: false,
            }
        } else {
            Event::KeyReleased {
                key: key.clone(),
                modified_key: key,
                physical_key,
                location: Location::Left,
                modifiers,
            }
        })
    };
    let k = |pressed: bool, modifiers: Modifiers| {
        let key = Key::Character("k".into());
        let physical_key = Physical::Code(Code::KeyK);
        iced::Event::Keyboard(if pressed {
            Event::KeyPressed {
                key: key.clone(),
                modified_key: key,
                physical_key,
                location: Location::Standard,
                modifiers,
                text: None,
                repeat: false,
            }
        } else {
            Event::KeyReleased {
                key: key.clone(),
                modified_key: key,
                physical_key,
                location: Location::Standard,
                modifiers,
            }
        })
    };
    for modifiers in [Modifiers::CTRL, Modifiers::CTRL | Modifiers::SHIFT] {
        let mut ui = simulator(&shell);
        let statuses = ui.simulate([
            ctrl(true, modifiers),
            k(true, modifiers),
            k(false, modifiers),
            ctrl(false, Modifiers::empty()),
        ]);
        assert_eq!(statuses[1], Status::Ignored, "the window's: {modifiers:?}");
        let sent: Vec<bool> = ui
            .into_messages()
            .filter_map(|message| match message {
                Message::App(AppMessage::DesktopInput { inputs, .. }) => Some(inputs),
                _ => None,
            })
            .flatten()
            .filter_map(|input| match input {
                DesktopInput::Key { pressed, .. } => Some(pressed),
                _ => None,
            })
            .collect();
        assert_eq!(
            sent,
            [true, false],
            "Ctrl alone, down and up: {modifiers:?}"
        );
    }
}

#[test]
fn ctrl_shift_a_on_a_desktop_is_left_to_the_window_and_never_reaches_the_server() {
    use iced::event::Status;
    use iced::keyboard::key::{Code, Physical};
    use iced::keyboard::{Event, Key, Location, Modifiers};

    let dir = tempfile::tempdir().expect("dir");
    let (shell, _, _received) = connected(dir.path());
    let held = Modifiers::CTRL | Modifiers::SHIFT;
    let a = |pressed: bool| {
        let key = Key::Character("A".into());
        let physical_key = Physical::Code(Code::KeyA);
        iced::Event::Keyboard(if pressed {
            Event::KeyPressed {
                key: key.clone(),
                modified_key: key,
                physical_key,
                location: Location::Standard,
                modifiers: held,
                text: None,
                repeat: false,
            }
        } else {
            Event::KeyReleased {
                key: key.clone(),
                modified_key: key,
                physical_key,
                location: Location::Standard,
                modifiers: held,
            }
        })
    };
    let mut ui = simulator(&shell);
    let statuses = ui.simulate([a(true), a(false)]);
    assert_eq!(statuses[0], Status::Ignored, "the window's");
    let sent = ui
        .into_messages()
        .filter_map(|message| match message {
            Message::App(AppMessage::DesktopInput { inputs, .. }) => Some(inputs),
            _ => None,
        })
        .flatten()
        .filter(|input| matches!(input, DesktopInput::Key { .. }))
        .count();
    assert_eq!(sent, 0, "its A never sent, down or up");
}

#[test]
fn a_key_is_released_with_the_keysym_it_was_pressed_with() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, tab, attempt) = opened(dir.path());
    let (input, _received) = tokio::sync::mpsc::unbounded_channel();
    connection(
        &mut shell,
        tab,
        attempt,
        ConnectionEvent::RdpReady {
            framebuffer: Framebuffer::new(1280, 800),
            input,
            size: tokio::sync::watch::channel(None).0,
            clipboard: None,
        },
    );
    let mut ui = simulator(&shell);
    // Shift+1 types '!'; Shift let go first, the key's release types '1'.
    let _ = ui.simulate([
        key_event("!", iced::keyboard::key::Code::Digit1, true),
        key_event("1", iced::keyboard::key::Code::Digit1, false),
    ]);
    let keysyms: Vec<(Option<u32>, bool)> = ui
        .into_messages()
        .filter_map(|message| match message {
            Message::App(AppMessage::DesktopInput { inputs, .. }) => Some(inputs),
            _ => None,
        })
        .flatten()
        .filter_map(|input| match input {
            DesktopInput::Key {
                keysym, pressed, ..
            } => Some((keysym, pressed)),
            _ => None,
        })
        .collect();
    assert_eq!(keysyms, [(Some(0x21), true), (Some(0x21), false)]);
}

#[test]
fn the_desktop_reports_the_size_it_is_shown_at() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, tab, attempt) = opened(dir.path());
    let (input, _received) = tokio::sync::mpsc::unbounded_channel();
    connection(
        &mut shell,
        tab,
        attempt,
        ConnectionEvent::RdpReady {
            framebuffer: Framebuffer::new(1280, 800),
            input,
            size: tokio::sync::watch::channel(None).0,
            clipboard: None,
        },
    );
    let mut ui = simulator(&shell);
    // A frame drawn: the view learns its area.
    let _ = ui.snapshot(&Theme::Dark).expect("drawn");
    let sizes: Vec<(u16, u16)> = ui
        .into_messages()
        .filter_map(|message| match message {
            Message::App(AppMessage::DesktopResize {
                tab: resized,
                width,
                height,
            }) if resized == tab => Some((width, height)),
            _ => None,
        })
        .collect();
    let [(width, height)] = sizes.as_slice() else {
        panic!("one report, got {sizes:?}");
    };
    // The window less the sidebar and the tab bar.
    assert!(
        (600..1200).contains(width) && (400..720).contains(height),
        "{width}x{height}"
    );
}

/// A connected RDP tab, and what its session receives.
fn connected(
    dir: &Path,
) -> (
    Shell,
    TabId,
    tokio::sync::mpsc::UnboundedReceiver<Vec<heimdall_rdp::Operation>>,
) {
    let (mut shell, tab, attempt) = opened(dir);
    let (input, received) = tokio::sync::mpsc::unbounded_channel();
    connection(
        &mut shell,
        tab,
        attempt,
        ConnectionEvent::RdpReady {
            framebuffer: Framebuffer::new(1280, 800),
            input,
            size: tokio::sync::watch::channel(None).0,
            clipboard: None,
        },
    );
    (shell, tab, received)
}

#[test]
fn ctrl_alt_del_from_the_menu_reaches_the_server_pressed_then_released_in_reverse() {
    use heimdall_rdp::{Operation, Scancode};
    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, tab, mut received) = connected(dir.path());
    // The menu is a pick list, which the simulator cannot open: its message is sent here.
    let _ = shell.update(Message::App(AppMessage::SendKeys {
        tab,
        keys: heimdall_app::SpecialKeys::CtrlAltDel,
    }));
    let mut operations = Vec::new();
    while let Ok(batch) = received.try_recv() {
        operations.extend(batch);
    }
    let (ctrl, alt, del) = (
        Scancode::from_u8(false, 0x1D),
        Scancode::from_u8(false, 0x38),
        Scancode::from_u8(true, 0x53),
    );
    let keys: Vec<(bool, Scancode)> = operations
        .iter()
        .map(|operation| match operation {
            Operation::KeyPressed(code) => (true, *code),
            Operation::KeyReleased(code) => (false, *code),
            other => panic!("not a key: {other:?}"),
        })
        .collect();
    assert_eq!(
        keys,
        [
            (true, ctrl),
            (true, alt),
            (true, del),
            (false, del),
            (false, alt),
            (false, ctrl),
        ]
    );
}

#[test]
fn the_anti_idle_badge_shows_while_the_keys_go_and_its_click_stops_them() {
    let dir = tempfile::tempdir().expect("dir");
    let mut core = app_of(
        dir.path(),
        RdpProfile {
            anti_idle: true,
            ..profile(heimdall_core::profile::RdpOptions::default())
        },
    );
    let effects = core.update(AppMessage::OpenRdp(ProfileId::new("dc")));
    let [Effect::ConnectRdp { tab, attempt, .. }] = effects.as_slice() else {
        panic!("one connection");
    };
    let (tab, attempt) = (*tab, *attempt);
    let mut shell = Shell::with_app(core);
    let (input, _received) = tokio::sync::mpsc::unbounded_channel();
    connection(
        &mut shell,
        tab,
        attempt,
        ConnectionEvent::RdpReady {
            framebuffer: Framebuffer::new(1280, 800),
            input,
            size: tokio::sync::watch::channel(None).0,
            clipboard: None,
        },
    );
    {
        let mut ui = simulator(&shell);
        ui.click("Anti-idle").expect("the badge");
        let messages: Vec<Message> = ui.into_messages().collect();
        assert!(
            messages.iter().any(|message| matches!(
                message,
                Message::App(AppMessage::StopAntiIdle(stopped)) if *stopped == tab
            )),
            "{messages:?}"
        );
        for message in messages {
            let _ = shell.update(message);
        }
    }
    let mut ui = simulator(&shell);
    assert!(ui.find("Anti-idle").is_err(), "gone once stopped");
}

#[test]
fn a_session_asking_for_no_anti_idle_shows_no_badge() {
    let dir = tempfile::tempdir().expect("dir");
    let (shell, _, _received) = connected(dir.path());
    let mut ui = simulator(&shell);
    assert!(ui.find("Anti-idle").is_err());
}

#[test]
fn full_screen_shows_the_session_only_and_comes_back() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, _, _received) = connected(dir.path());
    {
        let mut ui = simulator(&shell);
        ui.find("Sessions").expect("the tree");
        ui.click("Fullscreen (F11)").expect("the button");
        assert!(
            ui.into_messages()
                .any(|message| matches!(message, Message::ToggleFullscreen))
        );
    }
    let _ = shell.update(Message::ToggleFullscreen);
    {
        let mut ui = simulator(&shell);
        assert!(ui.find("Sessions").is_err(), "no tree in full screen");
        ui.find("Exit fullscreen (F11)").expect("the way back");
    }
    let _ = shell.update(Message::ToggleFullscreen);
    let mut ui = simulator(&shell);
    ui.find("Sessions").expect("the tree again");
    ui.find("Fullscreen (F11)").expect("the button again");
}

#[test]
fn f11_stays_with_the_window_and_never_reaches_the_server() {
    let dir = tempfile::tempdir().expect("dir");
    let (shell, _, _received) = connected(dir.path());
    let mut ui = simulator(&shell);
    let _ = ui.simulate([
        key_event("", iced::keyboard::key::Code::F11, true),
        key_event("", iced::keyboard::key::Code::F11, false),
        key_event("a", iced::keyboard::key::Code::KeyA, true),
    ]);
    let keys: Vec<DesktopInput> = ui
        .into_messages()
        .filter_map(|message| match message {
            Message::App(AppMessage::DesktopInput { inputs, .. }) => Some(inputs),
            _ => None,
        })
        .flatten()
        .filter(|input| matches!(input, DesktopInput::Key { .. }))
        .collect();
    // Only the A: a positive control that keys do reach it.
    assert_eq!(keys.len(), 1, "{keys:?}");
}

#[test]
fn a_desktop_smaller_than_its_tab_is_drawn_in_its_middle() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, tab, attempt) = opened(dir.path());
    let (input, _received) = tokio::sync::mpsc::unbounded_channel();
    connection(
        &mut shell,
        tab,
        attempt,
        ConnectionEvent::RdpReady {
            framebuffer: Framebuffer::new(200, 100),
            input,
            size: tokio::sync::watch::channel(None).0,
            clipboard: None,
        },
    );
    // Matching the window, as an RDP server does by default: from the tab's corner, and the
    // middle of the area right of the sidebar is the window's background.
    let (corner, renderer) = pixel_at(&shell, 300, DESKTOP_CORNER_Y);
    assert_eq!(corner, [0, 0, 0, 255], "drawn by {renderer}");
    let (middle, renderer) = pixel_at(&shell, 730, 420);
    assert_ne!(middle, [0, 0, 0, 255], "drawn by {renderer}");

    let _ = shell.update(Message::DesktopFit { tab, fit: true });
    // Fitted: never enlarged, centred; the corner is the window's background now.
    let (middle, renderer) = pixel_at(&shell, 730, 420);
    assert_eq!(middle, [0, 0, 0, 255], "drawn by {renderer}");
    let (corner, renderer) = pixel_at(&shell, 300, DESKTOP_CORNER_Y);
    assert_ne!(corner, [0, 0, 0, 255], "drawn by {renderer}");
}

/// The sizes the view reports when drawn.
fn sizes_reported(shell: &Shell) -> Vec<(u16, u16)> {
    let mut ui = simulator(shell);
    let _ = ui.snapshot(&Theme::Dark).expect("drawn");
    ui.into_messages()
        .filter_map(|message| match message {
            Message::App(AppMessage::DesktopResize { width, height, .. }) => Some((width, height)),
            _ => None,
        })
        .collect()
}

/// A 200x100 black desktop connected for a profile given `options`.
fn small_desktop(dir: &Path, options: heimdall_core::profile::RdpOptions) -> (Shell, TabId) {
    let (mut shell, tab, attempt) = opened_with(dir, options);
    let (input, _received) = tokio::sync::mpsc::unbounded_channel();
    connection(
        &mut shell,
        tab,
        attempt,
        ConnectionEvent::RdpReady {
            framebuffer: Framebuffer::new(200, 100),
            input,
            size: tokio::sync::watch::channel(None).0,
            clipboard: None,
        },
    );
    (shell, tab)
}

#[test]
fn without_dynamic_resolution_the_desktop_is_scaled_and_asks_the_tab_size_once() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, tab) = small_desktop(
        dir.path(),
        heimdall_core::profile::RdpOptions {
            dynamic_resolution: false,
            ..heimdall_core::profile::RdpOptions::default()
        },
    );
    // Fitted, as the C# session scales it: centred, the tab's corner is background.
    let (corner, renderer) = pixel_at(&shell, 300, DESKTOP_CORNER_Y);
    assert_ne!(corner, [0, 0, 0, 255], "drawn by {renderer}");
    let (middle, renderer) = pixel_at(&shell, 730, 420);
    assert_eq!(middle, [0, 0, 0, 255], "drawn by {renderer}");
    let first = sizes_reported(&shell);
    let [(width, height)] = first.as_slice() else {
        panic!("one report while scaled, got {first:?}");
    };
    let _ = shell.update(Message::App(AppMessage::DesktopResize {
        tab,
        width: *width,
        height: *height,
    }));
    assert_eq!(sizes_reported(&shell), [], "once only");
}

#[test]
fn a_fixed_desktop_shown_pixel_for_pixel_is_centred_and_asks_no_size() {
    use heimdall_core::profile::{RdpOptions, Resolution};

    let dir = tempfile::tempdir().expect("dir");
    let (shell, _) = small_desktop(
        dir.path(),
        RdpOptions {
            resolution: Resolution::Fixed,
            scale_fixed: false,
            ..RdpOptions::default()
        },
    );
    assert_eq!(sizes_reported(&shell), [], "its own size");
    let (corner, renderer) = pixel_at(&shell, 300, DESKTOP_CORNER_Y);
    assert_ne!(corner, [0, 0, 0, 255], "drawn by {renderer}");
    let (middle, renderer) = pixel_at(&shell, 730, 420);
    assert_eq!(middle, [0, 0, 0, 255], "centred: drawn by {renderer}");
}

#[test]
fn a_fitted_desktop_asks_the_server_for_no_size() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, tab, _received) = connected(dir.path());
    let _ = shell.update(Message::DesktopFit { tab, fit: true });
    let mut ui = simulator(&shell);
    let _ = ui.snapshot(&Theme::Dark).expect("drawn");
    assert!(
        !ui.into_messages()
            .any(|message| matches!(message, Message::App(AppMessage::DesktopResize { .. }))),
        "the server keeps its size"
    );
}

#[test]
fn an_unknown_certificate_is_asked_about_in_the_csharp_words_with_just_this_once() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, tab, attempt) = opened(dir.path());
    connection(
        &mut shell,
        tab,
        attempt,
        ConnectionEvent::UnknownRdpCertificate {
            subject: Some("CN=dc.lab".to_owned()),
            details: None,
            host: "dc.lab".to_owned(),
            port: 3389,
            fingerprint: "SHA256:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"
                .parse()
                .expect("fingerprint"),
            certificate: heimdall_rdp::CertificateHash::of(b"the certificate of dc.lab"),
        },
    );
    let mut ui = simulator(&shell);
    ui.find("Unrecognised Server Certificate").expect("title");
    ui.find(
        "\"Domain controller\" answered at dc.lab:3389, presenting a certificate this profile has never approved.",
    )
    .expect("body");
    ui.find("Subject: CN=dc.lab")
        .expect("the subject, as the C# prompt");
    ui.click("Just this once").expect("once");
    assert!(ui.into_messages().any(|message| matches!(
        message,
        Message::App(AppMessage::HostKeyTrustOnce(trusted)) if trusted == tab
    )));
    let mut ui = simulator(&shell);
    ui.click("Trust this certificate").expect("always");
    assert!(ui.into_messages().any(|message| matches!(
        message,
        Message::App(AppMessage::HostKeyDecision { tab: decided, accept: true }) if decided == tab
    )));
    let mut ui = simulator(&shell);
    ui.click("Do not connect").expect("refuse");
    assert!(ui.into_messages().any(|message| matches!(
        message,
        Message::App(AppMessage::HostKeyDecision { accept: false, .. })
    )));

    // Declined: the C# line, a choice the user made, not an error to report.
    let _ = shell.update(Message::App(AppMessage::HostKeyDecision {
        tab,
        accept: false,
    }));
    let mut ui = simulator(&shell);
    ui.find("Connection cancelled: you did not approve the certificate this server presented.")
        .expect("the C# line");
    assert!(ui.find("Copy error").is_err(), "nothing to report");
    ui.find("Reconnect").expect("a way back");
}

#[test]
fn a_certificate_renewed_on_the_trusted_key_is_said_so_with_both_validities_and_no_system_check() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, tab, attempt) = opened(dir.path());
    let at = |seconds| std::time::UNIX_EPOCH + std::time::Duration::from_secs(seconds);
    // 2020-01-01 to 2045-01-01 on record; 2021-01-01 to 2046-01-01 presented.
    let (recorded, presented) = (
        heimdall_rdp::Validity {
            not_before: at(1_577_836_800),
            not_after: at(2_366_841_600),
        },
        heimdall_rdp::Validity {
            not_before: at(1_609_459_200),
            not_after: at(2_398_377_600),
        },
    );
    connection(
        &mut shell,
        tab,
        attempt,
        ConnectionEvent::UnknownRdpCertificate {
            subject: Some("CN=dc.lab".to_owned()),
            details: Some(Box::new(heimdall_app::CertificateDetails {
                issuer: "CN=dc.lab".to_owned(),
                validity: presented,
                issue: None,
                renewal: Some(heimdall_app::Renewal {
                    recorded: Some(recorded),
                }),
            })),
            host: "dc.lab".to_owned(),
            port: 3389,
            fingerprint: "SHA256:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"
                .parse()
                .expect("fingerprint"),
            certificate: heimdall_rdp::CertificateHash::of(b"the renewed certificate of dc.lab"),
        },
    );
    let mut ui = simulator(&shell);
    ui.find("Unrecognised Server Certificate").expect("title");
    ui.find(
        "Renewed certificate: same key, new certificate. The server presents another \
         certificate on the key you trusted. A renewal is routine, but whoever holds the key \
         could also have made it: approve it only if you expect this renewal.",
    )
    .expect("said renewed, as the FTPS and VNC questions");
    ui.find("Certificate on record valid from / until: 2020-01-01 00:00 - 2045-01-01 00:00")
        .expect("the certificate on record");
    ui.find("Valid from / until: 2021-01-01 00:00 - 2046-01-01 00:00")
        .expect("the one presented");
    ui.find("Issuer: CN=dc.lab").expect("its issuer");
    // No authority of this computer was asked: no issue said.
    for issue in [
        "Validation issue: The certificate is self-signed: no certificate authority vouches for it.",
        "Validation issue: It was issued by a certificate authority this computer does not trust.",
    ] {
        assert!(ui.find(issue).is_err(), "{issue}");
    }
    ui.find("Trust this certificate").expect("the same answers");
    ui.find("Just this once").expect("the same answers");
}

#[test]
fn a_session_the_server_ended_says_why() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, tab, attempt) = opened(dir.path());
    connection(
        &mut shell,
        tab,
        attempt,
        ConnectionEvent::Ended {
            reason: Ending::Other("Another user connected to the session".to_owned()),
        },
    );
    let mut ui = simulator(&shell);
    ui.find("The Remote Desktop session has ended.")
        .expect("ended");
    ui.find("Reason: Another user connected to the session")
        .expect("its reason");
    ui.click("Reconnect").expect("reconnect");
}

#[test]
fn a_session_ended_by_the_server_says_it_as_the_csharp_and_a_logoff_says_nothing_more() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, tab, attempt) = opened(dir.path());
    connection(
        &mut shell,
        tab,
        attempt,
        ConnectionEvent::Ended {
            reason: Ending::AdminDisconnect,
        },
    );
    {
        let mut ui = simulator(&shell);
        ui.find(
            "Error: The remote computer ended the session. An administrator may have ended it, \
             the connection may have failed while it was being established, or a network \
             problem may have interrupted it.",
        )
        .expect("the C# sentence, as an error");
    }

    let other = tempfile::tempdir().expect("dir");
    let (mut shell, tab, attempt) = opened(other.path());
    connection(
        &mut shell,
        tab,
        attempt,
        ConnectionEvent::Ended {
            reason: Ending::Logoff,
        },
    );
    let mut ui = simulator(&shell);
    ui.find("The Remote Desktop session has ended.")
        .expect("ended");
    assert!(ui.find("Error:").is_err(), "a logoff is not an error");
    assert!(ui.find("Reason:").is_err(), "nor a reason");
}

#[tokio::test]
async fn a_dropped_desktop_counts_down_to_its_next_attempt_and_can_be_stopped() {
    // Held from before the drop: the countdown read below runs on the clock.
    let _turn = common::render_turn();
    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, tab, attempt) = opened(dir.path());
    let (input, _received) = tokio::sync::mpsc::unbounded_channel();
    connection(
        &mut shell,
        tab,
        attempt,
        ConnectionEvent::RdpReady {
            framebuffer: Framebuffer::new(1280, 800),
            input,
            size: tokio::sync::watch::channel(None).0,
            clipboard: None,
        },
    );
    connection(
        &mut shell,
        tab,
        attempt,
        ConnectionEvent::Failed(UiError::Timeout),
    );
    {
        let mut ui = simulator(&shell);
        ui.find("Reconnecting (attempt 1/20)...")
            .expect("which attempt");
        ui.find("in 2s").expect("how long");
        assert!(
            ui.find("The connection failed").is_err(),
            "not the failure yet"
        );
        ui.click("Cancel").expect("cancel");
        assert!(ui.into_messages().any(|message| matches!(
            message,
            Message::App(AppMessage::CancelAutoReconnect(cancelled)) if cancelled == tab
        )));
    }
    // The attempt itself says it is one.
    let _ = shell.update(Message::App(AppMessage::AutoReconnect { tab, attempt }));
    let mut ui = simulator(&shell);
    ui.find("Reconnecting (attempt 1/20)...")
        .expect("while connecting");
}

#[test]
fn an_rdp_profile_connects_as_the_other_protocols() {
    use heimdall_ui::tree_view::TreeMenu;

    let dir = tempfile::tempdir().expect("dir");
    let mut shell = Shell::with_app(app(dir.path()));
    let id = ProfileId::new("dc");
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
    for label in ["SSH", "SFTP", "VNC", "Telnet"] {
        ui.find(label).expect(label);
    }
    ui.click("Telnet").expect("telnet");
    assert!(ui.into_messages().any(|message| matches!(
        &message,
        Message::MenuChoice(AppMessage::ConnectAs {
            id: asked,
            protocol: heimdall_app::ConnectAs::Telnet,
        }) if *asked == id
    )));
}

#[test]
fn an_rdp_profile_connects_with_a_mode_this_once_and_its_tab_says_so() {
    use heimdall_core::profile::RdpMode;
    use heimdall_ui::tree_view::TreeMenu;

    let dir = tempfile::tempdir().expect("dir");
    let mut shell = Shell::with_app(app(dir.path()));
    let id = ProfileId::new("dc");
    // As the C# menu: "Connect with..." right under Connect, for an RDP profile.
    let _ = shell.update(Message::OpenTreeMenu(TreeMenu::Profile(id.clone())));
    {
        let mut ui = simulator(&shell);
        ui.click("Connect with...").expect("connect with");
        assert!(ui.into_messages().any(|message| matches!(
            &message,
            Message::OpenTreeMenu(TreeMenu::ConnectWith(asked)) if *asked == id
        )));
    }
    let _ = shell.update(Message::OpenTreeMenu(TreeMenu::ConnectWith(id.clone())));
    for (label, mode) in [
        ("Connect (embedded)", RdpMode::Embedded),
        ("Connect (external mstsc)", RdpMode::External),
    ] {
        let mut ui = simulator(&shell);
        ui.click(label).expect(label);
        assert!(
            ui.into_messages().any(|message| matches!(
                &message,
                Message::MenuChoice(AppMessage::OpenRdpWith { id: asked, mode: chosen })
                    if *asked == id && *chosen == mode
            )),
            "{label}"
        );
    }

    let _ = shell.update(Message::MenuChoice(AppMessage::OpenRdpWith {
        id: id.clone(),
        mode: RdpMode::Embedded,
    }));
    assert_eq!(shell.app().tabs.len(), 1);
    {
        let mut ui = simulator(&shell);
        ui.find("Domain controller (forced embedded)")
            .expect("the C# title suffix");
    }
    // A name the user gives replaces it, as the C# custom title.
    let tab = shell.app().tabs[0].id;
    let _ = shell.update(Message::App(AppMessage::TabMenu(
        heimdall_app::TabMenuMessage::Rename(tab),
    )));
    let _ = shell.update(Message::App(AppMessage::TabMenu(
        heimdall_app::TabMenuMessage::NameEdited("Primary".to_owned()),
    )));
    let _ = shell.update(Message::App(AppMessage::ConfirmDialog));
    assert_eq!(shell.app().tabs[0].custom_title.as_deref(), Some("Primary"));
    let mut ui = simulator(&shell);
    ui.find("Primary").expect("the name given");
    assert!(ui.find("Primary (forced embedded)").is_err());
}

#[test]
fn an_rdp_tab_s_menu_offers_the_csharp_resolution_menu() {
    use heimdall_ui::tree_view::TreeMenu;

    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, tab, _received) = connected(dir.path());
    let _ = shell.update(Message::OpenTreeMenu(TreeMenu::Tab(tab)));
    {
        let mut ui = simulator(&shell);
        ui.find("Resolution").expect("the C# submenu");
    }
    let _ = shell.update(Message::OpenTreeMenu(TreeMenu::Resolution(tab)));
    {
        let mut ui = simulator(&shell);
        for entry in [
            "Active mode: Fit window",
            "Match window",
            "1920 x 1080",
            "3840 x 2160",
            "Custom...",
            "Save as default for this server",
        ] {
            ui.find(entry).expect(entry);
        }
    }
    let _ = shell.update(Message::MenuChoice(AppMessage::TabMenu(
        heimdall_app::TabMenuMessage::Resolution {
            tab,
            choice: heimdall_app::ResolutionChoice::Fixed {
                width: 1280,
                height: 720,
            },
        },
    )));
    let _ = shell.update(Message::OpenTreeMenu(TreeMenu::Resolution(tab)));
    let mut ui = simulator(&shell);
    ui.find("Active mode: Fixed (1280x720)")
        .expect("the size chosen");
}

#[test]
fn a_fixed_desktop_still_tells_the_tab_s_size_without_asking_it_of_the_server() {
    use heimdall_core::profile::{RdpOptions, Resolution};

    let dir = tempfile::tempdir().expect("dir");
    let (shell, tab) = small_desktop(
        dir.path(),
        RdpOptions {
            resolution: Resolution::Fixed,
            scale_fixed: false,
            ..RdpOptions::default()
        },
    );
    let mut ui = simulator(&shell);
    let _ = ui.snapshot(&Theme::Dark).expect("drawn");
    let messages: Vec<Message> = ui.into_messages().collect();
    assert!(
        !messages
            .iter()
            .any(|message| matches!(message, Message::App(AppMessage::DesktopResize { .. }))),
        "its own size"
    );
    assert!(
        messages.iter().any(|message| matches!(
            message,
            Message::App(AppMessage::DesktopShown { tab: shown, width, height })
                if *shown == tab && *width > 0 && *height > 0
        )),
        "the tab's size is known"
    );
}

#[test]
fn the_session_bar_opens_the_resolution_menu_which_names_the_profile_s_mode() {
    use heimdall_core::profile::{RdpOptions, Resolution};
    use heimdall_ui::tree_view::TreeMenu;

    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, tab) = small_desktop(
        dir.path(),
        RdpOptions {
            resolution: Resolution::SmartSizing,
            ..RdpOptions::default()
        },
    );
    {
        let mut ui = simulator(&shell);
        ui.click("Resolution").expect("on the session bar");
        assert!(ui.into_messages().any(|message| matches!(
            message,
            Message::OpenTreeMenu(TreeMenu::Resolution(opened)) if opened == tab
        )));
    }
    let _ = shell.update(Message::OpenTreeMenu(TreeMenu::Resolution(tab)));
    let mut ui = simulator(&shell);
    ui.find("Active mode: Smart sizing")
        .expect("the profile's own mode");
}

/// A key going down with `modifiers` held, as a keyboard sends it.
fn modified_press(
    key: iced::keyboard::Key,
    code: iced::keyboard::key::Code,
    modifiers: iced::keyboard::Modifiers,
) -> iced::Event {
    use iced::keyboard::{Event, Location};
    iced::Event::Keyboard(Event::KeyPressed {
        key: key.clone(),
        modified_key: key,
        physical_key: iced::keyboard::key::Physical::Code(code),
        location: Location::Left,
        modifiers,
        text: None,
        repeat: false,
    })
}

/// The keys a window's events sent the desktop: pressed or released, by scancode.
fn keys_sent(messages: &[Message]) -> Vec<(bool, Option<heimdall_rdp::Scancode>)> {
    messages
        .iter()
        .filter_map(|message| match message {
            Message::App(AppMessage::DesktopInput { inputs, .. }) => Some(inputs.clone()),
            _ => None,
        })
        .flatten()
        .filter_map(|input| match input {
            DesktopInput::Key {
                scancode, pressed, ..
            } => Some((pressed, scancode)),
            _ => None,
        })
        .collect()
}

#[test]
fn ctrl_alt_home_lets_go_of_the_keys_held_and_gives_the_keyboard_back() {
    use heimdall_rdp::Scancode;
    use iced::keyboard::key::{Code, Named};
    use iced::keyboard::{Key, Modifiers};

    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, _, _received) = connected(dir.path());
    let messages: Vec<Message> = {
        let mut ui = simulator(&shell);
        let _ = ui.simulate([
            modified_press(
                Key::Named(Named::Control),
                Code::ControlLeft,
                Modifiers::CTRL,
            ),
            modified_press(
                Key::Named(Named::Alt),
                Code::AltLeft,
                Modifiers::CTRL | Modifiers::ALT,
            ),
            modified_press(
                Key::Named(Named::Home),
                Code::Home,
                Modifiers::CTRL | Modifiers::ALT,
            ),
        ]);
        ui.into_messages().collect()
    };
    let (ctrl, alt) = (
        Some(Scancode::from_u8(false, 0x1D)),
        Some(Scancode::from_u8(false, 0x38)),
    );
    // Home itself never reaches the server: it is the window's.
    assert_eq!(
        keys_sent(&messages),
        [(true, ctrl), (true, alt), (false, ctrl), (false, alt)],
        "pressed, then let go"
    );
    assert!(
        messages
            .iter()
            .any(|message| matches!(message, Message::ContentRelease)),
        "{messages:?}"
    );
    for message in messages {
        let _ = shell.update(message);
    }
    // The keyboard is the window's: a key typed now stays here.
    let mut ui = simulator(&shell);
    let _ = ui.simulate([key_event("a", Code::KeyA, true)]);
    let messages: Vec<Message> = ui.into_messages().collect();
    assert!(keys_sent(&messages).is_empty(), "{messages:?}");
}

#[test]
fn the_desktop_bar_shows_what_the_session_shares_and_offers_the_shortcuts() {
    let dir = tempfile::tempdir().expect("dir");
    let (shell, _, _received) = connected(dir.path());
    let mut ui = simulator(&shell);
    // The profile shares its clipboard only: no drives, no sound.
    ui.find("Clipboard").expect("the clipboard shared");
    assert!(ui.find("Drives").is_err(), "no drive shared");
    assert!(ui.find("Sound").is_err(), "no sound played here");
    ui.click("Keyboard shortcuts...").expect("the help");
    let messages: Vec<Message> = ui.into_messages().collect();
    assert!(
        messages
            .iter()
            .any(|message| matches!(message, Message::App(AppMessage::ShowShortcuts))),
        "{messages:?}"
    );
}

#[test]
fn a_session_sharing_its_drives_and_sound_shows_both() {
    let dir = tempfile::tempdir().expect("dir");
    let mut core = app_of(
        dir.path(),
        RdpProfile {
            redirect_clipboard: false,
            redirect_drives: true,
            ..profile(heimdall_core::profile::RdpOptions {
                audio: heimdall_core::profile::AudioPlayback::Local,
                ..heimdall_core::profile::RdpOptions::default()
            })
        },
    );
    let effects = core.update(AppMessage::OpenRdp(ProfileId::new("dc")));
    let [Effect::ConnectRdp { tab, attempt, .. }] = effects.as_slice() else {
        panic!("one connection");
    };
    let (tab, attempt) = (*tab, *attempt);
    let mut shell = Shell::with_app(core);
    let (input, _received) = tokio::sync::mpsc::unbounded_channel();
    connection(
        &mut shell,
        tab,
        attempt,
        ConnectionEvent::RdpReady {
            framebuffer: Framebuffer::new(1280, 800),
            input,
            size: tokio::sync::watch::channel(None).0,
            clipboard: None,
        },
    );
    let mut ui = simulator(&shell);
    ui.find("Drives").expect("the drives shared");
    ui.find("Sound").expect("the sound played here");
    assert!(ui.find("Clipboard").is_err(), "no clipboard shared");
}

#[test]
fn the_error_report_says_the_route_and_how_long_the_session_lasted_the_anonymized_one_no_name() {
    use heimdall_core::profile::SshGateway;

    let dir = tempfile::tempdir().expect("dir");
    let mut store = ProfileStore::open(dir.path().join("profiles.toml")).expect("store");
    store.merge_gateways([SshGateway {
        id: ProfileId::new("edge"),
        name: "Edge".to_owned(),
        host: "edge.corp.lab".to_owned(),
        port: 22,
        username: Some("ops".to_owned()),
        key_path: None,
        parent: None,
    }]);
    store.save().expect("save");
    let mut core = app_of(
        dir.path(),
        RdpProfile {
            gateway: Some(ProfileId::new("edge")),
            // The failure stays on the card, no attempt chained.
            auto_reconnect: false,
            ..profile(heimdall_core::profile::RdpOptions::default())
        },
    );
    let effects = core.update(AppMessage::OpenRdp(ProfileId::new("dc")));
    let Some((tab, attempt)) = effects.iter().find_map(|effect| match effect {
        Effect::ConnectRdp { tab, attempt, .. } => Some((*tab, *attempt)),
        _ => None,
    }) else {
        panic!("{effects:?}");
    };
    let mut shell = Shell::with_app(core);
    let (input, _received) = tokio::sync::mpsc::unbounded_channel();
    connection(
        &mut shell,
        tab,
        attempt,
        ConnectionEvent::RdpReady {
            framebuffer: Framebuffer::new(1280, 800),
            input,
            size: tokio::sync::watch::channel(None).0,
            clipboard: None,
        },
    );
    connection(
        &mut shell,
        tab,
        attempt,
        ConnectionEvent::Failed(UiError::Timeout),
    );

    let report = shell
        .failure_report(tab, std::time::UNIX_EPOCH)
        .expect("a report");
    let lines: Vec<&str> = report.lines().collect();
    assert_eq!(lines[0], "Heimdall RDP error report");
    assert_eq!(lines[2], "Server: Domain controller (dc.lab:3389)");
    assert_eq!(lines[3], "Tunnel: via Edge", "{report}");
    assert!(
        lines[4].starts_with("Session: connected for 0m "),
        "from connected to failed: {report}"
    );
    assert!(lines[5].starts_with("App: "), "{report}");

    let anonymous = shell
        .anonymous_report(tab, std::time::UNIX_EPOCH)
        .expect("a report");
    let lines: Vec<&str> = anonymous.lines().collect();
    assert_eq!(lines[2], "Tunnel: through 1 SSH gateway", "{anonymous}");
    assert!(
        lines[3].starts_with("Session: connected for 0m "),
        "{anonymous}"
    );
    for named in ["dc.lab", "Edge", "edge.corp.lab", "Domain controller"] {
        assert!(!anonymous.contains(named), "{named} in {anonymous}");
    }
}

#[test]
fn a_session_that_failed_before_it_connected_reports_no_duration() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, tab, attempt) = opened(dir.path());
    connection(
        &mut shell,
        tab,
        attempt,
        ConnectionEvent::Failed(UiError::Timeout),
    );
    for report in [
        shell.failure_report(tab, std::time::UNIX_EPOCH),
        shell.anonymous_report(tab, std::time::UNIX_EPOCH),
    ] {
        let report = report.expect("a report");
        assert!(!report.contains("Session:"), "{report}");
        assert!(
            !report.contains("Tunnel:"),
            "straight to the server: {report}"
        );
    }
}

#[test]
fn a_desktop_settling_counts_down_on_its_bar_and_its_resolution_menu_skips_the_wait() {
    use heimdall_ui::tree_view::TreeMenu;

    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, tab, _received) = connected(dir.path());
    let left = shell.app().tabs[0]
        .desktop
        .as_ref()
        .expect("desktop")
        .stabilization_seconds_left(std::time::Instant::now())
        .expect("settling after connecting");
    {
        let mut ui = simulator(&shell);
        // Drawn a moment later: the countdown may have passed a second.
        let shown = [left, left.saturating_sub(1)].into_iter().any(|seconds| {
            ui.find(format!("Stabilizing session... {seconds}s").as_str())
                .is_ok()
        });
        assert!(shown, "the C# status line's countdown");
    }
    let _ = shell.update(Message::OpenTreeMenu(TreeMenu::Resolution(tab)));
    {
        let mut ui = simulator(&shell);
        ui.click("Skip stabilization")
            .expect("offered while it settles");
        assert!(ui.into_messages().any(|message| matches!(
            message,
            Message::MenuChoice(AppMessage::SkipStabilization(skipped)) if skipped == tab
        )));
    }
    let _ = shell.update(Message::MenuChoice(AppMessage::SkipStabilization(tab)));
    {
        let mut ui = simulator(&shell);
        ui.find("Stabilization skipped - dynamic resolution is now active.")
            .expect("the C# notice");
        assert!(ui.find("Stabilizing session... 10s").is_err());
    }
    let _ = shell.update(Message::OpenTreeMenu(TreeMenu::Resolution(tab)));
    let mut ui = simulator(&shell);
    ui.find("Match window").expect("the menu");
    assert!(
        ui.find("Skip stabilization").is_err(),
        "no longer offered once over"
    );
}

/// Environment variable naming a directory for PNG snapshots.
const SNAPSHOT_VARIABLE: &str = "HEIMDALL_SNAPSHOT_DIR";

/// A picture of the window, written as `name` to the folder [`SNAPSHOT_VARIABLE`] names.
fn snapshot(shell: &Shell, name: &str) {
    let Some(dir) = std::env::var_os(SNAPSHOT_VARIABLE) else {
        return;
    };
    let dir = Path::new(&dir);
    // iced names the picture after its renderer: clear every variant, or an old picture is
    // only compared against and never replaced.
    let stem = name.trim_end_matches(".png");
    if let Ok(entries) = std::fs::read_dir(dir) {
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
        .matches_image(dir.join(name))
        .expect("written");
}

/// NTSTATUS of an expired password.
const STATUS_PASSWORD_EXPIRED: u32 = 0xC000_0071;

/// The tab of `shell` with identifier `tab`.
fn tab_of(shell: &Shell, tab: TabId) -> &heimdall_app::Tab {
    shell.app().tab(tab).expect("tab")
}

/// A shell whose RDP tab failed with `error`, its attempts not chained.
fn failed_with(dir: &Path, error: UiError) -> (Shell, TabId) {
    let mut core = app_of(
        dir,
        RdpProfile {
            auto_reconnect: false,
            ..profile(heimdall_core::profile::RdpOptions::default())
        },
    );
    let effects = core.update(AppMessage::OpenRdp(ProfileId::new("dc")));
    let [Effect::ConnectRdp { tab, attempt, .. }] = effects.as_slice() else {
        panic!("one connection");
    };
    let (tab, attempt) = (*tab, *attempt);
    let mut shell = Shell::with_app(core);
    connection(&mut shell, tab, attempt, ConnectionEvent::Failed(error));
    (shell, tab)
}

#[test]
fn a_refused_logon_shows_the_csharp_diagnostic_details_and_copies_them_with_the_error() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, tab) = failed_with(
        dir.path(),
        UiError::RdpRefused {
            refusal: heimdall_rdp::Refusal::PasswordExpired,
            status: Some(STATUS_PASSWORD_EXPIRED),
        },
    );
    {
        let mut ui = simulator(&shell);
        ui.find("Warning: The password has expired and must be changed before connecting.")
            .expect("the C# sentence");
        // Closed, as the C# expander: the header only.
        assert!(ui.find("0xC0000071").is_err(), "closed");
        ui.click("Diagnostic details").expect("the expander");
        let messages: Vec<Message> = ui.into_messages().collect();
        assert!(
            messages
                .iter()
                .any(|message| matches!(message, Message::ToggleFailureDetails(id) if *id == tab)),
            "{messages:?}"
        );
    }
    let _ = shell.update(Message::ToggleFailureDetails(tab));
    {
        let mut ui = simulator(&shell);
        ui.find("Stage").expect("its stage");
        ui.find("Network Level Authentication").expect("the logon");
        ui.find("Code").expect("its code");
        ui.find("0xC0000071").expect("the NTSTATUS");
        assert!(ui.find("Detail").is_err(), "no detail to give");
    }
    snapshot(&shell, "rdp-failure.png");

    let report = shell
        .failure_report(tab, std::time::UNIX_EPOCH)
        .expect("a report");
    let lines: Vec<&str> = report.lines().collect();
    let at = lines
        .iter()
        .position(|line| line.starts_with("Warning: The password has expired"))
        .expect("the error");
    assert_eq!(
        &lines[at + 1..],
        ["Stage: Network Level Authentication", "Code: 0xC0000071"],
        "{report}"
    );
}

#[test]
fn each_rdp_cause_the_csharp_words_is_said_in_its_words_with_its_stage() {
    use heimdall_app::NetworkFailure;

    let causes = [
        (
            UiError::Network {
                failure: NetworkFailure::Refused,
                detail: "Connection refused (os error 111)".to_owned(),
            },
            "Notice: Could not connect to the remote computer. It may be turned off, not on the \
             network, or Remote Desktop may be disabled.",
            "RDP connection",
            Some("Connection refused (os error 111)"),
        ),
        (
            UiError::Timeout,
            "Notice: The connection timed out while starting the session. The remote computer \
             did not respond. Check the host and network, then try reconnecting.",
            "RDP connection",
            None,
        ),
        (
            UiError::RdpEnded {
                ending: Ending::License,
                code: Some(0x101),
            },
            "Error: A Remote Desktop licensing error blocked the session. Contact your \
             administrator; the license server may be unreachable or out of CALs.",
            "RDP disconnect",
            Some(
                "[Protocol independent licensing error] A Remote Desktop License Server could \
                 not be found to provide a license",
            ),
        ),
        (
            UiError::JumpRefused {
                host: "dc.lab".to_owned(),
                port: 3389,
            },
            "Notice: The gateway could not reach dc.lab:3389. The target host or its RDP port \
             is unreachable from the SSH gateway.",
            "RDP tunnel",
            Some("dc.lab:3389"),
        ),
    ];
    for (error, said, stage, detail) in causes {
        let dir = tempfile::tempdir().expect("dir");
        let (mut shell, tab) = failed_with(dir.path(), error.clone());
        let _ = shell.update(Message::ToggleFailureDetails(tab));
        let mut ui = simulator(&shell);
        ui.find(said)
            .unwrap_or_else(|_| panic!("{error:?}: {said}"));
        ui.find(stage)
            .unwrap_or_else(|_| panic!("{error:?}: {stage}"));
        if let Some(detail) = detail {
            ui.find(detail)
                .unwrap_or_else(|_| panic!("{error:?}: {detail}"));
        }
    }
}

#[test]
fn the_health_dot_follows_the_session_as_the_csharp_one() {
    use heimdall_ui::rdp_status::Health;

    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, tab, attempt) = opened(dir.path());
    assert_eq!(Health::of(tab_of(&shell, tab)), Health::Transitional);
    let (input, _received) = tokio::sync::mpsc::unbounded_channel();
    connection(
        &mut shell,
        tab,
        attempt,
        ConnectionEvent::RdpReady {
            framebuffer: Framebuffer::new(1280, 800),
            input,
            size: tokio::sync::watch::channel(None).0,
            clipboard: None,
        },
    );
    assert_eq!(Health::of(tab_of(&shell, tab)), Health::Healthy);
    connection(
        &mut shell,
        tab,
        attempt,
        ConnectionEvent::Ended {
            reason: Ending::AdminDisconnect,
        },
    );
    assert_eq!(
        Health::of(tab_of(&shell, tab)),
        Health::Faulted,
        "ended by the server"
    );

    let other = tempfile::tempdir().expect("dir");
    let (mut shell, tab, attempt) = opened(other.path());
    connection(
        &mut shell,
        tab,
        attempt,
        ConnectionEvent::Ended {
            reason: Ending::Logoff,
        },
    );
    assert_eq!(Health::of(tab_of(&shell, tab)), Health::Idle, "logged off");

    let failed = tempfile::tempdir().expect("dir");
    let (shell, tab) = failed_with(
        failed.path(),
        UiError::RdpRefused {
            refusal: heimdall_rdp::Refusal::BadCredentials,
            status: None,
        },
    );
    assert_eq!(Health::of(tab_of(&shell, tab)), Health::Faulted);
    let cancelled = tempfile::tempdir().expect("dir");
    let (shell, tab) = failed_with(cancelled.path(), UiError::Cancelled);
    assert_eq!(Health::of(tab_of(&shell, tab)), Health::Idle, "the user's");
}

#[test]
fn the_phase_stepper_follows_the_steps_the_connection_reports() {
    use heimdall_rdp::Step;
    use heimdall_ui::rdp_status::ConnectPhase;

    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, tab, attempt) = opened(dir.path());
    assert_eq!(
        ConnectPhase::of(tab_of(&shell, tab)),
        ConnectPhase::Preparing
    );
    connection(
        &mut shell,
        tab,
        attempt,
        ConnectionEvent::RdpStep(Step::Connecting),
    );
    assert_eq!(
        ConnectPhase::of(tab_of(&shell, tab)),
        ConnectPhase::Connecting
    );
    connection(
        &mut shell,
        tab,
        attempt,
        ConnectionEvent::RdpStep(Step::Loading),
    );
    assert_eq!(ConnectPhase::of(tab_of(&shell, tab)), ConnectPhase::Loading);
    // Drawn on the connecting card, the health dot and the stepper above Cancel.
    {
        let mut ui = simulator(&shell);
        ui.find("Cancel").expect("the card");
    }
    let (input, _received) = tokio::sync::mpsc::unbounded_channel();
    connection(
        &mut shell,
        tab,
        attempt,
        ConnectionEvent::RdpReady {
            framebuffer: Framebuffer::new(1280, 800),
            input,
            size: tokio::sync::watch::channel(None).0,
            clipboard: None,
        },
    );
    assert_eq!(
        ConnectPhase::of(tab_of(&shell, tab)),
        ConnectPhase::Connected
    );
}

#[test]
fn a_dropped_desktop_counts_the_seconds_since_it_dropped() {
    let dir = tempfile::tempdir().expect("dir");
    let mut core = app(dir.path());
    let event = |core: &mut App, tab, attempt, event| {
        core.update(AppMessage::Connection {
            tab,
            attempt,
            event,
        })
    };
    let effects = core.update(AppMessage::OpenRdp(ProfileId::new("dc")));
    let [Effect::ConnectRdp { tab, attempt, .. }] = effects.as_slice() else {
        panic!("one connection");
    };
    let (tab, attempt) = (*tab, *attempt);
    let (input, _received) = tokio::sync::mpsc::unbounded_channel();
    let _ = event(
        &mut core,
        tab,
        attempt,
        ConnectionEvent::RdpReady {
            framebuffer: Framebuffer::new(1280, 800),
            input,
            size: tokio::sync::watch::channel(None).0,
            clipboard: None,
        },
    );
    let _ = event(
        &mut core,
        tab,
        attempt,
        ConnectionEvent::Failed(UiError::Timeout),
    );
    let since = core
        .tab(tab)
        .and_then(|found| found.retry)
        .expect("waiting to reconnect")
        .since;
    // Kept from one attempt to the next: counted from the drop.
    let effects = core.update(AppMessage::AutoReconnect { tab, attempt });
    let Some(again) = effects.iter().find_map(|effect| match effect {
        Effect::ConnectRdp { attempt, .. } => Some(*attempt),
        _ => None,
    }) else {
        panic!("{effects:?}");
    };
    let _ = event(
        &mut core,
        tab,
        again,
        ConnectionEvent::Failed(UiError::Timeout),
    );
    let retry = core
        .tab(tab)
        .and_then(|found| found.retry)
        .expect("waiting again");
    assert_eq!(retry.attempt, 2);
    assert_eq!(retry.since, since, "from the drop, not the attempt");
    assert_eq!(
        heimdall_ui::rdp_status::elapsed_text(retry, since + std::time::Duration::from_secs(42)),
        "42s elapsed"
    );
}

#[test]
fn the_desktop_bar_shows_the_redirections_not_shared_behind_their_badge() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, tab, _received) = connected(dir.path());
    {
        let mut ui = simulator(&shell);
        // The profile shares its clipboard only: seven more behind the badge.
        ui.click("+7").expect("the badge");
        let messages: Vec<Message> = ui.into_messages().collect();
        assert!(
            messages.iter().any(
                |message| matches!(message, Message::ShowDisabledRedirections(id) if *id == tab)
            ),
            "{messages:?}"
        );
    }
    let _ = shell.update(Message::ShowDisabledRedirections(tab));
    let mut ui = simulator(&shell);
    assert!(ui.find("+7").is_err(), "shown: no badge");
    snapshot(&shell, "rdp-bar.png");

    let listed = heimdall_ui::rdp_status::redirections(&RdpProfile {
        extras: heimdall_core::profile::RdpExtras {
            redirect_printers: true,
            ..heimdall_core::profile::RdpExtras::default()
        },
        ..profile(heimdall_core::profile::RdpOptions::default())
    });
    let said: Vec<String> = listed
        .iter()
        .map(heimdall_ui::rdp_status::Redirection::status)
        .collect();
    assert_eq!(
        said,
        [
            "Clipboard redirection: enabled",
            "Drive redirection: disabled",
            "Printer redirection: not supported yet by the built-in client",
            "COM port redirection: disabled",
            "Smart card redirection: disabled",
            "USB redirection: disabled",
            "Audio redirection: disabled",
            "Multi-monitor: disabled",
        ]
    );
    assert_eq!(heimdall_ui::rdp_status::hidden(&listed), 7);
}

#[test]
fn a_fixed_size_smaller_than_the_tab_says_so_at_first() {
    use heimdall_core::profile::{RdpOptions, Resolution};

    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, tab) = small_desktop(
        dir.path(),
        RdpOptions {
            resolution: Resolution::Fixed,
            scale_fixed: false,
            ..RdpOptions::default()
        },
    );
    let (width, height) = tab_of(&shell, tab)
        .desktop
        .as_deref()
        .and_then(heimdall_app::DesktopPane::fixed_size)
        .expect("a fixed size");
    // The tab's size comes from its first drawing; the hint from what follows.
    let reported: Vec<Message> = {
        let mut ui = simulator(&shell);
        let _ = ui.snapshot(&Theme::Dark).expect("drawn");
        ui.into_messages().collect()
    };
    for message in reported {
        let _ = shell.update(message);
    }
    let _ = shell.update(Message::Tick);
    let hint = format!("Fixed {width}x{height} - resize the window or change resolution to fill.");
    let mut ui = simulator(&shell);
    ui.find(hint.as_str()).expect("the C# hint");
}

#[test]
fn keys_sent_from_the_bar_are_said_as_the_csharp_toast_and_so_is_a_failure() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, tab, received) = connected(dir.path());
    let _ = shell.update(Message::App(AppMessage::SendKeys {
        tab,
        keys: heimdall_app::SpecialKeys::CtrlAltDel,
    }));
    {
        let mut ui = simulator(&shell);
        ui.find("Ctrl+Alt+Del sent to remote").expect("sent");
    }
    // The session gone, nothing reaches it: said so.
    drop(received);
    let _ = shell.update(Message::App(AppMessage::SendKeys {
        tab,
        keys: heimdall_app::SpecialKeys::WinL,
    }));
    let mut ui = simulator(&shell);
    ui.find("Could not send keys to the remote session.")
        .expect("not sent");
}
