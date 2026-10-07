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

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use heimdall_app::transcript::{
    HEADER_MARKER, TRANSCRIPT_MAX_BYTES, Transcript, TranscriptContext, TranscriptLines,
    prune_expired,
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

/// Days of retention the pruning tests run with.
const RETENTION_DAYS: u32 = 30;

/// A day, in seconds.
const DAY: u64 = 86_400;

/// When a file older than the retention was last written: ten days past it.
fn long_ago() -> SystemTime {
    SystemTime::now() - Duration::from_secs(u64::from(RETENTION_DAYS + 10) * DAY)
}

/// Writes `text` to `name` in `folder`, last written at `when`.
fn file_at(folder: &Path, name: &str, text: &str, when: SystemTime) -> PathBuf {
    let path = folder.join(name);
    std::fs::write(&path, text).expect("written");
    std::fs::File::options()
        .write(true)
        .open(&path)
        .expect("opened")
        .set_modified(when)
        .expect("dated");
    path
}

/// A header as every translation opens it.
fn header() -> String {
    format!(
        "{HEADER_MARKER} Session started 2026-01-01T12:00:00Z | SSH | host web {HEADER_MARKER}\n"
    )
}

#[test]
fn only_the_transcripts_written_here_and_past_their_retention_are_removed() {
    let dir = tempfile::tempdir().expect("dir");
    let folder = dir.path();
    let old = long_ago();
    let header = header();
    let gone = [
        // A transcript and its continuation, both old.
        file_at(folder, "SSH_web_20260101_120000.log", &header, old),
        file_at(folder, "SSH_web_20260101_120000.1.log", "more", old),
        // A second one of the same second, its name taken.
        file_at(folder, "SSH_web_20260101_120000_1.log", &header, old),
        // After a byte order mark, as another editor may save it.
        file_at(
            folder,
            "TELNET_my_host_20260102_080000.log",
            &format!("\u{feff}{header}"),
            old,
        ),
    ];
    let kept = [
        // Written within the retention.
        file_at(
            folder,
            "SSH_new_20260301_120000.log",
            &header,
            SystemTime::now(),
        ),
        // Old, but its continuation is not: the transcript goes whole or stays whole.
        file_at(folder, "SSH_mixed_20260101_120000.log", &header, old),
        file_at(
            folder,
            "SSH_mixed_20260101_120000.1.log",
            "more",
            SystemTime::now(),
        ),
        // Named as a transcript, but not one written here: PuTTY's default name.
        file_at(folder, "putty_host_20260101_120000.log", "PuTTY log", old),
        // A continuation whose transcript is gone: nothing says it was written here.
        file_at(folder, "SSH_orphan_20260101_120000.1.log", &header, old),
        // Names a transcript never has.
        file_at(folder, "notes.txt", &header, old),
        file_at(folder, "SSH_web.log", &header, old),
        file_at(folder, "SSH_web_2026_120000.log", &header, old),
        file_at(folder, "20260101_120000.log", &header, old),
        file_at(folder, "SSH_web_20260101_120000.txt", &header, old),
    ];
    // A folder inside is never entered, even named as a transcript.
    let inner = folder.join("SSH_dir_20260101_120000.log");
    std::fs::create_dir(&inner).expect("inner folder");
    let nested = file_at(&inner, "SSH_deep_20260101_120000.log", &header, old);

    let removed = prune_expired(folder, RETENTION_DAYS, SystemTime::now());

    assert_eq!(removed, gone.len());
    for path in &gone {
        assert!(!path.exists(), "{} removed", path.display());
    }
    for path in kept.iter().chain([&nested]) {
        assert!(path.exists(), "{} kept", path.display());
    }
    assert!(inner.is_dir());
}

#[test]
fn zero_days_keeps_every_transcript_and_a_missing_folder_is_no_error() {
    let dir = tempfile::tempdir().expect("dir");
    let path = file_at(
        dir.path(),
        "SSH_web_20260101_120000.log",
        &header(),
        long_ago(),
    );
    assert_eq!(prune_expired(dir.path(), 0, SystemTime::now()), 0);
    assert!(path.exists(), "0 keeps every transcript");
    assert_eq!(
        prune_expired(
            &dir.path().join("absent"),
            RETENTION_DAYS,
            SystemTime::now()
        ),
        0
    );
}

#[test]
fn the_files_a_transcript_writes_are_the_ones_its_retention_removes() {
    let dir = tempfile::tempdir().expect("dir");
    let marked = TranscriptLines {
        header: Arc::new(|_: &TranscriptContext| format!("{HEADER_MARKER} start {HEADER_MARKER}")),
        footer: Arc::new(|_, _| String::new()),
    };
    // Two of the same second, each continued past its cap.
    let first =
        Transcript::start(dir.path(), &context("web.lab"), Some(&marked), 4).expect("started");
    let mut second =
        Transcript::start(dir.path(), &context("web.lab"), Some(&marked), 4).expect("started");
    second.write(b"continued past the cap").expect("written");
    drop((first, second));
    let written: Vec<PathBuf> = std::fs::read_dir(dir.path())
        .expect("listed")
        .map(|entry| entry.expect("entry").path())
        .collect();
    // Each its first file and its continuations, the second's name taken.
    assert!(written.len() >= 4, "{written:?}");
    for path in &written {
        std::fs::File::options()
            .write(true)
            .open(path)
            .expect("opened")
            .set_modified(long_ago())
            .expect("dated");
    }
    assert_eq!(
        prune_expired(dir.path(), RETENTION_DAYS, SystemTime::now()),
        written.len()
    );
    assert_eq!(std::fs::read_dir(dir.path()).expect("listed").count(), 0);
}

#[cfg(unix)]
#[test]
fn a_link_named_as_a_transcript_is_never_followed_nor_removed() {
    let dir = tempfile::tempdir().expect("dir");
    let elsewhere = tempfile::tempdir().expect("elsewhere");
    let target = file_at(
        elsewhere.path(),
        "SSH_web_20260101_120000.log",
        &header(),
        long_ago(),
    );
    let link = dir.path().join("SSH_link_20260101_120000.log");
    std::os::unix::fs::symlink(&target, &link).expect("linked");
    assert_eq!(
        prune_expired(dir.path(), RETENTION_DAYS, SystemTime::now()),
        0
    );
    assert!(link.symlink_metadata().is_ok(), "the link stays");
    assert!(target.exists(), "what it points to stays");
}
