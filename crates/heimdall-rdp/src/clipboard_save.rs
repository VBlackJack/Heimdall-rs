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

//! The files the server copied, saved into a folder of this side (MS-RDPECLIP file
//! streams, read by this side).
//!
//! The server names everything: each name is made safe to write here, nothing is ever
//! written over, and no folder is entered that this copy did not make. The bytes come one
//! request at a time, each written before the next is asked for.

use std::collections::HashMap;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use ironrdp::cliprdr::pdu::{
    ClipboardFileAttributes, FileContentsFlags, FileContentsRequest, FileDescriptor,
};

use crate::clipboard_files::Limits;

/// Most bytes asked for at once.
const CHUNK: u32 = 1024 * 1024;

/// Longest name written, in UTF-16 units, as Windows takes it.
const MAX_NAME: usize = 255;

/// Most names tried for one entry: "name", then "name (2)" up to this.
const MAX_TRIES: u32 = 1_000;

/// Why the server's files are not saved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveRefusal {
    /// More files and folders than one copy takes.
    TooManyEntries,
    /// More bytes than one copy takes.
    TooLarge,
    /// The server did not say the size of a file.
    UnknownSize,
}

/// How saving the server's files ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveEnd {
    /// Every file and folder was saved: this many.
    Saved(usize),
    /// The server or the disk failed: what was saved before stays.
    Failed {
        /// Entries saved.
        saved: usize,
        /// Entries in the copy.
        total: usize,
    },
    /// The user stopped it: what was saved before stays.
    Cancelled {
        /// Entries saved.
        saved: usize,
        /// Entries in the copy.
        total: usize,
    },
    /// The copy was not saved at all.
    Refused(SaveRefusal),
}

/// A file or folder the server copied, as it is written here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Item {
    /// The server's index for it.
    pub(crate) index: i32,
    /// Its folders inside the copy, then its name: each safe to write here.
    pub(crate) components: Vec<String>,
    /// A folder.
    pub(crate) directory: bool,
    /// Its bytes, as the server said.
    pub(crate) size: u64,
}

/// What is saved of the server's list `files`: an entry whose name cannot be made safe is
/// left out.
///
/// # Errors
///
/// More than `limits` allow, or a file of unknown size: the copy is refused whole.
pub(crate) fn plan(files: &[FileDescriptor], limits: Limits) -> Result<Vec<Item>, SaveRefusal> {
    let mut items = Vec::new();
    let mut bytes: u64 = 0;
    for (index, file) in files.iter().enumerate() {
        let Ok(index) = i32::try_from(index) else {
            return Err(SaveRefusal::TooManyEntries);
        };
        let names = file
            .relative_path
            .iter()
            .flat_map(|path| path.split(['\\', '/']))
            .filter(|part| !part.is_empty())
            .chain(std::iter::once(file.name.as_str()));
        let Some(components) = names.map(safe_name).collect::<Option<Vec<_>>>() else {
            continue;
        };
        let directory = file
            .attributes
            .is_some_and(|attributes| attributes.contains(ClipboardFileAttributes::DIRECTORY));
        let size = if directory {
            0
        } else {
            file.file_size.ok_or(SaveRefusal::UnknownSize)?
        };
        if items.len() >= limits.entries {
            return Err(SaveRefusal::TooManyEntries);
        }
        bytes = bytes.saturating_add(size);
        if bytes > limits.bytes {
            return Err(SaveRefusal::TooLarge);
        }
        items.push(Item {
            index,
            components,
            directory,
            size,
        });
    }
    Ok(items)
}

/// `name`, from the server, made safe to write as one file or folder name here; `None`
/// when nothing of it is left. What Windows would read otherwise becomes `_`: a drive or a
/// stream (`:`), a separator, a character Windows refuses, a control or a direction mark,
/// a trailing dot or space; a device name gets `_` after it.
fn safe_name(name: &str) -> Option<String> {
    if matches!(name, "" | "." | "..") {
        return None;
    }
    let mut safe: String = name
        .chars()
        .map(|character| if refused(character) { '_' } else { character })
        .collect();
    if safe.ends_with(['.', ' ']) {
        safe.pop();
        safe.push('_');
    }
    let stem_length = safe.find('.').unwrap_or(safe.len());
    if is_device(safe[..stem_length].trim_end()) {
        safe.insert(stem_length, '_');
    }
    (safe.encode_utf16().count() <= MAX_NAME).then_some(safe)
}

