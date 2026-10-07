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

//! What goes into the shared session logs beside the transcripts, as the C# Heimdall
//! decides it: a remote desktop's connect and disconnect, RDP and VNC; a Files tab's
//! changes on its server, SFTP and FTP. Both only while session logging is on for the
//! session, read when the line is written, so the switch takes effect at once.
//!
//! A desktop's connected time is followed whatever the switch says, so a disconnect always
//! closes the time its connect opened. It ends by the server or the connection (`remote`,
//! an RDP disconnect saying why instead), by the user's Disconnect (`user`), or by its tab
//! or the application closing (`teardown`); a connection that never opened logs nothing.

use std::collections::{HashMap, HashSet};
use std::time::Instant;

use super::{App, Effect, Phase, Tab, TabProfile};
use crate::error::UiError;
use crate::ids::{AttemptId, TabId};
use crate::session_log::{EndTrigger, OperationJournal, SessionEvent, SessionEventKind};

/// An RDP desktop's protocol, as the C# writes it.
const RDP: &str = "RDP";

/// A VNC desktop's protocol, as the C# writes it.
const VNC: &str = "VNC";

/// An RDP end whose cause is not known, as the C# names it.
const RDP_UNKNOWN: &str = "RDP_UNKNOWN";

/// A desktop connected, until it ends.
#[derive(Debug)]
struct Connected {
    protocol: &'static str,
    host: String,
    title: String,
    /// The attempt that connected: another one is another session.
    attempt: AttemptId,
    since: Instant,
}

/// The desktops connected, and those the user is disconnecting.
#[derive(Debug, Default)]
pub(super) struct DesktopSessions {
    connected: HashMap<TabId, Connected>,
    by_user: HashSet<TabId>,
}

/// The protocol of `tab`'s desktop, when it is connected.
fn connected_desktop(tab: &Tab) -> Option<&'static str> {
    if tab.phase != Phase::Connected {
        return None;
    }
    match tab.profile {
        TabProfile::Rdp(_) => Some(RDP),
        TabProfile::Vnc(_) => Some(VNC),
        _ => None,
    }
}

/// The C# key of why the server or the connection ended `tab`'s RDP session.
fn rdp_reason(tab: Option<&Tab>) -> &'static str {
    let Some(tab) = tab else {
        return RDP_UNKNOWN;
    };
    let ending = match (&tab.end_reason, &tab.phase) {
        (Some(ending), _) | (None, Phase::Failed(UiError::RdpEnded { ending })) => ending,
        (None, Phase::Failed(UiError::ConnectionLost | UiError::Network { .. })) => {
            return "RDP_NETWORK_ERROR";
        }
        (None, Phase::Closed { .. }) => return "RDP_NO_INFO",
        _ => return RDP_UNKNOWN,
    };
    match ending {
        heimdall_rdp::Ending::Logoff => "RDP_USER_LOGOFF",
        heimdall_rdp::Ending::AdminDisconnect => "RDP_ADMIN_DISCONNECT",
        heimdall_rdp::Ending::BadCredentials => "RDP_BAD_CREDENTIALS",
        heimdall_rdp::Ending::License => "RDP_LICENSING_ERROR",
        heimdall_rdp::Ending::Other(_) => RDP_UNKNOWN,
    }
}

impl App {
    /// Whether the session logs take a line for `tab`'s session: as its profile says, else
    /// as the settings say.
    fn logs_session_events(&self, tab: Option<&Tab>) -> bool {
        tab.map_or(self.settings.session_logging, |tab| self.logs_sessions(tab))
    }

