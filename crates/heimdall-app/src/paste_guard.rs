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

//! Text about to be pasted into a shell, checked for commands that destroy data or stop
//! the machine, as the C# `SmartPasteGuard`: such a paste is asked about even on one line
//! and even into a shell that would not run it on its own.
//!
//! The C# patterns, each ported as a small matcher rather than a regular expression: case
//! is ignored, names stand as whole words, and a pattern never spans two lines. A fetch
//! piped into a shell is also caught with `sudo` before the shell.
//!
//! What is asked about is shown first, as the C# `PasteConfirmDialog` previews it: its first
//! lines, every character that would not show as itself written out, and a word when some
//! of it is left out of sight. See [`PastePreview`].

use crate::text::visible_text;

/// Whether a line, in lower case, holds a command.
type Matcher = fn(&str) -> bool;

/// The C# commands, by the label it gives them.
const COMMANDS: [(&str, Matcher); 25] = [
    ("rm -rf", |line| rm_flags(line, 'r', 'f')),
    ("rm -fr", |line| rm_flags(line, 'f', 'r')),
    ("mkfs", |line| has_word(line, "mkfs")),
    ("dd if=", |line| word_then(line, "dd", "if=")),
    ("format", format_drive),
    ("shutdown", |line| has_word(line, "shutdown")),
    ("Remove-Item -Recurse -Force", remove_item_recurse_force),
    ("Stop-Computer", |line| has_word(line, "stop-computer")),
    ("Restart-Computer", |line| {
        has_word(line, "restart-computer")
    }),
    ("Format-Volume", |line| has_word(line, "format-volume")),
    ("Clear-Disk", |line| has_word(line, "clear-disk")),
    ("Remove-Partition", |line| {
        has_word(line, "remove-partition")
    }),
    ("reg delete", reg_delete),
    ("bcdedit", |line| has_word(line, "bcdedit")),
    ("diskpart", |line| has_word(line, "diskpart")),
    ("reboot", |line| has_word(line, "reboot")),
    ("init 0/6", init_halt),
    ("halt", |line| has_word(line, "halt")),
    ("poweroff", |line| has_word(line, "poweroff")),
    ("> /dev/sda", write_to_disk),
    (":(){ :|:&};:", fork_bomb),
    ("chmod -R 777", chmod_recursive_777),
    ("chown -R", chown_recursive),
    ("wget | sh", |line| fetch_into_shell(line, "wget")),
    ("curl | sh", |line| fetch_into_shell(line, "curl")),
];

/// The label of the first destructive command `text` holds, as the C# names it; `None` when
/// it holds none.
#[must_use]
pub fn dangerous_command(text: &str) -> Option<&'static str> {
    text.split(['\r', '\n']).find_map(|line| {
        let lower = line.to_lowercase();
        COMMANDS
            .iter()
            .find(|(_, matches)| matches(&lower))
            .map(|(label, _)| *label)
    })
}

/// The text of a paste as its confirmation shows it, as the C#
/// `PasteConfirmDialogViewModel` previews it: at most [`PastePreview::MAX_LINES`] lines and
/// [`PastePreview::MAX_CHARS`] characters of them. Each line has its control characters, an
/// escape sequence's included, and every other invisible one written out as `\u{...}`
/// ([`visible_text`]): what is shown can neither hide a character nor be drawn as anything
/// but text.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PastePreview {
    /// The lines shown, first first, each written out.
    pub lines: Vec<String>,
    /// Some of the text is out of sight, past the lines or the characters shown; all of it
    /// is pasted all the same.
    pub truncated: bool,
}

impl PastePreview {
    /// Most lines shown, as the C# `PreviewMaxLines`.
    pub const MAX_LINES: usize = 50;

    /// Most characters shown, line breaks left out, as the C# `PreviewMaxChars`.
    pub const MAX_CHARS: usize = 4000;

