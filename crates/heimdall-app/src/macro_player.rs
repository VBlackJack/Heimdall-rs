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

//! Types a macro into a session, as the C# `MacroPlaybackExecutor`: each input after its
//! pause, and, when it expects text first, once the session has shown it since the input
//! before, within its time. The session's text is read as shown: its escape sequences go.

use std::sync::Arc;
use std::time::Duration;

use heimdall_core::macros::{Expect, MacroEntry, OnTimeout};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::sink::InputSink;

/// Most of the session's text kept to look for what is expected: its end.
const SEEN_LIMIT: usize = 64 * 1024;

/// How a macro ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MacroOutcome {
    /// Every input was typed.
    Completed,
    /// The user stopped it.
    Stopped,
    /// The text input `entry` (from 1) waits for did not come in time, and it stops then.
    TimedOut {
        /// The input, from 1.
        entry: usize,
    },
    /// The session ended.
    Closed,
}

/// What is looked for: text, or a regular expression; one that does not compile is looked
/// for as text.
enum Matcher {
    Text(String),
    Regex(regex::Regex),
}

impl Matcher {
    fn of(expect: &Expect) -> Self {
        if expect.regex
            && let Ok(regex) = regex::Regex::new(&expect.pattern)
        {
            return Self::Regex(regex);
        }
        Self::Text(expect.pattern.clone())
    }

    fn found(&self, seen: &str) -> bool {
        match self {
            Self::Text(text) => seen.contains(text.as_str()),
            Self::Regex(regex) => regex.is_match(seen),
        }
    }
}

/// The session's text shown since the last input typed.
#[derive(Default)]
struct Seen {
    text: String,
    /// An escape sequence cut between two reads: its start, kept for the next.
    pending: Vec<u8>,
}

impl Seen {
    fn push(&mut self, bytes: &[u8]) {
        let mut all = std::mem::take(&mut self.pending);
        all.extend_from_slice(bytes);
        let (shown, rest) = visible(&all);
        self.pending = rest.to_vec();
        self.text.push_str(&String::from_utf8_lossy(&shown));
        if self.text.len() > SEEN_LIMIT {
            let mut cut = self.text.len() - SEEN_LIMIT;
            while !self.text.is_char_boundary(cut) {
                cut += 1;
            }
            self.text.drain(..cut);
        }
    }

    fn clear(&mut self) {
        self.text.clear();
        self.pending.clear();
    }
}

/// `bytes` without their escape sequences, and the start of one cut at their end.
fn visible(bytes: &[u8]) -> (Vec<u8>, &[u8]) {
    const ESC: u8 = 0x1b;
    const BEL: u8 = 0x07;
    let mut shown = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != ESC {
            shown.push(bytes[index]);
            index += 1;
            continue;
        }
        let start = index;
        let Some(&kind) = bytes.get(index + 1) else {
            return (shown, &bytes[start..]);
        };
        index += 2;
        match kind {
            // A control sequence ends with a byte from '@' to '~'.
            b'[' => {
                while index < bytes.len() && !(0x40..=0x7e).contains(&bytes[index]) {
                    index += 1;
                }
                if index == bytes.len() {
                    return (shown, &bytes[start..]);
                }
                index += 1;
            }
            // An operating system command ends with BEL or ESC '\'.
            b']' => loop {
                match bytes.get(index) {
                    None => return (shown, &bytes[start..]),
                    Some(&BEL) => {
                        index += 1;
                        break;
                    }
                    Some(&ESC) if bytes.get(index + 1) == Some(&b'\\') => {
                        index += 2;
                        break;
                    }
                    Some(_) => index += 1,
                }
            },
            _ => {}
        }
    }
    (shown, &[])
}

/// Waits until `matcher` finds what it looks for in what the session shows; forever when
/// the session's text stops coming.
async fn shown(output: &mut mpsc::UnboundedReceiver<Vec<u8>>, seen: &mut Seen, matcher: &Matcher) {
    while !matcher.found(&seen.text) {
        match output.recv().await {
            Some(bytes) => seen.push(&bytes),
            None => std::future::pending().await,
        }
    }
}

/// Takes in what the session showed so far.
fn drain(output: &mut mpsc::UnboundedReceiver<Vec<u8>>, seen: &mut Seen) {
    while let Ok(bytes) = output.try_recv() {
        seen.push(&bytes);
    }
}

