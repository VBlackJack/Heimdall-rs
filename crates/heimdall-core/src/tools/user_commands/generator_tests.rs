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

//! The command generator's output shape and its shell safety, ported from the C#
//! `CommandGeneratorValueShapingTests` and `CommandGeneratorServiceTests`.

use super::quoting::{QuoteContext, placeholder_quote_contexts};
use super::*;

const WINDOWS: TemplatePlatform = TemplatePlatform::Windows;
const LINUX: TemplatePlatform = TemplatePlatform::Linux;
const BOTH: [TemplatePlatform; 2] = [LINUX, WINDOWS];

/// The characters both shells read as syntax outside quotes (the C# blacklist without the
/// line breaks, which are control characters).
const SHELL_METACHARACTERS: [&str; 9] = ["$", "`", ";", "|", "&", "(", ")", "<", ">"];

fn template(
    platform: TemplatePlatform,
    pattern: &str,
    parameters: Vec<UserParameter>,
) -> UserTemplate {
    UserTemplate {
        platform: match platform {
            TemplatePlatform::Windows => UserPlatform::Windows,
            TemplatePlatform::Linux => UserPlatform::Linux,
        },
        name: String::new(),
        command_pattern: pattern.to_owned(),
        parameters,
    }
}

fn parameter(name: &str, required: bool) -> UserParameter {
    let mut parameter = UserParameter::new(name);
    parameter.required = required;
    parameter
}

fn inline(name: &str) -> UserParameter {
    let mut parameter = parameter(name, true);
    parameter.quoting = Some(Quoting::InlineInQuotes);
    parameter
}

fn of_kind(name: &str, kind: &str) -> UserParameter {
    let mut parameter = parameter(name, true);
    parameter.kind = kind.to_owned();
    parameter
}

/// The command of `pattern` with `value` for `v` (and nothing for the rest).
fn render(
    platform: TemplatePlatform,
    pattern: &str,
    parameters: Vec<UserParameter>,
    values: &[(&str, &str)],
) -> Result<String, GenerateError> {
    let template = template(platform, pattern, parameters);
    generate(&template, platform, |name| {
        values
            .iter()
            .find(|(known, _)| *known == name)
            .map(|(_, value)| *value)
    })
}

fn render_one(
    platform: TemplatePlatform,
    pattern: &str,
    parameter: UserParameter,
    value: &str,
) -> Result<String, GenerateError> {
    let name = parameter.name.clone();
    render(
        platform,
        pattern,
        vec![parameter],
        &[(name.as_str(), value)],
    )
}

fn refused_dangerous(result: &Result<String, GenerateError>, label: &str) -> bool {
    matches!(
        result,
        Err(GenerateError::Values(errors)) if errors == &[ValueError::Dangerous(label.to_owned())]
    )
}

// ---- Length bounds ----------------------------------------------------------------------

#[test]
fn the_length_bounds_are_those_of_the_csharp_v2026_102101() {
    assert_eq!(MAX_PATTERN_LENGTH, 4000);
    assert_eq!(MAX_COMMAND_LENGTH, 4096);
    assert_eq!(MAX_PARAMETER_LENGTH, 1024);
}

#[test]
fn a_value_at_the_bound_passes_and_one_over_is_refused() {
    let at = "a".repeat(MAX_PARAMETER_LENGTH);
    let over = "a".repeat(MAX_PARAMETER_LENGTH + 1);
    for platform in BOTH {
        assert_eq!(
            render_one(platform, "x {v}", parameter("v", true), &at),
            Ok(format!("x {at}"))
        );
        assert_eq!(
            render_one(platform, "x {v}", parameter("v", true), &over),
            Err(GenerateError::Values(vec![ValueError::TooLong(
                "v".to_owned()
            )]))
        );
    }
}

#[test]
fn a_command_over_the_bound_is_refused_even_when_pattern_and_value_each_fit() {
    let pattern = format!(
        "{}{{v}}{}",
        "a".repeat(MAX_COMMAND_LENGTH - MAX_PARAMETER_LENGTH),
        "b".repeat(100)
    );
    let value = "c".repeat(MAX_PARAMETER_LENGTH);
    assert_eq!(
        render_one(LINUX, &pattern, parameter("v", true), &value),
        Err(GenerateError::TooLong)
    );
    let exact = "a".repeat(MAX_COMMAND_LENGTH);
    assert_eq!(
        render(LINUX, &exact, Vec::new(), &[]).as_deref(),
        Ok(exact.as_str())
    );
    let over = "a".repeat(MAX_COMMAND_LENGTH + 1);
    assert_eq!(
        render(LINUX, &over, Vec::new(), &[]),
        Err(GenerateError::TooLong)
    );
}

