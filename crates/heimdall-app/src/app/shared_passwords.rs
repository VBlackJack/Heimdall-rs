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
use super::Dialog;
use super::vault::{VaultState, asked_endpoint, try_number};
use crate::error::UiError;
use crate::event::{Answer, QuestionKind};
use crate::ids::{AttemptId, QuestionId, TabId};

/// The file beside the settings that says a reset's forgetting of the remembered passwords is
/// not done: it holds no credential data.
const PURGE_MARKER_FILE: &str = "shared-passwords-purge.pending";

/// What a marker file that could not be removed is overwritten with: any content says the
/// deletion it stood for is done.
const MARKER_DONE: &[u8] = b"done";

/// The file beside the settings that says the passwords of a source taken out are not all
/// forgotten yet: it holds no credential data.
const CLEANUP_MARKER_FILE: &str = "shared-passwords-cleanup.pending";

/// A password the user asked to remember, until the server accepts it.
struct PendingRemember {
    attempt: AttemptId,
    source: String,
    endpoint: Endpoint,
    password: Zeroizing<String>,
    /// The source's revocation token when the user asked: a Forget, a removal of the source
    /// or a reset since then makes it stale, and the password is never committed.
    token: RevocationToken,
}

/// What the consent to remember a password was given under: the source's own count of
/// revocations and the count of resets of every setting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RevocationToken {
    source: u64,
    epoch: u64,
}

/// How often the consent to remember was withdrawn, so that a password still waiting for its
/// server cannot be kept after the user said to forget.
#[derive(Debug, Default)]
struct Revocations {
    sources: HashMap<String, u64>,
    epoch: u64,
}

impl Revocations {
    fn token(&self, source: &str) -> RevocationToken {
        RevocationToken {
            source: self.sources.get(source).copied().unwrap_or(0),
            epoch: self.epoch,
        }
    }

    fn revoke(&mut self, source: &str) {
        let count = self.sources.entry(source.to_owned()).or_insert(0);
        *count = count.wrapping_add(1);
    }

    fn revoke_all(&mut self) {
        self.epoch = self.epoch.wrapping_add(1);
    }
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

impl TabPasswords {
    /// Forgets the password waiting for its server and the remembered one given.
    pub(super) fn clear(&mut self) {
        self.pending = None;
        self.used = None;
    }
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
// Four independent flags, each a fact about the store, not states of one machine.
#[allow(clippy::struct_excessive_bools)]
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
    /// Consent withdrawn, per source and for every source at a reset.
    revocations: Revocations,
    /// A reset to defaults forgets every remembered password; waits for the store to be
    /// readable, and for every deletion to succeed.
    purge_pending: bool,
    /// The user was told that some remembered passwords could not be deleted, until a pass
    /// deletes them all: a retry on the timer does not ask again.
    incomplete_reported: bool,
    /// The user was told that a clean-up of the sources taken out waits and could not be noted
    /// for a restart, until it is done.
    cleanup_unrecorded_reported: bool,
}

impl App {
    /// How many passwords are remembered for the shared source `source`, as far as can be
    /// read now: none while a vault is locked.
    #[must_use]
    pub fn shared_password_count(&self, source: &str) -> usize {
        self.vault.shared.counts.get(source).copied().unwrap_or(0)
    }

    /// Whether the settings still list the shared source `source`: a source taken out keeps
    /// no password, whatever a tab opened from it still believes.
    fn source_listed(&self, source: &str) -> bool {
        self.settings
            .shared_sources
            .iter()
            .any(|listed| listed.id == source)
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
        asked_endpoint(&tab.profile, kind).is_some_and(|(profile, _)| {
            self.vault
                .shared
                .origins
                .get(&profile)
                .is_some_and(|source| self.source_listed(source))
        }) && self.vault.usable()
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
        let Some(source) = self
            .vault
            .shared
            .origins
            .get(&profile)
            .filter(|source| self.source_listed(source))
            .cloned()
        else {
            return;
        };
        let attempt = tab.attempt;
        let token = self.vault.shared.revocations.token(&source);
        let password = Zeroizing::new(secret.expose().to_owned());
        if let Some(tab) = self.tab_mut(tab_id) {
            tab.shared_password.pending = Some(PendingRemember {
                attempt,
                source,
                endpoint,
                password,
                token,
            });
        }
    }

