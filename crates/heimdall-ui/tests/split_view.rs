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

//! A split tab drawn headless, as the C# one: both panes under their headers, the keyboard
//! with the focused one only, a press giving it to the other, the divider dragged then kept
//! once let go, a double click giving each half, the tab's menu and Ctrl+Shift+O.

mod common;

use std::path::Path;
use std::sync::Arc;

use heimdall_app::split::{Axis, DEFAULT_RATIO, SplitMessage};
use heimdall_app::{
    App, AppConfig, AttemptId, ConnectionEvent, Dialog, Message as AppMessage, TabId,
    TabMenuMessage,
};
use heimdall_core::profile::{ProfileId, SshProfile};
use heimdall_core::store::ProfileStore;
use heimdall_ssh::{AgentSource, SessionClosed, TerminalSize};
use heimdall_term::GridSize;
use heimdall_ui::shell::{Message, Shell};
use heimdall_ui::split_view::{self, DIVIDER, NUDGE};
use heimdall_ui::terminal_view::FONTS;
use heimdall_ui::terminal_view::keys::{WindowShortcut, window_shortcut};
use heimdall_ui::tree_view::TreeMenu;
use iced::keyboard::{self, key::Named};
use iced::mouse::{self, Button};
use iced::{Point, Rectangle, Settings, Size, event};

const GRID: GridSize = GridSize { cols: 80, rows: 24 };

/// Size of the simulated window, in logical pixels.
const WINDOW: Size = Size::new(1100.0, 700.0);

/// A window tall enough for a tab's whole menu, the split entries at its foot.
const TALL_WINDOW: Size = Size::new(1100.0, 1200.0);

/// Tries of a double click, which iced tells by the real time between its presses.
const DOUBLE_CLICK_TRIES: usize = 3;

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

fn profile(id: &str) -> SshProfile {
    SshProfile {
        id: ProfileId::new(id),
        name: format!("server {id}"),
        group: None,
        host: format!("{id}.lab"),
        port: 22,
        username: Some("admin".to_owned()),
        key_path: None,
        gateway: None,
        vault_entry: None,
        forwards: heimdall_core::profile::Forwards::default(),
        post_connect: heimdall_core::post_connect::PostConnect::default(),
        forward_agent: false,
        compression: false,
        sftp: false,
        legacy_algorithms: false,
        session_logging: None,
    }
}

fn app(dir: &Path) -> App {
    let profiles_file = dir.join("profiles.toml");
    let mut store = ProfileStore::open(&profiles_file).expect("store");
    store.merge([profile("a"), profile("b"), profile("c")]);
    store.save().expect("save");
    App::new(AppConfig {
        profiles_file,
        known_hosts: dir.join("known_hosts"),
        legacy_dir: None,
        agent: AgentSource::Disabled,
        initial_grid: GRID,
        files_start: dir.to_owned(),
        system_credentials: heimdall_app::SystemCredentials::memory(),
    })
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

/// A live shell of profile `id`, under a name of its own: the tree shows the profile's.
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

/// Two live shells split side by side, "left pane" the host, "right pane" docked and
/// focused.
fn split_shell(dir: &Path) -> (Shell, TabId, TabId) {
    let mut core = app(dir);
    let left = live(&mut core, "a", "left pane");
    let right = live(&mut core, "b", "right pane");
    core.update(AppMessage::Split(SplitMessage::Merge {
        host: left,
        tab: right,
        axis: Axis::SideBySide,
    }));
    assert_eq!(core.active, Some(right));
    (Shell::with_app(core), left, right)
}

fn simulator(shell: &Shell) -> common::Drawn<'_> {
    sized(shell, WINDOW)
}

fn sized(shell: &Shell, size: Size) -> common::Drawn<'_> {
    let settings = Settings {
        fonts: FONTS.iter().map(|face| (*face).into()).collect(),
        ..Settings::default()
    };
    common::simulator(settings, size, shell.view())
}

/// Where the split is drawn.
fn split_area(ui: &mut common::Drawn<'_>) -> Rectangle {
    ui.find(split_view::area_id()).expect("the split").bounds()
}

