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

//! A tab detached to a window of its own, as the C# Heimdall's `FloatingSessionWindow`.
//!
//! The C# takes the session off the strip and hosts it in a window with no owner: a header
//! naming it, with a Reattach button, above the session. Reattached, it goes back after the
//! last tab of its group, pinned or not, and is shown. Its window closed, it goes back to the
//! strip first, then is closed as any tab is: a close declined leaves it on the strip. A
//! split tab is refused, its panes being one tab.
//!
//! Here a detached tab stays among the tabs, with its session, and leaves the strip, as a
//! pane docked in a split does. The window's `active` pane is never a detached tab: showing
//! one focuses its window instead. Files tabs stay on the strip for now: their panes take
//! keys and drops through the main window.

use super::{App, Effect, Notice, Tab};
use crate::ids::{FloatId, TabId};

/// A tab detached to a window of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Floating {
    /// Its window, as the UI layer opened it.
    pub key: FloatId,
    /// The tab it shows; the tab it was opened again as, after a reconnect.
    pub tab: TabId,
}

/// Something about a tab's own window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FloatMessage {
    /// Move a tab to a window of its own, as the C# "Detach to Window": refused for a tab
    /// split or docked in a split, and for a Files tab, each said.
    Detach(TabId),
    /// Put the tab of a window back on the strip, as the C# Reattach button, and show it.
    Reattach(FloatId),
    /// The close button of a window: its tab goes back on the strip, then closes as any
    /// tab does, asking first when it would lose something.
    CloseRequested(FloatId),
    /// A window gained or lost the focus.
    Focused {
        /// The window.
        key: FloatId,
        /// Gained, rather than lost.
        focused: bool,
    },
}

impl App {
    /// The tabs detached to windows of their own, in the order they were.
    #[must_use]
    pub fn floating(&self) -> &[Floating] {
        &self.floating
    }

    /// Whether `tab` is detached to a window of its own, off the strip.
    #[must_use]
    pub fn is_floating(&self, tab: TabId) -> bool {
        self.floating_of(tab).is_some()
    }

    /// The window `tab` is detached to.
    #[must_use]
    pub fn floating_of(&self, tab: TabId) -> Option<FloatId> {
        self.floating
            .iter()
            .find(|floating| floating.tab == tab)
            .map(|floating| floating.key)
    }

    /// The tab window `key` shows, while both are there.
    #[must_use]
    pub fn floating_tab(&self, key: FloatId) -> Option<&Tab> {
        self.floating
            .iter()
            .find(|floating| floating.key == key)
            .and_then(|floating| self.tab(floating.tab))
    }

    /// Whether the tab's menu offers "Detach to Window": a tab of the strip, not split, not
    /// a Files tab, as the C# offers it to a tab not split.
    #[must_use]
    pub fn can_detach(&self, tab: &Tab) -> bool {
        tab.files.is_none() && !self.in_split(tab.id) && !self.is_floating(tab.id)
    }

    /// Applies a message about a tab's own window.
    pub(super) fn float_message(&mut self, message: FloatMessage) -> Vec<Effect> {
        match message {
            FloatMessage::Detach(tab) => self.detach(tab),
            FloatMessage::Reattach(key) => self.reattach(key),
            FloatMessage::CloseRequested(key) => {
                let Some(tab) = self.floating_tab(key).map(|tab| tab.id) else {
                    return self.reattach(key);
                };
                let mut effects = self.reattach(key);
                effects.extend(self.request_close(tab));
                effects
            }
            FloatMessage::Focused { key, focused } => self
                .floating_tab(key)
                .map(|tab| tab.id)
                .map(|tab| self.focus_report(tab, focused))
                .unwrap_or_default(),
        }
    }

