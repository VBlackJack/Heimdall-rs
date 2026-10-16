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

//! An FTP server run in the test, on a port it holds from the start: the listener is bound
//! here and kept, each connection it accepts handed to libunftp. A port picked free, let go,
//! then bound again by the server could be taken in between by a test running beside it,
//! and the test would talk to that test's server.

use std::future::Future;
use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::ops::RangeInclusive;
use std::path::{Path, PathBuf};
use std::pin::Pin;

use tokio::net::TcpSocket;
use unftp_sbe_fs::Filesystem;

/// Binds each passive data port where the system puts it: a port no other socket holds.
///
/// libunftp's own binding takes a random port in 49152-65535 with `SO_REUSEADDR` set.
/// Windows lets two sockets with that option listen on one port, every connection then
/// going to the first: a data connection could reach another test's server, the transfer
/// never reaching this one, and the client wait for its reply until the server's 600 s idle
/// timeout closed the session (CI run 38892908592). Linux refuses the second bind instead.
#[derive(Debug)]
struct SystemPort;

impl libunftp::options::Binder for SystemPort {
    fn bind<'a, 'b>(
        &'a mut self,
        local_addr: IpAddr,
        _passive_ports: RangeInclusive<u16>,
    ) -> Pin<Box<dyn Future<Output = io::Result<TcpSocket>> + Send + 'b>>
    where
        'a: 'b,
        Self: 'b,
    {
        let socket = if local_addr.is_ipv4() {
            TcpSocket::new_v4()
        } else {
            TcpSocket::new_v6()
        };
        let bound =
            socket.and_then(|socket| socket.bind(SocketAddr::new(local_addr, 0)).map(|()| socket));
        Box::pin(std::future::ready(bound))
    }
}

/// Serves `root` over FTP, over explicit FTPS with the certificate and key files `ftps`
/// names when given, on a free local port; returns the port.
///
/// The listener is bound before this returns, so a connection made then is queued until
/// the server accepts it: no wait for the server to start.
pub async fn serve(root: &Path, ftps: Option<(PathBuf, PathBuf)>) -> u16 {
    let home = root.to_owned();
    let build = move || {
        let home = home.clone();
        let mut builder = libunftp::ServerBuilder::new(Box::new(move || {
            Filesystem::new(home.clone()).expect("root")
        }))
        .binder(SystemPort);
        if let Some((cert, key)) = ftps.clone() {
            builder = builder.ftps(cert, key);
        }
        builder.build().expect("server")
    };
    // Built here once, so a server that cannot be configured fails the test, not a task.
    let mut first = Some(build());
    let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .await
        .expect("free port");
    let port = listener.local_addr().expect("address").port();
    tokio::spawn(async move {
        loop {
            // A connection that fails before it is accepted is the client's affair, as
            // libunftp's own accept loop treats it.
            if let Ok((stream, _)) = listener.accept().await {
                let server = first.take().unwrap_or_else(&build);
                tokio::spawn(server.service(stream));
            }
        }
    });
    port
}
