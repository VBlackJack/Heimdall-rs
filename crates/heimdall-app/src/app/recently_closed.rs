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

//! The tabs the user closed lately, for "Reopen closed tab" to bring them back the way a
//! browser does, as the C# `ConnectionViewModel.RecentlyClosed` (v2026.101301, #551, #582):
//! - only a close the user asked for is remembered: not a lock closing every session, a
//!   pane closed alone or a session ending by itself; a tool's tab, a local shell and the
//!   file browser beside it have nothing to reopen and are skipped;
//! - one gesture is one entry: "Close others", "Close to the right" or "Close all tabs" is
//!   remembered whole and reopened whole, as the C# `RememberAsOneGestureAsync`;
//! - the last [`RECENTLY_CLOSED_CAPACITY`] gestures, in memory alone, never written to disk;
//! - a session saved nowhere, typed in Quick Connect or opened with "Connect as", reopens
//!   too, which the C# skips: from its target and options as they were typed. Neither entry
//!   ever holds a password or any other secret: those are asked for, or read from the vault,
//!   when the session connects again;
//! - forgotten when the workspace locks and when the application quits: who unlocks it
//!   again finds nothing of what was closed before.

use std::collections::{HashSet, VecDeque};

use heimdall_core::profile::ProfileId;

use super::reconnect::Reopen;
use super::{App, Effect, Tab, TabProfile};
use crate::driver::Purpose;
use crate::ids::TabId;

/// How many close gestures are remembered, as the C# `RecentlyClosedCapacity`.
pub const RECENTLY_CLOSED_CAPACITY: usize = 10;

/// A tab closed, as it opens again: what names its session, and never a credential.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClosedTab {
    /// A saved profile, by its identifier alone: it reopens as it is saved then.
    Profile {
        /// The profile.
        id: ProfileId,
        /// What it was opened for: a shell or its files.
        purpose: Purpose,
    },
    /// A session saved nowhere, from Quick Connect or "Connect as": its host, port, account
    /// and options as they were typed. A profile holds no secret.
    Transient {
        /// What it connects to.
        profile: Box<TabProfile>,
        /// What it was opened for.
        purpose: Purpose,
    },
}

impl ClosedTab {
    /// What reopens `tab`, when anything does.
    fn of(tab: &Tab) -> Option<Self> {
        match &tab.reopen {
            Reopen::Profile(id) => Some(Self::Profile {
                id: id.clone(),
                purpose: tab.purpose,
            }),
            Reopen::Transient(profile, purpose)
                if !matches!(**profile, TabProfile::Local(_) | TabProfile::Tool(_)) =>
            {
                Some(Self::Transient {
                    profile: profile.clone(),
                    purpose: *purpose,
                })
            }
            Reopen::Transient(..)
            | Reopen::Shell(_)
            | Reopen::Script(_)
            | Reopen::LocalBrowser
            | Reopen::Tool => None,
        }
    }

    /// Whether it can still open: its profile is still saved, out of `saved`; a session
    /// saved nowhere always can.
    fn still_there(&self, saved: &HashSet<ProfileId>) -> bool {
        match self {
            Self::Profile { id, .. } => saved.contains(id),
            Self::Transient { .. } => true,
        }
    }
}

/// The close gestures remembered, oldest first, each the tabs it closed in their order.
#[derive(Debug, Default)]
pub struct RecentlyClosed {
    gestures: VecDeque<Vec<ClosedTab>>,
}

impl RecentlyClosed {
    /// One gesture's tabs, the oldest gesture forgotten past the capacity; nothing for a
    /// gesture that closed nothing to reopen.
    pub fn remember(&mut self, tabs: Vec<ClosedTab>) {
        if tabs.is_empty() {
            return;
        }
        self.gestures.push_back(tabs);
        while self.gestures.len() > RECENTLY_CLOSED_CAPACITY {
            self.gestures.pop_front();
        }
    }

    /// Takes the tabs of the most recent gesture that `still_there` accepts, in the order
    /// they were closed, as the C# `TakeMostRecentlyClosed`: a gesture whose tabs are all
    /// gone since is dropped, and the one before it taken. Empty when none is left.
    pub fn take_most_recent(&mut self, still_there: impl Fn(&ClosedTab) -> bool) -> Vec<ClosedTab> {
        while let Some(gesture) = self.gestures.pop_back() {
            let kept: Vec<ClosedTab> = gesture.into_iter().filter(&still_there).collect();
            if !kept.is_empty() {
                return kept;
            }
        }
        Vec::new()
    }

    /// How many tabs [`Self::take_most_recent`] would reopen now; 0 when none.
    #[must_use]
    pub fn most_recent_count(&self, still_there: impl Fn(&ClosedTab) -> bool) -> usize {
        self.gestures
            .iter()
            .rev()
            .map(|gesture| gesture.iter().filter(|tab| still_there(tab)).count())
            .find(|count| *count > 0)
            .unwrap_or(0)
    }

    /// The gestures remembered, oldest first.
    pub fn gestures(&self) -> impl Iterator<Item = &[ClosedTab]> {
        self.gestures.iter().map(Vec::as_slice)
    }

    /// Whether nothing is remembered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.gestures.is_empty()
    }

    /// Forgets every gesture.
    pub fn clear(&mut self) {
        self.gestures.clear();
    }
}

impl App {
    /// The tabs closed lately, to reopen.
    #[must_use]
    pub fn recently_closed(&self) -> &RecentlyClosed {
        &self.recently_closed
    }

