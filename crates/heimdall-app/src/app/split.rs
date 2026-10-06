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

//! A tab split in panes, as the C# Heimdall's: a tab of the strip shows other sessions beside
//! its own, side by side or stacked, each in a pane of a binary tree.
//!
//! The tab of the strip is the host: its own session is a leaf of the tree, and the tree is
//! kept on it. A session merged into it is docked: it stays among the tabs, with its
//! session, but leaves the strip. The window's `active` tab is the pane with the keyboard;
//! the tab shown on the strip is its host.

use heimdall_core::profile::ProfileId;
use heimdall_core::split_layouts::{self, Orientation};

use super::reconnect::Reopen;
use super::{App, Dialog, Effect, Message, Notice, QuickResult, Tab};
use crate::ids::TabId;

/// Most panes a tab is split in. The C# allows eight; two for now.
pub const MAX_PANES: usize = 2;

/// Smallest share of a split the first pane takes, as the C# `SplitRatio` clamp.
pub const MIN_RATIO: f32 = split_layouts::MIN_RATIO;

/// Largest share of a split the first pane takes.
pub const MAX_RATIO: f32 = split_layouts::MAX_RATIO;

/// The share a new split gives its first pane, and a reset gives back.
pub const DEFAULT_RATIO: f32 = 0.5;

/// How the two sides of a split are placed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    /// Side by side: the C# `Vertical`, a vertical divider between them.
    SideBySide,
    /// One above the other: the C# `Horizontal`, a horizontal divider between them.
    Stacked,
}

impl Axis {
    /// The other placement.
    #[must_use]
    pub fn toggled(self) -> Self {
        match self {
            Self::SideBySide => Self::Stacked,
            Self::Stacked => Self::SideBySide,
        }
    }

    /// The placement as the splits remembered keep it.
    fn orientation(self) -> Orientation {
        match self {
            Self::SideBySide => Orientation::SideBySide,
            Self::Stacked => Orientation::Stacked,
        }
    }
}

/// Which side of the split a tab merged takes, beside the pane it splits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placement {
    /// Left or above: a tab dropped on the left or top part of the content.
    First,
    /// Right or below, as the C# merge always places it.
    Second,
}

/// A node of a split tab's tree: a pane showing one tab's session, or two nodes split.
#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    /// A pane.
    Leaf(TabId),
    /// Two nodes, `first` left or above.
    Split {
        /// How they are placed.
        axis: Axis,
        /// The share `first` takes, within [`MIN_RATIO`] and [`MAX_RATIO`].
        ratio: f32,
        /// The node left or above.
        first: Box<Node>,
        /// The node right or below.
        second: Box<Node>,
    },
}

impl Node {
    /// The panes, first to last.
    #[must_use]
    pub fn leaves(&self) -> Vec<TabId> {
        let mut leaves = Vec::new();
        self.collect_leaves(&mut leaves);
        leaves
    }

    fn collect_leaves(&self, leaves: &mut Vec<TabId>) {
        match self {
            Self::Leaf(id) => leaves.push(*id),
            Self::Split { first, second, .. } => {
                first.collect_leaves(leaves);
                second.collect_leaves(leaves);
            }
        }
    }

    /// Whether `id` is one of its panes.
    #[must_use]
    pub fn contains(&self, id: TabId) -> bool {
        match self {
            Self::Leaf(leaf) => *leaf == id,
            Self::Split { first, second, .. } => first.contains(id) || second.contains(id),
        }
    }

    /// The first pane.
    #[must_use]
    pub fn first_leaf(&self) -> TabId {
        match self {
            Self::Leaf(id) => *id,
            Self::Split { first, .. } => first.first_leaf(),
        }
    }

    /// The pane `old` shows `new` instead; whether it was there.
    pub fn replace_leaf(&mut self, old: TabId, new: TabId) -> bool {
        match self {
            Self::Leaf(id) if *id == old => {
                *id = new;
                true
            }
            Self::Leaf(_) => false,
            Self::Split { first, second, .. } => {
                first.replace_leaf(old, new) || second.replace_leaf(old, new)
            }
        }
    }

