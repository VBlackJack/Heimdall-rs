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

//! What a `WinRM` session's first output says went wrong, as the C#
//! `WinRmEarlyOutputDiagnostic` reads it: `PowerShell`'s own error names a code the user
//! cannot act on, and this names the cause.
//!
//! Only the start of the session is read: up to [`MAX_OBSERVED_BYTES`], until the user types,
//! or until the remote prompt shows the session was entered.
//!
//! The C# also recognises an execution policy refusing its sign-in script; Heimdall-rs runs
//! no script (`-Command` only), so that case cannot arise here.

/// Bytes of output read at most, as the C# `DefaultMaxBufferedBytes`.
pub const MAX_OBSERVED_BYTES: usize = 16 * 1024;

/// How far around a code its context is looked for, in characters, as the C#.
const CONTEXT_RADIUS: usize = 512;

/// `SEC_E_TARGET_UNKNOWN`: Negotiate fell back to NTLM for the current identity, refused for
/// this machine itself or a host off the domain.
const NTLM_LOOPBACK_CODE: &str = "0x8009030e";

/// `ERROR_WINHTTP_INVALID_SERVER_RESPONSE`: what answered is no `WinRM` listener, typically
/// HTTPS spoken to an HTTP port, or the reverse, through a tunnel.
const WSMAN_INVALID_RESPONSE_CODE: &str = "12152";

/// A cause recognised in a `WinRM` session's first output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Diagnostic {
    /// The current Windows identity was refused: a stored account is needed for this
    /// machine itself or a host off the domain.
    NtlmLoopback,
    /// What answered did not speak `WSMan`.
    WsmanInvalidResponse,
}

/// Reads a session's first output for a [`Diagnostic`].
#[derive(Debug, Clone, Default)]
pub struct EarlyOutput {
    seen: String,
    bytes: usize,
    done: bool,
}

impl EarlyOutput {
    /// Nothing read yet.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether more output is still read.
    #[must_use]
    pub fn is_active(&self) -> bool {
        !self.done
    }

    /// No more output is read: the user typed, and what follows is theirs.
    pub fn stop(&mut self) {
        self.done = true;
    }

    /// Reads `data`, the next output; the cause found, once.
    pub fn observe(&mut self, data: &[u8]) -> Option<Diagnostic> {
        if self.done || data.is_empty() {
            return None;
        }
        let room = MAX_OBSERVED_BYTES - self.bytes;
        let full = data.len() >= room;
        let taken = &data[..data.len().min(room)];
        self.seen.push_str(&String::from_utf8_lossy(taken));
        self.bytes += taken.len();
        // The error first: one chunk can hold the error and the prompt printed after it.
        let found = diagnostic(&self.seen);
        if found.is_some() || full || has_remote_prompt(&self.seen) {
            self.done = true;
        }
        found
    }
}

/// The cause `output` names, as the C# looks for it.
fn diagnostic(output: &str) -> Option<Diagnostic> {
    let lower = output.to_ascii_lowercase();
    if let Some(at) = lower.find(NTLM_LOOPBACK_CODE)
        && winrm_context_near(&lower, at, NTLM_LOOPBACK_CODE.len())
    {
        return Some(Diagnostic::NtlmLoopback);
    }
    if lower.contains(WSMAN_INVALID_RESPONSE_CODE) && wsman_context(&lower) {
        return Some(Diagnostic::WsmanInvalidResponse);
    }
    None
}

/// Whether `lower`, lowercased, says `WinRM` around `at`.
fn winrm_context_near(lower: &str, at: usize, length: usize) -> bool {
    let start = floor_boundary(lower, at.saturating_sub(CONTEXT_RADIUS));
    let end = ceil_boundary(lower, (at + length + CONTEXT_RADIUS).min(lower.len()));
    let near = &lower[start..end];
    wsman_context(near) || near.contains("enter-pssession") || near.contains("new-pssession")
}

/// Whether `lower`, lowercased, names `WSMan` or `WinRM`.
fn wsman_context(lower: &str) -> bool {
    lower.contains("wsman") || lower.contains("ws-man") || lower.contains("winrm")
}

