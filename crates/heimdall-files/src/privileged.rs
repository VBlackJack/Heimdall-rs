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
//! It needs the coreutils behaviour of GNU, as the C#'s does, but checks the behaviour of the
//! tools it uses and not the name (the C# tests for the GNU banner): uutils, the default
//! coreutils of Ubuntu 25.10 and later, passes, and a tool that really lacks an option
//! (`BusyBox`) stops it ([`TOOLING`]) before anything is written. The delete of
//! [`crate::privileged_mode`] is not so relaxed: it still requires GNU `rm`.
//! After the attributes are copied, the `SELinux` context and the ACLs of the payload are
//! compared with the original's, when the server has them, and a difference is [`METADATA`]. Success is the line [`SudoScript::done`] and nothing else, which the script as
//! written never holds.

use std::fmt::Write as _;

use crate::server_copy::{TOKEN_LEN, Unquotable, quote};

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

/// Defines the `fail` helper of the scripts below and probes `stat`, which both use, by what
/// it does and not by what it calls itself. The GNU banner is no test of the tools used by
/// the read and the replace: uutils (the default coreutils of Ubuntu 25.10 and later) answers
/// the same on each option probed here and says `uutils` instead. A tool that lacks an option, or reads it differently, fails the probe
/// ([`TOOLING`]) before anything is written.
macro_rules! probe_head {
    () => {
        concat!(
            "fail() { echo \"unusable: $1\" >&2; exit 77; }; ",
            "case \"$(stat -c \"%u:%g %a %s\" -- / 2>/dev/null)\" in ",
            "[0-9]*:[0-9]*\\ [0-7]*\\ [0-9]*) ;; *) fail \"stat -c\";; esac; ",
            "[ \"$(stat -Lc %F -- / 2>/dev/null)\" = directory ] || fail \"stat -L\"; ",
        )
    };
}

/// Probes, in the work folder, what only the read needs: hard links and `base64`.
macro_rules! probe_work_read {
    () => {
        concat!(
            "printf x > probe-a; ",
            "ln -P -- probe-a probe-c 2>/dev/null || fail \"ln -P\"; ",
            "[ \"$(printf f | base64 2>/dev/null)\" = \"Zg==\" ] || fail base64; ",
            "rm -f -- probe-a probe-b probe-c probe-d; ",
        )
    };
}

/// Probes, in the work folder, what the replace needs (and leaves it empty): hard links, the attributes-only copy,
/// the rename of a file over another, a flush of one file system, and `sha256sum`.
macro_rules! probe_work_replace {
    () => {
        concat!(
            "printf x > probe-a; chmod 640 probe-a; : > probe-b; printf y > probe-d; ",
            "ln -P -- probe-a probe-c 2>/dev/null || fail \"ln -P\"; ",
            "mv -fT -- probe-c probe-d 2>/dev/null || fail \"mv -T\"; ",
            "sync -f probe-d 2>/dev/null || fail \"sync -f\"; ",
            "cp --attributes-only --preserve=mode,ownership,xattr -- probe-a probe-b 2>/dev/null ",
            "&& [ \"$(stat -c %a -- probe-b)\" = 640 ] && [ \"$(stat -c %s -- probe-b)\" = 0 ] ",
            "|| fail \"cp --attributes-only\"; ",
            "[ \"$(printf abc | sha256sum | cut -d \" \" -f 1)\" = ",
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad ] || fail sha256sum; ",
            "rm -f -- probe-a probe-b probe-c probe-d; ",
        )
    };
}

/// The probe of `stat` on its own, for the tests.
#[cfg(all(test, unix))]
const PROBE_HEAD: &str = probe_head!();
/// The probes run in the work folder by the replace, for the tests.
#[cfg(all(test, unix))]
const PROBE_WORK_REPLACE: &str = probe_work_replace!();

