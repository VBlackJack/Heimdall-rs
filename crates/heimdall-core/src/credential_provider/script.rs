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

//! Values put inside a script a shell reads (`sh -c "pass show {Title}"`).
//!
//! The C# strips the characters a shell reads from such a value (`SanitizeStrict`) and leaves
//! its spaces, so a title `x -c` still split the script into one more word. Quoting every value
//! for every shell, and for the shell behind `wsl` or `env`, is too fragile to trust: instead a
//! value inside a script may hold only letters, digits, a space and a few punctuation marks
//! that no shell reads, and anything else is refused. Where the template has no quote of its
//! own, a POSIX shell or PowerShell also gets the value in single quotes; `cmd` gets no quote
//! from here, its template's own quotes are the only ones, as in the C#.
//!
//! Only `sh`-like shells, `cmd` and PowerShell are known. `fish`, `nu` and other shells are out
//! of scope: they are not detected as shells, so their scripts are not treated as scripts.

use super::TemplateProblem;

/// The shell that reads a script.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Dialect {
    /// `sh`, `bash`, `zsh`, `dash`, `ksh`, and `wsl` (whose command runs on Linux).
    Posix,
    /// `cmd` and batch files.
    Cmd,
    /// Windows PowerShell and `pwsh`.
    PowerShell,
    /// A script host (`cscript`, `mshta`...): its scripts are not read for placeholders.
    Opaque,
}

/// Which quote is open at the end of the script text read so far.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Open {
    None,
    Single,
    Double,
}

/// The escape character of a POSIX shell.
const BACKSLASH: char = '\\';
/// The single quote of a POSIX shell.
const APOSTROPHE: char = '\'';
/// PowerShell reads these as a single quote, like the apostrophe.
const POWERSHELL_SINGLE: &str = "'\u{2018}\u{2019}\u{201a}\u{201b}";
/// What a value in a script may hold besides letters and digits.
const ALLOWED_PUNCTUATION: &str = " ._@/:+=,-";

/// Where the script of a command starts among its words, and whether it runs to the end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ScriptSpan {
    /// The shell that reads it.
    pub dialect: Dialect,
    /// The index of the first word of the script.
    pub start: usize,
    /// Every word from there on is the script (`cmd /c a b c`), else only that one.
    pub to_end: bool,
}

/// The dialect of `name` when it is a shell, as the C# `InputValidator.IsShellTarget` names
/// them, with `dash` and `ksh`; `None` for any other program.
pub(super) fn dialect_of(name: &str) -> Option<Dialect> {
    let file = name
        .trim()
        .trim_end_matches(['.', ' '])
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let (stem, extension) = file.rsplit_once('.').unwrap_or((&file, ""));
    match extension {
        "bat" | "cmd" => return Some(Dialect::Cmd),
        "ps1" => return Some(Dialect::PowerShell),
        "vbs" | "js" | "jse" | "vbe" | "wsf" | "hta" => return Some(Dialect::Opaque),
        _ => {}
    }
    let stem = if extension == "exe" { stem } else { &file };
    match stem {
        "cmd" => Some(Dialect::Cmd),
        "powershell" | "pwsh" => Some(Dialect::PowerShell),
        "bash" | "sh" | "zsh" | "dash" | "ksh" | "wsl" => Some(Dialect::Posix),
        "cscript" | "wscript" | "mshta" => Some(Dialect::Opaque),
        _ => None,
    }
}

/// The script of the shell named by `words[shell]`, if its words after it hold one.
pub(super) fn script_of(dialect: Dialect, words: &[String], shell: usize) -> Option<ScriptSpan> {
    let rest = words.get(shell + 1..)?;
    let name = file_stem(words.get(shell)?);
    let (offset, to_end) = match dialect {
        // `wsl` runs the words after its options through the shell of the distribution.
        Dialect::Posix if name == "wsl" => (wsl_script(rest)?, true),
        Dialect::Posix => (posix_script(rest)?, false),
        Dialect::Cmd => (cmd_script(rest)?, true),
        // pwsh 7 reads a first word that is no option as a file (-File is its implicit mode);
        // Windows PowerShell 5.1 reads it as a command.
        Dialect::PowerShell => (powershell_script(rest, name == "powershell")?, true),
        Dialect::Opaque => return None,
    };
    Some(ScriptSpan {
        dialect,
        start: shell + 1 + offset,
        to_end,
    })
}

