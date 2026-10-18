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

//! The network scanner dialog of Ctrl+Shift+N, as the C# `NetworkScannerService`
//! (`NetworkScannerService.cs:56-150`, opened by `MainWindow.xaml.cs:1958`): the subnet
//! typed, swept for hosts, the hosts listed with their open remote-access ports, then the
//! ones chosen added to the sessions.
//!
//! Where the C# asks one yes or no and adds every host found, one session each with the
//! kind its ports suggest, this lists them to choose from. A host whose address or name is
//! already saved for the same kind of session and port is shown as such and not added a
//! second time, and a host with no remote-access port is listed but not offered, where the
//! C# files it as an SSH session that cannot connect. No credential is created: the sessions
//! ask for theirs when they connect. They are filed in the folder the C# uses.

use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use heimdall_core::import::csharp::ImportReport;
use heimdall_core::profile::ProfileId;
use heimdall_core::tools::network_scanner::{
    self, DISCOVERED_FOLDER, PORT_HTTP, RemoteKind, ScanEvent, ScannedHost, Suggestion, TargetError,
};

use super::{App, Dialog, Effect, ProfileKind};
use crate::profile_draft::{
    DraftProfile, DraftProtocol, ProfileDraft, ProfileField, ProfileToggle,
};

/// A host of the scan and what is made of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkScanRow {
    /// What the sweep found.
    pub host: ScannedHost,
    /// The session its open ports suggest, none when it has no remote-access port.
    pub suggestion: Option<Suggestion>,
    /// A session of that kind is already saved for it.
    pub duplicate: bool,
    /// Chosen to be added.
    pub chosen: bool,
}

impl NetworkScanRow {
    /// Whether it can be chosen: it suggests a session, and none is saved for it yet.
    #[must_use]
    pub fn choosable(&self) -> bool {
        self.suggestion.is_some() && !self.duplicate
    }
}

/// The scanner dialog, as filled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkScan {
    /// The subnet or address typed.
    pub target: String,
    /// A sweep is going on.
    pub running: bool,
    /// Addresses swept and addresses to sweep, of the last sweep.
    pub progress: (usize, usize),
    /// Why the target was refused.
    pub error: Option<TargetError>,
    /// The hosts that answered, by address.
    pub rows: Vec<NetworkScanRow>,
    /// A sweep has ended, stopped or not: "no host answered" can be said.
    pub swept: bool,
}

impl NetworkScan {
    fn new() -> Self {
        Self {
            target: network_scanner::DEFAULT_SUBNET.to_owned(),
            running: false,
            progress: (0, 0),
            error: None,
            rows: Vec::new(),
            swept: false,
        }
    }

    /// How many hosts are chosen.
    #[must_use]
    pub fn chosen(&self) -> usize {
        self.rows
            .iter()
            .filter(|row| row.chosen && row.choosable())
            .count()
    }

    /// Whether the chosen hosts can be added now.
    #[must_use]
    pub fn can_add(&self) -> bool {
        !self.running && self.chosen() > 0
    }
}

/// What the scanner dialog is told.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetworkScanMessage {
    /// Open the dialog, as Ctrl+Shift+N does.
    Open,
    /// The target typed.
    TargetChanged(String),
    /// Sweep the target.
    Start,
    /// Stop the sweep.
    Stop,
    /// Sweep `scan` told of a host or of its progress.
    Event {
        /// Which sweep.
        scan: u64,
        /// What it told.
        event: ScanEvent,
    },
    /// Sweep `scan` is over.
    Ended(u64),
    /// Choose or unchoose the host in row `.0`.
    Choose(usize),
    /// Choose every host that can be, or none.
    ChooseAll(bool),
    /// Add the hosts chosen to the sessions.
    Add,
}

/// A saved session reaching `host`: what the duplicate check compares.
fn kind_of(kind: ProfileKind) -> Option<RemoteKind> {
    match kind {
        ProfileKind::Ssh | ProfileKind::Sftp => Some(RemoteKind::Ssh),
        ProfileKind::Rdp => Some(RemoteKind::Rdp),
        ProfileKind::Vnc => Some(RemoteKind::Vnc),
        ProfileKind::WinRm => Some(RemoteKind::WinRm),
        ProfileKind::Telnet => Some(RemoteKind::Telnet),
        ProfileKind::Web => Some(RemoteKind::Web),
        ProfileKind::Ftp | ProfileKind::Local | ProfileKind::Citrix | ProfileKind::Tool => None,
    }
}

