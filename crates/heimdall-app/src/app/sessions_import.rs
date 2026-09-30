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

//! "Import OpenSSH config" and "Import `PuTTY` sessions", as the C# Heimdall's: what the file
//! or `PuTTY`'s store gives, shown as a preview with what was left out, then the servers
//! chosen imported, an OpenSSH `ProxyJump` chain as SSH gateways. Both share the C#
//! `ImportSessionsPreviewDialog`.

use std::collections::HashSet;
use std::path::Path;

use heimdall_core::import::openssh::{self, Assessment, Candidate, Status};
use heimdall_core::import::putty::{self, RawSession};
use heimdall_core::profile::ProfileId;

use super::{App, Dialog, Effect};

/// Where the servers of a preview come from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionsSource {
    /// An OpenSSH client configuration.
    OpenSsh,
    /// `PuTTY`'s saved sessions.
    Putty,
}

/// What the preview shows: each server with whether it is chosen, and what was read
/// differently or left out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionsPreview {
    /// Where they come from.
    pub source: SessionsSource,
    /// The servers, in the order read.
    pub rows: Vec<SessionsRow>,
    /// What an OpenSSH file said, by line.
    pub diagnostics: Vec<openssh::Diagnostic>,
    /// What `PuTTY`'s sessions said, by session.
    pub putty_diagnostics: Vec<putty::Diagnostic>,
}

/// A server of the preview.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionsRow {
    /// The server, with what the import would do with it.
    pub assessment: Assessment,
    /// Chosen for the import: the new ones at first, as in the C# preview.
    pub chosen: bool,
}

impl SessionsRow {
    /// Whether it can be chosen: an invalid one never is, as in the C#.
    #[must_use]
    pub fn choosable(&self) -> bool {
        self.assessment.status != Status::Invalid
    }
}

impl SessionsPreview {
    /// How many servers, new ones, ones a profile already has the name of, and invalid
    /// ones: the C# summary line.
    #[must_use]
    pub fn counts(&self) -> (usize, usize, usize, usize) {
        let count = |status| {
            self.rows
                .iter()
                .filter(|row| row.assessment.status == status)
                .count()
        };
        (
            self.rows.len(),
            count(Status::New),
            count(Status::Duplicate),
            count(Status::Invalid),
        )
    }

    /// Whether every server that can be chosen is.
    #[must_use]
    pub fn all_chosen(&self) -> bool {
        self.rows.iter().any(SessionsRow::choosable)
            && self
                .rows
                .iter()
                .filter(|row| row.choosable())
                .all(|row| row.chosen)
    }

    /// Whether the import has something to do.
    #[must_use]
    pub fn can_import(&self) -> bool {
        self.rows.iter().any(|row| row.chosen)
    }
}

/// A change to the OpenSSH or `PuTTY` import.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionsMessage {
    /// "OpenSSH config...": pick the file.
    Start,
    /// The OpenSSH file picked, read; or why it could not be.
    Read(Result<String, String>),
    /// "`PuTTY` sessions...": read them.
    Putty,
    /// `PuTTY`'s sessions read; or why they could not be.
    PuttyRead(Result<Vec<RawSession>, String>),
    /// This server chosen, or no longer.
    Choose(usize),
    /// Every server that can be chosen, or none.
    ChooseAll(bool),
    /// "Import Sessions": pick a file, a Heimdall, `MobaXterm`, `mRemoteNG` or `RDCMan` one.
    File,
    /// The file picked, read; or why it could not be.
    FileRead(Result<super::ImportFile, String>),
}

