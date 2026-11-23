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

//! An FTP server for the tests of the Files session, run in the test on a temporary folder:
//! one server shared by the tests of `heimdall-files` and `heimdall-app`, in place of an
//! FTP server crate (the one used before pulled in an unmaintained time library).
//!
//! It serves exactly what those tests exercise, as a passive-mode server with explicit FTPS:
//! `USER`, `PASS`, `AUTH TLS`, `PBSZ`, `PROT`, `FEAT`, `TYPE`, `PWD`, `CWD`, `PASV`, `REST`,
//! `MLSD`, `SIZE`, `MDTM`, `RETR`, `STOR`, `DELE`, `MKD`, `RMD`, `RNFR`, `RNTO` and `QUIT`.
//! Any other command is answered `502`. Any user and password are accepted, as the server
//! it replaces did by default.
//!
//! The listener is bound before [`serve`] returns, so a connection made then is queued
//! until the server accepts it: no wait for the server to start. A port picked free, let
//! go, then bound again by the server could be taken in between by a test running beside
//! it, and the test would talk to that test's server.

use std::fmt::Write as _;
use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tokio::fs::{self, File, OpenOptions};
use tokio::io::{
    AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncSeekExt, AsyncWrite, AsyncWriteExt, BufReader,
};
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::TlsAcceptor;
use tokio_rustls::rustls::ServerConfig;
use tokio_rustls::rustls::crypto::ring::default_provider;
use tokio_rustls::rustls::pki_types::pem::PemObject;
use tokio_rustls::rustls::pki_types::{CertificateDer, PrivateKeyDer};

/// Longest wait for the client to connect to a data port it was told of.
const DATA_ACCEPT_TIMEOUT: Duration = Duration::from_secs(30);

/// Bytes read from a data connection at a time.
const CHUNK: usize = 64 * 1024;

/// The first reply line of `FEAT`, and its last: the client reads up to the line starting
/// with `211 `.
const FEAT_HEADER: &str = "211-Extensions supported:\r\n";
const FEAT_FOOTER: &str = "211 END\r\n";

/// What `FEAT` lists: the machine listing the client asks for with `MLSD`, resuming, sizes
/// and times. `AUTH TLS` is added when the server has a certificate.
const FEATURES: [&str; 5] = [" MDTM", " MLST", " REST STREAM", " SIZE", " UTF8"];
const FEATURE_AUTH_TLS: &str = " AUTH TLS";

/// The facts of an `MLSD` line end with this, then the name.
const FACT_END: &str = "; ";

/// Reply codes, as RFC 959, RFC 2228 and RFC 3659 give them.
const OPENING_DATA: u16 = 150;
const COMMAND_OK: u16 = 200;
const FILE_STATUS: u16 = 213;
const READY: u16 = 220;
const CLOSING: u16 = 221;
const TRANSFER_COMPLETE: u16 = 226;
const PASSIVE: u16 = 227;
const LOGGED_IN: u16 = 230;
const AUTH_OK: u16 = 234;
const FILE_ACTION_OK: u16 = 250;
const PATH_CREATED: u16 = 257;
const NEED_PASSWORD: u16 = 331;
const PENDING_FILE_ACTION: u16 = 350;
const NO_DATA_CONNECTION: u16 = 425;
const TRANSFER_ABORTED: u16 = 426;
const NOT_UNDERSTOOD: u16 = 502;
const BAD_SEQUENCE: u16 = 503;
const NOT_IMPLEMENTED_FOR_PARAMETER: u16 = 504;
const FILE_UNAVAILABLE: u16 = 550;

/// Seconds in a day, an hour and a minute, to read a time as a civil date.
const DAY_SECONDS: u64 = 86_400;
const HOUR_SECONDS: u64 = 3_600;
const MINUTE_SECONDS: u64 = 60;

/// Days from 0000-03-01 to 1970-01-01, and the days and years of one cycle of the civil
/// calendar, for [`civil`] (Howard Hinnant's algorithm).
const EPOCH_SHIFT_DAYS: i64 = 719_468;
const ERA_DAYS: i64 = 146_097;
const ERA_YEARS: i64 = 400;

/// A port is given in a passive reply as two numbers, high and low byte.
const BYTE: u16 = 256;

