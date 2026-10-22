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

//! The rules that are more than "mask the value group of a pattern": the private key blocks, the
//! echoed password prompts (C# `PrivateKeyBlockMasker`, `PasswordPromptMasker`) and the
//! pseudonyms (C# `PseudonymMasker`).

use super::patterns;
use super::scan::{Budget, Edit, Found, Line, Pattern, apply_edits, split_lines};
use super::{MASK_PREFIX, RedactionError, Run, is_mask_token, mask_token};

type Outcome = Result<String, RedactionError>;

/// Masks the value group of each pattern in turn (C# `RedactionApply.GroupMasker`). A value that
/// is empty or already exactly a mask token is left and not counted.
pub(super) fn mask_groups(
    text: &str,
    patterns: &[&Pattern],
    rule: &'static str,
    run: &mut Run,
    budget: &Budget,
) -> Outcome {
    let token = mask_token(rule);
    let mut current = text.to_owned();
    for pattern in patterns {
        let mut edits = Vec::new();
        for found in pattern.scan(&current, budget)? {
            let Some((start, end)) = found.value else {
                continue;
            };
            if start == end || is_mask_token(&current[start..end]) {
                continue;
            }
            run.count(rule);
            edits.push(Edit {
                start,
                end,
                replacement: token.clone(),
            });
        }
        current = apply_edits(&current, &edits);
    }
    Ok(current)
}

// Private key blocks.

const MARKER_LITERAL: &str = "-----";

const END_MARKER_START: &str = "-----END ";

const AUTHORIZATION_HEADER_NAME: &str = "authorization:";

const BEGIN_MARKER_START: &str = "-----BEGIN ";

const PRIVATE_KEY_MARKER_END: &str = "PRIVATE KEY-----";

const PROC_TYPE_HEADER: &str = "Proc-Type:";

const DEK_INFO_HEADER: &str = "DEK-Info:";

/// PEM bodies wrap at 64 or 76 columns; the final line is at least one base64 quantum.
const MIN_BODY_LINE_LENGTH: usize = 4;

#[derive(Clone, Copy, PartialEq, Eq)]
enum LineKind {
    Other,
    Base64,
    Header,
    Blank,
}

/// Masks PEM private key blocks with a single linear pass over the lines, so a buffer holding
/// many BEGIN lines and no END cannot cost more than one scan. Three shapes are handled: a full
/// block, an END line whose BEGIN was evicted (the text starts mid-key), and a BEGIN without END
/// (the key is cut).
pub(super) fn mask_private_keys(
    text: &str,
    rule: &'static str,
    run: &mut Run,
    budget: &Budget,
) -> Outcome {
    if !text.contains(MARKER_LITERAL) {
        return Ok(text.to_owned());
    }
    let marker = &*patterns::PRIVATE_KEY_MARKER;
    let token = mask_token(rule);
    let lines = split_lines(text);
    let mut edits = Vec::new();
    let mut emit = |edits: &mut Vec<Edit>, start: usize, end: usize| {
        edits.push(Edit {
            start,
            end,
            replacement: token.clone(),
        });
        run.count(rule);
    };
    let mut floor_line = 0;
    let mut index = 0;
    let mut position = 0;

    while index < lines.len() {
        let line = lines[index];
        position = position.max(line.start);
        let hay = &text[position..line.end];
        let Some(first) = marker.find_from(hay, 0, budget)? else {
            index += 1;
            continue;
        };
        let match_start = position + first.start;
        let match_end = position + first.end;
        let prefix_width = match_start - line.start;
        if is_end_marker(hay, &first) {
            let mut start_line = index;
            while start_line > floor_line
                && is_body_kind(
                    classify(text, lines[start_line - 1], prefix_width),
                    false,
                    false,
                )
            {
                start_line -= 1;
            }
            let start = if start_line < index {
                lines[start_line].start
            } else {
                match_start
            };
            emit(&mut edits, start, match_end);
            floor_line = index;
            position = match_end;
            continue;
        }

        if let Some(next) = marker.find_from(hay, first.end, budget)?
            && is_end_marker(hay, &next)
        {
            let same_line_end = position + next.end;
            emit(&mut edits, match_start, same_line_end);
            position = same_line_end;
            continue;
        }

        let mut block_end = line.end;
        let mut closed = false;
        let mut after_header = false;
        let mut scan = index + 1;
        while scan < lines.len() {
            let candidate = lines[scan];
            let kind = body_line_kind(text, &lines, scan, prefix_width);
            // A blank line inside the block is part of it when the body goes on after it.
            if kind == LineKind::Blank
                && !after_header
                && let Some(resume) = next_base64_after_blanks(text, &lines, scan, prefix_width)
            {
                after_header = false;
                scan = resume;
                continue;
            }
            if is_body_kind(kind, true, after_header) {
                after_header = kind == LineKind::Header;
                block_end = candidate.end;
                scan += 1;
                continue;
            }
            let candidate_hay = &text[candidate.start..candidate.end];
            if let Some(end_match) = marker.find_from(candidate_hay, 0, budget)?
                && is_end_marker(candidate_hay, &end_match)
            {
                block_end = candidate.start + end_match.end;
                closed = true;
            }
            break;
        }
        emit(&mut edits, match_start, block_end);
        floor_line = scan;
        index = scan;
        position = if closed { block_end } else { 0 };
    }
    Ok(apply_edits(text, &edits))
}