    /// The tree without pane `id`, its sibling taking the place of their split; `None` when
    /// the tree was that pane alone.
    #[must_use]
    pub fn remove_leaf(self, id: TabId) -> Option<Self> {
        match self {
            Self::Leaf(leaf) if leaf == id => None,
            Self::Leaf(_) => Some(self),
            Self::Split {
                axis,
                ratio,
                first,
                second,
            } => match (first.remove_leaf(id), second.remove_leaf(id)) {
                (Some(first), Some(second)) => Some(Self::Split {
                    axis,
                    ratio,
                    first: Box::new(first),
                    second: Box::new(second),
                }),
                (Some(kept), None) | (None, Some(kept)) => Some(kept),
                (None, None) => None,
            },
        }
    }

    /// The node `leaf` stands in split with `added`, `added` second, as the C# merge does.
    pub fn split_leaf(&mut self, leaf: TabId, added: TabId, axis: Axis) -> bool {
        self.split_leaf_at(leaf, added, axis, Placement::Second)
    }

    /// The node `leaf` stands in split with `added`, `added` on the side `placement` says.
    pub fn split_leaf_at(
        &mut self,
        leaf: TabId,
        added: TabId,
        axis: Axis,
        placement: Placement,
    ) -> bool {
        self.split_leaf_with(leaf, added, axis, placement, DEFAULT_RATIO)
    }

    /// The node `leaf` stands in split with `added`, `added` on the side `placement` says,
    /// the first side taking `ratio`, held within [`MIN_RATIO`] and [`MAX_RATIO`].
    pub fn split_leaf_with(
        &mut self,
        leaf: TabId,
        added: TabId,
        axis: Axis,
        placement: Placement,
        ratio: f32,
    ) -> bool {
        match self {
            Self::Leaf(id) if *id == leaf => {
                let (first, second) = match placement {
                    Placement::First => (added, leaf),
                    Placement::Second => (leaf, added),
                };
                *self = Self::Split {
                    axis,
                    ratio: ratio.clamp(MIN_RATIO, MAX_RATIO),
                    first: Box::new(Self::Leaf(first)),
                    second: Box::new(Self::Leaf(second)),
                };
                true
            }
            Self::Leaf(_) => false,
            Self::Split { first, second, .. } => {
                first.split_leaf_with(leaf, added, axis, placement, ratio)
                    || second.split_leaf_with(leaf, added, axis, placement, ratio)
            }
        }
    }
}

/// How a host tab is split: its tree, and the pane last given the keyboard.
#[derive(Debug, Clone, PartialEq)]
pub struct Layout {
    /// The tree; the host is one of its leaves.
    pub root: Node,
    /// The pane given the keyboard when the tab is shown.
    pub focus: TabId,
}

impl Layout {
    /// The panes, first to last.
    #[must_use]
    pub fn leaves(&self) -> Vec<TabId> {
        self.root.leaves()
    }

    /// The share the first side of the outer split takes.
    #[must_use]
    pub fn ratio(&self) -> Option<f32> {
        match &self.root {
            Node::Split { ratio, .. } => Some(*ratio),
            Node::Leaf(_) => None,
        }
    }

    /// How the outer split places its sides.
    #[must_use]
    pub fn axis(&self) -> Option<Axis> {
        match &self.root {
            Node::Split { axis, .. } => Some(*axis),
            Node::Leaf(_) => None,
        }
    }

    /// The secondary pane, as the C# `SecondaryPaneOrNull`: the first of the outer split's
    /// second side.
    #[must_use]
    pub fn secondary(&self) -> Option<TabId> {
        match &self.root {
            Node::Split { second, .. } => Some(second.first_leaf()),
            Node::Leaf(_) => None,
        }
    }

    fn set_ratio(&mut self, value: f32) {
        if let Node::Split { ratio, .. } = &mut self.root {
            *ratio = value.clamp(MIN_RATIO, MAX_RATIO);
        }
    }

    fn toggle_axis(&mut self) {
        if let Node::Split { axis, .. } = &mut self.root {
            *axis = axis.toggled();
        }
    }

    fn swap(&mut self) {
        if let Node::Split { first, second, .. } = &mut self.root {
            std::mem::swap(first, second);
        }
    }
}

