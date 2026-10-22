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

//! The passwords of shared SSH, SFTP and VNC sessions the user chose to remember on this
//! computer, as the C# `SharedCredentialApplier` and `SharedSessionSecretStore` do.
//!
//! The rules, each a guard against sending a password where it does not belong or against
//! keeping one the user did not ask to keep:
//! - nothing is remembered unless the user ticked "Remember this password on this computer"
//!   when the password was asked; the box starts cleared;
//! - a password is kept once the server accepted it (the shell is open, the files are
//!   listed, the desktop is shown), never before;
//! - it is filed per source under the protocol, host, port and login it was typed for, never
//!   under the shared session's identifier: a shared file that moves a session to another
//!   host, port or login finds no password and the user is asked;
//! - it answers the first try of an attempt only; asked again, or the attempt refused, it is
//!   forgotten, so a stale password is never sent twice;
//! - it lives where every other saved password lives (the system's credential store, or the
//!   vault once a master password is set), never in the settings, the shared file, the
//!   offline copy, a report or an export, and nothing about it is logged: that a password is
//!   remembered is itself credential data;
//! - a bastion on the way is not covered: its question is another server's.
//!
//! RDP sessions remember nothing here, as in the C#: the RDP client asks for its password.

use std::collections::HashMap;

use heimdall_core::credentials::{Endpoint, SavedPassword, decode, encode};
use heimdall_core::profile::ProfileId;
use heimdall_core::shared_passwords::{
    self as keys, MAX_PER_SOURCE, SOURCES_ENTRY, decode_index, decode_sources, encode_lines,
};
use heimdall_ssh::Secret;
use zeroize::Zeroizing;

use super::App;
use super::vault::{asked_endpoint, try_number};
use crate::error::UiError;
use crate::event::{Answer, QuestionKind};
use crate::ids::{AttemptId, QuestionId, TabId};

/// A password the user asked to remember, until the server accepts it.
struct PendingRemember {
    attempt: AttemptId,
    source: String,
    endpoint: Endpoint,
    password: Zeroizing<String>,
}

/// A remembered password given in an attempt, forgotten if the server refuses it.
struct UsedPassword {
    attempt: AttemptId,
    source: String,
    endpoint: Endpoint,
}

/// What a tab holds about the remembered password of its shared session.
#[derive(Default)]
pub(super) struct TabPasswords {
    pending: Option<PendingRemember>,
    used: Option<UsedPassword>,
}

impl std::fmt::Debug for TabPasswords {
    /// Neither the password nor the account it is for: only whether there is one.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TabPasswords")
            .field("pending", &self.pending.is_some())
            .field("used", &self.used.is_some())
            .finish()
    }
}

/// What the remembered passwords amount to, for the Settings page and the clean-up of the
/// sources taken out.
#[derive(Debug, Default)]
pub(super) struct Remembered {
    /// The source each shared SSH, SFTP or VNC tab's profile was opened from, by the
    /// identifier of the tab's profile: where its remembered password is looked up and kept.
    origins: HashMap<ProfileId, String>,
    /// How many are remembered per source, as far as can be read now.
    counts: HashMap<String, usize>,
    /// The sources the settings listed when last looked at; `None` before the first look.
    sources_seen: Option<Vec<String>>,
    /// A clean-up of the sources taken out waits for the store to be readable.
    cleanup_pending: bool,
    /// Whether the store could be read when last looked at: the counts follow its opening
    /// and closing.
    readable: bool,
}

impl App {
    /// How many passwords are remembered for the shared source `source`, as far as can be
    /// read now: none while a vault is locked.
    #[must_use]
    pub fn shared_password_count(&self, source: &str) -> usize {
        self.vault.shared.counts.get(source).copied().unwrap_or(0)
    }

    /// Records that `profile`, a shared SSH, SFTP or VNC session opened from `source`, may have
    /// its password remembered.
    pub(super) fn note_password_origin(&mut self, profile: ProfileId, source: &str) {
        self.vault.shared.origins.insert(profile, source.to_owned());
    }

