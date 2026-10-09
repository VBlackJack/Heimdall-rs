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

//! Citrix profiles: their application launched outside Heimdall, as the C# `CitrixHandler`
//! launches it, and a tab showing its status, as the C# `EmbeddedCitrixView` in its external
//! mode: how it launched, and its client's state, looked at every 3 seconds while the tab is
//! open. Closing the tab stops looking; the Citrix session is left as it is. Only the
//! user's Terminate, confirmed, ends it, as [`crate::citrix_terminate`] says.

use std::time::Instant;

use heimdall_core::profile::{CitrixProfile, ProfileId};
use tokio_util::sync::CancellationToken;

use super::{App, Dialog, Effect, Notice, Phase, Tab, TabProfile};
use crate::citrix::{self, CitrixRefusal};
use crate::citrix_session::{CitrixPane, ClientState, LaunchMethod, Launched, Pids, Probe};
use crate::citrix_terminate::{TerminateOffer, TerminateResult};
use crate::driver::Purpose;
use crate::ids::{AttemptId, TabId};

/// What a Citrix tab's phase is, its client being `state`: connecting until a client is
/// seen or cannot be looked for, closed once it ended or nothing ran.
fn phase_of(state: &ClientState) -> Phase {
    match state {
        ClientState::Launching | ClientState::NotFoundYet => Phase::Connecting,
        ClientState::Running(_) | ClientState::Shared | ClientState::Untracked(_) => {
            Phase::Connected
        }
        ClientState::Ended(_) | ClientState::LauncherFailed(_) | ClientState::NotStarted(_) => {
            Phase::Closed { exit_status: None }
        }
    }
}

/// The tab's phase follows its client, once it changed; said in the log, once a change.
fn follow(tab: &mut Tab) {
    let Some(pane) = tab.citrix.as_deref() else {
        return;
    };
    let state = pane.tracker.state();
    log::info!("Citrix tab {}: client {state:?}", tab.id.value());
    tab.phase = phase_of(state);
}

impl App {
    /// Launches saved Citrix profile `id`'s application and opens its status tab.
    pub(super) fn open_citrix(&mut self, id: &ProfileId) -> Vec<Effect> {
        let Some(profile) = self
            .store
            .citrix_profiles()
            .iter()
            .find(|p| &p.id == id)
            .cloned()
        else {
            return Vec::new();
        };
        self.open_citrix_profile(profile)
    }

    /// Launches `profile`'s application, with its cache launch line when the vault holds
    /// one: checked here, started off the UI thread, its status tab opened at once;
    /// refused, the status bar says why, as the C# error does, and no tab opens.
    pub(super) fn open_citrix_profile(&mut self, profile: CitrixProfile) -> Vec<Effect> {
        // A locked vault may hold a cache line it cannot give: the application is launched
        // another way when the profile has one, rather than refused as the C# refuses it;
        // with none, the vault is to be unlocked.
        let planned = match self.citrix_launch_line(&profile.id) {
            Ok(line) => citrix::plan(&profile, line.as_deref().map(String::as_str)),
            Err(CitrixRefusal::VaultLocked) => match citrix::plan(&profile, None) {
                Err(CitrixRefusal::NotConfigured) => Err(CitrixRefusal::VaultLocked),
                planned => planned,
            },
            Err(refusal) => Err(refusal),
        };
        let launch = match planned {
            Ok(launch) => launch,
            Err(refusal) => {
                self.tell(Notice::CitrixRefused(refusal));
                return Vec::new();
            }
        };
        let tab_id = TabId::fresh();
        let name = profile.name.clone();
        let mut tab = Tab::new(
            self.terminal_palette(),
            tab_id,
            TabProfile::Citrix(profile),
            Purpose::Citrix,
            self.viewport,
            AttemptId::fresh(),
            CancellationToken::new(),
        );
        tab.citrix = Some(Box::new(CitrixPane::new(LaunchMethod::of(&launch))));
        follow(&mut tab);
        self.tabs.push(tab);
        self.active = Some(tab_id);
        self.tell(Notice::CitrixLaunching);
        vec![Effect::LaunchCitrix {
            tab: tab_id,
            name,
            launch,
        }]
    }

    /// The Citrix application `name` of tab `tab` was launched, or why not.
    pub(super) fn citrix_launched(
        &mut self,
        tab: TabId,
        name: &str,
        result: Result<Launched, CitrixRefusal>,
    ) -> Vec<Effect> {
        let notice = match &result {
            Ok(_) => Notice::CitrixLaunched(name.to_owned()),
            Err(refusal) => Notice::CitrixRefused(refusal.clone()),
        };
        // Its tab follows first: the notice is said of the state it is now in. Closed
        // meanwhile, the launch is left to Citrix.
        if let Some(found) = self.tab_mut(tab)
            && let Some(pane) = found.citrix.as_deref_mut()
        {
            let changed = match result {
                Ok(launched) => pane.launched(launched),
                Err(refusal) => pane.tracker.refused(refusal),
            };
            if changed {
                follow(found);
            }
        }
        self.tell(notice);
        Vec::new()
    }

