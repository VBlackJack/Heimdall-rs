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

//! Folder listings: `MLSD` (RFC 3659), the machine format, when the server has it; else
//! `LIST`, meant for people, in the two shapes servers use, Unix `ls -l` and the DOS one of
//! IIS.
//!
//! A name that could step out of a folder once written on this side (`/`, `\`, NUL, `.`,
//! `..`) is dropped, whatever the format.

use std::time::{Duration, SystemTime};

/// What an entry is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    /// A file.
    File,
    /// A folder.
    Directory,
    /// A symbolic link.
    Link,
    /// Anything else the server lists.
    Other,
}

/// One entry of a folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// Its name in the folder.
    pub name: String,
    /// What it is.
    pub kind: EntryKind,
    /// Size in bytes, when the server tells it.
    pub size: Option<u64>,
    /// Last change, when the server tells it unambiguously (`MLSD` only).
    pub modified: Option<SystemTime>,
}

/// The entries of an `MLSD` answer, one per line; lines that do not parse are skipped, as
/// are the folder itself and its parent.
#[must_use]
pub fn parse_mlsd(text: &str) -> Vec<Entry> {
    text.lines().filter_map(mlsd_line).collect()
}

fn mlsd_line(line: &str) -> Option<Entry> {
    let (facts, name) = line.split_once(' ')?;
    let mut kind = EntryKind::Other;
    let mut size = None;
    let mut modified = None;
    for fact in facts.split(';').filter(|fact| !fact.is_empty()) {
        let (key, value) = fact.split_once('=')?;
        match key.to_ascii_lowercase().as_str() {
            "type" => {
                kind = match value.to_ascii_lowercase().as_str() {
                    "file" => EntryKind::File,
                    "dir" => EntryKind::Directory,
                    // The folder itself and its parent.
                    "cdir" | "pdir" => return None,
                    other
                        if other.starts_with("os.unix=symlink")
                            || other.starts_with("os.unix=slink") =>
                    {
                        EntryKind::Link
                    }
                    _ => EntryKind::Other,
                };
            }
            "size" => size = value.parse().ok(),
            "modify" => modified = mlsd_time(value),
            _ => {}
        }
    }
    entry(name, kind, size, modified)
}

/// `YYYYMMDDHHMMSS[.sss]`, in UTC.
fn mlsd_time(value: &str) -> Option<SystemTime> {
    let digits = value.split('.').next()?;
    if digits.len() != 14 || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let number = |range: std::ops::Range<usize>| digits[range].parse::<u32>().ok();
    let (year, month, day) = (number(0..4)?, number(4..6)?, number(6..8)?);
    let (hour, minute, second) = (number(8..10)?, number(10..12)?, number(12..14)?);
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
        || second > 60
    {
        return None;
    }
    let days = days_from_civil(i64::from(year), month, day);
    let seconds = days * 86_400 + i64::from(hour * 3600 + minute * 60 + second);
    u64::try_from(seconds)
        .ok()
        .map(|seconds| SystemTime::UNIX_EPOCH + Duration::from_secs(seconds))
}

