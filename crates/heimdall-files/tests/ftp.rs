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

//! The Files session over FTP, against an FTP server run in the test on a temporary folder:
//! what the Files tab relies on, the FTP way.

use std::net::{Ipv4Addr, TcpListener};
use std::path::Path;
use std::time::Duration;

use heimdall_files::{
    FtpClient, FtpSecurity, FtpTarget, ItemKind, RemoteError, RemotePath, RemoteSession,
};
use tokio_util::sync::CancellationToken;
use unftp_sbe_fs::Filesystem;

/// Longest wait for the test server, or for one operation.
const STEP: Duration = Duration::from_secs(20);

/// Tries to start a server this many times: a free port can be taken before it binds.
const START_TRIES: usize = 5;

/// Serves `root` over FTP on a free local port; returns the port.
async fn serve(root: &Path) -> u16 {
    for _ in 0..START_TRIES {
        let port = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .expect("free port")
            .local_addr()
            .expect("address")
            .port();
        let home = root.to_owned();
        let server = libunftp::ServerBuilder::new(Box::new(move || {
            Filesystem::new(home.clone()).expect("root")
        }))
        .build()
        .expect("server");
        tokio::spawn(server.listen(format!("127.0.0.1:{port}")));
        let started = tokio::time::timeout(STEP, async {
            loop {
                if tokio::net::TcpStream::connect((Ipv4Addr::LOCALHOST, port))
                    .await
                    .is_ok()
                {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await;
        if started.is_ok() {
            return port;
        }
    }
    panic!("no FTP server started");
}

async fn session(root: &Path) -> RemoteSession {
    let port = serve(root).await;
    let client = FtpClient::connect(&FtpTarget {
        host: "127.0.0.1".to_owned(),
        port,
        username: None,
        password: String::new(),
        passive: true,
        security: FtpSecurity::Plain,
        timeout: STEP,
    })
    .await
    .expect("connected");
    RemoteSession::Ftp(client)
}

fn names(items: &[heimdall_files::RemoteItem]) -> Vec<String> {
    let mut names: Vec<String> = items
        .iter()
        .map(|item| String::from_utf8_lossy(&item.name).into_owned())
        .collect();
    names.sort();
    names
}

#[tokio::test]
async fn a_folder_is_listed_created_and_renamed_without_replacing() {
    let root = tempfile::tempdir().expect("root");
    std::fs::write(root.path().join("notes.txt"), b"hello").expect("file");
    std::fs::create_dir(root.path().join("logs")).expect("folder");
    let session = session(root.path()).await;
    let top = session
        .canonical(&RemotePath::from("."))
        .await
        .expect("current folder");
    let listed = session.list(&top).await.expect("listed");
    assert_eq!(names(&listed), ["logs", "notes.txt"]);
    let notes = listed
        .iter()
        .find(|item| item.name == b"notes.txt")
        .expect("notes");
    assert_eq!((notes.kind, notes.size), (ItemKind::File, Some(5)));
    let logs = listed
        .iter()
        .find(|item| item.name == b"logs")
        .expect("logs");
    assert_eq!(logs.kind, ItemKind::Directory);

    session.make_folder(&top.join(b"made")).await.expect("made");
    assert!(root.path().join("made").is_dir());

    // An existing target is refused, never replaced.
    let refused = session
        .rename(&top.join(b"notes.txt"), &top.join(b"made"))
        .await;
    assert!(
        matches!(refused, Err(RemoteError::Refused { .. })),
        "{refused:?}"
    );
    assert!(root.path().join("notes.txt").is_file());
    // Onto an existing file too, which a server may replace itself.
    std::fs::write(root.path().join("taken.txt"), b"taken").expect("file");
    let refused = session
        .rename(&top.join(b"notes.txt"), &top.join(b"taken.txt"))
        .await;
    assert!(
        matches!(refused, Err(RemoteError::Refused { .. })),
        "{refused:?}"
    );
    assert_eq!(
        std::fs::read(root.path().join("taken.txt")).expect("read"),
        b"taken"
    );
    session
        .rename(&top.join(b"notes.txt"), &top.join(b"renamed.txt"))
        .await
        .expect("renamed");
    assert_eq!(
        std::fs::read(root.path().join("renamed.txt")).expect("read"),
        b"hello"
    );
}

#[tokio::test]
async fn a_folder_is_removed_with_everything_in_it() {
    let root = tempfile::tempdir().expect("root");
    let tree = root.path().join("tree");
    std::fs::create_dir_all(tree.join("a/b")).expect("folders");
    std::fs::write(tree.join("a/b/deep.txt"), b"x").expect("file");
    std::fs::write(tree.join("top.txt"), b"y").expect("file");
    let session = session(root.path()).await;
    let top = session
        .canonical(&RemotePath::from("."))
        .await
        .expect("top");
    session.remove(&top.join(b"tree")).await.expect("removed");
    assert!(!tree.exists());
}

#[tokio::test]
async fn a_file_goes_up_and_down_whole_and_an_upload_never_replaces_unasked() {
    let root = tempfile::tempdir().expect("root");
    let local = tempfile::tempdir().expect("local");
    let payload: Vec<u8> = (0..300_000u32).map(|i| (i % 251) as u8).collect();
    let source = local.path().join("payload.bin");
    std::fs::write(&source, &payload).expect("source");
    let session = session(root.path()).await;
    let top = session
        .canonical(&RemotePath::from("."))
        .await
        .expect("top");
    let remote = top.join(b"payload.bin");
    let cancel = CancellationToken::new();

    let mut reported = 0;
    let sent = session
        .upload(&source, &remote, false, &cancel, |done| reported = done)
        .await
        .expect("uploaded");
    assert_eq!(
        (sent, reported),
        (payload.len() as u64, payload.len() as u64)
    );
    assert_eq!(
        std::fs::read(root.path().join("payload.bin")).expect("read"),
        payload
    );
    let hidden: Vec<_> = std::fs::read_dir(root.path())
        .expect("listed")
        .map(|entry| entry.expect("entry").file_name())
        .collect();
    assert_eq!(hidden.len(), 1, "no temporary file left: {hidden:?}");

    // Not asked to replace: refused, the file untouched.
    std::fs::write(&source, b"other").expect("changed");
    let refused = session
        .upload(&source, &remote, false, &cancel, |_| {})
        .await;
    assert!(
        matches!(refused, Err(RemoteError::Refused { .. })),
        "{refused:?}"
    );
    assert_eq!(
        std::fs::read(root.path().join("payload.bin")).expect("read"),
        payload
    );
    session
        .upload(&source, &remote, true, &cancel, |_| {})
        .await
        .expect("replaced");
    assert_eq!(
        std::fs::read(root.path().join("payload.bin")).expect("read"),
        b"other"
    );

    let target = local.path().join("back.bin");
    let got = session
        .download(&remote, &target, &cancel, |_| {})
        .await
        .expect("downloaded");
    assert_eq!(got, 5);
    assert_eq!(std::fs::read(&target).expect("read"), b"other");
    assert!(!local.path().join("back.bin.heimdall-part").exists());
}

#[tokio::test]
async fn only_a_regular_file_is_downloaded() {
    let root = tempfile::tempdir().expect("root");
    std::fs::create_dir(root.path().join("folder")).expect("folder");
    let local = tempfile::tempdir().expect("local");
    let session = session(root.path()).await;
    let top = session
        .canonical(&RemotePath::from("."))
        .await
        .expect("top");
    let refused = session
        .download(
            &top.join(b"folder"),
            &local.path().join("folder"),
            &CancellationToken::new(),
            |_| {},
        )
        .await;
    assert_eq!(refused, Err(RemoteError::NotAFile));
}

#[tokio::test]
async fn a_folder_goes_up_and_comes_back_whole() {
    let root = tempfile::tempdir().expect("root");
    let local = tempfile::tempdir().expect("local");
    let source = local.path().join("site");
    std::fs::create_dir_all(source.join("css")).expect("folders");
    std::fs::write(source.join("index.html"), b"<html>").expect("file");
    std::fs::write(source.join("css/site.css"), b"body{}").expect("file");
    let session = session(root.path()).await;
    let top = session
        .canonical(&RemotePath::from("."))
        .await
        .expect("top");
    let cancel = CancellationToken::new();
    let up = session
        .upload_folder(&source, &top.join(b"site"), &cancel, |_| {})
        .await
        .expect("uploaded");
    assert_eq!(up.skipped, 0);
    assert_eq!(
        std::fs::read(root.path().join("site/css/site.css")).expect("read"),
        b"body{}"
    );
    let back = local.path().join("back");
    let down = session
        .download_folder(&top.join(b"site"), &back, &cancel, |_| {})
        .await
        .expect("downloaded");
    assert_eq!(down.skipped, 0);
    assert_eq!(
        std::fs::read(back.join("index.html")).expect("read"),
        b"<html>"
    );
    assert_eq!(
        std::fs::read(back.join("css/site.css")).expect("read"),
        b"body{}"
    );
}

#[tokio::test]
async fn a_stopped_download_resumes_where_it_stopped_while_the_file_is_unchanged() {
    let root = tempfile::tempdir().expect("root");
    let local = tempfile::tempdir().expect("local");
    let payload: Vec<u8> = (0..2_000_000u32).map(|i| (i % 253) as u8).collect();
    std::fs::write(root.path().join("big.bin"), &payload).expect("remote file");
    let session = session(root.path()).await;
    let top = session
        .canonical(&RemotePath::from("."))
        .await
        .expect("top");
    let remote = top.join(b"big.bin");
    let target = local.path().join("big.bin");

    // Stopped after its first bytes: the part file stays.
    let cancel = CancellationToken::new();
    let stopped = session
        .download(&remote, &target, &cancel, |_| cancel.cancel())
        .await;
    assert_eq!(stopped, Err(RemoteError::Cancelled));
    let part = local.path().join("big.bin.heimdall-part");
    let kept = std::fs::metadata(&part).expect("part kept").len();
    assert!(kept > 0 && kept < payload.len() as u64, "{kept}");
    assert!(
        !target.exists(),
        "never under the real name before it is whole"
    );

    // Resumed: the first report is already past what was kept.
    let mut first = None;
    let got = session
        .download(&remote, &target, &CancellationToken::new(), |done| {
            first.get_or_insert(done);
        })
        .await
        .expect("resumed");
    assert_eq!(got, payload.len() as u64);
    assert!(
        first.expect("reported") > kept,
        "resumed after {kept} bytes"
    );
    assert_eq!(std::fs::read(&target).expect("read"), payload);
    assert!(!part.exists());
}

#[tokio::test]
async fn a_file_changed_since_the_stop_is_downloaded_again_from_the_start() {
    let root = tempfile::tempdir().expect("root");
    let local = tempfile::tempdir().expect("local");
    let first: Vec<u8> = (0..2_000_000u32).map(|i| (i % 253) as u8).collect();
    std::fs::write(root.path().join("big.bin"), &first).expect("remote file");
    let session = session(root.path()).await;
    let top = session
        .canonical(&RemotePath::from("."))
        .await
        .expect("top");
    let remote = top.join(b"big.bin");
    let target = local.path().join("big.bin");
    let cancel = CancellationToken::new();
    let _ = session
        .download(&remote, &target, &cancel, |_| cancel.cancel())
        .await;
    assert!(local.path().join("big.bin.heimdall-part").exists());

    // Another file under the same name: a resume would splice two files together.
    let second: Vec<u8> = (0..1_500_000u32).map(|i| (i % 241) as u8).collect();
    std::fs::write(root.path().join("big.bin"), &second).expect("changed");
    let got = session
        .download(&remote, &target, &CancellationToken::new(), |_| {})
        .await
        .expect("downloaded");
    assert_eq!(got, second.len() as u64);
    assert_eq!(std::fs::read(&target).expect("read"), second);
}

#[cfg(unix)]
#[tokio::test]
async fn removing_a_link_removes_the_link_and_never_what_it_points_to() {
    let root = tempfile::tempdir().expect("root");
    let kept = root.path().join("kept");
    std::fs::create_dir(&kept).expect("folder");
    std::fs::write(kept.join("precious.txt"), b"keep").expect("file");
    std::os::unix::fs::symlink(&kept, root.path().join("link")).expect("link");
    let session = session(root.path()).await;
    let top = session
        .canonical(&RemotePath::from("."))
        .await
        .expect("top");
    let result = session.remove(&top.join(b"link")).await;
    assert!(
        kept.join("precious.txt").is_file(),
        "what the link points to is untouched: {result:?}"
    );
}
