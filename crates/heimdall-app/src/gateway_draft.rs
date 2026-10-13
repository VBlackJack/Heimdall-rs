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

//! An SSH gateway as typed into its dialog, the C# Heimdall's gateway dialog: name, host,
//! port, username, key, and the gateway it is reached through; then, read only, the
//! fingerprint its host key is trusted by.

use std::path::PathBuf;
use std::time::SystemTime;

use heimdall_core::profile::{ProfileId, SshGateway};
use heimdall_ssh::Step;

use crate::profile_draft::{DEFAULT_SSH_PORT, DraftError, ProfileField, SavedSecret, host};

/// A field of the gateway dialog; the profile form's names, as the fields are the same.
pub const GATEWAY_FIELDS: [ProfileField; 5] = [
    ProfileField::Name,
    ProfileField::Host,
    ProfileField::Port,
    ProfileField::Username,
    ProfileField::KeyPath,
];

/// What the gateway dialog holds, as typed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GatewayDraft {
    /// The gateway being edited; `None` for a new one.
    pub editing: Option<ProfileId>,
    /// Name.
    pub name: String,
    /// Host.
    pub host: String,
    /// Port.
    pub port: String,
    /// Account.
    pub username: String,
    /// Private key file.
    pub key_path: String,
    /// The gateway this one is reached through.
    pub parent: Option<ProfileId>,
    /// A password is saved for the gateway: the dialog says so, its field stays empty.
    pub password_saved: bool,
    /// The saved password is to be removed when the dialog is saved.
    pub clear_password: bool,
    /// The key passphrase saved for the gateway, as the dialog shows it.
    pub passphrase: SavedSecret,
    /// "Host Key Fingerprint", read only as in the C# dialog: what the trust files held for
    /// the gateway's host and port when the dialog opened, empty when nothing. Never saved:
    /// the trust files are its one source.
    pub trusted_fingerprint: String,
    /// "Test route": the destination to reach through the gateway, as typed; empty tests
    /// the gateways only.
    pub target_host: String,
    /// The destination's TCP port, as typed.
    pub target_port: String,
    /// "Test route": not run, refused, running, or what it found.
    pub route_test: RouteTest,
}

/// The gateway dialog's "Test route", as the C# card.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum RouteTest {
    /// Not run since the route last changed.
    #[default]
    Idle,
    /// Not run: what is wrong with the form.
    Refused(RouteProblem),
    /// Running: the steps ended so far.
    Running(Vec<Step>),
    /// Ended: its steps, and when it ended.
    Done {
        /// The steps, in order.
        steps: Vec<Step>,
        /// When it ended.
        at: SystemTime,
    },
}

/// Why "Test route" did not run, as the C# card says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteProblem {
    /// The gateway, or its chain, is not a route.
    Route,
    /// The destination is not a host and a port.
    Target,
}

/// A field of the "Test route" card.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetField {
    /// The destination's host.
    Host,
    /// Its port.
    Port,
}

impl Default for GatewayDraft {
    /// A new gateway, on the SSH port as the C# dialog starts one.
    fn default() -> Self {
        Self {
            editing: None,
            name: String::new(),
            host: String::new(),
            port: DEFAULT_SSH_PORT.to_string(),
            username: String::new(),
            key_path: String::new(),
            parent: None,
            password_saved: false,
            clear_password: false,
            passphrase: SavedSecret::Absent,
            trusted_fingerprint: String::new(),
            target_host: String::new(),
            target_port: DEFAULT_SSH_PORT.to_string(),
            route_test: RouteTest::Idle,
        }
    }
}

impl GatewayDraft {
    /// The dialog filled from a saved gateway.
    #[must_use]
    pub fn from_gateway(gateway: &SshGateway) -> Self {
        Self {
            editing: Some(gateway.id.clone()),
            name: gateway.name.clone(),
            host: gateway.host.clone(),
            port: gateway.port.to_string(),
            username: gateway.username.clone().unwrap_or_default(),
            key_path: gateway
                .key_path
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_default(),
            parent: gateway.parent.clone(),
            ..Self::default()
        }
    }