/// A character no name written here keeps.
fn refused(character: char) -> bool {
    character < ' '
        || matches!(
            character,
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' | '\u{7F}'
        )
        // Direction marks and overrides, which disguise an extension.
        || matches!(character, '\u{200E}' | '\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}')
}

/// Whether `stem` names a Windows device, whatever follows it.
fn is_device(stem: &str) -> bool {
    let upper = stem.to_uppercase();
    if matches!(
        upper.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
    ) {
        return true;
    }
    let mut characters = upper.chars();
    let prefix: String = characters.by_ref().take(3).collect();
    let rest: Vec<char> = characters.collect();
    matches!(prefix.as_str(), "COM" | "LPT")
        && matches!(rest.as_slice(), ['0'..='9' | '¹' | '²' | '³'])
}

/// What the writer is told to do next.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Command {
    /// Make the folder at these components.
    Folder(Vec<String>),
    /// Start the file at these components.
    Open(Vec<String>),
    /// Write these bytes at the end of the file started.
    Write(Vec<u8>),
    /// The file started is complete.
    Close,
}

/// Writes a copy under a folder of this side: never over what is there, never into a folder
/// it did not make.
#[derive(Debug)]
pub(crate) struct Writer {
    root: PathBuf,
    /// The folders this copy made, by their components lowercased: an entry of the server
    /// differing only in case goes into the same one.
    folders: HashMap<Vec<String>, PathBuf>,
    /// The folders made, in order.
    made: Vec<PathBuf>,
    /// The file being written, until complete.
    open: Option<(std::fs::File, PathBuf)>,
    /// The copy is complete: what it made stays, empty folders of the server's included.
    finished: bool,
}

impl Writer {
    pub(crate) fn new(root: PathBuf) -> Self {
        Self {
            root,
            folders: HashMap::new(),
            made: Vec::new(),
            open: None,
            finished: false,
        }
    }

    /// The copy is complete.
    pub(crate) fn finish(&mut self) {
        self.finished = true;
    }

    /// Does `command`.
    pub(crate) fn apply(&mut self, command: Command) -> std::io::Result<()> {
        match command {
            Command::Folder(components) => self.folder(&components).map(|_| ()),
            Command::Open(components) => {
                let Some((name, parents)) = components.split_last() else {
                    return Err(std::io::ErrorKind::InvalidInput.into());
                };
                let parent = self.folder(parents)?;
                let (path, file) = create_unique(&parent, name, |path| {
                    std::fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(path)
                })?;
                self.open = Some((file, path));
                Ok(())
            }
            Command::Write(data) => match &mut self.open {
                Some((file, _)) => file.write_all(&data),
                None => Err(std::io::ErrorKind::InvalidInput.into()),
            },
            Command::Close => match self.open.take() {
                Some((file, path)) => {
                    file.sync_all()?;
                    mark_from_elsewhere(&path);
                    Ok(())
                }
                None => Err(std::io::ErrorKind::InvalidInput.into()),
            },
        }
    }

    /// The folder at `components`, made with each folder above it when missing.
    fn folder(&mut self, components: &[String]) -> std::io::Result<PathBuf> {
        let mut path = self.root.clone();
        for depth in 1..=components.len() {
            let key: Vec<String> = components[..depth]
                .iter()
                .map(|component| component.to_lowercase())
                .collect();
            if let Some(made) = self.folders.get(&key) {
                path.clone_from(made);
                continue;
            }
            // Never into a folder that was there: a name taken gets " (2)" and so on.
            let (made, ()) = create_unique(&path, &components[depth - 1], |path| {
                std::fs::create_dir(path)
            })?;
            self.made.push(made.clone());
            self.folders.insert(key, made.clone());
            path = made;
        }
        Ok(path)
    }