/// Something done to a split tab, or to one of its panes.
#[derive(Debug, Clone, PartialEq)]
pub enum SplitMessage {
    /// Show `tab` in a pane of `host`, as the C# "Merge with..." and a tab dropped on the
    /// content: refused when it is the host, already in a split, or would make more than
    /// [`MAX_PANES`].
    Merge {
        /// The tab of the strip split.
        host: TabId,
        /// The tab merged into it.
        tab: TabId,
        /// How the two are placed.
        axis: Axis,
        /// The side the merged tab takes; the C# always gives it the second.
        placement: Placement,
    },
    /// Open a profile in a new tab and merge it into the tab shown, as the tree's "Open in
    /// split": nothing opens beyond [`MAX_PANES`], nothing merges when it does not open.
    OpenInSplit {
        /// The profile opened.
        profile: ProfileId,
        /// How the two are placed.
        axis: Axis,
    },
    /// Open what Quick Connect offered and merge it into `host`, as the C# palette in split
    /// mode, opened from a tab's "Split...".
    QuickConnect {
        /// The tab split.
        host: TabId,
        /// How the two are placed.
        axis: Axis,
        /// What was chosen.
        result: QuickResult,
    },
    /// Give the keyboard to the pane after the focused one of `host`'s split, the last one
    /// handing it to the first.
    FocusNext(TabId),
    /// Give the keyboard to the pane before the focused one of `host`'s split, the first
    /// one handing it to the last.
    FocusPrevious(TabId),
    /// Every pane of `host` back on the strip as a tab of its own, right after it.
    Unsplit(TabId),
    /// The two sides of `host`'s outer split change places.
    Swap(TabId),
    /// `host`'s outer split placed the other way, as Ctrl+Shift+O.
    ToggleAxis(TabId),
    /// The share the first side of `host`'s outer split takes, as dragged.
    Resize {
        /// The split tab.
        host: TabId,
        /// The share, clamped between [`MIN_RATIO`] and [`MAX_RATIO`].
        ratio: f32,
    },
    /// The share back to [`DEFAULT_RATIO`], as a double click on the divider.
    ResetRatio(TabId),
    /// Give a pane the keyboard.
    Focus(TabId),
    /// Close one pane, asking first as closing its tab would; the others stay split.
    ClosePane(TabId),
    /// Close `host`'s secondary pane, as the C# "Close Secondary Pane".
    CloseSecondary(TabId),
}

impl SplitMessage {
    /// The tabs a merge or a split asked for reaches, the host and the tab merged: a tab
    /// detached to a window of its own comes back to the strip first.
    pub(super) fn tabs(&self) -> Vec<TabId> {
        match self {
            Self::Merge { host, tab, .. } => vec![*host, *tab],
            Self::QuickConnect { host, .. } => vec![*host],
            Self::OpenInSplit { .. }
            | Self::FocusNext(_)
            | Self::FocusPrevious(_)
            | Self::Unsplit(_)
            | Self::Swap(_)
            | Self::ToggleAxis(_)
            | Self::Resize { .. }
            | Self::ResetRatio(_)
            | Self::Focus(_)
            | Self::ClosePane(_)
            | Self::CloseSecondary(_) => Vec::new(),
        }
    }
}

impl App {
    /// The tabs on the strip, in order: every tab but those docked in a split and those
    /// detached to a window of their own.
    #[must_use]
    pub fn strip(&self) -> Vec<&Tab> {
        self.tabs
            .iter()
            .filter(|tab| !self.is_docked(tab.id) && !self.is_floating(tab.id))
            .collect()
    }

    /// Whether `id` is docked in another tab's split, off the strip.
    #[must_use]
    pub fn is_docked(&self, id: TabId) -> bool {
        self.docked_in(id).is_some()
    }

    /// The tab of the strip whose split docks `id`.
    fn docked_in(&self, id: TabId) -> Option<TabId> {
        self.tabs
            .iter()
            .find(|tab| {
                tab.id != id
                    && tab
                        .layout
                        .as_ref()
                        .is_some_and(|layout| layout.root.contains(id))
            })
            .map(|tab| tab.id)
    }

    /// The tab of the strip that shows `id`: its host when docked, else itself; `None` once
    /// it is gone.
    #[must_use]
    pub fn host_of(&self, id: TabId) -> Option<TabId> {
        self.tab(id)?;
        Some(self.docked_in(id).unwrap_or(id))
    }

    /// The tab of the strip shown: the active pane's host.
    #[must_use]
    pub fn shown_tab(&self) -> Option<&Tab> {
        self.active
            .and_then(|id| self.host_of(id))
            .and_then(|id| self.tab(id))
    }

