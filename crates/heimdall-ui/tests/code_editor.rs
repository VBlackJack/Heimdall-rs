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

//! The integrated editor's text widget: its numbered lines, its long lines scrolled across
//! instead of wrapped, and its gutter selecting lines as the C# editor's.

mod common;

use heimdall_ui::code_editor::{Content, code_editor};
use heimdall_ui::editor_history::History;
use heimdall_ui::terminal_view::FONTS;
use iced::keyboard::key::Named;
use iced::keyboard::{self, Key, Modifiers};
use iced::mouse::{self, Button, ScrollDelta};
use iced::widget::text_editor::{Action, Cursor, Edit, Position};
use iced::{Event, Font, Point, Settings, Size, Theme};
use iced_test::simulator::{Simulator, click};

/// The editor's window: narrower than a long line.
const WINDOW: Size = Size::new(400.0, 300.0);

/// Characters of a line far wider than the window.
const LONG_LINE: usize = 300;

/// A point in the first line of the text, right of the gutter.
const FIRST_LINE: Point = Point::new(200.0, 12.0);

/// The editor of `content` in the window, laid out: its scroll across follows the cursor.
fn drawn(content: &Content) -> Simulator<'_, Action> {
    let settings = Settings {
        fonts: FONTS.iter().map(|face| (*face).into()).collect(),
        ..Settings::default()
    };
    Simulator::with_size(
        settings,
        WINDOW,
        code_editor(content)
            .on_action(|action| action)
            .font(Font::MONOSPACE),
    )
}

