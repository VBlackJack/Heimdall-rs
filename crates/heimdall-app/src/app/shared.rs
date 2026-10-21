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
use heimdall_core::settings::SharedApproval;
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
    /// or list for another file, as the C# `ForgetAllExceptAsync`: a source added again under
    /// the same identifier, or pointed at another file, must not inherit what was allowed for
    /// a file that was once there.
    fn forget_removed_approvals(&mut self) {
        let listed: Vec<(String, String)> = self
            .settings
            .shared_sources
            .iter()
            .map(|source| {
                (
                    source.id.clone(),
                    crate::shared_sessions::expand(&source.path),
                )
            })
            .collect();
        let kept: Vec<(&str, &str)> = listed
            .iter()
            .map(|(id, path)| (id.as_str(), path.as_str()))
            .collect();
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
        let Some(origin) = self.shared_origins.get(&profile.id) else {
            return;
        };
        if !origin.remembers {
            return;
        }
        let approval = origin.approval(&profile.post_connect.steps);
        if self
            .settings
            .shared_post_connect_approved(&origin.shared, &approval)
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
        let origin = self.shared_origins.get(id)?.clone();
        // The file names no account: the one typed when connecting is not known to the
        // approval, so it is given for this session only and asked again the next time.
        if !origin.remembers {
            return Some(Ok(()));
        }
        let shared = origin.shared.clone();
        let previous = self
            .settings
            .shared_post_connect_approvals
            .get(shared.as_str())
            .cloned();
        self.settings
            .approve_shared_post_connect(&shared, origin.approval(steps));
        let saved = self.settings.save(&self.settings_file);
        if saved.is_err() {
            match previous {
                Some(previous) => self.settings.approve_shared_post_connect(&shared, previous),
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
        let source_path = self
            .settings
            .shared_sources
            .iter()
            .find(|source| source.id == session.source_id)
            .map(|source| crate::shared_sessions::expand(&source.path))
            .unwrap_or_default();
        let origin = SharedOrigin::of(&session, source_path);
        let (profile, purpose) = opened(session);
        // Its commands, if the file asks for any, are looked up against what the user
        // approved when the shell opens, never taken as approved.
        if let TabProfile::Ssh(ssh) = &profile
            && ssh.post_connect.runnable() > 0
        {
            self.shared_origins.insert(ssh.id.clone(), origin);
        }
        let tabs = self.tabs.len();
        let effects = self.open_transient(profile.clone(), purpose);
        // How the tab reopens is recorded on the tab this opened, if it opened one: a question
        // asked first makes no tab yet, and the last tab is some other session's.
        if self.tabs.len() > tabs {
            self.reopened_by(Reopen::Transient(Box::new(profile), purpose));
        }
        effects
    }

    /// Records how the shell just opened from a shared session, after the question about its
    /// commands, opens again: the profile as the file gave it, its commands not approved, so
    /// that a reconnect looks the approval up again.
    pub(super) fn remember_shared_reopen(&mut self, profile: SshProfile) {
        if self.shared_origins.contains_key(&profile.id) {
            self.reopened_by(Reopen::Transient(
                Box::new(TabProfile::Ssh(profile)),
                Purpose::Shell,
            ));
        }
    }
}

/// The shared session a shell was opened from, and what its commands were approved against.
#[derive(Debug, Clone)]
pub(super) struct SharedOrigin {
    shared: ProfileId,
    source_path: String,
    destination: String,
    /// Whether an approval may be remembered: only for a session that names its account.
    remembers: bool,
}

/// Separates the parts of a destination text; no host, account or name holds it.
const DESTINATION_SEPARATOR: char = '\u{1f}';

impl SharedOrigin {
    fn of(session: &SharedSession, source_path: String) -> Self {
        let (host, port) = session.endpoint();
        let username = match &session.target {
            SharedTarget::Ssh(profile) => profile.username.as_deref(),
            SharedTarget::Rdp(profile) => profile.username.as_deref(),
        };
        let mut parts = vec![
            host.to_owned(),
            port.to_string(),
            username.unwrap_or_default().to_owned(),
        ];
        if let Some(gateway) = &session.gateway {
            parts.extend([
                gateway.id.as_str().to_owned(),
                gateway.host.clone(),
                gateway.port.to_string(),
                gateway.username.clone().unwrap_or_default(),
                gateway
                    .parent
                    .as_ref()
                    .map(|parent| parent.as_str().to_owned())
                    .unwrap_or_default(),
            ]);
        }
        Self {
            shared: session.id.clone(),
            source_path,
            destination: parts.join(&DESTINATION_SEPARATOR.to_string()),
            remembers: username.is_some_and(|name| !name.is_empty())
                && session.gateway.as_ref().is_none_or(|gateway| {
                    gateway
                        .username
                        .as_deref()
                        .is_some_and(|name| !name.is_empty())
                }),
        }
    }

    fn approval(&self, steps: &[PostConnectStep]) -> SharedApproval {
        SharedApproval {
            source_path: self.source_path.clone(),
            destination: self.destination.clone(),
            steps: steps
                .iter()
                .map(|step| PostConnectStep {
                    untrusted: false,
                    ..step.clone()
                })
                .collect(),
        }
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
