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

//! The shared sessions in the application, as the C# tree's "shared" section
//! (`ServerListViewModel`, `SharedSessionProvider`): read from the files of the settings'
//! shared sources, looked at again on a timer, shown in the tree and opened from it.
//!
//! They are not saved profiles: they are held in memory beside the store, so no menu that
//! edits, moves, renames, copies or deletes a saved profile can reach one, and nothing is
//! ever written back to the file. A shared session opens as a session never saved, and asks
//! about a server's key or certificate exactly as a saved one does: the file cannot trust
//! anything for the person who reads it.

use std::time::SystemTime;

use heimdall_core::profile::ProfileId;

use super::connect_as::transient_id;
use super::reconnect::Reopen;
use super::{App, Effect, TabProfile};
use crate::driver::Purpose;
use crate::shared_sessions::{SharedSession, SharedSessions, SharedTarget};

impl App {
    /// The shared sessions held, source by source.
    #[must_use]
    pub fn shared_sessions(&self) -> &SharedSessions {
        &self.shared
    }

    /// Reads the shared sources of the settings. Done at start; later, the timer's looks
    /// follow the files and the settings alike.
    pub(super) fn load_shared(&mut self) {
        let sources = self.settings.shared_sources.clone();
        self.shared.load(&sources);
    }

    /// Whether the files are worth looking at on a timer: a source is configured.
    #[must_use]
    pub fn watches_shared_sources(&self) -> bool {
        !self.settings.shared_sources.is_empty() || !self.shared.is_empty()
    }

    /// A look at the shared files: a source whose file changed is read again, and the
    /// settings' list of sources followed.
    pub(super) fn shared_tick(&mut self, now: SystemTime) {
        let sources = self.settings.shared_sources.clone();
        self.shared.refresh(&sources, now);
    }

    /// Opens the shared session `id` as a session never saved, a copy of what the file
    /// says. It carries no folder, so no saved folder's defaults apply to it.
    pub(super) fn connect_shared(&mut self, id: &ProfileId) -> Vec<Effect> {
        let Some(session) = self.shared.find(id) else {
            return Vec::new();
        };
        let (profile, purpose) = opened(session);
        let effects = self.open_transient(profile.clone(), purpose);
        self.reopened_by(Reopen::Transient(Box::new(profile), purpose));
        effects
    }
}

/// What a shared session opens, under an identifier of this run so that nothing saved
/// answers to it.
fn opened(session: SharedSession) -> (TabProfile, Purpose) {
    match session.target {
        SharedTarget::Ssh(mut profile) => {
            profile.id = transient_id();
            (TabProfile::Ssh(profile), Purpose::Shell)
        }
        SharedTarget::Rdp(mut profile) => {
            profile.id = transient_id();
            (TabProfile::Rdp(profile), Purpose::Rdp)
        }
    }
}
