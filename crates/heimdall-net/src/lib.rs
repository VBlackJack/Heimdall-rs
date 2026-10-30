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

//! The TCP connection every protocol opens to a host by name, as RFC 8305 ("Happy Eyeballs
//! Version 2") has it: every address the name gives is tried, the families interleaved and
//! the family of the resolver's first address first (section 4); the next attempt starts as
//! soon as the one before it fails, or once [`CONNECTION_ATTEMPT_DELAY`] went by without an
//! answer, while the earlier ones go on; the first connection made wins, and the attempts
//! still running are dropped, which closes their sockets.
//!
//! Two attempts that complete together, in the same wake-up, both made a connection: the
//! first in dialling order wins and the other is closed at once, a TCP handshake for nothing
//! that the server sees as a connection opened and closed. Rare, and harmless.
//!
//! Dialled in turn, a name whose first address never answers costs the whole wait for it
//! before the next one is tried: `localhost` gives `::1` before `127.0.0.1`, and Windows tries
//! a closed port again for about two seconds before it reports the refusal, so a server
//! listening on IPv4 alone was reached a second or more late.
//!
//! The name is looked up once, by the system's resolver, which sorts the addresses as RFC
//! 6724 says: that order decides the family dialled first, and the order within each family
//! is kept. No time limit is set here unless asked:
//! each caller keeps its own, and its own reading of a failure.

use std::collections::VecDeque;
use std::fmt;
use std::future::{Future, poll_fn};
use std::io;
use std::net::SocketAddr;
use std::pin::{Pin, pin};
use std::task::Poll;
use std::time::Duration;

use tokio::net::{TcpSocket, TcpStream};
use tokio::time::Instant;

/// How long an attempt runs alone before the next one starts beside it, as RFC 8305 section
/// 5 recommends.
pub const CONNECTION_ATTEMPT_DELAY: Duration = Duration::from_millis(250);

/// The error of a name that gave no address, as tokio words it.
/// Idle time before the first TCP keepalive probe, as the C# `KeepAliveTimeSeconds`.
pub const KEEPALIVE_TIME: Duration = Duration::from_secs(60);

/// Delay between two unanswered keepalive probes, as the C# `KeepAliveIntervalSeconds`.
pub const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(10);

const NO_ADDRESS: &str = "could not resolve to any address";

/// The error of attempts that did not end in time.
const TIMED_OUT: &str = "no address answered in time";

/// A connection made.
#[derive(Debug)]
pub struct Connected {
    /// The connection.
    pub stream: TcpStream,
    /// The address that answered.
    pub address: SocketAddr,
}

/// One attempt that failed.
#[derive(Debug)]
pub struct Attempt {
    /// The address dialled.
    pub address: SocketAddr,
    /// Why it failed, in the system's words.
    pub error: io::Error,
}

/// Why no connection was made.
#[derive(Debug)]
pub enum Failure {
    /// There was no address to dial.
    NoAddress,
    /// The time ran out with attempts still unanswered.
    TimedOut {
        /// An address whose attempt was still unanswered: of those, the one started last.
        address: SocketAddr,
        /// The attempts that failed before.
        failed: Vec<Attempt>,
    },
    /// Every attempt failed.
    Failed {
        /// The address whose failure says the most: see [`Failure::Failed::error`].
        address: SocketAddr,
        /// The last failure that is not the family being unusable from this computer (no
        /// route, no address of that family here), else the last failure: a refusal says
        /// more of the server than the absence of IPv6 routing says of the network.
        error: io::Error,
        /// The other failures, in the order they came.
        others: Vec<Attempt>,
    },
}

impl Failure {
    /// The failure an address gave that says the most of the server: for
    /// [`Failure::Failed`], its error; for [`Failure::TimedOut`], the last failure that is not
    /// the family being unusable from this computer, if any. A caller that tells the user why
    /// takes it over the time running out: an IPv6 address that never answers and an IPv4 one
    /// that refuses is a refusal, as dialling them in turn reported it.
    #[must_use]
    pub fn telling_failure(&self) -> Option<(SocketAddr, &io::Error)> {
        match self {
            Self::NoAddress => None,
            Self::Failed { address, error, .. } => Some((*address, error)),
            Self::TimedOut { failed, .. } => failed
                .iter()
                .rev()
                .find(|attempt| !family_unusable(&attempt.error))
                .map(|attempt| (attempt.address, &attempt.error)),
        }
    }
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoAddress => f.write_str(NO_ADDRESS),
            Self::TimedOut { address, .. } => write!(f, "{address}: {TIMED_OUT}"),
            Self::Failed { address, error, .. } => write!(f, "{address}: {error}"),
        }
    }
}

