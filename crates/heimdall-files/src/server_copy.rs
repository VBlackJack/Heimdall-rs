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

//! The script that copies an entry on the server itself, as the C# Heimdall's server-side
//! copy: the bytes never cross the network, and the copy keeps modes and times.
//!
//! The script is written for a POSIX `sh` reading it on its input (`sh -s`): the paths in
//! it are quoted for that shell alone, whatever the account's login shell, which only sees
//! `sh -s`. It needs GNU coreutils (`cp -a`, `ln -T`); elsewhere it fails, and the copy is
//! refused rather than half done.
//!
//! It never replaces anything:
//! - a file is copied to a staging name created exclusively (`set -C`), then published by a
//!   hard link (`ln -T`), which fails on an existing destination; a destination found to be
//!   a link afterwards was raced in, and is removed with status [`RACED`];
//! - a folder is created by `mkdir` without `-p`, which fails on an existing one, then
//!   filled by `cp -a`; a failed fill removes the folder it created.
//!
//! A stop asked of the script (`TERM`, `HUP`) is taken once the running `cp` ends: the
//! staging file or the folder it created is removed.
//!
//! Success is the line [`CopyScript::done`] on the output and nothing else. Its token never
//! appears in the script as written, so a server running something else on every exec (a
//! forced `internal-sftp` exiting 0, a `cat` echoing its input) is never taken for a copy.

use std::fmt::Write as _;

/// Exit status of a file copy whose destination turned out to be a link: something else
/// was put there meanwhile.
pub const RACED: u32 = 99;

/// Exit status of a script stopped by `TERM` or `HUP`, as a shell reports one.
const STOPPED: u32 = 143;

/// What is copied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CopyKind {
    /// A regular file.
    File,
    /// A folder and everything in it.
    Folder,
}

/// A path that cannot be written in the script.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Unquotable {
    /// An empty path.
    #[error("empty path")]
    Empty,
    /// A path holding a control character, a newline among them.
    #[error("control character in a path")]
    Control,
}

/// A copy script, and the line it writes once done.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CopyScript {
    /// The script, for `sh -s` on its input.
    pub script: Vec<u8>,
    /// The output of a finished copy, exactly.
    pub done: Vec<u8>,
}

/// The script copying `source` to `destination`, a `kind` entry, both absolute paths on the
/// server; `token`, random for each copy, names the staging file and marks success.
///
/// # Errors
///
/// [`Unquotable`] when a path is empty or holds a control character.
pub fn copy_script(
    source: &[u8],
    destination: &[u8],
    kind: CopyKind,
    token: [u8; 16],
) -> Result<CopyScript, Unquotable> {
    let hex = token.iter().fold(String::new(), |mut hex, byte| {
        let _ = write!(hex, "{byte:02x}");
        hex
    });
    let source = quote(source)?;
    let target = quote(destination)?;
    let mut staging_path = destination.to_vec();
    staging_path.extend_from_slice(format!(".heimdall-{hex}.part").as_bytes());
    let staging = quote(&staging_path)?;
    // Printed as two arguments joined by printf: the line looked for is never in the
    // script as written.
    let done = format!("copied {hex}\n").into_bytes();
    let report = format!("printf 'copied %s\\n' {hex}\n");
    let mut script = Vec::new();
    let mut line = |parts: &[&[u8]]| {
        for part in parts {
            script.extend_from_slice(part);
        }
        script.push(b'\n');
    };
    // Every node owner-only while written; cp restores the source's modes.
    line(&[b"umask 077"]);
    match kind {
        CopyKind::File => {
            line(&[b"set -C"]);
            line(&[b": > ", &staging, b" || exit $?"]);
            line(&[b"set +C"]);
            line(&[
                b"trap 'rm -f -- ",
                &escaped(&staging),
                format!("; exit {STOPPED}' TERM HUP").as_bytes(),
            ]);
            line(&[
                b"cp -p -- ",
                &source,
                b" ",
                &staging,
                b" && ln -T -- ",
                &staging,
                b" ",
                &target,
            ]);
            line(&[b"status=$?"]);
            line(&[
                b"if [ $status -eq 0 ] && [ -L ",
                &target,
                b" ]; then rm -f -- ",
                &target,
                format!("; status={RACED}; fi").as_bytes(),
            ]);
            line(&[b"rm -f -- ", &staging]);
            line(&[b"[ $status -eq 0 ] || exit $status"]);
        }
        CopyKind::Folder => {
            line(&[b"mkdir -- ", &target, b" || exit $?"]);
            line(&[
                b"trap 'rm -rf -- ",
                &escaped(&target),
                format!("; exit {STOPPED}' TERM HUP").as_bytes(),
            ]);
            line(&[
                b"cp -a -- ",
                &source,
                b"/. ",
                &target,
                b" || { status=$?; rm -rf -- ",
                &target,
                b"; exit $status; }",
            ]);
        }
    }
    script.extend_from_slice(report.as_bytes());
    Ok(CopyScript { script, done })
}

/// `path` as one word of `sh`: in single quotes, each quote of it closed, escaped and
/// opened again.
fn quote(path: &[u8]) -> Result<Vec<u8>, Unquotable> {
    if path.is_empty() {
        return Err(Unquotable::Empty);
    }
    if path.iter().any(u8::is_ascii_control) {
        return Err(Unquotable::Control);
    }
    let mut quoted = vec![b'\''];
    for byte in path {
        if *byte == b'\'' {
            quoted.extend_from_slice(b"'\\''");
        } else {
            quoted.push(*byte);
        }
    }
    quoted.push(b'\'');
    Ok(quoted)
}

/// A word already quoted, written again inside a single-quoted trap action.
fn escaped(quoted: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(quoted.len() * 2);
    for byte in quoted {
        if *byte == b'\'' {
            out.extend_from_slice(b"'\\''");
        } else {
            out.push(*byte);
        }
    }
    out
}

/// Whether `destination` is `source` or inside it: a folder copied into itself never ends.
#[must_use]
pub fn is_same_or_inside(source: &[u8], destination: &[u8]) -> bool {
    let source = source.strip_suffix(b"/").unwrap_or(source);
    destination == source
        || destination
            .strip_prefix(source)
            .is_some_and(|rest| rest.first() == Some(&b'/'))
}
