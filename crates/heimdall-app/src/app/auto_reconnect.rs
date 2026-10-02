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

//! Auto-reconnect, as the C# Heimdall's for RDP, on unless the profile (or the application's
//! RDP options it follows) turns it off: a desktop that drops for a reason that may pass (the network, a time-out, the protocol) is
//! opened again in its tab by itself, after 2, 5, then 15 seconds, up to 20 attempts. Cancel
//! stops it; anything that needs the user (a question, a key to trust) stops it too.
//!
//! An SSH shell does the same when its connection is lost or cannot be made again, as the C#
//! `SshAutoReconnect`: off unless the setting turns it on, up to the attempts it sets.

use std::time::{Duration, Instant};

use super::{App, Effect, Message, Phase, TabProfile};
use crate::driver::Purpose;
use crate::error::UiError;
use crate::ids::{AttemptId, TabId};

/// Most attempts for an RDP desktop, as the C# default.
pub const RDP_MAX_ATTEMPTS: u32 = 20;

/// Wait before the first attempt, the second, and every one after, as the C# defaults.
const DELAYS: [Duration; 3] = [
    Duration::from_secs(2),
    Duration::from_secs(5),
    Duration::from_secs(15),
];

/// A tab waiting to open its session again by itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Retry {
    /// Which attempt comes next, from 1.
    pub attempt: u32,
    /// The last one there will be.
    pub max: u32,
    /// When it starts.
    pub due: Instant,
}

/// Whether a session of `purpose` that failed with `error` may come back by itself: for
/// SSH, the causes the C# `SshReconnectPolicy` retries.
fn transient(purpose: Purpose, error: &UiError) -> bool {
    match purpose {
        Purpose::Rdp => matches!(
            error,
            UiError::Network { .. } | UiError::Timeout | UiError::RdpProtocol { .. }
        ),
        _ => matches!(
            error,
            UiError::Network { .. }
                | UiError::Timeout
                | UiError::ConnectionLost
                | UiError::Disconnected { .. }
        ),
    }
}

/// The wait before attempt `attempt`, counted from 1.
fn delay(attempt: u32) -> Duration {
    let index = usize::try_from(attempt.saturating_sub(1)).unwrap_or(usize::MAX);
    DELAYS[index.min(DELAYS.len() - 1)]
}

impl App {
    /// After the session of `tab_id` failed with `error`: the next attempt of its chain, a
    /// chain started when the session was live, or none.
    pub(super) fn retry_after(
        &mut self,
        tab_id: TabId,
        error: &UiError,
        was_live: bool,
    ) -> Vec<Effect> {
        let ssh_attempts = self
            .settings
            .ssh_auto_reconnect
            .then_some(self.settings.ssh_auto_reconnect_attempts);
        let Some(tab) = self.tab_mut(tab_id) else {
            return Vec::new();
        };
        // An RDP desktop whose profile asks; an SSH shell when the setting asks; nothing else.
        let max = match (&tab.profile, tab.purpose) {
            (TabProfile::Rdp(profile), Purpose::Rdp) => {
                profile.auto_reconnect.then_some(RDP_MAX_ATTEMPTS)
            }
            (TabProfile::Ssh(_), Purpose::Shell) => ssh_attempts,
            _ => None,
        };
        let attempt = match tab.retry {
            Some(retry) => retry.attempt + 1,
            None if was_live => 1,
            None => return Vec::new(),
        };
        let Some(max) = max.filter(|max| transient(tab.purpose, error) && attempt <= *max) else {
            tab.retry = None;
            return Vec::new();
        };
        let due = Instant::now() + delay(attempt);
        tab.retry = Some(Retry { attempt, max, due });
        vec![Effect::RetryAt {
            tab: tab_id,
            attempt: tab.attempt,
            deadline: due,
        }]
    }

    /// Applies a message about a tab's attempts.
    pub(super) fn retry_message(&mut self, message: &Message) -> Vec<Effect> {
        match *message {
            Message::AutoReconnect { tab, attempt } => self.auto_reconnect(tab, attempt),
            Message::CancelAutoReconnect(tab) => {
                self.cancel_auto_reconnect(tab);
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    /// The time of the attempt of `tab_id` waiting after `attempt` has come: its session
    /// opens again in its place, unless it was cancelled, closed or reopened meanwhile.
    fn auto_reconnect(&mut self, tab_id: TabId, attempt: AttemptId) -> Vec<Effect> {
        let waiting = self.tab(tab_id).is_some_and(|tab| {
            tab.retry.is_some() && tab.attempt == attempt && matches!(tab.phase, Phase::Failed(_))
        });
        if !waiting {
            return Vec::new();
        }
        if self
            .tab(tab_id)
            .is_some_and(|tab| tab.purpose == Purpose::Rdp)
        {
            return self.reconnect_rdp(tab_id, None);
        }
        // A shell opens again in a new tab in the same place; the chain goes with it, and a
        // tab in the background stays there.
        let retry = self.tab(tab_id).and_then(|tab| tab.retry);
        let index = self.tabs.iter().position(|tab| tab.id == tab_id);
        let shown = self.active;
        let effects = self.reconnect_tab(tab_id);
        if let Some(tab) = index.and_then(|index| self.tabs.get_mut(index))
            && tab.id != tab_id
        {
            tab.retry = retry;
        }
        if shown != Some(tab_id) {
            self.active = shown;
        }
        effects
    }

    /// Stops the attempts of `tab_id`: its failure is shown, with Reconnect.
    fn cancel_auto_reconnect(&mut self, tab_id: TabId) {
        if let Some(tab) = self.tab_mut(tab_id) {
            tab.retry = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_waits_are_the_csharp_ones() {
        assert_eq!(delay(1), Duration::from_secs(2));
        assert_eq!(delay(2), Duration::from_secs(5));
        assert_eq!(delay(3), Duration::from_secs(15));
        assert_eq!(delay(RDP_MAX_ATTEMPTS), Duration::from_secs(15));
        assert_eq!(delay(0), Duration::from_secs(2), "never before the first");
    }
}
