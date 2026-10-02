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

//! `WinRM` tabs: a local `PowerShell` in a terminal, entering a remote session and ending
//! with it.
//!
//! Heimdall writes the command from the profile's checked fields, so nothing typed by the
//! user runs as such and no approval is asked. The password never passes through Heimdall:
//! `PowerShell` asks for it in the terminal. A failed sign-in, a remote `exit` or a dropped
//! connection ends the tab's `PowerShell` rather than leaving a prompt that runs on this
//! machine under the remote host's name; see [`winrm::session_command`].

use heimdall_core::profile::{ProfileId, SshGateway, WinRmProfile};
use heimdall_core::winrm::{self, POWERSHELL_ARGUMENTS};
use heimdall_term::local::{self, LocalArguments};
use tokio_util::sync::CancellationToken;

use super::reconnect::Reopen;
use super::{App, Effect, Phase, Tab, TabProfile, terminal_size};
use crate::driver::Purpose;
use crate::error::UiError;
use crate::ids::{AttemptId, TabId};
use crate::local_driver::LocalShell;
use crate::winrm_driver::WinRmRequest;

/// `PowerShell` 7, preferred where it is installed: the only one on Linux.
const POWERSHELL_CORE: &str = "pwsh";

impl App {
    /// Opens a `WinRM` tab for a saved profile.
    pub(super) fn open_winrm(&mut self, id: &ProfileId) -> Vec<Effect> {
        let Some(profile) = self.winrm_profiles().iter().find(|p| &p.id == id).cloned() else {
            return Vec::new();
        };
        let id = profile.id.clone();
        let effects = self.open_winrm_profile(profile);
        // Opened again from the profile, checked again: never the refused tab's empty shell.
        self.reopened_by(Reopen::Profile(id));
        effects
    }

    /// Opens a `WinRM` tab for `profile`: a local `PowerShell` entering the session, through
    /// the profile's SSH gateway when it names one.
    pub(super) fn open_winrm_profile(&mut self, profile: WinRmProfile) -> Vec<Effect> {
        let command = match winrm::session_command(&profile) {
            Ok(command) => command,
            Err(error) => {
                self.open_refused(profile.name, UiError::from(&error));
                return Vec::new();
            }
        };
        if profile.gateway.is_some() {
            return self.open_winrm_routed(profile);
        }
        let mut arguments: Vec<String> = POWERSHELL_ARGUMENTS
            .iter()
            .map(|argument| (*argument).to_owned())
            .collect();
        arguments.push(command);
        self.open_local(LocalShell {
            name: profile.name,
            program: Some(powershell()),
            arguments: LocalArguments::List(arguments),
            working_directory: None,
        })
    }

    /// A tab whose attempt opens the route, the forward, then `PowerShell`.
    fn open_winrm_routed(&mut self, profile: WinRmProfile) -> Vec<Effect> {
        let tab_id = TabId::fresh();
        let attempt = AttemptId::fresh();
        let cancel = CancellationToken::new();
        let request = self.winrm_request(&profile, cancel.clone());
        let mut tab = Tab::new(
            self.terminal_palette(),
            tab_id,
            TabProfile::WinRm(profile),
            Purpose::Shell,
            self.viewport,
            attempt,
            cancel,
        );
        let effects = match request {
            Ok(request) => vec![Effect::ConnectWinRm {
                tab: tab_id,
                attempt,
                request: Box::new(request),
            }],
            Err(error) => {
                tab.phase = Phase::Failed(error);
                Vec::new()
            }
        };
        self.tabs.push(tab);
        self.active = Some(tab_id);
        effects
    }

    /// The attempt of `tab_id`'s `WinRM` session again, in place: once a gateway's key is
    /// trusted.
    pub(super) fn reconnect_winrm(&mut self, tab_id: TabId) -> Vec<Effect> {
        let Some(TabProfile::WinRm(profile)) = self.tab(tab_id).map(|tab| tab.profile.clone())
        else {
            return Vec::new();
        };
        let attempt = AttemptId::fresh();
        let cancel = CancellationToken::new();
        let request = self.winrm_request(&profile, cancel.clone());
        let Some(tab) = self.tab_mut(tab_id) else {
            return Vec::new();
        };
        tab.attempt = attempt;
        tab.cancel = cancel;
        match request {
            Ok(request) => {
                tab.phase = Phase::Connecting;
                vec![Effect::ConnectWinRm {
                    tab: tab_id,
                    attempt,
                    request: Box::new(request),
                }]
            }
            Err(error) => {
                tab.phase = Phase::Failed(error);
                Vec::new()
            }
        }
    }

    /// What reaching `profile` through its gateway needs; an error when the route cannot be
    /// followed.
    fn winrm_request(
        &self,
        profile: &WinRmProfile,
        cancel: CancellationToken,
    ) -> Result<WinRmRequest, UiError> {
        let route = self
            .store
            .route(profile.gateway.as_ref())
            .map_err(UiError::Route)?;
        let ssh = self.ssh_options();
        Ok(WinRmRequest {
            profile: profile.clone(),
            program: powershell(),
            route: route.iter().map(SshGateway::as_hop).collect(),
            ssh,
            size: terminal_size(self.viewport, None),
            fallback_directory: self.config.files_start.clone(),
            cancel,
        })
    }

    /// A tab that says why nothing was started.
    fn open_refused(&mut self, name: String, error: UiError) {
        let tab_id = TabId::fresh();
        let mut tab = Tab::new(
            self.terminal_palette(),
            tab_id,
            TabProfile::Local(LocalShell {
                name,
                program: None,
                arguments: LocalArguments::List(Vec::new()),
                working_directory: None,
            }),
            Purpose::Shell,
            self.viewport,
            AttemptId::fresh(),
            CancellationToken::new(),
        );
        tab.phase = Phase::Failed(error);
        self.tabs.push(tab);
        self.active = Some(tab_id);
    }
}

/// The `PowerShell` a `WinRM` tab runs: `pwsh` from `PATH`, else on Windows the one Windows
/// ships. When there is none the name is kept, and the tab says it was not found.
fn powershell() -> String {
    local::program_path(Some(POWERSHELL_CORE))
        .or_else(|error| {
            if cfg!(windows) {
                local::program_path(None)
            } else {
                Err(error)
            }
        })
        .map_or_else(
            |_| POWERSHELL_CORE.to_owned(),
            |path| path.to_string_lossy().into_owned(),
        )
}
