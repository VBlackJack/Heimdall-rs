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
//! once let go, a double click giving each half, the tab's menu and Ctrl+Shift+O; a tab
//! dropped on the content, the tree's "Open in split", "Split..." through Quick Connect,
//! and the keys that move between the panes; a tab let go out of the window detached, and
//! Escape giving its drag up.

mod common;

use std::path::Path;
use std::sync::Arc;

use heimdall_app::split::{Axis, DEFAULT_RATIO, Placement, SplitMessage};
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
use heimdall_ui::tab_drag;
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
const TALL_WINDOW: Size = Size::new(1100.0, 1400.0);

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
        ssh_mode: heimdall_core::profile::SshMode::Embedded,
        x11_forwarding: false,
    }
}

fn app(dir: &Path) -> App {
    let profiles_file = dir.join("profiles.toml");
    let mut store = ProfileStore::open(&profiles_file).expect("store");
    store.merge([profile("a"), profile("b"), profile("c")]);
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
    // The splits these tests make alone: no SFTP pane docked beside a shell connected.
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
        placement: Placement::Second,
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

/// What a click on the `nth` text reading `label`, counted from 0 in the order the view
/// holds them, asks of a menu: each pane's header says Disconnect before its menu does.
fn chosen_nth(shell: &Shell, label: &'static str, nth: usize) -> Vec<Message> {
    use iced_test::selector::{Candidate, Selector as _, Text};

    let mut seen = 0;
    let mut ui = sized(shell, TALL_WINDOW);
    ui.click(move |candidate: Candidate<'_>| -> Option<Text> {
        let mut said = label;
        let found = said.select(candidate)?;
        seen += 1;
        (seen > nth).then_some(found)
    })
    .expect(label);
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
    // The host pane's header, as the C#'s: its Disconnect button, that pane alone.
    {
        let mut ui = sized(&shell, TALL_WINDOW);
        ui.click("Disconnect").expect("the header's button");
        let close = Message::App(AppMessage::Split(SplitMessage::ClosePane(left)));
        let got: Vec<Message> = ui.into_messages().collect();
        assert!(got.iter().any(|message| same(message, &close)), "{got:?}");
    }
    // Its menu: the same, after both headers' buttons.
    let _ = shell.update(Message::OpenTreeMenu(TreeMenu::Pane(left)));
    let expected = Message::MenuChoice(AppMessage::Split(SplitMessage::ClosePane(left)));
    let got = chosen_nth(&shell, "Disconnect", 2);
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
    let got = chosen_nth(&shell, "Disconnect", 2);
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
            placement: Placement::Second,
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

/// Two live shells on the strip, "left pane" shown, "right pane" beside it.
fn two_tabs(dir: &Path) -> (Shell, TabId, TabId) {
    let mut core = app(dir);
    let left = live(&mut core, "a", "left pane");
    let right = live(&mut core, "b", "right pane");
    core.update(AppMessage::SelectTab(left));
    (Shell::with_app(core), left, right)
}

/// Where the content is drawn, under the tab bar.
fn content_area(shell: &Shell) -> Rectangle {
    let mut ui = simulator(shell);
    ui.find(tab_drag::content_area_id())
        .expect("the content")
        .bounds()
}

/// `tab` pressed on the strip and dragged to `to`, the content's area read as the drag
/// starts.
fn drag_to(shell: &mut Shell, tab: TabId, to: Point) {
    let area = content_area(shell);
    let _ = shell.update(Message::TabHover(tab));
    let _ = shell.update(Message::PointerPressed);
    let _ = shell.update(Message::TabHoverLeft(tab));
    let _ = shell.update(Message::TabDragMoved(to));
    let _ = shell.update(Message::TabDropArea(Some(area)));
}

/// Where "Drop to split" is drawn, if it is.
fn drop_label(shell: &Shell) -> Option<Rectangle> {
    let mut ui = simulator(shell);
    ui.find("Drop to split").ok().map(|found| found.bounds())
}

fn layout_of(shell: &Shell, host: TabId) -> Option<heimdall_app::split::Layout> {
    shell.app().tab(host).and_then(|tab| tab.layout.clone())
}

#[test]
fn a_tab_dragged_over_the_content_shows_its_half_and_splits_the_tab_shown() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, left, right) = two_tabs(dir.path());
    let area = content_area(&shell);
    // Near the left edge: side by side, the dragged tab first.
    drag_to(
        &mut shell,
        right,
        Point::new(area.x + area.width * 0.1, area.center_y()),
    );
    let label = drop_label(&shell).expect("the overlay");
    assert!(
        label.center_x() < area.center_x() && label.center_y() > area.y,
        "over the left half: {label:?} in {area:?}"
    );
    let _ = shell.update(Message::TabDragEnd);
    let split = layout_of(&shell, left).expect("split");
    assert_eq!(split.axis(), Some(Axis::SideBySide));
    assert_eq!(split.leaves(), [right, left], "dropped on the left: first");
    assert!(drop_label(&shell).is_none(), "gone once let go");

    // Near the bottom edge: stacked, the dragged tab second, as the C#.
    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, left, right) = two_tabs(dir.path());
    drag_to(
        &mut shell,
        right,
        Point::new(area.center_x(), area.y + area.height * 0.95),
    );
    let label = drop_label(&shell).expect("the overlay");
    assert!(
        label.center_y() > area.center_y(),
        "over the bottom half: {label:?} in {area:?}"
    );
    let _ = shell.update(Message::TabDragEnd);
    let split = layout_of(&shell, left).expect("split");
    assert_eq!(split.axis(), Some(Axis::Stacked));
    assert_eq!(split.leaves(), [left, right]);
}

