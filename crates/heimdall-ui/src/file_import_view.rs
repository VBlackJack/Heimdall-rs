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

//! "Import Sessions" in the window, as the C# one: one open dialog for every format it
//! reads, the number of sessions asked about, then what was imported, with what the file
//! said and, for `MobaXterm`, that passwords must be entered again.

use std::path::{Path, PathBuf};

use heimdall_app::{FileKind, ImportFile, PendingImport};
use heimdall_core::import::foreign::FileWarning;
use heimdall_core::paths::{LEGACY_SERVERS_FILE_NAME, LEGACY_SETTINGS_FILE_NAME};

use crate::i18n::fl;
use crate::sessions_view::Pick;

/// Largest file read, as the other imports: a larger one is refused before it is read.
pub const MAX_FILE_BYTES: u64 = 50 * 1024 * 1024;

/// Extension of a Heimdall document.
const JSON_EXTENSIONS: [&str; 1] = ["json"];

/// Extensions of a `MobaXterm` file.
const MOBAXTERM_EXTENSIONS: [&str; 3] = ["mxtsessions", "mobaconf", "ini"];

/// Extension of an `mRemoteNG` file.
const MREMOTENG_EXTENSIONS: [&str; 1] = ["xml"];

/// Extension of an `RDCMan` file.
const RDCMAN_EXTENSIONS: [&str; 1] = ["rdg"];

/// Extension of a Remote Desktop file, imported by the `.rdp` import.
pub const RDP_EXTENSION: &str = "rdp";

/// The filter that shows every file.
const ANY_EXTENSION: [&str; 1] = ["*"];

/// Names of the formats, product names shown as they are in every language.
const MOBAXTERM: &str = "MobaXterm";
const MREMOTENG: &str = "mRemoteNG";
const RDCMAN: &str = "RDCMan";

/// Opens the system's open dialog over `parent` when there is one, filtered as the C#
/// `ImportDialogFilterAll`.
#[must_use]
pub fn pick(parent: Option<&dyn iced::window::Window>) -> Pick {
    let all: Vec<&str> = JSON_EXTENSIONS
        .iter()
        .chain(&MOBAXTERM_EXTENSIONS)
        .chain(&MREMOTENG_EXTENSIONS)
        .chain(&RDCMAN_EXTENSIONS)
        .chain(&[RDP_EXTENSION])
        .copied()
        .collect();
    let mut dialog = rfd::AsyncFileDialog::new()
        .set_title(fl!("ui-import-file-title"))
        .add_filter(fl!("ui-import-file-filter-all"), &all)
        .add_filter(fl!("ui-import-file-filter-json"), &JSON_EXTENSIONS)
        .add_filter(MOBAXTERM, &MOBAXTERM_EXTENSIONS)
        .add_filter(MREMOTENG, &MREMOTENG_EXTENSIONS)
        .add_filter(RDCMAN, &RDCMAN_EXTENSIONS)
        .add_filter(fl!("ui-import-file-filter-rdp"), &[RDP_EXTENSION])
        .add_filter(fl!("ui-import-file-filter-any"), &ANY_EXTENSION);
    if let Some(parent) = parent {
        dialog = dialog.set_parent(&parent);
    }
    Box::pin(dialog.pick_file())
}

/// Whether `path` is a Remote Desktop file, which the `.rdp` import reads.
#[must_use]
pub fn is_rdp(path: &Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case(RDP_EXTENSION))
}

/// The file at `path` read, with the `settings.json` beside it when it is a `servers.json`;
/// or why it could not be read. A file larger than [`MAX_FILE_BYTES`] is not read.
///
/// # Errors
///
/// The path and why, when the file is too large or cannot be read as UTF-8 text.
pub async fn read(path: PathBuf) -> Result<ImportFile, String> {
    let size = tokio::fs::metadata(&path)
        .await
        .map_err(|error| format!("{}: {error}", path.display()))?
        .len();
    if size > MAX_FILE_BYTES {
        return Err(fl!("ui-import-file-too-large", size = size.to_string()));
    }
    let text = crate::sessions_view::read_file(&path).await?;
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    // As the application's own import: a `servers.json` takes its settings with it.
    let settings = if name.eq_ignore_ascii_case(LEGACY_SERVERS_FILE_NAME) {
        match path.parent() {
            Some(folder) => tokio::fs::read_to_string(folder.join(LEGACY_SETTINGS_FILE_NAME))
                .await
                .ok(),
            None => None,
        }
    } else {
        None
    };
    Ok(ImportFile {
        name,
        text,
        settings,
    })
}

/// The question before `pending` is imported, as the C# asks it.
#[must_use]
pub fn question(pending: &PendingImport) -> (String, String, String) {
    let count = pending.count();
    let body = if pending.kind == FileKind::MobaXterm {
        fl!("ui-import-file-confirm-mobaxterm", count = count)
    } else {
        fl!("ui-import-file-confirm", count = count)
    };
    (
        fl!("ui-import-file-title"),
        body,
        fl!("ui-import-file-button"),
    )
}

/// What a file said as a whole, a line each.
#[must_use]
pub fn warning_lines(warnings: &[FileWarning]) -> Vec<String> {
    warnings
        .iter()
        .map(|warning| match warning {
            FileWarning::Unreadable(detail) => {
                fl!("ui-import-file-unreadable", detail = detail.as_str())
            }
            FileWarning::FullyEncrypted => fl!("ui-import-file-encrypted"),
        })
        .collect()
}

/// For a `MobaXterm` file, that its passwords must be entered again: how many it stores
/// when it says, as the C# notice.
#[must_use]
pub fn password_notice(stored_credentials: Option<usize>) -> Option<String> {
    stored_credentials.map(|count| {
        if count > 0 {
            fl!("ui-import-mobaxterm-passwords-detected", count = count)
        } else {
            fl!("ui-import-mobaxterm-passwords")
        }
    })
}
