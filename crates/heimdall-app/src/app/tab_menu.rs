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

//! A tab's menu, as the C# Heimdall's: a name of the user's, the session opened again in a
//! second tab, the other tabs or those to the right closed at once. Reconnect, Edit and the
//! copies send the messages the failure card and the tree already send.

use super::reconnect::Reopen;
use super::tree::{ProfileKind, ProfileSummary};
use super::{App, Dialog, Effect, Tab};
use crate::ids::TabId;
use crate::text::server_text;

/// Something from a tab's menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TabMenuMessage {
    /// Ask for a name for a tab, its present one written in.
    Rename(TabId),
    /// The tab name typed so far.
    NameEdited(String),
    /// Give a tab back the title its session sets.
    ResetTitle(TabId),
    /// Open the session of a tab again, in a new tab.
    Duplicate(TabId),
    /// Close the tabs of a group, asking first when a live session would end.
    Close {
        /// The tab the menu is for.
        tab: TabId,
        /// Which of the others.
        group: TabGroup,
    },
}

/// Tabs closed together from a tab's menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabGroup {
    /// Every tab but this one.
    Others,
    /// The tabs after this one.
    Right,
}

impl App {
    /// Applies a message from a tab's menu.
    pub(super) fn tab_menu(&mut self, message: TabMenuMessage) -> Vec<Effect> {
        match message {
            TabMenuMessage::Rename(tab) => {
                if let Some(found) = self.tab(tab) {
                    let value = found.display_title().to_owned();
                    self.dialog = Some(Dialog::RenameTab { tab, value });
                }
                Vec::new()
            }
            TabMenuMessage::NameEdited(value) => {
                if let Some(Dialog::RenameTab { value: typed, .. }) = self.dialog.as_mut() {
                    *typed = value;
                }
                Vec::new()
            }
            TabMenuMessage::ResetTitle(tab) => {
                if let Some(found) = self.tab_mut(tab) {
                    found.custom_title = None;
                }
                Vec::new()
            }
            TabMenuMessage::Duplicate(tab) => self.duplicate_tab(tab),
            TabMenuMessage::Close { tab, group } => {
                self.close_group(tab, group);
                Vec::new()
            }
        }
    }

    /// Names `tab_id` as typed, made safe as a server's title is; a name left empty gives the
    /// tab its own title back, as in the C# Heimdall.
    pub(super) fn rename_tab(&mut self, tab_id: TabId, typed: &str) {
        if let Some(tab) = self.tab_mut(tab_id) {
            let name = server_text(typed.trim());
            tab.custom_title = (!name.is_empty()).then_some(name);
        }
    }

    /// The tabs `group` of `tab_id` holds, in tab order.
    #[must_use]
    pub fn tab_group(&self, tab_id: TabId, group: TabGroup) -> Vec<TabId> {
        let Some(index) = self.tabs.iter().position(|tab| tab.id == tab_id) else {
            return Vec::new();
        };
        // From the tab itself for the right, which the filter drops as it drops it from the
        // others.
        let from = match group {
            TabGroup::Others => 0,
            TabGroup::Right => index,
        };
        self.tabs[from..]
            .iter()
            .filter(|tab| tab.id != tab_id)
            .map(|tab| tab.id)
            .collect()
    }

    /// Closes the tabs of `group`, once asked when some are live, as the C# Heimdall asks
    /// once for them all.
    fn close_group(&mut self, tab_id: TabId, group: TabGroup) {
        let tabs = self.tab_group(tab_id, group);
        let live = tabs
            .iter()
            .filter(|id| self.tab(**id).is_some_and(Tab::is_live))
            .count();
        if live > 0 {
            self.dialog = Some(Dialog::ConfirmCloseTabs { tabs, live });
            return;
        }
        for tab in tabs {
            self.close_tab(tab);
        }
    }

    /// Whether the session of `tab` can open again, in its place or in a new tab: what it
    /// ran, its profile or its shell, is still there.
    #[must_use]
    pub fn can_reopen(&self, tab: &Tab) -> bool {
        match &tab.reopen {
            Reopen::Profile(id) => self.profile_summary(id).is_some(),
            Reopen::Shell(_) => true,
        }
    }

    /// Opens the session of `tab_id` again in a new tab, the last; the tab stays as it is.
    fn duplicate_tab(&mut self, tab_id: TabId) -> Vec<Effect> {
        // A profile deleted since opens nothing, as Reconnect's.
        let Some(tab) = self.tab(tab_id) else {
            return Vec::new();
        };
        let (reopen, purpose) = (tab.reopen.clone(), tab.purpose);
        match reopen {
            Reopen::Profile(id) => self.open_saved(&id, purpose),
            Reopen::Shell(shell) => self.open_local(shell),
        }
    }

    /// The protocol of `tab`: its saved profile's, else that of what it runs.
    #[must_use]
    pub fn tab_kind(&self, tab: &Tab) -> ProfileKind {
        self.tab_profile(tab)
            .map_or_else(|| tab.profile.kind(), |profile| profile.kind)
    }

    /// The saved profile `tab` was opened from, while it is still saved.
    #[must_use]
    pub fn tab_profile(&self, tab: &Tab) -> Option<ProfileSummary> {
        match &tab.reopen {
            Reopen::Profile(id) => self.profile_summary(id),
            Reopen::Shell(_) => None,
        }
    }
}