/// The protocol of the form a session of `kind` is made with.
fn protocol_of(kind: RemoteKind) -> DraftProtocol {
    match kind {
        RemoteKind::Rdp => DraftProtocol::Rdp,
        RemoteKind::Ssh => DraftProtocol::Ssh,
        RemoteKind::Vnc => DraftProtocol::Vnc,
        RemoteKind::WinRm => DraftProtocol::WinRm,
        RemoteKind::Telnet => DraftProtocol::Telnet,
        RemoteKind::Web => DraftProtocol::Web,
    }
}

/// The session for `host` as `suggestion` says, filed in the discovered folder, with no
/// account and no secret; `None` when the form refuses it.
fn profile_for(host: &ScannedHost, suggestion: Suggestion, id: ProfileId) -> Option<DraftProfile> {
    let mut draft = ProfileDraft::new_for(protocol_of(suggestion.kind));
    draft.set(ProfileField::Name, host.display_name());
    draft.set(ProfileField::Group, DISCOVERED_FOLDER.to_owned());
    draft.set(ProfileField::Host, host.ip.to_string());
    draft.set(ProfileField::Port, suggestion.port.to_string());
    if suggestion.kind == RemoteKind::Web && suggestion.port == PORT_HTTP {
        draft.toggle(ProfileToggle::PlainHttp, true);
    }
    draft.to_saved(id).ok()
}

impl App {
    /// Applies a message about the network scanner dialog.
    pub(super) fn network_scan_message(&mut self, message: NetworkScanMessage) -> Vec<Effect> {
        match message {
            NetworkScanMessage::Open => {
                if self.dialog.is_none() {
                    self.dialog = Some(Dialog::NetworkScan(Box::new(NetworkScan::new())));
                }
                Vec::new()
            }
            NetworkScanMessage::TargetChanged(typed) => {
                if let Some(scan) = self.scan_dialog_mut() {
                    scan.target = typed;
                    scan.error = None;
                }
                Vec::new()
            }
            NetworkScanMessage::Start => self.start_network_scan(),
            NetworkScanMessage::Stop => {
                self.stop_network_scan();
                Vec::new()
            }
            NetworkScanMessage::Event { scan, event } => {
                if self.network_scan.as_ref().map(|(running, _)| *running) == Some(scan) {
                    self.network_scan_event(event);
                }
                Vec::new()
            }
            NetworkScanMessage::Ended(scan) => {
                if self.network_scan.as_ref().map(|(running, _)| *running) == Some(scan) {
                    self.network_scan = None;
                    if let Some(dialog) = self.scan_dialog_mut() {
                        dialog.running = false;
                        dialog.swept = true;
                    }
                }
                Vec::new()
            }
            NetworkScanMessage::Choose(at) => {
                if let Some(row) = self
                    .scan_dialog_mut()
                    .and_then(|scan| scan.rows.get_mut(at))
                    .filter(|row| row.choosable())
                {
                    row.chosen = !row.chosen;
                }
                Vec::new()
            }
            NetworkScanMessage::ChooseAll(on) => {
                if let Some(scan) = self.scan_dialog_mut() {
                    for row in scan.rows.iter_mut().filter(|row| row.choosable()) {
                        row.chosen = on;
                    }
                }
                Vec::new()
            }
            NetworkScanMessage::Add => {
                self.add_scanned_hosts();
                Vec::new()
            }
        }
    }

    fn scan_dialog_mut(&mut self) -> Option<&mut NetworkScan> {
        match &mut self.dialog {
            Some(Dialog::NetworkScan(scan)) => Some(scan),
            _ => None,
        }
    }

    /// Stops the sweep going on, if any, its probes and its programs ended.
    pub(super) fn stop_network_scan(&mut self) {
        if let Some((_, stop)) = self.network_scan.take() {
            stop.store(true, Ordering::Relaxed);
        }
        if let Some(scan) = self.scan_dialog_mut() {
            scan.running = false;
            scan.swept = true;
        }
    }

