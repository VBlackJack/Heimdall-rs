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

//! Broadcast input, as the C# Heimdall's: while on, what is typed or pasted into a terminal
//! reaches every other terminal of its scope too, each encoded for its own session. All
//! tabs is asked before it starts; selected tabs starts with none marked. This version has
//! no split panes, so the C# current-tab scope has nothing to reach and is left out.

use std::collections::BTreeSet;

use heimdall_core::settings::BroadcastScope;

use super::{App, Dialog, Effect, Notice, Phase, Tab};
use crate::driver::Purpose;
use crate::ids::TabId;

/// A change of broadcast input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BroadcastMessage {
    /// Turn it on, all tabs asked first, or off.
    Toggle,
    /// Switch between all tabs and the tabs marked.
    Scope,
    /// Mark a tab as a target, or no longer.
    Target(TabId),
}

/// Broadcast input's state: on or off, and the tabs marked.
#[derive(Debug, Clone, Default)]
pub(super) struct Broadcast {
    on: bool,
    targets: BTreeSet<TabId>,
}

/// Whether `tab` takes typed input now: a session of text, connected, asking nothing.
fn takes_input(tab: &Tab) -> bool {
    tab.phase == Phase::Connected
        && tab.prompts.is_empty()
        && tab.purpose == Purpose::Shell
        && tab.desktop.is_none()
        && tab.files.is_none()
}

impl App {
    /// Whether broadcast input is on.
    #[must_use]
    pub fn broadcasting(&self) -> bool {
        self.broadcast.on
    }

    /// Whether `tab` is marked as a target.
    #[must_use]
    pub fn is_broadcast_target(&self, tab: TabId) -> bool {
        self.broadcast.targets.contains(&tab)
    }

    /// The tabs marked as targets that are still open.
    #[must_use]
    pub fn broadcast_target_count(&self) -> usize {
        self.broadcast
            .targets
            .iter()
            .filter(|id| self.tab(**id).is_some())
            .count()
    }

    /// The tabs what is typed into `source` goes to: itself first, then, while broadcast
    /// input is on, every other of its scope that takes input. A tab detached to a window
    /// of its own is out of it, either way, as the C# broadcasts to the main strip only.
    pub(super) fn input_targets(&self, source: TabId) -> Vec<TabId> {
        let mut targets = vec![source];
        if !self.broadcast.on || self.is_floating(source) {
            return targets;
        }
        let scope = self.settings.broadcast_scope;
        targets.extend(
            self.tabs
                .iter()
                .filter(|tab| tab.id != source && takes_input(tab) && !self.is_floating(tab.id))
                .filter(|tab| {
                    scope == BroadcastScope::AllTabs || self.broadcast.targets.contains(&tab.id)
                })
                .map(|tab| tab.id),
        );
        targets
    }

    pub(super) fn broadcast_message(&mut self, message: BroadcastMessage) -> Vec<Effect> {
        let scope = self.settings.broadcast_scope;
        match message {
            BroadcastMessage::Toggle if self.broadcast.on => {
                self.broadcast.on = false;
                self.tell(Notice::BroadcastOff);
            }
            BroadcastMessage::Toggle if scope == BroadcastScope::AllTabs => {
                self.dialog = Some(Dialog::ConfirmBroadcast);
            }
            BroadcastMessage::Toggle => self.start_broadcast(),
            BroadcastMessage::Scope => match scope {
                BroadcastScope::AllTabs => {
                    self.broadcast.targets.clear();
                    self.set_broadcast_scope(BroadcastScope::SelectedTabs);
                }
                BroadcastScope::SelectedTabs if self.broadcast.on => {
                    // Reaching every tab while on is asked first, as starting it is.
                    self.dialog = Some(Dialog::ConfirmBroadcast);
                }
                BroadcastScope::SelectedTabs => self.set_broadcast_scope(BroadcastScope::AllTabs),
            },
            BroadcastMessage::Target(tab) => {
                if !self.broadcast.targets.remove(&tab) && self.tab(tab).is_some() {
                    self.broadcast.targets.insert(tab);
                }
            }
        }
        Vec::new()
    }

    /// The all-tabs question answered yes: broadcast input on, reaching every tab.
    pub(super) fn confirm_broadcast(&mut self) {
        if self.settings.broadcast_scope != BroadcastScope::AllTabs {
            self.set_broadcast_scope(BroadcastScope::AllTabs);
            if self.settings.broadcast_scope != BroadcastScope::AllTabs {
                return;
            }
        }
        self.start_broadcast();
    }

    fn start_broadcast(&mut self) {
        self.broadcast.on = true;
        self.tell(Notice::BroadcastOn(self.settings.broadcast_scope));
    }

    /// Makes `scope` broadcast input's and saves it; a scope that cannot be saved is said
    /// and not applied.
    fn set_broadcast_scope(&mut self, scope: BroadcastScope) {
        let before = self.settings.broadcast_scope;
        self.settings.broadcast_scope = scope;
        if let Err(error) = self.settings.save(&self.settings_file) {
            self.settings.broadcast_scope = before;
            self.dialog = Some(Dialog::StoreError {
                detail: error.to_string(),
            });
            return;
        }
        self.tell(if self.broadcast.on {
            Notice::BroadcastOn(scope)
        } else {
            Notice::BroadcastScope(scope)
        });
    }
}
