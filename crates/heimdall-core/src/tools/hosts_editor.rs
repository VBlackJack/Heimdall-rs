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

//! The system hosts file, read and written back by the Hosts File Editor.
//!
//! The C# view (`HostsFileEditorView.xaml.cs`, `SaveHostsFile`) rebuilt the whole file from
//! its grid: aliases, comments between entries and the original line endings were lost on
//! every save. Here the file is kept line by line as it was read, bytes included, and only a
//! line the user changed or added is written anew; a save refuses a file changed on disk
//! since it was read, keeps a backup copy beside it, and replaces it through a temporary file,
//! so an interrupted save never leaves a truncated hosts file behind.

use std::collections::BTreeSet;
use std::fs;
use std::io::{self, Write as _};
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use thiserror::Error;

use crate::tools::date_time::civil_from_days;

/// Byte order mark some editors put at the head of a UTF-8 hosts file: kept as it was.
const UTF8_BOM: &[u8] = b"\xEF\xBB\xBF";

/// Character that starts a comment, and a disabled entry.
const COMMENT_MARK: char = '#';

/// The comment mark as a byte, for the lines kept as read.
const COMMENT_BYTE: u8 = b'#';

/// What a disabled entry is prefixed with, as the C# writes it.
const DISABLED_PREFIX: &str = "# ";

/// Separator between the address, the names and the comment of a line written anew, as the C#.
const FIELD_SEPARATOR: char = '\t';

/// Longest host name a hosts file entry takes, in characters.
const MAX_HOST_NAME_LENGTH: usize = 255;

/// Characters a host name holds besides letters and digits.
const HOST_NAME_PUNCTUATION: [char; 3] = ['-', '.', '_'];

/// Extension of the backup copy written before a save.
const BACKUP_EXTENSION: &str = "bak";

/// Backup names tried when one of the same second is already there.
const MAX_BACKUP_ATTEMPTS: u32 = 100;

/// Prefix of the temporary file a save is written to, beside the hosts file.
const TEMP_PREFIX: &str = ".hosts-heimdall-";

/// Seconds in a day, minute and hour, for the backup's timestamp.
const SECONDS_PER_DAY: u64 = 86_400;
const SECONDS_PER_HOUR: u64 = 3_600;
const SECONDS_PER_MINUTE: u64 = 60;

/// A single mapping of the hosts file: active, or disabled by a comment mark.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostEntry {
    /// Whether the mapping is active (true) or commented out (false).
    pub enabled: bool,
    /// Destination IP address (IPv4 or IPv6).
    pub ip: String,
    /// First host name mapped to the address.
    pub host: String,
    /// The other names on the same line, kept as they were.
    pub aliases: Vec<String>,
    /// Inline comment, without its mark.
    pub comment: String,
}

impl HostEntry {
    /// The host name and its aliases, as the line lists them.
    #[must_use]
    pub fn names(&self) -> String {
        std::iter::once(self.host.as_str())
            .chain(self.aliases.iter().map(String::as_str))
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// The entry as a line written anew: address, names and comment separated by a tab, as
    /// the C# `SaveHostsFile`.
    fn to_line(&self) -> String {
        let mut line = String::new();
        if !self.enabled {
            line.push_str(DISABLED_PREFIX);
        }
        line.push_str(&self.ip);
        line.push(FIELD_SEPARATOR);
        line.push_str(&self.names());
        if !self.comment.is_empty() {
            line.push(FIELD_SEPARATOR);
            line.push(COMMENT_MARK);
            line.push(' ');
            line.push_str(&self.comment);
        }
        line
    }
}

/// Why an entry typed in was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum EntryError {
    /// The address is not an IPv4 or IPv6 address.
    #[error("the address is not an IP address")]
    InvalidIp,
    /// No host name was given.
    #[error("no host name was given")]
    MissingHost,
    /// The host name holds a space, a comment mark or another character a name cannot hold.
    #[error("the host name is not valid")]
    InvalidHost,
    /// The comment holds a line break or another control character.
    #[error("the comment holds a control character")]
    InvalidComment,
}

/// Why the hosts file could not be read or written.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum HostsError {
    /// There is no hosts file where it was looked for.
    #[error("the hosts file was not found")]
    NotFound,
    /// Reading the hosts file was refused.
    #[error("reading the hosts file was refused")]
    ReadDenied,
    /// The hosts file could not be read: the system's message.
    #[error("the hosts file could not be read: {0}")]
    Read(String),
    /// Writing beside the hosts file was refused: administrator rights are needed.
    #[error("writing the hosts file needs administrator rights")]
    AdminRequired,
    /// The file on disk is no longer what was loaded: nothing was written.
    #[error("the hosts file changed on disk since it was loaded")]
    ChangedOnDisk,
    /// The backup copy could not be written: nothing else was.
    #[error("the backup copy could not be written: {0}")]
    Backup(String),
    /// The new content could not be written: the file is as it was.
    #[error("the hosts file could not be written: {0}")]
    Write(String),
}

