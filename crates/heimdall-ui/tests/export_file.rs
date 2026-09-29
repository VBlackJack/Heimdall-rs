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

//! The exported sessions written where the save dialog said.

use heimdall_app::ExportOutcome;
use heimdall_ui::export_file::write;

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
