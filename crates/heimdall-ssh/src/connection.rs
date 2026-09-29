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

//! An authenticated SSH connection that several channels share.
//!
//! A shell and SFTP channels on one connection authenticate once. Each channel keeps a
//! clone of the [`Connection`] and closes only itself; the connection sends its disconnect
//! when the last clone goes, so closing a terminal never ends a transfer on the same
//! connection.
//!
//! russh delivers every channel's data from one session loop that waits for each channel's
//! queue: a channel whose reader stops reading stalls the others. Readers of channels
//! opened here must keep reading.

use std::io;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use russh::client::{Handle, Msg};
use russh::{ChannelMsg, ChannelStream, Disconnect};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio_util::sync::CancellationToken;

use crate::client::{ClientHandler, Reached};
use crate::error::ConnectError;
use crate::forward::{self, RemoteForward, Routes};
use crate::options::ConnectOptions;
use crate::session::{self, ShellSession};

/// Description sent with the disconnect.
const DISCONNECT_DESCRIPTION: &str = "";

/// Language tag sent with the disconnect.
const DISCONNECT_LANGUAGE: &str = "";

struct Inner {
    /// Present until the last clone drops.
    handle: Option<Handle<ClientHandler>>,
    /// The remote forwards the handler serves.
    routes: Routes,
}

impl Drop for Inner {
    fn drop(&mut self) {
        let Some(handle) = self.handle.take() else {
            return;
        };
        // The disconnect is sent from a task: dropping cannot wait. Outside a runtime the
        // TCP connection simply closes with the handle.
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            runtime.spawn(async move {
                let _ = handle
                    .disconnect(
                        Disconnect::ByApplication,
                        DISCONNECT_DESCRIPTION,
                        DISCONNECT_LANGUAGE,
                    )
                    .await;
            });
        }
    }
}

/// An authenticated connection. Cheap to clone; the last clone dropped disconnects.
#[derive(Clone)]
pub struct Connection {
    inner: Arc<Inner>,
}

impl std::fmt::Debug for Connection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Connection")
            .field("closed", &self.is_closed())
            .finish_non_exhaustive()
    }
}

impl Connection {
    pub(crate) fn new((handle, routes): Reached) -> Self {
        Self {
            inner: Arc::new(Inner {
                handle: Some(handle),
                routes,
            }),
        }
    }

    pub(crate) fn routes(&self) -> &Routes {
        &self.inner.routes
    }

    /// Asks the server to listen on its loopback `port` and to send each connection it takes
    /// there to `local` on this computer's loopback address (`ssh -R`), until the returned
    /// forward is dropped.
    ///
    /// # Errors
    ///
    /// [`ConnectError::RemoteForwardRefused`] when the server will not listen there.
    pub async fn forward_remote(
        &self,
        port: u16,
        local: u16,
    ) -> Result<RemoteForward, ConnectError> {
        forward::start(self, port, local).await
    }

    pub(crate) fn handle(&self) -> &Handle<ClientHandler> {
        self.inner
            .handle
            .as_ref()
            .expect("the handle is present until the last clone drops")
    }

    /// Whether the server or the network ended the connection.
    #[must_use]
    pub fn is_closed(&self) -> bool {
        self.handle().is_closed()
    }

    /// Opens an interactive shell on this connection. `cancel` ends the shell only; the
    /// connection stays while other clones exist.
    ///
    /// # Errors
    ///
    /// [`ConnectError::PtyRefused`], [`ConnectError::ShellRefused`], or a protocol error.
    pub async fn open_shell(
        &self,
        options: &ConnectOptions,
        cancel: CancellationToken,
    ) -> Result<ShellSession, ConnectError> {
        session::open(self.clone(), options, cancel).await
    }

    /// Opens the subsystem `name` (such as `sftp`) as a byte stream. The server's answer is
    /// awaited for at most `timeout`: a server that neither accepts nor refuses cannot hang
    /// the caller.
    ///
    /// # Errors
    ///
    /// [`ConnectError::SubsystemRefused`] when the server says no, [`ConnectError::Timeout`]
    /// without an answer, or a protocol error.
    pub async fn open_subsystem(
        &self,
        name: &str,
        timeout: Duration,
    ) -> Result<SubsystemStream, ConnectError> {
        let opening = async {
            let mut channel = self
                .handle()
                .channel_open_session()
                .await
                .map_err(ConnectError::Protocol)?;
            channel
                .request_subsystem(true, name)
                .await
                .map_err(ConnectError::Protocol)?;
            loop {
                match channel.wait().await {
                    Some(ChannelMsg::Success) => return Ok(channel),
                    Some(ChannelMsg::Failure) => {
                        return Err(ConnectError::SubsystemRefused {
                            name: name.to_owned(),
                        });
                    }
                    Some(_) => {}
                    None => {
                        return Err(ConnectError::Disconnected {
                            server_message: None,
                        });
                    }
                }
            }
        };
        let channel = tokio::time::timeout(timeout, opening)
            .await
            .map_err(|_| ConnectError::Timeout)??;
        Ok(ChannelBytes {
            stream: channel.into_stream(),
            _connection: self.clone(),
        })
    }

    /// Asks the server to connect onward to `host:port` and carry the bytes both ways: a TCP
    /// connection made from the server, as a gateway makes one. The server resolves `host`
    /// itself, so it may be a name only the server knows.
    ///
    /// # Errors
    ///
    /// [`ConnectError::JumpRefused`] when the server will not connect onward, or a protocol
    /// error.
    pub async fn open_tunnel(&self, host: &str, port: u16) -> Result<Tunnel, ConnectError> {
        let channel = self
            .handle()
            .channel_open_direct_tcpip(host, u32::from(port), TUNNEL_ORIGINATOR, 0)
            .await
            .map_err(|error| match error {
                russh::Error::ChannelOpenFailure(_) => ConnectError::JumpRefused {
                    host: host.to_owned(),
                    port,
                },
                other => ConnectError::Protocol(other),
            })?;
        Ok(ChannelBytes {
            stream: channel.into_stream(),
            _connection: self.clone(),
        })
    }
}

/// Originator address reported with a tunnel: there is no address of this side to give.
const TUNNEL_ORIGINATOR: &str = "127.0.0.1";

/// A subsystem's bytes, both ways.
pub type SubsystemStream = ChannelBytes;

/// A tunnel's bytes, both ways.
pub type Tunnel = ChannelBytes;

/// The bytes of one channel, both ways. Keeps its connection alive; dropping it closes the
/// channel. Drop it inside the tokio runtime: russh closes the channel from a task.
pub struct ChannelBytes {
    stream: ChannelStream<Msg>,
    _connection: Connection,
}

impl std::fmt::Debug for ChannelBytes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChannelBytes").finish_non_exhaustive()
    }
}

impl AsyncRead for ChannelBytes {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stream).poll_read(cx, buf)
    }
}

impl AsyncWrite for ChannelBytes {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.stream).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stream).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stream).poll_shutdown(cx)
    }
}
