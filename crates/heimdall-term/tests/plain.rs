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

//! A session's output as the plain text of its transcript, as the C# stripper makes it.

use heimdall_term::PlainText;

fn plain(chunks: &[&[u8]]) -> String {
    let mut stripper = PlainText::new();
    let mut text: String = chunks.iter().map(|chunk| stripper.feed(chunk)).collect();
    text.push_str(&stripper.finish());
    text
}

#[test]
fn every_kind_of_sequence_is_left_out_and_the_text_kept() {
    for (output, text) in [
        (&b"hello \x1b[1;31mred\x1b[0m\r\n"[..], "hello red\r\n"),
        (b"\x1b]0;my title\x07after", "after"),
        (b"\x1b]0;my title\x1b\\after", "after"),
        (b"a\x1bPq#0;2;0;0;0\x1b\\b", "ab"),
        (b"\x1b_hidden\x07c", "c"),
        (b"\x1b(Bplain", "plain"),
        (b"\x1b=\x1b>\x1b7\x1b8x", "x"),
        (b"\x1b[?1049h\x1b[2J\x1b[Hscreen", "screen"),
        (b"\x1b[ qcursor", "cursor"),
        (b"tab\there\x08!", "tab\there\x08!"),
    ] {
        assert_eq!(plain(&[output]), text, "{output:?}");
    }
}

#[test]
fn a_sequence_or_a_character_cut_between_chunks_is_completed_by_the_next() {
    assert_eq!(plain(&[b"red: \x1b[3", b"1mX"]), "red: X");
    assert_eq!(plain(&[b"\x1b", b"]0;t", b"itle\x1b", b"\\Y"]), "Y");
    assert_eq!(plain(&[b"caf\xc3", b"\xa9"]), "caf\u{e9}");
    assert_eq!(plain(&[b"\xe2\x82", b"\xac"]), "\u{20ac}");
    let mut stripper = PlainText::new();
    assert_eq!(stripper.feed(b"\xe2\x82"), "", "held back");
    assert_eq!(stripper.feed(b"\xac"), "\u{20ac}");
}

#[test]
fn what_is_not_a_sequence_is_text_and_invalid_bytes_are_replaced() {
    assert_eq!(plain(&[b"\x1b\x01abc"]), "\x1b\x01abc");
    assert_eq!(plain(&[b"\x1b[1\x01z"]), "\x1b[1\x01z");
    assert_eq!(
        plain(&[b"\x1b[1 1m"]),
        "\x1b[1 1m",
        "no parameter after an intermediate"
    );
    assert_eq!(
        plain(&[b"\x1b]0;t\x1b[31mX"]),
        "X",
        "a string cut by a new sequence"
    );
    assert_eq!(plain(&[b"a\xffb"]), "a\u{fffd}b");
}

#[test]
fn the_end_gives_back_an_unfinished_character_and_drops_an_unfinished_sequence() {
    let mut stripper = PlainText::new();
    assert_eq!(stripper.feed(b"ok \xe2\x82"), "ok ");
    assert_eq!(stripper.finish(), "\u{fffd}");
    assert_eq!(stripper.feed(b"\x1b[3"), "");
    assert_eq!(stripper.finish(), "");
    assert_eq!(stripper.feed(b"1m"), "1m", "the sequence was forgotten");
}
