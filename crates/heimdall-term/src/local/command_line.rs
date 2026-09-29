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

//! The command line of a local program, built once: what is shown before it runs is what
//! runs.

use std::path::Path;

use super::LocalArguments;

/// The Windows command line for `program`, a full path, and `arguments`: the program quoted,
/// so that `C:\My Tools\x.exe` is not first tried as `C:\My.exe`; each listed argument quoted
/// as the C runtime reads it back; a Windows argument string as written.
#[cfg_attr(unix, allow(dead_code, reason = "tested everywhere, used on Windows"))]
pub fn windows(program: &Path, arguments: &LocalArguments) -> String {
    let mut line = format!("\"{}\"", program.to_string_lossy());
    let arguments = windows_arguments(arguments);
    if !arguments.is_empty() {
        line.push(' ');
        line.push_str(&arguments);
    }
    line
}

/// `arguments` as the one Windows argument string [`windows`] puts after the program: each
/// listed argument quoted as the C runtime reads it back, a Windows argument string as
/// written. Also what an exported profile hands the C# Heimdall.
#[must_use]
pub fn windows_arguments(arguments: &LocalArguments) -> String {
    match arguments {
        LocalArguments::List(args) => {
            let mut line = String::new();
            for (index, arg) in args.iter().enumerate() {
                if index > 0 {
                    line.push(' ');
                }
                push_quoted(&mut line, arg);
            }
            line
        }
        LocalArguments::WindowsLine(text) => text.clone(),
    }
}

/// `arg` as the C runtime splits it back out of a command line: quoted when it holds a space
/// or a tab or is empty, a quote inside preceded by a backslash, and the backslashes before
/// a quote doubled.
fn push_quoted(line: &mut String, arg: &str) {
    let quoted = arg.is_empty() || arg.contains([' ', '\t']);
    if quoted {
        line.push('"');
    }
    let mut backslashes = 0_usize;
    for c in arg.chars() {
        if c == '\\' {
            backslashes += 1;
        } else {
            if c == '"' {
                line.extend(std::iter::repeat_n('\\', backslashes + 1));
            }
            backslashes = 0;
        }
        line.push(c);
    }
    if quoted {
        line.extend(std::iter::repeat_n('\\', backslashes));
        line.push('"');
    }
}

/// The Unix command for reading: the program and each argument, single-quoted when it holds
/// anything a shell would read as more than text. Only shown: the program gets its arguments
/// one by one, never this text.
#[cfg_attr(windows, allow(dead_code, reason = "tested everywhere, used on Unix"))]
pub fn unix_display(program: &Path, arguments: &LocalArguments) -> String {
    let mut words = vec![shell_quoted(&program.to_string_lossy())];
    match arguments {
        LocalArguments::List(args) => words.extend(args.iter().map(|arg| shell_quoted(arg))),
        LocalArguments::WindowsLine(text) => words.push(text.clone()),
    }
    words.join(" ")
}

/// `word` single-quoted unless it is made of characters no shell reads specially.
fn shell_quoted(word: &str) -> String {
    let plain = !word.is_empty()
        && word
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_./=:,+@%".contains(c));
    if plain {
        word.to_owned()
    } else {
        format!("'{}'", word.replace('\'', r"'\''"))
    }
}

/// Whether Windows runs `program` through `cmd.exe`, which reads the line again: `cmd.exe`
/// itself and batch files.
#[cfg_attr(unix, allow(dead_code, reason = "tested everywhere, used on Windows"))]
pub fn is_cmd(program: &Path) -> bool {
    let is = |part: Option<&std::ffi::OsStr>, name: &str| {
        part.and_then(|part| part.to_str())
            .is_some_and(|part| part.eq_ignore_ascii_case(name))
    };
    is(program.file_name(), "cmd.exe")
        || is(program.extension(), "bat")
        || is(program.extension(), "cmd")
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    #[cfg(windows)]
    use super::is_cmd;
    use super::{LocalArguments, unix_display, windows, windows_arguments};

    fn list(args: &[&str]) -> LocalArguments {
        LocalArguments::List(args.iter().map(|arg| (*arg).to_owned()).collect())
    }

    const PROGRAM: &str = r"C:\My Tools\x.exe";

    /// Each line below was split back with `CommandLineToArgvW` on Windows 11 on 2026-09-27
    /// and gave the listed arguments.
    #[test]
    fn listed_arguments_are_quoted_as_the_c_runtime_reads_them_back() {
        assert_eq!(
            windows(Path::new(PROGRAM), &list(&["-a", "two words", ""])),
            r#""C:\My Tools\x.exe" -a "two words" """#
        );
        assert_eq!(
            windows(
                Path::new(PROGRAM),
                &list(&[r#"say "hi""#, r"C:\dir\", r#"a\"b"#])
            ),
            r#""C:\My Tools\x.exe" "say \"hi\"" C:\dir\ a\\\"b"#
        );
        assert_eq!(
            windows(Path::new(PROGRAM), &list(&[r"C:\a b\"])),
            r#""C:\My Tools\x.exe" "C:\a b\\""#
        );
    }

    #[test]
    fn the_arguments_alone_are_the_line_after_the_program() {
        assert_eq!(windows_arguments(&list(&[])), "");
        assert_eq!(
            windows_arguments(&list(&[""])),
            r#""""#,
            "an empty one kept"
        );
        assert_eq!(
            windows_arguments(&list(&["-a", "two words"])),
            r#"-a "two words""#
        );
        assert_eq!(
            windows_arguments(&LocalArguments::WindowsLine("/c dir".to_owned())),
            "/c dir"
        );
    }

    #[test]
    fn a_windows_argument_string_goes_in_as_written() {
        assert_eq!(
            windows(
                Path::new(PROGRAM),
                &LocalArguments::WindowsLine(r#"/c ""C:\a b\x.bat" arg"  &  calc"#.to_owned())
            ),
            r#""C:\My Tools\x.exe" /c ""C:\a b\x.bat" arg"  &  calc"#
        );
        assert_eq!(
            windows(
                Path::new(PROGRAM),
                &LocalArguments::WindowsLine(String::new())
            ),
            r#""C:\My Tools\x.exe""#
        );
    }

    #[test]
    fn the_unix_display_quotes_what_a_shell_would_read() {
        assert_eq!(
            unix_display(
                Path::new("/bin/sh"),
                &list(&["-c", "echo a; rm x", "it's", "-l"])
            ),
            r"/bin/sh -c 'echo a; rm x' 'it'\''s' -l"
        );
        assert_eq!(
            unix_display(Path::new("/bin/sh"), &list(&[""])),
            "/bin/sh ''"
        );
    }

    /// Windows paths: on Unix a backslash is not a separator.
    #[cfg(windows)]
    #[test]
    fn cmd_and_batch_files_are_known_to_read_the_line_again() {
        for program in [
            r"C:\Windows\System32\cmd.exe",
            r"C:\Windows\System32\CMD.EXE",
            r"C:\x\run.bat",
            r"C:\x\run.CMD",
        ] {
            assert!(is_cmd(Path::new(program)), "{program}");
        }
        for program in [
            r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe",
            r"C:\x\cmd.exe.txt",
            r"C:\x\notcmd.exe",
        ] {
            assert!(!is_cmd(Path::new(program)), "{program}");
        }
    }
}