#[test]
fn a_path_longer_than_the_windows_bound_is_refused_as_a_path() {
    let over = format!("C:\\{}", "a".repeat(MAX_PATH_LENGTH));
    for platform in BOTH {
        assert_eq!(
            render_one(platform, "ls {p}", of_kind("p", "path"), &over),
            Err(GenerateError::Values(vec![ValueError::Path(
                "p".to_owned()
            )]))
        );
    }
}

#[test]
fn a_pattern_of_the_new_bound_can_be_saved_and_one_over_cannot() {
    let mut library = UserLibrary::default();
    let pad = |length: usize| format!("echo {}", "a".repeat(length - "echo ".len()));
    let at = Draft {
        title: "t".to_owned(),
        category: "c".to_owned(),
        platform: UserPlatform::Linux,
        linux_pattern: pad(MAX_PATTERN_LENGTH),
        ..Draft::default()
    };
    assert!(library.upsert(&at).is_ok());
    let over = Draft {
        linux_pattern: pad(MAX_PATTERN_LENGTH + 1),
        ..at
    };
    assert_eq!(library.upsert(&over), Err(DraftError::PatternTooLong));
}

// ---- An optional value left empty is left out ---------------------------------------------

#[test]
fn an_empty_optional_value_leaves_the_argument_out() {
    for platform in BOTH {
        assert_eq!(
            render_one(platform, "hostname {name}", parameter("name", false), "").as_deref(),
            Ok("hostname")
        );
    }
}

#[test]
fn an_empty_optional_value_between_words_collapses_the_spaces() {
    for empty in ["", "   "] {
        for platform in BOTH {
            assert_eq!(
                render_one(platform, "a {x} b", parameter("x", false), empty).as_deref(),
                Ok("a b")
            );
        }
    }
}

#[test]
fn an_empty_optional_value_leaves_the_authors_own_spaces_alone() {
    assert_eq!(
        render_one(LINUX, "grep 'a  b' {file}", parameter("file", false), "").as_deref(),
        Ok("grep 'a  b'")
    );
}

#[test]
fn two_empty_optional_values_in_a_row_leave_no_trailing_space() {
    assert_eq!(
        render(
            LINUX,
            "a {x} {y}",
            vec![parameter("x", false), parameter("y", false)],
            &[]
        )
        .as_deref(),
        Ok("a")
    );
}

#[test]
fn an_empty_required_value_is_still_an_error() {
    for blank in ["", " "] {
        assert_eq!(
            render_one(LINUX, "ping {host}", parameter("host", true), blank),
            Err(GenerateError::Values(vec![ValueError::Required(
                "host".to_owned()
            )]))
        );
    }
}

#[test]
fn an_empty_optional_value_takes_its_default_before_being_left_out() {
    let mut level = parameter("level", false);
    level.default_value = Some("info".to_owned());
    assert_eq!(
        render_one(LINUX, "log {level}", level, "").as_deref(),
        Ok("log info")
    );
}

// ---- Values with nothing the shell could read go in bare ------------------------------------

#[test]
fn a_safe_value_arrives_bare_on_both_platforms() {
    for value in [
        "8080",
        "/var/log/syslog",
        "id_ed25519",
        "user@host:/srv/data",
    ] {
        for platform in BOTH {
            assert_eq!(
                render_one(platform, "run {v}", parameter("v", true), value),
                Ok(format!("run {value}")),
                "{value} holds nothing {platform:?} reads specially"
            );
        }
    }
}

#[test]
fn safe_values_around_literal_text_stay_one_token() {
    let parameters = vec![
        parameter("port", true),
        parameter("protocol", true),
        parameter("key", true),
    ];
    assert_eq!(
        render(
            LINUX,
            "firewall-cmd --add-port={port}/{protocol} && cat ~/.ssh/{key}.pub",
            parameters,
            &[("port", "8080"), ("protocol", "tcp"), ("key", "id_ed25519")]
        )
        .as_deref(),
        Ok("firewall-cmd --add-port=8080/tcp && cat ~/.ssh/id_ed25519.pub")
    );
}