    /// The preview of `text`, its lines broken where a shell would run them: at CR LF, CR
    /// or LF. A break ending the text starts no line of its own.
    #[must_use]
    pub fn of(text: &str) -> Self {
        let body = text
            .strip_suffix("\r\n")
            .or_else(|| text.strip_suffix(['\r', '\n']))
            .unwrap_or(text);
        let mut preview = Self::default();
        let mut room = Self::MAX_CHARS;
        for line in body.split("\r\n").flat_map(|part| part.split(['\r', '\n'])) {
            if preview.lines.len() == Self::MAX_LINES || (room == 0 && !line.is_empty()) {
                preview.truncated = true;
                break;
            }
            let kept: String = line.chars().take(room).collect();
            let count = kept.chars().count();
            room -= count;
            preview.lines.push(visible_text(&kept));
            if count < line.chars().count() {
                preview.truncated = true;
                break;
            }
        }
        preview
    }
}

/// Whether `c` is part of a word, as `\w`.
fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Where `word` stands as a whole word in `line`: not inside a longer word on either side.
fn word_positions<'a>(line: &'a str, word: &'a str) -> impl Iterator<Item = usize> + 'a {
    line.match_indices(word).filter_map(move |(at, _)| {
        let before = line[..at].chars().next_back();
        let after = line[at + word.len()..].chars().next();
        let starts = word.chars().next().is_some_and(is_word);
        let ends = word.chars().next_back().is_some_and(is_word);
        let left = !starts || before.is_none_or(|c| !is_word(c));
        let right = !ends || after.is_none_or(|c| !is_word(c));
        (left && right).then_some(at)
    })
}

fn has_word(line: &str, word: &str) -> bool {
    word_positions(line, word).next().is_some()
}

/// The rest of `line` after each whole `word` followed by blanks.
fn after_word<'a>(line: &'a str, word: &'a str) -> impl Iterator<Item = &'a str> + 'a {
    word_positions(line, word).filter_map(move |at| {
        let rest = &line[at + word.len()..];
        let trimmed = rest.trim_start();
        (trimmed.len() < rest.len()).then_some(trimmed)
    })
}

/// `word`, blanks, then `next` right away.
fn word_then(line: &str, word: &str, next: &str) -> bool {
    after_word(line, word).any(|rest| rest.starts_with(next))
}

/// The runs of word characters right after each `-` of `text`.
fn flag_runs(text: &str) -> impl Iterator<Item = &str> {
    text.match_indices('-').map(move |(at, _)| {
        let rest = &text[at + 1..];
        let end = rest.find(|c: char| !is_word(c)).unwrap_or(rest.len());
        &rest[..end]
    })
}

/// `rm`, then a flag holding `first` and later `then`, as `\brm\s+.*-\w*r\w*f`.
fn rm_flags(line: &str, first: char, then: char) -> bool {
    after_word(line, "rm").any(|rest| {
        flag_runs(rest).any(|run| {
            run.find(first)
                .is_some_and(|at| run[at + first.len_utf8()..].contains(then))
        })
    })
}

/// `format`, blanks, then a drive letter and a colon.
fn format_drive(line: &str) -> bool {
    after_word(line, "format").any(|rest| {
        let mut chars = rest.chars();
        chars.next().is_some_and(|c| c.is_ascii_lowercase()) && chars.next() == Some(':')
    })
}

/// `Remove-Item` with both `-Recurse` and `-Force` after it, each a whole flag after a
/// blank.
fn remove_item_recurse_force(line: &str) -> bool {
    let flag = |rest: &str, name: &str| {
        word_positions(rest, name)
            .any(|at| rest[..at].ends_with('-') && rest[..at - 1].ends_with(char::is_whitespace))
    };
    word_positions(line, "remove-item").any(|at| {
        let rest = &line[at..];
        flag(rest, "recurse") && flag(rest, "force")
    })
}

/// `reg` or `reg.exe`, blanks, then `delete` as a word.
fn reg_delete(line: &str) -> bool {
    ["reg", "reg.exe"].iter().any(|name| {
        after_word(line, name).any(|rest| {
            rest.strip_prefix("delete")
                .is_some_and(|after| after.chars().next().is_none_or(|c| !is_word(c)))
        })
    })
}