impl std::error::Error for Failure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Failed { error, .. } => Some(error),
            Self::NoAddress | Self::TimedOut { .. } => None,
        }
    }
}

impl From<Failure> for io::Error {
    /// The failure as one connection's error: the one that says the most, unchanged, so a
    /// caller reads its kind and words as it read a single connection's.
    fn from(failure: Failure) -> Self {
        match failure {
            Failure::NoAddress => io::Error::new(io::ErrorKind::InvalidInput, NO_ADDRESS),
            Failure::TimedOut { .. } => io::Error::new(io::ErrorKind::TimedOut, TIMED_OUT),
            Failure::Failed { error, .. } => error,
        }
    }
}

/// Looks `host` up and connects to `port` on the addresses it gives, as the module says. An
/// address literal is not looked up. No time limit: the caller sets its own.
///
/// # Errors
///
/// The lookup's error, or the failure of the attempts as [`From<Failure>`] gives it.
pub async fn connect(host: &str, port: u16) -> io::Result<TcpStream> {
    let addresses: Vec<SocketAddr> = tokio::net::lookup_host((host, port)).await?.collect();
    connect_addresses(&addresses, None)
        .await
        .map(|connected| connected.stream)
        .map_err(io::Error::from)
}

/// [`connect`], each socket asking the system for TCP keepalive before it dials, so that a
/// session idle on a path that died silently (a NAT or a firewall that dropped its state)
/// reads an error instead of waiting for ever. The timing is the C# Telnet session's
/// (`TelnetSession.cs:54-57`, `ApplyKeepAlive`): [`KEEPALIVE_TIME`] idle before the first
/// probe and [`KEEPALIVE_INTERVAL`] between two unanswered ones. On a system that cannot set
/// the interval, only the idle time is set and the system's own interval applies.
///
/// # Errors
///
/// As [`connect`]; also a socket that could not be made or set.
pub async fn connect_kept_alive(host: &str, port: u16) -> io::Result<TcpStream> {
    let addresses: Vec<SocketAddr> = tokio::net::lookup_host((host, port)).await?.collect();
    race(&addresses, CONNECTION_ATTEMPT_DELAY, None, |address| {
        Box::pin(dial_kept_alive(address))
    })
    .await
    .map(|(stream, _)| stream)
    .map_err(io::Error::from)
}

/// One attempt at `address` from a socket with TCP keepalive on.
async fn dial_kept_alive(address: SocketAddr) -> io::Result<TcpStream> {
    let socket = if address.is_ipv6() {
        TcpSocket::new_v6()?
    } else {
        TcpSocket::new_v4()?
    };
    socket2::SockRef::from(&socket).set_tcp_keepalive(&keepalive_parameters())?;
    socket.connect(address).await
}

/// The keepalive to set: on, with the timing above wherever the system takes it.
fn keepalive_parameters() -> socket2::TcpKeepalive {
    let parameters = socket2::TcpKeepalive::new().with_time(KEEPALIVE_TIME);
    #[cfg(any(
        windows,
        target_os = "linux",
        target_os = "android",
        target_os = "macos",
        target_os = "ios",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "dragonfly",
        target_os = "illumos"
    ))]
    let parameters = parameters.with_interval(KEEPALIVE_INTERVAL);
    parameters
}

/// Connects to the first of `addresses` that answers, as the module says, within `timeout`
/// when one is given.
///
/// # Errors
///
/// [`Failure`].
pub async fn connect_addresses(
    addresses: &[SocketAddr],
    timeout: Option<Duration>,
) -> Result<Connected, Failure> {
    race(addresses, CONNECTION_ATTEMPT_DELAY, timeout, |address| {
        Box::pin(TcpStream::connect(address))
    })
    .await
    .map(|(stream, address)| Connected { stream, address })
}

/// One attempt under way.
type Attempting<S> = Pin<Box<dyn Future<Output = io::Result<S>> + Send>>;

