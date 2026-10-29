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

//! Runs the external credential provider's commands, as the C# `CommandCredentialProvider`:
//! no shell, no window, the unlock secret on standard input, stopped when it takes too long.
//! Standard error is never read: a password manager may print anything there.
//!
//! One command runs at a time, however many connections ask (the C# bulk connect asks the
//! vault one server after the other, `ServerListViewModel.Bulk.cs:1421`): a vault unlock is
//! tuned to fill the machine alone, and parallel ones run past the timeout. A command stopped
//! takes its whole process tree with it, as the C# `Kill(entireProcessTree: true)`: the vault
//! tool behind `cmd /c` or `wsl` must not keep its database locked.

use std::process::Stdio;
use std::time::Duration;

use heimdall_core::credential_provider::{
    Lookup, ProviderSettings, TemplateProblem, ValueShape, command_line, password_in,
    program_file_name, value_shape,
};
use heimdall_ssh::Secret;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::sync::Mutex;
use zeroize::Zeroizing;

/// Windows: the command opens no console window.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Windows: the environment variable holding the Windows directory.
#[cfg(windows)]
const SYSTEM_ROOT_VARIABLE: &str = "SystemRoot";
/// Windows: the Windows directory when that variable is not set.
#[cfg(windows)]
const DEFAULT_SYSTEM_ROOT: &str = r"C:\Windows";
/// Windows: the system directory inside the Windows directory.
#[cfg(windows)]
const SYSTEM_DIRECTORY: &str = "System32";
/// Windows: the program that ends a process and the processes it started, in the system
/// directory: never found through `PATH` or the application's folder.
#[cfg(windows)]
const TASKKILL_PROGRAM: &str = "taskkill.exe";
/// `taskkill`'s arguments before the process number: the tree below it, by force.
#[cfg(windows)]
const TASKKILL_TREE_ARGS: [&str; 3] = ["/T", "/F", "/PID"];

/// Elsewhere: where the program that signals a process group lives, the first that exists
/// (the last is NixOS's).
#[cfg(unix)]
const KILL_PROGRAMS: [&str; 3] = [
    "/bin/kill",
    "/usr/bin/kill",
    "/run/current-system/sw/bin/kill",
];
/// `kill`'s arguments before the group: the signal that cannot be caught, then the end of the
/// options, since the group is given as a negative number.
#[cfg(unix)]
const KILL_GROUP_ARGS: [&str; 3] = ["-s", "KILL", "--"];

/// The most output of a command that is taken: a password is far shorter. More is an error,
/// never a value cut short.
const MAX_OUTPUT_BYTES: usize = 64 * 1024;
/// The room the output gets at first: the limit, the byte that tells it is past, and the probe
/// `read_to_end` makes after it. The vector never grows, so no earlier copy is left behind.
const OUTPUT_CAPACITY: usize = MAX_OUTPUT_BYTES + 1 + 1024;
/// How long a command dropped is waited for after its kill, and how often it is looked at.
const REAP_POLL: Duration = Duration::from_millis(10);
const REAP_TRIES: u32 = 300;

/// One question to the vault at a time, in the order they were asked.
static ONE_AT_A_TIME: Mutex<()> = Mutex::const_new(());

/// Why a command gave no password.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderFailure {
    /// The template cannot run.
    Template(TemplateProblem),
    /// The program could not be started.
    Launch(String),
    /// It ended with this exit code, or was ended by a signal.
    Exit(Option<i32>),
    /// It printed nothing.
    Empty,
    /// It printed more than a password can be: not taken, since a value cut short would be a
    /// wrong password.
    OutputTooLong,
    /// It took longer than allowed and was stopped. Only the program's file name is kept, never
    /// its arguments: a template may hold a token there.
    TimedOut {
        /// The program's file name.
        program: String,
        /// How long it was given.
        timeout: Duration,
    },
}

/// What the provider gave.
#[derive(Debug, Clone)]
pub struct Provided {
    /// The password.
    pub password: Secret,
    /// The user name from the user name command, when the profile has none.
    pub username: Option<String>,
}

/// `timeout` in milliseconds, as the messages say it.
#[must_use]
pub fn milliseconds(timeout: Duration) -> u64 {
    u64::try_from(timeout.as_millis()).unwrap_or(u64::MAX)
}

