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

//! The transcripts and the session logs, in a folder the user chose, restricted on Windows
//! to the user, the Administrators and SYSTEM as the C# `RestrictedLogFile` restricts them,
//! read back from the system: only on Windows.

#![cfg(windows)]

#[path = "../../heimdall-core/tests/support/dacl.rs"]
mod dacl;

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use heimdall_app::session_log::{
    OperationKind, OperationOutcome, OperationRecord, SESSION_EVENTS_FILE, SESSION_OPERATIONS_FILE,
    SessionEvent, SessionEventKind, SessionLogs,
};
use heimdall_app::transcript::{Transcript, TranscriptContext};

/// Flags of an entry set on a file itself: none, nothing inside it to inherit it.
const SET_ON_THE_FILE: &str = "";

/// A cap small enough for every write to continue in a file of its own.
const TINY_CAP: u64 = 4;

/// A folder the user chose, its access list the one it takes from its parent: wider than
/// the user's in a real chosen folder.
fn chosen_folder(dir: &Path) -> PathBuf {
    let folder = dir.join("chosen");
    std::fs::create_dir(&folder).expect("folder");
    let found = dacl::read(&folder);
    assert!(
        !found.protected,
        "inherited, as a folder the user chose: {found:?}"
    );
    folder
}

/// The files of `folder`.
fn files(folder: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(folder)
        .expect("listed")
        .map(|entry| entry.expect("entry").path())
        .collect();
    files.sort();
    files
}

/// Asserts that `file` keeps only the user, the Administrators and SYSTEM.
fn assert_restricted(file: &Path) {
    let found = dacl::read(file);
    assert!(found.protected, "{}: {found:?}", file.display());
    assert_eq!(
        found.entries,
        dacl::restricted(SET_ON_THE_FILE),
        "{}",
        file.display()
    );
}

#[test]
fn a_transcript_and_its_continuations_keep_only_the_user_the_administrators_and_system() {
    let dir = tempfile::tempdir().expect("dir");
    let folder = chosen_folder(dir.path());
    let context = TranscriptContext {
        protocol: "SSH".to_owned(),
        host: "web.lab".to_owned(),
        title: "web".to_owned(),
        started: SystemTime::now(),
    };
    let mut transcript = Transcript::start(&folder, &context, None, TINY_CAP).expect("started");
    transcript.write(b"Password: hunter2").expect("written");
    transcript.write(b"$ exit").expect("written");
    drop(transcript);

    let written = files(&folder);
    assert!(
        written.len() >= 2,
        "the first file and a continuation: {written:?}"
    );
    for file in &written {
        assert_restricted(file);
    }
}

#[test]
fn the_session_logs_keep_only_the_user_the_administrators_and_system() {
    let dir = tempfile::tempdir().expect("dir");
    let folder = chosen_folder(dir.path());
    let logs = SessionLogs::with_max_bytes(TINY_CAP);
    let event = SessionEvent {
        protocol: "RDP",
        kind: SessionEventKind::Connected,
        host: "srv.lab".to_owned(),
        title: None,
        reason: None,
        duration: None,
        end_trigger: None,
    };
    let record = OperationRecord {
        kind: OperationKind::Upload,
        remote_path: "/srv/a.txt".to_owned(),
        remote_path_to: None,
        local_path: None,
        bytes: Some(1),
        duration: Duration::from_millis(1),
        outcome: OperationOutcome::Success,
        privileged: false,
    };
    for _ in 0..2 {
        logs.event(&folder, &event);
        logs.operation(&folder, "SFTP", "srv.lab", &record);
    }
    logs.sync();

    let written = files(&folder);
    for name in [
        SESSION_EVENTS_FILE,
        SESSION_OPERATIONS_FILE,
        "session-events.1.log",
        "session-operations.1.log",
    ] {
        assert!(written.contains(&folder.join(name)), "{name}: {written:?}");
    }
    for file in &written {
        assert_restricted(file);
    }
}
