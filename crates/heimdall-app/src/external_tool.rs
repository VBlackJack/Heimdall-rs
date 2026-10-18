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

//! An external tool of the user's list started for a session, as the C#
//! `ExternalToolLaunchService.LaunchConfigured`: a shell or a script host refused first, as
//! its `ValidateExecutable` refuses them; the program found by its full path, or a bare name
//! in the absolute folders of `PATH`, never in the current folder; its arguments built one by
//! one by [`heimdall_core::external_tools::resolve_arguments`], and handed to the program
//! as a list, never through a shell. Run as administrator, it goes through the Windows
//! elevation prompt as the elevated local shell does; otherwise through the system program
//! runner, which leaves it running.
//!
//! The log names the tool and its program, never its arguments: they carry the session's
//! address and account.

use std::path::{Path, PathBuf};
use std::process::Command;

use heimdall_core::external_tools::{
    ArgumentProblem, ExternalTool, Placeholder, ToolTarget, is_shell_target, resolve_arguments,
};
use heimdall_core::tools::system_program::{self, Window};
use heimdall_term::local::{self, LocalArguments};

use crate::ProfileKind;
use crate::elevated_shell::{self, ElevatedLaunch, ElevatedOutcome, RUNAS_VERB};

/// Why an external tool did not start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolFailure {
    /// Its program is a shell or a script host, as the C# `ExternalToolShellTargetBlocked`
    /// refuses: it would read its arguments as commands.
    ShellTarget,
    /// Its program is not there, or in no absolute folder of `PATH`: as written.
    NotFound(String),
    /// Its program is named by a relative path, or holds a quote: as written.
    InvalidProgram(String),
    /// The value of this placeholder would start an argument with `-`.
    OptionLike(Placeholder),
    /// Its working folder is not there: as written.
    WorkingDirectory(String),
    /// Its arguments or its folder hold a NUL character, which would cut the command line.
    InvalidArguments,
    /// Run as administrator is asked where there is no elevation prompt: Windows only.
    ElevationUnsupported,
    /// The system could not start it: its words.
    NotStarted(String),
}

/// How an external tool's start went, when it did not fail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Launched {
    /// It runs.
    Started,
    /// The user declined the elevation prompt: nothing runs, and nothing more is said, as
    /// the C# only logs it.
    Declined,
}

/// What is started for an external tool, decided before anything runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolLaunch {
    /// The program, a full path.
    pub program: PathBuf,
    /// Its arguments, each its own.
    pub arguments: Vec<String>,
    /// The folder it starts in; `None` for Heimdall's own, as the C# inherits it.
    pub working_directory: Option<PathBuf>,
    /// Through the elevation prompt.
    pub elevated: bool,
    /// Without a console window.
    pub hidden: bool,
    /// Where an elevated start begins when no folder is chosen: the home folder.
    pub home: PathBuf,
}

/// The C# name of a protocol, as its `ConnectionTypeCatalog` writes it: what `{Protocol}`
/// stands for.
#[must_use]
pub const fn protocol_name(kind: ProfileKind) -> &'static str {
    match kind {
        ProfileKind::Rdp => "RDP",
        ProfileKind::Ssh => "SSH",
        ProfileKind::Sftp => "SFTP",
        ProfileKind::Vnc => "VNC",
        ProfileKind::Telnet => "Telnet",
        ProfileKind::Ftp => "FTP",
        ProfileKind::Citrix => "Citrix",
        ProfileKind::Local => "Local",
        ProfileKind::WinRm => "WINRM",
        ProfileKind::Web => "WEB",
    }
}

/// What starts `tool` for `target`, or with no session its template unfilled, as the C#
/// Test button; `home` where an elevated start begins without a folder of its own.
///
/// # Errors
///
/// [`ToolFailure`] when its program is a shell, is not found or not named by a full path or
/// a bare name, a value would read as an option, its folder is not there, or it is to run as
/// administrator where that cannot be asked.
pub fn prepare(
    tool: &ExternalTool,
    target: Option<&ToolTarget>,
    home: &Path,
) -> Result<ToolLaunch, ToolFailure> {
    let written = tool.executable_path.trim();
    if written.is_empty() {
        return Err(ToolFailure::InvalidProgram(String::new()));
    }
    if is_shell_target(written) {
        return Err(ToolFailure::ShellTarget);
    }
    let program = local::program_path(Some(written)).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            ToolFailure::NotFound(written.to_owned())
        } else {
            ToolFailure::InvalidProgram(written.to_owned())
        }
    })?;
    if !program.is_file() {
        return Err(ToolFailure::NotFound(written.to_owned()));
    }
    let arguments = resolve_arguments(&tool.arguments, target)
        .map_err(|ArgumentProblem::OptionLike(placeholder)| ToolFailure::OptionLike(placeholder))?;
    let folder = tool.working_directory.trim();
    let working_directory = if folder.is_empty() {
        None
    } else if Path::new(folder).is_dir() {
        Some(PathBuf::from(folder))
    } else {
        return Err(ToolFailure::WorkingDirectory(folder.to_owned()));
    };
    // A NUL in the template or the folder would cut the command line short: refused, as
    // the elevated local shell refuses it.
    local::program::check_arguments(
        &LocalArguments::List(arguments.clone()),
        working_directory.as_deref(),
    )
    .map_err(|_| ToolFailure::InvalidArguments)?;
    if tool.run_as_administrator && !elevated_shell::SUPPORTED {
        return Err(ToolFailure::ElevationUnsupported);
    }
    Ok(ToolLaunch {
        program,
        arguments,
        working_directory,
        elevated: tool.run_as_administrator,
        hidden: tool.run_hidden,
        home: home.to_owned(),
    })
}

