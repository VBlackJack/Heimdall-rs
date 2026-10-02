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
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use heimdall_core::credentials::{
    CredentialProtocol, Endpoint, SavedPassphrase, SavedPassword, decode, decode_passphrase,
    encode, encode_passphrase, passphrase_entry, password_entry, rdp_account,
};
use heimdall_core::lockout::Lockout;
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

/// Kinds of character (lower case, upper case, digit, other) a master password shorter than
/// [`LONG_MASTER_PASSWORD_CHARS`] mixes at least, as the C# Heimdall asks.
pub const MIN_MASTER_PASSWORD_CLASSES: usize = 3;

/// Length from which a master password needs no mix of characters: a passphrase.
pub const LONG_MASTER_PASSWORD_CHARS: usize = 20;

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
    /// Open the existing vault at start: cancelled, the application quits, as the C#
    /// Heimdall's gate does.
    Unlock,
    /// The workspace was locked: it opens again with the master password, and cannot be
    /// dismissed.
    Locked,
    /// Create a vault: the password is typed twice. Passwords saved in the system's store
    /// move into it.
    Create,
    /// Seal the open vault with a new password: the current one, then the new one twice.
    Change,
    /// Remove the master password: typed once, the saved passwords go back to the system's
    /// store and the vault is deleted.
    Disable,
}

/// What is done with the vault file away from the application's thread.
pub enum VaultJob {
    /// Open it.
    Open,
    /// Create it.
    Create,
    /// Open it, then seal it with this password.
    Rekey(Secret),
}

impl VaultJob {
    /// What it does, for logs: never the password.
    #[must_use]
    pub fn name(&self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Create => "create",
            Self::Rekey(_) => "rekey",
        }
    }
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
    /// Shorter than [`LONG_MASTER_PASSWORD_CHARS`] and mixing fewer than
    /// [`MIN_MASTER_PASSWORD_CLASSES`] kinds of character.
    TooSimple,
    /// The master password cannot be removed: this system has no credential store to put
    /// the saved passwords back in.
    NoSystemStore,
    /// A vault appeared where one was to be created.
    AlreadyExists,
    /// Too many wrong master passwords in a row: no try is taken until then, as the C#
    /// unlock dialogs lock out.
    LockedOut {
        /// When tries are taken again.
        until: SystemTime,
    },
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

