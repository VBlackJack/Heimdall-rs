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

//! The scripts of a Files tab's sudo mode, as the C# Heimdall's sudo listing, sudo delete and
//! chmod fallback: a folder listed, an entry deleted, permissions changed, as root.
//!
//! Each is written as the scripts of [`crate::privileged`] are, and starts as they do: the
//! system's `sudo`, tried without a password, else with the password alone, once. What runs
//! as root is a fixed `sh -c` word holding no single quote; the path and every other value
//! are given to it as arguments of their own (`$1`, `$2`...), never written into its code.
//!
//! - **A listing never parses `ls`**: GNU `find -printf` writes each entry's fields, its name
//!   last, each ended by a NUL, which no name holds. The folder listed comes first, as `pwd
//!   -P` names it once entered, so a link given as the folder lists what it points to.
//! - **A delete removes only what was confirmed**: the entry's kind and inode, as listed when
//!   the user confirmed, are checked again just before; one that changed is left as it is
//!   ([`CHANGED`]). A folder goes with `rm -rf --one-file-system`, anything else with `rm
//!   -f`; neither follows a link. Paths [`deletable_as_root`] refuses never reach it.
//! - **Permissions are never changed through a link**, which `chmod` would follow.
//!
//! Success is the line [`SudoScript::done`] at the end of the output, which the script as
//! written never holds.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::privileged::{Sudo, SudoScript, hex, prelude};
use crate::server_copy::{Unquotable, quote};
use crate::{ItemKind, RemoteItem, Special};

/// Exit status: the entry is no longer the one confirmed, or is gone; it was left as it is.
pub const CHANGED: u32 = crate::privileged::CHANGED;
/// Exit status: the folder to list cannot be entered: it is not a folder, or is not there.
pub const NOT_A_FOLDER: u32 = 74;

/// The system's own top-level folders, which a delete through sudo never removes, nor `/`
/// itself: removed as root, any of them leaves the server unable to start, to be reached or
/// to log anyone in. `/home` is here for itself; each home folder in it is refused too.
pub const PROTECTED_ROOTS: &[&[u8]] = &[
    b"/bin", b"/boot", b"/dev", b"/etc", b"/home", b"/lib", b"/lib64", b"/opt", b"/proc", b"/root",
    b"/run", b"/sbin", b"/srv", b"/sys", b"/usr", b"/var",
];

/// The folder whose every direct child is someone's home folder.
const HOMES: &[u8] = b"/home";

/// Why a path is not deleted as root.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Undeletable {
    /// Not an absolute path.
    NotAbsolute,
    /// A `..` in it: where it leads is not what it says.
    Parent,
    /// `/` or one of [`PROTECTED_ROOTS`].
    SystemFolder,
    /// A home folder itself: one of `/home`'s, or the account's own.
    HomeFolder,
}

/// `path` as a delete through sudo removes it: absolute, its empty and `.` segments dropped;
/// refused for `/`, the [`PROTECTED_ROOTS`], a home folder itself (one in `/home`, or `home`,
/// the account's own) and a path that is not absolute or goes up with `..`.
///
/// # Errors
///
/// [`Undeletable`] says why it is refused.
pub fn deletable_as_root(path: &[u8], home: Option<&[u8]>) -> Result<Vec<u8>, Undeletable> {
    let normal = normalised(path)?;
    if normal == b"/" || PROTECTED_ROOTS.contains(&normal.as_slice()) {
        return Err(Undeletable::SystemFolder);
    }
    let in_homes = normal
        .strip_prefix(HOMES)
        .and_then(|rest| rest.strip_prefix(b"/"))
        .is_some_and(|name| !name.contains(&b'/'));
    let own_home = home
        .and_then(|home| normalised(home).ok())
        .is_some_and(|home| home == normal);
    if in_homes || own_home {
        return Err(Undeletable::HomeFolder);
    }
    Ok(normal)
}

