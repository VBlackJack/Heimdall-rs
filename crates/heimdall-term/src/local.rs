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

//! Local shells on a pseudo-terminal: `alacritty_terminal`'s PTY (Unix) and `ConPTY`
//! (Windows), driven without `unsafe`.
//!
//! A session reports the shell's output and, last, its exit; its input side only queues.
//! On Unix one tokio task reads the master through `AsyncFd` and learns of the exit from
//! `SIGCHLD`; on Windows one thread waits on a poller that the `ConPTY` pipes and the child
//! watcher post to.

mod command_line;
pub use command_line::windows_arguments;
pub mod module_path;
pub mod program;
#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};

use alacritty_terminal::event::WindowSize;
use alacritty_terminal::tty::{Options, Shell};
use tokio::sync::mpsc;

/// Events queued before the session waits for the receiver: output is not read from the
/// shell faster than it is shown.
const EVENT_QUEUE: usize = 64;

/// Bytes read from the shell at a time.
const READ_BUFFER: usize = 64 * 1024;

/// The terminal the shell is told it runs in: what the emulator understands.
const TERMINAL_TYPE: &str = "xterm-256color";

/// Largest side `ConPTY` accepts, in characters.
const MAX_SIDE: u16 = 0x7fff;

/// The arguments of a local program.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LocalArguments {
    /// One per entry: on Windows each is quoted as the C runtime reads it back.
    List(Vec<String>),
    /// A Windows argument string, put after the program exactly as written. Refused on Unix,
    /// where there is no command line to put it in.
    WindowsLine(String),
}

impl Default for LocalArguments {
    fn default() -> Self {
        Self::List(Vec::new())
    }
}

/// What to run.
#[derive(Debug, Clone, Default)]
pub struct LocalConfig {
    /// The program, a full path or a name looked up in `PATH`; `None` for the user's default
    /// shell. See [`program`] for what is refused.
    pub program: Option<String>,
    /// Its arguments.
    pub arguments: LocalArguments,
    /// Folder it starts in; `None` for the current one.
    pub working_directory: Option<PathBuf>,
    /// Variables set for it over what it inherits, as name and value. One the system cannot
    /// carry, a name empty or holding `=` or NUL, a value holding NUL, is left out; the
    /// terminal's own variables are never replaced.
    pub environment: Vec<(String, String)>,
    /// Columns.
    pub columns: u16,
    /// Rows.
    pub rows: u16,
}

/// What a session reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LocalEvent {
    /// Output of the shell.
    Output(Vec<u8>),
    /// The shell ended, with its exit code when it has one. Last event.
    Exited(Option<i32>),
}

/// The session ended; nothing more can be sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the local session has ended")]
pub struct SessionEnded;

/// What the input side asks of the session.
#[derive(Debug)]
enum Command {
    Write(Vec<u8>),
    Resize { columns: u16, rows: u16 },
    Close,
}

/// A running local shell.
#[derive(Debug)]
pub struct LocalSession {
    /// Where input goes.
    pub input: LocalInput,
    /// What the session reports, ending with [`LocalEvent::Exited`].
    pub events: mpsc::Receiver<LocalEvent>,
}

/// The input side of a session. Calls only queue: none waits on the shell. The queue has no
/// bound: a bounded one would have to drop keystrokes or make the caller wait.
#[derive(Clone)]
pub struct LocalInput {
    #[cfg(unix)]
    commands: mpsc::UnboundedSender<Command>,
    #[cfg(windows)]
    commands: windows::Commands,
}

impl std::fmt::Debug for LocalInput {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("LocalInput")
    }
}

impl LocalInput {
    /// Sends what was typed.
    ///
    /// # Errors
    ///
    /// [`SessionEnded`].
    pub fn write(&self, bytes: Vec<u8>) -> Result<(), SessionEnded> {
        self.send(Command::Write(bytes))
    }

    /// Reports a new terminal size.
    ///
    /// # Errors
    ///
    /// [`SessionEnded`].
    pub fn resize(&self, columns: u16, rows: u16) -> Result<(), SessionEnded> {
        self.send(Command::Resize { columns, rows })
    }

    /// Ends the session: the shell is hung up.
    pub fn close(&self) {
        let _ = self.send(Command::Close);
    }

    fn send(&self, command: Command) -> Result<(), SessionEnded> {
        self.commands.send(command).map_err(|_| SessionEnded)
    }
}

/// Starts `config` on a pseudo-terminal. On Unix it must be called within a tokio runtime.
///
/// # Errors
///
/// The pseudo-terminal or the program could not be started.
pub fn spawn(config: &LocalConfig) -> io::Result<LocalSession> {
    let options = options(config)?;
    #[cfg(unix)]
    return unix::spawn(&options, window_size(config.columns, config.rows));
    #[cfg(windows)]
    return windows::spawn(&options, window_size(config.columns, config.rows));
}

