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

//! Tunnels the user opens by hand, as the C# tunnels panel: the "New tunnel" dialog, the
//! attempt, the open tunnels, and their closing.
//!
//! As in the C#, the dialog closes on "Open tunnel" and nothing is asked on the way: a
//! gateway's question is answered only from what is saved for that gateway, under the rules
//! a tab follows (the endpoint it was saved for, once an attempt), and anything else is
//! declined. Only a gateway's unknown host key is put to the user, in its own dialog.

use std::sync::Arc;
use std::time::SystemTime;

use heimdall_core::profile::ProfileId;
use heimdall_ssh::{KnownHosts, PublicKey, Verdict, verdict};
use tokio_util::sync::CancellationToken;

use super::{App, Dialog, Effect, Notice};
use crate::error::UiError;
use crate::event::{Answer, ConnectionEvent, QuestionKind};
use crate::ids::QuestionId;
use crate::tunnel::{Tunnel, TunnelEvent, TunnelField, TunnelForm, TunnelId, TunnelSpec};
use crate::tunnel_driver::TunnelRequest;

/// What the user does with tunnels, and what their attempts report.
#[derive(Debug, Clone)]
pub enum TunnelMessage {
    /// Opens the "New tunnel" dialog.
    New,
    /// A field of the dialog was typed in.
    Field {
        /// Which.
        field: TunnelField,
        /// Its text.
        value: String,
    },
    /// A gateway was chosen in the dialog.
    Gateway(ProfileId),
    /// What a tunnel's attempt, then the tunnel, report.
    Event {
        /// Tunnel.
        id: TunnelId,
        /// What happened.
        event: TunnelEvent,
    },
    /// Closes a tunnel.
    Close(TunnelId),
    /// Closes every tunnel.
    CloseAll,
    /// Opens an interrupted tunnel again, as it was asked for.
    Reopen(TunnelId),
    /// Copies a tunnel's local port.
    CopyPort(TunnelId),
    /// Opens or closes the tunnels panel.
    TogglePanel,
}

/// A tunnel being opened, or open: what it was asked for, and how to stop it.
#[derive(Debug)]
pub(super) struct TunnelRun {
    id: TunnelId,
    spec: TunnelSpec,
    cancel: CancellationToken,
    /// The gateways whose saved password this attempt gave.
    answered: Vec<ProfileId>,
}

/// A gateway's key the user was asked about, kept to learn it on Accept.
#[derive(Debug, Clone)]
pub(super) struct PendingTunnelKey {
    spec: TunnelSpec,
    host: String,
    port: u16,
    key: Arc<PublicKey>,
}

impl App {
    /// Applies a tunnel message.
    pub(super) fn tunnel_message(&mut self, message: TunnelMessage) -> Vec<Effect> {
        match message {
            TunnelMessage::New => {
                let first = self
                    .store
                    .gateways()
                    .first()
                    .map(|gateway| gateway.id.clone());
                self.dialog = Some(Dialog::NewTunnel(TunnelForm::new(first)));
                Vec::new()
            }
            TunnelMessage::Field { field, value } => {
                if let Some(Dialog::NewTunnel(form)) = &mut self.dialog {
                    form.set(field, value);
                }
                Vec::new()
            }
            TunnelMessage::Gateway(id) => {
                if let Some(Dialog::NewTunnel(form)) = &mut self.dialog {
                    form.gateway = Some(id);
                }
                Vec::new()
            }
            TunnelMessage::Event { id, event } => self.tunnel_event(id, event),
            TunnelMessage::Close(id) => {
                if let Some(port) = self.close_tunnel(id) {
                    self.tell(Notice::TunnelClosed { port, error: None });
                }
                Vec::new()
            }
            TunnelMessage::Reopen(id) => {
                let Some(index) = self
                    .tunnels
                    .iter()
                    .position(|tunnel| tunnel.id == id && tunnel.interrupted)
                else {
                    return Vec::new();
                };
                let spec = self.tunnels.remove(index).spec;
                self.open_tunnel(spec)
            }
            TunnelMessage::CloseAll => {
                // The interrupted ones too: their rows go.
                let ids: Vec<TunnelId> = self
                    .tunnel_runs
                    .iter()
                    .map(|run| run.id)
                    .chain(self.tunnels.iter().map(|tunnel| tunnel.id))
                    .collect();
                for id in ids {
                    self.close_tunnel(id);
                }
                self.tell(Notice::AllTunnelsClosed);
                Vec::new()
            }
            TunnelMessage::TogglePanel => {
                self.tunnels_panel = !self.tunnels_panel;
                Vec::new()
            }
            TunnelMessage::CopyPort(id) => {
                let Some(port) = self.tunnel(id).map(|tunnel| tunnel.local.port()) else {
                    return Vec::new();
                };
                self.tell(Notice::PortCopied(port));
                vec![Effect::WriteClipboard(port.to_string())]
            }
        }
    }

