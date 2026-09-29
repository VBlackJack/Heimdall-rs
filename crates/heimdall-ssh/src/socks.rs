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

//! A SOCKS5 proxy on this computer whose connections leave from an SSH gateway, as the C#
//! Heimdall's gateway profiles open one (`ssh -D`): a browser set to it reaches what the
//! gateway reaches.
//!
//! RFC 1928 without authentication, CONNECT only, to an IPv4 or IPv6 address or a name the
//! gateway resolves. [`start`] listens on the loopback address, where only this computer
//! reaches it.

use std::future::Future;
use std::io;
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncReadExt as _, AsyncWrite, AsyncWriteExt as _};
use tokio::net::TcpListener;
use tokio_util::sync::{CancellationToken, DropGuard};

use crate::connection::{Connection, Tunnel};
use crate::error::ConnectError;

/// The protocol's version byte.
const VERSION: u8 = 5;
/// The authentication method "none".
const NO_AUTHENTICATION: u8 = 0;
/// No method offered is accepted.
const NO_ACCEPTABLE_METHOD: u8 = 0xFF;
/// The CONNECT command.
const CONNECT: u8 = 1;
/// Address types.
const IPV4: u8 = 1;
const DOMAIN_NAME: u8 = 3;
const IPV6: u8 = 4;
/// Reply codes.
const SUCCEEDED: u8 = 0;
const HOST_UNREACHABLE: u8 = 4;
const COMMAND_NOT_SUPPORTED: u8 = 7;
const ADDRESS_TYPE_NOT_SUPPORTED: u8 = 8;
/// Longest wait for a client to say where it goes: a client that says nothing is let go.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);
/// Wait after a failed accept, so a listener in trouble does not spin.
const ACCEPT_RETRY: Duration = Duration::from_millis(100);

/// What opens the connections onward: the gateway.
pub trait Opener: Send + Sync + 'static {
    /// Both ways of a connection onward.
    type Stream: AsyncRead + AsyncWrite + Unpin + Send + 'static;

    /// Connects onward to `host:port`, from the gateway.
    fn open(
        &self,
        host: String,
        port: u16,
    ) -> impl Future<Output = Result<Self::Stream, ConnectError>> + Send;
}

impl Opener for Connection {
    type Stream = Tunnel;

    async fn open(&self, host: String, port: u16) -> Result<Tunnel, ConnectError> {
        self.open_tunnel(&host, port).await
    }
}

/// Serves SOCKS5 on `listener` until `cancel`: each client's connection is opened onward by
/// `opener`, then carried both ways until either side closes it.
pub async fn serve<O: Opener>(listener: TcpListener, opener: Arc<O>, cancel: CancellationToken) {
    loop {
        let accepted = tokio::select! {
            () = cancel.cancelled() => return,
            accepted = listener.accept() => accepted,
        };
        match accepted {
            Ok((client, _)) => {
                let opener = Arc::clone(&opener);
                let cancel = cancel.clone();
                tokio::spawn(async move {
                    tokio::select! {
                        () = cancel.cancelled() => {}
                        result = serve_one(client, opener.as_ref()) => {
                            if let Err(error) = result {
                                log::debug!("a SOCKS client ended: {error}");
                            }
                        }
                    }
                });
            }
            Err(error) => {
                log::warn!("the SOCKS proxy could not take a client: {error}");
                tokio::time::sleep(ACCEPT_RETRY).await;
            }
        }
    }
}

/// A SOCKS proxy that runs until dropped; the connections it carries end with it.
#[derive(Debug)]
pub struct Proxy {
    address: SocketAddr,
    _stop: DropGuard,
}

impl Proxy {
    /// Where the proxy listens.
    #[must_use]
    pub fn address(&self) -> SocketAddr {
        self.address
    }
}

/// Starts a SOCKS proxy on `port` of the loopback address, where only this computer reaches
/// it, opening its connections onward by `opener`.
///
/// # Errors
///
/// The port could not be taken: another program holds it.
pub async fn start<O: Opener>(port: u16, opener: Arc<O>) -> io::Result<Proxy> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, port)).await?;
    let address = listener.local_addr()?;
    let stop = CancellationToken::new();
    tokio::spawn(serve(listener, opener, stop.clone()));
    Ok(Proxy {
        address,
        _stop: stop.drop_guard(),
    })
}

