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

//! Telnet tabs: a terminal like an SSH shell, fed by a Telnet session.

use heimdall_core::profile::ProfileId;
use tokio_util::sync::CancellationToken;

use super::{App, Effect, Tab, TabProfile, terminal_size};
use crate::driver::Purpose;
use crate::ids::{AttemptId, TabId};
use crate::telnet_driver::TelnetRequest;

impl App {
    /// Opens a Telnet tab for a saved profile.
    pub(super) fn open_telnet(&mut self, id: &ProfileId) -> Vec<Effect> {
        let Some(profile) = self.telnet_profiles().iter().find(|p| &p.id == id).cloned() else {
            return Vec::new();
        };
        let grid = self.viewport;
        let tab_id = TabId::fresh();
        let attempt = AttemptId::fresh();
        let cancel = CancellationToken::new();
        let request = TelnetRequest {
            profile: profile.clone(),
            size: terminal_size(grid, None),
            cancel: cancel.clone(),
        };
        self.tabs.push(Tab::new(
            tab_id,
            TabProfile::Telnet(profile),
            Purpose::Shell,
            grid,
            attempt,
            cancel,
        ));
        self.active = Some(tab_id);
        vec![Effect::ConnectTelnet {
            tab: tab_id,
            attempt,
            request: Box::new(request),
        }]
    }
}
