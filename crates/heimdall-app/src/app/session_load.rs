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

//! The open sessions threshold, as the C# `ConnectionViewModel.SessionLoad.cs`: past it, a
//! banner says what the sessions cost, and nothing is refused.
//!
//! The setting was a hard limit: the session over it was not opened. Counting every pane on
//! top of that stopped an SSH user at half the limit, the SFTP pane docked beside each shell
//! counted as a session of its own. The C# made it a threshold (v2026.101301) and counts
//! connections, not panes: the SFTP pane beside an SSH shell of the same profile is none,
//! two hosts split in one tab are two.

use super::{App, TabProfile};
use crate::driver::Purpose;
use crate::ids::TabId;

/// The banner's state, as the C# `_sessionLoadThreshold`, `_sessionLoadNoticeDismissed` and
/// `IsSessionLoadNoticeShown`.
#[derive(Debug, Default)]
pub(super) struct SessionLoad {
    /// The threshold last noted; 0 until a session has been opened against one.
    threshold: u32,
    /// The banner is shown.
    shown: bool,
    /// The user hid the banner; forgotten once the count is no longer past the threshold.
    dismissed: bool,
}

/// What the open sessions banner says: the connections open, and the threshold they are
/// past.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionLoadNotice {
    /// The connections open.
    pub open: usize,
    /// The threshold set in the settings.
    pub threshold: u32,
}

impl App {
    /// The connections open, as the C# `CountEmbeddedConnections`: every tab, docked,
    /// detached or on the strip, but this computer's own shells and file browsers, the
    /// tools, and the SFTP pane docked beside a session of the same profile.
    #[must_use]
    pub fn connection_count(&self) -> usize {
        self.tabs
            .iter()
            .filter(|tab| !matches!(tab.profile, TabProfile::Local(_) | TabProfile::Tool(_)))
            .filter(|tab| !self.is_companion_pane(tab.id))
            .count()
    }

    /// Whether `tab` is an SFTP pane opened beside a session rather than a connection of
    /// its own, as the C# `IsCompanionPane`: a Files pane of a split whose other panes hold
    /// a session of the same saved profile. A Files tab alone, or a split of two hosts,
    /// still counts.
    fn is_companion_pane(&self, tab: TabId) -> bool {
        let is_sftp = |id: TabId| {
            self.tab(id).is_some_and(|tab| {
                matches!(tab.profile, TabProfile::Ssh(_)) && tab.purpose == Purpose::Files
            })
        };
        if !is_sftp(tab) {
            return false;
        }
        let Some(profile) = self.saved_profile(tab) else {
            return false;
        };
        let Some(host) = self.host_of(tab) else {
            return false;
        };
        self.panes_of(host).into_iter().any(|other| {
            other != tab && !is_sftp(other) && self.saved_profile(other).as_ref() == Some(&profile)
        })
    }

    /// Notes that one more connection is being opened, as the C# `NoteNewConnection`: when it
    /// takes the count past the threshold, the banner shows, unless the user hid it since the
    /// count last went past. Never refuses; a session opening in a tab's place takes no more
    /// room.
    pub(super) fn note_new_connection(&mut self) {
        if self.replacing {
            return;
        }
        let threshold = self.settings.max_sessions;
        self.session_load.threshold = threshold;
        if threshold == 0 || self.connection_count() < threshold_len(threshold) {
            return;
        }
        if !self.session_load.dismissed {
            self.session_load.shown = true;
        }
    }

    /// Hides the banner until the count is no longer past the threshold, as the C#
    /// `DismissSessionLoadNotice`.
    pub(super) fn hide_session_load_notice(&mut self) {
        self.session_load.shown = false;
        self.session_load.dismissed = true;
    }

    /// Takes the banner down once the connections are no longer past the threshold, and
    /// forgets that it was hidden, so going past again shows it again. The C#
    /// `RefreshSessionLoadNotice` keeps it until the count is under the threshold, its text
    /// frozen at the count it was raised with; taken down at the threshold, the banner never
    /// says "past the threshold" of a count that is not, and its text follows the count.
    pub(super) fn refresh_session_load_notice(&mut self) {
        let load = &self.session_load;
        if !load.shown && !load.dismissed {
            return;
        }
        if load.threshold == 0 || self.connection_count() > threshold_len(load.threshold) {
            return;
        }
        self.session_load.shown = false;
        self.session_load.dismissed = false;
    }

    /// The open sessions banner while it is shown and the connections open now are past the
    /// threshold: those connections, and the threshold.
    #[must_use]
    pub fn session_load_notice(&self) -> Option<SessionLoadNotice> {
        let open = self.connection_count();
        (self.session_load.shown && open > threshold_len(self.session_load.threshold)).then_some(
            SessionLoadNotice {
                open,
                threshold: self.session_load.threshold,
            },
        )
    }
}

/// `threshold` as a count of tabs.
fn threshold_len(threshold: u32) -> usize {
    usize::try_from(threshold).unwrap_or(usize::MAX)
}
