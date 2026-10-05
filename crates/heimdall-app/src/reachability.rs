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

//! "Test address", as the C# profile dialog's: does the address answer on its port, from this
//! computer? The name is looked up, then every address it gives is dialled in turn within one
//! budget; for SSH, the server's banner is read too. Nothing is sent: no credential is ever
//! checked, and no key is asked about.

use std::net::SocketAddr;
use std::time::{Duration, Instant};

use tokio::io::AsyncReadExt as _;
use tokio::net::TcpStream;
use tokio_util::sync::CancellationToken;

/// The time a test has for the lookup, then for every address together, then for the banner,
/// as the C# five seconds.
pub const TEST_BUDGET: Duration = Duration::from_secs(5);

/// The time the tree's "Test reachability" has, as the C# two seconds.
pub const TREE_BUDGET: Duration = Duration::from_secs(2);

/// Most bytes read for an SSH banner, as the C# probe: a server's identification line comes
/// first, after at most a few lines of its own.
const BANNER_LIMIT: usize = 512;

/// What an SSH server's identification line starts with.
const BANNER_PREFIX: &str = "SSH-";

/// The address answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reached {
    /// The address that answered.
    pub address: String,
    /// How long the TCP connection took, in milliseconds.
    pub millis: u64,
    /// The SSH server's identification line, when one was asked for and given.
    pub banner: Option<String>,
}

/// Why the address did not answer, as the C# test tells it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unreached {
    /// The lookup took longer than the budget.
    DnsTimeout,
    /// The lookup failed, for this reason.
    DnsFailed(String),
    /// The lookup gave no address.
    DnsNoResults,
    /// The last address tried did not answer in time.
    TcpTimeout(String),
    /// The last address tried refused, or failed otherwise.
    TcpFailed {
        /// The address.
        address: String,
        /// Why, in the system's words.
        detail: String,
        /// Why, as the system classes it.
        kind: std::io::ErrorKind,
    },
    /// The user stopped the test.
    Cancelled,
}

/// One server the background check dials.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Probe {
    /// The profile.
    pub id: heimdall_core::profile::ProfileId,
    /// Its host.
    pub host: String,
    /// Its port.
    pub port: u16,
}

/// What the background check last found of a server, as the C# health state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// Being checked for the first time.
    Checking,
    /// It answered, in this many milliseconds.
    Up(u64),
    /// It did not answer.
    Down(DownReason),
    /// It is not checked from here.
    Unchecked(Unchecked),
}

/// Why a server did not answer the background check, as the C# reasons.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DownReason {
    /// Not in time.
    Timeout,
    /// The connection was refused.
    Refused,
    /// No route to it.
    Unreachable,
    /// Its name was not found.
    Dns,
    /// Otherwise, in the system's words.
    Other(String),
}

/// Why a server is not checked from here, as the C# reasons.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unchecked {
    /// It is reached through a gateway: whether this computer reaches it says nothing.
    BehindGateway,
    /// Its protocol has no address to dial: a local shell.
    NoPort,
    /// No host is filled in.
    NoHost,
}

/// Whether `host` answers on `port` within `timeout`: the background check of one server.
pub async fn check(host: String, port: u16, timeout: Duration) -> Verdict {
    match test_within(host, port, false, timeout, CancellationToken::new()).await {
        // Under a millisecond is still an answer that took time, as the C# says it.
        Ok(reached) => Verdict::Up(reached.millis.max(1)),
        Err(Unreached::DnsTimeout | Unreached::DnsFailed(_) | Unreached::DnsNoResults) => {
            Verdict::Down(DownReason::Dns)
        }
        Err(Unreached::TcpTimeout(_) | Unreached::Cancelled) => Verdict::Down(DownReason::Timeout),
        Err(Unreached::TcpFailed { detail, kind, .. }) => Verdict::Down(match kind {
            std::io::ErrorKind::ConnectionRefused => DownReason::Refused,
            std::io::ErrorKind::HostUnreachable | std::io::ErrorKind::NetworkUnreachable => {
                DownReason::Unreachable
            }
            std::io::ErrorKind::TimedOut => DownReason::Timeout,
            _ => DownReason::Other(crate::server_text(&detail)),
        }),
    }
}

