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

//! "Capture a command from the terminal", as the C# `CommandCapture` (PR #659): text
//! selected in a session of text becomes the pattern of a new action of the command
//! library, for review in its editor.
//!
//! The selection is untrusted: it is whatever a program drew on the screen. It is cleaned
//! of terminal escape sequences and of control characters before it goes anywhere, a
//! multi-line selection is joined into one line (a pattern holds one command), and nothing is
//! sent to a session or run: the result is only a draft the user reads, completes with a
//! title and saves through the usual checks of the editor.

use std::iter::Peekable;
use std::str::Chars;

use heimdall_core::profile::RemoteOs;
use heimdall_core::tools::user_commands::{MAX_PATTERN_LENGTH, TemplatePlatform};

use super::tree::ProfileKind;
use super::{App, Notice, Phase, Tab};
use crate::driver::Purpose;
use crate::ids::TabId;
use crate::tools::ToolContext;

/// What joins two captured lines in a Windows pattern, as the C#
/// `CommandCapture.WindowsLineSeparator`.
pub const WINDOWS_LINE_SEPARATOR: &str = " ; ";

/// What joins two captured lines in a Linux pattern, as the C#
/// `CommandCapture.LinuxLineSeparator`.
pub const LINUX_LINE_SEPARATOR: &str = " && ";

/// How many characters of a selection are looked at before cleaning: a bound on the work, far
/// above what a pattern can hold, so that a screenful selected by mistake costs nothing.
const SCAN_LIMIT_CHARS: usize = MAX_PATTERN_LENGTH * 8;

/// How many characters the command line keeps: one more than a pattern can hold, so that a
/// selection too long for a pattern still reads as too long in the editor instead of being
/// cut silently into something that saves.
const KEEP_LIMIT_CHARS: usize = MAX_PATTERN_LENGTH + 1;

const ESC: char = '\u{1b}';
const BEL: char = '\u{07}';
/// The single-character form of CSI (C1).
const CSI_C1: char = '\u{9b}';
/// The single-character form of the string terminator (C1).
const ST_C1: char = '\u{9c}';
/// The C1 introducers of OSC, DCS, SOS, PM and APC strings.
const STRING_C1: [char; 5] = ['\u{9d}', '\u{90}', '\u{98}', '\u{9e}', '\u{9f}'];
/// The second character of `ESC x` that opens a string: OSC, DCS, SOS, PM, APC.
const STRING_INTRODUCERS: [char; 5] = [']', 'P', 'X', '^', '_'];
/// The second character of `ESC x` that takes one more character: charset designations and
/// the like.
const ESC_WITH_ARGUMENT: [char; 10] = ['(', ')', '*', '+', '-', '.', '/', '#', '%', ' '];

/// A command captured from a terminal, ready to prefill the editor of a new action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapturedCommand {
    /// The command line, cleaned and joined to one line.
    pub text: String,
    /// The pattern the text is written into: never both, a captured line is a command for
    /// one shell.
    pub platform: TemplatePlatform,
    /// The session's protocol in lower case, kept as a tag.
    pub tag: Option<String>,
}

/// The platform a command captured from a session is written for: Windows when the session
/// resolves to Windows (declared, or a protocol that is always Windows), Linux otherwise,
/// including when nothing can be told, as the C# `CommandCapture.ResolvePlatform`.
#[must_use]
pub fn capture_platform(platform: RemoteOs) -> TemplatePlatform {
    if platform == RemoteOs::Windows {
        TemplatePlatform::Windows
    } else {
        TemplatePlatform::Linux
    }
}

/// `text` without terminal escape sequences and control characters.
///
/// Removed whole: CSI (`ESC [` or the C1 form, up to its final byte), OSC, DCS, SOS, PM and
/// APC strings (up to BEL or the string terminator), and any other `ESC` with the character
/// that follows it. Then every control character goes, the C1 ones and the invisible
/// direction overrides included, except the line breaks (kept as `\n`, for the join) and the
/// tab (a space).
#[must_use]
pub fn clean_selection(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            ESC => match chars.next() {
                Some('[') => skip_csi(&mut chars),
                Some(next) if STRING_INTRODUCERS.contains(&next) => skip_string(&mut chars),
                Some(next) if ESC_WITH_ARGUMENT.contains(&next) => {
                    chars.next();
                }
                Some(_) | None => {}
            },
            CSI_C1 => skip_csi(&mut chars),
            c if STRING_C1.contains(&c) => skip_string(&mut chars),
            '\n' | '\r' | '\u{85}' | '\u{2028}' | '\u{2029}' => out.push('\n'),
            '\t' => out.push(' '),
            c if c.is_control() || is_invisible(c) => {}
            c => out.push(c),
        }
    }
    out
}