/// What the root side runs: `$1` the file, `$2` the SHA-256 its content must still have,
/// `$3` the token to say done with; the new content on its input. Single quotes never appear
/// in it: it is given whole as one quoted word.
const REPLACE: &str = concat!(
    "set -eu; umask 077; target=$1; expected=$2; token=$3; ",
    "for t in stat cp sync mv ln mktemp chmod chown rm rmdir sha256sum cut cat; do ",
    "command -v \"$t\" >/dev/null 2>&1 || { echo \"missing: $t\" >&2; exit 77; }; done; ",
    probe_head!(),
    "case \"$target\" in */*) dir=${target%/*}; [ -n \"$dir\" ] || dir=/ ;; *) dir=. ;; esac; ",
    "work=$(mktemp -d -- \"$dir/.heimdall-write.XXXXXXXXXX\"); entered=; ",
    "cleanup() { [ -z \"$entered\" ] || rm -f -- original payload probe-a probe-b probe-c probe-d; cd /; rmdir -- \"$work\" 2>/dev/null || :; }; ",
    "trap cleanup EXIT HUP INT TERM; chmod 700 -- \"$work\"; cd -- \"$work\"; entered=1; ",
    probe_work_replace!(),
    "ln -P -- \"$target\" original 2>/dev/null || exit 73; ",
    "if [ -L original ] || [ ! -f original ]; then exit 73; fi; ",
    "[ \"$(sha256sum < original | cut -d \" \" -f 1)\" = \"$expected\" ] || exit 81; ",
    "owner=$(stat -c %u:%g -- original); mode=$(stat -c %a -- original); ",
    "cat > payload; size=$(stat -c %s -- payload); ",
    "chown -- \"$owner\" payload; chmod -- \"$mode\" payload; ",
    "cp --attributes-only --preserve=mode,ownership,xattr -- original payload || exit 76; ",
    "[ \"$(stat -c %s -- payload)\" = \"$size\" ] || exit 76; ",
    "if command -v selinuxenabled >/dev/null 2>&1 && selinuxenabled 2>/dev/null; then ",
    "label=$(stat -c %C -- original); case \"$label\" in \"\"|\"?\") exit 76;; esac; ",
    "[ \"$label\" = \"$(stat -c %C -- payload)\" ] || exit 76; fi; ",
    "if command -v getfacl >/dev/null 2>&1; then ",
    "[ \"$(getfacl -cp -- original 2>/dev/null)\" = \"$(getfacl -cp -- payload 2>/dev/null)\" ] || exit 76; fi; ",
    "sync -f payload; ",
    "if [ -L \"$target\" ]; then exit 73; fi; ",
    "[ \"$(sha256sum < \"$target\" | cut -d \" \" -f 1)\" = \"$expected\" ] || exit 81; ",
    "rm -f -- original; mv -fT -- payload \"$target\"; ",
    "entered=; trap - EXIT HUP INT TERM; cd /; rmdir -- \"$work\" 2>/dev/null || :; sync -f \"$dir\"; ",
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
    token: [u8; TOKEN_LEN],
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
    probe_head!(),
    "case \"$target\" in */*) dir=${target%/*}; [ -n \"$dir\" ] || dir=/ ;; *) dir=. ;; esac; ",
    "work=$(mktemp -d -- \"$dir/.heimdall-read.XXXXXXXXXX\"); entered=; ",
    "cleanup() { [ -z \"$entered\" ] || rm -f -- source probe-a probe-b probe-c probe-d; cd /; rmdir -- \"$work\" 2>/dev/null || :; }; ",
    "trap cleanup EXIT HUP INT TERM; chmod 700 -- \"$work\"; cd -- \"$work\"; entered=1; ",
    probe_work_read!(),
    "ln -P -- \"$target\" source 2>/dev/null || exit 73; ",
    "if [ -L source ] || [ ! -f source ]; then exit 73; fi; ",
    "exec 3< source; rm -f -- source; ",
    "[ \"$(stat -Lc %s /proc/self/fd/3)\" -le \"$cap\" ] || exit 75; ",
    "base64 <&3; exec 3<&-; ",
    "entered=; trap - EXIT HUP INT TERM; cd /; rmdir -- \"$work\" 2>/dev/null || :; ",
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
    token: [u8; TOKEN_LEN],
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
    #[cfg(unix)]
    use super::{PROBE_HEAD, PROBE_WORK_REPLACE, TOOLING};
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

    /// Runs `script` with `sh`, in `dir`, with `stubs` ahead of the system's `PATH`; its exit status.
    #[cfg(unix)]
    fn run_probe(script: &str, dir: &std::path::Path, stubs: Option<&std::path::Path>) -> i32 {
        let path = stubs.map_or_else(
            || "/usr/bin:/bin".to_owned(),
            |stubs| format!("{}:/usr/bin:/bin", stubs.display()),
        );
        std::process::Command::new("/bin/sh")
            .args(["-c", &format!("set -eu; {script}")])
            .current_dir(dir)
            .env("PATH", path)
            .status()
            .expect("sh runs")
            .code()
            .expect("sh exits")
    }

    /// A `stat` that answers the two questions of the probe the way `body` says.
    #[cfg(unix)]
    fn stub_stat(dir: &std::path::Path, body: &str) {
        use std::os::unix::fs::PermissionsExt as _;
        let path = dir.join("stat");
        std::fs::write(
            &path,
            format!(
                "#!/bin/sh
{body}
"
            ),
        )
        .expect("stub written");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    }

    /// The banner is not what decides: a `stat` that behaves as GNU's does passes whatever it
    /// calls itself, as uutils' does on Ubuntu 25.10 and later.
    #[cfg(unix)]
    #[test]
    fn stat_is_probed_by_what_it_does_not_by_its_banner() {
        let behaves = r#"
case "$1" in
  --version) echo "stat (uutils coreutils) 0.2.2" ;;
  -c) [ "$2" = "%u:%g %a %s" ] && echo "0:0 755 4096" ;;
  -Lc) [ "$2" = "%F" ] && echo directory ;;
