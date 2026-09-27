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

//! The FTP session: the control connection, and a data connection per listing or transfer.

use std::fmt;
use std::io;
use std::net::{IpAddr, SocketAddr};
use std::pin::Pin;
use std::str::FromStr;
use std::task::{Context, Poll};
use std::time::Duration;

use data_encoding::BASE64_NOPAD;
use ring::digest::{SHA256, SHA256_OUTPUT_LEN, digest};
use tokio::io::{AsyncRead, AsyncReadExt as _, AsyncWrite, AsyncWriteExt as _, BufReader, ReadBuf};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;
use tokio_rustls::client::TlsStream;
use tokio_rustls::rustls::pki_types::ServerName;
use zeroize::Zeroizing;

use crate::listing::{Entry, parse_list, parse_mlsd};
use crate::reply::{Reply, ReplyError, read_reply};
use crate::tls;

/// The user name of an anonymous login.
const ANONYMOUS: &str = "anonymous";

/// Largest listing read, in bytes: a server sending more is not listing a folder.
pub const MAX_LISTING: usize = 64 * 1024 * 1024;

/// Bytes copied at a time in a transfer.
const TRANSFER_CHUNK: usize = 64 * 1024;

/// Prefix of a written fingerprint.
const FINGERPRINT_PREFIX: &str = "SHA256:";

/// The SHA-256 of a server certificate, as `SHA256:` and unpadded base64.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Fingerprint([u8; SHA256_OUTPUT_LEN]);

impl Fingerprint {
    /// The fingerprint of a certificate, DER encoded.
    #[must_use]
    pub fn of(der: &[u8]) -> Self {
        let mut bytes = [0; SHA256_OUTPUT_LEN];
        bytes.copy_from_slice(digest(&SHA256, der).as_ref());
        Self(bytes)
    }
}

impl fmt::Display for Fingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{FINGERPRINT_PREFIX}{}", BASE64_NOPAD.encode(&self.0))
    }
}

impl fmt::Debug for Fingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

/// A written fingerprint that is not one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("not a SHA-256 fingerprint")]
pub struct FingerprintParseError;

impl FromStr for Fingerprint {
    type Err = FingerprintParseError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let encoded = text
            .strip_prefix(FINGERPRINT_PREFIX)
            .ok_or(FingerprintParseError)?;
        let bytes = BASE64_NOPAD
            .decode(encoded.as_bytes())
            .map_err(|_| FingerprintParseError)?;
        bytes
            .try_into()
            .map(Self)
            .map_err(|_| FingerprintParseError)
    }
}

/// How the connection is protected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Security {
    /// None: the password, the names and the files cross in clear.
    Plain,
    /// `AUTH TLS` before logging in, and every data connection in TLS too. The server is
    /// trusted by its certificate: `pinned` is the one recorded for it, `None` on a first
    /// contact.
    ExplicitTls {
        /// The fingerprint recorded for this server.
        pinned: Option<Fingerprint>,
    },
}

/// Where to connect, and as whom.
#[derive(Debug, Clone)]
pub struct FtpConfig {
    /// Server.
    pub host: String,
    /// Port.
    pub port: u16,
    /// Account; `None` logs in anonymously.
    pub username: Option<String>,
    /// Protection.
    pub security: Security,
    /// Limit on connecting, on each reply, and on each silence in a transfer.
    pub timeout: Duration,
}

