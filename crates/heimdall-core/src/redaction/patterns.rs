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

//! Every pattern of the catalogue, as the C# `RedactionPatterns`. All are line-oriented
//! (`[ \t]` never `\s` between tokens) and a quantifier is bounded only where it limits a
//! gap, never a masked value.
//!
//! The `regex` crate has no look-around, so each look-behind and look-ahead of the C# pattern is
//! a `Guard` function next to its pattern (see `scan::Guard`). The groups named `v...` hold the
//! span that gets masked; the C# reuses the name `v` in alternations, here each alternative has
//! its own name.

use std::sync::LazyLock;

use super::MASK_PREFIX;
use super::scan::{Found, Pattern, char_at, char_before};

// No bound limits a masked span: a value is masked to the end of its token or line, however long.
// The `regex` engine is linear-time, the per-rule budget limits the work and the input cap limits
// the size, so an unbounded value cannot cost more than one pass. A bound on a value would leave
// its tail in the clear (a 4097 character secret masked up to 4096 and the rest kept).

/// A double quoted value: backslash escapes are part of the value, so `\"` does not close it.
const DOUBLE_QUOTED: &str = r#""(?P<v1>(?:[^"\\\r\n]|\\[^\r\n])*)""#;

/// A single quoted value, same escape handling.
const SINGLE_QUOTED: &str = r"'(?P<v2>(?:[^'\\\r\n]|\\[^\r\n])*)'";

/// A double quoted value glued to more text (`"first"TAILSECRET`, shell concatenation, or a YAML
/// doubled quote): the whole glued run is the value. A closer such as `}` or `,` is not glue.
const DOUBLE_QUOTED_GLUED: &str = r#""(?P<v6>(?:[^"\\\r\n]|\\[^\r\n])*"[A-Za-z0-9_"'][^\s;,&]*)"#;

/// Same for single quotes.
const SINGLE_QUOTED_GLUED: &str = r#"'(?P<v7>(?:[^'\\\r\n]|\\[^\r\n])*'[A-Za-z0-9_"'][^\s;,&]*)"#;

/// An opening quote that is never closed on its line: the rest of the line is masked (fail
/// closed) rather than left because no closing quote was found.
const UNTERMINATED_QUOTED: &str = r#""(?P<v4>[^\r\n]+)|'(?P<v5>[^\r\n]+)"#;

/// Whitespace of a header, including a folded continuation (line break then blank).
const HEADER_BLANK: &str = r"(?:[ \t]|\r?\n[ \t])";

/// Line break that continues a folded header value.
const HEADER_FOLD: &str = r"\r?\n[ \t]+";

/// Key names that identify a secret by their ending (`db_password`, `GITHUB_TOKEN`).
const SECRET_KEY_ENDINGS: [&str; 13] = [
    "password",
    "passwd",
    "secret",
    "token",
    "api_key",
    "apikey",
    "api-key",
    "access_key",
    "accesskey",
    "private_key",
    "privatekey",
    "client-key-data",
    "client_secret",
];

/// Short endings that are secrets only as a separate word of the key (`DB_PASS`, `db.pwd`,
/// `dbPass`, a bare `pass`): `bypass`, `compass` and the shell's `PWD` (the working directory)
/// are not secrets. The guard `key_ending_ok` enforces the word boundary.
const WEAK_KEY_ENDINGS: [&str; 2] = ["pass", "pwd"];

/// A double quoted value inside a JSON string that was itself encoded (`\"hunter2\"`): the value
/// runs to the next escaped quote.
const ESCAPED_DOUBLE_QUOTED: &str = r#"\\"(?P<v8>(?:\\\\\\"|\\[^"\r\n]|[^\\\r\n])*?)\\""#;

/// An escaped opening quote that is never closed on its line: the rest of the line is masked.
const ESCAPED_UNTERMINATED: &str = r#"\\"(?P<v9>[^\r\n]+)"#;

/// A quote of a key name, possibly escaped (`"`, `'`, `\"`).
const OPTIONAL_QUOTE: &str = r#"(?:\\?["'])?"#;

/// Authorization schemes whose credential is one token (`Bearer abc`).
const TOKEN_SCHEMES: [&str; 4] = ["bearer", "basic", "token", "digest"];

const MYSQL_COMMANDS: &str = "(?:mysql|mysqldump|mysqladmin|mariadb)";

