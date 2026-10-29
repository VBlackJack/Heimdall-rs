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

//! The external credential provider, as the C# Heimdall's: a password manager's command
//! line (`KeePassXC`, Bitwarden, pass...) asked for a server's password when none is saved.
//!
//! The command template is cut into arguments first, and each placeholder is put inside the
//! argument it stands in: a value can never add an argument, whatever it holds. No shell runs
//! the command unless the template names one; then the values lose the characters a shell
//! reads, as the C# strict rule strips them.

use std::time::Duration;

mod script;

use script::{Dialect, ScriptSpan};

/// Where the password comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ProviderKind {
    /// A command line, whose output is the password.
    #[default]
    Command,
    /// Windows Credential Manager's generic credentials.
    WindowsCredentialManager,
}

impl ProviderKind {
    /// The name the settings file holds, the C# one.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Command => "Command",
            Self::WindowsCredentialManager => "WindowsCredentialManager",
        }
    }

    /// The kind named `name`, whatever its case; a command for a name not known, as the C#
    /// factory falls back.
    #[must_use]
    pub fn named(name: &str) -> Self {
        if name
            .trim()
            .eq_ignore_ascii_case(Self::WindowsCredentialManager.name())
        {
            Self::WindowsCredentialManager
        } else {
            Self::Command
        }
    }
}

/// Shortest time a command is given, as the C# setting's range.
pub const MIN_TIMEOUT: Duration = Duration::from_secs(1);
/// Longest time a command is given, as the C# setting's range.
pub const MAX_TIMEOUT: Duration = Duration::from_mins(2);
/// Time a command is given unless chosen, as the C# default.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);

/// The provider's settings, as the C# Settings page holds them. The unlock secret is not
/// here: it is kept with the saved passwords.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderSettings {
    /// Asked at all.
    pub enabled: bool,
    /// Where the password comes from.
    pub kind: ProviderKind,
    /// The command giving the password.
    pub command: String,
    /// The command giving the user name, when a profile has none; empty when not used.
    pub username_command: String,
    /// The password database, for `{Database}`.
    pub database: String,
    /// The database's key file, for `{KeyFile}`.
    pub key_file: String,
    /// Only the first line of the output that is not empty is the password.
    pub first_line_only: bool,
    /// How long a command is given before it is stopped.
    pub timeout: Duration,
}

impl Default for ProviderSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            kind: ProviderKind::default(),
            command: String::new(),
            username_command: String::new(),
            database: String::new(),
            key_file: String::new(),
            first_line_only: false,
            timeout: DEFAULT_TIMEOUT,
        }
    }
}

/// A quick setup preset, as the C# `CredentialProviderPresetService` lists it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Preset {
    /// The tool's own name, shown in every language.
    pub name: &'static str,
    /// The command template.
    pub command: &'static str,
    /// Whether the tool prints more than the value, so that only the first line is read:
    /// picking the preset sets the "first line only" option to this.
    pub first_line_only: bool,
}

/// The command templates offered, as the C# quick setup lists them.
///
/// Where a tool reads `--` as the end of its options, the entry names follow it, so that a
/// name starting with `-` is never read as a flag. `KPScript` has no such marker: a value
/// starting with `-` is refused for it, see [`command_line`]. The 1Password preset has the C#
/// words with the flags moved before the name, since `op` (cobra) reads what follows `--` as
/// operands.
///
/// `KPScript` prints an `OK: ...` status line after the value and `pass` prints the notes after
/// the password, so both turn "first line only" on; `KPScript` also takes `-FailIfNoEntry`, so
/// that a missing entry is an error and not a status line left on line 1.
pub const PRESETS: [Preset; 7] = [
    Preset {
        name: "KeePassXC",
        command: r#"keepassxc-cli show -s -q -a Password -- "{Database}" "{Title}""#,
        first_line_only: false,
    },
    Preset {
        name: "KeePassXC (key file)",
        command: r#"keepassxc-cli show -s -q -k "{KeyFile}" -a Password -- "{Database}" "{Title}""#,
        first_line_only: false,
    },
    Preset {
        name: "KeePassXC (key file only)",
        command: r#"keepassxc-cli show -s -q --no-password -k "{KeyFile}" -a Password -- "{Database}" "{Title}""#,
        first_line_only: false,
    },
    Preset {
        name: "KeePass2 (KPScript)",
        command: r#"KPScript.exe -c:GetEntryString "{Database}" -Field:Password -ref-Title:"{Title}" -FailIfNoEntry"#,
        first_line_only: true,
    },
    Preset {
        name: "Bitwarden CLI",
        command: r#"bw get password -- "{Title}""#,
        first_line_only: false,
    },
    Preset {
        name: "1Password CLI",
        command: r#"op item get --fields label=password --reveal -- "{Title}""#,
        first_line_only: false,
    },
    Preset {
        name: "pass (GPG)",
        command: r#"pass show -- "{Title}""#,
        first_line_only: true,
    },
];

/// The server a password is asked for, as the placeholders name it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lookup {
    /// `{Host}`.
    pub host: String,
    /// `{Port}`.
    pub port: u16,
    /// `{User}`: the profile's user name, when it has one.
    pub user: Option<String>,
    /// `{Title}`: the profile's entry name, else its name.
    pub title: String,
}

/// The port the Test button asks with, the C# `DefaultPorts.Ssh`.
pub const TEST_PORT: u16 = 22;

impl Lookup {
    /// What the Test button asks with, as the C# one: the entry typed stands in for the host
    /// too, so that a template keyed on `{Host}` is tested with the name typed, and no user
    /// name is given, so that the user name command runs when there is one.
    #[must_use]
    pub fn for_test(entry: &str) -> Self {
        Self {
            host: entry.to_owned(),
            port: TEST_PORT,
            user: None,
            title: entry.to_owned(),
        }
    }
}