    /// Whether the question `kind` in `tab_id` offers to remember its answer: the password of
    /// the shared session's own server, with somewhere to keep it.
    pub(super) fn offers_remember(&self, tab_id: TabId, kind: &QuestionKind) -> bool {
        let Some(tab) = self.tab(tab_id) else {
            return false;
        };
        asked_endpoint(&tab.profile, kind)
            .is_some_and(|(profile, _)| self.vault.shared.origins.contains_key(&profile))
            && self.vault.usable()
    }

    /// Ticks or clears the box of question `question` in `tab_id`, if it offers one.
    pub(super) fn set_remember_password(
        &mut self,
        tab_id: TabId,
        question: QuestionId,
        remember: bool,
    ) {
        if let Some(prompt) = self
            .tab_mut(tab_id)
            .and_then(|tab| tab.prompts.iter_mut().find(|p| p.question == question))
            .filter(|prompt| prompt.remember.is_some())
        {
            prompt.remember = Some(remember);
        }
    }

    /// Notes, as the user answers, that the password typed is to be remembered once the
    /// server accepts it.
    pub(super) fn note_remember_request(
        &mut self,
        tab_id: TabId,
        question: QuestionId,
        answer: Option<&Answer>,
    ) {
        let Some(Answer::Secret(secret)) = answer else {
            return;
        };
        // A new password typed replaces whatever was waiting to be remembered: a first one the
        // server refused is never kept for the one typed after it with the box cleared.
        if let Some(tab) = self.tab_mut(tab_id) {
            tab.shared_password.pending = None;
        }
        if secret.expose().is_empty() {
            return;
        }
        let Some(tab) = self.tab(tab_id) else {
            return;
        };
        let Some(prompt) = tab.prompts.iter().find(|p| p.question == question) else {
            return;
        };
        if prompt.remember != Some(true) {
            return;
        }
        let Some((profile, endpoint)) = asked_endpoint(&tab.profile, &prompt.kind) else {
            return;
        };
        let Some(source) = self.vault.shared.origins.get(&profile).cloned() else {
            return;
        };
        let attempt = tab.attempt;
        let password = Zeroizing::new(secret.expose().to_owned());
        if let Some(tab) = self.tab_mut(tab_id) {
            tab.shared_password.pending = Some(PendingRemember {
                attempt,
                source,
                endpoint,
                password,
            });
        }
    }

    /// The password remembered for the server `kind` asks in `tab_id`, to answer it with.
    ///
    /// Given to the first try of an attempt only. Asked again after it was given, the server
    /// refused it: it is forgotten, and the user is asked.
    pub(super) fn shared_stored_answer(
        &mut self,
        tab_id: TabId,
        kind: &QuestionKind,
    ) -> Option<Answer> {
        let tab = self.tab(tab_id)?;
        let (profile, endpoint) = asked_endpoint(&tab.profile, kind)?;
        let source = self.vault.shared.origins.get(&profile)?.clone();
        let attempt = tab.attempt;
        let answered_before = tab
            .auto_answered
            .iter()
            .any(|(answered, who)| *answered == attempt && *who == profile);
        if answered_before {
            self.forget_shared_password(&source, &endpoint);
            return None;
        }
        if try_number(kind) > 1 || !self.vault.usable() {
            return None;
        }
        let saved = self.read_shared_password(&source, &endpoint)?;
        let tab = self.tab_mut(tab_id)?;
        tab.auto_answered.retain(|(earlier, _)| *earlier == attempt);
        tab.auto_answered.push((attempt, profile));
        tab.shared_password.used = Some(UsedPassword {
            attempt,
            source,
            endpoint,
        });
        Some(Answer::Secret(Secret::new(String::clone(&saved.password))))
    }

    /// The server of `tab_id` signed in: the password the user asked to remember is kept.
    pub(super) fn remember_signed_in_password(&mut self, tab_id: TabId) {
        let Some(tab) = self.tab_mut(tab_id) else {
            return;
        };
        let attempt = tab.attempt;
        tab.shared_password.used = None;
        let Some(pending) = tab
            .shared_password
            .pending
            .take()
            .filter(|pending| pending.attempt == attempt)
        else {
            return;
        };
        match self.remember_shared_password(&pending.source, &pending.endpoint, &pending.password) {
            Ok(()) => self.refresh_shared_password_counts(),
            Err(detail) => self.password_save_failed(&detail),
        }
    }