#[test]
fn a_value_the_shell_would_read_stays_quoted() {
    let rows = [
        (LINUX, "a b", "'a b'"),
        (LINUX, "x;y", "'x;y'"),
        (LINUX, "it's", "'it'\\''s'"),
        (LINUX, "$HOME", "'$HOME'"),
        (LINUX, "~/backup", "'~/backup'"),
        (LINUX, "C:\\inetpub", "'C:\\inetpub'"),
        (LINUX, "*.log", "'*.log'"),
        (WINDOWS, "a b", "'a b'"),
        (WINDOWS, "x;y", "'x;y'"),
        (WINDOWS, "it's", "'it''s'"),
        (WINDOWS, "$env:TEMP", "'$env:TEMP'"),
        (
            WINDOWS,
            "C:\\Program Files (x86)\\App",
            "'C:\\Program Files (x86)\\App'",
        ),
    ];
    for (platform, value, expected) in rows {
        assert_eq!(
            render_one(platform, "run {v}", parameter("v", true), value),
            Ok(format!("run {expected}")),
            "{platform:?} {value}"
        );
    }
}

#[test]
fn a_value_powershell_reads_unquoted_stays_quoted_on_windows_only() {
    for value in ["-Force", "@args", "a,b"] {
        assert_eq!(
            render_one(WINDOWS, "run {v}", parameter("v", true), value),
            Ok(format!("run '{value}'"))
        );
        assert_eq!(
            render_one(LINUX, "run {v}", parameter("v", true), value),
            Ok(format!("run {value}"))
        );
    }
}

#[test]
fn a_safe_value_keeps_its_quotes_where_the_shell_expands() {
    let rows = [
        (LINUX, "echo ${x}", "echo $'abc'"),
        (WINDOWS, "echo ${x}", "echo $'abc'"),
        (LINUX, "echo \"{x}\"", "echo \"'abc'\""),
        (LINUX, "echo {x} ${x}", "echo 'abc' $'abc'"),
        (LINUX, "echo $'{x}'", "echo $''abc''"),
        (WINDOWS, "echo \"{x}\"", "echo \"'abc'\""),
    ];
    for (platform, pattern, expected) in rows {
        assert_eq!(
            render_one(platform, pattern, parameter("x", true), "abc").as_deref(),
            Ok(expected),
            "{platform:?} {pattern}"
        );
    }
}

#[test]
fn an_inline_safe_value_keeps_only_the_patterns_quotes() {
    assert_eq!(
        render_one(LINUX, "grep '{v}' file", inline("v"), "8080").as_deref(),
        Ok("grep '8080' file")
    );
}

#[test]
fn a_path_is_judged_by_its_shape_and_comes_out_bare_or_quoted_alike_on_both_platforms() {
    let rows = [
        ("/var/log/syslog", false),
        ("/etc/nginx/nginx.conf", false),
        ("relative/dir", false),
        ("C:\\inetpub\\wwwroot", true),
        ("C:\\Program Files (x86)\\App", true),
        ("~/backup", true),
    ];
    for (value, quoted) in rows {
        let argument = if quoted {
            format!("'{value}'")
        } else {
            value.to_owned()
        };
        for platform in BOTH {
            assert_eq!(
                render_one(platform, "tail -n 50 {p}", of_kind("p", "path"), value),
                Ok(format!("tail -n 50 {argument}")),
                "{platform:?} {value}"
            );
        }
    }
}

#[test]
fn a_blank_or_control_path_is_refused() {
    for value in [
        "   ",
        "/var/log/\0syslog",
        "/var/log/syslog\nrm -rf /",
        "C:\\x\r",
    ] {
        assert!(
            render_one(LINUX, "ls {p}", of_kind("p", "path"), value).is_err(),
            "{value:?}"
        );
    }
}

// ---- The unknown type, the single-quote relaxation and the refusals ------------------------

#[test]
fn a_parameter_of_unknown_type_is_still_checked_for_control_characters() {
    for value in ["a\nb", "a\rb", "a\0b"] {
        assert_eq!(
            render_one(LINUX, "echo {v}", of_kind("v", "text"), value),
            Err(GenerateError::Values(vec![ValueError::Control(
                "v".to_owned()
            )])),
            "{value:?}"
        );
    }
}