    /// How many tabs "Reopen closed tab" reopens now: those of the most recent close whose
    /// profiles are still saved; 0 when there is none, and while the workspace is locked.
    #[must_use]
    pub fn reopen_closed_count(&self) -> usize {
        if self.is_locked() {
            return 0;
        }
        let saved = self.saved_profile_ids();
        self.recently_closed
            .most_recent_count(|tab| tab.still_there(&saved))
    }

    /// Closes `tabs`, each with every pane of its split, as one gesture remembered to be
    /// reopened whole: one tab closed alone, or a group from a tab's menu.
    pub(super) fn close_tabs_by_user(&mut self, tabs: &[TabId]) {
        // A pane docked in a split is the tab it is docked in's, as the C# remembers the
        // tab and not its panes: reopened, a shell docks its SFTP pane again by itself.
        let closed = tabs
            .iter()
            .filter(|id| !self.is_docked(**id))
            .filter_map(|id| self.tab(*id))
            .filter_map(ClosedTab::of)
            .collect();
        for tab in tabs {
            // A pane already closed with its split.
            if self.tab(*tab).is_some() {
                self.close_tab(*tab);
            }
        }
        self.recently_closed.remember(closed);
    }

    /// Ctrl+Shift+R, or the tab menu's "Reopen closed tab": the tabs of the most recent
    /// close open again, each in a new tab, those whose profile was deleted since left out,
    /// as the C# `SessionCoordinator.ReopenClosedTab`. Nothing behind the lock screen.
    pub(super) fn reopen_closed(&mut self) -> Vec<Effect> {
        if self.is_locked() {
            return Vec::new();
        }
        let saved = self.saved_profile_ids();
        let tabs = self
            .recently_closed
            .take_most_recent(|tab| tab.still_there(&saved));
        let mut effects = Vec::new();
        for tab in tabs {
            let (reopen, purpose) = match tab {
                ClosedTab::Profile { id, purpose } => (Reopen::Profile(id), purpose),
                ClosedTab::Transient { profile, purpose } => {
                    (Reopen::Transient(profile, purpose), purpose)
                }
            };
            effects.extend(self.open_again(reopen, purpose));
        }
        effects
    }

    /// The identifiers of every saved profile.
    fn saved_profile_ids(&self) -> HashSet<ProfileId> {
        self.profile_summaries()
            .into_iter()
            .map(|profile| profile.id)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(id: &str) -> ClosedTab {
        ClosedTab::Profile {
            id: ProfileId::new(id),
            purpose: Purpose::Shell,
        }
    }

    fn name(tab: &ClosedTab) -> &str {
        match tab {
            ClosedTab::Profile { id, .. } => id.as_str(),
            ClosedTab::Transient { .. } => "transient",
        }
    }

    fn ids(tabs: &[ClosedTab]) -> Vec<&str> {
        tabs.iter().map(name).collect()
    }

    #[test]
    fn the_last_ten_gestures_are_kept_and_the_most_recent_comes_back_first() {
        let mut closed = RecentlyClosed::default();
        for n in 0..12 {
            closed.remember(vec![profile(&format!("p{n}"))]);
        }
        assert_eq!(closed.gestures().count(), RECENTLY_CLOSED_CAPACITY);
        assert_eq!(RECENTLY_CLOSED_CAPACITY, 10, "as the C# capacity");
        assert_eq!(
            ids(closed.gestures().next().expect("oldest")),
            ["p2"],
            "the two oldest forgotten"
        );
        for n in (2..12).rev() {
            let expected = format!("p{n}");
            assert_eq!(ids(&closed.take_most_recent(|_| true)), [expected.as_str()]);
        }
        assert!(closed.take_most_recent(|_| true).is_empty());
        assert!(closed.is_empty());
    }

    #[test]
    fn a_bulk_close_is_one_entry_reopened_whole_in_its_order() {
        let mut closed = RecentlyClosed::default();
        closed.remember(vec![profile("alone")]);
        let many: Vec<ClosedTab> = (0..25).map(|n| profile(&format!("t{n:02}"))).collect();
        closed.remember(many);
        assert_eq!(
            closed.gestures().count(),
            2,
            "twenty-five tabs, one gesture"
        );
        assert_eq!(closed.most_recent_count(|_| true), 25);
        let back = closed.take_most_recent(|_| true);
        assert_eq!(back.len(), 25);
        assert_eq!(ids(&back)[0], "t00");
        assert_eq!(ids(&back)[24], "t24", "in the order they were closed");
        assert_eq!(ids(&closed.take_most_recent(|_| true)), ["alone"]);
    }

    #[test]
    fn a_gesture_whose_tabs_are_all_gone_is_skipped_for_the_one_before() {
        let mut closed = RecentlyClosed::default();
        closed.remember(vec![profile("kept"), profile("deleted")]);
        closed.remember(vec![profile("gone"), profile("deleted")]);
        let present = |tab: &ClosedTab| !matches!(name(tab), "gone" | "deleted");
        assert_eq!(closed.most_recent_count(present), 1);
        assert_eq!(ids(&closed.take_most_recent(present)), ["kept"]);
        assert!(closed.is_empty(), "the gesture all gone dropped on the way");
    }

    #[test]
    fn a_gesture_that_closed_nothing_to_reopen_is_not_remembered() {
        let mut closed = RecentlyClosed::default();
        closed.remember(Vec::new());
        assert!(closed.is_empty());
        assert_eq!(closed.most_recent_count(|_| true), 0);
        closed.remember(vec![profile("a")]);
        closed.clear();
        assert!(closed.is_empty());
    }
}
