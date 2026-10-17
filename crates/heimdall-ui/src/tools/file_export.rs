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

//! A tool's export saved to a file, as the C# tools' export buttons do it: a
//! `SaveFileDialog` offering a name made of the tool, its target and the moment, then
//! `File.WriteAllText(path, text, Encoding.UTF8)`. The file is written whole or not at all,
//! through [`heimdall_core::store::replace_user_file`].
//!
//! Never through the clipboard: a scan's results or an audit's evidence put there would
//! outlive the export in the clipboard's history and its cloud sync.

use std::fmt;
use std::path::{Path, PathBuf};

use chrono::NaiveDateTime;
use heimdall_app::TabId;
use iced::{Task, window};

use super::{CopySlot, ToolMessage};
use crate::file_dialog::{FileDialog, Pick};
use crate::i18n::fl;
use crate::shell::Message;

/// What the C# `Encoding.UTF8` writes first: a spreadsheet then reads the accents of a CSV
/// right instead of guessing the system's code page.
const UTF8_BOM: &[u8] = &[0xEF, 0xBB, 0xBF];

/// The moment in a suggested name, as the C# `DateTime.Now:yyyyMMdd_HHmmss`.
const TIMESTAMP_FORMAT: &str = "%Y%m%d_%H%M%S";

/// What joins the parts of a suggested name, and stands for a character no file name takes.
const SEPARATOR: char = '_';

/// What a file name may not hold beyond the control characters, as .NET's
/// `Path.GetInvalidFileNameChars()` on Windows: a host such as `fe80::1` or a subnet such as
/// `10.0.0.0/24` would otherwise name a folder, or nothing.
const INVALID_FILE_NAME_CHARACTERS: [char; 9] = ['<', '>', ':', '"', '/', '\\', '|', '?', '*'];

/// The kind of file an export writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// Comma-separated values, as the C# `FileDialogCsvFilter`.
    Csv,
    /// A standalone HTML document, as the C# `FileDialogHtmlFilter`.
    Html,
}

impl Format {
    /// The extension the dialog offers, without its dot.
    #[must_use]
    pub const fn extension(self) -> &'static str {
        match self {
            Self::Csv => "csv",
            Self::Html => "html",
        }
    }

    /// The name of the dialog's filter, in the user's language.
    fn filter(self) -> String {
        match self {
            Self::Csv => fl!("ui-tool-export-filter-csv"),
            Self::Html => fl!("ui-tool-export-filter-html"),
        }
    }
}

/// The name a save dialog offers: `prefix`, the target `subject` when there is one, and the
/// moment `now`, as the C# `ping_{host}_{now:yyyyMMdd_HHmmss}.csv`. The subject is trimmed
/// and every character a file name may not hold replaced by `_`.
#[must_use]
pub fn file_name(
    prefix: &str,
    subject: Option<&str>,
    format: Format,
    now: NaiveDateTime,
) -> String {
    let mut name = prefix.to_owned();
    if let Some(subject) = subject.map(str::trim).filter(|subject| !subject.is_empty()) {
        name.push(SEPARATOR);
        name.extend(subject.chars().map(|c| {
            if c.is_control() || INVALID_FILE_NAME_CHARACTERS.contains(&c) {
                SEPARATOR
            } else {
                c
            }
        }));
    }
    name.push(SEPARATOR);
    name.push_str(&now.format(TIMESTAMP_FORMAT).to_string());
    name.push('.');
    name.push_str(format.extension());
    name
}

/// [`file_name`] at the local time now, as the C# `DateTime.Now`.
#[must_use]
pub fn file_name_now(prefix: &str, subject: Option<&str>, format: Format) -> String {
    file_name(prefix, subject, format, chrono::Local::now().naive_local())
}

/// An export waiting for the place it is saved: the button it answers, the dialog's title
/// and suggested name, and the text written.
pub struct FileExport {
    slot: CopySlot,
    format: Format,
    title: String,
    file_name: String,
    content: String,
}

impl fmt::Debug for FileExport {
    /// The content is a scan's results or an audit's evidence: never written to a log.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FileExport")
            .field("slot", &self.slot)
            .field("format", &self.format)
            .field("file_name", &self.file_name)
            .finish_non_exhaustive()
    }
}