    /// Starts a sweep of the target typed, when it reads and none is going on.
    fn start_network_scan(&mut self) -> Vec<Effect> {
        let Some(scan) = self.scan_dialog_mut() else {
            return Vec::new();
        };
        if scan.running {
            return Vec::new();
        }
        let ips = match network_scanner::parse_target(&scan.target) {
            Ok(ips) => ips,
            Err(error) => {
                scan.error = Some(error);
                return Vec::new();
            }
        };
        scan.error = None;
        scan.running = true;
        scan.swept = false;
        scan.progress = (0, ips.len());
        scan.rows.clear();
        let run = self.next_network_scan;
        self.next_network_scan += 1;
        let stop = Arc::new(AtomicBool::new(false));
        self.network_scan = Some((run, Arc::clone(&stop)));
        vec![Effect::ScanNetwork {
            scan: run,
            ips,
            stop,
        }]
    }

    /// Shows what a sweep told.
    fn network_scan_event(&mut self, event: ScanEvent) {
        let saved: Vec<(String, u16, RemoteKind)> = self
            .profile_summaries()
            .into_iter()
            .filter_map(|profile| {
                let (host, port) = profile.endpoint?;
                Some((host, port, kind_of(profile.kind)?))
            })
            .collect();
        let Some(scan) = self.scan_dialog_mut() else {
            return;
        };
        match event {
            ScanEvent::Progress { done, total } => scan.progress = (done, total),
            ScanEvent::Host(host) => {
                let suggestion = host.suggestion();
                let duplicate =
                    suggestion.is_some_and(|suggestion| is_saved(&saved, &host, suggestion));
                scan.rows.push(NetworkScanRow {
                    chosen: suggestion.is_some() && !duplicate,
                    host,
                    suggestion,
                    duplicate,
                });
            }
        }
    }

    /// Adds the hosts chosen as sessions, in one save, and says what it did.
    fn add_scanned_hosts(&mut self) {
        let Some(scan) = self.scan_dialog_mut() else {
            return;
        };
        if !scan.can_add() {
            return;
        }
        let chosen: Vec<(ScannedHost, Suggestion)> = scan
            .rows
            .iter()
            .filter(|row| row.chosen && row.choosable())
            .filter_map(|row| Some((row.host.clone(), row.suggestion?)))
            .collect();
        let mut report = ImportReport::default();
        let mut taken: HashSet<ProfileId> = HashSet::new();
        for (host, suggestion) in chosen {
            let id = loop {
                let id = self.fresh_id();
                if taken.insert(id.clone()) {
                    break id;
                }
            };
            match profile_for(&host, suggestion, id) {
                Some(DraftProfile::Ssh(profile)) => report.profiles.push(profile),
                Some(DraftProfile::Rdp(profile)) => report.rdp.push(profile),
                Some(DraftProfile::Vnc(profile)) => report.vnc.push(profile),
                Some(DraftProfile::WinRm(profile)) => report.winrm.push(profile),
                Some(DraftProfile::Telnet(profile)) => report.telnet.push(profile),
                Some(DraftProfile::Web(profile)) => report.web.push(profile),
                Some(DraftProfile::Local(_) | DraftProfile::Ftp(_) | DraftProfile::Citrix(_))
                | None => {}
            }
        }
        self.stop_network_scan();
        self.dialog = None;
        if let Some(summary) = self.merge_import(report) {
            self.dialog = Some(Dialog::ScanAdded(summary));
        }
    }
}

/// Whether a session of `suggestion`'s kind and port already reaches `host`, by its address
/// or its name, whatever the case.
fn is_saved(
    saved: &[(String, u16, RemoteKind)],
    host: &ScannedHost,
    suggestion: Suggestion,
) -> bool {
    let address = host.ip.to_string();
    saved.iter().any(|(saved_host, port, kind)| {
        *kind == suggestion.kind
            && *port == suggestion.port
            && (saved_host.eq_ignore_ascii_case(&address)
                || host
                    .hostname
                    .as_deref()
                    .is_some_and(|name| saved_host.eq_ignore_ascii_case(name)))
    })
}
