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

//! The previous run's sessions offered again, as the C# session snapshot restores them: the
//! tabs of saved profiles open at exit are kept; the next start asks which to reopen, once
//! nothing else is asked, and the snapshot goes once answered, either way.

use std::time::SystemTime;

use heimdall_core::profile::ProfileId;
use heimdall_core::session_snapshot::{self, SessionSnapshot, SnapshotEntry};

use super::reconnect::Reopen;
use super::tree::ProfileKind;
use super::{App, Dialog, Effect};
use crate::driver::Purpose;

/// A session the restore dialog offers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestoreRow {
    /// The profile it reopens.
    pub profile: ProfileId,
    /// An SSH profile's files rather than its shell.
    pub files: bool,
    /// The profile's name and protocol; `None` when it is no longer saved: it cannot be
    /// reopened, and is said to be missing, as the C# dialog says it.
    pub found: Option<(String, ProfileKind)>,
    /// Ticked to reopen.
    pub chosen: bool,
}

/// What the restore dialog shows: the sessions, and when they were kept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestoreDialog {
    /// The sessions, in the order their tabs were.
    pub rows: Vec<RestoreRow>,
    /// When the previous run closed.
    pub saved_at: SystemTime,
}

impl App {
    /// The tabs of saved profiles open now, kept to be offered at the next start; none
    /// leaves no snapshot. A snapshot that cannot be written is logged: closing goes on.
    pub(super) fn keep_snapshot(&self) {
        // The tabs of the strip, as the C# keeps them: a pane docked in a split is not one.
        let sessions = self
            .strip()
            .into_iter()
            .filter_map(|tab| match &tab.reopen {
                Reopen::Profile(id) => Some(SnapshotEntry {
                    profile: id.clone(),
                    files: tab.purpose == Purpose::Files,
                }),
                Reopen::Shell(_) | Reopen::Transient(..) | Reopen::LocalBrowser => None,
            })
            .collect();
        let path = session_snapshot::snapshot_path(&self.config.profiles_file);
        if let Err(error) = session_snapshot::save(&path, &SessionSnapshot::now(sessions)) {
            log::warn!("the session snapshot could not be kept: {error}");
        }
    }

    /// Asks which of the previous run's sessions to reopen, when there are some and nothing
    /// else is asked; once.
    pub fn offer_restore(&mut self) {
        if self.dialog.is_some() {
            return;
        }
        let Some(snapshot) = self.pending_restore.take() else {
            return;
        };
        let summaries = self.profile_summaries();
        let rows = snapshot
            .sessions
            .iter()
            .map(|entry| {
                let found = summaries
                    .iter()
                    .find(|profile| profile.id == entry.profile)
                    .map(|profile| (profile.name.clone(), profile.kind));
                RestoreRow {
                    profile: entry.profile.clone(),
                    files: entry.files,
                    chosen: found.is_some(),
                    found,
                }
            })
            .collect();
        self.dialog = Some(Dialog::RestoreSessions(RestoreDialog {
            rows,
            saved_at: snapshot.saved_at(),
        }));
    }

    /// A session of the restore dialog ticked or not; every one for `None`.
    pub(super) fn choose_restored(&mut self, index: Option<usize>, chosen: bool) {
        let Some(Dialog::RestoreSessions(dialog)) = self.dialog.as_mut() else {
            return;
        };
        for (at, row) in dialog.rows.iter_mut().enumerate() {
            if index.is_none_or(|index| index == at) && row.found.is_some() {
                row.chosen = chosen;
            }
        }
    }

    /// "Restore selected": the sessions ticked reopen, in their order; the snapshot goes.
    pub(super) fn restore_sessions(&mut self, dialog: RestoreDialog) -> Vec<Effect> {
        self.forget_snapshot();
        let mut effects = Vec::new();
        for row in dialog
            .rows
            .into_iter()
            .filter(|row| row.chosen && row.found.is_some())
        {
            let purpose = if row.files {
                Purpose::Files
            } else {
                Purpose::Shell
            };
            effects.extend(self.open_saved(&row.profile, purpose));
        }
        effects
    }

    /// The snapshot answered: gone, so it is not offered again.
    pub(super) fn forget_snapshot(&self) {
        let path = session_snapshot::snapshot_path(&self.config.profiles_file);
        if let Err(error) = session_snapshot::clear(&path) {
            log::warn!("the session snapshot could not be removed: {error}");
        }
    }
}
