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

//! The script that replaces a file with sudo, as the C# Heimdall's privileged transfer,
//! for a file the account cannot write itself.
//!
//! It is written for a POSIX `sh` reading it on its input (`sh -s`), and holds everything:
//! the password and the content travel inside the script, never on a command line, in an
//! environment variable or in a here-document a shell could spool to disk; nothing follows
//! the script on the input, which `dash` reads ahead.
//!
//! - **sudo is the system's**: found on a fixed `PATH`, set-user-id and owned by root, or
//!   the script stops ([`UNTRUSTED_SUDO`]).
//! - **The password is tried once, alone**: `sudo -n` first, for an account without one;
//!   else the password, one line, with nothing after it. Then the content goes to a `sudo
//!   -k -S` that reads exactly that line again. A wrong password is one failed attempt,
//!   never the file's lines read as more (the C#'s `sudo -S` did, which can lock an account).
//! - **Nothing is replaced but the file opened**: the root side refuses a link or anything
//!   but a regular file, and a file whose content is no longer the one opened (its SHA-256,
//!   checked before writing and again just before the rename): [`CHANGED`].
//! - **The file keeps what it was**: owner, group, mode and extended attributes (ACLs,
//!   `SELinux` context, capabilities) copied from the file replaced, in a folder of root's own
//!   beside it, then renamed over it in one step. Unlike the C#, its modification time is the
//!   time of the save, so the change can be seen.
//!
//! It needs GNU coreutils, as the C#'s does; elsewhere it stops ([`TOOLING`]) before anything
//! is written. Success is the line [`SudoScript::done`] and nothing else, which the script as
//! written never holds.

use std::fmt::Write as _;

use crate::server_copy::{Unquotable, quote};

/// Exit status: the file is a link, or not a regular file.
pub const NOT_A_FILE: u32 = 73;
/// Exit status: the metadata of the file replaced could not be kept; nothing was replaced.
pub const METADATA: u32 = 76;
/// Exit status: the server lacks a tool of GNU coreutils the script needs.
pub const TOOLING: u32 = 77;
/// Exit status: the `sudo` found is not the system's: not set-user-id root.
pub const UNTRUSTED_SUDO: u32 = 79;
/// Exit status: sudo refused the password, or wants a terminal; its error stream says which.
pub const AUTHENTICATION: u32 = 80;
/// Exit status: the file's content is no longer the one opened; it was left as it is.
pub const CHANGED: u32 = 81;
/// Exit status: sudo wants a password and none was given.
pub const PASSWORD_NEEDED: u32 = 82;

/// The `PATH` the script runs with: the system's own folders only.
const SYSTEM_PATH: &str = "/usr/sbin:/usr/bin:/sbin:/bin";

/// Bytes of the content's base64 written on one line of the script.
const LINE: usize = 64 * 1024;

/// Which `sudo` the script runs.
#[derive(Debug, Clone, Copy)]
pub enum Sudo<'a> {
    /// The system's, checked: set-user-id and owned by root.
    System,
    /// This program, unchecked: a stand-in for tests.
    #[doc(hidden)]
    Unchecked(&'a str),
}

/// A sudo script, and the line it writes once done.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SudoScript {
    /// The script, for `sh -s` on its input.
    pub script: Vec<u8>,
    /// The output of a finished replace, exactly.
    pub done: Vec<u8>,
}

/// What the root side runs: `$1` the file, `$2` the SHA-256 its content must still have,
/// `$3` the token to say done with; the new content on its input. Single quotes never appear
/// in it: it is given whole as one quoted word.
const REPLACE: &str = concat!(
    "set -eu; umask 077; target=$1; expected=$2; token=$3; ",
    "for t in stat cp sync mv ln mktemp chmod chown rm rmdir sha256sum cut cat; do ",
    "command -v \"$t\" >/dev/null 2>&1 || { echo \"missing: $t\" >&2; exit 77; }; done; ",
    "case \"$(stat --version 2>/dev/null)\" in *GNU*) ;; *) echo \"GNU coreutils needed\" >&2; exit 77;; esac; ",
    "case \"$target\" in */*) dir=${target%/*}; [ -n \"$dir\" ] || dir=/ ;; *) dir=. ;; esac; ",
    "work=$(mktemp -d -- \"$dir/.heimdall-write.XXXXXXXXXX\"); chmod 700 -- \"$work\"; cd -- \"$work\"; ",
    "cleanup() { rm -f -- original payload; cd /; rmdir -- \"$work\" 2>/dev/null || :; }; ",
    "trap cleanup EXIT HUP INT TERM; ",
    "ln -P -- \"$target\" original 2>/dev/null || exit 73; ",
    "if [ -L original ] || [ ! -f original ]; then exit 73; fi; ",
    "[ \"$(sha256sum < original | cut -d \" \" -f 1)\" = \"$expected\" ] || exit 81; ",
    "owner=$(stat -c %u:%g -- original); mode=$(stat -c %a -- original); ",
    "cat > payload; size=$(stat -c %s -- payload); ",
    "chown -- \"$owner\" payload; chmod -- \"$mode\" payload; ",
    "cp --attributes-only --preserve=mode,ownership,xattr -- original payload || exit 76; ",
    "[ \"$(stat -c %s -- payload)\" = \"$size\" ] || exit 76; ",
    "sync -f payload; ",
    "if [ -L \"$target\" ]; then exit 73; fi; ",
    "[ \"$(sha256sum < \"$target\" | cut -d \" \" -f 1)\" = \"$expected\" ] || exit 81; ",
    "rm -f -- original; mv -fT -- payload \"$target\"; ",
    "cd /; rmdir -- \"$work\" 2>/dev/null || :; trap - EXIT HUP INT TERM; sync -f \"$dir\"; ",
    "printf \"saved %s\\n\" \"$token\"",
);