/// A connection, plain or TLS.
trait Io: AsyncRead + AsyncWrite + Send + Sync + Unpin {}

impl<T: AsyncRead + AsyncWrite + Send + Sync + Unpin> Io for T {}

type Stream = Box<dyn Io>;

/// Serves `root` over FTP, over explicit FTPS with the certificate and key files `ftps`
/// names when given, on a free local port; returns the port.
///
/// # Panics
///
/// When no port can be bound, or the certificate and key cannot be read: the test fails
/// there rather than in a task.
pub async fn serve(root: &Path, ftps: Option<(PathBuf, PathBuf)>) -> u16 {
    let acceptor = ftps.map(|(cert, key)| acceptor(&cert, &key));
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .await
        .expect("free port");
    let port = listener.local_addr().expect("address").port();
    let root = root.to_owned();
    tokio::spawn(async move {
        loop {
            // A connection that fails before it is accepted is the client's affair.
            if let Ok((stream, _)) = listener.accept().await {
                let session = Session::new(root.clone(), acceptor.clone(), stream);
                tokio::spawn(async move {
                    // A client that goes away ends its session, whatever it was doing.
                    let _ = session.run().await;
                });
            }
        }
    });
    port
}

/// The TLS acceptor of the certificate and key files.
fn acceptor(cert: &Path, key: &Path) -> TlsAcceptor {
    let certificates: Vec<CertificateDer<'static>> = CertificateDer::pem_file_iter(cert)
        .expect("certificate file")
        .collect::<Result<_, _>>()
        .expect("certificate");
    let key = PrivateKeyDer::from_pem_file(key).expect("key");
    let config = ServerConfig::builder_with_provider(Arc::new(default_provider()))
        .with_safe_default_protocol_versions()
        .expect("protocol versions")
        .with_no_client_auth()
        .with_single_cert(certificates, key)
        .expect("certificate and key");
    TlsAcceptor::from(Arc::new(config))
}

/// One client's control connection.
struct Session {
    /// The served folder: the root of every path the client names.
    root: PathBuf,
    /// The current folder, as the names from the root.
    folder: Vec<String>,
    /// The control connection; none only while it is being secured.
    control: Option<BufReader<Stream>>,
    /// The address the client reached the server on, the one passive ports are opened on.
    local: IpAddr,
    /// TLS, when the server has a certificate.
    acceptor: Option<TlsAcceptor>,
    /// Whether `PROT P` asked for the data connections to be secured.
    secure_data: bool,
    /// The port opened by `PASV`, waiting for the client's data connection.
    passive: Option<TcpListener>,
    /// The offset `REST` asked the next transfer to start at.
    restart: u64,
    /// The path `RNFR` named, waiting for `RNTO`.
    rename_from: Option<PathBuf>,
}

impl Session {
    fn new(root: PathBuf, acceptor: Option<TlsAcceptor>, stream: TcpStream) -> Self {
        let local = stream
            .local_addr()
            .map_or(IpAddr::V4(Ipv4Addr::LOCALHOST), |address| address.ip());
        Self {
            root,
            folder: Vec::new(),
            control: Some(BufReader::new(Box::new(stream))),
            local,
            acceptor,
            secure_data: false,
            passive: None,
            restart: 0,
            rename_from: None,
        }
    }

    /// Greets the client, then answers its commands until it quits or goes away.
    async fn run(mut self) -> io::Result<()> {
        self.reply(READY, "ready").await?;
        loop {
            let mut line = Vec::new();
            let read = self.control()?.read_until(b'\n', &mut line).await?;
            if read == 0 {
                return Ok(());
            }
            let line = String::from_utf8_lossy(&line);
            let line = line.trim_end_matches(['\r', '\n']);
            let (verb, argument) = line.split_once(' ').unwrap_or((line, ""));
            if !self.command(&verb.to_ascii_uppercase(), argument).await? {
                return Ok(());
            }
        }
    }

    fn control(&mut self) -> io::Result<&mut BufReader<Stream>> {
        self.control
            .as_mut()
            .ok_or_else(|| io::Error::from(io::ErrorKind::NotConnected))
    }

    async fn reply(&mut self, code: u16, text: &str) -> io::Result<()> {
        self.send(&format!("{code} {text}\r\n")).await
    }

