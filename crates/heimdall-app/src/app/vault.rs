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

//! Saved passwords, as the C# Heimdall keeps them: typed in the profile editor, kept in the
//! system's credential store, or in a vault sealed with a master password once one is set,
//! and given to the server when it asks.
//!
//! The rules, each a guard against sending a password where it does not belong or against
//! locking an account out:
//! - a saved password answers only a question from the server and account it was saved for,
//!   compared with the tab's profile as it is now: not a gateway on the way, not a host the
//!   profile was changed to outside the editor (an import); a change made in the editor
//!   carries the password along, since the user made it;
//! - it answers the first try of an attempt only, once; asked again, or the attempt failing
//!   in any way, it is taken as refused, and until a new one is saved the user is asked;
//! - it is never given to an RDP server reached without Network Level Authentication, which
//!   would show a logon screen rather than refuse a wrong one.
//!
//! What it does not do: two Heimdall windows each save their own copy of the vault, and the
//! last one saved wins; a vault replaced on disk while open is overwritten by the next save.
//! On Linux, a Secret Service that asks the user to unlock its keyring does so while the
//! window waits.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use heimdall_core::credentials::{
    CredentialProtocol, Endpoint, SavedPassword, decode, encode, password_entry, rdp_account,
};
use heimdall_core::profile::ProfileId;
use heimdall_keyring::SystemKeyring;
use heimdall_ssh::Secret;
use sealvault::{Vault, VaultError};
use zeroize::Zeroizing;

use super::{App, Dialog, Effect, Message, Tab, TabProfile};
use crate::event::{Answer, QuestionKind};
use crate::ids::TabId;

/// Name of the vault file, beside the profiles.
pub const VAULT_FILE_NAME: &str = "vault.hvlt";

/// Fewest characters of a new master password. The vault's key derivation makes each guess
/// cost, but a short password still falls to a patient attacker who copied the file.
pub const MIN_MASTER_PASSWORD_CHARS: usize = 12;

/// Where the vault stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VaultStatus {
    /// There is no vault yet.
    Missing,
    /// There is one, closed.
    Locked,
    /// It is open: saved passwords are used, new ones can be saved.
    Open,
}

/// What the vault dialog does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VaultMode {
    /// Open the existing vault.
    Unlock,
    /// Create a vault: the password is typed twice.
    Create,
}

/// Why the vault could not be opened or created, or a new master password was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VaultProblem {
    /// Wrong password, or a file that is not an intact vault: deliberately one answer.
    Unreadable,
    /// The two passwords typed differ.
    Mismatch,
    /// Shorter than [`MIN_MASTER_PASSWORD_CHARS`].
    TooShort,
    /// A vault appeared where one was to be created.
    AlreadyExists,
    /// The file could not be read or written, or no random bytes were given.
    System {
        /// Technical detail.
        detail: String,
    },
}

impl From<&VaultError> for VaultProblem {
    fn from(error: &VaultError) -> Self {
        match error {
            VaultError::Unreadable => Self::Unreadable,
            VaultError::AlreadyExists(_) => Self::AlreadyExists,
            other => Self::System {
                detail: other.to_string(),
            },
        }
    }
}

/// The vault dialog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultDialog {
    /// Unlock or create.
    pub mode: VaultMode,
    /// Why the last try failed.
    pub problem: Option<VaultProblem>,
    /// The password is being checked: the key derivation takes a moment.
    pub busy: bool,
}

/// A vault opened away from the application's thread, handed over once.
#[derive(Clone)]
pub struct OpenedVault(Arc<Mutex<Option<Vault>>>);

impl OpenedVault {
    fn take(&self) -> Option<Vault> {
        self.0.lock().ok().and_then(|mut vault| vault.take())
    }
}

impl std::fmt::Debug for OpenedVault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("OpenedVault(..)")
    }
}

/// Opens, or creates, the vault at `path` with `password`: the key derivation runs on a
/// blocking thread.
///
/// # Errors
///
/// [`VaultProblem`].
pub async fn open_vault(
    path: PathBuf,
    password: Secret,
    create: bool,
) -> Result<OpenedVault, VaultProblem> {
    tokio::task::spawn_blocking(move || {
        let password = password.expose().as_bytes();
        let vault = if create {
            Vault::create(path, password)
        } else {
            Vault::open(path, password)
        };
        vault
            .map(|vault| OpenedVault(Arc::new(Mutex::new(Some(vault)))))
            .map_err(|error| VaultProblem::from(&error))
    })
    .await
    .unwrap_or_else(|error| {
        Err(VaultProblem::System {
            detail: error.to_string(),
        })
    })
}