/// Why an FTP operation failed.
#[derive(Debug, thiserror::Error)]
pub enum FtpError {
    /// The connection failed.
    #[error("network: {0}")]
    Network(#[source] io::Error),
    /// The server did not answer in time.
    #[error("the server did not answer in time")]
    Timeout,
    /// The server sent something that is not FTP.
    #[error("the server does not speak FTP as expected: {0}")]
    Protocol(String),
    /// The server refused TLS, which this connection requires.
    #[error("the server refused to switch to TLS")]
    TlsRefused,
    /// The TLS handshake failed.
    #[error("TLS: {0}")]
    Tls(#[source] io::Error),
    /// First contact with this server: its certificate is not recorded. Nothing was sent.
    #[error("unknown server certificate {0}")]
    UnknownCertificate(Fingerprint),
    /// The server's certificate is not the recorded one. Nothing was sent.
    #[error("the server certificate changed: recorded {pinned}, presented {presented}")]
    CertificateChanged {
        /// Recorded.
        pinned: Fingerprint,
        /// Presented.
        presented: Fingerprint,
    },
    /// The data connection showed another certificate than the control connection.
    #[error("the data connection presented another certificate than the server")]
    DataCertificateMismatch,
    /// The login was refused.
    #[error("login refused: {0}")]
    Login(String),
    /// The server refused a command; its code and its words.
    #[error("refused ({code}): {text}")]
    Refused {
        /// Reply code.
        code: u16,
        /// The server's words, untrusted.
        text: String,
    },
    /// A name or a path would have broken the command line it goes in.
    #[error("a name holds a line break or a NUL")]
    UnsafeArgument,
    /// A listing longer than [`MAX_LISTING`].
    #[error("the listing is too long")]
    ListingTooLong,
}

impl From<ReplyError> for FtpError {
    fn from(error: ReplyError) -> Self {
        match error {
            ReplyError::Io(error) => Self::Network(error),
            ReplyError::Malformed => Self::Protocol("malformed reply".to_owned()),
        }
    }
}

/// The control connection, before or after `AUTH TLS`.
enum Control {
    Plain(TcpStream),
    Tls(Box<TlsStream<TcpStream>>),
    /// Only while the socket moves into TLS, or after a failed move: unusable.
    Closed,
}

fn closed() -> io::Error {
    io::ErrorKind::NotConnected.into()
}

impl AsyncRead for Control {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Self::Plain(stream) => Pin::new(stream).poll_read(cx, buf),
            Self::Tls(stream) => Pin::new(stream.as_mut()).poll_read(cx, buf),
            Self::Closed => Poll::Ready(Err(closed())),
        }
    }
}

impl AsyncWrite for Control {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        match self.get_mut() {
            Self::Plain(stream) => Pin::new(stream).poll_write(cx, buf),
            Self::Tls(stream) => Pin::new(stream.as_mut()).poll_write(cx, buf),
            Self::Closed => Poll::Ready(Err(closed())),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Self::Plain(stream) => Pin::new(stream).poll_flush(cx),
            Self::Tls(stream) => Pin::new(stream.as_mut()).poll_flush(cx),
            Self::Closed => Poll::Ready(Err(closed())),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Self::Plain(stream) => Pin::new(stream).poll_shutdown(cx),
            Self::Tls(stream) => Pin::new(stream.as_mut()).poll_shutdown(cx),
            Self::Closed => Poll::Ready(Err(closed())),
        }
    }
}

/// A data connection, in TLS when the control connection is.
enum Data {
    Plain(TcpStream),
    Tls(Box<TlsStream<TcpStream>>),
}

impl Data {
    fn reader(&mut self) -> &mut (dyn AsyncRead + Unpin + Send) {
        match self {
            Self::Plain(stream) => stream,
            Self::Tls(stream) => stream.as_mut(),
        }
    }

    fn writer(&mut self) -> &mut (dyn AsyncWrite + Unpin + Send) {
        match self {
            Self::Plain(stream) => stream,
            Self::Tls(stream) => stream.as_mut(),
        }
    }
}

/// What the server supports, from `FEAT`.
#[derive(Debug, Clone, Copy, Default)]
struct Features {
    mlsd: bool,
    epsv: bool,
    utf8: bool,
}

/// TLS of the session: what data connections are wrapped with and must present.
struct SessionTls {
    connector: TlsConnector,
    name: ServerName<'static>,
    fingerprint: Fingerprint,
}

/// A logged-in FTP session.
pub struct FtpClient {
    control: BufReader<Control>,
    /// The server's address: every data connection goes there, whatever `PASV` says.
    peer: IpAddr,
    tls: Option<SessionTls>,
    features: Features,
    timeout: Duration,
}

impl fmt::Debug for FtpClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FtpClient")
            .field("peer", &self.peer)
            .field("tls", &self.tls.is_some())
            .finish_non_exhaustive()
    }
}