    /// Stops: the file being written is deleted, and each folder made left empty.
    pub(crate) fn abandon(&mut self) {
        if let Some((file, path)) = self.open.take() {
            drop(file);
            let _ = std::fs::remove_file(path);
        }
        for folder in self.made.drain(..).rev() {
            // Only when empty: what was saved in it stays.
            let _ = std::fs::remove_dir(folder);
        }
    }
}

impl Drop for Writer {
    fn drop(&mut self) {
        // A copy never completed, the session gone with it: no file left half written, no
        // folder of its own left empty.
        if !self.finished {
            self.abandon();
        }
    }
}

/// Marks the file at `path` as come from elsewhere, as a browser marks a download, so
/// Windows warns before running it and opens documents in protected view.
#[cfg(windows)]
fn mark_from_elsewhere(path: &Path) {
    let mut stream = path.as_os_str().to_owned();
    stream.push(":Zone.Identifier");
    let _ = std::fs::write(stream, "[ZoneTransfer]\r\nZoneId=3\r\n");
}

/// Only Windows keeps where a file came from.
#[cfg(not(windows))]
fn mark_from_elsewhere(_: &Path) {}

/// Makes `name` in `parent` with `create`, or "name (2)" and so on while the name is taken.
fn create_unique<T>(
    parent: &Path,
    name: &str,
    create: impl Fn(&Path) -> std::io::Result<T>,
) -> std::io::Result<(PathBuf, T)> {
    for attempt in 1..=MAX_TRIES {
        let path = parent.join(numbered(name, attempt));
        match create(&path) {
            Ok(made) => return Ok((path, made)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            // Windows refuses a new file over a folder as a denial, not as a name taken.
            Err(error)
                if error.kind() == std::io::ErrorKind::PermissionDenied
                    && std::fs::symlink_metadata(&path).is_ok() => {}
            Err(error) => return Err(error),
        }
    }
    Err(std::io::ErrorKind::AlreadyExists.into())
}

/// `name` for its `attempt`: itself first, then "stem (2).extension" and so on.
fn numbered(name: &str, attempt: u32) -> String {
    if attempt == 1 {
        return name.to_owned();
    }
    match name.rfind('.') {
        Some(dot) if dot > 0 => format!("{} ({attempt}){}", &name[..dot], &name[dot..]),
        _ => format!("{name} ({attempt})"),
    }
}

/// What the download needs done next.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum SaveStep {
    /// Give the writer this, then report how it went.
    Write(Command),
    /// Ask the server for this, then report its answer.
    Ask(FileContentsRequest),
    /// Everything was saved.
    Done {
        /// Entries saved.
        saved: usize,
    },
    /// The copy stopped: the server or the disk failed.
    Failed {
        /// Entries saved before.
        saved: usize,
    },
}

/// What the download waits for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Waiting {
    Folder,
    Open,
    Written,
    Closed,
    /// The answer to this stream, of at most this many bytes.
    Answer {
        stream: u32,
        asked: u32,
    },
    Over,
}

/// The server's files fetched one request at a time, each answer written before the next
/// is asked for.
#[derive(Debug)]
pub(crate) struct Download {
    items: Vec<Item>,
    /// The item at work.
    current: usize,
    /// Bytes of the current file written.
    position: u64,
    waiting: Waiting,
    stream: u32,
    saved: usize,
    /// The server's lock on its copy: every request names it, so a new copy made there
    /// meanwhile is never read in its place.
    lock: Option<u32>,
}

impl Download {
    /// The download of `items`, under the server's `lock`; its requests numbered after
    /// `stream`, the last of an earlier save, so a late answer to one is never taken.
    pub(crate) fn new(items: Vec<Item>, lock: Option<u32>, stream: u32) -> Self {
        Self {
            items,
            current: 0,
            position: 0,
            waiting: Waiting::Over,
            stream,
            saved: 0,
            lock,
        }
    }

