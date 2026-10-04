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

//! A shell tab's server health panel, as the C# SSH view's: shown on the user's word, the
//! server asked every 15 seconds while it is.

use super::{App, Effect};
use crate::ids::TabId;
use crate::server_health::ServerHealth;

impl App {
    /// Whether a server is asked how it is: a health panel is shown.
    #[must_use]
    pub fn polls_health(&self) -> bool {
        self.tabs.iter().any(|tab| tab.health.shown)
    }

    /// Shows or hides `tab`'s server health panel; shown, the server is asked at once.
    pub(super) fn toggle_health(&mut self, tab: TabId) -> Vec<Effect> {
        let Some(found) = self.tab_mut(tab) else {
            return Vec::new();
        };
        if !found.health.shown && !found.health.available() {
            return Vec::new();
        }
        found.health.shown = !found.health.shown;
        found
            .health
            .ask()
            .map(|connection| Effect::ReadHealth { tab, connection })
            .into_iter()
            .collect()
    }

    /// A message of the server health panels.
    pub(super) fn health_message(&mut self, message: super::Message) -> Vec<Effect> {
        match message {
            super::Message::HealthTick => self.health_tick(),
            super::Message::HealthRead { tab, health } => {
                self.health_read(tab, *health);
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    /// An SSH shell's connection: its health asked over it, at once when its panel was
    /// shown before a reconnection.
    pub(super) fn shell_connection(
        &mut self,
        tab: TabId,
        connection: heimdall_ssh::WeakConnection,
    ) -> Vec<Effect> {
        let Some(found) = self.tab_mut(tab) else {
            return Vec::new();
        };
        found.health.connection = Some(connection);
        found.health.asking = false;
        found
            .health
            .ask()
            .map(|connection| Effect::ReadHealth { tab, connection })
            .into_iter()
            .collect()
    }

    /// Time to ask again the servers whose panel is shown.
    fn health_tick(&mut self) -> Vec<Effect> {
        self.tabs
            .iter_mut()
            .filter_map(|found| {
                found.health.ask().map(|connection| Effect::ReadHealth {
                    tab: found.id,
                    connection,
                })
            })
            .collect()
    }

    /// A server said how it is: shown while its panel is.
    fn health_read(&mut self, tab: TabId, health: ServerHealth) {
        if let Some(found) = self.tab_mut(tab) {
            found.health.asking = false;
            if found.health.shown {
                found.health.last = Some(health);
            }
        }
    }
}
