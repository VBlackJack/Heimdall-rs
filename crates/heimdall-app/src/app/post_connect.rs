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

//! A shell's post-connect sequence, as the C# Heimdall runs it: steps not approved yet are
//! shown and asked about before the session opens, the tab counts the steps while they run,
//! and a click on the count stops them.
//!
//! What runs is decided by the driver from the profile it is given: only steps approved as
//! they are. The question here is how steps get approved; a session opened any other way
//! types nothing it was not allowed to.

use heimdall_core::post_connect::PostConnect;
use heimdall_core::profile::SshProfile;

use super::{App, Dialog, Effect, Message};
use crate::driver::Purpose;
use crate::ids::TabId;
use crate::text::{server_text, visible_text};

/// A shell asking to type steps the user has not approved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PostConnectConfirmation {
    /// The profile to open, with the steps shown.
    pub profile: SshProfile,
    /// Its name, made safe.
    pub name: String,
    /// The commands that would be typed, in order, made safe.
    pub commands: Vec<String>,
}

impl App {
    /// Opens a shell for `profile`, asking first when it would type steps the user has not
    /// approved as they are.
    pub(super) fn open_ssh(&mut self, profile: SshProfile, purpose: Purpose) -> Vec<Effect> {
        if purpose == Purpose::Shell && profile.post_connect.needs_approval() {
            let commands = profile
                .post_connect
                .steps
                .iter()
                .filter(|step| step.runs())
                .map(|step| visible_text(&step.input))
                .collect();
            self.dialog = Some(Dialog::ConfirmPostConnect(Box::new(
                PostConnectConfirmation {
                    name: server_text(&profile.name),
                    commands,
                    profile,
                },
            )));
            return Vec::new();
        }
        self.open_ssh_now(profile, purpose)
    }

    /// Records that the steps shown are approved, then opens the shell that types them.
    pub(super) fn run_post_connect(
        &mut self,
        confirmation: PostConnectConfirmation,
    ) -> Vec<Effect> {
        let mut profile = confirmation.profile;
        let steps = profile.post_connect.steps.clone();
        let recorded = self
            .store
            .apply(|store| store.approve_post_connect(&profile.id, &steps));
        if let Err(error) = recorded {
            self.dialog = Some(Dialog::StoreError {
                detail: error.to_string(),
            });
            return Vec::new();
        }
        // Approved for this session whether or not the profile is still saved: the user
        // agreed to what was shown.
        profile.post_connect = PostConnect::approved_as(steps);
        self.open_ssh_now(profile, Purpose::Shell)
    }

    /// Handles the post-connect messages.
    pub(super) fn post_connect_message(&mut self, message: &Message) -> Vec<Effect> {
        match message {
            Message::SkipPostConnect => self.skip_post_connect(),
            Message::StopPostConnect(tab) => {
                self.stop_post_connect(*tab);
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    /// Opens the shell without typing the steps shown, as the C# answer No does; the next
    /// session asks again.
    fn skip_post_connect(&mut self) -> Vec<Effect> {
        let Some(Dialog::ConfirmPostConnect(confirmation)) = self.dialog.take() else {
            return Vec::new();
        };
        self.open_ssh_now(confirmation.profile, Purpose::Shell)
    }

    /// Stops the steps the tab `tab` is typing; those typed stay typed.
    fn stop_post_connect(&mut self, tab: TabId) {
        if let Some(progress) = self
            .tab_mut(tab)
            .and_then(|found| found.post_connect.as_ref())
        {
            progress.stop.cancel();
        }
    }
}