/// Tests whether `host` answers on `port`; with `ssh`, reads the SSH server's banner too.
///
/// # Errors
///
/// Why it did not answer, or [`Unreached::Cancelled`] once `cancel` fires.
pub async fn test(
    host: String,
    port: u16,
    ssh: bool,
    cancel: CancellationToken,
) -> Result<Reached, Unreached> {
    test_within(host, port, ssh, TEST_BUDGET, cancel).await
}

/// The tree's "Test reachability": TCP only, within [`TREE_BUDGET`], as the C# one.
///
/// # Errors
///
/// Why it did not answer.
pub async fn test_from_tree(host: String, port: u16) -> Result<Reached, Unreached> {
    test_within(host, port, false, TREE_BUDGET, CancellationToken::new()).await
}

/// [`test`], with `budget` instead of [`TEST_BUDGET`].
///
/// # Errors
///
/// Why it did not answer, or [`Unreached::Cancelled`] once `cancel` fires.
pub async fn test_within(
    host: String,
    port: u16,
    ssh: bool,
    budget: Duration,
    cancel: CancellationToken,
) -> Result<Reached, Unreached> {
    tokio::select! {
        // A stop already asked for wins over an answer arriving at the same time.
        biased;
        () = cancel.cancelled() => Err(Unreached::Cancelled),
        result = run(host, port, ssh, budget) => result,
    }
}

async fn run(host: String, port: u16, ssh: bool, budget: Duration) -> Result<Reached, Unreached> {
    let addresses: Vec<SocketAddr> =
        match tokio::time::timeout(budget, tokio::net::lookup_host((host.as_str(), port))).await {
            Err(_) => return Err(Unreached::DnsTimeout),
            Ok(Err(error)) => return Err(Unreached::DnsFailed(error.to_string())),
            Ok(Ok(found)) => found.collect(),
        };
    if addresses.is_empty() {
        return Err(Unreached::DnsNoResults);
    }
    let started = Instant::now();
    let mut last = None;
    // Every address the name gives, not just the first, as the C#: a host whose IPv6
    // address comes first may answer on its IPv4 one. The budget is shared out among the
    // addresses left, so trying more never makes the user wait longer.
    for (index, address) in addresses.iter().enumerate() {
        let Some(remaining) = budget.checked_sub(started.elapsed()) else {
            break;
        };
        let left = u32::try_from(addresses.len() - index).unwrap_or(u32::MAX);
        let attempt = remaining / left;
        match tokio::time::timeout(attempt, TcpStream::connect(address)).await {
            Ok(Ok(stream)) => {
                let millis = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
                let banner = if ssh {
                    banner(stream, budget).await
                } else {
                    None
                };
                return Ok(Reached {
                    address: address.ip().to_string(),
                    millis,
                    banner,
                });
            }
            Ok(Err(error)) => {
                last = Some(Unreached::TcpFailed {
                    address: address.ip().to_string(),
                    detail: error.to_string(),
                    kind: error.kind(),
                });
            }
            Err(_) => last = Some(Unreached::TcpTimeout(address.ip().to_string())),
        }
    }
    // Only ends without a verdict when the budget ran out before an address was tried: the
    // first one timing out says the same.
    Err(last.unwrap_or_else(|| Unreached::TcpTimeout(addresses[0].ip().to_string())))
}

/// The SSH server's identification line on `stream`, read within `budget`: the first line
/// starting with "SSH-" in the first [`BANNER_LIMIT`] bytes.
async fn banner(mut stream: TcpStream, budget: Duration) -> Option<String> {
    let mut read = Vec::with_capacity(BANNER_LIMIT);
    let reading = async {
        let mut chunk = [0_u8; BANNER_LIMIT];
        while read.len() < BANNER_LIMIT {
            let count = stream.read(&mut chunk).await.ok()?;
            if count == 0 {
                break;
            }
            let room = BANNER_LIMIT - read.len();
            read.extend_from_slice(&chunk[..count.min(room)]);
            if let Some(line) = identification(&read) {
                return Some(line);
            }
        }
        None
    };
    tokio::time::timeout(budget, reading).await.ok().flatten()
}