/// Serves one client: where it goes, the connection onward, then the bytes both ways.
///
/// # Errors
///
/// The client spoke another protocol, asked for what is not offered, went silent, or a side
/// failed while bytes were carried.
pub async fn serve_one<C, O>(mut client: C, opener: &O) -> io::Result<()>
where
    C: AsyncRead + AsyncWrite + Unpin,
    O: Opener,
{
    let (host, port) = tokio::time::timeout(HANDSHAKE_TIMEOUT, destination(&mut client))
        .await
        .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "no request in time"))??;
    match opener.open(host, port).await {
        Ok(mut onward) => {
            reply(&mut client, SUCCEEDED).await?;
            tokio::io::copy_bidirectional(&mut client, &mut onward).await?;
            Ok(())
        }
        Err(error) => {
            reply(&mut client, HOST_UNREACHABLE).await?;
            Err(io::Error::other(error.to_string()))
        }
    }
}

/// Reads the greeting and the request: where the client goes. What is refused is answered
/// before the error returns.
async fn destination<C>(client: &mut C) -> io::Result<(String, u16)>
where
    C: AsyncRead + AsyncWrite + Unpin,
{
    let [version, methods] = read_array(client).await?;
    if version != VERSION {
        return Err(invalid("not SOCKS5"));
    }
    let mut offered = vec![0; usize::from(methods)];
    client.read_exact(&mut offered).await?;
    if !offered.contains(&NO_AUTHENTICATION) {
        client.write_all(&[VERSION, NO_ACCEPTABLE_METHOD]).await?;
        return Err(invalid("no method without authentication offered"));
    }
    client.write_all(&[VERSION, NO_AUTHENTICATION]).await?;

    let [version, command, _reserved, address_type] = read_array(client).await?;
    if version != VERSION {
        return Err(invalid("not SOCKS5"));
    }
    if command != CONNECT {
        reply(client, COMMAND_NOT_SUPPORTED).await?;
        return Err(invalid("only CONNECT is offered"));
    }
    let host = match address_type {
        IPV4 => Ipv4Addr::from(read_array::<_, 4>(client).await?).to_string(),
        IPV6 => Ipv6Addr::from(read_array::<_, 16>(client).await?).to_string(),
        DOMAIN_NAME => {
            let [length] = read_array(client).await?;
            let mut name = vec![0; usize::from(length)];
            client.read_exact(&mut name).await?;
            match String::from_utf8(name) {
                Ok(name) if !name.is_empty() => name,
                _ => {
                    reply(client, ADDRESS_TYPE_NOT_SUPPORTED).await?;
                    return Err(invalid("a name that is not text"));
                }
            }
        }
        _ => {
            reply(client, ADDRESS_TYPE_NOT_SUPPORTED).await?;
            return Err(invalid("an address type not known"));
        }
    };
    let port = u16::from_be_bytes(read_array(client).await?);
    Ok((host, port))
}

async fn read_array<C: AsyncRead + Unpin, const N: usize>(client: &mut C) -> io::Result<[u8; N]> {
    let mut bytes = [0; N];
    client.read_exact(&mut bytes).await?;
    Ok(bytes)
}

/// Answers the request with `code`; the bound address is not told, as `ssh -D` does not.
async fn reply<C: AsyncWrite + Unpin>(client: &mut C, code: u8) -> io::Result<()> {
    client
        .write_all(&[VERSION, code, 0, IPV4, 0, 0, 0, 0, 0, 0])
        .await
}

