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
//! port, username, key, and the gateway it is reached through.

use std::path::PathBuf;

use heimdall_core::profile::{ProfileId, SshGateway};

use crate::profile_draft::{DEFAULT_SSH_PORT, DraftError, ProfileField, host};

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
            password_saved: false,
            clear_password: false,
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
            | ProfileField::Domain
            | ProfileField::FixedWidth
            | ProfileField::FixedHeight => "",
        }
    }

    /// Replaces the text of `field`; a field the dialog does not have is ignored.
    pub fn set(&mut self, field: ProfileField, value: String) {
        match field {
            ProfileField::Name => self.name = value,
            ProfileField::Host => self.host = value,
            ProfileField::Port => self.port = value,
            ProfileField::Username => self.username = value,
            ProfileField::KeyPath => self.key_path = value,
            ProfileField::Group
            | ProfileField::Domain
            | ProfileField::FixedWidth
            | ProfileField::FixedHeight => {}
        }
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
    fn a_gateway_is_never_its_own_parent() {
        let mut form = draft();
        form.parent = Some(ProfileId::new("g"));
        assert_eq!(
            form.to_gateway(ProfileId::new("g")).expect("valid").parent,
            None
        );
    }
}
