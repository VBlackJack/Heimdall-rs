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

//! Escape sequences in, screen and replies out.

use std::time::Instant;

use heimdall_term::palette::dim;
use heimdall_term::{
    CellPixels, CellPoint, CellWidth, ClipboardPolicy, CursorStyle, GridSize, Palette, Rgb,
    SelectionKind, Terminal, TerminalConfig, TitleChange,
};

const DRACULA: Palette = Palette::dracula();

fn terminal(cols: usize, rows: usize) -> Terminal {
    Terminal::new(GridSize { cols, rows }, TerminalConfig::default())
}

fn rgb(r: u8, g: u8, b: u8) -> Rgb {
    Rgb { r, g, b }
}

fn at(row: usize, col: usize, right_half: bool) -> CellPoint {
    CellPoint {
        row,
        col,
        right_half,
    }
}

// ---- text, cursor, colours --------------------------------------------------------------

#[test]
fn text_lands_where_the_cursor_is() {
    let mut term = terminal(20, 3);
    term.feed(b"hello\r\nworld");
    let screen = term.snapshot();
    assert!(screen.row_text(0).starts_with("hello"));
    assert!(screen.row_text(1).starts_with("world"));
    let cursor = screen.cursor.expect("visible");
    assert_eq!((cursor.row, cursor.col), (1, 5));
}

#[test]
fn named_bold_becomes_bright_but_indexed_bold_does_not() {
    let mut term = terminal(10, 2);
    term.feed(b"\x1b[31mA\x1b[1;31mB\x1b[0;1;38;5;1mC");
    let screen = term.snapshot();
    assert_eq!(screen.cell(0, 0).expect("A").fg, DRACULA.ansi[1]);
    assert_eq!(screen.cell(0, 1).expect("B").fg, DRACULA.ansi[9]);
    assert_eq!(screen.cell(0, 2).expect("C").fg, DRACULA.ansi[1]);
}

#[test]
fn truecolor_indexed_and_the_server_palette_override() {
    let mut term = terminal(10, 2);
    term.feed(b"\x1b[38;2;1;2;3mT\x1b[38;5;196mI\x1b]4;1;rgb:12/34/56\x1b\\\x1b[0;31mO");
    let screen = term.snapshot();
    assert_eq!(screen.cell(0, 0).expect("T").fg, rgb(1, 2, 3));
    assert_eq!(screen.cell(0, 1).expect("I").fg, rgb(255, 0, 0));
    assert_eq!(
        screen.cell(0, 2).expect("O").fg,
        rgb(0x12, 0x34, 0x56),
        "OSC 4 overrides colour 1"
    );
}

#[test]
fn inverse_swaps_after_resolution_even_with_default_colours() {
    let mut term = terminal(10, 2);
    term.feed(b"\x1b[7mX\x1b[0;8mH\x1b[0;2;31mD");
    let screen = term.snapshot();
    let inverse = screen.cell(0, 0).expect("X");
    assert_eq!(
        (inverse.fg, inverse.bg),
        (DRACULA.background, DRACULA.foreground)
    );
    let hidden = screen.cell(0, 1).expect("H");
    assert_eq!(hidden.fg, hidden.bg);
    assert_eq!(screen.cell(0, 2).expect("D").fg, dim(DRACULA.ansi[1]));
}

#[test]
fn wide_zero_width_and_tab_cells() {
    let mut term = terminal(10, 2);
    term.feed("\u{4e2d}e\u{301}\tz".as_bytes());
    let screen = term.snapshot();
    assert_eq!(screen.cell(0, 0).expect("wide").width, CellWidth::Wide);
    assert_eq!(screen.cell(0, 1).expect("spacer").width, CellWidth::Spacer);
    let accented = screen.cell(0, 2).expect("e");
    assert_eq!(
        (accented.ch, accented.zerowidth.as_slice()),
        ('e', &['\u{301}'][..])
    );
    assert_eq!(screen.cell(0, 3).expect("tab").ch, ' ');
    assert!(screen.row_text(0).starts_with("\u{4e2d}e\u{301}"));
}

#[test]
fn cursor_shape_and_visibility_follow_the_application() {
    let mut term = terminal(10, 2);
    term.feed(b"\x1b[6 q");
    assert_eq!(
        term.snapshot().cursor.expect("visible").style,
        CursorStyle::Beam
    );
    term.feed(b"\x1b[?25l");
    assert_eq!(term.snapshot().cursor, None);
}

// ---- history ----------------------------------------------------------------------------

fn numbered_lines(term: &mut Terminal, count: usize) {
    for line in 0..count {
        term.feed(format!("line{line}\r\n").as_bytes());
    }
}