/// The middle of the divider of a side-by-side split at `ratio`.
fn divider_at(area: Rectangle, ratio: f32) -> Point {
    Point::new(
        area.x + (area.width - DIVIDER) * ratio + DIVIDER / 2.0,
        area.center_y(),
    )
}

fn press() -> iced::Event {
    iced::Event::Mouse(mouse::Event::ButtonPressed(Button::Left))
}

fn release() -> iced::Event {
    iced::Event::Mouse(mouse::Event::ButtonReleased(Button::Left))
}

fn ratio(shell: &Shell, host: TabId) -> Option<f32> {
    shell
        .app()
        .tab(host)
        .and_then(|tab| tab.layout.as_ref())
        .and_then(heimdall_app::split::Layout::ratio)
}

/// The columns of `tab`'s terminal as a fresh drawing lays it out.
fn columns(shell: &Shell, tab: TabId) -> usize {
    let mut ui = simulator(shell);
    // Any event: a terminal reports its size as it first sees one.
    ui.simulate([iced::Event::Mouse(mouse::Event::CursorLeft)]);
    ui.into_messages()
        .find_map(|message| match message {
            Message::App(AppMessage::Resize { tab: t, grid, .. }) if t == tab => Some(grid.cols),
            _ => None,
        })
        .expect("its size reported")
}

#[test]
fn both_panes_are_drawn_and_only_the_focused_one_takes_typing() {
    let dir = tempfile::tempdir().expect("dir");
    let (shell, left, right) = split_shell(dir.path());
    let mut ui = simulator(&shell);
    ui.find("left pane").expect("the host's header");
    ui.find("right pane").expect("the docked pane's header");
    ui.typewrite("x");
    let messages: Vec<Message> = ui.into_messages().collect();
    let sized = |tab: TabId| {
        messages.iter().find_map(|message| match message {
            Message::App(AppMessage::Resize { tab: t, grid, .. }) if *t == tab => Some(grid.cols),
            _ => None,
        })
    };
    let (Some(left_cols), Some(right_cols)) = (sized(left), sized(right)) else {
        panic!("both terminals drawn: {messages:?}");
    };
    assert!(left_cols < 100 && right_cols < 100, "each half the tab");
    let typed = |tab: TabId| {
        messages
            .iter()
            .filter(|message| matches!(message, Message::App(AppMessage::Key { tab: t, .. }) if *t == tab))
            .count()
    };
    assert_eq!(typed(right), 1, "the focused pane");
    assert_eq!(typed(left), 0, "the other takes nothing");
}

#[test]
fn a_press_in_the_other_pane_gives_it_the_keyboard() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, left, right) = split_shell(dir.path());
    let focus = {
        let mut ui = simulator(&shell);
        let area = split_area(&mut ui);
        // In its terminal, below the header.
        ui.point_at(Point::new(area.x + area.width / 4.0, area.center_y()));
        ui.simulate([press(), release()]);
        let messages: Vec<Message> = ui.into_messages().collect();
        messages
            .into_iter()
            .find(|message| {
                matches!(
                    message,
                    Message::App(AppMessage::Split(SplitMessage::Focus(tab))) if *tab == left
                )
            })
            .expect("the press gives the left pane the keyboard")
    };
    let _ = shell.update(focus);
    assert_eq!(shell.app().active, Some(left));
    let mut ui = simulator(&shell);
    ui.typewrite("y");
    let keys: Vec<TabId> = ui
        .into_messages()
        .filter_map(|message| match message {
            Message::App(AppMessage::Key { tab, .. }) => Some(tab),
            _ => None,
        })
        .collect();
    assert_eq!(keys, [left], "typing follows: {right:?} takes none");
}