/// One character of a command's arguments before `-p`: a command separator (`;`, `|`, `&`) ends
/// the command (`mysql --version; ssh -p 2222 h`), unless it is inside a quoted argument.
const MYSQL_ARGUMENT: &str = r#"(?:[^\r\n;|&'"]|'[^'\r\n]*'|"[^"\r\n]*")"#;

const PROMPT_WORDS: &str = r"(?:password|passphrase|mot de passe|passwort|contrase(?:na|\x{F1}a))";

const PROMPT_FOR_WORDS: &str = r"(?:for|pour|f\x{FC}r|para)";

const HOST_LABEL: &str = "[A-Za-z0-9](?:[A-Za-z0-9-]{0,61}[A-Za-z0-9])?";

/// A quoted value (mask inside the quotes) or a bare value up to whitespace; empty never matches.
fn quoted_or_bare() -> String {
    format!(
        r#"(?:{DOUBLE_QUOTED_GLUED}|{SINGLE_QUOTED_GLUED}|{DOUBLE_QUOTED}|{SINGLE_QUOTED}|(?P<v3>[^\s"']+)|{UNTERMINATED_QUOTED})"#
    )
}

/// Same, for a bare value that must not start with `-` (an option, not a password) or a quote.
fn quoted_or_bare_no_option() -> String {
    format!(
        r#"(?:{DOUBLE_QUOTED_GLUED}|{SINGLE_QUOTED_GLUED}|{DOUBLE_QUOTED}|{SINGLE_QUOTED}|(?P<v3>[^\s"'-][^\s"']*)|{UNTERMINATED_QUOTED})"#
    )
}

/// Key-value form with `=`: a bare value stops before `&name=` (query strings, see
/// `cut_before_next_parameter`). It does not stop at `,` or `;`: those occur inside passwords,
/// and an early stop would leave the tail in the clear.
fn quoted_or_bare_delimited() -> String {
    format!(
        r#"(?:{ESCAPED_DOUBLE_QUOTED}|{ESCAPED_UNTERMINATED}|{DOUBLE_QUOTED_GLUED}|{SINGLE_QUOTED_GLUED}|{DOUBLE_QUOTED}|{SINGLE_QUOTED}|(?P<v3>[^\s"']+)|{UNTERMINATED_QUOTED})"#
    )
}

/// Where a bare `=` value ends: before the `&` that starts another query string parameter
/// (`&name=`), not at a raw `&` inside a password (`Abc&123xyz`).
fn cut_before_next_parameter(hay: &str, start: usize, end: usize) -> usize {
    let value = &hay[start..end];
    let bytes = value.as_bytes();
    for (index, _) in value.match_indices('&') {
        let name_length = bytes[index + 1..]
            .iter()
            .take_while(|b| b.is_ascii_alphanumeric() || matches!(**b, b'_' | b'.' | b'-'))
            .count();
        if name_length > 0 && bytes.get(index + 1 + name_length) == Some(&b'=') {
            return start + index;
        }
    }
    end
}

/// Where a YAML block scalar ends: at the first line that is not indented deeper than the line
/// of its key (`  secret: >-` then `    abc` then `  other: 1`).
fn cut_block_scalar(hay: &str, start: usize, end: usize) -> usize {
    let key_line_start = hay[..start].rfind('\n').map_or(0, |index| index + 1);
    let indent_of = |line: &str| line.len() - line.trim_start_matches([' ', '\t']).len();
    let key_indent = indent_of(&hay[key_line_start..start]);
    let value = &hay[start..end];
    let mut kept = start;
    let mut offset = 0;
    for (number, line) in value.split('\n').enumerate() {
        let content = line.strip_suffix('\r').unwrap_or(line);
        if number == 0 || indent_of(content) > key_indent {
            kept = start + offset + content.len();
            offset += line.len() + 1;
        } else {
            break;
        }
    }
    kept
}

/// Key-value form with `:` (YAML, `secret: correct horse battery`): a bare value is every word up
/// to a quote, so a passphrase with spaces is masked whole.
fn quoted_or_bare_words() -> String {
    format!(
        r#"(?:{ESCAPED_DOUBLE_QUOTED}|{ESCAPED_UNTERMINATED}|{DOUBLE_QUOTED_GLUED}|{SINGLE_QUOTED_GLUED}|{DOUBLE_QUOTED}|{SINGLE_QUOTED}|(?P<v3>[^\s"']+(?:[ \t]+[^\s"']+)*)|{UNTERMINATED_QUOTED})"#
    )
}

/// The alternation of the key endings that identify a secret.
fn key_endings(endings: &[&[&str]]) -> String {
    let all: Vec<&str> = endings
        .iter()
        .flat_map(|group| group.iter().copied())
        .collect();
    format!("(?:{})", all.join("|"))
}

