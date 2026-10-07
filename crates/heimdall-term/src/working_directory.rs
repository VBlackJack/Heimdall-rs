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

//! The working folder a shell reports with OSC 7, `ESC ] 7 ; file://host/path`, ended by
//! BEL or by ST (`ESC \`), read as the C# Heimdall's terminal reads it: the host left aside,
//! the path percent-decoded. The emulator ignores this sequence, so the output is scanned
//! for it beside the emulator, a sequence cut between two chunks completed by the next.
//!
//! A report is the server's word: it is kept only when it names an absolute path, valid
//! UTF-8 once decoded, with no control character, and no longer than
//! [`MAX_REPORT_LENGTH`].
//!
//! `ConEmu`'s report, OSC 9;9, `ESC ] 9 ; 9 ; path`, is read the same way, within the same
//! bounds: Windows Terminal's shell integration and the usual Windows prompts send it
//! rather than OSC 7. Its path is a Windows path as it is, not a URL: never
//! percent-decoded, one pair of double quotes around it taken off. It is kept only when
//! absolute, `C:\...` or `C:/...` or `\\server\share\...`, and given with backslashes, so
//! it never starts with `/` as an OSC 7 path always does: a POSIX path is an OSC 7 report.
//! The other OSC 9 forms, such as 9;4 for progress, are no report.
//!
//! On this computer, the path a local shell reports is the folder [`local_folder`] makes of
//! it: on Windows, as [`windows_folder`] reads it, `/C:/Users/x` and `C:\Users\x` naming
//! `C:\Users\x`; elsewhere, as [`unix_folder`] reads it, the path as it is when it is a
//! POSIX one.

use std::path::PathBuf;

/// Escape, which opens every sequence; in an operating system command, it ends the
/// command, as the first byte of ST.
const ESCAPE: u8 = 0x1b;

/// Bell, which ends an operating system command.
const BELL: u8 = 0x07;

/// Cancel, which abandons the sequence begun.
const CANCEL: u8 = 0x18;

/// Substitute, which abandons the sequence begun.
const SUBSTITUTE: u8 = 0x1a;

/// The byte after ESC that opens an operating system command.
const OSC_INTRODUCER: u8 = b']';

/// What ends an operating system command's number.
const PARAMETER_SEPARATOR: u8 = b';';

/// The number of the operating system command reporting the working folder.
const WORKING_DIRECTORY_COMMAND: &[u8] = b"7";

/// The number of `ConEmu`'s operating system commands, among them its working folder report.
const CONEMU_COMMAND: &[u8] = b"9";

/// What starts `ConEmu`'s working folder report, after its command number.
const CONEMU_DIRECTORY: &[u8] = b"9;";

/// What may surround the path of `ConEmu`'s report.
const QUOTE: char = '"';

/// The most digits of a command number read: past them, it is no command looked for.
const MAX_COMMAND_DIGITS: usize = 4;

/// The first byte past the C0 controls, which are all below it.
const FIRST_PRINTABLE: u8 = 0x20;

/// What a report starts with: the scheme of a file URL.
const FILE_SCHEME: &[u8] = b"file://";

/// What starts the path, after the host.
const PATH_SEPARATOR: u8 = b'/';

/// What starts an escaped byte in a URL.
const PERCENT: u8 = b'%';

/// The base of an escaped byte's two digits.
const HEX_RADIX: u32 = 16;

/// The bits of one hexadecimal digit.
const NIBBLE_BITS: u8 = 4;

/// What ends a drive letter in a Windows path.
const DRIVE_SUFFIX: char = ':';

/// What separates the folders of a Windows path.
const WINDOWS_SEPARATOR: char = '\\';

/// What a Windows path to a network share starts with, before the server's name.
const UNC_PREFIX: &str = "\\\\";

/// The longest report read, in bytes, scheme and host included: a path as long as Linux
/// takes one (4096 bytes), each byte escaped as three, and room for the scheme and a host.
/// A longer one is no folder this side would follow, and is dropped whole. An OSC 9;9
/// report is held to the same bound.
pub const MAX_REPORT_LENGTH: usize = 16 * 1024;

/// Where the scanner is in the output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum State {
    /// Outside any operating system command.
    #[default]
    Ground,
    /// Just after ESC.
    Escape,
    /// In an operating system command, reading its number.
    Command,
    /// In a working folder report, reading it.
    Report,
    /// In another operating system command, or in a report too long: skipped to its end.
    Skip,
}

/// Finds the working folders a shell reports in its output, fed chunk by chunk.
#[derive(Debug, Default)]
pub struct WorkingDirectoryScanner {
    state: State,
    /// The number of the command being read.
    command: Vec<u8>,
    /// The report being read, as it came.
    report: Vec<u8>,
}

