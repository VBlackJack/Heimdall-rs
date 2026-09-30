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

//! "Trusted SSH hosts...", as the C# Heimdall's: another `known_hosts` file picked, its keys
//! previewed against the keys already trusted, then the new ones trusted. A key that
//! contradicts one trusted is shown and never written.

use std::fmt;

use heimdall_ssh::KnownHosts;
use heimdall_ssh::known_hosts_import::{
    self, HostKeyCandidate, HostKeyDiagnostic, HostKeyStatus, HostKeysImported,
};

use super::{App, Dialog, Effect};

/// A key of the preview.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostKeyRow {
    /// The key and its server.
    pub candidate: HostKeyCandidate,
    /// New, trusted already, or in conflict.
    pub status: HostKeyStatus,
    /// Chosen: every new key at first; only a new key can be.
    pub chosen: bool,
    /// Its SHA-256 fingerprint, `SHA256:...`.
    pub fingerprint: String,
}

/// What a `known_hosts` file gives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostKeysPreview {
    /// The keys, in the file's order.
    pub rows: Vec<HostKeyRow>,
    /// What was left out, by line.
    pub diagnostics: Vec<HostKeyDiagnostic>,
}

impl HostKeysPreview {
    /// Keys, new ones, ones trusted already, ones in conflict: the C# summary.
    #[must_use]
    pub fn counts(&self) -> (usize, usize, usize, usize) {
        let count = |status| self.rows.iter().filter(|row| row.status == status).count();
        (
            self.rows.len(),
            count(HostKeyStatus::New),
            count(HostKeyStatus::Existing),
            count(HostKeyStatus::Conflict),
        )
    }

    /// Whether every new key is chosen, as the C# "Import all": not when there is none.
    #[must_use]
    pub fn all_chosen(&self) -> bool {
        let mut new = self
            .rows
            .iter()
            .filter(|row| row.status == HostKeyStatus::New)
            .peekable();
        new.peek().is_some() && new.all(|row| row.chosen)
    }

    /// Whether the import has something to do.
    #[must_use]
    pub fn can_import(&self) -> bool {
        self.rows.iter().any(|row| row.chosen)
    }
}

/// A change to the `known_hosts` import, a change to the trusted keys.
#[derive(Clone, PartialEq, Eq)]
pub enum HostKeysMessage {
    /// "Trusted SSH hosts...": pick the file.
    Start,
    /// The file picked, read; or why it could not be.
    Read(Result<String, String>),
    /// This key chosen, or no longer.
    Choose(usize),
    /// Every new key chosen, or none.
    ChooseAll(bool),
}

impl fmt::Debug for HostKeysMessage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Start => f.write_str("Start"),
            // A file's text names servers: never shown.
            Self::Read(_) => f.write_str("Read(..)"),
            Self::Choose(index) => write!(f, "Choose({index})"),
            Self::ChooseAll(on) => write!(f, "ChooseAll({on})"),
        }
    }
}

impl App {
    pub(super) fn hostkeys_message(&mut self, message: HostKeysMessage) -> Vec<Effect> {
        match message {
            HostKeysMessage::Start => return vec![Effect::PickKnownHosts],
            HostKeysMessage::Read(Ok(text)) => self.dialog = Some(self.hostkeys_preview(&text)),
            HostKeysMessage::Read(Err(detail)) => {
                self.dialog = Some(Dialog::HostKeysUnreadable { detail });
            }
            HostKeysMessage::Choose(index) => {
                if let Some(Dialog::HostKeysPreview(preview)) = &mut self.dialog
                    && let Some(row) = preview
                        .rows
                        .get_mut(index)
                        .filter(|row| row.status == HostKeyStatus::New)
                {
                    row.chosen = !row.chosen;
                }
            }
            HostKeysMessage::ChooseAll(on) => {
                if let Some(Dialog::HostKeysPreview(preview)) = &mut self.dialog {
                    for row in preview
                        .rows
                        .iter_mut()
                        .filter(|row| row.status == HostKeyStatus::New)
                    {
                        row.chosen = on;
                    }
                }
            }
        }
        Vec::new()
    }

    fn known_hosts(&self) -> KnownHosts {
        KnownHosts::new(&self.config.known_hosts)
    }

    fn hostkeys_preview(&self, text: &str) -> Dialog {
        let parsed = known_hosts_import::parse(text);
        if parsed.candidates.is_empty() && parsed.diagnostics.is_empty() {
            return Dialog::HostKeysEmpty;
        }
        let statuses = match known_hosts_import::assess(&parsed.candidates, &self.known_hosts()) {
            Ok(statuses) => statuses,
            Err(error) => {
                return Dialog::HostKeysUnreadable {
                    detail: error.to_string(),
                };
            }
        };
        let rows = parsed
            .candidates
            .into_iter()
            .zip(statuses)
            .map(|(candidate, status)| HostKeyRow {
                fingerprint: heimdall_ssh::fingerprint(&candidate.key),
                chosen: status == HostKeyStatus::New,
                candidate,
                status,
            })
            .collect();
        Dialog::HostKeysPreview(Box::new(HostKeysPreview {
            rows,
            diagnostics: parsed.diagnostics,
        }))
    }

    /// Trusts the chosen keys of `preview`, each checked again as it is written.
    pub(super) fn import_hostkeys(&mut self, preview: &HostKeysPreview) {
        let chosen: Vec<HostKeyCandidate> = preview
            .rows
            .iter()
            .filter(|row| row.chosen)
            .map(|row| row.candidate.clone())
            .collect();
        let mut done = known_hosts_import::import(&chosen, &self.known_hosts());
        if let Ok(done) = &mut done {
            // What was not chosen is counted as the C# counts it.
            done.existing += preview
                .rows
                .iter()
                .filter(|row| row.status == HostKeyStatus::Existing)
                .count();
            done.conflicts += preview
                .rows
                .iter()
                .filter(|row| row.status == HostKeyStatus::Conflict)
                .count();
        }
        // The trusted keys page shows what was just trusted.
        self.read_trusted_keys();
        self.dialog = Some(match done {
            Ok(done) => Dialog::HostKeysDone {
                done,
                warnings: preview
                    .diagnostics
                    .iter()
                    .filter(|diagnostic| diagnostic.note.is_warning())
                    .count(),
            },
            Err(error) => Dialog::HostKeysUnreadable {
                detail: error.to_string(),
            },
        });
    }
}

/// Re-exported for the window: what an import did.
pub type HostKeysOutcome = HostKeysImported;
