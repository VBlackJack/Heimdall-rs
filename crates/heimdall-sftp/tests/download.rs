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

//! Downloads against OpenSSH's `sftp-server`: complete, resumed, restarted when the
//! remote file changed or the part file is damaged, cancelled then resumed, never
//! written through a link planted where the part file goes, and never committed over a
//! local file the user did not agree to replace.

#![cfg(unix)]

mod common;

use std::fmt::Write as _;
use std::os::unix::ffi::OsStrExt as _;
use std::os::unix::fs::MetadataExt as _;
use std::path::Path;

use common::{pattern, remote, start, step};
use heimdall_sftp::transfer::{
    PART_SUFFIX, RESUME_SUFFIX, TransferConfig, TransferError, download,
};
use tokio_util::sync::CancellationToken;

/// Size of the file downloaded: several chunks and several saves of the mark.
const SIZE: usize = 9 * 1024 * 1024 + 4321;

/// Where the earlier attempts stopped.
const HALF: usize = 4 * 1024 * 1024;

struct Setup {
    dir: tempfile::TempDir,
    content: Vec<u8>,
}

impl Setup {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("dir");
        let content = pattern(SIZE);
        std::fs::write(dir.path().join("source.bin"), &content).expect("written");
        Self { dir, content }
    }

    fn source(&self) -> std::path::PathBuf {
        self.dir.path().join("source.bin")
    }

    fn target(&self) -> std::path::PathBuf {
        self.dir.path().join("target.bin")
    }

    fn part(&self) -> std::path::PathBuf {
        with_suffix(&self.target(), PART_SUFFIX)
    }

    fn resume(&self) -> std::path::PathBuf {
        with_suffix(&self.target(), RESUME_SUFFIX)
    }

    /// An earlier attempt that stopped at `mark`, recorded for `size` and the source's
    /// modification time.
    fn earlier_attempt(&self, part: &[u8], mark: usize, size: usize) {
        std::fs::write(self.part(), part).expect("part");
        let meta = std::fs::metadata(self.source()).expect("source");
        let mut hex = String::new();
        for byte in self.source().as_os_str().as_bytes() {
            let _ = write!(hex, "{byte:02x}");
        }
        let record = format!(
            "heimdall-resume 1\nremote {hex}\nsize {size}\nmtime {}\nmark {mark}\n",
            meta.mtime()
        );
        std::fs::write(self.resume(), record).expect("record");
    }

    fn assert_complete(&self) {
        let got = std::fs::read(self.target()).expect("downloaded");
        assert!(got == self.content, "the downloaded bytes are the source's");
        assert!(!self.part().exists(), "no part file left");
        assert!(!self.resume().exists(), "no resume record left");
    }
}

fn with_suffix(path: &Path, suffix: &str) -> std::path::PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    name.into()
}

#[tokio::test]
async fn a_download_arrives_whole_under_its_name_with_its_date() {
    let Some((_server, client)) = start().await else {
        return;
    };
    let setup = Setup::new();
    let report = step(download(
        &client,
        &remote(&setup.source()),
        &setup.target(),
        &TransferConfig::default(),
        &CancellationToken::new(),
        |_| {},
    ))
    .await
    .expect("downloaded");
    assert_eq!((report.bytes, report.resumed_from), (SIZE as u64, 0));
    setup.assert_complete();
    let source = std::fs::metadata(setup.source()).expect("source");
    let target = std::fs::metadata(setup.target()).expect("target");
    assert_eq!(
        target.mtime(),
        source.mtime(),
        "modification time preserved"
    );
}

#[tokio::test]
async fn a_valid_earlier_attempt_is_resumed_from_its_mark() {
    let Some((_server, client)) = start().await else {
        return;
    };
    let setup = Setup::new();
    setup.earlier_attempt(&setup.content[..HALF], HALF, SIZE);
    let report = step(download(
        &client,
        &remote(&setup.source()),
        &setup.target(),
        &TransferConfig::default(),
        &CancellationToken::new(),
        |_| {},
    ))
    .await
    .expect("downloaded");
    assert_eq!(report.resumed_from, HALF as u64);
    setup.assert_complete();
}