/// The lowercase file name of `program`, less `.exe`.
fn file_stem(program: &str) -> String {
    let name =
        super::program_file_name(program.trim().trim_end_matches(['.', ' '])).to_ascii_lowercase();
    name.strip_suffix(".exe")
        .map_or(name.clone(), str::to_owned)
}

/// `wsl`'s options that take a value.
const WSL_VALUE_OPTIONS: [&str; 5] = ["-d", "--distribution", "-u", "--user", "--cd"];

/// `wsl`'s command is the first word that is no option, or what follows `--`: a script for the
/// shell of the distribution (brace expansion and the rest included). `-e` and `--exec` run
/// the program directly instead: its words are plain arguments, no script.
fn wsl_script(rest: &[String]) -> Option<usize> {
    let mut index = 0;
    while let Some(word) = rest.get(index) {
        let lower = word.to_ascii_lowercase();
        match lower.as_str() {
            "-e" | "--exec" => return None,
            "--" => return (index + 1 < rest.len()).then_some(index + 1),
            option if WSL_VALUE_OPTIONS.contains(&option) => index += 1,
            option if option.starts_with('-') => {}
            _ => return Some(index),
        }
        index += 1;
    }
    None
}

/// A POSIX shell's script is the word after a flag cluster holding `c` (`-c`, `-lc`, `-ec`),
/// before any word that is not an option. Long options (`--login`, `--norc`) are skipped, and
/// so is the value of the ones that take one (`-o pipefail`, `+o xtrace`, `-O extglob`).
fn posix_script(rest: &[String]) -> Option<usize> {
    let mut index = 0;
    while let Some(word) = rest.get(index) {
        if word == "--" {
            return None;
        }
        if word.starts_with("--") {
            index += 1;
            continue;
        }
        let flags = word
            .strip_prefix(['-', '+'])
            .filter(|flags| !flags.is_empty() && flags.chars().all(|c| c.is_ascii_alphabetic()))?;
        if word.starts_with('-') && flags.contains('c') {
            return (index + 1 < rest.len()).then_some(index + 1);
        }
        // `-o name`, `+o name`, `-O name`: the name is the option's value.
        if flags.ends_with(['o', 'O']) {
            index += 1;
        }
        index += 1;
    }
    None
}

/// `cmd`'s script is everything after `/c`, `/k` or `/r`; the flags before it start with `/`.
fn cmd_script(rest: &[String]) -> Option<usize> {
    for (index, word) in rest.iter().enumerate() {
        let lower = word.to_ascii_lowercase();
        if matches!(lower.as_str(), "/c" | "/k" | "/r") {
            return (index + 1 < rest.len()).then_some(index + 1);
        }
        if !word.starts_with('/') {
            return None;
        }
    }
    None
}

/// PowerShell's options that take a value, by the name they are written with.
const POWERSHELL_VALUE_OPTIONS: [&str; 9] = [
    "executionpolicy",
    "windowstyle",
    "inputformat",
    "outputformat",
    "configurationname",
    "workingdirectory",
    "version",
    "custompipename",
    "settingsfile",
];

/// Whether `flag` is a prefix of `name` (and not empty), as PowerShell reads its options.
fn prefix_of(flag: &str, name: &str) -> bool {
    !flag.is_empty() && name.starts_with(&flag.to_ascii_lowercase())
}

/// PowerShell's script is what follows `-Command` (any unambiguous prefix of it, `-c`, `-co`),
/// or, for Windows PowerShell, the first word that is no option when none is named (the
/// implicit `-Command`); never when `-File` or `-EncodedCommand` comes first.
fn powershell_script(rest: &[String], implicit_command: bool) -> Option<usize> {
    let mut index = 0;
    while let Some(word) = rest.get(index) {
        let Some(flag) = word.strip_prefix(['-', '/']) else {
            // The first word that is no option: the implicit -Command (5.1), a file (7).
            return implicit_command.then_some(index);
        };
        if prefix_of(flag, "command") {
            return (index + 1 < rest.len()).then_some(index + 1);
        }
        if prefix_of(flag, "file") || prefix_of(flag, "encodedcommand") {
            return None;
        }
        // An option with a value takes the next word with it.
        if POWERSHELL_VALUE_OPTIONS
            .iter()
            .any(|name| prefix_of(flag, name))
        {
            index += 1;
        }
        index += 1;
    }
    None
}

