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

//! RDP profiles opened in Remote Desktop Connection, as the C# external mode: those set to,
//! and those behind an RD Gateway, which the built-in client does not go through. No tab
//! opens; the status bar says how it went.

use heimdall_core::profile::RdpProfile;

use super::{App, Effect, Notice};
use crate::rdp_external::{self, ExternalRefusal};

impl App {
    /// Opens `profile`, its options already the session's, in Remote Desktop Connection when
    /// it is set to, or names an RD Gateway; `None` when it opens in a tab. One through an
    /// SSH gateway is refused: Remote Desktop Connection would go straight to the server.
    pub(super) fn open_rdp_external(&mut self, profile: &RdpProfile) -> Option<Vec<Effect>> {
        let gateway = profile.extras.rd_gateway();
        if !profile.extras.external && gateway.is_none() {
            return None;
        }
        if profile.gateway.is_some() {
            self.tell(Notice::RdpExternalRefused(ExternalRefusal::SshGateway));
            return Some(Vec::new());
        }
        Some(vec![Effect::LaunchRdpExternal {
            name: profile.name.clone(),
            // Said only when the gateway, not the profile, chose the external client.
            gateway: gateway
                .filter(|_| !profile.extras.external)
                .map(str::to_owned),
            content: rdp_external::rdp_file(profile),
        }])
    }

    /// The RDP profile `name` was opened in Remote Desktop Connection, or why not.
    pub(super) fn rdp_external_launched(
        &mut self,
        name: String,
        gateway: Option<String>,
        result: Result<(), ExternalRefusal>,
    ) -> Vec<Effect> {
        self.tell(match result {
            Ok(()) => Notice::RdpExternalLaunched { name, gateway },
            Err(refusal) => Notice::RdpExternalRefused(refusal),
        });
        Vec::new()
    }
}
