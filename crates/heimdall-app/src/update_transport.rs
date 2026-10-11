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

//! The update check's TCP connections, to GitHub or to the proxy, made as every other one is
//! (`heimdall_net`): ureq's own connector dials the addresses in turn, each with a share of the
//! time. The rest of ureq's chain is kept: a CONNECT proxy first, TLS on top.
//!
//! ureq is blocking and runs on a blocking thread: each connection races on a runtime of its
//! own, then the winner is handed back as a blocking socket.

use std::fmt;
use std::io::{self, Read as _, Write as _};
use std::net::TcpStream;
use std::time::Duration;

use ureq::unversioned::resolver::DefaultResolver;
use ureq::unversioned::transport::{
    Buffers, ConnectProxyConnector, ConnectionDetails, Connector, Either, LazyBuffers, NextTimeout,
    RustlsConnector, Transport,
};

/// An agent with `config`, its TCP connections raced.
pub(crate) fn agent(config: ureq::config::Config) -> ureq::Agent {
    let connector =
        ().chain(ConnectProxyConnector::default())
            .chain(RacedTcpConnector)
            .chain(RustlsConnector::default());
    ureq::Agent::with_parts(config, connector, DefaultResolver::default())
}

/// Opens the TCP connection, unless one before it in the chain did.
#[derive(Debug)]
struct RacedTcpConnector;

impl<In: Transport> Connector<In> for RacedTcpConnector {
    type Out = Either<In, RacedTcpTransport>;

    fn connect(
        &self,
        details: &ConnectionDetails,
        chained: Option<In>,
    ) -> Result<Option<Self::Out>, ureq::Error> {
        if chained.is_some() {
            return Ok(chained.map(Either::A));
        }
        let timeout = details.timeout.not_zero().map(|after| *after);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        // Taken off the runtime's reactor, which goes with the runtime, while it still runs.
        let raced = runtime.block_on(async {
            heimdall_net::connect_addresses(&details.addrs, timeout)
                .await
                .map(|connected| connected.stream.into_std())
        });
        let stream = match raced {
            Ok(stream) => stream?,
            Err(heimdall_net::Failure::TimedOut { .. }) => {
                return Err(ureq::Error::Timeout(details.timeout.reason));
            }
            Err(failure) => return Err(ureq::Error::Io(io::Error::from(failure))),
        };
        // Blocking, as ureq reads and writes.
        stream.set_nonblocking(false)?;
        let config = details.config;
        if config.no_delay() {
            stream.set_nodelay(true)?;
        }
        let buffers = LazyBuffers::new(config.input_buffer_size(), config.output_buffer_size());
        Ok(Some(Either::B(RacedTcpTransport::new(stream, buffers))))
    }
}

/// A blocking TCP connection, as ureq's own, which it keeps private.
struct RacedTcpTransport {
    stream: TcpStream,
    buffers: LazyBuffers,
    read_timeout: Option<Duration>,
    write_timeout: Option<Duration>,
}

impl RacedTcpTransport {
    fn new(stream: TcpStream, buffers: LazyBuffers) -> Self {
        Self {
            stream,
            buffers,
            read_timeout: None,
            write_timeout: None,
        }
    }
}

impl fmt::Debug for RacedTcpTransport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RacedTcpTransport")
            .field("peer", &self.stream.peer_addr().ok())
            .finish_non_exhaustive()
    }
}

/// `timeout` set on `stream` by `set` when it differs from `previous`, which it becomes: a
/// system call only when the time changes.
fn update_timeout(
    timeout: NextTimeout,
    previous: &mut Option<Duration>,
    stream: &TcpStream,
    set: impl Fn(&TcpStream, Option<Duration>) -> io::Result<()>,
) -> io::Result<()> {
    let wanted = timeout.not_zero().map(|after| *after);
    if wanted != *previous {
        set(stream, wanted)?;
        *previous = wanted;
    }
    Ok(())
}

/// `error` as ureq reads a transport's: a time run out is its timeout, whichever kind the
/// system gives it (`WouldBlock` on Unix, `TimedOut` on Windows).
fn transport_error(error: io::Error, timeout: NextTimeout) -> ureq::Error {
    match error.kind() {
        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut => ureq::Error::Timeout(timeout.reason),
        _ => ureq::Error::Io(error),
    }
}

impl Transport for RacedTcpTransport {
    fn buffers(&mut self) -> &mut dyn Buffers {
        &mut self.buffers
    }