/// `path` absolute, its empty and `.` segments dropped; `/` for the root.
fn normalised(path: &[u8]) -> Result<Vec<u8>, Undeletable> {
    if path.first() != Some(&b'/') {
        return Err(Undeletable::NotAbsolute);
    }
    let mut normal = Vec::with_capacity(path.len());
    for segment in path.split(|byte| *byte == b'/') {
        match segment {
            b"" | b"." => {}
            b".." => return Err(Undeletable::Parent),
            segment => {
                normal.push(b'/');
                normal.extend_from_slice(segment);
            }
        }
    }
    if normal.is_empty() {
        normal.push(b'/');
    }
    Ok(normal)
}

/// What the root side runs to list: `$1` the folder, `$2` the token to say done with. The
/// folder is entered first: what is listed is where that led, named as `pwd -P` names it.
const LIST: &str = concat!(
    "set -eu; unset CDPATH; LC_ALL=C; export LC_ALL; target=$1; token=$2; ",
    "command -v find >/dev/null 2>&1 || { echo \"missing: find\" >&2; exit 77; }; ",
    "case \"$(find --version 2>/dev/null)\" in *GNU*) ;; *) echo \"GNU findutils needed\" >&2; exit 77;; esac; ",
    "cd -P -- \"$target\" 2>/dev/null || exit 74; ",
    "here=$(pwd -P; echo .); here=${here%?.}; ",
    "printf \"%s\\0\" \"$here\"; ",
    "find . -mindepth 1 -maxdepth 1 -printf \"%y\\0%i\\0%m\\0%U\\0%G\\0%s\\0%T@\\0%f\\0\" || :; ",
    "printf \"listed %s\\n\" \"$token\"",
);

/// The fields `find` writes for each entry, its name last.
const FIELDS: usize = 8;

/// What the root side runs to delete: `$1` the entry, `$2` its kind as `find -printf %y`
/// names it (`D` for a device of either kind), `$3` its inode, `$4` the token to say done
/// with.
const REMOVE: &str = concat!(
    "set -eu; LC_ALL=C; export LC_ALL; target=$1; kind=$2; inode=$3; token=$4; ",
    "for t in find rm; do command -v \"$t\" >/dev/null 2>&1 || { echo \"missing: $t\" >&2; exit 77; }; ",
    "case \"$(\"$t\" --version 2>/dev/null)\" in *GNU*) ;; *) echo \"GNU $t needed\" >&2; exit 77;; esac; done; ",
    "found=$(find -P \"$target\" -maxdepth 0 -printf \"%y %i\" 2>/dev/null) || exit 81; ",
    "case \"$found\" in b\\ *|c\\ *) found=\"D ${found#? }\";; esac; ",
    "[ \"$found\" = \"$kind $inode\" ] || exit 81; ",
    "case \"$kind\" in d) rm -rf --one-file-system -- \"$target\";; *) rm -f -- \"$target\";; esac; ",
    "printf \"removed %s\\n\" \"$token\"",
);

/// What the root side runs to change permissions: `$1` the entry, `$2` the octal mode, `$3`
/// the token to say done with. A link is refused: `chmod` would change what it points to.
const CHMOD: &str = concat!(
    "set -eu; unset POSIXLY_CORRECT; target=$1; mode=$2; token=$3; ",
    "if [ -L \"$target\" ]; then exit 73; fi; ",
    "chmod \"$mode\" -- \"$target\"; ",
    "printf \"changed %s\\n\" \"$token\"",
);

