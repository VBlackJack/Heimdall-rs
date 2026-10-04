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
        id: ProfileId::new("dc"),
        name: "Domain controller".to_owned(),
        group: Some("Windows".to_owned()),
        host: "dc.lab".to_owned(),
        port: 3389,
        username: Some("admin".to_owned()),
        domain: None,
        allow_tls_only: false,
        gateway: None,
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
    let shell = Shell::with_app(app(dir.path()));
    let mut ui = simulator(&shell);
    ui.find("RDP").expect("protocol");
    // As in the C# tree: a click selects, a double click connects.
    drop(ui);
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
    let (corner, renderer) = pixel_at(&shell, 300, 130);
    assert_eq!(corner, [0, 0, 0, 255], "drawn by {renderer}");
    let (middle, renderer) = pixel_at(&shell, 730, 420);
    assert_ne!(middle, [0, 0, 0, 255], "drawn by {renderer}");

    let _ = shell.update(Message::DesktopFit { tab, fit: true });
    // Fitted: never enlarged, centred; the corner is the window's background now.
    let (middle, renderer) = pixel_at(&shell, 730, 420);
    assert_eq!(middle, [0, 0, 0, 255], "drawn by {renderer}");
    let (corner, renderer) = pixel_at(&shell, 300, 130);
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
    let (corner, renderer) = pixel_at(&shell, 300, 130);
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
    let (corner, renderer) = pixel_at(&shell, 300, 130);
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
            host: "dc.lab".to_owned(),
            port: 3389,
            fingerprint: "SHA256:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"
                .parse()
                .expect("fingerprint"),
        },
    );
    let mut ui = simulator(&shell);
    ui.find("Unrecognised Server Certificate").expect("title");
    ui.find(
        "\"Domain controller\" answered at dc.lab:3389, presenting a certificate this profile has never approved.",
    )
    .expect("body");
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
