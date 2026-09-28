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

/// The command templates offered, as the C# quick setup lists them: name, then template.
pub const PRESETS: [(&str, &str); 7] = [
    (
        "KeePassXC",
        r#"keepassxc-cli show -s -q -a Password "{Database}" "{Title}""#,
    ),
    (
        "KeePassXC (key file)",
        r#"keepassxc-cli show -s -q -k "{KeyFile}" -a Password "{Database}" "{Title}""#,
    ),
    (
        "KeePassXC (key file only)",
        r#"keepassxc-cli show -s -q --no-password -k "{KeyFile}" -a Password "{Database}" "{Title}""#,
    ),
    (
        "KeePass2 (KPScript)",
        r#"KPScript.exe -c:GetEntryString "{Database}" -Field:Password -ref-Title:"{Title}""#,
    ),
    ("Bitwarden CLI", r#"bw get password "{Title}""#),
    ("1Password CLI", r#"op read "op://{Title}/password""#),
    ("pass (GPG)", r#"pass show "{Title}""#),
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

/// Why a command template cannot run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TemplateProblem {
    /// Nothing to run.
    Empty,
    /// A quote is opened and never closed.
    UnclosedQuote,
    /// The template uses `{KeyFile}` and no key file is chosen.
    NoKeyFile,
}

/// The program and arguments `template` runs for `lookup`, placeholders filled.
///
/// # Errors
///
/// [`TemplateProblem`] when there is nothing to run, a quote is not closed, or a key file is
/// asked for and none is chosen.
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
    let shell = is_shell_target(program);
    let clean = |value: &str| {
        if shell {
            strip_for_shell(value)
        } else {
            // One argument whatever it holds; a line break is never a value's.
            value
                .chars()
                .filter(|c| !matches!(c, '\r' | '\n'))
                .collect()
        }
    };
    let port = lookup.port.to_string();
    let values = [
        ("{Host}", clean(&lookup.host)),
        ("{Port}", port),
        ("{User}", clean(lookup.user.as_deref().unwrap_or_default())),
        ("{Title}", clean(&lookup.title)),
        ("{Database}", clean(settings.database.trim())),
        ("{KeyFile}", clean(settings.key_file.trim())),
    ];
    Ok(words
        .iter()
        .enumerate()
        .map(|(index, word)| {
            // The program itself is the template's, never a value's.
            if index == 0 {
                return word.clone();
            }
            values.iter().fold(word.clone(), |word, (name, value)| {
                replace(&word, name, value)
            })
        })
        .collect())
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

/// `text` with every `name`, whatever its case, as C# replaces them, turned into `value`.
fn replace(text: &str, name: &str, value: &str) -> String {
    let lower = text.to_ascii_lowercase();
    let needle = name.to_ascii_lowercase();
    let mut out = String::with_capacity(text.len());
    let mut rest = 0;
    for (at, _) in lower.match_indices(&needle) {
        out.push_str(&text[rest..at]);
        out.push_str(value);
        rest = at + needle.len();
    }
    out.push_str(&text[rest..]);
    out
}

fn contains_placeholder(template: &str, name: &str) -> bool {
    template
        .to_ascii_lowercase()
        .contains(&name.to_ascii_lowercase())
}

/// Scripts and shells: what reads its arguments as a command line, as the C# list names
/// them.
const SHELL_EXTENSIONS: [&str; 9] = ["bat", "cmd", "ps1", "vbs", "js", "jse", "vbe", "wsf", "hta"];
const SHELL_NAMES: [&str; 10] = [
    "cmd",
    "powershell",
    "pwsh",
    "bash",
    "sh",
    "zsh",
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

/// The password in a command's `output`: the first line not empty when `first_line_only`,
/// else all of it, trimmed; `None` when nothing is left.
#[must_use]
pub fn password_in(output: &str, first_line_only: bool) -> Option<String> {
    let password = if first_line_only {
        output
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty())
            .unwrap_or_default()
    } else {
        output.trim()
    };
    (!password.is_empty()).then(|| password.to_owned())
}

/// The values the Test button asks with, as the C# one.
#[must_use]
pub fn test_lookup() -> Lookup {
    Lookup {
        host: "test.example.com".to_owned(),
        port: 22,
        user: Some("testuser".to_owned()),
        title: "TestEntry".to_owned(),
    }
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
    fn a_shell_loses_what_it_would_read() {
        for program in [
            "cmd",
            "cmd.exe",
            "powershell",
            "bash",
            "run.bat",
            r"C:\x\y.CMD. ",
        ] {
            let line = command_line(
                &format!("{program} /c get {{Title}}"),
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
        for (name, template) in PRESETS {
            let line = command_line(template, &lookup("Web server"), &settings())
                .unwrap_or_else(|problem| panic!("{name}: {problem:?}"));
            assert!(
                line.iter().any(|word| word.contains("Web server")),
                "{name}: {line:?}"
            );
            assert!(
                !line.iter().any(|word| word.contains('{')),
                "{name}: {line:?}"
            );
        }
    }

    #[test]
    fn the_password_is_the_output_trimmed_or_its_first_line() {
        assert_eq!(
            password_in("  s3cret \r\n", false),
            Some("s3cret".to_owned())
        );
        assert_eq!(
            password_in("\n  s3cret\nOK: done\n", true),
            Some("s3cret".to_owned())
        );
        assert_eq!(
            password_in("s3cret\nnotes", false),
            Some("s3cret\nnotes".to_owned())
        );
        assert_eq!(password_in(" \n \n", true), None);
        assert_eq!(password_in("", false), None);
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