/// A sudo script running `root` as root with `arguments`, each one word of `sh` already;
/// done when it writes `done_word` and the token.
fn as_root(
    root: &str,
    arguments: &[&[u8]],
    password: Option<&[u8]>,
    token: &str,
    (done_word, sudo): (&str, Sudo<'_>),
) -> Result<SudoScript, Unquotable> {
    let mut script = prelude(password, sudo)?;
    let mut command = [b"-- sh -c '", root.as_bytes(), b"' sh"].concat();
    for argument in arguments {
        command.push(b' ');
        command.extend_from_slice(argument);
    }
    let mut line = |parts: &[&[u8]]| {
        for part in parts {
            script.extend_from_slice(part);
        }
        script.push(b'\n');
    };
    line(&[b"if [ \"$m\" = S ]; then"]);
    line(&[b"printf '%s\\n' \"$pw\" | \"$s\" -S -k -p '' ", &command]);
    line(&[b"else"]);
    line(&[b"\"$s\" -n ", &command, b" </dev/null"]);
    line(&[b"fi"]);
    Ok(SudoScript {
        script,
        done: format!("{done_word} {token}\n").into_bytes(),
    })
}

/// The script listing `folder` on the server as root, with `password` when the account's
/// sudo asks for one; its output, read by [`list_output`].
///
/// # Errors
///
/// [`Unquotable`] for a path that is empty or holds a control character, and for a password
/// holding a line end.
pub fn list_script(
    folder: &[u8],
    password: Option<&[u8]>,
    token: [u8; 16],
    sudo: Sudo<'_>,
) -> Result<SudoScript, Unquotable> {
    let token = hex(&token);
    let folder = quote(folder)?;
    as_root(
        LIST,
        &[&folder, token.as_bytes()],
        password,
        &token,
        ("listed", sudo),
    )
}

/// The script deleting `target`, an absolute path on the server, as root, only while it is
/// still of `kind` and `inode`; with `password` when the account's sudo asks for one. Paths
/// are checked with [`deletable_as_root`] first: this script does not refuse them itself.
///
/// # Errors
///
/// As [`list_script`].
pub fn remove_script(
    target: &[u8],
    (kind, inode): (ItemKind, u64),
    password: Option<&[u8]>,
    token: [u8; 16],
    sudo: Sudo<'_>,
) -> Result<SudoScript, Unquotable> {
    let token = hex(&token);
    let target = quote(target)?;
    let inode = inode.to_string();
    as_root(
        REMOVE,
        &[
            &target,
            kind_letter(kind).as_bytes(),
            inode.as_bytes(),
            token.as_bytes(),
        ],
        password,
        &token,
        ("removed", sudo),
    )
}

/// The script giving `target`, an absolute path on the server, the permission bits `mode`
/// as root, with `password` when the account's sudo asks for one.
///
/// # Errors
///
/// As [`list_script`].
pub fn chmod_script(
    target: &[u8],
    mode: u32,
    password: Option<&[u8]>,
    token: [u8; 16],
    sudo: Sudo<'_>,
) -> Result<SudoScript, Unquotable> {
    let token = hex(&token);
    let target = quote(target)?;
    // Octal digits only: the permission bits, set-user, set-group and sticky bits included.
    let mode = format!("{:o}", mode & 0o7777);
    as_root(
        CHMOD,
        &[&target, mode.as_bytes(), token.as_bytes()],
        password,
        &token,
        ("changed", sudo),
    )
}

/// `kind` as `find -printf %y` names it; `D` for a device of either kind, `?` for what
/// `find` never names, which no entry then matches.
fn kind_letter(kind: ItemKind) -> &'static str {
    match kind {
        ItemKind::Directory => "d",
        ItemKind::File => "f",
        ItemKind::Link => "l",
        ItemKind::Other(Special::Pipe) => "p",
        ItemKind::Other(Special::Socket) => "s",
        ItemKind::Other(Special::Device) => "D",
        ItemKind::Other(Special::Unknown) => "?",
    }
}

/// An entry of a folder listed as root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListedEntry {
    /// What it is, as a listing over SFTP says it.
    pub item: RemoteItem,
    /// Its inode, which a delete checks again.
    pub inode: u64,
}