    /// The panes of strip tab `id`: its split's, or itself alone.
    #[must_use]
    pub fn panes_of(&self, id: TabId) -> Vec<TabId> {
        match self.tab(id).and_then(|tab| tab.layout.as_ref()) {
            Some(layout) => layout.leaves(),
            None => vec![id],
        }
    }

    /// Whether `id` is a pane of a split: its host, or docked in one.
    #[must_use]
    pub fn in_split(&self, id: TabId) -> bool {
        self.host_of(id)
            .and_then(|host| self.tab(host))
            .is_some_and(|host| host.layout.is_some())
    }

    /// The tabs of the strip `host` can be merged with, as the C# "Merge with..." lists them:
    /// the others not split themselves, nor detached; none once `host` is split, docked or
    /// detached.
    #[must_use]
    pub fn merge_candidates(&self, host: TabId) -> Vec<&Tab> {
        let splittable = self.tab(host).is_some_and(|tab| {
            tab.layout.is_none() && !self.is_docked(host) && !self.is_floating(host)
        });
        if !splittable {
            return Vec::new();
        }
        self.strip()
            .into_iter()
            .filter(|tab| tab.id != host && tab.layout.is_none())
            .collect()
    }

    /// Applies a message about a split.
    pub(super) fn split_message(&mut self, message: SplitMessage) -> Vec<Effect> {
        match message {
            SplitMessage::Merge {
                host,
                tab,
                axis,
                placement,
            } => return self.merge(host, tab, axis, placement),
            SplitMessage::OpenInSplit { profile, axis } => {
                // The tab shown when chosen, as the C# reads the active session on the click.
                let Some(host) = self.shown_tab().map(|tab| tab.id) else {
                    return Vec::new();
                };
                return self.open_merged(host, axis, Message::ConnectProfile(profile));
            }
            SplitMessage::QuickConnect { host, axis, result } => {
                return self.open_merged(host, axis, Message::QuickConnect(result));
            }
            SplitMessage::FocusNext(host) => return self.cycle_focus(host, true),
            SplitMessage::FocusPrevious(host) => return self.cycle_focus(host, false),
            SplitMessage::Unsplit(host) => self.unsplit(host),
            SplitMessage::Swap(host) => self.edit_layout(host, Layout::swap),
            SplitMessage::ToggleAxis(host) => self.edit_layout(host, Layout::toggle_axis),
            // A share no split has is not taken.
            // Remembered for its pair, as the C# records a drag's end and a reset.
            SplitMessage::Resize { host, ratio } if ratio.is_finite() => {
                self.edit_layout(host, |layout| layout.set_ratio(ratio));
                self.remember_layout(host);
            }
            SplitMessage::Resize { .. } => {}
            SplitMessage::ResetRatio(host) => {
                self.edit_layout(host, |layout| layout.set_ratio(DEFAULT_RATIO));
                self.remember_layout(host);
            }
            SplitMessage::Focus(tab) => return self.focus_pane(tab),
            SplitMessage::ClosePane(tab) => self.request_pane_close(tab),
            SplitMessage::CloseSecondary(host) => {
                if let Some(secondary) = self
                    .tab(host)
                    .and_then(|found| found.layout.as_ref())
                    .and_then(Layout::secondary)
                {
                    self.request_pane_close(secondary);
                }
            }
        }
        Vec::new()
    }

    /// Changes `host`'s split, when it is one.
    fn edit_layout(&mut self, host: TabId, edit: impl FnOnce(&mut Layout)) {
        if let Some(layout) = self.tab_mut(host).and_then(|tab| tab.layout.as_mut()) {
            edit(layout);
        }
    }

