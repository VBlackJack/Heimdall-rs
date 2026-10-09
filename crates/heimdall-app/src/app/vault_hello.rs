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

//! Windows Hello unlocking the vault, as the C# `VaultLifecycleService` enrols, unlocks and
//! removes it, its unlock dialog offers it (`VaultUnlockDialogViewModel`,
//! `VaultHelloUnlockUx`) and its settings card shows it (`SettingsViewModel`):
//! - the unlock dialog, at start and on the lock screen, offers it while an envelope is
//!   enrolled and the master password is not due again; tried, a failure says what the C#
//!   says, a prompt dismissed says nothing, and the master password stays to type; it does
//!   not count as a wrong master password;
//! - a credential gone (Windows Hello reset or removed) offers, once the master password
//!   opened the vault, to enrol again;
//! - the settings enrol it while the vault is open, and remove it, its credential with it;
//! - the master password opening the vault notes when, for the days the settings allow
//!   before asking it again; Windows Hello opening it does not;
//! - locking keeps it; changing the master password keeps it, the data key being the same;
//!   removing the master password removes it, as the C# `DisableAsync`; a vault created
//!   anew drops any left from a vault deleted outside the application.

use std::time::SystemTime;

use super::vault::VaultProblem;
use super::{App, Dialog, Effect, Notice, VaultMode, VaultStatus};
use crate::OpenedVault;
use crate::vault_hello::{Envelope, HelloFailure, master_password_due};

/// A step of Windows Hello for the vault.
#[derive(Debug, Clone)]
pub enum VaultHelloMessage {
    /// The settings are shown: whether it can be enrolled is asked again.
    Refresh,
    /// Whether it can be enrolled here, as Windows answered.
    Checked(bool),
    /// Enrol it, from the settings.
    Enable,
    /// Remove it and its credential, from the settings.
    Disable,
    /// Unlock with it, from the unlock dialog.
    Unlock,
    /// The enrolment ended.
    Enrolled(Result<Envelope, HelloFailure>),
    /// The unlock ended.
    Unlocked(Result<OpenedVault, HelloFailure>),
}

/// What the settings say of Windows Hello for the vault, as the C# `VaultHelloStatusText`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VaultHelloStatus {
    /// It unlocks the vault.
    Enabled,
    /// It can be enrolled.
    Available,
    /// It cannot: no Windows Hello, no TPM 2.0, or the last enrolment failed.
    Unavailable,
    /// It is being enrolled.
    Enrolling,
    /// Enrolling needs the vault open with the master password first.
    UnlockRequired,
}

/// The settings card of Windows Hello for the vault, as it stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VaultHelloCard {
    /// What is said; nothing without a vault, or while Windows is being asked.
    pub status: Option<VaultHelloStatus>,
    /// "Enable Windows Hello unlock" can be pressed.
    pub can_enable: bool,
    /// "Disable Windows Hello unlock" can be pressed.
    pub can_disable: bool,
}

/// What an enrolment was asked from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Enrolling {
    /// The settings' button.
    Settings,
    /// The offer after the master password, the credential having gone.
    Again,
}

/// Windows Hello for the vault, in this run.
#[derive(Debug, Default)]
pub(super) struct VaultHelloState {
    /// The envelope enrolled for this vault, as the system's store keeps it.
    envelope: Option<Envelope>,
    /// Whether it can be enrolled here; `None` until Windows answered.
    available: Option<bool>,
    /// Windows is being asked whether it can.
    checking: bool,
    /// An enrolment is under way.
    enrolling: Option<Enrolling>,
    /// What the last "Enable" came to, said until the settings are shown again: the vault
    /// closed, or the enrolment failed.
    said: Option<VaultHelloStatus>,
    /// The credential was found gone: enrolling again is offered after the master password.
    again_pending: bool,
}

impl VaultHelloState {
    /// Whether an enrolment is under way: the master password is then neither changed nor
    /// removed until it ends.
    pub(super) fn enrolling(&self) -> bool {
        self.enrolling.is_some()
    }

    /// What the system's store keeps for the vault, `envelope`.
    pub(super) fn with(envelope: Option<Envelope>) -> Self {
        Self {
            envelope,
            ..Self::default()
        }
    }
}

/// What a failed unlock says in the dialog, as the C# `VaultHelloUnlockUx.Map`: nothing for
/// a prompt dismissed or the password preferred, the master password left to type.
fn unlock_problem(failure: HelloFailure) -> Option<VaultProblem> {
    match failure {
        HelloFailure::UserCanceled | HelloFailure::UserPrefersPassword => None,
        HelloFailure::SecurityDeviceLocked => Some(VaultProblem::HelloLocked),
        HelloFailure::NotFound => Some(VaultProblem::HelloNotFound),
        HelloFailure::Unavailable | HelloFailure::CryptoFailure | HelloFailure::TimedOut => {
            Some(VaultProblem::HelloFailed)
        }
    }
}