#[test]
fn a_parameter_of_unknown_type_is_quoted_like_a_string_and_gets_the_blacklist() {
    assert_eq!(
        render_one(LINUX, "echo {v}", of_kind("v", "text"), "a b").as_deref(),
        Ok("echo 'a b'")
    );
    // Bare placement: the quotes protect, so a metacharacter is data.
    assert_eq!(
        render_one(LINUX, "echo {v}", of_kind("v", "text"), "a;b").as_deref(),
        Ok("echo 'a;b'")
    );
    // Double-quoted placement: the quotes do not protect.
    assert!(refused_dangerous(
        &render_one(LINUX, "echo \"{v}\"", of_kind("v", "text"), "$(id)"),
        "v"
    ));
}

#[test]
fn a_shell_character_in_a_string_arrives_literally_inside_the_quotes() {
    for value in [
        "a&b",
        "a|b",
        "a;b",
        "a`b",
        "a$b",
        "a(b",
        "a)b",
        "a<b",
        "a>b",
        "(objectClass=user)",
        "error|warn",
        "$env:TEMP",
        "-Confirm:$false",
    ] {
        assert_eq!(
            render_one(WINDOWS, "Write-Output {v}", parameter("v", true), value),
            Ok(format!("Write-Output '{value}'"))
        );
        assert_eq!(
            render_one(LINUX, "printf %s {v}", parameter("v", true), value),
            Ok(format!("printf %s '{value}'"))
        );
    }
}

#[test]
fn a_value_closing_the_quote_stays_inside_the_quotes() {
    assert_eq!(
        render_one(LINUX, "printf %s {v}", parameter("v", true), "x'; rm -rf /").as_deref(),
        Ok("printf %s 'x'\\''; rm -rf /'")
    );
    assert_eq!(
        render_one(
            WINDOWS,
            "Write-Output {v}",
            parameter("v", true),
            "x'; rm -rf /"
        )
        .as_deref(),
        Ok("Write-Output 'x''; rm -rf /'")
    );
}

#[test]
fn a_typographic_quote_is_doubled_on_windows_and_left_alone_on_linux() {
    for quote in ['\u{2018}', '\u{2019}', '\u{201A}', '\u{201B}'] {
        let value = format!("a{quote}; Remove-Item x; {quote}b");
        let windows = render_one(WINDOWS, "echo {v}", parameter("v", true), &value);
        assert_eq!(
            windows,
            Ok(format!(
                "echo 'a{quote}{quote}; Remove-Item x; {quote}{quote}b'"
            ))
        );
        let linux = render_one(LINUX, "echo {v}", parameter("v", true), &value);
        assert_eq!(linux, Ok(format!("echo '{value}'")));
    }
}

#[test]
fn an_inline_value_inside_single_quotes_may_hold_shell_characters() {
    // The LDAP filter of the C# test: `&`, `(` and `)` are literal between single quotes.
    assert_eq!(
        render_one(
            WINDOWS,
            "Get-ADObject -LDAPFilter '{filter}'",
            inline("filter"),
            "(&(objectClass=user)(cn=O'Brien))"
        )
        .as_deref(),
        Ok("Get-ADObject -LDAPFilter '(&(objectClass=user)(cn=O''Brien))'")
    );
    for (platform, pattern, expected) in [
        (LINUX, "grep -E '{v}' f", "grep -E 'a|b $x `y` ;z' f"),
        (
            WINDOWS,
            "Select-String '{v}' f",
            "Select-String 'a|b $x `y` ;z' f",
        ),
    ] {
        assert_eq!(
            render_one(platform, pattern, inline("v"), "a|b $x `y` ;z").as_deref(),
            Ok(expected)
        );
    }
}

