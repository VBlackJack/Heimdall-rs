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

//! "Import Sessions", as the C# Heimdall's: a file picked, a Heimdall document or a
//! `MobaXterm`, `mRemoteNG` or `RDCMan` file, its kind told by its extension; the number of
//! sessions asked about, then merged, with what was left out and, for `MobaXterm`, that
//! passwords must be entered again.
//!
//! A file can be written by someone else, so a trust decision it carries is not taken: as the
//! C# `ImportedProfileSanitizer`, a `WinRM` profile comes in checking its host's certificate.
//! The C# Heimdall's own files on this machine, read by the home page's "Import Connections",
//! keep theirs: that decision was made here.

use std::fmt;

use heimdall_core::import::csharp::{self, ImportReport};
use heimdall_core::import::foreign::{FileWarning, Parsed};
use heimdall_core::import::{mobaxterm, mremoteng, rdcman};
use heimdall_core::store::MergeReport;

use super::{App, Dialog, Effect, ImportSummary, server_text};

/// Extensions of a `MobaXterm` file, compared without case.
const MOBAXTERM_EXTENSIONS: [&str; 3] = ["mxtsessions", "mobaconf", "ini"];

/// Extension of an `RDCMan` file.
const RDCMAN_EXTENSION: &str = "rdg";

/// Extension of an XML file, `mRemoteNG`'s or `RDCMan`'s by its root.
const XML_EXTENSION: &str = "xml";

/// Root elements of an `RDCMan` file, found in an XML one.
const RDCMAN_ROOTS: [&str; 2] = ["<RDCMan", "<file"];

/// What a file is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    /// A Heimdall session document, the C# export or its `servers.json`.
    Heimdall,
    /// `MobaXterm` sessions.
    MobaXterm,
    /// `mRemoteNG` connections.
    MRemoteNg,
    /// Remote Desktop Connection Manager groups.
    RdcMan,
}

impl FileKind {
    /// The kind of the file named `name`, holding `text`: by extension, an XML file by its
    /// root, anything else a Heimdall document as the C# reads it.
    #[must_use]
    pub fn of(name: &str, text: &str) -> Self {
        let extension = name
            .rsplit_once('.')
            .map(|(_, extension)| extension.to_ascii_lowercase())
            .unwrap_or_default();
        if MOBAXTERM_EXTENSIONS.contains(&extension.as_str()) {
            Self::MobaXterm
        } else if extension == RDCMAN_EXTENSION {
            Self::RdcMan
        } else if extension == XML_EXTENSION {
            if RDCMAN_ROOTS.iter().any(|root| text.contains(root)) {
                Self::RdcMan
            } else {
                Self::MRemoteNg
            }
        } else {
            Self::Heimdall
        }
    }
}

/// A file read, to import.
#[derive(Clone, PartialEq, Eq)]
pub struct ImportFile {
    /// Its name, which tells its kind.
    pub name: String,
    /// Its text.
    pub text: String,
    /// The `settings.json` beside a `servers.json`, whose group defaults and gateways it
    /// uses.
    pub settings: Option<String>,
}

impl fmt::Debug for ImportFile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Its text names servers and accounts: never shown.
        f.debug_struct("ImportFile")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

/// What a file gives, before it is imported.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingImport {
    /// What the file is.
    pub kind: FileKind,
    /// The sessions converted, and those left out.
    pub report: ImportReport,
    /// What the file says as a whole.
    pub warnings: Vec<FileWarning>,
    /// Passwords the file stores, never read.
    pub stored_credentials: usize,
}

impl PendingImport {
    /// Sessions the import would add or update.
    #[must_use]
    pub fn count(&self) -> usize {
        let report = &self.report;
        report.profiles.len()
            + report.rdp.len()
            + report.telnet.len()
            + report.vnc.len()
            + report.local.len()
            + report.winrm.len()
            + report.ftp.len()
    }
}

