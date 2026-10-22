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

//! Masked copy engine: pure, offline redaction of terminal output, as the C#
//! `Heimdall.Core.Redaction.TextRedactor` (PR #647). Secrets the rules recognise are replaced by
//! `[masked:<rule-id>]` and counted per rule; host names and IPv4 addresses can optionally be
//! replaced by stable pseudonyms (`host-1`, `ip-1`) within one call.
//!
//! Best effort, not a guarantee: the rules are heuristics over text, so a secret of a shape no
//! rule knows is not masked, and a masked copy must still be reviewed by a person before it is
//! shared. Any screen or message built on this engine has to say so. Pseudonyms are consistent
//! within one call only; the mapping lives in memory for that call and is never written to disk,
//! to a log or to the clipboard.
//!
//! The result is idempotent: redacting the output again changes nothing and counts zero.
//!
//! Fail closed: when a rule runs out of its time budget the result is an error and no text is
//! returned, never a partially masked one. The input is a sequence of logical lines separated by
//! `\n`; joining rows the terminal wrapped is the caller's job, and a header or value wrapped
//! without indentation is not recognised as one (an indented, folded header is).
//!
//! No network, nothing written to disk, and neither the input nor a match is ever logged or put
//! in an error.
//!
//! Engine: every pattern runs on the linear-time `regex` crate. The C# patterns use look-behind
//! and look-ahead, which that crate lacks; each is replaced by a check on the characters around
//! the match (see `scan::Guard`), so `fancy-regex` is not needed.

mod maskers;
mod patterns;
mod scan;

#[cfg(test)]
mod tests;

use std::time::Duration;

use scan::Budget;

/// Most recent bytes of the session text that are redacted; older output is dropped (the C#
/// terminal bridge caps the text it hands over at the same size).
pub const MAX_INPUT_BYTES: usize = 4 * 1024 * 1024;

/// Time one rule may take on the capped input (C# `RedactionRule.MatchTimeoutMilliseconds`).
pub const RULE_TIME_BUDGET: Duration = Duration::from_millis(2000);

/// The start of every mask token.
pub const MASK_PREFIX: &str = "[masked:";

/// The end of every mask token.
pub const MASK_SUFFIX: &str = "]";

/// Prefix of an IPv4 pseudonym.
pub const IP_PSEUDONYM_PREFIX: &str = "ip-";

/// Prefix of a host name pseudonym.
pub const HOST_PSEUDONYM_PREFIX: &str = "host-";

/// Stable rule identifiers. They appear inside mask tokens and in the counts, so renaming one is
/// a format change: they are pinned by tests.
pub mod rule_id {
    pub const PRIVATE_KEY_BLOCK: &str = "private-key-block";
    pub const AUTHORIZATION_HEADER: &str = "authorization-header";
    pub const COOKIE_HEADER: &str = "cookie-header";
    pub const URL_CREDENTIALS: &str = "url-credentials";
    pub const CLI_PASSWORD: &str = "cli-password";
    pub const KEY_VALUE_SECRET: &str = "key-value-secret";
    pub const AWS_ACCESS_KEY: &str = "aws-access-key";
    pub const GITHUB_TOKEN: &str = "github-token";
    pub const SLACK_TOKEN: &str = "slack-token";
    pub const JWT: &str = "jwt";
    pub const PASSWORD_PROMPT: &str = "password-prompt";
    pub const IPV4_ADDRESS: &str = "ipv4-address";
    pub const HOST_NAME: &str = "host-name";
}

/// What a redaction is asked to do.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RedactionOptions {
    /// Replace IPv4 addresses and host names by stable pseudonyms within one call. Off by default.
    pub pseudonymize_hosts: bool,
}

/// Why nothing was returned.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum RedactionError {
    /// A rule ran out of its time budget: the caller must not copy anything (fail closed).
    #[error("redaction rule {rule} ran out of time")]
    Timeout {
        /// The id of the rule that ran out of time.
        rule: &'static str,
    },
}

