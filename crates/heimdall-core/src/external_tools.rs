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

//! The external tools the user lists, as the C# `ExternalToolDefinition`
//! (`ExternalToolDefinition.cs`): a name, a program, an argument template with the C#
//! placeholders, a working folder, and whether it runs as administrator or hidden.
//!
//! The C# fills the placeholders into the whole argument line and lets the program's C
//! runtime cut it into arguments afterwards, so a host holding a space became two arguments.
//! Here the template is cut first, by the C runtime's own rules, and each placeholder is
//! filled inside the argument it stands in: a value can never add an argument. The values
//! lose the characters the C# relaxed rule strips (`SanitizeRelaxed`), double quotes among
//! them, so what reaches the program is what the C# hands over when the placeholder is
//! quoted. A value that would start an argument with `-`, or `/` on Windows, is refused: the
//! program would read it as one of its options.
//!
//! There is no password placeholder, as in the C#: a secret on a command line is readable by
//! every process of the computer.

use serde::{Deserialize, Serialize};

pub use crate::credential_provider::is_shell_target;

/// The arguments a new tool starts with, as the C# `NewExternalToolArguments`.
pub const NEW_TOOL_ARGUMENTS: &str = "{Host}";

/// The characters a value loses, as the C# `SanitizeRelaxed` strips them: what chains or
/// redirects a command, the double quote the C runtime reads, and line breaks. NUL too,
/// which no command line can carry.
const STRIPPED: &[char] = &[';', '&', '|', '`', '$', '<', '>', '"', '\r', '\n', '\0'];

/// What starts an option: `-` everywhere, and `/` on Windows, where many programs read
/// their options after one (`mstsc /v:`, `WinSCP /script=`). A Unix path starts with `/`:
/// there it is no option.
#[cfg(windows)]
const OPTION_PREFIXES: &[char] = &['-', '/'];
#[cfg(not(windows))]
const OPTION_PREFIXES: &[char] = &['-'];

/// The double quote that groups an argument.
const QUOTE: char = '"';

/// The backslash that escapes a double quote.
const BACKSLASH: char = '\\';

/// A tool of the user's list, as the C# `ExternalToolDefinition`. Its `IconGlyph`, which no
/// C# editor sets and its Save drops, is not kept.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ExternalTool {
    /// Its name, unique in the list whatever the case, as the menus show it.
    pub name: String,
    /// The program: a full path, or a bare name found in the absolute folders of `PATH`.
    pub executable_path: String,
    /// The argument template, with the placeholders of [`Placeholder`].
    pub arguments: String,
    /// The folder it starts in; empty for Heimdall's own, as the C# inherits it.
    pub working_directory: String,
    /// Started through the Windows elevation prompt, as the C# `RunAsAdministrator`.
    pub run_as_administrator: bool,
    /// Started without a console window, as the C# `RunHidden`.
    pub run_hidden: bool,
}

impl ExternalTool {
    /// A new tool of the list, as the C# "Add" makes one: `name`, and the host as its
    /// arguments.
    #[must_use]
    pub fn new(name: String) -> Self {
        Self {
            name,
            arguments: NEW_TOOL_ARGUMENTS.to_owned(),
            ..Self::default()
        }
    }
}

/// A placeholder of the argument template, as the C# `SupportedPlaceholders`, in its order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Placeholder {
    /// `{Host}`: the server's address.
    Host,
    /// `{Port}`: its port, the protocol's own.
    Port,
    /// `{User}`: the account.
    User,
    /// `{ServerName}`: the session's name.
    ServerName,
    /// `{Protocol}`: its protocol, as the C# names it (`RDP`, `SSH`...).
    Protocol,
    /// `{KeyFile}`: the SSH key file of an SSH or SFTP session.
    KeyFile,
    /// `{Project}`: the C# project, which this version does not have: always empty.
    Project,
    /// `{Gateway}`: the name of the SSH gateway it goes through.
    Gateway,
}

impl Placeholder {
    /// Every placeholder, in the C# order.
    pub const ALL: [Self; 8] = [
        Self::Host,
        Self::Port,
        Self::User,
        Self::ServerName,
        Self::Protocol,
        Self::KeyFile,
        Self::Project,
        Self::Gateway,
    ];

    /// As it is written in a template, the C# token; matched whatever its case.
    #[must_use]
    pub const fn token(self) -> &'static str {
        match self {
            Self::Host => "{Host}",
            Self::Port => "{Port}",
            Self::User => "{User}",
            Self::ServerName => "{ServerName}",
            Self::Protocol => "{Protocol}",
            Self::KeyFile => "{KeyFile}",
            Self::Project => "{Project}",
            Self::Gateway => "{Gateway}",
        }
    }
}