    /// Detaches `tab_id` to a window of its own; the keyboard, when it was the tab shown's,
    /// goes to the tab of the strip that takes its place, else to the one before.
    fn detach(&mut self, tab_id: TabId) -> Vec<Effect> {
        let Some(files) = self.tab(tab_id).map(|tab| tab.files.is_some()) else {
            return Vec::new();
        };
        if let Some(key) = self.floating_of(tab_id) {
            return vec![Effect::FocusWindow(key)];
        }
        // A split's panes are one tab, as the C# refuses it; a pane docked in one too.
        if self.in_split(tab_id) {
            self.tell(Notice::DetachSplitRefused);
            return Vec::new();
        }
        if files {
            self.tell(Notice::DetachFilesRefused);
            return Vec::new();
        }
        let shown = self.shown_tab().map(|shown| shown.id) == Some(tab_id);
        let slot = self.strip().iter().position(|found| found.id == tab_id);
        let key = FloatId::fresh();
        self.floating.push(Floating { key, tab: tab_id });
        let mut effects = vec![Effect::OpenWindow(key)];
        if shown {
            let strip: Vec<TabId> = self.strip().iter().map(|found| found.id).collect();
            let neighbour = slot.and_then(|slot| {
                strip
                    .get(slot)
                    .or_else(|| slot.checked_sub(1).and_then(|before| strip.get(before)))
                    .copied()
            });
            match neighbour {
                Some(neighbour) => effects.extend(self.select_tab(neighbour)),
                None => self.active = None,
            }
        }
        effects
    }

    /// Puts the tab of window `key` back on the strip, after the last tab of its group,
    /// pinned or not, as the C# `ReintroduceSession`, and shows it; the window closes and
    /// the main one is focused.
    fn reattach(&mut self, key: FloatId) -> Vec<Effect> {
        let Some(at) = self
            .floating
            .iter()
            .position(|floating| floating.key == key)
        else {
            return Vec::new();
        };
        let Floating { tab, .. } = self.floating.remove(at);
        let mut effects = vec![Effect::CloseWindow(key), Effect::FocusMainWindow];
        let Some(from) = self.tabs.iter().position(|found| found.id == tab) else {
            return effects;
        };
        let moved = self.tabs.remove(from);
        let to = if moved.pinned {
            self.tabs
                .iter()
                .rposition(|found| found.pinned)
                .map_or(0, |last| last + 1)
        } else {
            self.tabs.len()
        };
        self.tabs.insert(to, moved);
        effects.extend(self.select_tab(tab));
        effects
    }

    /// Closes the windows of the tabs gone, as the C# closes a window whose session was
    /// closed elsewhere.
    pub(super) fn prune_floating(&mut self) -> Vec<Effect> {
        let mut effects = Vec::new();
        let tabs: Vec<TabId> = self.tabs.iter().map(|tab| tab.id).collect();
        self.floating.retain(|floating| {
            let kept = tabs.contains(&floating.tab);
            if !kept {
                effects.push(Effect::CloseWindow(floating.key));
            }
            kept
        });
        effects
    }

    /// Tab `old`, opened again as `new`, stays in its window.
    pub(super) fn repoint_floating(&mut self, old: TabId, new: TabId) {
        for floating in &mut self.floating {
            if floating.tab == old {
                floating.tab = new;
            }
        }
    }

    /// The window of `tab`, for a message showing it: none for a tab of the strip.
    pub(super) fn focus_window_of(&self, tab: TabId) -> Option<Vec<Effect>> {
        self.floating_of(tab)
            .map(|key| vec![Effect::FocusWindow(key)])
    }

    /// The windows a user's merge or split of `host` and `tab` reaches are put back on the
    /// strip first, as the C# `EnsureInMainWindow`.
    pub(super) fn reattach_for_split(&mut self, tabs: &[TabId]) -> Vec<Effect> {
        let keys: Vec<FloatId> = tabs
            .iter()
            .filter_map(|tab| self.floating_of(*tab))
            .collect();
        keys.into_iter()
            .flat_map(|key| self.reattach(key))
            .collect()
    }

    /// Whether the keyboard's pane is never a detached tab, which the window cannot show.
    #[must_use]
    pub fn floating_invariant_holds(&self) -> bool {
        self.active.is_none_or(|active| !self.is_floating(active))
    }
}