    /// The number of the last request.
    pub(crate) fn stream(&self) -> u32 {
        self.stream
    }

    /// The server's lock on its copy, when locks were agreed on.
    pub(crate) fn lock(&self) -> Option<u32> {
        self.lock
    }

    /// Entries in the copy.
    pub(crate) fn total(&self) -> usize {
        self.items.len()
    }

    /// Entries saved so far.
    pub(crate) fn saved(&self) -> usize {
        self.saved
    }

    /// The first step.
    pub(crate) fn start(&mut self) -> SaveStep {
        self.begin()
    }

    /// The writer did the last command, or failed to.
    pub(crate) fn written(&mut self, ok: bool) -> Option<SaveStep> {
        if !ok {
            return Some(self.fail());
        }
        Some(match self.waiting {
            Waiting::Folder | Waiting::Closed => {
                self.saved += 1;
                self.current += 1;
                self.begin()
            }
            Waiting::Open => {
                self.position = 0;
                self.next_piece()
            }
            Waiting::Written => self.next_piece(),
            Waiting::Answer { .. } | Waiting::Over => return None,
        })
    }

    /// The server answered `stream`: its bytes, or `None` when it failed. An answer to
    /// anything but the request waited for is ignored.
    pub(crate) fn answered(&mut self, stream: u32, data: Option<Vec<u8>>) -> Option<SaveStep> {
        let Waiting::Answer {
            stream: waited,
            asked,
        } = self.waiting
        else {
            return None;
        };
        if stream != waited {
            return None;
        }
        let Some(data) = data else {
            return Some(self.fail());
        };
        // Nothing, or more than asked: the server cannot be followed.
        let received = u32::try_from(data.len()).unwrap_or(u32::MAX);
        if received == 0 || received > asked {
            return Some(self.fail());
        }
        self.position += u64::from(received);
        self.waiting = Waiting::Written;
        Some(SaveStep::Write(Command::Write(data)))
    }

    /// The copy can no longer be read: the server's lock is gone.
    pub(crate) fn lost(&mut self) -> SaveStep {
        self.fail()
    }

    fn begin(&mut self) -> SaveStep {
        let Some(item) = self.items.get(self.current) else {
            self.waiting = Waiting::Over;
            return SaveStep::Done { saved: self.saved };
        };
        let components = item.components.clone();
        if item.directory {
            self.waiting = Waiting::Folder;
            SaveStep::Write(Command::Folder(components))
        } else {
            self.waiting = Waiting::Open;
            SaveStep::Write(Command::Open(components))
        }
    }

    fn next_piece(&mut self) -> SaveStep {
        let item = &self.items[self.current];
        if self.position >= item.size {
            self.waiting = Waiting::Closed;
            return SaveStep::Write(Command::Close);
        }
        self.stream = self.stream.wrapping_add(1);
        let asked = u32::try_from(item.size - self.position).map_or(CHUNK, |left| left.min(CHUNK));
        self.waiting = Waiting::Answer {
            stream: self.stream,
            asked,
        };
        SaveStep::Ask(FileContentsRequest {
            stream_id: self.stream,
            index: item.index,
            flags: FileContentsFlags::RANGE,
            position: self.position,
            requested_size: asked,
            data_id: self.lock,
        })
    }

    fn fail(&mut self) -> SaveStep {
        self.waiting = Waiting::Over;
        SaveStep::Failed { saved: self.saved }
    }
}

#[cfg(test)]
mod tests {
    use ironrdp::cliprdr::pdu::{ClipboardFileAttributes, FileContentsFlags, FileDescriptor};

    use super::{
        CHUNK, Command, Download, Item, SaveRefusal, SaveStep, Writer, numbered, plan, safe_name,
    };
    use crate::clipboard_files::{COPY_LIMITS, Limits};

    fn file(name: &str, folder: Option<&str>, size: Option<u64>) -> FileDescriptor {
        let mut file = FileDescriptor::new(name);
        file.relative_path = folder.map(str::to_owned);
        file.file_size = size;
        file
    }

