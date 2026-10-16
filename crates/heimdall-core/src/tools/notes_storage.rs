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

//! The notes, kept as the C# `NotesStorageService` (`Services/NotesStorageService.cs`) keeps
//! them: one Markdown file per note under `<data>/config/notes`, its title the first `# `
//! line, its tags the `> tags:` lines, its date the file's last write. A note's file is its
//! identity: a new note gets a name never used, so an edit always reaches the note shown.

use std::fs;
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use super::date_time::WallClock;

/// The folder of the notes inside the data folder: `config`, then `notes`, as the C#.
const NOTES_PATH: [&str; 2] = ["config", "notes"];

/// The extension of a note's file, as the C# `*.md`.
pub const NOTE_EXTENSION: &str = "md";

/// What starts a note's title line, as the C# `ExtractTitle`.
const TITLE_PREFIX: &str = "# ";

/// What starts a metadata line, as the C# `TryGetMetadataPayload`.
const METADATA_PREFIX: char = '>';

/// The metadata naming a note's tags, as the C# `ExtractTags`.
const TAGS_KEY: &str = "tags:";

/// What separates the tags of a `> tags:` line.
const TAG_SEPARATOR: char = ',';

/// The most numbered names tried for a new note, as the C# `GetUniquePath`.
const MAX_NAME_SUFFIX: u32 = 1000;

/// The UTF-8 byte order mark, left out when a note is read.
const UTF8_BOM: &str = "\u{FEFF}";

/// The folder earlier builds wrote the notes started from the session tree in: `notes`
/// beside the profiles, in the configuration folder, apart from the Notes tool's.
const OLD_NOTES_FOLDER: &str = "notes";

/// The folder of the notes, under the data folder; `None` when the platform has none. The
/// one notes folder: the Notes tool lists it and the session tree's notes are written in
/// it, as the C# has one notes root for both (`NotesStorageService.NotesRootPath`), under
/// its local data folder (`App.ResolveNotesStoragePath`: `<data>\config\notes`).
#[must_use]
pub fn notes_dir() -> Option<PathBuf> {
    crate::paths::data_dir().map(|dir| NOTES_PATH.iter().fold(dir, |dir, part| dir.join(part)))
}

/// The folder earlier builds wrote the session tree's notes in, whether or not it exists;
/// `None` when the platform has no configuration folder.
#[must_use]
pub fn old_notes_dir() -> Option<PathBuf> {
    crate::paths::config_dir().map(|dir| dir.join(OLD_NOTES_FOLDER))
}

/// The file, in the old notes folder, naming each of its files copied into the notes folder
/// already, one path under it a line: a copy edited or deleted since is never made again.
const COPIED_RECORD: &str = ".copied-to-notes-folder";

/// What separates the parts of a path in [`COPIED_RECORD`], whatever the platform.
const RECORD_SEPARATOR: char = '/';

/// What copying the notes of an old folder into the notes folder did.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct NotesCopy {
    /// Each file copied, from where to where; under another name when its own was taken by
    /// another file.
    pub copied: Vec<(PathBuf, PathBuf)>,
    /// Each file found in the notes folder already, the same to the byte: not copied again.
    pub present: Vec<PathBuf>,
    /// Each file not copied, and why.
    pub failed: Vec<(PathBuf, String)>,
}

/// Copies the notes of the folder earlier builds wrote the session tree's notes in into the
/// notes folder, at start before any note is opened (a note's file is its identity);
/// nothing when either folder is unknown or the old one is not there. What it did is
/// logged.
pub fn copy_old_notes() {
    if let (Some(from), Some(to)) = (old_notes_dir(), notes_dir()) {
        let _copied = copy_notes(&from, &to);
    }
}

