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

//! Long texts kept on one line, as the C# trims them: a name of the import preview inside
//! its column, a name of the tree on one row, a pane's header keeping its buttons, the
//! tabs on one row that scrolls with the tab shown in view; and a tooltip put away by a
//! press, as a WPF tooltip by a click.

mod common;

use std::path::Path;
use std::sync::Arc;

use heimdall_app::split::{Axis, Placement, SplitMessage};
use heimdall_app::{
    App, AppConfig, AttemptId, ConnectionEvent, ImportFile, Message as AppMessage, SessionsMessage,
    TabId, TabMenuMessage,
};
use heimdall_core::profile::{ProfileId, SshProfile};
use heimdall_core::store::ProfileStore;
use heimdall_ssh::{AgentSource, SessionClosed, TerminalSize};
use heimdall_term::GridSize;
use heimdall_ui::press_tooltip::hide_on_press;
use heimdall_ui::shell::{Message, Shell, tab_id, tab_strip_id};
use heimdall_ui::split_view;
use heimdall_ui::terminal_view::FONTS;
use iced::mouse::{self, Button, ScrollDelta};
use iced::widget::{container, text, tooltip};
use iced::{Point, Settings, Size};

const GRID: GridSize = GridSize { cols: 80, rows: 24 };

/// The window of the test pass that found these: 1296 by 839.
const WINDOW: Size = Size::new(1296.0, 839.0);

/// A name far longer than any column, tree or header has room for.
const LONG_NAME: &str =
    "TestEnv Linux A (SSH, passphrase key via the gateway of the west region data centre)";

/// Narrower than "Connected" is drawn whole at the caption's size: an ellipsis alone.
const STATE_LEAST_WIDTH: f32 = 30.0;

/// Tabs opened at once: more than the strip shows side by side at [`WINDOW`]'s width.
const MANY_TABS: usize = 16;

#[derive(Debug, Default)]
struct NullSink;

impl heimdall_app::InputSink for NullSink {
    fn write(&self, _bytes: Vec<u8>) -> Result<(), SessionClosed> {
        Ok(())
    }

    fn resize(&self, _size: TerminalSize) -> Result<(), SessionClosed> {
        Ok(())
    }

    fn close(&self) {}
}

fn profile(id: &str, name: &str) -> SshProfile {
    SshProfile {
        id: ProfileId::new(id),
        name: name.to_owned(),
        group: None,
        host: format!("{id}.lab"),
        port: 22,
        username: Some("admin".to_owned()),
        key_path: None,
        gateway: None,
        local_tunnel_port: None,
        vault_entry: None,
        forwards: heimdall_core::profile::Forwards::default(),
        post_connect: heimdall_core::post_connect::PostConnect::default(),
        forward_agent: false,
        compression: false,
        sftp: false,
        legacy_algorithms: false,
        session_logging: None,
        ssh_mode: heimdall_core::profile::SshMode::Embedded,
        x11_forwarding: false,
    }
}

/// An application whose store holds `profiles`, no SFTP pane docked beside a shell.
fn app(dir: &Path, profiles: Vec<SshProfile>) -> App {
    let profiles_file = dir.join("profiles.toml");
    let mut store = ProfileStore::open(&profiles_file).expect("store");
    store.merge(profiles);
    store.save().expect("save");
    let mut app = App::new(AppConfig {
        profiles_file,
        known_hosts: dir.join("known_hosts"),
        legacy_dir: None,
        agent: AgentSource::Disabled,
        initial_grid: GRID,
        files_start: dir.to_owned(),
        system_credentials: heimdall_app::SystemCredentials::memory(),
    });
    app.update(AppMessage::Settings(
        heimdall_app::SettingsMessage::SftpBrowser(heimdall_core::settings::SftpBrowser {
            auto_open_on_ssh: false,
            ..heimdall_core::settings::SftpBrowser::default()
        }),
    ));
    app
}

fn open(app: &mut App, id: &str) -> (TabId, AttemptId) {
    match app
        .update(AppMessage::OpenProfile(ProfileId::new(id)))
        .as_slice()
    {
        [heimdall_app::Effect::Connect { tab, attempt, .. }] => (*tab, *attempt),
        other => panic!("expected one Connect, got {other:?}"),
    }
}

