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

//! "Import OpenSSH config", as the C# Heimdall's: the file picked, read, shown as a preview
//! of what it gives with what was left out, then the servers chosen imported, their
//! `ProxyJump` chains as SSH gateways.

use std::collections::HashSet;
use std::path::Path;

use heimdall_core::import::openssh::{self, Assessment, Diagnostic, Status};
use heimdall_core::profile::ProfileId;

use super::{App, Dialog, Effect};

/// What the preview shows: each server of the file with whether it is chosen, and what was
/// read differently or left out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenSshPreview {
    /// The servers, in the file's order.
    pub rows: Vec<OpenSshRow>,
    /// What was read differently or left out, by line.
    pub diagnostics: Vec<Diagnostic>,
}

/// A server of the preview.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenSshRow {
    /// The server, with what the import would do with it.
    pub assessment: Assessment,
    /// Chosen for the import: the new ones at first, as in the C# preview.
    pub chosen: bool,
}

impl OpenSshPreview {
    /// How many servers the file gives, new ones, and ones a profile already has the name
    /// of: the C# summary line.
    #[must_use]
    pub fn counts(&self) -> (usize, usize, usize) {
        let new = self
            .rows
            .iter()
            .filter(|row| row.assessment.status == Status::New)
            .count();
        (self.rows.len(), new, self.rows.len() - new)
    }

    /// Whether every server is chosen.
    #[must_use]
    pub fn all_chosen(&self) -> bool {
        !self.rows.is_empty() && self.rows.iter().all(|row| row.chosen)
    }

    /// Whether the import has something to do.
    #[must_use]
    pub fn can_import(&self) -> bool {
        self.rows.iter().any(|row| row.chosen)
    }
}

/// A change to the OpenSSH import.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpenSshMessage {
    /// "OpenSSH config...": pick the file.
    Start,
    /// The file picked, read; or why it could not be.
    Read(Result<String, String>),
    /// This server chosen, or no longer.
    Choose(usize),
    /// Every server chosen, or none.
    ChooseAll(bool),
}

impl App {
    pub(super) fn openssh_message(&mut self, message: OpenSshMessage) -> Vec<Effect> {
        match message {
            OpenSshMessage::Start => return vec![Effect::PickOpenSshConfig],
            OpenSshMessage::Read(Ok(text)) => {
                self.dialog = Some(self.openssh_preview(&text));
            }
            OpenSshMessage::Read(Err(detail)) => {
                self.dialog = Some(Dialog::OpenSshUnreadable { detail });
            }
            OpenSshMessage::Choose(index) => {
                if let Some(Dialog::OpenSshPreview(preview)) = &mut self.dialog
                    && let Some(row) = preview.rows.get_mut(index)
                {
                    row.chosen = !row.chosen;
                }
            }
            OpenSshMessage::ChooseAll(on) => {
                if let Some(Dialog::OpenSshPreview(preview)) = &mut self.dialog {
                    for row in &mut preview.rows {
                        row.chosen = on;
                    }
                }
            }
        }
        Vec::new()
    }

    /// The preview of `text`, or the word that it gives nothing.
    fn openssh_preview(&self, text: &str) -> Dialog {
        let parsed = openssh::parse(text, std::env::home_dir().as_deref());
        if parsed.candidates.is_empty() && parsed.diagnostics.is_empty() {
            return Dialog::OpenSshEmpty;
        }
        let rows = openssh::assess(
            &parsed.candidates,
            &self.profile_names(),
            self.store.gateways(),
        )
        .into_iter()
        .map(|assessment| OpenSshRow {
            chosen: assessment.status == Status::New,
            assessment,
        })
        .collect();
        Dialog::OpenSshPreview(Box::new(OpenSshPreview {
            rows,
            diagnostics: parsed.diagnostics,
        }))
    }

    /// Every profile's name, lowercase: an alias one has is not imported again.
    fn profile_names(&self) -> HashSet<String> {
        self.profile_summaries()
            .into_iter()
            .map(|profile| profile.name.to_lowercase())
            .collect()
    }

    /// Imports the chosen servers of `preview`: saved first, then kept, as the C# import.
    pub(super) fn import_openssh(&mut self, preview: &OpenSshPreview) {
        let chosen: Vec<_> = preview
            .rows
            .iter()
            .filter(|row| row.chosen)
            .map(|row| row.assessment.candidate.clone())
            .collect();
        let mut taken: HashSet<ProfileId> = self
            .profile_summaries()
            .into_iter()
            .map(|profile| profile.id)
            .chain(
                self.store
                    .gateways()
                    .iter()
                    .map(|gateway| gateway.id.clone()),
            )
            .collect();
        let mut new_id = || loop {
            let id = crate::profile_draft::new_id(&[]);
            if taken.insert(id.clone()) {
                return id;
            }
        };
        let plan = openssh::plan(
            &chosen,
            &self.profile_names(),
            self.store.gateways(),
            &mut new_id,
        );
        // As the C# import warns: a key that is not there is kept, for the user to fix.
        let warnings = plan
            .profiles
            .iter()
            .filter(|profile| {
                profile
                    .key_path
                    .as_deref()
                    .is_some_and(|key| !Path::new(key).exists())
            })
            .count();
        let (imported, gateways, duplicates) = (
            plan.profiles.len(),
            plan.gateways.len(),
            plan.duplicates.len(),
        );
        let saved = self.store.apply(|store| {
            store.merge_gateways(plan.gateways);
            store.merge(plan.profiles);
        });
        self.dialog = Some(match saved {
            Ok(()) => Dialog::OpenSshDone {
                imported,
                gateways,
                duplicates,
                warnings,
            },
            Err(error) => Dialog::StoreError {
                detail: error.to_string(),
            },
        });
    }
}
