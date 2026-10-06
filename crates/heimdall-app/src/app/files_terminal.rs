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

//! "Open in terminal" in a Files tab, as the C# SFTP view's: a shell on the same server,
//! in the folder chosen.
//!
//! The C# types `cd` into the SSH pane beside the Files pane in its split tab. Heimdall-rs
//! never types into a shell already running, docked beside the Files pane or not: it may be
//! running something. It opens a shell tab of the same profile, whose `cd` is one more step
//! after the profile's approved ones. Steps the user has not approved are never typed.

use heimdall_core::post_connect::{PostConnect, PostConnectStep};

use super::{App, Effect, TabProfile};
use crate::driver::Purpose;
use crate::files::EntryKind;
use crate::ids::TabId;

impl App {
    /// Opens a shell on `tab_id`'s server in its selected folder, else in the folder shown.
    pub(super) fn open_in_terminal(&mut self, tab_id: TabId) -> Vec<Effect> {
        let Some(tab) = self.tab(tab_id) else {
            return Vec::new();
        };
        let (TabProfile::Ssh(profile), Some(files)) = (&tab.profile, tab.files.as_deref()) else {
            return Vec::new();
        };
        let remote = &files.remote;
        let folder = remote
            .selected
            .and_then(|index| remote.entries.get(index))
            .filter(|entry| entry.kind == EntryKind::Directory)
            .map_or_else(
                || remote.path.clone(),
                |entry| remote.path.join(&entry.name),
            );
        let Some(command) = cd_command(folder.as_bytes()) else {
            return Vec::new();
        };
        let mut profile = profile.clone();
        // A shell, whatever the profile opens by default.
        profile.sftp = false;
        let mut steps = profile.post_connect.to_run();
        steps.push(PostConnectStep::new(command));
        profile.post_connect = PostConnect::approved_as(steps);
        self.open_ssh_now(profile, Purpose::Shell)
    }
}

/// `cd` into `path`, quoted for a POSIX shell; `None` for a path a shell line cannot carry
/// as it is: not UTF-8, or holding a control character.
fn cd_command(path: &[u8]) -> Option<String> {
    let path = std::str::from_utf8(path).ok()?;
    if path.is_empty() || path.chars().any(char::is_control) {
        return None;
    }
    Some(format!("cd -- '{}'", path.replace('\'', "'\\''")))
}
