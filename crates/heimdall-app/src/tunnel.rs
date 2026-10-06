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

//! Tunnels the user opens by hand, as the C# "New tunnel" dialog: a port of this computer's
//! loopback address whose connections go, through a saved SSH gateway, to one host and port
//! (`ssh -L`). A tunnel lasts until it is closed, its gateway goes, or the application quits:
//! it is not saved.

use std::net::SocketAddr;
use std::time::SystemTime;

use heimdall_core::profile::ProfileId;

use crate::event::ConnectionEvent;

/// Lowest remote port, as the C# check.
pub const REMOTE_PORT_MIN: u16 = 1;
/// Highest port of either side.
pub const PORT_MAX: u16 = u16::MAX;
/// Lowest local port: below it, ports are the system's, as the C# check.
pub const LOCAL_PORT_MIN: u16 = 1024;
/// The remote port a new tunnel starts with, as the C# dialog.
pub const DEFAULT_REMOTE_PORT: u16 = 22;
/// The local port a new tunnel starts with, as the C# dialog.
pub const DEFAULT_LOCAL_PORT: u16 = 9090;

/// A tunnel, from its opening to its end; never given to another.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TunnelId(u64);

impl TunnelId {
    /// The identifier after `self`, for the next tunnel.
    #[must_use]
    pub(crate) fn next(self) -> Self {
        Self(self.0 + 1)
    }

    /// The raw value, for logs.
    #[must_use]
    pub fn value(self) -> u64 {
        self.0
    }
}

impl Default for TunnelId {
    fn default() -> Self {
        Self(1)
    }
}

/// A field of the "New tunnel" dialog the user types in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TunnelField {
    /// Remote host.
    RemoteHost,
    /// Remote port.
    RemotePort,
    /// Local port.
    LocalPort,
    /// Label, optional.
    Label,
}

/// What stops a tunnel from being opened, the first one found, in the C# order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TunnelProblem {
    /// No gateway is chosen.
    Gateway,
    /// No remote host is given.
    RemoteHost,
    /// The remote port is not a number from [`REMOTE_PORT_MIN`] to [`PORT_MAX`].
    RemotePort,
    /// The local port is not a number from [`LOCAL_PORT_MIN`] to [`PORT_MAX`].
    LocalPort,
    /// The local port is an open tunnel's.
    LocalPortInUse(u16),
}

/// The "New tunnel" dialog, as the user fills it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TunnelForm {
    /// The gateway to go through: the first saved one to begin with, `None` when none is.
    pub gateway: Option<ProfileId>,
    /// Remote host, as typed.
    pub remote_host: String,
    /// Remote port, as typed.
    pub remote_port: String,
    /// Local port, as typed.
    pub local_port: String,
    /// Label, as typed.
    pub label: String,
}

/// A tunnel to open: the dialog's fields, checked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TunnelSpec {
    /// The gateway it goes through.
    pub gateway: ProfileId,
    /// Where its connections go, from the gateway.
    pub remote_host: String,
    /// The port there.
    pub remote_port: u16,
    /// The port of this computer's loopback address it listens on.
    pub local_port: u16,
    /// What the user called it, if anything.
    pub label: Option<String>,
}

impl TunnelForm {
    /// A new form, on gateway `first` when there is one, with the C# defaults.
    #[must_use]
    pub fn new(first: Option<ProfileId>) -> Self {
        Self {
            gateway: first,
            remote_host: String::new(),
            remote_port: DEFAULT_REMOTE_PORT.to_string(),
            local_port: DEFAULT_LOCAL_PORT.to_string(),
            label: String::new(),
        }
    }

    /// Sets `field` to `value`.
    pub fn set(&mut self, field: TunnelField, value: String) {
        let slot = match field {
            TunnelField::RemoteHost => &mut self.remote_host,
            TunnelField::RemotePort => &mut self.remote_port,
            TunnelField::LocalPort => &mut self.local_port,
            TunnelField::Label => &mut self.label,
        };
        *slot = value;
    }

    /// The tunnel the form asks for, or the first thing wrong with it, as the C# checks
    /// in this order: the gateway, the remote host, the remote port, the local port, then
    /// whether an open tunnel already has that local port (one of `taken`).
    ///
    /// # Errors
    ///
    /// The first [`TunnelProblem`] found.
    pub fn spec(&self, taken: &[u16]) -> Result<TunnelSpec, TunnelProblem> {
        let gateway = self.gateway.clone().ok_or(TunnelProblem::Gateway)?;
        let remote_host = self.remote_host.trim();
        if remote_host.is_empty() {
            return Err(TunnelProblem::RemoteHost);
        }
        let remote_port =
            port(&self.remote_port, REMOTE_PORT_MIN).ok_or(TunnelProblem::RemotePort)?;
        let local_port = port(&self.local_port, LOCAL_PORT_MIN).ok_or(TunnelProblem::LocalPort)?;
        if taken.contains(&local_port) {
            return Err(TunnelProblem::LocalPortInUse(local_port));
        }
        let label = self.label.trim();
        Ok(TunnelSpec {
            gateway,
            remote_host: remote_host.to_owned(),
            remote_port,
            local_port,
            label: (!label.is_empty()).then(|| label.to_owned()),
        })
    }
}

/// `text` as a port from `min` to [`PORT_MAX`].
fn port(text: &str, min: u16) -> Option<u16> {
    text.trim().parse::<u16>().ok().filter(|port| *port >= min)
}

