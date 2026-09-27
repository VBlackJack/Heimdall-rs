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

/// What to run.
#[derive(Debug, Clone, Default)]
pub struct LocalConfig {
    /// The program, a full path or a name looked up in `PATH`; `None` for the user's default
    /// shell. See [`program`] for what is refused.
    pub program: Option<String>,
    /// Its arguments.
    pub args: Vec<String>,
    /// Folder it starts in; `None` for the current one.
    pub working_directory: Option<PathBuf>,
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

fn options(config: &LocalConfig) -> io::Result<Options> {
    program::check_arguments(&config.args, config.working_directory.as_deref())?;
    let search = std::env::var_os("PATH");
    let resolve = |name: &str| program::resolve(name, search.as_deref(), Path::is_file);
    let shell = match config.program.as_deref() {
        Some(name) => Some(shell(&resolve(name)?, config.args.clone())),
        None => default_shell(&config.args)?,
    };
    Ok(Options {
        shell,
        working_directory: config.working_directory.clone(),
        drain_on_exit: false,
        // Passed to the child only: `tty::setup_env` would change this process's own.
        env: HashMap::from([
            ("TERM".to_owned(), TERMINAL_TYPE.to_owned()),
            ("COLORTERM".to_owned(), "truecolor".to_owned()),
        ]),
        // Arguments with spaces stay whole on the command line Windows builds from them.
        #[cfg(windows)]
        escape_args: true,
    })
}

/// The shell run for `program`, a full path. On Windows the path goes into the command line
/// quoted: unquoted, `C:\My Tools\x.exe` would first be tried as `C:\My.exe`.
fn shell(program: &Path, args: Vec<String>) -> Shell {
    Shell::new(command_program(program), args)
}

/// `program` as it starts the command line.
fn command_program(program: &Path) -> String {
    let program = program.to_string_lossy();
    #[cfg(windows)]
    return format!("\"{program}\"");
    #[cfg(unix)]
    return program.into_owned();
}

/// The shell run when none is named: Windows `PowerShell` by its full path.
#[cfg(windows)]
fn default_shell(args: &[String]) -> io::Result<Option<Shell>> {
    let root = std::env::var_os("SystemRoot")
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "SystemRoot is not set"))?;
    Ok(Some(shell(
        &program::default_shell(Path::new(&root)),
        args.to_vec(),
    )))
}

/// The shell run when none is named: the account's login shell, a full path from the user
/// database, which `alacritty_terminal` looks up itself.
#[cfg(unix)]
#[expect(
    clippy::unnecessary_wraps,
    reason = "the same signature as on Windows, where finding the default can fail"
)]
fn default_shell(_args: &[String]) -> io::Result<Option<Shell>> {
    Ok(None)
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
            args: vec!["-l".to_owned()],
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

    #[cfg(windows)]
    #[test]
    fn a_windows_program_goes_into_the_command_line_quoted() {
        assert_eq!(
            command_program(Path::new(r"C:\My Tools\x.exe")),
            r#""C:\My Tools\x.exe""#
        );
    }
}