/// The kind of the line at `index` in the forward walk of a key block. The last line of the text
/// may be a base64 fragment cut by the end of the buffer.
fn body_line_kind(text: &str, lines: &[Line], index: usize, prefix_width: usize) -> LineKind {
    let line = lines[index];
    let kind = classify(text, line, prefix_width);
    if kind == LineKind::Other && index + 1 == lines.len() && is_truncated_fragment(text, line) {
        LineKind::Base64
    } else {
        kind
    }
}

/// The index of the first line after the blank ones from `from` on, when that line is base64.
fn next_base64_after_blanks(
    text: &str,
    lines: &[Line],
    from: usize,
    prefix_width: usize,
) -> Option<usize> {
    let mut next = from;
    while next < lines.len() && classify(text, lines[next], prefix_width) == LineKind::Blank {
        next += 1;
    }
    (next < lines.len() && classify(text, lines[next], prefix_width) == LineKind::Base64)
        .then_some(next)
}

fn is_end_marker(hay: &str, found: &Found) -> bool {
    hay[found.start..].starts_with(END_MARKER_START)
}

fn is_body_kind(kind: LineKind, allow_blank: bool, after_header: bool) -> bool {
    match kind {
        LineKind::Base64 | LineKind::Header => true,
        LineKind::Blank => allow_blank && after_header,
        LineKind::Other => false,
    }
}

/// One to three base64 characters: too short for a body line, but possible as the final line of
/// a key whose end was cut off.
fn is_truncated_fragment(text: &str, line: Line) -> bool {
    let content = text[line.start..line.end].trim();
    !content.is_empty()
        && content.len() < MIN_BODY_LINE_LENGTH
        && content.bytes().all(is_base64_byte)
}

fn is_base64_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'+' || byte == b'/' || byte == b'='
}

/// The line that starts at `start`.
fn line_from(text: &str, start: usize) -> Line {
    let end = text[start..]
        .find('\n')
        .map_or(text.len(), |offset| start + offset);
    let content_end = if end > start && text.as_bytes()[end - 1] == b'\r' {
        end - 1
    } else {
        end
    };
    Line {
        start,
        end: content_end,
    }
}

fn is_private_key_begin(content: &str) -> bool {
    content.contains(BEGIN_MARKER_START) && content.contains(PRIVATE_KEY_MARKER_END)
}

fn is_key_body(kind: LineKind) -> bool {
    matches!(kind, LineKind::Base64 | LineKind::Header | LineKind::Blank)
}

/// True when the text at `cut` is inside a private key whose BEGIN line precedes the cut: the
/// line holding `cut` and the ones before it are body lines up to a BEGIN marker. Used by the
/// input cap, which would otherwise keep the rest of the body with no marker left to recognise.
pub(super) fn cut_inside_private_key(text: &str, cut: usize) -> bool {
    let mut line_start = text[..cut].rfind('\n').map_or(0, |index| index + 1);
    loop {
        let line = line_from(text, line_start);
        if is_private_key_begin(&text[line.start..line.end]) {
            return true;
        }
        if !is_key_body(classify(text, line, 0)) || line_start == 0 {
            return false;
        }
        line_start = text[..line_start - 1]
            .rfind('\n')
            .map_or(0, |index| index + 1);
    }
}

/// The offset of the first line at or after `from` that is not a private key body line.
pub(super) fn skip_private_key_body(text: &str, from: usize) -> usize {
    let mut start = from;
    while start < text.len() {
        let line = line_from(text, start);
        let last_line = !text[start..].contains('\n');
        let in_body = is_key_body(classify(text, line, 0))
            || (last_line && is_truncated_fragment(text, line));
        if !in_body {
            break;
        }
        start = text[start..]
            .find('\n')
            .map_or(text.len(), |offset| start + offset + 1);
    }
    start
}

