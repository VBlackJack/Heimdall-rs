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

//! A local shell profile's form, as the C# "Local shell" card: the program, its arguments
//! as one line, the folder it starts in.
//!
//! The line is kept as Windows reads it on Windows, where each program splits its own
//! command line, as the C# Heimdall passed it; elsewhere it is split as a POSIX shell would,
//! since a program receives its arguments one by one.

use heimdall_core::profile::LocalArguments;

/// Shells the executable list offers, as the C# one on Windows: what it runs, then its name.
#[cfg(windows)]
pub const SHELL_PRESETS: [&str; 5] = ["powershell.exe", "pwsh.exe", "cmd.exe", "bash", "wsl.exe"];

/// Shells the executable list offers.
#[cfg(not(windows))]
pub const SHELL_PRESETS: [&str; 4] = ["bash", "zsh", "sh", "pwsh"];

/// The arguments `line` stands for; `None` when it cannot be read (a quote left open).
#[must_use]
pub fn arguments_of(line: &str) -> Option<LocalArguments> {
    let line = line.trim();
    if line.is_empty() {
        return Some(LocalArguments::default());
    }
    if cfg!(windows) {
        Some(LocalArguments::WindowsLine(line.to_owned()))
    } else {
        shlex::split(line).map(LocalArguments::List)
    }
}

/// The arguments as the form's line shows them: a Windows line as written, a list quoted as
/// a POSIX shell reads it back.
#[must_use]
pub fn line_of(arguments: &LocalArguments) -> String {
    match arguments {
        LocalArguments::WindowsLine(line) => line.clone(),
        LocalArguments::List(words) => {
            shlex::try_join(words.iter().map(String::as_str)).unwrap_or_else(|_| words.join(" "))
        }
    }
}

#[cfg(test)]
mod tests {
    use heimdall_core::profile::LocalArguments;

    use super::{arguments_of, line_of};

    #[test]
    fn an_empty_line_is_no_argument() {
        assert_eq!(arguments_of("   "), Some(LocalArguments::default()));
    }

    #[cfg(not(windows))]
    #[test]
    fn a_line_is_split_as_a_posix_shell_does_and_reads_back() {
        let arguments = arguments_of(r#"-l -c "echo 'a b'" --x=y"#).expect("read");
        assert_eq!(
            arguments,
            LocalArguments::List(vec![
                "-l".to_owned(),
                "-c".to_owned(),
                "echo 'a b'".to_owned(),
                "--x=y".to_owned(),
            ])
        );
        assert_eq!(arguments_of(&line_of(&arguments)), Some(arguments));
        assert_eq!(arguments_of(r#"-c "open"#), None, "a quote left open");
    }

    #[cfg(windows)]
    #[test]
    fn a_line_is_kept_as_windows_reads_it() {
        let line = r#"/k "C:\a b\x.bat" & echo"#;
        assert_eq!(
            arguments_of(line),
            Some(LocalArguments::WindowsLine(line.to_owned()))
        );
        assert_eq!(line_of(&LocalArguments::WindowsLine(line.to_owned())), line);
    }

    #[test]
    fn an_imported_windows_line_is_shown_as_written() {
        let line = "-NoExit -Command Get-Date";
        assert_eq!(line_of(&LocalArguments::WindowsLine(line.to_owned())), line);
    }
}
