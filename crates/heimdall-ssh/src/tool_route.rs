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

//! The "Route via" of the network tools: their TCP connections made from an SSH gateway, as
//! the C# `ToolGatewayConnector` routes them.
//!
//! Unlike the C#, which runs `bash`, `curl`, `dig` and `ping` on the gateway and puts what
//! they need on its command line, nothing is ever run on the gateway: the only thing asked of
//! it is a `direct-tcpip` channel to the host and port the tool is pointed at (what
//! `ssh -L` asks), the host name as typed, so that the gateway resolves it. This module
//! reaches [`Connection::open_tunnel`] and nothing else of the connection, and a test reads
//! its own source to keep it so.
//!
//! The route fails closed. A channel the gateway will not open, a gateway that cannot be
//! reached or signed in to, a host key not yet trusted: each is an error for the tool to
//! say, and none is answered by connecting from this computer instead. The gateways are
//! walked by [`establish_via`] with the host keys of the sessions' `known_hosts`, so the
//! trust is exactly the sessions' trust, and a key never seen ends the attempt: a tool asks
//! no question during a scan.
//!
//! A scan opens many channels. [`ToolGateway`] carries at most [`MAX_CHANNELS_PER_GATEWAY`]
//! at once and makes the others wait for one to end, so that a wide scan cannot exhaust the
//! gateway's limit on sessions.

use std::collections::HashMap;
use std::future::Future;
use std::io;
use std::net::Ipv4Addr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use heimdall_core::profile::SshProfile;
use tokio::io::{AsyncRead, AsyncWrite, copy_bidirectional};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio_util::sync::CancellationToken;

use crate::client::establish_via;
use crate::connection::Connection;
use crate::diagnose::HopSecrets;
use crate::error::ConnectError;
use crate::options::ConnectOptions;
use crate::prompter::{
    KeyboardInteractiveQuestion, PassphraseQuestion, PasswordQuestion, Prompter, UsernameQuestion,
};
use crate::secret::Secret;
use crate::socks::Opener;

/// Channels one gateway carries at once for a tool: the C# scans through a gateway ten ports
/// at a time (`PortScanService`), and so does this.
pub const MAX_CHANNELS_PER_GATEWAY: usize = 10;

/// What is said when the socket pair of a channel was not the one made for it.
const FOREIGN_PEER: &str = "the local end of a gateway channel was taken by another program";

/// What is said when the route holds no gateway.
const EMPTY_ROUTE: &str = "the route holds no gateway";

/// What is said when the gateway did not open a channel in time.
const CHANNEL_TIMED_OUT: &str = "the gateway did not open the channel in time";

/// What is said when the channels of a gateway can no longer be counted.
const CLOSED_GATEWAY: &str = "the gateway is closed";

/// A gateway whose channels a tool uses, at most [`MAX_CHANNELS_PER_GATEWAY`] at once.
///
/// Dropping it, once the channels it carries end, disconnects the gateway.
pub struct ToolGateway<O: Opener = Connection> {
    opener: Arc<O>,
    permits: Arc<Semaphore>,
}

impl<O: Opener> std::fmt::Debug for ToolGateway<O> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToolGateway")
            .field("free", &self.permits.available_permits())
            .finish_non_exhaustive()
    }
}

impl<O: Opener> ToolGateway<O> {
    /// A route over `opener`, carrying at most `limit` channels at once.
    #[must_use]
    pub fn over(opener: O, limit: usize) -> Self {
        Self {
            opener: Arc::new(opener),
            permits: Arc::new(Semaphore::new(limit.max(1))),
        }
    }

    /// Channels that could be opened now without waiting.
    #[must_use]
    pub fn free_channels(&self) -> usize {
        self.permits.available_permits()
    }

    /// A TCP connection to `host:port` made from the gateway, `host` as typed: the gateway
    /// resolves it. Waits for a free channel first; dropped, the wait or the opening ends.
    ///
    /// The stream is a connection to this computer's own loopback, carried to the gateway's
    /// channel by a task that ends with it, so that code written for a `TcpStream` (the TLS
    /// handshake, the banner read) runs over the channel as it is.
    ///
    /// # Errors
    ///
    /// As [`ToolGateway::dial_within`], without a time limit.
    pub async fn dial(&self, host: &str, port: u16) -> io::Result<TcpStream> {
        self.dial_within(host, port, None).await
    }