/// Days from 1970-01-01 to the given date of the proleptic Gregorian calendar (Howard
/// Hinnant's algorithm).
fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let month = i64::from(month);
    let day_of_year =
        (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + i64::from(day) - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// The entries of a `LIST` answer, Unix or DOS shaped; lines that are neither (a `total`
/// line, a blank one) are skipped.
#[must_use]
pub fn parse_list(text: &str) -> Vec<Entry> {
    text.lines()
        .filter_map(|line| unix_line(line).or_else(|| dos_line(line)))
        .collect()
}

/// `drwxr-xr-x 2 owner group 4096 Sep 27 10:15 name`, the name possibly with spaces, a link
/// shown `name -> target`.
fn unix_line(line: &str) -> Option<Entry> {
    let kind = match line.as_bytes().first()? {
        b'-' => EntryKind::File,
        b'd' => EntryKind::Directory,
        b'l' => EntryKind::Link,
        b'b' | b'c' | b'p' | b's' => EntryKind::Other,
        _ => return None,
    };
    let (fields, name) = take_fields(line, 8)?;
    if fields[0].len() < 10 {
        return None;
    }
    let size = fields[4].parse().ok();
    let name = match kind {
        EntryKind::Link => name.split_once(" -> ").map_or(name, |(link, _)| link),
        _ => name,
    };
    entry(name, kind, size, None)
}

/// `09-27-26  10:15AM       <DIR>          folder` or `...  1234 file`.
fn dos_line(line: &str) -> Option<Entry> {
    let (fields, name) = take_fields(line, 3)?;
    let date_like = fields[0].len() >= 8
        && fields[0]
            .bytes()
            .filter(|b| *b == b'-' || *b == b'/')
            .count()
            == 2;
    let time_like = fields[1].contains(':');
    if !date_like || !time_like {
        return None;
    }
    let (kind, size) = if fields[2].eq_ignore_ascii_case("<DIR>") {
        (EntryKind::Directory, None)
    } else {
        (EntryKind::File, Some(fields[2].parse().ok()?))
    };
    entry(name, kind, size, None)
}

/// The first `count` whitespace-separated fields, and the rest of the line as it is.
fn take_fields(line: &str, count: usize) -> Option<(Vec<&str>, &str)> {
    let mut fields = Vec::with_capacity(count);
    let mut rest = line;
    for _ in 0..count {
        rest = rest.trim_start();
        let end = rest.find(char::is_whitespace)?;
        fields.push(&rest[..end]);
        rest = &rest[end..];
    }
    // One separator between the last field and the name; the name keeps any other space.
    let name = rest
        .strip_prefix(' ')
        .unwrap_or(rest)
        .trim_start_matches(' ');
    (!name.is_empty()).then_some((fields, name))
}

fn entry(
    name: &str,
    kind: EntryKind,
    size: Option<u64>,
    modified: Option<SystemTime>,
) -> Option<Entry> {
    let unsafe_name =
        name.is_empty() || name == "." || name == ".." || name.contains(['/', '\\', '\0']);
    (!unsafe_name).then(|| Entry {
        name: name.to_owned(),
        kind,
        size,
        modified,
    })
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, SystemTime};

    use super::{EntryKind, parse_list, parse_mlsd};

    #[test]
    fn mlsd_facts_are_read_and_the_folder_and_its_parent_skipped() {
        let entries = parse_mlsd(
            "type=cdir;modify=20260927101500; .\r\n\
             type=pdir; ..\r\n\
             type=file;size=1234;modify=20260927101500;perm=r; report 2026.pdf\r\n\
             Type=DIR;Modify=19700101000001; Photos\r\n\
             type=OS.unix=symlink;size=9; latest\r\n",
        );
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["report 2026.pdf", "Photos", "latest"]);
        assert_eq!(entries[0].kind, EntryKind::File);
        assert_eq!(entries[0].size, Some(1234));
        assert_eq!(
            entries[0].modified,
            Some(SystemTime::UNIX_EPOCH + Duration::from_mins(29_841_735))
        );
        assert_eq!(entries[1].kind, EntryKind::Directory);
        assert_eq!(
            entries[1].modified,
            Some(SystemTime::UNIX_EPOCH + Duration::from_secs(1))
        );
        assert_eq!(entries[2].kind, EntryKind::Link);
    }

    #[test]
    fn unix_list_lines_keep_spaces_in_names_and_drop_link_targets() {
        let entries = parse_list(
            "total 12\r\n\
             drwxr-xr-x    2 ftp      ftp          4096 Sep 27 10:15 My Documents\r\n\
             -rw-r--r--    1 ftp      ftp        123456 Jan  1  2025 notes.txt\r\n\
             lrwxrwxrwx    1 ftp      ftp             9 Sep 27 10:15 latest -> notes.txt\r\n",
        );
        let summary: Vec<(&str, EntryKind, Option<u64>)> = entries
            .iter()
            .map(|e| (e.name.as_str(), e.kind, e.size))
            .collect();
        assert_eq!(
            summary,
            [
                ("My Documents", EntryKind::Directory, Some(4096)),
                ("notes.txt", EntryKind::File, Some(123_456)),
                ("latest", EntryKind::Link, Some(9)),
            ]
        );
        assert!(
            entries.iter().all(|e| e.modified.is_none()),
            "a year may be missing"
        );
    }

    #[test]
    fn dos_list_lines_from_iis_are_read_too() {
        let entries = parse_list(
            "09-27-26  10:15AM       <DIR>          Old Reports\r\n\
             09-27-26  10:16AM                 2048 data.csv\r\n",
        );
        let summary: Vec<(&str, EntryKind, Option<u64>)> = entries
            .iter()
            .map(|e| (e.name.as_str(), e.kind, e.size))
            .collect();
        assert_eq!(
            summary,
            [
                ("Old Reports", EntryKind::Directory, None),
                ("data.csv", EntryKind::File, Some(2048)),
            ]
        );
    }

    #[test]
    fn names_that_could_step_out_of_a_folder_are_dropped() {
        let mlsd = parse_mlsd(
            "type=file;size=1; ../../etc/passwd\r\ntype=file;size=1; a\\b\r\ntype=file;size=1; ..\r\n",
        );
        assert!(mlsd.is_empty(), "{mlsd:?}");
        let list = parse_list("-rw-r--r-- 1 u g 1 Sep 27 10:15 ../escape\r\n");
        assert!(list.is_empty(), "{list:?}");
    }
}