impl FtpClient {
    /// Connects and logs in. With [`Security::ExplicitTls`], the certificate is checked
    /// against the pin before the user name or the password is sent.
    ///
    /// # Errors
    ///
    /// [`FtpError`]; [`FtpError::UnknownCertificate`] asks the caller to decide about the
    /// server, then to connect again with the fingerprint pinned.
    pub async fn connect(
        config: &FtpConfig,
        password: &Zeroizing<String>,
    ) -> Result<Self, FtpError> {
        let tcp = within(
            config.timeout,
            TcpStream::connect((config.host.as_str(), config.port)),
        )
        .await?
        .map_err(FtpError::Network)?;
        let peer = tcp.peer_addr().map_err(FtpError::Network)?.ip();
        let _ = tcp.set_nodelay(true);
        let mut client = Self {
            control: BufReader::new(Control::Plain(tcp)),
            peer,
            tls: None,
            features: Features::default(),
            timeout: config.timeout,
        };
        let greeting = client.reply().await?;
        if greeting.class() != 2 {
            return Err(refused(&greeting));
        }
        if let Security::ExplicitTls { pinned } = config.security {
            client.start_tls(&config.host, pinned).await?;
        }
        client.login(config.username.as_deref(), password).await?;
        if client.tls.is_some() {
            client.expect(&["PBSZ 0"], 2).await?;
            client.expect(&["PROT P"], 2).await?;
        }
        client.features = client.features().await?;
        if client.features.utf8 {
            // Names in UTF-8; a server saying no keeps its own.
            let _ = client.command(&["OPTS UTF8 ON"]).await?;
        }
        client.expect(&["TYPE I"], 2).await?;
        Ok(client)
    }

    /// `AUTH TLS`, the handshake, and the pin, before anything else is sent.
    async fn start_tls(&mut self, host: &str, pinned: Option<Fingerprint>) -> Result<(), FtpError> {
        let reply = self.command(&["AUTH TLS"]).await?;
        if reply.code != 234 {
            return Err(FtpError::TlsRefused);
        }
        let connector = tls::connector();
        let name = ServerName::try_from(host.to_owned())
            .map_err(|_| FtpError::Protocol("the server name is not valid".to_owned()))?;
        // Nothing may be waiting in the buffer: it would be clear text taken for protected.
        if !self.control.buffer().is_empty() {
            return Err(FtpError::Protocol("data sent before TLS".to_owned()));
        }
        let Control::Plain(tcp) = std::mem::replace(self.control.get_mut(), Control::Closed) else {
            return Err(FtpError::Protocol("TLS started twice".to_owned()));
        };
        let stream = within(self.timeout, connector.connect(name.clone(), tcp))
            .await?
            .map_err(FtpError::Tls)?;
        let presented = presented(&stream)?;
        match pinned {
            Some(pinned) if pinned == presented => {}
            Some(pinned) => return Err(FtpError::CertificateChanged { pinned, presented }),
            None => return Err(FtpError::UnknownCertificate(presented)),
        }
        *self.control.get_mut() = Control::Tls(Box::new(stream));
        self.tls = Some(SessionTls {
            connector,
            name,
            fingerprint: presented,
        });
        Ok(())
    }

    async fn login(
        &mut self,
        username: Option<&str>,
        password: &Zeroizing<String>,
    ) -> Result<(), FtpError> {
        let user = username
            .filter(|name| !name.is_empty())
            .unwrap_or(ANONYMOUS);
        let reply = self.command(&["USER ", user]).await?;
        let reply = match reply.code {
            230 => return Ok(()),
            331 => self.command_secret("PASS ", password).await?,
            _ => return Err(FtpError::Login(reply.text())),
        };
        match reply.code {
            230 | 202 => Ok(()),
            _ => Err(FtpError::Login(reply.text())),
        }
    }

    async fn features(&mut self) -> Result<Features, FtpError> {
        let reply = self.command(&["FEAT"]).await?;
        if reply.code != 211 {
            return Ok(Features::default());
        }
        let mut features = Features::default();
        for line in &reply.lines {
            let feature = line
                .split_whitespace()
                .next()
                .unwrap_or("")
                .to_ascii_uppercase();
            match feature.as_str() {
                "MLSD" | "MLST" => features.mlsd = true,
                "EPSV" => features.epsv = true,
                "UTF8" => features.utf8 = true,
                _ => {}
            }
        }
        Ok(features)
    }