/// Copies every file under `from` to the same place under `to`, except those copied by an
/// earlier run (named in [`COPIED_RECORD`]). A file is never written over: when its name is
/// taken by another file there, it goes under the first free numbered name, as the C#
/// `GetUniquePath` (`name-2.md`, `name-3.md`...), and the same file there already is not
/// copied again. Each copy is written whole through [`crate::store::replace_file`], read
/// back and given the original's last write time. Nothing in `from` is moved or removed:
/// the originals stay, logged as copied and free to remove.
#[must_use]
pub fn copy_notes(from: &Path, to: &Path) -> NotesCopy {
    let mut report = NotesCopy::default();
    let is_dir = fs::symlink_metadata(from).is_ok_and(|meta| meta.is_dir());
    if !is_dir || to.starts_with(from) || from.starts_with(to) {
        return report;
    }
    let record = from.join(COPIED_RECORD);
    let mut done = read_record(&record);
    let mut files = Vec::new();
    collect_files(from, &mut files, &mut report);
    for file in files {
        let Some(relative) = file.strip_prefix(from).ok().map(record_line) else {
            continue;
        };
        if file == record || done.contains(&relative) {
            continue;
        }
        let target = to.join(file.strip_prefix(from).unwrap_or(&file));
        match copy_note(&file, &target) {
            Ok(Copied::To(copy)) => {
                if copy != target {
                    log::warn!(
                        "note {} copied to {}: its name was taken by another note",
                        file.display(),
                        copy.display()
                    );
                }
                report.copied.push((file, copy));
                done.push(relative);
            }
            Ok(Copied::Present(there)) => {
                log::info!("note {} was in {} already", file.display(), there.display());
                report.present.push(file);
                done.push(relative);
            }
            Err(error) => {
                log::warn!("note {} not copied: {error}", file.display());
                report.failed.push((file, error.to_string()));
            }
        }
    }
    if !report.copied.is_empty() || !report.present.is_empty() {
        let mut text = done.join("\n");
        text.push('\n');
        if let Err(error) = crate::store::replace_file(&record, text.as_bytes()) {
            log::warn!("{} not written: {error}", record.display());
        }
        log::info!(
            "{} notes copied from {} to {}; the originals were left there and can be removed",
            report.copied.len(),
            from.display(),
            to.display()
        );
    }
    report
}

/// The paths [`COPIED_RECORD`] names; none when it is not there.
fn read_record(record: &Path) -> Vec<String> {
    fs::read_to_string(record)
        .map(|text| {
            text.lines()
                .filter(|line| !line.is_empty())
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

/// `relative` as a line of [`COPIED_RECORD`].
fn record_line(relative: &Path) -> String {
    relative
        .components()
        .map(|part| part.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join(&RECORD_SEPARATOR.to_string())
}

/// The files under `dir` and its folders; anything else (a link) noted as not copied.
fn collect_files(dir: &Path, files: &mut Vec<PathBuf>, report: &mut NotesCopy) {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) => {
            log::warn!("notes folder {} not read: {error}", dir.display());
            report.failed.push((dir.to_owned(), error.to_string()));
            return;
        }
    };
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                report.failed.push((dir.to_owned(), error.to_string()));
                continue;
            }
        };
        let path = entry.path();
        match entry.file_type() {
            Ok(kind) if kind.is_dir() => collect_files(&path, files, report),
            Ok(kind) if kind.is_file() => files.push(path),
            Ok(_) => {
                log::warn!("{} not copied: not a file", path.display());
                report.failed.push((path, "not a file".to_owned()));
            }
            Err(error) => report.failed.push((path, error.to_string())),
        }
    }
}

/// Where a copied file is.
enum Copied {
    /// Written there.
    To(PathBuf),
    /// There already, the same.
    Present(PathBuf),
}

/// Copies the file `from` to `target`, or to the first free numbered name beside it.
fn copy_note(from: &Path, target: &Path) -> io::Result<Copied> {
    let bytes = fs::read(from)?;
    let mut free = None;
    for candidate in candidates(target) {
        match fs::read(&candidate) {
            Ok(there) if there == bytes => return Ok(Copied::Present(candidate)),
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                free = Some(candidate);
                break;
            }
            Err(error) => return Err(error),
        }
    }
    let Some(target) = free else {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "no free name for the note",
        ));
    };
    crate::store::replace_file(&target, &bytes)?;
    if fs::read(&target)? != bytes {
        return Err(io::Error::other("the copy does not read back the same"));
    }
    // Its date kept: the Notes tool lists the newest first.
    if let Ok(modified) = fs::metadata(from).and_then(|meta| meta.modified()) {
        let _ = fs::File::options()
            .write(true)
            .open(&target)
            .and_then(|file| file.set_modified(modified));
    }
    Ok(Copied::To(target))
}

