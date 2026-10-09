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

//! Windows Hello asked before a connection, as the C# `EnsureWindowsHelloAsync`: with
//! the setting on, a saved session waits for a successful verification before anything
//! saved for it is used. It fails closed: Windows Hello unavailable or not set up, a
//! verification that does not succeed or a request cancelled refuses the connection, said
//! in the status bar. A success counts for the grace the settings give, in memory only.
//!
//! What waits: a saved session opened, whatever opened it (the tree, Quick Connect, a
//! folder's "Connect all", "Connect selected", a restore, an RDP profile's one-time
//! "Connect with"), a saved local program included, as the C# Connect command gates it,
//! `ConnectEmbedded` and `ConnectExternal` going through the same `ConnectCoreAsync`
//! (`ServerListViewModel.cs:835-854`); one opened in a split, a tab reconnected and an
//! auto-reconnect, as each reaches the C# gate. Sessions opened while a verification is
//! asked wait for that one: a batch is asked once, and refused at once. A session saved
//! nowhere ("Connect as...", Quick Connect to a host, a local shell without a profile)
//! does not wait, as the C# ones do not.
//!
//! Two differences with the C#, on purpose: a folder's "Connect all" asks after its
//! confirmation, where the C# asks before it, so a batch the user cancels raises no prompt;
//! and a restore of several sessions is one batch asked once even with no grace, where the
//! C# asks for each session in turn.

use std::time::{Duration, Instant};

use super::split::SplitMessage;
use super::{App, Effect, Message, Notice, QuickResult};
use crate::windows_hello::HelloRefusal;

/// Seconds in a minute, for the grace counted in minutes.
const SECONDS_PER_MINUTE: u64 = 60;

/// The connection gate's state.
#[derive(Default)]
pub(super) struct HelloGate {
    /// When the last verification succeeded, in this run.
    verified_at: Option<Instant>,
    /// The messages that open a connection, held until Windows Hello answers.
    waiting: Vec<Message>,
    /// A verification is asked and not answered yet.
    asking: bool,
    /// The messages held are being applied, past the gate.
    passed: bool,
}

/// Whether a verification that succeeded at `verified_at` still counts at `now` with a
/// grace of `grace_minutes`: never with none, as the C# `0 = always re-verify`.
#[must_use]
fn within_grace(verified_at: Option<Instant>, now: Instant, grace_minutes: u32) -> bool {
    let grace = Duration::from_secs(u64::from(grace_minutes) * SECONDS_PER_MINUTE);
    verified_at.is_some_and(|at| now.saturating_duration_since(at) < grace)
}

impl App {
    /// Whether a connection must wait for Windows Hello now: the setting is on, the one
    /// being opened has not passed the gate, and no verification is within its grace.
    pub(super) fn hello_needed(&self) -> bool {
        let hello = self.settings.windows_hello;
        hello.require_on_connect
            && !self.hello.passed
            && !within_grace(self.hello.verified_at, Instant::now(), hello.grace_minutes)
    }

    /// Whether `message` opens a saved session that must wait for Windows Hello now.
    pub(super) fn waits_for_hello(&self, message: &Message) -> bool {
        if !self.hello_needed() {
            return false;
        }
        match message {
            Message::OpenProfile(_)
            | Message::OpenFiles(_)
            | Message::OpenRdp(_)
            | Message::OpenRdpWith { .. }
            | Message::OpenTelnet(_)
            | Message::OpenVnc(_)
            | Message::OpenFtp(_)
            | Message::OpenWinRm(_)
            | Message::OpenCitrix(_)
            | Message::OpenLocalProfile(_)
            // Held whole, so the session opened is merged when it opens.
            | Message::Split(
                SplitMessage::OpenInSplit { .. }
                | SplitMessage::QuickConnect {
                    result: QuickResult::Profile(_),
                    ..
                },
            ) => true,
            // Held whole, so the session opens again in its tab's place.
            Message::ReconnectTab(tab) => self
                .tab(*tab)
                .is_some_and(|tab| self.can_restart(tab) && tab.saved_profile().is_some()),
            _ => false,
        }
    }

    /// Holds `message` until Windows Hello answers; asks it unless it is asked already.
    pub(super) fn wait_for_hello(&mut self, message: Message) -> Vec<Effect> {
        log::info!("a connection waits for Windows Hello");
        self.hello.waiting.push(message);
        if std::mem::replace(&mut self.hello.asking, true) {
            Vec::new()
        } else {
            vec![Effect::VerifyWindowsHello]
        }
    }

    /// Windows Hello answered: the sessions held open, past the gate; refused, they are
    /// given up, the reason said once, and an auto-reconnect held stops.
    pub(super) fn hello_answered(&mut self, answer: Result<(), HelloRefusal>) -> Vec<Effect> {
        self.hello.asking = false;
        let waiting = std::mem::take(&mut self.hello.waiting);
        if let Err(refusal) = answer {
            log::warn!(
                "Windows Hello refused {} connection(s): {refusal:?}",
                waiting.len()
            );
            for message in &waiting {
                if let Message::AutoReconnect { tab, .. } = message
                    && let Some(tab) = self.tab_mut(*tab)
                {
                    tab.retry = None;
                }
            }
            self.tell(Notice::WindowsHelloRefused(refusal));
            return Vec::new();
        }
        self.hello.verified_at = Some(Instant::now());
        self.hello.passed = true;
        let mut effects = Vec::new();
        for message in waiting {
            effects.extend(self.update(message));
        }
        self.hello.passed = false;
        effects
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_grace_counts_minutes_and_none_asks_every_time() {
        let at = Instant::now();
        let later = |seconds| at + Duration::from_secs(seconds);
        assert!(!within_grace(None, at, 5), "never verified");
        assert!(within_grace(Some(at), later(299), 5));
        assert!(!within_grace(Some(at), later(300), 5), "five minutes past");
        assert!(!within_grace(Some(at), at, 0), "0 always asks");
        assert!(within_grace(Some(at), later(86_399), 1440));
    }
}
