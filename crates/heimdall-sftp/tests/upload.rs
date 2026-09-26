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

//! Uploads against OpenSSH's `sftp-server`: the file arrives whole with its mode (never
//! set-user-id) and date, replaces a target only when asked, and leaves no temporary file
//! behind whatever stopped it.

#![cfg(unix)]

mod common;

use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};
use std::path::Path;

use common::{pattern, remote, start, step};
use heimdall_sftp::transfer::{TransferConfig, TransferError, upload};
use tokio_util::sync::CancellationToken;

/// Modification time given to the source: 2001-09-09.
const OLD_DATE: std::time::Duration = std::time::Duration::from_secs(1_000_000_000);

/// Size of the file uploaded: many writes in flight.
const SIZE: usize = 5 * 1024 * 1024 + 99;

/// Whether two buffers hold the same bytes; a failure does not print megabytes.
fn same(left: &[u8], right: &[u8]) -> bool {
    left == right
}

/// Names in `dir`, sorted.
fn names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .expect("readable")
        .map(|entry| {
            entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    names.sort();
    names
}

struct Setup {
    local: tempfile::TempDir,
    remote: tempfile::TempDir,
    content: Vec<u8>,
}

impl Setup {
    fn new(mode: u32) -> Self {
        let local = tempfile::tempdir().expect("local");
        let remote = tempfile::tempdir().expect("remote");
        let content = pattern(SIZE);
        let source = local.path().join("source.bin");
        std::fs::write(&source, &content).expect("written");
        std::fs::set_permissions(&source, std::fs::Permissions::from_mode(mode)).expect("mode");
        // A date far from now: a copy that forgot the date cannot match it by chance.
        std::fs::File::options()
            .write(true)
            .open(&source)
            .expect("opened")
            .set_modified(std::time::SystemTime::UNIX_EPOCH + OLD_DATE)
            .expect("dated");
        Self {
            local,
            remote,
            content,
        }
    }

    fn source(&self) -> std::path::PathBuf {
        self.local.path().join("source.bin")
    }

    fn target(&self) -> std::path::PathBuf {
        self.remote.path().join("target.bin")
    }
}

#[tokio::test]
async fn an_upload_arrives_whole_with_its_mode_but_never_setuid() {
    let Some((_server, client)) = start().await else {
        return;
    };
    let setup = Setup::new(0o4751);
    let report = step(upload(
        &client,
        &setup.source(),
        &remote(&setup.target()),
        false,
        &TransferConfig::default(),
        &CancellationToken::new(),
        |_| {},
    ))
    .await
    .expect("uploaded");
    assert_eq!(report.bytes, SIZE as u64);
    assert!(report.flushed, "OpenSSH offers fsync");
    assert!(
        same(
            &std::fs::read(setup.target()).expect("arrived"),
            &setup.content
        ),
        "the bytes arrived as sent"
    );
    let target = std::fs::metadata(setup.target()).expect("target");
    assert_eq!(
        target.mode() & 0o7777,
        0o751,
        "permission bits kept, set-user-id dropped"
    );
    let source = std::fs::metadata(setup.source()).expect("source");
    assert_eq!(
        target.mtime(),
        source.mtime(),
        "modification time preserved"
    );
    assert_eq!(
        names(setup.remote.path()),
        vec!["target.bin"],
        "no temporary file left"
    );
}

#[tokio::test]
async fn an_existing_target_is_replaced_only_when_asked() {
    let Some((_server, client)) = start().await else {
        return;
    };
    let setup = Setup::new(0o644);
    std::fs::write(setup.target(), b"older").expect("existing target");
    let refused = step(upload(
        &client,
        &setup.source(),
        &remote(&setup.target()),
        false,
        &TransferConfig::default(),
        &CancellationToken::new(),
        |_| {},
    ))
    .await
    .expect_err("refused");
    assert!(matches!(refused, TransferError::Sftp(_)), "{refused:?}");
    assert_eq!(std::fs::read(setup.target()).expect("kept"), b"older");
    assert_eq!(
        names(setup.remote.path()),
        vec!["target.bin"],
        "temporary file removed"
    );

    step(upload(
        &client,
        &setup.source(),
        &remote(&setup.target()),
        true,
        &TransferConfig::default(),
        &CancellationToken::new(),
        |_| {},
    ))
    .await
    .expect("replaced");
    assert!(
        same(
            &std::fs::read(setup.target()).expect("replaced"),
            &setup.content
        ),
        "the bytes arrived as sent"
    );
    assert_eq!(names(setup.remote.path()), vec!["target.bin"]);
}

#[tokio::test]
async fn a_cancelled_upload_leaves_nothing_on_the_server() {
    let Some((_server, client)) = start().await else {
        return;
    };
    let setup = Setup::new(0o644);
    let cancel = CancellationToken::new();
    let trigger = cancel.clone();
    let stopped = step(upload(
        &client,
        &setup.source(),
        &remote(&setup.target()),
        false,
        &TransferConfig::default(),
        &cancel,
        move |sent| {
            if sent >= (SIZE / 2) as u64 {
                trigger.cancel();
            }
        },
    ))
    .await
    .expect_err("cancelled");
    assert!(
        matches!(stopped, TransferError::Cancelled { .. }),
        "{stopped:?}"
    );
    assert!(
        names(setup.remote.path()).is_empty(),
        "no target, no temporary file"
    );
}

#[tokio::test]
async fn a_local_link_is_not_followed() {
    let Some((_server, client)) = start().await else {
        return;
    };
    let setup = Setup::new(0o644);
    let link = setup.local.path().join("link.bin");
    std::os::unix::fs::symlink(setup.source(), &link).expect("link");
    let refused = step(upload(
        &client,
        &link,
        &remote(&setup.target()),
        false,
        &TransferConfig::default(),
        &CancellationToken::new(),
        |_| {},
    ))
    .await
    .expect_err("refused");
    assert!(
        matches!(refused, TransferError::NotARegularFile),
        "{refused:?}"
    );
    assert!(names(setup.remote.path()).is_empty());
}
