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

//! X11 forwarding (`ssh -X`), for a shell that asks for it: the server's X11 programs reach
//! this computer's X server through channels the server opens back.
//!
//! The server is given a fake cookie, random for each connection, never the display's own.
//! Each X11 channel must present it in the X11 connection setup: it is checked, then replaced
//! by the display's real cookie, if any, before anything reaches the X server. A channel with
//! another cookie is closed and the X server never hears of it. A connection whose shell did
//! not ask, a gateway's among them, has every X11 channel refused, and so has a connection
//! whose asking shell has ended.
//!
//! The forwarding is trusted, as `PuTTY -X` and the C# Heimdall forward it: an X11 program on
//! the server is a full client of this computer's X server. It can read every window shown
//! there, the keys typed in them and the clipboard, and send input of its own. Untrusted
//! cookies (the X SECURITY extension, `ssh -X` on OpenSSH) are not used: the X servers of
//! Windows, `VcXsrv` and `Xming`, run without authorization and cannot issue one. That is why
//! the profile form warns, and why the setting stays off unless turned on.
//!
//! On a Windows computer several users share, the display is TCP port 6000 of 127.0.0.1: a
//! program of another user listening there first receives the X11 programs of the server,
//! and what they show. The X server detection only checks that something answers there.

use std::fmt;
use std::net::{IpAddr, Ipv4Addr};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use data_encoding::HEXLOWER;
use russh::client::Msg;
use russh::{Channel, ChannelStream};
use tokio::io::{AsyncRead, AsyncReadExt as _, AsyncWrite, AsyncWriteExt as _};
use tokio::net::TcpStream;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use zeroize::Zeroizing;

use crate::connection::Connection;
use crate::error::ConnectError;
use crate::forward::{self, X11Hold};
use crate::xauthority::{self, Cookie, Wanted};

/// The only authorization protocol a forwarded X11 program may present.
const MIT_MAGIC_COOKIE: &str = "MIT-MAGIC-COOKIE-1";

/// Bytes of a `MIT-MAGIC-COOKIE-1` cookie.
const COOKIE_LENGTH: usize = 16;

/// The screen announced to the server.
const SCREEN: u32 = 0;

/// Every X11 program of the shell may connect, not only the first.
const SINGLE_CONNECTION: bool = false;

/// No reply is awaited, as OpenSSH does: a server that refuses still opens the shell.
const WANT_REPLY: bool = false;

/// TCP port of display 0: display `n` listens on this plus `n`.
const TCP_PORT_BASE: u16 = 6000;

/// Where a local X server's sockets are.
#[cfg(unix)]
const SOCKET_DIRECTORY: &str = "/tmp/.X11-unix";

/// A display's socket: this, then the display's number.
#[cfg(unix)]
const SOCKET_PREFIX: &str = "X";

/// Hosts naming this computer's own display, reached through its socket.
const SOCKET_HOSTS: [&str; 2] = ["", "unix"];

/// A host naming this computer over TCP.
const LOCALHOST: &str = "localhost";

/// Where `localhost` is reached: IPv4, where an X server on Windows listens.
const LOOPBACK: &str = "127.0.0.1";

/// The byte order byte of a setup message sent most significant byte first.
const MSB_FIRST: u8 = b'B';

/// The byte order byte of a setup message sent least significant byte first.
const LSB_FIRST: u8 = b'l';

/// Bytes of the setup message before the authorization name.
const SETUP_HEADER_LENGTH: usize = 12;

/// Offsets in the header: protocol major and minor versions, then the lengths of the
/// authorization name and data, each two bytes.
const MAJOR_OFFSET: usize = 2;
const MINOR_OFFSET: usize = 4;
const NAME_LENGTH_OFFSET: usize = 6;
const DATA_LENGTH_OFFSET: usize = 8;

/// The name and the data are each padded to a multiple of this.
const PAD_ALIGNMENT: usize = 4;

/// Longest authorization name read from a channel.
const AUTH_NAME_LIMIT: usize = 256;

/// Longest authorization data read from a channel.
const AUTH_DATA_LIMIT: usize = 256;