    fn folder(name: &str, parent: Option<&str>) -> FileDescriptor {
        file(name, parent, None).with_attributes(ClipboardFileAttributes::DIRECTORY)
    }

    fn names(components: &[&str]) -> Vec<String> {
        components.iter().map(|&name| name.to_owned()).collect()
    }

    #[test]
    fn a_name_from_the_server_is_written_only_once_made_safe() {
        for (given, written) in [
            ("report.docx", Some("report.docx")),
            ("C:evil.txt", Some("C_evil.txt")),
            ("a.txt:hidden", Some("a.txt_hidden")),
            ("CON", Some("CON_")),
            ("con.txt", Some("con_.txt")),
            ("CON .txt", Some("CON _.txt")),
            ("COM¹.log", Some("COM¹_.log")),
            ("LPT0", Some("LPT0_")),
            ("CONIN$", Some("CONIN$_")),
            ("COM10", Some("COM10")),
            ("console.log", Some("console.log")),
            ("name.", Some("name_")),
            ("name ", Some("name_")),
            ("tab\there", Some("tab_here")),
            ("invoice\u{202E}fdp.exe", Some("invoice_fdp.exe")),
            ("", None),
            (".", None),
            ("..", None),
        ] {
            assert_eq!(safe_name(given).as_deref(), written, "{given:?}");
        }
        assert_eq!(safe_name(&"x".repeat(256)), None, "too long for Windows");
    }

    #[test]
    fn the_servers_list_becomes_what_is_written_each_part_made_safe() {
        let items = plan(
            &[
                folder("logs", None),
                file("a.txt", Some("logs"), Some(3)),
                file("b:c", Some("logs\\C:\\x"), Some(0)),
                file("gone", Some(".."), Some(1)),
            ],
            COPY_LIMITS,
        )
        .expect("planned");
        assert_eq!(
            items,
            [
                Item {
                    index: 0,
                    components: names(&["logs"]),
                    directory: true,
                    size: 0,
                },
                Item {
                    index: 1,
                    components: names(&["logs", "a.txt"]),
                    directory: false,
                    size: 3,
                },
                Item {
                    index: 2,
                    components: names(&["logs", "C_", "x", "b_c"]),
                    directory: false,
                    size: 0,
                },
            ],
            "the entry naming `..` is left out, the others keep their index"
        );
    }

    #[test]
    fn a_copy_beyond_its_limits_or_of_unknown_size_is_refused_whole() {
        let three = [
            file("a", None, Some(10)),
            file("b", None, Some(10)),
            file("c", None, Some(10)),
        ];
        let few = Limits {
            entries: 2,
            bytes: 1_000,
        };
        assert_eq!(plan(&three, few), Err(SaveRefusal::TooManyEntries));
        let small = Limits {
            entries: 100,
            bytes: 25,
        };
        assert_eq!(plan(&three, small), Err(SaveRefusal::TooLarge));
        assert_eq!(
            plan(&[file("a", None, None)], COPY_LIMITS),
            Err(SaveRefusal::UnknownSize)
        );
    }

    #[test]
    fn a_name_taken_gets_a_number_before_its_extension() {
        assert_eq!(numbered("report.docx", 1), "report.docx");
        assert_eq!(numbered("report.docx", 2), "report (2).docx");
        assert_eq!(numbered("Makefile", 3), "Makefile (3)");
        assert_eq!(numbered(".bashrc", 2), ".bashrc (2)");
    }