/// How a line ended in the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ending {
    /// Carriage return and line feed, as Windows writes them.
    CrLf,
    /// Line feed alone.
    Lf,
    /// The last line, without a line break.
    None,
}

impl Ending {
    fn bytes(self) -> &'static [u8] {
        match self {
            Self::CrLf => b"\r\n",
            Self::Lf => b"\n",
            Self::None => b"",
        }
    }
}

/// One line of the file: its bytes as read, without its line break, and the entry it holds.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Line {
    bytes: Vec<u8>,
    ending: Ending,
    entry: Option<HostEntry>,
}

impl Line {
    fn new(bytes: Vec<u8>, ending: Ending) -> Self {
        let entry = parse_entry(&String::from_utf8_lossy(&bytes));
        Self {
            bytes,
            ending,
            entry,
        }
    }
}

/// The hosts file as read, line by line: what the editor does not show (comments, blank
/// lines, lines it does not understand, bytes that are not UTF-8) is written back unchanged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostsDocument {
    bom: bool,
    lines: Vec<Line>,
    /// The line break of a line added: the file's own.
    ending: Ending,
}

impl Default for HostsDocument {
    fn default() -> Self {
        Self::parse(b"")
    }
}

impl HostsDocument {
    /// Reads `bytes`, whatever their encoding: a line that is not UTF-8 is shown as read
    /// lossily but kept byte for byte.
    #[must_use]
    pub fn parse(bytes: &[u8]) -> Self {
        let (bom, mut rest) = match bytes.strip_prefix(UTF8_BOM) {
            Some(rest) => (true, rest),
            None => (false, bytes),
        };
        let mut lines = Vec::new();
        let mut first_ending = None;
        while !rest.is_empty() {
            let Some(end) = rest.iter().position(|&byte| byte == b'\n') else {
                lines.push(Line::new(rest.to_vec(), Ending::None));
                break;
            };
            let (text, ending) = match rest[..end].strip_suffix(b"\r") {
                Some(text) => (text, Ending::CrLf),
                None => (&rest[..end], Ending::Lf),
            };
            first_ending.get_or_insert(ending);
            lines.push(Line::new(text.to_vec(), ending));
            rest = &rest[end + 1..];
        }
        Self {
            bom,
            lines,
            ending: first_ending.unwrap_or(if cfg!(windows) {
                Ending::CrLf
            } else {
                Ending::Lf
            }),
        }
    }

