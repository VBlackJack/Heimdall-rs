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

//! A session's transcript file, as the C# session log writes it.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, UNIX_EPOCH};

use heimdall_app::transcript::{
    TRANSCRIPT_MAX_BYTES, Transcript, TranscriptContext, TranscriptLines,
};

/// 2026-09-27 19:15:03 UTC.
const STARTED: u64 = 1_790_536_503;

const NEWLINE: &str = if cfg!(windows) { "\r\n" } else { "\n" };

fn context(host: &str) -> TranscriptContext {
    TranscriptContext {
        protocol: "SSH".to_owned(),
        host: host.to_owned(),
        title: "web".to_owned(),
        started: UNIX_EPOCH + Duration::from_secs(STARTED),
    }
}

fn lines() -> TranscriptLines {
    TranscriptLines {
        header: Arc::new(|context: &TranscriptContext| {
            format!(
                "start {} {} {}",
                context.protocol, context.host, context.title
            )
        }),
        footer: Arc::new(|_, lasted: Duration| format!("end after {}s", lasted.as_secs())),
    }
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).expect("read")
}

#[test]
fn the_output_is_kept_as_text_between_its_header_and_its_footer() {
    let dir = tempfile::tempdir().expect("dir");
    let folder = dir.path().join("logs").join("sessions");
    let mut transcript = Transcript::start(
        &folder,
        &context("web.lab"),
        Some(&lines()),
        TRANSCRIPT_MAX_BYTES,
    )
    .expect("started");
    let path = transcript.path().to_owned();
    assert_eq!(path, folder.join("SSH_web.lab_20260927_191503.log"));
    transcript
        .write(b"\x1b[1;32m$\x1b[0m ls\r\nfile\r\n\xe2\x82")
        .expect("written");
    transcript
        .finish(UNIX_EPOCH + Duration::from_secs(STARTED + 65))
        .expect("finished");
    transcript
        .finish(UNIX_EPOCH + Duration::from_secs(STARTED + 99))
        .expect("once only");
    drop(transcript);
    assert_eq!(
        read(&path),
        format!(
            "start SSH web.lab web{NEWLINE}$ ls\r\nfile\r\n\u{fffd}{NEWLINE}end after 65s{NEWLINE}"
        )
    );
}

#[test]
fn without_lines_there_is_no_header_nor_footer() {
    let dir = tempfile::tempdir().expect("dir");
    let mut transcript =
        Transcript::start(dir.path(), &context("web.lab"), None, TRANSCRIPT_MAX_BYTES)
            .expect("started");
    let path = transcript.path().to_owned();
    transcript.write(b"just this").expect("written");
    transcript
        .finish(UNIX_EPOCH + Duration::from_secs(STARTED))
        .expect("finished");
    assert_eq!(read(&path), "just this");
}

#[test]
fn a_file_is_never_written_over_and_a_name_holds_no_forbidden_character() {
    let dir = tempfile::tempdir().expect("dir");
    let start = |host: &str| {
        Transcript::start(dir.path(), &context(host), None, TRANSCRIPT_MAX_BYTES)
            .expect("started")
            .path()
            .file_name()
            .expect("name")
            .to_string_lossy()
            .into_owned()
    };
    assert_eq!(start("web.lab"), "SSH_web.lab_20260927_191503.log");
    assert_eq!(start("web.lab"), "SSH_web.lab_20260927_191503_1.log");
    assert_eq!(start("web.lab"), "SSH_web.lab_20260927_191503_2.log");
    assert_eq!(
        start("fe80::1|a/b\\c\t*?\"<>"),
        "SSH_fe80__1_a_b_c_______20260927_191503.log"
    );
}

#[test]
fn past_the_cap_it_continues_in_numbered_files() {
    let dir = tempfile::tempdir().expect("dir");
    let mut transcript =
        Transcript::start(dir.path(), &context("web.lab"), None, 10).expect("started");
    let first = transcript.path().to_owned();
    transcript.write(b"123456").expect("written");
    transcript.write(b"7890").expect("written");
    transcript.write(b"abc").expect("written");
    transcript
        .write(b"a chunk larger than the cap")
        .expect("written");
    transcript.write(b"z").expect("written");
    transcript
        .finish(UNIX_EPOCH + Duration::from_secs(STARTED))
        .expect("finished");
    let part = |n: u32| {
        dir.path()
            .join(format!("SSH_web.lab_20260927_191503.{n}.log"))
    };
    let big = dir.path().join("big");
    let mut first_big = Transcript::start(&big, &context("web.lab"), None, 4).expect("started");
    first_big.write(b"123456").expect("written");
    assert_eq!(
        read(first_big.path()),
        "123456",
        "the first chunk, larger than the cap, stays in the first file"
    );
    drop(first_big);
    assert_eq!(std::fs::read_dir(&big).expect("folder").count(), 1);
    assert_eq!(read(&first), "1234567890", "up to the cap exactly");
    assert_eq!(read(&part(1)), "abc");
    assert_eq!(
        read(&part(2)),
        "a chunk larger than the cap",
        "a file of its own"
    );
    assert_eq!(read(&part(3)), "z");
    assert!(!part(4).exists());
}

#[cfg(unix)]
#[test]
fn on_unix_only_its_owner_reads_it() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().expect("dir");
    let transcript = Transcript::start(dir.path(), &context("web.lab"), None, 4).expect("started");
    let mode = std::fs::metadata(transcript.path())
        .expect("metadata")
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600);
}

#[test]
fn a_transcript_dropped_unfinished_is_finished_with_its_footer() {
    let dir = tempfile::tempdir().expect("dir");
    let mut transcript = Transcript::start(
        dir.path(),
        &context("web.lab"),
        Some(&lines()),
        TRANSCRIPT_MAX_BYTES,
    )
    .expect("started");
    let path = transcript.path().to_owned();
    transcript.write(b"bye").expect("written");
    drop(transcript);
    let text = read(&path);
    assert!(
        text.starts_with(&format!(
            "start SSH web.lab web{NEWLINE}bye{NEWLINE}end after "
        )),
        "{text}"
    );
}