    #[test]
    fn nothing_there_is_written_over_nor_entered() {
        let dir = tempfile::tempdir().expect("dir");
        std::fs::write(dir.path().join("a.txt"), b"mine").expect("mine");
        std::fs::create_dir(dir.path().join("Docs")).expect("docs");
        std::fs::write(dir.path().join("Docs").join("b.txt"), b"mine too").expect("b");

        let mut writer = Writer::new(dir.path().to_path_buf());
        writer
            .apply(Command::Open(names(&["a.txt"])))
            .expect("open");
        writer
            .apply(Command::Write(b"theirs".to_vec()))
            .expect("write");
        writer.apply(Command::Close).expect("close");
        writer
            .apply(Command::Folder(names(&["Docs"])))
            .expect("folder");
        writer
            .apply(Command::Open(names(&["docs", "b.txt"])))
            .expect("open in the copy's folder, whatever its case");
        writer.apply(Command::Close).expect("close");

        assert_eq!(
            std::fs::read(dir.path().join("a.txt")).expect("kept"),
            b"mine"
        );
        assert_eq!(
            std::fs::read(dir.path().join("a (2).txt")).expect("saved"),
            b"theirs"
        );
        assert_eq!(
            std::fs::read(dir.path().join("Docs").join("b.txt")).expect("kept"),
            b"mine too"
        );
        assert!(dir.path().join("Docs (2)").join("b.txt").is_file());
    }

    #[test]
    fn a_folder_missing_from_the_list_is_made_and_a_stop_keeps_only_what_was_saved() {
        let dir = tempfile::tempdir().expect("dir");
        let mut writer = Writer::new(dir.path().to_path_buf());
        writer
            .apply(Command::Open(names(&["kept", "done.txt"])))
            .expect("open");
        writer.apply(Command::Close).expect("close");
        writer
            .apply(Command::Folder(names(&["empty"])))
            .expect("folder");
        writer
            .apply(Command::Open(names(&["partial", "half.bin"])))
            .expect("open");
        writer.apply(Command::Write(vec![1; 10])).expect("write");
        writer.abandon();

        assert!(dir.path().join("kept").join("done.txt").is_file());
        assert!(!dir.path().join("partial").exists(), "nothing half written");
        assert!(!dir.path().join("empty").exists(), "folders left empty go");
    }

    #[test]
    fn a_file_never_completed_is_not_left_behind() {
        let dir = tempfile::tempdir().expect("dir");
        {
            let mut writer = Writer::new(dir.path().to_path_buf());
            writer
                .apply(Command::Open(names(&["half.bin"])))
                .expect("open");
            writer.apply(Command::Write(vec![1; 10])).expect("write");
        }
        assert!(!dir.path().join("half.bin").exists());
    }

    #[cfg(unix)]
    #[test]
    fn a_link_already_there_is_never_entered() {
        let dir = tempfile::tempdir().expect("dir");
        let elsewhere = tempfile::tempdir().expect("elsewhere");
        std::os::unix::fs::symlink(elsewhere.path(), dir.path().join("Docs")).expect("link");
        let mut writer = Writer::new(dir.path().to_path_buf());
        writer
            .apply(Command::Open(names(&["Docs", "x.txt"])))
            .expect("open");
        writer.apply(Command::Close).expect("close");
        assert!(dir.path().join("Docs (2)").join("x.txt").is_file());
        assert_eq!(
            std::fs::read_dir(elsewhere.path()).expect("read").count(),
            0
        );
    }

    fn asked(action: &SaveStep) -> (u32, u64, u32, Option<u32>) {
        let SaveStep::Ask(request) = action else {
            panic!("a request, not {action:?}");
        };
        assert_eq!(request.flags, FileContentsFlags::RANGE);
        (
            request.stream_id,
            request.position,
            request.requested_size,
            request.data_id,
        )
    }

