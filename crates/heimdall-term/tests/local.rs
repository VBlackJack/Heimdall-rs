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

//! Local shells on a pseudo-terminal, run for real: `sh` on Unix, `cmd` on Windows.

use std::time::Duration;

use heimdall_term::local::{LocalArguments, LocalConfig, LocalEvent, LocalSession, spawn};

/// Bound on anything the test waits for.
const WAIT: Duration = Duration::from_secs(15);

fn config(program: &str, args: &[&str]) -> LocalConfig {
    LocalConfig {
        program: Some(program.to_owned()),
        arguments: LocalArguments::List(args.iter().map(|arg| (*arg).to_owned()).collect()),
        working_directory: None,
        columns: 80,
        rows: 24,
    }
}

#[cfg(unix)]
fn shell(script: &str) -> LocalConfig {
    config("/bin/sh", &["-c", script])
}

#[cfg(windows)]
fn shell(script: &str) -> LocalConfig {
    config("cmd.exe", &["/C", script])
}

/// The output until the exit, and the exit code.
async fn until_exit(session: &mut LocalSession) -> (String, Option<i32>) {
    let mut output = Vec::new();
    loop {
        match tokio::time::timeout(WAIT, session.events.recv())
            .await
            .expect("in time")
            .expect("an event")
        {
            LocalEvent::Output(bytes) => output.extend(bytes),
            LocalEvent::Exited(code) => {
                return (String::from_utf8_lossy(&output).into_owned(), code);
            }
        }
    }
}

#[tokio::test]
async fn a_relative_program_or_a_nul_is_refused_before_anything_runs() {
    for refused in [
        config("./tool", &[]),
        config("bin/tool", &[]),
        config("cmd\"x", &[]),
        config("sh", &["-c", "echo a\0b"]),
    ] {
        let error = spawn(&refused).expect_err("refused");
        assert_eq!(
            error.kind(),
            std::io::ErrorKind::InvalidInput,
            "{refused:?}"
        );
    }
}

#[tokio::test]
async fn a_name_found_nowhere_is_refused() {
    let error = spawn(&config("heimdall-no-such-shell", &[])).expect_err("refused");
    assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
}

/// A copy of `cmd.exe` named `powershell.exe` next to the test program: the first place the
/// `CreateProcessW` search looks. Removed when dropped.
#[cfg(windows)]
struct Planted(std::path::PathBuf);