    /// Docks `tab` in a pane of `host`, beside the pane that had the keyboard on the side
    /// `placement` says, and shows them. A tab still connecting is merged too: it connects
    /// in its pane. The split starts at the share last given to the pair of their profiles,
    /// and is remembered, as the C# merge does.
    fn merge(&mut self, host: TabId, tab: TabId, axis: Axis, placement: Placement) -> Vec<Effect> {
        let mergeable = host != tab
            && self.tab(host).is_some()
            && self.tab(tab).is_some_and(|found| found.layout.is_none())
            && !self.is_docked(host)
            && !self.is_docked(tab);
        if !mergeable {
            return Vec::new();
        }
        if self.panes_of(host).len() + 1 > MAX_PANES {
            self.tell(Notice::SplitMaxPanesReached(MAX_PANES));
            return Vec::new();
        }
        // The pane shown keeps the keyboard: the merged one when it was the tab shown.
        let shown = self.active.and_then(|id| self.host_of(id));
        let focus = if shown == Some(tab) { tab } else { host };
        let beside = self.focus_of(host);
        let (first, second) = match placement {
            Placement::First => (tab, beside),
            Placement::Second => (beside, tab),
        };
        let ratio = self.remembered_ratio(first, second);
        let Some(found) = self.tab_mut(host) else {
            return Vec::new();
        };
        let layout = found.layout.get_or_insert(Layout {
            root: Node::Leaf(host),
            focus: host,
        });
        layout
            .root
            .split_leaf_with(beside, tab, axis, placement, ratio);
        layout.focus = focus;
        // Off the strip, a tab is never pinned.
        if let Some(merged) = self.tab_mut(tab) {
            merged.pinned = false;
        }
        self.remember_split(first, second, axis, ratio);
        self.select_tab(host)
    }

    /// The saved profile pane `tab` shows; `None` for a session saved nowhere or a local
    /// shell: the C# remembers splits by server identifier, and these have none.
    pub(super) fn saved_profile(&self, tab: TabId) -> Option<ProfileId> {
        match &self.tab(tab)?.reopen {
            Reopen::Profile(id) => Some(id.clone()),
            Reopen::Shell(_) | Reopen::Script(_) | Reopen::Transient(..) | Reopen::LocalBrowser => {
                None
            }
        }
    }

    /// The share a new split of `first` and `second` gives `first`: the one last given to
    /// the pair of their profiles, else [`DEFAULT_RATIO`], as the C# `RememberedRatio`.
    fn remembered_ratio(&self, first: TabId, second: TabId) -> f32 {
        match (self.saved_profile(first), self.saved_profile(second)) {
            (Some(first), Some(second)) => self.split_layouts.ratio(&first, &second),
            _ => None,
        }
        .unwrap_or(DEFAULT_RATIO)
    }

    /// Remembers the split of `first` and `second`, `first` left or above, when both show
    /// saved profiles; a file that cannot be written is logged, the split goes on.
    fn remember_split(&mut self, first: TabId, second: TabId, axis: Axis, ratio: f32) {
        let (Some(first), Some(second)) = (self.saved_profile(first), self.saved_profile(second))
        else {
            return;
        };
        if let Err(error) = self
            .split_layouts
            .record(&first, &second, axis.orientation(), ratio)
        {
            log::warn!("the split layout was not kept: {error}");
        }
    }

    /// Remembers the outer split of `host` as it now is, as the C# `RememberSplitRatio`:
    /// its first pane and its secondary one.
    fn remember_layout(&mut self, host: TabId) {
        let Some(Layout {
            root:
                Node::Split {
                    axis,
                    ratio,
                    first,
                    second,
                },
            ..
        }) = self.tab(host).and_then(|tab| tab.layout.as_ref())
        else {
            return;
        };
        let (first, second, axis, ratio) = (first.first_leaf(), second.first_leaf(), *axis, *ratio);
        self.remember_split(first, second, axis, ratio);
    }

    /// Opens a session with `open`, then merges the tab it opened into `host`, the new pane
    /// with the keyboard. Beyond [`MAX_PANES`] it is said and nothing opens; an open
    /// refused, by the session limit or a question asked first, merges nothing.
    fn open_merged(&mut self, host: TabId, axis: Axis, open: Message) -> Vec<Effect> {
        if self.tab(host).is_none() || self.is_docked(host) {
            return Vec::new();
        }
        if self.panes_of(host).len() >= MAX_PANES {
            self.tell(Notice::SplitMaxPanesReached(MAX_PANES));
            return Vec::new();
        }
        let before: Vec<TabId> = self.tabs.iter().map(|tab| tab.id).collect();
        let mut effects = self.update(open);
        let opened = self
            .tabs
            .iter()
            .map(|tab| tab.id)
            .find(|id| !before.contains(id));
        if let Some(opened) = opened {
            effects.extend(self.merge(host, opened, axis, Placement::Second));
        }
        effects
    }