/// The script replacing `target`, an absolute path on the server, with `content`, only while
/// its content still has the SHA-256 `expected`; with `password` when the account's sudo
/// asks for one. `token`, random for each save, marks success.
///
/// # Errors
///
/// [`Unquotable`] for a path that is empty or holds a control character, and for a password
/// holding a line end, which sudo would read as two.
pub fn replace_script(
    target: &[u8],
    content: &[u8],
    expected: &[u8; 32],
    password: Option<&[u8]>,
    token: [u8; 16],
    sudo: Sudo<'_>,
) -> Result<SudoScript, Unquotable> {
    let target = quote(target)?;
    let token = hex(&token);
    let mut script = prelude(password, sudo)?;
    let mut line = |text: &[u8]| {
        script.extend_from_slice(text);
        script.push(b'\n');
    };
    // The content, decoded by the server from lines of base64.
    line(b"content() {");
    let encoded = base64(content);
    for chunk in encoded.as_bytes().chunks(LINE) {
        line(&[b"printf %s '", chunk, b"'"].concat());
    }
    line(b"}");
    let replace = [
        b"-- sh -c '",
        REPLACE.as_bytes(),
        b"' sh ",
        target.as_slice(),
        format!(" {} {token}", hex(expected)).as_bytes(),
    ]
    .concat();
    line(b"if [ \"$m\" = S ]; then");
    line(
        &[
            b"{ printf '%s\\n' \"$pw\"; content | base64 -d; } | \"$s\" -S -k -p '' ",
            replace.as_slice(),
        ]
        .concat(),
    );
    line(b"else");
    line(&[b"content | base64 -d | \"$s\" -n ", replace.as_slice()].concat());
    line(b"fi");
    Ok(SudoScript {
        script,
        done: format!("saved {token}\n").into_bytes(),
    })
}

/// The start of every sudo script: the system's `PATH` and sudo, then sudo asked without a
/// password, else with `password` alone, once; `m` says how it answered (`n` or `S`).
pub(crate) fn prelude(password: Option<&[u8]>, sudo: Sudo<'_>) -> Result<Vec<u8>, Unquotable> {
    if password.is_some_and(|password| password.contains(&b'\n') || password.contains(&b'\r')) {
        return Err(Unquotable::Control);
    }
    let mut script = Vec::new();
    let mut line = |text: &[u8]| {
        script.extend_from_slice(text);
        script.push(b'\n');
    };
    line(format!("PATH={SYSTEM_PATH}; export PATH").as_bytes());
    match sudo {
        Sudo::System => {
            line(format!("s=$(command -v sudo) || exit {UNTRUSTED_SUDO}").as_bytes());
            line(
                format!(
                    "[ -u \"$s\" ] && [ \"$(stat -c %u -- \"$s\")\" = 0 ] || exit {UNTRUSTED_SUDO}"
                )
                .as_bytes(),
            );
        }
        Sudo::Unchecked(path) => {
            let path = quote(path.as_bytes())?;
            line(&[b"s=", path.as_slice()].concat());
        }
    }
    match password {
        Some(password) => {
            line(format!("pw=$(printf %s '{}' | base64 -d)", base64(password)).as_bytes());
        }
        None => line(b"pw="),
    }
    // Without a password first; else the password alone, one line, nothing after it.
    line(b"if \"$s\" -n true </dev/null 2>/dev/null; then m=n");
    line(format!("elif [ -z \"$pw\" ]; then exit {PASSWORD_NEEDED}").as_bytes());
    line(
        format!("elif printf '%s\\n' \"$pw\" | \"$s\" -S -k -p '' true; then m=S; else exit {AUTHENTICATION}; fi")
            .as_bytes(),
    );
    Ok(script)
}