#[test]
fn a_tab_dropped_on_the_strip_still_takes_the_place_of_another() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, left, right) = two_tabs(dir.path());
    let area = content_area(&shell);
    drag_to(&mut shell, right, Point::new(area.x + 10.0, area.y - 10.0));
    let _ = shell.update(Message::TabHover(left));
    assert!(drop_label(&shell).is_none(), "over the strip: no overlay");
    let _ = shell.update(Message::TabDragEnd);
    let strip: Vec<TabId> = shell.app().strip().iter().map(|tab| tab.id).collect();
    assert_eq!(strip, [right, left]);
    assert!(layout_of(&shell, left).is_none() && layout_of(&shell, right).is_none());
}

#[test]
fn no_overlay_for_the_tab_shown_a_split_tab_or_onto_a_full_split() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, left, _) = two_tabs(dir.path());
    let area = content_area(&shell);
    let inside = Point::new(area.x + area.width * 0.1, area.center_y());
    drag_to(&mut shell, left, inside);
    assert!(drop_label(&shell).is_none(), "the tab shown onto itself");
    let _ = shell.update(Message::TabDragEnd);
    assert!(layout_of(&shell, left).is_none());

    // Onto a split of two panes already: no overlay, and the drop says why.
    let dir = tempfile::tempdir().expect("dir");
    let (shell, left, _) = split_shell(dir.path());
    let mut core = shell.into_app();
    let third = live(&mut core, "c", "third");
    core.update(AppMessage::SelectTab(left));
    let mut shell = Shell::with_app(core);
    drag_to(&mut shell, third, inside);
    assert!(drop_label(&shell).is_none(), "full already");
    let _ = shell.update(Message::TabDragEnd);
    assert_eq!(layout_of(&shell, left).expect("split").leaves().len(), 2);
    assert_eq!(
        shell.app().notice(),
        Some(&heimdall_app::Notice::SplitMaxPanesReached(
            heimdall_app::split::MAX_PANES
        ))
    );

    // The split tab dragged over another: no overlay, nothing merged.
    let _ = shell.update(Message::App(AppMessage::SelectTab(third)));
    drag_to(&mut shell, left, inside);
    assert!(drop_label(&shell).is_none(), "a split tab is not merged");
    let _ = shell.update(Message::TabDragEnd);
    assert!(layout_of(&shell, third).is_none());
}