    async fn send(&mut self, text: &str) -> io::Result<()> {
        let control = self.control()?.get_mut();
        control.write_all(text.as_bytes()).await?;
        control.flush().await
    }

    /// Answers one command; false once the session is over.
    async fn command(&mut self, verb: &str, argument: &str) -> io::Result<bool> {
        match verb {
            "USER" => self.reply(NEED_PASSWORD, "password needed").await?,
            "PASS" => self.reply(LOGGED_IN, "logged in").await?,
            "AUTH" => self.authenticate(argument).await?,
            "PBSZ" => self.reply(COMMAND_OK, "PBSZ=0").await?,
            "PROT" => self.protect(argument).await?,
            "FEAT" => self.features().await?,
            "TYPE" => self.reply(COMMAND_OK, "type set").await?,
            "PWD" => {
                let here = absolute(&self.folder);
                self.reply(PATH_CREATED, &format!("\"{here}\" is the current folder"))
                    .await?;
            }
            "CWD" => self.change_folder(argument).await?,
            "PASV" => self.open_passive().await?,
            "REST" => match argument.trim().parse() {
                Ok(offset) => {
                    self.restart = offset;
                    self.reply(PENDING_FILE_ACTION, "restarting").await?;
                }
                Err(_) => {
                    self.reply(NOT_IMPLEMENTED_FOR_PARAMETER, "bad offset")
                        .await?;
                }
            },
            "MLSD" => self.list(argument).await?,
            "SIZE" => self.size(argument).await?,
            "MDTM" => self.modified(argument).await?,
            "RETR" => self.download(argument).await?,
            "STOR" => self.upload(argument).await?,
            "DELE" => self.delete(argument).await?,
            "MKD" => self.make_folder(argument).await?,
            "RMD" => self.remove_folder(argument).await?,
            "RNFR" => self.rename_start(argument).await?,
            "RNTO" => self.rename_end(argument).await?,
            "QUIT" => {
                self.reply(CLOSING, "bye").await?;
                return Ok(false);
            }
            _ => self.reply(NOT_UNDERSTOOD, "not understood").await?,
        }
        Ok(true)
    }

    /// `AUTH TLS`: the control connection secured, when the server has a certificate.
    async fn authenticate(&mut self, mechanism: &str) -> io::Result<()> {
        let Some(acceptor) = self.acceptor.clone() else {
            return self.reply(NOT_UNDERSTOOD, "no TLS here").await;
        };
        if !mechanism.eq_ignore_ascii_case("TLS") {
            return self.reply(NOT_IMPLEMENTED_FOR_PARAMETER, "TLS only").await;
        }
        self.reply(AUTH_OK, "go ahead").await?;
        // The client waits for the reply before it starts the handshake: nothing is buffered.
        let plain = self
            .control
            .take()
            .ok_or_else(|| io::Error::from(io::ErrorKind::NotConnected))?
            .into_inner();
        let secured = acceptor.accept(plain).await?;
        self.control = Some(BufReader::new(Box::new(secured)));
        Ok(())
    }

    /// `PROT P` or `PROT C`: whether the data connections are secured.
    async fn protect(&mut self, level: &str) -> io::Result<()> {
        match level.to_ascii_uppercase().as_str() {
            "P" if self.acceptor.is_some() => {
                self.secure_data = true;
                self.reply(COMMAND_OK, "data connections are private").await
            }
            "C" => {
                self.secure_data = false;
                self.reply(COMMAND_OK, "data connections are clear").await
            }
            _ => {
                self.reply(NOT_IMPLEMENTED_FOR_PARAMETER, "no such level")
                    .await
            }
        }
    }

    async fn features(&mut self) -> io::Result<()> {
        let mut text = String::from(FEAT_HEADER);
        for line in FEATURES {
            text.push_str(line);
            text.push_str("\r\n");
        }
        if self.acceptor.is_some() {
            text.push_str(FEATURE_AUTH_TLS);
            text.push_str("\r\n");
        }
        text.push_str(FEAT_FOOTER);
        self.send(&text).await
    }