impl FileExport {
    /// `content` in `format`, for the button of `slot`, its dialog titled `title` and
    /// offering `file_name`.
    #[must_use]
    pub fn new(
        slot: CopySlot,
        format: Format,
        title: String,
        file_name: String,
        content: String,
    ) -> Self {
        Self {
            slot,
            format,
            title,
            file_name,
            content,
        }
    }

    /// A CSV export of the button of `slot`, titled `title`, named after `prefix` and
    /// `subject` (see [`file_name`]).
    #[must_use]
    pub fn csv(
        slot: CopySlot,
        title: String,
        prefix: &str,
        subject: Option<&str>,
        content: String,
    ) -> Self {
        let name = file_name_now(prefix, subject, Format::Csv);
        Self::new(slot, Format::Csv, title, name, content)
    }

    /// An HTML export, as [`FileExport::csv`].
    #[must_use]
    pub fn html(
        slot: CopySlot,
        title: String,
        prefix: &str,
        subject: Option<&str>,
        content: String,
    ) -> Self {
        let name = file_name_now(prefix, subject, Format::Html);
        Self::new(slot, Format::Html, title, name, content)
    }

    /// The button it answers.
    #[must_use]
    pub fn slot(&self) -> CopySlot {
        self.slot
    }

    /// The kind of file it writes.
    #[must_use]
    pub fn format(&self) -> Format {
        self.format
    }

    /// The name the dialog offers.
    #[must_use]
    pub fn suggested_name(&self) -> &str {
        &self.file_name
    }

    /// The text written.
    #[must_use]
    pub fn content(&self) -> &str {
        &self.content
    }

    /// The save dialog over the main window `main`, then the file written off the window's
    /// thread; how it went told to tool tab `tab` as [`ToolMessage::Exported`].
    pub fn task(self, tab: TabId, main: Option<window::Id>) -> Task<Message> {
        let Self {
            slot,
            format,
            title,
            file_name,
            content,
        } = self;
        crate::shell::main_window_task(main).then(move |id| {
            let dialog = {
                let (title, file_name) = (title.clone(), file_name.clone());
                move || {
                    FileDialog::new()
                        .set_title(title)
                        .set_file_name(file_name)
                        .add_filter(format.filter(), &[format.extension()])
                }
            };
            let pick = match id {
                Some(id) => window::run(id, move |window| dialog().set_parent(window).save_file()),
                None => Task::done(dialog().save_file()),
            };
            let content = content.clone();
            pick.then(move |pick: Pick<PathBuf>| {
                Task::perform(save(pick, content.clone()), move |saved| {
                    Message::Tool(tab, ToolMessage::Exported(slot, saved))
                })
            })
        })
    }
}

/// How an export went.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Saved {
    /// The file is written.
    Written,
    /// The dialog was closed without a place.
    Cancelled,
    /// No dialog could be shown, or the file could not be written, for this reason.
    Failed(String),
}

/// Waits for the place picked, then writes `content` there, off the window's thread.
pub async fn save(pick: Pick<PathBuf>, content: String) -> Saved {
    match pick.await {
        Ok(Some(path)) => tokio::task::spawn_blocking(move || write(&path, &content))
            .await
            .unwrap_or_else(|error| Saved::Failed(error.to_string())),
        Ok(None) => Saved::Cancelled,
        Err(unavailable) => Saved::Failed(unavailable.message()),
    }
}

/// Writes `content` at `path`, UTF-8 after its byte order mark as the C# `Encoding.UTF8`
/// writes it, through a temporary file renamed over it: an interruption leaves the old file
/// or the new one, never a truncated export.
#[must_use]
pub fn write(path: &Path, content: &str) -> Saved {
    let mut bytes = Vec::with_capacity(UTF8_BOM.len() + content.len());
    bytes.extend_from_slice(UTF8_BOM);
    bytes.extend_from_slice(content.as_bytes());
    match heimdall_core::store::replace_user_file(path, &bytes) {
        Ok(()) => Saved::Written,
        Err(error) => Saved::Failed(error.to_string()),
    }
}