    /// As [`ToolGateway::dial`], the gateway given `within` to connect onward once a channel
    /// is free: the wait for a free channel is not part of it, so that a scan wider than the
    /// limit does not time its own queue out.
    ///
    /// # Errors
    ///
    /// The gateway would not open the channel ([`io::ErrorKind::ConnectionRefused`]), took
    /// longer than `within` ([`io::ErrorKind::TimedOut`]) or could not be asked, or the local
    /// end could not be made. Never answered with a connection from this computer.
    pub async fn dial_within(
        &self,
        host: &str,
        port: u16,
        within: Option<Duration>,
    ) -> io::Result<TcpStream> {
        let permit = Arc::clone(&self.permits)
            .acquire_owned()
            .await
            .map_err(|_| io::Error::new(io::ErrorKind::NotConnected, CLOSED_GATEWAY))?;
        let opening = self.opener.open(host.to_owned(), port);
        let channel = match within {
            Some(within) => tokio::time::timeout(within, opening)
                .await
                .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, CHANNEL_TIMED_OUT))?,
            None => opening.await,
        }
        .map_err(channel_error)?;
        pair(channel, permit).await
    }
}

/// The error of a channel that could not be opened, as one connection's.
fn channel_error(error: ConnectError) -> io::Error {
    match error {
        ConnectError::JumpRefused { .. } => io::Error::new(io::ErrorKind::ConnectionRefused, error),
        ConnectError::Timeout | ConnectError::PromptTimedOut => {
            io::Error::new(io::ErrorKind::TimedOut, error)
        }
        ConnectError::Network(inner) => inner,
        other => io::Error::other(other),
    }
}

/// `channel` joined to a connection of this computer's loopback, which is returned: what is
/// written to it goes down the channel, what the channel says is read from it. `permit` is
/// held until either side ends.
async fn pair<S>(channel: S, permit: OwnedSemaphorePermit) -> io::Result<TcpStream>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await?;
    let client = TcpStream::connect(listener.local_addr()?).await?;
    let (accepted, peer) = listener.accept().await?;
    // Only the connection made here is carried: another program that got in first is let go.
    if client.local_addr()? != peer {
        return Err(io::Error::other(FOREIGN_PEER));
    }
    drop(listener);
    tokio::spawn(async move {
        let _held = permit;
        let (mut channel, mut accepted) = (channel, accepted);
        let _ = copy_bidirectional(&mut channel, &mut accepted).await;
    });
    Ok(client)
}

/// Opens the gateway at the end of `hops`, nearest first, each signed in to with its
/// `secrets` by its place on the route: the tool's channels go through it.
///
/// # Errors
///
/// As [`establish_via`]: a host key not yet trusted ends the attempt, whichever hop it is
/// of; a hop that refuses the sign-in or cannot be reached is named by the error.
pub async fn open(
    hops: &[SshProfile],
    secrets: &[HopSecrets],
    options: &ConnectOptions,
    cancel: CancellationToken,
) -> Result<ToolGateway<Connection>, ConnectError> {
    let Some((last, before)) = hops.split_last() else {
        return Err(ConnectError::Network(io::Error::new(
            io::ErrorKind::InvalidInput,
            EMPTY_ROUTE,
        )));
    };
    let prompter = Arc::new(Given::of(hops, secrets));
    let connection = establish_via(before, last, options, prompter, cancel).await?;
    Ok(ToolGateway::over(connection, MAX_CHANNELS_PER_GATEWAY))
}

/// The key a password is given for: the account of a hop at its endpoint, never an address
/// alone.
type PasswordKey = (String, u16, String);

/// Answers the sign-in questions of the gateways from what was given, each secret once, and
/// nothing else: a tool asks no person anything.
struct Given {
    passwords: Mutex<HashMap<PasswordKey, Secret>>,
    passphrases: Mutex<HashMap<PathBuf, Secret>>,
}

impl Given {
    /// What `secrets` give each of `hops`, by their places.
    fn of(hops: &[SshProfile], secrets: &[HopSecrets]) -> Self {
        let mut passwords = HashMap::new();
        let mut passphrases = HashMap::new();
        for (hop, given) in hops.iter().zip(secrets) {
            if let (Some(user), Some(password)) = (&hop.username, &given.password) {
                passwords.insert((hop.host.clone(), hop.port, user.clone()), password.clone());
            }
            if let (Some(path), Some(passphrase)) = (&hop.key_path, &given.passphrase) {
                passphrases.insert(path.clone(), passphrase.clone());
            }
        }
        Self {
            passwords: Mutex::new(passwords),
            passphrases: Mutex::new(passphrases),
        }
    }
}