    /// Gives the keyboard to the pane of `host`'s split after the focused one, or before it
    /// when not `forward`, round from the last to the first, as Next and Previous go round
    /// the tabs; nothing when it is not split.
    fn cycle_focus(&mut self, host: TabId, forward: bool) -> Vec<Effect> {
        let Some(layout) = self.tab(host).and_then(|tab| tab.layout.as_ref()) else {
            return Vec::new();
        };
        let panes = layout.leaves();
        let at = panes
            .iter()
            .position(|id| *id == layout.focus)
            .unwrap_or_default();
        let next = if forward {
            (at + 1) % panes.len()
        } else {
            (at + panes.len() - 1) % panes.len()
        };
        self.focus_pane(panes[next])
    }

    /// Every docked pane of `host` back on the strip, right after it, in the order of the
    /// split; the host keeps the keyboard when one of them had it.
    fn unsplit(&mut self, host: TabId) {
        let Some(layout) = self.tab_mut(host).and_then(|tab| tab.layout.take()) else {
            return;
        };
        let docked: Vec<TabId> = layout
            .leaves()
            .into_iter()
            .filter(|id| *id != host)
            .collect();
        for id in &docked {
            self.local_browser_left(host, *id);
        }
        for (offset, id) in docked.iter().enumerate() {
            let Some(from) = self.tabs.iter().position(|tab| tab.id == *id) else {
                continue;
            };
            let moved = self.tabs.remove(from);
            let at = self
                .tabs
                .iter()
                .position(|tab| tab.id == host)
                .map_or(self.tabs.len(), |index| index + 1 + offset)
                .min(self.tabs.len());
            self.tabs.insert(at, moved);
        }
        // A stable sort: a host pinned keeps its pinned tabs first.
        self.tabs.sort_by_key(|tab| !tab.pinned);
        if self.active.is_some_and(|id| docked.contains(&id)) {
            self.active = Some(host);
        }
    }

    /// Gives pane `tab` the keyboard, its split showing; a tab detached to a window of its
    /// own has its window focused instead, the keyboard's pane left as it is.
    pub(super) fn focus_pane(&mut self, tab: TabId) -> Vec<Effect> {
        if let Some(effects) = self.focus_window_of(tab) {
            return effects;
        }
        let Some(host) = self.host_of(tab) else {
            return Vec::new();
        };
        if let Some(layout) = self.tab_mut(host).and_then(|found| found.layout.as_mut()) {
            layout.focus = tab;
        }
        // Every pane of the split is in sight: their bells are heard.
        for pane in self.panes_of(host) {
            if let Some(found) = self.tab_mut(pane) {
                found.bell = false;
            }
        }
        self.active = Some(tab);
        self.tab(tab)
            .map(super::clipboard_offer)
            .unwrap_or_default()
    }

    /// The pane of strip tab `id` given the keyboard when it is shown.
    pub(super) fn focus_of(&self, id: TabId) -> TabId {
        self.tab(id)
            .and_then(|tab| tab.layout.as_ref())
            .map_or(id, |layout| layout.focus)
    }

    /// The split showing the active pane remembers it as the one with the keyboard.
    pub(super) fn sync_focus(&mut self) {
        let Some(active) = self.active else {
            return;
        };
        let Some(host) = self.host_of(active) else {
            return;
        };
        if let Some(layout) = self.tab_mut(host).and_then(|tab| tab.layout.as_mut()) {
            layout.focus = active;
        }
    }

    /// Tab `old`, opened again as `new`, keeps its place in a split: its host's pane, or
    /// as the host, its split, moved onto `new` beforehand; detached, its window.
    pub(super) fn repoint_pane(&mut self, old: TabId, new: TabId) {
        self.repoint_floating(old, new);
        for tab in &mut self.tabs {
            if let Some(layout) = tab.layout.as_mut()
                && layout.root.replace_leaf(old, new)
                && layout.focus == old
            {
                layout.focus = new;
            }
        }
    }

    /// Asks before closing pane `tab` alone, as closing its tab asks; the others of its split
    /// stay, the last one alone back to a plain tab.
    fn request_pane_close(&mut self, tab: TabId) {
        if self.tab(tab).is_none() {
            return;
        }
        if self.ask_before_closing(tab, &[tab]) {
            self.closing_pane = Some(tab);
        } else {
            self.close_pane(tab);
        }
    }