/// Where saved passwords go while no master password is set.
#[derive(Clone)]
pub enum SystemCredentials {
    /// This system has no credential store: saving a password needs a master password.
    Unavailable,
    /// The system's credential store, under this application's name.
    Keyring(SystemKeyring),
    /// Kept in this process only, lost on exit: for tests.
    Memory(Arc<Mutex<HashMap<String, Zeroizing<Vec<u8>>>>>),
}

impl SystemCredentials {
    /// The system's store under `service`, or [`SystemCredentials::Unavailable`] when this
    /// system has none.
    #[must_use]
    pub fn keyring(service: &str) -> Self {
        match SystemKeyring::open(service) {
            Ok(keyring) => Self::Keyring(keyring),
            Err(error) => {
                log::info!("no system credential store: {error}");
                Self::Unavailable
            }
        }
    }

    /// An empty store kept in this process.
    #[must_use]
    pub fn memory() -> Self {
        Self::Memory(Arc::default())
    }

    fn available(&self) -> bool {
        !matches!(self, Self::Unavailable)
    }

    fn get(&self, name: &str) -> Result<Option<Zeroizing<Vec<u8>>>, String> {
        match self {
            Self::Unavailable => Ok(None),
            Self::Keyring(keyring) => keyring.get(name).map_err(|error| error.to_string()),
            Self::Memory(entries) => Ok(entries
                .lock()
                .map_err(|error| error.to_string())?
                .get(name)
                .cloned()),
        }
    }

    fn set(&self, name: &str, secret: &[u8]) -> Result<(), String> {
        match self {
            Self::Unavailable => Err("no system credential store".to_owned()),
            Self::Keyring(keyring) => keyring.set(name, secret).map_err(|error| error.to_string()),
            Self::Memory(entries) => {
                entries
                    .lock()
                    .map_err(|error| error.to_string())?
                    .insert(name.to_owned(), Zeroizing::new(secret.to_vec()));
                Ok(())
            }
        }
    }

    fn remove(&self, name: &str) -> Result<(), String> {
        match self {
            Self::Unavailable => Ok(()),
            Self::Keyring(keyring) => keyring
                .remove(name)
                .map(|_| ())
                .map_err(|error| error.to_string()),
            Self::Memory(entries) => {
                entries
                    .lock()
                    .map_err(|error| error.to_string())?
                    .remove(name);
                Ok(())
            }
        }
    }
}

impl std::fmt::Debug for SystemCredentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Unavailable => "SystemCredentials::Unavailable",
            Self::Keyring(_) => "SystemCredentials::Keyring",
            Self::Memory(_) => "SystemCredentials::Memory",
        })
    }
}

/// Where passwords are saved and what this session learnt about them.
#[derive(Debug)]
pub(super) struct VaultState {
    path: PathBuf,
    open: Option<Vault>,
    system: SystemCredentials,
    /// Profiles whose saved password a server refused in this session: the user is asked.
    refused: HashSet<ProfileId>,
}

impl VaultState {
    pub(super) fn beside(profiles_file: &std::path::Path, system: SystemCredentials) -> Self {
        Self {
            path: profiles_file.with_file_name(VAULT_FILE_NAME),
            open: None,
            system,
            refused: HashSet::new(),
        }
    }

    /// Whether a master password is set: the vault is then where passwords are, and the
    /// system's store is not used.
    pub(super) fn exists(&self) -> bool {
        self.path.is_file()
    }

    /// Whether passwords can be read and saved now.
    fn usable(&self) -> bool {
        if self.exists() {
            self.open.is_some()
        } else {
            self.system.available()
        }
    }

    /// The entry `name`; nothing when it cannot be read (a closed vault, a failing store).
    fn read(&self, name: &str) -> Option<Zeroizing<Vec<u8>>> {
        if self.exists() {
            return self
                .open
                .as_ref()?
                .get(name)
                .map(|bytes| Zeroizing::new(bytes.to_vec()));
        }
        self.system
            .get(name)
            .inspect_err(|error| log::warn!("a saved password could not be read: {error}"))
            .ok()
            .flatten()
    }

