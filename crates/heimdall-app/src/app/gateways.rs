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

//! SSH gateways, added and edited through the C# Heimdall's gateway dialog, from the "+"
//! and tree menus or from a session's form, which the dialog returns to with the new
//! gateway chosen.

use heimdall_core::credentials::{CredentialProtocol, Endpoint};
use heimdall_core::profile::{ProfileId, SshGateway};
use heimdall_ssh::Secret;

use super::{App, Dialog, Message};
use crate::gateway_draft::GatewayDraft;
use crate::profile_draft::{DraftError, SavedSecret};

impl App {
    /// Saved SSH gateways.
    #[must_use]
    pub fn gateways(&self) -> &[SshGateway] {
        self.store.gateways()
    }

    /// Applies a message about gateways.
    pub(super) fn gateway_message(&mut self, message: Message) {
        match message {
            Message::NewGateway => self.open_gateway(GatewayDraft::default()),
            Message::EditGateway(id) => {
                if let Some(gateway) = self.gateways().iter().find(|g| g.id == id) {
                    let mut draft = GatewayDraft::from_gateway(gateway);
                    draft.password_saved = self.password_saved(&id);
                    draft.passphrase = SavedSecret::from_saved(self.passphrase_saved(&id));
                    self.open_gateway(draft);
                }
            }
            Message::GatewayField { field, value } => {
                if let Some(Dialog::EditGateway { draft, error, .. }) = self.dialog.as_mut() {
                    draft.set(field, value);
                    *error = None;
                }
            }
            Message::ChooseParentGateway(parent) => {
                if let Some(Dialog::EditGateway { draft, error, .. }) = self.dialog.as_mut() {
                    draft.parent = parent;
                    *error = None;
                }
            }
            Message::ClearGatewayPassword => {
                if let Some(Dialog::EditGateway { draft, .. }) = self.dialog.as_mut() {
                    draft.clear_password = true;
                    draft.password_saved = false;
                }
            }
            Message::ClearGatewayPassphrase => {
                if let Some(Dialog::EditGateway { draft, .. }) = self.dialog.as_mut() {
                    draft.passphrase = SavedSecret::Cleared;
                }
            }
            // Not while its route is being tested: what is tested is what is saved.
            Message::SaveGateway { .. } if self.route_test_running() => {}
            Message::SaveGateway {
                password,
                passphrase,
            } => match self.dialog.take() {
                Some(Dialog::EditGateway { draft, back, .. }) => {
                    self.save_gateway(draft, back, password.as_ref(), passphrase.as_ref());
                }
                other => self.dialog = other,
            },
            Message::ChooseGateway(id) => {
                if let Some(Dialog::EditProfile { draft, error }) = self.dialog.as_mut() {
                    draft.choose_gateway(id);
                    *error = None;
                }
            }
            _ => {}
        }
    }

    /// Opens the gateway dialog over what is open: a session's form comes back after it.
    fn open_gateway(&mut self, draft: GatewayDraft) {
        let back = match self.dialog.take() {
            Some(form @ Dialog::EditProfile { .. }) => Some(Box::new(form)),
            _ => None,
        };
        self.dialog = Some(Dialog::EditGateway {
            draft: Box::new(draft),
            error: None,
            back,
        });
    }

    /// Closes the gateway dialog: back to the session's form it was opened from, if any.
    /// Whether it was open.
    pub(super) fn dismiss_gateway(&mut self) -> bool {
        match self.dialog.take() {
            Some(Dialog::EditGateway { back, .. }) => {
                self.dialog = back.map(|form| *form);
                true
            }
            other => {
                self.dialog = other;
                false
            }
        }
    }

    fn save_gateway(
        &mut self,
        draft: Box<GatewayDraft>,
        back: Option<Box<Dialog>>,
        password: Option<&Secret>,
        passphrase: Option<&Secret>,
    ) {
        let id = draft.editing.clone().unwrap_or_else(|| self.fresh_id());
        let checked = draft
            .to_gateway(id)
            .and_then(|gateway| self.without_loop(gateway));
        let gateway = match checked {
            Ok(gateway) => gateway,
            Err(error) => {
                self.dialog = Some(Dialog::EditGateway {
                    draft,
                    error: Some(error),
                    back,
                });
                return;
            }
        };
        let endpoint = Endpoint {
            protocol: CredentialProtocol::Ssh,
            host: gateway.host.clone(),
            port: gateway.port,
            username: gateway.username.clone(),
        };
        let id = gateway.id.clone();
        let key_path = gateway.key_path.clone();
        if let Err(error) = self.store.apply(|store| store.merge_gateways([gateway])) {
            self.dialog = Some(Dialog::StoreError {
                detail: error.to_string(),
            });
            return;
        }
        // Back to the session's form, the new gateway chosen, as the C# dialog does.
        self.dialog = back.map(|form| *form);
        if draft.editing.is_none()
            && let Some(Dialog::EditProfile { draft, .. }) = self.dialog.as_mut()
        {
            draft.choose_gateway(id.clone());
        }
        if self.can_save_passwords() {
            let typed = password.filter(|typed| !typed.expose().is_empty());
            self.save_edited_password(&id, endpoint, typed, draft.clear_password);
            self.save_edited_passphrase(
                &id,
                key_path.as_deref(),
                passphrase,
                draft.passphrase == SavedSecret::Cleared,
            );
        }
    }

    /// `gateway`, unless its parents lead back to it.
    pub(super) fn without_loop(&self, gateway: SshGateway) -> Result<SshGateway, DraftError> {
        let mut next = gateway.parent.clone();
        let mut steps = 0;
        while let Some(parent) = next {
            if parent == gateway.id || steps > self.gateways().len() {
                return Err(DraftError::GatewayLoop);
            }
            steps += 1;
            next = self
                .gateways()
                .iter()
                .find(|known| known.id == parent)
                .and_then(|known| known.parent.clone());
        }
        Ok(gateway)
    }
}

/// Whether `profile` names a gateway that is not saved.
#[must_use]
pub(super) fn is_missing(gateways: &[SshGateway], gateway: Option<&ProfileId>) -> bool {
    gateway.is_some_and(|id| !gateways.iter().any(|known| known.id == *id))
}