/// The main window's size known, as the window reports it once a drag starts.
fn sized_window(shell: &mut Shell) {
    let _ = shell.update(Message::WindowResized(WINDOW));
}

/// A place beyond the window's right edge, past the margin a drop there detaches beyond.
fn out_right() -> Point {
    Point::new(WINDOW.width + tab_drag::DETACH_MARGIN + 10.0, 300.0)
}

/// Whether the hint that a tab let go detaches is drawn.
fn detach_hint(shell: &Shell) -> bool {
    let mut ui = simulator(shell);
    ui.find("Release to detach to a window").is_ok()
}

fn strip_of(shell: &Shell) -> Vec<TabId> {
    shell.app().strip().iter().map(|tab| tab.id).collect()
}

#[test]
fn a_tab_let_go_out_of_the_window_goes_to_a_window_of_its_own() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, left, right) = two_tabs(dir.path());
    sized_window(&mut shell);
    let area = content_area(&shell);
    // Over the content, the hint is not drawn: a drop there splits.
    drag_to(&mut shell, right, area.center());
    assert!(drop_label(&shell).is_some());
    assert!(!detach_hint(&shell), "inside: no hint");
    // Out of the window: the overlay gone, the hint drawn.
    let _ = shell.update(Message::TabDragMoved(out_right()));
    assert!(
        drop_label(&shell).is_none(),
        "out of the window: no overlay"
    );
    assert!(detach_hint(&shell), "out of the window: the hint");
    let _ = shell.update(Message::TabDragEnd);
    assert!(shell.app().is_floating(right));
    assert_eq!(strip_of(&shell), [left]);
    assert!(layout_of(&shell, left).is_none(), "not split");
    assert!(!detach_hint(&shell), "gone once let go");

    // Above the window and beyond the margin, the tab shown goes too.
    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, left, right) = two_tabs(dir.path());
    sized_window(&mut shell);
    drag_to(
        &mut shell,
        left,
        Point::new(400.0, -tab_drag::DETACH_MARGIN - 1.0),
    );
    let _ = shell.update(Message::TabDragEnd);
    assert!(shell.app().is_floating(left));
    assert_eq!(strip_of(&shell), [right]);
}

#[test]
fn a_tab_let_go_on_the_window_frame_stays() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, left, right) = two_tabs(dir.path());
    sized_window(&mut shell);
    // Just above the drawn area, on the title bar: within the margin.
    drag_to(
        &mut shell,
        right,
        Point::new(400.0, -tab_drag::DETACH_MARGIN + 1.0),
    );
    assert!(!detach_hint(&shell), "within the margin: no hint");
    let _ = shell.update(Message::TabDragEnd);
    assert!(!shell.app().is_floating(right));
    assert_eq!(strip_of(&shell), [left, right]);
    assert!(layout_of(&shell, left).is_none());

    // The window's size unknown, nothing is out of it.
    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, _, right) = two_tabs(dir.path());
    drag_to(&mut shell, right, out_right());
    let _ = shell.update(Message::TabDragEnd);
    assert!(!shell.app().is_floating(right));
}

#[test]
fn a_split_tab_let_go_out_of_the_window_is_refused_and_stays() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, left, right) = split_shell(dir.path());
    sized_window(&mut shell);
    drag_to(&mut shell, left, out_right());
    assert!(!detach_hint(&shell), "a split tab cannot go: no hint");
    let _ = shell.update(Message::TabDragEnd);
    assert!(!shell.app().is_floating(left) && !shell.app().is_floating(right));
    assert_eq!(
        shell.app().notice(),
        Some(&heimdall_app::Notice::DetachSplitRefused)
    );
    assert_eq!(
        layout_of(&shell, left).expect("still split").leaves(),
        [left, right]
    );
    assert_eq!(strip_of(&shell), [left]);
}

