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

//! The web addresses a Ctrl+click opens, found as the C# terminal finds them.

use heimdall_term::{CellPoint, GridSize, Terminal, TerminalConfig};

fn shown(text: &str) -> Terminal {
    let mut term = Terminal::new(GridSize { cols: 80, rows: 4 }, TerminalConfig::default());
    term.feed(text.as_bytes());
    term
}

fn at(term: &Terminal, row: usize, col: usize) -> Option<String> {
    term.url_at(CellPoint {
        row,
        col,
        right_half: false,
    })
}

#[test]
fn an_address_is_found_under_the_pointer_without_the_punctuation_after_it() {
    let term = shown("see https://git.lab/team/repo.git). Or http://wiki.lab, done\r\n");
    let first = "see ".len();
    let address = "https://git.lab/team/repo.git";
    assert_eq!(at(&term, 0, first).as_deref(), Some(address));
    assert_eq!(
        at(&term, 0, first + address.len() - 1).as_deref(),
        Some(address)
    );
    assert_eq!(
        at(&term, 0, first + address.len()),
        None,
        "the bracket after it"
    );
    assert_eq!(at(&term, 0, first - 1), None, "the space before it");
    let second = "see https://git.lab/team/repo.git). Or ".len();
    assert_eq!(at(&term, 0, second + 3).as_deref(), Some("http://wiki.lab"));
}

#[test]
fn an_address_ends_at_a_quote_or_an_angle_bracket_and_a_bare_scheme_is_none() {
    let term = shown("<https://a.lab/x> \"http://b.lab/y\" https:// ftp://c.lab\r\n");
    assert_eq!(at(&term, 0, 3).as_deref(), Some("https://a.lab/x"));
    assert_eq!(at(&term, 0, 22).as_deref(), Some("http://b.lab/y"));
    let bare = "<https://a.lab/x> \"http://b.lab/y\" ".len();
    assert_eq!(at(&term, 0, bare + 2), None, "nothing after the scheme");
    assert_eq!(at(&term, 0, bare + 12), None, "not a web address");
}

#[test]
fn wide_characters_before_an_address_do_not_shift_it() {
    let term = shown("\u{65e5}\u{672c} https://jp.lab\r\n");
    // Two wide characters take four columns, then the space.
    assert_eq!(at(&term, 0, 5).as_deref(), Some("https://jp.lab"));
    assert_eq!(at(&term, 0, 4), None);
}