/// A folder listed as root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SudoListing {
    /// The folder, as `pwd -P` named it once entered.
    pub folder: Vec<u8>,
    /// Its entries, in the order `find` wrote them.
    pub entries: Vec<ListedEntry>,
}

/// The listing a list script's `output` brings, ended by the line `done`; `None` when it is
/// not, or a record is not whole.
#[must_use]
pub fn list_output(output: &[u8], done: &[u8]) -> Option<SudoListing> {
    let body = output.strip_suffix(done)?.strip_suffix(b"\0")?;
    let mut fields = body.split(|byte| *byte == 0);
    let folder = fields
        .next()
        .filter(|folder| folder.first() == Some(&b'/'))?;
    let fields: Vec<&[u8]> = fields.collect();
    let (records, rest) = fields.as_chunks::<FIELDS>();
    if !rest.is_empty() {
        return None;
    }
    let entries = records
        .iter()
        .map(listed_entry)
        .collect::<Option<Vec<_>>>()?;
    Some(SudoListing {
        folder: folder.to_vec(),
        entries,
    })
}

/// One entry's record: kind, inode, mode, owner, group, size, time, name.
fn listed_entry(record: &[&[u8]; FIELDS]) -> Option<ListedEntry> {
    let [kind, inode, mode, owner, group, size, time, name] = record;
    if name.is_empty() || name.contains(&b'/') {
        return None;
    }
    let number = |text: &[u8]| std::str::from_utf8(text).ok()?.parse::<u64>().ok();
    let small = |text: &[u8]| std::str::from_utf8(text).ok()?.parse::<u32>().ok();
    let kind = match *kind {
        b"d" => ItemKind::Directory,
        b"f" => ItemKind::File,
        b"l" => ItemKind::Link,
        b"p" => ItemKind::Other(Special::Pipe),
        b"s" => ItemKind::Other(Special::Socket),
        b"b" | b"c" => ItemKind::Other(Special::Device),
        _ => ItemKind::Other(Special::Unknown),
    };
    let permissions = u32::from_str_radix(std::str::from_utf8(mode).ok()?, 8).ok()?;
    Some(ListedEntry {
        item: RemoteItem {
            name: name.to_vec(),
            kind,
            size: number(size),
            modified: modified(time),
            permissions: Some(permissions & 0o7777),
            owner: small(owner),
            group: small(group),
        },
        inode: number(inode)?,
    })
}

