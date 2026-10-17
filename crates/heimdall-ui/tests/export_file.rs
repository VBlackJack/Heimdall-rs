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

//! The exported sessions, and a tool's export, written where the save dialog said; a tool's
//! dialog played by a ready answer. One test binary for both: each binary of this crate links
//! the whole interface, and the CI runner's disk is short.

use std::path::PathBuf;

use chrono::NaiveDate;
use heimdall_app::ExportOutcome;
use heimdall_ui::export_file::write;
use heimdall_ui::file_dialog::{Pick, Unavailable};
use heimdall_ui::tools::file_export::{self, Format, Saved, file_name, save};

#[tokio::test]
async fn the_document_is_written_as_given_and_counted() {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join("servers.json");
    let outcome = write(&path, "{\"servers\": []}".to_owned(), 4).await;
    assert_eq!(outcome, ExportOutcome::Saved(4));
    let bytes = std::fs::read(&path).expect("written");
    assert_eq!(
        bytes, b"{\"servers\": []}",
        "no byte order mark, as the C# writes"
    );
}

#[tokio::test]
async fn a_file_that_cannot_be_written_is_a_failure_with_its_reason() {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join("missing").join("servers.json");
    let outcome = write(&path, String::new(), 1).await;
    assert!(
        matches!(&outcome, ExportOutcome::Failed(reason) if !reason.is_empty()),
        "{outcome:?}"
    );
}

/// What the C# `Encoding.UTF8` writes first.
const BOM: &[u8] = b"\xEF\xBB\xBF";

/// A save dialog that answers `answer` at once.
fn dialog(answer: Result<Option<PathBuf>, Unavailable>) -> Pick<PathBuf> {
    Box::pin(async move { answer })
}

#[tokio::test]
async fn the_export_is_written_where_the_dialog_says_after_a_byte_order_mark() {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join("ping_host_20261017_090503.csv");
    let csv = "Seq,Host\n1,\"h\u{f4}te\"\n".to_owned();
    let saved = save(dialog(Ok(Some(path.clone()))), csv.clone()).await;
    assert_eq!(saved, Saved::Written);
    let bytes = std::fs::read(&path).expect("written");
    assert_eq!(
        &bytes[..BOM.len()],
        BOM,
        "as the C# Encoding.UTF8 writes it"
    );
    assert_eq!(&bytes[BOM.len()..], csv.as_bytes());
}

#[test]
fn an_existing_file_is_replaced_whole() {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join("report.html");
    std::fs::write(&path, "an older and much longer report than the new one").expect("old");
    assert_eq!(file_export::write(&path, "<p>new</p>"), Saved::Written);
    let bytes = std::fs::read(&path).expect("written");
    assert_eq!(
        &bytes[BOM.len()..],
        b"<p>new</p>",
        "nothing of the old file left"
    );
}

#[tokio::test]
async fn a_closed_dialog_writes_nothing() {
    let dir = tempfile::tempdir().expect("dir");
    let saved = save(dialog(Ok(None)), "x".to_owned()).await;
    assert_eq!(saved, Saved::Cancelled);
    assert_eq!(std::fs::read_dir(dir.path()).expect("dir").count(), 0);
}

#[tokio::test]
async fn no_dialog_or_no_write_is_a_failure_with_its_reason() {
    let saved = save(dialog(Err(Unavailable::Closed)), "x".to_owned()).await;
    assert_eq!(saved, Saved::Failed(Unavailable::Closed.message()));

    let dir = tempfile::tempdir().expect("dir");
    let blocked = dir.path().join("a-file");
    std::fs::write(&blocked, "x").expect("file");
    let under_a_file = blocked.join("export.csv");
    let saved = save(dialog(Ok(Some(under_a_file))), "x".to_owned()).await;
    assert!(
        matches!(&saved, Saved::Failed(reason) if !reason.is_empty()),
        "{saved:?}"
    );
}

#[test]
fn the_suggested_name_is_the_tool_its_target_and_the_moment_as_the_csharp() {
    let now = NaiveDate::from_ymd_opt(2026, 10, 17)
        .and_then(|day| day.and_hms_opt(9, 5, 3))
        .expect("a moment");
    assert_eq!(
        file_name("ping", Some(" 192.0.2.1 "), Format::Csv, now),
        "ping_192.0.2.1_20261017_090503.csv"
    );
    assert_eq!(
        file_name("SecNumCloud_Audit", None, Format::Html, now),
        "SecNumCloud_Audit_20261017_090503.html"
    );
    assert_eq!(
        file_name("firewall_test", Some("  "), Format::Csv, now),
        "firewall_test_20261017_090503.csv",
        "no target, no empty part"
    );
    assert_eq!(
        file_name("cartography", Some("10.0.0.0/24"), Format::Csv, now),
        "cartography_10.0.0.0_24_20261017_090503.csv",
        "as the C# Replace('/', '_')"
    );
    assert_eq!(
        file_name(
            "certscan",
            Some("fe80::1%eth0\\..\\x?*\n"),
            Format::Csv,
            now
        ),
        "certscan_fe80__1%eth0_.._x___20261017_090503.csv",
        "as the C# SanitizeFileName: no folder named, nothing Windows refuses"
    );
}