/// Time a channel has to send its setup message once taken: an X11 program sends it at
/// once, so a channel silent this long is closed, and its place among the channels waiting
/// freed.
pub(crate) const SETUP_TIMEOUT: Duration = Duration::from_secs(10);

/// X11 channels of one connection waiting for their setup message at a time; beyond, a new
/// one is refused, so a server cannot hold this side's tasks open by the thousand.
pub(crate) const MAX_PENDING_SETUPS: usize = 32;

/// Time the X server has to take the connection of a channel.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// The local X display a shell's X11 programs are shown on, as `DISPLAY` names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct X11Display {
    host: String,
    number: u16,
    authority: Option<PathBuf>,
}

impl X11Display {
    /// The display `name` names, `[host]:number[.screen]` as `DISPLAY` holds it, its cookie
    /// read from the usual Xauthority file: the one `XAUTHORITY` names, else `.Xauthority`
    /// in the home folder. `None` for a name not of that form.
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        let (host, rest) = name.trim().rsplit_once(':')?;
        if host.contains('/') {
            return None;
        }
        let number = rest.split_once('.').map_or(rest, |(number, _)| number);
        let number = number.parse::<u16>().ok()?;
        TCP_PORT_BASE.checked_add(number)?;
        Some(Self {
            host: host.to_owned(),
            number,
            authority: xauthority::default_path(),
        })
    }

    /// The same display, its cookie read from `authority`, or none sent when `None`.
    #[must_use]
    pub fn with_authority(self, authority: Option<PathBuf>) -> Self {
        Self { authority, ..self }
    }

    /// This computer's own display, through its socket.
    fn on_socket(&self) -> bool {
        SOCKET_HOSTS.contains(&self.host.as_str())
    }

    /// Where its X server is reached.
    fn target(&self) -> Target {
        let port = TCP_PORT_BASE.saturating_add(self.number);
        if self.on_socket() {
            #[cfg(unix)]
            return Target::Socket(
                PathBuf::from(SOCKET_DIRECTORY).join(format!("{SOCKET_PREFIX}{}", self.number)),
            );
            #[cfg(not(unix))]
            return Target::Tcp(LOOPBACK.to_owned(), port);
        }
        if self.host.eq_ignore_ascii_case(LOCALHOST) {
            return Target::Tcp(LOOPBACK.to_owned(), port);
        }
        Target::Tcp(self.host.clone(), port)
    }

    /// The real cookie of the display, from its Xauthority file; empty without one.
    fn cookie(&self) -> Cookie {
        let Some(path) = &self.authority else {
            return Cookie::default();
        };
        let address = if self.host.eq_ignore_ascii_case(LOCALHOST) {
            Some(IpAddr::V4(Ipv4Addr::LOCALHOST))
        } else {
            self.host.parse().ok()
        };
        let local = self.on_socket() || address.is_some_and(|address| address.is_loopback());
        let hostname = xauthority::hostname();
        let wanted = Wanted {
            number: self.number,
            local,
            address,
            hostname: hostname.as_deref(),
        };
        xauthority::cookie_in(path, &wanted).unwrap_or_default()
    }
}

impl fmt::Display for X11Display {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.host, self.number)
    }
}

/// Where an X server is reached.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Target {
    Tcp(String, u16),
    #[cfg(unix)]
    Socket(PathBuf),
}

/// X11 granted to a connection: the fake cookie its server was given, the real one, and the
/// display its channels go to.
pub(crate) struct X11Grant {
    fake: Zeroizing<[u8; COOKIE_LENGTH]>,
    real: Cookie,
    target: Target,
    display: String,
    /// Places for the channels waiting for their setup message.
    pending: Arc<Semaphore>,
}

impl fmt::Debug for X11Grant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("X11Grant")
            .field("display", &self.display)
            .finish_non_exhaustive()
    }
}

impl X11Grant {
    /// A grant to `display` with a fresh fake cookie; `None` when no randomness is to be had.
    pub(crate) fn new(display: &X11Display) -> Option<Self> {
        let mut fake = Zeroizing::new([0; COOKIE_LENGTH]);
        if let Err(error) = sealvault::random::fill(fake.as_mut()) {
            log::warn!("no randomness for the X11 cookie: {error}");
            return None;
        }
        Some(Self {
            fake,
            real: display.cookie(),
            target: display.target(),
            display: display.to_string(),
            pending: Arc::new(Semaphore::new(MAX_PENDING_SETUPS)),
        })
    }