/// Does `gesture` on the editor of `content`, performs what it asked, as the integrated
/// editor does, and lays the editor out again.
fn act(content: &mut Content, gesture: impl FnOnce(&mut Simulator<'_, Action>)) {
    let actions: Vec<Action> = {
        let mut ui = drawn(content);
        gesture(&mut ui);
        ui.into_messages().collect()
    };
    for action in actions {
        content.perform(action);
    }
    drop(drawn(content));
}

/// The folder a picture of the editor is written to, when set.
const SNAPSHOT_VARIABLE: &str = "HEIMDALL_SNAPSHOT_DIR";

/// Writes a picture of the editor of `content` as `name`, when [`SNAPSHOT_VARIABLE`] names
/// a folder.
fn snapshot(content: &Content, name: &str) {
    let Some(dir) = std::env::var_os(SNAPSHOT_VARIABLE) else {
        return;
    };
    let dir = std::path::Path::new(&dir);
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
    drawn(content)
        .snapshot(&Theme::Dracula)
        .expect("drawn")
        .matches_image(dir.join(name))
        .expect("written");
}

/// `count` short lines.
fn lines(count: usize) -> String {
    (1..=count)
        .map(|number| format!("line {number}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Width of the gutter of an editor of `count` lines.
fn gutter_width(count: usize) -> f32 {
    let content = Content::with_text(&lines(count));
    let mut ui = drawn(&content);
    ui.find("1").expect("line 1").bounds().width
}

#[test]
fn the_gutter_numbers_the_lines_and_widens_at_ten_and_a_hundred() {
    let _turn = common::render_turn();
    let content = Content::with_text(&lines(9));
    let mut ui = drawn(&content);
    let mut above = f32::MIN;
    for number in 1..=9 {
        let row = ui.find(number.to_string()).expect("numbered").bounds();
        assert!(row.y > above, "line {number} under the one before");
        above = row.y;
    }
    assert!(ui.find("10").is_err(), "nine lines only");
    drop(ui);

    let nine = gutter_width(9);
    let ten = gutter_width(10);
    let ninety_nine = gutter_width(99);
    let hundred = gutter_width(100);
    assert!(ten > nine, "{ten} > {nine}");
    assert!((ninety_nine - ten).abs() < 0.01, "two digits either way");
    assert!(hundred > ninety_nine, "{hundred} > {ninety_nine}");
}

#[test]
fn a_long_line_is_scrolled_across_not_wrapped() {
    let _turn = common::render_turn();
    let long = "x".repeat(LONG_LINE);
    let mut content = Content::with_text(&format!("{long}\nnext"));
    {
        let mut ui = drawn(&content);
        let one = ui.find("1").expect("line 1").bounds();
        let two = ui.find("2").expect("line 2").bounds();
        assert!(
            (two.y - (one.y + one.height)).abs() < 0.5,
            "the long line on one row"
        );
    }
    assert!(content.horizontal_scroll().abs() < f32::EPSILON);

    // Typed at its end: the text scrolls so that the caret stays in view.
    act(&mut content, |ui| {
        ui.point_at(FIRST_LINE);
        let _ = ui.simulate(click());
        let _ = ui.tap_key(Key::Named(Named::End));
        let _ = ui.typewrite("yz");
    });
    assert!(
        content
            .line(0)
            .is_some_and(|line| line.text.ends_with("xyz"))
    );
    let scrolled = content.horizontal_scroll();
    assert!(scrolled > 0.0, "scrolled across");
    assert!(content.caret_in_view());
    snapshot(&content, "code-editor-scrolled.png");

    // Shift and the wheel scroll across, as a trackpad's sideways movement does.
    {
        let mut ui = drawn(&content);
        ui.point_at(FIRST_LINE);
        let _ = ui.simulate([
            Event::Keyboard(keyboard::Event::ModifiersChanged(Modifiers::SHIFT)),
            Event::Mouse(mouse::Event::WheelScrolled {
                delta: ScrollDelta::Lines { x: 0.0, y: 1.0 },
            }),
        ]);
    }
    let left = content.horizontal_scroll();
    if !cfg!(target_os = "macos") {
        assert!(left < scrolled, "Shift and the wheel up: back left");
    }
    {
        let mut ui = drawn(&content);
        ui.point_at(FIRST_LINE);
        let _ = ui.simulate([
            Event::Keyboard(keyboard::Event::ModifiersChanged(Modifiers::empty())),
            Event::Mouse(mouse::Event::WheelScrolled {
                delta: ScrollDelta::Pixels { x: 10.0, y: 0.0 },
            }),
        ]);
    }
    assert!(content.horizontal_scroll() < left, "a trackpad's movement");

    // A click in the scrolled text puts the caret under the pointer, and keeps the scroll.
    let before = content.horizontal_scroll();
    act(&mut content, |ui| {
        ui.point_at(FIRST_LINE);
        let _ = ui.simulate(click());
    });
    assert!(
        content.cursor().position.column > LONG_LINE / 2,
        "in the scrolled part"
    );
    assert!((content.horizontal_scroll() - before).abs() < 1.0);

    // Home: back to the start of the line.
    act(&mut content, |ui| {
        ui.point_at(FIRST_LINE);
        let _ = ui.simulate(click());
        let _ = ui.tap_key(Key::Named(Named::Home));
    });
    assert_eq!(content.cursor().position, Position { line: 0, column: 0 });
    assert!(content.horizontal_scroll().abs() < f32::EPSILON);
}

#[test]
fn the_scrollbar_scrolls_a_long_line() {
    let _turn = common::render_turn();
    let mut content = Content::with_text(&"x".repeat(LONG_LINE));
    act(&mut content, |ui| {
        // The right end of the scrollbar, under the text.
        ui.point_at(Point::new(WINDOW.width - 8.0, WINDOW.height - 10.0));
        let _ = ui.simulate(click());
    });
    assert!(content.horizontal_scroll() > 0.0, "scrolled to the end");
    assert_eq!(
        content.cursor().position,
        Position { line: 0, column: 0 },
        "the caret stays"
    );
}

#[test]
fn a_click_in_the_gutter_selects_its_line_as_the_csharp_editor() {
    let _turn = common::render_turn();
    let mut content = Content::with_text("alpha\nbeta\ngamma\n");
    act(&mut content, |ui| {
        let _ = ui.click("2").expect("line 2's number");
    });
    assert_eq!(content.selection().as_deref(), Some("beta\n"));
    snapshot(&content, "code-editor-line-selected.png");
    assert_eq!(
        content.cursor(),
        Cursor {
            position: Position { line: 2, column: 0 },
            selection: Some(Position { line: 1, column: 0 }),
        },
        "the caret at the start of the next line"
    );

    // A plain selection: the undo layer replaces it and puts it back.
    let mut history = History::default();
    history.edit(&mut content, Edit::Insert('X'));
    assert_eq!(content.text(), "alpha\nXgamma\n");
    assert!(history.undo(&mut content));
    assert_eq!(content.text(), "alpha\nbeta\ngamma\n");

    // Right of the gutter: the start of the line, no column further.
    act(&mut content, |ui| {
        let row = ui.find("3").expect("line 3's number").bounds();
        ui.point_at(Point::new(row.x + row.width + 1.0, row.center_y()));
        let _ = ui.simulate(click());
    });
    assert_eq!(content.cursor().position, Position { line: 2, column: 0 });
}

#[test]
fn a_drag_in_the_gutter_selects_whole_lines_either_way() {
    let _turn = common::render_turn();
    let mut content = Content::with_text("alpha\nbeta\ngamma\ndelta");
    let drag = |content: &mut Content, from: &str, to: &str| {
        act(content, |ui| {
            let from = ui.find(from).expect("pressed").bounds().center();
            let to = ui.find(to).expect("reached").bounds().center();
            ui.point_at(from);
            let _ = ui.simulate([Event::Mouse(mouse::Event::ButtonPressed(Button::Left))]);
            ui.point_at(to);
            let _ = ui.simulate([
                Event::Mouse(mouse::Event::CursorMoved { position: to }),
                Event::Mouse(mouse::Event::ButtonReleased(Button::Left)),
            ]);
        });
    };

    drag(&mut content, "1", "3");
    assert_eq!(content.selection().as_deref(), Some("alpha\nbeta\ngamma\n"));

    drag(&mut content, "3", "2");
    assert_eq!(content.selection().as_deref(), Some("beta\ngamma\n"));
    assert_eq!(
        content.cursor().position,
        Position { line: 1, column: 0 },
        "the caret where the pointer went"
    );

    drag(&mut content, "2", "4");
    assert_eq!(
        content.selection().as_deref(),
        Some("beta\ngamma\ndelta"),
        "the last line, to the end of the text"
    );
}
