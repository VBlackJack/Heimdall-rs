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

//! The port and the account of several profiles at once, as the C# tree's "Edit" menu of a
//! selection offers them: the value they share written in, or none when they differ; saved
//! in one write, and said.

use heimdall_core::profile::ProfileId;

use super::tree::ProfileKind;
use super::{App, Dialog, Notice};

/// Profiles a bulk edit needs at least, as the C# one.
pub(super) const BULK_EDIT_MINIMUM: usize = 2;

/// What several profiles get at once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BulkField {
    /// The port they connect to.
    Port,
    /// The account they log in with.
    Username,
}

impl BulkField {
    /// Whether a profile of `kind` has this field, as the C# bulk edit takes them: a port
    /// for all but a local shell and a Citrix application, an account for SSH, SFTP, RDP,
    /// FTP and `WinRM`.
    #[must_use]
    pub fn applies_to(self, kind: ProfileKind) -> bool {
        match self {
            Self::Port => !matches!(kind, ProfileKind::Local | ProfileKind::Citrix),
            Self::Username => matches!(
                kind,
                ProfileKind::Ssh
                    | ProfileKind::Sftp
                    | ProfileKind::Rdp
                    | ProfileKind::Ftp
                    | ProfileKind::WinRm
            ),
        }
    }
}

/// Why a value typed for several profiles is refused, as the C# dialog says it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BulkRefusal {
    /// A port not between 1 and 65535.
    Port,
    /// An empty account, or one holding a control character.
    Username,
}

impl App {
    /// How many of `ids` can go through a gateway: SSH, SFTP, RDP and `WinRM`, the count the
    /// C# "Set gateway..." entry acts on.
    #[must_use]
    pub fn gateway_targets(&self, ids: &[ProfileId]) -> usize {
        ids.iter()
            .filter_map(|id| self.profile_summary(id))
            .filter(|profile| {
                matches!(
                    profile.kind,
                    ProfileKind::Ssh | ProfileKind::Sftp | ProfileKind::Rdp | ProfileKind::WinRm
                )
            })
            .count()
    }

    /// How many of `ids` take `field`: the count the C# menu entry shows.
    #[must_use]
    pub fn bulk_targets(&self, ids: &[ProfileId], field: BulkField) -> usize {
        ids.iter()
            .filter_map(|id| self.profile_summary(id))
            .filter(|profile| field.applies_to(profile.kind))
            .count()
    }

    /// Opens the dialog setting `field` on the profiles selected that take it, the value
    /// they share written in; nothing for fewer than two selected, or none taking it.
    pub(super) fn open_bulk_edit(&mut self, field: BulkField) {
        let selected = self.selected_profiles();
        if selected.len() < BULK_EDIT_MINIMUM {
            return;
        }
        let profiles: Vec<_> = selected
            .iter()
            .filter_map(|id| self.profile_summary(id))
            .filter(|profile| field.applies_to(profile.kind))
            .collect();
        if profiles.is_empty() {
            return;
        }
        let values: Vec<String> = profiles
            .iter()
            .map(|profile| match field {
                BulkField::Port => profile
                    .endpoint
                    .as_ref()
                    .map(|(_, port)| port.to_string())
                    .unwrap_or_default(),
                BulkField::Username => profile.username.clone().unwrap_or_default(),
            })
            .collect();
        let shared = values.iter().all(|value| *value == values[0]);
        self.dialog = Some(Dialog::BulkEdit {
            field,
            ids: profiles.into_iter().map(|profile| profile.id).collect(),
            value: if shared {
                values[0].clone()
            } else {
                String::new()
            },
            mixed: !shared,
            refused: None,
        });
    }

    /// The value typed in the bulk edit dialog.
    pub(super) fn bulk_edited(&mut self, typed: String) {
        if let Some(Dialog::BulkEdit { value, refused, .. }) = self.dialog.as_mut() {
            *value = typed;
            *refused = None;
        }
    }

    /// Sets `value` on `ids`, saved at once, and says how many changed; the dialog back,
    /// saying why, when the value is refused.
    pub(super) fn confirm_bulk_edit(
        &mut self,
        field: BulkField,
        ids: &[ProfileId],
        value: &str,
        mixed: bool,
    ) {
        let refused = |refusal| Dialog::BulkEdit {
            field,
            ids: ids.to_vec(),
            value: value.to_owned(),
            mixed,
            refused: Some(refusal),
        };
        let saved = match field {
            BulkField::Port => {
                let Some(port) = value.trim().parse::<u16>().ok().filter(|port| *port > 0) else {
                    self.dialog = Some(refused(BulkRefusal::Port));
                    return;
                };
                self.store
                    .apply(|store| ids.iter().filter(|id| store.set_port(id, port)).count())
            }
            BulkField::Username => {
                if value.is_empty() || value.chars().any(char::is_control) {
                    self.dialog = Some(refused(BulkRefusal::Username));
                    return;
                }
                self.store.apply(|store| {
                    ids.iter()
                        .filter(|id| store.set_username(id, value))
                        .count()
                })
            }
        };
        match saved {
            Ok(changed) => self.tell(match (field, changed) {
                (BulkField::Port, 0) => Notice::BulkPortUnchanged,
                (BulkField::Port, changed) => Notice::BulkPortUpdated(changed),
                (BulkField::Username, 0) => Notice::BulkUsernameUnchanged,
                (BulkField::Username, changed) => Notice::BulkUsernameUpdated(changed),
            }),
            Err(error) => {
                self.dialog = Some(Dialog::save_failed(&error));
            }
        }
    }
}