    fn transmit_output(&mut self, amount: usize, timeout: NextTimeout) -> Result<(), ureq::Error> {
        update_timeout(
            timeout,
            &mut self.write_timeout,
            &self.stream,
            TcpStream::set_write_timeout,
        )?;
        let output = &self.buffers.output()[..amount];
        self.stream
            .write_all(output)
            .map_err(|error| transport_error(error, timeout))
    }

    fn await_input(&mut self, timeout: NextTimeout) -> Result<bool, ureq::Error> {
        update_timeout(
            timeout,
            &mut self.read_timeout,
            &self.stream,
            TcpStream::set_read_timeout,
        )?;
        let input = self.buffers.input_append_buf();
        let amount = self
            .stream
            .read(input)
            .map_err(|error| transport_error(error, timeout))?;
        self.buffers.input_appended(amount);
        Ok(amount > 0)
    }

    fn is_open(&mut self) -> bool {
        // Open when nothing waits to be read and the connection is not closed: a byte the
        // request did not ask for means the connection is not to be used again.
        if self.stream.set_nonblocking(true).is_err() {
            return false;
        }
        let mut byte = [0_u8; 1];
        let open = matches!(
            self.stream.read(&mut byte),
            Err(error) if error.kind() == io::ErrorKind::WouldBlock
        );
        open && self.stream.set_nonblocking(false).is_ok()
    }
}

#[cfg(test)]
mod tests {
    use std::io::{BufRead as _, BufReader};
    use std::net::{Ipv4Addr, TcpListener};

    use super::*;

    /// The time the tests' requests have.
    const WAIT: Duration = Duration::from_secs(10);

    /// The body the test server answers.
    const BODY: &str = "{\"tag_name\":\"v2026.101101\"}";

    /// Requests the test server answers, on as many connections as the client opens.
    const REQUESTS: usize = 2;

    /// How long the test server waits for another request on a connection before it takes
    /// the next connection instead.
    const IDLE: Duration = Duration::from_secs(2);

    /// Answers `REQUESTS` requests on `listener`, then stops; the number of connections it
    /// accepted.
    fn serve(listener: &TcpListener) -> usize {
        let mut connections = 0;
        let mut answered = 0;
        while answered < REQUESTS {
            let Ok((stream, _)) = listener.accept() else {
                break;
            };
            connections += 1;
            stream.set_read_timeout(Some(IDLE)).expect("timeout");
            let mut reader = BufReader::new(stream.try_clone().expect("clone"));
            let mut writer = stream;
            while answered < REQUESTS {
                // The request line and headers, up to the blank line.
                let mut line = String::new();
                let mut seen = 0;
                while reader.read_line(&mut line).is_ok_and(|read| read > 0) {
                    seen += 1;
                    if line == "\r\n" {
                        break;
                    }
                    line.clear();
                }
                if seen == 0 {
                    break;
                }
                let length = BODY.len();
                let reply = format!("HTTP/1.1 200 OK\r\nContent-Length: {length}\r\n\r\n{BODY}");
                writer.write_all(reply.as_bytes()).expect("reply");
                answered += 1;
            }
        }
        connections
    }

    #[test]
    fn a_request_by_name_goes_over_the_raced_connection_and_the_connection_is_reused() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("bind");
        let port = listener.local_addr().expect("address").port();
        let server = std::thread::spawn(move || serve(&listener));
        let agent = agent(
            ureq::Agent::config_builder()
                .proxy(None)
                .timeout_global(Some(WAIT))
                .build(),
        );
        for _ in 0..REQUESTS {
            let body = agent
                .get(format!("http://localhost:{port}/"))
                .call()
                .expect("answered")
                .into_body()
                .read_to_string()
                .expect("body");
            assert_eq!(body, BODY);
        }
        assert_eq!(
            server.join().expect("server"),
            1,
            "kept open and used again"
        );
    }

    #[test]
    fn nothing_listening_is_a_connection_failure_as_ureq_reports_it() {
        // A port just freed: nothing listens on it any more.
        let reserved = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("bind");
        let port = reserved.local_addr().expect("address").port();
        drop(reserved);
        let agent = agent(
            ureq::Agent::config_builder()
                .proxy(None)
                .timeout_global(Some(WAIT))
                .build(),
        );
        let error = agent
            .get(format!("http://127.0.0.1:{port}/"))
            .call()
            .expect_err("nothing listens");
        assert!(
            matches!(
                &error,
                ureq::Error::Io(error) if error.kind() == io::ErrorKind::ConnectionRefused
            ) || matches!(error, ureq::Error::Timeout(_)),
            "{error}"
        );
    }
}