    /// Whether a Citrix tab looks at its client: one is open with something to learn.
    #[must_use]
    pub fn polls_citrix(&self) -> bool {
        self.tabs
            .iter()
            .filter_map(|tab| tab.citrix.as_deref())
            .any(CitrixPane::polls)
    }

    /// Time to look at the Citrix tabs' clients: a probe for each tab with none under way.
    pub(super) fn citrix_tick(&mut self) -> Vec<Effect> {
        self.tabs
            .iter_mut()
            .filter_map(|tab| {
                let (launcher, lists) = tab.citrix.as_deref_mut()?.start_probe()?;
                Some(Effect::ProbeCitrix {
                    tab: tab.id,
                    launcher,
                    lists,
                })
            })
            .collect()
    }

    /// The clients the Citrix tabs other than `tab` follow.
    fn citrix_claimed(&self, tab: TabId) -> Pids {
        self.tabs
            .iter()
            .filter(|other| other.id != tab)
            .filter_map(|other| other.citrix.as_deref()?.tracker.state().client())
            .collect()
    }

    /// What tab `tab`'s probe saw; nothing when it was closed meanwhile.
    pub(super) fn citrix_probed(&mut self, tab: TabId, probe: &Probe) {
        // Another tab's client is never this one's.
        let claimed = self.citrix_claimed(tab);
        let Some(found) = self.tab_mut(tab) else {
            return;
        };
        let Some(pane) = found.citrix.as_deref_mut() else {
            return;
        };
        let before = pane.tracker.launcher();
        let changed = pane.probed(probe, &claimed);
        let launcher = pane.tracker.launcher();
        if launcher != before {
            log::info!("Citrix tab {}: launcher {launcher:?}", found.id.value());
        }
        if changed {
            follow(found);
        }
    }

    /// What Citrix tab `tab` offers at `now` to end its session.
    #[must_use]
    pub fn citrix_terminate_offer(&self, tab: TabId, now: Instant) -> TerminateOffer {
        let claimed = self.citrix_claimed(tab);
        self.tab(tab)
            .and_then(|found| found.citrix.as_deref())
            .map_or(TerminateOffer::Nothing, |pane| {
                pane.terminate_offer(&claimed, now)
            })
    }

    /// The client of tab `tab` that Terminate, or Force terminate when `force`, would end at
    /// `now`, when it is offered.
    fn citrix_terminable(&self, tab: TabId, force: bool, now: Instant) -> Option<u32> {
        let offered = match self.citrix_terminate_offer(tab, now) {
            TerminateOffer::Terminate => !force,
            TerminateOffer::Force(_) => force,
            TerminateOffer::Nothing
            | TerminateOffer::Pending { .. }
            | TerminateOffer::Asked { .. } => false,
        };
        if !offered {
            return None;
        }
        self.tab(tab)?.citrix.as_deref()?.tracker.state().client()
    }

    /// Terminate, or Force terminate, pressed in tab `tab`: asked first, as the C# asks.
    pub(super) fn request_citrix_terminate(&mut self, tab: TabId, force: bool, now: Instant) {
        if let Some(pid) = self.citrix_terminable(tab, force, now) {
            self.dialog = Some(Dialog::ConfirmCitrixTerminate { tab, pid, force });
        }
    }

    /// Ending client `pid` of tab `tab` confirmed: asked of `taskkill.exe`, when it is still
    /// the client offered; the launcher is never the fallback.
    pub(super) fn confirm_citrix_terminate(
        &mut self,
        tab: TabId,
        pid: u32,
        force: bool,
        now: Instant,
    ) -> Vec<Effect> {
        if self.citrix_terminable(tab, force, now) != Some(pid) {
            log::info!(
                "Citrix tab {}: terminate not requested, the client changed",
                tab.value()
            );
            return Vec::new();
        }
        if let Some(pane) = self
            .tab_mut(tab)
            .and_then(|found| found.citrix.as_deref_mut())
        {
            pane.terminate_started(pid, force, now);
        }
        log::info!(
            "Citrix tab {}: terminate client {pid} requested, force {force}",
            tab.value()
        );
        vec![Effect::TerminateCitrix { tab, pid, force }]
    }

    /// What the request to end client `pid` of tab `tab` came to, at `now`; nothing when the
    /// tab was closed meanwhile.
    pub(super) fn citrix_terminated(
        &mut self,
        tab: TabId,
        pid: u32,
        result: TerminateResult,
        now: Instant,
    ) {
        log::info!(
            "Citrix tab {}: terminate client {pid}: {result:?}",
            tab.value()
        );
        if let Some(pane) = self
            .tab_mut(tab)
            .and_then(|found| found.citrix.as_deref_mut())
        {
            pane.terminate_answered(pid, result, now);
        }
    }
}
