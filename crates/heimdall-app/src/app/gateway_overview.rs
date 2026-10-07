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

//! The SSH gateways, as the C# Gateways list and its Overview show them together: each
//! gateway with the sessions that go through it, and the references to a gateway that is
//! not configured, to be reassigned or cleared. A gateway is deleted here, as the C# one,
//! its references cleared.

use std::collections::BTreeMap;

use heimdall_core::profile::ProfileId;

use super::{App, Dialog, Notice, ProfileKind};

/// A session going through a gateway.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoutedSession {
    /// The profile.
    pub id: ProfileId,
    /// Its name.
    pub name: String,
    /// Its protocol.
    pub kind: ProfileKind,
}

/// A configured gateway and what goes through it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GatewayEntry {
    /// The gateway.
    pub id: ProfileId,
    /// Its name.
    pub name: String,
    /// Its host and port.
    pub host: String,
    /// Its port.
    pub port: u16,
    /// The gateway it is reached through, by name, or by identifier when that one is not
    /// configured.
    pub parent: Option<String>,
    /// The sessions going through it, by name.
    pub sessions: Vec<RoutedSession>,
}

/// References to a gateway that is not configured.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissingGateway {
    /// The identifier referred to.
    pub id: ProfileId,
    /// The sessions going through it, by name.
    pub sessions: Vec<RoutedSession>,
    /// The gateways reached through it, by name.
    pub gateways: Vec<(ProfileId, String)>,
}

/// The gateways, and the references to gateways that are not configured.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GatewayOverview {
    /// The configured gateways, in the order they were added.
    pub gateways: Vec<GatewayEntry>,
    /// The references to gateways that are not configured, by identifier.
    pub missing: Vec<MissingGateway>,
}

impl GatewayOverview {
    /// The sessions going through a gateway, configured or not.
    #[must_use]
    pub fn routed(&self) -> usize {
        self.gateways
            .iter()
            .map(|gateway| gateway.sessions.len())
            .sum::<usize>()
            + self
                .missing
                .iter()
                .map(|missing| missing.sessions.len())
                .sum::<usize>()
    }

    /// The references to gateways that are not configured.
    #[must_use]
    pub fn unresolved(&self) -> usize {
        self.missing
            .iter()
            .map(|missing| missing.sessions.len() + missing.gateways.len())
            .sum()
    }
}

/// What the Gateways tab does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GatewaysMessage {
    /// Delete this gateway, asked first with what it would clear.
    AskDelete(ProfileId),
    /// The sessions going through the missing gateway `missing` go through `to` instead.
    Reassign {
        /// The identifier of the missing gateway.
        missing: ProfileId,
        /// The configured gateway to go through.
        to: ProfileId,
    },
    /// The sessions going through the missing gateway connect directly.
    Clear(ProfileId),
}