impl WorkingDirectoryScanner {
    /// A scanner outside any sequence, holding nothing.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The last working folder `bytes` report, when one does and it is valid; a report cut
    /// at its end waits for the next chunk.
    pub fn feed(&mut self, bytes: &[u8]) -> Option<String> {
        let mut last = None;
        for &byte in bytes {
            if let Some(directory) = self.step(byte) {
                last = Some(directory);
            }
        }
        last
    }

    /// Moves on by `byte`; the folder a report it ends names, when valid.
    fn step(&mut self, byte: u8) -> Option<String> {
        match self.state {
            State::Ground => {
                if byte == ESCAPE {
                    self.state = State::Escape;
                }
            }
            State::Escape => {
                self.state = match byte {
                    OSC_INTRODUCER => {
                        self.command.clear();
                        State::Command
                    }
                    ESCAPE => State::Escape,
                    _ => State::Ground,
                };
            }
            State::Command => self.step_command(byte),
            State::Report => return self.step_report(byte),
            State::Skip => {
                self.state = match byte {
                    BELL | CANCEL | SUBSTITUTE => State::Ground,
                    ESCAPE => State::Escape,
                    _ => State::Skip,
                };
            }
        }
        None
    }

    /// Reads `byte` of a command's number. An OSC 9 is read whole as a report would be,
    /// its form told once it ends.
    fn step_command(&mut self, byte: u8) {
        self.state = match byte {
            PARAMETER_SEPARATOR
                if self.command == WORKING_DIRECTORY_COMMAND || self.command == CONEMU_COMMAND =>
            {
                self.report.clear();
                State::Report
            }
            BELL | CANCEL | SUBSTITUTE => State::Ground,
            ESCAPE => State::Escape,
            digit if digit.is_ascii_digit() && self.command.len() < MAX_COMMAND_DIGITS => {
                self.command.push(digit);
                State::Command
            }
            _ => State::Skip,
        };
    }

    /// Reads `byte` of a report; the folder it names once it ends, when valid. ESC ends it
    /// as the emulators do, the ST it opens or whatever follows read as a new sequence.
    fn step_report(&mut self, byte: u8) -> Option<String> {
        match byte {
            BELL => {
                self.state = State::Ground;
                self.finish()
            }
            ESCAPE => {
                self.state = State::Escape;
                self.finish()
            }
            CANCEL | SUBSTITUTE => {
                self.state = State::Ground;
                self.report.clear();
                None
            }
            // Ignored inside the command, as the emulators ignore them.
            control if control < FIRST_PRINTABLE => None,
            _ if self.report.len() >= MAX_REPORT_LENGTH => {
                self.state = State::Skip;
                self.report.clear();
                None
            }
            _ => {
                self.report.push(byte);
                None
            }
        }
    }

    /// The folder the report read names, when valid, read as its command's: OSC 7 or
    /// `ConEmu`'s OSC 9;9; the report let go.
    fn finish(&mut self) -> Option<String> {
        let report = std::mem::take(&mut self.report);
        if self.command == CONEMU_COMMAND {
            conemu_directory(&report)
        } else {
            directory(&report)
        }
    }
}

/// The absolute Windows path `ConEmu`'s report `report` names, with backslashes: after `9;`,
/// the path as it is, never percent-decoded, one pair of double quotes around it taken off.
/// None for another OSC 9 form, such as 9;4 for progress, bytes that are not UTF-8, a
/// control character or a double quote in the path, or a path that is not an absolute
/// Windows one.
fn conemu_directory(report: &[u8]) -> Option<String> {
    let path = std::str::from_utf8(report.strip_prefix(CONEMU_DIRECTORY)?).ok()?;
    let path = path
        .strip_prefix(QUOTE)
        .and_then(|inner| inner.strip_suffix(QUOTE))
        .unwrap_or(path);
    if path
        .chars()
        .any(|character| character.is_control() || character == QUOTE)
    {
        return None;
    }
    windows_absolute(path)
}

/// The absolute path `report` names, as the C# `tryParseOsc7Path`: after the scheme, the
/// host is everything up to the first `/`, left aside; the rest is the path, decoded. None
/// for another scheme, no path, an escape that is not two hexadecimal digits, bytes that
/// are not UTF-8 once decoded, or a control character in it.
fn directory(report: &[u8]) -> Option<String> {
    let rest = report.strip_prefix(FILE_SCHEME)?;
    let start = rest.iter().position(|&byte| byte == PATH_SEPARATOR)?;
    let path = String::from_utf8(percent_decode(&rest[start..])?).ok()?;
    (path.starts_with(char::from(PATH_SEPARATOR)) && !path.chars().any(char::is_control))
        .then_some(path)
}

