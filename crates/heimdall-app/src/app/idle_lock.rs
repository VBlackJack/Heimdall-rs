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

//! The workspace lock beyond Ctrl+L, as the C# `WorkspaceLockService` has it:
//! - the idle auto-lock: the window measures how long the computer has had no input, every
//!   [`IDLE_POLL`] while the vault is open and a threshold is set, and the workspace locks
//!   once that reaches the threshold, as the C# `VaultIdlePolicy` decides;
//! - disconnect on lock: off, the sessions go on running hidden behind the lock screen, the
//!   C# "survive and mask"; on, every session closes when the workspace locks, without a
//!   question, as a lock must lock;
//! - a session's auto-reconnect waits while the workspace is locked, as the C#
//!   `VaultReconnectPolicy`: its saved password cannot be read then. It is attempted once
//!   the master password is typed again.

use std::time::Duration;

use super::{App, Effect, Message, VaultStatus};
use crate::ids::{AttemptId, TabId};

/// How often the window measures the computer's idle time, as the C# `IdlePollSeconds`.
pub const IDLE_POLL: Duration = Duration::from_secs(5);

/// Milliseconds in a minute.
const MILLIS_PER_MINUTE: u64 = 60_000;

/// Whether the workspace locks after `idle_ms` milliseconds without input, with a threshold
/// of `minutes`, as the C# `VaultIdlePolicy`: never with a threshold of 0, else once the idle
/// time reaches it.
#[must_use]
pub fn should_auto_lock(idle_ms: u64, minutes: u32) -> bool {
    minutes != 0 && idle_ms >= u64::from(minutes).saturating_mul(MILLIS_PER_MINUTE)
}

impl App {
    /// Whether the window measures the idle time: the vault open, the workspace not locked,
    /// and a threshold set.
    #[must_use]
    pub fn watches_idle(&self) -> bool {
        self.vault_status() == VaultStatus::Open
            && !self.is_locked()
            && self.settings.auto_lock_idle_minutes != 0
    }

    /// The computer has had no input for `idle`: the workspace locks when that reaches the
    /// threshold.
    pub(super) fn idle_measured(&mut self, idle: Duration) {
        if !self.watches_idle() {
            return;
        }
        let minutes = self.settings.auto_lock_idle_minutes;
        let idle_ms = u64::try_from(idle.as_millis()).unwrap_or(u64::MAX);
        if should_auto_lock(idle_ms, minutes) {
            log::info!("Workspace auto-lock: no input for {minutes} min.");
            self.lock_vault();
        }
    }

    /// Closes every session as the workspace locks, those in a split or a window of their
    /// own included, without a question: one asked would leave it open and connected behind
    /// the lock screen, as the C# `DisconnectAllSessionsForLock` says.
    pub(super) fn close_all_for_lock(&mut self) {
        self.closing_pane = None;
        let tabs: Vec<TabId> = self.tabs.iter().map(|tab| tab.id).collect();
        if !tabs.is_empty() {
            log::info!("Disconnecting every session on lock, as the settings ask.");
        }
        for tab in tabs {
            // A pane already closed with its split.
            if self.tab(tab).is_some() {
                self.close_tab(tab);
            }
        }
    }

    /// Whether the auto-reconnect of `tab` after `attempt` waits for the workspace to be
    /// unlocked; it is then kept to be attempted at the unlock.
    pub(super) fn defer_reconnect(&mut self, tab: TabId, attempt: AttemptId) -> bool {
        if !self.is_locked() {
            return false;
        }
        if !self.deferred_reconnects.contains(&(tab, attempt)) {
            log::info!("auto-reconnect deferred until the workspace is unlocked");
            self.deferred_reconnects.push((tab, attempt));
        }
        true
    }

    /// The auto-reconnects that waited for the unlock, attempted now: those of a tab closed
    /// or reconnected meanwhile do nothing.
    pub(super) fn resume_reconnects(&mut self) -> Vec<Effect> {
        let waiting = std::mem::take(&mut self.deferred_reconnects);
        waiting
            .into_iter()
            .flat_map(|(tab, attempt)| self.retry_message(&Message::AutoReconnect { tab, attempt }))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_threshold_of_zero_never_locks() {
        assert!(!should_auto_lock(0, 0));
        assert!(!should_auto_lock(u64::MAX, 0));
    }

    #[test]
    fn the_workspace_locks_once_the_idle_time_reaches_the_threshold() {
        assert!(!should_auto_lock(MILLIS_PER_MINUTE - 1, 1));
        assert!(should_auto_lock(MILLIS_PER_MINUTE, 1));
        assert!(should_auto_lock(MILLIS_PER_MINUTE + 1, 1));
        assert!(!should_auto_lock(15 * MILLIS_PER_MINUTE - 1, 15));
        assert!(should_auto_lock(15 * MILLIS_PER_MINUTE, 15));
    }

    #[test]
    fn the_largest_threshold_does_not_overflow() {
        let max = heimdall_core::settings::AUTO_LOCK_IDLE_MINUTES_MAX;
        assert!(!should_auto_lock(
            u64::from(max) * MILLIS_PER_MINUTE - 1,
            max
        ));
        assert!(should_auto_lock(u64::from(max) * MILLIS_PER_MINUTE, max));
        assert!(!should_auto_lock(0, u32::MAX));
    }
}
