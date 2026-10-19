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
//! opened again in its tab by itself, after 2, 5, then 15 seconds, up to the attempts the
//! settings give (20 unless changed). Cancel
//! stops it; anything that needs the user (a question, a key to trust) stops it too.
//!
//! An SSH shell does the same when its connection is lost or cannot be made again, as the C#
//! `SshAutoReconnect`: off unless the setting turns it on, up to the attempts it sets.
//!
//! An attempt due while the workspace is locked waits for the unlock, as the C#
//! `VaultReconnectPolicy` defers it.

use std::time::{Duration, Instant};

use heimdall_core::settings::{
    SSH_AUTO_RECONNECT_FIRST_DELAY_DEFAULT, SSH_AUTO_RECONNECT_SECOND_DELAY_DEFAULT,
    SSH_AUTO_RECONNECT_SUBSEQUENT_DELAY_DEFAULT, Settings,
};

use super::{App, Effect, Message, Phase, TabProfile};
use crate::driver::Purpose;
use crate::error::UiError;
use crate::ids::{AttemptId, TabId};

/// Wait before the first attempt, the second, and every one after, as the C# defaults: those
/// of an RDP desktop, which has no setting for them, and what an SSH shell's settings start
/// from.
const DELAYS: [Duration; 3] = [
    Duration::from_secs(SSH_AUTO_RECONNECT_FIRST_DELAY_DEFAULT as u64),
    Duration::from_secs(SSH_AUTO_RECONNECT_SECOND_DELAY_DEFAULT as u64),
    Duration::from_secs(SSH_AUTO_RECONNECT_SUBSEQUENT_DELAY_DEFAULT as u64),
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
    /// When the session dropped, the first attempt's wait began: the C# "elapsed" counts
    /// from it, across the attempts.
    pub since: Instant,
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

/// The wait before attempt `attempt`, counted from 1, among `delays`: the first, the second,
/// then the one every later attempt takes.
fn delay(attempt: u32, delays: [Duration; 3]) -> Duration {
    let index = usize::try_from(attempt.saturating_sub(1)).unwrap_or(usize::MAX);
    delays[index.min(delays.len() - 1)]
}

/// The waits of an SSH shell's attempts, as the C# `SshAutoReconnect*DelaySeconds` settings
/// give them.
fn ssh_delays(settings: &Settings) -> [Duration; 3] {
    [
        settings.ssh_auto_reconnect_first_delay,
        settings.ssh_auto_reconnect_second_delay,
        settings.ssh_auto_reconnect_subsequent_delay,
    ]
    .map(|seconds| Duration::from_secs(u64::from(seconds)))
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
        // A tab a schedule opened never comes back by itself: an attempt nobody watches
        // could only fail on what it would have to ask.
        if self.is_scheduled_tab(tab_id) {
            if let Some(tab) = self.tab_mut(tab_id) {
                tab.retry = None;
            }
            return Vec::new();
        }
        let ssh_attempts = self
            .settings
            .ssh_auto_reconnect
            .then_some(self.settings.ssh_auto_reconnect_attempts);
        let rdp_attempts = self.settings.rdp_auto_reconnect_attempts;
        let ssh_delays = ssh_delays(&self.settings);
        let Some(tab) = self.tab_mut(tab_id) else {
            return Vec::new();
        };
        // An RDP desktop whose profile asks; an SSH shell when the setting asks; nothing else.
        let max = match (&tab.profile, tab.purpose) {
            (TabProfile::Rdp(profile), Purpose::Rdp) => {
                profile.auto_reconnect.then_some(rdp_attempts)
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
        let now = Instant::now();
        let delays = if tab.purpose == Purpose::Rdp {
            DELAYS
        } else {
            ssh_delays
        };
        let due = now + delay(attempt, delays);
        let since = tab.retry.map_or(now, |retry| retry.since);
        tab.retry = Some(Retry {
            attempt,
            max,
            due,
            since,
        });
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
        // Locked, its saved password cannot be read: it waits for the unlock.
        if !waiting || self.defer_reconnect(tab_id, attempt) {
            return Vec::new();
        }
        // Credential Guard asked first for a desktop, then Windows Hello, as the C# reconnect
        // passes its gates; after the unlock, never behind the lock.
        if self.credential_guard_needed()
            && self
                .tab(tab_id)
                .is_some_and(|tab| tab.purpose == Purpose::Rdp)
        {
            return self.wait_for_credential_guard(Message::AutoReconnect {
                tab: tab_id,
                attempt,
            });
        }
        if self.hello_needed()
            && self
                .tab(tab_id)
                .is_some_and(|tab| tab.saved_profile().is_some())
        {
            return self.wait_for_hello(Message::AutoReconnect {
                tab: tab_id,
                attempt,
            });
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
            self.sync_focus();
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
        assert_eq!(delay(1, DELAYS), Duration::from_secs(2));
        assert_eq!(delay(2, DELAYS), Duration::from_secs(5));
        assert_eq!(delay(3, DELAYS), Duration::from_secs(15));
        assert_eq!(
            delay(
                heimdall_core::settings::RDP_AUTO_RECONNECT_ATTEMPTS_MAX,
                DELAYS
            ),
            Duration::from_secs(15)
        );
        assert_eq!(
            delay(0, DELAYS),
            Duration::from_secs(2),
            "never before the first"
        );
    }

    #[test]
    fn an_ssh_shell_waits_as_its_settings_say() {
        let mut settings = Settings::default();
        assert_eq!(ssh_delays(&settings), DELAYS, "the C# defaults");
        settings.ssh_auto_reconnect_first_delay = 1;
        settings.ssh_auto_reconnect_second_delay = 30;
        settings.ssh_auto_reconnect_subsequent_delay = 600;
        let delays = ssh_delays(&settings);
        assert_eq!(delay(1, delays), Duration::from_secs(1));
        assert_eq!(delay(2, delays), Duration::from_secs(30));
        assert_eq!(delay(3, delays), Duration::from_secs(600));
        assert_eq!(delay(9, delays), Duration::from_secs(600));
    }
}
