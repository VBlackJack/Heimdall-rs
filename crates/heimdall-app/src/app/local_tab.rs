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

//! Local shell tabs: a terminal like an SSH shell, fed by a program on this machine.
//!
//! A saved local profile runs only what the user approved: the command as shown and the file
//! its program is found at. Anything else is shown first, whole, and runs only once agreed.

use std::path::Path;

use heimdall_core::profile::{LocalApproval, LocalArguments, LocalCommand, ProfileId};
use heimdall_term::local::{self, LocalArguments as TermArguments};
use tokio_util::sync::CancellationToken;

use super::reconnect::Reopen;
use super::{App, Dialog, Effect, Tab, TabProfile, terminal_size};
use crate::driver::Purpose;
use crate::ids::{AttemptId, TabId};
use crate::local_driver::{LocalRequest, LocalShell};
use crate::text::{server_text, visible_text};

/// What the user is asked before a saved local profile first runs, or runs something else
/// than it did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalConfirmation {
    /// Profile.
    pub id: ProfileId,
    /// Its name, made safe.
    pub name: String,
    /// What agreeing approves, and what then runs: this, not the profile read again.
    pub approval: LocalApproval,
    /// The command as it runs, every invisible character written out.
    pub command: String,
    /// The folder it starts in, every invisible character written out.
    pub folder: Option<String>,
    /// The program reads its command line again with rules of its own (`cmd.exe`, batch
    /// files): what the arguments look like is not what it does.
    pub rereads: bool,
}

impl App {
    /// Opens a tab running `shell`.
    pub(super) fn open_local(&mut self, mut shell: LocalShell) -> Vec<Effect> {
        // Where a terminal opens: the home folder, not wherever Heimdall was started from.
        shell
            .working_directory
            .get_or_insert_with(|| self.config.files_start.clone());
        let grid = self.viewport;
        let tab_id = TabId::fresh();
        let attempt = AttemptId::fresh();
        let cancel = CancellationToken::new();
        let request = LocalRequest {
            shell: shell.clone(),
            size: terminal_size(grid, None),
            fallback_directory: self.config.files_start.clone(),
            cancel: cancel.clone(),
        };
        self.tabs.push(Tab::new(
            self.terminal_palette(),
            tab_id,
            TabProfile::Local(shell),
            Purpose::Shell,
            grid,
            attempt,
            cancel,
        ));
        self.active = Some(tab_id);
        vec![Effect::ConnectLocal {
            tab: tab_id,
            attempt,
            request: Box::new(request),
        }]
    }

    /// Opens a saved local profile: at once when what it runs is approved, else once the
    /// user agrees to what is shown.
    pub(super) fn open_local_profile(&mut self, id: &ProfileId) -> Vec<Effect> {
        let Some(profile) = self.local_profiles().iter().find(|p| &p.id == id).cloned() else {
            return Vec::new();
        };
        let Ok(program_path) = local::program_path(profile.command.program.as_deref()) else {
            // Nothing can run: the tab says why, as starting it would.
            let effects = self.open_local(shell(&profile.name, &profile.command, None));
            self.reopened_by(Reopen::Profile(profile.id));
            return effects;
        };
        if profile.may_run(&program_path) {
            let effects =
                self.open_local(shell(&profile.name, &profile.command, Some(&program_path)));
            self.reopened_by(Reopen::Profile(profile.id));
            return effects;
        }
        let arguments = term_arguments(&profile.command.arguments);
        self.dialog = Some(Dialog::ConfirmLocalCommand(Box::new(LocalConfirmation {
            id: profile.id.clone(),
            name: server_text(&profile.name),
            command: visible_text(&local::command_text(&program_path, &arguments)),
            folder: profile
                .command
                .working_directory
                .as_ref()
                .map(|folder| visible_text(&folder.to_string_lossy())),
            rereads: local::rereads_its_command_line(&program_path),
            approval: LocalApproval {
                command: profile.command,
                program_path,
            },
        })));
        Vec::new()
    }

    /// Records the approval, then runs what was approved.
    pub(super) fn confirm_local(&mut self, confirmation: LocalConfirmation) -> Vec<Effect> {
        let LocalConfirmation {
            id, approval, name, ..
        } = confirmation;
        let recorded = self
            .store
            .apply(|store| store.approve_local(&id, approval.clone()));
        match recorded {
            Ok(true) => {
                let effects = self.open_local(shell(
                    &name,
                    &approval.command,
                    Some(&approval.program_path),
                ));
                self.reopened_by(Reopen::Profile(id));
                effects
            }
            // Deleted meanwhile: nothing to approve, and nothing is run.
            Ok(false) => Vec::new(),
            Err(error) => {
                self.dialog = Some(Dialog::StoreError {
                    detail: error.to_string(),
                });
                Vec::new()
            }
        }
    }
}

/// What a tab runs for `command`: the program at `program_path` when it was found, so that
/// the file run is the one approved, not whatever the name finds by then.
fn shell(name: &str, command: &LocalCommand, program_path: Option<&Path>) -> LocalShell {
    LocalShell {
        name: name.to_owned(),
        program: program_path
            .map(|path| path.to_string_lossy().into_owned())
            .or_else(|| command.program.clone()),
        arguments: term_arguments(&command.arguments),
        working_directory: command.working_directory.clone(),
    }
}

fn term_arguments(arguments: &LocalArguments) -> TermArguments {
    match arguments {
        LocalArguments::List(args) => TermArguments::List(args.clone()),
        LocalArguments::WindowsLine(line) => TermArguments::WindowsLine(line.clone()),
    }
}