    /// Writes entry `name`, or removes it for `None`.
    fn write(&mut self, name: &str, bytes: Option<&[u8]>) -> Result<(), String> {
        if !self.exists() {
            return match bytes {
                Some(bytes) => self.system.set(name, bytes),
                None => self.system.remove(name),
            };
        }
        let Some(vault) = self.open.as_mut() else {
            return Err("the vault is locked".to_owned());
        };
        let before = vault.get(name).map(<[u8]>::to_vec).map(Zeroizing::new);
        match bytes {
            Some(bytes) => vault.set(name.to_owned(), bytes.to_vec()),
            None => {
                vault.remove(name);
            }
        }
        vault.save().map_err(|error| {
            // Not saved is not used either: the vault in memory is put back as on disk.
            match before {
                Some(bytes) => vault.set(name.to_owned(), bytes.to_vec()),
                None => {
                    vault.remove(name);
                }
            }
            error.to_string()
        })
    }
}

/// The profile and server a question in a tab is about, when it is the tab's own server
/// asking for its account's password; `None` for anything else (a gateway, a passphrase, a
/// keyboard-interactive round).
fn asked_endpoint(profile: &TabProfile, kind: &QuestionKind) -> Option<(ProfileId, Endpoint)> {
    let (id, protocol, host, port, username) = match (profile, kind) {
        (TabProfile::Ssh(profile), QuestionKind::Password(question)) => (
            &profile.id,
            CredentialProtocol::Ssh,
            (&profile.host, profile.port),
            (question.host.as_str(), question.port),
            Some(question.username.clone()),
        ),
        (TabProfile::Rdp(profile), QuestionKind::Password(question)) => (
            &profile.id,
            CredentialProtocol::Rdp,
            (&profile.host, profile.port),
            (question.host.as_str(), question.port),
            // The domain names the account as much as the user name does.
            Some(rdp_account(profile.domain.as_deref(), &question.username)),
        ),
        (TabProfile::Vnc(profile), QuestionKind::ServerPassword(question)) => (
            &profile.id,
            CredentialProtocol::Vnc,
            (&profile.host, profile.port),
            (question.host.as_str(), question.port),
            None,
        ),
        _ => return None,
    };
    let ((profile_host, profile_port), (asked_host, asked_port)) = (host, port);
    // Another server on the way (a gateway) asks with its own host and port.
    if !asked_host.eq_ignore_ascii_case(profile_host) || asked_port != profile_port {
        return None;
    }
    Some((
        id.clone(),
        Endpoint {
            protocol,
            host: profile_host.clone(),
            port: profile_port,
            username,
        },
    ))
}

/// Which try of the attempt a question is: only passwords count them.
fn try_number(kind: &QuestionKind) -> u32 {
    match kind {
        QuestionKind::Password(question) => question.attempt,
        _ => 1,
    }
}

impl App {
    /// Where the vault stands.
    #[must_use]
    pub fn vault_status(&self) -> VaultStatus {
        if self.vault.open.is_some() {
            VaultStatus::Open
        } else if self.vault.exists() {
            VaultStatus::Locked
        } else {
            VaultStatus::Missing
        }
    }

    /// Whether passwords can be saved now: in the open vault, or in the system's store while
    /// no master password is set.
    #[must_use]
    pub fn can_save_passwords(&self) -> bool {
        self.vault.usable()
    }

    /// Whether a password is saved for `profile`, as far as can be read now.
    #[must_use]
    pub(super) fn password_saved(&self, profile: &ProfileId) -> bool {
        self.vault.read(&password_entry(profile)).is_some()
    }