#[test]
fn the_window_size_known_drops_inside_still_reorder_and_split() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, left, right) = two_tabs(dir.path());
    sized_window(&mut shell);
    let area = content_area(&shell);
    drag_to(&mut shell, right, Point::new(area.x + 10.0, area.y - 10.0));
    let _ = shell.update(Message::TabHover(left));
    let _ = shell.update(Message::TabDragEnd);
    assert_eq!(strip_of(&shell), [right, left], "the strip reorders");
    assert!(!shell.app().is_floating(right));

    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, left, right) = two_tabs(dir.path());
    sized_window(&mut shell);
    drag_to(
        &mut shell,
        right,
        Point::new(area.x + area.width * 0.9, area.center_y()),
    );
    let _ = shell.update(Message::TabDragEnd);
    assert_eq!(
        layout_of(&shell, left).expect("split").leaves(),
        [left, right],
        "the content splits"
    );
    assert!(!shell.app().is_floating(right));
}

#[test]
fn escape_gives_a_tab_drag_up_and_nothing_moves() {
    // Escape no widget took, then one a terminal took: either gives the drag up.
    for escape in [
        Message::EscapeUntaken,
        Message::DialogKey { confirm: false },
    ] {
        let dir = tempfile::tempdir().expect("dir");
        let (mut shell, left, right) = two_tabs(dir.path());
        sized_window(&mut shell);
        drag_to(&mut shell, right, out_right());
        assert!(detach_hint(&shell));
        let _ = shell.update(escape);
        assert!(!detach_hint(&shell), "given up: no hint");
        let _ = shell.update(Message::TabDragEnd);
        assert!(!shell.app().is_floating(right));
        assert_eq!(strip_of(&shell), [left, right]);

        // Over the content, given up, nothing splits.
        let area = content_area(&shell);
        drag_to(&mut shell, right, area.center());
        let _ = shell.update(Message::EscapeUntaken);
        assert!(drop_label(&shell).is_none(), "given up: no overlay");
        let _ = shell.update(Message::TabDragEnd);
        assert!(layout_of(&shell, left).is_none());
    }
}

#[test]
fn a_drag_whose_release_was_lost_ends_with_the_next_press() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, left, right) = two_tabs(dir.path());
    sized_window(&mut shell);
    drag_to(&mut shell, right, out_right());
    // The release went elsewhere: the next press, on no tab, starts no drag, and its own
    // release detaches nothing.
    let _ = shell.update(Message::PointerPressed);
    assert!(!detach_hint(&shell), "the press ends it");
    let _ = shell.update(Message::TabDragEnd);
    assert!(!shell.app().is_floating(right));
    assert_eq!(strip_of(&shell), [left, right]);
}

