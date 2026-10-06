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

//! The working folder a shell reports with OSC 7, as the C# terminal reads it: ended by BEL
//! or ST, cut anywhere between chunks, percent-decoded, the host left aside; refused when
//! not UTF-8, too long, or not an absolute path.

use heimdall_term::{
    GridSize, MAX_REPORT_LENGTH, Terminal, TerminalConfig, WorkingDirectoryScanner,
};

/// The last folder `chunks` report, fed one after the other.
fn reported(chunks: &[&[u8]]) -> Option<String> {
    let mut scanner = WorkingDirectoryScanner::new();
    let mut last = None;
    for chunk in chunks {
        if let Some(directory) = scanner.feed(chunk) {
            last = Some(directory);
        }
    }
    last
}

fn one(output: &[u8]) -> Option<String> {
    reported(&[output])
}

#[test]
fn a_report_ended_by_bel_or_st_names_its_folder() {
    assert_eq!(
        one(b"\x1b]7;file://web01/var/log\x07").as_deref(),
        Some("/var/log")
    );
    assert_eq!(
        one(b"\x1b]7;file://web01/var/log\x1b\\").as_deref(),
        Some("/var/log")
    );
    assert_eq!(
        one(b"before\x1b]7;file:///etc\x07$ ").as_deref(),
        Some("/etc"),
        "an empty host, among text"
    );
}

#[test]
fn a_report_cut_anywhere_between_chunks_is_completed_by_the_next() {
    let whole: &[u8] = b"$ \x1b]7;file://host/home/admin/projects\x1b\\$ ";
    for cut in 0..=whole.len() {
        let (first, second) = whole.split_at(cut);
        assert_eq!(
            reported(&[first, second]).as_deref(),
            Some("/home/admin/projects"),
            "cut at {cut}"
        );
    }
    let bytes: Vec<&[u8]> = whole.chunks(1).collect();
    assert_eq!(
        reported(&bytes).as_deref(),
        Some("/home/admin/projects"),
        "a byte at a time"
    );
}

#[test]
fn escapes_are_decoded_and_spaces_kept() {
    assert_eq!(
        one(b"\x1b]7;file://host/srv/my%20files/caf%C3%A9\x07").as_deref(),
        Some("/srv/my files/caf\u{e9}")
    );
    assert_eq!(
        one(b"\x1b]7;file://host/srv/my files\x07").as_deref(),
        Some("/srv/my files"),
        "a space sent as it is"
    );
    assert_eq!(
        one(b"\x1b]7;file://host/a%3bb;c\x07").as_deref(),
        Some("/a;b;c"),
        "a semicolon, escaped or not"
    );
    assert_eq!(
        one("\x1b]7;file://host/donn\u{e9}es\x07".as_bytes()).as_deref(),
        Some("/donn\u{e9}es"),
        "UTF-8 sent as it is"
    );
}

#[test]
fn the_host_is_left_aside_as_the_csharp() {
    for output in [
        &b"\x1b]7;file://other-server/opt/app\x07"[..],
        b"\x1b]7;file://localhost/opt/app\x07",
        b"\x1b]7;file:///opt/app\x07",
    ] {
        assert_eq!(one(output).as_deref(), Some("/opt/app"), "{output:?}");
    }
}

#[test]
fn bytes_that_are_not_utf8_once_decoded_are_refused() {
    assert_eq!(one(b"\x1b]7;file://host/bad%FF%FE\x07"), None);
    assert_eq!(one(b"\x1b]7;file://host/caf%C3\x07"), None, "cut short");
    assert_eq!(
        one(b"\x1b]7;file://host/raw\xff\x07"),
        None,
        "sent as it is"
    );
    assert_eq!(one(b"\x1b]7;file://host/50%\x07"), None, "an escape cut");
    assert_eq!(
        one(b"\x1b]7;file://host/50%zz\x07"),
        None,
        "not hexadecimal"
    );
}

#[test]
fn a_report_too_long_is_dropped_whole_and_the_next_one_read() {
    let mut long = b"\x1b]7;file://host/".to_vec();
    long.resize(long.len() + MAX_REPORT_LENGTH, b'a');
    long.push(0x07);
    assert_eq!(one(&long), None);
    let mut scanner = WorkingDirectoryScanner::new();
    assert_eq!(scanner.feed(&long), None);
    assert_eq!(
        scanner.feed(b"\x1b]7;file://host/tmp\x07").as_deref(),
        Some("/tmp"),
        "the next one read"
    );
    let mut longest = b"file://host/".to_vec();
    longest.resize(MAX_REPORT_LENGTH, b'b');
    let mut fits = b"\x1b]7;".to_vec();
    fits.extend_from_slice(&longest);
    fits.push(0x07);
    assert!(one(&fits).is_some(), "the longest taken");
}

#[test]
fn a_relative_path_or_another_scheme_is_refused() {
    for output in [
        &b"\x1b]7;file://host\x07"[..],
        b"\x1b]7;file://hostname%2Fetc\x07",
        b"\x1b]7;relative/path\x07",
        b"\x1b]7;/etc\x07",
        b"\x1b]7;http://host/etc\x07",
        b"\x1b]7;FILE://host/etc\x07",
        b"\x1b]7;\x07",
    ] {
        assert_eq!(one(output), None, "{output:?}");
    }
}

#[test]
fn a_control_character_in_the_path_is_refused() {
    assert_eq!(one(b"\x1b]7;file://host/a%0Ab\x07"), None);
    assert_eq!(one(b"\x1b]7;file://host/a%00b\x07"), None);
    assert_eq!(
        one(b"\x1b]7;file://host/a\x01b\x07").as_deref(),
        Some("/ab"),
        "sent as it is, ignored inside the command as the emulators do"
    );
}

#[test]
fn other_commands_and_abandoned_reports_say_nothing() {
    for output in [
        &b"\x1b]0;file://host/etc\x07"[..],
        b"\x1b]2;title\x1b\\",
        b"\x1b]77;file://host/etc\x07",
        b"\x1b]8;;file://host/etc\x07link\x1b]8;;\x07",
        b"\x1b[7;1Hplain file://host/etc",
        b"\x1b]7;file://host/etc\x18",
        b"\x1b]7;file://host/etc\x1a",
        b"\x1b]7;file://host/etc",
    ] {
        assert_eq!(one(output), None, "{output:?}");
    }
}

#[test]
fn the_last_report_of_a_chunk_is_the_one_given() {
    assert_eq!(
        one(b"\x1b]7;file://h/one\x07\x1b]0;t\x07\x1b]7;file://h/two\x1b\\").as_deref(),
        Some("/two")
    );
    assert_eq!(
        one(b"\x1b]7;file://h/one\x07\x1b]7;bad\x07").as_deref(),
        Some("/one"),
        "an invalid one after a valid one"
    );
}

#[test]
fn the_terminal_gives_the_report_with_its_output() {
    let mut terminal = Terminal::new(GridSize { cols: 80, rows: 24 }, TerminalConfig::default());
    let output = terminal.feed(b"\x1b]7;file://host/var/www\x07$ ");
    assert_eq!(output.working_directory.as_deref(), Some("/var/www"));
    assert_eq!(terminal.feed(b"ls\r\n").working_directory, None);
    assert_eq!(
        terminal.feed(b"\x1b]7;file://host/v").working_directory,
        None
    );
    assert_eq!(
        terminal.feed(b"ar\x1b\\").working_directory.as_deref(),
        Some("/var"),
        "cut between two reads"
    );
}
