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

//! The application PIN, as the C# Heimdall's: asked at start before the master password,
//! and set, changed or removed from the Settings page.
//!
//! Wrong PINs are counted wherever the PIN is checked, the Settings dialog included, where
//! the C# one lets a person at the keyboard try as many as they like.

use std::time::SystemTime;

use heimdall_core::lockout::{Lockout, MAX_FAILED_ATTEMPTS};
use heimdall_core::pin::{PinHash, PinProblem, pin_problem};

use super::{App, Dialog, Effect};
use heimdall_ssh::Secret;

/// What the PIN dialog is for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PinMode {
    /// Asked at start: cancelled, the application quits, as the C# gate does.
    Start {
        /// The dialog the start had to show, shown once the PIN is taken.
        then: Option<Box<Dialog>>,
    },
    /// Set a PIN, change it, or remove it.
    Setup {
        /// A PIN is set: it is asked before it is changed or removed.
        current: bool,
    },
}

/// Why the PIN typed was not taken.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PinFailure {
    /// Wrong PIN at start.
    Wrong {
        /// Tries left before the lockout.
        remaining: u32,
    },
    /// Too many wrong PINs in a row: none is taken until then.
    LockedOut {
        /// When tries are taken again.
        until: SystemTime,
    },
    /// The current PIN typed to change or remove it is wrong.
    WrongCurrent,
    /// The new PIN does not follow the rule.
    Refused(PinProblem),
    /// The new PIN and its confirmation differ.
    Mismatch,
    /// The PIN could not be made or saved.
    System {
        /// Technical detail.
        detail: String,
    },
}

/// The PIN dialog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PinDialog {
    /// At start, or from the Settings page.
    pub mode: PinMode,
    /// Why the last try failed.
    pub problem: Option<PinFailure>,
}

/// What is done with the PIN.
#[derive(Clone)]
pub enum PinMessage {
    /// Open the dialog setting, changing or removing the PIN.
    Configure,
    /// The PIN typed at start.
    Submit(Secret),
    /// Set the PIN, or change it.
    Save {
        /// The PIN set, when there is one.
        current: Secret,
        /// The new PIN.
        new: Secret,
        /// The new PIN typed again.
        confirm: Secret,
    },
    /// Remove the PIN, asked first.
    Remove(Secret),
}

impl std::fmt::Debug for PinMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Configure => "Configure",
            Self::Submit(_) => "Submit(..)",
            Self::Save { .. } => "Save(..)",
            Self::Remove(_) => "Remove(..)",
        })
    }
}

impl App {
    /// The gates at start: the PIN when one is set, the dialog the start had to show kept
    /// for after it; else the master password when a vault is there.
    pub(super) fn show_start_gates(&mut self) {
        if self.settings.pin.is_some() {
            let then = self.dialog.take().map(Box::new);
            let problem = self
                .settings
                .pin_unlock
                .locked_until(SystemTime::now())
                .map(|until| PinFailure::LockedOut { until });
            self.dialog = Some(Dialog::Pin(PinDialog {
                mode: PinMode::Start { then },
                problem,
            }));
        } else if self.dialog.is_none() {
            self.show_vault_if_locked();
        }
    }

    /// Whether the PIN is asked at start, nothing else of the window being usable.
    #[must_use]
    pub fn pin_asked(&self) -> bool {
        matches!(
            &self.dialog,
            Some(Dialog::Pin(PinDialog {
                mode: PinMode::Start { .. },
                ..
            }))
        )
    }

    /// Applies a message about the PIN.
    pub(super) fn pin_message(&mut self, message: PinMessage) -> Vec<Effect> {
        match message {
            PinMessage::Configure => {
                if self.dialog.is_none() {
                    self.dialog = Some(Dialog::Pin(PinDialog {
                        mode: PinMode::Setup {
                            current: self.settings.pin.is_some(),
                        },
                        problem: None,
                    }));
                }
            }
            PinMessage::Submit(pin) => self.submit_start_pin(&pin),
            PinMessage::Save {
                current,
                new,
                confirm,
            } => self.save_pin(&current, &new, &confirm),
            PinMessage::Remove(current) => self.remove_pin(&current),
        }
        Vec::new()
    }

    /// Dismisses the PIN dialog as its Cancel does: at start the application quits.
    /// `None` when the dialog is not the PIN's.
    pub(super) fn dismiss_pin(&mut self) -> Option<Vec<Effect>> {
        let Some(Dialog::Pin(dialog)) = &self.dialog else {
            return None;
        };
        let start = matches!(dialog.mode, PinMode::Start { .. });
        self.dialog = None;
        Some(if start {
            vec![Effect::Exit]
        } else {
            Vec::new()
        })
    }