#[test]
fn an_inline_value_that_tries_to_close_the_pattern_quotes_cannot() {
    assert_eq!(
        render_one(LINUX, "echo '{v}'", inline("v"), "x'; rm -rf /; echo '").as_deref(),
        Ok("echo 'x'\\''; rm -rf /; echo '\\'''")
    );
    assert_eq!(
        render_one(WINDOWS, "echo '{v}'", inline("v"), "x'; rm -rf /; echo '").as_deref(),
        Ok("echo 'x''; rm -rf /; echo '''")
    );
    for quote in ['\u{2018}', '\u{2019}', '\u{201A}', '\u{201B}'] {
        let value = format!("x{quote}; Remove-Item y; {quote}");
        assert_eq!(
            render_one(WINDOWS, "echo '{v}'", inline("v"), &value),
            Ok(format!(
                "echo 'x{quote}{quote}; Remove-Item y; {quote}{quote}'"
            ))
        );
    }
}

#[test]
fn an_inline_value_where_the_quotes_do_not_protect_still_refuses_shell_characters() {
    let cases = [
        // Not inside any quote.
        (WINDOWS, "Write-Output {v}"),
        (LINUX, "echo {v}"),
        // Between single quotes inside a double-quoted string: the shell expands there.
        (WINDOWS, "Get-ADUser -Filter \"Name -like '*{v}*'\""),
        (LINUX, "echo \"it '{v}'\""),
        // Inside a bash ANSI-C string, which reads backslash escapes.
        (LINUX, "echo $'{v}'"),
        // One occurrence protected, one not: the generator replaces both.
        (LINUX, "echo '{v}' {v}"),
    ];
    for (platform, pattern) in cases {
        for value in ["foo;bar", "foo$(bar)", "a`b`", "a|b", "a&b"] {
            let result = render_one(platform, pattern, inline("v"), value);
            assert!(
                refused_dangerous(&result, "v"),
                "{platform:?} {pattern} {value}: {result:?}"
            );
        }
    }
}

#[test]
fn a_shell_quote_value_inside_double_quotes_still_refuses_a_subexpression() {
    for platform in BOTH {
        let result = render_one(
            platform,
            "Write-Host \"Done: {v}\"",
            parameter("v", true),
            "$(whoami)",
        );
        assert!(refused_dangerous(&result, "v"), "{platform:?}");
    }
}

#[test]
fn a_value_after_a_dollar_brace_keeps_the_blacklist() {
    for platform in BOTH {
        let result = render_one(platform, "echo ${msg}", parameter("msg", true), "a;b");
        assert!(refused_dangerous(&result, "msg"), "{platform:?}");
    }
}

#[test]
fn a_line_break_is_refused_in_every_quoting_mode_and_placement() {
    let cases = [
        ("echo {v}", parameter("v", true)),
        ("echo '{v}'", inline("v")),
        ("echo {v}", inline("v")),
        ("echo \"{v}\"", parameter("v", true)),
    ];
    for newline in ["\n", "\r", "\r\n", "\u{85}", "\u{1b}"] {
        for platform in BOTH {
            for (pattern, parameter) in &cases {
                let value = format!("a{newline}b");
                assert_eq!(
                    render_one(platform, pattern, parameter.clone(), &value),
                    Err(GenerateError::Values(vec![ValueError::Control(
                        "v".to_owned()
                    )])),
                    "{platform:?} {pattern} {value:?}"
                );
            }
        }
    }
}

// ---- A value cannot break out whatever the platform ------------------------------------------

/// The words of `text` read as bash reads a command line, single quotes and the backslash
/// escape only; `None` when an unquoted character would make the shell interpret something.
fn posix_words(text: &str) -> Option<Vec<String>> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut started = false;
    let mut quoted = false;
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if quoted {
            if c == '\'' {
                quoted = false;
            } else {
                current.push(c);
            }
        } else if c == '\'' {
            quoted = true;
            started = true;
        } else if c == '\\' {
            current.push(chars.next()?);
            started = true;
        } else if c == ' ' {
            if started {
                words.push(std::mem::take(&mut current));
                started = false;
            }
        } else if c.is_ascii_alphanumeric() || "._/:=@+,%-".contains(c) {
            current.push(c);
            started = true;
        } else {
            return None;
        }
    }
    if quoted {
        return None;
    }
    if started {
        words.push(current);
    }
    Some(words)
}