/// The prompt of an entered remote session, `[host]: PS path>`, the colon possibly after a
/// space as localised hosts write it. A bare local `PS path>` is not one: it is what follows
/// a failed `Enter-PSSession`, right under the error.
fn has_remote_prompt(output: &str) -> bool {
    output.lines().any(|line| {
        let mut rest = line;
        while let Some(open) = rest.find('[') {
            let after = &rest[open + 1..];
            let Some(close) = after.find(']') else {
                return false;
            };
            if close > 0 {
                let tail = after[close + 1..].trim_start_matches(' ');
                if let Some(tail) = tail.strip_prefix(':')
                    && let Some(path) = tail.trim_start_matches(' ').strip_prefix("PS ")
                    && path.contains('>')
                {
                    return true;
                }
            }
            rest = &after[close + 1..];
        }
        false
    })
}

fn floor_boundary(text: &str, mut index: usize) -> usize {
    while !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

fn ceil_boundary(text: &str, mut index: usize) -> usize {
    while !text.is_char_boundary(index) {
        index += 1;
    }
    index
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ntlm_loopback_code_near_winrm_names_its_cause() {
        let mut early = EarlyOutput::new();
        let error = "Enter-PSSession : Connecting to remote server localhost failed with the \
                     following error message : WinRM cannot process the request. Error \
                     code 0x8009030e occurred.\r\nPS C:\\Users\\me> ";
        assert_eq!(
            early.observe(error.as_bytes()),
            Some(Diagnostic::NtlmLoopback)
        );
        assert!(!early.is_active(), "said once");
        assert_eq!(early.observe(error.as_bytes()), None);
    }

    #[test]
    fn the_code_alone_far_from_any_winrm_word_is_not_taken() {
        let mut early = EarlyOutput::new();
        let far = format!("0x8009030e{}", " ".repeat(CONTEXT_RADIUS + 10));
        assert_eq!(early.observe(far.as_bytes()), None);
        assert_eq!(early.observe(b"WinRM"), None, "too far from the code");
    }

    #[test]
    fn an_invalid_wsman_response_is_named_split_across_chunks() {
        let mut early = EarlyOutput::new();
        assert_eq!(
            early.observe(b"Enter-PSSession : The WinRM client received an "),
            None
        );
        assert_eq!(
            early.observe(b"HTTP status code of 12152 from the remote WS-Management service."),
            Some(Diagnostic::WsmanInvalidResponse)
        );
    }

    #[test]
    fn the_remote_prompt_ends_the_reading_and_the_local_one_does_not() {
        let mut early = EarlyOutput::new();
        assert_eq!(early.observe(b"PS C:\\Users\\me> "), None);
        assert!(early.is_active(), "a local prompt follows a failure");
        assert_eq!(
            early.observe(b"\r\n[dc01.lab]: PS C:\\Users\\admin\\Documents> "),
            None
        );
        assert!(!early.is_active(), "entered");
        let mut french = EarlyOutput::new();
        let _ = french.observe(b"[dc01.lab] : PS C:\\> ");
        assert!(!french.is_active(), "the localised spacing");
    }

    #[test]
    fn typing_or_too_much_output_ends_the_reading() {
        let mut early = EarlyOutput::new();
        early.stop();
        assert_eq!(early.observe(b"WinRM 12152"), None);
        let mut flooded = EarlyOutput::new();
        assert_eq!(flooded.observe(&vec![b'x'; MAX_OBSERVED_BYTES]), None);
        assert!(!flooded.is_active());
        assert_eq!(flooded.observe(b"WinRM 12152"), None);
    }

    #[test]
    fn text_cut_inside_a_character_is_read_without_a_panic() {
        let mut early = EarlyOutput::new();
        let mut text = "é".repeat(CONTEXT_RADIUS);
        text.push_str("0x8009030e WinRM");
        assert_eq!(
            early.observe(text.as_bytes()),
            Some(Diagnostic::NtlmLoopback)
        );
    }
}