impl Prompter for Given {
    fn username(&self, _question: UsernameQuestion) -> impl Future<Output = Option<String>> + Send {
        std::future::ready(None)
    }

    fn password(&self, question: PasswordQuestion) -> impl Future<Output = Option<Secret>> + Send {
        let key = (question.host, question.port, question.username);
        std::future::ready(
            self.passwords
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .remove(&key),
        )
    }

    fn key_passphrase(
        &self,
        question: PassphraseQuestion,
    ) -> impl Future<Output = Option<Secret>> + Send {
        std::future::ready(
            self.passphrases
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .remove(&question.key_path),
        )
    }

    fn keyboard_interactive(
        &self,
        _question: KeyboardInteractiveQuestion,
    ) -> impl Future<Output = Option<Vec<Secret>>> + Send {
        std::future::ready(None)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    use heimdall_core::profile::{ProfileId, SshGateway};
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _, DuplexStream, duplex};

    use super::*;

    /// A gateway that opens channels onto an echo, or refuses, and counts.
    struct Fake {
        refuse: bool,
        opened: AtomicUsize,
        live: Arc<AtomicUsize>,
        most: Arc<AtomicUsize>,
        asked: Mutex<Vec<(String, u16)>>,
    }

    impl Fake {
        fn new(refuse: bool) -> Self {
            Self {
                refuse,
                opened: AtomicUsize::new(0),
                live: Arc::new(AtomicUsize::new(0)),
                most: Arc::new(AtomicUsize::new(0)),
                asked: Mutex::new(Vec::new()),
            }
        }
    }

    impl Opener for Arc<Fake> {
        type Stream = DuplexStream;

        async fn open(&self, host: String, port: u16) -> Result<DuplexStream, ConnectError> {
            // A real gateway answers after a round trip.
            tokio::task::yield_now().await;
            self.asked.lock().expect("lock").push((host.clone(), port));
            if self.refuse {
                return Err(ConnectError::JumpRefused { host, port });
            }
            self.opened.fetch_add(1, Ordering::SeqCst);
            let now = self.live.fetch_add(1, Ordering::SeqCst) + 1;
            self.most.fetch_max(now, Ordering::SeqCst);
            let (ours, mut theirs) = duplex(1024);
            let live = Arc::clone(&self.live);
            tokio::spawn(async move {
                let mut buffer = [0_u8; 64];
                while let Ok(count) = theirs.read(&mut buffer).await {
                    if count == 0 || theirs.write_all(&buffer[..count]).await.is_err() {
                        break;
                    }
                }
                live.fetch_sub(1, Ordering::SeqCst);
            });
            Ok(ours)
        }
    }

    #[tokio::test]
    async fn a_channel_carries_bytes_both_ways_and_the_name_is_sent_as_typed() {
        let fake = Arc::new(Fake::new(false));
        let gateway = ToolGateway::over(Arc::clone(&fake), 4);
        let mut stream = gateway.dial("db.internal", 5432).await.expect("channel");
        stream.write_all(b"ping").await.expect("write");
        let mut echoed = [0_u8; 4];
        stream.read_exact(&mut echoed).await.expect("read");
        assert_eq!(&echoed, b"ping");
        assert_eq!(
            *fake.asked.lock().expect("lock"),
            vec![("db.internal".to_owned(), 5432)],
            "the gateway resolves the name: it is not looked up here"
        );
    }

    #[tokio::test]
    async fn a_channel_the_gateway_refuses_is_an_error_and_nothing_connects_directly() {
        // A listener that would answer a direct connection: it must stay untouched.
        let bait = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .expect("bind");
        let port = bait.local_addr().expect("address").port();
        let fake = Arc::new(Fake::new(true));
        let gateway = ToolGateway::over(Arc::clone(&fake), 4);
        let error = gateway.dial("127.0.0.1", port).await.expect_err("refused");
        assert_eq!(error.kind(), io::ErrorKind::ConnectionRefused);
        let direct = tokio::time::timeout(Duration::from_millis(200), bait.accept()).await;
        assert!(direct.is_err(), "no direct connection was made");
        assert_eq!(gateway.free_channels(), 4, "the refused try holds nothing");
    }