    /// The server's current folder.
    ///
    /// # Errors
    ///
    /// [`FtpError`].
    pub async fn current_dir(&mut self) -> Result<String, FtpError> {
        let reply = self.expect(&["PWD"], 2).await?;
        let text = reply.text();
        // `257 "/path" is current`, a quote inside doubled.
        let start = text
            .find('"')
            .ok_or_else(|| FtpError::Protocol(text.clone()))?;
        let mut path = String::new();
        let mut chars = text[start + 1..].chars().peekable();
        while let Some(c) = chars.next() {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    path.push('"');
                } else {
                    return Ok(path);
                }
            } else {
                path.push(c);
            }
        }
        Err(FtpError::Protocol(text))
    }

    /// The entries of the folder `path`.
    ///
    /// # Errors
    ///
    /// [`FtpError`].
    pub async fn list(&mut self, path: &str) -> Result<Vec<Entry>, FtpError> {
        let machine = self.features.mlsd;
        let verb = if machine { "MLSD " } else { "LIST " };
        let mut data = self.open_data(&[verb, path]).await?;
        let mut bytes = Vec::new();
        let read = within(
            self.timeout,
            data.reader()
                .take(u64::try_from(MAX_LISTING + 1).unwrap_or(u64::MAX))
                .read_to_end(&mut bytes),
        )
        .await?
        .map_err(FtpError::Network)?;
        drop(data);
        self.finish().await?;
        if read > MAX_LISTING {
            return Err(FtpError::ListingTooLong);
        }
        let text = String::from_utf8_lossy(&bytes);
        Ok(if machine {
            parse_mlsd(&text)
        } else {
            parse_list(&text)
        })
    }

    /// Copies the file `path` into `sink`, telling `progress` the bytes copied so far; the
    /// number of bytes copied.
    ///
    /// # Errors
    ///
    /// [`FtpError`].
    pub async fn retrieve(
        &mut self,
        path: &str,
        sink: &mut (impl AsyncWrite + Unpin),
        mut progress: impl FnMut(u64),
    ) -> Result<u64, FtpError> {
        let mut data = self.open_data(&["RETR ", path]).await?;
        let mut buffer = vec![0; TRANSFER_CHUNK];
        let mut total = 0_u64;
        loop {
            let read = within(self.timeout, data.reader().read(&mut buffer))
                .await?
                .map_err(FtpError::Network)?;
            if read == 0 {
                break;
            }
            sink.write_all(&buffer[..read])
                .await
                .map_err(FtpError::Network)?;
            total += u64::try_from(read).unwrap_or(u64::MAX);
            progress(total);
        }
        sink.flush().await.map_err(FtpError::Network)?;
        drop(data);
        self.finish().await?;
        Ok(total)
    }

    /// Writes `source` to the file `path`, replacing it, telling `progress` the bytes sent
    /// so far; the number of bytes sent.
    ///
    /// # Errors
    ///
    /// [`FtpError`].
    pub async fn store(
        &mut self,
        path: &str,
        source: &mut (impl AsyncRead + Unpin),
        mut progress: impl FnMut(u64),
    ) -> Result<u64, FtpError> {
        let mut data = self.open_data(&["STOR ", path]).await?;
        let mut buffer = vec![0; TRANSFER_CHUNK];
        let mut total = 0_u64;
        loop {
            let read = source.read(&mut buffer).await.map_err(FtpError::Network)?;
            if read == 0 {
                break;
            }
            within(self.timeout, data.writer().write_all(&buffer[..read]))
                .await?
                .map_err(FtpError::Network)?;
            total += u64::try_from(read).unwrap_or(u64::MAX);
            progress(total);
        }
        // The end of the file is the end of the data connection.
        within(self.timeout, data.writer().shutdown())
            .await?
            .map_err(FtpError::Network)?;
        drop(data);
        self.finish().await?;
        Ok(total)
    }

    /// The size of the file `path`, when the server tells it.
    ///
    /// # Errors
    ///
    /// [`FtpError`].
    pub async fn size(&mut self, path: &str) -> Result<Option<u64>, FtpError> {
        let reply = self.command(&["SIZE ", path]).await?;
        Ok((reply.code == 213)
            .then(|| reply.text().trim().parse().ok())
            .flatten())
    }

    /// Deletes the file `path`.
    ///
    /// # Errors
    ///
    /// [`FtpError`].
    pub async fn delete(&mut self, path: &str) -> Result<(), FtpError> {
        self.expect(&["DELE ", path], 2).await.map(drop)
    }

    /// Creates the folder `path`.
    ///
    /// # Errors
    ///
    /// [`FtpError`].
    pub async fn make_dir(&mut self, path: &str) -> Result<(), FtpError> {
        self.expect(&["MKD ", path], 2).await.map(drop)
    }

    /// Removes the empty folder `path`.
    ///
    /// # Errors
    ///
    /// [`FtpError`].
    pub async fn remove_dir(&mut self, path: &str) -> Result<(), FtpError> {
        self.expect(&["RMD ", path], 2).await.map(drop)
    }

    /// Renames `from` to `to`.
    ///
    /// # Errors
    ///
    /// [`FtpError`].
    pub async fn rename(&mut self, from: &str, to: &str) -> Result<(), FtpError> {
        self.expect(&["RNFR ", from], 3).await?;
        self.expect(&["RNTO ", to], 2).await.map(drop)
    }

    /// Says goodbye; the connection closes either way.
    pub async fn quit(mut self) {
        let _ = self.command(&["QUIT"]).await;
    }

    /// Opens a data connection and sends `command` on the control connection: the
    /// connection comes from `EPSV` (or `PASV`), to the server's own address, and turns to
    /// TLS once the server has accepted the command.
    async fn open_data(&mut self, command: &[&str]) -> Result<Data, FtpError> {
        let port = self.passive_port().await?;
        let tcp = within(
            self.timeout,
            TcpStream::connect(SocketAddr::new(self.peer, port)),
        )
        .await?
        .map_err(FtpError::Network)?;
        let reply = self.command(command).await?;
        if reply.class() != 1 {
            return Err(refused(&reply));
        }
        let Some(session) = &self.tls else {
            return Ok(Data::Plain(tcp));
        };
        let stream = within(
            self.timeout,
            session.connector.connect(session.name.clone(), tcp),
        )
        .await?
        .map_err(FtpError::Tls)?;
        // The same server on both connections, or a third party slipped in between.
        if presented(&stream)? != session.fingerprint {
            return Err(FtpError::DataCertificateMismatch);
        }
        Ok(Data::Tls(Box::new(stream)))
    }

    /// The port of a passive data connection; its host is never taken from the reply.
    async fn passive_port(&mut self) -> Result<u16, FtpError> {
        if self.features.epsv {
            let reply = self.command(&["EPSV"]).await?;
            if reply.code == 229 {
                return epsv_port(&reply.text()).ok_or_else(|| FtpError::Protocol(reply.text()));
            }
        }
        let reply = self.expect(&["PASV"], 2).await?;
        pasv_port(&reply.text()).ok_or_else(|| FtpError::Protocol(reply.text()))
    }

    /// The completion reply of a transfer: 226 or 250.
    async fn finish(&mut self) -> Result<(), FtpError> {
        let reply = self.reply().await?;
        if reply.class() == 2 {
            Ok(())
        } else {
            Err(refused(&reply))
        }
    }

    /// Sends a command made of `parts`, then reads the reply.
    async fn command(&mut self, parts: &[&str]) -> Result<Reply, FtpError> {
        let mut line = Zeroizing::new(String::new());
        for part in parts {
            if part.contains(['\r', '\n', '\0']) {
                return Err(FtpError::UnsafeArgument);
            }
            line.push_str(part);
        }
        line.push_str("\r\n");
        self.send(line.as_bytes()).await?;
        self.reply().await
    }

    /// Sends `verb` and a secret, never kept in a plain buffer.
    async fn command_secret(
        &mut self,
        verb: &str,
        secret: &Zeroizing<String>,
    ) -> Result<Reply, FtpError> {
        if secret.contains(['\r', '\n', '\0']) {
            return Err(FtpError::UnsafeArgument);
        }
        let mut line = Zeroizing::new(String::with_capacity(verb.len() + secret.len() + 2));
        line.push_str(verb);
        line.push_str(secret);
        line.push_str("\r\n");
        self.send(line.as_bytes()).await?;
        self.reply().await
    }

    async fn send(&mut self, bytes: &[u8]) -> Result<(), FtpError> {
        let control = self.control.get_mut();
        within(self.timeout, async {
            control.write_all(bytes).await?;
            control.flush().await
        })
        .await?
        .map_err(FtpError::Network)
    }

    /// The command's reply, which must be of class `class`.
    async fn expect(&mut self, parts: &[&str], class: u16) -> Result<Reply, FtpError> {
        let reply = self.command(parts).await?;
        if reply.class() == class {
            Ok(reply)
        } else {
            Err(refused(&reply))
        }
    }

    async fn reply(&mut self) -> Result<Reply, FtpError> {
        Ok(within(self.timeout, read_reply(&mut self.control)).await??)
    }
}