/// `target`, then each numbered name beside it, as the C# `GetUniquePath`.
fn candidates(target: &Path) -> impl Iterator<Item = PathBuf> + '_ {
    let stem = target
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default();
    let extension = target
        .extension()
        .map(|extension| format!(".{}", extension.to_string_lossy()))
        .unwrap_or_default();
    std::iter::once(target.to_owned()).chain(
        (2..MAX_NAME_SUFFIX)
            .map(move |suffix| target.with_file_name(format!("{stem}-{suffix}{extension}"))),
    )
}

/// A note read from its file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Note {
    /// Its file: its identity.
    pub path: PathBuf,
    /// Its Markdown text.
    pub content: String,
    /// When its file was last written, when the system says.
    pub modified: Option<SystemTime>,
}

impl Note {
    /// Its title: the first `# ` line, else its file's name, as the C# `ExtractTitle`.
    #[must_use]
    pub fn title(&self) -> String {
        title_of(&self.path, &self.content)
    }

    /// Its tags, from its `> tags:` lines, each once, as the C# `ExtractTags`.
    #[must_use]
    pub fn tags(&self) -> Vec<String> {
        tags_of(&self.content)
    }

    /// Whether `query` is in its text or title, or its file's name, ignoring case, as the
    /// C# `ListNotesAsync` search; an empty query matches every note.
    #[must_use]
    pub fn matches(&self, query: &str) -> bool {
        let query = query.trim().to_lowercase();
        if query.is_empty() {
            return true;
        }
        let name = self
            .path
            .file_name()
            .map(|name| name.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        self.content.to_lowercase().contains(&query)
            || self.title().to_lowercase().contains(&query)
            || name.contains(&query)
    }
}

/// The title of the note `content` held in `path`.
#[must_use]
pub fn title_of(path: &Path, content: &str) -> String {
    content
        .lines()
        .map(str::trim)
        .find_map(|line| line.strip_prefix(TITLE_PREFIX))
        .map(|title| title.trim().to_owned())
        .unwrap_or_else(|| {
            path.file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
                .unwrap_or_default()
        })
}

/// The tags of `content`: the comma-separated values of its `> tags:` lines, trimmed, each
/// once whatever its case.
#[must_use]
pub fn tags_of(content: &str) -> Vec<String> {
    let mut tags: Vec<String> = Vec::new();
    for line in content.lines() {
        let Some(payload) = line.trim().strip_prefix(METADATA_PREFIX) else {
            continue;
        };
        let payload = payload.trim();
        let Some(values) = payload
            .get(..TAGS_KEY.len())
            .filter(|key| key.eq_ignore_ascii_case(TAGS_KEY))
            .map(|_| &payload[TAGS_KEY.len()..])
        else {
            continue;
        };
        for tag in values.split(TAG_SEPARATOR).map(str::trim) {
            if !tag.is_empty()
                && !tags
                    .iter()
                    .any(|kept| kept.to_lowercase() == tag.to_lowercase())
            {
                tags.push(tag.to_owned());
            }
        }
    }
    tags
}

/// The notes under `dir` and its folders, newest first, then by path, as the C# default
/// order; none when the folder does not exist yet.
///
/// # Errors
///
/// The folder or a note cannot be read.
pub fn list_notes(dir: &Path) -> io::Result<Vec<Note>> {
    let mut notes = Vec::new();
    if dir.is_dir() {
        collect_notes(dir, &mut notes)?;
    }
    notes.sort_by(|a, b| {
        b.modified
            .cmp(&a.modified)
            .then_with(|| a.path.cmp(&b.path))
    });
    Ok(notes)
}

/// The notes of `dir` and its folders, added to `notes`.
fn collect_notes(dir: &Path, notes: &mut Vec<Note>) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let kind = entry.file_type()?;
        if kind.is_dir() {
            collect_notes(&path, notes)?;
        } else if kind.is_file() && is_note_file(&path) {
            notes.push(read_note(&path)?);
        }
    }
    Ok(())
}

/// Whether `path` names a note's file.
fn is_note_file(path: &Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case(NOTE_EXTENSION))
}

/// The note in `path`, read as UTF-8 as the C# reads it.
///
/// # Errors
///
/// The file cannot be read.
pub fn read_note(path: &Path) -> io::Result<Note> {
    let bytes = fs::read(path)?;
    let text = String::from_utf8_lossy(&bytes);
    let content = text.strip_prefix(UTF8_BOM).unwrap_or(&text).to_owned();
    let modified = fs::metadata(path).and_then(|meta| meta.modified()).ok();
    Ok(Note {
        path: path.to_owned(),
        content,
        modified,
    })
}

