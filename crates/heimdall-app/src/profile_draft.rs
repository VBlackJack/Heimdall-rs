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

//! A profile as typed into its form, and the checks that turn it into a saved profile.

use std::hash::{BuildHasher as _, RandomState};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use heimdall_core::profile::{ProfileId, SshProfile};

/// Port when the field is left empty.
pub const DEFAULT_SSH_PORT: u16 = 22;

/// Prefix of the identifiers this application gives, apart from imported ones.
const ID_PREFIX: &str = "rs-";

/// A field of the profile form.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfileField {
    /// Name shown in the list.
    Name,
    /// Group it is listed under.
    Group,
    /// Server address.
    Host,
    /// Server port.
    Port,
    /// User name.
    Username,
    /// Private key file.
    KeyPath,
}

impl ProfileField {
    /// Every field, in form order.
    pub const ALL: [Self; 6] = [
        Self::Name,
        Self::Group,
        Self::Host,
        Self::Port,
        Self::Username,
        Self::KeyPath,
    ];
}

/// What the form holds, as typed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProfileDraft {
    /// The profile being edited; `None` for a new one.
    pub editing: Option<ProfileId>,
    /// Name.
    pub name: String,
    /// Group.
    pub group: String,
    /// Address.
    pub host: String,
    /// Port; empty is [`DEFAULT_SSH_PORT`].
    pub port: String,
    /// User name.
    pub username: String,
    /// Key file.
    pub key_path: String,
    /// The SSH gateway of the profile being edited, which the form does not show: kept as it
    /// is, so that saving the form never drops it.
    pub gateway: Option<ProfileId>,
    /// A password is saved for the profile: the form says so, its field stays empty.
    pub password_saved: bool,
    /// The saved password is to be removed when the form is saved.
    pub clear_password: bool,
}

/// Why a form cannot be saved yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DraftError {
    /// The name is empty.
    NameMissing,
    /// The address is empty.
    HostMissing,
    /// The address holds a space or a control character.
    HostInvalid,
    /// The address holds `user@`: the user has its own field.
    HostHasUser,
    /// The address holds `:port`: the port has its own field.
    HostHasPort,
    /// The port is not a number from 1 to 65535.
    PortInvalid,
    /// The user name holds a space or a control character.
    UsernameInvalid,
    /// A text holds a control character.
    ControlCharacter,
    /// A password is typed but no user name: a password is for an account.
    UsernameForPassword,
}

impl DraftError {
    /// The field to fix.
    #[must_use]
    pub fn field(self) -> ProfileField {
        match self {
            Self::NameMissing | Self::ControlCharacter => ProfileField::Name,
            Self::HostMissing | Self::HostInvalid | Self::HostHasUser | Self::HostHasPort => {
                ProfileField::Host
            }
            Self::PortInvalid => ProfileField::Port,
            Self::UsernameInvalid | Self::UsernameForPassword => ProfileField::Username,
        }
    }
}

impl ProfileDraft {
    /// A form filled from a saved profile.
    #[must_use]
    pub fn from_profile(profile: &SshProfile) -> Self {
        Self {
            editing: Some(profile.id.clone()),
            name: profile.name.clone(),
            group: profile.group.clone().unwrap_or_default(),
            host: profile.host.clone(),
            port: profile.port.to_string(),
            username: profile.username.clone().unwrap_or_default(),
            key_path: profile
                .key_path
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_default(),
            gateway: profile.gateway.clone(),
            password_saved: false,
            clear_password: false,
        }
    }

    /// The text of `field`.
    #[must_use]
    pub fn value(&self, field: ProfileField) -> &str {
        match field {
            ProfileField::Name => &self.name,
            ProfileField::Group => &self.group,
            ProfileField::Host => &self.host,
            ProfileField::Port => &self.port,
            ProfileField::Username => &self.username,
            ProfileField::KeyPath => &self.key_path,
        }
    }

    /// Replaces the text of `field`.
    pub fn set(&mut self, field: ProfileField, value: String) {
        *match field {
            ProfileField::Name => &mut self.name,
            ProfileField::Group => &mut self.group,
            ProfileField::Host => &mut self.host,
            ProfileField::Port => &mut self.port,
            ProfileField::Username => &mut self.username,
            ProfileField::KeyPath => &mut self.key_path,
        } = value;
    }

    /// The profile this form describes, under `id`.
    ///
    /// # Errors
    ///
    /// The first [`DraftError`] in form order.
    pub fn to_profile(&self, id: ProfileId) -> Result<SshProfile, DraftError> {
        let name = self.name.trim();
        if name.is_empty() {
            return Err(DraftError::NameMissing);
        }
        let group = self.group.trim();
        let key_path = self.key_path.trim();
        if [name, group, key_path]
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
        if username
            .chars()
            .any(|c| c.is_whitespace() || c.is_control())
        {
            return Err(DraftError::UsernameInvalid);
        }
        let optional = |text: &str| (!text.is_empty()).then(|| text.to_owned());
        Ok(SshProfile {
            id,
            name: name.to_owned(),
            group: optional(group),
            host,
            port,
            username: optional(username),
            key_path: optional(key_path).map(PathBuf::from),
            gateway: self.gateway.clone(),
        })
    }
}