    #[tokio::test]
    async fn no_more_than_the_limit_of_channels_are_open_at_once() {
        let fake = Arc::new(Fake::new(false));
        let gateway = Arc::new(ToolGateway::over(Arc::clone(&fake), 3));
        let mut tasks = Vec::new();
        for _ in 0..12 {
            let gateway = Arc::clone(&gateway);
            tasks.push(tokio::spawn(async move {
                let mut stream = gateway.dial("host", 80).await.expect("channel");
                stream.write_all(b"x").await.expect("write");
                let mut one = [0_u8; 1];
                stream.read_exact(&mut one).await.expect("read");
                tokio::time::sleep(Duration::from_millis(20)).await;
            }));
        }
        for task in tasks {
            task.await.expect("task");
        }
        assert_eq!(fake.opened.load(Ordering::SeqCst), 12, "all were served");
        assert!(
            fake.most.load(Ordering::SeqCst) <= 3,
            "three at once at most, saw {}",
            fake.most.load(Ordering::SeqCst)
        );
    }

    #[tokio::test]
    async fn a_channel_ending_frees_its_place() {
        let fake = Arc::new(Fake::new(false));
        let gateway = ToolGateway::over(Arc::clone(&fake), 1);
        let stream = gateway.dial("host", 80).await.expect("channel");
        assert_eq!(gateway.free_channels(), 0);
        drop(stream);
        let freed = tokio::time::timeout(Duration::from_secs(5), gateway.dial("host", 81)).await;
        assert!(freed.is_ok(), "the next channel was served");
    }

    #[tokio::test]
    async fn the_wait_for_a_free_channel_is_not_part_of_the_time_given_to_the_gateway() {
        let fake = Arc::new(Fake::new(false));
        let gateway = Arc::new(ToolGateway::over(Arc::clone(&fake), 1));
        let held = gateway.dial("host", 80).await.expect("channel");
        let waiting = {
            let gateway = Arc::clone(&gateway);
            tokio::spawn(async move {
                gateway
                    .dial_within("host", 81, Some(Duration::from_millis(100)))
                    .await
            })
        };
        // Longer than the time given, spent waiting for the place.
        tokio::time::sleep(Duration::from_millis(300)).await;
        drop(held);
        let served = waiting.await.expect("task");
        assert!(served.is_ok(), "served once a place was free: {served:?}");
    }

    #[tokio::test]
    async fn an_empty_route_is_an_error() {
        let options = ConnectOptions::new(PathBuf::from("known_hosts"));
        let error = open(&[], &[], &options, CancellationToken::new())
            .await
            .expect_err("no gateway");
        assert!(matches!(error, ConnectError::Network(_)));
    }

    #[test]
    fn the_route_only_opens_direct_tcpip_channels() {
        let source = include_str!("tool_route.rs");
        let production = source
            .split("#[cfg(test)]")
            .next()
            .expect("the code before the tests");
        // Nothing that runs a program, a shell or a subsystem on the gateway.
        for forbidden in [
            ["run_", "command"].concat(),
            ["open_", "shell"].concat(),
            ["open_", "subsystem"].concat(),
            ["exec", "("].concat(),
            ["forward_", "remote"].concat(),
        ] {
            assert!(
                !production.contains(&forbidden),
                "the gateway route uses {forbidden}"
            );
        }
        assert!(production.contains(".open(host.to_owned(), port)"));
    }

    #[tokio::test]
    async fn only_the_gateways_own_secrets_are_given_to_its_endpoint() {
        let hop = SshGateway {
            id: ProfileId::new("g"),
            name: "bastion".to_owned(),
            host: "bastion".to_owned(),
            port: 22,
            username: Some("ops".to_owned()),
            key_path: None,
            parent: None,
        }
        .as_hop();
        let given = Given::of(
            std::slice::from_ref(&hop),
            &[HopSecrets {
                password: Some(Secret::new("pw".to_owned())),
                passphrase: None,
            }],
        );
        let ask = |host: &str, user: &str| PasswordQuestion {
            host: host.to_owned(),
            port: 22,
            username: user.to_owned(),
            attempt: 1,
        };
        assert!(given.password(ask("other", "ops")).await.is_none());
        assert!(given.password(ask("bastion", "root")).await.is_none());
        assert!(given.password(ask("bastion", "ops")).await.is_some());
        assert!(
            given.password(ask("bastion", "ops")).await.is_none(),
            "once only: a refused password is not sent again"
        );
        assert!(
            given
                .username(UsernameQuestion {
                    host: "b".to_owned(),
                    port: 22
                })
                .await
                .is_none()
        );
    }
}
