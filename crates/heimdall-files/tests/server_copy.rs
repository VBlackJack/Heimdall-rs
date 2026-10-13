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

//! The server-side copy script: what it accepts, and, run by this computer's own `sh` on a
//! folder of its own, what it does.

use heimdall_files::server_copy::{
    CopyKind, TOKEN_LEN, Unquotable, copy_script, is_same_or_inside, random_token,
};

const TOKEN: [u8; 16] = [0xab; 16];

#[test]
fn a_path_with_a_control_character_or_none_is_refused() {
    for path in [
        &b""[..],
        b"/srv/a\nrm -rf /",
        b"/srv/\x1b[31m",
        b"/srv/a\x7f",
    ] {
        let refused = copy_script(path, b"/srv/b", CopyKind::File, TOKEN);
        let expected = if path.is_empty() {
            Unquotable::Empty
        } else {
            Unquotable::Control
        };
        assert_eq!(refused, Err(expected), "{path:?}");
        assert_eq!(
            copy_script(b"/srv/b", path, CopyKind::Folder, TOKEN),
            Err(expected)
        );
    }
}

#[test]
fn the_line_that_says_done_is_never_in_the_script_itself() {
    let copy = copy_script(b"/srv/a", b"/srv/b", CopyKind::File, TOKEN).expect("script");
    assert_eq!(
        copy.done,
        format!("copied {}\n", "ab".repeat(16)).into_bytes()
    );
    let done = &copy.done[..copy.done.len() - 1];
    assert!(
        !copy.script.windows(done.len()).any(|window| window == done),
        "an echo of the script must not pass for a copy"
    );
}

#[test]
fn a_folder_is_never_copied_into_itself() {
    assert!(is_same_or_inside(b"/srv/a", b"/srv/a"));
    assert!(is_same_or_inside(b"/srv/a", b"/srv/a/b"));
    assert!(is_same_or_inside(b"/srv/a/", b"/srv/a/b"));
    assert!(is_same_or_inside(b"/", b"/srv"));
    assert!(!is_same_or_inside(b"/srv/a", b"/srv/ab"));
    assert!(!is_same_or_inside(b"/srv/a", b"/srv"));
}

#[cfg(unix)]
mod run {
    use std::io::Write as _;
    use std::os::unix::ffi::OsStrExt as _;
    use std::os::unix::fs::PermissionsExt as _;
    use std::path::Path;
    use std::process::{Command, Output, Stdio};

    use super::TOKEN;
    use heimdall_files::server_copy::{CopyKind, RACED, copy_script};

    /// Runs the script copying `source` to `destination` as the server would, `sh -s`
    /// reading it on its input; `before` runs first, to set a trap of its own.
    fn copy(source: &Path, destination: &Path, kind: CopyKind, before: &[u8]) -> Output {
        let copy = copy_script(
            source.as_os_str().as_bytes(),
            destination.as_os_str().as_bytes(),
            kind,
            TOKEN,
        )
        .expect("script");
        let mut child = Command::new("sh")
            .arg("-s")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("sh");
        let mut input = child.stdin.take().expect("stdin");
        input.write_all(before).expect("before");
        input.write_all(&copy.script).expect("script");
        drop(input);
        let output = child.wait_with_output().expect("ran");
        if output.status.success() {
            assert_eq!(output.stdout, copy.done, "done says so");
        } else {
            assert!(output.stdout.is_empty(), "never says done when it is not");
        }
        output
    }

    fn mode(path: &Path) -> u32 {
        std::fs::symlink_metadata(path)
            .expect("there")
            .permissions()
            .mode()
            & 0o7777
    }

