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

//! The password of several profiles at once, as the C# tree's "Edit" menu of a selection
//! offers it: typed twice, then saved for each profile at its own server and account, in one
//! write of the vault. Nothing of it goes to the profiles file.

use heimdall_core::profile::ProfileId;
use heimdall_ssh::Secret;

use super::bulk_edit::BULK_EDIT_MINIMUM;
use super::tree::ProfileKind;
use super::{App, Dialog, Effect, Notice};

/// Why a password typed for several profiles is refused, as the C# dialog says it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BulkPasswordRefusal {
    /// It holds a control character.
    Control,
    /// The two typed differ.
    Mismatch,
}

/// The profiles selected that a bulk password leaves alone, counted by why.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BulkPasswordSkips {
    /// `WinRM` profiles naming no account: the current Windows identity has no password.
    pub winrm: usize,
    /// SSH, SFTP and RDP profiles naming no account: a password saved for them needs one,
    /// as the profile editor says.
    pub no_account: usize,
    /// Profiles whose protocol has no saved password: Telnet, a local shell, Citrix.
    pub other: usize,
}

impl BulkPasswordSkips {
    /// Whether no profile selected is left alone.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// Where a profile of `kind` stands for a bulk password.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Eligibility {
    /// Its password is saved.
    Takes,
    /// Left alone, a `WinRM` profile naming no account.
    WinRm,
    /// Left alone, without the account its password needs.
    NoAccount,
    /// Left alone, a protocol without a saved password.
    Other,
}

/// Where a profile of `kind`, naming an account or not, stands for a bulk password.
///
/// As the C# `CanBulkEditPasswordTarget`: RDP, SSH, SFTP, FTP and VNC, and `WinRM` when it
/// names an account; a `WinRM` profile naming none is left alone and counted apart.
fn eligibility(kind: ProfileKind, has_account: bool) -> Eligibility {
    match kind {
        ProfileKind::Ssh | ProfileKind::Sftp | ProfileKind::Rdp if !has_account => {
            Eligibility::NoAccount
        }
        ProfileKind::WinRm if !has_account => Eligibility::WinRm,
        ProfileKind::Ssh
        | ProfileKind::Sftp
        | ProfileKind::Rdp
        | ProfileKind::Ftp
        | ProfileKind::Vnc
        | ProfileKind::WinRm => Eligibility::Takes,
        ProfileKind::Telnet | ProfileKind::Local | ProfileKind::Citrix => Eligibility::Other,
    }
}

/// Why `password`, typed again as `confirm`, is refused, if it is: as the C# dialog checks
/// it, a control character first, then the two typed alike.
#[must_use]
pub fn bulk_password_refusal(password: &str, confirm: &str) -> Option<BulkPasswordRefusal> {
    if password.chars().any(char::is_control) {
        Some(BulkPasswordRefusal::Control)
    } else if password != confirm {
        Some(BulkPasswordRefusal::Mismatch)
    } else {
        None
    }
}

impl App {
    /// The profiles of `ids` whose password a bulk edit saves, and those it leaves alone.
    fn bulk_password_split(&self, ids: &[ProfileId]) -> (Vec<ProfileId>, BulkPasswordSkips) {
        let mut targets = Vec::new();
        let mut skips = BulkPasswordSkips::default();
        for profile in ids.iter().filter_map(|id| self.profile_summary(id)) {
            let has_account = profile
                .username
                .as_deref()
                .is_some_and(|username| !username.is_empty());
            match eligibility(profile.kind, has_account) {
                Eligibility::Takes => targets.push(profile.id),
                Eligibility::WinRm => skips.winrm += 1,
                Eligibility::NoAccount => skips.no_account += 1,
                Eligibility::Other => skips.other += 1,
            }
        }
        (targets, skips)
    }

    /// How many of `ids` a bulk password is saved for: the count the menu entry shows.
    #[must_use]
    pub fn bulk_password_targets(&self, ids: &[ProfileId]) -> usize {
        self.bulk_password_split(ids).0.len()
    }

    /// Opens the dialog setting the password of the profiles selected that take one: only
    /// for two selected or more, and while passwords can be saved. With none taking one, the
    /// `WinRM` profiles left alone are said, as the C# does.
    pub(super) fn open_bulk_password(&mut self) {
        let selected = self.selected_profiles();
        if selected.len() < BULK_EDIT_MINIMUM || !self.can_save_passwords() {
            return;
        }
        let (ids, skipped) = self.bulk_password_split(&selected);
        if ids.is_empty() {
            log::info!(
                "a bulk password was not offered: {} profile(s) selected, none take one",
                selected.len()
            );
            if skipped.winrm > 0 {
                self.tell(Notice::BulkPasswordWinRmSkipped(skipped.winrm));
            }
            return;
        }
        self.dialog = Some(Dialog::BulkPassword {
            ids,
            skipped,
            refused: None,
        });
    }

    /// The fields of the bulk password dialog changed: the last refusal no longer holds.
    pub(super) fn bulk_password_edited(&mut self) -> Vec<Effect> {
        if let Some(Dialog::BulkPassword { refused, .. }) = self.dialog.as_mut() {
            *refused = None;
        }
        Vec::new()
    }

    /// Saves `password`, typed again as `confirm`, for the profiles of the bulk password
    /// dialog, and says how many; the dialog stays, saying why, when it is refused. An empty
    /// password does nothing, as the C# Apply stays disabled.
    pub(super) fn set_bulk_password(&mut self, password: &Secret, confirm: &Secret) {
        let Some(Dialog::BulkPassword { ids, skipped, .. }) = self.dialog.as_ref() else {
            return;
        };
        if password.expose().is_empty() {
            return;
        }
        if let Some(refusal) = bulk_password_refusal(password.expose(), confirm.expose()) {
            if let Some(Dialog::BulkPassword { refused, .. }) = self.dialog.as_mut() {
                *refused = Some(refusal);
            }
            return;
        }
        let (ids, skipped) = (ids.clone(), *skipped);
        self.dialog = None;
        let total = ids.len();
        let saved = self.save_passwords(&ids, password);
        match saved {
            Ok(count) => self.tell(Notice::BulkPasswordUpdated {
                count,
                winrm_skipped: skipped.winrm,
            }),
            Err((0, detail)) => self.password_save_failed(&detail),
            Err((count, detail)) => {
                self.tell(Notice::BulkPasswordPartial { count, total });
                self.password_save_failed(&detail);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_winrm_profile_takes_one_only_with_an_account() {
        assert_eq!(eligibility(ProfileKind::WinRm, true), Eligibility::Takes);
        assert_eq!(eligibility(ProfileKind::WinRm, false), Eligibility::WinRm);
    }

    #[test]
    fn ftp_and_vnc_take_one_without_an_account() {
        assert_eq!(eligibility(ProfileKind::Ftp, false), Eligibility::Takes);
        assert_eq!(eligibility(ProfileKind::Vnc, false), Eligibility::Takes);
        assert_eq!(
            eligibility(ProfileKind::Sftp, false),
            Eligibility::NoAccount
        );
        assert_eq!(eligibility(ProfileKind::Telnet, true), Eligibility::Other);
    }

    #[test]
    fn a_control_character_is_refused_before_a_mismatch() {
        assert_eq!(
            bulk_password_refusal("a\tb", "other"),
            Some(BulkPasswordRefusal::Control)
        );
        assert_eq!(
            bulk_password_refusal("secret", "Secret"),
            Some(BulkPasswordRefusal::Mismatch)
        );
        assert_eq!(bulk_password_refusal("secret", "secret"), None);
    }
}
