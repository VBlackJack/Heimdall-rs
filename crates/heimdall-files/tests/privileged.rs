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

//! The sudo replace script: what it holds as written, and, run by this computer's `sh`
//! against a stand-in `sudo`, what it does.

use heimdall_files::privileged::{Sudo, replace_script};

const TOKEN: [u8; 16] = [0x5a; 16];

fn sha256(data: &[u8]) -> [u8; 32] {
    let digest = ring::digest::digest(&ring::digest::SHA256, data);
    let mut out = [0; 32];
    out.copy_from_slice(digest.as_ref());
    out
}

#[test]
fn the_script_never_holds_the_password_its_success_line_or_an_unchecked_sudo() {
    let password = b"correct horse battery";
    let copy = replace_script(
        b"/etc/nginx/nginx.conf",
        b"worker_processes 4;\n",
        &sha256(b"old"),
        Some(password),
        TOKEN,
        Sudo::System,
    )
    .expect("script");
    let holds = |needle: &[u8]| copy.script.windows(needle.len()).any(|w| w == needle);
    assert!(!holds(password), "the password is never written as it is");
    assert!(!holds(b"worker_processes"), "nor the content");
    assert!(
        !holds(&copy.done[..copy.done.len() - 1]),
        "nor the success line"
    );
    assert!(holds(b"[ -u \"$s\" ]"), "sudo checked set-user-id");
    assert!(holds(b"PATH=/usr/sbin:/usr/bin:/sbin:/bin"));
    assert!(
        replace_script(
            b"/etc/a",
            b"",
            &[0; 32],
            Some(b"two\nlines"),
            TOKEN,
            Sudo::System
        )
        .is_err(),
        "a password sudo would read as two lines"
    );
}

#[cfg(unix)]
mod run {
    use std::io::Write as _;
    use std::os::unix::fs::PermissionsExt as _;
    use std::path::{Path, PathBuf};
    use std::process::{Command, Output, Stdio};

    use super::{TOKEN, sha256};
    use heimdall_files::privileged::{
        AUTHENTICATION, CHANGED, NOT_A_FILE, PASSWORD_NEEDED, Sudo, TOO_LARGE, read_output,
        read_script, replace_script,
    };

    const PASSWORD: &str = "s3cret";

    /// A stand-in for sudo, beside a `password` file; with a `nopasswd` file it asks none.
    /// A wrong password makes it read one more line, as sudo does: what it reads then is
    /// kept in `extra`, which must never be the file's content.
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
  if IFS= read -r again; then printf '%s\n' "$again" >> "$dir/extra"; fi
  echo "sudo: 1 incorrect password attempt" >&2
  exit 1
fi
exec "$@"
"#;

    struct Bench {
        _dir: tempfile::TempDir,
        bin: PathBuf,
        file: PathBuf,
    }