/// What happened while the attempts ran.
enum Event<S> {
    /// The attempt at this index of those running ended.
    Ended(usize, io::Result<S>),
    /// The next attempt is due.
    Due,
    /// The time ran out.
    Expired,
}

/// The race itself, over any kind of attempt: `attempt` starts one, `delay` is the head start
/// of each over the next.
async fn race<S, F>(
    addresses: &[SocketAddr],
    delay: Duration,
    timeout: Option<Duration>,
    mut attempt: F,
) -> Result<(S, SocketAddr), Failure>
where
    F: FnMut(SocketAddr) -> Attempting<S>,
{
    let mut queue = interleave(addresses).into_iter();
    let Some(first) = queue.next() else {
        return Err(Failure::NoAddress);
    };
    let mut running: Vec<(SocketAddr, Attempting<S>)> = vec![(first, attempt(first))];
    let mut failed: Vec<Attempt> = Vec::new();
    let mut next_due = pin!(tokio::time::sleep(delay));
    let mut expiry = pin!(async move {
        match timeout {
            Some(timeout) => tokio::time::sleep(timeout).await,
            None => std::future::pending::<()>().await,
        }
    });
    loop {
        let more = !queue.as_slice().is_empty();
        let event = poll_fn(|context| {
            // An answer that came in with the end of the time still counts.
            for (index, (_, attempting)) in running.iter_mut().enumerate() {
                if let Poll::Ready(result) = attempting.as_mut().poll(context) {
                    return Poll::Ready(Event::Ended(index, result));
                }
            }
            if expiry.as_mut().poll(context).is_ready() {
                return Poll::Ready(Event::Expired);
            }
            if more && next_due.as_mut().poll(context).is_ready() {
                return Poll::Ready(Event::Due);
            }
            Poll::Pending
        })
        .await;
        match event {
            // Returning drops the attempts still running.
            Event::Ended(index, Ok(stream)) => return Ok((stream, running[index].0)),
            Event::Ended(index, Err(error)) => {
                let (address, _) = running.remove(index);
                log::debug!("connecting to {address} failed: {error}");
                failed.push(Attempt { address, error });
                if !more && running.is_empty() {
                    return Err(verdict(failed));
                }
            }
            Event::Expired => {
                // An attempt is always running here: the last failure of all returns above.
                return Err(match running.last() {
                    Some((address, _)) => Failure::TimedOut {
                        address: *address,
                        failed,
                    },
                    None => verdict(failed),
                });
            }
            Event::Due => {}
        }
        // Due, or the one before failed: the next one starts now, with its own head start.
        if let Some(address) = queue.next() {
            running.push((address, attempt(address)));
            next_due.as_mut().reset(Instant::now() + delay);
        }
    }
}

/// `addresses` in the order they are dialled: the families alternate, starting with the family
/// of the first address, as the resolver sorted them (RFC 6724, kept by RFC 8305 section 4),
/// each family in the order the resolver gave; an address given twice is dialled once.
fn interleave(addresses: &[SocketAddr]) -> Vec<SocketAddr> {
    let mut six = VecDeque::new();
    let mut four = VecDeque::new();
    for address in addresses {
        let family = if address.is_ipv6() {
            &mut six
        } else {
            &mut four
        };
        if !family.contains(address) {
            family.push_back(*address);
        }
    }
    let mut ordered = Vec::with_capacity(six.len() + four.len());
    let mut six_next = addresses.first().is_some_and(SocketAddr::is_ipv6);
    loop {
        let next = if six_next {
            six.pop_front().or_else(|| four.pop_front())
        } else {
            four.pop_front().or_else(|| six.pop_front())
        };
        let Some(next) = next else {
            return ordered;
        };
        ordered.push(next);
        six_next = !six_next;
    }
}

/// The failure of attempts that all failed, `failed` in the order they came.
fn verdict(mut failed: Vec<Attempt>) -> Failure {
    let chosen = failed
        .iter()
        .rposition(|attempt| !family_unusable(&attempt.error))
        .or_else(|| failed.len().checked_sub(1));
    let Some(chosen) = chosen else {
        return Failure::NoAddress;
    };
    let Attempt { address, error } = failed.remove(chosen);
    Failure::Failed {
        address,
        error,
        others: failed,
    }
}

