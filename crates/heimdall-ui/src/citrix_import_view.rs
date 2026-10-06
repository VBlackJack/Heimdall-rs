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

//! "Import Citrix Apps" in the window, as the C# dialogs say it: the number of applications
//! asked about, then how many were imported, with what the scan said.

use heimdall_app::{Dialog, server_text};
use heimdall_core::import::citrix_cache::{CacheScan, CacheWarning};

use crate::i18n::fl;

/// Warnings shown under the result at most, as the C# shows them.
const MAX_WARNINGS_SHOWN: usize = 5;

/// The title, question and action asked before the applications of `scan` are imported.
#[must_use]
pub fn question(scan: &CacheScan) -> (String, String, String) {
    (
        fl!("ui-citrix-import-title"),
        fl!("ui-citrix-import-confirm", count = scan.apps.len()),
        fl!("ui-import-file-button"),
    )
}

/// What the import said when it ended, or that the cache gives nothing: the title and the
/// lines under it.
#[must_use]
pub fn report_lines(dialog: &Dialog) -> Option<(String, Vec<String>)> {
    let lines = match dialog {
        Dialog::CitrixImportNothing { warnings } if warnings.is_empty() => {
            vec![fl!("ui-citrix-import-none")]
        }
        Dialog::CitrixImportNothing { warnings } => warnings.iter().map(warning).collect(),
        Dialog::CitrixImportDone(outcome) => {
            let mut lines = vec![fl!("ui-citrix-import-done", count = outcome.added)];
            if outcome.refreshed > 0 {
                lines.push(fl!("ui-citrix-import-refreshed", count = outcome.refreshed));
            }
            if outcome.without_launch_lines {
                lines.push(fl!("ui-citrix-import-no-launch-lines"));
            }
            lines.extend(
                outcome
                    .warnings
                    .iter()
                    .take(MAX_WARNINGS_SHOWN)
                    .map(warning),
            );
            lines
        }
        _ => return None,
    };
    Some((fl!("ui-citrix-import-title"), lines))
}

/// What a scan warning says, as the C# warnings.
fn warning(warning: &CacheWarning) -> String {
    match warning {
        CacheWarning::FolderMissing => fl!("ui-citrix-cache-folder-missing"),
        CacheWarning::NoCacheFiles => fl!("ui-citrix-cache-no-files"),
        CacheWarning::Unreadable { file, detail } => fl!(
            "ui-citrix-cache-unreadable",
            file = server_text(file),
            detail = server_text(detail)
        ),
    }
}
