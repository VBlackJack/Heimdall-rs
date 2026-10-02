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

//! What the Files tab asks of a server, against OpenSSH's `sftp-server` on standard input
//! and output, as heimdall-sftp's tests run it: skipped where there is none, unless
//! `HEIMDALL_REQUIRE_SFTP_SERVER` is set.

#![cfg(unix)]

use std::os::unix::ffi::OsStrExt as _;
use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use heimdall_files::{ItemKind, RemoteSession};
use heimdall_sftp::{ClientConfig, RemotePath, SftpClient};
use tokio::process::{Child, Command};

const SERVER_VARIABLE: &str = "HEIMDALL_SFTP_SERVER";
const REQUIRE_VARIABLE: &str = "HEIMDALL_REQUIRE_SFTP_SERVER";
const KNOWN_PATHS: [&str; 2] = [
    "/usr/libexec/openssh/sftp-server",
    "/usr/lib/openssh/sftp-server",
];

/// Bound on each step.
const STEP: Duration = Duration::from_secs(30);

async fn start() -> Option<(Child, RemoteSession)> {
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
    Some((child, RemoteSession::Sftp(client)))
}

fn remote(path: &Path) -> RemotePath {
    RemotePath::from_bytes(path.as_os_str().as_bytes())
}

async fn step<T>(future: impl std::future::Future<Output = T>) -> T {
    tokio::time::timeout(STEP, future).await.expect("in time")
}

#[tokio::test]
async fn permissions_set_are_the_ones_listed_and_on_disk_with_the_owner_and_group() {
    let Some((_server, session)) = start().await else {
        return;
    };
    let dir = tempfile::tempdir().expect("dir");
    let file = dir.path().join("run.sh");
    std::fs::write(&file, b"#!/bin/sh\n").expect("written");
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).expect("mode");

    step(session.set_permissions(&remote(&file), 0o4750))
        .await
        .expect("set");
    let on_disk = std::fs::metadata(&file).expect("metadata");
    assert_eq!(on_disk.permissions().mode() & 0o7777, 0o4750);

    let listed = step(session.list(&remote(dir.path())))
        .await
        .expect("listed");
    let item = listed
        .iter()
        .find(|item| item.name == b"run.sh")
        .expect("listed");
    assert_eq!(item.kind, ItemKind::File);
    assert_eq!(item.permissions, Some(0o4750), "the bits alone, as set");
    assert_eq!(item.owner, Some(on_disk.uid()));
    assert_eq!(item.group, Some(on_disk.gid()));
}

#[tokio::test]
async fn the_permissions_of_a_link_are_never_set_on_what_it_points_to() {
    let Some((_server, session)) = start().await else {
        return;
    };
    let dir = tempfile::tempdir().expect("dir");
    let file = dir.path().join("secret");
    std::fs::write(&file, b"key").expect("written");
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).expect("mode");
    let link = dir.path().join("link");
    std::os::unix::fs::symlink(&file, &link).expect("link");

    let refused = step(session.set_permissions(&remote(&link), 0o777)).await;
    assert_eq!(refused, Err(heimdall_files::RemoteError::IsLink));
    let on_disk = std::fs::metadata(&file).expect("metadata");
    assert_eq!(on_disk.permissions().mode() & 0o7777, 0o600, "untouched");
}

#[tokio::test]
async fn a_download_never_replaces_a_local_file_unasked() {
    let Some((_server, session)) = start().await else {
        return;
    };
    let dir = tempfile::tempdir().expect("dir");
    let source = dir.path().join("source");
    std::fs::write(&source, b"theirs").expect("source");
    let target = dir.path().join("target");
    std::fs::write(&target, b"mine").expect("target");
    let cancel = tokio_util::sync::CancellationToken::new();

    let refused =
        step(session.download_with(&remote(&source), &target, false, &cancel, |_| {})).await;
    assert_eq!(refused, Err(heimdall_files::RemoteError::LocalExists));
    assert_eq!(std::fs::read(&target).expect("target"), b"mine");
    step(session.download_with(&remote(&source), &target, true, &cancel, |_| {}))
        .await
        .expect("replaced when agreed");
    assert_eq!(std::fs::read(&target).expect("target"), b"theirs");
}