/// Types `entries` into `sink`, `output` carrying what the session shows, until done or
/// `stop` is cancelled.
pub async fn play(
    entries: Vec<MacroEntry>,
    sink: Arc<dyn InputSink>,
    mut output: mpsc::UnboundedReceiver<Vec<u8>>,
    stop: CancellationToken,
) -> MacroOutcome {
    let mut seen = Seen::default();
    for (index, entry) in entries.iter().enumerate() {
        if let Some(expect) = &entry.expect {
            let matcher = Matcher::of(expect);
            drain(&mut output, &mut seen);
            let found = tokio::select! {
                biased;
                () = stop.cancelled() => return MacroOutcome::Stopped,
                found = tokio::time::timeout(
                    expect.timeout(),
                    shown(&mut output, &mut seen, &matcher),
                ) => found.is_ok(),
            };
            if !found && expect.on_timeout == OnTimeout::Abort {
                log::info!("macro: input {} waited in vain", index + 1);
                return MacroOutcome::TimedOut { entry: index + 1 };
            }
        }
        tokio::select! {
            biased;
            () = stop.cancelled() => return MacroOutcome::Stopped,
            () = tokio::time::sleep(Duration::from_millis(u64::from(entry.delay_ms))) => {}
        }
        // What the next input waits for comes after this one.
        drain(&mut output, &mut seen);
        seen.clear();
        if !entry.input.is_empty() && sink.write(entry.input.clone().into_bytes()).is_err() {
            return MacroOutcome::Closed;
        }
    }
    MacroOutcome::Completed
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use heimdall_core::macros::{Expect, MacroEntry, OnTimeout};
    use heimdall_ssh::{SessionClosed, TerminalSize};

    use super::*;

    #[derive(Debug, Default)]
    struct Shell {
        typed: Mutex<Vec<u8>>,
    }

    impl InputSink for Shell {
        fn write(&self, bytes: Vec<u8>) -> Result<(), SessionClosed> {
            self.typed.lock().expect("typed").extend(bytes);
            Ok(())
        }

        fn resize(&self, _: TerminalSize) -> Result<(), SessionClosed> {
            Ok(())
        }

        fn close(&self) {}
    }

    fn entry(input: &str) -> MacroEntry {
        MacroEntry {
            input: input.to_owned(),
            delay_ms: 0,
            expect: None,
        }
    }

    fn expecting(input: &str, pattern: &str, regex: bool, on_timeout: OnTimeout) -> MacroEntry {
        MacroEntry {
            expect: Some(Expect {
                pattern: pattern.to_owned(),
                regex,
                timeout_ms: 200,
                on_timeout,
            }),
            ..entry(input)
        }
    }

    #[test]
    fn escape_sequences_are_not_text_even_cut_between_two_reads() {
        let mut seen = Seen::default();
        seen.push(b"\x1b[1;32muser@web\x1b[0m:~$ \x1b]0;title\x07ok\x1b[");
        assert_eq!(seen.text, "user@web:~$ ok");
        seen.push(b"Kdone");
        assert_eq!(seen.text, "user@web:~$ okdone");
    }

    #[tokio::test]
    async fn each_input_waits_for_what_it_expects_then_is_typed() {
        let shell = Arc::new(Shell::default());
        let (sender, output) = mpsc::unbounded_channel();
        let entries = vec![
            entry("sudo -i\r"),
            expecting("secret\r", "password for", false, OnTimeout::Abort),
            expecting("id\r", r"root@\w+:", true, OnTimeout::Abort),
        ];
        let typing = tokio::spawn(play(
            entries,
            shell.clone(),
            output,
            CancellationToken::new(),
        ));
        // What the session shows after each input.
        tokio::time::sleep(Duration::from_millis(20)).await;
        sender
            .send(b"[sudo] password for admin: ".to_vec())
            .expect("send");
        tokio::time::sleep(Duration::from_millis(20)).await;
        sender
            .send(b"\x1b[01;31mroot@web\x1b[0m:~# ".to_vec())
            .expect("send");
        assert_eq!(typing.await.expect("ran"), MacroOutcome::Completed);
        assert_eq!(
            shell.typed.lock().expect("typed").as_slice(),
            b"sudo -i\rsecret\rid\r"
        );
    }

    #[tokio::test]
    async fn text_that_does_not_come_stops_the_macro_or_not_as_it_says() {
        let shell = Arc::new(Shell::default());
        let (_sender, output) = mpsc::unbounded_channel();
        let entries = vec![
            entry("a\r"),
            expecting("b\r", "never", false, OnTimeout::Continue),
            expecting("c\r", "never", false, OnTimeout::Abort),
            entry("d\r"),
        ];
        let ended = play(entries, shell.clone(), output, CancellationToken::new()).await;
        assert_eq!(ended, MacroOutcome::TimedOut { entry: 3 });
        assert_eq!(shell.typed.lock().expect("typed").as_slice(), b"a\rb\r");
    }

    #[tokio::test]
    async fn a_stop_ends_it_where_it_is() {
        let shell = Arc::new(Shell::default());
        let (_sender, output) = mpsc::unbounded_channel();
        let stop = CancellationToken::new();
        stop.cancel();
        let entries = vec![MacroEntry {
            delay_ms: 10_000,
            ..entry("late\r")
        }];
        assert_eq!(
            play(entries, shell.clone(), output, stop).await,
            MacroOutcome::Stopped
        );
        assert!(shell.typed.lock().expect("typed").is_empty());
    }
}
