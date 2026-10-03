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

fn root(
    remote_path: &Path,
    local: &Path,
    kind: heimdall_files::conflict::Kind,
) -> heimdall_files::Root {
    heimdall_files::Root {
        remote: remote(remote_path),
        local: local.to_owned(),
        kind,
    }
}

fn shown(target: &[Vec<u8>]) -> String {
    target
        .iter()
        .map(|name| String::from_utf8_lossy(name).into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

#[tokio::test]
async fn an_upload_asks_for_every_file_in_the_way_and_writes_only_what_was_answered() {
    use heimdall_files::conflict::{Choice, Kind};
    let Some((_server, session)) = start().await else {
        return;
    };
    let local = tempfile::tempdir().expect("local");
    let server = tempfile::tempdir().expect("server");
    let site = local.path().join("site");
    std::fs::create_dir_all(site.join("sub")).expect("folders");
    std::fs::write(site.join("a.txt"), b"new a").expect("a");
    std::fs::write(site.join("sub/b.txt"), b"new b").expect("b");
    std::fs::write(site.join("sub/c.txt"), b"new c").expect("c");
    let there = server.path().join("site");
    std::fs::create_dir_all(there.join("sub")).expect("folders there");
    std::fs::write(there.join("a.txt"), b"old a").expect("old a");
    std::fs::write(there.join("sub/b.txt"), b"old b").expect("old b");
    let cancel = tokio_util::sync::CancellationToken::new();

    let plan = step(session.plan_upload(&[root(&there, &site, Kind::Folder)], &cancel))
        .await
        .expect("planned");
    let mut asked: Vec<_> = plan
        .conflicts()
        .map(|(index, step, _)| (shown(&step.target), index))
        .collect();
    asked.sort();
    let names: Vec<_> = asked.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(
        names,
        ["site/a.txt", "site/sub/b.txt"],
        "a folder already there is added to, not asked about"
    );
    let ready = plan
        .resolve(&[
            (asked[0].1, Choice::Replace),
            (asked[1].1, Choice::AutoRename),
        ])
        .expect("resolved");
    let report = step(session.run(false, &ready[0], &cancel, |_| {}))
        .await
        .expect("ran");
    assert_eq!(report.skipped, 0);
    assert_eq!(std::fs::read(there.join("a.txt")).expect("a"), b"new a");
    assert_eq!(std::fs::read(there.join("sub/b.txt")).expect("b"), b"old b");
    assert_eq!(
        std::fs::read(there.join("sub/b (copy).txt")).expect("copy"),
        b"new b"
    );
    assert_eq!(std::fs::read(there.join("sub/c.txt")).expect("c"), b"new c");
}

#[tokio::test]
async fn a_download_never_writes_inside_a_folder_whose_name_a_local_file_takes() {
    use heimdall_files::conflict::{Choice, Kind};
    let Some((_server, session)) = start().await else {
        return;
    };
    let local = tempfile::tempdir().expect("local");
    let server = tempfile::tempdir().expect("server");
    let docs = server.path().join("docs");
    std::fs::create_dir_all(docs.join("deep")).expect("folders");
    std::fs::write(docs.join("deep/x.txt"), b"x").expect("x");
    std::fs::write(server.path().join("note.txt"), b"theirs").expect("note");
    std::fs::write(local.path().join("docs"), b"a file").expect("in the way");
    std::fs::write(local.path().join("note.txt"), b"mine").expect("mine");
    let cancel = tokio_util::sync::CancellationToken::new();
    let roots = [
        root(&docs, &local.path().join("docs"), Kind::Folder),
        root(
            &server.path().join("note.txt"),
            &local.path().join("note.txt"),
            Kind::File,
        ),
    ];

    let plan = step(session.plan_download(&roots, &cancel))
        .await
        .expect("planned");
    let conflicts: Vec<_> = plan
        .conflicts()
        .map(|(index, step, checked)| (index, shown(&step.target), checked.allowed))
        .collect();
    assert_eq!(conflicts.len(), 2, "{conflicts:?}");
    let folder = conflicts
        .iter()
        .find(|(_, name, _)| name == "docs")
        .expect("the folder");
    let allowed = folder.2.expect("asked");
    assert!(
        allowed.skip && !allowed.replace && !allowed.rename,
        "skip only"
    );
    let note = conflicts
        .iter()
        .find(|(_, name, _)| name == "note.txt")
        .expect("the file");
    assert!(
        plan.resolve(&[(folder.0, Choice::Skip)]).is_err(),
        "every conflict needs an answer"
    );
    let ready = plan
        .resolve(&[(folder.0, Choice::Skip), (note.0, Choice::Skip)])
        .expect("resolved");
    assert!(ready.iter().all(Vec::is_empty), "{ready:?}");
    assert_eq!(
        std::fs::read(local.path().join("docs")).expect("file"),
        b"a file"
    );
    assert_eq!(
        std::fs::read(local.path().join("note.txt")).expect("note"),
        b"mine"
    );
}

#[tokio::test]
async fn a_file_is_read_whole_and_replaced_only_while_unchanged_keeping_its_mode() {
    use heimdall_files::RemoteError;
    use tokio_util::sync::CancellationToken;

    let Some((_server, session)) = start().await else {
        return;
    };
    let cancel = CancellationToken::new();
    let dir = tempfile::tempdir().expect("dir");
    let file = dir.path().join("config.ini");
    std::fs::write(&file, b"old").expect("file");
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o640)).expect("mode");

    let (data, fingerprint) = step(session.read_whole(&remote(&file), 1024, &cancel))
        .await
        .expect("read");
    assert_eq!(data, b"old");
    assert_eq!(
        step(session.fingerprint(&remote(&file))).await,
        Ok(fingerprint),
        "the fingerprint of what was read"
    );
    assert_eq!(
        step(session.read_whole(&remote(&file), 2, &cancel)).await,
        Err(RemoteError::FileTooLarge)
    );

    let replaced = step(session.replace_if(&remote(&file), b"new content", &fingerprint, &cancel))
        .await
        .expect("replaced");
    assert_eq!(std::fs::read(&file).expect("read back"), b"new content");
    assert_eq!(
        std::fs::metadata(&file)
            .expect("there")
            .permissions()
            .mode()
            & 0o777,
        0o640,
        "its mode kept"
    );
    assert_eq!(replaced.size, Some(11));

    // Changed on the server meanwhile: left as it is.
    std::fs::write(&file, b"someone else's").expect("changed");
    assert_eq!(
        step(session.replace_if(&remote(&file), b"mine", &replaced, &cancel)).await,
        Err(RemoteError::Changed)
    );
    assert_eq!(std::fs::read(&file).expect("kept"), b"someone else's");
    let leftovers = std::fs::read_dir(dir.path())
        .expect("listed")
        .filter(|entry| {
            entry
                .as_ref()
                .is_ok_and(|entry| entry.file_name().to_string_lossy().contains("heimdall"))
        })
        .count();
    assert_eq!(leftovers, 0, "no temporary file left");

    // A folder is not read as a file.
    assert_eq!(
        step(session.read_whole(&remote(dir.path()), 1024, &cancel)).await,
        Err(RemoteError::NotAFile)
    );
}
