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

//! A server's file read and replaced with sudo over an SSH connection: the in-process
//! server runs the scripts with this computer's `sh`, and a stand-in `sudo` answers.

#![cfg(unix)]

#[path = "../../heimdall-ssh/tests/common/mod.rs"]
mod ssh;

use std::os::unix::ffi::OsStrExt as _;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use heimdall_app::files::FilesError;
use heimdall_app::sudo_edit::{sudo_read, sudo_replace};
use heimdall_files::RemotePath;
use heimdall_files::privileged::Sudo;
use heimdall_ssh::{Connection, establish};
use tokio_util::sync::CancellationToken;

const PASSWORD: &str = "s3cret";

/// A stand-in for sudo, beside a `password` file; with a `nopasswd` file it asks none.
const FAKE_SUDO: &str = r#"#!/bin/sh
dir=$(dirname "$0")
n=0
while [ $# -gt 0 ]; do
  case "$1" in
    -n) n=1 ;;
    -S|-k) ;;
    -p) shift ;;
    --) shift; break ;;
    *) break ;;
  esac
  shift
done
[ -f "$dir/nopasswd" ] && exec "$@"
if [ "$n" = 1 ]; then echo "sudo: a password is required" >&2; exit 1; fi
IFS= read -r line || { echo "sudo: no password was provided" >&2; exit 1; }
if [ "$line" != "$(cat "$dir/password")" ]; then
  echo "Sorry, try again." >&2
  echo "sudo: 1 incorrect password attempt" >&2
  exit 1
fi
exec "$@"
"#;

async fn shell(server: &ssh::TestServer, dir: &Path) -> Connection {
    let options = ssh::options_trusting(dir, server.port, "host-ed25519");
    let prompter = Arc::new(ssh::ScriptedPrompter::passwords(&[ssh::PASSWORD]));
    tokio::time::timeout(
        ssh::STEP_TIMEOUT,
        establish(
            &ssh::profile(server.port, None),
            &options,
            prompter,
            CancellationToken::new(),
        ),
    )
    .await
    .expect("in time")
    .expect("connected")
}

fn fake_sudo(dir: &Path) -> PathBuf {
    let bin = dir.join("bin");
    std::fs::create_dir(&bin).expect("bin");
    let sudo = bin.join("sudo");
    std::fs::write(&sudo, FAKE_SUDO).expect("sudo");
    std::fs::set_permissions(&sudo, std::fs::Permissions::from_mode(0o755)).expect("mode");
    std::fs::write(bin.join("password"), PASSWORD).expect("password");
    sudo
}

fn remote(path: &Path) -> RemotePath {
    RemotePath::from_bytes(path.as_os_str().as_bytes())
}

#[tokio::test]
async fn a_root_file_is_read_and_saved_with_sudo_over_ssh_only_over_what_was_read() {
    let server = ssh::start(ssh::Spec::default()).await;
    let keys = tempfile::tempdir().expect("keys");
    let connection = shell(&server, keys.path()).await;
    let dir = tempfile::tempdir().expect("dir");
    let sudo_path = fake_sudo(dir.path());
    let sudo = Sudo::Unchecked(sudo_path.to_str().expect("utf-8"));
    let file = dir.path().join("sshd_config");
    std::fs::write(&file, b"PermitRootLogin yes\n").expect("file");
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).expect("mode");

    assert_eq!(
        sudo_read(&connection, &remote(&file), None, sudo).await,
        Err(FilesError::SudoPasswordNeeded),
        "asks for the password"
    );
    assert_eq!(
        sudo_read(&connection, &remote(&file), Some(b"wrong"), sudo).await,
        Err(FilesError::SudoPasswordRejected)
    );
    let (content, opened) = sudo_read(&connection, &remote(&file), Some(PASSWORD.as_bytes()), sudo)
        .await
        .expect("read");
    assert_eq!(content, b"PermitRootLogin yes\n");

    sudo_replace(
        &connection,
        &remote(&file),
        b"PermitRootLogin no\n",
        &opened,
        Some(PASSWORD.as_bytes()),
        sudo,
    )
    .await
    .expect("saved");
    assert_eq!(
        std::fs::read(&file).expect("saved"),
        b"PermitRootLogin no\n"
    );
    assert_eq!(
        std::fs::metadata(&file)
            .expect("there")
            .permissions()
            .mode()
            & 0o777,
        0o600,
        "its mode kept"
    );

    // Saved again over what was first read: the file has changed since, left as it is.
    assert_eq!(
        sudo_replace(
            &connection,
            &remote(&file),
            b"PermitRootLogin prohibit-password\n",
            &opened,
            Some(PASSWORD.as_bytes()),
            sudo,
        )
        .await,
        Err(FilesError::ChangedOnServer)
    );
    assert_eq!(std::fs::read(&file).expect("kept"), b"PermitRootLogin no\n");
}