/// `init`, blanks, then 0 or 6 standing alone.
fn init_halt(line: &str) -> bool {
    after_word(line, "init").any(|rest| {
        let mut chars = rest.chars();
        chars.next().is_some_and(|c| c == '0' || c == '6')
            && chars.next().is_none_or(|c| !is_word(c))
    })
}

/// `>`, blanks, then `/dev/sd` and a letter.
fn write_to_disk(line: &str) -> bool {
    line.match_indices('>').any(|(at, _)| {
        line[at + 1..]
            .trim_start()
            .strip_prefix("/dev/sd")
            .and_then(|rest| rest.chars().next())
            .is_some_and(|c| c.is_ascii_lowercase())
    })
}

/// `:(){ :|:& };:`, blanks anywhere between its signs.
fn fork_bomb(line: &str) -> bool {
    let packed: String = line.chars().filter(|c| !c.is_whitespace()).collect();
    packed.contains(":(){:|:&};:") || packed.contains(":(){:|:&}:")
}

/// `chmod`, then a flag holding `R`, blanks, then `777` as a word.
fn chmod_recursive_777(line: &str) -> bool {
    after_word(line, "chmod").any(|rest| {
        rest.match_indices('-').any(|(at, _)| {
            let flag = &rest[at + 1..];
            let end = flag.find(|c: char| !is_word(c)).unwrap_or(flag.len());
            let after = &flag[end..];
            let trimmed = after.trim_start();
            flag[..end].contains('r')
                && trimmed.len() < after.len()
                && trimmed
                    .strip_prefix("777")
                    .is_some_and(|tail| tail.chars().next().is_none_or(|c| !is_word(c)))
        })
    })
}

/// `chown`, then a flag ending with `R`.
fn chown_recursive(line: &str) -> bool {
    after_word(line, "chown").any(|rest| flag_runs(rest).any(|run| run.ends_with('r')))
}