/// Ends the process `pid` and every process it started.
///
/// Windows: `taskkill /T /F /PID`, which finds the children by their parent's number alone and
/// does not check when they were created. A job object would be exact, but needs FFI or a new
/// direct dependency. The remaining limit: a process started by a command that has already
/// ended, whose number was then given to another process, could be taken for a child. The
/// caller keeps the window small by asking only for a command it still holds and that has not
/// ended (`Child::try_wait`).
///
/// Unix: the command leads a process group of its own (see `run`), and its number is the
/// group's; the group is signalled with `kill`.
fn kill_tree(pid: u32) {
    #[cfg(windows)]
    let mut command = {
        use std::os::windows::process::CommandExt as _;
        let root =
            std::env::var_os(SYSTEM_ROOT_VARIABLE).unwrap_or_else(|| DEFAULT_SYSTEM_ROOT.into());
        let directory = std::path::Path::new(&root).join(SYSTEM_DIRECTORY);
        let mut command = std::process::Command::new(directory.join(TASKKILL_PROGRAM));
        command
            .args(TASKKILL_TREE_ARGS)
            .arg(pid.to_string())
            .current_dir(&directory)
            .creation_flags(CREATE_NO_WINDOW);
        command
    };
    #[cfg(unix)]
    let mut command = {
        let Some(program) = KILL_PROGRAMS
            .iter()
            .find(|path| std::path::Path::new(path).exists())
        else {
            // No `kill` to run: the direct child is still killed by the caller.
            static ONCE: std::sync::Once = std::sync::Once::new();
            ONCE.call_once(|| {
                log::debug!("no kill program found: only the command itself is stopped");
            });
            return;
        };
        let mut command = std::process::Command::new(program);
        command.args(KILL_GROUP_ARGS).arg(format!("-{pid}"));
        command
    };
    // Best effort: the tree may already be gone.
    let _ = command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

/// A command started, with the process handle held until it is over: the tree is ended while
/// the handle is open, so that no other process can have taken its number (Windows reuses them
/// at once). Dropped while still held (the question was given up, the tab closed), it ends the
/// tree and the command on a thread of its own, and only then releases the handle.
struct Running {
    child: Option<tokio::process::Child>,
    pid: Option<u32>,
}

impl Running {
    fn child(&mut self) -> &mut tokio::process::Child {
        self.child.as_mut().expect("held until it is over")
    }

    /// The command is over and reaped: nothing is left to end.
    fn finish(&mut self) {
        self.child = None;
        self.pid = None;
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        if let Some(child) = self.child.take() {
            let pid = self.pid;
            // A thread of its own: a drop may run on the executor, which must not wait for a
            // process to start and end.
            std::thread::spawn(move || end_tree(child, pid));
        }
    }
}

/// Ends `child` and its tree, with the handle held throughout, and waits for it to be reaped.
fn end_tree(mut child: tokio::process::Child, pid: Option<u32>) {
    // Windows: only a command still running is asked for by its number; elsewhere the group
    // outlives its leader and is signalled whatever became of it.
    let live = cfg!(not(windows)) || matches!(child.try_wait(), Ok(None));
    if live && let Some(pid) = pid {
        kill_tree(pid);
    }
    let _ = child.start_kill();
    for _ in 0..REAP_TRIES {
        if !matches!(child.try_wait(), Ok(None)) {
            break;
        }
        std::thread::sleep(REAP_POLL);
    }
}

/// Runs `argv` and returns its standard output; `unlock` is written to its standard input,
/// then a line break, then the input is closed.
async fn run(
    argv: &[String],
    unlock: Option<&Secret>,
    timeout: Duration,
) -> Result<Zeroizing<Vec<u8>>, ProviderFailure> {
    let (program, arguments) = argv
        .split_first()
        .ok_or(ProviderFailure::Template(TemplateProblem::Empty))?;
    let mut command = tokio::process::Command::new(program);
    command
        .args(arguments)
        .stdin(if unlock.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        // Dropped when the time is up: stopped, not left running.
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);
    // Its own process group, so that the whole tree can be signalled at once.
    #[cfg(unix)]
    command.process_group(0);
    let child = command
        .spawn()
        .map_err(|error| ProviderFailure::Launch(error.to_string()))?;
    let mut running = Running {
        pid: child.id(),
        child: Some(child),
    };
    let stdin = running.child().stdin.take();
    let stdout = running.child().stdout.take();
    let waiting = running.child();
    let work = async move {
        if let (Some(mut stdin), Some(secret)) = (stdin, unlock) {
            let mut line = Zeroizing::new(secret.expose().as_bytes().to_vec());
            line.push(b'\n');
            // A program that does not read its input closes it: not a failure.
            let _ = stdin.write_all(&line).await;
        }
        // Sized once and never grown: a vector that grows leaves its earlier copies behind,
        // freed but not zeroed, with the password in them.
        let mut output = Zeroizing::new(Vec::with_capacity(OUTPUT_CAPACITY));
        let mut too_long = false;
        if let Some(mut stdout) = stdout {
            (&mut stdout)
                .take(MAX_OUTPUT_BYTES as u64 + 1)
                .read_to_end(&mut output)
                .await
                .map_err(|error| ProviderFailure::Launch(error.to_string()))?;
            too_long = output.len() > MAX_OUTPUT_BYTES;
            if too_long {
                // Not kept; read to the end all the same, so the command is not left blocked
                // on a full pipe.
                output.clear();
                let _ = tokio::io::copy(&mut stdout, &mut tokio::io::sink()).await;
            }
        }
        let status = waiting
            .wait()
            .await
            .map_err(|error| ProviderFailure::Launch(error.to_string()))?;
        if too_long {
            Err(ProviderFailure::OutputTooLong)
        } else if status.success() {
            Ok(output)
        } else {
            Err(ProviderFailure::Exit(status.code()))
        }
    };
    let outcome = tokio::time::timeout(timeout, work).await;
    if let Ok(result) = outcome {
        if !matches!(result, Err(ProviderFailure::Launch(_))) {
            // Ended by itself: nothing is left to stop.
            running.finish();
        }
        return result;
    }
    // Windows: only a command still running is asked for by its number, which another process
    // could take once it ended and was reaped (it is held, so it is not reaped yet).
    let live = cfg!(not(windows)) || matches!(running.child().try_wait(), Ok(None));
    if live && let Some(pid) = running.pid {
        // Off the executor's thread: a process to start and wait for.
        let _ = tokio::task::spawn_blocking(move || kill_tree(pid)).await;
    }
    // The direct child too, whether or not the tree kill could run, and reaped.
    let _ = running.child().kill().await;
    running.finish();
    // The delay and the program's file name, never its arguments: a template may embed a token.
    Err(ProviderFailure::TimedOut {
        program: program_file_name(program).to_owned(),
        timeout,
    })
}

/// The text a command printed, as its password rule reads it.
async fn ask_one(
    template: &str,
    lookup: &Lookup,
    settings: &ProviderSettings,
    unlock: Option<&Secret>,
) -> Result<Zeroizing<String>, ProviderFailure> {
    let argv = command_line(template, lookup, settings).map_err(ProviderFailure::Template)?;
    let output = run(&argv, unlock, settings.timeout).await?;
    let text = Zeroizing::new(String::from_utf8_lossy(&output).into_owned());
    password_in(&text, settings.first_line_only)
        .map(Zeroizing::new)
        .ok_or(ProviderFailure::Empty)
}

/// Asks the provider for `lookup`'s password and, when the profile has no user name and a
/// user name command is set, its user name: a user name that cannot be had is left out, as
/// the C# one falls back.
///
/// Only one question is in the vault at a time: a second one waits its turn, and its timeout
/// starts when its command does.
///
/// # Errors
///
/// [`ProviderFailure`] when no password was given.
pub async fn ask(
    settings: &ProviderSettings,
    lookup: &Lookup,
    unlock: Option<&Secret>,
) -> Result<Provided, ProviderFailure> {
    // The password command and the user name command, then the next question.
    let _turn = ONE_AT_A_TIME.lock().await;
    let password = ask_one(&settings.command, lookup, settings, unlock).await?;
    let wants_user = lookup.user.as_deref().is_none_or(str::is_empty)
        && !settings.username_command.trim().is_empty();
    let username = if wants_user {
        ask_one(&settings.username_command, lookup, settings, unlock)
            .await
            .ok()
            // A user name has no edge blanks worth keeping, unlike a password (C# `Trim`).
            .map(|name| name.trim().to_owned())
    } else {
        None
    };
    Ok(Provided {
        password: Secret::new(String::clone(&password)),
        username,
    })
}

/// The generic credential of Windows Credential Manager named `target`, as the C#
/// `WindowsCredentialManagerProvider` reads it: its password, and its user name for a
/// profile without one. Read away from the application's thread.
///
/// # Errors
///
/// [`ProviderFailure::Empty`] when there is no such credential, or its password is empty;
/// [`ProviderFailure::Launch`] when the store cannot be read, or away from Windows.
pub async fn from_credential_manager(target: String) -> Result<Provided, ProviderFailure> {
    let target = heimdall_core::paths::credential_target(&target);
    let read = tokio::task::spawn_blocking(move || heimdall_keyring::read_generic(&target))
        .await
        .map_err(|error| ProviderFailure::Launch(error.to_string()))?;
    match read {
        Ok(Some(credential)) => Ok(Provided {
            password: Secret::new(String::clone(&credential.password)),
            username: credential.username,
        }),
        Ok(None) => Err(ProviderFailure::Empty),
        Err(error) => Err(ProviderFailure::Launch(error.to_string())),
    }
}

/// What the Settings page's Test button found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderTest {
    /// Running.
    Running,
    /// No vault entry was typed to look up.
    NoEntry,
    /// A password was given; it is never kept, only what can be said of it.
    Success {
        /// The entry looked up.
        entry: String,
        /// The user name the user name command gave, when it did.
        username: Option<String>,
        /// The value's shape: how many lines, and a blank at an edge.
        shape: ValueShape,
    },
    /// The command printed nothing, or ended in failure.
    NoResult {
        /// The entry looked up.
        entry: String,
    },
    /// The command took longer than allowed.
    TimedOut {
        /// The program's file name.
        program: String,
        /// How long it was given.
        timeout: Duration,
    },
    /// The command cannot run: the reason.
    Failed(ProviderFailure),
}