/// A time as `find -printf %T@` writes it: seconds since 1970, a fraction after a dot.
fn modified(text: &[u8]) -> Option<SystemTime> {
    let text = std::str::from_utf8(text).ok()?;
    let (seconds, fraction) = text.split_once('.').unwrap_or((text, ""));
    let seconds = seconds.parse::<u64>().ok()?;
    let digits: String = fraction
        .chars()
        .chain(std::iter::repeat('0'))
        .take(9)
        .collect();
    let nanos = digits.parse::<u32>().ok()?;
    UNIX_EPOCH.checked_add(Duration::new(seconds, nanos))
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, UNIX_EPOCH};

    use super::{
        CHMOD, LIST, REMOVE, Undeletable, deletable_as_root, list_output, list_script,
        remove_script,
    };
    use crate::privileged::Sudo;
    use crate::{ItemKind, Special};

    #[test]
    fn protected_roots_home_folders_and_paths_that_are_not_plain_are_refused() {
        let refused = |path: &str| deletable_as_root(path.as_bytes(), Some(b"/srv/admin"));
        for path in [
            "/", "//", "/.", "/etc", "/etc/", "//etc/.", "/usr", "/var/", "/home",
        ] {
            assert_eq!(refused(path), Err(Undeletable::SystemFolder), "{path}");
        }
        for path in [
            "/home/user",
            "/home/user/",
            "/home//user/.",
            "/srv/admin",
            "/srv/admin/",
        ] {
            assert_eq!(refused(path), Err(Undeletable::HomeFolder), "{path}");
        }
        for path in ["etc", "home/user", "./x", ""] {
            assert_eq!(refused(path), Err(Undeletable::NotAbsolute), "{path}");
        }
        for path in ["/tmp/../etc", "/home/user/..", "/var/log/../../x"] {
            assert_eq!(refused(path), Err(Undeletable::Parent), "{path}");
        }
        assert_eq!(
            refused("/etc/nginx//sites/./old"),
            Ok(b"/etc/nginx/sites/old".to_vec())
        );
        assert_eq!(
            refused("/home/user/notes"),
            Ok(b"/home/user/notes".to_vec())
        );
        assert_eq!(refused("/srv/admin/old"), Ok(b"/srv/admin/old".to_vec()));
        assert_eq!(refused("/srv"), Err(Undeletable::SystemFolder));
        assert_eq!(
            deletable_as_root(b"/opt/app", None),
            Ok(b"/opt/app".to_vec()),
            "a folder under a protected root is not protected itself"
        );
    }

    #[test]
    fn what_runs_as_root_is_one_quoted_word_and_names_travel_as_arguments() {
        for root in [LIST, REMOVE, CHMOD] {
            assert!(!root.contains('\''), "one single-quoted word: {root}");
        }
        let name = b"/srv/it's $(reboot) `id`; rm -rf /";
        let script = remove_script(
            name,
            (ItemKind::Directory, 42),
            None,
            [0; 16],
            Sudo::Unchecked("/usr/bin/sudo"),
        )
        .expect("script");
        let text = String::from_utf8(script.script).expect("text");
        assert!(
            text.contains(" sh '/srv/it'\\''s $(reboot) `id`; rm -rf /' d 42 "),
            "{text}"
        );
        assert!(
            list_script(b"/srv/a\nb", None, [0; 16], Sudo::System).is_err(),
            "a line end in a path is refused"
        );
    }

    #[test]
    fn a_listing_is_read_from_nul_ended_records_and_a_partial_one_refused() {
        let done = b"listed 00\n";
        let mut output = b"/srv\0".to_vec();
        output.extend_from_slice(
            b"d\x0012\x00755\x000\x000\x004096\x001700000000.5000000000\x00logs\x00",
        );
        output
            .extend_from_slice(b"f\x0013\x004644\x001000\x0010\x005\x001700000001\x00a\nb.txt\x00");
        output.extend_from_slice(b"c\x0014\x00620\x000\x005\x000\x001700000002.25\x00tty\x00");
        output.extend_from_slice(done);
        let listing = list_output(&output, done).expect("listed");
        assert_eq!(listing.folder, b"/srv");
        let [logs, file, tty] = listing.entries.as_slice() else {
            panic!("{listing:?}");
        };
        assert_eq!(
            (logs.item.kind, logs.inode, logs.item.permissions),
            (ItemKind::Directory, 12, Some(0o755))
        );
        assert_eq!(
            logs.item.modified,
            Some(UNIX_EPOCH + Duration::from_millis(1_700_000_000_500))
        );
        assert_eq!(file.item.name, b"a\nb.txt", "a line end in a name is kept");
        assert_eq!(
            (
                file.item.permissions,
                file.item.owner,
                file.item.group,
                file.item.size
            ),
            (Some(0o4644), Some(1000), Some(10), Some(5))
        );
        assert_eq!(tty.item.kind, ItemKind::Other(Special::Device));
        assert_eq!(
            list_output(b"/srv\0", b"").map(|listing| listing.entries.len()),
            Some(0),
            "an empty folder"
        );
        let cut = &output[..output.len() - done.len() - 4];
        assert_eq!(list_output(cut, b""), None, "a record cut short");
        assert_eq!(
            list_output(&output[..output.len() - 1], done),
            None,
            "not done"
        );
        assert_eq!(list_output(b"srv\0listed 00\n", done), None, "not absolute");
    }
}