    /// The file's bytes: every line not changed exactly as it was read.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        if self.bom {
            bytes.extend_from_slice(UTF8_BOM);
        }
        for line in &self.lines {
            bytes.extend_from_slice(&line.bytes);
            bytes.extend_from_slice(line.ending.bytes());
        }
        bytes
    }

    /// The entries, active and disabled, in the file's order.
    pub fn entries(&self) -> impl Iterator<Item = &HostEntry> {
        self.lines.iter().filter_map(|line| line.entry.as_ref())
    }

    /// The line of entry `index`.
    fn entry_line(&mut self, index: usize) -> Option<&mut Line> {
        self.lines
            .iter_mut()
            .filter(|line| line.entry.is_some())
            .nth(index)
    }

    /// Activates entry `index` when disabled, disables it when active: only its comment mark
    /// is added or taken away, the rest of its line stays as it was.
    pub fn toggle(&mut self, index: usize) {
        let Some(line) = self.entry_line(index) else {
            return;
        };
        let Some(enabled) = line.entry.as_ref().map(|entry| entry.enabled) else {
            return;
        };
        let bytes = if enabled {
            [DISABLED_PREFIX.as_bytes(), &line.bytes].concat()
        } else {
            let text = line.bytes.trim_ascii_start();
            let start = text
                .iter()
                .position(|&byte| byte != COMMENT_BYTE)
                .unwrap_or(text.len());
            text[start..].trim_ascii_start().to_vec()
        };
        let toggled = Line::new(bytes, line.ending);
        // The same entry, its state turned: anything else leaves the line as it was.
        if toggled.entry.as_ref().map(|entry| entry.enabled) == Some(!enabled) {
            *line = toggled;
        }
    }

    /// Takes entry `index` and its line out of the file.
    pub fn remove(&mut self, index: usize) {
        self.remove_all(&BTreeSet::from([index]));
    }

    /// Takes the entries of `indices` and their lines out of the file at once, as the C#
    /// `OnDeleteSelectedClick` removes every row selected: every other line stays as it was.
    pub fn remove_all(&mut self, indices: &BTreeSet<usize>) {
        let mut entry = 0;
        self.lines.retain(|line| {
            if line.entry.is_none() {
                return true;
            }
            let keep = !indices.contains(&entry);
            entry += 1;
            keep
        });
    }

    /// Changes the address, the names and the comment of entry `index` in place, as the C#
    /// grid's cells edit them (`HostsFileEditorView.xaml:128-132`), checked as an entry
    /// added is. `names` holds the host name and its aliases, separated by spaces. The entry
    /// keeps its state and its place; only its line is written anew, and not even that when
    /// nothing changed. Every other line stays byte for byte as it was.
    ///
    /// # Errors
    ///
    /// The address is not an IP address, no host name is given, a name holds a character a
    /// name cannot hold, or the comment holds a control character: the file is unchanged.
    pub fn edit(
        &mut self,
        index: usize,
        ip: &str,
        names: &str,
        comment: &str,
    ) -> Result<(), EntryError> {
        let Some(line) = self.entry_line(index) else {
            return Ok(());
        };
        let Some(current) = line.entry.as_ref() else {
            return Ok(());
        };
        let mut words = names.split_whitespace();
        let host = words.next().unwrap_or_default();
        let mut entry = checked_entry(ip, host, comment)?;
        entry.aliases = words.map(str::to_owned).collect();
        if !entry.aliases.iter().all(|alias| is_valid_host_name(alias)) {
            return Err(EntryError::InvalidHost);
        }
        entry.enabled = current.enabled;
        if &entry == current {
            return Ok(());
        }
        let edited = Line::new(entry.to_line().into_bytes(), line.ending);
        // A line written anew reads back as the entry it was made from.
        if edited.entry.as_ref() != Some(&entry) {
            return Err(EntryError::InvalidHost);
        }
        *line = edited;
        Ok(())
    }

    /// Adds an active entry at the end of the file, on a line of its own ending as the file's
    /// lines do.
    ///
    /// # Errors
    ///
    /// The address is not an IP address, the host name is missing or holds a space, a comment
    /// mark or another character a name cannot hold, or the comment holds a control character.
    pub fn add(&mut self, ip: &str, host: &str, comment: &str) -> Result<(), EntryError> {
        let entry = checked_entry(ip, host, comment)?;
        if let Some(last) = self.lines.last_mut()
            && last.ending == Ending::None
        {
            last.ending = self.ending;
        }
        let line = Line::new(entry.to_line().into_bytes(), self.ending);
        // A line written anew reads back as the entry it was made from.
        if line.entry.as_ref() != Some(&entry) {
            return Err(EntryError::InvalidHost);
        }
        self.lines.push(line);
        Ok(())
    }
}

/// The entry typed in, checked.
fn checked_entry(ip: &str, host: &str, comment: &str) -> Result<HostEntry, EntryError> {
    let ip = ip.trim();
    if !validate_ip(ip) {
        return Err(EntryError::InvalidIp);
    }
    let host = host.trim();
    if host.is_empty() {
        return Err(EntryError::MissingHost);
    }
    if !is_valid_host_name(host) {
        return Err(EntryError::InvalidHost);
    }
    let comment = comment.trim();
    if comment.chars().any(char::is_control) {
        return Err(EntryError::InvalidComment);
    }
    Ok(HostEntry {
        enabled: true,
        ip: ip.to_owned(),
        host: host.to_owned(),
        aliases: Vec::new(),
        comment: comment.to_owned(),
    })
}

/// Whether `ip` is a valid IPv4 or IPv6 address.
#[must_use]
pub fn validate_ip(ip: &str) -> bool {
    ip.trim().parse::<IpAddr>().is_ok()
}

/// Whether `name` can stand as a host name of an entry: letters, digits, dots, hyphens and
/// underscores, never a space or a comment mark that would change what the line means.
#[must_use]
pub fn is_valid_host_name(name: &str) -> bool {
    !name.is_empty()
        && name.chars().count() <= MAX_HOST_NAME_LENGTH
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || HOST_NAME_PUNCTUATION.contains(&c))
}

