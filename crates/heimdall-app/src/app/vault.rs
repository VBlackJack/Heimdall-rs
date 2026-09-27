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

//! Saved passwords: a vault sealed with a master password, answering the questions of the
//! servers they were accepted by.
//!
//! The rules, each a guard against sending a password where it does not belong or against
//! locking an account out:
//! - a saved password answers only a question from the server and account it was saved for,
//!   compared with the tab's profile as it is now: not a gateway on the way, not a host the
//!   profile was edited to;
//! - it answers the first try of an attempt only, once; asked again, or the attempt failing
//!   in any way, it is taken as refused, and until a new one is saved the user is asked;
//! - it is never given to an RDP server reached without Network Level Authentication, which
//!   would show a logon screen rather than refuse a wrong one;
//! - a password is saved only once the connection it was typed for succeeded, and only if
//!   nothing else was answered after it: a later answer is what the server accepted.
//!
//! What it does not do: two Heimdall windows each save their own copy of the vault, and the
//! last one saved wins; a vault replaced on disk while open is overwritten by the next save.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use heimdall_core::credentials::{
    CredentialProtocol, Endpoint, SavedPassword, decode, encode, password_entry,
};
use heimdall_core::profile::ProfileId;
use heimdall_ssh::Secret;
use sealvault::{Vault, VaultError};
use zeroize::Zeroizing;

use super::{App, Dialog, Effect, Message, Prompt, Tab, TabProfile};
use crate::event::{Answer, QuestionKind};
use crate::ids::{AttemptId, QuestionId, TabId};

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

/// The vault and what this session learnt about its passwords.
#[derive(Debug)]
pub(super) struct VaultState {
    path: PathBuf,
    open: Option<Vault>,
    /// Profiles whose saved password a server refused in this session: the user is asked.
    refused: HashSet<ProfileId>,
}

impl VaultState {
    pub(super) fn beside(profiles_file: &std::path::Path) -> Self {
        Self {
            path: profiles_file.with_file_name(VAULT_FILE_NAME),
            open: None,
            refused: HashSet::new(),
        }
    }

    pub(super) fn exists(&self) -> bool {
        self.path.is_file()
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
            Some(match &profile.domain {
                Some(domain) => format!("{domain}\\{}", question.username),
                None => question.username.clone(),
            }),
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

    /// Whether the password typed for `prompt` may be saved, once the connection succeeds.
    #[must_use]
    pub fn can_remember(&self, tab: TabId, prompt: &Prompt) -> bool {
        self.vault.open.is_some()
            && self
                .tab(tab)
                .is_some_and(|tab| usable_endpoint(tab, &prompt.kind).is_some())
    }

    /// Applies a message about the vault.
    pub(super) fn vault_message(&mut self, message: Message) -> Vec<Effect> {
        match message {
            Message::AnswerRemembered {
                tab,
                question,
                password,
            } => {
                // Held first, while the question is still there to say what it was for.
                self.remember(tab, question, &password);
                self.update(Message::Answer {
                    tab,
                    question,
                    answer: Some(Answer::Secret(password)),
                })
            }
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
        let saved = decode(self.vault.open.as_ref()?.get(&password_entry(&profile))?)?;
        if !saved.endpoint.is(&endpoint) {
            return None;
        }
        let tab = self.tab_mut(tab_id)?;
        tab.auto_answered = Some(tab.attempt);
        Some(Answer::Secret(Secret::new(String::clone(&saved.password))))
    }

    /// Holds `password`, typed for `question` with "remember" ticked, until the connection
    /// succeeds.
    pub(super) fn remember(&mut self, tab_id: TabId, question: QuestionId, password: &Secret) {
        if self.vault.open.is_none() {
            return;
        }
        let Some(tab) = self.tab_mut(tab_id) else {
            return;
        };
        let Some(prompt) = tab.prompts.iter().find(|p| p.question == question) else {
            return;
        };
        let Some((profile, endpoint)) = usable_endpoint(tab, &prompt.kind) else {
            return;
        };
        tab.remembered = Some(Remembered {
            attempt: tab.attempt,
            question,
            profile,
            saved: SavedPassword {
                endpoint,
                password: Zeroizing::new(password.expose().to_owned()),
            },
        });
    }

    /// `question` of `tab_id` is being answered: a password held for an earlier question is
    /// dropped, since whatever succeeds now does so on a later answer.
    pub(super) fn answering(&mut self, tab_id: TabId, question: QuestionId) {
        let Some(tab) = self.tab_mut(tab_id) else {
            return;
        };
        let asks_a_secret = tab.prompts.iter().any(|prompt| {
            prompt.question == question && !matches!(prompt.kind, QuestionKind::Username(_))
        });
        if asks_a_secret
            && tab
                .remembered
                .as_ref()
                .is_some_and(|held| held.question != question)
        {
            tab.remembered = None;
        }
    }

    /// The connection of `tab_id` succeeded: a password held for it is saved.
    pub(super) fn credentials_accepted(&mut self, tab_id: TabId) {
        let Some(tab) = self.tab_mut(tab_id) else {
            return;
        };
        let Some(held) = tab.remembered.take() else {
            return;
        };
        if held.attempt != tab.attempt {
            return;
        }
        let entry = password_entry(&held.profile);
        let Some(vault) = self.vault.open.as_mut() else {
            return;
        };
        let before = vault.get(&entry).map(<[u8]>::to_vec).map(Zeroizing::new);
        vault.set(entry.clone(), encode(&held.saved).to_vec());
        match vault.save() {
            Ok(()) => {
                self.vault.refused.remove(&held.profile);
            }
            Err(error) => {
                // Not saved is not used either: the vault in memory is put back as on disk.
                match before {
                    Some(bytes) => vault.set(entry, bytes.to_vec()),
                    None => {
                        vault.remove(&entry);
                    }
                }
                self.vault_save_failed(&error);
            }
        }
    }

    /// Forgets the saved password of a profile being deleted, when the vault is open.
    pub(super) fn forget_password(&mut self, profile: &ProfileId) {
        let Some(vault) = self.vault.open.as_mut() else {
            return;
        };
        if vault.remove(&password_entry(profile))
            && let Err(error) = vault.save()
        {
            self.vault_save_failed(&error);
        }
    }

    /// Says a save failed, unless another dialog is open: what the user is doing there is
    /// not thrown away for it.
    fn vault_save_failed(&mut self, error: &VaultError) {
        log::warn!("the vault could not be saved: {error}");
        if self.dialog.is_none() {
            self.dialog = Some(Dialog::VaultSaveFailed {
                detail: error.to_string(),
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
        // A password held for this attempt stays unsaved: it is tied to the attempt, which
        // is over.
        if tab.auto_answered == Some(tab.attempt)
            && let Some(profile) = saved_profile(&tab.profile)
        {
            self.vault.refused.insert(profile);
        }
    }
}

/// A password typed with "remember" ticked, held until its attempt succeeds.
#[derive(Debug)]
pub(super) struct Remembered {
    attempt: AttemptId,
    question: QuestionId,
    profile: ProfileId,
    saved: SavedPassword,
}

/// The endpoint a password for `kind` is saved for and given to, when there is one: never an
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
