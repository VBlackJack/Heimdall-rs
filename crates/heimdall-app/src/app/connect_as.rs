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

//! "Connect as...", as the C# Heimdall's: a profile's host opened with another protocol,
//! as a session of its own that is never saved. Its port is the protocol's default, its
//! account the profile's for the protocols that have one; no gateway, no saved password.
//! An SSH profile's SFTP is its own Files view, gateway and key included.

use std::sync::atomic::{AtomicU64, Ordering};

use heimdall_core::profile::{
    DEFAULT_RDP_PORT, DEFAULT_SSH_PORT, DEFAULT_TELNET_PORT, DEFAULT_VNC_PORT, ProfileId,
    RdpProfile, SshProfile, TelnetProfile, VncProfile,
};

use super::reconnect::Reopen;
use super::tree::{ProfileKind, ProfileSummary};
use super::{App, Effect, Message, TabProfile};
use crate::driver::Purpose;

/// A protocol "Connect as..." offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectAs {
    /// A shell.
    Ssh,
    /// A remote desktop.
    Rdp,
    /// The Files view over SSH.
    Sftp,
    /// A VNC desktop.
    Vnc,
    /// A Telnet terminal.
    Telnet,
}

impl ConnectAs {
    /// Every protocol, in the C# menu's order.
    pub const ALL: [Self; 5] = [Self::Ssh, Self::Rdp, Self::Sftp, Self::Vnc, Self::Telnet];

    /// The protocol's name, as the C# menu shows it.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Ssh => ProfileKind::Ssh.label(),
            Self::Rdp => ProfileKind::Rdp.label(),
            Self::Sftp => "SFTP",
            Self::Vnc => ProfileKind::Vnc.label(),
            Self::Telnet => ProfileKind::Telnet.label(),
        }
    }

    /// Whether it is the protocol a profile of `kind` already opens with.
    fn is(self, kind: ProfileKind) -> bool {
        matches!(
            (self, kind),
            (Self::Ssh, ProfileKind::Ssh)
                | (Self::Rdp, ProfileKind::Rdp)
                | (Self::Vnc, ProfileKind::Vnc)
                | (Self::Telnet, ProfileKind::Telnet)
        )
    }
}

/// Identifiers of sessions never saved, one per "Connect as...": never those of a profile,
/// whose identifiers are ULID-like or the C# ones.
fn transient_id() -> ProfileId {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    ProfileId::new(format!(
        "connect-as:{}",
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}

impl App {
    /// What "Connect as..." offers for profile `id`: every protocol but its own, when it
    /// has a host to connect to.
    #[must_use]
    pub fn connect_as_choices(&self, id: &ProfileId) -> Vec<ConnectAs> {
        let Some(profile) = self.profile_summary(id).filter(|p| p.endpoint.is_some()) else {
            return Vec::new();
        };
        ConnectAs::ALL
            .into_iter()
            .filter(|protocol| !protocol.is(profile.kind))
            .collect()
    }

    /// Opens profile `id`'s host with `protocol`, as a session never saved.
    pub(super) fn connect_as(&mut self, id: &ProfileId, protocol: ConnectAs) -> Vec<Effect> {
        if !self.connect_as_choices(id).contains(&protocol) {
            return Vec::new();
        }
        let Some(profile) = self.profile_summary(id) else {
            return Vec::new();
        };
        if protocol == ConnectAs::Sftp && profile.kind == ProfileKind::Ssh {
            return self.update(Message::OpenFiles(profile.id));
        }
        let Some((transient, purpose)) = transient(&profile, protocol) else {
            return Vec::new();
        };
        let effects = self.open_transient(transient.clone(), purpose);
        self.reopened_by(Reopen::Transient(Box::new(transient), purpose));
        effects
    }

    /// Opens a tab for `profile`, saved nowhere.
    pub(super) fn open_transient(&mut self, profile: TabProfile, purpose: Purpose) -> Vec<Effect> {
        match profile {
            TabProfile::Ssh(profile) => self.open_ssh(profile, purpose),
            TabProfile::Rdp(profile) => self.open_rdp_profile(profile),
            TabProfile::Telnet(profile) => self.open_telnet_profile(profile),
            TabProfile::Vnc(profile) => self.open_vnc_profile(profile),
            TabProfile::Local(shell) => self.open_local(shell),
        }
    }
}

/// The session "Connect as..." opens for `profile` with `protocol`, and what it is for.
fn transient(profile: &ProfileSummary, protocol: ConnectAs) -> Option<(TabProfile, Purpose)> {
    let (host, _) = profile.endpoint.clone()?;
    // The C# name of such a session: its host.
    let name = host.clone();
    let username = profile.username.clone().filter(|user| !user.is_empty());
    let ssh = |purpose| {
        (
            TabProfile::Ssh(SshProfile {
                id: transient_id(),
                name: name.clone(),
                group: None,
                host: host.clone(),
                port: DEFAULT_SSH_PORT,
                username: username.clone(),
                key_path: None,
                gateway: None,
            }),
            purpose,
        )
    };
    Some(match protocol {
        ConnectAs::Ssh => ssh(Purpose::Shell),
        ConnectAs::Sftp => ssh(Purpose::Files),
        ConnectAs::Rdp => (
            TabProfile::Rdp(RdpProfile {
                id: transient_id(),
                name,
                group: None,
                host,
                port: DEFAULT_RDP_PORT,
                username,
                domain: None,
                allow_tls_only: false,
                gateway: None,
                redirect_clipboard: true,
                redirect_drives: false,
            }),
            Purpose::Rdp,
        ),
        ConnectAs::Vnc => (
            TabProfile::Vnc(VncProfile {
                id: transient_id(),
                name,
                group: None,
                host,
                port: DEFAULT_VNC_PORT,
                view_only: false,
                allow_no_password: false,
            }),
            Purpose::Vnc,
        ),
        ConnectAs::Telnet => (
            TabProfile::Telnet(TelnetProfile {
                id: transient_id(),
                name,
                group: None,
                host,
                port: DEFAULT_TELNET_PORT,
            }),
            Purpose::Shell,
        ),
    })
}