    /// `CWD`: the current folder changed to a folder of the served tree.
    async fn change_folder(&mut self, argument: &str) -> io::Result<()> {
        let names = self.names(argument);
        let is_folder = match self.real(&names) {
            Some(path) => fs::metadata(path).await.is_ok_and(|meta| meta.is_dir()),
            None => false,
        };
        if is_folder {
            self.folder = names;
            self.reply(FILE_ACTION_OK, "folder changed").await
        } else {
            self.reply(FILE_UNAVAILABLE, "no such folder").await
        }
    }

    /// `PASV`: a port opened for the next transfer, on a port no other socket holds.
    ///
    /// Bound with port 0 by tokio's own listener, which sets no `SO_REUSEADDR` on Windows.
    /// With that option set, as the server this one replaces set it on a random port, two
    /// sockets can listen on one port there, every connection going to the first: a data
    /// connection of one test could reach another test's server, the transfer never reaching
    /// its own, and the client would wait for its reply until the session timed out (CI run
    /// 38892908592). Linux refuses the second bind instead.
    async fn open_passive(&mut self) -> io::Result<()> {
        let IpAddr::V4(address) = self.local else {
            return self.reply(NOT_IMPLEMENTED_FOR_PARAMETER, "IPv4 only").await;
        };
        let listener = TcpListener::bind(SocketAddr::new(IpAddr::V4(address), 0)).await?;
        let port = listener.local_addr()?.port();
        self.passive = Some(listener);
        let [a, b, c, d] = address.octets();
        let (high, low) = (port / BYTE, port % BYTE);
        self.reply(
            PASSIVE,
            &format!("entering passive mode ({a},{b},{c},{d},{high},{low})"),
        )
        .await
    }

    /// `MLSD`: the entries of a folder, one line of facts each.
    async fn list(&mut self, argument: &str) -> io::Result<()> {
        let listener = self.passive.take();
        let folder = if argument.is_empty() {
            self.real(&self.folder)
        } else {
            self.real(&self.names(argument))
        };
        let Some(mut entries) = (match folder {
            Some(folder) => fs::read_dir(folder).await.ok(),
            None => None,
        }) else {
            return self.reply(FILE_UNAVAILABLE, "no such folder").await;
        };
        let mut text = String::new();
        while let Ok(Some(entry)) = entries.next_entry().await {
            // The entry itself, a link not followed: it is listed as what it is.
            let Ok(meta) = entry.metadata().await else {
                continue;
            };
            let kind = if meta.is_dir() { "dir" } else { "file" };
            let _ = write!(text, "type={kind};size={}", meta.len());
            if let Ok(modified) = meta.modified() {
                let _ = write!(text, ";modify={}", timestamp(modified));
            }
            text.push_str(FACT_END);
            text.push_str(&entry.file_name().to_string_lossy());
            text.push_str("\r\n");
        }
        self.reply(OPENING_DATA, "listing").await?;
        let Some(mut data) = self.accept(listener).await else {
            return self.reply(NO_DATA_CONNECTION, "no data connection").await;
        };
        let sent = async {
            data.write_all(text.as_bytes()).await?;
            data.shutdown().await
        }
        .await;
        self.finish(sent.is_ok()).await
    }

    /// `SIZE`: the length of a regular file.
    async fn size(&mut self, argument: &str) -> io::Result<()> {
        match self.file_meta(argument).await {
            Some(meta) => self.reply(FILE_STATUS, &meta.len().to_string()).await,
            None => self.reply(FILE_UNAVAILABLE, "no such file").await,
        }
    }

    /// `MDTM`: the time a regular file was last written, in UTC.
    async fn modified(&mut self, argument: &str) -> io::Result<()> {
        match self
            .file_meta(argument)
            .await
            .and_then(|meta| meta.modified().ok())
        {
            Some(modified) => self.reply(FILE_STATUS, &timestamp(modified)).await,
            None => self.reply(FILE_UNAVAILABLE, "no such file").await,
        }
    }

    /// The metadata of the regular file `argument` names.
    async fn file_meta(&self, argument: &str) -> Option<std::fs::Metadata> {
        let path = self.real(&self.names(argument))?;
        fs::metadata(path)
            .await
            .ok()
            .filter(std::fs::Metadata::is_file)
    }

