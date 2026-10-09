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
//! agent only on a connection whose shell asked for it. So does X11 forwarding (`ssh -X`):
//! the server may reach this computer's X server only on a connection whose shell asked.

use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::sync::{Arc, Mutex};

use russh::client::Msg;
use russh::{Channel, ChannelStream};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;
use tokio_util::sync::{CancellationToken, WaitForCancellationFutureOwned};

use crate::agent;
use crate::connection::Connection;
use crate::error::ConnectError;
use crate::options::AgentSource;
use crate::x11::{X11Display, X11Grant};

/// Address the server listens on for this side: its own loopback, as in the C# Heimdall.
pub(crate) const SERVER_LOOPBACK: &str = "127.0.0.1";

/// What the server may send back over a connection: the ports it listens on for this side,
/// each with the local port its connections go to, the agent it may reach, and the X display;
/// and whether the connection has ended. Shared between a connection and its russh handler:
/// each connection has its own, a gateway's apart from the server's beyond it.
#[derive(Debug, Clone, Default)]
pub(crate) struct Routes {
    ports: Arc<Mutex<HashMap<u32, u16>>>,
    agent: Arc<Mutex<Option<AgentSource>>>,
    x11: Arc<Mutex<X11Granted>>,
    ended: CancellationToken,
}

/// The X display granted to a connection, and how many shells hold it: it goes with the
/// last of them.
#[derive(Debug, Default)]
struct X11Granted {
    grant: Option<Arc<X11Grant>>,
    holders: usize,
}

/// A shell's hold on its connection's X11 grant: dropped, the grant goes once no other
/// shell holds it, and an X11 channel opened after that is refused.
#[derive(Debug)]
pub(crate) struct X11Hold {
    routes: Routes,
}

impl Drop for X11Hold {
    fn drop(&mut self) {
        self.routes.release_x11();
    }
}

impl Routes {
    /// Marks the connection ended: the server, the network or this side closed it.
    pub(crate) fn end(&self) {
        self.ended.cancel();
    }

    /// Completes once the connection has ended.
    pub(crate) fn ended(&self) -> WaitForCancellationFutureOwned {
        self.ended.clone().cancelled_owned()
    }

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

    /// The X display the server may reach, while a shell that asked to forward X11 lasts.
    pub(crate) fn x11(&self) -> Option<Arc<X11Grant>> {
        self.x11.lock().ok()?.grant.clone()
    }

    /// Lets the server reach `display` while the hold returned lasts: the grant already
    /// given, or a new one; `None` when none can be made.
    pub(crate) fn grant_x11(&self, display: &X11Display) -> Option<(Arc<X11Grant>, X11Hold)> {
        let mut granted = self.x11.lock().ok()?;
        if granted.grant.is_none() {
            granted.grant = Some(Arc::new(X11Grant::new(display)?));
        }
        granted.holders += 1;
        let grant = granted.grant.clone()?;
        Some((
            grant,
            X11Hold {
                routes: self.clone(),
            },
        ))
    }

    /// One shell less holds the X11 grant; with none left, it goes.
    fn release_x11(&self) {
        if let Ok(mut granted) = self.x11.lock() {
            granted.holders = granted.holders.saturating_sub(1);
            if granted.holders == 0 {
                granted.grant = None;
            }
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
        Ok(near) => {
            let what = format!("a forwarded connection to local port {local}");
            join(near, channel.into_stream(), &what).await;
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
    join(
        agent.into_inner(),
        channel.into_stream(),
        "a forwarded agent connection",
    )
    .await;
}

/// Carries bytes between `near`, on this computer, and `far`, a channel the server opened,
/// both ways until either side closes; `what` names the connection in the debug log.
pub(crate) async fn join<N: AsyncRead + AsyncWrite + Unpin>(
    mut near: N,
    mut far: ChannelStream<Msg>,
    what: &str,
) {
    if let Err(error) = tokio::io::copy_bidirectional(&mut near, &mut far).await {
        log::debug!("{what} ended: {error}");
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
    use crate::x11::X11Display;

    #[test]
    fn the_x11_grant_lasts_while_a_shell_holds_it() {
        let routes = Routes::default();
        assert!(routes.x11().is_none());
        let display = X11Display::parse(":0")
            .expect("display")
            .with_authority(None);
        let (first, held) = routes.grant_x11(&display).expect("granted");
        let (second, also_held) = routes.grant_x11(&display).expect("granted");
        assert!(
            std::sync::Arc::ptr_eq(&first, &second),
            "one cookie a connection"
        );
        drop(held);
        assert!(routes.x11().is_some(), "the second shell still holds it");
        drop(also_held);
        assert!(routes.x11().is_none(), "gone with the last shell");
    }

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