/// A header token that may continue on folded lines: `piece (fold piece)*`.
fn folded(piece: &str) -> String {
    format!("{piece}(?:[ \t]*{HEADER_FOLD}{piece})*")
}

// Look-behind and look-ahead stand-ins.

fn is_ascii_alnum(c: char) -> bool {
    c.is_ascii_alphanumeric()
}

fn is_word(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

fn is_word_dash(c: char) -> bool {
    is_word(c) || c == '-'
}

fn not_preceded_by(hay: &str, index: usize, forbidden: fn(char) -> bool) -> bool {
    !char_before(hay, index).is_some_and(forbidden)
}

fn not_followed_by(hay: &str, index: usize, forbidden: fn(char) -> bool) -> bool {
    !char_at(hay, index).is_some_and(forbidden)
}

fn after_alnum_ok(hay: &str, found: &Found) -> bool {
    not_preceded_by(hay, found.start, is_ascii_alnum)
}

fn after_word_ok(hay: &str, found: &Found) -> bool {
    not_preceded_by(hay, found.start, is_word)
}

fn after_word_dash_ok(hay: &str, found: &Found) -> bool {
    not_preceded_by(hay, found.start, is_word_dash)
}

fn after_letter_ok(hay: &str, found: &Found) -> bool {
    not_preceded_by(hay, found.start, |c| c.is_ascii_alphabetic())
}

/// A header with no scheme at all is the credential itself, unless that word is a scheme name
/// left without a credential (`Authorization: Token`).
fn authorization_other_ok(hay: &str, found: &Found) -> bool {
    let name_before = &hay[..found.start];
    let name_before = name_before
        .get(name_before.len().saturating_sub("proxy-".len())..)
        .filter(|tail| tail.eq_ignore_ascii_case("proxy-"))
        .map_or(name_before, |_| {
            &name_before[..name_before.len() - "proxy-".len()]
        });
    // A long token after the name is a credential wherever the header sits (after a log prefix);
    // a short word needs the name to start a field, so prose stays unchanged.
    let long_token = found.value.is_some_and(|(start, end)| {
        hay[start..end]
            .split_whitespace()
            .next()
            .is_some_and(|token| token.len() >= MIN_UNPREFIXED_CREDENTIAL_LENGTH)
    });
    (starts_a_field(name_before) || long_token) && authorization_value_ok(hay, found)
}

/// A credential at least this long is masked after an `Authorization` name even in the middle of
/// a line.
const MIN_UNPREFIXED_CREDENTIAL_LENGTH: usize = 12;

/// True when what precedes a name is the start of a line (after blanks), or an opening quote,
/// brace, bracket or comma, a list dash, a comment mark or a `>` of a verbose header dump:
/// the name starts a header or a field instead of sitting in the middle of a sentence.
fn starts_a_field(before: &str) -> bool {
    let trimmed = before.trim_end_matches([' ', '\t']);
    trimmed.is_empty()
        || trimmed.ends_with(['\n', '"', '\'', '{', '[', ',', '-', '#', '>', '<', '|', ';'])
}

fn authorization_value_ok(hay: &str, found: &Found) -> bool {
    let Some((start, end)) = found.value else {
        return true;
    };
    let before = hay[..start].trim_end_matches(|c: char| {
        c.is_whitespace() || c == '"' || c == '\'' || c == '\\' || c == '['
    });
    let known_scheme = |word: &str| {
        TOKEN_SCHEMES
            .iter()
            .any(|scheme| word.eq_ignore_ascii_case(scheme))
    };
    if before.ends_with([':', '=']) {
        // No scheme: the scheme word alone is not a credential, and a scheme followed by a
        // mask token is the first rule's output, not a bare token.
        return !known_scheme(&hay[start..end])
            && !hay[end..].trim_start().starts_with(MASK_PREFIX);
    }
    // A known scheme before the value is the other pattern's: it holds one token only.
    let scheme = before
        .rsplit(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '-'))
        .next()
        .unwrap_or_default();
    !known_scheme(scheme)
}

fn is_key_character(c: char) -> bool {
    is_word_dash(c) || c == '.'
}