/// A live shell of profile `id`, its tab named `name`.
fn live(app: &mut App, id: &str, name: &str) -> TabId {
    let (tab, attempt) = open(app, id);
    app.update(AppMessage::Connection {
        tab,
        attempt,
        event: ConnectionEvent::Connected {
            input: Arc::new(NullSink),
        },
    });
    app.update(AppMessage::TabMenu(TabMenuMessage::Rename(tab)));
    app.update(AppMessage::TabMenu(TabMenuMessage::NameEdited(
        name.to_owned(),
    )));
    app.update(AppMessage::ConfirmDialog);
    tab
}

fn settings() -> Settings {
    Settings {
        fonts: FONTS.iter().map(|face| (*face).into()).collect(),
        ..Settings::default()
    }
}

fn simulator(shell: &Shell) -> common::Drawn<'_> {
    common::simulator(settings(), WINDOW, shell.view())
}

/// Set, the folder a picture of each window tested is written to, for a visual pass.
const SNAPSHOT_VARIABLE: &str = "HEIMDALL_SNAPSHOT_DIR";

/// A picture of the window, written as `name` to the folder [`SNAPSHOT_VARIABLE`] names.
fn snapshot(shell: &Shell, name: &str) {
    let Some(dir) = std::env::var_os(SNAPSHOT_VARIABLE) else {
        return;
    };
    // iced names the picture after its renderer: clear every variant, or an old picture is
    // only compared against and never replaced.
    let stem = name.trim_end_matches(".png");
    if let Ok(entries) = std::fs::read_dir(Path::new(&dir)) {
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
        .matches_image(Path::new(&dir).join(name))
        .expect("written");
}

#[test]
fn a_long_name_of_the_import_preview_stays_on_one_line_inside_its_column() {
    let dir = tempfile::tempdir().expect("dir");
    let mut core = app(dir.path(), Vec::new());
    core.update(AppMessage::Sessions(SessionsMessage::FileRead(Ok(
        ImportFile {
            name: "servers.json".to_owned(),
            text: format!(
                r#"[{{"id":"l1","displayName":"{LONG_NAME}","remoteServer":"long.lab","connectionType":"SSH"}},{{"id":"s1","displayName":"Short","remoteServer":"short.lab","connectionType":"SSH"}}]"#
            ),
            settings: None,
            rename: "{name} (Imported {n})".to_owned(),
        },
    ))));
    let shell = Shell::with_app(core);
    snapshot(&shell, "import-preview.png");
    let mut ui = simulator(&shell);
    let long = ui.find(LONG_NAME).expect("the long name").bounds();
    let short = ui.find("Short").expect("the short name").bounds();
    let host = ui.find("long.lab:22").expect("its host").bounds();
    assert!(
        long.x + long.width <= host.x,
        "the name ends before the host column: {long:?} {host:?}"
    );
    assert!(
        (long.height - short.height).abs() < 0.5,
        "one line, as a short name: {long:?} {short:?}"
    );
}

#[test]
fn a_long_name_of_the_tree_stays_on_one_line() {
    let dir = tempfile::tempdir().expect("dir");
    let shell = Shell::with_app(app(
        dir.path(),
        vec![profile("long", LONG_NAME), profile("short", "web")],
    ));
    snapshot(&shell, "tree.png");
    let mut ui = simulator(&shell);
    let long = ui.find(LONG_NAME).expect("the long row").bounds();
    let short = ui.find("web").expect("the short row").bounds();
    assert!(
        (long.height - short.height).abs() < 0.5,
        "one line, as a short name: {long:?} {short:?}"
    );
    assert!(
        long.x + long.width < WINDOW.width / 2.0,
        "cut in the sidebar, not laid out whole: {long:?}"
    );
}

#[test]
fn a_pane_header_keeps_its_buttons_beside_a_long_name() {
    let dir = tempfile::tempdir().expect("dir");
    let mut core = app(dir.path(), vec![profile("a", "a"), profile("b", "b")]);
    let left = live(&mut core, "a", LONG_NAME);
    let right = live(&mut core, "b", &format!("{LONG_NAME} 2"));
    core.update(AppMessage::Split(SplitMessage::Merge {
        host: left,
        tab: right,
        axis: Axis::SideBySide,
        placement: Placement::Second,
    }));
    let shell = Shell::with_app(core);
    snapshot(&shell, "pane-header.png");
    let mut ui = simulator(&shell);
    let area = ui.find(split_view::area_id()).expect("the split").bounds();
    // The left pane's, met first.
    let disconnect = ui.find("Disconnect").expect("its Disconnect");
    let bounds = disconnect.bounds();
    let middle = area.x + area.width / 2.0;
    assert!(
        bounds.x + bounds.width <= middle,
        "inside its own pane: {bounds:?}, the panes meet at {middle}"
    );
    assert!(bounds.x > area.x, "drawn: {bounds:?}");
    // The state keeps its word, the long name ending with an ellipsis before it.
    let state = ui.find("Connected").expect("its state").bounds();
    assert!(
        state.width > STATE_LEAST_WIDTH && state.x + state.width <= bounds.x,
        "the state whole, before the buttons: {state:?} {bounds:?}"
    );
}

#[test]
fn many_tabs_stay_on_one_row_that_scrolls_with_the_tab_shown_in_view() {
    let dir = tempfile::tempdir().expect("dir");
    let ids: Vec<String> = (0..MANY_TABS).map(|index| format!("s{index:02}")).collect();
    let mut core = app(
        dir.path(),
        ids.iter()
            .map(|id| profile(id, &format!("server {id} of the lab")))
            .collect(),
    );
    let tabs: Vec<TabId> = ids.iter().map(|id| open(&mut core, id).0).collect();
    let shell = Shell::with_app(core);
    snapshot(&shell, "tab-strip.png");
    let mut ui = simulator(&shell);
    let first = ui.find(tab_id(tabs[0])).expect("the first tab");
    let last = ui.find(tab_id(tabs[MANY_TABS - 1])).expect("the last tab");
    assert!(
        (first.bounds().y - last.bounds().y).abs() < 0.5,
        "one row: {:?} {:?}",
        first.bounds(),
        last.bounds()
    );
    let shown = last.visible_bounds().expect("the tab shown, in view");
    assert!(
        (shown.width - last.bounds().width).abs() < 0.5,
        "seen whole: {shown:?} of {:?}",
        last.bounds()
    );
    assert!(
        first.visible_bounds().is_none(),
        "the first scrolled out of view"
    );
    // The wheel turned up over the strip brings the first back.
    let strip = ui.find(tab_strip_id()).expect("the strip").bounds();
    ui.point_at(strip.center());
    ui.simulate([iced::Event::Mouse(mouse::Event::WheelScrolled {
        delta: ScrollDelta::Lines { x: 0.0, y: 100.0 },
    })]);
    let first = ui.find(tab_id(tabs[0])).expect("the first tab");
    assert!(
        first.visible_bounds().is_some(),
        "the wheel scrolls the strip sideways"
    );
}

#[test]
fn a_tooltip_is_put_away_by_a_press_and_comes_back_once_the_pointer_returns() {
    let dir = tempfile::tempdir().expect("dir");
    let size = Size::new(300.0, 200.0);
    let view: iced::Element<'_, Message> = container(hide_on_press(tooltip(
        container(text("row")).width(200.0).height(30.0),
        text("the row's tooltip"),
        tooltip::Position::Bottom,
    )))
    .width(iced::Length::Fill)
    .height(iced::Length::Fill)
    .into();
    let mut ui = common::simulator(settings(), size, view);
    let moved = |ui: &mut common::Drawn<'_>, at: Point| {
        ui.point_at(at);
        ui.simulate([iced::Event::Mouse(mouse::Event::CursorMoved {
            position: at,
        })]);
    };
    let theme = iced::Theme::Dark;
    let frame = |ui: &mut common::Drawn<'_>, name: &str| {
        ui.snapshot(&theme)
            .expect("drawn")
            .matches_image(dir.path().join(name))
            .expect("compared")
    };
    let away = Point::new(250.0, 180.0);
    let over = Point::new(50.0, 15.0);
    moved(&mut ui, away);
    assert!(frame(&mut ui, "plain.png"), "the plain window, written");
    moved(&mut ui, over);
    assert!(
        !frame(&mut ui, "plain.png"),
        "the tooltip shown under the pointer"
    );
    ui.simulate([
        iced::Event::Mouse(mouse::Event::ButtonPressed(Button::Left)),
        iced::Event::Mouse(mouse::Event::ButtonReleased(Button::Left)),
    ]);
    assert!(frame(&mut ui, "plain.png"), "put away by the press");
    moved(&mut ui, Point::new(60.0, 20.0));
    assert!(
        frame(&mut ui, "plain.png"),
        "still away while the pointer stays"
    );
    moved(&mut ui, away);
    moved(&mut ui, over);
    assert!(
        !frame(&mut ui, "plain.png"),
        "back once the pointer left and came back"
    );
}