    /// Asks before `panes` close, as `closed`, when one of them would lose something: the
    /// first guard any of them raises, in the C# order; whether a question was asked.
    pub(super) fn ask_before_closing(&mut self, closed: TabId, panes: &[TabId]) -> bool {
        let tabs: Vec<&Tab> = panes.iter().filter_map(|id| self.tab(*id)).collect();
        let dialog = if let Some(tab) = tabs.iter().find(|tab| tab.holds_unsaved_text()) {
            // Its editor's text would be lost: said first, as the C# close guard.
            let name = tab
                .files
                .as_deref()
                .and_then(|pane| pane.editor.as_ref())
                .map(|edit| edit.name.clone())
                .unwrap_or_default();
            Dialog::ConfirmCloseEditor { tab: closed, name }
        } else if let Some(tab) = tabs
            .iter()
            .find(|tab| tab.files.as_deref().is_some_and(|pane| pane.running() > 0))
        {
            // Its transfers would be cancelled: said, as the C# Files tab says it.
            Dialog::ConfirmCloseTransfers {
                tab: closed,
                name: tab.display_title().to_owned(),
            }
        } else if let Some(tab) = tabs.iter().find(|tab| {
            tab.files
                .as_deref()
                .is_some_and(|pane| !pane.edits.is_empty())
        }) {
            // Its edits' next saves would no longer be sent: said, as the C# close guard.
            Dialog::ConfirmCloseEdits {
                tab: closed,
                name: tab.display_title().to_owned(),
            }
        } else if tabs.iter().any(|tab| tab.is_live()) {
            Dialog::ConfirmCloseTab(closed)
        } else {
            return false;
        };
        self.dialog = Some(dialog);
        true
    }

    /// Closes pane `tab_id` alone. Docked, it leaves its host's split; the host, its split
    /// and its place on the strip pass to the pane left first. The keyboard goes to another
    /// pane of the same split, else to the tab of the strip that takes the closed one's
    /// place, never to a pane hidden in another tab's split.
    pub(super) fn close_pane(&mut self, tab_id: TabId) {
        let Some(host) = self.host_of(tab_id) else {
            return;
        };
        let strip: Vec<TabId> = self.strip().iter().map(|tab| tab.id).collect();
        let slot = strip.iter().position(|id| *id == host);
        let mut heir = None;
        if host == tab_id {
            heir = self.hand_over_split(tab_id);
        } else {
            self.leave_split(host, tab_id);
            self.local_browser_left(host, tab_id);
        }
        let Some(index) = self.tabs.iter().position(|tab| tab.id == tab_id) else {
            return;
        };
        // Cancelling the attempt drops its pending questions from the registry.
        let mut tab = self.tabs.remove(index);
        tab.stop();
        if self.active != Some(tab_id) {
            return;
        }
        let next = if host == tab_id {
            heir.or_else(|| {
                let strip: Vec<TabId> = self.strip().iter().map(|tab| tab.id).collect();
                slot.and_then(|slot| strip.get(slot.min(strip.len().saturating_sub(1))))
                    .copied()
            })
        } else {
            Some(host)
        };
        self.active = next.map(|id| self.focus_of(id));
    }

    /// Pane `tab` taken out of its split, a tab of its own again, as the C# `MoveIntoOwnTab`:
    /// the panes left keep the split, a single one back to a plain tab. The host taken out
    /// hands its split and its place on the strip to the pane left first. The tab of the
    /// strip that shows the panes left; `None` when `tab` is in no split.
    pub(super) fn take_out_pane(&mut self, tab: TabId) -> Option<TabId> {
        let host = self.host_of(tab)?;
        self.tab(host)?.layout.as_ref()?;
        if host == tab {
            let heir = self.hand_over_split(tab)?;
            // A shell leaving its file browser docks none again, as the browser closed.
            self.local_browser_left(tab, heir);
            Some(heir)
        } else {
            self.leave_split(host, tab);
            self.local_browser_left(host, tab);
            Some(host)
        }
    }

