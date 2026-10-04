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

//! Which sessions keep a transcript, as the C# Heimdall decides: every SSH, Telnet and local
//! session from when it connects, when session logging is on or its profile says so, unless
//! its profile says not; `WinRM` only by hand, its `PowerShell` host able to echo what sets
//! it up. Any session of text by hand, from its tab's menu; each ends with its session.

use std::time::SystemTime;

use super::reconnect::Reopen;
use super::{App, Notice, Phase, ProfileKind, Tab, TabProfile};
use crate::driver::Purpose;
use crate::ids::TabId;
use crate::transcript::{TRANSCRIPT_MAX_BYTES, Transcript, TranscriptContext, TranscriptLines};

/// The host a local session's transcript is named after.
const LOCAL_HOST: &str = "localhost";

impl App {
    /// Words the transcripts' first and last lines, in the window's language.
    pub fn set_transcript_lines(&mut self, lines: TranscriptLines) {
        self.transcript_lines = Some(lines);
    }

    /// Whether a transcript can be started for `tab` by hand: a session of text, connected,
    /// keeping none yet.
    #[must_use]
    pub fn can_start_transcript(&self, tab: &Tab) -> bool {
        tab.phase == Phase::Connected
            && tab.purpose == Purpose::Shell
            && tab.desktop.is_none()
            && tab.files.is_none()
            && tab.transcript.is_none()
    }

    /// After `tab_id`'s connection moved on, `was_connected` before: a session just
    /// connected starts its transcript when session logging says so; one no longer
    /// connected ends its own.
    pub(super) fn follow_transcript(&mut self, tab_id: TabId, was_connected: bool) {
        let Some(tab) = self.tab(tab_id) else {
            return;
        };
        if tab.phase != Phase::Connected {
            self.end_transcript(tab_id, false);
        } else if !was_connected
            && self.logs_sessions(tab)
            && matches!(
                self.tab_kind(tab),
                ProfileKind::Ssh | ProfileKind::Telnet | ProfileKind::Local
            )
        {
            self.start_transcript(tab_id, false);
        }
    }

    /// Whether `tab`'s session keeps a transcript from when it connects: as its profile
    /// says, else as the settings say, as the C# `SessionLoggingOverride`.
    fn logs_sessions(&self, tab: &Tab) -> bool {
        let chosen = match &tab.profile {
            TabProfile::Ssh(profile) => profile.session_logging,
            TabProfile::Telnet(profile) => profile.session_logging,
            // A local shell's tab keeps what it runs, not its profile: the profile it opened
            // from says.
            TabProfile::Local(_) => match &tab.reopen {
                Reopen::Profile(id) => self
                    .store
                    .local_profiles()
                    .iter()
                    .find(|profile| profile.id == *id)
                    .and_then(|profile| profile.session_logging),
                Reopen::Shell(_) | Reopen::Transient(..) => None,
            },
            _ => None,
        };
        chosen.unwrap_or(self.settings.session_logging)
    }

    /// Starts `tab_id`'s transcript in the folder of the settings; `told` says where.
    pub(super) fn start_transcript(&mut self, tab_id: TabId, told: bool) {
        let Some(tab) = self
            .tab(tab_id)
            .filter(|tab| self.can_start_transcript(tab))
        else {
            return;
        };
        let context = TranscriptContext {
            protocol: self.tab_kind(tab).label().to_uppercase(),
            host: tab
                .profile
                .endpoint()
                .map_or(LOCAL_HOST, |(host, _)| host)
                .to_owned(),
            title: tab.display_title().to_owned(),
            started: SystemTime::now(),
        };
        let folder = self.settings.session_log_folder(&self.settings_file);
        match Transcript::start(
            &folder,
            &context,
            self.transcript_lines.as_ref(),
            TRANSCRIPT_MAX_BYTES,
        ) {
            Ok(transcript) => {
                let path = transcript.path().display().to_string();
                if let Some(tab) = self.tab_mut(tab_id) {
                    tab.transcript = Some(transcript);
                }
                if told {
                    self.tell(Notice::TranscriptStarted(path));
                }
            }
            Err(error) => self.tell(Notice::TranscriptFailed(error.to_string())),
        }
    }

    /// Ends `tab_id`'s transcript with its footer; `told` says so.
    pub(super) fn end_transcript(&mut self, tab_id: TabId, told: bool) {
        let Some(mut transcript) = self.tab_mut(tab_id).and_then(|tab| tab.transcript.take())
        else {
            return;
        };
        match transcript.finish(SystemTime::now()) {
            Ok(()) if told => self.tell(Notice::TranscriptStopped),
            Ok(()) => {}
            Err(error) => self.tell(Notice::TranscriptFailed(error.to_string())),
        }
    }

    /// Adds `bytes`, output of `tab_id`, to its transcript; one that cannot be written
    /// stops, and says so.
    pub(super) fn record(&mut self, tab_id: TabId, bytes: &[u8]) {
        let Some(tab) = self.tab_mut(tab_id) else {
            return;
        };
        let Some(transcript) = tab.transcript.as_mut() else {
            return;
        };
        if let Err(error) = transcript.write(bytes) {
            // Dropped: it tries its footer, and a failure then is this one again.
            tab.transcript = None;
            self.tell(Notice::TranscriptFailed(error.to_string()));
        }
    }
}
