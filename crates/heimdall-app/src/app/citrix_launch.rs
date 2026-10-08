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
//! open. Closing the tab stops looking; the Citrix session is left as it is.

use heimdall_core::profile::{CitrixProfile, ProfileId};
use tokio_util::sync::CancellationToken;

use super::{App, Effect, Notice, Phase, Tab, TabProfile};
use crate::citrix::{self, CitrixRefusal};
use crate::citrix_session::{CitrixPane, ClientState, LaunchMethod, Launched, Pids, Probe};
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

    /// What tab `tab`'s probe saw; nothing when it was closed meanwhile.
    pub(super) fn citrix_probed(&mut self, tab: TabId, probe: &Probe) {
        // Another tab's client is never this one's.
        let claimed: Pids = self
            .tabs
            .iter()
            .filter(|other| other.id != tab)
            .filter_map(|other| other.citrix.as_deref()?.tracker.state().client())
            .collect();
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
}