impl App {
    /// Applies a step of Windows Hello for the vault.
    pub(super) fn vault_hello_message(&mut self, message: VaultHelloMessage) -> Vec<Effect> {
        match message {
            VaultHelloMessage::Refresh => self.refresh_vault_hello(),
            VaultHelloMessage::Checked(available) => {
                self.vault_hello.checking = false;
                self.vault_hello.available = Some(available);
                Vec::new()
            }
            VaultHelloMessage::Enable => self.enable_vault_hello(Enrolling::Settings),
            VaultHelloMessage::Disable => self.disable_vault_hello(),
            VaultHelloMessage::Unlock => self.unlock_with_hello(),
            VaultHelloMessage::Enrolled(result) => self.vault_hello_enrolled(result),
            VaultHelloMessage::Unlocked(result) => self.unlocked_with_hello(result),
        }
    }

    /// Whether the unlock dialog offers Windows Hello now, as the C#
    /// `VaultHelloUnlockOfferPolicy`: enrolled, and the master password not due again.
    pub(super) fn vault_hello_offered(&self) -> bool {
        let settings = &self.settings;
        self.vault_hello.envelope.is_some()
            && !master_password_due(
                settings.vault_last_master_unlock,
                settings.windows_hello.vault_max_days,
                SystemTime::now(),
            )
    }

    /// The settings card of Windows Hello for the vault. Unlike the C#, which shows
    /// "Disable" only where Windows Hello is available, an enrolment can always be removed.
    #[must_use]
    pub fn vault_hello_card(&self) -> VaultHelloCard {
        let hello = &self.vault_hello;
        let vault = self.vault_status() != VaultStatus::Missing;
        let enrolled = hello.envelope.is_some();
        let status = if !vault {
            None
        } else if hello.enrolling.is_some() {
            Some(VaultHelloStatus::Enrolling)
        } else if let Some(said) = hello.said {
            Some(said)
        } else if enrolled {
            Some(VaultHelloStatus::Enabled)
        } else {
            hello.available.map(|available| {
                if available {
                    VaultHelloStatus::Available
                } else {
                    VaultHelloStatus::Unavailable
                }
            })
        };
        VaultHelloCard {
            status,
            can_enable: vault
                && !enrolled
                && hello.available == Some(true)
                && hello.enrolling.is_none(),
            can_disable: vault && enrolled && hello.enrolling.is_none(),
        }
    }

    /// Asks again whether Windows Hello can be enrolled, as the C# `RefreshVaultStatusAsync`
    /// does when its settings load; what the last button said is forgotten.
    fn refresh_vault_hello(&mut self) -> Vec<Effect> {
        let hello = &mut self.vault_hello;
        hello.said = None;
        if self.vault.exists() && !hello.checking {
            hello.checking = true;
            return vec![Effect::CheckVaultHello];
        }
        Vec::new()
    }

    /// Enrols Windows Hello with the open vault's data key, as the C# `EnrollHelloAsync`:
    /// with the vault closed, nothing but the reason.
    fn enable_vault_hello(&mut self, from: Enrolling) -> Vec<Effect> {
        if self.vault_hello.enrolling.is_some() {
            return Vec::new();
        }
        let Some(data_key) = self.vault.opened().map(sealvault::Vault::data_key) else {
            if from == Enrolling::Settings {
                self.vault_hello.said = Some(VaultHelloStatus::UnlockRequired);
            }
            return Vec::new();
        };
        let hello = &mut self.vault_hello;
        hello.enrolling = Some(from);
        hello.said = None;
        vec![Effect::EnrolVaultHello {
            data_key,
            previous: hello.envelope.clone(),
        }]
    }

    /// The enrolment ended: kept in the system's store, it unlocks from now on.
    fn vault_hello_enrolled(&mut self, result: Result<Envelope, HelloFailure>) -> Vec<Effect> {
        let Some(from) = self.vault_hello.enrolling.take() else {
            return Vec::new();
        };
        let mut effects = Vec::new();
        // The vault it wrapped the key of is gone meanwhile: nothing is kept, nothing left.
        let result = result.and_then(|envelope| {
            if self.vault.exists() {
                return Ok(envelope);
            }
            log::warn!("the vault was removed while Windows Hello was enrolled: discarded");
            effects.push(Effect::DeleteVaultHelloCredential(
                envelope.credential_name(),
            ));
            Err(HelloFailure::Unavailable)
        });
        let kept = result.and_then(|envelope| {
            self.vault.save_hello_record(&envelope).map_err(|error| {
                log::warn!("the Windows Hello envelope could not be kept: {error}");
                // A credential nothing points at is not left behind.
                effects.push(Effect::DeleteVaultHelloCredential(
                    envelope.credential_name(),
                ));
                HelloFailure::Unavailable
            })?;
            Ok(envelope)
        });
        match kept {
            Ok(envelope) => {
                let hello = &mut self.vault_hello;
                hello.envelope = Some(envelope);
                hello.available = Some(true);
                hello.again_pending = false;
            }
            Err(failure) => {
                log::warn!("Windows Hello vault enrollment failed: {failure:?}");
                match from {
                    Enrolling::Settings => {
                        self.vault_hello.said = Some(VaultHelloStatus::Unavailable);
                    }
                    Enrolling::Again => self.tell(Notice::VaultHelloEnrolAgainFailed),
                }
            }
        }
        effects
    }

