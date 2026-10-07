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
//! through a gateway, favorites; every one chosen must hold, as the C# `ServerFilterSpec` combines them.
//! Showing the gateway badge is a view choice beside them, not a filter: kept in the
//! settings across runs, as the C# `ShowGatewayBadge`, where the filters are not.

use super::tree::{ProfileKind, ProfileSummary};
use super::{App, Dialog, SessionState};

/// Which profiles the tree lists, beyond its search.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TreeFilter {
    /// Only these protocols; none chosen is every one.
    protocols: Vec<ProfileKind>,
    /// Only profiles with a session connected.
    connected: bool,
    /// Only profiles that go through an SSH gateway.
    gateway: bool,
    /// Only profiles marked as favorites.
    favorites: bool,
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
    /// Favorites only, or no longer.
    Favorites,
    /// The gateway badge shown, or hidden, and kept so in the settings.
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

    /// Whether only favorites are listed.
    #[must_use]
    pub fn favorites(&self) -> bool {
        self.favorites
    }

    /// Whether any filter leaves profiles out: the C# button's dot.
    #[must_use]
    pub fn is_active(&self) -> bool {
        !self.protocols.is_empty() || self.connected || self.gateway || self.favorites
    }

    /// Whether `profile`, its sessions in `state`, passes every filter chosen.
    pub(super) fn accepts(&self, profile: &ProfileSummary, state: Option<SessionState>) -> bool {
        (self.protocols.is_empty() || self.protocols.contains(&profile.kind))
            && (!self.connected || state == Some(SessionState::Connected))
            && (!self.gateway || profile.gateway.is_some())
            && (!self.favorites || profile.favorite)
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
            FilterMessage::Favorites => self.favorites = !self.favorites,
            // The settings', not a filter: see `App::filter_message`.
            FilterMessage::GatewayBadge => {}
            FilterMessage::Reset => *self = Self::default(),
        }
    }
}

impl App {
    /// The tree's filters.
    #[must_use]
    pub fn tree_filter(&self) -> &TreeFilter {
        &self.tree_filter
    }

    /// Whether the tree's rows show the gateway their profile goes through, as the
    /// settings keep it.
    #[must_use]
    pub fn shows_gateway_badge(&self) -> bool {
        self.settings.show_gateway_badge
    }

    pub(super) fn filter_message(&mut self, message: FilterMessage) {
        if message == FilterMessage::GatewayBadge {
            self.toggle_gateway_badge();
        } else {
            self.tree_filter.apply(message);
        }
    }

    /// The gateway badge shown or hidden, saved at once as the C# saves `ShowGatewayBadge`;
    /// a choice that cannot be saved is said and not made.
    fn toggle_gateway_badge(&mut self) {
        self.settings.show_gateway_badge = !self.settings.show_gateway_badge;
        if let Err(error) = self.settings.save(&self.settings_file) {
            self.settings.show_gateway_badge = !self.settings.show_gateway_badge;
            self.dialog = Some(Dialog::save_failed(&error));
        }
    }
}
