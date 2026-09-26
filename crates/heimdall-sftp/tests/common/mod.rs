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

/// A client connected to a fresh `sftp-server`; the server lives as long as the child.
pub async fn start() -> Option<(Child, SftpClient)> {
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
