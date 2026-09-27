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

//! `WinRM` tabs: a local `PowerShell` in a terminal, entering a remote session.
//!
//! Heimdall writes the command from the profile's checked fields, so nothing typed by the
//! user runs as such and no approval is asked. The password never passes through Heimdall:
//! `PowerShell` asks for it in the terminal.

use heimdall_core::profile::ProfileId;
use heimdall_core::winrm::{self, CommandError, POWERSHELL_ARGUMENTS};
use heimdall_term::local::{self, LocalArguments};
use tokio_util::sync::CancellationToken;

use super::reconnect::Reopen;
use super::{App, Effect, Phase, Tab, TabProfile};
use crate::driver::Purpose;
use crate::error::UiError;
use crate::ids::{AttemptId, TabId};
use crate::local_driver::LocalShell;

/// `PowerShell` 7, preferred where it is installed: the only one on Linux.
const POWERSHELL_CORE: &str = "pwsh";

impl App {
    /// Opens a `WinRM` tab for a saved profile.
    pub(super) fn open_winrm(&mut self, id: &ProfileId) -> Vec<Effect> {
        let Some(profile) = self.winrm_profiles().iter().find(|p| &p.id == id).cloned() else {
            return Vec::new();
        };
        let effects = match winrm::enter_session(&profile) {
            Ok(command) => {
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
            Err(error) => {
                self.open_refused(profile.name, command_error(&error));
                Vec::new()
            }
        };
        // Opened again from the profile, checked again: never the refused tab's empty shell.
        self.reopened_by(Reopen::Profile(profile.id));
        effects
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

fn command_error(error: &CommandError) -> UiError {
    match error {
        CommandError::InvalidHost => UiError::InvalidHost,
        CommandError::InvalidUsername => UiError::InvalidUsername,
    }
}