/// The words of `text` read as PowerShell reads a command line with single-quoted strings
/// only; `None` when an unquoted character would be interpreted.
fn powershell_words(text: &str) -> Option<Vec<String>> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut started = false;
    let mut quoted = false;
    let chars: Vec<char> = text.chars().collect();
    let mut index = 0;
    while index < chars.len() {
        let c = chars[index];
        let is_quote = POWERSHELL_SINGLE_QUOTES.contains(&c);
        if quoted {
            if is_quote {
                if chars.get(index + 1) == Some(&c) {
                    current.push(c);
                    index += 1;
                } else {
                    quoted = false;
                }
            } else {
                current.push(c);
            }
        } else if is_quote {
            quoted = true;
            started = true;
        } else if c == ' ' {
            if started {
                words.push(std::mem::take(&mut current));
                started = false;
            }
        } else if c.is_ascii_alphanumeric() || "._/:=%+@-".contains(c) {
            current.push(c);
            started = true;
        } else {
            return None;
        }
        index += 1;
    }
    if quoted {
        return None;
    }
    if started {
        words.push(current);
    }
    Some(words)
}

#[test]
fn every_value_is_either_refused_or_exactly_one_literal_argument() {
    let values = [
        "plain",
        "with space",
        "it's",
        "x'; rm -rf /",
        "x'$(id)'",
        "$(id)",
        "${IFS}",
        "`id`",
        "a;b",
        "a|b",
        "a&b",
        "a && b",
        "a > /etc/passwd",
        "a < /etc/passwd",
        "(subshell)",
        "\"double\"",
        "\\",
        "\\'",
        "''",
        "'",
        "*",
        "~",
        "%PATH%",
        "-rf",
        "a\u{2018}b",
        "a\u{2019}; Remove-Item x; \u{2018}",
        "a\nb",
        "a\rb",
        "\0",
    ];
    for value in values {
        for platform in BOTH {
            let Ok(command) = render_one(platform, "run {v}", parameter("v", true), value) else {
                continue;
            };
            let words = match platform {
                LINUX => posix_words(&command),
                WINDOWS => powershell_words(&command),
            }
            .unwrap_or_else(|| panic!("{platform:?} {value:?} left syntax bare: {command}"));
            assert_eq!(
                words,
                ["run".to_owned(), value.to_owned()],
                "{platform:?} {value:?} became {command}"
            );
        }
    }
}

#[test]
fn a_value_holding_a_metacharacter_is_refused_or_quoted_never_bare() {
    for character in SHELL_METACHARACTERS {
        let value = format!("a{character}b");
        for platform in BOTH {
            let command = render_one(platform, "run {v}", parameter("v", true), &value)
                .expect("quoted whole, the character is data");
            assert_eq!(command, format!("run '{value}'"), "{platform:?}");
            // Where the quotes do not protect it, it is refused.
            let refused = render_one(platform, "run \"{v}\"", parameter("v", true), &value);
            assert!(refused_dangerous(&refused, "v"), "{platform:?} {character}");
        }
    }
}

#[test]
fn a_value_cannot_borrow_another_parameters_placeholder() {
    // Replaced one after another, the value of `a` would be read as the placeholder `{b}`
    // and the value of `b` would land inside the quotes of `a`.
    for platform in BOTH {
        let command = render(
            platform,
            "run {a} {b}",
            vec![parameter("a", true), parameter("b", true)],
            &[("a", "{b}"), ("b", "x; id")],
        )
        .expect("both are data");
        assert_eq!(command, "run '{b}' 'x; id'", "{platform:?}");
    }
}

#[test]
fn a_hostname_or_number_cannot_carry_syntax() {
    for platform in BOTH {
        for (kind, value) in [
            ("hostname", "a;b"),
            ("hostname", "a b"),
            ("hostname", "$(id)"),
            ("int", "1;2"),
            ("int", "1 2"),
            ("number", "1;2"),
            ("ipaddress", "1.2.3.4;id"),
            ("driveletter", "C;"),
        ] {
            assert!(
                render_one(platform, "run {v}", of_kind("v", kind), value).is_err(),
                "{kind} {value}"
            );
        }
    }
}

// ---- The placeholder lexer --------------------------------------------------------------------