fn starts_with_blank(text: &str, line: Line) -> bool {
    text[line.start..].starts_with([' ', '\t'])
}

/// True when the text at `cut` is on a continuation line (one that starts with a blank) of an
/// `Authorization:` header that begins before the cut. The cap would drop the header line and
/// keep its continuation, which then has nothing to be recognised by.
pub(super) fn cut_inside_folded_authorization(text: &str, cut: usize) -> bool {
    let mut line_start = text[..cut].rfind('\n').map_or(0, |index| index + 1);
    loop {
        let line = line_from(text, line_start);
        if !starts_with_blank(text, line) {
            return contains_ignore_ascii_case(
                &text[line.start..line.end],
                AUTHORIZATION_HEADER_NAME,
            );
        }
        if line_start == 0 {
            return false;
        }
        line_start = text[..line_start - 1]
            .rfind('\n')
            .map_or(0, |index| index + 1);
    }
}

/// The offset of the first line at or after `from` that does not start with a blank.
pub(super) fn skip_continuation_lines(text: &str, from: usize) -> usize {
    let mut start = from;
    while start < text.len() && starts_with_blank(text, line_from(text, start)) {
        start = text[start..]
            .find('\n')
            .map_or(text.len(), |offset| start + offset + 1);
    }
    start
}

/// The kind of a line of a key block. A line that is not a body line as it stands is tried again
/// without a prefix that a tool puts on every line: a text of the width of the BEGIN line's
/// prefix (a timestamp, a file name), a diff marker (`-`, `+`) or a `cat -n` line number and tab.
fn classify(text: &str, line: Line, prefix_width: usize) -> LineKind {
    let raw = &text[line.start..line.end];
    let kind = classify_content(raw.trim());
    if kind != LineKind::Other {
        return kind;
    }
    let width_stripped = raw
        .get(prefix_width..)
        .filter(|_| prefix_width > 0 && raw.len() > prefix_width);
    let diff_stripped = raw.strip_prefix(['-', '+']);
    let numbered = strip_line_number(raw);
    for stripped in [width_stripped, diff_stripped, numbered]
        .into_iter()
        .flatten()
    {
        let stripped_kind = classify_content(stripped.trim());
        if stripped_kind != LineKind::Other {
            return stripped_kind;
        }
    }
    LineKind::Other
}

/// `     2<tab>rest` of `cat -n`: the rest of the line.
fn strip_line_number(raw: &str) -> Option<&str> {
    let (number, rest) = raw.split_once('\t')?;
    let digits = number.trim_start();
    (!digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit())).then_some(rest)
}

fn classify_content(content: &str) -> LineKind {
    if content.is_empty() {
        return LineKind::Blank;
    }
    if content.starts_with(PROC_TYPE_HEADER) || content.starts_with(DEK_INFO_HEADER) {
        return LineKind::Header;
    }
    if content.len() < MIN_BODY_LINE_LENGTH {
        return LineKind::Other;
    }
    if content.bytes().all(is_base64_byte) {
        LineKind::Base64
    } else {
        LineKind::Other
    }
}

// Echoed password prompts.

/// How many lines after a prompt (or after "Login incorrect") a "command not found" is still
/// attributed to it.
const COMMAND_NOT_FOUND_WINDOW: usize = 3;

const LOGIN_INCORRECT: &str = "login incorrect";

/// Shell and account names that stand alone before a prompt character (`root#`, `bash-5.1$`).
const SHELL_PROMPT_NAMES: [&str; 11] = [
    "root", "sh", "bash", "zsh", "fish", "ksh", "csh", "tcsh", "dash", "ash", "sudo",
];

const SHELL_PROMPT_SUFFIX_CHARACTERS: &str = "$#>%";

/// An explicitly empty value (YAML or env style) holds no secret.
const EMPTY_DOUBLE_QUOTES: &str = "\"\"";

const EMPTY_SINGLE_QUOTES: &str = "''";

