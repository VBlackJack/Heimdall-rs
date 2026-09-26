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

//! Folders against OpenSSH's `sftp-server`: transferred whole both ways and deleted, with
//! links never followed, so what they point to is neither copied nor deleted.

#![cfg(unix)]

mod common;

use std::os::unix::fs::symlink;
use std::path::Path;

use common::{remote, start, step};
use heimdall_sftp::transfer::TransferConfig;
use heimdall_sftp::tree::{Skip, download_tree, remove_tree, upload_tree};
use tokio_util::sync::CancellationToken;

/// A folder with a file, a sub-folder with a file, a link to a file, and a link to a
/// folder outside it that holds a file of its own.
fn build_tree(root: &Path, outside: &Path) {
    std::fs::create_dir_all(root.join("sub")).expect("folders");
    std::fs::write(root.join("b.txt"), b"bee").expect("file");
    std::fs::write(root.join("sub").join("c.txt"), b"sea").expect("file");
    std::fs::create_dir_all(outside).expect("outside");
    std::fs::write(outside.join("precious.txt"), b"keep").expect("outside file");
    symlink(outside, root.join("away")).expect("link to a folder");
    symlink(root.join("b.txt"), root.join("alias")).expect("link to a file");
}

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

fn links(report: &heimdall_sftp::tree::TreeReport) -> Vec<String> {
    let mut links: Vec<String> = report
        .skipped
        .iter()
        .filter(|(_, why)| *why == Skip::Link)
        .map(|(name, _)| name.clone())
        .collect();
    links.sort();
    links
}

#[tokio::test]
async fn a_folder_downloads_whole_and_its_links_are_not_followed() {
    let Some((_server, client)) = start().await else {
        return;
    };
    let dir = tempfile::tempdir().expect("dir");
    let source = dir.path().join("source");
    build_tree(&source, &dir.path().join("outside"));
    let target = dir.path().join("copy");
    let report = step(download_tree(
        &client,
        &remote(&source),
        &target,
        &TransferConfig::default(),
        &CancellationToken::new(),
        |_| {},
    ))
    .await
    .expect("downloaded");
    assert_eq!((report.files, report.folders), (2, 2));
    assert_eq!(names(&target), ["b.txt", "sub"]);
    assert_eq!(
        std::fs::read(target.join("sub").join("c.txt")).expect("nested"),
        b"sea"
    );
    assert_eq!(links(&report), ["alias", "away"]);
}

#[tokio::test]
async fn a_folder_uploads_whole_twice_and_its_links_are_not_followed() {
    let Some((_server, client)) = start().await else {
        return;
    };
    let dir = tempfile::tempdir().expect("dir");
    let source = dir.path().join("source");
    build_tree(&source, &dir.path().join("outside"));
    let target = dir.path().join("uploaded");
    for round in 0..2 {
        let report = step(upload_tree(
            &client,
            &source,
            &remote(&target),
            &TransferConfig::default(),
            &CancellationToken::new(),
            |_| {},
        ))
        .await
        .expect("uploaded");
        assert_eq!(report.files, 2, "round {round}");
        assert_eq!(links(&report), ["alias", "away"], "round {round}");
    }
    assert_eq!(names(&target), ["b.txt", "sub"]);
    assert_eq!(
        std::fs::read(target.join("sub").join("c.txt")).expect("nested"),
        b"sea"
    );
}

#[tokio::test]
async fn deleting_a_folder_removes_a_link_inside_but_never_its_target() {
    let Some((_server, client)) = start().await else {
        return;
    };
    let dir = tempfile::tempdir().expect("dir");
    let doomed = dir.path().join("doomed");
    let outside = dir.path().join("outside");
    build_tree(&doomed, &outside);
    let removed = step(remove_tree(
        &client,
        &remote(&doomed),
        &CancellationToken::new(),
    ))
    .await
    .expect("removed");
    assert_eq!(removed, 6, "two files, two links, two folders");
    assert!(!doomed.exists());
    assert_eq!(
        std::fs::read(outside.join("precious.txt")).expect("kept"),
        b"keep",
        "what the link pointed to is untouched"
    );
}

#[tokio::test]
async fn deleting_a_link_to_a_folder_removes_only_the_link() {
    let Some((_server, client)) = start().await else {
        return;
    };
    let dir = tempfile::tempdir().expect("dir");
    let outside = dir.path().join("outside");
    std::fs::create_dir_all(&outside).expect("outside");
    std::fs::write(outside.join("precious.txt"), b"keep").expect("file");
    let link = dir.path().join("link");
    symlink(&outside, &link).expect("link");
    let removed = step(remove_tree(
        &client,
        &remote(&link),
        &CancellationToken::new(),
    ))
    .await
    .expect("removed");
    assert_eq!(removed, 1);
    assert!(link.symlink_metadata().is_err(), "the link is gone");
    assert_eq!(names(&outside), ["precious.txt"]);
}
