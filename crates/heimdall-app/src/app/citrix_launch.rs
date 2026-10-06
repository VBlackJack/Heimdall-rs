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

//! Citrix profiles: their application launched outside Heimdall, as the C# `CitrixHandler`
//! launches it; no tab opens, the status bar says how it went.

use heimdall_core::profile::ProfileId;

use super::{App, Effect, Notice};
use crate::citrix::{self, CitrixRefusal};

impl App {
    /// Launches saved Citrix profile `id`'s application: checked here, started off the UI
    /// thread; refused, the status bar says why, as the C# error does.
    pub(super) fn open_citrix(&mut self, id: &ProfileId) -> Vec<Effect> {
        let Some(profile) = self
            .store
            .citrix_profiles()
            .iter()
            .find(|p| &p.id == id)
            .cloned()
        else {
            return Vec::new();
        };
        match citrix::plan(&profile) {
            Ok(launch) => {
                self.tell(Notice::CitrixLaunching);
                vec![Effect::LaunchCitrix {
                    name: profile.name,
                    launch,
                }]
            }
            Err(refusal) => {
                self.tell(Notice::CitrixRefused(refusal));
                Vec::new()
            }
        }
    }

    /// The Citrix application `name` was launched, or why not.
    pub(super) fn citrix_launched(
        &mut self,
        name: String,
        result: Result<(), CitrixRefusal>,
    ) -> Vec<Effect> {
        self.tell(match result {
            Ok(()) => Notice::CitrixLaunched(name),
            Err(refusal) => Notice::CitrixRefused(refusal),
        });
        Vec::new()
    }
}