    /// Pane `tab` out of `host`'s split; the split gone once a single pane is left.
    fn leave_split(&mut self, host: TabId, tab: TabId) {
        let Some(found) = self.tab_mut(host) else {
            return;
        };
        let Some(layout) = found.layout.take() else {
            return;
        };
        let focus = layout.focus;
        found.layout = match layout.root.remove_leaf(tab) {
            Some(root @ Node::Split { .. }) => {
                let focus = if focus == tab {
                    root.first_leaf()
                } else {
                    focus
                };
                Some(Layout { root, focus })
            }
            Some(Node::Leaf(_)) | None => None,
        };
    }

    /// Host `tab`'s split and place on the strip handed to the first pane left, its pin
    /// with them, before the host closes; that pane, when there was a split.
    fn hand_over_split(&mut self, tab: TabId) -> Option<TabId> {
        let index = self.tabs.iter().position(|found| found.id == tab)?;
        let layout = self.tabs[index].layout.take()?;
        let pinned = self.tabs[index].pinned;
        let focus = layout.focus;
        let root = layout.root.remove_leaf(tab)?;
        let heir = root.first_leaf();
        let from = self.tabs.iter().position(|found| found.id == heir)?;
        let mut moved = self.tabs.remove(from);
        moved.pinned = pinned;
        moved.layout = match root {
            root @ Node::Split { .. } => Some(Layout {
                focus: if focus == tab { heir } else { focus },
                root,
            }),
            Node::Leaf(_) => None,
        };
        // Into the host's place, which is where it was or one before once it left.
        let at = self
            .tabs
            .iter()
            .position(|found| found.id == tab)
            .unwrap_or(self.tabs.len());
        self.tabs.insert(at, moved);
        Some(heir)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids() -> (TabId, TabId, TabId) {
        (TabId::fresh(), TabId::fresh(), TabId::fresh())
    }

    #[test]
    fn a_leaf_removed_promotes_its_sibling() {
        let (a, b, c) = ids();
        let mut root = Node::Leaf(a);
        assert!(root.split_leaf(a, b, Axis::SideBySide));
        assert!(root.split_leaf(b, c, Axis::Stacked));
        assert_eq!(root.leaves(), [a, b, c]);
        let root = root.remove_leaf(b).expect("two left");
        assert_eq!(root.leaves(), [a, c]);
        assert!(matches!(
            &root,
            Node::Split { axis: Axis::SideBySide, second, .. } if **second == Node::Leaf(c)
        ));
        assert_eq!(Node::Leaf(a).remove_leaf(a), None);
    }

    #[test]
    fn a_leaf_split_first_puts_the_added_pane_before_it() {
        let (a, b, _) = ids();
        let mut root = Node::Leaf(a);
        assert!(root.split_leaf_at(a, b, Axis::Stacked, Placement::First));
        assert_eq!(root.leaves(), [b, a]);
        assert!(!root.split_leaf_at(TabId::fresh(), b, Axis::Stacked, Placement::First));
        let mut root = Node::Leaf(a);
        assert!(root.split_leaf_at(a, b, Axis::SideBySide, Placement::Second));
        assert_eq!(root.leaves(), [a, b], "as the C# merge");
    }

    #[test]
    fn a_leaf_replaced_keeps_its_place() {
        let (a, b, c) = ids();
        let mut root = Node::Leaf(a);
        root.split_leaf(a, b, Axis::Stacked);
        assert!(root.replace_leaf(a, c));
        assert!(!root.replace_leaf(a, c), "no longer there");
        assert_eq!(root.leaves(), [c, b]);
        assert_eq!(root.first_leaf(), c);
    }

    #[test]
    fn the_ratio_stays_within_the_csharp_clamp() {
        let (a, b, _) = ids();
        let mut layout = Layout {
            root: Node::Leaf(a),
            focus: a,
        };
        layout.root.split_leaf(a, b, Axis::SideBySide);
        assert_eq!(layout.ratio(), Some(DEFAULT_RATIO));
        layout.set_ratio(0.01);
        assert_eq!(layout.ratio(), Some(MIN_RATIO));
        layout.set_ratio(2.0);
        assert_eq!(layout.ratio(), Some(MAX_RATIO));
        layout.swap();
        assert_eq!(layout.leaves(), [b, a]);
        assert_eq!(layout.secondary(), Some(a));
        layout.toggle_axis();
        assert_eq!(layout.axis(), Some(Axis::Stacked));
    }
}
