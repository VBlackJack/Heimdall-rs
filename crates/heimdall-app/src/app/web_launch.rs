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

//! Web consoles opened in the system's browser: no tab opens, the status bar says how it
//! went.
//!
//! Behind an SSH gateway, a tunnel of this computer's loopback address is opened to the
//! console, on a port the system picks, as one of the "New tunnel" dialog: its route's
//! questions answered from what is saved, the gateway's unknown key asked about, its row on
//! the Tunnels page until it is closed there or the application quits, as the C# keeps the
//! gateway's proxy open while it serves a console. A console whose tunnel is open already
//! goes through it again, as the C# consoles behind one gateway share its tunnel.

use std::net::SocketAddr;

use heimdall_core::profile::{ProfileId, WebProfile};

use super::{App, Effect, Notice};
use crate::tunnel::{SYSTEM_PICKED_PORT, TunnelBrowse, TunnelSpec};
use crate::web_console::{self, WebRefusal};

impl App {
    /// Opens saved web console `id` in the system's browser, through a tunnel of its gateway
    /// when it names one.
    pub(super) fn open_web(&mut self, id: &ProfileId) -> Vec<Effect> {
        let Some(profile) = self
            .store
            .web_profiles()
            .iter()
            .find(|profile| &profile.id == id)
            .cloned()
        else {
            return Vec::new();
        };
        match profile.gateway.clone() {
            None => match web_console::direct_url(&profile) {
                Ok(url) => vec![Effect::OpenWebConsole {
                    name: profile.name,
                    url,
                    through: None,
                }],
                Err(refusal) => {
                    self.tell(Notice::WebRefused(refusal));
                    Vec::new()
                }
            },
            Some(gateway) => self.open_web_through(&profile, gateway),
        }
    }

    /// Opens `profile` through a tunnel of `gateway`: the one open to the console already,
    /// else a new one, the browser pointed at it once it listens.
    fn open_web_through(&mut self, profile: &WebProfile, gateway: ProfileId) -> Vec<Effect> {
        let (remote_host, remote_port) = match web_console::forward_target(profile) {
            Ok(target) => target,
            Err(refusal) => {
                self.tell(Notice::WebRefused(refusal));
                return Vec::new();
            }
        };
        let browse = web_console::browse_of(profile);
        let wanted = TunnelSpec {
            gateway,
            remote_host,
            remote_port,
            local_port: SYSTEM_PICKED_PORT,
            label: Some(profile.name.clone()),
            browse: Some(browse.clone()),
        };
        if let Some(local) = self.web_tunnel(&wanted).map(|tunnel| tunnel.local) {
            return self.browse_tunnel(&browse, local);
        }
        // A second double click while the tunnel is being opened, or its gateway's key asked
        // about: the browser opens once it listens, a second tunnel would only hold a second
        // port and ask the same question over the first.
        if self.web_tunnel_opening(&wanted) {
            return Vec::new();
        }
        self.open_tunnel(wanted)
    }

    /// Points the browser at the console `browse` describes, through the tunnel listening at
    /// `local`.
    pub(super) fn browse_tunnel(
        &mut self,
        browse: &TunnelBrowse,
        local: SocketAddr,
    ) -> Vec<Effect> {
        match web_console::tunnel_url(browse, local) {
            Ok(url) => vec![Effect::OpenWebConsole {
                name: browse.name.clone(),
                url,
                through: Some(local.port()),
            }],
            Err(refusal) => {
                self.tell(Notice::WebRefused(refusal));
                Vec::new()
            }
        }
    }

    /// The web console `name` was handed to the browser, through the tunnel on port
    /// `through` when it went through one, or why not.
    pub(super) fn web_opened(
        &mut self,
        name: String,
        through: Option<u16>,
        result: Result<(), WebRefusal>,
    ) -> Vec<Effect> {
        self.tell(match (result, through) {
            (Ok(()), None) => Notice::WebOpened(name),
            (Ok(()), Some(port)) => Notice::WebOpenedThrough { name, port },
            (Err(refusal), _) => Notice::WebRefused(refusal),
        });
        Vec::new()
    }
}