#[test]
fn scrolling_back_shows_history_and_hides_the_cursor() {
    let mut term = terminal(10, 3);
    numbered_lines(&mut term, 10);
    term.scroll(5);
    let screen = term.snapshot();
    assert_eq!(screen.display_offset, 5);
    assert!(
        screen.row_text(0).starts_with("line3"),
        "{:?}",
        screen.row_text(0)
    );
    assert_eq!(screen.cursor, None, "the cursor is below the view");
    term.scroll_to_bottom();
    assert!(term.snapshot().cursor.is_some());
}

#[test]
fn new_output_does_not_move_a_view_scrolled_back() {
    let mut term = terminal(10, 3);
    numbered_lines(&mut term, 10);
    term.scroll(5);
    let before = term.snapshot().row_text(0);
    numbered_lines(&mut term, 2);
    assert_eq!(term.snapshot().row_text(0), before);
}

#[test]
fn a_zero_size_is_clamped_instead_of_panicking() {
    let mut term = terminal(0, 0);
    assert_eq!(term.size(), GridSize { cols: 2, rows: 1 });
    assert_eq!(
        term.resize(GridSize { cols: 0, rows: 0 }),
        GridSize { cols: 2, rows: 1 }
    );
    term.feed(b"abc");
}

// ---- replies ----------------------------------------------------------------------------

#[test]
fn cursor_position_and_device_attributes_are_answered() {
    let mut term = terminal(20, 5);
    let output = term.feed(b"ab\x1b[6n\x1b[c");
    assert_eq!(output.replies, b"\x1b[1;3R\x1b[?6c");
}

#[test]
fn colour_queries_are_answered_in_order_with_the_current_colours() {
    let mut term = terminal(20, 5);
    let output = term.feed(b"\x1b]11;?\x1b\\\x1b[c");
    let text = String::from_utf8(output.replies).expect("utf8");
    assert!(text.starts_with("\x1b]11;rgb:2828/2a2a/3636"), "{text:?}");
    assert!(
        text.ends_with("\x1b[?6c"),
        "DA answered after the colour: {text:?}"
    );
}

#[test]
fn the_window_size_query_is_answered_only_once_the_cell_size_is_known() {
    let mut term = terminal(80, 24);
    assert!(term.feed(b"\x1b[14t").replies.is_empty());
    term.set_cell_pixels(CellPixels {
        width: 10,
        height: 20,
    });
    assert_eq!(term.feed(b"\x1b[14t").replies, b"\x1b[4;480;800t");
}

#[test]
fn title_and_bell() {
    let mut term = terminal(20, 5);
    let output = term.feed(b"\x1b]2;prod-web\x07\x07");
    assert_eq!(output.title, Some(TitleChange::Set("prod-web".to_owned())));
    assert!(output.bell);
}

#[test]
fn osc52_writes_only_when_allowed_and_reads_never() {
    let store = b"\x1b]52;c;aGVsbG8=\x07";
    assert_eq!(
        terminal(20, 5).feed(store).clipboard,
        None,
        "disabled by default"
    );

    let mut allowed = Terminal::new(
        GridSize { cols: 20, rows: 5 },
        TerminalConfig {
            clipboard: ClipboardPolicy::AllowWrite,
            ..TerminalConfig::default()
        },
    );
    assert_eq!(allowed.feed(store).clipboard, Some("hello".to_owned()));
    assert!(
        allowed.feed(b"\x1b]52;c;?\x07").replies.is_empty(),
        "the clipboard is never read back to the server"
    );
}

// ---- synchronized updates ---------------------------------------------------------------

#[test]
fn an_unclosed_synchronized_update_is_flushed_on_demand() {
    let mut term = terminal(20, 3);
    let output = term.feed(b"\x1b[?2026hHIDDEN");
    assert!(!output.redraw);
    let deadline = term.sync_deadline().expect("a deadline is set");
    assert!(deadline > Instant::now());
    assert!(!term.snapshot().row_text(0).contains("HIDDEN"));
    assert!(term.flush_sync().redraw);
    assert!(term.snapshot().row_text(0).starts_with("HIDDEN"));
    assert_eq!(term.sync_deadline(), None);
}

// ---- selection --------------------------------------------------------------------------

#[test]
fn the_side_of_the_end_cell_decides_whether_it_is_selected() {
    let mut term = terminal(20, 3);
    term.feed(b"hello world");
    term.begin_selection(at(0, 0, false), SelectionKind::Simple);
    term.extend_selection(at(0, 4, true));
    assert_eq!(term.selected_text().as_deref(), Some("hello"));
    term.extend_selection(at(0, 4, false));
    assert_eq!(term.selected_text().as_deref(), Some("hell"));
}

#[test]
fn a_wrapped_line_is_copied_without_a_line_break() {
    let mut term = terminal(5, 3);
    term.feed(b"abcdefgh");
    term.begin_selection(at(0, 0, false), SelectionKind::Simple);
    term.extend_selection(at(1, 2, true));
    assert_eq!(term.selected_text().as_deref(), Some("abcdefgh"));
}