#[test]
fn the_divider_drags_live_and_is_kept_only_once_let_go() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, left, _) = split_shell(dir.path());
    let before = columns(&shell, left);
    let messages: Vec<Message> = {
        let mut ui = simulator(&shell);
        let area = split_area(&mut ui);
        ui.point_at(divider_at(area, DEFAULT_RATIO));
        ui.simulate([press()]);
        let to = Point::new(area.x + area.width * 0.3, area.center_y());
        ui.point_at(to);
        ui.simulate([iced::Event::Mouse(mouse::Event::CursorMoved {
            position: to,
        })]);
        ui.simulate([release()]);
        ui.into_messages().collect()
    };
    assert!(
        !messages.iter().any(|message| matches!(
            message,
            Message::App(AppMessage::Split(SplitMessage::Focus(_)))
        )),
        "a press on the divider is no pane's"
    );
    let dragged = messages
        .iter()
        .position(|message| matches!(message, Message::SplitDragged { host, divider: 0, .. } if *host == left))
        .expect("dragged");
    let released = messages
        .iter()
        .position(|message| matches!(message, Message::SplitReleased { host, divider: 0, .. } if *host == left))
        .expect("let go");
    assert!(dragged < released);
    let _ = shell.update(messages[dragged].clone());
    assert_eq!(
        ratio(&shell, left),
        Some(DEFAULT_RATIO),
        "not kept while held"
    );
    let during = columns(&shell, left);
    assert!(during < before, "drawn where held: {during} < {before}");
    let _ = shell.update(messages[released].clone());
    let kept = ratio(&shell, left).expect("split");
    assert!((kept - 0.3).abs() < 0.02, "kept once let go: {kept}");
    assert_eq!(columns(&shell, left), during, "as drawn while held");
}

#[test]
fn a_double_click_on_the_divider_gives_each_pane_half() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, left, _) = split_shell(dir.path());
    let _ = shell.update(Message::App(AppMessage::Split(SplitMessage::Resize {
        host: left,
        ratio: 0.3,
    })));
    let mut reset = None;
    for _ in 0..DOUBLE_CLICK_TRIES {
        let mut ui = simulator(&shell);
        let area = split_area(&mut ui);
        ui.point_at(divider_at(area, 0.3));
        ui.simulate([press(), release(), press(), release()]);
        reset = ui.into_messages().find(|message| {
            matches!(
                message,
                Message::App(AppMessage::Split(SplitMessage::ResetRatio(host))) if *host == left
            )
        });
        if reset.is_some() {
            break;
        }
    }
    let _ = shell.update(reset.expect("a double click resets"));
    assert_eq!(ratio(&shell, left), Some(DEFAULT_RATIO));
}

#[test]
fn a_divider_clicked_takes_the_arrow_keys() {
    let dir = tempfile::tempdir().expect("dir");
    let (shell, left, right) = split_shell(dir.path());
    let mut ui = simulator(&shell);
    let area = split_area(&mut ui);
    ui.point_at(divider_at(area, DEFAULT_RATIO));
    ui.simulate([press(), release()]);
    let status = ui.tap_key(keyboard::Key::Named(Named::ArrowRight));
    assert_eq!(status, event::Status::Captured);
    let messages: Vec<Message> = ui.into_messages().collect();
    let moved: Vec<f32> = messages
        .iter()
        .filter_map(|message| match message {
            Message::SplitReleased { host, ratio, .. } if *host == left => Some(*ratio),
            _ => None,
        })
        .collect();
    assert_eq!(
        moved.len(),
        1,
        "a click alone keeps nothing; the arrow does"
    );
    assert!((moved[0] - (DEFAULT_RATIO + NUDGE)).abs() < 0.001);
    assert!(
        !messages.iter().any(
            |message| matches!(message, Message::App(AppMessage::Key { tab, .. }) if *tab == right)
        ),
        "the arrow is the divider's, not the terminal's"
    );
}

/// The message the entry `label` of the open menu sends, if any.
fn chosen(shell: &Shell, label: &str) -> Vec<Message> {
    let mut ui = sized(shell, TALL_WINDOW);
    ui.click(label).expect(label);
    ui.into_messages()
        .filter(|message| matches!(message, Message::MenuChoice(_) | Message::OpenTreeMenu(_)))
        .collect()
}

fn same(message: &Message, expected: &Message) -> bool {
    format!("{message:?}") == format!("{expected:?}")
}