/// The address as saved: trimmed, and an IPv6 address without the brackets it may have
/// been typed with.
fn host(typed: &str) -> Result<String, DraftError> {
    let typed = typed.trim();
    if typed.is_empty() {
        return Err(DraftError::HostMissing);
    }
    if typed.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(DraftError::HostInvalid);
    }
    if typed.contains('@') {
        return Err(DraftError::HostHasUser);
    }
    let bare = typed
        .strip_prefix('[')
        .and_then(|inner| inner.strip_suffix(']'))
        .unwrap_or(typed);
    // One colon is a port; an IPv6 address has at least two.
    match bare.matches(':').count() {
        0 => Ok(bare.to_owned()),
        1 => Err(DraftError::HostHasPort),
        _ if bare.contains(['[', ']']) => Err(DraftError::HostHasPort),
        _ => Ok(bare.to_owned()),
    }
}

/// An identifier none of `taken` holds.
#[must_use]
pub fn new_id(taken: &[SshProfile]) -> ProfileId {
    // Nanoseconds since the epoch, mixed with a per-process random key: unique across
    // processes in practice, and checked against what is saved here.
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos());
    let mut seed = RandomState::new().hash_one(nanos);
    loop {
        let id = ProfileId::new(format!("{ID_PREFIX}{seed:016x}"));
        if !taken.iter().any(|profile| profile.id == id) {
            return id;
        }
        seed = seed.wrapping_add(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft(host: &str, port: &str) -> ProfileDraft {
        ProfileDraft {
            name: "web".to_owned(),
            host: host.to_owned(),
            port: port.to_owned(),
            ..ProfileDraft::default()
        }
    }

    fn id() -> ProfileId {
        ProfileId::new("x")
    }

    #[test]
    fn a_full_form_becomes_a_trimmed_profile() {
        let form = ProfileDraft {
            editing: None,
            name: "  web  ".to_owned(),
            group: " Prod ".to_owned(),
            host: " web.example.org ".to_owned(),
            port: " 2222 ".to_owned(),
            username: " admin ".to_owned(),
            key_path: " /home/me/.ssh/id_ed25519 ".to_owned(),
            ..ProfileDraft::default()
        };
        let profile = form.to_profile(id()).expect("valid");
        assert_eq!(profile.name, "web");
        assert_eq!(profile.group.as_deref(), Some("Prod"));
        assert_eq!(profile.host, "web.example.org");
        assert_eq!(profile.port, 2222);
        assert_eq!(profile.username.as_deref(), Some("admin"));
        assert_eq!(
            profile.key_path,
            Some(PathBuf::from("/home/me/.ssh/id_ed25519"))
        );
        assert_eq!(
            ProfileDraft::from_profile(&profile).to_profile(id()),
            Ok(profile)
        );
    }

    #[test]
    fn empty_optional_fields_are_absent_and_the_port_defaults() {
        let profile = draft("host", "").to_profile(id()).expect("valid");
        assert_eq!(profile.port, DEFAULT_SSH_PORT);
        assert_eq!(
            (profile.group, profile.username, profile.key_path),
            (None, None, None)
        );
    }

    #[test]
    fn addresses_are_checked_and_ipv6_loses_its_brackets() {
        let host = |typed: &str| draft(typed, "").to_profile(id()).map(|p| p.host);
        assert_eq!(host("[2001:db8::1]"), Ok("2001:db8::1".to_owned()));
        assert_eq!(host("2001:db8::1"), Ok("2001:db8::1".to_owned()));
        assert_eq!(host("10.0.0.1"), Ok("10.0.0.1".to_owned()));
        assert_eq!(host("  "), Err(DraftError::HostMissing));
        assert_eq!(host("a b"), Err(DraftError::HostInvalid));
        assert_eq!(host("root@web"), Err(DraftError::HostHasUser));
        assert_eq!(host("web:2222"), Err(DraftError::HostHasPort));
        assert_eq!(host("[2001:db8::1]:22"), Err(DraftError::HostHasPort));
    }

    #[test]
    fn ports_are_one_to_65535() {
        let port = |typed: &str| draft("h", typed).to_profile(id()).map(|p| p.port);
        assert_eq!(port("1"), Ok(1));
        assert_eq!(port("65535"), Ok(65535));
        for bad in ["0", "65536", "-1", "22a", "2 2"] {
            assert_eq!(port(bad), Err(DraftError::PortInvalid), "{bad}");
        }
    }

    #[test]
    fn names_users_and_control_characters() {
        let mut form = draft("h", "");
        form.name = " ".to_owned();
        assert_eq!(form.to_profile(id()), Err(DraftError::NameMissing));
        form.name = "a\tb".to_owned();
        assert_eq!(form.to_profile(id()), Err(DraftError::ControlCharacter));
        form.name = "ok".to_owned();
        form.username = "two words".to_owned();
        assert_eq!(form.to_profile(id()), Err(DraftError::UsernameInvalid));
    }

    #[test]
    fn a_new_id_avoids_every_taken_one() {
        let first = new_id(&[]);
        assert!(first.as_str().starts_with(ID_PREFIX));
        let taken = SshProfile {
            id: first.clone(),
            name: "a".to_owned(),
            group: None,
            host: "h".to_owned(),
            port: 22,
            username: None,
            key_path: None,
            gateway: None,
        };
        let second = new_id(std::slice::from_ref(&taken));
        assert_ne!(second, first);
    }
}