impl App {
    pub(super) fn sessions_message(&mut self, message: SessionsMessage) -> Vec<Effect> {
        match message {
            SessionsMessage::Start => return vec![Effect::PickOpenSshConfig],
            SessionsMessage::Putty => return vec![Effect::ReadPuttySessions],
            SessionsMessage::File => return vec![Effect::PickSessionsFile],
            SessionsMessage::FileRead(read) => return self.import_file(read),
            SessionsMessage::Read(Ok(text)) => {
                let parsed = openssh::parse(&text, std::env::home_dir().as_deref());
                self.dialog = Some(self.sessions_preview(
                    SessionsSource::OpenSsh,
                    &parsed.candidates,
                    parsed.diagnostics,
                    Vec::new(),
                ));
            }
            SessionsMessage::PuttyRead(Ok(sessions)) => {
                let parsed = putty::parse(&sessions);
                self.dialog = Some(self.sessions_preview(
                    SessionsSource::Putty,
                    &parsed.candidates,
                    Vec::new(),
                    parsed.diagnostics,
                ));
            }
            SessionsMessage::Read(Err(detail)) => {
                self.dialog = Some(Dialog::SessionsUnreadable {
                    source: SessionsSource::OpenSsh,
                    detail,
                });
            }
            SessionsMessage::PuttyRead(Err(detail)) => {
                self.dialog = Some(Dialog::SessionsUnreadable {
                    source: SessionsSource::Putty,
                    detail,
                });
            }
            SessionsMessage::Choose(index) => {
                if let Some(Dialog::SessionsPreview(preview)) = &mut self.dialog
                    && let Some(row) = preview.rows.get_mut(index).filter(|row| row.choosable())
                {
                    row.chosen = !row.chosen;
                }
            }
            SessionsMessage::ChooseAll(on) => {
                if let Some(Dialog::SessionsPreview(preview)) = &mut self.dialog {
                    for row in preview.rows.iter_mut().filter(|row| row.choosable()) {
                        row.chosen = on;
                    }
                }
            }
        }
        Vec::new()
    }

    /// The preview of what was read, or the word that it gives nothing.
    fn sessions_preview(
        &self,
        source: SessionsSource,
        candidates: &[Candidate],
        diagnostics: Vec<openssh::Diagnostic>,
        putty_diagnostics: Vec<putty::Diagnostic>,
    ) -> Dialog {
        if candidates.is_empty() && diagnostics.is_empty() && putty_diagnostics.is_empty() {
            return Dialog::SessionsEmpty { source };
        }
        let rows = openssh::assess(candidates, &self.profile_names(), self.store.gateways())
            .into_iter()
            .map(|assessment| SessionsRow {
                chosen: assessment.status == Status::New,
                assessment,
            })
            .collect();
        Dialog::SessionsPreview(Box::new(SessionsPreview {
            source,
            rows,
            diagnostics,
            putty_diagnostics,
        }))
    }

    /// Every profile's name, lowercase: a name one has is not imported again.
    fn profile_names(&self) -> HashSet<String> {
        self.profile_summaries()
            .into_iter()
            .map(|profile| profile.name.to_lowercase())
            .collect()
    }

    /// Imports the chosen servers of `preview`: saved first, then kept, as the C# import.
    pub(super) fn import_sessions(&mut self, preview: &SessionsPreview) {
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
        let counts = SessionsCounts {
            imported: plan.profiles.len(),
            gateways: plan.gateways.len(),
            duplicates: plan.duplicates.len(),
            invalid: plan.invalid.len(),
            warnings,
        };
        let saved = self.store.apply(|store| {
            store.merge_gateways(plan.gateways);
            store.merge(plan.profiles);
        });
        self.dialog = Some(match saved {
            Ok(()) => Dialog::SessionsDone {
                source: preview.source,
                counts,
            },
            Err(error) => Dialog::StoreError {
                detail: error.to_string(),
            },
        });
    }
}

/// What an OpenSSH or `PuTTY` import did, as the C# counts it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SessionsCounts {
    /// Profiles added.
    pub imported: usize,
    /// Gateways added for their chains.
    pub gateways: usize,
    /// Servers left out: a profile has their name.
    pub duplicates: usize,
    /// Servers left out: no host.
    pub invalid: usize,
    /// Profiles whose key file is not there.
    pub warnings: usize,
}
