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

//! The gateway dialog's "Test route", as the C# card: the unsaved form and the gateways it
//! is reached through tested hop by hop, one test at a time, stopped when the dialog goes.
//!
//! Each gateway signs in with what belongs to it, by its place on the route, never by an
//! address: a parent with its saved password, only to the endpoint it was saved for; the
//! dialog's own with what is typed, or its saved one only while the form still names the
//! endpoint it was saved for and the dialog does not clear it. Nothing is learnt from the
//! outcome: a refusal does not mark a saved password refused.

use std::path::Path;
use std::time::SystemTime;

use heimdall_core::credentials::{CredentialProtocol, Endpoint};
use heimdall_core::gateway_parents::MAX_GATEWAY_CHAIN_DEPTH;
use heimdall_core::profile::SshGateway;
use heimdall_ssh::{HopSecrets, Secret, Step};
use tokio_util::sync::CancellationToken;

use super::{App, Dialog, Effect, Message};
use crate::gateway_draft::{RouteProblem, RouteTest};
use crate::profile_draft::SavedSecret;
use crate::route_test::RouteTestRequest;

/// The endpoint `gateway` signs in at.
fn endpoint(gateway: &SshGateway) -> Endpoint {
    Endpoint {
        protocol: CredentialProtocol::Ssh,
        host: gateway.host.clone(),
        port: gateway.port,
        username: gateway.username.clone(),
    }
}

impl App {
    /// Applies a message about "Test route".
    pub(super) fn route_test_message(&mut self, message: Message) -> Vec<Effect> {
        match message {
            Message::TestRoute {
                password,
                passphrase,
            } => self.test_route(password, passphrase),
            Message::StopRouteTest => {
                if let Some((_, cancel)) = &self.route_test {
                    cancel.cancel();
                }
                Vec::new()
            }
            Message::RouteTarget { field, value } => {
                if let Some(Dialog::EditGateway { draft, .. }) = &mut self.dialog {
                    draft.set_target(field, value);
                }
                Vec::new()
            }
            Message::ForgetRouteTest => {
                if let Some(Dialog::EditGateway { draft, .. }) = &mut self.dialog {
                    draft.forget_route_test();
                }
                Vec::new()
            }
            Message::RouteStep { run, step } => {
                if let Some(steps) = self.running_route_test(run) {
                    steps.push(step);
                }
                Vec::new()
            }
            Message::RouteTestDone { run } => {
                if self.route_test.as_ref().map(|(running, _)| *running) == Some(run) {
                    self.route_test = None;
                }
                if let Some(Dialog::EditGateway { draft, .. }) = &mut self.dialog
                    && let RouteTest::Running(steps) = &mut draft.route_test
                {
                    let steps = std::mem::take(steps);
                    draft.route_test = RouteTest::Done {
                        steps,
                        at: SystemTime::now(),
                    };
                }
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    /// Stops a test whose dialog is gone, whatever closed it: its sessions do not linger.
    pub(super) fn stop_orphan_route_test(&mut self) {
        if !matches!(self.dialog, Some(Dialog::EditGateway { .. }))
            && let Some((_, cancel)) = self.route_test.take()
        {
            cancel.cancel();
        }
    }

    /// Whether the gateway dialog's test is running: it is not saved meanwhile.
    pub(super) fn route_test_running(&self) -> bool {
        matches!(
            &self.dialog,
            Some(Dialog::EditGateway { draft, .. }) if matches!(draft.route_test, RouteTest::Running(_))
        )
    }

    /// The steps of test `run`, when it is the one running in the dialog open.
    fn running_route_test(&mut self, run: u64) -> Option<&mut Vec<Step>> {
        if self.route_test.as_ref().map(|(running, _)| *running) != Some(run) {
            return None;
        }
        let Some(Dialog::EditGateway { draft, .. }) = &mut self.dialog else {
            return None;
        };
        match &mut draft.route_test {
            RouteTest::Running(steps) => Some(steps),
            _ => None,
        }
    }

    /// Starts a test of the dialog's route, signed in with `password` and `passphrase`, as
    /// typed in it.
    fn test_route(&mut self, password: Option<Secret>, passphrase: Option<Secret>) -> Vec<Effect> {
        let Some(Dialog::EditGateway { draft, .. }) = &self.dialog else {
            return Vec::new();
        };
        if matches!(draft.route_test, RouteTest::Running(_)) {
            return Vec::new();
        }
        let id = draft.editing.clone().unwrap_or_else(|| self.fresh_id());
        let checked = draft
            .to_gateway(id)
            .and_then(|gateway| self.without_loop(gateway));
        let target = draft.target();
        let route = checked
            .as_ref()
            .ok()
            .map(|gateway| self.store.route(gateway.parent.as_ref()));
        let (gateway, target, parents) = match (checked, target, route) {
            // The gateway tested is one more on the chain: a connection refuses it too deep.
            (Ok(gateway), Ok(target), Some(Ok(parents)))
                if parents.len() < MAX_GATEWAY_CHAIN_DEPTH =>
            {
                (gateway, target, parents)
            }
            (_, Err(problem), _) => return self.refuse_route_test(problem),
            _ => return self.refuse_route_test(RouteProblem::Route),
        };
        let mut secrets: Vec<HopSecrets> = parents
            .iter()
            .map(|parent| HopSecrets {
                password: self.saved_password_for(&parent.id, &endpoint(parent)),
                passphrase: parent
                    .key_path
                    .as_deref()
                    .and_then(|path| self.saved_passphrase_for(&parent.id, path)),
            })
            .collect();
        secrets.push(self.own_secrets(&gateway, password, passphrase));
        let hops = parents
            .iter()
            .chain(std::iter::once(&gateway))
            .map(SshGateway::as_hop)
            .collect();
        let run = self.next_route_test;
        self.next_route_test += 1;
        let cancel = CancellationToken::new();
        self.route_test = Some((run, cancel.clone()));
        if let Some(Dialog::EditGateway { draft, .. }) = &mut self.dialog {
            draft.route_test = RouteTest::Running(Vec::new());
        }
        vec![Effect::TestRoute {
            run,
            request: Box::new(RouteTestRequest {
                hops,
                secrets,
                destination: target,
                options: self.ssh_options(),
                cancel,
            }),
        }]
    }

    /// What the dialog's own gateway signs in with: what is typed; else what is saved for it,
    /// while the form names the endpoint, or the key file, it was saved for and the dialog
    /// does not clear it.
    fn own_secrets(
        &self,
        gateway: &SshGateway,
        password: Option<Secret>,
        passphrase: Option<Secret>,
    ) -> HopSecrets {
        let Some(Dialog::EditGateway { draft, .. }) = &self.dialog else {
            return HopSecrets::default();
        };
        let typed = |secret: Option<Secret>| secret.filter(|typed| !typed.expose().is_empty());
        let saved = draft.editing.as_ref();
        HopSecrets {
            password: typed(password).or_else(|| {
                saved
                    .filter(|_| !draft.clear_password)
                    .and_then(|id| self.saved_password_for(id, &endpoint(gateway)))
            }),
            passphrase: typed(passphrase).or_else(|| {
                let key = gateway.key_path.as_deref()?;
                saved
                    .filter(|_| draft.passphrase != SavedSecret::Cleared)
                    .and_then(|id| self.saved_passphrase_for(id, Path::new(key)))
            }),
        }
    }

    fn refuse_route_test(&mut self, problem: RouteProblem) -> Vec<Effect> {
        if let Some(Dialog::EditGateway { draft, .. }) = &mut self.dialog {
            draft.route_test = RouteTest::Refused(problem);
        }
        Vec::new()
    }
}
