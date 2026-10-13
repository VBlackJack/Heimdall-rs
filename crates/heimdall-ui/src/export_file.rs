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

//! Where "Export Sessions" writes: the system's save dialog, as the C# `SaveFileDialog`,
//! offering `servers.json` and held by the window as the C# one is, then the file written.

use std::path::{Path, PathBuf};

use heimdall_app::ExportOutcome;
use heimdall_core::export::EXPORT_FILE_NAME;

use crate::file_dialog::{self, FileDialog};

/// Extension the dialog offers.
const JSON_EXTENSION: &str = "json";

/// The file the user picks in the save dialog, once it closes; `None` when cancelled.
pub type Pick = file_dialog::Pick<PathBuf>;

/// Opens the save dialog titled `title`, its files named `filter`, over `parent` when there
/// is one: the dialog then belongs to the window, in front of it and modal, as the C# one.
#[must_use]
pub fn dialog(title: String, filter: String, parent: Option<&dyn iced::window::Window>) -> Pick {
    let mut dialog = FileDialog::new()
        .set_title(title)
        .set_file_name(EXPORT_FILE_NAME)
        .add_filter(filter, &[JSON_EXTENSION]);
    if let Some(parent) = parent {
        dialog = dialog.set_parent(parent);
    }
    dialog.save_file()
}

/// Waits for the file picked, then writes `document` there; why not when no dialog could
/// be shown.
pub async fn save(pick: Pick, document: String, count: usize) -> ExportOutcome {
    match pick.await {
        Ok(Some(path)) => write(&path, document, count).await,
        Ok(None) => ExportOutcome::Cancelled,
        Err(unavailable) => ExportOutcome::Failed(unavailable.message()),
    }
}

/// Writes `document` at `path`, UTF-8 without a byte order mark, as the C# writes it.
pub async fn write(path: &Path, document: String, count: usize) -> ExportOutcome {
    match tokio::fs::write(path, document).await {
        Ok(()) => ExportOutcome::Saved(count),
        Err(error) => ExportOutcome::Failed(error.to_string()),
    }
}
