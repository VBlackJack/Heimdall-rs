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

//! "Run in Shell" in the local file browser: a script run by its interpreter in a new tab,
//! as [`crate::script_shell`] says, once the user agreed to the command shown whole.
//!
//! The C# types the script's path into the shell beside the browser, at once. Here nothing
//! is typed into any shell, and nothing runs before the question is answered. Agreeing
//! records nothing: Reconnect asks again, showing the same command, and the script then
//! takes the tab's place; Duplicate asks too. A script's tab is left out of the sessions
//! offered again at the next start, which reopen saved profiles alone.
//!
//! The new tab is a local shell as any other: named after the script, started in its
//! folder, given no `HEIMDALL_*` variable, and docked a file browser of its own.

use std::path::{Path, PathBuf};

use heimdall_core::settings::ExecutionPolicy;
use heimdall_term::local;

use super::local_tab::powershell_options;
use super::reconnect::Reopen;
use super::{App, Dialog, Effect};
use crate::files::{EntryKind, FilesError};
use crate::ids::TabId;
use crate::local_driver::LocalShell;
use crate::script_shell::{self, ScriptKind, ScriptRefusal};
use crate::text::{server_text, visible_text};

/// What the user is asked before a script runs from the local file browser. Unlike
/// [`super::LocalConfirmation`], agreeing records nothing: each run is asked about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptConfirmation {
    /// The script's name, made safe: the new tab's.
    pub name: String,
    /// What runs once agreed: this, not the script looked at again.
    pub shell: LocalShell,
    /// The command as it runs, every invisible character written out.
    pub command: String,
    /// The folder it starts in, the script's, every invisible character written out.
    pub folder: Option<String>,
    /// The interpreter reads its command line again with rules of its own (`cmd.exe`):
    /// what the arguments look like is not what it does.
    pub rereads: bool,
    /// The tab whose Reconnect asked, whose place the script takes once agreed; `None` for
    /// a new tab.
    pub replaces: Option<TabId>,
}

impl App {
    /// Whether entry `index` of `tab` is offered "Run in Shell", as the C# offers it: a
    /// regular file of the local file browser, chosen alone, here a script this platform
    /// runs.
    #[must_use]
    pub fn offers_run_in_shell(&self, tab: TabId, index: usize) -> bool {
        let Some(local) = self
            .tab(tab)
            .and_then(|tab| tab.files.as_deref())
            .filter(|files| files.local_only)
            .map(|files| &files.local)
        else {
            return false;
        };
        let script = local.entries.get(index).is_some_and(|entry| {
            entry.kind == EntryKind::File
                && script_shell::runnable_here(&entry.name.to_string_lossy()).is_some()
        });
        // Several chosen: none, as the C# offers it for one alone.
        script && local.chosen().len() <= 1
    }

    /// "Run in Shell" on entry `index` of local file browser `tab`: the command its
    /// interpreter would run is shown first, and nothing runs before it is agreed to. A
    /// path that command cannot carry as it is is said on the pane, and nothing is run.
    pub(super) fn run_in_shell(&mut self, tab: TabId, index: usize) -> Vec<Effect> {
        if !self.offers_run_in_shell(tab, index) {
            return Vec::new();
        }
        let policy = self.settings.powershell_execution_policy;
        let Some(files) = self.files_mut(tab) else {
            return Vec::new();
        };
        let Some(entry) = files.local.entries.get(index) else {
            return Vec::new();
        };
        let name = entry.name.to_string_lossy().into_owned();
        let script = files.local.path.join(&entry.name);
        let Some(kind) = script_shell::runnable_here(&name) else {
            return Vec::new();
        };
        let folder = files.local.path.clone();
        match script_run(kind, &script, folder, &name, policy) {
            Ok(shell) => self.ask_script(shell, None),
            Err(error) => files.local.error = Some(error),
        }
        Vec::new()
    }

    /// Asks whether to run `shell`, a script's interpreter, showing its command whole; in
    /// the place of tab `replaces` once agreed, when Reconnect asks.
    pub(super) fn ask_script(&mut self, shell: LocalShell, replaces: Option<TabId>) {
        let Some(program) = shell.program.as_deref().map(PathBuf::from) else {
            return;
        };
        self.dialog = Some(Dialog::ConfirmRunScript(Box::new(ScriptConfirmation {
            name: shell.name.clone(),
            command: visible_text(&local::command_text(&program, &shell.arguments)),
            folder: shell
                .working_directory
                .as_ref()
                .map(|folder| visible_text(&folder.to_string_lossy())),
            rereads: local::rereads_its_command_line(&program),
            shell,
            replaces,
        })));
    }

    /// Runs the script agreed to, as shown: in a new tab, or in the place of the tab whose
    /// Reconnect asked while that tab is still there. Nothing is recorded.
    pub(super) fn confirm_script(&mut self, confirmation: ScriptConfirmation) -> Vec<Effect> {
        let ScriptConfirmation {
            shell, replaces, ..
        } = confirmation;
        let index = replaces.and_then(|old| {
            self.tabs
                .iter()
                .position(|tab| tab.id == old && self.can_restart(tab))
        });
        let keyboard = index.map(|index| self.keyboard_kept(self.tabs[index].id));
        let count = self.tabs.len();
        let effects = self.open_local_built(shell.clone());
        self.reopened_by(Reopen::Script(Box::new(shell)));
        if let (Some(index), Some(keyboard)) = (index, keyboard) {
            self.take_place(index, count, keyboard);
        }
        effects
    }
}

/// The local shell running a `kind` script at `script`, named `name`, in `folder`: its
/// interpreter by its full path, `PowerShell`'s options of `policy` added as to any local
/// `PowerShell`, and no variable of its own.
fn script_run(
    kind: ScriptKind,
    script: &Path,
    folder: PathBuf,
    name: &str,
    policy: ExecutionPolicy,
) -> Result<LocalShell, FilesError> {
    let arguments = script_shell::arguments(kind, script).map_err(|refusal| match refusal {
        ScriptRefusal::Character(character) => FilesError::ScriptPathCharacter {
            character: visible_text(&character.to_string()),
        },
        ScriptRefusal::NotText => FilesError::ScriptPathNotText,
    })?;
    let program = script_shell::interpreter(kind).map_err(|error| FilesError::OpenFailed {
        detail: error.to_string(),
    })?;
    Ok(powershell_options(
        LocalShell {
            name: server_text(name),
            program: Some(program.to_string_lossy().into_owned()),
            arguments,
            working_directory: Some(folder),
            environment: Vec::new(),
        },
        policy,
    ))
}