/// `encoded` with each `%` and its two hexadecimal digits turned into the byte they name;
/// none when an escape is not followed by two digits.
fn percent_decode(encoded: &[u8]) -> Option<Vec<u8>> {
    let mut decoded = Vec::with_capacity(encoded.len());
    let mut bytes = encoded.iter();
    while let Some(&byte) = bytes.next() {
        if byte == PERCENT {
            let high = hex_digit(*bytes.next()?)?;
            let low = hex_digit(*bytes.next()?)?;
            decoded.push((high << NIBBLE_BITS) | low);
        } else {
            decoded.push(byte);
        }
    }
    Some(decoded)
}

/// The value of hexadecimal digit `digit`, either case.
fn hex_digit(digit: u8) -> Option<u8> {
    char::from(digit)
        .to_digit(HEX_RADIX)
        .and_then(|value| u8::try_from(value).ok())
}

/// The folder of this computer reported path `reported` names, as a local shell reports it:
/// read as [`windows_folder`] reads it on Windows, as [`unix_folder`] elsewhere. None when
/// it names no folder of this computer.
#[must_use]
pub fn local_folder(reported: &str) -> Option<PathBuf> {
    #[cfg(windows)]
    let folder = windows_folder(reported);
    #[cfg(not(windows))]
    let folder = unix_folder(reported);
    folder.map(PathBuf::from)
}

/// The Windows path reported path `reported` names. An OSC 7 path is read as a Windows
/// shell writes it in its file URL (RFC 8089): `/C:/Users/x` is `C:\Users\x`, `/c:` and
/// `/c:/` the root of drive `c:`, and `//server/share/x`, from a `file:////server/share/x`
/// report, the network share `\\server\share\x`. An OSC 9;9 path, already a Windows one,
/// is taken as it is. None for any other path: one with no drive letter, as WSL's shells
/// report their Linux folders, or a share with no server or no share named. A
/// `file://server/share/x` report loses its host to the scanner, as the C# leaves it, and
/// arrives as `/share/x`: refused too. Either separator is taken.
#[must_use]
pub fn windows_folder(reported: &str) -> Option<String> {
    let separator = char::from(PATH_SEPARATOR);
    match reported.strip_prefix(separator) {
        // A file URL's path to a share: `//server/share`.
        Some(rest) if rest.starts_with(separator) => windows_absolute(reported),
        // A file URL's path to a drive: `/C:/x`, or `/C:` for its root.
        Some(rest) => {
            let mut chars = rest.chars();
            let bare_drive = chars
                .next()
                .is_some_and(|drive| drive.is_ascii_alphabetic())
                && chars.next() == Some(DRIVE_SUFFIX)
                && chars.as_str().is_empty();
            if bare_drive {
                windows_absolute(&format!("{rest}{WINDOWS_SEPARATOR}"))
            } else {
                windows_absolute(rest)
            }
        }
        // A Windows path as it is, as `ConEmu`'s report gives it.
        None => windows_absolute(reported),
    }
}

/// The absolute Windows path `path` is, with backslashes: a drive's, `C:\...`, or a
/// share's, `\\server\share\...`, either separator taken. None for any other: a relative
/// one, a drive with no separator after it, or a share with no server or no share named.
fn windows_absolute(path: &str) -> Option<String> {
    let path = backslashed(path);
    let absolute = if let Some(share) = path.strip_prefix(UNC_PREFIX) {
        let mut names = share.split(WINDOWS_SEPARATOR);
        let (server, name) = (names.next()?, names.next()?);
        !server.is_empty() && !name.is_empty()
    } else {
        let mut chars = path.chars();
        chars
            .next()
            .is_some_and(|drive| drive.is_ascii_alphabetic())
            && chars.next() == Some(DRIVE_SUFFIX)
            && chars.next() == Some(WINDOWS_SEPARATOR)
    };
    absolute.then_some(path)
}

/// `path` with each `/` turned into the Windows separator.
fn backslashed(path: &str) -> String {
    let separator = char::from(PATH_SEPARATOR);
    path.chars()
        .map(|character| {
            if character == separator {
                WINDOWS_SEPARATOR
            } else {
                character
            }
        })
        .collect()
}

/// The Unix path reported path `reported` names: the path as it is, when absolute. A
/// Windows path, from an OSC 9;9 report, names none.
#[must_use]
pub fn unix_folder(reported: &str) -> Option<String> {
    reported
        .starts_with(char::from(PATH_SEPARATOR))
        .then(|| reported.to_owned())
}