/// Writes `content` as the note in `path`, in UTF-8 without a byte order mark as the C#;
/// the file replaced at once, never left half written. When it was last written then.
///
/// # Errors
///
/// The folder or the file cannot be written.
pub fn save_note(path: &Path, content: &str) -> io::Result<Option<SystemTime>> {
    if path
        .parent()
        .is_none_or(|folder| folder.as_os_str().is_empty())
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "a note needs a folder",
        ));
    }
    crate::store::replace_file(path, content.as_bytes())?;
    Ok(fs::metadata(path).and_then(|meta| meta.modified()).ok())
}

/// A new note titled `title`, made at `now`, saved in `dir` under a name never used: the
/// time, then the title made a file name, as the C# blank note
/// (`NotesTemplateFactory.BuildBlank`).
///
/// # Errors
///
/// The folder or the file cannot be written.
pub fn create_note(dir: &Path, title: &str, now: &WallClock) -> io::Result<Note> {
    fs::create_dir_all(dir)?;
    let stamp = format!(
        "{:04}{:02}{:02}-{:02}{:02}{:02}",
        now.year, now.month, now.day, now.hour, now.minute, now.second
    );
    let slug = slugify(title);
    let stem = if slug.is_empty() {
        stamp
    } else {
        format!("{stamp}-{slug}")
    };
    let content = format!("{TITLE_PREFIX}{title}\n\n");
    for suffix in 1..=MAX_NAME_SUFFIX {
        let name = if suffix == 1 {
            format!("{stem}.{NOTE_EXTENSION}")
        } else {
            format!("{stem}-{suffix}.{NOTE_EXTENSION}")
        };
        let path = dir.join(name);
        // Made only when no file has its name: a note is never written over.
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(mut file) => {
                file.write_all(content.as_bytes())?;
                drop(file);
                return read_note(&path);
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "no free name for the note",
    ))
}

/// Deletes the note in `path`, as the C# `DeleteNoteAsync`; a note already gone is not an
/// error.
///
/// # Errors
///
/// The file cannot be deleted.
pub fn delete_note(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

/// `text` as a file name, as the C# `Slugify`: letters and digits lower-cased, each run of
/// anything else one dash, none at either end.
#[must_use]
pub fn slugify(text: &str) -> String {
    let mut slug = String::with_capacity(text.len());
    let mut dash = false;
    for c in text.chars() {
        if c.is_alphanumeric() {
            slug.extend(c.to_lowercase());
            dash = false;
        } else if !dash {
            slug.push('-');
            dash = true;
        }
    }
    slug.trim_matches('-').to_owned()
}

/// `text` safe inside HTML text and attribute values.
#[must_use]
pub fn escape_html(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            _ => escaped.push(c),
        }
    }
    escaped
}

/// What opens a fenced code block.
const FENCE: &str = "```";

/// The note as a standalone HTML document: its title and tags, then its Markdown's
/// headings, list items, fenced code and paragraphs; every text from the note escaped, its
/// tags included.
#[must_use]
pub fn to_html(note: &Note) -> String {
    let title = escape_html(&note.title());
    let mut html = format!(
        "<!DOCTYPE html>\n<html>\n<head>\n<meta charset=\"utf-8\">\n<title>{title}</title>\n</head>\n<body>\n"
    );
    let tags = note.tags();
    if !tags.is_empty() {
        let tags: Vec<String> = tags
            .iter()
            .map(|tag| format!("<span class=\"tag\">#{}</span>", escape_html(tag)))
            .collect();
        html.push_str(&format!("<p class=\"tags\">{}</p>\n", tags.join(" ")));
    }
    let mut in_code = false;
    let mut in_list = false;
    for line in note.content.lines() {
        let trimmed = line.trim();
        if let Some(language) = trimmed.strip_prefix(FENCE) {
            if in_list {
                html.push_str("</ul>\n");
                in_list = false;
            }
            if in_code {
                html.push_str("</code></pre>\n");
            } else if language.trim().is_empty() {
                html.push_str("<pre><code>");
            } else {
                html.push_str(&format!(
                    "<pre><code class=\"language-{}\">",
                    escape_html(language.trim())
                ));
            }
            in_code = !in_code;
            continue;
        }
        if in_code {
            html.push_str(&escape_html(line));
            html.push('\n');
            continue;
        }
        let item = trimmed
            .strip_prefix("- ")
            .or_else(|| trimmed.strip_prefix("* "));
        if in_list && item.is_none() {
            html.push_str("</ul>\n");
            in_list = false;
        }
        if trimmed.is_empty() {
            continue;
        }
        if let Some(item) = item {
            if !in_list {
                html.push_str("<ul>\n");
                in_list = true;
            }
            html.push_str(&format!("<li>{}</li>\n", escape_html(item)));
        } else if let Some((level, heading)) = heading(trimmed) {
            html.push_str(&format!("<h{level}>{}</h{level}>\n", escape_html(heading)));
        } else if trimmed.starts_with(METADATA_PREFIX) {
            let quote = trimmed[METADATA_PREFIX.len_utf8()..].trim();
            html.push_str(&format!(
                "<blockquote>{}</blockquote>\n",
                escape_html(quote)
            ));
        } else {
            html.push_str(&format!("<p>{}</p>\n", escape_html(trimmed)));
        }
    }
    if in_code {
        html.push_str("</code></pre>\n");
    }
    if in_list {
        html.push_str("</ul>\n");
    }
    html.push_str("</body>\n</html>\n");
    html
}