    /// Whether `setup` presents the fake cookie, in constant time.
    fn check(&self, setup: &Setup) -> Result<(), Refusal> {
        if setup.name != MIT_MAGIC_COOKIE.as_bytes() {
            return Err(Refusal::Protocol);
        }
        if !sealvault::compare::equal(&setup.data, self.fake.as_ref()) {
            return Err(Refusal::Cookie);
        }
        Ok(())
    }
}

/// Asks the server of `connection` to forward X11 on `channel`, a shell's session channel,
/// to `display`. The grant is in place before the request: the server may open a channel at
/// once. A second shell on the connection shares the first one's cookie and display, as
/// OpenSSH does. The grant lasts as long as the hold returned, which the shell keeps until
/// it ends; `None` when nothing was asked.
pub(crate) async fn request(
    connection: &Connection,
    channel: &Channel<Msg>,
    display: &X11Display,
) -> Result<Option<X11Hold>, ConnectError> {
    let Some((grant, hold)) = connection.routes().grant_x11(display) else {
        log::warn!("X11 forwarding asked for, but not forwarded: no cookie could be made");
        return Ok(None);
    };
    let fake = Zeroizing::new(HEXLOWER.encode(grant.fake.as_ref()));
    channel
        .request_x11(
            WANT_REPLY,
            SINGLE_CONNECTION,
            MIT_MAGIC_COOKIE,
            fake.as_str(),
            SCREEN,
        )
        .await
        .map_err(ConnectError::Protocol)?;
    let cookie = if grant.real.data.is_empty() {
        "no cookie"
    } else {
        "the display's cookie"
    };
    log::info!(
        "X11 forwarding asked for, to display {} with {cookie}",
        grant.display
    );
    Ok(Some(hold))
}

/// A place for a new X11 channel among those waiting for their setup message; `None`, said
/// in the log, when all are taken.
pub(crate) fn admit(grant: &Arc<X11Grant>) -> Option<OwnedSemaphorePermit> {
    let permit = grant.pending.clone().try_acquire_owned().ok();
    if permit.is_none() {
        log::warn!(
            "the server opened an X11 channel while {MAX_PENDING_SETUPS} wait for their setup: refused"
        );
    }
    permit
}

/// Why an X11 channel was closed.
#[derive(Debug, thiserror::Error)]
enum Refusal {
    #[error("it ended before its X11 setup")]
    Truncated,
    #[error("its X11 setup gave no byte order")]
    ByteOrder,
    #[error("its X11 authorization is longer than allowed")]
    Oversize,
    #[error("its X11 authorization is not MIT-MAGIC-COOKIE-1")]
    Protocol,
    #[error("its X11 cookie is not the one given to the server")]
    Cookie,
    #[error("it sent no X11 setup in time")]
    Timeout,
}

/// The byte order an X11 program speaks in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ByteOrder {
    Msb,
    Lsb,
}

impl ByteOrder {
    fn of(byte: u8) -> Option<Self> {
        match byte {
            MSB_FIRST => Some(Self::Msb),
            LSB_FIRST => Some(Self::Lsb),
            _ => None,
        }
    }

    fn byte(self) -> u8 {
        match self {
            Self::Msb => MSB_FIRST,
            Self::Lsb => LSB_FIRST,
        }
    }

    fn read(self, header: &[u8; SETUP_HEADER_LENGTH], offset: usize) -> u16 {
        let bytes = [header[offset], header[offset + 1]];
        match self {
            Self::Msb => u16::from_be_bytes(bytes),
            Self::Lsb => u16::from_le_bytes(bytes),
        }
    }

    fn write(self, value: u16) -> [u8; 2] {
        match self {
            Self::Msb => value.to_be_bytes(),
            Self::Lsb => value.to_le_bytes(),
        }
    }
}

/// An X11 connection setup message, as far as its authorization.
struct Setup {
    order: ByteOrder,
    major: u16,
    minor: u16,
    name: Vec<u8>,
    data: Zeroizing<Vec<u8>>,
}