    /// The open tunnel `id`.
    #[must_use]
    pub fn tunnel(&self, id: TunnelId) -> Option<&Tunnel> {
        self.tunnels.iter().find(|tunnel| tunnel.id == id)
    }

    /// How many tunnels listen: the interrupted ones are not counted.
    #[must_use]
    pub fn live_tunnels(&self) -> usize {
        self.tunnels
            .iter()
            .filter(|tunnel| !tunnel.interrupted)
            .count()
    }

    /// What stops the "New tunnel" dialog's tunnel from being opened, if it is open and
    /// anything does.
    #[must_use]
    pub fn tunnel_problem(&self) -> Option<crate::tunnel::TunnelProblem> {
        let Some(Dialog::NewTunnel(form)) = &self.dialog else {
            return None;
        };
        form.spec(&self.tunnel_ports()).err()
    }

    /// The local ports of the tunnels open or being opened: a new one may take none of them.
    #[must_use]
    pub fn tunnel_ports(&self) -> Vec<u16> {
        self.tunnel_runs
            .iter()
            .map(|run| run.spec.local_port)
            .collect()
    }

    /// Confirms the tunnel dialog open: "Open tunnel", or trusting the gateway's key.
    pub(super) fn confirm_tunnel_dialog(&mut self) -> Vec<Effect> {
        let Some(Dialog::NewTunnel(form)) = self.dialog.take() else {
            return self.tunnel_host_key_decision(true);
        };
        // The dialog closes and the attempt starts, when the form holds.
        if let Ok(spec) = form.spec(&self.tunnel_ports()) {
            return self.open_tunnel(spec);
        }
        // Not openable: the dialog stays, saying why.
        self.dialog = Some(Dialog::NewTunnel(form));
        Vec::new()
    }

    /// Starts the attempt of a tunnel `spec` asks for.
    fn open_tunnel(&mut self, spec: TunnelSpec) -> Vec<Effect> {
        let route = match self.store.route(Some(&spec.gateway)) {
            Ok(route) => route,
            Err(error) => {
                self.tell(Notice::TunnelFailed(UiError::Route(error)));
                return Vec::new();
            }
        };
        let mut hops: Vec<_> = route
            .iter()
            .map(heimdall_core::profile::SshGateway::as_hop)
            .collect();
        let Some(gateway) = hops.pop() else {
            self.tell(Notice::TunnelFailed(UiError::Cancelled));
            return Vec::new();
        };
        let id = self.next_tunnel;
        self.next_tunnel = id.next();
        let cancel = CancellationToken::new();
        let request = TunnelRequest {
            before: hops,
            gateway,
            remote_host: spec.remote_host.clone(),
            remote_port: spec.remote_port,
            local_port: spec.local_port,
            ssh: self.ssh_options(),
            cancel: cancel.clone(),
        };
        self.tunnel_runs.push(TunnelRun {
            id,
            spec,
            cancel,
            answered: Vec::new(),
        });
        vec![Effect::OpenTunnel {
            id,
            request: Box::new(request),
        }]
    }

    /// Applies what tunnel `id` reports.
    fn tunnel_event(&mut self, id: TunnelId, event: TunnelEvent) -> Vec<Effect> {
        if !self.tunnel_runs.iter().any(|run| run.id == id) {
            // Closed meanwhile: a question still waiting is declined.
            if let TunnelEvent::Route(ConnectionEvent::Question { question, .. }) = event {
                return vec![decline(question)];
            }
            return Vec::new();
        }
        match event {
            TunnelEvent::Route(ConnectionEvent::Question { question, kind }) => {
                let answer = self.tunnel_saved_answer(id, &kind);
                vec![Effect::Answer { question, answer }]
            }
            TunnelEvent::Route(ConnectionEvent::UnknownHostKey {
                host,
                port,
                fingerprint,
                key,
            }) => {
                let Some(run) = self.take_run(id) else {
                    return Vec::new();
                };
                // Never queued behind another dialog, never accepted unseen.
                if self.dialog.is_some() {
                    self.tell(Notice::TunnelFailed(UiError::Cancelled));
                    return Vec::new();
                }
                self.dialog = Some(Dialog::TunnelHostKey {
                    host: host.clone(),
                    port,
                    fingerprint,
                });
                self.pending_tunnel_key = Some(PendingTunnelKey {
                    spec: run.spec,
                    host,
                    port,
                    key,
                });
                Vec::new()
            }
            TunnelEvent::Route(ConnectionEvent::Failed(error)) => {
                if let Some(run) = self.take_run(id) {
                    // A saved password given on the way may be what the gateway refused.
                    self.refuse_saved(run.answered);
                }
                self.tell(Notice::TunnelFailed(error));
                Vec::new()
            }
            TunnelEvent::Route(_) => Vec::new(),
            TunnelEvent::Opened(local) => {
                let Some(run) = self.tunnel_runs.iter().find(|run| run.id == id) else {
                    return Vec::new();
                };
                let gateway_name = self
                    .store
                    .gateways()
                    .iter()
                    .find(|gateway| gateway.id == run.spec.gateway)
                    .map_or_else(
                        || run.spec.gateway.to_string(),
                        |gateway| gateway.name.clone(),
                    );
                let tunnel = Tunnel {
                    id,
                    spec: run.spec.clone(),
                    gateway_name,
                    local,
                    started: SystemTime::now(),
                    interrupted: false,
                };
                self.tell(Notice::TunnelOpened {
                    port: local.port(),
                    host: tunnel.spec.remote_host.clone(),
                    remote_port: tunnel.spec.remote_port,
                });
                self.tunnels.push(tunnel);
                Vec::new()
            }
            TunnelEvent::Closed => {
                // Kept and said interrupted, as the C# row: which one went stays in sight,
                // to be opened again or closed.
                let Some(run) = self.take_run(id) else {
                    return Vec::new();
                };
                run.cancel.cancel();
                let Some(tunnel) = self.tunnels.iter_mut().find(|tunnel| tunnel.id == id) else {
                    return Vec::new();
                };
                tunnel.interrupted = true;
                let port = tunnel.local.port();
                self.tell(Notice::TunnelClosed {
                    port,
                    error: Some(UiError::ConnectionLost),
                });
                Vec::new()
            }
        }
    }

