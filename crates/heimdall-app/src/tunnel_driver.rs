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

//! Opening a tunnel the user asked for: the route to its gateway, then a port of this
//! computer listening for it, held until it is closed or the gateway goes.
//!
//! The route asks its questions as a tab's does, as connection events; the application
//! answers them only from what is saved for the gateways on the way, as the C# opens a
//! tunnel without asking anything.

use std::sync::Arc;

use heimdall_core::profile::SshProfile;
use heimdall_ssh::{ConnectOptions, at_gateway, establish_via, local_forward};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::{Stream, StreamExt as _};
use tokio_util::sync::CancellationToken;

use crate::driver::{AnswerRegistry, ChannelPrompter, report_failure};
use crate::error::UiError;
use crate::event::ConnectionEvent;
use crate::tunnel::TunnelEvent;

/// Events held while the application reads them: a tunnel reports a handful.
const EVENT_BUFFER: usize = 8;

/// A tunnel to open.
#[derive(Debug)]
pub struct TunnelRequest {
    /// The gateways to go through to reach [`TunnelRequest::gateway`], nearest first.
    pub before: Vec<SshProfile>,
    /// The gateway that carries the tunnel.
    pub gateway: SshProfile,
    /// Where the tunnel's connections go, from the last gateway.
    pub remote_host: String,
    /// The port there.
    pub remote_port: u16,
    /// The port of this computer's loopback address to listen on.
    pub local_port: u16,
    /// SSH options for the route.
    pub ssh: ConnectOptions,
    /// Stops the attempt, then closes the tunnel.
    pub cancel: CancellationToken,
}

/// Opens the tunnel `request` asks for: what the route reports, then
/// [`TunnelEvent::Opened`], then [`TunnelEvent::Closed`] should the gateway go.
pub fn tunnel_events(
    request: TunnelRequest,
    registry: AnswerRegistry,
) -> impl Stream<Item = TunnelEvent> + Send + 'static {
    let (route, route_events) = mpsc::channel(EVENT_BUFFER);
    let (tunnel, tunnel_events) = mpsc::channel(EVENT_BUFFER);
    tokio::spawn(run(request, registry, route, tunnel));
    // The route's events first, all of them: its sender is gone before the tunnel says
    // anything, so a question never follows the opening.
    ReceiverStream::new(route_events)
        .map(TunnelEvent::Route)
        .chain(ReceiverStream::new(tunnel_events))
}

async fn run(
    request: TunnelRequest,
    registry: AnswerRegistry,
    route: mpsc::Sender<ConnectionEvent>,
    tunnel: mpsc::Sender<TunnelEvent>,
) {
    let target = format!("{}:{}", request.remote_host, request.remote_port);
    let prompter = Arc::new(ChannelPrompter {
        events: route.clone(),
        registry,
    });
    let gateway = match establish_via(
        &request.before,
        &request.gateway,
        &request.ssh,
        prompter,
        request.cancel.clone(),
    )
    .await
    {
        Ok(gateway) => Arc::new(gateway),
        // The tunnel's own end is a gateway: a refusal there is said as one.
        Err(error) => return report_failure(at_gateway(error, true), &route, &target).await,
    };
    let listening = local_forward::start_on(
        Arc::clone(&gateway),
        request.remote_host.clone(),
        request.remote_port,
        Some(request.local_port),
    )
    .await;
    let forward = match listening {
        Ok(forward) => forward,
        Err(error) => {
            log::warn!(
                "the tunnel to {target} could not listen on port {}: {error}",
                request.local_port
            );
            let failed = if local_forward::port_unavailable(&error) {
                UiError::LocalPortUnavailable {
                    port: request.local_port,
                }
            } else {
                UiError::network(&error)
            };
            let _ = route.send(ConnectionEvent::Failed(failed)).await;
            return;
        }
    };
    // Nothing more comes on the way: the route's events end here.
    drop(route);
    log::info!("tunnel to {target} open at {}", forward.address());
    if tunnel
        .send(TunnelEvent::Opened(forward.address()))
        .await
        .is_err()
    {
        return;
    }
    tokio::select! {
        () = request.cancel.cancelled() => log::info!("tunnel to {target} closed"),
        () = gateway.closed() => {
            log::warn!("tunnel to {target} closed: its gateway connection ended");
            let _ = tunnel.send(TunnelEvent::Closed).await;
        }
        () = tunnel.closed() => {}
    }
    // The listener and every connection it carries end here.
    drop(forward);
}