/// The session a tool is started for, as the placeholders name it; empty text for what it
/// does not have.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ToolTarget {
    /// `{Host}`.
    pub host: String,
    /// `{Port}`; `None` for a session without a server, filled as empty text.
    pub port: Option<u16>,
    /// `{User}`.
    pub user: String,
    /// `{ServerName}`.
    pub server_name: String,
    /// `{Protocol}`.
    pub protocol: String,
    /// `{KeyFile}`.
    pub key_file: String,
    /// `{Project}`.
    pub project: String,
    /// `{Gateway}`.
    pub gateway: String,
}

impl ToolTarget {
    /// What `placeholder` stands for: the value, each character as [`ansi_form`] has it,
    /// without the characters [`STRIPPED`] names, as the C# relaxed rule strips them; the
    /// port as it is.
    fn value(&self, placeholder: Placeholder) -> String {
        let text = match placeholder {
            Placeholder::Port => return self.port.map(|port| port.to_string()).unwrap_or_default(),
            Placeholder::Host => &self.host,
            Placeholder::User => &self.user,
            Placeholder::ServerName => &self.server_name,
            Placeholder::Protocol => &self.protocol,
            Placeholder::KeyFile => &self.key_file,
            Placeholder::Project => &self.project,
            Placeholder::Gateway => &self.gateway,
        };
        text.chars()
            .map(ansi_form)
            .filter(|c| !STRIPPED.contains(c))
            .collect()
    }
}

/// `c` as a program reading an ANSI command line receives it, when that is a plain ASCII
/// character: Windows converts the command line to the system's code page for such a program
/// (`PuTTY`'s `WinMain`, most ports), and its best fit turns lookalikes, a fullwidth quote or
/// hyphen, an ideographic space, into the ASCII they resemble, after every check here. Taken
/// as that ASCII first, they are stripped, quoted or refused as the ASCII itself is. Any
/// other character stays as it is.
#[cfg(windows)]
fn ansi_form(c: char) -> char {
    use winsafe::co;
    if c.is_ascii() {
        return c;
    }
    let mut units = [0_u16; 2];
    let wide = c.encode_utf16(&mut units);
    match winsafe::WideCharToMultiByte(co::CP::ACP, co::WC::NoValue, wide, None, None) {
        Ok(bytes) => match bytes.as_slice() {
            [byte] if byte.is_ascii() && *byte != 0 => char::from(*byte),
            _ => c,
        },
        Err(_) => c,
    }
}

/// Elsewhere a program receives its arguments as they are.
#[cfg(not(windows))]
const fn ansi_form(c: char) -> char {
    c
}

/// Why the arguments of a tool are not built.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArgumentProblem {
    /// The value of this placeholder would start an argument with `-`, or `/` on Windows:
    /// the program would read it as an option of its own.
    OptionLike(Placeholder),
}

/// `template` cut into arguments as the Microsoft C runtime cuts a command line after the
/// program's name, the C# leaving that cut to the program: spaces and tabs part them outside
/// double quotes; `2n` backslashes before a double quote are `n` backslashes and the quote
/// opens or closes a quoted part, `2n + 1` are `n` and a literal quote; two double quotes
/// inside a quoted part are one literal quote; any other backslash is itself. A quote never
/// closed runs to the end, as the C runtime reads it.
#[must_use]
pub fn split_arguments(template: &str) -> Vec<String> {
    let chars: Vec<char> = template.chars().collect();
    let mut arguments = Vec::new();
    let mut at = 0;
    loop {
        while chars.get(at).is_some_and(|c| *c == ' ' || *c == '\t') {
            at += 1;
        }
        if at >= chars.len() {
            return arguments;
        }
        let mut argument = String::new();
        let mut quoted = false;
        loop {
            let mut backslashes = 0;
            while chars.get(at) == Some(&BACKSLASH) {
                backslashes += 1;
                at += 1;
            }
            match chars.get(at) {
                Some(&QUOTE) => {
                    argument.extend(std::iter::repeat_n(BACKSLASH, backslashes / 2));
                    if backslashes % 2 == 1 {
                        argument.push(QUOTE);
                        at += 1;
                    } else if quoted && chars.get(at + 1) == Some(&QUOTE) {
                        argument.push(QUOTE);
                        at += 2;
                    } else {
                        quoted = !quoted;
                        at += 1;
                    }
                }
                Some(&c) => {
                    argument.extend(std::iter::repeat_n(BACKSLASH, backslashes));
                    if !quoted && (c == ' ' || c == '\t') {
                        break;
                    }
                    argument.push(c);
                    at += 1;
                }
                None => {
                    argument.extend(std::iter::repeat_n(BACKSLASH, backslashes));
                    break;
                }
            }
        }
        arguments.push(argument);
    }
}

