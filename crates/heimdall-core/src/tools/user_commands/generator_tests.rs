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
    for value in ["8080", "var/log/syslog", "id_ed25519", "user@host:srv/data"] {
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
        (LINUX, "it's", "'it'\\''s'"),
        (LINUX, "~/backup", "'~/backup'"),
        (LINUX, "C:\\inetpub", "'C:\\inetpub'"),
        (LINUX, "*.log", "'*.log'"),
        (WINDOWS, "a b", "'a b'"),
        (WINDOWS, "it's", "'it''s'"),
        (
            WINDOWS,
            "C:\\Program Files\\App",
            "'C:\\Program Files\\App'",
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
    for value in ["@args", "a,b"] {
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
        ("My Documents/App", true),
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
    // Bare placement: stricter than the C#, a metacharacter is refused even though the
    // quotes would carry it (a second-stage shell reads the value again).
    assert!(refused_dangerous(
        &render_one(LINUX, "echo {v}", of_kind("v", "text"), "a;b"),
        "v"
    ));
    // Double-quoted placement: the quotes do not protect.
    assert!(refused_dangerous(
        &render_one(LINUX, "echo \"{v}\"", of_kind("v", "text"), "$(id)"),
        "v"
    ));
}

#[test]
fn a_shell_character_in_a_string_is_refused_even_inside_the_quotes() {
    // Intentionally stricter than the C#, which lets them through single quotes: ssh, sudo
    // sh -c and cmd /c read the value a second time.
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
        "x -Confirm:$false",
    ] {
        for (platform, pattern) in [(WINDOWS, "Write-Output {v}"), (LINUX, "printf %s {v}")] {
            let result = render_one(platform, pattern, parameter("v", true), value);
            assert!(refused_dangerous(&result, "v"), "{platform:?} {value}");
        }
    }
}

#[test]
fn a_value_closing_the_quote_stays_inside_the_quotes() {
    assert_eq!(
        render_one(LINUX, "printf %s {v}", parameter("v", true), "x'rmrfx").as_deref(),
        Ok("printf %s 'x'\\''rmrfx'")
    );
    assert_eq!(
        render_one(WINDOWS, "Write-Output {v}", parameter("v", true), "x'rmrfx").as_deref(),
        Ok("Write-Output 'x''rmrfx'")
    );
}

#[test]
fn a_typographic_quote_is_doubled_on_windows_and_left_alone_on_linux() {
    for quote in ['\u{2018}', '\u{2019}', '\u{201A}', '\u{201B}'] {
        let value = format!("a{quote} Remove-Item x {quote}b");
        let windows = render_one(WINDOWS, "echo {v}", parameter("v", true), &value);
        assert_eq!(
            windows,
            Ok(format!(
                "echo 'a{quote}{quote} Remove-Item x {quote}{quote}b'"
            ))
        );
        let linux = render_one(LINUX, "echo {v}", parameter("v", true), &value);
        assert_eq!(linux, Ok(format!("echo '{value}'")));
    }
}

#[test]
fn an_inline_value_inside_single_quotes_is_escaped_but_refuses_shell_characters() {
    // The LDAP filter of the C# test is refused here: stricter than the C#.
    assert!(refused_dangerous(
        &render_one(
            WINDOWS,
            "Get-ADObject -LDAPFilter '{filter}'",
            inline("filter"),
            "(&(objectClass=user)(cn=O'Brien))"
        ),
        "filter"
    ));
    assert_eq!(
        render_one(
            WINDOWS,
            "Get-ADObject -LDAPFilter '{filter}'",
            inline("filter"),
            "cn=O'Brien*"
        )
        .as_deref(),
        Ok("Get-ADObject -LDAPFilter 'cn=O''Brien*'")
    );
    for (platform, pattern, expected) in [
        (LINUX, "grep -E '{v}' f", "grep -E 'a b *z' f"),
        (WINDOWS, "Select-String '{v}' f", "Select-String 'a b *z' f"),
    ] {
        assert_eq!(
            render_one(platform, pattern, inline("v"), "a b *z").as_deref(),
            Ok(expected)
        );
    }
}

#[test]
fn an_inline_value_that_tries_to_close_the_pattern_quotes_cannot() {
    assert_eq!(
        render_one(LINUX, "echo '{v}'", inline("v"), "x'rmecho'").as_deref(),
        Ok("echo 'x'\\''rmecho'\\'''")
    );
    assert_eq!(
        render_one(WINDOWS, "echo '{v}'", inline("v"), "x'rmecho'").as_deref(),
        Ok("echo 'x''rmecho'''")
    );
    for quote in ['\u{2018}', '\u{2019}', '\u{201A}', '\u{201B}'] {
        let value = format!("x{quote} Remove-Item y {quote}");
        assert_eq!(
            render_one(WINDOWS, "echo '{v}'", inline("v"), &value),
            Ok(format!(
                "echo 'x{quote}{quote} Remove-Item y {quote}{quote}'"
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
            // Bare, the quotes would carry it; refused all the same (second-stage shells).
            let bare = render_one(platform, "run {v}", parameter("v", true), &value);
            assert!(refused_dangerous(&bare, "v"), "{platform:?} {character}");
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
            &[("a", "{b}"), ("b", "x id")],
        )
        .expect("both are data");
        assert_eq!(command, "run '{b}' 'x id'", "{platform:?}");
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
    use QuoteContext::{AnsiC, Bare, DoubleQuoted, Expanding, SingleQuoted};
    let rows: [(&str, bool, &[QuoteContext]); 12] = [
        ("echo {x}", false, &[Bare]),
        ("echo '{x}'", false, &[SingleQuoted]),
        ("echo \"{x}\"", false, &[DoubleQuoted]),
        ("echo \"a '{x}'\"", false, &[DoubleQuoted]),
        ("echo $'{x}'", false, &[AnsiC]),
        ("echo $'a\\'{x}'", false, &[AnsiC]),
        ("echo ${x}", false, &[Expanding]),
        ("echo ${x}", true, &[Expanding]),
        ("echo \\${x}", false, &[Bare]),
        (
            "echo '{x}' {x} \"{x}\"",
            false,
            &[SingleQuoted, Bare, DoubleQuoted],
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
    use QuoteContext::{Bare, DoubleQuoted, SingleQuoted};
    // Bash escapes with a backslash, PowerShell with a backtick.
    assert_eq!(
        placeholder_quote_contexts("echo \\'{x}", "x", false),
        [Bare]
    );
    assert_eq!(placeholder_quote_contexts("echo `'{x}", "x", true), [Bare]);
    assert_eq!(
        placeholder_quote_contexts("echo \"a\\\"{x}\"", "x", false),
        [DoubleQuoted]
    );
    assert_eq!(
        placeholder_quote_contexts("echo \"a`\"{x}\"", "x", true),
        [DoubleQuoted]
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
        [DoubleQuoted]
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

// ---- Regressions found by review: the lexer and the generator must agree ----------------------

#[test]
fn an_escaped_placeholder_is_still_a_placeholder_and_keeps_the_blacklist() {
    // The generator replaces `{v}` whatever precedes it, so `\{v}` is the value glued to a
    // backslash, and the opening quote the generator adds is the one the backslash escapes.
    assert_eq!(
        placeholder_quote_contexts("echo \\{v}", "v", false),
        [QuoteContext::Uncertain]
    );
    assert_eq!(
        placeholder_quote_contexts("Write-Output `{v}", "v", true),
        [QuoteContext::Uncertain]
    );
    for (platform, pattern) in [
        (LINUX, "echo \\{v}"),
        (WINDOWS, "Write-Output `{v}"),
        (LINUX, "echo {v} \\{v}"),
    ] {
        for value in [
            "x; printf INJECTED #",
            "x | id",
            "x & id",
            "x $(id)",
            "x`id`",
        ] {
            let result = render_one(platform, pattern, parameter("v", true), value);
            assert!(
                refused_dangerous(&result, "v"),
                "{platform:?} {pattern} {value}: {result:?}"
            );
        }
    }
    // A word split would also leave the quotes the generator adds unbalanced.
    assert!(refused_dangerous(
        &render_one(LINUX, "echo \\{v}", parameter("v", true), "a b"),
        "v"
    ));
}

#[test]
fn a_placeholder_after_a_substitution_or_expansion_is_not_trusted_to_any_quoting() {
    use QuoteContext::Uncertain;
    // The inner double quotes open a string of their own: the lexer used to take them for the
    // end of the outer string and class the placeholder bare.
    assert_eq!(
        placeholder_quote_contexts("echo \"$(printf \"{v}\")\"", "v", false),
        [Uncertain]
    );
    assert_eq!(
        placeholder_quote_contexts("Write-Output \"$(Write-Output \"{v}\")\"", "v", true),
        [Uncertain]
    );
    assert_eq!(
        placeholder_quote_contexts("echo \"${x:-\"{v}\"}\"", "v", false),
        [Uncertain]
    );
    assert_eq!(
        placeholder_quote_contexts("echo \"`printf \"{v}\"`\"", "v", false),
        [Uncertain]
    );
    for (platform, pattern, value) in [
        (LINUX, "echo \"$(printf \"{v}\")\"", "$(printf INJECTED)"),
        (
            WINDOWS,
            "Write-Output \"$(Write-Output \"{v}\")\"",
            "$(Write-Output INJECTED)",
        ),
        (LINUX, "echo \"${x:-\"{v}\"}\"", "$(id)"),
        (LINUX, "echo \"`printf \"{v}\"`\"", "$(id)"),
    ] {
        let result = render_one(platform, pattern, parameter("v", true), value);
        assert!(
            refused_dangerous(&result, "v"),
            "{platform:?} {pattern}: {result:?}"
        );
    }
    // What comes before the substitution is still followed; what comes after is not.
    assert_eq!(
        placeholder_quote_contexts("echo '{v}' $(date) \"{v}\"", "v", false),
        [QuoteContext::SingleQuoted, Uncertain]
    );
}

#[test]
fn a_double_quote_or_an_escape_in_a_value_cannot_end_the_double_quoted_string() {
    for (platform, pattern, values) in [
        (LINUX, "echo \"{v}\"", ["x\" -rf \"y", "x\\", "a\"b"]),
        (
            WINDOWS,
            "Write-Host \"{v}\"",
            ["x\" -Force \"y", "x`", "a\u{201C}b"],
        ),
    ] {
        for value in values {
            let result = render_one(platform, pattern, parameter("v", true), value);
            assert!(
                refused_dangerous(&result, "v"),
                "{platform:?} {pattern} {value}: {result:?}"
            );
        }
    }
    // An apostrophe is the escaper's business there: `''` inside a PowerShell filter.
    assert_eq!(
        render_one(
            WINDOWS,
            "Get-ADUser -Filter \"Name -like '*{v}*'\"",
            inline("v"),
            "O'Brien"
        )
        .as_deref(),
        Ok("Get-ADUser -Filter \"Name -like '*O''Brien*'\"")
    );
}

#[test]
fn leaving_an_argument_out_cannot_change_how_another_value_is_read() {
    // `echo ${empty}'{v}'` becomes `echo $'{v}'`, a bash ANSI-C string that reads backslash
    // escapes: the value is judged against that pattern, not the one before the removal.
    let result = render(
        LINUX,
        "echo ${empty}'{v}'",
        vec![parameter("empty", false), inline("v")],
        &[("v", "\\'; printf INJECTED #")],
    );
    assert!(refused_dangerous(&result, "v"), "{result:?}");
    // Even a plain value is refused there (stricter than C#): `$'...'` is not a
    // single-quoted span, and an inline value brings no quote of its own.
    let result = render(
        LINUX,
        "echo ${empty}'{v}'",
        vec![parameter("empty", false), inline("v")],
        &[("v", "plain")],
    );
    assert!(refused_dangerous(&result, "v"), "{result:?}");
    assert_eq!(
        render(
            LINUX,
            "echo '{v}'",
            vec![parameter("empty", false), inline("v")],
            &[("v", "\\'printfOK#")],
        )
        .as_deref(),
        Ok("echo '\\'\\''printfOK#'")
    );
}

#[test]
fn a_safe_value_stays_quoted_where_the_text_around_it_would_join_the_syntax() {
    // `{{v}}` with `a,b` would be the bash brace expansion `{a,b}`.
    assert_eq!(
        render_one(LINUX, "printf '<%s>\\n' {{v}}", parameter("v", true), "a,b").as_deref(),
        Ok("printf '<%s>\\n' {'a,b'}")
    );
    // `({v})` with `1+2` would be a PowerShell expression, `@{v}` a splat.
    assert_eq!(
        render_one(WINDOWS, "Write-Output ({v})", parameter("v", true), "1+2").as_deref(),
        Ok("Write-Output ('1+2')")
    );
    assert_eq!(
        render_one(WINDOWS, "Write-Output @{v}", parameter("v", true), "args").as_deref(),
        Ok("Write-Output @'args'")
    );
    assert_eq!(
        render_one(LINUX, "echo a,{v}", parameter("v", true), "b").as_deref(),
        Ok("echo a,'b'")
    );
    // A value with `=` at the start of a command would be an assignment.
    for platform in BOTH {
        assert_eq!(
            render_one(platform, "{v} run", parameter("v", true), "FOO=bar").as_deref(),
            Ok("'FOO=bar' run")
        );
        assert_eq!(
            render_one(platform, "run --opt={v}", parameter("v", true), "a=b").as_deref(),
            Ok("run --opt=a=b")
        );
    }
}

#[test]
fn a_misplaced_quoting_gets_the_strictest_refusals() {
    // A ShellQuote value in the pattern's own single quotes ends up outside any quote, and an
    // InlineInQuotes value outside any quote is not quoted at all.
    for (pattern, parameter) in [
        ("echo '{v}'", parameter("v", true)),
        ("echo {v}", inline("v")),
    ] {
        for value in [
            "a b", "a*", "a,b", "~", "a#b", "a'b", "a\\b", "a!b", "{a,b}",
        ] {
            let result = render_one(LINUX, pattern, parameter.clone(), value);
            assert!(
                refused_dangerous(&result, "v"),
                "{pattern} {value}: {result:?}"
            );
        }
    }
}

#[test]
fn a_value_cannot_change_how_a_later_placeholder_is_read() {
    // `\{a}` is the value glued to a backslash: if `a` were quoted, its opening quote would be
    // the one the backslash escapes and the quotes after it would swap sides, exposing `b`.
    for (platform, pattern) in [
        (LINUX, "echo \\{a} '{b}'"),
        (WINDOWS, "Write-Output `{a} '{b}'"),
    ] {
        let result = render(
            platform,
            pattern,
            vec![parameter("a", true), inline("b")],
            &[("a", "abc"), ("b", "x; printf INJECTED; #")],
        );
        assert!(result.is_err(), "{platform:?} {pattern}: {result:?}");
    }
}

#[test]
fn a_parameter_default_inside_double_quotes_reads_single_quotes_as_text() {
    // `${absent:-'{v}'}` inside double quotes: the apostrophes are not quotes.
    let pattern = "echo \"${absent:-'{v}'}\"";
    let result = render_one(LINUX, pattern, inline("v"), "$(printf INJECTED)");
    assert!(refused_dangerous(&result, "v"), "{result:?}");
    let result = render_one(LINUX, pattern, inline("v"), "x' y");
    assert!(refused_dangerous(&result, "v"), "{result:?}");
    assert_eq!(
        render_one(LINUX, pattern, inline("v"), "abc").as_deref(),
        Ok("echo \"${absent:-'abc'}\"")
    );
}

#[test]
fn a_backtick_in_a_value_cannot_end_a_legacy_substitution() {
    let value = "`\"; printf INJECTED; #";
    for parameter in [parameter("v", true), inline("v")] {
        for pattern in [
            "echo \"`printf %s {v}`\"",
            "echo \"`printf %s '{v}'`\"",
            "echo `printf %s '{v}'`",
            "echo $(printf %s '{v}')",
        ] {
            let result = render_one(LINUX, pattern, parameter.clone(), value);
            assert!(refused_dangerous(&result, "v"), "{pattern}: {result:?}");
        }
    }
    // A plain value is fine there, and goes in bare.
    assert_eq!(
        render_one(LINUX, "echo $(ls '{v}')", inline("v"), "logs").as_deref(),
        Ok("echo $(ls 'logs')")
    );
    assert_eq!(
        render_one(
            WINDOWS,
            "Write-Output $(Get-Item '{v}')",
            inline("v"),
            "logs"
        )
        .as_deref(),
        Ok("Write-Output $(Get-Item 'logs')")
    );
}

#[test]
fn a_script_pattern_is_not_followed_and_takes_plain_values_only() {
    // Comments and here-documents change what a quote means on the lines of a script.
    let pattern = "echo 'start'\n# it's a comment\necho '{v}'";
    let result = render_one(LINUX, pattern, inline("v"), "x y");
    assert!(refused_dangerous(&result, "v"), "{result:?}");
    let result = render_one(LINUX, pattern, inline("v"), "x'; id; echo '");
    assert!(refused_dangerous(&result, "v"), "{result:?}");
    assert_eq!(
        render_one(LINUX, pattern, inline("v"), "plain").as_deref(),
        Ok("echo 'start'\n# it's a comment\necho 'plain'")
    );
}

#[test]
fn a_comment_or_document_in_the_pattern_makes_the_quotes_after_it_untrusted() {
    // The apostrophe of the block comment is text, not a quote.
    let comment = "Write-Output <# user's label #> '{v}'";
    for parameter in [parameter("v", true), inline("v")] {
        let result = render_one(
            WINDOWS,
            comment,
            parameter.clone(),
            "; Write-Output INJECTED; #",
        );
        assert!(refused_dangerous(&result, "v"), "{result:?}");
    }
    // Nothing is put in after a block comment, plain or not (stricter than C#).
    let result = render_one(WINDOWS, comment, inline("v"), "plain");
    assert!(refused_dangerous(&result, "v"), "{result:?}");
    for (platform, pattern) in [
        (WINDOWS, "Write-Output a # user's note '{v}'"),
        (LINUX, "echo a # user's note '{v}'"),
        (WINDOWS, "Write-Output @'\nit's\n'@ '{v}'"),
        (LINUX, "cat <<'EOF' | tee '{v}'"),
    ] {
        let result = render_one(platform, pattern, inline("v"), "x y");
        assert!(
            refused_dangerous(&result, "v"),
            "{platform:?} {pattern}: {result:?}"
        );
    }
}

#[test]
fn a_value_alone_on_a_line_of_a_script_cannot_end_a_heredoc() {
    let pattern = "cat <<EOF\n{v}\nprintf INJECTED\nEOF";
    for value in ["EOF", "abc"] {
        let result = render_one(LINUX, pattern, parameter("v", true), value);
        assert!(refused_dangerous(&result, "v"), "{value}: {result:?}");
    }
    // Inside a line too: no substitution is made in a heredoc body (stricter than C#).
    let result = render_one(
        LINUX,
        "cat <<EOF
value: {v}
EOF",
        parameter("v", true),
        "EOF",
    );
    assert!(refused_dangerous(&result, "v"), "{result:?}");
}

#[test]
fn a_bare_value_cannot_turn_the_word_around_it_into_syntax() {
    // `-{v}` with `Force` would be the switch `-Force`, not the text `-Force`.
    assert_eq!(
        render_one(WINDOWS, "Probe -{v}", parameter("v", true), "Force").as_deref(),
        Ok("Probe -'Force'")
    );
    // `@prefix{v}` with `args` would splat `@prefixargs`.
    assert_eq!(
        render_one(
            WINDOWS,
            "Write-Output @prefix{v}",
            parameter("v", true),
            "args"
        )
        .as_deref(),
        Ok("Write-Output @prefix'args'")
    );
    // `{pre{v}post,end}` with `a,b` would be a three-way brace expansion.
    assert_eq!(
        render_one(
            LINUX,
            "printf '<%s>\\n' {pre{v}post,end}",
            parameter("v", true),
            "a,b"
        )
        .as_deref(),
        Ok("printf '<%s>\\n' {pre'a,b'post,end}")
    );
    // Other placeholders in the word do not count against it.
    assert_eq!(
        render(
            LINUX,
            "run --addr={host}:{port}",
            vec![parameter("host", true), parameter("port", true)],
            &[("host", "example.org"), ("port", "8080")]
        )
        .as_deref(),
        Ok("run --addr=example.org:8080")
    );
}

#[test]
fn a_value_glued_to_text_cannot_join_it_where_the_pattern_is_not_followed() {
    // `END_{v}` with `CONFIG` is the line that ends the heredoc `END_CONFIG`.
    let heredoc = "cat <<END_CONFIG\nEND_{v}\nprintf INJECTED\nEND_CONFIG";
    let result = render_one(LINUX, heredoc, parameter("v", true), "CONFIG");
    assert!(refused_dangerous(&result, "v"), "{result:?}");
    // The checks on the word around a value hold after a comment or a substitution too.
    for (platform, pattern, value) in [
        (WINDOWS, "Write-Output <# label #> @prefix{v}", "args"),
        (
            LINUX,
            "printf '<%s>\n' \"$(printf stamp)\" {pre{v}post,end}",
            "a,b",
        ),
        (WINDOWS, "Probe <# label #> -Fo{v}", "rce"),
    ] {
        let result = render_one(platform, pattern, parameter("v", true), value);
        assert!(
            refused_dangerous(&result, "v"),
            "{platform:?} {pattern}: {result:?}"
        );
    }
}

#[test]
fn a_value_cannot_complete_the_name_of_an_option() {
    // `-Fo{v}` with `rce` would be the switch `-Force`.
    assert_eq!(
        render_one(WINDOWS, "Probe -Fo{v}", parameter("v", true), "rce").as_deref(),
        Ok("Probe -Fo'rce'")
    );
    // A value after the `=` or `:` that binds it to the option is a value.
    assert_eq!(
        render_one(WINDOWS, "Probe -Name:{v}", parameter("v", true), "abc").as_deref(),
        Ok("Probe -Name:abc")
    );
    assert_eq!(
        render_one(LINUX, "run --opt={v}", parameter("v", true), "abc").as_deref(),
        Ok("run --opt=abc")
    );
}

#[test]
fn placeholders_cannot_jointly_close_a_heredoc() {
    // Neither value is alone on its line, yet `END END` is the delimiter line of `<<'END END'`.
    let pattern = "cat <<'END END'\n{a} {b}\nprintf INJECTED\nEND END";
    let result = render(
        LINUX,
        pattern,
        vec![parameter("a", true), parameter("b", true)],
        &[("a", "END"), ("b", "END")],
    );
    assert!(
        matches!(&result, Err(GenerateError::Values(errors)) if errors.len() == 2),
        "{result:?}"
    );
    // No substitution is made in the delimiter either, nor on the line of a PowerShell
    // here-string.
    for (platform, pattern, value) in [
        (LINUX, "cat <<{v}\nbody\n{v}", "EOF"),
        (WINDOWS, "Write-Output @'\n{v}\n'@", "text"),
    ] {
        let result = render_one(platform, pattern, parameter("v", true), value);
        assert!(
            refused_dangerous(&result, "v"),
            "{platform:?} {pattern}: {result:?}"
        );
    }
}

#[test]
fn a_typed_value_is_judged_by_where_it_stands() {
    // `0` is a valid integer and also the delimiter line of `<<0`.
    let result = render_one(
        LINUX,
        "cat <<0\n{n}\nprintf INJECTED\n0",
        of_kind("n", "integer"),
        "0",
    );
    assert!(refused_dangerous(&result, "n"), "{result:?}");
    for (kind, value) in [
        ("hostname", "END"),
        ("ipaddress", "127.0.0.1"),
        ("number", "1.5"),
        ("driveletter", "C"),
    ] {
        let result = render_one(LINUX, "cat <<EOF\n{n}\nEOF", of_kind("n", kind), value);
        assert!(refused_dangerous(&result, "n"), "{kind}: {result:?}");
    }
    // A typed value in a plain place is unchanged.
    assert_eq!(
        render_one(LINUX, "ping -c {n} host", of_kind("n", "integer"), " 4 ").as_deref(),
        Ok("ping -c 4 host")
    );
}

#[test]
fn an_inline_value_outside_single_quotes_is_a_bare_word() {
    // `@args` would splat `$args`, `-NoEnumerate` would be a switch.
    for value in ["@args", "-NoEnumerate"] {
        let result = render_one(WINDOWS, "Write-Output {v}", inline("v"), value);
        assert!(refused_dangerous(&result, "v"), "{value}: {result:?}");
    }
    // A value safe bare in a bare place is accepted, and one inside single quotes is still
    // only escaped.
    assert_eq!(
        render_one(WINDOWS, "Write-Output {v}", inline("v"), "plain").as_deref(),
        Ok("Write-Output plain")
    );
    assert_eq!(
        render_one(WINDOWS, "Write-Output '{v}'", inline("v"), "@args").as_deref(),
        Ok("Write-Output '@args'")
    );
}

#[test]
fn bash_arithmetic_is_not_a_place_for_a_quoted_value() {
    // `(( x = '$(printf INJECTED >&2)0' ))` still runs the substitution.
    for pattern in ["(( x = {v} ))", "[[ {v} -eq 1 ]]", "echo $[ {v} + 1 ]"] {
        let result = render_one(
            LINUX,
            pattern,
            parameter("v", true),
            "$(printf INJECTED >&2)0",
        );
        assert!(refused_dangerous(&result, "v"), "{pattern}: {result:?}");
    }
}

#[test]
fn an_escaped_line_break_cannot_hide_a_heredoc() {
    // Bash removes the continuation, so the pattern is `cat <<'END END'`.
    let pattern = "cat <\\\n<'END END'\n{a} {b}\nprintf 'INJECTED\\n'\nEND END";
    let result = render(
        LINUX,
        pattern,
        vec![parameter("a", true), parameter("b", true)],
        &[("a", "END"), ("b", "END")],
    );
    assert!(
        matches!(&result, Err(GenerateError::Values(errors)) if errors.len() == 2),
        "{result:?}"
    );
}

#[test]
fn a_typed_value_cannot_join_the_text_around_it_into_a_switch_or_a_splat() {
    // `Probe -Force` binds a switch, `@prefixargs` splats a variable.
    for pattern in ["Probe -{v}", "Write-Output @prefix{v}"] {
        let value = if pattern.starts_with("Probe") {
            "Force"
        } else {
            "args"
        };
        let result = render_one(WINDOWS, pattern, of_kind("v", "hostname"), value);
        assert!(refused_dangerous(&result, "v"), "{pattern}: {result:?}");
    }
    // A negative number is a number to PowerShell, and a typed value after a space is a word.
    assert_eq!(
        render_one(WINDOWS, "Probe -Count {v}", of_kind("v", "integer"), "-1").as_deref(),
        Ok("Probe -Count -1")
    );
    assert_eq!(
        render_one(
            WINDOWS,
            "Test-Connection {v}",
            of_kind("v", "hostname"),
            "example.org"
        )
        .as_deref(),
        Ok("Test-Connection example.org")
    );
}

// ---- Review round: option injection and second-stage shells -----------------------------

#[test]
fn a_string_or_path_starting_with_a_dash_is_refused_on_both_platforms() {
    // `ssh -oProxyCommand=x` runs a command on the local machine.
    for platform in BOTH {
        for kind in ["string", "path", "text"] {
            let result = render_one(
                platform,
                "ssh {target}",
                of_kind("target", kind),
                "-oProxyCommand=x",
            );
            assert!(refused_dangerous(&result, "target"), "{platform:?} {kind}");
        }
    }
}

#[test]
fn a_string_starting_with_a_slash_is_refused_on_windows_only_and_a_path_is_not() {
    let result = render_one(WINDOWS, "ping {t}", parameter("t", true), "/t");
    assert!(refused_dangerous(&result, "t"));
    assert!(render_one(LINUX, "ls {t}", parameter("t", true), "/var/log").is_ok());
    for value in [r"C:\Temp", r"\\server\share", "/var/log"] {
        assert!(
            render_one(WINDOWS, "dir {p}", of_kind("p", "path"), value).is_ok(),
            "{value}"
        );
    }
}

#[test]
fn a_substitution_in_a_string_is_refused_on_linux_whatever_the_context() {
    // `let` evaluates `a[$(cmd)]` as an arithmetic index, quoted or not.
    for pattern in ["let n={v}", "let n='{v}'", "let \"n={v}\"", "echo {v}"] {
        for value in ["a[$(cmd)]", "$(id)", "`id`", "a[$x]"] {
            let result = render_one(LINUX, pattern, parameter("v", true), value);
            assert!(refused_dangerous(&result, "v"), "{pattern} {value}");
        }
    }
}

#[test]
fn metacharacters_are_refused_inside_single_quotes_too() {
    // Intentionally stricter than the C# (single quotes relaxation undone): a second-stage
    // shell (ssh, sudo sh -c, cmd /c) reads the value again.
    for platform in BOTH {
        for character in SHELL_METACHARACTERS {
            let value = format!("a{character}b");
            for (pattern, parameter) in [
                ("run '{v}'", inline("v")),
                ("run {v}", parameter("v", true)),
                ("run {v}", of_kind("v", "path")),
            ] {
                let result = render_one(platform, pattern, parameter, &value);
                assert!(
                    refused_dangerous(&result, "v"),
                    "{platform:?} {pattern} {value}"
                );
            }
        }
    }
}

#[test]
fn a_double_quote_or_trailing_backslash_is_refused_on_windows() {
    for value in [
        "a\"b",
        "a\u{201C}b",
        "a\u{201D}b",
        "a\u{201E}b",
        "C:\\Temp\\",
    ] {
        for kind in ["string", "path"] {
            let result = render_one(WINDOWS, "run {v}", of_kind("v", kind), value);
            assert!(refused_dangerous(&result, "v"), "{kind} {value}");
        }
    }
    assert!(render_one(LINUX, "run {v}", parameter("v", true), "a\"b").is_ok());
    assert!(render_one(LINUX, "run {v}", parameter("v", true), "dir\\").is_ok());
}

#[test]
fn an_exclamation_mark_is_refused_in_double_quotes_on_linux_only() {
    let result = render_one(LINUX, "echo \"{v}\"", parameter("v", true), "a!b");
    assert!(refused_dangerous(&result, "v"));
    assert!(render_one(WINDOWS, "echo \"{v}\"", parameter("v", true), "a!b").is_ok());
}

#[test]
fn a_number_holds_only_digits_signs_point_and_exponent() {
    for platform in BOTH {
        for value in ["inf", "NaN", "-infinity", "1_0", "0x10"] {
            let result = render_one(platform, "run {v}", of_kind("v", "number"), value);
            assert_eq!(
                result,
                Err(GenerateError::Values(vec![ValueError::NotNumber(
                    "v".to_owned()
                )])),
                "{platform:?} {value}"
            );
        }
        for value in ["3", "1.5", "1e5", "2.5E+3", "1e-5"] {
            assert!(
                render_one(platform, "run {v}", of_kind("v", "number"), value).is_ok(),
                "{platform:?} {value}"
            );
        }
    }
}

#[test]
fn an_option_after_white_space_is_refused_in_free_text() {
    // `find {dir} -name x.log` in a second-stage shell: the value adds an `-exec`.
    for platform in BOTH {
        for kind in ["string", "path", "text"] {
            let result = render_one(
                platform,
                "ssh h find {dir} -name x.log",
                of_kind("dir", kind),
                "/ -exec id {} +",
            );
            assert!(refused_dangerous(&result, "dir"), "{platform:?} {kind}");
        }
    }
    let result = render_one(WINDOWS, "ssh h dir {d}", parameter("d", true), "x /s");
    assert!(refused_dangerous(&result, "d"));
    assert!(render_one(WINDOWS, "dir {p}", of_kind("p", "path"), r"C:\a b/c").is_ok());
    assert!(render_one(LINUX, "ls {d}", parameter("d", true), "a b/c").is_ok());
}

#[test]
fn a_negative_integer_or_number_is_refused_on_linux_only() {
    for kind in ["int", "number"] {
        let result = render_one(LINUX, "kill -s KILL {pid}", of_kind("pid", kind), "-1");
        assert!(matches!(result, Err(GenerateError::Values(_))), "{kind}");
        assert!(render_one(WINDOWS, "Stop {pid}", of_kind("pid", kind), "-1").is_ok());
        assert!(render_one(LINUX, "kill {pid}", of_kind("pid", kind), "1").is_ok());
    }
}

#[test]
fn parentheses_are_let_through_in_a_windows_path_only() {
    let path = r"C:\Program Files (x86)\App";
    assert_eq!(
        render_one(WINDOWS, "dir {p}", of_kind("p", "path"), path).as_deref(),
        Ok(r"dir 'C:\Program Files (x86)\App'")
    );
    assert!(render_one(WINDOWS, "dir {p}", parameter("p", true), path).is_err());
    assert!(render_one(LINUX, "ls {p}", of_kind("p", "path"), path).is_err());
}

#[test]
fn a_dash_is_let_through_only_after_a_letter_or_digit() {
    let pattern = "ssh {host} find {dir} -name x.log";
    let parameters = || vec![parameter("host", true), of_kind("dir", "path")];
    for value in [
        "/ -exec x {} +",
        "/ '-exec' x {} +",
        "/ \"-exec\" x {} +",
        r"/ \-exec x {} +",
        "/ {,-exec} x {} +",
        "/ a,-exec",
        "-x",
    ] {
        for platform in BOTH {
            let result = render(
                platform,
                pattern,
                parameters(),
                &[("host", "h"), ("dir", value)],
            );
            assert!(
                matches!(result, Err(GenerateError::Values(_))),
                "{platform:?} {value}"
            );
        }
    }
    for value in ["my-file", "2026-10-22"] {
        for platform in BOTH {
            assert!(
                render_one(platform, "ls {v}", parameter("v", true), value).is_ok(),
                "{platform:?} {value}"
            );
        }
    }
    let result = render_one(WINDOWS, "ssh h dir {d}", parameter("d", true), "x ^/s");
    assert!(refused_dangerous(&result, "d"));
}

#[test]
fn quotes_escapes_and_braces_are_refused_on_linux_when_the_value_has_white_space() {
    for value in ["a 'b", "a \"b", r"a \b", "a {b", "a b}"] {
        let result = render_one(LINUX, "ls {v}", parameter("v", true), value);
        assert!(refused_dangerous(&result, "v"), "{value}");
    }
    assert!(render_one(LINUX, "ls {v}", parameter("v", true), "it's").is_ok());
}

#[test]
fn only_the_x86_folder_text_keeps_its_parentheses_in_a_windows_path() {
    for value in [
        r"C:\x(Stop-Computer)",
        r"C:\a (calc)",
        r"C:\(x86",
        r"C:\x(x(x86)86)",
    ] {
        let result = render_one(WINDOWS, "dir {p}", of_kind("p", "path"), value);
        assert!(refused_dangerous(&result, "p"), "{value}");
    }
}

#[test]
fn a_windows_path_may_hold_a_slash_but_not_one_after_white_space() {
    let result = render_one(
        WINDOWS,
        "icacls {p}",
        of_kind("p", "path"),
        r"C:\x /grant Everyone:F",
    );
    assert!(refused_dangerous(&result, "p"));
    for value in [r"C:\dir/sub", r"\server\share"] {
        assert!(
            render_one(WINDOWS, "dir {p}", of_kind("p", "path"), value).is_ok(),
            "{value}"
        );
    }
}

#[test]
fn a_number_in_an_expression_slot_generates_and_a_string_there_does_not() {
    let pattern = "Get-WinEvent -FilterHashtable @{LogName='System'; StartTime=(Get-Date).AddHours(-{hours})}";
    for kind in ["int", "integer", "number"] {
        let mut hours = of_kind("hours", kind);
        hours.default_value = Some("24".to_owned());
        assert_eq!(
            render(WINDOWS, pattern, vec![hours.clone()], &[]).as_deref(),
            Ok(&pattern.replace("{hours}", "24")[..]),
            "{kind}"
        );
        // A value that is not a number is refused before the slot is looked at.
        let result = render(WINDOWS, pattern, vec![hours], &[("hours", "1)+(whoami")]);
        assert!(matches!(result, Err(GenerateError::Values(_))), "{kind}");
    }
    let result = render(
        WINDOWS,
        pattern,
        vec![of_kind("hours", "string")],
        &[("hours", "1)+(whoami")],
    );
    assert!(
        matches!(result, Err(GenerateError::Values(_))),
        "a string cannot use the slot of a number: {result:?}"
    );
}

#[test]
fn a_number_in_a_word_that_starts_with_a_dash_on_linux_takes_digits_only() {
    let mut n = of_kind("n", "number");
    n.default_value = Some("5".to_owned());
    assert!(render(LINUX, "cmd -{n}", vec![n.clone()], &[]).is_ok());
    let result = render(LINUX, "cmd -{n}", vec![n.clone()], &[("n", "1e5")]);
    assert!(
        matches!(result, Err(GenerateError::Values(_))),
        "{result:?}"
    );
    assert!(render(LINUX, "cmd {n}", vec![n], &[("n", "1e5")]).is_ok());
}