/// Opens, creates or seals again the vault at `path` with `password`: the key derivations
/// run on a blocking thread.
///
/// # Errors
///
/// [`VaultProblem`].
pub async fn open_vault(
    path: PathBuf,
    password: Secret,
    job: VaultJob,
) -> Result<OpenedVault, VaultProblem> {
    tokio::task::spawn_blocking(move || {
        let password = password.expose().as_bytes();
        let vault = match job {
            VaultJob::Open => Vault::open(path, password),
            VaultJob::Create => Vault::create(path, password),
            VaultJob::Rekey(new) => Vault::open(path, password).and_then(|mut vault| {
                vault.change_password(new.expose().as_bytes())?;
                // The copy kept beside the vault is the file before this change, sealed with
                // the old password: saved once more, it is sealed with the new one too.
                vault.save()?;
                Ok(vault)
            }),
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
    /// Profiles and gateways whose saved key passphrase did not unlock their key in this
    /// session: the user is asked.
    refused_passphrases: HashSet<ProfileId>,
    /// Wrong master passwords in a row at the lock screen, for this run only, as the C#
    /// overlay counts them; those at start are kept in the settings.
    lock_screen: Lockout,
}

impl VaultState {
    /// Whether a server refused `profile`'s password in this session.
    pub(super) fn is_refused(&self, profile: &ProfileId) -> bool {
        self.refused.contains(profile)
    }

    pub(super) fn beside(profiles_file: &std::path::Path, system: SystemCredentials) -> Self {
        Self {
            path: profiles_file.with_file_name(VAULT_FILE_NAME),
            open: None,
            system,
            refused: HashSet::new(),
            refused_passphrases: HashSet::new(),
            lock_screen: Lockout::default(),
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
    pub(super) fn read(&self, name: &str) -> Option<Zeroizing<Vec<u8>>> {
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
    pub(super) fn write(&mut self, name: &str, bytes: Option<&[u8]>) -> Result<(), String> {
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

impl VaultState {
    /// Takes `vault`, just created, as where passwords are: those saved in the system's store
    /// under `names` move into it. Nothing is lost on a failure: the new vault is removed and
    /// the system's store left as it was.
    fn move_in(&mut self, mut vault: Vault, names: &[String]) -> Result<(), VaultProblem> {
        let mut moved = Vec::new();
        let filled = names.iter().try_for_each(|name| {
            if let Some(bytes) = self.system.get(name)? {
                vault.set(name.clone(), bytes.to_vec());
                moved.push(name);
            }
            Ok::<(), String>(())
        });
        if let Err(detail) = filled.and_then(|()| vault.save().map_err(|error| error.to_string())) {
            drop(vault);
            self.remove_files();
            return Err(VaultProblem::System { detail });
        }
        for name in moved {
            // Left behind, it is only a copy the vault makes unused.
            if let Err(error) = self.system.remove(name) {
                log::warn!("a password moved into the vault stays in the system's store: {error}");
            }
        }
        self.open = Some(vault);
        Ok(())
    }

    /// Puts every password of `vault`, just opened with its master password, back in the
    /// system's store, then deletes the vault. On a failure the vault stays where passwords
    /// are, whole; trying again finishes.
    fn move_out(&mut self, vault: &Vault) -> Result<(), VaultProblem> {
        for name in vault.names() {
            let bytes = vault.get(name).unwrap_or_default();
            self.system
                .set(name, bytes)
                .map_err(|detail| VaultProblem::System { detail })?;
        }
        std::fs::remove_file(&self.path).map_err(|error| VaultProblem::System {
            detail: error.to_string(),
        })?;
        self.remove_files();
        self.open = None;
        Ok(())
    }

    /// Deletes the vault and the copy kept beside it, whichever are there.
    fn remove_files(&self) {
        for path in [self.path.clone(), sealvault::backup_path(&self.path)] {
            if let Err(error) = std::fs::remove_file(&path)
                && error.kind() != std::io::ErrorKind::NotFound
            {
                log::warn!("{} could not be deleted: {error}", path.display());
            }
        }
    }
}

/// Why a new master password typed with `confirm` is refused, if it is: the C# Heimdall's
/// rules, then the two typed alike.
fn new_password_problem(password: &Secret, confirm: Option<&Secret>) -> Option<VaultProblem> {
    let password = password.expose();
    master_password_problem(password).or_else(|| {
        (confirm.map(Secret::expose) != Some(password)).then_some(VaultProblem::Mismatch)
    })
}

/// Why `password` is refused as a new master password by the C# Heimdall's rules, if it
/// is: [`VaultProblem::TooShort`] or [`VaultProblem::TooSimple`]. The dialog says it as the
/// password is typed, with the same rule the core applies.
#[must_use]
pub fn master_password_problem(password: &str) -> Option<VaultProblem> {
    let length = password.chars().count();
    let classes = [
        password.chars().any(char::is_lowercase),
        password.chars().any(char::is_uppercase),
        password.chars().any(char::is_numeric),
        password
            .chars()
            .any(|c| !(c.is_lowercase() || c.is_uppercase() || c.is_numeric())),
    ]
    .into_iter()
    .filter(|&present| present)
    .count();
    if length < MIN_MASTER_PASSWORD_CHARS {
        Some(VaultProblem::TooShort)
    } else if length < LONG_MASTER_PASSWORD_CHARS && classes < MIN_MASTER_PASSWORD_CLASSES {
        Some(VaultProblem::TooSimple)
    } else {
        None
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
        (TabProfile::Ftp(profile), QuestionKind::Password(question)) => (
            &profile.id,
            CredentialProtocol::Ftp,
            (&profile.host, profile.port),
            (question.host.as_str(), question.port),
            Some(question.username.clone()),
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

    /// Whether a key passphrase is saved for `profile`, as far as can be read now.
    #[must_use]
    pub(super) fn passphrase_saved(&self, profile: &ProfileId) -> bool {
        self.vault.read(&passphrase_entry(profile)).is_some()
    }

    /// Applies a message about the vault.
    pub(super) fn vault_message(&mut self, message: Message) -> Vec<Effect> {
        match message {
            Message::ShowVault => {
                self.show_vault();
                Vec::new()
            }
            Message::ChangeMasterPassword => {
                self.show_master_password(VaultMode::Change);
                Vec::new()
            }
            Message::DisableMasterPassword => {
                self.show_master_password(VaultMode::Disable);
                Vec::new()
            }
            Message::SubmitVault {
                password,
                new,
                confirm,
            } => self.submit_vault(password, new, confirm.as_ref()),
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

    /// Opens the dialog changing the master password, or removing it: only when there is
    /// one, open, as the C# settings offer them.
    fn show_master_password(&mut self, mode: VaultMode) {
        if self.vault_status() == VaultStatus::Open {
            self.dialog = Some(Dialog::Vault(VaultDialog {
                mode,
                problem: None,
                busy: false,
            }));
        }
    }

    /// Checks what was typed into the vault dialog, then has the vault opened, created or
    /// sealed again. `password` is the dialog's first field: the master password, or the new
    /// one when creating; `new` is the new one when changing it.
    fn submit_vault(
        &mut self,
        password: Secret,
        new: Option<Secret>,
        confirm: Option<&Secret>,
    ) -> Vec<Effect> {
        let system_available = self.vault.system.available();
        let waiting = match &self.dialog {
            Some(Dialog::Vault(dialog)) if !dialog.busy => Some(dialog.mode),
            _ => None,
        };
        let locked_until = waiting
            .and_then(|mode| self.unlock_lockout(mode))
            .and_then(|lockout| lockout.locked_until(SystemTime::now()));
        let Some(Dialog::Vault(dialog)) = self.dialog.as_mut() else {
            return Vec::new();
        };
        if dialog.busy {
            return Vec::new();
        }
        if let Some(until) = locked_until {
            dialog.problem = Some(VaultProblem::LockedOut { until });
            return Vec::new();
        }
        let (problem, job) = match dialog.mode {
            VaultMode::Unlock | VaultMode::Locked => (None, VaultJob::Open),
            VaultMode::Create => (new_password_problem(&password, confirm), VaultJob::Create),
            VaultMode::Change => {
                let new = new.unwrap_or_else(|| Secret::new(String::new()));
                (new_password_problem(&new, confirm), VaultJob::Rekey(new))
            }
            VaultMode::Disable => (
                (!system_available).then_some(VaultProblem::NoSystemStore),
                VaultJob::Open,
            ),
        };
        if problem.is_some() {
            dialog.problem = problem;
            return Vec::new();
        }
        dialog.busy = true;
        dialog.problem = None;
        vec![Effect::OpenVault {
            path: self.vault.path.clone(),
            password,
            job,
        }]
    }

    /// The vault was opened, created or sealed again, or could not be: finishes what the
    /// dialog was for.
    pub(super) fn vault_opened(&mut self, result: Result<OpenedVault, VaultProblem>) {
        let mode = match &self.dialog {
            Some(Dialog::Vault(dialog)) if dialog.busy => dialog.mode,
            // Cancelled while the key was derived: nothing changes.
            _ => return,
        };
        let names = self.password_entries();
        let done = result.and_then(|opened| {
            let vault = opened.take().ok_or_else(|| VaultProblem::System {
                detail: "the vault was handed over twice".to_owned(),
            })?;
            match mode {
                VaultMode::Unlock | VaultMode::Locked | VaultMode::Change => {
                    self.vault.open = Some(vault);
                    Ok(())
                }
                VaultMode::Create => self.vault.move_in(vault, &names),
                VaultMode::Disable => self.vault.move_out(&vault),
            }
        });
        let done = self.count_unlock(mode, done);
        match done {
            Ok(()) => self.dialog = None,
            Err(problem) => {
                if let Some(Dialog::Vault(dialog)) = self.dialog.as_mut() {
                    dialog.busy = false;
                    dialog.problem = Some(problem);
                }
            }
        }
    }

    /// The count of wrong master passwords `mode` keeps: at start, kept across runs; at the
    /// lock screen, for this run. `None` for the dialogs that are not unlocking.
    fn unlock_lockout(&mut self, mode: VaultMode) -> Option<&mut Lockout> {
        match mode {
            VaultMode::Unlock => Some(&mut self.settings.vault_unlock),
            VaultMode::Locked => Some(&mut self.vault.lock_screen),
            VaultMode::Create | VaultMode::Change | VaultMode::Disable => None,
        }
    }

    /// Counts the try `done` was, when `mode` unlocks: a right one starts the count again, a
    /// wrong one adds to it and, the last allowed, locks the tries out, said instead of the
    /// wrong password. The count at start is saved at once, as the C# gate saves it.
    fn count_unlock(
        &mut self,
        mode: VaultMode,
        done: Result<(), VaultProblem>,
    ) -> Result<(), VaultProblem> {
        let now = SystemTime::now();
        let Some(lockout) = self.unlock_lockout(mode) else {
            return done;
        };
        let done = match done {
            Ok(()) => {
                lockout.reset();
                Ok(())
            }
            Err(VaultProblem::Unreadable) => {
                lockout.register_failure(now);
                Err(lockout
                    .locked_until(now)
                    .map_or(VaultProblem::Unreadable, |until| VaultProblem::LockedOut {
                        until,
                    }))
            }
            other => other,
        };
        if mode == VaultMode::Unlock {
            // Not saved, the count still holds for this run: the unlock is not refused for it.
            let _ = self.settings.save(&self.settings_file);
        }
        done
    }

    /// The entry of every profile and gateway that may have a saved password: the system's
    /// store cannot be listed.
    fn password_entries(&self) -> Vec<String> {
        self.profiles()
            .iter()
            .map(|profile| &profile.id)
            .chain(self.gateways().iter().map(|gateway| &gateway.id))
            .map(password_entry)
            .collect()
    }

    /// Closes the vault: its saved passwords leave memory, and nothing is answered from it.
    /// Locks the workspace, as Ctrl+L does in the C# Heimdall: the vault closes, and the
    /// master password is asked for before anything else. Without an open vault, nothing
    /// happens.
    pub(super) fn lock_vault(&mut self) {
        if self.vault_status() != VaultStatus::Open {
            return;
        }
        self.vault.open = None;
        self.dialog = Some(Dialog::Vault(VaultDialog {
            mode: VaultMode::Locked,
            problem: None,
            busy: false,
        }));
    }

    /// Whether the workspace is locked: the vault closed behind the lock screen.
    #[must_use]
    pub fn is_locked(&self) -> bool {
        matches!(&self.dialog, Some(Dialog::Vault(dialog)) if dialog.mode == VaultMode::Locked)
    }

    /// Dismisses the vault dialog as its Cancel does: at start the application quits, the
    /// lock screen stays, anything else closes. `None` when the dialog is not the vault's.
    pub(super) fn dismiss_vault(&mut self) -> Option<Vec<Effect>> {
        let Some(Dialog::Vault(dialog)) = &self.dialog else {
            return None;
        };
        match dialog.mode {
            VaultMode::Locked => Some(Vec::new()),
            VaultMode::Unlock => {
                self.dialog = None;
                Some(vec![Effect::Exit])
            }
            VaultMode::Create | VaultMode::Change | VaultMode::Disable => {
                self.dialog = None;
                Some(Vec::new())
            }
        }
    }

    /// The saved password answering `kind` in `tab_id`, if the rules allow one.
    pub(super) fn saved_answer(&mut self, tab_id: TabId, kind: &QuestionKind) -> Option<Answer> {
        if let QuestionKind::Passphrase(question) = kind {
            return self.saved_passphrase(tab_id, &question.key_path, question.attempt);
        }
        let tab = self.tab(tab_id)?;
        let (profile, endpoint) =
            usable_endpoint(tab, kind).or_else(|| self.gateway_endpoint(&tab.profile, kind))?;
        let answered_before = tab
            .auto_answered
            .iter()
            .any(|(attempt, answered)| *attempt == tab.attempt && *answered == profile);
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
        let attempt = tab.attempt;
        tab.auto_answered.retain(|(earlier, _)| *earlier == attempt);
        tab.auto_answered.push((attempt, profile));
        Some(Answer::Secret(Secret::new(String::clone(&saved.password))))
    }

    /// The gateway on the way to `profile`'s server that `kind` comes from, when it is one:
    /// same host, port and account as a gateway of the route. Its own saved password is
    /// then given, never the server's.
    fn gateway_endpoint(
        &self,
        profile: &TabProfile,
        kind: &QuestionKind,
    ) -> Option<(ProfileId, Endpoint)> {
        let QuestionKind::Password(question) = kind else {
            return None;
        };
        let gateway = match profile {
            TabProfile::Ssh(profile) => profile.gateway.as_ref(),
            TabProfile::Rdp(profile) => profile.gateway.as_ref(),
            TabProfile::WinRm(profile) => profile.gateway.as_ref(),
            _ => None,
        }?;
        let route = self.store.route(Some(gateway)).ok()?;
        route
            .into_iter()
            .find(|hop| {
                hop.host.eq_ignore_ascii_case(&question.host)
                    && hop.port == question.port
                    && hop.username.as_deref() == Some(question.username.as_str())
            })
            .map(|hop| {
                (
                    hop.id,
                    Endpoint {
                        protocol: CredentialProtocol::Ssh,
                        host: hop.host,
                        port: hop.port,
                        username: hop.username,
                    },
                )
            })
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

    /// Saves `from`'s password and key passphrase for `to` as well, a copy of the profile.
    pub(super) fn copy_password(&mut self, from: &ProfileId, to: &ProfileId) {
        for entry in [password_entry, passphrase_entry] {
            if let Some(bytes) = self.vault.read(&entry(from))
                && let Err(error) = self.vault.write(&entry(to), Some(&bytes))
            {
                self.password_save_failed(&error);
            }
        }
    }

    /// Forgets the saved password and key passphrase of a profile being deleted.
    pub(super) fn forget_password(&mut self, profile: &ProfileId) {
        for entry in [password_entry(profile), passphrase_entry(profile)] {
            if self.vault.read(&entry).is_some()
                && let Err(error) = self.vault.write(&entry, None)
            {
                self.password_save_failed(&error);
            }
        }
    }

    /// The saved passphrase of the key file at `key_path`, for the first question of an
    /// attempt, when the tab's profile or a gateway on its way has that key and saved it.
    /// Asked again, the saved passphrase did not unlock the key: the user is asked from then
    /// on.
    fn saved_passphrase(&mut self, tab_id: TabId, key_path: &Path, attempt: u32) -> Option<Answer> {
        let owner = self.key_owner(tab_id, key_path)?;
        let saved = decode_passphrase(&self.vault.read(&passphrase_entry(&owner))?)?;
        if Path::new(&saved.key_path) != key_path {
            return None;
        }
        if attempt > 1 {
            self.vault.refused_passphrases.insert(owner);
            return None;
        }
        if self.vault.refused_passphrases.contains(&owner) {
            return None;
        }
        Some(Answer::Secret(Secret::new(String::clone(
            &saved.passphrase,
        ))))
    }

    /// The profile or gateway whose key file is `key_path`, in `tab_id`'s route.
    fn key_owner(&self, tab_id: TabId, key_path: &Path) -> Option<ProfileId> {
        let TabProfile::Ssh(profile) = &self.tab(tab_id)?.profile else {
            return None;
        };
        if profile.key_path.as_deref() == Some(key_path) {
            return Some(profile.id.clone());
        }
        let route = self.store.route(profile.gateway.as_ref()).ok()?;
        route
            .into_iter()
            .find(|hop| hop.key_path.as_deref() == Some(key_path))
            .map(|hop| hop.id)
    }

    /// Saves what an editor says about the passphrase of `profile`'s key, now `key_path`: a
    /// new one typed, the saved one cleared, or the saved one dropped when the profile names
    /// another key file or none.
    pub(super) fn save_edited_passphrase(
        &mut self,
        profile: &ProfileId,
        key_path: Option<&Path>,
        typed: Option<&Secret>,
        clear: bool,
    ) {
        let entry = passphrase_entry(profile);
        let typed = typed.filter(|typed| !typed.expose().is_empty());
        // A passphrase typed wins over a clear, as a password typed does.
        let result = if let (Some(key_path), Some(typed)) = (key_path, typed) {
            let saved = SavedPassphrase {
                key_path: key_path.to_string_lossy().into_owned(),
                passphrase: Zeroizing::new(typed.expose().to_owned()),
            };
            self.vault.write(&entry, Some(&encode_passphrase(&saved)))
        } else {
            let stale = self
                .vault
                .read(&entry)
                .and_then(|bytes| decode_passphrase(&bytes))
                .is_some_and(|saved| key_path != Some(Path::new(&saved.key_path)));
            if clear || stale {
                self.vault.write(&entry, None)
            } else {
                return;
            }
        };
        match result {
            Ok(()) => {
                self.vault.refused_passphrases.remove(profile);
            }
            Err(error) => self.password_save_failed(&error),
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
        // Which one was wrong is not known: every saved password given in the attempt is
        // taken as refused, the user asked for each next time.
        let attempt = tab.attempt;
        let given: Vec<ProfileId> = tab
            .auto_answered
            .iter()
            .filter(|(answered, _)| *answered == attempt)
            .map(|(_, profile)| profile.clone())
            .collect();
        self.vault.refused.extend(given);
    }
}

/// The endpoint a password for `kind` is given to, when there is one: never an
/// RDP server reached without Network Level Authentication, where a desktop shown proves
/// nothing about the password and a wrong one is never refused.
pub(super) fn usable_endpoint(tab: &Tab, kind: &QuestionKind) -> Option<(ProfileId, Endpoint)> {
    if let TabProfile::Rdp(profile) = &tab.profile
        && profile.allow_tls_only
    {
        return None;
    }
    asked_endpoint(&tab.profile, kind)
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
            redirect_drives: false,
            options: heimdall_core::profile::RdpOptions::default(),
            vault_entry: None,
            forwards: heimdall_core::profile::Forwards::default(),
            follow_defaults: false,
            several_servers: false,
            anti_idle: false,
            auto_reconnect: true,
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