impl App {
    /// The gateways and what goes through each, as the C# Gateway Overview.
    #[must_use]
    pub fn gateway_overview(&self) -> GatewayOverview {
        let gateways = self.store.gateways();
        let known = |id: &ProfileId| gateways.iter().any(|gateway| gateway.id == *id);
        let mut by_gateway: BTreeMap<ProfileId, Vec<RoutedSession>> = BTreeMap::new();
        let mut routes = Vec::new();
        for profile in self.store.ssh_profiles() {
            let kind = if profile.sftp {
                ProfileKind::Sftp
            } else {
                ProfileKind::Ssh
            };
            routes.push((&profile.gateway, &profile.id, &profile.name, kind));
        }
        for profile in self.store.rdp_profiles() {
            routes.push((
                &profile.gateway,
                &profile.id,
                &profile.name,
                ProfileKind::Rdp,
            ));
        }
        for profile in self.store.winrm_profiles() {
            routes.push((
                &profile.gateway,
                &profile.id,
                &profile.name,
                ProfileKind::WinRm,
            ));
        }
        for (gateway, id, name, kind) in routes {
            if let Some(gateway) = gateway {
                by_gateway
                    .entry(gateway.clone())
                    .or_default()
                    .push(RoutedSession {
                        id: id.clone(),
                        name: name.clone(),
                        kind,
                    });
            }
        }
        for sessions in by_gateway.values_mut() {
            sessions.sort_by_key(|session| session.name.to_lowercase());
        }
        let entries = gateways
            .iter()
            .map(|gateway| GatewayEntry {
                id: gateway.id.clone(),
                name: gateway.name.clone(),
                host: gateway.host.clone(),
                port: gateway.port,
                parent: gateway.parent.as_ref().map(|parent| {
                    gateways
                        .iter()
                        .find(|known| known.id == *parent)
                        .map_or_else(|| parent.to_string(), |known| known.name.clone())
                }),
                sessions: by_gateway.remove(&gateway.id).unwrap_or_default(),
            })
            .collect();
        let mut missing: BTreeMap<ProfileId, MissingGateway> = by_gateway
            .into_iter()
            .map(|(id, sessions)| {
                (
                    id.clone(),
                    MissingGateway {
                        id,
                        sessions,
                        gateways: Vec::new(),
                    },
                )
            })
            .collect();
        for gateway in gateways {
            if let Some(parent) = gateway.parent.as_ref().filter(|parent| !known(parent)) {
                missing
                    .entry(parent.clone())
                    .or_insert_with(|| MissingGateway {
                        id: parent.clone(),
                        sessions: Vec::new(),
                        gateways: Vec::new(),
                    })
                    .gateways
                    .push((gateway.id.clone(), gateway.name.clone()));
            }
        }
        GatewayOverview {
            gateways: entries,
            missing: missing.into_values().collect(),
        }
    }

    /// A step of the Gateways tab.
    pub(super) fn gateways_message(&mut self, message: GatewaysMessage) {
        match message {
            GatewaysMessage::AskDelete(id) => {
                let overview = self.gateway_overview();
                let Some(entry) = overview.gateways.iter().find(|entry| entry.id == id) else {
                    return;
                };
                let gateways = self
                    .store
                    .gateways()
                    .iter()
                    .filter(|gateway| gateway.parent.as_ref() == Some(&id))
                    .count();
                self.dialog = Some(Dialog::ConfirmDeleteGateway {
                    id: id.clone(),
                    name: entry.name.clone(),
                    servers: entry.sessions.len(),
                    gateways,
                });
            }
            GatewaysMessage::Reassign { missing, to } => self.reroute(&missing, Some(&to)),
            GatewaysMessage::Clear(missing) => self.reroute(&missing, None),
        }
    }

    /// The gateway `id` deleted, as the user agreed to, its references cleared and its
    /// saved password and passphrase forgotten.
    pub(super) fn confirm_delete_gateway(&mut self, id: &ProfileId, name: &str) {
        match self.store.apply(|store| store.remove_gateway(id)) {
            Ok(Some(_)) => {
                self.forget_password(id);
                self.tell(Notice::GatewayDeleted(name.to_owned()));
            }
            Ok(None) => {}
            Err(error) => {
                self.dialog = Some(Dialog::save_failed(&error));
            }
        }
    }

    /// The sessions going through the missing gateway `missing` sent through `to`, or
    /// directly.
    fn reroute(&mut self, missing: &ProfileId, to: Option<&ProfileId>) {
        let Some(sessions) = self
            .gateway_overview()
            .missing
            .into_iter()
            .find(|entry| entry.id == *missing)
            .map(|entry| entry.sessions)
        else {
            return;
        };
        let changed = self.store.apply(|store| {
            sessions
                .iter()
                .filter(|session| store.set_gateway(&session.id, to))
                .count()
        });
        match changed {
            Ok(0) => self.tell(Notice::GatewaysUnchanged),
            Ok(count) if to.is_some() => self.tell(Notice::GatewaysReassigned(count)),
            Ok(count) => self.tell(Notice::GatewaysCleared(count)),
            Err(error) => {
                self.dialog = Some(Dialog::save_failed(&error));
            }
        }
    }
}