    /// The connection of `tab_id` failed. A password not yet accepted is dropped; a remembered
    /// one the server refused is forgotten.
    pub(super) fn shared_password_failed(&mut self, tab_id: TabId, error: &UiError) {
        let Some(tab) = self.tab_mut(tab_id) else {
            return;
        };
        let attempt = tab.attempt;
        tab.shared_password.pending = None;
        let used = tab
            .shared_password
            .used
            .take()
            .filter(|used| used.attempt == attempt);
        if let Some(used) = used
            && matches!(error, UiError::AuthenticationFailed { .. })
        {
            self.forget_shared_password(&used.source, &used.endpoint);
        }
    }

    /// The password remembered for `endpoint` in `source`, only if it was kept for exactly
    /// that protocol, host, port and login.
    fn read_shared_password(&self, source: &str, endpoint: &Endpoint) -> Option<SavedPassword> {
        let name = keys::entry_name(source, &keys::digest(endpoint));
        let saved = decode(&self.vault.read(&name)?)?;
        saved.endpoint.is(endpoint).then_some(saved)
    }

    /// Keeps `password` for `endpoint` in `source`, replacing any; the oldest of a source
    /// that holds [`MAX_PER_SOURCE`] goes.
    fn remember_shared_password(
        &mut self,
        source: &str,
        endpoint: &Endpoint,
        password: &str,
    ) -> Result<(), String> {
        let digest = keys::digest(endpoint);
        let name = keys::entry_name(source, &digest);
        let mut index = self.shared_index(source);
        index.retain(|known| *known != digest);
        let mut evicted = Vec::new();
        while index.len() >= MAX_PER_SOURCE {
            evicted.push(index.remove(0));
        }
        index.push(digest);
        let saved = SavedPassword {
            endpoint: keys::normalized(endpoint),
            password: Zeroizing::new(password.to_owned()),
        };
        self.vault.write(&name, Some(&encode(&saved)))?;
        let listed = self
            .write_lines(&keys::index_entry(source), &index)
            .and_then(|()| self.list_source(source, true));
        if let Err(detail) = listed {
            // Not listed is not kept: a password the source's Forget could not reach.
            let _ = self.vault.write(&name, None);
            return Err(detail);
        }
        for old in evicted {
            let _ = self.vault.write(&keys::entry_name(source, &old), None);
        }
        Ok(())
    }

    /// Forgets the password remembered for `endpoint` in `source`, if any.
    fn forget_shared_password(&mut self, source: &str, endpoint: &Endpoint) {
        let digest = keys::digest(endpoint);
        if !self.vault.usable() {
            return;
        }
        let mut index = self.shared_index(source);
        let name = keys::entry_name(source, &digest);
        if let Err(error) = self.vault.write(&name, None) {
            // Left listed: the source's Forget can still reach it.
            log::warn!("a remembered shared password could not be forgotten: {error}");
            return;
        }
        index.retain(|known| *known != digest);
        self.save_index(source, &index);
        self.refresh_shared_password_counts();
    }

    /// Forgets every password remembered for `source`, as the Forget button of the Settings
    /// does. A locked vault is asked to be unlocked first.
    pub(super) fn forget_shared_passwords(&mut self, source: &str) {
        if !self.vault.usable() {
            self.show_vault_if_locked();
            return;
        }
        self.forget_source_entries(source);
        self.refresh_shared_password_counts();
    }

    fn forget_source_entries(&mut self, source: &str) {
        for digest in self.shared_index(source) {
            if let Err(error) = self.vault.write(&keys::entry_name(source, &digest), None) {
                log::warn!("a remembered shared password could not be forgotten: {error}");
            }
        }
        self.save_index(source, &[]);
    }