    /// Removes Windows Hello for the vault, as the C# `RemoveHelloAsync`: the envelope from
    /// the system's store, then its credential from Windows.
    fn disable_vault_hello(&mut self) -> Vec<Effect> {
        if self.vault_hello.enrolling.is_some() {
            return Vec::new();
        }
        self.forget_vault_hello()
    }

    /// Forgets the envelope, here and in the system's store, and has its credential deleted.
    pub(super) fn forget_vault_hello(&mut self) -> Vec<Effect> {
        let Some(envelope) = self.vault_hello.envelope.take() else {
            return Vec::new();
        };
        if let Err(error) = self.vault.remove_hello_record() {
            log::warn!("the Windows Hello envelope could not be removed: {error}");
        }
        vec![Effect::DeleteVaultHelloCredential(
            envelope.credential_name(),
        )]
    }

    /// Unlocks with Windows Hello from the unlock dialog, as the C# `UnlockWithHello`: only
    /// while offered, nothing else under way, and the tries not locked out.
    fn unlock_with_hello(&mut self) -> Vec<Effect> {
        let offered = self.vault_hello_offered();
        let Some(envelope) = self.vault_hello.envelope.clone() else {
            return Vec::new();
        };
        let path = self.vault.file().to_owned();
        let Some(Dialog::Vault(dialog)) = &self.dialog else {
            return Vec::new();
        };
        let mode = dialog.mode;
        // The lockout as counted and saved, a restart included, not only as the dialog says
        // it, as the C# `CanUnlockWithHello` reads `IsLockedOut`.
        let locked_until = self
            .unlock_lockout(mode)
            .and_then(|lockout| lockout.locked_until(SystemTime::now()));
        let Some(Dialog::Vault(dialog)) = self.dialog.as_mut() else {
            return Vec::new();
        };
        if let Some(until) = locked_until {
            dialog.problem = Some(VaultProblem::LockedOut { until });
            return Vec::new();
        }
        let unlocking = matches!(mode, VaultMode::Unlock | VaultMode::Locked);
        if !unlocking || !offered || !dialog.hello || dialog.busy {
            return Vec::new();
        }
        dialog.busy = true;
        dialog.hello_waiting = true;
        dialog.problem = None;
        vec![Effect::UnlockVaultHello { path, envelope }]
    }

    /// Windows Hello answered the unlock dialog: the vault open, the dialog closes as with
    /// the master password, the wrong tries counted so far forgotten, as the C#
    /// `ResetFailures`: below the lockout only, Windows Hello not being asked during one;
    /// else what the C# says, the master password left to type.
    fn unlocked_with_hello(&mut self, result: Result<OpenedVault, HelloFailure>) -> Vec<Effect> {
        let mode = match &self.dialog {
            Some(Dialog::Vault(dialog)) if dialog.hello_waiting => dialog.mode,
            // Dismissed meanwhile: nothing opens.
            _ => return Vec::new(),
        };
        let vault = result.and_then(|opened| opened.take().ok_or(HelloFailure::Unavailable));
        match vault {
            Ok(vault) => {
                self.vault.put_open(vault);
                if let Some(lockout) = self.unlock_lockout(mode) {
                    lockout.reset();
                }
                self.dialog = None;
                if mode == VaultMode::Unlock {
                    let _ = self.settings.save(&self.settings_file);
                    log::info!("Vault unlock gate satisfied.");
                    Vec::new()
                } else {
                    log::info!("Workspace unlocked with Windows Hello.");
                    self.resume_reconnects()
                }
            }
            Err(failure) => {
                log::info!("Windows Hello did not unlock the vault ({failure:?})");
                if failure == HelloFailure::NotFound {
                    self.vault_hello.again_pending = true;
                }
                if let Some(Dialog::Vault(dialog)) = self.dialog.as_mut() {
                    dialog.busy = false;
                    dialog.hello_waiting = false;
                    dialog.problem = unlock_problem(failure);
                }
                Vec::new()
            }
        }
    }

    /// The master password just opened the vault from the unlock dialog: noted, as the C#
    /// `StampLastMasterUnlockAsync`, and, the credential having been found gone, enrolling
    /// again is asked, as the C# `OfferHelloReenrollAfterMasterUnlockAsync`.
    pub(super) fn vault_opened_with_master_password(&mut self) {
        self.settings.vault_last_master_unlock = Some(SystemTime::now());
        // Not saved, it holds for this run: the unlock is not refused for it.
        let _ = self.settings.save(&self.settings_file);
        if std::mem::take(&mut self.vault_hello.again_pending) && self.dialog.is_none() {
            self.dialog = Some(Dialog::ConfirmVaultHelloEnrolAgain);
        }
    }

    /// "Re-enable" answered to the question after the master password.
    pub(super) fn enrol_vault_hello_again(&mut self) -> Vec<Effect> {
        self.enable_vault_hello(Enrolling::Again)
    }
}