/// The outcome of one redaction.
#[derive(Clone, PartialEq, Eq)]
pub struct Redacted {
    /// The masked text. Best effort: it can still hold a secret no rule recognised.
    pub text: String,
    /// Number of masked occurrences per rule, in catalogue order; every rule is present, zero
    /// allowed.
    pub counts: Vec<(&'static str, usize)>,
    /// True when the input was longer than [`MAX_INPUT_BYTES`] and its oldest part was dropped.
    pub truncated: bool,
}

/// Debug never prints the text: it can hold what the rules missed.
impl std::fmt::Debug for Redacted {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Redacted")
            .field("text_bytes", &self.text.len())
            .field("counts", &self.counts)
            .field("truncated", &self.truncated)
            .finish()
    }
}

impl Redacted {
    /// The count of one rule, zero for an unknown id.
    #[must_use]
    pub fn count(&self, rule: &str) -> usize {
        self.counts
            .iter()
            .find(|(id, _)| *id == rule)
            .map_or(0, |(_, n)| *n)
    }

    /// The count of every rule together.
    #[must_use]
    pub fn total(&self) -> usize {
        self.counts.iter().map(|(_, n)| n).sum()
    }
}

type Apply = fn(&str, &mut Run, &Budget) -> Result<String, RedactionError>;

struct Rule {
    id: &'static str,
    /// Runs only when [`RedactionOptions::pseudonymize_hosts`] is set.
    requires_pseudonymization: bool,
    apply: Apply,
}

/// The shipped rules, in application order. Order matters: a more specific rule runs before a
/// general one so a span is masked, and counted, once (a bearer JWT is the authorization
/// header's, not the jwt rule's). The password prompt rule runs before key-value-secret so an
/// echoed "Password: x" is the prompt rule's.
static CATALOGUE: [Rule; 13] = [
    Rule {
        id: rule_id::PRIVATE_KEY_BLOCK,
        requires_pseudonymization: false,
        apply: |text, run, budget| {
            maskers::mask_private_keys(text, rule_id::PRIVATE_KEY_BLOCK, run, budget)
        },
    },
    Rule {
        id: rule_id::AUTHORIZATION_HEADER,
        requires_pseudonymization: false,
        apply: |text, run, budget| {
            maskers::mask_groups(
                text,
                &[
                    &patterns::AUTHORIZATION_HEADER,
                    &patterns::AUTHORIZATION_OTHER,
                    &patterns::AUTHORIZATION_BARE,
                ],
                rule_id::AUTHORIZATION_HEADER,
                run,
                budget,
            )
        },
    },
    Rule {
        id: rule_id::COOKIE_HEADER,
        requires_pseudonymization: false,
        apply: |text, run, budget| maskers::mask_cookies(text, rule_id::COOKIE_HEADER, run, budget),
    },
    Rule {
        id: rule_id::URL_CREDENTIALS,
        requires_pseudonymization: false,
        apply: |text, run, budget| {
            maskers::mask_groups(
                text,
                &[&patterns::URL_CREDENTIALS, &patterns::URL_USER_TOKEN],
                rule_id::URL_CREDENTIALS,
                run,
                budget,
            )
        },
    },
    Rule {
        id: rule_id::CLI_PASSWORD,
        requires_pseudonymization: false,
        apply: |text, run, budget| {
            maskers::mask_groups(
                text,
                &[
                    &patterns::CLI_PASSWORD_OPTION,
                    &patterns::CLI_SSHPASS,
                    &patterns::CLI_EXPORT,
                    &patterns::CLI_ENVIRONMENT_PASSWORD,
                    &patterns::CLI_MYSQL_PASSWORD,
                ],
                rule_id::CLI_PASSWORD,
                run,
                budget,
            )
        },
    },
    Rule {
        id: rule_id::PASSWORD_PROMPT,
        requires_pseudonymization: false,
        apply: |text, run, budget| {
            maskers::mask_password_prompts(text, rule_id::PASSWORD_PROMPT, run, budget)
        },
    },
    Rule {
        id: rule_id::KEY_VALUE_SECRET,
        requires_pseudonymization: false,
        apply: |text, run, budget| {
            maskers::mask_groups(
                text,
                &[
                    &patterns::KEY_VALUE_BLOCK_SCALAR,
                    &patterns::KEY_VALUE_SECRET_COLON,
                    &patterns::KEY_VALUE_SECRET_COLON_TOKEN,
                    &patterns::KEY_VALUE_SECRET,
                    &patterns::XML_ELEMENT_SECRET,
                    &patterns::ODBC_PASSWORD,
                    &patterns::NAME_VALUE_PAIR_SECRET,
                ],
                rule_id::KEY_VALUE_SECRET,
                run,
                budget,
            )
        },
    },
    Rule {
        id: rule_id::AWS_ACCESS_KEY,
        requires_pseudonymization: false,
        apply: |text, run, budget| {
            maskers::mask_groups(
                text,
                &[&patterns::AWS_ACCESS_KEY],
                rule_id::AWS_ACCESS_KEY,
                run,
                budget,
            )
        },
    },
    Rule {
        id: rule_id::GITHUB_TOKEN,
        requires_pseudonymization: false,
        apply: |text, run, budget| {
            maskers::mask_groups(
                text,
                &[&patterns::GITHUB_TOKEN],
                rule_id::GITHUB_TOKEN,
                run,
                budget,
            )
        },
    },
    Rule {
        id: rule_id::SLACK_TOKEN,
        requires_pseudonymization: false,
        apply: |text, run, budget| {
            maskers::mask_groups(
                text,
                &[&patterns::SLACK_TOKEN],
                rule_id::SLACK_TOKEN,
                run,
                budget,
            )
        },
    },
    Rule {
        id: rule_id::JWT,
        requires_pseudonymization: false,
        apply: |text, run, budget| {
            maskers::mask_groups(text, &[&patterns::JWT], rule_id::JWT, run, budget)
        },
    },
    Rule {
        id: rule_id::IPV4_ADDRESS,
        requires_pseudonymization: true,
        apply: |text, run, budget| {
            maskers::pseudonymize_ipv4(text, rule_id::IPV4_ADDRESS, run, budget)
        },
    },
    Rule {
        id: rule_id::HOST_NAME,
        requires_pseudonymization: true,
        apply: |text, run, budget| {
            maskers::pseudonymize_hosts(text, rule_id::HOST_NAME, run, budget)
        },
    },
];

