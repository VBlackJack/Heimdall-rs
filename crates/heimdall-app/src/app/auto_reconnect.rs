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

//! Auto-reconnect, as the C# Heimdall's for RDP, on unless a setting turns it off there: a
//! desktop that drops for a reason that may pass (the network, a time-out, the protocol) is
//! opened again in its tab by itself, after 2, 5, then 15 seconds, up to 20 attempts. Cancel
//! stops it; anything that needs the user (a question, a key to trust) stops it too.
//!
//! SSH has the same mechanism in the C# Heimdall, off unless a setting turns it on; it comes
//! with that setting.

use std::time::{Duration, Instant};

use super::{App, Effect, Phase};
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

/// Whether a session that failed with `error` may come back by itself.
fn transient(error: &UiError) -> bool {
    matches!(
        error,
        UiError::Network { .. } | UiError::Timeout | UiError::RdpProtocol { .. }
    )
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
        let Some(tab) = self.tab_mut(tab_id) else {
            return Vec::new();
        };
        let attempt = match tab.retry {
            Some(retry) => retry.attempt + 1,
            None if was_live => 1,
            None => return Vec::new(),
        };
        if tab.purpose != Purpose::Rdp || !transient(error) || attempt > RDP_MAX_ATTEMPTS {
            tab.retry = None;
            return Vec::new();
        }
        let due = Instant::now() + delay(attempt);
        tab.retry = Some(Retry {
            attempt,
            max: RDP_MAX_ATTEMPTS,
            due,
        });
        vec![Effect::RetryAt {
            tab: tab_id,
            attempt: tab.attempt,
            deadline: due,
        }]
    }

    /// The time of the attempt of `tab_id` waiting after `attempt` has come: its session
    /// opens again in its place, unless it was cancelled, closed or reopened meanwhile.
    pub(super) fn auto_reconnect(&mut self, tab_id: TabId, attempt: AttemptId) -> Vec<Effect> {
        let waiting = self.tab(tab_id).is_some_and(|tab| {
            tab.retry.is_some() && tab.attempt == attempt && matches!(tab.phase, Phase::Failed(_))
        });
        if !waiting {
            return Vec::new();
        }
        self.reconnect_rdp(tab_id, None)
    }

    /// Stops the attempts of `tab_id`: its failure is shown, with Reconnect.
    pub(super) fn cancel_auto_reconnect(&mut self, tab_id: TabId) {
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