    /// Writes the connects and disconnects of the desktops since the last message.
    pub(super) fn follow_desktop_sessions(&mut self) {
        let ended: Vec<TabId> = self
            .desktop_sessions
            .connected
            .iter()
            .filter(|(id, connected)| {
                !self.tab(**id).is_some_and(|tab| {
                    connected_desktop(tab).is_some() && tab.attempt == connected.attempt
                })
            })
            .map(|(id, _)| *id)
            .collect();
        for id in ended {
            let trigger = if self.tab(id).is_none() {
                EndTrigger::Teardown
            } else if self.desktop_sessions.by_user.remove(&id) {
                EndTrigger::User
            } else {
                EndTrigger::Remote
            };
            self.end_desktop_session(id, trigger);
        }
        let started: Vec<(TabId, &'static str)> = self
            .tabs
            .iter()
            .filter(|tab| !self.desktop_sessions.connected.contains_key(&tab.id))
            .filter_map(|tab| connected_desktop(tab).map(|protocol| (tab.id, protocol)))
            .collect();
        for (id, protocol) in started {
            let Some(tab) = self.tab(id) else {
                continue;
            };
            let connected = Connected {
                protocol,
                host: tab
                    .profile
                    .endpoint()
                    .map(|(host, _)| host.to_owned())
                    .unwrap_or_default(),
                title: tab.display_title().to_owned(),
                attempt: tab.attempt,
                since: Instant::now(),
            };
            if self.logs_session_events(Some(tab)) {
                self.write_session_event(&SessionEvent {
                    protocol,
                    kind: SessionEventKind::Connected,
                    host: connected.host.clone(),
                    title: Some(connected.title.clone()),
                    reason: None,
                    duration: None,
                    end_trigger: None,
                });
            }
            self.desktop_sessions.connected.insert(id, connected);
        }
        let connected = &self.desktop_sessions.connected;
        self.desktop_sessions
            .by_user
            .retain(|id| connected.contains_key(id));
    }

    /// `tab`'s desktop is being disconnected by the user.
    pub(super) fn desktop_disconnected_by_user(&mut self, tab: TabId) {
        if self.desktop_sessions.connected.contains_key(&tab) {
            self.desktop_sessions.by_user.insert(tab);
        }
    }

    /// The application is closing: every desktop still connected ends with it, and the
    /// session logs are written before it goes.
    pub(super) fn close_session_logs(&mut self) {
        let open: Vec<TabId> = self.desktop_sessions.connected.keys().copied().collect();
        for id in open {
            self.end_desktop_session(id, EndTrigger::Teardown);
        }
        self.session_logs.sync();
    }

    /// Writes the end of `id`'s connected time: an RDP session the server or the connection
    /// ended says why, any other end what ended it.
    fn end_desktop_session(&mut self, id: TabId, trigger: EndTrigger) {
        let Some(connected) = self.desktop_sessions.connected.remove(&id) else {
            return;
        };
        let tab = self.tab(id);
        if !self.logs_session_events(tab) {
            return;
        }
        let (reason, end_trigger) = match (connected.protocol, trigger) {
            (RDP, EndTrigger::Remote) => (Some(rdp_reason(tab).to_owned()), None),
            _ => (None, Some(trigger)),
        };
        self.write_session_event(&SessionEvent {
            protocol: connected.protocol,
            kind: SessionEventKind::Disconnected,
            host: connected.host,
            title: Some(connected.title),
            reason,
            duration: Some(connected.since.elapsed()),
            end_trigger,
        });
    }

    fn write_session_event(&self, event: &SessionEvent) {
        let folder = self.settings.session_log_folder(&self.settings_file);
        self.session_logs.event(&folder, event);
    }

    /// Where the work `effect` hands out records the changes it makes on a server: the
    /// operations log, when `effect` changes a Files tab's server and session logging is on
    /// for the tab. Read as the work starts, as the C# reads its switch.
    #[must_use]
    pub fn operation_journal(&self, effect: &Effect) -> Option<OperationJournal> {
        let tab = match effect {
            Effect::Transfer { tab, .. }
            | Effect::FileOperation { tab, .. }
            | Effect::FileBatchStep { tab, .. }
            | Effect::MoveRemote { tab, .. }
            | Effect::CopyRemote { tab, .. } => *tab,
            _ => return None,
        };
        let tab = self.tab(tab)?;
        if !self.logs_sessions(tab) {
            return None;
        }
        let client = tab.files.as_ref()?.client.as_ref()?;
        let (host, _) = tab.profile.endpoint()?;
        Some(OperationJournal::new(
            self.session_logs.clone(),
            self.settings.session_log_folder(&self.settings_file),
            client,
            host,
        ))
    }

    /// Waits until every line given to the session logs is written: for a caller about to
    /// read them.
    pub fn sync_session_logs(&self) {
        self.session_logs.sync();
    }
}
