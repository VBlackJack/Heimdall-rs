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

//! The Extended Clipboard pseudo-encoding of the RFB community specification, as noVNC
//! speaks it: a cut text message with a negative length carries a flags word, the actions in
//! its high byte and the formats in its low 16 bits, then a payload. Text is UTF-8 ending in
//! a NUL, inside a zlib stream of its own for each message, after its length.
//!
//! Only the text format is spoken. Whatever the server announces is bounded before it is
//! kept or inflated: the inflater never produces more than one text's bound.

use std::io::{Read as _, Write as _};

use flate2::Compression;
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;

use super::protocol::MAX_CUT_TEXT;

/// The pseudo-encoding asked for, `0xC0A1E5CE`, noVNC's `pseudoEncodingExtendedClipboard`.
pub(super) const PSEUDO_EXTENDED_CLIPBOARD: i32 = 0xC0A1_E5CE_u32.cast_signed();

/// The text format: bit 0 of the formats.
pub(super) const FORMAT_TEXT: u32 = 1;
/// The formats: the low 16 bits of the flags.
const FORMATS_MASK: u32 = 0x0000_FFFF;
/// How many format bits there are.
const FORMAT_BITS: u32 = 16;
/// The actions: the high byte of the flags.
const ACTIONS_MASK: u32 = 0xFF00_0000;

/// The actions.
pub(super) const ACTION_CAPS: u32 = 1 << 24;
pub(super) const ACTION_REQUEST: u32 = 1 << 25;
pub(super) const ACTION_PEEK: u32 = 1 << 26;
pub(super) const ACTION_NOTIFY: u32 = 1 << 27;
pub(super) const ACTION_PROVIDE: u32 = 1 << 28;

/// The actions this client takes, announced in its caps: all five, as noVNC.
const CLIENT_ACTIONS: u32 =
    ACTION_CAPS | ACTION_REQUEST | ACTION_PEEK | ACTION_NOTIFY | ACTION_PROVIDE;

/// Bytes of the flags word, and of each size that follows it.
const WORD_BYTES: usize = 4;

/// The NUL that ends a text.
const NUL: u8 = 0;

/// Longest extended clipboard message accepted from the server, flags and compressed payload;
/// a longer one is read and dropped. Room for a text at its bound beside other formats the
/// server may put in the same message.
pub const MAX_EXTENDED_CUT_TEXT: usize = 4 * MAX_CUT_TEXT;

/// The server's capabilities, from its caps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ServerCaps {
    /// The actions it takes.
    pub(super) actions: u32,
    /// The longest text it takes unasked, when it speaks the text format.
    pub(super) text_max: Option<u32>,
}

impl ServerCaps {
    /// Whether it takes `action`.
    pub(super) fn takes(self, action: u32) -> bool {
        self.actions & action != 0
    }
}

/// One extended clipboard message from the server.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Incoming {
    /// Its capabilities.
    Caps(ServerCaps),
    /// It asks for the client's clipboard in these formats.
    Request(u32),
    /// It asks which formats the client's clipboard has.
    Peek,
    /// Its clipboard changed and has these formats.
    Notify(u32),
    /// Its clipboard: the text, `None` when the message has none or a text past its bound.
    Provide(Option<String>),
}

/// Parses `message`, the flags and the payload of an extended clipboard message, all of it.
///
/// # Errors
///
/// What is malformed in it.
pub(super) fn parse(message: &[u8]) -> Result<Incoming, String> {
    let (flags, payload) = message
        .split_first_chunk::<WORD_BYTES>()
        .ok_or_else(|| format!("an extended clipboard message of {} bytes", message.len()))?;
    let flags = u32::from_be_bytes(*flags);
    let formats = flags & FORMATS_MASK;
    let actions = flags & ACTIONS_MASK;
    // The caps bit wins over the others, as in noVNC.
    if actions & ACTION_CAPS != 0 {
        return caps(actions, formats, payload).map(Incoming::Caps);
    }
    match actions {
        ACTION_REQUEST => Ok(Incoming::Request(formats)),
        ACTION_PEEK => Ok(Incoming::Peek),
        ACTION_NOTIFY => Ok(Incoming::Notify(formats)),
        ACTION_PROVIDE => provide(formats, payload).map(Incoming::Provide),
        other => Err(format!(
            "an extended clipboard message with actions {other:#010x}"
        )),
    }
}