#[tokio::test]
async fn a_changed_remote_file_or_a_damaged_part_starts_over() {
    let Some((_server, client)) = start().await else {
        return;
    };
    let setup = Setup::new();
    // The record says another size: the remote file changed since.
    setup.earlier_attempt(&setup.content[..HALF], HALF, SIZE - 1);
    let changed = step(download(
        &client,
        &remote(&setup.source()),
        &setup.target(),
        &TransferConfig::default(),
        &CancellationToken::new(),
        |_| {},
    ))
    .await
    .expect("downloaded");
    assert_eq!(changed.resumed_from, 0);
    setup.assert_complete();

    // The part file's last bytes are not the remote ones.
    std::fs::remove_file(setup.target()).expect("removed");
    let mut damaged = setup.content[..HALF].to_vec();
    damaged[HALF - 10] ^= 0xFF;
    setup.earlier_attempt(&damaged, HALF, SIZE);
    let restarted = step(download(
        &client,
        &remote(&setup.source()),
        &setup.target(),
        &TransferConfig::default(),
        &CancellationToken::new(),
        |_| {},
    ))
    .await
    .expect("downloaded");
    assert_eq!(restarted.resumed_from, 0);
    setup.assert_complete();
}

#[tokio::test]
async fn a_cancelled_download_keeps_a_whole_prefix_and_resumes_from_it() {
    let Some((_server, client)) = start().await else {
        return;
    };
    let setup = Setup::new();
    let cancel = CancellationToken::new();
    let trigger = cancel.clone();
    let stopped = step(download(
        &client,
        &remote(&setup.source()),
        &setup.target(),
        &TransferConfig::default(),
        &cancel,
        move |mark| {
            if mark >= HALF as u64 {
                trigger.cancel();
            }
        },
    ))
    .await
    .expect_err("cancelled");
    let TransferError::Cancelled { kept } = stopped else {
        panic!("expected a cancel, got {stopped:?}");
    };
    assert!(kept >= HALF as u64, "kept {kept}");
    let part = std::fs::read(setup.part()).expect("part kept");
    assert_eq!(part.len() as u64, kept, "cut back to the mark");
    assert!(
        part[..] == setup.content[..part.len()],
        "the kept bytes are a whole prefix of the file"
    );
    assert!(!setup.target().exists(), "nothing under the real name yet");

    let resumed = step(download(
        &client,
        &remote(&setup.source()),
        &setup.target(),
        &TransferConfig::default(),
        &CancellationToken::new(),
        |_| {},
    ))
    .await
    .expect("downloaded");
    assert_eq!(resumed.resumed_from, kept);
    setup.assert_complete();
}

#[tokio::test]
async fn a_link_planted_as_the_part_file_is_never_written_through() {
    let Some((_server, client)) = start().await else {
        return;
    };
    let setup = Setup::new();
    let victim = setup.dir.path().join("victim.txt");
    std::fs::write(&victim, b"keep me").expect("victim");
    std::os::unix::fs::symlink(&victim, setup.part()).expect("planted link");
    step(download(
        &client,
        &remote(&setup.source()),
        &setup.target(),
        &TransferConfig::default(),
        &CancellationToken::new(),
        |_| {},
    ))
    .await
    .expect("downloaded");
    assert_eq!(std::fs::read(&victim).expect("victim"), b"keep me");
    setup.assert_complete();
}

#[tokio::test]
async fn a_directory_is_not_downloaded() {
    let Some((_server, client)) = start().await else {
        return;
    };
    let setup = Setup::new();
    let refused = step(download(
        &client,
        &remote(setup.dir.path()),
        &setup.target(),
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
    assert!(!setup.part().exists());
}

#[tokio::test]
async fn a_local_file_not_agreed_to_be_replaced_is_left_and_the_download_kept() {
    let Some((_server, client)) = start().await else {
        return;
    };
    let setup = Setup::new();
    // Appeared after the user was asked, or never asked about.
    std::fs::write(setup.target(), b"mine").expect("local file");
    let config = TransferConfig {
        replace_local: false,
        ..TransferConfig::default()
    };
    let refused = step(download(
        &client,
        &remote(&setup.source()),
        &setup.target(),
        &config,
        &CancellationToken::new(),
        |_| {},
    ))
    .await
    .expect_err("refused");
    assert!(matches!(refused, TransferError::LocalExists), "{refused:?}");
    assert_eq!(std::fs::read(setup.target()).expect("target"), b"mine");
    assert!(
        std::fs::read(setup.part()).expect("part") == setup.content,
        "the complete download is kept"
    );

    // Agreed this time: resumed from the kept part's last mark, and the file replaced.
    let report = step(download(
        &client,
        &remote(&setup.source()),
        &setup.target(),
        &TransferConfig::default(),
        &CancellationToken::new(),
        |_| {},
    ))
    .await
    .expect("downloaded");
    assert!(report.resumed_from > 0, "resumed, not started over");
    setup.assert_complete();
}
