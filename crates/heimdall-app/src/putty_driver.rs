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

//! An SSH profile opened in `PuTTY` through its SSH gateway, as the C# `SshHandler` external
//! mode with a tunnel. The route to the gateway comes first, its questions asked as a
//! tunnel's. Then the server's host key is probed through the gateway and checked against
//! the application's own `known_hosts` for the server's own host and port. Then a forward
//! listens on this computer's loopback address, on the profile's own port when it has one
//! and it is free, else on one the system picks, as the C# `TunnelManager.AllocatePort`, and
//! `PuTTY` is pointed at it, told to accept the server's key alone.
//!
//! `PuTTY` keeps the keys it saw by host and port, and the forward's port may change each time.
//! Given `-hostkey`, it accepts that key without reading its cache or writing to it, so it
//! never asks about `127.0.0.1` and its cache does not grow a row per port.
//!
//! The forward and the gateway live as long as `PuTTY` does: a task waits for it to exit,
//! then releases both. Cancelling the request, as the application does when it closes,
//! releases them too.

use std::sync::Arc;

use heimdall_core::profile::SshProfile;
use heimdall_ssh::{ConnectOptions, at_gateway, establish_via, local_forward};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::{Stream, StreamExt as _};
use tokio_util::sync::CancellationToken;

use crate::driver::{AnswerRegistry, ChannelPrompter, report_failure};
use crate::event::ConnectionEvent;
use crate::putty::{self, HostKeyProbe, PuttyLaunch, PuttyRefusal, PuttyStarted, Running};

/// Events held while the application reads them: a launch reports a handful.
const EVENT_BUFFER: usize = 8;

/// Clients the forward carries at once: `PuTTY`'s own connection, and the one its "Restart
/// Session" dials, which may come before the first is seen gone. Any other program of this
/// computer is refused beyond that.
pub const PUTTY_CLIENTS: usize = 2;

/// Identifies a launch through a gateway, from its start to the release of its forward.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PuttyRouteId(u64);

impl PuttyRouteId {
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

impl Default for PuttyRouteId {
    fn default() -> Self {
        Self(1)
    }
}

/// A launch of `PuTTY` through a gateway.
#[derive(Debug)]
pub struct PuttyRouteRequest {
    /// The gateways to go through to reach [`PuttyRouteRequest::gateway`], nearest first.
    pub before: Vec<SshProfile>,
    /// The gateway that carries the forward.
    pub gateway: SshProfile,
    /// The profile opened: its server's host and port, as the gateway reaches them.
    pub profile: SshProfile,
    /// How `PuTTY` starts; its host key and address are filled in on the way.
    pub launch: PuttyLaunch,
    /// SSH options for the route and the probe, `known_hosts` among them.
    pub ssh: ConnectOptions,
    /// Stops the attempt, then releases the forward and the gateway.
    pub cancel: CancellationToken,
}

/// What a launch through a gateway reports.
#[derive(Debug, Clone)]
pub enum PuttyRouteEvent {
    /// What the route to the gateway reports: a question, a gateway's unknown key, or the
    /// failure that stops the launch.
    Route(ConnectionEvent),
    /// The server's host key, probed through the gateway, is not trusted: never seen, or
    /// changed, or not read. The launch stopped.
    HostKey(HostKeyProbe),
    /// `PuTTY` started on the forward, or why not.
    Launched(Result<PuttyStarted, PuttyRefusal>),
    /// `PuTTY` exited, or the launch was cancelled: the forward and the gateway are
    /// released.
    Released,
}

/// Launches `PuTTY` as `request` asks, `starter` starting it: what the route reports first,
/// then the rest. `starter` runs off the runtime's threads, as starting a program waits on
/// the system; [`putty::start`] is the real one.
pub fn putty_route_events<S>(
    request: PuttyRouteRequest,
    registry: AnswerRegistry,
    starter: S,
) -> impl Stream<Item = PuttyRouteEvent> + Send + 'static
where
    S: FnOnce(&PuttyLaunch) -> Result<Running, PuttyRefusal> + Send + 'static,
{
    let (route, route_events) = mpsc::channel(EVENT_BUFFER);
    let (launch, launch_events) = mpsc::channel(EVENT_BUFFER);
    tokio::spawn(run(request, registry, starter, route, launch));
    // The route's events first, all of them: its sender is gone before the launch says
    // anything, so a question never follows it.
    ReceiverStream::new(route_events)
        .map(PuttyRouteEvent::Route)
        .chain(ReceiverStream::new(launch_events))
}

async fn run<S>(
    request: PuttyRouteRequest,
    registry: AnswerRegistry,
    starter: S,
    route: mpsc::Sender<ConnectionEvent>,
    events: mpsc::Sender<PuttyRouteEvent>,
) where
    S: FnOnce(&PuttyLaunch) -> Result<Running, PuttyRefusal> + Send + 'static,
{
    let PuttyRouteRequest {
        before,
        gateway,
        profile,
        launch,
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
    let probed = heimdall_ssh::trusted_host_key_via(&gateway, &profile, &ssh, &cancel).await;
    let host_key = match putty::host_key_probe(&profile, probed) {
        HostKeyProbe::Trusted(host_key) => host_key,
        other => {
            let _ = events.send(PuttyRouteEvent::HostKey(other)).await;
            return;
        }
    };
    let opened = local_forward::start_preferred(
        Arc::clone(&gateway),
        profile.host.clone(),
        profile.port,
        profile.local_tunnel_port,
        PUTTY_CLIENTS,
    )
    .await;
    let forward = match opened {
        Ok(forward) => forward,
        Err(error) => {
            log::warn!("the PuTTY forward to {target} could not listen: {error}");
            let refused = PuttyRefusal::Forward(error.to_string());
            let _ = events.send(PuttyRouteEvent::Launched(Err(refused))).await;
            return;
        }
    };
    let address = forward.address();
    let launch = PuttyLaunch { host_key, ..launch }.through(address);
    let outcome = tokio::task::spawn_blocking(move || starter(&launch))
        .await
        .unwrap_or_else(|error| Err(PuttyRefusal::NotStarted(error.to_string())));
    let running = match outcome {
        Ok(running) => running,
        Err(refusal) => {
            let _ = events.send(PuttyRouteEvent::Launched(Err(refusal))).await;
            return;
        }
    };
    log::info!("PuTTY to {target} through its gateway, forwarded at {address}");
    let _ = events
        .send(PuttyRouteEvent::Launched(Ok(running.started)))
        .await;
    tokio::select! {
        () = running.exited => {}
        () = cancel.cancelled() => {}
        () = gateway.closed() => {}
    }
    // The listener, every connection it carries, and the gateway end here.
    drop(forward);
    drop(gateway);
    log::info!("the PuTTY forward to {target} at {address} is released");
    let _ = events.send(PuttyRouteEvent::Released).await;
}