/// Whether `error` says that the address's family cannot be used from this computer, rather
/// than anything of the server.
fn family_unusable(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::NetworkUnreachable | io::ErrorKind::AddrNotAvailable
    )
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Instant as StdInstant;

    use tokio::net::TcpListener;

    use super::*;

    /// Room for a loaded machine above the delays the tests measure.
    const SLACK: Duration = Duration::from_millis(700);

    /// A time limit for the tests that need one.
    const LIMIT: Duration = Duration::from_millis(400);

    /// A head start short enough for every attempt of a test to start early.
    const HEAD_START: Duration = Duration::from_millis(10);

    /// How long the winning attempt of a test takes: after every other one started.
    const WINNER_TIME: Duration = Duration::from_millis(50);

    /// A port that is never dialled for real.
    const PORT: u16 = 7;

    fn v4(last: u8) -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 0, 2, last)), PORT)
    }

    fn v6(last: u16) -> SocketAddr {
        SocketAddr::new(
            IpAddr::V6(Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, last)),
            PORT,
        )
    }

    /// An attempt that never ends, as one to an address that drops every packet.
    fn blackholed<S: Send + 'static>() -> Attempting<S> {
        Box::pin(std::future::pending())
    }

    fn refused<S: Send + 'static>(kind: io::ErrorKind) -> Attempting<S> {
        Box::pin(async move { Err(io::Error::from(kind)) })
    }

    fn answered(address: SocketAddr) -> Attempting<SocketAddr> {
        Box::pin(async move { Ok(address) })
    }

    #[test]
    fn the_families_alternate_from_the_resolvers_first_each_in_the_resolver_order() {
        assert_eq!(
            interleave(&[v6(1), v6(2), v4(1), v6(3), v4(2), v6(1)]),
            vec![v6(1), v4(1), v6(2), v4(2), v6(3)],
            "IPv6 first as the resolver put it first, the duplicate dialled once"
        );
        assert_eq!(
            interleave(&[v4(1), v4(2), v6(1), v4(3), v6(2), v4(1)]),
            vec![v4(1), v6(1), v4(2), v6(2), v4(3)],
            "IPv4 first as the resolver put it first, the duplicate dialled once"
        );
        assert_eq!(
            interleave(&[v4(2), v4(1)]),
            vec![v4(2), v4(1)],
            "IPv4 alone"
        );
        assert_eq!(
            interleave(&[v6(1), v6(2)]),
            vec![v6(1), v6(2)],
            "IPv6 alone"
        );
        assert!(interleave(&[]).is_empty());
    }

    #[tokio::test]
    async fn an_unanswered_ipv6_address_delays_the_ipv4_one_by_the_attempt_delay_only() {
        let started = StdInstant::now();
        let (winner, _) = race(&[v6(1), v4(1)], CONNECTION_ATTEMPT_DELAY, None, |address| {
            if address.is_ipv6() {
                blackholed()
            } else {
                answered(address)
            }
        })
        .await
        .expect("connected");
        let elapsed = started.elapsed();
        assert_eq!(winner, v4(1));
        assert!(elapsed >= CONNECTION_ATTEMPT_DELAY, "{elapsed:?}");
        assert!(elapsed < CONNECTION_ATTEMPT_DELAY + SLACK, "{elapsed:?}");
    }

    #[tokio::test]
    async fn a_refused_attempt_starts_the_next_at_once() {
        let started = StdInstant::now();
        let (winner, _) = race(&[v6(1), v4(1)], CONNECTION_ATTEMPT_DELAY, None, |address| {
            if address.is_ipv6() {
                refused(io::ErrorKind::ConnectionRefused)
            } else {
                answered(address)
            }
        })
        .await
        .expect("connected");
        assert_eq!(winner, v4(1));
        assert!(
            started.elapsed() < CONNECTION_ATTEMPT_DELAY,
            "no head start waited"
        );
    }

    #[tokio::test]
    async fn the_attempts_still_running_are_dropped_once_one_wins() {
        struct Guard(Arc<AtomicUsize>);
        impl Drop for Guard {
            fn drop(&mut self) {
                self.0.fetch_add(1, Ordering::SeqCst);
            }
        }
        let dropped = Arc::new(AtomicUsize::new(0));
        let counter = dropped.clone();
        let (winner, _) = race(
            &[v6(1), v4(1), v6(2)],
            HEAD_START,
            None,
            move |address| -> Attempting<SocketAddr> {
                if address.is_ipv6() {
                    let guard = Guard(counter.clone());
                    return Box::pin(async move {
                        let _guard = guard;
                        std::future::pending().await
                    });
                }
                Box::pin(async move {
                    tokio::time::sleep(WINNER_TIME).await;
                    Ok(address)
                })
            },
        )
        .await
        .expect("connected");
        assert_eq!(winner, v4(1));
        assert_eq!(
            dropped.load(Ordering::SeqCst),
            2,
            "both IPv6 attempts dropped with the race"
        );
    }

    #[tokio::test]
    async fn every_failure_is_kept_and_a_refusal_says_more_than_no_route() {
        let failure = race::<SocketAddr, _>(
            &[v6(1), v4(1), v6(2)],
            CONNECTION_ATTEMPT_DELAY,
            None,
            |address| {
                if address == v4(1) {
                    refused(io::ErrorKind::ConnectionRefused)
                } else {
                    refused(io::ErrorKind::NetworkUnreachable)
                }
            },
        )
        .await
        .expect_err("all failed");
        let Failure::Failed {
            address,
            error,
            others,
        } = failure
        else {
            panic!("{failure:?}");
        };
        assert_eq!(address, v4(1));
        assert_eq!(error.kind(), io::ErrorKind::ConnectionRefused);
        let others: Vec<_> = others.iter().map(|attempt| attempt.address).collect();
        assert_eq!(others, vec![v6(1), v6(2)]);
    }

    #[tokio::test]
    async fn the_last_failure_is_told_when_none_says_more() {
        let failure =
            race::<SocketAddr, _>(&[v6(1), v4(1)], CONNECTION_ATTEMPT_DELAY, None, |address| {
                if address.is_ipv6() {
                    refused(io::ErrorKind::ConnectionRefused)
                } else {
                    refused(io::ErrorKind::ConnectionReset)
                }
            })
            .await
            .expect_err("all failed");
        let error = io::Error::from(failure);
        assert_eq!(
            error.kind(),
            io::ErrorKind::ConnectionReset,
            "as one connection's"
        );
    }

    #[tokio::test]
    async fn the_time_limit_ends_the_race_and_names_the_last_attempt_still_running() {
        let started = StdInstant::now();
        let failure = race::<SocketAddr, _>(
            &[v6(1), v4(1)],
            CONNECTION_ATTEMPT_DELAY,
            Some(LIMIT),
            |_| blackholed(),
        )
        .await
        .expect_err("nothing answers");
        let elapsed = started.elapsed();
        assert!(
            matches!(failure, Failure::TimedOut { address, ref failed } if address == v4(1) && failed.is_empty()),
            "{failure:?}"
        );
        assert!(elapsed >= LIMIT, "{elapsed:?}");
        assert!(elapsed < LIMIT + SLACK, "{elapsed:?}");
        assert_eq!(
            io::Error::from(failure).kind(),
            io::ErrorKind::TimedOut,
            "as one connection's"
        );
    }

    #[tokio::test]
    async fn a_refusal_before_the_time_limit_is_told_over_the_time_running_out() {
        let failure = race::<SocketAddr, _>(
            &[v6(1), v4(1)],
            CONNECTION_ATTEMPT_DELAY,
            Some(LIMIT),
            |address| {
                if address.is_ipv6() {
                    blackholed()
                } else {
                    refused(io::ErrorKind::ConnectionRefused)
                }
            },
        )
        .await
        .expect_err("nothing answers");
        let Failure::TimedOut {
            address,
            ref failed,
        } = failure
        else {
            panic!("{failure:?}");
        };
        assert_eq!(
            address,
            v6(1),
            "the attempt still unanswered, not the last started"
        );
        assert_eq!(failed.len(), 1);
        let (told, error) = failure.telling_failure().expect("a refusal");
        assert_eq!(told, v4(1));
        assert_eq!(error.kind(), io::ErrorKind::ConnectionRefused);
    }

    #[test]
    fn no_route_is_not_told_over_the_time_running_out() {
        let failure = Failure::TimedOut {
            address: v4(1),
            failed: vec![Attempt {
                address: v6(1),
                error: io::Error::from(io::ErrorKind::NetworkUnreachable),
            }],
        };
        assert!(failure.telling_failure().is_none());
        assert!(Failure::NoAddress.telling_failure().is_none());
    }

    #[tokio::test]
    async fn no_address_is_a_failure_of_its_own() {
        let failure = connect_addresses(&[], Some(LIMIT))
            .await
            .expect_err("nothing to dial");
        assert!(matches!(failure, Failure::NoAddress), "{failure:?}");
        assert_eq!(
            io::Error::from(failure).kind(),
            io::ErrorKind::InvalidInput,
            "as tokio's lookup of no address"
        );
    }

    #[tokio::test]
    async fn a_server_on_ipv4_alone_is_reached_though_ipv6_comes_first() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .expect("bind");
        let port = listener.local_addr().expect("address").port();
        let loopback6 = SocketAddr::new(IpAddr::V6(Ipv6Addr::LOCALHOST), port);
        let loopback4 = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);
        let started = StdInstant::now();
        let connected = connect_addresses(&[loopback6, loopback4], Some(LIMIT + SLACK))
            .await
            .expect("connected");
        assert_eq!(connected.address, loopback4);
        // Refused at once on ::1, or never answered there: at most the attempt delay late.
        assert!(
            started.elapsed() < CONNECTION_ATTEMPT_DELAY + SLACK,
            "{:?}",
            started.elapsed()
        );
    }

    #[tokio::test]
    async fn the_resolvers_first_family_wins_when_the_server_listens_on_both() {
        let Ok(six) = TcpListener::bind((Ipv6Addr::LOCALHOST, 0)).await else {
            // No IPv6 loopback on this machine: nothing to prefer.
            return;
        };
        let port = six.local_addr().expect("address").port();
        let Ok(_four) = TcpListener::bind((Ipv4Addr::LOCALHOST, port)).await else {
            // The same port taken on IPv4 by something else: not this test's subject.
            return;
        };
        let loopback6 = SocketAddr::new(IpAddr::V6(Ipv6Addr::LOCALHOST), port);
        let loopback4 = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);
        for (given, first) in [
            ([loopback6, loopback4], loopback6),
            ([loopback4, loopback6], loopback4),
        ] {
            let connected = connect_addresses(&given, Some(LIMIT + SLACK))
                .await
                .expect("connected");
            assert_eq!(connected.address, first, "the family given first");
        }
    }

    #[tokio::test]
    async fn a_name_is_looked_up_then_connected_to() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .expect("bind");
        let port = listener.local_addr().expect("address").port();
        let stream = tokio::time::timeout(LIMIT + SLACK, connect("localhost", port))
            .await
            .expect("in time")
            .expect("connected");
        assert_eq!(
            stream.peer_addr().expect("peer"),
            SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port)
        );
    }

    /// Whether the system sends keepalive probes on `stream`.
    fn keepalive(stream: TcpStream) -> bool {
        let socket = TcpSocket::from_std_stream(stream.into_std().expect("std"));
        socket.keepalive().expect("read back")
    }

    #[tokio::test]
    async fn a_connection_kept_alive_asks_the_system_for_probes_and_a_plain_one_does_not() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .expect("bind");
        let port = listener.local_addr().expect("address").port();
        let kept = tokio::time::timeout(LIMIT + SLACK, connect_kept_alive("localhost", port))
            .await
            .expect("in time")
            .expect("connected");
        assert_eq!(
            kept.peer_addr().expect("peer"),
            SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port)
        );
        // Windows has no getter for the timing; the setter is exercised all the same.
        #[cfg(any(target_os = "linux", target_os = "android", target_os = "macos"))]
        {
            let socket = TcpSocket::from_std_stream(kept.into_std().expect("std"));
            let reference = socket2::SockRef::from(&socket);
            assert!(reference.keepalive().expect("read back"));
            assert_eq!(
                reference.tcp_keepalive_time().expect("time"),
                KEEPALIVE_TIME
            );
            assert_eq!(
                reference.tcp_keepalive_interval().expect("interval"),
                KEEPALIVE_INTERVAL
            );
        }
        #[cfg(not(any(target_os = "linux", target_os = "android", target_os = "macos")))]
        assert!(keepalive(kept));
        let plain = tokio::time::timeout(LIMIT + SLACK, connect("localhost", port))
            .await
            .expect("in time")
            .expect("connected");
        assert!(!keepalive(plain), "the system's default, left alone");
    }
}