esac"#;
        let gnu = behaves.replace("(uutils coreutils) 0.2.2", "(GNU coreutils) 9.4");
        for (name, body) in [("uutils", behaves.to_owned()), ("gnu", gnu)] {
            let stubs = tempfile::tempdir().expect("stub folder");
            stub_stat(stubs.path(), &body);
            let work = tempfile::tempdir().expect("work folder");
            assert_eq!(
                run_probe(PROBE_HEAD, work.path(), Some(stubs.path())),
                0,
                "{name}"
            );
        }
    }

    /// A `stat` that lacks an option, or reads it differently, stops the script with
    /// [`TOOLING`], even when its banner says GNU.
    #[cfg(unix)]
    #[test]
    fn a_stat_lacking_an_option_stops_the_script_with_tooling() {
        let lacks = [
            // No `-c`: prints nothing, as a stat that only knows `--version`.
            r#"[ "$1" = --version ] && echo "stat (GNU coreutils) 9.4" || exit 1"#,
            // `-c` reads the format differently (BusyBox-like): wrong fields.
            r#"case "$1" in -c) echo "File: /" ;; -Lc) echo directory ;; esac"#,
            // No `-L` support: a directory is not named.
            r#"case "$1" in -c) echo "0:0 755 4096" ;; *) exit 1 ;; esac"#,
        ];
        for body in lacks {
            let stubs = tempfile::tempdir().expect("stub folder");
            stub_stat(stubs.path(), body);
            let work = tempfile::tempdir().expect("work folder");
            assert_eq!(
                run_probe(PROBE_HEAD, work.path(), Some(stubs.path())),
                i32::try_from(TOOLING).expect("fits"),
                "{body}"
            );
        }
    }

    /// The tools of the server this runs on (GNU, or uutils on a recent Ubuntu) pass the probes
    /// of the replace, in a work folder as the script makes one.
    #[cfg(unix)]
    #[test]
    fn the_servers_own_coreutils_pass_the_probes_of_the_replace() {
        let work = tempfile::tempdir().expect("work folder");
        let script = format!("{PROBE_HEAD}{PROBE_WORK_REPLACE}");
        assert_eq!(run_probe(&script, work.path(), None), 0);
    }

    /// The order the safety of the replace rests on: the work folder is covered by the trap
    /// before it is entered, and the metadata is compared before the rename.
    #[test]
    fn the_trap_comes_first_and_the_metadata_is_checked_before_the_rename() {
        use super::REPLACE;
        let at = |needle: &str| REPLACE.find(needle).unwrap_or_else(|| panic!("{needle}"));
        assert!(at("trap cleanup") < at("cd -- "));
        assert!(at("trap cleanup") < at("chmod 700"));
        assert!(at("stat -c %C") < at("mv -fT -- payload"));
        assert!(REPLACE.contains(r#"case "$label" in ""|"?") exit 76;; esac"#));
        assert!(at("getfacl -cp") < at("mv -fT -- payload"));
        assert!(REPLACE.contains(r#"%C -- payload)" ] || exit 76"#));
    }

    /// The cleanup runs as root: it removes by relative name, and only once the work folder
    /// was entered, never by a path that could be read from elsewhere.
    #[test]
    fn the_cleanup_removes_relative_names_only_once_entered() {
        use super::{READ, REPLACE};
        for script in [REPLACE, READ] {
            let start = script.find("cleanup() {").expect("cleanup");
            let body = &script[start..script[start..].find("}; ").map(|e| start + e).expect("end")];
            assert!(body.contains(r#"[ -z "$entered" ] || rm -f -- "#), "{body}");
            assert!(!body.contains("$work/"), "{body}");
            assert!(
                !body.contains(r#""$work""#) || body.contains("rmdir -- \"$work\""),
                "{body}"
            );
            assert!(
                script.find("entered=1").expect("set")
                    > script.find("cd -- \"$work\"").expect("cd")
            );
            // The flag is cleared and the trap removed before the script leaves the folder.
            let tail = script
                .rfind("entered=; trap - EXIT HUP INT TERM; cd /;")
                .expect("tail");
            assert!(tail > script.find("entered=1").expect("set"));
        }
    }
}
