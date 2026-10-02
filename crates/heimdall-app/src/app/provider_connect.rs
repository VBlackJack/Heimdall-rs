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

//! The external credential provider asked at connect, as the C# `ServerListViewModel` asks
//! it: only when no password is saved for the server, only for the tab's own server (never
//! a gateway on the way), and once per attempt. `{Title}` is the profile's vault entry
//! name, or its name when it has none, as the C# fallback.
//!
//! When it gives nothing, the question is asked of the user as it would have been, and the
//! status bar says why, where the C# one shows a warning and connects without a password.

use heimdall_core::credential_provider::{Lookup, ProviderKind, ProviderSettings};
use heimdall_ssh::Secret;

use super::vault::usable_endpoint;
use super::{App, Effect, Notice, TabProfile};
use crate::credential_provider::{Provided, ProviderFailure, ask, from_credential_manager};
use crate::event::{Answer, ConnectionEvent, QuestionKind};
use crate::ids::{AttemptId, QuestionId, TabId};

/// A tab's question the provider is asked to answer.
#[derive(Debug, Clone)]
pub struct ProviderRequest {
    /// Tab.
    pub tab: TabId,
    /// The attempt asking.
    pub attempt: AttemptId,
    /// The question.
    pub question: QuestionId,
    /// What it asks, to ask the user when the provider gives nothing.
    pub kind: QuestionKind,
    /// The provider's settings.
    pub settings: ProviderSettings,
    /// The server asked about.
    pub lookup: Lookup,
    /// The unlock secret, when one is kept and can be read.
    pub unlock: Option<Secret>,
}

/// The provider's answer to a [`ProviderRequest`].
#[derive(Debug, Clone)]
pub struct ProviderAnswer {
    /// Tab.
    pub tab: TabId,
    /// The attempt that asked.
    pub attempt: AttemptId,
    /// The question.
    pub question: QuestionId,
    /// What it asks.
    pub kind: QuestionKind,
    /// The password given, or why none was.
    pub result: Result<Provided, ProviderFailure>,
}

impl ProviderRequest {
    /// Asks the provider: for a task of the UI layer.
    pub async fn run(self) -> ProviderAnswer {
        let result = match self.settings.kind {
            ProviderKind::Command => ask(&self.settings, &self.lookup, self.unlock.as_ref()).await,
            ProviderKind::WindowsCredentialManager => {
                from_credential_manager(self.lookup.title.clone()).await
            }
        };
        ProviderAnswer {
            tab: self.tab,
            attempt: self.attempt,
            question: self.question,
            kind: self.kind,
            result,
        }
    }
}

/// The profile's entry in the password manager, `{Title}`: its vault entry name when set,
/// else its name, as the C# fallback.
fn provider_title(profile: &TabProfile) -> String {
    let entry = match profile {
        TabProfile::Ssh(profile) => profile.vault_entry.as_deref(),
        TabProfile::Rdp(profile) => profile.vault_entry.as_deref(),
        TabProfile::Vnc(profile) => profile.vault_entry.as_deref(),
        TabProfile::Ftp(profile) => profile.vault_entry.as_deref(),
        TabProfile::Telnet(_) | TabProfile::Local(_) | TabProfile::WinRm(_) => None,
    };
    entry
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .unwrap_or_else(|| profile.name())
        .to_owned()
}

impl App {
    /// The provider's request for `question` in `tab_id`, when the provider is to be asked:
    /// it is on and runs a command, the question is the tab's own server's password, first
    /// asked in this attempt, and no password of the server was refused in this run.
    pub(super) fn provider_request(
        &self,
        tab_id: TabId,
        question: QuestionId,
        kind: &QuestionKind,
    ) -> Option<Effect> {
        let settings = &self.settings.credential_provider;
        // Windows Credential Manager needs nothing more than the entry's name.
        let ready = match settings.kind {
            ProviderKind::Command => !settings.command.trim().is_empty(),
            ProviderKind::WindowsCredentialManager => true,
        };
        if !settings.enabled || !ready {
            return None;
        }
        let tab = self.tab(tab_id)?;
        let (profile, endpoint) = usable_endpoint(tab, kind)?;
        let first_try = match kind {
            QuestionKind::Password(asked) => asked.attempt <= 1,
            _ => true,
        };
        // Asked again after the provider's password, the saved passwords' check, which runs
        // first, has already marked the server's password refused.
        if !first_try || self.vault.is_refused(&profile) {
            return None;
        }
        let user = match kind {
            QuestionKind::Password(asked) => Some(asked.username.clone()),
            _ => None,
        };
        Some(Effect::AskCredentialProvider(Box::new(ProviderRequest {
            tab: tab_id,
            attempt: tab.attempt,
            question,
            kind: kind.clone(),
            settings: settings.clone(),
            lookup: Lookup {
                host: endpoint.host,
                port: endpoint.port,
                user,
                title: provider_title(&tab.profile),
            },
            unlock: self.provider_unlock_secret(),
        })))
    }

    /// The provider answered a tab's question: its password is given, counted as given so
    /// a refusal asks the user; without one, the user is asked, and told why.
    pub(super) fn provider_answered(&mut self, answer: ProviderAnswer) -> Vec<Effect> {
        let ProviderAnswer {
            tab: tab_id,
            attempt,
            question,
            kind,
            result,
        } = answer;
        let current = self.tab(tab_id).is_some_and(|tab| tab.attempt == attempt);
        if !current {
            // The tab was closed, or the attempt given up, while the provider ran.
            return vec![Effect::Answer {
                question,
                answer: None,
            }];
        }
        match result {
            Ok(provided) => {
                if let Some(tab) = self.tab(tab_id)
                    && let Some((profile, _)) = usable_endpoint(tab, &kind)
                    && let Some(tab) = self.tab_mut(tab_id)
                {
                    tab.auto_answered.retain(|(earlier, _)| *earlier == attempt);
                    tab.auto_answered.push((attempt, profile));
                }
                vec![Effect::Answer {
                    question,
                    answer: Some(Answer::Secret(provided.password)),
                }]
            }
            Err(failure) => {
                let name = self
                    .tab(tab_id)
                    .map(|tab| tab.profile.name().to_owned())
                    .unwrap_or_default();
                log::warn!("the credential provider gave no password for {name}: {failure:?}");
                self.tell(match failure {
                    ProviderFailure::Launch(detail) => Notice::ProviderFailed(detail),
                    ProviderFailure::TimedOut => Notice::ProviderTimedOut,
                    _ => Notice::ProviderNoPassword(name),
                });
                self.apply_connection_event(tab_id, ConnectionEvent::Question { question, kind })
            }
        }
    }
}
