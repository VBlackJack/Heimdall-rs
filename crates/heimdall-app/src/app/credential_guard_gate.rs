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

//! Credential Guard required before an embedded RDP session, as the C#
//! `EnforceCredentialGuardAsync` (`ServerListViewModel.cs:1051-1093`) and its check in
//! `RdpHandler.cs:209-229`: with the setting on, a desktop opens in a tab only while
//! Credential Guard runs on this computer. It fails closed: Credential Guard not running,
//! or its state not found, refuses the session, said once in the status bar for a batch, as
//! the C# says it once for a "Connect all".
//!
//! It comes before the Windows Hello gate, as the C# checks it before
//! `EnsureWindowsHelloAsync`: a session refused here never raises the Windows Hello prompt.
//!
//! What waits: a saved RDP profile opened in a tab, whatever opened it (the tree, Quick
//! Connect, a folder's "Connect all", "Connect selected", a restore), an RDP profile's
//! one-time "Connect with" the built-in client, one opened in a split, an RDP tab
//! reconnected and an auto-reconnect. Remote Desktop Connection's own window is not
//! concerned, an RD Gateway profile, which opens there, included, as the C# checks embedded
//! sessions only. The answer known, a session goes on or is refused at once; unknown, it
//! waits for one check, which the sessions opened meanwhile wait for as well. Every RDP
//! attempt asks again before it connects ([`crate::rdp_driver::RdpRequest::credential_guard`]),
//! so a session saved nowhere is held to it too, on its tab.

use std::sync::Arc;

use super::split::SplitMessage;
use super::{App, Effect, Message, Notice, QuickResult};
use crate::credential_guard::{Detector, Status};
use crate::driver::Purpose;
use heimdall_core::profile::{ProfileId, RdpMode};

/// The gate's state.
pub(super) struct CredentialGuardGate {
    /// The detection and its kept answer, shared with the RDP attempts.
    pub(super) detector: Arc<Detector>,
    /// The messages that open an embedded RDP session, held until the check answers.
    waiting: Vec<Message>,
    /// A check is running and has not answered yet.
    checking: bool,
    /// The messages held are being applied, past the gate.
    passed: bool,
    /// While a message is applied, whether the refusal was said in it: the messages it
    /// applies in turn belong to its batch, which says it once. `None` between messages.
    batch: Option<bool>,
    /// The last answer, kept or not, for the Settings page.
    last: Option<Status>,
}

impl Default for CredentialGuardGate {
    fn default() -> Self {
        Self {
            detector: Arc::new(Detector::default()),
            waiting: Vec::new(),
            checking: false,
            passed: false,
            batch: None,
            last: None,
        }
    }
}

impl App {
    /// Starts applying a message: `true` for one not applied by another, which opens a
    /// batch whose refusal is said once.
    pub(super) fn begin_batch(&mut self) -> bool {
        let batch = &mut self.credential_guard.batch;
        let outermost = batch.is_none();
        if outermost {
            *batch = Some(false);
        }
        outermost
    }

    /// Ends applying a message begun with [`Self::begin_batch`] answering `outermost`.
    pub(super) fn end_batch(&mut self, outermost: bool) {
        if outermost {
            self.credential_guard.batch = None;
        }
    }

    /// What is known of Credential Guard for the Settings page: the answer kept, else the
    /// last one, which says why it could not be had; `None` before any check answered.
    /// Never waits.
    #[must_use]
    pub fn credential_guard_status(&self) -> Option<Status> {
        self.credential_guard
            .detector
            .cached()
            .or_else(|| self.credential_guard.last.clone())
    }

    /// Checks Credential Guard in the background while the setting asks for it and nothing
    /// is known or being checked, so that the first session does not wait for it: at start,
    /// and when the setting is turned on.
    pub fn warm_credential_guard(&mut self) -> Vec<Effect> {
        if !self.settings.require_credential_guard
            || self.credential_guard.detector.cached().is_some()
            || std::mem::replace(&mut self.credential_guard.checking, true)
        {
            return Vec::new();
        }
        vec![Effect::CheckCredentialGuard(Arc::clone(
            &self.credential_guard.detector,
        ))]
    }

    /// Whether an embedded RDP session must pass the gate now: the setting is on, the one
    /// being opened has not passed it, and Credential Guard is not known to run.
    pub(super) fn credential_guard_needed(&self) -> bool {
        self.settings.require_credential_guard
            && !self.credential_guard.passed
            && !self
                .credential_guard
                .detector
                .cached()
                .is_some_and(|status| status.is_active())
    }