    /// The answer to `kind` for tunnel `id`: what is saved for the gateway on the way that
    /// asks, given as a tab gives it, or nothing.
    fn tunnel_saved_answer(&mut self, id: TunnelId, kind: &QuestionKind) -> Option<Answer> {
        let gateway = self
            .tunnel_runs
            .iter()
            .find(|run| run.id == id)?
            .spec
            .gateway
            .clone();
        if let QuestionKind::Passphrase(question) = kind {
            let owner = self.route_key_owner(Some(&gateway), &question.key_path)?;
            return self.saved_passphrase(&owner, &question.key_path, question.attempt);
        }
        let (profile, endpoint) = self.route_endpoint(&gateway, kind)?;
        let answered_before = self
            .tunnel_runs
            .iter()
            .any(|run| run.id == id && run.answered.contains(&profile));
        let answer = self.saved_password(&profile, &endpoint, kind, answered_before)?;
        if let Some(run) = self.tunnel_runs.iter_mut().find(|run| run.id == id) {
            run.answered.push(profile);
        }
        Some(answer)
    }

    /// The user's answer about a gateway's unknown key: learnt, and the tunnel tried again;
    /// or refused, and the tunnel not opened.
    pub(super) fn tunnel_host_key_decision(&mut self, accept: bool) -> Vec<Effect> {
        let Some(pending) = self.pending_tunnel_key.take() else {
            return Vec::new();
        };
        if !accept {
            self.tell(Notice::TunnelFailed(UiError::Cancelled));
            return Vec::new();
        }
        let known_hosts = KnownHosts::new(&self.config.known_hosts);
        // Another tab may have recorded a key for this host meanwhile: read again.
        let learned = match known_hosts.recorded(&pending.host, pending.port) {
            Ok(recorded) => match verdict(&recorded, &pending.key) {
                Verdict::Trusted => Ok(()),
                Verdict::Unknown => known_hosts
                    .learn(&pending.host, pending.port, &pending.key)
                    .map_err(|error| UiError::from(&error)),
                // Changed since it was asked about: not learnt, never overwritten.
                Verdict::Changed { .. } | Verdict::OtherAlgorithm { .. } => Err(UiError::Cancelled),
            },
            Err(error) => Err(UiError::from(&error)),
        };
        match learned {
            Ok(()) => self.open_tunnel(pending.spec),
            Err(error) => {
                self.tell(Notice::TunnelFailed(error));
                Vec::new()
            }
        }
    }

    /// Stops tunnel `id`, open, being opened or interrupted, its row gone; its local port
    /// when it was listening.
    fn close_tunnel(&mut self, id: TunnelId) -> Option<u16> {
        if let Some(run) = self.take_run(id) {
            run.cancel.cancel();
        }
        let index = self.tunnels.iter().position(|tunnel| tunnel.id == id)?;
        let tunnel = self.tunnels.remove(index);
        (!tunnel.interrupted).then(|| tunnel.local.port())
    }

    fn take_run(&mut self, id: TunnelId) -> Option<TunnelRun> {
        let index = self.tunnel_runs.iter().position(|run| run.id == id)?;
        Some(self.tunnel_runs.remove(index))
    }

    /// Refuses the gateway's key the user was asked about, if that is the dialog open.
    pub(super) fn dismiss_tunnel_key(&mut self) -> Option<Vec<Effect>> {
        if !matches!(self.dialog, Some(Dialog::TunnelHostKey { .. })) {
            return None;
        }
        self.dialog = None;
        Some(self.tunnel_host_key_decision(false))
    }
}

/// Declines `question`: nobody is there to answer it.
fn decline(question: QuestionId) -> Effect {
    Effect::Answer {
        question,
        answer: None,
    }
}