/// The ids of the shipped rules, in application order.
#[must_use]
pub fn rule_ids() -> Vec<&'static str> {
    CATALOGUE.iter().map(|rule| rule.id).collect()
}

/// The mask token written in place of a secret: `[masked:<rule-id>]`.
#[must_use]
pub fn mask_token(rule: &str) -> String {
    format!("{MASK_PREFIX}{rule}{MASK_SUFFIX}")
}

/// True when the whole text is exactly one mask token of a shipped rule. Stricter than the C#
/// (starts with the prefix and ends with the suffix): a value such as `[masked:x]hunter2]` is a
/// secret wearing a token's outline, so it is masked, not skipped.
fn is_mask_token(value: &str) -> bool {
    value
        .strip_prefix(MASK_PREFIX)
        .and_then(|rest| rest.strip_suffix(MASK_SUFFIX))
        .is_some_and(|id| CATALOGUE.iter().any(|rule| rule.id == id))
}

/// Mutable state of one call: counts and pseudonym tables.
struct Run {
    counts: Vec<(&'static str, usize)>,
    ip_pseudonyms: std::collections::HashMap<String, String>,
    host_pseudonyms: std::collections::HashMap<String, String>,
}

impl Run {
    fn new() -> Self {
        Self {
            counts: CATALOGUE.iter().map(|rule| (rule.id, 0)).collect(),
            ip_pseudonyms: std::collections::HashMap::new(),
            host_pseudonyms: std::collections::HashMap::new(),
        }
    }

    fn count(&mut self, rule: &str) {
        if let Some((_, n)) = self.counts.iter_mut().find(|(id, _)| *id == rule) {
            *n += 1;
        }
    }

    fn ip_pseudonym(&mut self, address: &str) -> String {
        pseudonym(&mut self.ip_pseudonyms, address, IP_PSEUDONYM_PREFIX)
    }