/// Why a command template cannot run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TemplateProblem {
    /// Nothing to run.
    Empty,
    /// A quote is opened and never closed.
    UnclosedQuote,
    /// The template uses `{KeyFile}` and no key file is chosen.
    NoKeyFile,
    /// The value of this placeholder starts with `-` and would stand where the tool reads an
    /// option, with no `--` before it to end them. Refused: it could be a flag of the tool.
    OptionLikeValue(&'static str),
    /// The value of this placeholder cannot be held as one word by the shell that reads the
    /// script it stands in (a `cmd` metacharacter, a quote the template already opened...).
    UnquotableValue(&'static str),
}

/// The program and arguments `template` runs for `lookup`, placeholders filled.
///
/// The placeholders are filled in one pass, so that a value holding another placeholder's name
/// (`{KeyFile}` in an entry name) is kept as it is, as the C# `ArgumentTemplate.Expand`.
///
/// A value starting with `-` (or a Unicode dash) that would open an argument is refused unless
/// a `--` argument of the template comes before it: the tool would read it as an option, and a
/// vault entry named `--output=x` must not be able to change what the tool does.
///
/// A script a shell reads (the word after `sh -c`, the words after `cmd /c`...) gets each value
/// quoted for that shell, so that it stays one word whatever it holds; where the shell has no
/// safe quoting the value is refused. The check for a leading dash runs on the words of the
/// script too, and an outer `--` (`wsl -- sh -c "..."`) does not end the inner options.
///
/// # Errors
///
/// [`TemplateProblem`] when there is nothing to run, a quote is not closed, a key file is
/// asked for and none is chosen, or a value would be read as an option or cannot be quoted.
pub fn command_line(
    template: &str,
    lookup: &Lookup,
    settings: &ProviderSettings,
) -> Result<Vec<String>, TemplateProblem> {
    let words = split(template)?;
    let Some(program) = words.first() else {
        return Err(TemplateProblem::Empty);
    };
    if contains_placeholder(template, "{KeyFile}") && settings.key_file.trim().is_empty() {
        return Err(TemplateProblem::NoKeyFile);
    }
    let port = lookup.port.to_string();
    let values = [
        ("{Host}", lookup.host.clone()),
        ("{Port}", port),
        ("{User}", lookup.user.clone().unwrap_or_default()),
        ("{Title}", lookup.title.clone()),
        ("{Database}", settings.database.trim().to_owned()),
        ("{KeyFile}", settings.key_file.trim().to_owned()),
    ];
    let shell = is_shell_target(program);
    let wrapper = is_wrapper(program);
    let mut span: Option<ScriptSpan> =
        script::dialect_of(program).and_then(|dialect| script::script_of(dialect, &words, 0));
    let mut options_ended = false;
    let mut line = Vec::with_capacity(words.len());
    for (index, word) in words.iter().enumerate() {
        // The program itself is the template's, never a value's.
        if index == 0 {
            line.push(word.clone());
            continue;
        }
        let script = span
            .filter(|span| index == span.start || (span.to_end && index > span.start))
            .map(|span| span.dialect);
        let context = Context {
            script,
            shell,
            options_ended,
        };
        line.push(expand(word, &values, &context)?);
        // A shell named in the template (behind `wsl`, `env`, `sudo`) reads a script of its own.
        if script.is_none()
            && !word.contains('{')
            && let Some(dialect) = script::dialect_of(word)
            && let Some(found) = script::script_of(dialect, &words, index)
        {
            span = Some(found);
        }
        // Only a `--` the template wrote ends the options, never a value. Right after a wrapper
        // (`wsl --`, `env --`, `sudo --`) it is the wrapper's own: the program it runs has its
        // options still. Anywhere else it ends the options of the program before it.
        if word == END_OF_OPTIONS && !(index == 1 && wrapper) {
            options_ended = true;
        }
    }
    Ok(line)
}

/// Whether `program` only starts another one: `wsl`, `env`, `sudo`.
fn is_wrapper(program: &str) -> bool {
    let name = program_file_name(program).to_ascii_lowercase();
    matches!(
        name.strip_suffix(".exe").unwrap_or(&name),
        "wsl" | "env" | "sudo"
    )
}

/// The argument that ends a tool's options.
const END_OF_OPTIONS: &str = "--";

/// Where a word of the template stands.
struct Context {
    /// The shell reading the word as its script, when it is one.
    script: Option<Dialect>,
    /// The program is a shell: its other words lose what a shell reads.
    shell: bool,
    /// A `--` of the template came before.
    options_ended: bool,
}

/// Whether `value` opens with a dash: the ASCII one, a Unicode hyphen or dash (U+2010 to
/// U+2015), or the minus sign (U+2212), which some tools normalise to it.
fn starts_with_dash(value: &str) -> bool {
    value
        .chars()
        .next()
        .is_some_and(|c| c == '-' || ('\u{2010}'..='\u{2015}').contains(&c) || c == '\u{2212}')
}

/// `word` with each placeholder of `values`, whatever its case, replaced by its value in one
/// pass over `word`: what is put in is never scanned again.
fn expand(
    word: &str,
    values: &[(&'static str, String)],
    context: &Context,
) -> Result<String, TemplateProblem> {
    let mut out = String::with_capacity(word.len());
    let mut at = 0;
    while let Some(rest) = word.get(at..).filter(|rest| !rest.is_empty()) {
        let found = values.iter().find(|(name, _)| {
            rest.get(..name.len())
                .is_some_and(|head| head.eq_ignore_ascii_case(name))
        });
        if let Some((name, value)) = found {
            // One argument whatever it holds; a line break is never a value's.
            let raw: String = value
                .chars()
                .filter(|c| !matches!(c, '\r' | '\n'))
                .collect();
            let (begins, ended) = match context.script {
                Some(_) => (
                    script::begins_word(&out),
                    context.options_ended || script::has_end_of_options(&out),
                ),
                None => (out.is_empty(), context.options_ended),
            };
            if begins && !ended && starts_with_dash(&raw) {
                return Err(TemplateProblem::OptionLikeValue(name));
            }
            // In a script a value splits at its spaces: each word after the first is a word of
            // the command, wherever the options ended.
            if context.script.is_some() {
                let lead = raw.starts_with(char::is_whitespace);
                if raw
                    .split_whitespace()
                    .enumerate()
                    .any(|(at, token)| (at > 0 || lead) && starts_with_dash(token))
                {
                    return Err(TemplateProblem::OptionLikeValue(name));
                }
            }
            let text = match context.script {
                Some(dialect) => script::quote(dialect, &out, &raw, name)?,
                None if context.shell => strip_for_shell(&raw),
                None => raw,
            };
            out.push_str(&text);
            at += name.len();
        } else if let Some(c) = rest.chars().next() {
            out.push(c);
            at += c.len_utf8();
        }
    }
    Ok(out)
}

/// `template` cut into words: spaces part them, double quotes group them and are dropped.
fn split(template: &str) -> Result<Vec<String>, TemplateProblem> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut quoted = false;
    let mut started = false;
    for c in template.chars() {
        match c {
            '"' => {
                quoted = !quoted;
                started = true;
            }
            c if c.is_whitespace() && !quoted => {
                if started {
                    words.push(std::mem::take(&mut word));
                    started = false;
                }
            }
            c => {
                word.push(c);
                started = true;
            }
        }
    }
    if quoted {
        return Err(TemplateProblem::UnclosedQuote);
    }
    if started {
        words.push(word);
    }
    Ok(words)
}

fn contains_placeholder(template: &str, name: &str) -> bool {
    template
        .to_ascii_lowercase()
        .contains(&name.to_ascii_lowercase())
}

/// Scripts and shells: what reads its arguments as a command line, as the C# list names
/// them.
const SHELL_EXTENSIONS: [&str; 9] = ["bat", "cmd", "ps1", "vbs", "js", "jse", "vbe", "wsf", "hta"];
const SHELL_NAMES: [&str; 12] = [
    "cmd",
    "powershell",
    "pwsh",
    "bash",
    "sh",
    "zsh",
    "dash",
    "ksh",
    "wsl",
    "cscript",
    "wscript",
    "mshta",
];

/// Whether `program` reads its arguments as a command line: a shell or a script.
#[must_use]
pub fn is_shell_target(program: &str) -> bool {
    let name = program
        .trim()
        .trim_end_matches(['.', ' '])
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    if name.is_empty() {
        return true;
    }
    let (stem, extension) = name.rsplit_once('.').unwrap_or((&name, ""));
    if SHELL_EXTENSIONS.contains(&extension) {
        return true;
    }
    let stem = if extension == "exe" {
        stem
    } else {
        name.as_str()
    };
    SHELL_NAMES.contains(&stem)
}

/// `value` without what a shell reads, as the C# strict rule strips it.
fn strip_for_shell(value: &str) -> String {
    value
        .chars()
        .filter(|c| !";&|`$<>()!\"'\r\n%^".contains(*c))
        .collect()
}

/// The prefixes of `KPScript`'s status lines, printed after the value.
const KPSCRIPT_OK_PREFIX: &str = "OK: ";
const KPSCRIPT_ERROR_PREFIX: &str = "E: ";
/// The start of a one-time password URI, which is a secret seed and never a password.
const OTPAUTH_PREFIX: &str = "otpauth://";

/// The password in a command's `output`, as the C# `CommandCredentialProvider` reads it: only
/// the line breaks around the value are removed, never a space or a tab (a password may begin or
/// end with one, and `keepassxc-cli` prints it so); with `first_line_only`, the first line that
/// is not blank, unless it is a `KPScript` status line. `None` when nothing but blanks is left.
#[must_use]
pub fn password_in(output: &str, first_line_only: bool) -> Option<String> {
    let value = if first_line_only {
        first_value_line(output)
    } else {
        output.trim_matches(['\r', '\n'])
    };
    (!value.trim().is_empty()).then(|| value.to_owned())
}

/// The first line of `text` that is not blank, without its `\r`; empty when it is a `KPScript`
/// status line, which is no value: for an entry whose field is empty `KPScript` prints a blank
/// line and then `OK: ...`, and the status line must not be handed over as the password.
fn first_value_line(text: &str) -> &str {
    text.split('\n')
        .map(|line| line.trim_end_matches('\r'))
        .find(|line| !line.trim().is_empty())
        .filter(|line| {
            // Deliberately stricter than the C#: a tool that prints its one-time password URI
            // first (pass with a `otpauth://` line, an entry's notes) must not hand the seed
            // over as the password of the server.
            let head = line.trim_start().to_ascii_lowercase();
            !line.starts_with(KPSCRIPT_OK_PREFIX)
                && !line.starts_with(KPSCRIPT_ERROR_PREFIX)
                && !head.starts_with(OTPAUTH_PREFIX)
        })
        .unwrap_or_default()
}

/// What the Test button says of the shape of the value it read, never the value itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ValueShape {
    /// How many lines the value spans.
    pub lines: usize,
    /// It begins or ends with a space (or any blank): it is sent as is.
    pub edge_space: bool,
}

/// The shape of `value`, as the C# Test button describes it.
#[must_use]
pub fn value_shape(value: &str) -> ValueShape {
    ValueShape {
        lines: value.split('\n').count(),
        edge_space: value.chars().next().is_some_and(char::is_whitespace)
            || value.chars().next_back().is_some_and(char::is_whitespace),
    }
}

/// The file name of `program`, as the template wrote it: what can be said of a command without
/// its arguments, which may hold a token.
#[must_use]
pub fn program_file_name(program: &str) -> &str {
    program.rsplit(['/', '\\']).next().unwrap_or(program)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lookup(title: &str) -> Lookup {
        Lookup {
            host: "web.lab".to_owned(),
            port: 2222,
            user: Some("admin".to_owned()),
            title: title.to_owned(),
        }
    }

    fn settings() -> ProviderSettings {
        ProviderSettings {
            database: r"C:\Vaults\Team Vault.kdbx".to_owned(),
            key_file: "/keys/team.keyx".to_owned(),
            ..ProviderSettings::default()
        }
    }

    #[test]
    fn each_placeholder_is_filled_inside_its_own_argument() {
        let line = command_line(
            r#"tool --db "{Database}" {host}:{Port} -u {User} "{Title}" -k {KeyFile}"#,
            &lookup("Web server"),
            &settings(),
        )
        .expect("runs");
        assert_eq!(
            line,
            [
                "tool",
                "--db",
                r"C:\Vaults\Team Vault.kdbx",
                "web.lab:2222",
                "-u",
                "admin",
                "Web server",
                "-k",
                "/keys/team.keyx",
            ]
        );
    }

    #[test]
    fn a_value_never_adds_an_argument() {
        // Spaces, quotes and a trailing backslash: one argument all the same.
        let title = r#"a" --delete "b \"#;
        let line = command_line("tool {Title}", &lookup(title), &settings()).expect("runs");
        assert_eq!(line, ["tool", title]);
        let line = command_line("tool {Title}", &lookup("x\ny"), &settings()).expect("runs");
        assert_eq!(line, ["tool", "xy"], "no line break");
    }

    #[test]
    fn a_shell_s_own_arguments_lose_what_it_would_read() {
        for program in ["cmd", "cmd.exe", "bash", "run.bat", r"C:\x\y.CMD. "] {
            let line = command_line(
                &format!("{program} get {{Title}}"),
                &lookup(r#"a&b|c$(d)"e'f%g^h!i;j<k>l`m"#),
                &settings(),
            )
            .expect("runs");
            assert_eq!(
                line.last().map(String::as_str),
                Some("abcdefghijklm"),
                "{program}"
            );
        }
        let line =
            command_line("keepassxc-cli {Title}", &lookup("a&b"), &settings()).expect("runs");
        assert_eq!(line[1], "a&b", "not a shell: kept whole");
    }

    #[test]
    fn the_program_is_the_template_s() {
        let line = command_line("{Title} x", &lookup("evil"), &settings()).expect("runs");
        assert_eq!(line[0], "{Title}");
    }

    #[test]
    fn what_cannot_run_is_said() {
        assert_eq!(
            command_line("  ", &lookup("t"), &settings()),
            Err(TemplateProblem::Empty)
        );
        assert_eq!(
            command_line(r#"tool "{Title}"#, &lookup("t"), &settings()),
            Err(TemplateProblem::UnclosedQuote)
        );
        let no_key = ProviderSettings {
            key_file: " ".to_owned(),
            ..settings()
        };
        assert_eq!(
            command_line("tool -k {keyfile}", &lookup("t"), &no_key),
            Err(TemplateProblem::NoKeyFile)
        );
    }

    #[test]
    fn every_preset_runs_with_its_values() {
        for preset in PRESETS {
            let line = command_line(preset.command, &lookup("Web server"), &settings())
                .unwrap_or_else(|problem| panic!("{}: {problem:?}", preset.name));
            assert!(
                line.iter().any(|word| word.contains("Web server")),
                "{}: {line:?}",
                preset.name
            );
            assert!(
                !line.iter().any(|word| word.contains('{')),
                "{}: {line:?}",
                preset.name
            );
        }
    }

    /// The preset table of the C# `CredentialProviderPresetService`, command and "first line
    /// only" side by side. The words are the C# ones, less the `--` this port adds and the
    /// place of the 1Password flags (moved before the name so that `--` can end them).
    #[test]
    fn the_presets_are_the_csharp_ones() {
        let csharp = [
            (
                "KeePassXC",
                r#"keepassxc-cli show -s -q -a Password "{Database}" "{Title}""#,
                false,
            ),
            (
                "KeePassXC (key file)",
                r#"keepassxc-cli show -s -q -k "{KeyFile}" -a Password "{Database}" "{Title}""#,
                false,
            ),
            (
                "KeePassXC (key file only)",
                r#"keepassxc-cli show -s -q --no-password -k "{KeyFile}" -a Password "{Database}" "{Title}""#,
                false,
            ),
            (
                "KeePass2 (KPScript)",
                r#"KPScript.exe -c:GetEntryString "{Database}" -Field:Password -ref-Title:"{Title}" -FailIfNoEntry"#,
                true,
            ),
            ("Bitwarden CLI", r#"bw get password "{Title}""#, false),
            (
                "1Password CLI",
                r#"op item get "{Title}" --fields label=password --reveal"#,
                false,
            ),
            ("pass (GPG)", r#"pass show "{Title}""#, true),
        ];
        assert_eq!(PRESETS.len(), csharp.len());
        let words = |command: &str, drop_dashes: bool| {
            let mut words: Vec<String> = split(command)
                .expect("splits")
                .into_iter()
                .filter(|word| !(drop_dashes && word == "--"))
                .collect();
            words.sort();
            words
        };
        for (preset, (name, command, first_line)) in PRESETS.iter().zip(csharp) {
            assert_eq!(preset.name, name);
            assert_eq!(
                preset.first_line_only, first_line,
                "{name}: first line only"
            );
            assert_eq!(words(preset.command, true), words(command, false), "{name}");
        }
    }

    /// The argv tail from the first argument holding `needle`.
    fn position(line: &[String], needle: &str) -> Option<usize> {
        line.iter().position(|word| word.contains(needle))
    }

    #[test]
    fn a_title_starting_with_a_dash_never_becomes_a_flag_in_any_preset() {
        let flag = "--output=/tmp/x";
        for preset in PRESETS {
            match command_line(preset.command, &lookup(flag), &settings()) {
                Ok(line) => {
                    let at = position(&line, flag).expect(preset.name);
                    // Either after the `--` that ends the options, or inside a longer
                    // argument (`-ref-Title:...`), never an argument of its own.
                    let after_dashes = line[..at].iter().any(|word| word == "--");
                    let inside = !line[at].starts_with(flag);
                    assert!(after_dashes || inside, "{}: {line:?}", preset.name);
                }
                Err(problem) => assert_eq!(
                    problem,
                    TemplateProblem::OptionLikeValue("{Title}"),
                    "{}",
                    preset.name
                ),
            }
        }
    }

    #[test]
    fn each_preset_takes_a_dashed_title_the_way_its_tool_allows() {
        let by_name = |name: &str| PRESETS.iter().find(|p| p.name == name).expect(name).command;
        let dashed = lookup("-weird");
        for name in [
            "KeePassXC",
            "KeePassXC (key file)",
            "KeePassXC (key file only)",
            "Bitwarden CLI",
            "1Password CLI",
            "pass (GPG)",
        ] {
            let line = command_line(by_name(name), &dashed, &settings()).expect(name);
            let end_marker = line.iter().position(|word| word == "--").expect(name);
            assert_eq!(line.last().map(String::as_str), Some("-weird"), "{name}");
            assert!(end_marker < line.len() - 1, "{name}: {line:?}");
        }
        // KPScript: the title is inside `-ref-Title:`, a flag of its own already.
        let line = command_line(by_name("KeePass2 (KPScript)"), &dashed, &settings()).expect("ok");
        assert!(line.contains(&"-ref-Title:-weird".to_owned()), "{line:?}");
    }

    #[test]
    fn a_dashed_database_or_key_file_is_refused_where_no_dashes_end_the_options() {
        let by_name = |name: &str| PRESETS.iter().find(|p| p.name == name).expect(name).command;
        let dashed_database = ProviderSettings {
            database: "-db.kdbx".to_owned(),
            ..settings()
        };
        // KPScript's database is its first positional, with nothing to end the options.
        assert_eq!(
            command_line(
                by_name("KeePass2 (KPScript)"),
                &lookup("t"),
                &dashed_database
            ),
            Err(TemplateProblem::OptionLikeValue("{Database}"))
        );
        // keepassxc-cli: after its `--`.
        let line = command_line(by_name("KeePassXC"), &lookup("t"), &dashed_database).expect("ok");
        assert_eq!(line[line.len() - 2], "-db.kdbx");
        // The key file stands before the `--`, as the value of `-k`: refused.
        let dashed_key = ProviderSettings {
            key_file: "-k.keyx".to_owned(),
            ..settings()
        };
        assert_eq!(
            command_line(by_name("KeePassXC (key file)"), &lookup("t"), &dashed_key),
            Err(TemplateProblem::OptionLikeValue("{KeyFile}"))
        );
    }

    #[test]
    fn a_dash_inside_an_argument_or_a_value_that_is_not_first_is_no_flag() {
        let line = command_line("tool -u{User} --title={Title}", &lookup("-x"), &settings())
            .expect("runs");
        assert_eq!(line, ["tool", "-uadmin", "--title=-x"]);
        // A `--` the template wrote ends the options for the arguments after it only.
        assert_eq!(
            command_line("tool {Title} -- {Title}", &lookup("-x"), &settings()),
            Err(TemplateProblem::OptionLikeValue("{Title}"))
        );
        let line = command_line("tool -- {Title}", &lookup("-x"), &settings()).expect("runs");
        assert_eq!(line, ["tool", "--", "-x"]);
        // A value of "--" is a value, not the end of the options.
        assert_eq!(
            command_line("tool {Title} {Title}", &lookup("--"), &settings()),
            Err(TemplateProblem::OptionLikeValue("{Title}"))
        );
    }

    #[test]
    fn placeholders_are_filled_in_one_pass() {
        // An entry name that holds a placeholder is kept as it is, not expanded again.
        let tricky = lookup("{KeyFile}|{Database}|{HOST}");
        let line = command_line("tool {Title} {Database}", &tricky, &settings()).expect("runs");
        assert_eq!(
            line,
            [
                "tool",
                "{KeyFile}|{Database}|{HOST}",
                r"C:\Vaults\Team Vault.kdbx"
            ]
        );
        // A path that holds one too.
        let odd = ProviderSettings {
            database: "/v/{Title}.kdbx".to_owned(),
            ..settings()
        };
        let line = command_line("tool {Database}", &lookup("t"), &odd).expect("runs");
        assert_eq!(line, ["tool", "/v/{Title}.kdbx"]);
        // Several in one argument, whatever the case, and non-ASCII text around them.
        let line = command_line("tool \u{e9}{host}:{PORT}\u{e9}", &lookup("t"), &settings())
            .expect("runs");
        assert_eq!(line, ["tool", "\u{e9}web.lab:2222\u{e9}"]);
    }

    #[test]
    fn the_password_is_the_output_with_its_line_breaks_off_and_nothing_else() {
        assert_eq!(password_in("s3cret\r\n", false), Some("s3cret".to_owned()));
        // The C# rule: Trim('\r', '\n'), never a space or a tab.
        assert_eq!(
            password_in("\r\n\n  s3cret \t\r\n\r\n", false),
            Some("  s3cret \t".to_owned())
        );
        assert_eq!(
            password_in("s3cret\nnotes\n", false),
            Some("s3cret\nnotes".to_owned())
        );
        // Blanks only is nothing.
        assert_eq!(password_in("  \n \t \n", false), None);
        assert_eq!(password_in("\r\n", false), None);
        assert_eq!(password_in("", false), None);
    }

    #[test]
    fn the_first_line_keeps_its_spaces_and_refuses_status_lines() {
        assert_eq!(
            password_in("\n\n  pw with spaces \r\nnotes", true),
            Some("  pw with spaces ".to_owned())
        );
        // KPScript: the value, then the status line.
        assert_eq!(
            password_in("hunter2\nOK: Operation completed successfully.\n", true),
            Some("hunter2".to_owned())
        );
        // An empty field: a blank line, then the status line, which is no password.
        assert_eq!(password_in("\nOK: Operation completed.\n", true), None);
        assert_eq!(password_in("E: Entry not found.\n", true), None);
        // The prefixes are exact: no leading space, case matters.
        assert_eq!(password_in(" OK: pw\n", true), Some(" OK: pw".to_owned()));
        assert_eq!(password_in("ok: pw\n", true), Some("ok: pw".to_owned()));
        assert_eq!(password_in(" \n \n", true), None);
        // A status line is refused in the whole-output mode only by its content, as C#.
        assert_eq!(
            password_in("pw\nOK: done", false),
            Some("pw\nOK: done".to_owned())
        );
    }

    #[test]
    fn the_shape_of_a_value_is_told_without_the_value() {
        assert_eq!(
            value_shape("plain"),
            ValueShape {
                lines: 1,
                edge_space: false
            }
        );
        assert!(value_shape(" lead").edge_space);
        assert!(value_shape("trail ").edge_space);
        assert!(value_shape("tab\t").edge_space);
        assert!(!value_shape("in side").edge_space);
        assert_eq!(value_shape("a\nb\nc").lines, 3);
        assert_eq!(value_shape("").lines, 1);
        assert!(!value_shape("").edge_space);
    }

    #[test]
    fn the_test_lookup_stands_the_entry_in_for_the_host_and_title() {
        let lookup = Lookup::for_test("srv-01");
        assert_eq!(lookup.host, "srv-01");
        assert_eq!(lookup.title, "srv-01");
        assert_eq!(lookup.port, 22);
        assert_eq!(lookup.user, None);
    }

    #[test]
    fn a_program_is_named_by_its_file_only() {
        assert_eq!(program_file_name("keepassxc-cli"), "keepassxc-cli");
        assert_eq!(program_file_name(r"C:\Tools\KPScript.exe"), "KPScript.exe");
        assert_eq!(program_file_name("/usr/bin/pass"), "pass");
    }

    #[test]
    fn a_script_a_shell_reads_gets_its_values_in_single_quotes() {
        // The word after `sh -c`: the value is one word of the script.
        let line = command_line(
            r#"sh -c "pass show {Title}""#,
            &lookup("my server"),
            &settings(),
        )
        .expect("runs");
        assert_eq!(line, ["sh", "-c", "pass show 'my server'"]);
        let line = command_line(
            r#"bash -c "get {Title}""#,
            &lookup("a.b_c@d/e:f+g=h,i-j"),
            &settings(),
        )
        .expect("runs");
        assert_eq!(line[2], "get 'a.b_c@d/e:f+g=h,i-j'");
        // Unicode letters and digits are plain text.
        let line = command_line(
            r#"bash -c "get {Title}""#,
            &lookup("caf\u{e9} \u{4e2d}9"),
            &settings(),
        )
        .expect("runs");
        assert_eq!(line[2], "get 'caf\u{e9} \u{4e2d}9'");
        // PowerShell too, and behind a wrapper.
        let line = command_line(
            r#"pwsh -Command "Get-Secret {Title}""#,
            &lookup("a b"),
            &settings(),
        )
        .expect("runs");
        assert_eq!(line[2], "Get-Secret 'a b'");
        let line =
            command_line(r#"env sh -c "get {Title}""#, &lookup("a b"), &settings()).expect("runs");
        assert_eq!(line[3], "get 'a b'");
        // A quote the template opened is the template's.
        let line =
            command_line(r#"sh -c "echo '{Title}'""#, &lookup("a b"), &settings()).expect("runs");
        assert_eq!(line[2], "echo 'a b'");
    }

    #[test]
    fn every_class_of_character_a_shell_reads_is_refused_in_a_script() {
        let refused = [
            ("dollar", "a$b"),
            ("command substitution", "$(touch /tmp/x)"),
            ("backtick", "a`id`b"),
            ("double quote", "a\"b"),
            ("single quote", "it's"),
            ("semicolon", "a;b"),
            ("ampersand", "a&b"),
            ("pipe", "a|b"),
            ("less", "a<b"),
            ("greater", "a>b"),
            ("caret", "a^b"),
            ("percent", "50%"),
            ("bang", "a!b"),
            ("parenthesis", "a(b"),
            ("bracket", "a[b"),
            ("brace", "a{b}"),
            ("star", "a*b"),
            ("question", "a?b"),
            ("tilde", "~root"),
            ("hash", "a #b"),
            ("backslash", r"a\b"),
            ("tab", "a\tb"),
            ("control", "a\u{1}b"),
        ];
        for program in [
            "sh -c",
            "wsl -- sh -c",
            "env bash -c",
            "cmd /c",
            "pwsh -Command",
        ] {
            for (class, value) in refused {
                if class == "backslash" && matches!(program, "cmd /c" | "pwsh -Command") {
                    continue;
                }
                assert_eq!(
                    command_line(
                        &format!(r#"{program} "get {{Title}}""#),
                        &lookup(value),
                        &settings()
                    ),
                    Err(TemplateProblem::UnquotableValue("{Title}")),
                    "{program}: {class}"
                );
            }
        }
        // A line break is dropped before anything else.
        let line = command_line(
            r#"sh -c "get {Title}""#,
            &lookup(
                "a
b",
            ),
            &settings(),
        )
        .expect("runs");
        assert_eq!(line[2], "get 'ab'");
        // Outside a script the value is an argument of its own and is not judged.
        let line = command_line("tool {Title}", &lookup("a$b;c"), &settings()).expect("runs");
        assert_eq!(line[1], "a$b;c");
    }

    #[test]
    fn a_value_with_a_dash_word_is_refused_in_a_script() {
        for title in ["x -c", "x --flag", " -c", "a \u{2013}b", "a \u{2212}1"] {
            assert_eq!(
                command_line(
                    r#"sh -c "pass show -- {Title}""#,
                    &lookup(title),
                    &settings()
                ),
                Err(TemplateProblem::OptionLikeValue("{Title}")),
                "{title:?}"
            );
        }
        // A dash inside a word is plain text.
        let line = command_line(
            r#"sh -c "pass show {Title}""#,
            &lookup("web-01 a-b"),
            &settings(),
        )
        .expect("runs");
        assert_eq!(line[2], "pass show 'web-01 a-b'");
    }

    #[test]
    fn the_wsl_command_substitution_never_reaches_the_shell() {
        assert_eq!(
            command_line(
                r#"wsl -- sh -c "pass show {Title}""#,
                &lookup("$(touch /tmp/pwned)"),
                &settings()
            ),
            Err(TemplateProblem::UnquotableValue("{Title}"))
        );
        assert_eq!(
            command_line(
                r#"wsl sh -c "pass show {Title}""#,
                &lookup("`touch /tmp/pwned`"),
                &settings()
            ),
            Err(TemplateProblem::UnquotableValue("{Title}"))
        );
    }

    #[test]
    fn a_cmd_script_gets_no_quote_but_the_templates_own() {
        // An imported C# template: the value arrives as it is, one argument, spaces and all.
        let plain = ProviderSettings {
            database: "C:/Vaults/Team.kdbx".to_owned(),
            ..settings()
        };
        let line = command_line(
            r#"cmd /c keepassxc-cli show -s -q -a Password "{Database}" "{Title}""#,
            &lookup("my server"),
            &plain,
        )
        .expect("runs");
        assert_eq!(
            line,
            [
                "cmd",
                "/c",
                "keepassxc-cli",
                "show",
                "-s",
                "-q",
                "-a",
                "Password",
                "C:/Vaults/Team.kdbx",
                "my server"
            ]
        );
        // Within one word of the script, the template's quotes are kept as they are.
        let line =
            command_line(r#"cmd /c "get {Title}""#, &lookup("my server"), &plain).expect("runs");
        assert_eq!(line[2], "get my server");
        // Words of cmd that are not its script keep the old stripping.
        let line = command_line("cmd get {Title}", &lookup("a&b"), &settings()).expect("runs");
        assert_eq!(line[2], "ab");
    }

    #[test]
    fn a_windows_path_passes_in_cmd_and_powershell_scripts_only() {
        // C:\Vaults\Team Vault.kdbx is the test settings' database.
        let line = command_line(
            r#"cmd /c keepassxc-cli show -s -q -a Password "{Database}" "{Title}""#,
            &lookup("web"),
            &settings(),
        )
        .expect("runs");
        assert_eq!(line[8], r"C:\Vaults\Team Vault.kdbx");
        let keyed = ProviderSettings {
            key_file: r"C:\keys\team.keyx".to_owned(),
            ..settings()
        };
        let line = command_line(
            r#"pwsh -Command "Get-Secret {KeyFile}""#,
            &lookup("t"),
            &keyed,
        )
        .expect("runs");
        assert_eq!(line[2], r"Get-Secret 'C:\keys\team.keyx'");
        // POSIX, and anything behind a wrapper, keeps refusing it.
        for template in [
            r#"wsl -- sh -c "get {KeyFile}""#,
            r#"sh -c "get {KeyFile}""#,
            r#"env bash -c "get {KeyFile}""#,
        ] {
            assert_eq!(
                command_line(template, &lookup("t"), &keyed),
                Err(TemplateProblem::UnquotableValue("{KeyFile}")),
                "{template}"
            );
        }
        // A cmd value ending in a backslash would eat the template's closing quote.
        assert_eq!(
            command_line(r#"cmd /c get "{Title}""#, &lookup(r"dir\"), &settings()),
            Err(TemplateProblem::UnquotableValue("{Title}"))
        );
        // PowerShell holds it in single quotes: no such risk.
        assert!(command_line(r#"pwsh -c "get {Title}""#, &lookup(r"dir\"), &settings()).is_ok());
    }

    #[test]
    fn shell_flags_are_read_as_the_script_flag() {
        for flags in ["-c", "-lc", "-ec", "-fc", "-ic"] {
            let line = command_line(
                &format!(r#"bash {flags} "get {{Title}}""#),
                &lookup("a$b"),
                &settings(),
            );
            assert_eq!(
                line,
                Err(TemplateProblem::UnquotableValue("{Title}")),
                "{flags}"
            );
        }
        for shell in ["dash", "ksh", "zsh", "/bin/sh"] {
            assert_eq!(
                command_line(
                    &format!(r#"{shell} -c "get {{Title}}""#),
                    &lookup("a;b"),
                    &settings()
                ),
                Err(TemplateProblem::UnquotableValue("{Title}")),
                "{shell}"
            );
        }
        // A flag without `c` is no script, and a word that is no flag ends the flags.
        let line =
            command_line(r"bash -x script.sh {Title}", &lookup("a;b"), &settings()).expect("runs");
        assert_eq!(line[3], "ab", "not a script: stripped as before");
    }

    #[test]
    fn powershell_commands_are_found_by_prefix_or_implicitly() {
        for template in [
            r#"pwsh -c "Get-Secret {Title}""#,
            r#"pwsh -co "Get-Secret {Title}""#,
            r#"powershell.exe -com "Get-Secret {Title}""#,
            r#"pwsh -NoProfile -COMMAND "Get-Secret {Title}""#,
            r#"pwsh -NoProfile -ExecutionPolicy Bypass -Command "Get-Secret {Title}""#,
            // The implicit -Command: the first word that is no option.
            r#"pwsh -NoProfile "Get-Secret {Title}""#,
            r#"pwsh -ExecutionPolicy Bypass "Get-Secret {Title}""#,
            r"powershell Get-Secret {Title}",
        ] {
            assert_eq!(
                command_line(template, &lookup("a$b"), &settings()),
                Err(TemplateProblem::UnquotableValue("{Title}")),
                "{template}"
            );
        }
        // -File and -EncodedCommand name no script to put values in.
        for template in [
            r"pwsh -File script.ps1 {Title}",
            r"pwsh -f script.ps1 {Title}",
            r"pwsh -EncodedCommand {Title}",
        ] {
            assert!(
                command_line(template, &lookup("a b"), &settings()).is_ok(),
                "{template}"
            );
        }
    }

    #[test]
    fn only_a_dashes_right_after_the_wrapper_is_the_wrappers() {
        // `wsl --` ends wsl's options; `pass` still reads `-x` as a flag.
        for template in [
            r"wsl -- pass show {Title}",
            r"env -- pass show {Title}",
            r"sudo -- pass show {Title}",
        ] {
            assert_eq!(
                command_line(template, &lookup("-x"), &settings()),
                Err(TemplateProblem::OptionLikeValue("{Title}")),
                "{template}"
            );
        }
        // The program's own `--` ends its options.
        let line =
            command_line(r"wsl -- pass show -- {Title}", &lookup("-x"), &settings()).expect("runs");
        assert_eq!(line.last().map(String::as_str), Some("-x"));
        // Not a wrapper: any `--` ends the options.
        assert!(command_line(r"tool -- {Title}", &lookup("-x"), &settings()).is_ok());
        assert!(command_line(r"sh -- {Title}", &lookup("-x"), &settings()).is_ok());
        // In a script the inner `--` is read in the script.
        assert_eq!(
            command_line(
                r#"wsl -- sh -c "pass show {Title}""#,
                &lookup("-x"),
                &settings()
            ),
            Err(TemplateProblem::OptionLikeValue("{Title}"))
        );
        let line = command_line(
            r#"wsl -- sh -c "pass show -- {Title}""#,
            &lookup("-x"),
            &settings(),
        )
        .expect("runs");
        assert_eq!(line.last().map(String::as_str), Some("pass show -- '-x'"));
        // A quoted placeholder at the start of a word is the start of a word all the same.
        assert_eq!(
            command_line(r#"sh -c "pass show '{Title}'""#, &lookup("-x"), &settings()),
            Err(TemplateProblem::OptionLikeValue("{Title}"))
        );
    }

    #[test]
    fn a_unicode_dash_is_refused_like_the_ascii_one() {
        for dash in [
            '\u{2010}', '\u{2011}', '\u{2012}', '\u{2013}', '\u{2014}', '\u{2015}', '\u{2212}',
        ] {
            let title = format!("{dash}weird");
            assert_eq!(
                command_line("tool {Title}", &lookup(&title), &settings()),
                Err(TemplateProblem::OptionLikeValue("{Title}")),
                "U+{:04X}",
                u32::from(dash)
            );
            assert_eq!(
                command_line(r#"sh -c "get {Title}""#, &lookup(&title), &settings()),
                Err(TemplateProblem::OptionLikeValue("{Title}")),
                "script, U+{:04X}",
                u32::from(dash)
            );
            // Inside the argument, after `--`: no flag.
            assert!(command_line("tool -- {Title}", &lookup(&title), &settings()).is_ok());
        }
    }

    #[test]
    fn a_one_time_password_uri_is_never_a_password() {
        assert_eq!(password_in("otpauth://totp/x?secret=ABC\npw\n", true), None);
        assert_eq!(password_in("  OTPAUTH://totp/x?secret=ABC\n", true), None);
        // Only the first line decides, and only in this mode.
        assert_eq!(
            password_in("pw\notpauth://totp/x\n", true),
            Some("pw".to_owned())
        );
        assert_eq!(
            password_in("otpauth://totp/x", false),
            Some("otpauth://totp/x".to_owned())
        );
    }

    #[test]
    fn the_kind_is_read_by_its_csharp_name() {
        assert_eq!(
            ProviderKind::named("windowscredentialmanager"),
            ProviderKind::WindowsCredentialManager
        );
        assert_eq!(ProviderKind::named("Command"), ProviderKind::Command);
        assert_eq!(ProviderKind::named("other"), ProviderKind::Command);
    }
}