#[test]
fn the_tree_menu_opens_a_session_in_a_split_once_one_is_open() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = Shell::with_app(app(dir.path()));
    let profile = ProfileId::new("c");
    let _ = shell.update(Message::OpenTreeMenu(TreeMenu::Profile(profile.clone())));
    let got = chosen(&shell, "Open in split");
    assert!(got.is_empty(), "no session open: disabled, {got:?}");

    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, _, _) = two_tabs(dir.path());
    let _ = shell.update(Message::OpenTreeMenu(TreeMenu::Profile(profile.clone())));
    let got = chosen(&shell, "Open in split");
    assert!(
        matches!(got.as_slice(), [Message::OpenTreeMenu(TreeMenu::OpenInSplit(id))] if *id == profile),
        "{got:?}"
    );
    let _ = shell.update(Message::OpenTreeMenu(TreeMenu::OpenInSplit(
        profile.clone(),
    )));
    for (entry, axis) in [
        ("Horizontal", Axis::Stacked),
        ("Vertical", Axis::SideBySide),
    ] {
        let expected = Message::MenuChoice(AppMessage::Split(SplitMessage::OpenInSplit {
            profile: profile.clone(),
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
fn split_opens_quick_connect_to_merge_into_the_tab_and_escape_cancels_it() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, left, right) = two_tabs(dir.path());
    let _ = shell.update(Message::OpenTreeMenu(TreeMenu::Tab(left)));
    let got = chosen(&shell, "Split...");
    assert!(
        matches!(got.as_slice(), [Message::OpenTreeMenu(TreeMenu::SplitAxis(tab))] if *tab == left),
        "{got:?}"
    );
    let _ = shell.update(Message::OpenTreeMenu(TreeMenu::SplitAxis(left)));
    let mut ui = sized(&shell, TALL_WINDOW);
    ui.click("Vertical").expect("Vertical");
    let split = ui
        .into_messages()
        .find(|message| matches!(message, Message::SplitPalette { .. }))
        .expect("the palette asked for");
    assert!(
        matches!(split, Message::SplitPalette { host, axis: Axis::SideBySide } if host == left),
        "{split:?}"
    );

    // Escape: the split mode goes with the palette.
    let _ = shell.update(split.clone());
    let _ = shell.update(Message::DialogKey { confirm: false });
    let _ = shell.update(Message::PaletteChoose(0));
    assert_eq!(shell.app().tabs.len(), 2, "nothing opened");

    let _ = shell.update(split);
    let _ = shell.update(Message::PaletteQuery("server c".to_owned()));
    let _ = shell.update(Message::PaletteChoose(0));
    let leaves = layout_of(&shell, left).expect("split").leaves();
    assert_eq!(leaves.len(), 2);
    assert_eq!(leaves[0], left);
    let strip: Vec<TabId> = shell.app().strip().iter().map(|tab| tab.id).collect();
    assert_eq!(strip, [left, right], "merged, not on the strip");

    // A tab split offers no "Split...".
    let _ = shell.update(Message::OpenTreeMenu(TreeMenu::Tab(left)));
    let mut ui = sized(&shell, TALL_WINDOW);
    assert!(ui.find("Split...").is_err());
}

#[test]
fn the_pane_shortcuts_move_the_keyboard_and_terminals_leave_them_to_the_window() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, left, right) = split_shell(dir.path());
    for (key, physical, modifiers, expected) in [
        (
            keyboard::Key::Named(Named::ArrowRight),
            keyboard::key::Physical::Code(keyboard::key::Code::ArrowRight),
            keyboard::Modifiers::CTRL | keyboard::Modifiers::ALT,
            WindowShortcut::NextPane,
        ),
        (
            keyboard::Key::Named(Named::F6),
            keyboard::key::Physical::Code(keyboard::key::Code::F6),
            keyboard::Modifiers::CTRL | keyboard::Modifiers::SHIFT,
            WindowShortcut::PreviousPane,
        ),
    ] {
        assert_eq!(window_shortcut(&key, physical, modifiers), Some(expected));
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
        assert_eq!(status, [event::Status::Ignored], "{expected:?}");
        assert!(
            !ui.into_messages()
                .any(|message| matches!(message, Message::App(AppMessage::Key { tab, .. }) if tab == right)),
            "not the session's"
        );
    }
    let _ = shell.update(Message::Shortcut(WindowShortcut::NextPane));
    assert_eq!(shell.app().active, Some(left), "round to the first");
    let _ = shell.update(Message::Shortcut(WindowShortcut::NextPane));
    assert_eq!(shell.app().active, Some(right));
    let _ = shell.update(Message::Shortcut(WindowShortcut::PreviousPane));
    assert_eq!(shell.app().active, Some(left));
    let mut ui = simulator(&shell);
    ui.typewrite("z");
    let keys: Vec<TabId> = ui
        .into_messages()
        .filter_map(|message| match message {
            Message::App(AppMessage::Key { tab, .. }) => Some(tab),
            _ => None,
        })
        .collect();
    assert_eq!(keys, [left], "typing follows the keyboard");
}