impl Setup {
    /// The same message with `cookie`'s authorization, in its byte order.
    fn encode(&self, cookie: &Cookie) -> Zeroizing<Vec<u8>> {
        let length = |bytes: &[u8]| u16::try_from(bytes.len()).unwrap_or(u16::MAX);
        let mut message = Zeroizing::new(Vec::with_capacity(
            SETUP_HEADER_LENGTH + padded(cookie.name.len()) + padded(cookie.data.len()),
        ));
        message.extend([self.order.byte(), 0]);
        message.extend(self.order.write(self.major));
        message.extend(self.order.write(self.minor));
        message.extend(self.order.write(length(&cookie.name)));
        message.extend(self.order.write(length(&cookie.data)));
        message.extend([0, 0]);
        for field in [&cookie.name[..], &cookie.data[..]] {
            message.extend(field);
            let padding = padded(field.len()) - field.len();
            message.extend(std::iter::repeat_n(0, padding));
        }
        message
    }
}

/// `length` rounded up to the protocol's alignment.
fn padded(length: usize) -> usize {
    length.next_multiple_of(PAD_ALIGNMENT)
}

/// Reads a setup message from `reader`, as far as its authorization: nothing past it.
async fn read_setup<R: AsyncRead + Unpin>(reader: &mut R) -> Result<Setup, Refusal> {
    let mut header = [0; SETUP_HEADER_LENGTH];
    reader
        .read_exact(&mut header)
        .await
        .map_err(|_| Refusal::Truncated)?;
    let order = ByteOrder::of(header[0]).ok_or(Refusal::ByteOrder)?;
    let name_length = usize::from(order.read(&header, NAME_LENGTH_OFFSET));
    let data_length = usize::from(order.read(&header, DATA_LENGTH_OFFSET));
    if name_length > AUTH_NAME_LIMIT || data_length > AUTH_DATA_LIMIT {
        return Err(Refusal::Oversize);
    }
    let name = read_padded(reader, name_length).await?;
    let data = Zeroizing::new(read_padded(reader, data_length).await?);
    Ok(Setup {
        order,
        major: order.read(&header, MAJOR_OFFSET),
        minor: order.read(&header, MINOR_OFFSET),
        name,
        data,
    })
}

/// Reads `length` bytes and their padding; the bytes.
async fn read_padded<R: AsyncRead + Unpin>(
    reader: &mut R,
    length: usize,
) -> Result<Vec<u8>, Refusal> {
    let mut bytes = vec![0; padded(length)];
    reader
        .read_exact(&mut bytes)
        .await
        .map_err(|_| Refusal::Truncated)?;
    bytes.truncate(length);
    Ok(bytes)
}

/// Reads the setup message from `reader` within `limit` and checks it against `grant`.
async fn read_checked<R: AsyncRead + Unpin>(
    reader: &mut R,
    grant: &X11Grant,
    limit: Duration,
) -> Result<Setup, Refusal> {
    match tokio::time::timeout(limit, read_setup(reader)).await {
        Ok(Ok(setup)) => grant.check(&setup).map(|()| setup),
        Ok(Err(refusal)) => Err(refusal),
        Err(_) => Err(Refusal::Timeout),
    }
}

/// Carries an X11 channel the server opened to the display of `grant`: its setup checked
/// against the fake cookie, rewritten with the real one, then everything both ways until
/// either side closes. A channel refused is closed with nothing sent on. `waiting` is its
/// place among the channels waiting for their setup, given back once it is read.
pub(crate) async fn carry(
    channel: Channel<Msg>,
    grant: Arc<X11Grant>,
    waiting: OwnedSemaphorePermit,
) {
    let mut far = channel.into_stream();
    let checked = read_checked(&mut far, &grant, SETUP_TIMEOUT).await;
    drop(waiting);
    let setup = match checked {
        Ok(setup) => setup,
        Err(refusal) => {
            // Dropping the stream closes the channel.
            log::warn!("the server opened an X11 channel, closed: {refusal}");
            return;
        }
    };
    log::info!(
        "the server opened an X11 channel, carried to display {}",
        grant.display
    );
    let message = setup.encode(&grant.real);
    let unanswered = match &grant.target {
        Target::Tcp(host, port) => {
            match tokio::time::timeout(CONNECT_TIMEOUT, TcpStream::connect((host.as_str(), *port)))
                .await
            {
                Ok(Ok(near)) => {
                    relay(near, far, &message).await;
                    None
                }
                Ok(Err(error)) => Some(error.to_string()),
                Err(_) => Some(timed_out()),
            }
        }
        #[cfg(unix)]
        Target::Socket(path) => {
            match tokio::time::timeout(CONNECT_TIMEOUT, tokio::net::UnixStream::connect(path)).await
            {
                Ok(Ok(near)) => {
                    relay(near, far, &message).await;
                    None
                }
                Ok(Err(error)) => Some(error.to_string()),
                Err(_) => Some(timed_out()),
            }
        }
    };
    if let Some(error) = unanswered {
        // The channel, dropped with it, closes.
        log::warn!(
            "display {} did not answer an X11 channel, closed: {error}",
            grant.display
        );
    }
}