/// The quote left open by `text`, a script read up to a placeholder.
fn open_quote(dialect: Dialect, text: &str) -> Open {
    // The character that escapes the next one outside single quotes, if the shell has one.
    let escape = match dialect {
        Dialect::Posix => Some(BACKSLASH),
        Dialect::PowerShell => Some('`'),
        Dialect::Cmd | Dialect::Opaque => None,
    };
    let is_single = |c: char| match dialect {
        Dialect::Posix => c == APOSTROPHE,
        Dialect::PowerShell => POWERSHELL_SINGLE.contains(c),
        Dialect::Cmd | Dialect::Opaque => false,
    };
    let mut state = Open::None;
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if state != Open::Single && Some(c) == escape {
            chars.next();
        } else if c == '"' {
            state = match state {
                Open::Double => Open::None,
                Open::None => Open::Double,
                Open::Single => Open::Single,
            };
        } else if is_single(c) {
            state = match state {
                Open::Single => Open::None,
                Open::None => Open::Single,
                Open::Double => Open::Double,
            };
        }
    }
    state
}

/// Whether the next thing written after `text` begins a word of the script (an option could
/// stand there).
pub(super) fn begins_word(text: &str) -> bool {
    let before = text.trim_end_matches(['\'', '"']);
    before.is_empty() || before.ends_with(char::is_whitespace)
}

/// Whether `text` holds a lone `--`: the end of the options of the command it writes.
pub(super) fn has_end_of_options(text: &str) -> bool {
    text.split_whitespace().any(|word| word == "--")
}

/// Whether a value put into the script `before` reaches the shell as plain, unquoted text that
/// it splits at the spaces: a `cmd` script with no quote of the template around the value.
/// `wrapped` is a value that is a whole argument of the template written in quotes
/// (`"{Title}"`). A value in single quotes added here, or in quotes of the template, is one
/// argument whatever it holds.
pub(super) fn splits_at_spaces(dialect: Dialect, before: &str, wrapped: bool) -> bool {
    dialect == Dialect::Cmd && !wrapped && open_quote(dialect, before) == Open::None
}

/// Whether a value in a script may hold `c`.
fn allowed(c: char) -> bool {
    c.is_alphanumeric() || ALLOWED_PUNCTUATION.contains(c)
}

/// `value` as it is written into the script `before` (the script so far).
///
/// # Errors
///
/// [`TemplateProblem::UnquotableValue`] for `token` when the value holds anything but letters,
/// digits, a space and `. _ @ / : + = , -`: no shell reads those, so no quoting has to be right.
pub(super) fn quote(
    dialect: Dialect,
    before: &str,
    value: &str,
    token: &'static str,
) -> Result<String, TemplateProblem> {
    // A Windows shell reads a backslash as part of a path; a POSIX one (also behind `wsl`,
    // `env`, `sudo`) reads it as an escape. Ending a `cmd` value with one would escape the
    // closing quote of the template.
    let windows_path = matches!(dialect, Dialect::Cmd | Dialect::PowerShell);
    let backslash = |c: char| windows_path && c == BACKSLASH;
    if !value.chars().all(|c| allowed(c) || backslash(c))
        || (dialect == Dialect::Cmd && value.ends_with(BACKSLASH))
    {
        return Err(TemplateProblem::UnquotableValue(token));
    }
    // On top of the list, a POSIX shell or PowerShell holds the value in single quotes unless
    // the template opened a quote already; nothing in it can close one. `cmd` gets none.
    let quoted = matches!(dialect, Dialect::Posix | Dialect::PowerShell)
        && open_quote(dialect, before) == Open::None;
    Ok(if quoted {
        format!("'{value}'")
    } else {
        value.to_owned()
    })
}