    fn leftovers(dir: &Path) -> Vec<String> {
        std::fs::read_dir(dir)
            .expect("listed")
            .map(|entry| {
                entry
                    .expect("entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .filter(|name| name.contains(".part"))
            .collect()
    }

    #[test]
    fn a_file_is_copied_with_its_mode_and_nothing_left_behind() {
        let dir = tempfile::tempdir().expect("dir");
        let source = dir.path().join("it's -n a file");
        std::fs::write(&source, b"content").expect("source");
        std::fs::set_permissions(&source, std::fs::Permissions::from_mode(0o640)).expect("mode");
        let destination = dir.path().join("-copy of it's");
        let output = copy(&source, &destination, CopyKind::File, b"");
        assert!(output.status.success(), "{output:?}");
        assert_eq!(std::fs::read(&destination).expect("copied"), b"content");
        assert_eq!(mode(&destination), 0o640);
        assert!(leftovers(dir.path()).is_empty());
    }

    #[test]
    fn a_file_never_replaces_what_is_there() {
        let dir = tempfile::tempdir().expect("dir");
        let source = dir.path().join("a");
        std::fs::write(&source, b"new").expect("source");
        let taken = dir.path().join("b");
        std::fs::write(&taken, b"keep").expect("taken");
        let output = copy(&source, &taken, CopyKind::File, b"");
        assert!(!output.status.success());
        assert_eq!(std::fs::read(&taken).expect("kept"), b"keep");
        assert!(leftovers(dir.path()).is_empty());

        // A link in the way is not followed nor replaced either.
        let elsewhere = dir.path().join("elsewhere");
        std::fs::write(&elsewhere, b"untouched").expect("elsewhere");
        let link = dir.path().join("link");
        std::os::unix::fs::symlink(&elsewhere, &link).expect("link");
        let output = copy(&source, &link, CopyKind::File, b"");
        assert!(!output.status.success());
        assert_eq!(std::fs::read(&elsewhere).expect("kept"), b"untouched");
    }

    #[test]
    fn a_link_raced_into_the_destination_is_removed_and_said() {
        let dir = tempfile::tempdir().expect("dir");
        let source = dir.path().join("a");
        std::fs::write(&source, b"new").expect("source");
        let destination = dir.path().join("b");
        // `ln` on this computer, standing for the race: it publishes a link instead.
        let fake = dir.path().join("bin");
        std::fs::create_dir(&fake).expect("bin");
        let ln = fake.join("ln");
        std::fs::write(
            &ln,
            format!(
                "#!/bin/sh\n/bin/ln -s -- /nowhere '{}'\n",
                destination.display()
            ),
        )
        .expect("fake");
        std::fs::set_permissions(&ln, std::fs::Permissions::from_mode(0o755)).expect("mode");
        let path = format!("PATH={}:$PATH\n", fake.display());
        let output = copy(&source, &destination, CopyKind::File, path.as_bytes());
        assert_eq!(
            output.status.code(),
            Some(i32::try_from(RACED).expect("small"))
        );
        assert!(std::fs::symlink_metadata(&destination).is_err(), "removed");
        assert!(leftovers(dir.path()).is_empty());
    }

    #[test]
    fn a_folder_is_copied_whole_and_never_into_one_that_is_there() {
        let dir = tempfile::tempdir().expect("dir");
        let source = dir.path().join("it's a folder");
        std::fs::create_dir_all(source.join("inner")).expect("tree");
        std::fs::write(source.join("inner/file"), b"deep").expect("file");
        std::fs::set_permissions(&source, std::fs::Permissions::from_mode(0o750)).expect("mode");
        let destination = dir.path().join("copy");
        let output = copy(&source, &destination, CopyKind::Folder, b"");
        assert!(output.status.success(), "{output:?}");
        assert_eq!(
            std::fs::read(destination.join("inner/file")).expect("copied"),
            b"deep"
        );
        assert_eq!(mode(&destination), 0o750, "the folder's own mode");

        std::fs::write(source.join("more"), b"more").expect("more");
        let output = copy(&source, &destination, CopyKind::Folder, b"");
        assert!(!output.status.success(), "it is there already");
        assert!(!destination.join("more").exists(), "nothing merged into it");
    }

    /// Before the script: `cp` does its work, then the script is asked to stop, as a cancel
    /// does while `cp` runs.
    const STOP_AFTER_CP: &[u8] = b"cp() { command cp \"$@\"; kill -TERM $$; }\n";

    #[test]
    fn a_stopped_copy_removes_what_it_made_even_under_a_quoted_name() {
        let dir = tempfile::tempdir().expect("dir");
        let source = dir.path().join("a");
        std::fs::write(&source, b"new").expect("source");
        let destination = dir.path().join("it's b");
        let output = copy(&source, &destination, CopyKind::File, STOP_AFTER_CP);
        assert_eq!(output.status.code(), Some(143), "{output:?}");
        assert!(!destination.exists(), "never published");
        assert!(leftovers(dir.path()).is_empty(), "staging removed");

        let folder = dir.path().join("folder");
        std::fs::create_dir(&folder).expect("folder");
        std::fs::write(folder.join("file"), b"x").expect("file");
        let copied = dir.path().join("it's a copy");
        let output = copy(&folder, &copied, CopyKind::Folder, STOP_AFTER_CP);
        assert_eq!(output.status.code(), Some(143), "{output:?}");
        assert!(!copied.exists(), "the folder it made is removed");
    }

    #[test]
    fn a_failed_folder_copy_leaves_nothing() {
        let dir = tempfile::tempdir().expect("dir");
        let source = dir.path().join("gone");
        let destination = dir.path().join("copy");
        let output = copy(&source, &destination, CopyKind::Folder, b"");
        assert!(!output.status.success());
        assert!(!destination.exists());
    }
}

#[test]
fn each_token_is_fresh_and_of_the_length_the_script_writes() {
    let first = random_token().expect("random");
    let second = random_token().expect("random");
    assert_eq!(first.len(), TOKEN_LEN);
    assert_ne!(first, second, "two draws of 128 bits");
    assert_ne!(first, [0; TOKEN_LEN]);
}
