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

//! The network side of the PING, DNS, DNSBATCH, PORTSCAN and HTTPHEADERS tools, as the C#
//! `PingService`, `DnsLookupService`, `DnsBatchResolverService`, `PortScanService` and
//! `HttpHeaderService`, and of the TCPPING, BANNER and WHOIS tools, as the C#
//! `TcpPingService`, `BannerGrabService` and `WhoisLookupService`: what they send, read and
//! run, the engines in `heimdall_core::tools` deciding what it means.
//!
//! Everything here is a future that does nothing once dropped: a tool stops a run by
//! dropping it, which closes its sockets and kills the program it started. Each is bounded
//! in time on its own; what a server or a program writes comes back made safe to show.

pub mod banner;
pub mod dns;
pub mod http;
pub mod ping;
pub mod port_scan;
pub mod tcp_ping;
pub mod whois;

#[cfg(test)]
mod route_tests;

use std::net::IpAddr;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncReadExt as _};

use crate::text::{server_text, visible_text};

/// The most of a program's output read, each stream: the rest is left unread.
pub const MAX_PROGRAM_OUTPUT: u64 = 64 * 1024;

/// The process gets no console window, as the C# `CreateNoWindow`.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// What a program said, once it ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProgramRun {
    /// It exited with success.
    pub exited_ok: bool,
    /// Its standard output, decoded as UTF-8 where it can be.
    pub stdout: String,
    /// Its standard error, decoded as UTF-8 where it can be.
    pub stderr: String,
}

/// Why a program said nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProgramFailure {
    /// It ran past its time and was killed.
    TimedOut,
    /// It did not start, or its output could not be read: why, made safe to show.
    Failed(String),
}

/// Runs `program` with `arguments`, no input and no window, within `timeout`: past it, or
/// once the future is dropped, the program is killed.
///
/// # Errors
///
/// [`ProgramFailure`].
pub async fn run_program(
    program: &Path,
    arguments: &[String],
    timeout: Duration,
) -> Result<ProgramRun, ProgramFailure> {
    let failed = |error: std::io::Error| ProgramFailure::Failed(server_text(&error.to_string()));
    let mut command = tokio::process::Command::new(program);
    command
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // Dropped when the time is up or the tool stops: killed, not left running.
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);
    let mut child = command.spawn().map_err(failed)?;
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let work = async move {
        let (stdout, stderr) = tokio::try_join!(read_capped(stdout), read_capped(stderr))?;
        let status = child.wait().await?;
        Ok::<_, std::io::Error>(ProgramRun {
            exited_ok: status.success(),
            stdout: String::from_utf8_lossy(&stdout).into_owned(),
            stderr: String::from_utf8_lossy(&stderr).into_owned(),
        })
    };
    match tokio::time::timeout(timeout, work).await {
        Ok(run) => run.map_err(failed),
        Err(_) => Err(ProgramFailure::TimedOut),
    }
}

/// At most [`MAX_PROGRAM_OUTPUT`] bytes of `stream`; its end closed after them, so a
/// program that writes on has its pipe broken rather than filled.
async fn read_capped(stream: Option<impl AsyncRead + Unpin>) -> std::io::Result<Vec<u8>> {
    let mut read = Vec::new();
    if let Some(stream) = stream {
        stream
            .take(MAX_PROGRAM_OUTPUT)
            .read_to_end(&mut read)
            .await?;
    }
    Ok(read)
}

/// Why a host name gave no address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolveFailure {
    /// No answer within the time given.
    TimedOut,
    /// The resolver answered with no address.
    NoAddress,
    /// The resolver failed: why, made safe to show.
    Failed(String),
}

/// The addresses `host` resolves to, through the system's resolver, in the order it gives
/// them, each once; an address literal as it is. Within `timeout`.
///
/// # Errors
///
/// [`ResolveFailure`].
pub async fn resolve(host: &str, timeout: Duration) -> Result<Vec<IpAddr>, ResolveFailure> {
    if let Ok(address) = host.parse::<IpAddr>() {
        return Ok(vec![address]);
    }
    let found = tokio::time::timeout(timeout, tokio::net::lookup_host((host, 0)))
        .await
        .map_err(|_| ResolveFailure::TimedOut)?
        .map_err(|error| ResolveFailure::Failed(server_text(&error.to_string())))?;
    let mut addresses: Vec<IpAddr> = Vec::new();
    for address in found.map(|socket| socket.ip()) {
        if !addresses.contains(&address) {
            addresses.push(address);
        }
    }
    if addresses.is_empty() {
        return Err(ResolveFailure::NoAddress);
    }
    Ok(addresses)
}

/// `text`, sent by a server or a program, one line each, made safe to show: every
/// character that would not show as itself written out, nothing removed or cut.
#[must_use]
pub fn shown_lines(text: &str) -> Vec<String> {
    text.lines().map(visible_text).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn an_address_literal_is_not_looked_up() {
        assert_eq!(
            resolve("2001:db8::1", Duration::from_millis(1)).await,
            Ok(vec!["2001:db8::1".parse().unwrap()])
        );
    }

    #[tokio::test]
    async fn a_name_that_does_not_resolve_is_said_as_such() {
        // `.invalid` never resolves (RFC 2606).
        assert!(matches!(
            resolve("nothing.invalid", Duration::from_secs(10)).await,
            Err(ResolveFailure::Failed(_) | ResolveFailure::NoAddress)
        ));
    }

    #[test]
    fn what_a_server_writes_shows_as_it_is_without_its_controls() {
        assert_eq!(
            shown_lines("a\u{1b}[31mb\r\nc\u{202e}d\n"),
            ["a\\u{001B}[31mb", "c\\u{202E}d"]
        );
    }

    #[tokio::test]
    async fn a_program_that_does_not_exist_is_said_as_such() {
        let missing = Path::new("/nonexistent/heimdall-no-such-program");
        assert!(matches!(
            run_program(missing, &[], Duration::from_secs(5)).await,
            Err(ProgramFailure::Failed(_))
        ));
    }
}