    /// Whether saved RDP profile `id` opens in a tab, in `mode` when one is chosen this
    /// once: neither in Remote Desktop Connection nor through an RD Gateway, which opens
    /// there.
    fn opens_embedded(&self, id: &ProfileId, mode: Option<RdpMode>) -> bool {
        self.rdp_profiles().iter().any(|profile| {
            &profile.id == id
                && !mode.map_or(profile.extras.external, RdpMode::is_external)
                && profile.extras.rd_gateway().is_none()
        })
    }

    /// Whether `message` opens an embedded RDP session that must pass the gate now.
    pub(super) fn waits_for_credential_guard(&self, message: &Message) -> bool {
        if !self.credential_guard_needed() {
            return false;
        }
        match message {
            Message::OpenRdp(id) => self.opens_embedded(id, None),
            Message::OpenRdpWith { id, mode } => self.opens_embedded(id, Some(*mode)),
            // Held whole, so the session opened is merged when it opens.
            Message::Split(SplitMessage::OpenInSplit { profile, .. }) => {
                self.opens_embedded(profile, None)
            }
            Message::Split(SplitMessage::QuickConnect {
                result: QuickResult::Profile(profile),
                ..
            }) => self.opens_embedded(&profile.id, None),
            // An RDP tab is always a desktop in the application: held whole, so the session
            // opens again in its tab's place.
            Message::ReconnectTab(tab) => self
                .tab(*tab)
                .is_some_and(|tab| tab.purpose == Purpose::Rdp && self.can_restart(tab)),
            _ => false,
        }
    }

    /// Holds `message` until the check answers, and starts it unless it runs already;
    /// refused at once when Credential Guard is known not to run.
    pub(super) fn wait_for_credential_guard(&mut self, message: Message) -> Vec<Effect> {
        if self.credential_guard.detector.cached().is_some() {
            self.refuse_for_credential_guard(&[message]);
            return Vec::new();
        }
        log::info!("a connection waits for the Credential Guard check");
        self.credential_guard.waiting.push(message);
        if std::mem::replace(&mut self.credential_guard.checking, true) {
            Vec::new()
        } else {
            vec![Effect::CheckCredentialGuard(Arc::clone(
                &self.credential_guard.detector,
            ))]
        }
    }

    /// The check answered `status`: kept when definitive; the sessions held open, past the
    /// gate, when it runs or the setting was turned off meanwhile; else they are refused.
    pub(super) fn credential_guard_answered(&mut self, status: Status) -> Vec<Effect> {
        let active = status.is_active();
        self.credential_guard.detector.record(&status);
        self.credential_guard.checking = false;
        self.credential_guard.last = Some(status);
        let waiting = std::mem::take(&mut self.credential_guard.waiting);
        if self.settings.require_credential_guard && !active {
            self.refuse_for_credential_guard(&waiting);
            return Vec::new();
        }
        self.credential_guard.passed = true;
        let mut effects = Vec::new();
        for message in waiting {
            effects.extend(self.update(message));
        }
        self.credential_guard.passed = false;
        effects
    }

    /// Gives up `messages`: an auto-reconnect held stops, each is said in the log as the C#
    /// `LogEmbeddedCredentialGuardBlocked`, and the reason once for the batch.
    fn refuse_for_credential_guard(&mut self, messages: &[Message]) {
        if messages.is_empty() {
            return;
        }
        for message in messages {
            if let Message::AutoReconnect { tab, .. } = message
                && let Some(tab) = self.tab_mut(*tab)
            {
                tab.retry = None;
            }
            let name = self.refused_name(message).unwrap_or_default();
            log::warn!("Embedded RDP blocked: Credential Guard not enabled for {name}");
        }
        let said = match &mut self.credential_guard.batch {
            Some(said) => std::mem::replace(said, true),
            None => false,
        };
        if !said {
            self.tell(Notice::CredentialGuardRequired);
        }
    }

    /// The name of the session `message` opens, for the log.
    fn refused_name(&self, message: &Message) -> Option<String> {
        let profile = |id: &ProfileId| {
            self.rdp_profiles()
                .iter()
                .find(|profile| &profile.id == id)
                .map(|profile| profile.name.clone())
        };
        match message {
            Message::OpenRdp(id)
            | Message::OpenRdpWith { id, .. }
            | Message::Split(SplitMessage::OpenInSplit { profile: id, .. }) => profile(id),
            Message::Split(SplitMessage::QuickConnect {
                result: QuickResult::Profile(summary),
                ..
            }) => profile(&summary.id),
            Message::ReconnectTab(tab) | Message::AutoReconnect { tab, .. } => {
                self.tab(*tab).map(|tab| tab.profile.name().to_owned())
            }
            _ => None,
        }
    }
}