/// The rest of a CSI sequence: parameters and intermediates up to the final byte.
fn skip_csi(chars: &mut Peekable<Chars<'_>>) {
    for c in chars.by_ref() {
        if ('\u{40}'..='\u{7e}').contains(&c) {
            return;
        }
    }
}

/// The rest of a string sequence, up to BEL or the string terminator (`ESC \` or C1).
fn skip_string(chars: &mut Peekable<Chars<'_>>) {
    while let Some(c) = chars.next() {
        match c {
            BEL | ST_C1 => return,
            ESC => {
                if chars.peek() == Some(&'\\') {
                    chars.next();
                }
                return;
            }
            _ => {}
        }
    }
}

/// Characters that draw nothing and change how text reads: the zero-width and direction
/// marks and overrides, the isolates, and the byte-order mark.
fn is_invisible(c: char) -> bool {
    matches!(
        c,
        '\u{200b}'..='\u{200f}'
            | '\u{202a}'..='\u{202e}'
            | '\u{2060}'..='\u{2064}'
            | '\u{2066}'..='\u{206f}'
            | '\u{feff}'
    )
}

/// The selection as one command line: cleaned, each line trimmed, blank lines dropped, the
/// rest joined with the separator of `platform`. Empty when nothing is left.
#[must_use]
pub fn to_command_line(selection: &str, platform: TemplatePlatform) -> String {
    let bounded: String = selection.chars().take(SCAN_LIMIT_CHARS).collect();
    let cleaned = clean_selection(&bounded);
    let lines: Vec<&str> = cleaned
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    let separator = match platform {
        TemplatePlatform::Windows => WINDOWS_LINE_SEPARATOR,
        TemplatePlatform::Linux => LINUX_LINE_SEPARATOR,
    };
    lines
        .join(separator)
        .chars()
        .take(KEEP_LIMIT_CHARS)
        .collect()
}

impl App {
    /// Whether `tab` shows a session of text whose selection can be captured into the
    /// command library: an SSH, Telnet, `WinRM` or local shell, connected or ended. Never a
    /// remote desktop, a Files tab, a web console or a tool.
    #[must_use]
    pub fn can_capture_command(&self, tab: &Tab) -> bool {
        matches!(
            self.tab_kind(tab),
            ProfileKind::Ssh | ProfileKind::Telnet | ProfileKind::WinRm | ProfileKind::Local
        ) && tab.purpose == Purpose::Shell
            && tab.files.is_none()
            && tab.desktop.is_none()
            && tab.elevated.is_none()
            && matches!(tab.phase, Phase::Connected | Phase::Closed { .. })
    }

    /// Reads the selection of `tab_id`'s terminal and keeps it, cleaned and joined, for the
    /// window to open the command library on. Nothing is written to the session.
    pub(super) fn capture_command(&mut self, tab_id: TabId) {
        if self.dialog.is_some() {
            return;
        }
        let Some(tab) = self.tab(tab_id).filter(|tab| self.can_capture_command(tab)) else {
            return;
        };
        let platform = capture_platform(tab.profile.platform());
        let text = tab
            .terminal
            .selected_text()
            .map(|selection| to_command_line(&selection, platform))
            .unwrap_or_default();
        if text.is_empty() {
            self.tell(Notice::CaptureNothingSelected);
            return;
        }
        let tag = self.tab_kind(tab).label().to_ascii_lowercase();
        let context = ToolContext {
            session_tab: Some(tab.id),
            capture: Some(CapturedCommand {
                text,
                platform,
                tag: Some(tag),
            }),
            ..ToolContext::of_session(
                tab.profile.endpoint().map(|(host, _)| host),
                tab.profile.username(),
                tab.profile.platform(),
            )
        };
        self.pending_capture = Some(context);
    }