    fn pin_problem_shown(&mut self, problem: PinFailure) {
        if let Some(Dialog::Pin(dialog)) = self.dialog.as_mut() {
            dialog.problem = Some(problem);
        }
    }

    /// The PIN typed at start: taken, the start goes on; wrong, it is counted and saved at
    /// once, as the C# gate saves it. An empty one is not a try, as in C#.
    fn submit_start_pin(&mut self, pin: &Secret) {
        let Some(Dialog::Pin(PinDialog {
            mode: PinMode::Start { then },
            ..
        })) = &self.dialog
        else {
            return;
        };
        let then = then.clone();
        if pin.expose().is_empty() {
            return;
        }
        match self.check_pin(pin, |remaining| PinFailure::Wrong { remaining }) {
            Ok(()) => {
                self.dialog = then.map(|dialog| *dialog);
                if self.dialog.is_none() {
                    self.show_vault_if_locked();
                }
            }
            Err(problem) => self.pin_problem_shown(problem),
        }
    }

    /// Checks `pin` against the one set, counting a wrong one: `wrong` says it when tries
    /// are left. Saves the count either way.
    fn check_pin(
        &mut self,
        pin: &Secret,
        wrong: impl FnOnce(u32) -> PinFailure,
    ) -> Result<(), PinFailure> {
        let now = SystemTime::now();
        let lockout: &mut Lockout = &mut self.settings.pin_unlock;
        if let Some(until) = lockout.locked_until(now) {
            return Err(PinFailure::LockedOut { until });
        }
        let taken = self
            .settings
            .pin
            .as_ref()
            .is_some_and(|kept| kept.verify(pin.expose()));
        let lockout = &mut self.settings.pin_unlock;
        let checked = if taken {
            lockout.reset();
            Ok(())
        } else {
            lockout.register_failure(now);
            Err(lockout.locked_until(now).map_or_else(
                || wrong(MAX_FAILED_ATTEMPTS.saturating_sub(lockout.failures())),
                |until| PinFailure::LockedOut { until },
            ))
        };
        // Not saved, the count still holds for this run.
        let _ = self.settings.save(&self.settings_file);
        checked
    }

    fn setup_current(&self) -> Option<bool> {
        match &self.dialog {
            Some(Dialog::Pin(PinDialog {
                mode: PinMode::Setup { current },
                ..
            })) => Some(*current),
            _ => None,
        }
    }

    /// Sets the PIN, or changes it once the current one is right, in the C# order: the
    /// current PIN, the rule, the confirmation.
    fn save_pin(&mut self, current: &Secret, new: &Secret, confirm: &Secret) {
        let Some(has_current) = self.setup_current() else {
            return;
        };
        if has_current && let Err(problem) = self.check_pin(current, |_| PinFailure::WrongCurrent) {
            self.pin_problem_shown(problem);
            return;
        }
        if let Some(problem) = pin_problem(new.expose()) {
            self.pin_problem_shown(PinFailure::Refused(problem));
            return;
        }
        if new.expose() != confirm.expose() {
            self.pin_problem_shown(PinFailure::Mismatch);
            return;
        }
        match PinHash::new(new.expose()) {
            Ok(hash) => self.store_pin(Some(hash)),
            Err(detail) => self.pin_problem_shown(PinFailure::System { detail }),
        }
    }

    /// Removes the PIN once the current one is right.
    fn remove_pin(&mut self, current: &Secret) {
        if self.setup_current() != Some(true) {
            return;
        }
        match self.check_pin(current, |_| PinFailure::WrongCurrent) {
            Ok(()) => self.store_pin(None),
            Err(problem) => self.pin_problem_shown(problem),
        }
    }

    /// Keeps `pin` as the PIN, saved at once, the count of wrong ones starting again. When
    /// it cannot be saved, the PIN stays as it was and the dialog says why.
    fn store_pin(&mut self, pin: Option<PinHash>) {
        let before = (self.settings.pin.clone(), self.settings.pin_unlock);
        self.settings.pin = pin;
        self.settings.pin_unlock.reset();
        match self.settings.save(&self.settings_file) {
            Ok(()) => self.dialog = None,
            Err(error) => {
                (self.settings.pin, self.settings.pin_unlock) = before;
                self.pin_problem_shown(PinFailure::System {
                    detail: error.to_string(),
                });
            }
        }
    }
}