/// `word` with each placeholder, whatever its case, replaced by its value for `target`, in
/// one pass: a value holding a placeholder's token keeps it as text, where the C# chain of
/// `Replace` calls filled it again.
fn fill(word: &str, target: &ToolTarget) -> String {
    let lower = word.to_ascii_lowercase();
    let mut filled = String::with_capacity(word.len());
    let mut at = 0;
    'scan: while at < word.len() {
        for placeholder in Placeholder::ALL {
            let token = placeholder.token();
            if lower[at..].starts_with(&token.to_ascii_lowercase()) {
                filled.push_str(&target.value(placeholder));
                at += token.len();
                continue 'scan;
            }
        }
        let Some(c) = word[at..].chars().next() else {
            break;
        };
        filled.push(c);
        at += c.len_utf8();
    }
    filled
}

/// The first placeholder `word` holds, whatever its case.
fn first_placeholder(word: &str) -> Option<Placeholder> {
    let lower = word.to_ascii_lowercase();
    Placeholder::ALL
        .into_iter()
        .filter_map(|placeholder| {
            lower
                .find(&placeholder.token().to_ascii_lowercase())
                .map(|at| (at, placeholder))
        })
        .min_by_key(|(at, _)| *at)
        .map(|(_, placeholder)| placeholder)
}

/// The arguments `template` gives a tool started for `target`: cut by
/// [`split_arguments`], then each placeholder filled inside its own argument. With no
/// target, as the C# Test button and its palette with no session selected, the template's
/// arguments as written, the placeholders unfilled.
///
/// # Errors
///
/// [`ArgumentProblem::OptionLike`] when a value would start an argument with `-`, or `/` on
/// Windows, that the template did not start with one.
pub fn resolve_arguments(
    template: &str,
    target: Option<&ToolTarget>,
) -> Result<Vec<String>, ArgumentProblem> {
    let words = split_arguments(template);
    let Some(target) = target else {
        return Ok(words);
    };
    words
        .into_iter()
        .map(|word| {
            let filled = fill(&word, target);
            if filled.starts_with(OPTION_PREFIXES)
                && !word.starts_with(OPTION_PREFIXES)
                && let Some(placeholder) = first_placeholder(&word)
            {
                return Err(ArgumentProblem::OptionLike(placeholder));
            }
            Ok(filled)
        })
        .collect()
}

/// Why the list of tools is not saved, as the C# `ValidateExternalTools` refuses a Save.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolsProblem {
    /// A tool has no name or no program, as the C# `ValidationExtToolIncomplete`.
    Incomplete,
    /// Two tools have this name, whatever the case, as the C# `ValidationExtToolDuplicate`.
    Duplicate(String),
    /// The program of this tool is not found, as the C# `ValidationExtToolNotFound`.
    NotFound {
        /// The tool.
        name: String,
        /// Its program, as written.
        program: String,
    },
}

/// The first problem of `tools`, in the C# order, `found` saying whether a program written
/// is there; `None` when they can be saved.
#[must_use]
pub fn problem(tools: &[ExternalTool], found: impl Fn(&str) -> bool) -> Option<ToolsProblem> {
    let mut seen: Vec<String> = Vec::new();
    for tool in tools {
        let name = tool.name.trim();
        let program = tool.executable_path.trim();
        if name.is_empty() || program.is_empty() {
            return Some(ToolsProblem::Incomplete);
        }
        let folded = name.to_lowercase();
        if seen.contains(&folded) {
            return Some(ToolsProblem::Duplicate(name.to_owned()));
        }
        seen.push(folded);
        if !found(program) {
            return Some(ToolsProblem::NotFound {
                name: name.to_owned(),
                program: program.to_owned(),
            });
        }
    }
    None
}