/// The server's caps: a size for each format it speaks, lowest bit first.
fn caps(actions: u32, formats: u32, payload: &[u8]) -> Result<ServerCaps, String> {
    let mut sizes = payload.as_chunks::<WORD_BYTES>().0.iter();
    let mut text_max = None;
    for bit in 0..FORMAT_BITS {
        let format = 1 << bit;
        if formats & format == 0 {
            continue;
        }
        let size = sizes
            .next()
            .ok_or_else(|| "extended clipboard caps missing a format's size".to_owned())?;
        if format == FORMAT_TEXT {
            text_max = Some(u32::from_be_bytes(*size));
        }
    }
    Ok(ServerCaps { actions, text_max })
}

/// The text of a provide: the payload inflated, a length then the text for each format, the
/// text first. Nothing past the text is inflated.
fn provide(formats: u32, payload: &[u8]) -> Result<Option<String>, String> {
    if formats & FORMAT_TEXT == 0 {
        return Ok(None);
    }
    let malformed = |error: std::io::Error| format!("an extended clipboard provide: {error}");
    let mut inflater = ZlibDecoder::new(payload);
    let mut size = [0; WORD_BYTES];
    inflater.read_exact(&mut size).map_err(malformed)?;
    let size = u32::from_be_bytes(size);
    let Some(size) = usize::try_from(size)
        .ok()
        .filter(|size| *size <= MAX_CUT_TEXT)
    else {
        return Ok(None);
    };
    let mut text = Vec::new();
    inflater
        .take(u64::try_from(size).unwrap_or(u64::MAX))
        .read_to_end(&mut text)
        .map_err(malformed)?;
    if text.len() != size {
        return Err(format!(
            "an extended clipboard text of {} bytes where {size} were announced",
            text.len()
        ));
    }
    if text.last() == Some(&NUL) {
        text.pop();
    }
    let text = String::from_utf8(text)
        .map_err(|_| "an extended clipboard text that is not UTF-8".to_owned())?;
    Ok(Some(text.replace("\r\n", "\n")))
}

/// The flags word of `actions` with `formats`.
fn flags(actions: u32, formats: u32) -> [u8; WORD_BYTES] {
    (actions | (formats & FORMATS_MASK)).to_be_bytes()
}

/// The client's caps: its actions, and the text format with the longest text it takes.
pub(super) fn client_caps() -> Vec<u8> {
    let mut message = flags(CLIENT_ACTIONS, FORMAT_TEXT).to_vec();
    message.extend_from_slice(
        &u32::try_from(MAX_CUT_TEXT)
            .unwrap_or(u32::MAX)
            .to_be_bytes(),
    );
    message
}

/// A notify of `formats`.
pub(super) fn notify(formats: u32) -> Vec<u8> {
    flags(ACTION_NOTIFY, formats).to_vec()
}

/// A request for `formats`.
pub(super) fn request(formats: u32) -> Vec<u8> {
    flags(ACTION_REQUEST, formats).to_vec()
}

/// `text` as it is provided before compression: every line ending made CR LF as the
/// specification has it, then UTF-8 and the NUL.
pub(super) fn wire_text(text: &str) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(text.len() + 1);
    let mut characters = text.chars().peekable();
    while let Some(character) = characters.next() {
        match character {
            '\r' => {
                characters.next_if_eq(&'\n');
                bytes.extend_from_slice(b"\r\n");
            }
            '\n' => bytes.extend_from_slice(b"\r\n"),
            other => {
                let mut buffer = [0; 4];
                bytes.extend_from_slice(other.encode_utf8(&mut buffer).as_bytes());
            }
        }
    }
    bytes.push(NUL);
    bytes
}

/// A provide of the text `wire`, from [`wire_text`]: its length and it, compressed; `None`
/// when it is too long to be said.
pub(super) fn provide_message(wire: &[u8]) -> Option<Vec<u8>> {
    let size = u32::try_from(wire.len()).ok()?;
    let mut deflater = ZlibEncoder::new(
        flags(ACTION_PROVIDE, FORMAT_TEXT).to_vec(),
        Compression::default(),
    );
    // Writing into memory does not fail.
    deflater.write_all(&size.to_be_bytes()).ok()?;
    deflater.write_all(wire).ok()?;
    deflater.finish().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_endings_become_cr_lf_once() {
        assert_eq!(wire_text("a\nb\r\nc\rd"), b"a\r\nb\r\nc\r\nd\0");
        assert_eq!(wire_text(""), b"\0");
    }

    #[test]
    fn caps_short_of_a_size_are_malformed() {
        let message = flags(ACTION_CAPS, FORMAT_TEXT | 0b10);
        assert!(parse(&message).is_err());
        let mut message = message.to_vec();
        message.extend_from_slice(&[0, 0, 0, 9, 0, 0, 0, 7]);
        assert_eq!(
            parse(&message),
            Ok(Incoming::Caps(ServerCaps {
                actions: ACTION_CAPS,
                text_max: Some(9)
            }))
        );
    }
}