/// An open tunnel: a row of the tunnels panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tunnel {
    /// Identifier.
    pub id: TunnelId,
    /// What was asked for.
    pub spec: TunnelSpec,
    /// The gateway's name, as it was when the tunnel opened.
    pub gateway_name: String,
    /// Where it listens.
    pub local: SocketAddr,
    /// When it opened.
    pub started: SystemTime,
    /// Its gateway connection went: it listens no more. Its row stays, said interrupted as
    /// the C# one, until it is closed or opened again.
    pub interrupted: bool,
}

impl Tunnel {
    /// When it opened, in this computer's time, as the C# "Started" column: `HH:MM:SS`.
    #[must_use]
    pub fn started_clock(&self) -> String {
        clock(self.started)
    }
}

/// A session going through gateways, listed beside the tunnels as the C# lists the forward
/// it opens for one. Here the gateway carries it inside Heimdall: it has no local port, and
/// it ends with its tab, never on its own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionRoute {
    /// The tab it serves.
    pub tab: crate::ids::TabId,
    /// The tab's title.
    pub title: String,
    /// The gateways it goes through, the first hop first.
    pub route: Vec<String>,
    /// The server it reaches.
    pub remote: (String, u16),
    /// Its session failed, ended or waits to open again.
    pub interrupted: bool,
    /// When its tab opened.
    pub started: SystemTime,
}

impl SessionRoute {
    /// When its tab opened, as a tunnel's "Started" column says it.
    #[must_use]
    pub fn started_clock(&self) -> String {
        clock(self.started)
    }
}

/// `time` in this computer's time, as the C# "Started" column: `HH:MM:SS`.
fn clock(time: SystemTime) -> String {
    chrono::DateTime::<chrono::Local>::from(time)
        .format(STARTED_FORMAT)
        .to_string()
}

/// The C# "Started" column's format.
const STARTED_FORMAT: &str = "%H:%M:%S";

/// What a tunnel's attempt, then the tunnel, report.
#[derive(Debug, Clone)]
pub enum TunnelEvent {
    /// On the way through the gateway: a question, an unknown host key, a failure. Its
    /// attempt ends on anything but a question.
    Route(ConnectionEvent),
    /// The tunnel listens, there.
    Opened(SocketAddr),
    /// The tunnel ended by itself: its gateway connection is gone. Last event.
    Closed,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn filled() -> TunnelForm {
        TunnelForm {
            remote_host: "intranet.lab".to_owned(),
            ..TunnelForm::new(Some(ProfileId::new("bastion")))
        }
    }

    #[test]
    fn a_new_form_has_the_csharp_defaults_and_its_first_gateway() {
        let form = TunnelForm::new(Some(ProfileId::new("bastion")));
        assert_eq!(form.gateway, Some(ProfileId::new("bastion")));
        assert_eq!(form.remote_port, "22");
        assert_eq!(form.local_port, "9090");
        assert!(form.remote_host.is_empty() && form.label.is_empty());
    }

    #[test]
    fn a_form_filled_gives_its_tunnel_and_a_blank_label_is_none() {
        let mut form = filled();
        form.set(TunnelField::Label, "  ".to_owned());
        assert_eq!(
            form.spec(&[]),
            Ok(TunnelSpec {
                gateway: ProfileId::new("bastion"),
                remote_host: "intranet.lab".to_owned(),
                remote_port: 22,
                local_port: 9090,
                label: None,
            })
        );
        form.set(TunnelField::Label, " wiki ".to_owned());
        assert_eq!(
            form.spec(&[]).map(|spec| spec.label),
            Ok(Some("wiki".to_owned()))
        );
    }

    #[test]
    fn the_first_problem_is_said_in_the_csharp_order() {
        let mut form = TunnelForm::new(None);
        form.set(TunnelField::RemotePort, "0".to_owned());
        assert_eq!(form.spec(&[]), Err(TunnelProblem::Gateway));
        form.gateway = Some(ProfileId::new("bastion"));
        assert_eq!(form.spec(&[]), Err(TunnelProblem::RemoteHost));
        form.set(TunnelField::RemoteHost, "intranet.lab".to_owned());
        assert_eq!(form.spec(&[]), Err(TunnelProblem::RemotePort));
        form.set(TunnelField::RemotePort, "443".to_owned());
        for local in ["1023", "65536", "port", ""] {
            form.set(TunnelField::LocalPort, local.to_owned());
            assert_eq!(form.spec(&[]), Err(TunnelProblem::LocalPort), "{local:?}");
        }
        form.set(TunnelField::LocalPort, "1024".to_owned());
        assert_eq!(form.spec(&[1024]), Err(TunnelProblem::LocalPortInUse(1024)));
        assert!(form.spec(&[9090]).is_ok());
    }

    #[test]
    fn the_remote_port_takes_any_port_and_the_local_one_none_of_the_system_s() {
        let mut form = filled();
        form.set(TunnelField::RemotePort, "1".to_owned());
        form.set(TunnelField::LocalPort, "65535".to_owned());
        assert!(form.spec(&[]).is_ok());
        form.set(TunnelField::RemotePort, "65536".to_owned());
        assert_eq!(form.spec(&[]), Err(TunnelProblem::RemotePort));
    }

    #[test]
    fn tunnel_identifiers_follow_each_other() {
        let first = TunnelId::default();
        assert_ne!(first, first.next());
        assert_eq!(first.next().value(), first.value() + 1);
    }
}