    #[test]
    fn each_file_is_asked_for_piece_by_piece_and_written_before_the_next() {
        let size = u64::from(CHUNK) + 5;
        let mut download = Download::new(
            vec![
                Item {
                    index: 0,
                    components: names(&["d"]),
                    directory: true,
                    size: 0,
                },
                Item {
                    index: 3,
                    components: names(&["d", "big.bin"]),
                    directory: false,
                    size,
                },
                Item {
                    index: 4,
                    components: names(&["empty"]),
                    directory: false,
                    size: 0,
                },
            ],
            Some(9),
            0,
        );
        assert_eq!(
            download.start(),
            SaveStep::Write(Command::Folder(names(&["d"])))
        );
        assert_eq!(
            download.written(true),
            Some(SaveStep::Write(Command::Open(names(&["d", "big.bin"]))))
        );
        let first = download.written(true).expect("asked");
        assert_eq!(asked(&first), (1, 0, CHUNK, Some(9)), "the lock named");
        assert_eq!(download.written(true), None, "an answer is waited for");
        assert_eq!(download.answered(7, Some(vec![0])), None, "another stream");

        // A short answer: the rest is asked from where it stopped.
        let piece = vec![1; 100];
        assert_eq!(
            download.answered(1, Some(piece.clone())),
            Some(SaveStep::Write(Command::Write(piece)))
        );
        let second = download.written(true).expect("asked");
        let rest = CHUNK - 95;
        assert_eq!(
            asked(&second),
            (2, 100, rest, Some(9)),
            "no more than is left"
        );
        let rest_bytes = vec![2; usize::try_from(rest).expect("fits")];
        assert!(download.answered(2, Some(rest_bytes)).is_some());
        assert_eq!(
            download.written(true),
            Some(SaveStep::Write(Command::Close))
        );
        assert_eq!(
            download.written(true),
            Some(SaveStep::Write(Command::Open(names(&["empty"]))))
        );
        assert_eq!(
            download.written(true),
            Some(SaveStep::Write(Command::Close)),
            "an empty file: nothing asked"
        );
        assert_eq!(download.written(true), Some(SaveStep::Done { saved: 3 }));
    }

    #[test]
    fn requests_go_on_from_the_last_save() {
        let mut download = Download::new(
            vec![Item {
                index: 0,
                components: names(&["f"]),
                directory: false,
                size: 10,
            }],
            None,
            40,
        );
        download.start();
        let first = download.written(true).expect("asked");
        assert_eq!(asked(&first).0, 41);
        assert_eq!(download.stream(), 41);
        assert_eq!(
            download.answered(1, Some(vec![0])),
            None,
            "an earlier save's"
        );
    }

    #[test]
    fn a_copy_never_completed_leaves_no_folder_of_its_own_and_one_completed_keeps_them() {
        let dir = tempfile::tempdir().expect("dir");
        {
            let mut writer = Writer::new(dir.path().to_path_buf());
            writer
                .apply(Command::Folder(names(&["gone"])))
                .expect("folder");
        }
        assert!(
            !dir.path().join("gone").exists(),
            "the session ended mid copy"
        );
        {
            let mut writer = Writer::new(dir.path().to_path_buf());
            writer
                .apply(Command::Folder(names(&["empty"])))
                .expect("folder");
            writer.finish();
        }
        assert!(
            dir.path().join("empty").is_dir(),
            "the server's empty folder"
        );
    }

    #[test]
    fn a_server_that_cannot_be_followed_stops_the_copy() {
        let item = Item {
            index: 0,
            components: names(&["f"]),
            directory: false,
            size: 10,
        };
        for answer in [None, Some(Vec::new()), Some(vec![0; 11])] {
            let mut download = Download::new(vec![item.clone()], None, 0);
            download.start();
            download.written(true).expect("asked");
            assert_eq!(
                download.answered(1, answer),
                Some(SaveStep::Failed { saved: 0 })
            );
        }
        let mut download = Download::new(vec![item], None, 0);
        download.start();
        assert_eq!(download.written(false), Some(SaveStep::Failed { saved: 0 }));
    }

    #[test]
    fn an_empty_file_is_written_without_asking_and_the_end_counts_what_was_saved() {
        let mut download = Download::new(
            vec![Item {
                index: 0,
                components: names(&["empty"]),
                directory: false,
                size: 0,
            }],
            None,
            0,
        );
        download.start();
        assert_eq!(
            download.written(true),
            Some(SaveStep::Write(Command::Close))
        );
        assert_eq!(download.written(true), Some(SaveStep::Done { saved: 1 }));
        assert_eq!((download.saved(), download.total()), (1, 1));
    }
}