#[test]
fn the_tab_menu_offers_the_split_entries_as_the_csharp_one() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, left, right) = split_shell(dir.path());
    let split = |message| Message::MenuChoice(AppMessage::Split(message));
    // The docked pane's header opens its menu on a right click.
    {
        let mut ui = simulator(&shell);
        let header = ui.find("right pane").expect("header");
        ui.point_at(header.bounds().center());
        ui.simulate([
            iced::Event::Mouse(mouse::Event::ButtonPressed(Button::Right)),
            iced::Event::Mouse(mouse::Event::ButtonReleased(Button::Right)),
        ]);
        assert!(ui.into_messages().any(|message| matches!(
            message,
            Message::OpenTreeMenu(TreeMenu::Pane(opened)) if opened == right
        )));
    }
    for tab in [left, right] {
        let _ = shell.update(Message::OpenTreeMenu(TreeMenu::Pane(tab)));
        for (entry, expected) in [
            ("Unsplit", split(SplitMessage::Unsplit(left))),
            ("Swap Panes", split(SplitMessage::Swap(left))),
            (
                "Toggle Split Orientation",
                split(SplitMessage::ToggleAxis(left)),
            ),
            (
                "Close Secondary Pane",
                split(SplitMessage::CloseSecondary(left)),
            ),
        ] {
            let got = chosen(&shell, entry);
            assert!(
                matches!(got.as_slice(), [message] if same(message, &expected)),
                "{entry}: {got:?}"
            );
        }
        let mut ui = sized(&shell, TALL_WINDOW);
        assert!(ui.find("Merge with...").is_err(), "split: nothing to merge");
    }
    let _ = shell.update(Message::OpenTreeMenu(TreeMenu::Pane(right)));
    let mut ui = sized(&shell, TALL_WINDOW);
    assert!(ui.find("Pin tab").is_err(), "a docked pane is never pinned");
    ui.find("Unsplit").expect("its split's entries");
}

#[test]
fn disconnect_from_a_pane_header_closes_that_pane_and_from_the_strip_the_whole_tab() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, left, right) = split_shell(dir.path());
    // The host pane's header: that pane alone, asked as its close button asks.
    let _ = shell.update(Message::OpenTreeMenu(TreeMenu::Pane(left)));
    let expected = Message::MenuChoice(AppMessage::Split(SplitMessage::ClosePane(left)));
    let got = chosen(&shell, "Disconnect");
    assert!(
        matches!(got.as_slice(), [message] if same(message, &expected)),
        "{got:?}"
    );
    let _ = shell.update(expected);
    assert_eq!(
        shell.app().dialog,
        Some(Dialog::ConfirmCloseTab(left)),
        "live: asked"
    );
    let _ = shell.update(Message::App(AppMessage::ConfirmDialog));
    assert!(shell.app().tab(left).is_none());
    let strip: Vec<TabId> = shell.app().strip().iter().map(|tab| tab.id).collect();
    assert_eq!(strip, [right], "the docked pane left open, a plain tab");
    assert!(shell.app().tab(right).expect("right").layout.is_none());
    assert_eq!(shell.app().active, Some(right));

    // The strip's own menu: the whole tab, every pane, as the C# `CloseAllPanes`.
    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, left, right) = split_shell(dir.path());
    let _ = shell.update(Message::OpenTreeMenu(TreeMenu::Tab(left)));
    let expected = Message::MenuChoice(AppMessage::RequestCloseTab(left));
    let got = chosen(&shell, "Disconnect");
    assert!(
        matches!(got.as_slice(), [message] if same(message, &expected)),
        "{got:?}"
    );
    let _ = shell.update(expected);
    assert_eq!(shell.app().dialog, Some(Dialog::ConfirmCloseTab(left)));
    let _ = shell.update(Message::App(AppMessage::ConfirmDialog));
    assert!(shell.app().tab(left).is_none() && shell.app().tab(right).is_none());
    assert!(shell.app().tabs.is_empty());
}