/// The command `launch` runs when it does not run as administrator: the program and its
/// arguments one by one, in its folder when it has one.
#[must_use]
pub fn command(launch: &ToolLaunch) -> Command {
    let mut command = Command::new(&launch.program);
    command.args(&launch.arguments);
    if let Some(folder) = &launch.working_directory {
        command.current_dir(folder);
    }
    command
}

/// What Windows is asked to start for `launch` run as administrator: its arguments as one
/// Windows argument string, each quoted as the C runtime reads it back, so that the program
/// gets the very list built.
#[must_use]
pub fn elevated_request(launch: &ToolLaunch) -> ElevatedLaunch {
    ElevatedLaunch {
        program: launch.program.clone(),
        verb: RUNAS_VERB,
        parameters: local::windows_arguments(&LocalArguments::List(launch.arguments.clone())),
        working_directory: launch
            .working_directory
            .clone()
            .unwrap_or_else(|| launch.home.clone()),
        fallback_directory: launch.home.clone(),
        hidden: launch.hidden,
    }
}

/// The command `tool` runs for `target`, for someone to read before it does, as the C#
/// Settings preview shows it: its program as written and its arguments as they are built,
/// on Windows quoted as the command line the program receives. Nothing is read on disk.
///
/// # Errors
///
/// [`ToolFailure::OptionLike`] when a value would read as an option.
pub fn preview(tool: &ExternalTool, target: Option<&ToolTarget>) -> Result<String, ToolFailure> {
    let arguments = resolve_arguments(&tool.arguments, target)
        .map_err(|ArgumentProblem::OptionLike(placeholder)| ToolFailure::OptionLike(placeholder))?;
    Ok(local::command_text(
        Path::new(tool.executable_path.trim()),
        &LocalArguments::List(arguments),
    ))
}

