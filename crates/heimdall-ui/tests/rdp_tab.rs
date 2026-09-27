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

use std::path::Path;

use heimdall_app::{
    App, AppConfig, AttemptId, ConnectionEvent, DesktopInput, Effect, Message as AppMessage,
    PointerButton, TabId, UiError,
};
use heimdall_core::profile::{ProfileId, RdpProfile};
use heimdall_core::store::ProfileStore;
use heimdall_rdp::Framebuffer;
use heimdall_ssh::AgentSource;
use heimdall_term::GridSize;
use heimdall_ui::shell::{Message, Shell};
use heimdall_ui::terminal_view::FONTS;
use iced::{Point, Settings, Size, Theme, mouse};
use iced_test::simulator::Simulator;

const WINDOW: Size = Size::new(1200.0, 720.0);

/// Physical pixels per logical pixel in a simulator snapshot.
const SNAPSHOT_SCALE: u32 = 2;

fn app(dir: &Path) -> App {
    let profiles_file = dir.join("profiles.toml");
    let mut store = ProfileStore::open(&profiles_file).expect("store");
    store.merge_rdp([RdpProfile {
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

fn simulator(shell: &Shell) -> Simulator<'_, Message> {
    let settings = Settings {
        fonts: FONTS.iter().map(|face| (*face).into()).collect(),
        ..Settings::default()
    };
    Simulator::with_size(settings, WINDOW, shell.view())
}

/// A shell with the RDP tab open; the tab and its attempt.
fn opened(dir: &Path) -> (Shell, TabId, AttemptId) {
    let mut core = app(dir);
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
    ui.find("RDP admin@dc.lab:3389").expect("address line");
    ui.click("Domain controller").expect("profile");
    assert!(ui.into_messages().any(|message| matches!(
        message,
        Message::App(AppMessage::OpenRdp(id)) if id.as_str() == "dc"
    )));
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