/// The entry `text` holds: an address and one or more host names, active, or disabled behind
/// comment marks. A comment whose words are not an address and host names (prose that
/// happens to start with an address, for instance) is no entry and stays a comment.
fn parse_entry(text: &str) -> Option<HostEntry> {
    let text = text.trim();
    let (enabled, body) = match text.strip_prefix(COMMENT_MARK) {
        Some(rest) => (false, rest.trim_start_matches(COMMENT_MARK).trim()),
        None => (true, text),
    };
    let (mapping, comment) = body.split_once(COMMENT_MARK).unwrap_or((body, ""));
    let mut words = mapping.split_whitespace();
    let ip = words.next()?;
    if !validate_ip(ip) {
        return None;
    }
    let names: Vec<&str> = words.collect();
    let (host, aliases) = names.split_first()?;
    if !names.iter().all(|name| is_valid_host_name(name)) {
        return None;
    }
    Some(HostEntry {
        enabled,
        ip: ip.to_owned(),
        host: (*host).to_owned(),
        aliases: aliases.iter().map(|&alias| alias.to_owned()).collect(),
        comment: comment.trim().to_owned(),
    })
}

/// The system hosts file path for the target platform.
#[must_use]
pub fn default_path() -> PathBuf {
    #[cfg(windows)]
    {
        // The system folder as Windows says where it is, never `SystemRoot` from the
        // environment, as the C# `Environment.SpecialFolder.System`.
        crate::paths::system_dir()
            .unwrap_or_else(|| PathBuf::from(r"C:\Windows\System32"))
            .join("drivers")
            .join("etc")
            .join("hosts")
    }
    #[cfg(not(windows))]
    {
        PathBuf::from("/etc/hosts")
    }
}

/// The hosts file as read: its bytes, kept to compare the disk with before a save, and the
/// document made of them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedHosts {
    /// The bytes read.
    pub original: Vec<u8>,
    /// The lines and entries they hold.
    pub document: HostsDocument,
    /// When the file was last written, as the C# `File.GetLastWriteTime` shows it; `None`
    /// when the system does not say.
    pub modified: Option<SystemTime>,
}

/// What a save did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedHosts {
    /// The bytes now on disk.
    pub written: Vec<u8>,
    /// The backup copy of the file as it was; `None` when nothing changed and nothing was
    /// written.
    pub backup: Option<PathBuf>,
    /// When the file was last written, read again after the save; `None` when the system
    /// does not say.
    pub modified: Option<SystemTime>,
}

/// Reads the hosts file at `path`, whatever its encoding.
///
/// # Errors
///
/// The file is missing, reading it is refused, or it cannot be read.
pub fn load(path: &Path) -> Result<LoadedHosts, HostsError> {
    let original = fs::read(path).map_err(read_error)?;
    let document = HostsDocument::parse(&original);
    Ok(LoadedHosts {
        original,
        document,
        modified: modified_time(path),
    })
}

/// When the file at `path` was last written, when the system says.
fn modified_time(path: &Path) -> Option<SystemTime> {
    fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
}

/// Writes `document` to the hosts file at `path`, which must still hold `original`, the
/// bytes it was loaded from: a backup copy of them is written beside it first, then the new
/// content goes to a temporary file in the same folder, which replaces the hosts file at
/// once. `now` names the backup.
///
/// # Errors
///
/// The file changed on disk since it was loaded, writing beside it is refused, or the backup
/// or the new content could not be written: the hosts file is then as it was.
pub fn save(
    path: &Path,
    original: &[u8],
    document: &HostsDocument,
    now: SystemTime,
) -> Result<SavedHosts, HostsError> {
    // A link is followed, so that the file it names is replaced and the link kept.
    let target = match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            fs::canonicalize(path).map_err(read_error)?
        }
        _ => path.to_owned(),
    };
    let current = fs::read(&target).map_err(read_error)?;
    if current != original {
        return Err(HostsError::ChangedOnDisk);
    }
    let written = document.to_bytes();
    if written == current {
        return Ok(SavedHosts {
            written,
            backup: None,
            modified: modified_time(&target),
        });
    }
    let folder = target
        .parent()
        .filter(|folder| !folder.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let name = target.file_name().map_or_else(
        || "hosts".into(),
        |name| name.to_string_lossy().into_owned(),
    );
    let backup = write_backup(folder, &name, &current, now)?;

    let mut temp = tempfile::Builder::new()
        .prefix(TEMP_PREFIX)
        .tempfile_in(folder)
        .map_err(write_error)?;
    temp.write_all(&written).map_err(write_error)?;
    temp.as_file().sync_all().map_err(write_error)?;
    // The new file keeps the old one's permissions (its mode, on Unix).
    if let Ok(metadata) = fs::metadata(&target) {
        fs::set_permissions(temp.path(), metadata.permissions()).map_err(write_error)?;
    }
    // Read again at the last moment: another program's write since is never overwritten.
    if fs::read(&target).map_err(read_error)? != original {
        return Err(HostsError::ChangedOnDisk);
    }
    temp.persist(&target)
        .map_err(|error| write_error(error.error))?;
    Ok(SavedHosts {
        written,
        backup: Some(backup),
        modified: modified_time(&target),
    })
}