    /// `RETR`: a file sent, from the offset `REST` gave.
    async fn download(&mut self, argument: &str) -> io::Result<()> {
        let listener = self.passive.take();
        let offset = std::mem::take(&mut self.restart);
        let opened = async {
            let path = self.real(&self.names(argument))?;
            let mut file = File::open(path).await.ok()?;
            if !file.metadata().await.ok()?.is_file() {
                return None;
            }
            file.seek(io::SeekFrom::Start(offset)).await.ok()?;
            Some(file)
        }
        .await;
        let Some(mut file) = opened else {
            return self.reply(FILE_UNAVAILABLE, "no such file").await;
        };
        self.reply(OPENING_DATA, "sending").await?;
        let Some(mut data) = self.accept(listener).await else {
            return self.reply(NO_DATA_CONNECTION, "no data connection").await;
        };
        let sent = async {
            tokio::io::copy(&mut file, &mut data).await?;
            data.shutdown().await
        }
        .await;
        self.finish(sent.is_ok()).await
    }

    /// `STOR`: a file received, replacing what is there.
    async fn upload(&mut self, argument: &str) -> io::Result<()> {
        let listener = self.passive.take();
        self.restart = 0;
        let created = async {
            let path = self.real(&self.names(argument))?;
            OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(path)
                .await
                .ok()
        }
        .await;
        let Some(mut file) = created else {
            return self.reply(FILE_UNAVAILABLE, "cannot store there").await;
        };
        self.reply(OPENING_DATA, "receiving").await?;
        let Some(mut data) = self.accept(listener).await else {
            return self.reply(NO_DATA_CONNECTION, "no data connection").await;
        };
        let received = async {
            let mut chunk = vec![0; CHUNK];
            loop {
                // A TLS stream that ends without its close_notify is an error, so a cut
                // upload is aborted (426), as libunftp and vsftpd did.
                let read = data.read(&mut chunk).await?;
                if read == 0 {
                    break;
                }
                file.write_all(&chunk[..read]).await?;
            }
            file.flush().await?;
            // Closed on this side too: the client waits for it before it reads the reply.
            data.shutdown().await
        }
        .await;
        drop(file);
        self.finish(received.is_ok()).await
    }

    /// The reply that ends a transfer.
    async fn finish(&mut self, done: bool) -> io::Result<()> {
        if done {
            self.reply(TRANSFER_COMPLETE, "transfer complete").await
        } else {
            self.reply(TRANSFER_ABORTED, "transfer aborted").await
        }
    }

    /// The data connection the client opens on the port `PASV` gave, secured when asked.
    async fn accept(&self, listener: Option<TcpListener>) -> Option<Stream> {
        let (stream, _) = tokio::time::timeout(DATA_ACCEPT_TIMEOUT, listener?.accept())
            .await
            .ok()?
            .ok()?;
        if self.secure_data {
            let secured = self.acceptor.clone()?.accept(stream).await.ok()?;
            Some(Box::new(secured))
        } else {
            Some(Box::new(stream))
        }
    }

    /// `DELE`: a file deleted; a folder is not one.
    async fn delete(&mut self, argument: &str) -> io::Result<()> {
        let done = match self.real(&self.names(argument)) {
            Some(path) => fs::remove_file(path).await.is_ok(),
            None => false,
        };
        self.answer(done, FILE_ACTION_OK, "deleted").await
    }

    /// `MKD`: a folder made.
    async fn make_folder(&mut self, argument: &str) -> io::Result<()> {
        let names = self.names(argument);
        let done = match self.real(&names) {
            Some(path) => fs::create_dir(path).await.is_ok(),
            None => false,
        };
        let made = absolute(&names);
        self.answer(done, PATH_CREATED, &format!("\"{made}\" created"))
            .await
    }

    /// `RMD`: an empty folder removed.
    async fn remove_folder(&mut self, argument: &str) -> io::Result<()> {
        let done = match self.real(&self.names(argument)) {
            Some(path) => fs::remove_dir(path).await.is_ok(),
            None => false,
        };
        self.answer(done, FILE_ACTION_OK, "removed").await
    }