/// The file `program` names, as [`spawn`] finds it: the default shell for `None`, a full
/// path as given, a bare name in the absolute folders of `PATH`.
///
/// # Errors
///
/// The name is refused or found nowhere; see [`program`].
pub fn program_path(program: Option<&str>) -> io::Result<PathBuf> {
    match program {
        Some(name) => {
            let search = std::env::var_os("PATH");
            Ok(program::resolve(name, search.as_deref(), Path::is_file)?)
        }
        None => default_program(),
    }
}

/// The command as it runs, for someone to read before it does. On Windows it is the very
/// command line the program receives, built by the function [`spawn`] uses; on Unix, where a
/// program receives its arguments one by one, each is shown quoted when it needs to be.
#[must_use]
pub fn command_text(program_path: &Path, arguments: &LocalArguments) -> String {
    #[cfg(windows)]
    return command_line::windows(program_path, arguments);
    #[cfg(unix)]
    return command_line::unix_display(program_path, arguments);
}

/// Whether the program reads its command line again with rules of its own, so that what the
/// arguments look like is not what it does with them: `cmd.exe`, and batch files, which
/// Windows runs through it. `&`, `|`, `^` and `%` are commands to it.
#[must_use]
pub fn rereads_its_command_line(program_path: &Path) -> bool {
    #[cfg(windows)]
    return command_line::is_cmd(program_path);
    #[cfg(unix)]
    return {
        let _ = program_path;
        false
    };
}

fn options(config: &LocalConfig) -> io::Result<Options> {
    program::check_arguments(&config.arguments, config.working_directory.as_deref())?;
    let program = program_path(config.program.as_deref())?;
    Ok(Options {
        shell: Some(shell(&program, &config.arguments)?),
        working_directory: config.working_directory.clone(),
        drain_on_exit: false,
        // Passed to the child only: `tty::setup_env` would change this process's own.
        env: environment(&program, &config.environment),
        // The command line is built whole here, by `command_line::windows`: nothing is added
        // to it on the way.
        #[cfg(windows)]
        escape_args: false,
    })
}

/// The variables set for the child, over what it inherits: those `asked` that the system can
/// carry, then the terminal it runs in and, for Windows `PowerShell`, a module path without
/// `PowerShell` 7's entries ([`module_path::for_windows_powershell`]), which win.
fn environment(program: &Path, asked: &[(String, String)]) -> HashMap<String, String> {
    asked
        .iter()
        .filter(|(name, value)| carried(name, value))
        .cloned()
        .chain([
            ("TERM".to_owned(), TERMINAL_TYPE.to_owned()),
            ("COLORTERM".to_owned(), "truecolor".to_owned()),
        ])
        .chain(
            windows_powershell_module_path(program)
                .map(|path| (module_path::VARIABLE.to_owned(), path)),
        )
        .collect()
}

/// Whether a variable can be handed to a program: a name, without `=` or NUL, and a value
/// without NUL, which would end it early.
fn carried(name: &str, value: &str) -> bool {
    !name.is_empty() && !name.contains(['=', '\0']) && !value.contains('\0')
}

/// The module path `program` gets when it is Windows `PowerShell`.
#[cfg(windows)]
fn windows_powershell_module_path(program: &Path) -> Option<String> {
    if !module_path::is_windows_powershell(&program.to_string_lossy()) {
        return None;
    }
    module_path::for_windows_powershell(
        std::env::var(module_path::VARIABLE).ok().as_deref(),
        &module_path::ModuleRoots::for_current_user(),
        |file| Path::new(file).is_file(),
    )
}

/// No Windows `PowerShell` runs here.
#[cfg(unix)]
fn windows_powershell_module_path(_program: &Path) -> Option<String> {
    None
}

/// What `alacritty_terminal` runs. On Windows the whole command line goes in as the program:
/// it is put into `CreateProcessW` as it is, so the line shown is the line run.
#[cfg(windows)]
#[expect(
    clippy::unnecessary_wraps,
    reason = "the same signature as on Unix, where a Windows argument string is refused"
)]
fn shell(program: &Path, arguments: &LocalArguments) -> io::Result<Shell> {
    Ok(Shell::new(
        command_line::windows(program, arguments),
        Vec::new(),
    ))
}

/// What `alacritty_terminal` runs: the program and its arguments, one by one.
#[cfg(unix)]
fn shell(program: &Path, arguments: &LocalArguments) -> io::Result<Shell> {
    match arguments {
        LocalArguments::List(args) => Ok(Shell::new(
            program.to_string_lossy().into_owned(),
            args.clone(),
        )),
        LocalArguments::WindowsLine(_) => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "a Windows argument string has no meaning here",
        )),
    }
}

