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

//! The client against OpenSSH's `sftp-server` on standard input and output. Lookup and
//! skipping as in `openssh_codec.rs`.

#![cfg(unix)]

use std::os::unix::ffi::OsStrExt as _;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use heimdall_sftp::protocol::{Attributes, StatusCode, open_flags};
use heimdall_sftp::{ClientConfig, RemotePath, SftpClient, SftpError};
use tokio::process::{Child, Command};

const SERVER_VARIABLE: &str = "HEIMDALL_SFTP_SERVER";
const REQUIRE_VARIABLE: &str = "HEIMDALL_REQUIRE_SFTP_SERVER";
const KNOWN_PATHS: [&str; 2] = [
    "/usr/libexec/openssh/sftp-server",
    "/usr/lib/openssh/sftp-server",
];

/// Bound on each step.
const STEP: Duration = Duration::from_secs(30);

/// Size of the file read with many requests in flight.
const BIG_FILE: usize = 2 * 1024 * 1024 + 123;

/// Bytes per read request in the concurrent read.
const CHUNK: u32 = 32 * 1024;

async fn start() -> Option<(Child, SftpClient)> {
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
    let client = tokio::time::timeout(
        STEP,
        SftpClient::start(tokio::io::join(output, input), ClientConfig::default()),
    )
    .await
    .expect("in time")
    .expect("started");
    Some((child, client))
}

fn remote(path: &Path) -> RemotePath {
    RemotePath::from_bytes(path.as_os_str().as_bytes())
}

async fn step<T>(future: impl std::future::Future<Output = T>) -> T {
    tokio::time::timeout(STEP, future).await.expect("in time")
}

#[tokio::test]
async fn files_and_directories_round_trip_with_a_latin1_name() {
    let Some((_server, client)) = start().await else {
        return;
    };
    assert!(client.limits().max_read > 0, "OpenSSH announces its limits");
    let dir = tempfile::tempdir().expect("dir");
    let root = remote(dir.path());
    assert_eq!(step(client.realpath(&root)).await.expect("realpath"), root);

    let folder = root.join(b"d\xE9p\xF4t");
    step(client.mkdir(&folder, Attributes::default()))
        .await
        .expect("mkdir");
    let file = folder.join(b"caf\xE9.txt");
    let handle = step(client.open(
        &file,
        open_flags::WRITE | open_flags::CREATE | open_flags::EXCLUSIVE,
        Attributes::default(),
    ))
    .await
    .expect("created");
    step(client.write(&handle, 0, b"bonjour ".to_vec()))
        .await
        .expect("written");
    step(client.write(&handle, 8, b"monde".to_vec()))
        .await
        .expect("written");
    step(client.close(&handle)).await.expect("closed");

    let listed = step(client.read_dir(&folder)).await.expect("listed");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].name, b"caf\xE9.txt");
    assert_eq!(listed[0].attributes.size, Some(13));

    let handle = step(client.open(&file, open_flags::READ, Attributes::default()))
        .await
        .expect("opened");
    let data = step(client.read(&handle, 0, 64)).await.expect("read");
    assert_eq!(data.as_deref(), Some(&b"bonjour monde"[..]));
    assert_eq!(step(client.read(&handle, 13, 64)).await.expect("eof"), None);
    step(client.close(&handle)).await.expect("closed");

    let other = folder.join(b"autre");
    step(client.rename(&file, &other, false))
        .await
        .expect("renamed");
    let second = step(client.open(
        &file,
        open_flags::WRITE | open_flags::CREATE,
        Attributes::default(),
    ))
    .await
    .expect("created again");
    step(client.close(&second)).await.expect("closed");
    let refused = step(client.rename(&file, &other, false))
        .await
        .expect_err("a plain rename keeps an existing target");
    assert!(matches!(refused, SftpError::Status { .. }), "{refused:?}");
    step(client.rename(&file, &other, true))
        .await
        .expect("posix-rename replaces it");

    let missing = step(client.stat(&file)).await.expect_err("gone");
    assert!(
        matches!(
            missing,
            SftpError::Status {
                code: StatusCode::NoSuchFile,
                ..
            }
        ),
        "{missing:?}"
    );
    step(client.remove(&other)).await.expect("removed");
    step(client.rmdir(&folder)).await.expect("rmdir");
    assert!(
        std::fs::read_dir(dir.path())
            .expect("readable")
            .next()
            .is_none()
    );
}

#[tokio::test]
async fn many_reads_in_flight_reassemble_the_file() {
    let Some((_server, client)) = start().await else {
        return;
    };
    let dir = tempfile::tempdir().expect("dir");
    let local = dir.path().join("big.bin");
    let content: Vec<u8> = (0..BIG_FILE)
        .map(|n| u8::try_from(n % 251).expect("below 251"))
        .collect();
    std::fs::write(&local, &content).expect("written");

    let handle = step(client.open(&remote(&local), open_flags::READ, Attributes::default()))
        .await
        .expect("opened");
    let mut reads = tokio::task::JoinSet::new();
    for offset in (0..content.len() as u64).step_by(CHUNK as usize) {
        let (client, handle) = (client.clone(), handle.clone());
        reads.spawn(async move { (offset, client.read(&handle, offset, CHUNK).await) });
    }
    let mut assembled = vec![0; content.len()];
    while let Some(joined) = step(reads.join_next()).await {
        let (offset, read) = joined.expect("joined");
        let data = read.expect("read").expect("not past the end");
        let start = usize::try_from(offset).expect("small");
        assembled[start..start + data.len()].copy_from_slice(&data);
    }
    step(client.close(&handle)).await.expect("closed");
    assert!(assembled == content, "every chunk landed at its offset");
}
