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

//! The connection history: the C# `ConnectionHistory` file, line for line.

use std::fs;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use heimdall_core::connection_history::{
    Action, Entry, FILE_NAME, MAX_FILE_BYTES, ROTATED_FILE_NAME, record, record_in,
};

/// 2026-09-27T21:05:03Z.
fn instant() -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(1_790_543_103)
}

#[test]
fn a_line_holds_the_csharp_fields_in_camel_case_and_nothing_else() {
    let dir = tempfile::tempdir().expect("dir");
    record_in(
        dir.path(),
        instant(),
        Action::Connected,
        "id-1",
        "web 01",
        "SSH",
    )
    .expect("recorded");
    let text = fs::read_to_string(dir.path().join(FILE_NAME)).expect("file");
    assert_eq!(
        text,
        "{\"timestamp\":\"2026-09-27T21:05:03Z\",\"action\":\"Connected\",\
         \"serverId\":\"id-1\",\"displayName\":\"web 01\",\"connectionType\":\"SSH\"}\n"
    );
}

#[test]
fn lines_are_added_in_order_and_read_back() {
    let dir = tempfile::tempdir().expect("dir");
    record_in(dir.path(), instant(), Action::Connected, "a", "A", "RDP").expect("first");
    record_in(dir.path(), instant(), Action::Disconnected, "a", "A", "RDP").expect("second");
    let text = fs::read_to_string(dir.path().join(FILE_NAME)).expect("file");
    let entries: Vec<Entry> = text
        .lines()
        .map(|line| serde_json::from_str(line).expect("a JSON line"))
        .collect();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].action, "Connected");
    assert_eq!(entries[1].action, "Disconnected");
    assert_eq!(entries[1].server_id, "a");
}

#[test]
fn the_folder_is_made_when_it_is_missing() {
    let dir = tempfile::tempdir().expect("dir");
    let nested = dir.path().join("logs").join("deeper");
    record(&nested, Action::Connected, "a", "A", "VNC");
    assert!(nested.join(FILE_NAME).is_file());
}

#[test]
fn a_full_file_moves_to_the_rotated_name_and_the_one_before_it_is_dropped() {
    let dir = tempfile::tempdir().expect("dir");
    let current = dir.path().join(FILE_NAME);
    let rotated = dir.path().join(ROTATED_FILE_NAME);
    fs::write(&rotated, "old\n").expect("older");
    fs::write(
        &current,
        vec![b'x'; usize::try_from(MAX_FILE_BYTES).expect("size")],
    )
    .expect("full");
    record_in(dir.path(), instant(), Action::Connected, "a", "A", "SSH").expect("recorded");
    assert_eq!(
        fs::metadata(&rotated).expect("rotated").len(),
        MAX_FILE_BYTES,
        "the full file moved, the older one gone"
    );
    let text = fs::read_to_string(&current).expect("current");
    assert_eq!(text.lines().count(), 1, "a fresh file: {text}");
    assert!(text.contains("\"serverId\":\"a\""));
}

#[test]
fn a_file_under_the_limit_is_added_to() {
    let dir = tempfile::tempdir().expect("dir");
    for _ in 0..3 {
        record_in(dir.path(), instant(), Action::Connected, "a", "A", "SSH").expect("recorded");
    }
    assert!(!dir.path().join(ROTATED_FILE_NAME).exists());
    let text = fs::read_to_string(dir.path().join(FILE_NAME)).expect("file");
    assert_eq!(text.lines().count(), 3);
}

#[cfg(unix)]
#[test]
fn the_file_is_readable_by_its_owner_alone() {
    use std::os::unix::fs::PermissionsExt as _;

    let dir = tempfile::tempdir().expect("dir");
    record_in(dir.path(), instant(), Action::Connected, "a", "A", "SSH").expect("recorded");
    let mode = fs::metadata(dir.path().join(FILE_NAME))
        .expect("file")
        .permissions()
        .mode();
    assert_eq!(mode & 0o077, 0, "{mode:o}");
}

#[test]
fn a_folder_that_cannot_be_made_is_an_error_to_record_in_and_nothing_to_record() {
    let dir = tempfile::tempdir().expect("dir");
    let file = dir.path().join("not-a-folder");
    fs::write(&file, "").expect("file");
    assert!(record_in(&file, instant(), Action::Connected, "a", "A", "SSH").is_err());
    record(&file, Action::Connected, "a", "A", "SSH");
}