/// `fetcher` as a word, then later a pipe into `sh` or `bash`, `sudo` allowed between.
fn fetch_into_shell(line: &str, fetcher: &str) -> bool {
    word_positions(line, fetcher).any(|at| {
        line[at..].match_indices('|').any(|(pipe, _)| {
            let rest = line[at + pipe + 1..].trim_start();
            let rest = rest
                .strip_prefix("sudo")
                .filter(|after| after.starts_with(char::is_whitespace))
                .map_or(rest, str::trim_start);
            ["bash", "sh"].iter().any(|shell| {
                rest.strip_prefix(shell)
                    .is_some_and(|tail| tail.chars().next().is_none_or(|c| !is_word(c)))
            })
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_csharp_command_is_caught_and_its_harmless_lookalike_is_not() {
        let cases = [
            ("rm -rf", "sudo rm -rf /var/lib/app", "rm -r notes.txt"),
            ("rm -fr", "rm -fr ./build", "rm -f stale.lock"),
            ("mkfs", "mkfs.ext4 /dev/sdb1", "mkfsutil --help"),
            ("dd if=", "dd if=/dev/zero of=/dev/sdb", "add if=1"),
            ("format", "format c: /q", "git log --format=%h"),
            ("shutdown", "shutdown -h now", "echo shutdowns"),
            (
                "Remove-Item -Recurse -Force",
                "Remove-Item C:\\data -Recurse -Force",
                "Remove-Item C:\\data -Recurse",
            ),
            ("Stop-Computer", "Stop-Computer -Force", "Get-Computer"),
            (
                "Restart-Computer",
                "restart-computer",
                "Restart-Service w32time",
            ),
            (
                "Format-Volume",
                "Format-Volume -DriveLetter D",
                "Get-Volume",
            ),
            ("Clear-Disk", "Clear-Disk -Number 1", "Clear-Host"),
            (
                "Remove-Partition",
                "Remove-Partition -DiskNumber 1",
                "Get-Partition",
            ),
            (
                "reg delete",
                "reg.exe delete HKLM\\Software\\X /f",
                "reg query HKLM",
            ),
            ("bcdedit", "bcdedit /set safeboot minimal", "bcd editor"),
            ("diskpart", "diskpart.exe /s wipe.txt", "disk part"),
            ("reboot", "sudo reboot", "rebooted fine"),
            ("init 0/6", "init 6", "init 3"),
            ("halt", "halt -p", "halting problem"),
            ("poweroff", "systemctl poweroff", "power off"),
            ("> /dev/sda", "cat image > /dev/sdb", "echo x > /dev/null"),
            (":(){ :|:&};:", ":(){ :|: & };:", ":(){ echo hi; }"),
            ("chmod -R 777", "chmod -R 777 /srv", "chmod 777 file"),
            ("chown -R", "chown -R www:www /srv", "chown www file"),
            (
                "wget | sh",
                "wget -qO- https://x.example | sh",
                "wget https://x.example",
            ),
            (
                "curl | sh",
                "curl -fsSL https://x.example | sudo bash",
                "curl https://x.example | jq .",
            ),
        ];
        assert_eq!(cases.len(), COMMANDS.len(), "one case per command");
        for (label, caught, harmless) in cases {
            assert_eq!(dangerous_command(caught), Some(label), "{caught:?}");
            assert_eq!(dangerous_command(harmless), None, "{harmless:?}");
        }
    }

    #[test]
    fn case_is_ignored_and_a_command_is_found_on_any_line() {
        assert_eq!(dangerous_command("SHUTDOWN /s /t 0"), Some("shutdown"));
        assert_eq!(
            dangerous_command("cd /tmp\nls\r\nrm -rf ./cache\n"),
            Some("rm -rf")
        );
        assert_eq!(dangerous_command("ls -la\npwd\n"), None);
        assert_eq!(dangerous_command(""), None);
    }

    #[test]
    fn a_pattern_never_spans_two_lines() {
        assert_eq!(dangerous_command("curl https://x.example\n| sh"), None);
        assert_eq!(dangerous_command("rm\n-rf"), None);
    }

    #[test]
    fn the_preview_breaks_lines_as_a_shell_and_writes_out_what_would_not_show() {
        let preview = PastePreview::of("ls -l\r\n\x1b[2J\x1b]0;x\x07\rcd\t/tmp\n\nend\n");
        assert_eq!(
            preview.lines,
            [
                "ls -l",
                r"\u{001B}[2J\u{001B}]0;x\u{0007}",
                r"cd\u{0009}/tmp",
                "",
                "end"
            ],
            "no escape sequence left to be drawn, no line after the last break"
        );
        assert!(!preview.truncated);
        assert_eq!(PastePreview::of("a\u{202E}b").lines, [r"a\u{202E}b"]);
    }

    #[test]
    fn the_preview_stops_at_the_csharp_limits_and_says_so() {
        let lines = |count: usize| {
            (0..count)
                .map(|n| format!("echo {n}"))
                .collect::<Vec<_>>()
                .join("\n")
        };
        let whole = PastePreview::of(&lines(PastePreview::MAX_LINES));
        assert_eq!(whole.lines.len(), PastePreview::MAX_LINES);
        assert!(!whole.truncated, "at the limit, all of it shown");
        let cut = PastePreview::of(&lines(PastePreview::MAX_LINES + 1));
        assert_eq!(cut.lines.len(), PastePreview::MAX_LINES);
        assert!(cut.truncated);

        let long = "é".repeat(PastePreview::MAX_CHARS);
        assert!(!PastePreview::of(&long).truncated);
        let longer = PastePreview::of(&format!("{long}x\nnext"));
        assert_eq!(
            longer.lines,
            std::slice::from_ref(&long),
            "cut at a character, never a byte"
        );
        assert!(longer.truncated);
        let filled = PastePreview::of(&format!("{long}\nnext"));
        assert_eq!(filled.lines, [long]);
        assert!(filled.truncated, "a line past the characters shown");
    }
}