#[test]
fn the_lexer_classes_every_occurrence_in_order() {
    use QuoteContext::{Bare, Expanding, SingleQuoted};
    let rows: [(&str, bool, &[QuoteContext]); 12] = [
        ("echo {x}", false, &[Bare]),
        ("echo '{x}'", false, &[SingleQuoted]),
        ("echo \"{x}\"", false, &[Expanding]),
        ("echo \"a '{x}'\"", false, &[Expanding]),
        ("echo $'{x}'", false, &[Expanding]),
        ("echo $'a\\'{x}'", false, &[Expanding]),
        ("echo ${x}", false, &[Expanding]),
        ("echo ${x}", true, &[Expanding]),
        ("echo \\${x}", false, &[Bare]),
        (
            "echo '{x}' {x} \"{x}\"",
            false,
            &[SingleQuoted, Bare, Expanding],
        ),
        ("echo {y}", false, &[]),
        ("{x} arg", false, &[Bare]),
    ];
    for (pattern, powershell, expected) in rows {
        assert_eq!(
            placeholder_quote_contexts(pattern, "x", powershell),
            expected,
            "{pattern} powershell={powershell}"
        );
    }
}

#[test]
fn the_lexer_follows_the_escape_and_quote_rules_of_each_shell() {
    use QuoteContext::{Bare, Expanding, SingleQuoted};
    // Bash escapes with a backslash, PowerShell with a backtick.
    assert_eq!(
        placeholder_quote_contexts("echo \\'{x}", "x", false),
        [Bare]
    );
    assert_eq!(placeholder_quote_contexts("echo `'{x}", "x", true), [Bare]);
    assert_eq!(
        placeholder_quote_contexts("echo \"a\\\"{x}\"", "x", false),
        [Expanding]
    );
    assert_eq!(
        placeholder_quote_contexts("echo \"a`\"{x}\"", "x", true),
        [Expanding]
    );
    // A typographic single quote opens and closes a literal in PowerShell, not in bash.
    assert_eq!(
        placeholder_quote_contexts("echo \u{2018}{x}\u{2019}", "x", true),
        [SingleQuoted]
    );
    assert_eq!(
        placeholder_quote_contexts("echo \u{2018}{x}\u{2019}", "x", false),
        [Bare]
    );
    // And a typographic double quote opens an expanding string in PowerShell only.
    assert_eq!(
        placeholder_quote_contexts("echo \u{201C}{x}\u{201D}", "x", true),
        [Expanding]
    );
    assert_eq!(
        placeholder_quote_contexts("echo \u{201C}{x}\u{201D}", "x", false),
        [Bare]
    );
}

#[test]
fn a_placeholder_only_in_single_quoted_spans_is_the_precondition_of_inline_quoting() {
    for platform in BOTH {
        assert!(placeholder_in_single_quotes("echo '{x}'", "x", platform));
        assert!(placeholder_in_single_quotes(
            "echo '{x}' '{x}'",
            "x",
            platform
        ));
        assert!(!placeholder_in_single_quotes(
            "echo '{x}' {x}",
            "x",
            platform
        ));
        assert!(!placeholder_in_single_quotes("echo {x}", "x", platform));
        assert!(!placeholder_in_single_quotes("echo 'a'", "x", platform));
        assert!(!placeholder_in_single_quotes(
            "-Filter \"Name -like '*{x}*'\"",
            "x",
            platform
        ));
    }
}

#[test]
fn an_inline_quoting_whose_placeholder_the_shell_expands_is_reset_on_import() {
    let mut library = UserLibrary::default();
    let draft = Draft {
        title: "t".to_owned(),
        category: "c".to_owned(),
        platform: UserPlatform::Windows,
        windows_pattern: "Get-ADUser -Filter \"Name -like '*{x}*'\" -Path '{y}'".to_owned(),
        ..Draft::default()
    };
    let id = library.upsert(&draft).expect("made");
    let mut command = library.get(&id).expect("kept").clone();
    let template = command
        .windows_command_template
        .as_mut()
        .expect("windows template");
    for parameter in &mut template.parameters {
        parameter.quoting = Some(Quoting::InlineInQuotes);
    }
    normalize_quoting(&mut command);
    let parameters = &command
        .windows_command_template
        .as_ref()
        .expect("windows template")
        .parameters;
    let quoting_of = |name: &str| {
        parameters
            .iter()
            .find(|parameter| parameter.name == name)
            .and_then(|parameter| parameter.quoting)
    };
    // Inside a double-quoted string the single quotes protect nothing.
    assert_eq!(quoting_of("x"), Some(Quoting::ShellQuote));
    assert_eq!(quoting_of("y"), Some(Quoting::InlineInQuotes));
}