/// The deepest heading Markdown writes.
const MAX_HEADING_LEVEL: usize = 6;

/// The level and text of a Markdown heading line.
fn heading(line: &str) -> Option<(usize, &str)> {
    let level = line.chars().take_while(|c| *c == '#').count();
    if level == 0 || level > MAX_HEADING_LEVEL {
        return None;
    }
    line[level..]
        .strip_prefix(' ')
        .map(|text| (level, text.trim()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::date_time::Instant;

    fn now() -> WallClock {
        // 2026-10-15 12:07:09 UTC.
        Instant::from_unix_seconds(1_792_066_029).wall_clock(0)
    }

    fn note(content: &str) -> Note {
        Note {
            path: PathBuf::from("notes/20261015-120709-working-note.md"),
            content: content.to_owned(),
            modified: None,
        }
    }

    #[test]
    fn the_title_is_the_first_heading_else_the_file_name() {
        assert_eq!(note("intro\n  # Main  \n# Other").title(), "Main");
        assert_eq!(note("no heading").title(), "20261015-120709-working-note");
        assert_eq!(note("## Sub only").title(), "20261015-120709-working-note");
    }

    #[test]
    fn tags_come_from_tags_lines_each_once() {
        let content = "# T\n> tags: Ops, network ,ops\n>Tags:linux\n> date: today\ntags: no\n";
        assert_eq!(note(content).tags(), ["Ops", "network", "linux"]);
    }

    #[test]
    fn a_new_note_gets_a_name_never_used_and_is_listed_newest_first() {
        let dir = tempfile::tempdir().expect("temp dir");
        let first = create_note(dir.path(), "Working note", &now()).expect("created");
        let second = create_note(dir.path(), "Working note", &now()).expect("created");
        assert_eq!(
            first.path.file_name().and_then(|name| name.to_str()),
            Some("20261015-120709-working-note.md")
        );
        assert_eq!(
            second.path.file_name().and_then(|name| name.to_str()),
            Some("20261015-120709-working-note-2.md")
        );
        assert_eq!(first.content, "# Working note\n\n");
        assert_eq!(first.title(), "Working note");
        let listed = list_notes(dir.path()).expect("listed");
        assert_eq!(listed.len(), 2);
        delete_note(&first.path).expect("deleted");
        let third = create_note(dir.path(), "Working note", &now()).expect("created");
        assert_ne!(third.path, second.path, "a name in use is never taken");
    }

    #[test]
    fn a_saved_note_is_read_back_and_found_in_its_folders() {
        let dir = tempfile::tempdir().expect("temp dir");
        let nested = dir.path().join("daily").join("2026").join("a.md");
        save_note(&nested, "# Day\n> tags: daily\nbody").expect("saved");
        save_note(&dir.path().join("ignored.txt"), "not a note").expect("saved");
        let listed = list_notes(dir.path()).expect("listed");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].path, nested);
        assert_eq!(listed[0].content, "# Day\n> tags: daily\nbody");
        assert!(listed[0].modified.is_some());
        assert!(listed[0].matches("BODY"));
        assert!(listed[0].matches("a.md"));
        assert!(!listed[0].matches("absent"));
        save_note(&nested, "# Day\nchanged").expect("saved again");
        assert_eq!(read_note(&nested).expect("read").content, "# Day\nchanged");
        delete_note(&nested).expect("deleted");
        delete_note(&nested).expect("already gone is fine");
        assert!(list_notes(dir.path()).expect("listed").is_empty());
        assert!(
            list_notes(&dir.path().join("missing"))
                .expect("listed")
                .is_empty()
        );
    }

    /// Every file under `dir`, by its path there, with what it holds.
    fn files_under(dir: &Path) -> Vec<(PathBuf, Vec<u8>)> {
        fn walk(root: &Path, dir: &Path, found: &mut Vec<(PathBuf, Vec<u8>)>) {
            for entry in fs::read_dir(dir).expect("read") {
                let path = entry.expect("entry").path();
                if path.is_dir() {
                    walk(root, &path, found);
                } else {
                    let relative = path.strip_prefix(root).expect("under").to_owned();
                    found.push((relative, fs::read(&path).expect("read")));
                }
            }
        }
        let mut found = Vec::new();
        if dir.is_dir() {
            walk(dir, dir, &mut found);
        }
        found.sort();
        found
    }

    /// The files under `dir` but the record of the copies made.
    fn notes_under(dir: &Path) -> Vec<(PathBuf, Vec<u8>)> {
        let mut found = files_under(dir);
        found.retain(|(path, _)| path != Path::new(COPIED_RECORD));
        found
    }

    #[test]
    fn the_old_notes_are_copied_their_dates_kept_and_the_originals_left() {
        let root = tempfile::tempdir().expect("temp dir");
        let (old, notes) = (root.path().join("old"), root.path().join("notes"));
        let daily = Path::new("daily").join("2026").join("2026-10-05.md");
        let working = Path::new("20261005-140703-working-note.md");
        save_note(&old.join(working), "# Working").expect("saved");
        save_note(&old.join(&daily), "# Day").expect("saved");
        fs::write(old.join("attached.txt"), b"kept too").expect("written");
        let written = fs::metadata(old.join(&daily))
            .and_then(|meta| meta.modified())
            .expect("date");
        let before = files_under(&old);
        let report = copy_notes(&old, &notes);
        assert_eq!(report.copied.len(), 3);
        assert!(report.present.is_empty() && report.failed.is_empty());
        assert_eq!(
            files_under(&notes),
            before,
            "every file, the same, in the same place"
        );
        assert_eq!(notes_under(&old), before, "the originals left as they were");
        assert_eq!(
            fs::metadata(notes.join(&daily))
                .and_then(|meta| meta.modified())
                .expect("date"),
            written
        );
        assert_eq!(list_notes(&notes).expect("listed").len(), 2);
        // A copy edited, another deleted: neither made again at the next start.
        save_note(&notes.join(working), "# Working, edited").expect("saved");
        delete_note(&notes.join(&daily)).expect("deleted");
        assert_eq!(copy_notes(&old, &notes), NotesCopy::default());
        assert_eq!(list_notes(&notes).expect("listed").len(), 1);
        // A note written in the old folder since is copied.
        save_note(&old.join("later.md"), "# Later").expect("saved");
        assert_eq!(
            copy_notes(&old, &notes).copied,
            [(old.join("later.md"), notes.join("later.md"))]
        );
    }

    #[test]
    fn a_name_taken_keeps_both_notes_and_a_note_there_already_is_not_doubled() {
        let root = tempfile::tempdir().expect("temp dir");
        let (old, notes) = (root.path().join("old"), root.path().join("notes"));
        save_note(&old.join("a.md"), "# From the tree").expect("saved");
        save_note(&old.join("same.md"), "# Same").expect("saved");
        save_note(&notes.join("a.md"), "# From the tool").expect("saved");
        save_note(&notes.join("a-2.md"), "# Another").expect("saved");
        save_note(&notes.join("same.md"), "# Same").expect("saved");
        let report = copy_notes(&old, &notes);
        assert_eq!(
            report.copied,
            [(old.join("a.md"), notes.join("a-3.md"))],
            "the first free numbered name"
        );
        assert_eq!(report.present, [old.join("same.md")]);
        assert!(report.failed.is_empty());
        let contents = |name: &str| fs::read_to_string(notes.join(name)).expect("read");
        assert_eq!(contents("a.md"), "# From the tool", "never written over");
        assert_eq!(contents("a-2.md"), "# Another");
        assert_eq!(contents("a-3.md"), "# From the tree");
        assert_eq!(contents("same.md"), "# Same");
        assert_eq!(list_notes(&notes).expect("listed").len(), 4);
        assert_eq!(
            fs::read_to_string(old.join("a.md")).expect("left"),
            "# From the tree"
        );
        assert_eq!(copy_notes(&old, &notes), NotesCopy::default());
    }

    #[test]
    fn an_open_note_keeps_its_file_and_its_text_through_the_copy() {
        let root = tempfile::tempdir().expect("temp dir");
        let (old, notes) = (root.path().join("old"), root.path().join("notes"));
        let open = create_note(&notes, "Working note", &now()).expect("created");
        let name = open.path.file_name().expect("name").to_owned();
        save_note(&old.join(&name), "# The tree's").expect("saved");
        let report = copy_notes(&old, &notes);
        assert_eq!(report.copied.len(), 1);
        assert_ne!(
            report.copied[0].1, open.path,
            "the open note's file not taken"
        );
        let after = read_note(&open.path).expect("still there");
        assert_eq!(after.content, open.content);
        assert_eq!(after.title(), "Working note");
        let listed = list_notes(&notes).expect("listed");
        assert!(listed.iter().any(|note| note.path == open.path));
        assert!(
            listed
                .iter()
                .any(|note| note.path == report.copied[0].1 && note.content == "# The tree's")
        );
    }

    #[test]
    fn a_note_that_cannot_be_copied_is_tried_again_and_never_lost() {
        let root = tempfile::tempdir().expect("temp dir");
        let (old, notes) = (root.path().join("old"), root.path().join("notes"));
        save_note(&old.join("blocked.md"), "# Mine").expect("saved");
        save_note(&old.join("free.md"), "# Free").expect("saved");
        // A folder where the note would go: neither read nor written over.
        fs::create_dir_all(notes.join("blocked.md")).expect("folder");
        let report = copy_notes(&old, &notes);
        assert_eq!(
            report.copied,
            [(old.join("free.md"), notes.join("free.md"))]
        );
        assert_eq!(
            report
                .failed
                .iter()
                .map(|(path, _)| path.clone())
                .collect::<Vec<_>>(),
            [old.join("blocked.md")]
        );
        assert_eq!(
            fs::read_to_string(old.join("blocked.md")).expect("still there"),
            "# Mine"
        );
        assert!(notes.join("blocked.md").is_dir());
        fs::remove_dir(notes.join("blocked.md")).expect("removed");
        assert_eq!(
            copy_notes(&old, &notes).copied,
            [(old.join("blocked.md"), notes.join("blocked.md"))]
        );
    }

    #[test]
    fn no_old_folder_copies_nothing_and_a_folder_is_never_copied_into_itself() {
        let root = tempfile::tempdir().expect("temp dir");
        assert_eq!(
            copy_notes(&root.path().join("missing"), &root.path().join("notes")),
            NotesCopy::default()
        );
        save_note(&root.path().join("a.md"), "# A").expect("saved");
        assert_eq!(
            copy_notes(root.path(), &root.path().join("notes")),
            NotesCopy::default()
        );
        assert!(!root.path().join("notes").exists());
    }

    #[test]
    fn a_title_becomes_a_file_name() {
        assert_eq!(
            slugify("Working note - srv01 (prod)!"),
            "working-note-srv01-prod"
        );
        assert_eq!(slugify("Proc\u{e9}dure"), "proc\u{e9}dure");
        assert_eq!(slugify("***"), "");
    }

    #[test]
    fn the_html_export_escapes_the_text_and_the_tags() {
        let html = to_html(&note(
            "# A <b>&\n> tags: <script>, x\"y\n\n- item <i>\nplain 'text'\n```sh\necho <x>\n```\n",
        ));
        assert!(html.contains("<title>A &lt;b&gt;&amp;</title>"));
        assert!(html.contains("<h1>A &lt;b&gt;&amp;</h1>"));
        assert!(html.contains("#&lt;script&gt;"));
        assert!(html.contains("#x&quot;y"));
        assert!(html.contains("<ul>\n<li>item &lt;i&gt;</li>\n</ul>"));
        assert!(html.contains("<p>plain &#39;text&#39;</p>"));
        assert!(html.contains("<pre><code class=\"language-sh\">echo &lt;x&gt;\n</code></pre>"));
        assert!(!html.contains("<script>"));
    }
}
