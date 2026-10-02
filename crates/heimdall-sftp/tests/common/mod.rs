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

//! OpenSSH's `sftp-server` on standard input and output, for the tests that need the
//! reference server. Looked up at `HEIMDALL_SFTP_SERVER`, then at the usual Linux paths;
//! absent, the test says so and passes, unless `HEIMDALL_REQUIRE_SFTP_SERVER` is set.

#![cfg(unix)]
#![allow(dead_code, reason = "each test file uses part of this module")]

use std::os::unix::ffi::OsStrExt as _;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use heimdall_sftp::{ClientConfig, RemotePath, SftpClient};
use tokio::process::{Child, Command};

const SERVER_VARIABLE: &str = "HEIMDALL_SFTP_SERVER";
const REQUIRE_VARIABLE: &str = "HEIMDALL_REQUIRE_SFTP_SERVER";
const KNOWN_PATHS: [&str; 2] = [
    "/usr/libexec/openssh/sftp-server",
    "/usr/lib/openssh/sftp-server",
];

/// Bound on each step.
pub const STEP: Duration = Duration::from_secs(60);

/// The `sftp-server` to run, or `None` when there is none and none is required.
fn server_binary() -> Option<PathBuf> {
    let binary = std::env::var_os(SERVER_VARIABLE)
        .map(PathBuf::from)
        .or_else(|| {
            KNOWN_PATHS
                .iter()
                .map(PathBuf::from)
                .find(|path| path.is_file())
        });
    let Some(binary) = binary else {
        assert!(
            std::env::var_os(REQUIRE_VARIABLE).is_none(),
            "{REQUIRE_VARIABLE} is set and no sftp-server was found"
        );
        eprintln!("no sftp-server found; set {SERVER_VARIABLE} to run this test");
        return None;
    };
    Some(binary)
}

/// A client connected to a fresh `sftp-server`; the server lives as long as the child.
pub async fn start() -> Option<(Child, SftpClient)> {
    let binary = server_binary()?;
    let mut child = Command::new(binary)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .expect("sftp-server starts");
    let output = child.stdout.take().expect("stdout");
    let input = child.stdin.take().expect("stdin");
    let client = step(SftpClient::start(
        tokio::io::join(output, input),
        ClientConfig::default(),
    ))
    .await
    .expect("started");
    Some((child, client))
}

/// The remote form of a local path: `sftp-server` runs on this machine.
pub fn remote(path: &Path) -> RemotePath {
    RemotePath::from_bytes(path.as_os_str().as_bytes())
}

/// `future`, bounded by [`STEP`].
pub async fn step<T>(future: impl std::future::Future<Output = T>) -> T {
    tokio::time::timeout(STEP, future).await.expect("in time")
}

/// Bytes that differ at every offset of a large file.
pub fn pattern(length: usize) -> Vec<u8> {
    (0..length)
        .map(|n| u8::try_from(n % 251).expect("below 251"))
        .collect()
}

/// A client connected to a fresh `sftp-server` that announces no extension: the server's
/// version reply is rewritten on its way, everything else passes as it is. Stands for a
/// server without `posix-rename@openssh.com`, `fsync@openssh.com` or `limits@openssh.com`.
pub async fn start_without_extensions() -> Option<(Child, SftpClient)> {
    use heimdall_sftp::protocol::Response;
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

    let binary = server_binary()?;
    let mut child = Command::new(binary)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .expect("sftp-server starts");
    let mut from_server = child.stdout.take().expect("stdout");
    let mut to_server = child.stdin.take().expect("stdin");
    let (client_side, relay) = tokio::io::duplex(1 << 20);
    let (mut from_client, mut to_client) = tokio::io::split(relay);
    tokio::spawn(async move {
        let _ = tokio::io::copy(&mut from_client, &mut to_server).await;
    });
    tokio::spawn(async move {
        let mut length = [0; 4];
        if from_server.read_exact(&mut length).await.is_err() {
            return;
        }
        let mut body = vec![0; u32::from_be_bytes(length) as usize];
        if from_server.read_exact(&mut body).await.is_err() {
            return;
        }
        let Ok(Response::Version { version, .. }) = Response::decode(&body) else {
            return;
        };
        let bare = Response::Version {
            version,
            extensions: Vec::new(),
        };
        if to_client.write_all(&bare.encode()).await.is_err() {
            return;
        }
        let _ = tokio::io::copy(&mut from_server, &mut to_client).await;
    });
    let client = step(SftpClient::start(client_side, ClientConfig::default()))
        .await
        .expect("started");
    Some((child, client))
}