/// A key that ends in `pass` or `pwd` names a secret only when that is a word of its own: the
/// whole key (`pass`, not `pwd`, which is the shell's working directory), after a separator
/// (`DB_PASS`, `db.pwd`) or after a lower case letter in camel case (`dbPass`). Any other ending
/// is a secret as it stands.
fn key_ending_ok(hay: &str, found: &Found) -> bool {
    let rest = &hay[found.start..];
    let (rest, is_tag) = rest
        .strip_prefix('<')
        .map_or((rest, false), |tag| (tag, true));
    let key_end = if is_tag {
        rest.find(|c: char| c == '>' || c.is_whitespace())
    } else {
        rest.find(|c: char| !is_key_character(c))
    };
    let key = &rest[..key_end.unwrap_or(rest.len())];
    let ends_with = |ending: &str| {
        key.len() >= ending.len()
            && key.is_char_boundary(key.len() - ending.len())
            && key[key.len() - ending.len()..].eq_ignore_ascii_case(ending)
    };
    if SECRET_KEY_ENDINGS.iter().any(|ending| ends_with(ending)) {
        return true;
    }
    let Some(weak) = WEAK_KEY_ENDINGS.iter().find(|ending| ends_with(ending)) else {
        return true;
    };
    let stem = &key[..key.len() - weak.len()];
    match stem.chars().next_back() {
        None => *weak == "pass",
        Some('_' | '-' | '.' | ':') => true,
        Some(c) => {
            c.is_ascii_lowercase()
                && key[stem.len()..]
                    .chars()
                    .next()
                    .is_some_and(|first| first.is_ascii_uppercase())
        }
    }
}

fn key_value_ok(hay: &str, found: &Found) -> bool {
    after_alnum_ok(hay, found)
        && key_ending_ok(hay, found)
        && !is_authorization_scheme_value(hay, found)
}

/// `Authorization=Bearer abc` after the header rule: the value `Bearer` is a scheme, not a secret.
fn is_authorization_scheme_value(hay: &str, found: &Found) -> bool {
    let Some((start, end)) = found.value else {
        return false;
    };
    let key_end = hay[found.start..]
        .find(|c: char| !is_key_character(c))
        .map_or(hay.len(), |offset| found.start + offset);
    let key = &hay[found.start..key_end];
    key.len() >= "authorization".len()
        && key[key.len() - "authorization".len()..].eq_ignore_ascii_case("authorization")
        && (hay[start..end].contains(MASK_PREFIX)
            || hay[start..end]
                .split_whitespace()
                .next()
                .is_some_and(|word| {
                    TOKEN_SCHEMES
                        .iter()
                        .any(|scheme| word.eq_ignore_ascii_case(scheme))
                }))
}

/// After a log prefix (`INFO token: abc123xyz`) the first token only, and only when it has a
/// digit or a symbol or is long: a bare word is prose (`unexpected token: ident`).
fn key_value_token_ok(hay: &str, found: &Found) -> bool {
    const MIN_WORD_SECRET_LENGTH: usize = 16;
    let Some((start, end)) = found.value else {
        return false;
    };
    if starts_a_field(&hay[..found.start]) || !key_value_words_shape_ok(hay, found) {
        return false;
    }
    let quoted = char_before(hay, start).is_some_and(|c| c == '"' || c == '\'');
    let value = &hay[start..end];
    quoted
        || value.len() >= MIN_WORD_SECRET_LENGTH
        || value.chars().any(|c| !c.is_ascii_alphabetic())
}

/// The words form (`key: a few words`, `key = a few words`) takes a whole value, so it is limited
/// to a key that starts a line or a field, and a bare `pass` before a colon (`PASS: test_login`,
/// `pass: 12 fail: 0`) is a test report, not a secret.
fn key_value_words_ok(hay: &str, found: &Found) -> bool {
    starts_a_field(&hay[..found.start]) && key_value_words_shape_ok(hay, found)
}

fn key_value_words_shape_ok(hay: &str, found: &Found) -> bool {
    if !key_value_ok(hay, found) {
        return false;
    }
    let key_end = hay[found.start..]
        .find(|c: char| !is_key_character(c))
        .map_or(hay.len(), |offset| found.start + offset);
    let is_bare_pass = hay[found.start..key_end].eq_ignore_ascii_case("pass");
    !(is_bare_pass
        && hay[key_end..]
            .trim_start_matches(['"', '\'', ' ', '\t'])
            .starts_with(':'))
}

/// A bare `Pwd` key is a secret after a `;` of a connection string; at the start of a line only
/// when the string goes on after the value, and never for a path (`PWD=/home/x`, `PWD=~`).
fn odbc_password_ok(hay: &str, found: &Found) -> bool {
    let Some((start, end)) = found.value else {
        return false;
    };
    if hay[found.start..].starts_with(';') {
        return true;
    }
    let value = &hay[start..end];
    hay[end..].starts_with(';') && !value.starts_with(['/', '~'])
}