/// Writes `content` to a new backup file in `folder`, named after `name` and `now` (UTC);
/// one already there is never overwritten.
fn write_backup(
    folder: &Path,
    name: &str,
    content: &[u8],
    now: SystemTime,
) -> Result<PathBuf, HostsError> {
    let stamp = backup_stamp(now);
    for attempt in 0..MAX_BACKUP_ATTEMPTS {
        let file_name = if attempt == 0 {
            format!("{name}.{stamp}.{BACKUP_EXTENSION}")
        } else {
            format!("{name}.{stamp}-{attempt}.{BACKUP_EXTENSION}")
        };
        let path = folder.join(file_name);
        let file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path);
        let mut file = match file {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(backup_error(error)),
        };
        file.write_all(content).map_err(backup_error)?;
        file.sync_all().map_err(backup_error)?;
        return Ok(path);
    }
    Err(backup_error(io::Error::from(io::ErrorKind::AlreadyExists)))
}

/// `now` as `YYYYMMDD-HHMMSS`, UTC.
fn backup_stamp(now: SystemTime) -> String {
    let seconds = now
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs());
    let days = i64::try_from(seconds / SECONDS_PER_DAY).unwrap_or_default();
    let (year, month, day) = civil_from_days(days);
    let of_day = seconds % SECONDS_PER_DAY;
    format!(
        "{year:04}{month:02}{day:02}-{:02}{:02}{:02}",
        of_day / SECONDS_PER_HOUR,
        of_day % SECONDS_PER_HOUR / SECONDS_PER_MINUTE,
        of_day % SECONDS_PER_MINUTE
    )
}

fn read_error(error: io::Error) -> HostsError {
    match error.kind() {
        io::ErrorKind::NotFound => HostsError::NotFound,
        io::ErrorKind::PermissionDenied => HostsError::ReadDenied,
        _ => HostsError::Read(error.to_string()),
    }
}

fn write_error(error: io::Error) -> HostsError {
    match error.kind() {
        io::ErrorKind::PermissionDenied => HostsError::AdminRequired,
        _ => HostsError::Write(error.to_string()),
    }
}