    /// The text of `field`; empty for a field the dialog does not have.
    #[must_use]
    pub fn value(&self, field: ProfileField) -> &str {
        match field {
            ProfileField::Name => &self.name,
            ProfileField::Host => &self.host,
            ProfileField::Port => &self.port,
            ProfileField::Username => &self.username,
            ProfileField::KeyPath => &self.key_path,
            ProfileField::Group
            | ProfileField::StoreFrontUrl
            | ProfileField::AppName
            | ProfileField::IcaFile
            | ProfileField::Domain
            | ProfileField::FixedWidth
            | ProfileField::FixedHeight
            | ProfileField::ResizeDelay
            | ProfileField::VaultEntry
            | ProfileField::SocksPort
            | ProfileField::RemoteBindPort
            | ProfileField::RemoteLocalPort
            | ProfileField::LocalTunnelPort
            | ProfileField::RdGateway
            | ProfileField::LocalProgram
            | ProfileField::LocalArguments
            | ProfileField::WorkingDirectory
            | ProfileField::Tags
            | ProfileField::MacAddress => "",
        }
    }

    /// Replaces the text of `field`; a field the dialog does not have is ignored.
    pub fn set(&mut self, field: ProfileField, value: String) {
        // What the test found was about the route it walked, not this one.
        self.forget_route_test();
        match field {
            ProfileField::Name => self.name = value,
            ProfileField::Host => self.host = value,
            ProfileField::Port => self.port = value,
            ProfileField::Username => self.username = value,
            ProfileField::KeyPath => self.key_path = value,
            ProfileField::Group
            | ProfileField::StoreFrontUrl
            | ProfileField::AppName
            | ProfileField::IcaFile
            | ProfileField::Domain
            | ProfileField::FixedWidth
            | ProfileField::FixedHeight
            | ProfileField::ResizeDelay
            | ProfileField::VaultEntry
            | ProfileField::SocksPort
            | ProfileField::RemoteBindPort
            | ProfileField::RemoteLocalPort
            | ProfileField::LocalTunnelPort
            | ProfileField::RdGateway
            | ProfileField::LocalProgram
            | ProfileField::LocalArguments
            | ProfileField::WorkingDirectory
            | ProfileField::Tags
            | ProfileField::MacAddress => {}
        }
    }

    /// Replaces the text of the "Test route" card's `field`.
    pub fn set_target(&mut self, field: TargetField, value: String) {
        self.forget_route_test();
        match field {
            TargetField::Host => self.target_host = value,
            TargetField::Port => self.target_port = value,
        }
    }

    /// Clears what "Test route" found, the route or what signs in to it having changed; a
    /// test running goes on.
    pub fn forget_route_test(&mut self) {
        if !matches!(self.route_test, RouteTest::Running(_)) {
            self.route_test = RouteTest::Idle;
        }
    }

    /// The destination "Test route" reaches through the gateway: `None` when none is typed.
    ///
    /// # Errors
    ///
    /// [`RouteProblem::Target`] when a host is typed and the host or the port is not valid.
    pub fn target(&self) -> Result<Option<(String, u16)>, RouteProblem> {
        if self.target_host.trim().is_empty() {
            return Ok(None);
        }
        let host = host(&self.target_host).map_err(|_| RouteProblem::Target)?;
        let port = self
            .target_port
            .trim()
            .parse::<u16>()
            .ok()
            .filter(|port| *port != 0)
            .ok_or(RouteProblem::Target)?;
        Ok(Some((host, port)))
    }

