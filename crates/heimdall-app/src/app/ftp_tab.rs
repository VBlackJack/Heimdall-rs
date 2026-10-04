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

//! FTP tabs: a Files tab on an FTP or explicit FTPS server, as the C# FTP profile opens.

use std::path::PathBuf;

use heimdall_core::profile::{FtpProfile, ProfileId};
use heimdall_rdp::Fingerprint;
use tokio_util::sync::CancellationToken;

use super::{App, Effect, Phase, Tab, TabProfile};
use crate::driver::Purpose;
use crate::files::FilesPane;
use crate::ftp_driver::FtpRequest;
use crate::ids::{AttemptId, TabId};

/// Name of the file of the FTPS servers the user trusts, beside the SSH `known_hosts`.
const KNOWN_FTPS_HOSTS_FILE_NAME: &str = "known_ftps_hosts";

impl App {
    /// The file of the FTPS servers the user trusts, beside the SSH one.
    pub(super) fn known_ftps_hosts(&self) -> PathBuf {
        self.config
            .known_hosts
            .with_file_name(KNOWN_FTPS_HOSTS_FILE_NAME)
    }

    /// Opens an FTP tab for a saved profile.
    pub(super) fn open_ftp(&mut self, id: &ProfileId) -> Vec<Effect> {
        let Some(profile) = self
            .store
            .ftp_profiles()
            .iter()
            .find(|p| &p.id == id)
            .cloned()
        else {
            return Vec::new();
        };
        self.open_ftp_profile(profile)
    }

    /// Opens an FTP tab for `profile`.
    pub(super) fn open_ftp_profile(&mut self, profile: FtpProfile) -> Vec<Effect> {
        let tab_id = TabId::fresh();
        let attempt = AttemptId::fresh();
        let cancel = CancellationToken::new();
        let request = self.ftp_request(&profile, None, cancel.clone());
        let mut tab = Tab::new(
            self.terminal_palette(),
            tab_id,
            TabProfile::Ftp(profile),
            Purpose::Files,
            self.viewport,
            attempt,
            cancel,
        );
        tab.files = Some(Box::new(FilesPane::new(self.files_start())));
        self.tabs.push(tab);
        self.active = Some(tab_id);
        vec![Effect::ConnectFtp {
            tab: tab_id,
            attempt,
            request: Box::new(request),
        }]
    }

    /// What connecting to `profile` needs.
    fn ftp_request(
        &self,
        profile: &FtpProfile,
        accepted: Option<Fingerprint>,
        cancel: CancellationToken,
    ) -> FtpRequest {
        FtpRequest {
            profile: profile.clone(),
            known_hosts: self.known_ftps_hosts(),
            accepted,
            trusted_for_run: self.certificates_trusted_for_run(&profile.host, profile.port),
            cancel,
        }
    }

    /// Connects an FTP tab again, with a key the user just accepted if any.
    pub(super) fn reconnect_ftp(
        &mut self,
        tab_id: TabId,
        accepted: Option<Fingerprint>,
    ) -> Vec<Effect> {
        let Some(TabProfile::Ftp(profile)) = self.tab(tab_id).map(|tab| tab.profile.clone()) else {
            return Vec::new();
        };
        let attempt = AttemptId::fresh();
        let cancel = CancellationToken::new();
        let request = self.ftp_request(&profile, accepted, cancel.clone());
        let Some(tab) = self.tab_mut(tab_id) else {
            return Vec::new();
        };
        tab.attempt = attempt;
        tab.cancel = cancel;
        tab.phase = Phase::Connecting;
        vec![Effect::ConnectFtp {
            tab: tab_id,
            attempt,
            request: Box::new(request),
        }]
    }
}