fn presented(stream: &TlsStream<TcpStream>) -> Result<Fingerprint, FtpError> {
    stream
        .get_ref()
        .1
        .peer_certificates()
        .and_then(|certificates| certificates.first())
        .map(|certificate| Fingerprint::of(certificate.as_ref()))
        .ok_or_else(|| FtpError::Protocol("the server presented no certificate".to_owned()))
}

fn refused(reply: &Reply) -> FtpError {
    FtpError::Refused {
        code: reply.code,
        text: reply.text(),
    }
}

async fn within<T>(limit: Duration, future: impl Future<Output = T>) -> Result<T, FtpError> {
    tokio::time::timeout(limit, future)
        .await
        .map_err(|_| FtpError::Timeout)
}

/// The port of `229 Entering Extended Passive Mode (|||6446|)`.
fn epsv_port(text: &str) -> Option<u16> {
    let inside = text.split_once('(')?.1.split_once(')')?.0;
    let delimiter = inside.chars().next()?;
    let fields: Vec<&str> = inside.split(delimiter).collect();
    // `|||port|`: empty, empty, empty, port, empty.
    (fields.len() == 5)
        .then(|| fields[3].parse().ok())
        .flatten()
}

/// The port of `227 Entering Passive Mode (h1,h2,h3,h4,p1,p2)`; the host is ignored.
fn pasv_port(text: &str) -> Option<u16> {
    let start = text.find(|c: char| c.is_ascii_digit())?;
    let numbers: Vec<u8> = text[start..]
        .split(|c: char| !c.is_ascii_digit())
        .filter(|part| !part.is_empty())
        .take(6)
        .map(str::parse)
        .collect::<Result<_, _>>()
        .ok()?;
    (numbers.len() == 6).then(|| u16::from(numbers[4]) * 256 + u16::from(numbers[5]))
}