/// Why a display did not answer in time.
fn timed_out() -> String {
    format!("no answer in {} s", CONNECT_TIMEOUT.as_secs())
}

/// Sends `message` to the X server, then carries both ways.
async fn relay<N: AsyncRead + AsyncWrite + Unpin>(
    mut near: N,
    far: ChannelStream<Msg>,
    message: &[u8],
) {
    if let Err(error) = near.write_all(message).await {
        log::debug!("an X11 setup did not reach the X server: {error}");
        return;
    }
    forward::join(near, far, "an X11 connection").await;
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{
        AUTH_DATA_LIMIT, ByteOrder, COOKIE_LENGTH, Cookie, MIT_MAGIC_COOKIE, Refusal,
        SETUP_TIMEOUT, Setup, Target, X11Display, X11Grant, padded, read_checked, read_setup, same,
    };
    use zeroize::Zeroizing;

    fn grant() -> X11Grant {
        let display = X11Display::parse("127.0.0.1:0")
            .expect("display")
            .with_authority(None);
        X11Grant::new(&display).expect("granted")
    }

    #[test]
    fn each_grant_draws_its_own_fake_cookie_of_the_cookie_length() {
        let first = grant();
        let second = grant();
        assert_eq!(first.fake.len(), COOKIE_LENGTH);
        assert_ne!(*first.fake, *second.fake, "two draws of 128 bits");
        assert_ne!(*first.fake, [0; COOKIE_LENGTH]);
    }

    #[tokio::test]
    async fn only_the_whole_fake_cookie_of_its_length_is_taken() {
        let grant = grant();
        let fake = grant.fake.to_vec();
        let mut almost = fake.clone();
        almost[COOKIE_LENGTH - 1] ^= 1;
        let mut longer = fake.clone();
        longer.push(0);
        let shorter = fake[..COOKIE_LENGTH - 1].to_vec();
        for (data, taken) in [
            (&fake, true),
            (&almost, false),
            (&longer, false),
            (&shorter, false),
        ] {
            let bytes = message(ByteOrder::Msb, MIT_MAGIC_COOKIE.as_bytes(), data);
            let checked = read_checked(&mut &bytes[..], &grant, SETUP_TIMEOUT).await;
            assert_eq!(checked.is_ok(), taken, "{} bytes", data.len());
            if !taken {
                assert!(matches!(checked, Err(Refusal::Cookie)));
            }
        }
        let other = message(ByteOrder::Msb, b"XDM-AUTHORIZATION-1", &fake);
        assert!(matches!(
            read_checked(&mut &other[..], &grant, SETUP_TIMEOUT).await,
            Err(Refusal::Protocol)
        ));
    }

    #[tokio::test]
    async fn a_channel_that_never_sends_its_setup_is_given_up_on() {
        assert_eq!(SETUP_TIMEOUT, Duration::from_secs(10));
        let grant = grant();
        // Kept open, never written to: the reader waits for ever without the limit.
        let (mut silent, _peer) = tokio::io::duplex(64);
        assert!(matches!(
            read_checked(&mut silent, &grant, Duration::ZERO).await,
            Err(Refusal::Timeout)
        ));
    }

    fn message(order: ByteOrder, name: &[u8], data: &[u8]) -> Vec<u8> {
        let setup = Setup {
            order,
            major: 11,
            minor: 0,
            name: Vec::new(),
            data: Zeroizing::new(Vec::new()),
        };
        setup
            .encode(&Cookie {
                name: name.to_vec(),
                data: Zeroizing::new(data.to_vec()),
            })
            .to_vec()
    }

    async fn read(bytes: &[u8]) -> Result<Setup, Refusal> {
        let mut reader = bytes;
        read_setup(&mut reader).await
    }

    #[tokio::test]
    async fn a_setup_is_read_in_either_byte_order_and_written_back_in_it() {
        for order in [ByteOrder::Msb, ByteOrder::Lsb] {
            let bytes = message(order, MIT_MAGIC_COOKIE.as_bytes(), &[7; 16]);
            assert_eq!(bytes.len(), 12 + 20 + 16);
            let setup = read(&bytes).await.expect("read");
            assert_eq!(setup.order, order);
            assert_eq!((setup.major, setup.minor), (11, 0));
            assert_eq!(setup.name, MIT_MAGIC_COOKIE.as_bytes());
            assert_eq!(&setup.data[..], &[7; 16]);
            let empty = setup.encode(&Cookie::default());
            assert_eq!(empty.len(), 12, "no authorization: the header alone");
            assert_eq!(empty[0], order.byte());
        }
        let lsb = message(ByteOrder::Lsb, b"ab", &[1, 2, 3]);
        assert_eq!(&lsb[..12], &[b'l', 0, 11, 0, 0, 0, 2, 0, 3, 0, 0, 0]);
        let msb = message(ByteOrder::Msb, b"ab", &[1, 2, 3]);
        assert_eq!(&msb[..12], &[b'B', 0, 0, 11, 0, 0, 0, 2, 0, 3, 0, 0]);
        assert_eq!(&msb[12..], &[b'a', b'b', 0, 0, 1, 2, 3, 0]);
    }

    #[tokio::test]
    async fn a_setup_cut_short_oversize_or_of_no_byte_order_is_refused() {
        let bytes = message(ByteOrder::Msb, MIT_MAGIC_COOKIE.as_bytes(), &[7; 16]);
        for cut in [0, 11, 12, 31, bytes.len() - 1] {
            assert!(
                matches!(read(&bytes[..cut]).await, Err(Refusal::Truncated)),
                "cut at {cut}"
            );
        }
        let mut no_order = bytes.clone();
        no_order[0] = b'x';
        assert!(matches!(read(&no_order).await, Err(Refusal::ByteOrder)));
        let oversize = message(ByteOrder::Lsb, b"n", &vec![0; AUTH_DATA_LIMIT + 1]);
        assert!(matches!(read(&oversize).await, Err(Refusal::Oversize)));
    }

    #[test]
    fn padding_and_comparison() {
        assert_eq!([0, 1, 4, 5, 18].map(padded), [0, 4, 4, 8, 20]);
        assert!(same(&[1, 2], &[1, 2]));
        assert!(!same(&[1, 2], &[1, 3]));
        assert!(!same(&[1, 2], &[1, 2, 3]));
    }

    #[test]
    fn a_display_name_is_read_as_display_sets_it() {
        let display = X11Display::parse("localhost:10.0").expect("parsed");
        assert_eq!(display.to_string(), "localhost:10");
        assert_eq!(display.target(), Target::Tcp("127.0.0.1".to_owned(), 6010));
        let remote = X11Display::parse("desk.lab:2").expect("parsed");
        assert_eq!(remote.target(), Target::Tcp("desk.lab".to_owned(), 6002));
        let own = X11Display::parse(":0").expect("parsed");
        #[cfg(unix)]
        assert_eq!(
            own.target(),
            Target::Socket(std::path::PathBuf::from("/tmp/.X11-unix/X0"))
        );
        #[cfg(not(unix))]
        assert_eq!(own.target(), Target::Tcp("127.0.0.1".to_owned(), 6000));
        for invalid in ["", "localhost", ":x", ":60000", "/tmp/launch/org.xquartz:0"] {
            assert_eq!(X11Display::parse(invalid), None, "{invalid}");
        }
    }
}