/// The shell run when none is named: Windows `PowerShell` by its full path, under the
/// Windows folder Windows says, never the one the environment names.
#[cfg(windows)]
fn default_program() -> io::Result<PathBuf> {
    let root = heimdall_core::paths::system_root()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "the Windows folder is unknown"))?;
    Ok(program::default_shell(&root))
}

/// The shell run when none is named: `$SHELL` when it is a full path, as a terminal opens it,
/// else `/bin/sh`. Named here rather than left to `alacritty_terminal`, so that what is shown
/// and approved is the file that runs.
#[cfg(unix)]
#[expect(
    clippy::unnecessary_wraps,
    reason = "the same signature as on Windows, where finding the default can fail"
)]
fn default_program() -> io::Result<PathBuf> {
    Ok(std::env::var_os("SHELL")
        .map(PathBuf::from)
        .filter(|shell| shell.is_absolute())
        .unwrap_or_else(|| PathBuf::from(program::FALLBACK_SHELL)))
}

/// A size the pseudo-terminal accepts: at least one cell, at most what `ConPTY` takes, which
/// would otherwise end the process.
fn window_size(columns: u16, rows: u16) -> WindowSize {
    WindowSize {
        num_lines: rows.clamp(1, MAX_SIDE),
        num_cols: columns.clamp(1, MAX_SIDE),
        cell_width: 1,
        cell_height: 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A program by its full path: taken without a lookup.
    #[cfg(unix)]
    const FULL_PATH: &str = "/bin/sh";
    #[cfg(windows)]
    const FULL_PATH: &str = r"C:\Windows\System32\cmd.exe";

    #[test]
    fn sizes_are_kept_within_what_conpty_accepts() {
        let size = window_size(0, u16::MAX);
        assert_eq!((size.num_cols, size.num_lines), (1, MAX_SIDE));
        let size = window_size(120, 40);
        assert_eq!((size.num_cols, size.num_lines), (120, 40));
    }

    #[test]
    fn the_shell_is_told_its_terminal_and_nothing_else_is_changed() {
        let options = options(&LocalConfig {
            program: Some(FULL_PATH.to_owned()),
            arguments: LocalArguments::List(vec!["-l".to_owned()]),
            ..LocalConfig::default()
        })
        .expect("options");
        assert_eq!(
            options.env.get("TERM").map(String::as_str),
            Some("xterm-256color")
        );
        assert_eq!(
            options.env.get("COLORTERM").map(String::as_str),
            Some("truecolor")
        );
        assert!(!options.drain_on_exit);
        assert!(options.shell.is_some());
    }

    #[test]
    fn the_variables_asked_are_set_unless_the_system_cannot_carry_them() {
        let asked = |name: &str, value: &str| (name.to_owned(), value.to_owned());
        let options = options(&LocalConfig {
            program: Some(FULL_PATH.to_owned()),
            environment: vec![
                asked("HEIMDALL_NAME", "Build box"),
                asked("HEIMDALL_GROUP", "Lab/Linux"),
                asked("", "nameless"),
                asked("A=B", "split"),
                asked("CUT", "before\0after"),
                asked("TERM", "dumb"),
            ],
            ..LocalConfig::default()
        })
        .expect("options");
        assert_eq!(
            options.env.get("HEIMDALL_NAME").map(String::as_str),
            Some("Build box")
        );
        assert_eq!(
            options.env.get("HEIMDALL_GROUP").map(String::as_str),
            Some("Lab/Linux")
        );
        assert_eq!(
            options.env.get("TERM").map(String::as_str),
            Some("xterm-256color"),
            "the terminal's own wins"
        );
        for refused in ["", "A=B", "CUT"] {
            assert!(!options.env.contains_key(refused), "{refused:?}");
        }
    }

    #[test]
    fn the_default_program_is_named_by_its_full_path() {
        let default = program_path(None).expect("a default");
        assert!(default.is_absolute(), "{default:?}");
    }

    #[cfg(unix)]
    #[test]
    fn a_windows_argument_string_is_refused_on_unix() {
        let error = options(&LocalConfig {
            program: Some(FULL_PATH.to_owned()),
            arguments: LocalArguments::WindowsLine("-c exit".to_owned()),
            ..LocalConfig::default()
        })
        .expect_err("refused");
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    }

    #[test]
    fn the_command_shown_is_built_by_the_same_function_as_the_one_run() {
        let arguments = LocalArguments::List(vec!["two words".to_owned()]);
        let shown = command_text(Path::new(FULL_PATH), &arguments);
        #[cfg(windows)]
        assert_eq!(
            shown,
            command_line::windows(Path::new(FULL_PATH), &arguments)
        );
        #[cfg(unix)]
        assert_eq!(shown, "/bin/sh 'two words'");
    }
}
