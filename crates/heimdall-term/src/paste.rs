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

//! Pasted text to bytes.
//!
//! Line endings become one carriage return each, as Enter sends. Control characters are
//! removed, tab and carriage return excepted: that includes ESC, so a pasted `ESC[201~`
//! cannot close a bracketed paste early and have the rest run as typed commands. DEL and
//! the C1 range go too. Form feeds in pasted source text are lost; that is the price.

use crate::mode::InputMode;

const PASTE_START: &[u8] = b"\x1b[200~";
const PASTE_END: &[u8] = b"\x1b[201~";

/// Bytes sending `text` as a paste.
#[must_use]
pub fn encode_paste(text: &str, mode: &InputMode) -> Vec<u8> {
    let mut cleaned = String::with_capacity(text.len());
    let mut characters = text.chars().peekable();
    while let Some(character) = characters.next() {
        match character {
            '\r' => {
                if characters.peek() == Some(&'\n') {
                    characters.next();
                }
                cleaned.push('\r');
            }
            '\n' => cleaned.push('\r'),
            '\t' => cleaned.push('\t'),
            other if other.is_control() => {}
            other => cleaned.push(other),
        }
    }
    if !mode.bracketed_paste {
        return cleaned.into_bytes();
    }
    let mut out = Vec::with_capacity(cleaned.len() + PASTE_START.len() + PASTE_END.len());
    out.extend_from_slice(PASTE_START);
    out.extend_from_slice(cleaned.as_bytes());
    out.extend_from_slice(PASTE_END);
    out
}