/// `tools` as they are saved, as the C# Save trims them: the name and the program without
/// the spaces around them; the arguments and the folder as typed.
#[must_use]
pub fn saved(tools: &[ExternalTool]) -> Vec<ExternalTool> {
    tools
        .iter()
        .map(|tool| ExternalTool {
            name: tool.name.trim().to_owned(),
            executable_path: tool.executable_path.trim().to_owned(),
            ..tool.clone()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target() -> ToolTarget {
        ToolTarget {
            host: "web.lab".to_owned(),
            port: Some(2222),
            user: "admin".to_owned(),
            server_name: "Web (prod)".to_owned(),
            protocol: "SSH".to_owned(),
            key_file: r"C:\Keys\id ed25519".to_owned(),
            project: String::new(),
            gateway: "Bastion".to_owned(),
        }
    }

    fn args(template: &str, target: &ToolTarget) -> Vec<String> {
        resolve_arguments(template, Some(target)).expect("built")
    }

    /// Each line below is how the Microsoft C runtime reads it back, as documented in
    /// "Parsing C++ command-line arguments".
    #[test]
    fn the_template_is_cut_as_the_c_runtime_cuts_a_command_line() {
        assert_eq!(split_arguments(r#""a b c" d e"#), ["a b c", "d", "e"]);
        assert_eq!(split_arguments(r#""ab\"c" "\\" d"#), [r#"ab"c"#, r"\", "d"]);
        assert_eq!(
            split_arguments(r#"a\\\b d"e f"g h"#),
            [r"a\\\b", "de fg", "h"]
        );
        assert_eq!(split_arguments(r#"a\\\"b c d"#), [r#"a\"b"#, "c", "d"]);
        assert_eq!(split_arguments(r#"a\\\\"b c" d e"#), [r"a\\b c", "d", "e"]);
        assert_eq!(split_arguments(r#"a"b"" c d"#), [r#"ab" c d"#]);
        assert_eq!(split_arguments("  \t "), Vec::<String>::new());
        assert_eq!(split_arguments(r#"-a """#), ["-a", ""], "an empty one kept");
        assert_eq!(split_arguments(r#"open "unclosed"#), ["open", "unclosed"]);
    }

    #[test]
    fn each_placeholder_is_filled_inside_its_own_argument_whatever_its_case() {
        assert_eq!(
            args(
                r#"/v:{Host}:{PORT} -l {user} "{ServerName}" -i "{KeyFile}" {Protocol} via={gateway} [{Project}]"#,
                &target()
            ),
            [
                "/v:web.lab:2222",
                "-l",
                "admin",
                "Web (prod)",
                "-i",
                r"C:\Keys\id ed25519",
                "SSH",
                "via=Bastion",
                "[]",
            ]
        );
    }

    #[test]
    fn a_value_with_spaces_or_quotes_never_adds_an_argument() {
        let mut target = target();
        // Unquoted in the template: the C# split it in two, here it stays one.
        target.host = "web lab --evil".to_owned();
        assert_eq!(args("{Host}", &target), ["web lab --evil"]);
        // Quotes are stripped as the C# strips them: they cannot close the argument.
        target.host = r#"a" --delete "b"#.to_owned();
        assert_eq!(args(r#""{Host}""#, &target), ["a --delete b"]);
        target.user = "root\" & calc & \"".to_owned();
        assert_eq!(args("-l {User}", &target), ["-l", "root  calc  "]);
    }

    #[test]
    fn a_value_loses_what_chains_or_redirects_a_command_as_the_csharp_relaxed_rule() {
        let mut target = target();
        target.host = "h;o&s|t`$(x)<y>z\r\n\0".to_owned();
        assert_eq!(args("{Host}", &target), ["host(x)yz"]);
        // Kept, as the C# relaxed rule keeps them: parentheses, single quotes, percent.
        target.server_name = "O'Brien (100%)".to_owned();
        assert_eq!(args("{ServerName}", &target), ["O'Brien (100%)"]);
    }

    #[test]
    fn a_value_that_would_read_as_an_option_is_refused() {
        let mut target = target();
        target.host = "-oProxyCommand=calc".to_owned();
        assert_eq!(
            resolve_arguments("{Host}", Some(&target)),
            Err(ArgumentProblem::OptionLike(Placeholder::Host))
        );
        target.host = "web.lab".to_owned();
        target.user = "-oProxyCommand=calc".to_owned();
        assert_eq!(
            resolve_arguments("ssh {User}@{Host}", Some(&target)),
            Err(ArgumentProblem::OptionLike(Placeholder::User))
        );
        // Behind an option of the template, it is that option's value.
        assert_eq!(
            args("--login={User}", &target),
            ["--login=-oProxyCommand=calc"]
        );
    }

    #[test]
    fn a_value_that_would_read_as_a_windows_option_is_refused_there() {
        let mut target = target();
        target.host = "/script=evil.txt".to_owned();
        let built = resolve_arguments("{Host}", Some(&target));
        if cfg!(windows) {
            assert_eq!(built, Err(ArgumentProblem::OptionLike(Placeholder::Host)));
        } else {
            assert_eq!(
                built,
                Ok(vec!["/script=evil.txt".to_owned()]),
                "a Unix path"
            );
        }
        // The template's own option, filled.
        target.host = "web.lab".to_owned();
        assert_eq!(args("/v:{Host}", &target), ["/v:web.lab"]);
    }

    #[test]
    fn a_lookalike_the_ansi_code_page_reads_as_ascii_is_taken_as_that_ascii() {
        let mut target = target();
        // A fullwidth hyphen and quote, an ideographic space.
        for (lookalike, ascii) in [('\u{FF0D}', '-'), ('\u{FF02}', '"'), ('\u{3000}', ' ')] {
            if ansi_form(lookalike) != ascii {
                // This system's code page keeps no ASCII form of it: it reaches the program
                // as it is, and cannot be read as the ASCII.
                assert_eq!(ansi_form(lookalike), lookalike);
                continue;
            }
            target.host = format!("{lookalike}oProxyCommand=x");
            let built = resolve_arguments("{Host}", Some(&target));
            match ascii {
                '-' => assert_eq!(built, Err(ArgumentProblem::OptionLike(Placeholder::Host))),
                '"' => assert_eq!(built, Ok(vec!["oProxyCommand=x".to_owned()]), "stripped"),
                _ => assert_eq!(
                    built,
                    Ok(vec![" oProxyCommand=x".to_owned()]),
                    "a space, quoted by the runner"
                ),
            }
        }
        // A letter of the code page, or one outside it, is kept.
        target.server_name = "R\u{e9}seau \u{4e2d}".to_owned();
        assert_eq!(args("{ServerName}", &target), ["R\u{e9}seau \u{4e2d}"]);
    }

    #[test]
    fn a_value_holding_a_token_is_not_filled_again() {
        let mut target = target();
        target.host = "{User}".to_owned();
        assert_eq!(args("{Host}", &target), ["{User}"]);
    }

    #[test]
    fn with_no_session_the_template_is_cut_and_left_unfilled() {
        assert_eq!(
            resolve_arguments(r#"/v:{Host} "{ServerName}""#, None),
            Ok(vec!["/v:{Host}".to_owned(), "{ServerName}".to_owned()])
        );
    }

    #[test]
    fn a_session_without_a_server_fills_empty_text() {
        let target = ToolTarget {
            server_name: "Local".to_owned(),
            ..ToolTarget::default()
        };
        assert_eq!(args("{Host}:{Port} {ServerName}", &target), [":", "Local"]);
    }

    #[test]
    fn the_list_is_refused_as_the_csharp_save_refuses_it() {
        let tool = |name: &str, program: &str| ExternalTool {
            name: name.to_owned(),
            executable_path: program.to_owned(),
            ..ExternalTool::default()
        };
        let found = |program: &str| program != "missing.exe";
        assert_eq!(problem(&[], found), None);
        assert_eq!(
            problem(&[tool("WinSCP", "winscp.exe"), tool(" ", "x.exe")], found),
            Some(ToolsProblem::Incomplete)
        );
        assert_eq!(
            problem(&[tool("WinSCP", "")], found),
            Some(ToolsProblem::Incomplete)
        );
        assert_eq!(
            problem(
                &[tool("WinSCP", "winscp.exe"), tool(" winscp ", "other.exe")],
                found
            ),
            Some(ToolsProblem::Duplicate("winscp".to_owned()))
        );
        assert_eq!(
            problem(&[tool("Gone", " missing.exe ")], found),
            Some(ToolsProblem::NotFound {
                name: "Gone".to_owned(),
                program: "missing.exe".to_owned()
            })
        );
    }

    #[test]
    fn a_new_tool_takes_the_host_and_a_saved_one_is_trimmed() {
        let tool = ExternalTool::new("New Tool".to_owned());
        assert_eq!(tool.arguments, "{Host}");
        let saved = saved(&[ExternalTool {
            name: " WinSCP ".to_owned(),
            executable_path: " winscp.exe ".to_owned(),
            arguments: " {Host} ".to_owned(),
            ..ExternalTool::default()
        }]);
        assert_eq!(saved[0].name, "WinSCP");
        assert_eq!(saved[0].executable_path, "winscp.exe");
        assert_eq!(saved[0].arguments, " {Host} ", "as typed");
    }

    #[test]
    fn shells_and_scripts_are_known_as_the_csharp_knows_them() {
        for program in [
            "cmd.exe",
            r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe",
            "pwsh",
            "/bin/sh",
            "run.bat",
            "x.ps1",
            "mshta.exe",
            "cmd.exe. ",
        ] {
            assert!(is_shell_target(program), "{program}");
        }
        for program in ["winscp.exe", "/usr/bin/remmina", "putty"] {
            assert!(!is_shell_target(program), "{program}");
        }
    }
}