    /// The capture waiting for the window, with the facts of its session, taken.
    pub fn take_capture(&mut self) -> Option<ToolContext> {
        self.pending_capture.take()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_windows_selection_is_joined_with_a_semicolon_and_a_linux_one_with_a_double_ampersand() {
        let selection = "cd C:\\logs\r\nGet-ChildItem\n";
        assert_eq!(
            to_command_line(selection, TemplatePlatform::Windows),
            "cd C:\\logs ; Get-ChildItem"
        );
        assert_eq!(
            to_command_line("cd /var/log\nls -la\n", TemplatePlatform::Linux),
            "cd /var/log && ls -la"
        );
    }

    #[test]
    fn lines_are_trimmed_and_blank_ones_dropped() {
        assert_eq!(
            to_command_line("  uptime  \n\n   \n\tdf -h\t\n", TemplatePlatform::Linux),
            "uptime && df -h"
        );
        assert_eq!(to_command_line("  \n \r\n", TemplatePlatform::Linux), "");
        assert_eq!(to_command_line("", TemplatePlatform::Windows), "");
    }

    #[test]
    fn a_single_line_is_left_alone() {
        assert_eq!(
            to_command_line("systemctl status sshd", TemplatePlatform::Linux),
            "systemctl status sshd"
        );
    }

    #[test]
    fn the_platform_is_windows_only_when_the_session_is() {
        assert_eq!(
            capture_platform(RemoteOs::Windows),
            TemplatePlatform::Windows
        );
        assert_eq!(capture_platform(RemoteOs::Linux), TemplatePlatform::Linux);
        assert_eq!(
            capture_platform(RemoteOs::Auto),
            TemplatePlatform::Linux,
            "nothing can be told: Linux, as the C#"
        );
    }

    #[test]
    fn escape_sequences_are_removed_whole() {
        // Colours, cursor moves, a window title, a hyperlink, a device control string.
        let text = "\u{1b}[31mls\u{1b}[0m \u{1b}[2J-la\u{1b}]0;owned\u{7}\
                    \u{1b}]8;;http://x\u{1b}\\link\u{1b}]8;;\u{1b}\\\u{1b}Pq#0;2;0;0;0\u{1b}\\";
        assert_eq!(clean_selection(text), "ls -lalink");
    }

    #[test]
    fn c1_controls_and_lone_escapes_are_removed_too() {
        assert_eq!(clean_selection("a\u{9b}31mb"), "ab");
        assert_eq!(clean_selection("a\u{9d}0;title\u{9c}b"), "ab");
        assert_eq!(clean_selection("a\u{1b}Mb"), "ab", "a two-character escape");
        assert_eq!(clean_selection("a\u{1b}(Bb"), "ab", "a charset designation");
        assert_eq!(clean_selection("ab\u{1b}"), "ab", "an escape at the end");
        assert_eq!(
            clean_selection("a\u{1b}[31"),
            "a",
            "a sequence never finished"
        );
    }

    #[test]
    fn control_characters_and_invisible_marks_are_removed_but_line_breaks_stay() {
        let text = "a\u{0}b\u{7}c\u{8}d\u{7f}e\u{200b}f\u{202e}g\u{feff}h\u{2066}i";
        assert_eq!(clean_selection(text), "abcdefghi");
        assert_eq!(clean_selection("a\tb"), "a b");
        assert_eq!(clean_selection("a\r\nb\rc\u{2028}d"), "a\n\nb\nc\nd");
        assert_eq!(
            to_command_line("rm\u{0}\u{202e} -rf /tmp/x\u{7}", TemplatePlatform::Linux),
            "rm -rf /tmp/x",
            "the join of a cleaned selection holds none"
        );
    }

    #[test]
    fn what_is_kept_is_text_the_user_could_have_typed() {
        let kept = to_command_line(
            "echo \u{1b}[1mh\u{e9}llo\u{1b}[0m \u{1f600}\r\n",
            TemplatePlatform::Linux,
        );
        assert_eq!(kept, "echo h\u{e9}llo \u{1f600}");
        assert!(!kept.chars().any(char::is_control));
    }

    #[test]
    fn a_selection_too_long_for_a_pattern_still_reads_as_too_long() {
        let long = "x".repeat(MAX_PATTERN_LENGTH * 3);
        let kept = to_command_line(&long, TemplatePlatform::Linux);
        assert_eq!(
            kept.chars().count(),
            MAX_PATTERN_LENGTH + 1,
            "one past the limit, so the editor refuses it instead of saving a cut command"
        );
    }
}