/// Masks what a user typed at a password prompt and the terminal echoed. Three cases: the rest of
/// the prompt line when it is not empty; the next line when the prompt line is empty and that
/// line is a single token that does not look like a shell prompt or a message; and the token of
/// a "command not found" line shortly after a prompt (the password typed into a shell).
pub(super) fn mask_password_prompts(
    text: &str,
    rule: &'static str,
    run: &mut Run,
    budget: &Budget,
) -> Outcome {
    let token = mask_token(rule);
    let lines = split_lines(text);
    let mut edits = Vec::new();
    let mut last_trigger: Option<usize> = None;
    let mut pending_next: Option<usize> = None;

    for (i, line) in lines.iter().enumerate() {
        let hay = &text[line.start..line.end];
        if let Some(prompt) = patterns::PASSWORD_PROMPT.find_from(hay, 0, budget)? {
            last_trigger = Some(i);
            let rest_start = skip_blanks(text, line.start + prompt.end, line.end);
            if rest_start >= line.end {
                pending_next = Some(i + 1);
                continue;
            }
            let rest = &text[rest_start..line.end];
            if rest == EMPTY_DOUBLE_QUOTES
                || rest == EMPTY_SINGLE_QUOTES
                || is_block_scalar_indicator(rest)
            {
                continue;
            }
            if !is_mask_token(rest) {
                edits.push(Edit {
                    start: rest_start,
                    end: line.end,
                    replacement: token.clone(),
                });
                // A rest that already holds a token was counted by the rule that masked that part.
                if !rest.contains(MASK_PREFIX) {
                    run.count(rule);
                }
            }
            continue;
        }

        if contains_ignore_ascii_case(hay, LOGIN_INCORRECT) {
            last_trigger = Some(i);
        }

        if pending_next == Some(i)
            && let Some((start, end)) = find_bare_token(text, *line)
        {
            edits.push(Edit {
                start,
                end,
                replacement: token.clone(),
            });
            run.count(rule);
            continue;
        }

        if last_trigger
            .is_some_and(|trigger| i > trigger && i - trigger <= COMMAND_NOT_FOUND_WINDOW)
            && let Some(not_found) = patterns::COMMAND_NOT_FOUND.find_from(hay, 0, budget)?
            && let Some((start, end)) = not_found.value
            && !is_mask_token(&hay[start..end])
        {
            edits.push(Edit {
                start: line.start + start,
                end: line.start + end,
                replacement: token.clone(),
            });
            run.count(rule);
        }
    }
    Ok(apply_edits(text, &edits))
}

/// A YAML block scalar header (`|`, `>-`, `|2`): the value is on the indented lines below, which
/// the key-value rule masks, not an answer typed at a prompt.
fn is_block_scalar_indicator(rest: &str) -> bool {
    let mut chars = rest.chars();
    matches!(chars.next(), Some('|' | '>')) && chars.all(|c| matches!(c, '+' | '-' | '0'..='9'))
}

fn skip_blanks(text: &str, start: usize, end: usize) -> usize {
    let skipped = text.as_bytes()[start..end]
        .iter()
        .take_while(|&&b| b == b' ' || b == b'\t')
        .count();
    start + skipped
}

fn contains_ignore_ascii_case(hay: &str, needle_lowercase: &str) -> bool {
    let needle = needle_lowercase.as_bytes();
    hay.as_bytes()
        .windows(needle.len())
        .any(|window| window.eq_ignore_ascii_case(needle))
}

/// The known messages ("Last login", "Sorry, try again.", "Permission denied", "Login
/// incorrect", "Access denied") all contain a space, so the single-token test already excludes
/// them; what remains to exclude is a bare shell prompt such as `user@host:~$` or `root#`.
/// A token that merely starts like a mask token is still masked: only an exact one is skipped.
fn find_bare_token(text: &str, line: Line) -> Option<(usize, usize)> {
    let start = skip_blanks(text, line.start, line.end);
    let mut end = line.end;
    while end > start && matches!(text.as_bytes()[end - 1], b' ' | b'\t') {
        end -= 1;
    }
    if end <= start {
        return None;
    }
    let token = &text[start..end];
    if token.chars().any(char::is_whitespace)
        || is_mask_token(token)
        || looks_like_shell_prompt(token)
    {
        return None;
    }
    Some((start, end))
}

/// A bare shell prompt: a prompt character alone, after a shell or account name (`root#`,
/// `bash-5.1$`) or after `user@host` with an optional `:path` (`user@host:~$`). Any other token
/// ending in `$`, `#`, `>` or `%` may well be a password (`Tr0ub4dor#`) and is masked.
fn looks_like_shell_prompt(token: &str) -> bool {
    let Some(last) = token.chars().next_back() else {
        return false;
    };
    if !SHELL_PROMPT_SUFFIX_CHARACTERS.contains(last) {
        return false;
    }
    let body = &token[..token.len() - last.len_utf8()];
    body.is_empty() || is_shell_name(body) || is_user_host_prompt(body)
}

fn is_shell_name(body: &str) -> bool {
    let (name, version) = body.split_once('-').unwrap_or((body, ""));
    SHELL_PROMPT_NAMES.contains(&name) && version.bytes().all(|b| b.is_ascii_digit() || b == b'.')
}