fn invalid(what: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, what.to_owned())
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use tokio::io::DuplexStream;

    use super::*;

    /// Opens onward into a pipe whose other end the test holds; says where it was asked to go.
    #[derive(Default)]
    struct FakeGateway {
        asked: Mutex<Vec<(String, u16)>>,
        server: Mutex<Option<DuplexStream>>,
        refuse: bool,
    }

    impl Opener for FakeGateway {
        type Stream = DuplexStream;

        fn open(
            &self,
            host: String,
            port: u16,
        ) -> impl Future<Output = Result<DuplexStream, ConnectError>> + Send {
            self.asked.lock().expect("asked").push((host.clone(), port));
            let result = if self.refuse {
                Err(ConnectError::JumpRefused { host, port })
            } else {
                let (near, far) = tokio::io::duplex(1024);
                *self.server.lock().expect("server") = Some(far);
                Ok(near)
            };
            async move { result }
        }
    }

    const GREETING: [u8; 3] = [5, 1, 0];

    fn request(address_type: u8, address: &[u8], port: u16) -> Vec<u8> {
        let mut bytes = vec![5, CONNECT, 0, address_type];
        bytes.extend_from_slice(address);
        bytes.extend_from_slice(&port.to_be_bytes());
        bytes
    }

    /// Sends `sent` as a client, then reads `expected.len()` bytes back.
    async fn exchange(gateway: &FakeGateway, sent: &[u8], expected: usize) -> (Vec<u8>, bool) {
        let (mut client, proxy) = tokio::io::duplex(1024);
        let serving = serve_one(proxy, gateway);
        let talking = async {
            client.write_all(sent).await.expect("write");
            let mut answer = vec![0; expected];
            client.read_exact(&mut answer).await.expect("read");
            // Both ends closed: the carrying of bytes, if it began, ends.
            drop(client);
            drop(gateway.server.lock().expect("server").take());
            answer
        };
        let (served, answer) = tokio::join!(serving, talking);
        (answer, served.is_ok())
    }

    #[tokio::test]
    async fn a_name_is_opened_onward_and_the_bytes_go_both_ways() {
        let gateway = Arc::new(FakeGateway::default());
        let (mut client, proxy) = tokio::io::duplex(1024);
        let serving = tokio::spawn({
            let gateway = Arc::clone(&gateway);
            async move { serve_one(proxy, gateway.as_ref()).await }
        });
        client.write_all(&GREETING).await.expect("greeting");
        let mut chosen = [0; 2];
        client.read_exact(&mut chosen).await.expect("method");
        assert_eq!(chosen, [5, NO_AUTHENTICATION]);
        let mut name = vec![9];
        name.extend_from_slice(b"intranet1");
        client
            .write_all(&request(DOMAIN_NAME, &name, 8080))
            .await
            .expect("request");
        let mut answer = [0; 10];
        client.read_exact(&mut answer).await.expect("reply");
        assert_eq!(answer[..2], [5, SUCCEEDED]);
        assert_eq!(
            gateway.asked.lock().expect("asked").as_slice(),
            [("intranet1".to_owned(), 8080)]
        );
        let mut server = gateway
            .server
            .lock()
            .expect("server")
            .take()
            .expect("opened");
        client.write_all(b"GET /").await.expect("out");
        let mut out = [0; 5];
        server.read_exact(&mut out).await.expect("arrives");
        assert_eq!(&out, b"GET /");
        server.write_all(b"200").await.expect("back");
        let mut back = [0; 3];
        client.read_exact(&mut back).await.expect("returns");
        assert_eq!(&back, b"200");
        drop(client);
        drop(server);
        serving.await.expect("task").expect("served");
    }

    #[tokio::test]
    async fn addresses_of_both_families_are_read_as_the_gateway_takes_them() {
        for (address_type, address, expected) in [
            (IPV4, vec![10, 0, 0, 7], "10.0.0.7".to_owned()),
            (
                IPV6,
                Ipv6Addr::new(0xfd00, 0, 0, 0, 0, 0, 0, 1).octets().to_vec(),
                "fd00::1".to_owned(),
            ),
        ] {
            let gateway = FakeGateway::default();
            let mut sent = GREETING.to_vec();
            sent.extend(request(address_type, &address, 443));
            let (answer, _) = exchange(&gateway, &sent, 12).await;
            assert_eq!(answer[2..4], [5, SUCCEEDED], "{expected}");
            assert_eq!(
                gateway.asked.lock().expect("asked").as_slice(),
                [(expected, 443)]
            );
        }
    }

    #[tokio::test]
    async fn a_client_offering_only_a_password_is_refused() {
        let gateway = FakeGateway::default();
        let (answer, served) = exchange(&gateway, &[5, 1, 2], 2).await;
        assert_eq!(answer, [5, NO_ACCEPTABLE_METHOD]);
        assert!(!served);
        assert!(gateway.asked.lock().expect("asked").is_empty());
    }

    #[tokio::test]
    async fn only_connect_is_offered() {
        let gateway = FakeGateway::default();
        let mut sent = GREETING.to_vec();
        // BIND, then UDP ASSOCIATE.
        sent.extend([5, 2, 0, IPV4, 10, 0, 0, 7, 0, 80]);
        let (answer, served) = exchange(&gateway, &sent, 4).await;
        assert_eq!(answer, [5, NO_AUTHENTICATION, 5, COMMAND_NOT_SUPPORTED]);
        assert!(!served);
        assert!(gateway.asked.lock().expect("asked").is_empty());
    }

    #[tokio::test]
    async fn an_unknown_address_type_or_an_empty_name_is_refused() {
        for sent in [
            request(9, &[1, 2, 3, 4], 80),
            request(DOMAIN_NAME, &[0], 80),
            request(DOMAIN_NAME, &[2, 0xFF, 0xFE], 80),
        ] {
            let gateway = FakeGateway::default();
            let mut all = GREETING.to_vec();
            all.extend(&sent);
            let (answer, served) = exchange(&gateway, &all, 4).await;
            assert_eq!(
                answer,
                [5, NO_AUTHENTICATION, 5, ADDRESS_TYPE_NOT_SUPPORTED]
            );
            assert!(!served);
            assert!(gateway.asked.lock().expect("asked").is_empty());
        }
    }

    #[tokio::test]
    async fn another_protocol_is_let_go() {
        let gateway = FakeGateway::default();
        let (mut client, proxy) = tokio::io::duplex(1024);
        let serving = serve_one(proxy, &gateway);
        let talking = async {
            // SOCKS4.
            client.write_all(&[4, 1, 0, 80]).await.expect("write");
        };
        let (served, ()) = tokio::join!(serving, talking);
        assert!(served.is_err());
    }

    #[tokio::test]
    async fn a_refusal_of_the_gateway_is_said_to_the_client() {
        let gateway = FakeGateway {
            refuse: true,
            ..FakeGateway::default()
        };
        let mut sent = GREETING.to_vec();
        sent.extend(request(IPV4, &[10, 0, 0, 7], 22));
        let (answer, served) = exchange(&gateway, &sent, 4).await;
        assert_eq!(answer, [5, NO_AUTHENTICATION, 5, HOST_UNREACHABLE]);
        assert!(!served);
    }

    #[tokio::test]
    async fn the_proxy_serves_on_its_listener_until_cancelled() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .expect("listener");
        let address = listener.local_addr().expect("address");
        let gateway = Arc::new(FakeGateway::default());
        let cancel = CancellationToken::new();
        let proxy = tokio::spawn(serve(listener, Arc::clone(&gateway), cancel.clone()));
        let mut client = tokio::net::TcpStream::connect(address)
            .await
            .expect("connect");
        client.write_all(&GREETING).await.expect("greeting");
        client
            .write_all(&request(IPV4, &[10, 0, 0, 7], 22))
            .await
            .expect("request");
        let mut answer = [0; 12];
        client.read_exact(&mut answer).await.expect("reply");
        assert_eq!(answer[2..4], [5, SUCCEEDED]);
        cancel.cancel();
        tokio::time::timeout(Duration::from_secs(5), proxy)
            .await
            .expect("stops")
            .expect("task");
    }
    #[tokio::test]
    async fn a_started_proxy_listens_on_loopback_and_stops_when_dropped() {
        let proxy = start(0, Arc::new(FakeGateway::default()))
            .await
            .expect("started");
        let address = proxy.address();
        assert!(address.ip().is_loopback(), "{address}");
        let mut client = tokio::net::TcpStream::connect(address)
            .await
            .expect("connect");
        client.write_all(&GREETING).await.expect("greeting");
        let mut answer = [0; 2];
        client.read_exact(&mut answer).await.expect("method");
        assert_eq!(answer, [5, NO_AUTHENTICATION]);
        drop(proxy);
        // The listener goes once the serving task sees the stop: then nothing answers.
        let refused = tokio::time::timeout(Duration::from_secs(5), async {
            while tokio::net::TcpStream::connect(address).await.is_ok() {
                tokio::task::yield_now().await;
            }
        })
        .await;
        assert!(refused.is_ok(), "still listening after the drop");
    }

    #[tokio::test]
    async fn a_taken_port_is_an_error() {
        let holder = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .expect("holder");
        let port = holder.local_addr().expect("address").port();
        let taken = start(port, Arc::new(FakeGateway::default())).await;
        assert!(taken.is_err());
    }
}