    /// A question `kind` reached `tab_id`. A password typed in this attempt that the server
    /// asks again (a password question past the first try, the account's password alone in
    /// a keyboard-interactive round, a VNC password again) shows the server refused it: it is
    /// dropped at once, so that a later sign-in by another means never keeps a password
    /// that was refused. A second factor (a one-time code, a key's passphrase) after it is
    /// no refusal: the password was accepted as the first step and is kept once signed in.
    pub(super) fn shared_question_asked(&mut self, tab_id: TabId, kind: &QuestionKind) {
        let refusal = match kind {
            QuestionKind::Password(_) => try_number(kind) > 1,
            QuestionKind::ServerPassword(_) => true,
            QuestionKind::KeyboardInteractive(round) => {
                heimdall_ssh::asks_for_the_account_password(round)
            }
            QuestionKind::Username(_) | QuestionKind::Passphrase(_) => false,
        };
        if refusal && let Some(tab) = self.tab_mut(tab_id) {
            tab.shared_password.pending = None;
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
        self.settle_pending_purge();
        if self.vault.shared.purge_pending {
            return None;
        }
        let tab = self.tab(tab_id)?;
        let (profile, endpoint) = asked_endpoint(&tab.profile, kind)?;
        let source = self
            .vault
            .shared
            .origins
            .get(&profile)
            .filter(|source| self.source_listed(source))?
            .clone();
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
        self.settle_pending_purge();
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
        // Forgotten, or the source taken out, or every setting reset, while the server was
        // still answering: the consent is gone with it.
        if pending.token != self.vault.shared.revocations.token(&pending.source)
            || !self.source_listed(&pending.source)
        {
            return;
        }
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
    /// that holds [`MAX_PER_SOURCE`] goes, and if one cannot be deleted nothing is kept.
    fn remember_shared_password(
        &mut self,
        source: &str,
        endpoint: &Endpoint,
        password: &str,
    ) -> Result<(), String> {
        let digest = keys::digest(endpoint);
        let name = keys::entry_name(source, &digest);
        let previous = self.shared_index(source)?;
        let mut index = previous.clone();
        index.retain(|known| *known != digest);
        let mut evicted = Vec::new();
        while index.len() >= MAX_PER_SOURCE {
            evicted.push(index.remove(0));
        }
        // The room is made first: an entry whose deletion failed stays listed, so that the
        // source's Forget still reaches it and it is never kept unseen.
        let mut stuck = Vec::new();
        for old in &evicted {
            if let Err(error) = self.vault.write(&keys::entry_name(source, old), None) {
                log::warn!("a credential store operation failed: {error}");
                stuck.push(old.clone());
            }
        }
        if !stuck.is_empty() {
            let mut listed = previous;
            listed.retain(|known| !evicted.contains(known) || stuck.contains(known));
            self.save_index(source, &listed);
            return Err("an older entry could not be deleted".to_owned());
        }
        index.push(digest);
        // Listed first, kept second: whatever fails after the listing leaves a name with no
        // password behind it, never a password the source's Forget cannot reach.
        let listed = self
            .write_lines(&keys::index_entry(source), &index)
            .and_then(|()| self.list_source(source, true));
        if let Err(detail) = listed {
            self.save_index(source, &previous);
            return Err(detail);
        }
        let saved = SavedPassword {
            endpoint: keys::normalized(endpoint),
            password: Zeroizing::new(password.to_owned()),
        };
        if let Err(detail) = self.vault.write(&name, Some(&encode(&saved))) {
            self.save_index(source, &previous);
            return Err(detail);
        }
        Ok(())
    }

    /// Forgets the password remembered for `endpoint` in `source`, if any.
    fn forget_shared_password(&mut self, source: &str, endpoint: &Endpoint) {
        let digest = keys::digest(endpoint);
        if !self.vault.usable() {
            return;
        }
        let Ok(mut index) = self.shared_index(source) else {
            // Left as it is: an index that could not be read is not an empty one.
            log::warn!("a credential store operation failed: the store is unreadable");
            return;
        };
        let name = keys::entry_name(source, &digest);
        if let Err(error) = self.vault.write(&name, None) {
            // Left listed: the source's Forget can still reach it.
            log::warn!("a credential store operation failed: {error}");
            return;
        }
        index.retain(|known| *known != digest);
        self.save_index(source, &index);
        self.refresh_shared_password_counts();
    }

    /// Forgets every password remembered for `source`, as the Forget button of the Settings
    /// does, and withdraws the consent of those still waiting for their server. A locked
    /// vault is asked to be unlocked first; a deletion that fails leaves its entry listed, is
    /// said, and is tried again by the next Forget.
    pub(super) fn forget_shared_passwords(&mut self, source: &str) {
        self.revoke_source_consent(source);
        if !self.vault.usable() {
            self.show_vault_if_locked();
            return;
        }
        let remaining = self.forget_source_entries(source);
        self.refresh_shared_password_counts();
        self.report_incomplete_forget(remaining, true);
    }

    /// Withdraws the consent to remember given for `source`: what waits for a server's answer
    /// is dropped, and never kept when the server answers.
    fn revoke_source_consent(&mut self, source: &str) {
        self.vault.shared.revocations.revoke(source);
        for tab in &mut self.tabs {
            if tab
                .shared_password
                .pending
                .as_ref()
                .is_some_and(|pending| pending.source == source)
            {
                tab.shared_password.pending = None;
            }
        }
    }

    /// Deletes every entry of `source`; those whose deletion failed stay in its index, and
    /// their number is returned.
    fn forget_source_entries(&mut self, source: &str) -> usize {
        let Ok(index) = self.shared_index(source) else {
            log::warn!("a credential store operation failed: the store is unreadable");
            return 1;
        };
        let mut kept = Vec::new();
        for digest in index {
            if let Err(error) = self.vault.write(&keys::entry_name(source, &digest), None) {
                log::warn!("a credential store operation failed: {error}");
                kept.push(digest);
            }
        }
        self.save_index(source, &kept);
        kept.len()
    }

    /// Says that `remaining` remembered passwords could not be deleted and will be tried
    /// again; a pass that deleted everything re-arms the notice. A retry on the timer says it
    /// once only, the user's own Forget always.
    fn report_incomplete_forget(&mut self, remaining: usize, asked: bool) {
        if remaining == 0 {
            self.vault.shared.incomplete_reported = false;
            return;
        }
        if (asked || !self.vault.shared.incomplete_reported) && self.dialog.is_none() {
            self.dialog = Some(Dialog::PasswordForgetFailed { remaining });
        }
        self.vault.shared.incomplete_reported = true;
    }

    /// A reset to defaults: every remembered shared password goes, and none waiting for its
    /// server is kept. A locked vault defers the deletion until it is opened.
    pub(super) fn purge_shared_passwords(&mut self) {
        self.vault.shared.revocations.revoke_all();
        for tab in &mut self.tabs {
            tab.shared_password.pending = None;
            tab.shared_password.used = None;
        }
        let noted = self.set_purge_pending(true);
        self.sync_shared_passwords();
        // A deletion still waiting that a restart would not take up: the user is told, since
        // nothing else will.
        if !noted && self.vault.shared.purge_pending && self.dialog.is_none() {
            self.dialog = Some(Dialog::PasswordPurgeUnrecorded);
        }
    }

    /// Whether a deletion of remembered passwords (a reset's, or a source taken out's) still
    /// waits for the store, and so for the timer.
    pub(super) fn shared_deletion_pending(&self) -> bool {
        self.vault.shared.purge_pending || self.vault.shared.cleanup_pending
    }

    /// Notes that a reset's deletion waits, or is done. A marker file beside the settings
    /// keeps it across a restart: the reset keeps the sources listed, so nothing else would
    /// tell the next start that their passwords were to go.
    fn set_purge_pending(&mut self, pending: bool) -> bool {
        self.vault.shared.purge_pending = pending;
        self.set_marker(PURGE_MARKER_FILE, pending)
    }

    /// Notes that the forgetting of the passwords of a source taken out waits, or is done;
    /// kept across a restart by `persist`, as [`Self::set_purge_pending`] does: the source
    /// is no longer in the settings, so nothing else would tell the next start.
    fn set_cleanup_pending(&mut self, pending: bool, persist: bool) {
        self.vault.shared.cleanup_pending = pending;
        if !pending {
            self.vault.shared.cleanup_unrecorded_reported = false;
        }
        if (persist || !pending) && !self.set_marker(CLEANUP_MARKER_FILE, pending) {
            // A deletion still waiting that a restart would not take up: said once, since
            // nothing else will.
            let shared = &mut self.vault.shared;
            if pending && !shared.cleanup_unrecorded_reported && self.dialog.is_none() {
                shared.cleanup_unrecorded_reported = true;
                self.dialog = Some(Dialog::PasswordPurgeUnrecorded);
            }
        }
    }

    /// Creates or removes the marker file `name` beside the settings; whether it was done.
    fn set_marker(&self, name: &str, present: bool) -> bool {
        let Some(marker) = self.settings_file.parent().map(|folder| folder.join(name)) else {
            return false;
        };
        let done = if present {
            std::fs::write(&marker, b"")
        } else {
            match std::fs::remove_file(&marker) {
                // A marker that cannot be removed is emptied of its meaning instead: only an
                // empty one says a deletion waits, so a later start does not wipe what was
                // remembered after this one.
                Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
                    std::fs::write(&marker, MARKER_DONE).map_err(|_| error)
                }
                _ => Ok(()),
            }
        };
        if let Err(error) = &done {
            log::warn!("a file beside the settings could not be written: {error}");
        }
        done.is_ok()
    }

    /// Takes up the deletions that a previous run could not finish.
    pub(super) fn resume_shared_purge(&mut self) {
        let marked = |name: &str| {
            self.settings_file.parent().is_some_and(|folder| {
                std::fs::read(folder.join(name)).is_ok_and(|content| content.is_empty())
            })
        };
        let (purge, cleanup) = (marked(PURGE_MARKER_FILE), marked(CLEANUP_MARKER_FILE));
        self.vault.shared.purge_pending |= purge;
        self.vault.shared.cleanup_pending |= cleanup;
    }

    /// Finishes a reset's deletion that was waiting for the store, if it can be done now:
    /// no remembered password is read or committed before a reset has taken its effect.
    fn settle_pending_purge(&mut self) {
        if self.vault.shared.purge_pending && self.vault.usable() {
            self.purge_listed_passwords();
        }
    }

    /// Writes `source`'s index, or removes it (and the source from the list) when empty.
    fn save_index(&mut self, source: &str, index: &[String]) {
        let written = self
            .write_lines(&keys::index_entry(source), index)
            .and_then(|()| self.list_source(source, !index.is_empty()));
        if let Err(error) = written {
            log::warn!("a credential store operation failed: {error}");
        }
    }

    /// Adds `source` to the list of sources that have remembered passwords, or takes it out.
    fn list_source(&mut self, source: &str, listed: bool) -> Result<(), String> {
        let mut sources = self.sources_with_passwords()?;
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

    /// `source`'s index; an error when the store could not be read, which is not an empty
    /// index: deleting by it would orphan every entry it lists.
    fn shared_index(&self, source: &str) -> Result<Vec<String>, String> {
        match self.vault.read_checked(&keys::index_entry(source))? {
            None => Ok(Vec::new()),
            // A newer version's or a damaged index is not an empty one: its entries would be
            // lost to the clean-up, and the cleanup stays pending.
            Some(bytes) => decode_index(&bytes).ok_or_else(|| "the index is unreadable".to_owned()),
        }
    }

    /// The sources that have remembered passwords; an error when the store could not be read.
    fn sources_with_passwords(&self) -> Result<Vec<String>, String> {
        match self.vault.read_checked(SOURCES_ENTRY)? {
            None => Ok(Vec::new()),
            Some(bytes) => {
                decode_sources(&bytes).ok_or_else(|| "the list of sources is unreadable".to_owned())
            }
        }
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
        system_entry_names(&self.vault)
    }

    /// Counts again what is remembered per source listed in the settings; none while the
    /// store cannot be read.
    pub(super) fn refresh_shared_password_counts(&mut self) {
        let counts = if self.vault.usable() {
            self.settings
                .shared_sources
                .iter()
                .map(|source| {
                    let count = self.shared_index(&source.id).map_or_else(
                        // Unreadable now is not none: the Forget button stays.
                        |_| {
                            self.vault
                                .shared
                                .counts
                                .get(&source.id)
                                .copied()
                                .unwrap_or(0)
                        },
                        |index| index.len(),
                    );
                    (source.id.clone(), count)
                })
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
        // A tab opened from a source since taken out must not keep offering, saving or
        // answering a password under it.
        self.vault
            .shared
            .origins
            .retain(|_, source| listed.contains(source));
        let before = self.vault.shared.sources_seen.replace(listed.clone());
        // A source taken out withdraws its consent at once, whether or not the store can be
        // read yet.
        for gone in before.iter().flatten().filter(|id| !listed.contains(id)) {
            self.revoke_source_consent(gone);
        }
        self.settle_pending_purge();
        match &before {
            None if listed.is_empty() && !self.vault.shared.cleanup_pending => return,
            Some(seen) if *seen == listed && !self.vault.shared.cleanup_pending => return,
            _ => {}
        }
        if !self.vault.usable() {
            // Only a source really taken out is worth keeping across a restart.
            let removed = before.iter().flatten().any(|id| !listed.contains(id));
            let persist = removed || self.vault.shared.cleanup_pending;
            self.set_cleanup_pending(true, persist);
            return;
        }
        let mut remaining = 0;
        match self.sources_with_passwords() {
            Ok(sources) => {
                for source in sources.iter().filter(|source| !listed.contains(source)) {
                    remaining += self.forget_source_entries(source);
                }
            }
            // Unknown: tried again at the next look.
            Err(_) => remaining += 1,
        }
        // Kept listed, they are tried again at the next look.
        self.set_cleanup_pending(remaining > 0, true);
        self.report_incomplete_forget(remaining, false);
        self.refresh_shared_password_counts();
    }

    /// Deletes the passwords of every source, as a reset to defaults asks; what could not be
    /// deleted keeps the purge waiting for the next look.
    fn purge_listed_passwords(&mut self) {
        let mut remaining = 0;
        let mut sources = self.sources_with_passwords().unwrap_or_else(|_| {
            // The list could not be read: the sources of the settings only, and not done.
            remaining += 1;
            Vec::new()
        });
        for source in &self.settings.shared_sources {
            if !sources.contains(&source.id) {
                sources.push(source.id.clone());
            }
        }
        for source in sources {
            remaining += self.forget_source_entries(&source);
        }
        self.set_purge_pending(remaining > 0);
        self.report_incomplete_forget(remaining, false);
        self.refresh_shared_password_counts();
    }
}

/// Whether `name` is an entry of the remembered shared passwords: one password, an index or
/// the list of sources.
pub(super) fn is_shared_password_entry(name: &str) -> bool {
    name == SOURCES_ENTRY || keys::is_entry_or_index(name)
}

/// The name of every entry the remembered shared passwords occupy in the system's store, as
/// far as its own list of sources and indexes tell; read from the system's store itself,
/// whether or not a vault exists.
pub(super) fn system_entry_names(vault: &VaultState) -> Vec<String> {
    let read = |name: &str| vault.read_system(name);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_attempt_clears_what_was_typed_for_the_one_before() {
        let mut passwords = TabPasswords {
            pending: Some(PendingRemember {
                attempt: AttemptId::fresh(),
                source: "team".to_owned(),
                endpoint: Endpoint {
                    protocol: heimdall_core::credentials::CredentialProtocol::Vnc,
                    host: "h".to_owned(),
                    port: 5900,
                    username: None,
                },
                password: Zeroizing::new("typed".to_owned()),
                token: RevocationToken {
                    source: 0,
                    epoch: 0,
                },
            }),
            used: None,
        };
        assert!(passwords.pending.is_some());
        passwords.clear();
        assert!(passwords.pending.is_none());
    }
}