impl App {
    /// A file picked for "Import Sessions", read.
    pub(super) fn import_file(&mut self, read: Result<ImportFile, String>) -> Vec<Effect> {
        let file = match read {
            Ok(file) => file,
            Err(detail) => {
                self.dialog = Some(Dialog::ImportFailed { detail });
                return Vec::new();
            }
        };
        let kind = FileKind::of(&file.name, &file.text);
        let mut pending = match kind {
            FileKind::Heimdall => match csharp::import(&file.text, file.settings.as_deref()) {
                Ok(report) => PendingImport {
                    kind,
                    report,
                    warnings: Vec::new(),
                    stored_credentials: 0,
                },
                Err(error) => {
                    self.dialog = Some(Dialog::ImportFailed {
                        detail: error.to_string(),
                    });
                    return Vec::new();
                }
            },
            FileKind::MobaXterm => self.pending(kind, &mobaxterm::parse(&file.text)),
            FileKind::MRemoteNg => self.pending(kind, &mremoteng::parse(&file.text)),
            FileKind::RdcMan => self.pending(kind, &rdcman::parse(&file.text)),
        };
        distrust(&mut pending.report);
        self.dialog = Some(if pending.count() == 0 {
            // As the C#: nothing to import is said, with why when the file says it.
            Dialog::ImportNothing {
                skipped: pending
                    .report
                    .skipped
                    .into_iter()
                    .map(|skipped| (server_text(&skipped.name), skipped.reason))
                    .collect(),
                warnings: pending.warnings,
            }
        } else {
            Dialog::ConfirmImportFile(Box::new(pending))
        });
        Vec::new()
    }

    /// A foreign file's sessions, each kept one given its own identifier, as the C# gives
    /// each a new GUID.
    fn pending(&self, kind: FileKind, parsed: &Parsed) -> PendingImport {
        PendingImport {
            kind,
            report: parsed.report(&mut || self.fresh_id().as_str().to_owned()),
            warnings: parsed.warnings.clone(),
            stored_credentials: parsed.stored_credentials,
        }
    }

    /// An import's preview or question confirmed: what it chose imported.
    pub(super) fn confirm_import(&mut self, dialog: Dialog) {
        match dialog {
            Dialog::SessionsPreview(preview) => self.import_sessions(&preview),
            Dialog::RdpPreview(preview) => self.import_rdp(&preview),
            Dialog::HostKeysPreview(preview) => self.import_hostkeys(&preview),
            Dialog::ConfirmImportFile(pending) => self.confirm_import_file(*pending),
            _ => {}
        }
    }

    /// The sessions of `pending` merged, and what the merge did said.
    fn confirm_import_file(&mut self, pending: PendingImport) {
        let stored_credentials =
            (pending.kind == FileKind::MobaXterm).then_some(pending.stored_credentials);
        let warnings = pending.warnings.clone();
        if let Some(mut summary) = self.merge_import(pending.report) {
            summary.warnings = warnings;
            summary.stored_credentials = stored_credentials;
            self.dialog = Some(Dialog::ImportDone(summary));
        }
    }

    /// Merges `report` into the store, saved before it is kept; the summary of what it did,
    /// or `None` with the store's error shown.
    pub(super) fn merge_import(&mut self, report: ImportReport) -> Option<ImportSummary> {
        let merged = self.store.apply(|store| {
            let ssh = store.merge(report.profiles);
            let rdp = store.merge_rdp(report.rdp);
            let telnet = store.merge_telnet(report.telnet);
            let vnc = store.merge_vnc(report.vnc);
            let local = store.merge_local(report.local);
            let winrm = store.merge_winrm(report.winrm);
            let ftp = store.merge_ftp(report.ftp);
            let gateways = store.merge_gateways(report.gateways);
            for id in &report.favorites {
                store.set_favorite(id, true);
            }
            for (id, metadata) in &report.metadata {
                store.set_metadata(id, metadata.clone());
            }
            [ssh, rdp, telnet, vnc, local, winrm, ftp, gateways]
                .into_iter()
                .fold(MergeReport::default(), |total, one| MergeReport {
                    added: total.added + one.added,
                    updated: total.updated + one.updated,
                    unchanged: total.unchanged + one.unchanged,
                })
        });
        match merged {
            Ok(merged) => Some(ImportSummary {
                merged,
                skipped: report
                    .skipped
                    .into_iter()
                    .map(|skipped| (server_text(&skipped.name), skipped.reason))
                    .collect(),
                warnings: Vec::new(),
                stored_credentials: None,
                dropped: report
                    .dropped
                    .into_iter()
                    .map(|dropped| (server_text(&dropped.name), dropped.settings))
                    .collect(),
                host_keys: None,
            }),
            Err(error) => {
                self.dialog = Some(Dialog::StoreError {
                    detail: error.to_string(),
                });
                None
            }
        }
    }
}

/// `report` without the trust decisions made on the machine that wrote its file: skipping the
/// check of a `WinRM` host's certificate is decided here, by whoever imports.
fn distrust(report: &mut ImportReport) {
    // Servers trusted elsewhere are not trusted here: only this computer's own C# store
    // carries its trust over.
    report.host_keys.clear();
    for profile in &mut report.winrm {
        profile.skip_certificate_check = false;
    }
}