#[test]
fn merge_with_lists_the_other_tabs_and_how_to_place_them() {
    const LONG: &str = "a tab whose name is longer than the strip shows";
    let dir = tempfile::tempdir().expect("dir");
    let (shell, _, _) = split_shell(dir.path());
    let mut core = shell.into_app();
    let host = live(&mut core, "c", "third");
    let other = live(&mut core, "a", LONG);
    let mut shell = Shell::with_app(core);
    let _ = shell.update(Message::OpenTreeMenu(TreeMenu::Tab(host)));
    let got = chosen(&shell, "Merge with...");
    assert!(
        matches!(got.as_slice(), [Message::OpenTreeMenu(TreeMenu::MergeWith(menu))] if *menu == host),
        "{got:?}"
    );
    let _ = shell.update(Message::OpenTreeMenu(TreeMenu::MergeWith(host)));
    let got = chosen(&shell, LONG);
    assert!(
        matches!(
            got.as_slice(),
            [Message::OpenTreeMenu(TreeMenu::MergeAxis { host: h, tab })] if *h == host && *tab == other
        ),
        "the split tab is not offered, the other is: {got:?}"
    );
    let _ = shell.update(Message::OpenTreeMenu(TreeMenu::MergeAxis {
        host,
        tab: other,
    }));
    for (entry, axis) in [
        ("Horizontal", Axis::Stacked),
        ("Vertical", Axis::SideBySide),
    ] {
        let expected = Message::MenuChoice(AppMessage::Split(SplitMessage::Merge {
            host,
            tab: other,
            axis,
        }));
        let got = chosen(&shell, entry);
        assert!(
            matches!(got.as_slice(), [message] if same(message, &expected)),
            "{entry}: {got:?}"
        );
    }
}

#[test]
fn ctrl_shift_o_turns_the_split_shown() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, left, right) = split_shell(dir.path());
    let modifiers = keyboard::Modifiers::CTRL | keyboard::Modifiers::SHIFT;
    let physical = keyboard::key::Physical::Code(keyboard::key::Code::KeyO);
    let key = keyboard::Key::Character("O".into());
    assert_eq!(
        window_shortcut(&key, physical, modifiers),
        Some(WindowShortcut::ToggleSplit)
    );
    {
        // The terminal with the keyboard leaves it to the window.
        let mut ui = simulator(&shell);
        let status = ui.simulate([iced::Event::Keyboard(keyboard::Event::KeyPressed {
            key: key.clone(),
            modified_key: key,
            physical_key: physical,
            location: keyboard::Location::Standard,
            modifiers,
            text: None,
            repeat: false,
        })]);
        assert_eq!(status, [event::Status::Ignored]);
        assert!(
            !ui.into_messages()
                .any(|message| matches!(message, Message::App(AppMessage::Key { tab, .. }) if tab == right)),
            "not the session's"
        );
    }
    let axis = |shell: &Shell| {
        shell
            .app()
            .tab(left)
            .and_then(|tab| tab.layout.as_ref())
            .and_then(heimdall_app::split::Layout::axis)
    };
    let _ = shell.update(Message::Shortcut(WindowShortcut::ToggleSplit));
    assert_eq!(axis(&shell), Some(Axis::Stacked));
    let _ = shell.update(Message::Shortcut(WindowShortcut::ToggleSplit));
    assert_eq!(axis(&shell), Some(Axis::SideBySide));
}

#[test]
fn the_tab_shortcuts_and_the_settings_follow_the_strip_not_the_panes() {
    let dir = tempfile::tempdir().expect("dir");
    let (shell, left, right) = split_shell(dir.path());
    let mut core = shell.into_app();
    let third = live(&mut core, "c", "third");
    let mut shell = Shell::with_app(core);
    // Next and Previous go over the strip's two tabs, the split one shown on its pane.
    let _ = shell.update(Message::Shortcut(WindowShortcut::NextTab));
    assert_eq!(shell.app().active, Some(right), "the split's last focus");
    let _ = shell.update(Message::Shortcut(WindowShortcut::NextTab));
    assert_eq!(shell.app().active, Some(third));
    let _ = shell.update(Message::Shortcut(WindowShortcut::PreviousTab));
    assert_eq!(shell.app().active, Some(right));

    // The settings stay over the split while the keyboard moves between its panes.
    let _ = shell.update(Message::ShowSettings);
    let _ = shell.update(Message::App(AppMessage::Split(SplitMessage::Focus(left))));
    assert!(shell.settings_shown(), "the same tab shown");

    // Ctrl+W closes the whole tab, asked once for its live panes.
    let _ = shell.update(Message::Shortcut(WindowShortcut::CloseTab));
    assert_eq!(shell.app().dialog, Some(Dialog::ConfirmCloseTab(left)));
    let _ = shell.update(Message::App(AppMessage::ConfirmDialog));
    assert!(shell.app().tab(left).is_none() && shell.app().tab(right).is_none());
    assert_eq!(shell.app().active, Some(third));
}