#[cfg(test)]
mod tests {
    use super::{Fingerprint, epsv_port, pasv_port};

    #[test]
    fn passive_replies_give_only_a_port() {
        assert_eq!(
            epsv_port("Entering Extended Passive Mode (|||6446|)"),
            Some(6446)
        );
        assert_eq!(
            epsv_port("Entering Extended Passive Mode (!!!21!)"),
            Some(21)
        );
        assert_eq!(epsv_port("no parenthesis"), None);
        assert_eq!(
            pasv_port("Entering Passive Mode (10,0,0,9,19,137)."),
            Some(19 * 256 + 137)
        );
        assert_eq!(pasv_port("Entering Passive Mode (10,0,0,9,300,1)"), None);
        assert_eq!(pasv_port("Entering Passive Mode (1,2,3)"), None);
    }

    #[test]
    fn a_fingerprint_reads_back_as_written() {
        let fingerprint = Fingerprint::of(b"certificate");
        let text = fingerprint.to_string();
        assert!(text.starts_with("SHA256:"), "{text}");
        assert_eq!(text.parse::<Fingerprint>(), Ok(fingerprint));
        assert!("SHA256:!".parse::<Fingerprint>().is_err());
        assert!("MD5:abc".parse::<Fingerprint>().is_err());
    }
}
