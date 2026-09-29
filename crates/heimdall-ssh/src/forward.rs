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

//! Remote forwarding (`ssh -R`), as the C# Heimdall's gateway profiles ask for it: the server
//! listens on a port of its loopback address, and each connection it takes there comes back
//! over the SSH connection to a port of this computer's loopback address.
//!
//! The server may open a forwarded channel only on a port this side asked for: any other is
//! refused, so a server cannot reach this computer's ports on its own initiative.
//!
//! Agent forwarding (`ssh -A`) goes the same way: the server may reach this computer's SSH
//! agent only on a connection whose shell asked for it.

use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::sync::{Arc, Mutex};

use russh::Channel;
use russh::client::Msg;
use tokio::net::TcpStream;

use crate::agent;
use crate::connection::Connection;
use crate::error::ConnectError;
use crate::options::AgentSource;

/// Address the server listens on for this side: its own loopback, as in the C# Heimdall.
pub(crate) const SERVER_LOOPBACK: &str = "127.0.0.1";

/// What the server may send back over a connection: the ports it listens on for this side,
/// each with the local port its connections go to, and the agent it may reach. Shared between
/// a connection and its russh handler.
#[derive(Debug, Clone, Default)]
pub(crate) struct Routes {
    ports: Arc<Mutex<HashMap<u32, u16>>>,
    agent: Arc<Mutex<Option<AgentSource>>>,
}

impl Routes {
    /// The agent the server may reach, once a shell asked to forward it.
    pub(crate) fn agent(&self) -> Option<AgentSource> {
        self.agent.lock().ok()?.clone()
    }

    /// Lets the server reach the agent `source`.
    pub(crate) fn grant_agent(&self, source: AgentSource) {
        if let Ok(mut agent) = self.agent.lock() {
            *agent = Some(source);
        }
    }

    /// Where a connection the server forwards from its `port` goes, if this side asked for
    /// that port.
    pub(crate) fn local_port(&self, port: u32) -> Option<u16> {
        self.ports.lock().ok()?.get(&port).copied()
    }

    fn insert(&self, port: u32, local: u16) {
        if let Ok(mut routes) = self.ports.lock() {
            routes.insert(port, local);
        }
    }

    fn remove(&self, port: u32) {
        if let Ok(mut routes) = self.ports.lock() {
            routes.remove(&port);
        }
    }
}

/// Carries a channel the server forwarded to `local` on this computer's loopback address,
/// both ways until either side closes it.
pub(crate) async fn carry(channel: Channel<Msg>, local: u16) {
    match TcpStream::connect((Ipv4Addr::LOCALHOST, local)).await {
        Ok(mut near) => {
            let mut far = channel.into_stream();
            if let Err(error) = tokio::io::copy_bidirectional(&mut near, &mut far).await {
                log::debug!("a forwarded connection to local port {local} ended: {error}");
            }
        }
        Err(error) => {
            log::debug!("nothing took the forwarded connection on local port {local}: {error}");
            let _ = channel.close().await;
        }
    }
}

/// Carries an agent channel the server opened to the agent `source`, both ways until either
/// side closes it.
pub(crate) async fn carry_agent(channel: Channel<Msg>, source: AgentSource) {
    let Some(agent) = agent::connect(&source).await else {
        log::debug!("no SSH agent to forward to");
        let _ = channel.close().await;
        return;
    };
    let mut near = agent.into_inner();
    let mut far = channel.into_stream();
    if let Err(error) = tokio::io::copy_bidirectional(&mut near, &mut far).await {
        log::debug!("a forwarded agent connection ended: {error}");
    }
}

/// A remote forward in place: the server listens until it is dropped.
#[derive(Debug)]
pub struct RemoteForward {
    connection: Connection,
    port: u16,
    local: u16,
}

impl RemoteForward {
    /// The port the server listens on.
    #[must_use]
    pub fn port(&self) -> u16 {
        self.port
    }

    /// The local port its connections go to.
    #[must_use]
    pub fn local_port(&self) -> u16 {
        self.local
    }
}

impl Drop for RemoteForward {
    fn drop(&mut self) {
        let port = u32::from(self.port);
        self.connection.routes().remove(port);
        // The cancel is sent from a task: dropping cannot wait. Without a runtime the
        // connection is going too, and the server stops listening with it.
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            let connection = self.connection.clone();
            runtime.spawn(async move {
                let _ = connection
                    .handle()
                    .cancel_tcpip_forward(SERVER_LOOPBACK, port)
                    .await;
            });
        }
    }
}

/// Asks the server of `connection` to listen on its loopback `port` and send each
/// connection to `local` here.
///
/// # Errors
///
/// [`ConnectError::RemoteForwardRefused`] when the server will not listen there: forwarding
/// is off on it, or the port is taken.
pub(crate) async fn start(
    connection: &Connection,
    port: u16,
    local: u16,
) -> Result<RemoteForward, ConnectError> {
    let routes = connection.routes();
    // In place before the request: a connection may arrive as soon as the server listens.
    routes.insert(u32::from(port), local);
    match connection
        .handle()
        .tcpip_forward(SERVER_LOOPBACK, u32::from(port))
        .await
    {
        Ok(_) => Ok(RemoteForward {
            connection: connection.clone(),
            port,
            local,
        }),
        Err(error) => {
            routes.remove(u32::from(port));
            log::warn!("the server refused to listen on its port {port}: {error}");
            Err(ConnectError::RemoteForwardRefused { port })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Routes;

    #[test]
    fn only_a_port_asked_for_has_a_route() {
        let routes = Routes::default();
        assert_eq!(routes.local_port(8080), None);
        routes.insert(8080, 3000);
        assert_eq!(routes.local_port(8080), Some(3000));
        assert_eq!(routes.local_port(8081), None);
        routes.remove(8080);
        assert_eq!(routes.local_port(8080), None);
    }
}