    /// Applies a message about the vault.
    pub(super) fn vault_message(&mut self, message: Message) -> Vec<Effect> {
        match message {
            Message::ShowVault => {
                self.show_vault();
                Vec::new()
            }
            Message::SubmitVault { password, confirm } => {
                self.submit_vault(password, confirm.as_ref())
            }
            Message::VaultOpened(result) => {
                self.vault_opened(result);
                Vec::new()
            }
            Message::LockVault => {
                self.lock_vault();
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    /// Opens the unlock dialog when a vault is there, closed.
    pub(super) fn show_vault_if_locked(&mut self) {
        if self.vault_status() == VaultStatus::Locked {
            self.show_vault();
        }
    }

    /// Opens the vault dialog: unlock when there is a vault, create when there is none.
    pub(super) fn show_vault(&mut self) {
        let mode = match self.vault_status() {
            VaultStatus::Open => return,
            VaultStatus::Locked => VaultMode::Unlock,
            VaultStatus::Missing => VaultMode::Create,
        };
        self.dialog = Some(Dialog::Vault(VaultDialog {
            mode,
            problem: None,
            busy: false,
        }));
    }

    /// Checks a master password typed into the vault dialog, then has it tried.
    fn submit_vault(&mut self, password: Secret, confirm: Option<&Secret>) -> Vec<Effect> {
        let Some(Dialog::Vault(dialog)) = self.dialog.as_mut() else {
            return Vec::new();
        };
        if dialog.busy {
            return Vec::new();
        }
        let create = dialog.mode == VaultMode::Create;
        if create {
            let problem = if password.expose().chars().count() < MIN_MASTER_PASSWORD_CHARS {
                Some(VaultProblem::TooShort)
            } else if confirm.map(Secret::expose) != Some(password.expose()) {
                Some(VaultProblem::Mismatch)
            } else {
                None
            };
            if problem.is_some() {
                dialog.problem = problem;
                return Vec::new();
            }
        }
        dialog.busy = true;
        dialog.problem = None;
        vec![Effect::OpenVault {
            path: self.vault.path.clone(),
            password,
            create,
        }]
    }

    /// The vault was opened, or could not be.
    pub(super) fn vault_opened(&mut self, result: Result<OpenedVault, VaultProblem>) {
        let waiting = matches!(&self.dialog, Some(Dialog::Vault(dialog)) if dialog.busy);
        if !waiting {
            // Cancelled while the key was derived: the vault stays closed.
            return;
        }
        match result {
            Ok(opened) => {
                if let Some(vault) = opened.take() {
                    self.vault.open = Some(vault);
                }
                self.dialog = None;
            }
            Err(problem) => {
                if let Some(Dialog::Vault(dialog)) = self.dialog.as_mut() {
                    dialog.busy = false;
                    dialog.problem = Some(problem);
                }
            }
        }
    }

    /// Closes the vault: its saved passwords leave memory, and nothing is answered from it.
    pub(super) fn lock_vault(&mut self) {
        self.vault.open = None;
    }

    /// The saved password answering `kind` in `tab_id`, if the rules allow one.
    pub(super) fn saved_answer(&mut self, tab_id: TabId, kind: &QuestionKind) -> Option<Answer> {
        let tab = self.tab(tab_id)?;
        let (profile, endpoint) = usable_endpoint(tab, kind)?;
        let answered_before = tab.auto_answered == Some(tab.attempt);
        if try_number(kind) > 1 || answered_before {
            // Asked again after a saved password: the server refused it.
            if answered_before {
                self.vault.refused.insert(profile);
            }
            return None;
        }
        if self.vault.refused.contains(&profile) {
            return None;
        }
        let saved = decode(&self.vault.read(&password_entry(&profile))?)?;
        if !saved.endpoint.is(&endpoint) {
            return None;
        }
        let tab = self.tab_mut(tab_id)?;
        tab.auto_answered = Some(tab.attempt);
        Some(Answer::Secret(Secret::new(String::clone(&saved.password))))
    }

    /// Saves what the profile editor says about `profile`'s password, now at `endpoint`: a
    /// new one typed, the saved one cleared, or the saved one carried to the endpoint the
    /// user just changed the profile to.
    pub(super) fn save_edited_password(
        &mut self,
        profile: &ProfileId,
        endpoint: Endpoint,
        typed: Option<&Secret>,
        clear: bool,
    ) {
        let entry = password_entry(profile);
        let result = if let Some(typed) = typed.filter(|typed| !typed.expose().is_empty()) {
            let saved = SavedPassword {
                endpoint,
                password: Zeroizing::new(typed.expose().to_owned()),
            };
            self.vault.write(&entry, Some(&encode(&saved)))
        } else if clear {
            self.vault.write(&entry, None)
        } else {
            match self.vault.read(&entry).and_then(|bytes| decode(&bytes)) {
                Some(saved) if !saved.endpoint.is(&endpoint) => {
                    let moved = SavedPassword { endpoint, ..saved };
                    self.vault.write(&entry, Some(&encode(&moved)))
                }
                _ => return,
            }
        };
        match result {
            // A new password, or none: whatever the server refused is no longer it.
            Ok(()) => {
                self.vault.refused.remove(profile);
            }
            Err(error) => self.password_save_failed(&error),
        }
    }

    /// Saves `from`'s password for `to` as well, a copy of the profile.
    pub(super) fn copy_password(&mut self, from: &ProfileId, to: &ProfileId) {
        if let Some(bytes) = self.vault.read(&password_entry(from))
            && let Err(error) = self.vault.write(&password_entry(to), Some(&bytes))
        {
            self.password_save_failed(&error);
        }
    }

    /// Forgets the saved password of a profile being deleted.
    pub(super) fn forget_password(&mut self, profile: &ProfileId) {
        let entry = password_entry(profile);
        if self.vault.read(&entry).is_some()
            && let Err(error) = self.vault.write(&entry, None)
        {
            self.password_save_failed(&error);
        }
    }

    /// Says a save failed, unless another dialog is open: what the user is doing there is
    /// not thrown away for it.
    fn password_save_failed(&mut self, detail: &str) {
        log::warn!("a password could not be saved: {detail}");
        if self.dialog.is_none() {
            self.dialog = Some(Dialog::PasswordSaveFailed {
                detail: detail.to_owned(),
            });
        }
    }

    /// The connection of `tab_id` failed. After a saved password, whatever the reason: a
    /// server tired of wrong passwords disconnects rather than refuses, and asking the user
    /// once too often costs less than a locked account.
    pub(super) fn credentials_failed(&mut self, tab_id: TabId) {
        let Some(tab) = self.tab_mut(tab_id) else {
            return;
        };
        if tab.auto_answered == Some(tab.attempt)
            && let Some(profile) = saved_profile(&tab.profile)
        {
            self.vault.refused.insert(profile);
        }
    }
}

/// The endpoint a password for `kind` is given to, when there is one: never an
/// RDP server reached without Network Level Authentication, where a desktop shown proves
/// nothing about the password and a wrong one is never refused.
fn usable_endpoint(tab: &Tab, kind: &QuestionKind) -> Option<(ProfileId, Endpoint)> {
    if let TabProfile::Rdp(profile) = &tab.profile
        && profile.allow_tls_only
    {
        return None;
    }
    asked_endpoint(&tab.profile, kind)
}

/// The profile of a tab that can have a saved password.
fn saved_profile(profile: &TabProfile) -> Option<ProfileId> {
    match profile {
        TabProfile::Ssh(profile) => Some(profile.id.clone()),
        TabProfile::Rdp(profile) => Some(profile.id.clone()),
        TabProfile::Vnc(profile) => Some(profile.id.clone()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use heimdall_core::profile::RdpProfile;
    use heimdall_ssh::PasswordQuestion;

    use super::*;

    fn rdp(domain: Option<&str>) -> TabProfile {
        TabProfile::Rdp(RdpProfile {
            id: ProfileId::new("r"),
            name: "r".to_owned(),
            group: None,
            host: "dc.lab".to_owned(),
            port: 3389,
            username: Some("admin".to_owned()),
            domain: domain.map(str::to_owned),
            allow_tls_only: false,
            gateway: None,
            redirect_clipboard: false,
        })
    }

    fn asked(port: u16) -> QuestionKind {
        QuestionKind::Password(PasswordQuestion {
            host: "DC.lab".to_owned(),
            port,
            username: "admin".to_owned(),
            attempt: 1,
        })
    }

    #[test]
    fn an_rdp_account_is_named_with_its_domain() {
        let account = |domain| {
            asked_endpoint(&rdp(domain), &asked(3389)).and_then(|(_, endpoint)| endpoint.username)
        };
        assert_eq!(account(Some("CORP")).as_deref(), Some("CORP\\admin"));
        assert_eq!(account(None).as_deref(), Some("admin"));
    }

    #[test]
    fn another_port_on_the_same_host_is_another_server() {
        assert!(asked_endpoint(&rdp(None), &asked(3389)).is_some());
        assert!(asked_endpoint(&rdp(None), &asked(22)).is_none());
    }
}