/// Runs the password command for the vault entry `entry`, as the C# Test button does: the
/// entry stands in for the host and the title, and no user name is given.
pub async fn test(
    settings: ProviderSettings,
    entry: String,
    unlock: Option<Secret>,
) -> ProviderTest {
    let entry = entry.trim().to_owned();
    if entry.is_empty() {
        // The C# order: what is wrong with the command is said before the missing entry.
        return match command_line(&settings.command, &Lookup::for_test(""), &settings) {
            Err(problem) => ProviderTest::Failed(ProviderFailure::Template(problem)),
            Ok(_) => ProviderTest::NoEntry,
        };
    }
    match ask(&settings, &Lookup::for_test(&entry), unlock.as_ref()).await {
        Ok(provided) => ProviderTest::Success {
            shape: value_shape(provided.password.expose()),
            username: provided.username,
            entry,
        },
        Err(ProviderFailure::Empty | ProviderFailure::Exit(_)) => ProviderTest::NoResult { entry },
        Err(ProviderFailure::TimedOut { program, timeout }) => {
            ProviderTest::TimedOut { program, timeout }
        }
        Err(failure) => ProviderTest::Failed(failure),
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn settings(command: &str) -> ProviderSettings {
        ProviderSettings {
            enabled: true,
            command: command.to_owned(),
            ..ProviderSettings::default()
        }
    }

    fn lookup(user: Option<&str>) -> Lookup {
        Lookup {
            host: "web.lab".to_owned(),
            port: 22,
            user: user.map(str::to_owned),
            title: "Web server".to_owned(),
        }
    }

    #[tokio::test]
    async fn the_output_is_the_password_with_its_spaces() {
        let provided = ask(
            &settings(r#"printf "\n  %s-pw \n\n" "{Title}""#),
            &lookup(Some("admin")),
            None,
        )
        .await
        .expect("given");
        // Only the line breaks around it go, as the C# `Trim('\r', '\n')`.
        assert_eq!(provided.password.expose(), "  Web server-pw ");
        assert_eq!(provided.username, None);
    }

    #[tokio::test]
    async fn the_first_line_mode_keeps_spaces_and_refuses_the_status_line() {
        let first = |command: &str| ProviderSettings {
            first_line_only: true,
            ..settings(command)
        };
        // KPScript: the value, then its status line.
        let provided = ask(
            &first(r#"printf " pw \nOK: Operation completed successfully.\n""#),
            &lookup(None),
            None,
        )
        .await
        .expect("given");
        assert_eq!(provided.password.expose(), " pw ");
        // pass: the password, then the notes.
        let provided = ask(&first(r#"printf "pw\nnotes: x\n""#), &lookup(None), None)
            .await
            .expect("given");
        assert_eq!(provided.password.expose(), "pw", "never the notes");
        // An empty field: the status line is no password.
        assert_eq!(
            ask(&first(r#"printf "\nOK: done\n""#), &lookup(None), None)
                .await
                .err(),
            Some(ProviderFailure::Empty)
        );
    }

    #[tokio::test]
    async fn the_unlock_secret_is_the_first_line_of_its_input() {
        let provided = ask(
            &settings(r#"sh -c "read secret; printf %s-open $secret""#),
            &lookup(Some("admin")),
            Some(&Secret::new("hunter2".to_owned())),
        )
        .await
        .expect("given");
        assert_eq!(provided.password.expose(), "hunter2-open");
    }

    #[tokio::test]
    async fn without_a_secret_the_input_is_empty() {
        let provided = ask(
            &settings(r#"sh -c "cat; printf done""#),
            &lookup(Some("admin")),
            None,
        )
        .await
        .expect("given");
        assert_eq!(provided.password.expose(), "done");
    }

    #[tokio::test]
    async fn the_user_name_is_asked_only_for_a_profile_without_one() {
        let with_user = ProviderSettings {
            username_command: "printf deploy".to_owned(),
            ..settings("printf pw")
        };
        let provided = ask(&with_user, &lookup(None), None).await.expect("given");
        assert_eq!(provided.username.as_deref(), Some("deploy"));
        let provided = ask(&with_user, &lookup(Some("admin")), None)
            .await
            .expect("given");
        assert_eq!(provided.username, None, "the profile's name stays");
        let failing = ProviderSettings {
            username_command: "false".to_owned(),
            ..settings("printf pw")
        };
        let provided = ask(&failing, &lookup(None), None).await.expect("given");
        assert_eq!(provided.username, None, "a failure leaves it out");
        let padded = ProviderSettings {
            username_command: r#"printf " deploy \n""#.to_owned(),
            ..settings("printf pw")
        };
        let provided = ask(&padded, &lookup(None), None).await.expect("given");
        assert_eq!(provided.username.as_deref(), Some("deploy"), "trimmed");
    }

    #[tokio::test]
    async fn a_failure_gives_no_password() {
        assert_eq!(
            ask(&settings("false"), &lookup(None), None).await.err(),
            Some(ProviderFailure::Exit(Some(1)))
        );
        assert_eq!(
            // Only blanks and a line break.
            ask(&settings(r#"printf "  \n ""#), &lookup(None), None)
                .await
                .err(),
            Some(ProviderFailure::Empty)
        );
        assert!(matches!(
            ask(&settings("no-such-program-heimdall"), &lookup(None), None)
                .await
                .err(),
            Some(ProviderFailure::Launch(_))
        ));
        assert_eq!(
            ask(&settings(r#"tool "{Title}"#), &lookup(None), None)
                .await
                .err(),
            Some(ProviderFailure::Template(TemplateProblem::UnclosedQuote))
        );
    }

    #[tokio::test]
    async fn a_title_that_would_be_a_flag_is_refused_before_anything_runs() {
        let dashed = Lookup {
            title: "--exec=evil".to_owned(),
            ..lookup(None)
        };
        assert_eq!(
            ask(&settings("bw get {Title}"), &dashed, None).await.err(),
            Some(ProviderFailure::Template(TemplateProblem::OptionLikeValue(
                "{Title}"
            )))
        );
    }

    #[tokio::test]
    async fn an_output_past_the_cap_is_an_error_never_a_cut_value() {
        // 70000 bytes of "a": more than a password can be.
        let long = settings(r#"sh -c "yes aaaaaaaa | head -c 70000""#);
        assert_eq!(
            ask(&long, &lookup(None), None).await.err(),
            Some(ProviderFailure::OutputTooLong)
        );
        // Just at the cap is a value.
        let at_cap = settings(r#"sh -c "head -c 65536 /dev/zero | tr '\0' a""#);
        let provided = ask(&at_cap, &lookup(None), None).await.expect("given");
        assert_eq!(provided.password.expose().len(), 65536);
    }

    #[tokio::test]
    async fn a_command_too_slow_is_stopped_and_named_without_its_arguments() {
        let slow = ProviderSettings {
            timeout: Duration::from_millis(200),
            ..settings(r#"sh -c "sleep 5 # t0ken-in-argv""#)
        };
        let started = std::time::Instant::now();
        let failure = ask(&slow, &lookup(None), None).await.err();
        assert_eq!(
            failure,
            Some(ProviderFailure::TimedOut {
                program: "sh".to_owned(),
                timeout: Duration::from_millis(200),
            })
        );
        assert!(!format!("{failure:?}").contains("t0ken"), "no argument");
        assert!(started.elapsed() < Duration::from_secs(3));
    }

    /// Whether process `pid` is gone, or only a zombie nobody has reaped yet.
    fn gone(pid: &str) -> bool {
        let output = std::process::Command::new("ps")
            .args(["-o", "stat=", "-p", pid])
            .output()
            .expect("ps");
        let state = String::from_utf8_lossy(&output.stdout);
        let state = state.trim();
        state.is_empty() || state.starts_with('Z')
    }

    #[tokio::test]
    async fn the_whole_process_tree_is_killed_on_timeout() {
        let dir = tempfile::tempdir().expect("dir");
        let pid_file = dir.path().join("grandchild.pid");
        // The shell starts a grandchild, writes its number, and waits for it: the vault behind
        // `cmd /c`, holding its database lock.
        let tree = ProviderSettings {
            timeout: Duration::from_millis(600),
            database: pid_file.display().to_string(),
            ..settings(r#"sh -c "sleep 30 & echo $! > {Database}; wait""#)
        };
        assert!(matches!(
            ask(&tree, &lookup(None), None).await.err(),
            Some(ProviderFailure::TimedOut { .. })
        ));
        let pid = std::fs::read_to_string(&pid_file).expect("the grandchild's number");
        let pid = pid.trim();
        let mut tries = 0;
        while !gone(pid) {
            tries += 1;
            assert!(tries < 60, "the grandchild {pid} is still running");
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn only_one_provider_process_is_in_flight() {
        let dir = tempfile::tempdir().expect("dir");
        let lock = dir.path().join("in-flight");
        // `mkdir` fails when another command is inside: exit 9, so no password.
        let provider = ProviderSettings {
            database: lock.display().to_string(),
            ..settings(
                r#"sh -c "mkdir {Database} || exit 9; sleep 0.2; rmdir {Database}; printf pw""#,
            )
        };
        let tasks: Vec<_> = (0..5)
            .map(|_| {
                let provider = provider.clone();
                tokio::spawn(async move { ask(&provider, &lookup(None), None).await.is_ok() })
            })
            .collect();
        for task in tasks {
            assert!(task.await.expect("joined"), "two commands ran side by side");
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_question_given_up_leaves_the_queue_and_stops_its_command() {
        let dir = tempfile::tempdir().expect("dir");
        let pid_file = dir.path().join("running.pid");
        let running = ProviderSettings {
            database: pid_file.display().to_string(),
            ..settings(r#"sh -c "sleep 30 & echo $! > {Database}; wait""#)
        };
        let first = tokio::spawn({
            let running = running.clone();
            async move { ask(&running, &lookup(None), None).await }
        });
        // Wait until the first command is in the vault.
        for _ in 0..100 {
            if pid_file.exists() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert!(pid_file.exists(), "the first question never started");
        // A second question queues behind it, and a third.
        let queued = tokio::spawn({
            let running = running.clone();
            async move { ask(&running, &lookup(None), None).await }
        });
        let waiting = tokio::spawn(async move {
            ask(&settings("printf pw"), &lookup(None), None)
                .await
                .is_ok()
        });
        tokio::time::sleep(Duration::from_millis(200)).await;
        // The tab of the second one closes: its task is aborted, and leaves the queue.
        queued.abort();
        assert!(queued.await.expect_err("aborted").is_cancelled());
        // The first one is abandoned too: its whole tree goes, and the third one gets its turn.
        first.abort();
        assert!(first.await.expect_err("aborted").is_cancelled());
        let answered = tokio::time::timeout(Duration::from_secs(5), waiting)
            .await
            .expect("the queue moved on")
            .expect("joined");
        assert!(answered, "the third question was answered");
        let pid = std::fs::read_to_string(&pid_file).expect("the grandchild's number");
        let pid = pid.trim();
        let mut tries = 0;
        while !gone(pid) {
            tries += 1;
            assert!(tries < 60, "the abandoned command's child {pid} still runs");
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    #[tokio::test]
    async fn the_wait_for_a_turn_is_not_part_of_the_timeout() {
        let provider = ProviderSettings {
            timeout: Duration::from_secs(1),
            ..settings(r#"sh -c "sleep 0.6; printf pw""#)
        };
        let who = lookup(None);
        let (first, second) = tokio::join!(ask(&provider, &who, None), ask(&provider, &who, None));
        assert!(first.is_ok() && second.is_ok(), "{first:?} {second:?}");
    }

    #[tokio::test]
    async fn the_test_button_sorts_the_outcomes() {
        let entry = || "srv-01".to_owned();
        assert_eq!(
            test(settings(r#"printf "pw""#), entry(), None).await,
            ProviderTest::Success {
                entry: entry(),
                username: None,
                shape: ValueShape {
                    lines: 1,
                    edge_space: false
                },
            }
        );
        assert_eq!(
            test(settings("false"), entry(), None).await,
            ProviderTest::NoResult { entry: entry() }
        );
        assert_eq!(
            test(settings("true"), entry(), None).await,
            ProviderTest::NoResult { entry: entry() }
        );
        let slow = ProviderSettings {
            timeout: Duration::from_millis(100),
            ..settings("sleep 5")
        };
        assert_eq!(
            test(slow, entry(), None).await,
            ProviderTest::TimedOut {
                program: "sleep".to_owned(),
                timeout: Duration::from_millis(100),
            }
        );
        assert_eq!(
            test(settings(" "), entry(), None).await,
            ProviderTest::Failed(ProviderFailure::Template(TemplateProblem::Empty))
        );
    }

    #[tokio::test]
    async fn the_test_button_asks_for_the_entry_and_looks_it_up() {
        // The command is checked first, then the entry.
        assert_eq!(
            test(settings(" "), String::new(), None).await,
            ProviderTest::Failed(ProviderFailure::Template(TemplateProblem::Empty))
        );
        assert_eq!(
            test(settings("printf pw"), "  ".to_owned(), None).await,
            ProviderTest::NoEntry
        );
        // The entry stands in for the title and the host, trimmed.
        let echo = settings(r#"printf "%s@%s" "{Title}" "{Host}""#);
        let ProviderTest::Success {
            entry, username, ..
        } = test(echo, " db-1 ".to_owned(), None).await
        else {
            panic!("a password");
        };
        assert_eq!(entry, "db-1");
        assert_eq!(username, None);
    }

    #[tokio::test]
    async fn the_test_button_shows_the_user_name_and_the_shape_never_the_password() {
        let provider = ProviderSettings {
            username_command: "printf deploy".to_owned(),
            ..settings(r#"printf " line one\nline two ""#)
        };
        let outcome = test(provider, "srv".to_owned(), None).await;
        assert_eq!(
            outcome,
            ProviderTest::Success {
                entry: "srv".to_owned(),
                username: Some("deploy".to_owned()),
                shape: ValueShape {
                    lines: 2,
                    edge_space: true
                },
            }
        );
        let said = format!("{outcome:?}");
        assert!(
            !said.contains("line one") && !said.contains("line two"),
            "{said}"
        );
        let spaced = test(settings(r#"printf "pw ""#), "srv".to_owned(), None).await;
        let ProviderTest::Success { shape, .. } = spaced else {
            panic!("a password");
        };
        assert!(shape.edge_space && shape.lines == 1);
    }
}