    /// Writes `source`'s index, or removes it (and the source from the list) when empty.
    fn save_index(&mut self, source: &str, index: &[String]) {
        let written = self
            .write_lines(&keys::index_entry(source), index)
            .and_then(|()| self.list_source(source, !index.is_empty()));
        if let Err(error) = written {
            log::warn!("the remembered shared passwords could not be updated: {error}");
        }
    }

    /// Adds `source` to the list of sources that have remembered passwords, or takes it out.
    fn list_source(&mut self, source: &str, listed: bool) -> Result<(), String> {
        let mut sources = self.sources_with_passwords();
        let was = sources.iter().any(|known| known == source);
        if was == listed {
            return Ok(());
        }
        if listed {
            sources.push(source.to_owned());
        } else {
            sources.retain(|known| known != source);
        }
        self.write_lines(SOURCES_ENTRY, &sources)
    }

    fn shared_index(&self, source: &str) -> Vec<String> {
        self.vault
            .read(&keys::index_entry(source))
            .and_then(|bytes| decode_index(&bytes))
            .unwrap_or_default()
    }

    fn sources_with_passwords(&self) -> Vec<String> {
        self.vault
            .read(SOURCES_ENTRY)
            .and_then(|bytes| decode_sources(&bytes))
            .unwrap_or_default()
    }

    /// Writes `lines` as entry `name`, or removes the entry for none.
    fn write_lines(&mut self, name: &str, lines: &[String]) -> Result<(), String> {
        if lines.is_empty() {
            self.vault.write(name, None)
        } else {
            self.vault.write(name, Some(&encode_lines(lines)))
        }
    }

    /// The name of every entry the remembered shared passwords occupy in the system's store,
    /// for moving them into a vault being created; the system's store cannot be listed. Read
    /// from the system's store itself: the vault file already exists by then, still closed.
    pub(super) fn shared_password_entries(&self) -> Vec<String> {
        let read = |name: &str| self.vault.read_system(name);
        let sources = read(SOURCES_ENTRY)
            .and_then(|bytes| decode_sources(&bytes))
            .unwrap_or_default();
        if sources.is_empty() {
            return Vec::new();
        }
        let mut names = vec![SOURCES_ENTRY.to_owned()];
        for source in &sources {
            names.push(keys::index_entry(source));
            let index = read(&keys::index_entry(source))
                .and_then(|bytes| decode_index(&bytes))
                .unwrap_or_default();
            names.extend(index.iter().map(|digest| keys::entry_name(source, digest)));
        }
        names
    }

    /// Counts again what is remembered per source listed in the settings; none while the
    /// store cannot be read.
    pub(super) fn refresh_shared_password_counts(&mut self) {
        let counts = if self.vault.usable() {
            self.settings
                .shared_sources
                .iter()
                .map(|source| (source.id.clone(), self.shared_index(&source.id).len()))
                .filter(|(_, count)| *count > 0)
                .collect()
        } else {
            HashMap::new()
        };
        self.vault.shared.counts = counts;
    }

    /// Follows the settings' list of sources: the passwords of a source taken out are
    /// forgotten, as the C# `ForgetAllExceptAsync`, and the counts for the Settings kept.
    ///
    /// Nothing is read before the list changes, or while no source is configured at start, so
    /// an application that never used shared sessions never touches the credential store for
    /// them.
    pub(super) fn sync_shared_passwords(&mut self) {
        let readable = self.vault.usable();
        if std::mem::replace(&mut self.vault.shared.readable, readable) != readable {
            self.refresh_shared_password_counts();
        }
        let listed: Vec<String> = self
            .settings
            .shared_sources
            .iter()
            .map(|source| source.id.clone())
            .collect();
        let before = self.vault.shared.sources_seen.replace(listed.clone());
        match &before {
            None if listed.is_empty() => return,
            Some(seen) if *seen == listed && !self.vault.shared.cleanup_pending => return,
            _ => {}
        }
        if !self.vault.usable() {
            self.vault.shared.cleanup_pending = true;
            return;
        }
        self.vault.shared.cleanup_pending = false;
        for source in self.sources_with_passwords() {
            if !listed.contains(&source) {
                self.forget_source_entries(&source);
            }
        }
        self.refresh_shared_password_counts();
    }
}