/// What the root side runs to read: `$1` the file, `$2` the most bytes it may hold, `$3` the
/// token to say done with. The file is held open by a link of its own, never followed, so
/// what is read is the file found, all of it, whatever is renamed meanwhile.
const READ: &str = concat!(
    "set -eu; umask 077; target=$1; cap=$2; token=$3; ",
    "for t in stat ln mktemp chmod rm rmdir base64; do ",
    "command -v \"$t\" >/dev/null 2>&1 || { echo \"missing: $t\" >&2; exit 77; }; done; ",
    "case \"$(stat --version 2>/dev/null)\" in *GNU*) ;; *) echo \"GNU coreutils needed\" >&2; exit 77;; esac; ",
    "case \"$target\" in */*) dir=${target%/*}; [ -n \"$dir\" ] || dir=/ ;; *) dir=. ;; esac; ",
    "work=$(mktemp -d -- \"$dir/.heimdall-read.XXXXXXXXXX\"); chmod 700 -- \"$work\"; cd -- \"$work\"; ",
    "cleanup() { rm -f -- source; cd /; rmdir -- \"$work\" 2>/dev/null || :; }; ",
    "trap cleanup EXIT HUP INT TERM; ",
    "ln -P -- \"$target\" source 2>/dev/null || exit 73; ",
    "if [ -L source ] || [ ! -f source ]; then exit 73; fi; ",
    "exec 3< source; rm -f -- source; ",
    "[ \"$(stat -Lc %s /proc/self/fd/3)\" -le \"$cap\" ] || exit 75; ",
    "base64 <&3; exec 3<&-; ",
    "cd /; rmdir -- \"$work\" 2>/dev/null || :; trap - EXIT HUP INT TERM; ",
    "printf \"read %s\\n\" \"$token\"",
);

/// Exit status: the file is larger than what is read.
pub const TOO_LARGE: u32 = 75;

/// The script reading `target`, an absolute path on the server, of `cap` bytes at most, with
/// `password` when the account's sudo asks for one; its output is the content in base64,
/// then the line [`SudoScript::done`], which [`read_output`] takes apart.
///
/// # Errors
///
/// As [`replace_script`].
pub fn read_script(
    target: &[u8],
    cap: u64,
    password: Option<&[u8]>,
    token: [u8; 16],
    sudo: Sudo<'_>,
) -> Result<SudoScript, Unquotable> {
    let target = quote(target)?;
    let token = hex(&token);
    let mut script = prelude(password, sudo)?;
    let mut line = |text: &[u8]| {
        script.extend_from_slice(text);
        script.push(b'\n');
    };
    let read = [
        b"-- sh -c '",
        READ.as_bytes(),
        b"' sh ",
        target.as_slice(),
        format!(" {cap} {token}").as_bytes(),
    ]
    .concat();
    line(b"if [ \"$m\" = S ]; then");
    line(
        &[
            b"printf '%s\\n' \"$pw\" | \"$s\" -S -k -p '' ",
            read.as_slice(),
        ]
        .concat(),
    );
    line(b"else");
    line(&[b"\"$s\" -n ", read.as_slice(), b" </dev/null"].concat());
    line(b"fi");
    Ok(SudoScript {
        script,
        done: format!("read {token}\n").into_bytes(),
    })
}

/// The content a read script's `output` brings: its base64 before the line `done`, which
/// must end it; `None` when it does not, or the base64 is not.
#[must_use]
pub fn read_output(output: &[u8], done: &[u8]) -> Option<Vec<u8>> {
    let encoded = output.strip_suffix(done)?;
    let mut out = Vec::with_capacity(encoded.len() / 4 * 3);
    let mut group = 0u32;
    let mut held = 0;
    let mut padding = 0;
    for byte in encoded.iter().filter(|byte| !byte.is_ascii_whitespace()) {
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => {
                padding += 1;
                0
            }
            _ => return None,
        };
        if padding > 0 && *byte != b'=' {
            return None;
        }
        group = group << 6 | u32::from(value);
        held += 1;
        if held == 4 {
            let bytes = group.to_be_bytes();
            out.extend_from_slice(&bytes[1..4 - padding.min(2)]);
            group = 0;
            held = 0;
        }
    }
    (held == 0).then_some(out)
}

/// `bytes` in lowercase hexadecimal.
pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut out, byte| {
        let _ = write!(out, "{byte:02x}");
        out
    })
}

/// `bytes` in standard base64, padded: what `base64 -d` reads.
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for group in bytes.chunks(3) {
        let triple = group.iter().enumerate().fold(0u32, |all, (index, byte)| {
            all | u32::from(*byte) << (16 - 8 * index)
        });
        for index in 0..4 {
            if index <= group.len() {
                let sextet = (triple >> (18 - 6 * index)) & 0x3f;
                let sextet = usize::try_from(sextet).unwrap_or_default();
                out.push(char::from(ALPHABET[sextet]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{base64, read_output};

    #[test]
    fn read_output_is_the_base64_before_the_line_that_ends_it() {
        let done = b"read 00\n";
        for content in [
            &b""[..],
            b"f",
            b"fo",
            b"foo",
            b"foobar",
            &[0xff, 0xfe, 0x00, 0x01],
        ] {
            let mut output = base64(content).into_bytes();
            output.push(b'\n');
            output.extend_from_slice(done);
            assert_eq!(read_output(&output, done).as_deref(), Some(content));
        }
        assert_eq!(read_output(b"Zm9v\n", done), None, "not done");
        assert_eq!(read_output(b"Zm9!\nread 00\n", done), None, "not base64");
        assert_eq!(read_output(b"Zm9\nread 00\n", done), None, "cut short");
    }

    #[test]
    fn base64_is_the_standard_padded_one() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
        assert_eq!(base64(&[0xff, 0xfe, 0x00]), "//4A");
    }
}
