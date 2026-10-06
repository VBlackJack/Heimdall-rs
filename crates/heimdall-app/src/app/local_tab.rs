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

use heimdall_core::profile::{
    LocalApproval, LocalArguments, LocalCommand, LocalProfile, ProfileId,
};
use heimdall_core::settings::ExecutionPolicy;
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
    pub(super) fn open_local(&mut self, shell: LocalShell) -> Vec<Effect> {
        let shell = powershell_options(shell, self.settings.powershell_execution_policy);
        self.open_local_built(shell)
    }

    /// Opens a tab running `shell` with the arguments it has, `PowerShell`'s options added
    /// already: what a question showed whole, run as shown.
    pub(super) fn open_local_built(&mut self, mut shell: LocalShell) -> Vec<Effect> {
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
            let effects = self.open_local(self.profile_shell(&profile, &profile.command, None));
            self.reopened_by(Reopen::Profile(profile.id));
            return effects;
        };
        if profile.may_run(&program_path) {
            let effects = self.open_local(self.profile_shell(
                &profile,
                &profile.command,
                Some(&program_path),
            ));
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
        let LocalConfirmation { id, approval, .. } = confirmation;
        let recorded = self
            .store
            .apply(|store| store.approve_local(&id, approval.clone()));
        match recorded {
            Ok(true) => {
                let Some(profile) = self.local_profiles().iter().find(|p| p.id == id).cloned()
                else {
                    return Vec::new();
                };
                let effects = self.open_local(self.profile_shell(
                    &profile,
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

impl App {
    /// What a tab runs for `profile`'s `command`, with its environment's name among the
    /// variables it is given, as the C# `HEIMDALL_ENV`.
    fn profile_shell(
        &self,
        profile: &LocalProfile,
        command: &LocalCommand,
        program_path: Option<&Path>,
    ) -> LocalShell {
        let mut shell = shell(profile, command, program_path);
        if let Some(environment) = self
            .store
            .metadata(&profile.id)
            .and_then(|metadata| metadata.environment)
        {
            shell
                .environment
                .push((CONTEXT_ENV.to_owned(), environment.name().to_owned()));
        }
        shell
    }
}

/// `shell` as the C# starts a local `PowerShell`: `-ExecutionPolicy` when one is chosen,
/// and `-NoLogo` unless its arguments already ask it. Any other program is left as it is; on
/// Windows, no program at all is `PowerShell`.
pub(super) fn powershell_options(mut shell: LocalShell, policy: ExecutionPolicy) -> LocalShell {
    let is_powershell = match shell.program.as_deref() {
        // Its last part, whichever separator a Windows or Unix path uses, without ".exe".
        Some(program) => {
            let name = program.rsplit(['/', '\\']).next().unwrap_or(program);
            let stem = name
                .len()
                .checked_sub(EXE_SUFFIX.len())
                .filter(|at| name.is_char_boundary(*at))
                .filter(|at| name[*at..].eq_ignore_ascii_case(EXE_SUFFIX))
                .map_or(name, |at| &name[..at]);
            POWERSHELL_PROGRAMS
                .iter()
                .any(|known| stem.eq_ignore_ascii_case(known))
        }
        None => cfg!(windows),
    };
    if !is_powershell {
        return shell;
    }
    let mut prefix: Vec<String> = Vec::new();
    if policy != ExecutionPolicy::Default {
        prefix.extend([EXECUTION_POLICY_FLAG.to_owned(), policy.name().to_owned()]);
    }
    let has_no_logo = match &shell.arguments {
        TermArguments::List(arguments) => arguments
            .iter()
            .any(|argument| argument.eq_ignore_ascii_case(NO_LOGO_FLAG)),
        TermArguments::WindowsLine(line) => line
            .to_ascii_lowercase()
            .contains(&NO_LOGO_FLAG.to_ascii_lowercase()),
    };
    if !has_no_logo {
        prefix.push(NO_LOGO_FLAG.to_owned());
    }
    if prefix.is_empty() {
        return shell;
    }
    shell.arguments = match shell.arguments {
        TermArguments::List(arguments) => {
            TermArguments::List(prefix.into_iter().chain(arguments).collect())
        }
        TermArguments::WindowsLine(line) if line.trim().is_empty() => {
            TermArguments::WindowsLine(prefix.join(" "))
        }
        TermArguments::WindowsLine(line) => {
            TermArguments::WindowsLine(format!("{} {line}", prefix.join(" ")))
        }
    };
    shell
}

/// The programs taken for `PowerShell`, by their name without extension, as the C#
/// `IsPowerShellExecutable`.
const POWERSHELL_PROGRAMS: [&str; 2] = ["powershell", "pwsh"];
/// The extension of a Windows program, left out of its name.
const EXE_SUFFIX: &str = ".exe";
/// `PowerShell`'s flag naming the execution policy.
const EXECUTION_POLICY_FLAG: &str = "-ExecutionPolicy";
/// `PowerShell`'s flag leaving its banner out.
const NO_LOGO_FLAG: &str = "-NoLogo";

/// What a tab runs for `profile`'s `command`: the program at `program_path` when it was
/// found, so that the file run is the one approved, not whatever the name finds by then.
fn shell(
    profile: &LocalProfile,
    command: &LocalCommand,
    program_path: Option<&Path>,
) -> LocalShell {
    LocalShell {
        name: profile.name.clone(),
        program: program_path
            .map(|path| path.to_string_lossy().into_owned())
            .or_else(|| command.program.clone()),
        arguments: term_arguments(&command.arguments),
        working_directory: command.working_directory.clone(),
        environment: context_environment(profile),
    }
}

/// The variables a saved profile's shell is given to know what it was opened for, as the C#
/// `BuildContextEnvironment`: its name, its type and its folder, each only when it has one.
/// A local profile has no server, so the C# host, port and user are never set here.
fn context_environment(profile: &LocalProfile) -> Vec<(String, String)> {
    [
        (CONTEXT_NAME, Some(profile.name.as_str())),
        (CONTEXT_TYPE, Some(CONTEXT_TYPE_LOCAL)),
        (CONTEXT_GROUP, profile.group.as_deref()),
    ]
    .into_iter()
    .filter_map(|(variable, value)| {
        let value = value?.trim();
        (!value.is_empty()).then(|| (variable.to_owned(), value.to_owned()))
    })
    .collect()
}

/// The profile's name, as the C# `HEIMDALL_NAME`.
const CONTEXT_NAME: &str = "HEIMDALL_NAME";
/// Its type, as the C# `HEIMDALL_TYPE`.
const CONTEXT_TYPE: &str = "HEIMDALL_TYPE";
/// Its folder, as the C# `HEIMDALL_GROUP`.
const CONTEXT_GROUP: &str = "HEIMDALL_GROUP";
/// Its environment, as the C# `HEIMDALL_ENV`.
const CONTEXT_ENV: &str = "HEIMDALL_ENV";
/// The type a local profile is said to be, the C# connection type's.
const CONTEXT_TYPE_LOCAL: &str = "Local";

pub(super) fn term_arguments(arguments: &LocalArguments) -> TermArguments {
    match arguments {
        LocalArguments::List(args) => TermArguments::List(args.clone()),
        LocalArguments::WindowsLine(line) => TermArguments::WindowsLine(line.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn local(program: Option<&str>, arguments: TermArguments) -> LocalShell {
        LocalShell {
            name: "shell".to_owned(),
            program: program.map(str::to_owned),
            arguments,
            working_directory: None,
            environment: Vec::new(),
        }
    }

    fn list(arguments: &[&str]) -> TermArguments {
        TermArguments::List(
            arguments
                .iter()
                .map(|argument| (*argument).to_owned())
                .collect(),
        )
    }

    #[test]
    fn a_powershell_starts_without_its_banner_and_with_the_policy_chosen_as_the_csharp() {
        let started = |program, arguments, policy| {
            powershell_options(local(program, arguments), policy).arguments
        };
        assert_eq!(
            started(
                Some(r"C:\Tools\pwsh.exe"),
                list(&["-NoExit"]),
                ExecutionPolicy::Default
            ),
            list(&["-NoLogo", "-NoExit"])
        );
        assert_eq!(
            started(Some("powershell"), list(&[]), ExecutionPolicy::Bypass),
            list(&["-ExecutionPolicy", "Bypass", "-NoLogo"])
        );
        assert_eq!(
            started(
                Some("PWSH.EXE"),
                list(&["-nologo"]),
                ExecutionPolicy::Default
            ),
            list(&["-nologo"]),
            "asked already: not twice"
        );
        assert_eq!(
            started(
                Some("powershell.exe"),
                TermArguments::WindowsLine("-Command Get-Date".to_owned()),
                ExecutionPolicy::RemoteSigned
            ),
            TermArguments::WindowsLine(
                "-ExecutionPolicy RemoteSigned -NoLogo -Command Get-Date".to_owned()
            )
        );
        assert_eq!(
            started(Some("cmd.exe"), list(&["/k"]), ExecutionPolicy::Bypass),
            list(&["/k"]),
            "another program is left as it is"
        );
        // No program: PowerShell on Windows, the user's shell elsewhere.
        let default = started(None, list(&[]), ExecutionPolicy::Default);
        if cfg!(windows) {
            assert_eq!(default, list(&["-NoLogo"]));
        } else {
            assert_eq!(default, list(&[]));
        }
    }
}
