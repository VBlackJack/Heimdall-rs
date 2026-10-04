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

//! Finding text in the history, as the C# terminal's search bar.

use heimdall_term::{FindDirection, Found, GridSize, Terminal, TerminalConfig};

/// Thirty lines on a five-row screen: 26 in the history, "error" on lines 3, 12 and 25.
fn terminal() -> Terminal {
    let mut term = Terminal::new(GridSize { cols: 20, rows: 5 }, TerminalConfig::default());
    for n in 0..30 {
        let line = match n {
            3 => "Error A".to_owned(),
            12 => "an error B".to_owned(),
            25 => "ERROR C".to_owned(),
            _ => format!("line {n:02}"),
        };
        term.feed(format!("{line}\r\n").as_bytes());
    }
    term
}

#[expect(
    clippy::unnecessary_wraps,
    reason = "compared with what `find` gives back"
)]
fn found(index: usize, total: usize) -> Option<Found> {
    Some(Found { index, total })
}

#[test]
fn up_from_the_view_finds_each_match_older_and_wraps_around_as_the_csharp() {
    let mut term = terminal();
    assert_eq!(term.display_offset(), 0);

    assert_eq!(term.find("error", FindDirection::Up), found(3, 3));
    assert_eq!(term.selected_text().as_deref(), Some("ERROR"));
    // Line 25 is the last of the history: five lines kept above it.
    assert_eq!(term.display_offset(), 6);

    assert_eq!(term.find("error", FindDirection::Up), found(2, 3));
    assert_eq!(term.selected_text().as_deref(), Some("error"));
    assert_eq!(term.display_offset(), 19);

    assert_eq!(
        term.find("ErRoR", FindDirection::Up),
        found(1, 3),
        "whatever the case"
    );
    assert_eq!(term.selected_text().as_deref(), Some("Error"));
    assert_eq!(term.display_offset(), 26, "no further up than the history");

    assert_eq!(
        term.find("error", FindDirection::Up),
        found(3, 3),
        "past the oldest: the newest again"
    );
    assert_eq!(term.selected_text().as_deref(), Some("ERROR"));
    assert_eq!(
        term.find("error", FindDirection::Down),
        found(1, 3),
        "past the newest: the oldest again"
    );
    assert_eq!(term.selected_text().as_deref(), Some("Error"));
}

#[test]
fn down_from_the_view_starts_at_its_top_line() {
    let mut term = terminal();
    // Line 3 at the top of the view: found there, not below it.
    term.scroll(23);
    assert_eq!(term.find("error", FindDirection::Down), found(1, 3));
    assert_eq!(term.selected_text().as_deref(), Some("Error"));
    assert_eq!(term.display_offset(), 26);

    let mut term = terminal();
    assert_eq!(
        term.find("error", FindDirection::Down),
        found(1, 3),
        "from the bottom, nothing below: the first"
    );
    assert_eq!(
        term.find("", FindDirection::Up),
        None,
        "nothing to look for"
    );
    assert_eq!(term.find("absent", FindDirection::Up), None);
    assert_eq!(
        term.selected_text(),
        None,
        "nothing found: nothing selected"
    );
}

#[test]
fn every_match_of_a_line_is_counted_and_stepped_through() {
    let mut term = Terminal::new(GridSize { cols: 40, rows: 3 }, TerminalConfig::default());
    term.feed(b"aa bb aa cc aa\r\nnone\r\n");
    assert_eq!(term.find("aa", FindDirection::Down), found(1, 3));
    assert_eq!(term.find("aa", FindDirection::Down), found(2, 3));
    assert_eq!(term.find("aa", FindDirection::Down), found(3, 3));
    assert_eq!(term.find("aa", FindDirection::Down), found(1, 3));
    term.clear_selection();
    assert_eq!(
        term.find("a", FindDirection::Down),
        found(1, 6),
        "not overlapping"
    );
}

#[test]
fn a_match_after_wide_characters_is_selected_at_its_own_columns() {
    let mut term = Terminal::new(GridSize { cols: 30, rows: 3 }, TerminalConfig::default());
    term.feed("日本 error\r\n\u{130}stanbul x\r\n".as_bytes());
    assert!(term.find("error", FindDirection::Up).is_some());
    assert_eq!(term.selected_text().as_deref(), Some("error"));
    term.clear_selection();
    assert!(
        term.find("日本", FindDirection::Up).is_some(),
        "the wide characters themselves"
    );
    assert_eq!(term.selected_text().as_deref(), Some("日本"));
    term.clear_selection();
    // İ lowers to two characters: the column after it is still its own.
    assert!(term.find("x", FindDirection::Up).is_some());
    assert_eq!(term.selected_text().as_deref(), Some("x"));
    term.clear_selection();
    assert!(term.find("\u{130}s", FindDirection::Up).is_some());
    assert_eq!(term.selected_text().as_deref(), Some("\u{130}s"));
}
