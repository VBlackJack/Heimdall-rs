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

//! A session's output as plain text, for its transcript: decoded as UTF-8 and its escape
//! sequences left out, as the C# Heimdall's `StreamingAnsiStripper` does, a character or a
//! sequence cut between two chunks completed by the next.

/// Escape, which opens every sequence.
const ESCAPE: char = '\u{1b}';

/// Bell, which ends an operating system command.
const BELL: char = '\u{07}';

/// Where the stripper is in a sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum State {
    /// In text.
    #[default]
    Text,
    /// Just after ESC.
    Escape,
    /// In a control sequence, ESC [.
    Csi {
        /// An intermediate byte was seen: no parameter may follow.
        intermediate: bool,
    },
    /// In an escape sequence with intermediate bytes (nF).
    Intermediate,
    /// In a string: OSC, DCS, SOS, PM or APC, ended by BEL or ESC \.
    String,
    /// In a string, just after ESC.
    StringEscape,
}

/// What a character is to the stripper.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Step {
    /// Text, to keep.
    Text,
    /// Part of a sequence, dropped.
    Sequence,
    /// Not able to continue the sequence begun.
    Rejected,
}

/// Plain text out of a session's output, fed chunk by chunk.
#[derive(Debug, Default)]
pub struct PlainText {
    /// The end of the last chunk, an unfinished UTF-8 character.
    bytes: Vec<u8>,
    state: State,
    /// The sequence read so far, given back as text if it turns out to be none.
    sequence: String,
}

impl PlainText {
    /// A stripper in text, holding nothing.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The text of `bytes`, sequences left out; what is cut at its end waits for the next.
    pub fn feed(&mut self, bytes: &[u8]) -> String {
        let decoded = self.decode(bytes);
        let mut text = String::with_capacity(decoded.len());
        for c in decoded.chars() {
            self.strip(c, &mut text);
        }
        text
    }

    /// What is left at the end: an unfinished character as U+FFFD; an unfinished sequence
    /// is dropped, as the C# stripper drops it.
    pub fn finish(&mut self) -> String {
        let mut text = String::new();
        if !self.bytes.is_empty() {
            self.bytes.clear();
            self.strip(char::REPLACEMENT_CHARACTER, &mut text);
        }
        self.state = State::Text;
        self.sequence.clear();
        text
    }

    /// `bytes` after what was held, as text; an unfinished character at the end is held.
    fn decode(&mut self, bytes: &[u8]) -> String {
        self.bytes.extend_from_slice(bytes);
        let mut text = String::with_capacity(self.bytes.len());
        let mut rest = self.bytes.as_slice();
        loop {
            match std::str::from_utf8(rest) {
                Ok(valid) => {
                    text.push_str(valid);
                    rest = &[];
                    break;
                }
                Err(error) => {
                    let (valid, after) = rest.split_at(error.valid_up_to());
                    text.push_str(&String::from_utf8_lossy(valid));
                    let Some(length) = error.error_len() else {
                        // A character cut at the end: kept for the next chunk.
                        rest = after;
                        break;
                    };
                    text.push(char::REPLACEMENT_CHARACTER);
                    rest = &after[length..];
                }
            }
        }
        self.bytes = rest.to_vec();
        text
    }

    /// Reads `c`, adding to `text` what is not part of a sequence.
    fn strip(&mut self, c: char, text: &mut String) {
        let mut step = self.step(c);
        if step == Step::Rejected {
            // Not a sequence after all: what was read is text, and `c` is read again.
            text.push_str(&self.sequence);
            self.end_sequence();
            step = self.step(c);
        }
        if step == Step::Text {
            text.push(c);
        }
    }

    /// Moves the state on by `c`.
    fn step(&mut self, c: char) -> Step {
        match self.state {
            State::Text => {
                if c != ESCAPE {
                    return Step::Text;
                }
                self.enter(c, State::Escape);
            }
            State::Escape => {
                if c == '[' {
                    self.enter(
                        c,
                        State::Csi {
                            intermediate: false,
                        },
                    );
                } else if matches!(c, 'P' | ']' | 'X' | '^' | '_') {
                    self.enter(c, State::String);
                } else if is_intermediate(c) {
                    self.enter(c, State::Intermediate);
                } else if is_escape_final(c) {
                    self.end_sequence();
                } else {
                    return Step::Rejected;
                }
            }
            State::Csi { intermediate } => {
                if is_parameter(c) && !intermediate {
                    self.sequence.push(c);
                } else if is_intermediate(c) {
                    self.enter(c, State::Csi { intermediate: true });
                } else if is_csi_final(c) {
                    self.end_sequence();
                } else {
                    return Step::Rejected;
                }
            }
            State::Intermediate => {
                if is_intermediate(c) {
                    self.sequence.push(c);
                } else if is_escape_final(c) {
                    self.end_sequence();
                } else {
                    return Step::Rejected;
                }
            }
            State::String => {
                if c == BELL {
                    self.end_sequence();
                } else {
                    self.sequence.push(c);
                    if c == ESCAPE {
                        self.state = State::StringEscape;
                    }
                }
            }
            State::StringEscape => {
                // The string ends; the ESC begins what comes next. ESC \ is the string
                // terminator, whose \ is a final byte that ends that sequence too.
                self.sequence.clear();
                self.enter(ESCAPE, State::Escape);
                return self.step(c);
            }
        }
        Step::Sequence
    }

    fn enter(&mut self, c: char, state: State) {
        self.sequence.push(c);
        self.state = state;
    }

    fn end_sequence(&mut self) {
        self.sequence.clear();
        self.state = State::Text;
    }
}

/// An intermediate byte, 0x20 to 0x2F.
fn is_intermediate(c: char) -> bool {
    ('\u{20}'..='\u{2f}').contains(&c)
}

/// The final byte of a two-character escape, 0x30 to 0x7E.
fn is_escape_final(c: char) -> bool {
    ('\u{30}'..='\u{7e}').contains(&c)
}

/// A parameter byte of a control sequence, 0x30 to 0x3F.
fn is_parameter(c: char) -> bool {
    ('\u{30}'..='\u{3f}').contains(&c)
}

/// The final byte of a control sequence, 0x40 to 0x7E.
fn is_csi_final(c: char) -> bool {
    ('\u{40}'..='\u{7e}').contains(&c)
}