    fn bench(content: &[u8], mode: u32) -> Bench {
        let dir = tempfile::tempdir().expect("dir");
        let bin = dir.path().join("bin");
        std::fs::create_dir(&bin).expect("bin");
        let sudo = bin.join("sudo");
        std::fs::write(&sudo, FAKE_SUDO).expect("sudo");
        std::fs::set_permissions(&sudo, std::fs::Permissions::from_mode(0o755)).expect("mode");
        std::fs::write(bin.join("password"), PASSWORD).expect("password");
        let etc = dir.path().join("etc");
        std::fs::create_dir(&etc).expect("etc");
        let file = etc.join("app.conf");
        std::fs::write(&file, content).expect("file");
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(mode)).expect("mode");
        Bench {
            _dir: dir,
            bin,
            file,
        }
    }

    fn save(bench: &Bench, content: &[u8], opened: &[u8], password: Option<&str>) -> Output {
        use std::os::unix::ffi::OsStrExt as _;
        let sudo = bench.bin.join("sudo");
        let copy = replace_script(
            bench.file.as_os_str().as_bytes(),
            content,
            &sha256(opened),
            password.map(str::as_bytes),
            TOKEN,
            Sudo::Unchecked(sudo.to_str().expect("utf-8")),
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
        input.write_all(&copy.script).expect("script");
        drop(input);
        let output = child.wait_with_output().expect("ran");
        if output.status.success() {
            assert_eq!(output.stdout, copy.done, "done says so");
        } else {
            assert!(output.stdout.is_empty(), "never done when it is not");
        }
        output
    }

    /// Runs `script` as the server would: `sh -s` reading it on its input.
    fn run(script: &[u8]) -> Output {
        let mut child = Command::new("sh")
            .arg("-s")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("sh");
        let mut input = child.stdin.take().expect("stdin");
        input.write_all(script).expect("script");
        drop(input);
        child.wait_with_output().expect("ran")
    }

    /// Reads the bench's file through sudo, `cap` bytes at most: its content, or the exit
    /// status.
    fn read(bench: &Bench, cap: u64, password: Option<&str>) -> Result<Vec<u8>, Option<i32>> {
        use std::os::unix::ffi::OsStrExt as _;
        let sudo = bench.bin.join("sudo");
        let read = read_script(
            bench.file.as_os_str().as_bytes(),
            cap,
            password.map(str::as_bytes),
            TOKEN,
            Sudo::Unchecked(sudo.to_str().expect("utf-8")),
        )
        .expect("script");
        let output = run(&read.script);
        if output.status.success() {
            Ok(read_output(&output.stdout, &read.done).expect("the content, then done"))
        } else {
            assert!(output.stdout.is_empty() || read_output(&output.stdout, &read.done).is_none());
            Err(output.status.code())
        }
    }

    fn mode(path: &Path) -> u32 {
        std::fs::metadata(path).expect("there").permissions().mode() & 0o7777
    }

    fn work_left(file: &Path) -> bool {
        std::fs::read_dir(file.parent().expect("folder"))
            .expect("listed")
            .flatten()
            .any(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .contains(".heimdall-write")
            })
    }

    #[test]
    fn a_file_is_replaced_with_the_password_keeping_its_mode_and_nothing_left() {
        let bench = bench(b"listen 80;\n", 0o640);
        let output = save(&bench, b"listen 443;\n", b"listen 80;\n", Some(PASSWORD));
        assert!(output.status.success(), "{output:?}");
        assert_eq!(std::fs::read(&bench.file).expect("saved"), b"listen 443;\n");
        assert_eq!(mode(&bench.file), 0o640);
        assert!(!work_left(&bench.file));
        assert!(!bench.bin.join("extra").exists());
    }

    #[test]
    fn a_wrong_password_is_one_attempt_and_the_content_is_never_read_as_another() {
        let bench = bench(b"old\n", 0o644);
        let output = save(&bench, b"secret line 1\nline 2\n", b"old\n", Some("wrong"));
        assert_eq!(
            output.status.code(),
            i32::try_from(AUTHENTICATION).ok(),
            "{output:?}"
        );
        assert_eq!(std::fs::read(&bench.file).expect("kept"), b"old\n");
        assert!(
            !bench.bin.join("extra").exists(),
            "nothing after the password was there to be read"
        );
        assert!(String::from_utf8_lossy(&output.stderr).contains("incorrect password"));
    }

    #[test]
    fn without_a_password_sudo_asking_one_is_said_and_one_asking_none_works() {
        let bench = bench(b"old\n", 0o644);
        let output = save(&bench, b"new\n", b"old\n", None);
        assert_eq!(
            output.status.code(),
            i32::try_from(PASSWORD_NEEDED).ok(),
            "{output:?}"
        );
        assert_eq!(std::fs::read(&bench.file).expect("kept"), b"old\n");

        std::fs::write(bench.bin.join("nopasswd"), b"").expect("nopasswd");
        let output = save(&bench, b"new\n", b"old\n", None);
        assert!(output.status.success(), "{output:?}");
        assert_eq!(std::fs::read(&bench.file).expect("saved"), b"new\n");
    }

    #[test]
    fn a_file_changed_since_opened_or_a_link_is_never_replaced() {
        let bench = bench(b"changed by someone else\n", 0o644);
        let output = save(&bench, b"mine\n", b"as I opened it\n", Some(PASSWORD));
        assert_eq!(
            output.status.code(),
            i32::try_from(CHANGED).ok(),
            "{output:?}"
        );
        assert_eq!(
            std::fs::read(&bench.file).expect("kept"),
            b"changed by someone else\n"
        );
        assert!(!work_left(&bench.file));

        let elsewhere = bench.file.with_file_name("elsewhere");
        std::fs::write(&elsewhere, b"untouched\n").expect("elsewhere");
        std::fs::remove_file(&bench.file).expect("removed");
        std::os::unix::fs::symlink(&elsewhere, &bench.file).expect("link");
        let output = save(&bench, b"mine\n", b"untouched\n", Some(PASSWORD));
        assert_eq!(
            output.status.code(),
            i32::try_from(NOT_A_FILE).ok(),
            "{output:?}"
        );
        assert_eq!(std::fs::read(&elsewhere).expect("kept"), b"untouched\n");
    }

    #[test]
    fn a_large_content_crosses_whole() {
        let bench = bench(b"old\n", 0o600);
        let content: Vec<u8> = (0..300_000u32)
            .map(|i| u8::try_from(i % 251).unwrap_or_default())
            .collect();
        let output = save(&bench, &content, b"old\n", Some(PASSWORD));
        assert!(output.status.success(), "{output:?}");
        assert_eq!(std::fs::read(&bench.file).expect("saved"), content);
    }

    #[test]
    fn a_file_is_read_whole_through_sudo_capped_and_never_through_a_link() {
        let content: Vec<u8> = (0..70_000u32)
            .map(|i| u8::try_from(i % 253).unwrap_or_default())
            .collect();
        let bench = bench(&content, 0o600);
        assert_eq!(read(&bench, 1 << 20, Some(PASSWORD)), Ok(content.clone()));
        assert_eq!(
            read(&bench, 1000, Some(PASSWORD)),
            Err(i32::try_from(TOO_LARGE).ok())
        );
        assert_eq!(
            read(&bench, 1 << 20, None),
            Err(i32::try_from(PASSWORD_NEEDED).ok())
        );
        std::fs::write(bench.bin.join("nopasswd"), b"").expect("nopasswd");
        assert_eq!(read(&bench, 1 << 20, None), Ok(content));
        assert!(!work_left(&bench.file));

        let elsewhere = bench.file.with_file_name("elsewhere");
        std::fs::write(&elsewhere, b"secret").expect("elsewhere");
        std::fs::remove_file(&bench.file).expect("removed");
        std::os::unix::fs::symlink(&elsewhere, &bench.file).expect("link");
        assert_eq!(
            read(&bench, 1 << 20, None),
            Err(i32::try_from(NOT_A_FILE).ok())
        );
    }
}