fn backup_error(error: io::Error) -> HostsError {
    match error.kind() {
        io::ErrorKind::PermissionDenied => HostsError::AdminRequired,
        _ => HostsError::Backup(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    /// 2026-10-15 12:34:56 UTC.
    const NOW_SECONDS: u64 = 1_792_067_696;

    fn now() -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(NOW_SECONDS)
    }

    const WINDOWS_SAMPLE: &[u8] = b"# Copyright (c) 1993-2009 Microsoft Corp.\r\n\
#\r\n\
#      102.54.94.97     rhino.acme.com          # source server\r\n\
# localhost name resolution is handled within DNS itself.\r\n\
#\t127.0.0.1       localhost\r\n\
\r\n\
127.0.0.1       localhost loopback\r\n\
# 1.2.3.4 was the old gateway, do not use\r\n\
::1             localhost\r\n\
# 192.168.1.100 dev.local # local dev server\r\n";

    #[test]
    fn test_validate_ip() {
        assert!(validate_ip("127.0.0.1"));
        assert!(validate_ip("::1"));
        assert!(!validate_ip("invalid.host.name"));
        assert!(!validate_ip("999.999.999.999"));
    }

    #[test]
    fn host_names_refuse_spaces_comment_marks_and_control_characters() {
        assert!(is_valid_host_name("dev.local"));
        assert!(is_valid_host_name("my_host-1.example"));
        for bad in ["", "two words", "a#b", "a\tb", "a\nb", "a,b", "a\u{0}b"] {
            assert!(!is_valid_host_name(bad), "{bad:?}");
        }
        assert!(!is_valid_host_name(&"a".repeat(MAX_HOST_NAME_LENGTH + 1)));
    }

    #[test]
    fn entries_are_read_with_their_aliases_and_prose_stays_a_comment() {
        let document = HostsDocument::parse(WINDOWS_SAMPLE);
        let entries: Vec<&HostEntry> = document.entries().collect();
        let shown: Vec<(bool, &str, String)> = entries
            .iter()
            .map(|entry| (entry.enabled, entry.ip.as_str(), entry.names()))
            .collect();
        assert_eq!(
            shown,
            [
                (false, "102.54.94.97", "rhino.acme.com".to_owned()),
                (false, "127.0.0.1", "localhost".to_owned()),
                (true, "127.0.0.1", "localhost loopback".to_owned()),
                (true, "::1", "localhost".to_owned()),
                (false, "192.168.1.100", "dev.local".to_owned()),
            ]
        );
        assert_eq!(entries[2].aliases, ["loopback"]);
        assert_eq!(entries[0].comment, "source server");
        assert_eq!(entries[4].comment, "local dev server");
    }

    #[test]
    fn an_unchanged_file_is_written_back_byte_for_byte() {
        for sample in [
            WINDOWS_SAMPLE,
            b"127.0.0.1 a\n# note\n10.0.0.1 b c d".as_slice(),
            b"\xEF\xBB\xBF127.0.0.1 a\r\n",
            b"",
            b"\n\n",
        ] {
            assert_eq!(HostsDocument::parse(sample).to_bytes(), sample);
        }
    }

    #[test]
    fn a_non_utf8_comment_is_read_and_kept_byte_for_byte() {
        // "# Réseau" in Windows-1252: not UTF-8.
        let sample = b"# R\xE9seau local\r\n127.0.0.1 a\r\n";
        let mut document = HostsDocument::parse(sample);
        assert_eq!(document.entries().count(), 1);
        document.add("10.0.0.2", "b", "").expect("added");
        let written = document.to_bytes();
        assert!(written.starts_with(b"# R\xE9seau local\r\n127.0.0.1 a\r\n"));
    }

    #[test]
    fn a_change_rewrites_only_its_line_and_keeps_aliases_comments_and_crlf() {
        let mut document = HostsDocument::parse(WINDOWS_SAMPLE);
        // Disable "127.0.0.1 localhost loopback", enable "# 192.168.1.100 dev.local".
        document.toggle(2);
        document.toggle(4);
        document
            .add("10.0.0.5", "new.local", "added")
            .expect("added");
        let written = String::from_utf8(document.to_bytes()).expect("utf8");
        let expected = "# Copyright (c) 1993-2009 Microsoft Corp.\r\n\
#\r\n\
#      102.54.94.97     rhino.acme.com          # source server\r\n\
# localhost name resolution is handled within DNS itself.\r\n\
#\t127.0.0.1       localhost\r\n\
\r\n\
# 127.0.0.1       localhost loopback\r\n\
# 1.2.3.4 was the old gateway, do not use\r\n\
::1             localhost\r\n\
192.168.1.100 dev.local # local dev server\r\n\
10.0.0.5\tnew.local\t# added\r\n";
        assert_eq!(written, expected);
        // Read again, the same entries.
        let again = HostsDocument::parse(written.as_bytes());
        assert_eq!(again.entries().count(), 6);
        assert!(!again.entries().nth(2).expect("entry").enabled);
    }

    #[test]
    fn an_entry_removed_takes_only_its_line() {
        let mut document = HostsDocument::parse(b"# head\n127.0.0.1 a\n# keep\n10.0.0.1 b\n");
        document.remove(0);
        assert_eq!(document.to_bytes(), b"# head\n# keep\n10.0.0.1 b\n");
    }

    #[test]
    fn several_entries_removed_at_once_take_only_their_lines() {
        let mut document = HostsDocument::parse(WINDOWS_SAMPLE);
        // "# 102.54.94.97 rhino", "127.0.0.1 localhost loopback" and "# 192.168.1.100".
        document.remove_all(&BTreeSet::from([0, 2, 4]));
        let expected: &[u8] = b"# Copyright (c) 1993-2009 Microsoft Corp.\r\n\
#\r\n\
# localhost name resolution is handled within DNS itself.\r\n\
#\t127.0.0.1       localhost\r\n\
\r\n\
# 1.2.3.4 was the old gateway, do not use\r\n\
::1             localhost\r\n";
        assert_eq!(document.to_bytes(), expected);
        // An index past the entries removes nothing more.
        document.remove_all(&BTreeSet::from([9]));
        assert_eq!(document.to_bytes(), expected);
    }

    #[test]
    fn an_edit_rewrites_only_its_line_and_keeps_every_other_byte() {
        let mut document = HostsDocument::parse(WINDOWS_SAMPLE);
        // "127.0.0.1       localhost loopback", active.
        document
            .edit(2, " 10.0.0.9 ", "web.local  www.local", " moved ")
            .expect("edited");
        let mut expected = WINDOWS_SAMPLE.to_vec();
        let old: &[u8] = b"127.0.0.1       localhost loopback\r\n";
        let at = expected
            .windows(old.len())
            .position(|window| window == old)
            .expect("line");
        expected.splice(
            at..at + old.len(),
            b"10.0.0.9\tweb.local www.local\t# moved\r\n"
                .iter()
                .copied(),
        );
        assert_eq!(document.to_bytes(), expected);
        let entry = document.entries().nth(2).expect("entry");
        assert!(entry.enabled);
        assert_eq!(entry.aliases, ["www.local"]);
    }

    #[test]
    fn a_disabled_entry_edited_stays_disabled_and_in_its_place() {
        let mut document = HostsDocument::parse(b"# head\n# 10.0.0.1 a # old\n10.0.0.2 b");
        document.edit(0, "10.0.0.3", "c", "").expect("edited");
        assert_eq!(document.to_bytes(), b"# head\n# 10.0.0.3\tc\n10.0.0.2 b");
        let entry = document.entries().next().expect("entry");
        assert!(!entry.enabled);
        assert_eq!(entry.comment, "");
    }

    #[test]
    fn an_edit_that_changes_nothing_leaves_the_line_as_it_was() {
        let sample = b"127.0.0.1       localhost loopback   # here\n";
        let mut document = HostsDocument::parse(sample);
        document
            .edit(0, "127.0.0.1", "localhost loopback", "here")
            .expect("edited");
        assert_eq!(document.to_bytes(), sample);
        // An entry that is not there is no change either.
        document.edit(5, "10.0.0.1", "a", "").expect("nothing");
        assert_eq!(document.to_bytes(), sample);
    }

    #[test]
    fn an_edit_is_checked_as_an_entry_added_and_a_refused_one_changes_nothing() {
        let mut document = HostsDocument::parse(WINDOWS_SAMPLE);
        for (ip, names, comment, error) in [
            ("x", "a", "", EntryError::InvalidIp),
            ("999.1.1.1", "a", "", EntryError::InvalidIp),
            ("10.0.0.1", " ", "", EntryError::MissingHost),
            ("10.0.0.1", "a#b", "", EntryError::InvalidHost),
            ("10.0.0.1", "a b,c", "", EntryError::InvalidHost),
            (
                "10.0.0.1",
                "a",
                "x\r\n10.6.6.6 evil",
                EntryError::InvalidComment,
            ),
        ] {
            assert_eq!(
                document.edit(2, ip, names, comment),
                Err(error),
                "{ip:?} {names:?}"
            );
        }
        assert_eq!(document.to_bytes(), WINDOWS_SAMPLE);
    }

    #[test]
    fn an_edit_saved_reads_back_as_edited_and_keeps_the_rest_of_the_file() {
        let folder = tempfile::tempdir().expect("folder");
        let path = folder.path().join("hosts");
        fs::write(&path, WINDOWS_SAMPLE).expect("written");
        let loaded = load(&path).expect("loaded");
        assert!(loaded.modified.is_some());
        let mut document = loaded.document.clone();
        document
            .edit(4, "192.168.1.101", "dev.local api.local", "moved")
            .expect("edited");
        let saved = save(&path, &loaded.original, &document, now()).expect("saved");
        assert!(saved.backup.is_some());
        assert!(saved.modified.is_some());
        let again = load(&path).expect("loaded again");
        assert_eq!(again.original, document.to_bytes());
        let entries: Vec<&HostEntry> = again.document.entries().collect();
        assert_eq!(entries.len(), 5);
        assert_eq!(entries[4].ip, "192.168.1.101");
        assert_eq!(entries[4].names(), "dev.local api.local");
        assert_eq!(entries[4].comment, "moved");
        assert!(!entries[4].enabled);
        // Every line but the one edited is the same.
        let before: Vec<&[u8]> = WINDOWS_SAMPLE.split(|&byte| byte == b'\n').collect();
        let after: Vec<&[u8]> = again.original.split(|&byte| byte == b'\n').collect();
        assert_eq!(before.len(), after.len());
        let changed: Vec<usize> = (0..before.len())
            .filter(|&line| before[line] != after[line])
            .collect();
        assert_eq!(changed, [9]);
    }

    #[test]
    fn a_line_added_after_a_last_line_without_a_break_gets_one() {
        let mut document = HostsDocument::parse(b"127.0.0.1 a\r\n10.0.0.1 b");
        document.add("10.0.0.2", "c", "").expect("added");
        assert_eq!(
            document.to_bytes(),
            b"127.0.0.1 a\r\n10.0.0.1 b\r\n10.0.0.2\tc\r\n"
        );
    }

    #[test]
    fn an_entry_typed_in_is_checked() {
        let mut document = HostsDocument::default();
        assert_eq!(document.add("x", "a", ""), Err(EntryError::InvalidIp));
        assert_eq!(
            document.add("10.0.0.1", " ", ""),
            Err(EntryError::MissingHost)
        );
        assert_eq!(
            document.add("10.0.0.1", "a b", ""),
            Err(EntryError::InvalidHost)
        );
        assert_eq!(
            document.add("10.0.0.1", "a#b", ""),
            Err(EntryError::InvalidHost)
        );
        assert_eq!(
            document.add("10.0.0.1", "a", "x\r\n10.6.6.6 evil"),
            Err(EntryError::InvalidComment)
        );
        assert_eq!(document.entries().count(), 0);
    }

    #[test]
    fn a_save_replaces_the_file_and_keeps_a_backup_of_the_old_one() {
        let folder = tempfile::tempdir().expect("folder");
        let path = folder.path().join("hosts");
        fs::write(&path, WINDOWS_SAMPLE).expect("written");
        let loaded = load(&path).expect("loaded");
        let mut document = loaded.document.clone();
        document.toggle(2);
        let saved = save(&path, &loaded.original, &document, now()).expect("saved");
        assert_eq!(fs::read(&path).expect("read"), document.to_bytes());
        assert_eq!(saved.written, document.to_bytes());
        let backup = saved.backup.expect("a backup");
        assert_eq!(backup.parent(), Some(folder.path()));
        assert_eq!(
            backup.file_name().and_then(|name| name.to_str()),
            Some("hosts.20261015-123456.bak")
        );
        assert_eq!(fs::read(&backup).expect("backup"), WINDOWS_SAMPLE);
        // A second save in the same second keeps the first backup.
        let saved_again = save(&path, &saved.written, &loaded.document, now()).expect("saved");
        let second = saved_again.backup.expect("a backup");
        assert_ne!(second, backup);
        assert_eq!(fs::read(&backup).expect("backup"), WINDOWS_SAMPLE);
        // No temporary file left behind.
        let left: Vec<String> = fs::read_dir(folder.path())
            .expect("listed")
            .map(|entry| {
                entry
                    .expect("entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .filter(|name| name.starts_with(TEMP_PREFIX))
            .collect();
        assert!(left.is_empty(), "{left:?}");
    }

    #[test]
    fn a_file_changed_on_disk_since_it_was_loaded_is_never_overwritten() {
        let folder = tempfile::tempdir().expect("folder");
        let path = folder.path().join("hosts");
        fs::write(&path, b"127.0.0.1 a\n").expect("written");
        let loaded = load(&path).expect("loaded");
        fs::write(&path, b"127.0.0.1 a\n10.0.0.1 other-program\n").expect("changed");
        let mut document = loaded.document.clone();
        document.add("10.0.0.2", "b", "").expect("added");
        assert_eq!(
            save(&path, &loaded.original, &document, now()),
            Err(HostsError::ChangedOnDisk)
        );
        assert_eq!(
            fs::read(&path).expect("read"),
            b"127.0.0.1 a\n10.0.0.1 other-program\n"
        );
        assert_eq!(fs::read_dir(folder.path()).expect("listed").count(), 1);
    }

    #[test]
    fn nothing_changed_writes_nothing() {
        let folder = tempfile::tempdir().expect("folder");
        let path = folder.path().join("hosts");
        fs::write(&path, b"127.0.0.1 a\n").expect("written");
        let loaded = load(&path).expect("loaded");
        let saved = save(&path, &loaded.original, &loaded.document, now()).expect("saved");
        assert_eq!(saved.backup, None);
        assert_eq!(fs::read_dir(folder.path()).expect("listed").count(), 1);
    }

    #[test]
    fn a_missing_file_is_said_not_found() {
        let folder = tempfile::tempdir().expect("folder");
        assert_eq!(
            load(&folder.path().join("hosts")),
            Err(HostsError::NotFound)
        );
    }

    #[test]
    fn the_backup_stamp_is_the_utc_date_and_time() {
        assert_eq!(backup_stamp(now()), "20261015-123456");
        assert_eq!(backup_stamp(UNIX_EPOCH), "19700101-000000");
    }
}
