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
    App, AppConfig, AttemptId, ConnectionEvent, Effect, Message as AppMessage, TabId, UiError,
};
use heimdall_core::profile::{ProfileId, RdpProfile};
use heimdall_core::store::ProfileStore;
use heimdall_rdp::{Framebuffer, MouseButton, Operation};
use heimdall_ssh::AgentSource;
use heimdall_term::GridSize;
use heimdall_ui::shell::{Message, Shell};
use heimdall_ui::terminal_view::FONTS;
use iced::{Point, Settings, Size, mouse};
use iced_test::simulator::Simulator;

const WINDOW: Size = Size::new(1200.0, 720.0);

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
    }]);
    store.save().expect("save");
    App::new(AppConfig {
        profiles_file,
        known_hosts: dir.join("known_hosts"),
        legacy_dir: None,
        agent: AgentSource::Disabled,
        initial_grid: GridSize { cols: 80, rows: 24 },
        files_start: dir.to_owned(),
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
    let batches: Vec<Vec<Operation>> = ui
        .into_messages()
        .filter_map(|message| match message {
            Message::App(AppMessage::RdpInput { operations, .. }) => Some(operations),
            _ => None,
        })
        .collect();
    let pressed = batches
        .iter()
        .flatten()
        .any(|operation| matches!(operation, Operation::MouseButtonPressed(MouseButton::Left)));
    assert!(pressed, "{batches:?}");
    let moved = batches
        .iter()
        .flatten()
        .filter_map(|operation| match operation {
            Operation::MouseMove(at) => Some(*at),
            _ => None,
        })
        .next_back()
        .expect("a move");
    // Desktop coordinates, not window ones: the view starts right of the sidebar.
    assert!(moved.x < 700 && moved.y < 400, "{moved:?}");
}
