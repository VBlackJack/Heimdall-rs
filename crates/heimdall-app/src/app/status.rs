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

//! What the status bar says, as the C# Heimdall's: the state of the session shown, read
//! from it each time rather than kept; and a notice of what was just done, shown while
//! the same session is shown in the same state.

use heimdall_core::settings::BroadcastScope;

use super::{App, Phase, Tab};
use crate::ids::TabId;

/// The state of the session shown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionStatus {
    /// No session shown.
    Ready,
    /// Connecting, or asking something before it can.
    Connecting(String),
    /// Connected.
    Connected(String),
    /// Waiting to connect again.
    Reconnecting(String),
    /// Ended.
    Disconnected(String),
    /// Failed.
    Error(String),
}

/// What was just done, said while the same session is shown in the same state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Notice {
    /// This text was copied.
    Copied(String),
    /// This folder was created.
    FolderCreated(String),
    /// A transcript was started, in this file.
    TranscriptStarted(String),
    /// A transcript was stopped.
    TranscriptStopped,
    /// A transcript could not be written, for this reason, and stopped.
    TranscriptFailed(String),
    /// The server's folder at this path was bookmarked.
    Bookmarked(String),
    /// Broadcast input is on, reaching this scope.
    BroadcastOn(BroadcastScope),
    /// Broadcast input is off.
    BroadcastOff,
    /// Broadcast input will reach this scope when on.
    BroadcastScope(BroadcastScope),
}

/// The state of `tab`, named by its title.
fn tab_status(tab: &Tab) -> SessionStatus {
    let title = tab.display_title().to_owned();
    if tab.retry.is_some() {
        return SessionStatus::Reconnecting(title);
    }
    match tab.phase {
        Phase::Connected => SessionStatus::Connected(title),
        Phase::Closed { .. } => SessionStatus::Disconnected(title),
        Phase::Failed(_) => SessionStatus::Error(title),
        _ => SessionStatus::Connecting(title),
    }
}

impl App {
    /// The state of the session shown.
    #[must_use]
    pub fn session_status(&self) -> SessionStatus {
        self.active_tab().map_or(SessionStatus::Ready, tab_status)
    }

    /// The notice of what was just done, while the session shown is as it was then.
    #[must_use]
    pub fn notice(&self) -> Option<&Notice> {
        self.notice
            .as_ref()
            .filter(|(_, then)| *then == self.shown())
            .map(|(notice, _)| notice)
    }

    /// Forgets a notice once the session shown or its state changed: it is not said again
    /// on coming back.
    pub(super) fn forget_stale_notice(&mut self) {
        if self.notice.is_some() && self.notice().is_none() {
            self.notice = None;
        }
    }

    /// Says `notice` while the same session is shown in the same state.
    pub(super) fn tell(&mut self, notice: Notice) {
        self.notice = Some((notice, self.shown()));
    }

    /// The session shown, and its state.
    fn shown(&self) -> (Option<TabId>, SessionStatus) {
        (self.active, self.session_status())
    }
}
