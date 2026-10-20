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

use heimdall_core::post_connect::{PostConnect, PostConnectStep};
use heimdall_core::profile::{ProfileId, SshProfile};
use heimdall_core::store::StoreError;

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
        let variables = self.settings.mobaxterm_variables.clone();
        // The offline copies beside the settings file, in a folder of their own.
        let cache = self
            .settings_file
            .parent()
            .map(|folder| folder.join(crate::shared_cache::DIRECTORY_NAME));
        self.shared.set_cache_directory(cache);
        self.shared.load(&sources, &variables);
        self.sync_shared_gateways();
        self.forget_removed_approvals();
    }

    /// Hands the store the gateways the shared sessions go through, so that a shared session
    /// crosses the bastion its file names. They are held in memory beside the saved ones: in
    /// no list, no export and no save.
    fn sync_shared_gateways(&mut self) {
        let gateways = self.shared.gateways();
        self.store.set_shared_gateways(gateways);
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
        let variables = self.settings.mobaxterm_variables.clone();
        self.shared.refresh(&sources, &variables, now);
        self.sync_shared_gateways();
        self.forget_removed_approvals();
    }

    /// Forgets the approved commands of the sessions of sources the settings no longer list,
    /// as the C# `ForgetAllExceptAsync`: a source added again under the same identifier must
    /// not inherit what was allowed for a file that was once there.
    fn forget_removed_approvals(&mut self) {
        let ids: Vec<String> = self
            .settings
            .shared_sources
            .iter()
            .map(|source| source.id.clone())
            .collect();
        let kept: Vec<&str> = ids.iter().map(String::as_str).collect();
        let forgotten = self
            .settings
            .forget_shared_post_connect_approvals_except(&kept);
        if forgotten == 0 {
            return;
        }
        log::info!(
            "approved commands of {forgotten} shared session(s) of removed sources forgotten"
        );
        if let Err(error) = self.settings.save(&self.settings_file) {
            log::error!("the approved commands could not be forgotten on disk: {error}");
        }
    }

    /// Marks `profile`, a shell opened from a shared session, approved when the user already
    /// approved exactly its commands for that session. Nothing else approves them: the file
    /// is rebuilt by other people, so what it says next is asked about again.
    pub(super) fn approve_known_shared_steps(&self, profile: &mut SshProfile) {
        let Some(shared) = self.shared_origins.get(&profile.id) else {
            return;
        };
        if self
            .settings
            .shared_post_connect_approved(shared, &profile.post_connect.steps)
        {
            profile.post_connect = PostConnect::approved_as(profile.post_connect.steps.clone());
        }
    }

    /// Whether the shell `profile` was opened from a shared session.
    pub(super) fn is_shared_shell(&self, profile: &SshProfile) -> bool {
        self.shared_origins.contains_key(&profile.id)
    }

    /// Records the approval of `steps` for the shared session the shell `id` was opened from,
    /// saved at once. None when the shell is not from a shared session; the error leaves the
    /// approval unrecorded.
    pub(super) fn approve_shared_steps(
        &mut self,
        id: &ProfileId,
        steps: &[PostConnectStep],
    ) -> Option<Result<(), StoreError>> {
        let shared = self.shared_origins.get(id)?.clone();
        let previous = self
            .settings
            .shared_post_connect_approvals
            .get(shared.as_str())
            .cloned();
        self.settings.approve_shared_post_connect(&shared, steps);
        let saved = self.settings.save(&self.settings_file);
        if saved.is_err() {
            match previous {
                Some(previous) => self
                    .settings
                    .approve_shared_post_connect(&shared, &previous),
                None => {
                    self.settings
                        .shared_post_connect_approvals
                        .remove(shared.as_str());
                }
            }
        }
        Some(saved)
    }

    /// Opens the shared session `id` as a session never saved, a copy of what the file
    /// says. It carries no folder, so no saved folder's defaults apply to it.
    pub(super) fn connect_shared(&mut self, id: &ProfileId) -> Vec<Effect> {
        let Some(session) = self.shared.find(id) else {
            return Vec::new();
        };
        let shared_id = session.id.clone();
        let (profile, purpose) = opened(session);
        // Its commands, if the file asks for any, are looked up against what the user
        // approved when the shell opens, never taken as approved.
        if let TabProfile::Ssh(ssh) = &profile
            && ssh.post_connect.runnable() > 0
        {
            self.shared_origins.insert(ssh.id.clone(), shared_id);
        }
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