/// Starts `launch`, the tool named `name`. Run as administrator it waits while the prompt
/// is shown: call it off the UI thread.
///
/// # Errors
///
/// [`ToolFailure::NotStarted`] with the system's words, or
/// [`ToolFailure::ElevationUnsupported`].
pub fn launch(name: &str, launch: &ToolLaunch) -> Result<Launched, ToolFailure> {
    log::info!(
        "external tool {name:?} starting: {}",
        launch.program.display()
    );
    if launch.elevated {
        return match elevated_shell::launch(&elevated_request(launch)) {
            ElevatedOutcome::Started => Ok(Launched::Started),
            ElevatedOutcome::Cancelled => Ok(Launched::Declined),
            ElevatedOutcome::Failed(reason) => Err(ToolFailure::NotStarted(reason)),
            ElevatedOutcome::Unsupported => Err(ToolFailure::ElevationUnsupported),
        };
    }
    let window = if launch.hidden {
        Window::Hidden
    } else {
        Window::Own
    };
    match system_program::launch_detached(command(launch), window) {
        Ok(id) => {
            log::info!("external tool {name:?} started, process {id}");
            Ok(Launched::Started)
        }
        Err(error) => {
            log::warn!("external tool {name:?} failed to start: {error}");
            Err(ToolFailure::NotStarted(match error {
                system_program::ProgramError::NotStarted(reason) => reason,
                other => other.to_string(),
            }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A program that is there on this computer, by its full path.
    fn present() -> PathBuf {
        std::env::current_exe().expect("the test itself")
    }

    fn tool(program: &str, arguments: &str) -> ExternalTool {
        ExternalTool {
            name: "Tool".to_owned(),
            executable_path: program.to_owned(),
            arguments: arguments.to_owned(),
            ..ExternalTool::default()
        }
    }

    fn target(host: &str) -> ToolTarget {
        ToolTarget {
            host: host.to_owned(),
            port: Some(22),
            user: "ops".to_owned(),
            ..ToolTarget::default()
        }
    }

    #[test]
    fn the_program_gets_each_argument_its_own_whatever_the_host_holds() {
        let program = present();
        let home = std::env::temp_dir();
        let launch = prepare(
            &tool(program.to_str().expect("text"), "-P {Port} {User}@{Host}"),
            Some(&target("web.lab & calc \"x y\"")),
            &home,
        )
        .expect("prepared");
        assert_eq!(launch.program, program);
        assert_eq!(launch.arguments, ["-P", "22", "ops@web.lab  calc x y"]);
        let command = command(&launch);
        let given: Vec<String> = command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        assert_eq!(given, launch.arguments, "a list, no shell");
        assert_eq!(command.get_program(), program.as_os_str());
    }

    #[test]
    fn an_elevated_start_hands_windows_the_same_list_quoted() {
        let launch = ToolLaunch {
            program: PathBuf::from(r"C:\Tools\x.exe"),
            arguments: vec!["-a".to_owned(), "two words".to_owned(), "q\"x".to_owned()],
            working_directory: None,
            elevated: true,
            hidden: false,
            home: PathBuf::from(r"C:\Users\me"),
        };
        let request = elevated_request(&launch);
        assert_eq!(request.verb, "runas");
        assert_eq!(request.parameters, r#"-a "two words" q\"x"#);
        assert_eq!(request.working_directory, PathBuf::from(r"C:\Users\me"));
        assert!(!request.hidden);
        let hidden = elevated_request(&ToolLaunch {
            hidden: true,
            ..launch
        });
        assert!(
            hidden.hidden,
            "RunHidden kept, as the C# keeps it with runas"
        );
    }

    #[test]
    fn a_shell_or_a_script_is_refused_before_anything_runs() {
        let home = std::env::temp_dir();
        for program in ["cmd.exe", "powershell", "/bin/sh", "x.bat", "run.ps1"] {
            assert_eq!(
                prepare(&tool(program, "/c {Host}"), Some(&target("h")), &home),
                Err(ToolFailure::ShellTarget),
                "{program}"
            );
        }
    }

    #[test]
    fn a_program_missing_relative_or_blank_is_said_so() {
        let home = std::env::temp_dir();
        let missing = home.join("heimdall-no-such-tool.exe");
        let missing = missing.to_str().expect("text");
        assert_eq!(
            prepare(&tool(missing, ""), None, &home),
            Err(ToolFailure::NotFound(missing.to_owned()))
        );
        assert_eq!(
            prepare(&tool("heimdall-no-such-tool", ""), None, &home),
            Err(ToolFailure::NotFound("heimdall-no-such-tool".to_owned()))
        );
        let relative = if cfg!(windows) {
            r"tools\x.exe"
        } else {
            "tools/x"
        };
        assert_eq!(
            prepare(&tool(relative, ""), None, &home),
            Err(ToolFailure::InvalidProgram(relative.to_owned()))
        );
        assert_eq!(
            prepare(&tool("  ", ""), None, &home),
            Err(ToolFailure::InvalidProgram(String::new()))
        );
    }

    #[test]
    fn a_value_read_as_an_option_or_a_folder_missing_stops_it() {
        let program = present();
        let program = program.to_str().expect("text");
        let home = std::env::temp_dir();
        assert_eq!(
            prepare(
                &tool(program, "{Host}"),
                Some(&target("-oProxyCommand=calc")),
                &home
            ),
            Err(ToolFailure::OptionLike(Placeholder::Host))
        );
        let gone = home.join("heimdall-no-such-folder");
        let gone = gone.to_str().expect("text");
        let mut in_gone = tool(program, "");
        in_gone.working_directory = gone.to_owned();
        assert_eq!(
            prepare(&in_gone, None, &home),
            Err(ToolFailure::WorkingDirectory(gone.to_owned()))
        );
        let mut in_home = tool(program, "");
        in_home.working_directory = home.to_str().expect("text").to_owned();
        assert_eq!(
            prepare(&in_home, None, &home)
                .expect("prepared")
                .working_directory,
            Some(home.clone())
        );
    }

    #[test]
    fn a_nul_in_the_template_is_refused() {
        let program = present();
        assert_eq!(
            prepare(
                &tool(program.to_str().expect("text"), "a\0b"),
                None,
                &std::env::temp_dir()
            ),
            Err(ToolFailure::InvalidArguments)
        );
    }

    #[test]
    fn run_as_administrator_is_windows_only() {
        let program = present();
        let mut elevated = tool(program.to_str().expect("text"), "");
        elevated.run_as_administrator = true;
        let prepared = prepare(&elevated, None, &std::env::temp_dir());
        if cfg!(windows) {
            assert!(prepared.expect("prepared").elevated);
        } else {
            assert_eq!(prepared, Err(ToolFailure::ElevationUnsupported));
        }
    }

    #[test]
    fn the_preview_shows_the_arguments_as_they_are_built() {
        let shown =
            preview(&tool("winscp", "{User}@{Host}"), Some(&target("web lab"))).expect("shown");
        if cfg!(windows) {
            assert_eq!(shown, r#""winscp" "ops@web lab""#);
        } else {
            assert_eq!(shown, "winscp 'ops@web lab'");
        }
        assert_eq!(
            preview(&tool("winscp", "{Host}"), Some(&target("-x"))),
            Err(ToolFailure::OptionLike(Placeholder::Host))
        );
    }

    #[test]
    fn the_protocols_are_named_as_the_csharp_names_them() {
        assert_eq!(protocol_name(ProfileKind::WinRm), "WINRM");
        assert_eq!(protocol_name(ProfileKind::Telnet), "Telnet");
        assert_eq!(protocol_name(ProfileKind::Sftp), "SFTP");
    }
}
