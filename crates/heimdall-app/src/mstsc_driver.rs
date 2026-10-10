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

//! An RDP profile opened in Remote Desktop Connection through its SSH gateway, as the C#
//! `RdpHandler` external mode with a tunnel (`RdpHandler.cs`, `SetupTunnelIfNeededAsync`).
//! The route to the gateway comes first, its questions asked as a tunnel's. Then a forward
//! listens on this computer's loopback address, on the profile's own port when it has one and
//! it is free, else on one the system picks, as the C# `TunnelService` and
//! `TunnelManager.AllocatePort` do, and carries what comes to it to the server's host and
//! port, as the gateway reaches them. The `.rdp` file names the forward, and `mstsc.exe` is
//! started on it.
//!
//! The forward and the gateway live as long as `mstsc.exe` does: a task waits for it to
//! exit, then releases both, as the C# releases its tunnel on the process's `Exited`.
//! Cancelling the request, as the application does when it closes, releases them too.

use std::sync::Arc;

use heimdall_core::profile::{RdpProfile, SshProfile};
use heimdall_ssh::{ConnectOptions, at_gateway, establish_via, local_forward};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::{Stream, StreamExt as _};
use tokio_util::sync::CancellationToken;

use crate::driver::{AnswerRegistry, ChannelPrompter, report_failure};
use crate::event::ConnectionEvent;
use crate::rdp_external::{self, ExternalRefusal, Running};

/// Events held while the application reads them: a launch reports a handful.
const EVENT_BUFFER: usize = 8;

/// Clients the forward carries at once: the client's own connection, and the one its
/// automatic reconnection dials, which may come before the first is seen gone. Any other
/// program of this computer is refused beyond that.
pub const MSTSC_CLIENTS: usize = 2;

/// Identifies a launch through a gateway, from its start to the release of its forward.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MstscRouteId(u64);

impl MstscRouteId {
    /// The identifier after `self`, for the next launch.
    #[must_use]
    pub(crate) fn next(self) -> Self {
        Self(self.0 + 1)
    }

    /// The raw value, for logs.
    #[must_use]
    pub fn value(self) -> u64 {
        self.0
    }
}

impl Default for MstscRouteId {
    fn default() -> Self {
        Self(1)
    }
}

/// A launch of Remote Desktop Connection through a gateway.
#[derive(Debug)]
pub struct MstscRouteRequest {
    /// The gateways to go through to reach [`MstscRouteRequest::gateway`], nearest first.
    pub before: Vec<SshProfile>,
    /// The gateway that carries the forward.
    pub gateway: SshProfile,
    /// The profile opened, its options already the session's: its server's host and port,
    /// as the gateway reaches them, and the rest of its `.rdp` file.
    pub profile: RdpProfile,
    /// SSH options for the route, `known_hosts` among them.
    pub ssh: ConnectOptions,
    /// Stops the attempt, then releases the forward and the gateway.
    pub cancel: CancellationToken,
}

/// What a launch through a gateway reports.
#[derive(Debug, Clone)]
pub enum MstscRouteEvent {
    /// What the route to the gateway reports: a question, a gateway's unknown key, or the
    /// failure that stops the launch.
    Route(ConnectionEvent),
    /// Remote Desktop Connection started on the forward, or why not.
    Launched(Result<(), ExternalRefusal>),
    /// Remote Desktop Connection exited, or the launch was cancelled: the forward and the
    /// gateway are released.
    Released,
}

/// Launches Remote Desktop Connection as `request` asks, `starter` starting it on the
/// `.rdp` file's content: what the route reports first, then the rest. `starter` runs off
/// the runtime's threads, as writing a file and starting a program wait on the system;
/// [`rdp_external::start`] is the real one.
pub fn mstsc_route_events<S>(
    request: MstscRouteRequest,
    registry: AnswerRegistry,
    starter: S,
) -> impl Stream<Item = MstscRouteEvent> + Send + 'static
where
    S: FnOnce(&str) -> Result<Running, ExternalRefusal> + Send + 'static,
{
    let (route, route_events) = mpsc::channel(EVENT_BUFFER);
    let (launch, launch_events) = mpsc::channel(EVENT_BUFFER);
    tokio::spawn(run(request, registry, starter, route, launch));
    // The route's events first, all of them: its sender is gone before the launch says
    // anything, so a question never follows it.
    ReceiverStream::new(route_events)
        .map(MstscRouteEvent::Route)
        .chain(ReceiverStream::new(launch_events))
}

async fn run<S>(
    request: MstscRouteRequest,
    registry: AnswerRegistry,
    starter: S,
    route: mpsc::Sender<ConnectionEvent>,
    events: mpsc::Sender<MstscRouteEvent>,
) where
    S: FnOnce(&str) -> Result<Running, ExternalRefusal> + Send + 'static,
{
    let MstscRouteRequest {
        before,
        gateway,
        profile,
        ssh,
        cancel,
    } = request;
    let target = heimdall_core::profile::display_address(&profile.host, profile.port);
    let prompter = Arc::new(ChannelPrompter {
        events: route.clone(),
        registry,
    });
    let gateway = match establish_via(&before, &gateway, &ssh, prompter, cancel.clone()).await {
        Ok(gateway) => Arc::new(gateway),
        Err(error) => return report_failure(at_gateway(error, true), &route, &target).await,
    };
    // Nothing more is asked on the way: the route's events end here.
    drop(route);
    let opened = local_forward::start_preferred(
        Arc::clone(&gateway),
        profile.host.clone(),
        profile.port,
        profile.local_tunnel_port,
        MSTSC_CLIENTS,
    )
    .await;
    let forward = match opened {
        Ok(forward) => forward,
        Err(error) => {
            log::warn!(
                "the Remote Desktop Connection forward to {target} could not listen: {error}"
            );
            let refused = ExternalRefusal::Forward(error.to_string());
            let _ = events.send(MstscRouteEvent::Launched(Err(refused))).await;
            return;
        }
    };
    let address = forward.address();
    // As the C# `TunnelService` says which it took.
    if profile.local_tunnel_port == Some(address.port()) {
        log::info!("Allocated tunnel port: {address} for {target}");
    } else {
        log::info!("Using OS-assigned tunnel port: {address} for {target}");
    }
    let content = rdp_external::rdp_file_through(&profile, address);
    let outcome = tokio::task::spawn_blocking(move || starter(&content))
        .await
        .unwrap_or_else(|error| Err(ExternalRefusal::NotStarted(error.to_string())));
    let running = match outcome {
        Ok(running) => running,
        Err(refusal) => {
            let _ = events.send(MstscRouteEvent::Launched(Err(refusal))).await;
            return;
        }
    };
    log::info!("Remote Desktop Connection to {target} through its gateway, forwarded at {address}");
    let _ = events.send(MstscRouteEvent::Launched(Ok(()))).await;
    tokio::select! {
        () = running.exited => {}
        () = cancel.cancelled() => {}
        () = gateway.closed() => {}
    }
    // The listener, every connection it carries, and the gateway end here.
    drop(forward);
    drop(gateway);
    log::info!("the Remote Desktop Connection forward to {target} at {address} is released");
    let _ = events.send(MstscRouteEvent::Released).await;
}