/// The secret in a user-only URL (`https://glpat-AbCd1234@gitlab.com`) has no `:` to tell it
/// from a user name, so it is judged by its shape: long enough, with letters and digits, and
/// either very long or of mixed case. A plain `git@host` or `alice@corp.com` is not masked.
fn token_like_user_ok(hay: &str, found: &Found) -> bool {
    const MIN_TOKEN_LENGTH: usize = 12;
    const LONG_TOKEN_LENGTH: usize = 16;
    let Some((start, end)) = found.value else {
        return false;
    };
    let user = &hay[start..end];
    let has = |test: fn(&char) -> bool| user.chars().any(|c| test(&c));
    let digit = has(char::is_ascii_digit);
    let letter = has(char::is_ascii_alphabetic);
    let mixed_case = has(char::is_ascii_uppercase) && has(char::is_ascii_lowercase);
    user.len() >= MIN_TOKEN_LENGTH
        && digit
        && letter
        && (user.len() >= LONG_TOKEN_LENGTH || mixed_case)
}

fn aws_ok(hay: &str, found: &Found) -> bool {
    after_alnum_ok(hay, found)
        && not_followed_by(hay, found.end, |c| {
            c.is_ascii_digit() || c.is_ascii_uppercase()
        })
}

/// `(?<![0-9])(?<![0-9]\.)` before and `(?![0-9])(?!\.[0-9])` after.
fn ipv4_ok(hay: &str, found: &Found) -> bool {
    let before = &hay[..found.start];
    let mut back = before.chars().rev();
    let before_ok = match back.next() {
        Some(c) if c.is_ascii_digit() => false,
        Some('.') => !back.next().is_some_and(|c| c.is_ascii_digit()),
        _ => true,
    };
    before_ok && no_digit_continuation(hay, found.end)
}

/// `(?![0-9])(?!\.[0-9])`: no digit follows, and no dot followed by a digit.
fn no_digit_continuation(hay: &str, end: usize) -> bool {
    let mut ahead = hay[end..].chars();
    match ahead.next() {
        Some(c) if c.is_ascii_digit() => false,
        Some('.') => !ahead.next().is_some_and(|c| c.is_ascii_digit()),
        _ => true,
    }
}

/// `(?![A-Za-z0-9-])(?!\.[A-Za-z0-9])` after a host name.
fn host_end_ok(hay: &str, end: usize) -> bool {
    let mut ahead = hay[end..].chars();
    match ahead.next() {
        Some(c) if c.is_ascii_alphanumeric() || c == '-' => false,
        Some('.') => !ahead.next().is_some_and(|c| c.is_ascii_alphanumeric()),
        _ => true,
    }
}

fn prompt_host_ok(hay: &str, found: &Found) -> bool {
    not_preceded_by(hay, found.start, |c| is_word(c) || c == '.' || c == '-')
        && host_end_ok(hay, found.value.map_or(found.end, |(_, end)| end))
}

fn fully_qualified_ok(hay: &str, found: &Found) -> bool {
    not_preceded_by(hay, found.start, |c| {
        is_word(c) || c == '.' || c == '@' || c == '-'
    }) && host_end_ok(hay, found.end)
}

macro_rules! lazy_pattern {
    ($name:ident, $source:expr, $guard:expr) => {
        pub(super) static $name: LazyLock<Pattern> =
            LazyLock::new(|| Pattern::new(&$source, $guard));
    };
}

// The name of a cookie header; the cookies themselves are split by `maskers::mask_cookies`.
//
// Same name grammar as the Authorization header: the name may be quoted (JSON `"Cookie": "a=b"`,
// Python `{'Cookie': 'a=b'}`, escaped too) and the separator may be `=` (PowerShell
// `-Headers @{Cookie="a=b"}`, `--cookie=a=b`). A value without a `name=value` pair in it
// (`cookie=accepted`) has nothing to mask.
lazy_pattern!(
    COOKIE_HEADER,
    format!(r"(?i)cookie{OPTIONAL_QUOTE}[ \t]*[:=][ \t]*{OPTIONAL_QUOTE}"),
    None
);