#[test]
fn selected_cells_are_marked_and_a_word_selection_takes_the_word() {
    let mut term = terminal(20, 3);
    term.feed(b"one two three");
    term.begin_selection(at(0, 5, false), SelectionKind::Word);
    assert_eq!(term.selected_text().as_deref(), Some("two"));
    let screen = term.snapshot();
    let marked: Vec<usize> = (0..20)
        .filter(|col| screen.cell(0, *col).is_some_and(|cell| cell.selected))
        .collect();
    assert_eq!(marked, vec![4, 5, 6]);
}

#[test]
fn selecting_while_scrolled_back_uses_the_history_lines() {
    let mut term = terminal(10, 3);
    numbered_lines(&mut term, 10);
    term.scroll(5);
    term.begin_selection(at(0, 0, false), SelectionKind::Line);
    assert_eq!(term.selected_text().as_deref(), Some("line3\n"));
}

// ---- block selection, as xterm.js's Alt+drag --------------------------------------------

/// What ends each line of a block copied, as xterm.js joins them.
const BLOCK_BREAK: &str = if cfg!(windows) { "\r\n" } else { "\n" };

fn block(term: &mut Terminal, from: CellPoint, to: CellPoint) -> Option<String> {
    term.begin_selection(from, SelectionKind::Block);
    term.extend_selection(to);
    term.selected_text()
}

#[test]
fn a_block_takes_the_same_columns_from_every_line_ragged_ones_included() {
    let mut term = terminal(20, 4);
    term.feed(b"abcdefgh\r\nab\r\nabcdefghij");
    let expected = ["cdef", "", "cdef"].join(BLOCK_BREAK);
    assert_eq!(
        block(&mut term, at(0, 2, false), at(2, 5, true)).as_deref(),
        Some(expected.as_str())
    );
    let screen = term.snapshot();
    let marked = |row: usize| -> Vec<usize> {
        (0..20)
            .filter(|col| screen.cell(row, *col).is_some_and(|cell| cell.selected))
            .collect()
    };
    for row in 0..3 {
        assert_eq!(marked(row), [2, 3, 4, 5], "row {row}: a rectangle");
    }
    assert!(marked(3).is_empty());
    // Dragged the other way, from the bottom right to the top left: the same rectangle.
    assert_eq!(
        block(&mut term, at(2, 5, true), at(0, 2, false)).as_deref(),
        Some(expected.as_str())
    );
}

#[test]
fn a_block_drops_the_blanks_ending_each_line_and_an_empty_one_copies_nothing() {
    let mut term = terminal(20, 3);
    term.feed(b"ab  cd\r\n  x");
    let expected = ["ab", "  x"].join(BLOCK_BREAK);
    assert_eq!(
        block(&mut term, at(0, 0, false), at(1, 3, true)).as_deref(),
        Some(expected.as_str())
    );
    // Not dragged across a column: no rectangle.
    assert_eq!(block(&mut term, at(0, 2, false), at(1, 2, false)), None);
}

#[test]
fn a_wide_character_starting_in_a_block_is_taken_whole_and_a_half_one_is_a_space() {
    let mut term = terminal(10, 3);
    // Row 0: a, the wide character over columns 1-2, b. Row 1: two wide characters.
    term.feed("a\u{4e2d}b\r\n\u{4e2d}\u{6587}".as_bytes());
    let expected = [" b", "\u{6587}"].join(BLOCK_BREAK);
    assert_eq!(
        block(&mut term, at(0, 2, false), at(1, 3, true)).as_deref(),
        Some(expected.as_str()),
        "the right half of a character starting left of the block is a space"
    );
    let expected = ["\u{4e2d}", ""].join(BLOCK_BREAK);
    assert_eq!(
        block(&mut term, at(0, 1, false), at(1, 1, true)).as_deref(),
        Some(expected.as_str()),
        "one starting in the block is taken whole, its right half past it"
    );
}

#[test]
fn the_cursor_blinks_until_the_application_asks_for_a_steady_one() {
    let mut term = terminal(10, 2);
    assert!(
        term.cursor_blinks(),
        "xterm.js cursorBlink, as the C# terminal"
    );
    term.feed(b"\x1b[2 q");
    assert!(!term.cursor_blinks(), "a steady block");
    term.feed(b"\x1b[5 q");
    assert!(term.cursor_blinks(), "a blinking bar");
    term.feed(b"\x1b[?12l");
    assert!(!term.cursor_blinks(), "blinking reset");
    term.feed(b"\x1b[0 q");
    assert!(term.cursor_blinks(), "back to the default");
    assert_eq!(term.cursor_position(), (0, 0));
    term.feed(b"ab\r\nc");
    assert_eq!(term.cursor_position(), (1, 1));
}
