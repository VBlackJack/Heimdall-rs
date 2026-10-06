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
//! Home goes back to where it started.
//!
//! The C# also types into the shell from it: `cd` into a folder for "Open in terminal", and
//! a script's path for "Run in shell", each followed by Enter. Heimdall-rs never types into
//! a shell already running, as `files_terminal` says: the shell may be running something,
//! which would take the line as its own input. Neither is offered, and nothing the browser
//! does reaches the shell.
//!
//! Here the browser is a Files tab of this computer's files alone, docked as the second
//! pane of the shell's tab once it started, the keyboard left on the shell. It is no
//! session: it connects to nothing, is never live, and nothing opens it again. A shell
//! already split has no room for it: a reconnect keeps the browser it had rather than
//! docking another.

use tokio_util::sync::CancellationToken;

use super::reconnect::Reopen;
use super::split::{Axis, MAX_PANES, Placement, SplitMessage};
use super::{App, Effect, Phase, Tab, TabProfile};
use crate::driver::Purpose;
use crate::files::{EntryKind, FilesPane, Side};
use crate::ids::{AttemptId, TabId};

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
        let Some(local) = self
            .tab(shell)
            .filter(|tab| tab.purpose == Purpose::Shell)
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
        tab.files = Some(Box::new(FilesPane::local_browser(start)));
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
        let local = &files.local;
        let folder = local
            .selected
            .and_then(|index| local.entries.get(index))
            .filter(|entry| entry.kind == EntryKind::Directory)
            .map_or_else(|| local.path.clone(), |entry| local.path.join(&entry.name));
        vec![Effect::OpenFolder { tab, folder }]
    }
}