// The curl cookie option and the quote that opens its argument: `-b 'a=b'`, `--cookie "a=b; c=d"`,
// `-b a=b`, and a short flag cluster ending in b (`-sb`, `-Lb`), read on curl lines only. A bare
// file name (`-b jar.txt`) holds no pair and is left alone. The attached form
// (`--cookie=a=b`) is the header pattern's.
lazy_pattern!(
    COOKIE_OPTION,
    r#"(?:^|[ \t])(?:--cookie|-[A-Za-z]*b)[ \t]+["']?"#,
    None
);

lazy_pattern!(
    PRIVATE_KEY_MARKER,
    "-----(?P<k>BEGIN|END) [A-Z ]{0,40}PRIVATE KEY-----",
    None
);

// Short credentials are masked too: the explicit header and scheme already identify the value.
// A folded header (`Authorization:` then a line break and a blank, or a value continued on an
// indented line) is one value. A wrap without indentation is the caller's job to join.
//
// The header name may be quoted (JSON `"Authorization": "Bearer abc"`, also escaped), and the
// name matches inside `Proxy-Authorization`. Bearer, Basic and Token take one token (anything up
// to whitespace or a quote, so `12|hJ8k...` and `alice:hunter2` are whole). Digest takes the
// rest of the line. Any other scheme (Negotiate, NTLM, AWS4-HMAC-SHA256 ...) keeps its name and
// masks the rest of the value up to a quote, and no scheme at all (`lin_api_...`) masks the
// token.
lazy_pattern!(
    AUTHORIZATION_HEADER,
    format!(
        r"(?i)Authorization{OPTIONAL_QUOTE}{HEADER_BLANK}*[:=]{HEADER_BLANK}*\[?{OPTIONAL_QUOTE}(?:(?:Bearer|Basic|Token){HEADER_BLANK}+(?P<va>{})|Digest{HEADER_BLANK}+(?P<vd>{}))",
        folded(r#"[^\s"'\\]+"#),
        folded(r"[^\r\n]+"),
    ),
    None
);

// Any other scheme, or none, only where the name starts a header or a field: `checking
// authorization: user alice allowed` is prose. Runs after the pattern above, so a known scheme
// already holds its token.
lazy_pattern!(
    AUTHORIZATION_OTHER,
    format!(
        r"(?i)Authorization{OPTIONAL_QUOTE}{HEADER_BLANK}*[:=]{HEADER_BLANK}*\[?{OPTIONAL_QUOTE}[A-Za-z][A-Za-z0-9_-]*{HEADER_BLANK}+(?P<ve>{})",
        folded(r#"[^\r\n"'\\]+"#),
    ),
    Some(authorization_other_ok)
);

// No scheme at all: the token is the credential (`lin_api_...`), under the same position rule.
lazy_pattern!(
    AUTHORIZATION_BARE,
    format!(
        r"(?i)Authorization{OPTIONAL_QUOTE}{HEADER_BLANK}*[:=]{HEADER_BLANK}*(?:(?P<vm>\[masked:[a-z-]+\])|\[?{OPTIONAL_QUOTE}(?P<vf>{}))",
        folded(r#"[^\s"'\\]+"#),
    ),
    Some(authorization_other_ok)
);

// The password runs to the last `@` of the token, so a raw `@`, `:`, `/`, `#`, `?` or `%` inside
// it cannot end the match early. Ambiguous tokens (`https://h:80/p?mail=a@b`) are masked too:
// better a false positive than a password tail in the clear. The token ends at whitespace or a
// quote or bracket that cannot be part of a credential URL.
lazy_pattern!(
    URL_CREDENTIALS,
    r#"://[^/\s:"`<>]*:(?P<v>[^\s"`<>]*)@"#,
    None
);

// A user part without a password: a token used as the user name.
lazy_pattern!(
    URL_USER_TOKEN,
    r"://(?P<v>[A-Za-z0-9._~%+=-]+)@",
    Some(token_like_user_ok)
);

lazy_pattern!(
    CLI_PASSWORD_OPTION,
    format!(
        "--(?:password|passwd)(?:=|[ \\t]{{1,16}}){}",
        quoted_or_bare()
    ),
    Some(after_word_dash_ok)
);

lazy_pattern!(
    CLI_SSHPASS,
    format!("sshpass[ \\t]{{1,16}}-p[ \\t]{{0,16}}{}", quoted_or_bare()),
    Some(after_word_dash_ok)
);

lazy_pattern!(
    CLI_EXPORT,
    format!(
        "export[ \\t]{{1,16}}[A-Za-z0-9_]{{0,60}}(?:PASSWORD|PASSWD|_PWD)={}",
        quoted_or_bare()
    ),
    Some(after_word_ok)
);

lazy_pattern!(
    CLI_ENVIRONMENT_PASSWORD,
    format!("(?:PGPASSWORD|MYSQL_PWD)={}", quoted_or_bare()),
    Some(after_word_ok)
);

// Case-sensitive on purpose: -P is the port, -p is the password. The C# `(?=[ \t])` after the
// command is the blank consumed here, so `mysql -p x` is `mysql` + blank + `-p`.
lazy_pattern!(
    CLI_MYSQL_PASSWORD,
    format!(
        "{MYSQL_COMMANDS}[ \\t](?:{MYSQL_ARGUMENT}*?[ \\t])?-p[ \\t]?{}",
        quoted_or_bare_no_option()
    ),
    Some(after_word_dash_ok)
);

// The key may be quoted (`"password"`, `'password'`, escaped in double-encoded JSON) and any
// prefix is allowed (`keyVaultReferenceIdentityClientApplicationSecret`). `=` and `=>` take one
// bare token, `:` and the spaced INI `key = value` take every word of the value.
pub(super) static KEY_VALUE_SECRET: LazyLock<Pattern> = LazyLock::new(|| {
    Pattern::with_trim(
        &format!(
            r"(?i)[A-Za-z0-9_.-]*{}{OPTIONAL_QUOTE}[ \t]{{0,16}}(?:=>|=)[ \t]{{0,16}}{}",
            key_endings(&[&SECRET_KEY_ENDINGS, &WEAK_KEY_ENDINGS, &["authorization"]]),
            quoted_or_bare_delimited()
        ),
        Some(key_value_ok),
        cut_before_next_parameter,
    )
});

lazy_pattern!(
    KEY_VALUE_SECRET_COLON,
    format!(
        r"(?i)[A-Za-z0-9_.-]*{}{OPTIONAL_QUOTE}(?:[ \t]{{0,16}}:[ \t]{{0,16}}|[ \t]{{1,16}}=[ \t]{{1,16}}){}",
        key_endings(&[&SECRET_KEY_ENDINGS, &WEAK_KEY_ENDINGS]),
        quoted_or_bare_words()
    ),
    Some(key_value_words_ok)
);

// A YAML block scalar: `password: |` and the indented lines under it are the value.
pub(super) static KEY_VALUE_BLOCK_SCALAR: LazyLock<Pattern> = LazyLock::new(|| {
    Pattern::with_trim(
        &format!(
            r"(?i)[A-Za-z0-9_.-]*{}{OPTIONAL_QUOTE}[ \t]{{0,16}}:[ \t]{{1,16}}(?P<v>[|>][+-]?[0-9]?[ \t]*\r?\n[ \t]+[^\r\n]*(?:\r?\n[ \t]+[^\r\n]*)*)",
            key_endings(&[&SECRET_KEY_ENDINGS, &WEAK_KEY_ENDINGS]),
        ),
        Some(key_value_words_ok),
        cut_block_scalar,
    )
});

// The same key and colon after a log prefix (`INFO token: abc123xyz`): the first token only, and
// only when it looks like a secret, so `unexpected token: ident foo` is not masked.
lazy_pattern!(
    KEY_VALUE_SECRET_COLON_TOKEN,
    format!(
        r"(?i)[A-Za-z0-9_.-]*{}{OPTIONAL_QUOTE}[ \t]{{0,16}}:[ \t]{{0,16}}{}",
        key_endings(&[&SECRET_KEY_ENDINGS, &WEAK_KEY_ENDINGS]),
        quoted_or_bare_delimited()
    ),
    Some(key_value_token_ok)
);

// The XML element form: `<password>hunter2</password>`, also with a CDATA section.
lazy_pattern!(
    XML_ELEMENT_SECRET,
    format!(
        r"(?i)<[A-Za-z0-9_.:-]*{}(?:[ \t][^>\r\n]*)?>(?:<!\[CDATA\[(?P<vc>[^\r\n]*?)\]\]>|(?P<v>[^<\r\n]+))</",
        key_endings(&[&SECRET_KEY_ENDINGS, &WEAK_KEY_ENDINGS])
    ),
    Some(key_value_ok)
);

// An ODBC or ADO connection string: `Driver={ODBC Driver 18};Uid=sa;Pwd=hunter2;`. A bare `Pwd`
// key is a secret only inside such a string (after a `;`, or a string that goes on after the
// value), never as the shell's `PWD=/home/x`.
lazy_pattern!(
    ODBC_PASSWORD,
    r"(?im)(?:^|;)[ \t]*Pwd[ \t]*=(?P<v>[^;\r\n]*)",
    Some(odbc_password_ok)
);

// A HAR or similar name and value pair: `{"name": "Authorization", "value": "Bearer abc"}`.
lazy_pattern!(
    NAME_VALUE_PAIR_SECRET,
    format!(
        r#"(?i)"name"[ \t]*:[ \t]*"[A-Za-z0-9_.-]*(?:{}|authorization)"[ \t]*,[ \t]*"value"[ \t]*:[ \t]*{DOUBLE_QUOTED}"#,
        SECRET_KEY_ENDINGS.join("|")
    ),
    None
);

lazy_pattern!(
    AWS_ACCESS_KEY,
    "(?P<v>(?:AKIA|ASIA)[0-9A-Z]{16})",
    Some(aws_ok)
);

lazy_pattern!(
    GITHUB_TOKEN,
    "(?P<v>gh[pousr]_[A-Za-z0-9]{36,}|github_pat_[A-Za-z0-9_]{22,})",
    Some(after_word_ok)
);

lazy_pattern!(
    SLACK_TOKEN,
    "(?P<v>xox[baprs]-[A-Za-z0-9-]{10,})",
    Some(after_alnum_ok)
);

lazy_pattern!(
    JWT,
    r"(?P<v>eyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,})",
    Some(after_word_dash_ok)
);

// A prompt word, optionally "for <who>", then a colon followed by a blank or the end of the
// line. Run on one line at a time; the C# `(?=[ \t]|$)` is the blank consumed here.
lazy_pattern!(
    PASSWORD_PROMPT,
    format!(
        "(?i){PROMPT_WORDS}(?:[ \\t]{{1,8}}{PROMPT_FOR_WORDS}[ \\t]{{1,8}}[^\\r\\n]{{1,128}}?)?[ \\t]{{0,8}}:(?:[ \\t]|$)[ \\t]{{0,64}}"
    ),
    Some(after_letter_ok)
);

lazy_pattern!(
    COMMAND_NOT_FOUND,
    "(?:-?bash|zsh|sh|ksh|fish): (?P<v>\\S+): command not found",
    Some(after_word_dash_ok)
);

lazy_pattern!(
    IPV4_ADDRESS,
    "(?P<v>[0-9]{1,3}\\.[0-9]{1,3}\\.[0-9]{1,3}\\.[0-9]{1,3})",
    Some(ipv4_ok)
);

// The host part of user@HOST prompt forms.
lazy_pattern!(
    PROMPT_HOST,
    format!("[A-Za-z_][A-Za-z0-9_.-]{{0,31}}@(?P<v>{HOST_LABEL}(?:\\.{HOST_LABEL})*)"),
    Some(prompt_host_ok)
);

// A fully qualified name: at least two dots, alphabetic last label of 2 to 24 characters.
lazy_pattern!(
    FULLY_QUALIFIED_NAME,
    format!("(?P<v>(?:{HOST_LABEL}\\.){{2,}}[A-Za-z]{{2,24}})"),
    Some(fully_qualified_ok)
);

/// Compiles every pattern now. The bounded repetitions of a Unicode class make compilation take
/// noticeable time, which must not be charged to a rule's time budget.
pub(super) fn warm_up() {
    for pattern in [
        &PRIVATE_KEY_MARKER,
        &COOKIE_HEADER,
        &AUTHORIZATION_HEADER,
        &AUTHORIZATION_OTHER,
        &AUTHORIZATION_BARE,
        &URL_CREDENTIALS,
        &URL_USER_TOKEN,
        &CLI_PASSWORD_OPTION,
        &CLI_SSHPASS,
        &CLI_EXPORT,
        &CLI_ENVIRONMENT_PASSWORD,
        &CLI_MYSQL_PASSWORD,
        &COOKIE_OPTION,
        &KEY_VALUE_SECRET,
        &KEY_VALUE_SECRET_COLON,
        &KEY_VALUE_SECRET_COLON_TOKEN,
        &KEY_VALUE_BLOCK_SCALAR,
        &XML_ELEMENT_SECRET,
        &ODBC_PASSWORD,
        &NAME_VALUE_PAIR_SECRET,
        &AWS_ACCESS_KEY,
        &GITHUB_TOKEN,
        &SLACK_TOKEN,
        &JWT,
        &PASSWORD_PROMPT,
        &COMMAND_NOT_FOUND,
        &IPV4_ADDRESS,
        &PROMPT_HOST,
        &FULLY_QUALIFIED_NAME,
    ] {
        LazyLock::force(pattern);
    }
}
