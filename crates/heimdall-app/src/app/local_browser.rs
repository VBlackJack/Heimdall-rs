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

//! The file browser a local shell gets beside it, as the C# `LocalFileBrowserView`.
//!
//! The C# docks it beside every local shell it embeds, whatever the program, side by side
//! at an even share, with no setting; an elevated shell started in a window of its own has
//! none. It starts in the shell's folder when it is there, else in the home folder, and its
//! Home goes back to where it started. Here a setting, on by default, can leave it out, and
//! a browser the user closed is not docked again when the same shell starts again.
//!
//! The C# also types into the shell from it: `cd` into a folder for "Open in terminal", and
//! a script's path for "Run in shell", each followed by Enter. Heimdall-rs never types into
//! a shell already running, as `files_terminal` says: the shell may be running something,
//! which would take the line as its own input. "Open in terminal" opens a new tab instead,
//! the user's default shell started in the folder, as the sidebar's "Local shell" button
//! opens it: never the program or the profile of the shell beside it. "Run in shell" runs
//! the script in a new tab of its own too, by its interpreter, once agreed, as the module
//! `run_in_shell` says. Nothing the browser does reaches the shell.
//!
//! The other way round, the browser follows the shell: when the shell reports its working
//! folder (OSC 7, or OSC 9;9 on Windows), the browser goes there while its "cwd" toggle
//! is on, as the module `shell_directory` says. A shell that never reports leaves it
//! where it is.
//!
//! Opening a file is as [`crate::local_open`] says: a text file in the external editor, a
//! file that would run only once agreed, anything else with the system's default program.
//! The other entries of its menu the C# has, "Open With", "Open in Editor", "Copy",
//! "Paste" and "Properties", are as the module `local_menu` says.
//!
//! Here the browser is a Files tab of this computer's files alone, docked as the second
//! pane of the shell's tab once it started, the keyboard left on the shell. It is no
//! session: it connects to nothing, is never live, and nothing opens it again. A shell
//! already split has no room for it: a reconnect keeps the browser it had rather than
//! docking another.

use std::path::PathBuf;

use heimdall_term::local::LocalArguments;
use tokio_util::sync::CancellationToken;

use super::reconnect::Reopen;
use super::split::{Axis, MAX_PANES, Placement, SplitMessage};
use super::{App, Dialog, Effect, Phase, Tab, TabProfile};
use crate::driver::Purpose;
use crate::files::{EntryKind, FilesPane, LocalPane, ShellFollow, Side};
use crate::ids::{AttemptId, TabId};
use crate::local_driver::LocalShell;
use crate::local_open::{self, LocalOpening};
use crate::text::{server_text, visible_text};

impl Tab {
    /// Whether it is the file browser docked beside a local shell, or once beside one:
    /// this computer's files alone, no session.
    #[must_use]
    pub fn is_local_browser(&self) -> bool {
        matches!(self.reopen, Reopen::LocalBrowser)
    }
}

impl App {
    /// Docks the file browser beside local shell `shell`, just started, when the shell has
    /// room: what lists its first folder.
    pub(super) fn dock_local_browser(&mut self, shell: TabId) -> Vec<Effect> {
        // A shell detached to a window of its own stays one tab there.
        if self.is_floating(shell) {
            return Vec::new();
        }
        // Left out by the settings, or closed by the user beside this shell.
        if !self.settings.sftp_browser.dock_local_browser {
            return Vec::new();
        }
        let Some(local) = self
            .tab(shell)
            .filter(|tab| tab.purpose == Purpose::Shell && !tab.local_browser_closed)
            .and_then(|tab| match &tab.profile {
                TabProfile::Local(local) => Some(local.clone()),
                _ => None,
            })
        else {
            return Vec::new();
        };
        // Split already, a reconnect among them, or docked in another tab: no room.
        if self.is_docked(shell) || self.panes_of(shell).len() >= MAX_PANES {
            return Vec::new();
        }
        // The shell's folder; one not there is left for the home folder once listed.
        let start = local
            .working_directory
            .clone()
            .unwrap_or_else(|| self.config.files_start.clone());
        let pane = TabId::fresh();
        let mut tab = Tab::new(
            self.terminal_palette(),
            pane,
            TabProfile::Local(local),
            Purpose::Files,
            self.viewport,
            AttemptId::fresh(),
            CancellationToken::new(),
        );
        // Nothing to connect: shown at once.
        tab.phase = Phase::Connected;
        tab.reopen = Reopen::LocalBrowser;
        let mut files = FilesPane::local_browser(start);
        // Its "cwd" toggle, seeded as an SFTP pane's is.
        files.follow = Some(ShellFollow::seeded(
            self.settings.sftp_browser.follow_local_directory,
        ));
        tab.files = Some(Box::new(files));
        let active = self.active;
        self.tabs.push(tab);
        let mut effects = self.list(pane, Side::Local);
        effects.extend(self.split_message(SplitMessage::Merge {
            host: shell,
            tab: pane,
            axis: Axis::SideBySide,
            placement: Placement::Second,
        }));
        // The keyboard stays on the shell, and on the tab shown when another one is.
        if let Some(layout) = self.tab_mut(shell).and_then(|tab| tab.layout.as_mut()) {
            layout.focus = shell;
        }
        self.active = active;
        effects
    }