    /// Host names compare without regard to case (C# `StringComparer.OrdinalIgnoreCase`).
    fn host_pseudonym(&mut self, host: &str) -> String {
        pseudonym(
            &mut self.host_pseudonyms,
            &host.to_lowercase(),
            HOST_PSEUDONYM_PREFIX,
        )
    }
}

fn pseudonym(
    table: &mut std::collections::HashMap<String, String>,
    value: &str,
    prefix: &str,
) -> String {
    let next = table.len() + 1;
    table
        .entry(value.to_owned())
        .or_insert_with(|| format!("{prefix}{next}"))
        .clone()
}

/// Lines dropped after the cut besides the partial first one. A rule looks back at most this
/// many lines (the "command not found" window after a password prompt), so a line whose prompt
/// was cut away would otherwise be kept unmasked.
const CUT_CONTEXT_LINES: usize = 4;

/// The last [`MAX_INPUT_BYTES`] of the text, starting on a line: a cut inside a UTF-8 sequence
/// moves forward to the next character, the partial first line is dropped, as the C# terminal
/// bridge does, and so are the next [`CUT_CONTEXT_LINES`] lines, whose masking depends on lines
/// that are gone (`Password:` cut away, its echoed answer on the line after).
fn cap_input(text: &str, max_bytes: usize) -> (&str, bool) {
    if text.len() <= max_bytes {
        return (text, false);
    }
    let mut start = text.len() - max_bytes;
    while !text.is_char_boundary(start) {
        start += 1;
    }
    if !text[..start].ends_with('\n') {
        start = after_next_newline(text, start);
    }
    // The cut can fall inside a private key: its BEGIN line is gone, so the body that remains
    // would have no marker to be recognised by. Drop the rest of the body.
    // The same holds for the continuation lines of a folded Authorization header.
    let inside_key = maskers::cut_inside_private_key(text, start);
    let inside_header = maskers::cut_inside_folded_authorization(text, start);
    for _ in 0..CUT_CONTEXT_LINES {
        start = after_next_newline(text, start);
    }
    if inside_key {
        start = maskers::skip_private_key_body(text, start);
    }
    if inside_header {
        start = maskers::skip_continuation_lines(text, start);
    }
    (&text[start..], true)
}

/// The offset just after the next line feed at or after `from`, or the end of the text.
fn after_next_newline(text: &str, from: usize) -> usize {
    text[from..]
        .find('\n')
        .map_or(text.len(), |offset| from + offset + 1)
}

/// Masks the secrets of the session text that the rules recognise. Best effort: heuristic, so a
/// secret of an unknown shape stays in the output and the result must be reviewed before it is
/// shared.
///
/// # Errors
///
/// [`RedactionError::Timeout`] when a rule runs out of its time budget; nothing is returned
/// then, not even the part already masked.
pub fn redact(text: &str, options: RedactionOptions) -> Result<Redacted, RedactionError> {
    redact_with_limits(text, options, MAX_INPUT_BYTES, RULE_TIME_BUDGET)
}

fn redact_with_limits(
    text: &str,
    options: RedactionOptions,
    max_bytes: usize,
    rule_budget: Duration,
) -> Result<Redacted, RedactionError> {
    patterns::warm_up();
    let (capped, truncated) = cap_input(text, max_bytes);
    let mut run = Run::new();
    let mut current = capped.to_owned();
    for rule in &CATALOGUE {
        if rule.requires_pseudonymization && !options.pseudonymize_hosts {
            continue;
        }
        let budget = Budget::new(rule.id, rule_budget);
        current = (rule.apply)(&current, &mut run, &budget)?;
        // A rule that used up its time on its last step must not be accepted either.
        budget.check()?;
    }
    // A mask token can be longer than the secret it replaces, so the output may exceed the cap.
    // Cap it too: a second pass over the output then finds nothing to cut and changes nothing.
    let (output, output_cut) = cap_input(&current, max_bytes);
    Ok(Redacted {
        text: output.to_owned(),
        counts: run.counts,
        truncated: truncated || output_cut,
    })
}
