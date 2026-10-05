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

//! The background check of every server, as the C# session health monitor: at start, then
//! every interval, each server's address is dialled, a few at a time, and the tree's dot
//! says what answered. A server behind a gateway is not dialled from here, nor one with no
//! address. Nothing is sent: a TCP connection opened and closed.

use std::collections::HashMap;
use std::time::Duration;

use heimdall_core::profile::ProfileId;
use heimdall_core::settings::Reachability;

use super::{App, Effect};
use crate::reachability::{Probe, Unchecked, Verdict};

/// What the background check knows of the servers.
#[derive(Debug, Default)]
pub(super) struct Monitor {
    /// The last verdict of each server.
    verdicts: HashMap<ProfileId, Verdict>,
    /// Checks of the current round not answered yet: a round still running is not started
    /// again.
    pending: usize,
    /// The first round was started.
    started: bool,
}

impl App {
    /// What the background check last found of profile `id`; `None` while it is off or
    /// has not looked yet.
    #[must_use]
    pub fn reachability(&self, id: &ProfileId) -> Option<&Verdict> {
        self.monitor.verdicts.get(id)
    }

    /// How often the background check runs; `None` while it is off.
    #[must_use]
    pub fn reachability_interval(&self) -> Option<Duration> {
        let settings = self.settings.reachability;
        settings
            .enabled
            .then(|| Duration::from_secs(u64::from(settings.interval)))
    }

    /// The first round, once the window can show it: nothing when it ran already or the
    /// check is off.
    pub fn start_reachability(&mut self) -> Vec<Effect> {
        if self.monitor.started || !self.settings.reachability.enabled {
            return Vec::new();
        }
        self.monitor.started = true;
        self.reachability_round()
    }

    /// A round: every server dialled, a few at a time, unless the last one still runs.
    /// Those that cannot be dialled from here are said so at once; a server deleted is
    /// forgotten.
    pub(super) fn reachability_round(&mut self) -> Vec<Effect> {
        let settings = self.settings.reachability;
        if !settings.enabled || self.monitor.pending > 0 {
            return Vec::new();
        }
        let mut probes = Vec::new();
        let mut verdicts = HashMap::new();
        for profile in self.profile_summaries() {
            let id = profile.id;
            let verdict = match (&profile.gateway, profile.endpoint) {
                (Some(_), _) => Verdict::Unchecked(Unchecked::BehindGateway),
                (None, None) => Verdict::Unchecked(Unchecked::NoPort),
                (None, Some((host, _))) if host.trim().is_empty() => {
                    Verdict::Unchecked(Unchecked::NoHost)
                }
                (None, Some((host, port))) => {
                    probes.push(Probe {
                        id: id.clone(),
                        host: host.trim().to_owned(),
                        port,
                    });
                    // What it said last stays until it says otherwise.
                    self.monitor
                        .verdicts
                        .remove(&id)
                        .filter(|verdict| !matches!(verdict, Verdict::Unchecked(_)))
                        .unwrap_or(Verdict::Checking)
                }
            };
            verdicts.insert(id, verdict);
        }
        self.monitor.verdicts = verdicts;
        if probes.is_empty() {
            return Vec::new();
        }
        self.monitor.pending = probes.len();
        vec![Effect::CheckReachability {
            probes,
            timeout: Duration::from_millis(u64::from(settings.timeout)),
            at_once: usize::try_from(settings.probes).unwrap_or(1).max(1),
        }]
    }

    /// Server `id` answered the background check, or did not.
    pub(super) fn reachability_checked(&mut self, id: &ProfileId, verdict: Verdict) {
        self.monitor.pending = self.monitor.pending.saturating_sub(1);
        // Off, or deleted, since: nothing to say.
        if self.settings.reachability.enabled
            && let Some(known) = self.monitor.verdicts.get_mut(id)
        {
            *known = verdict;
        }
    }

    /// The background check as the settings now say, from `before`: turned off, it forgets
    /// what it knew; turned on, it looks at once.
    pub(super) fn reachability_changed(&mut self, before: Reachability) -> Vec<Effect> {
        let now = self.settings.reachability;
        match (before.enabled, now.enabled) {
            (true, false) => {
                self.monitor = Monitor::default();
                Vec::new()
            }
            (false, true) => {
                self.monitor.started = true;
                self.reachability_round()
            }
            _ => Vec::new(),
        }
    }
}