    /// `RNFR`: the entry to rename, which must be there.
    async fn rename_start(&mut self, argument: &str) -> io::Result<()> {
        self.rename_from = None;
        let found = match self.real(&self.names(argument)) {
            Some(path) => fs::symlink_metadata(&path).await.is_ok().then_some(path),
            None => None,
        };
        if found.is_none() {
            return self.reply(FILE_UNAVAILABLE, "no such entry").await;
        }
        self.rename_from = found;
        self.reply(PENDING_FILE_ACTION, "ready for the new name")
            .await
    }

    /// `RNTO`: the entry `RNFR` named renamed, as the system renames: a file is replaced.
    async fn rename_end(&mut self, argument: &str) -> io::Result<()> {
        let Some(from) = self.rename_from.take() else {
            return self.reply(BAD_SEQUENCE, "RNFR first").await;
        };
        let done = match self.real(&self.names(argument)) {
            Some(to) => fs::rename(from, to).await.is_ok(),
            None => false,
        };
        self.answer(done, FILE_ACTION_OK, "renamed").await
    }

    /// `ok` when `done`, the refusal of a file action otherwise.
    async fn answer(&mut self, done: bool, ok: u16, text: &str) -> io::Result<()> {
        if done {
            self.reply(ok, text).await
        } else {
            self.reply(FILE_UNAVAILABLE, "not done").await
        }
    }

    /// The names from the root of the path `argument` gives, absolute or from the current
    /// folder; `.` and `..` resolved, never above the root.
    fn names(&self, argument: &str) -> Vec<String> {
        let mut names = if argument.starts_with('/') {
            Vec::new()
        } else {
            self.folder.clone()
        };
        for name in argument.split('/') {
            match name {
                "" | "." => {}
                ".." => {
                    names.pop();
                }
                other => names.push(other.to_owned()),
            }
        }
        names
    }

    /// The path in the served folder of `names`; none for a name that is not one plain
    /// segment of this system (a separator, a drive, a device), which cannot be asked for.
    fn real(&self, names: &[String]) -> Option<PathBuf> {
        let mut path = self.root.clone();
        for name in names {
            let mut parts = Path::new(name).components();
            let (Some(Component::Normal(part)), None) = (parts.next(), parts.next()) else {
                return None;
            };
            if part != name.as_str() {
                return None;
            }
            path.push(part);
        }
        Some(path)
    }
}

/// `names` as the client writes a path: absolute, from the root.
fn absolute(names: &[String]) -> String {
    format!("/{}", names.join("/"))
}

/// `time` as `MDTM` and `MLSD` write it: `YYYYMMDDHHMMSS`, in UTC.
fn timestamp(time: SystemTime) -> String {
    let seconds = time
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs());
    let (year, month, day) = civil(i64::try_from(seconds / DAY_SECONDS).unwrap_or(0));
    let of_day = seconds % DAY_SECONDS;
    format!(
        "{year:04}{month:02}{day:02}{:02}{:02}{:02}",
        of_day / HOUR_SECONDS,
        of_day % HOUR_SECONDS / MINUTE_SECONDS,
        of_day % MINUTE_SECONDS
    )
}

/// The year, month and day of `days` since 1970-01-01 (Howard Hinnant's `civil_from_days`).
fn civil(days: i64) -> (i64, i64, i64) {
    let shifted = days + EPOCH_SHIFT_DAYS;
    let era = shifted.div_euclid(ERA_DAYS);
    let day_of_era = shifted.rem_euclid(ERA_DAYS);
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * shifted_month + 2) / 5 + 1;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    };
    let year = year_of_era + era * ERA_YEARS + i64::from(month <= 2);
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, UNIX_EPOCH};

    use super::{civil, timestamp};

    #[test]
    fn a_day_count_becomes_a_civil_date() {
        assert_eq!(civil(0), (1970, 1, 1));
        assert_eq!(civil(-1), (1969, 12, 31));
        assert_eq!(civil(11_017), (2000, 3, 1));
        // A leap day.
        assert_eq!(civil(19_782), (2024, 2, 29));
    }

    #[test]
    fn a_time_is_written_as_mdtm_does() {
        assert_eq!(timestamp(UNIX_EPOCH), "19700101000000");
        assert_eq!(
            timestamp(UNIX_EPOCH + Duration::from_secs(1_709_210_096)),
            "20240229123456"
        );
    }
}