    /// The gateway this dialog describes, under `id`. As in the C# dialog, the name, host
    /// and username are required: a gateway is an account on a server.
    ///
    /// # Errors
    ///
    /// The first [`DraftError`] in dialog order.
    pub fn to_gateway(&self, id: ProfileId) -> Result<SshGateway, DraftError> {
        let name = self.name.trim();
        if name.is_empty() {
            return Err(DraftError::NameMissing);
        }
        let key_path = self.key_path.trim();
        if [name, key_path]
            .iter()
            .any(|text| text.chars().any(char::is_control))
        {
            return Err(DraftError::ControlCharacter);
        }
        let host = host(&self.host)?;
        let port = match self.port.trim() {
            "" => DEFAULT_SSH_PORT,
            typed => typed
                .parse::<u16>()
                .ok()
                .filter(|port| *port != 0)
                .ok_or(DraftError::PortInvalid)?,
        };
        let username = self.username.trim();
        if username.is_empty() {
            return Err(DraftError::UsernameMissing);
        }
        if username
            .chars()
            .any(|c| c.is_whitespace() || c.is_control())
        {
            return Err(DraftError::UsernameInvalid);
        }
        // Reached through itself is no route; a longer loop is refused on save.
        let parent = self.parent.clone().filter(|parent| *parent != id);
        Ok(SshGateway {
            id,
            name: name.to_owned(),
            host,
            port,
            username: Some(username.to_owned()),
            key_path: (!key_path.is_empty()).then(|| PathBuf::from(key_path)),
            parent,
        })
    }

    /// The dialog showing `trusted`, the fingerprint the trust files hold for the gateway's
    /// host and port, or nothing.
    #[must_use]
    pub fn with_trusted_fingerprint(mut self, trusted: Option<String>) -> Self {
        self.trusted_fingerprint = trusted.unwrap_or_default();
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft() -> GatewayDraft {
        GatewayDraft {
            name: " bastion ".to_owned(),
            host: " bastion.lab ".to_owned(),
            username: " jump ".to_owned(),
            ..GatewayDraft::default()
        }
    }

    #[test]
    fn a_gateway_starts_on_port_22_and_reads_back_from_its_dialog() {
        assert_eq!(GatewayDraft::default().port, "22");
        let gateway = draft().to_gateway(ProfileId::new("g")).expect("valid");
        assert_eq!(gateway.name, "bastion");
        assert_eq!(gateway.host, "bastion.lab");
        assert_eq!(gateway.port, 22);
        assert_eq!(gateway.username.as_deref(), Some("jump"));
        let parented = SshGateway {
            parent: Some(ProfileId::new("outer")),
            key_path: Some(PathBuf::from("/k")),
            ..gateway
        };
        assert_eq!(
            GatewayDraft::from_gateway(&parented).to_gateway(ProfileId::new("g")),
            Ok(parented)
        );
    }

    #[test]
    fn name_host_port_and_username_are_checked_as_in_the_csharp_dialog() {
        let check = |change: fn(&mut GatewayDraft)| {
            let mut form = draft();
            change(&mut form);
            form.to_gateway(ProfileId::new("g")).err()
        };
        assert_eq!(check(|f| f.name.clear()), Some(DraftError::NameMissing));
        assert_eq!(check(|f| f.host.clear()), Some(DraftError::HostMissing));
        assert_eq!(
            check(|f| f.port = "0".to_owned()),
            Some(DraftError::PortInvalid)
        );
        assert_eq!(
            check(|f| f.username.clear()),
            Some(DraftError::UsernameMissing)
        );
        assert_eq!(
            check(|f| f.username = "two words".to_owned()),
            Some(DraftError::UsernameInvalid)
        );
    }

    #[test]
    fn the_fingerprint_shown_is_never_part_of_the_gateway_saved() {
        let shown = format!("SHA256:{}", "A".repeat(43));
        let opened = draft().with_trusted_fingerprint(Some(shown.clone()));
        assert_eq!(opened.trusted_fingerprint, shown);
        assert_eq!(
            opened.to_gateway(ProfileId::new("g")),
            draft().to_gateway(ProfileId::new("g"))
        );
        assert_eq!(
            draft().with_trusted_fingerprint(None).trusted_fingerprint,
            ""
        );
    }

    #[test]
    fn a_gateway_is_never_its_own_parent() {
        let mut form = draft();
        form.parent = Some(ProfileId::new("g"));
        assert_eq!(
            form.to_gateway(ProfileId::new("g")).expect("valid").parent,
            None
        );
    }
}