    /// The first folder of local browser `tab` could not be listed: it starts in the home
    /// folder instead, as the C# browser does when the shell's folder is not there; whether
    /// it does. Once a folder was shown, a failure is shown as in any Files tab.
    pub(super) fn local_browser_start_failed(&mut self, tab: TabId) -> bool {
        let home = self.config.files_start.clone();
        let Some(files) = self
            .files_mut(tab)
            .filter(|files| files.local_only && files.local.home.is_none())
        else {
            return false;
        };
        if files.local.path == home || files.local.discard_listing {
            return false;
        }
        files.local.path = home;
        true
    }

    /// Opens, in the system's file manager, the local folder selected in `tab`, else the
    /// folder shown, as the C# "Open in Explorer" of the local file browser. A file
    /// selected opens the folder it is in.
    pub(super) fn open_in_explorer(&mut self, tab: TabId) -> Vec<Effect> {
        let Some(files) = self.files_mut(tab) else {
            return Vec::new();
        };
        let folder = chosen_folder(&files.local);
        vec![Effect::OpenFolder { tab, folder }]
    }

    /// "Open in terminal" in local file browser `tab`: a new tab of the user's default
    /// shell, as the sidebar's "Local shell" button opens it, started in the folder
    /// selected, else in the folder shown, and named after it. The shell beside the browser
    /// is left as it is: its program, its profile and what it runs are not taken, and
    /// nothing is typed into it.
    pub(super) fn open_local_terminal(&mut self, tab: TabId) -> Vec<Effect> {
        let Some(files) = self.files_mut(tab).filter(|files| files.local_only) else {
            return Vec::new();
        };
        let folder = chosen_folder(&files.local);
        let name = folder.file_name().map_or_else(
            || folder.to_string_lossy().into_owned(),
            |name| name.to_string_lossy().into_owned(),
        );
        self.open_local(LocalShell {
            name: server_text(&name),
            program: None,
            arguments: LocalArguments::default(),
            working_directory: Some(folder),
            environment: Vec::new(),
        })
    }

    /// Opens entry `index` of local file browser `tab`, a file or a link to one, as
    /// [`crate::local_open`] says: a text file in the external editor set, nothing
    /// watched; a file that would run once the user agreed to its full path; anything else
    /// with the system's default program. A link to a folder goes into it; a device, a pipe
    /// or a socket opens nothing.
    pub(super) fn open_local_file(&mut self, tab: TabId, index: usize) -> Vec<Effect> {
        let Some(files) = self.files_mut(tab) else {
            return Vec::new();
        };
        let Some(entry) = files.local.entries.get(index) else {
            return Vec::new();
        };
        let file = files.local.path.join(&entry.name);
        let name = entry.name.to_string_lossy().into_owned();
        let kind = entry.kind;
        if !matches!(kind, EntryKind::File | EntryKind::Link) {
            return Vec::new();
        }
        if kind == EntryKind::Link && file.is_dir() {
            files.local.leave();
            files.local.path = file;
            return self.list(tab, Side::Local);
        }
        files.local.select_only(Some(index));
        match local_open::opening(&name, local_open::runnable(&name, &file)) {
            LocalOpening::Edit => self.edit_local_file(tab, file),
            LocalOpening::Confirm => {
                self.dialog = Some(Dialog::ConfirmOpenRunnable {
                    tab,
                    shown: visible_text(&file.to_string_lossy()),
                    file,
                    chooser: false,
                });
                Vec::new()
            }
            LocalOpening::Open => vec![Effect::OpenLocalFile { tab, file }],
        }
    }

    /// Pane `pane` left the split of `host`, closed or taken out by the user: when it is
    /// the local file browser, `host` docks none again when its shell starts again.
    pub(super) fn local_browser_left(&mut self, host: TabId, pane: TabId) {
        if !self.tab(pane).is_some_and(Tab::is_local_browser) {
            return;
        }
        if let Some(shell) = self.tab_mut(host) {
            shell.local_browser_closed = true;
        }
    }
}

/// The folder selected in `local`, else the folder it shows.
fn chosen_folder(local: &LocalPane) -> PathBuf {
    local
        .selected
        .and_then(|index| local.entries.get(index))
        .filter(|entry| entry.kind == EntryKind::Directory)
        .map_or_else(|| local.path.clone(), |entry| local.path.join(&entry.name))
}
