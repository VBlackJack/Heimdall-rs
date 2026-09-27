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

use tokio_util::sync::CancellationToken;

use super::{App, Effect, Tab, TabProfile, terminal_size};
use crate::driver::Purpose;
use crate::ids::{AttemptId, TabId};
use crate::local_driver::{LocalRequest, LocalShell};

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
            cancel: cancel.clone(),
        };
        self.tabs.push(Tab::new(
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
}