/// The first complete line of `read` that starts with "SSH-", without its line ending.
fn identification(read: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(read);
    let complete = text.rfind('\n').map(|end| &text[..end])?;
    complete
        .lines()
        .map(|line| line.trim_end_matches('\r'))
        .find(|line| line.starts_with(BANNER_PREFIX))
        .map(crate::server_text)
}

#[cfg(test)]
mod tests {
    use tokio::io::AsyncWriteExt as _;
    use tokio::net::TcpListener;

    use super::*;

    /// A budget long enough for the loopback, short enough for a test.
    const SHORT: Duration = Duration::from_secs(2);

    #[test]
    fn the_identification_line_is_found_after_other_lines_and_only_once_complete() {
        assert_eq!(identification(b"SSH-2.0-Open"), None, "not complete");
        assert_eq!(
            identification(b"hello\r\nSSH-2.0-OpenSSH_9.6\r\n"),
            Some("SSH-2.0-OpenSSH_9.6".to_owned())
        );
        assert_eq!(identification(b"HTTP/1.1 400 Bad Request\r\n"), None);
    }

    #[tokio::test]
    async fn a_listening_port_answers_and_an_ssh_server_gives_its_banner() {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let port = listener.local_addr().expect("address").port();
        tokio::spawn(async move {
            if let Ok((mut stream, _)) = listener.accept().await {
                let _ = stream.write_all(b"SSH-2.0-Test_1.0\r\n").await;
                tokio::time::sleep(SHORT).await;
            }
        });
        let reached = run("127.0.0.1".to_owned(), port, true, SHORT)
            .await
            .expect("answered");
        assert_eq!(reached.address, "127.0.0.1");
        assert_eq!(reached.banner.as_deref(), Some("SSH-2.0-Test_1.0"));
    }

    #[tokio::test]
    async fn a_port_that_answers_without_a_banner_still_answers() {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let port = listener.local_addr().expect("address").port();
        let reached = run("127.0.0.1".to_owned(), port, false, SHORT)
            .await
            .expect("answered");
        assert_eq!(reached.banner, None, "not asked for");
        drop(listener);
    }

    #[tokio::test]
    async fn a_closed_port_does_not_answer_and_is_named_by_its_address() {
        // Reserved, not listening: nothing else can take it while the test runs.
        let socket = tokio::net::TcpSocket::new_v4().expect("socket");
        socket
            .bind("127.0.0.1:0".parse().expect("address"))
            .expect("bind");
        let port = socket.local_addr().expect("address").port();
        let failed = run("127.0.0.1".to_owned(), port, false, SHORT)
            .await
            .expect_err("closed");
        // Refused at once on Linux; Windows tries a closed port again for about two seconds
        // before it refuses, which the budget may not wait for.
        assert!(
            matches!(
                &failed,
                Unreached::TcpFailed { address, .. } | Unreached::TcpTimeout(address)
                    if address == "127.0.0.1"
            ),
            "{failed:?}"
        );
    }

    #[tokio::test]
    async fn a_name_that_does_not_resolve_fails_the_lookup() {
        let failed = run("no-such-host.invalid".to_owned(), 22, false, SHORT)
            .await
            .expect_err("no such name");
        assert!(
            matches!(failed, Unreached::DnsFailed(_) | Unreached::DnsNoResults),
            "{failed:?}"
        );
    }

    #[tokio::test]
    async fn a_test_stopped_by_the_user_says_so() {
        let cancel = CancellationToken::new();
        cancel.cancel();
        let stopped = test("127.0.0.1".to_owned(), 9, false, cancel).await;
        assert_eq!(stopped, Err(Unreached::Cancelled));
    }
}