#[cfg(windows)]
impl Drop for Planted {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[cfg(windows)]
#[tokio::test]
async fn the_default_shell_is_system_powershell_even_with_one_planted_beside_heimdall() {
    let beside = std::env::current_exe()
        .expect("test program")
        .with_file_name("powershell.exe");
    let root = std::env::var_os("SystemRoot").expect("SystemRoot");
    std::fs::copy(
        std::path::Path::new(&root).join(r"System32\cmd.exe"),
        &beside,
    )
    .expect("planted");
    let _planted = Planted(beside);
    let mut session = spawn(&LocalConfig {
        columns: 80,
        rows: 24,
        ..LocalConfig::default()
    })
    .expect("spawned");
    // Only PowerShell turns this into an exit code; `cmd` would reject the line.
    session
        .input
        .write(b"exit (40 + $PSVersionTable.PSVersion.Major)\r".to_vec())
        .expect("typed");
    let (output, code) = until_exit(&mut session).await;
    assert_eq!(code, Some(45), "Windows PowerShell 5 expected: {output:?}");
}

/// Whether the module path of this process holds a `PowerShell` 7 home's module folder, as it
/// does when the tests run from `pwsh`: only then can the next test tell the fix apart.
#[cfg(windows)]
fn inherits_powershell_7_modules() -> bool {
    std::env::var("PSModulePath").is_ok_and(|path| {
        path.split(';').any(|entry| {
            std::path::Path::new(entry.trim_end_matches('\\'))
                .parent()
                .is_some_and(|home| home.join("pwsh.exe").is_file())
        })
    })
}

#[cfg(windows)]
#[tokio::test]
async fn windows_powershell_gets_no_powershell_7_module_folder_and_loads_its_own_cmdlets() {
    if !inherits_powershell_7_modules() {
        eprintln!("PSModulePath holds no PowerShell 7 folder here: the run checks the rest only");
    }
    let mut session = spawn(&LocalConfig {
        columns: 120,
        rows: 24,
        ..LocalConfig::default()
    })
    .expect("spawned");
    // .NET only up to the exit: a module path that breaks cmdlets cannot break the probe.
    // Tens: PowerShell 7 module folders seen; units: Get-FileHash, of the Utility module,
    // missing.
    let mut line = br"$p = 0; foreach ($e in $env:PSModulePath.Split(';')) { if ($e -and [IO.File]::Exists([IO.Path]::Combine([IO.Path]::GetDirectoryName($e.TrimEnd('\')), 'pwsh.exe'))) { $p++ } }; $h = 0; try { $null = Get-FileHash -LiteralPath ([IO.Path]::Combine($env:SystemRoot, 'win.ini')) -ErrorAction Stop } catch { $h = 1 }; exit (10 * $p + $h)".to_vec();
    line.push(b'\r');
    session.input.write(line).expect("typed");
    let (output, code) = until_exit(&mut session).await;
    assert_eq!(code, Some(0), "{output:?}");
}

#[tokio::test]
async fn the_exit_code_is_reported() {
    let mut session = spawn(&shell("exit 7")).expect("spawned");
    assert_eq!(until_exit(&mut session).await.1, Some(7));
}

#[tokio::test]
async fn the_output_comes_before_the_exit() {
    #[cfg(unix)]
    let script = "printf hello; exit 0";
    #[cfg(windows)]
    let script = "echo hello";
    let mut session = spawn(&shell(script)).expect("spawned");
    let (output, code) = until_exit(&mut session).await;
    assert!(output.contains("hello"), "{output:?}");
    assert_eq!(code, Some(0));
}

#[cfg(unix)]
#[tokio::test]
async fn output_left_unread_at_the_exit_still_arrives() {
    // Paced lines, one read each, fill the event queue while nobody reads it: the session
    // waits on a full queue when the shell writes its last word and exits.
    let mut session = spawn(&shell(
        "for i in $(seq 1 80); do echo line$i; sleep 0.02; done; printf END; exit 0",
    ))
    .expect("spawned");
    tokio::time::sleep(Duration::from_secs(4)).await;
    let (output, code) = until_exit(&mut session).await;
    assert!(output.contains("line80"), "{output:?}");
    assert!(output.ends_with("END"), "{output:?}");
    assert_eq!(code, Some(0));
}

#[cfg(unix)]
#[tokio::test]
async fn a_background_job_does_not_hold_the_exit_back() {
    // The job keeps the terminal open: the exit must come from the shell, not the terminal.
    let mut session = spawn(&shell("sleep 30 & exit 3")).expect("spawned");
    let started = std::time::Instant::now();
    assert_eq!(until_exit(&mut session).await.1, Some(3));
    assert!(started.elapsed() < Duration::from_secs(10));
}

#[cfg(unix)]
#[tokio::test]
async fn the_shell_is_told_its_terminal() {
    let mut session = spawn(&shell("echo $TERM $COLORTERM")).expect("spawned");
    let (output, _) = until_exit(&mut session).await;
    assert!(output.contains("xterm-256color truecolor"), "{output:?}");
}

#[cfg(unix)]
#[tokio::test]
async fn typed_input_reaches_the_shell() {
    let mut session = spawn(&shell("read line; echo got:$line")).expect("spawned");
    // Enter, as a terminal sends it.
    session.input.write(b"abc\r".to_vec()).expect("typed");
    let (output, code) = until_exit(&mut session).await;
    assert!(output.contains("got:abc"), "{output:?}");
    assert_eq!(code, Some(0));
}

#[cfg(unix)]
#[tokio::test]
async fn a_resize_reaches_the_shell() {
    let mut session = spawn(&shell("sleep 1; stty size")).expect("spawned");
    session.input.resize(100, 40).expect("resized");
    let (output, _) = until_exit(&mut session).await;
    assert!(output.contains("40 100"), "rows then columns: {output:?}");
}

/// Output until `marker` shows.
#[cfg(unix)]
async fn until(session: &mut LocalSession, marker: &str) {
    let mut output = Vec::new();
    while !String::from_utf8_lossy(&output).contains(marker) {
        match tokio::time::timeout(WAIT, session.events.recv())
            .await
            .expect("in time")
            .expect("an event")
        {
            LocalEvent::Output(bytes) => output.extend(bytes),
            LocalEvent::Exited(code) => panic!("exited early: {code:?}"),
        }
    }
}

#[cfg(unix)]
#[tokio::test]
async fn a_large_paste_goes_through_whole() {
    let size = 200_000;
    // Raw mode first: a line in canonical mode holds 4096 bytes and drops the rest.
    let mut session = spawn(&shell(&format!(
        "stty raw -echo; echo ready; head -c {size} | wc -c"
    )))
    .expect("spawned");
    until(&mut session, "ready").await;
    session.input.write(vec![b'x'; size]).expect("pasted");
    let (output, _) = until_exit(&mut session).await;
    assert!(output.contains(&size.to_string()), "{output:?}");
}

#[cfg(unix)]
#[tokio::test]
async fn closing_ends_even_a_shell_that_ignores_the_hang_up() {
    let mut session = spawn(&shell("trap '' HUP; echo ready; sleep 100")).expect("spawned");
    // Once the trap is in place.
    until(&mut session, "ready").await;
    let started = std::time::Instant::now();
    session.input.close();
    let (_, code) = until_exit(&mut session).await;
    assert_eq!(code, None, "killed, so no exit code");
    assert!(
        started.elapsed() < Duration::from_secs(8),
        "{:?}",
        started.elapsed()
    );
}
