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

//! The tree's filters, as the C# sidebar's filter button offers them: protocols, connected,
//! through a gateway; every one chosen must hold, as the C# `ServerFilterSpec` combines them.
//! Showing the gateway badge is a view choice beside them, not a filter.

use super::tree::{ProfileKind, ProfileSummary};
use super::{App, SessionState};

/// Which profiles the tree lists, beyond its search.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeFilter {
    /// Only these protocols; none chosen is every one.
    protocols: Vec<ProfileKind>,
    /// Only profiles with a session connected.
    connected: bool,
    /// Only profiles that go through an SSH gateway.
    gateway: bool,
    /// Rows show the gateway a profile goes through: on, as in the C#.
    show_gateway_badge: bool,
}

impl Default for TreeFilter {
    fn default() -> Self {
        Self {
            protocols: Vec::new(),
            connected: false,
            gateway: false,
            show_gateway_badge: true,
        }
    }
}

/// A change to the tree's filters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterMessage {
    /// This protocol chosen, or no longer.
    Protocol(ProfileKind),
    /// Connected only, or no longer.
    Connected,
    /// Through a gateway only, or no longer.
    Gateway,
    /// The gateway badge shown, or hidden.
    GatewayBadge,
    /// Every filter off, as the C# "Reset filters"; the badge choice stays.
    Reset,
}

impl TreeFilter {
    /// Whether this protocol is chosen.
    #[must_use]
    pub fn has_protocol(&self, kind: ProfileKind) -> bool {
        self.protocols.contains(&kind)
    }

    /// Whether only connected profiles are listed.
    #[must_use]
    pub fn connected(&self) -> bool {
        self.connected
    }

    /// Whether only profiles through a gateway are listed.
    #[must_use]
    pub fn gateway(&self) -> bool {
        self.gateway
    }

    /// Whether rows show their gateway.
    #[must_use]
    pub fn shows_gateway_badge(&self) -> bool {
        self.show_gateway_badge
    }

    /// Whether any filter leaves profiles out: the C# button's dot.
    #[must_use]
    pub fn is_active(&self) -> bool {
        !self.protocols.is_empty() || self.connected || self.gateway
    }

    /// Whether `profile`, its sessions in `state`, passes every filter chosen.
    pub(super) fn accepts(&self, profile: &ProfileSummary, state: Option<SessionState>) -> bool {
        (self.protocols.is_empty() || self.protocols.contains(&profile.kind))
            && (!self.connected || state == Some(SessionState::Connected))
            && (!self.gateway || profile.gateway.is_some())
    }

    fn apply(&mut self, message: FilterMessage) {
        match message {
            FilterMessage::Protocol(kind) => {
                if let Some(index) = self.protocols.iter().position(|chosen| *chosen == kind) {
                    self.protocols.remove(index);
                } else {
                    self.protocols.push(kind);
                }
            }
            FilterMessage::Connected => self.connected = !self.connected,
            FilterMessage::Gateway => self.gateway = !self.gateway,
            FilterMessage::GatewayBadge => self.show_gateway_badge = !self.show_gateway_badge,
            FilterMessage::Reset => {
                *self = Self {
                    show_gateway_badge: self.show_gateway_badge,
                    ..Self::default()
                };
            }
        }
    }
}

impl App {
    /// The tree's filters.
    #[must_use]
    pub fn tree_filter(&self) -> &TreeFilter {
        &self.tree_filter
    }

    pub(super) fn filter_message(&mut self, message: FilterMessage) {
        self.tree_filter.apply(message);
    }
}