fn is_user_host_prompt(body: &str) -> bool {
    let Some((user, rest)) = body.split_once('@') else {
        return false;
    };
    let host = rest.split_once(':').map_or(rest, |(host, _)| host);
    let name_character = |b: u8| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-');
    !user.is_empty()
        && !host.is_empty()
        && user.bytes().all(name_character)
        && host.bytes().all(name_character)
}

// Pseudonyms.

const OCTET_COUNT: usize = 4;

const MAX_OCTET: u32 = 255;

const FULL_OCTET: u32 = 255;

const EXCLUDED_ADDRESSES: [&str; 3] = ["0.0.0.0", "127.0.0.1", "255.255.255.255"];

/// A name ending in one of these is a file name, not a host.
const FILE_EXTENSIONS: [&str; 39] = [
    "dll", "exe", "json", "yaml", "yml", "txt", "log", "cs", "md", "sh", "ps1", "py", "conf",
    "ini", "xml", "html", "gz", "tgz", "bz2", "xz", "zip", "tar", "js", "css", "map", "so", "cfg",
    "bak", "old", "tmp", "pem", "crt", "key", "pub", "csv", "sql", "jar", "war", "whl",
];

/// Replaces IPv4 addresses by stable pseudonyms (`ip-1`, `ip-2`, ...).
pub(super) fn pseudonymize_ipv4(
    text: &str,
    rule: &'static str,
    run: &mut Run,
    budget: &Budget,
) -> Outcome {
    let mut edits = Vec::new();
    for found in patterns::IPV4_ADDRESS.scan(text, budget)? {
        let Some((start, end)) = found.value else {
            continue;
        };
        if !is_maskable_address(&text[start..end]) {
            continue;
        }
        run.count(rule);
        edits.push(Edit {
            start,
            end,
            replacement: run.ip_pseudonym(&text[start..end]),
        });
    }
    Ok(apply_edits(text, &edits))
}

/// Replaces host names (the host of `user@host` forms, then fully qualified names) by stable
/// pseudonyms (`host-1`, `host-2`, ...).
pub(super) fn pseudonymize_hosts(
    text: &str,
    rule: &'static str,
    run: &mut Run,
    budget: &Budget,
) -> Outcome {
    let after_prompt_hosts = mask_hosts(text, &patterns::PROMPT_HOST, false, rule, run, budget)?;
    mask_hosts(
        &after_prompt_hosts,
        &patterns::FULLY_QUALIFIED_NAME,
        true,
        rule,
        run,
        budget,
    )
}

fn mask_hosts(
    text: &str,
    pattern: &Pattern,
    check_extension: bool,
    rule: &'static str,
    run: &mut Run,
    budget: &Budget,
) -> Outcome {
    let mut edits = Vec::new();
    for found in pattern.scan(text, budget)? {
        let Some((start, end)) = found.value else {
            continue;
        };
        let host = &text[start..end];
        if is_pseudonym(host) || (check_extension && has_file_extension(host)) {
            continue;
        }
        run.count(rule);
        edits.push(Edit {
            start,
            end,
            replacement: run.host_pseudonym(host),
        });
    }
    Ok(apply_edits(text, &edits))
}

fn is_pseudonym(value: &str) -> bool {
    is_numbered_pseudonym(value, super::HOST_PSEUDONYM_PREFIX)
        || is_numbered_pseudonym(value, super::IP_PSEUDONYM_PREFIX)
}

fn is_numbered_pseudonym(value: &str, prefix: &str) -> bool {
    value
        .strip_prefix(prefix)
        .is_some_and(|digits| !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()))
}

fn has_file_extension(host: &str) -> bool {
    let last = host.rsplit('.').next().unwrap_or(host);
    FILE_EXTENSIONS
        .iter()
        .any(|extension| last.eq_ignore_ascii_case(extension))
}

fn is_maskable_address(address: &str) -> bool {
    if EXCLUDED_ADDRESSES.contains(&address) {
        return false;
    }
    let mut value: u32 = 0;
    let mut first_octet = 0;
    let mut octets = 0;
    for part in address.split('.') {
        let Ok(octet) = part.parse::<u32>() else {
            return false;
        };
        if octet > MAX_OCTET {
            return false;
        }
        if octets == 0 {
            first_octet = octet;
        }
        value = (value << 8) | octet;
        octets += 1;
    }
    octets == OCTET_COUNT && !is_netmask(first_octet, value)
}

/// 255.x.x.x with contiguous leading ones (255.255.255.0, 255.255.240.0, ...).
fn is_netmask(first_octet: u32, value: u32) -> bool {
    if first_octet != FULL_OCTET {
        return false;
    }
    let inverted = !value;
    inverted & inverted.wrapping_add(1) == 0
}
