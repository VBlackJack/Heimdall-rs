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

//! The Telnet protocol without input or output: bytes from the server in, terminal data and
//! replies out; terminal input in, bytes for the server out.
//!
//! Options follow RFC 1143 without ever initiating a negotiation: the client only answers.
//! It accepts a request only when it changes the option's state, and refuses an option once
//! per connection, so a server that keeps asking cannot draw it into a loop.

/// Interpret As Command.
const IAC: u8 = 255;
const DONT: u8 = 254;
const DO: u8 = 253;
const WONT: u8 = 252;
const WILL: u8 = 251;
/// Subnegotiation begin.
const SB: u8 = 250;
/// Subnegotiation end.
const SE: u8 = 240;

/// RFC 856: 8-bit data, no CR padding.
const BINARY: u8 = 0;
/// RFC 857: the server echoes what is typed.
const ECHO: u8 = 1;
/// RFC 858: no go-ahead signals.
const SUPPRESS_GO_AHEAD: u8 = 3;
/// RFC 1091: the terminal's type.
const TERMINAL_TYPE: u8 = 24;
/// RFC 1073: the window's size.
const WINDOW_SIZE: u8 = 31;

/// Terminal-type subnegotiation: "here is my type".
const TERMINAL_TYPE_IS: u8 = 0;
/// Terminal-type subnegotiation: "send your type".
const TERMINAL_TYPE_SEND: u8 = 1;

/// The terminal type reported: what the terminal emulator understands.
const TERMINAL_TYPE_NAME: &[u8] = b"XTERM-256COLOR";

/// Longest subnegotiation kept; the rest is dropped. None of the options answered needs
/// more than a few bytes: this only bounds what a server can make the client hold.
const MAX_SUBNEGOTIATION: usize = 64;

const CR: u8 = b'\r';
const LF: u8 = b'\n';
const NUL: u8 = 0;

/// The terminal's size in characters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowSize {
    /// Columns.
    pub columns: u16,
    /// Rows.
    pub rows: u16,
}

/// What a chunk of the server's bytes turned into.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Received {
    /// Data for the terminal.
    pub data: Vec<u8>,
    /// Bytes to send back to the server.
    pub reply: Vec<u8>,
}

/// Where the parser stands between two chunks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Parse {
    Data,
    /// After an IAC.
    Command,
    /// After IAC and a verb (WILL, WONT, DO or DONT), waiting for the option.
    Option(u8),
    /// Inside a subnegotiation.
    Sub,
    /// An IAC inside a subnegotiation.
    SubCommand,
}

/// One Telnet connection's protocol state.
#[derive(Debug, Clone)]
pub struct Telnet {
    parse: Parse,
    /// The previous data byte was a CR: a following NUL is padding.
    after_cr: bool,
    /// Options the client performs, agreed with the server.
    ours: [bool; 256],
    /// Options the server performs, agreed with the client.
    theirs: [bool; 256],
    /// Options the client refused to perform, answered once.
    refused_ours: [bool; 256],
    /// Options the client refused to let the server perform, answered once.
    refused_theirs: [bool; 256],
    size: WindowSize,
    sub: Vec<u8>,
}

impl Telnet {
    /// A new connection, for a terminal of `size`.
    #[must_use]
    pub fn new(size: WindowSize) -> Self {
        Self {
            parse: Parse::Data,
            after_cr: false,
            ours: [false; 256],
            theirs: [false; 256],
            refused_ours: [false; 256],
            refused_theirs: [false; 256],
            size,
            sub: Vec::new(),
        }
    }

    /// Whether the server echoes what is typed.
    #[must_use]
    pub fn server_echoes(&self) -> bool {
        self.theirs[usize::from(ECHO)]
    }

    /// Takes the next bytes from the server. A command may span chunks.
    pub fn receive(&mut self, bytes: &[u8]) -> Received {
        let mut received = Received::default();
        for &byte in bytes {
            self.parse = match self.parse {
                Parse::Data if byte == IAC => Parse::Command,
                Parse::Data => {
                    self.data(byte, &mut received.data);
                    Parse::Data
                }
                Parse::Command => match byte {
                    IAC => {
                        self.data(IAC, &mut received.data);
                        Parse::Data
                    }
                    WILL | WONT | DO | DONT => Parse::Option(byte),
                    SB => {
                        self.sub.clear();
                        Parse::Sub
                    }
                    // GA, NOP, DM, AYT and the rest carry nothing a terminal shows.
                    _ => Parse::Data,
                },
                Parse::Option(verb) => {
                    self.negotiate(verb, byte, &mut received.reply);
                    Parse::Data
                }
                Parse::Sub if byte == IAC => Parse::SubCommand,
                Parse::Sub => {
                    self.keep(byte);
                    Parse::Sub
                }
                Parse::SubCommand => match byte {
                    SE => {
                        self.subnegotiation(&mut received.reply);
                        Parse::Data
                    }
                    IAC => {
                        self.keep(IAC);
                        Parse::Sub
                    }
                    // Not valid inside a subnegotiation: end it there.
                    _ => Parse::Data,
                },
            };
        }
        received
    }

    /// Bytes for the server carrying `input` typed at the terminal.
    ///
    /// A CR ending `input` goes out as CR NUL at once, even if the next input starts with
    /// LF: holding it back would leave a server that waits for the byte after a CR, as
    /// `telnetd` does, without the end of the line until the next key.
    #[must_use]
    pub fn encode_input(&self, input: &[u8]) -> Vec<u8> {
        let binary = self.ours[usize::from(BINARY)];
        let mut out = Vec::with_capacity(input.len() + input.len() / 8);
        let mut bytes = input.iter().copied().peekable();
        while let Some(byte) = bytes.next() {
            match byte {
                IAC => out.extend_from_slice(&[IAC, IAC]),
                // A lone CR is sent as CR NUL outside binary mode (RFC 854).
                CR if !binary && bytes.peek() != Some(&LF) => out.extend_from_slice(&[CR, NUL]),
                _ => out.push(byte),
            }
        }
        out
    }

    /// Records a new terminal size; the bytes telling the server, if it asked for sizes.
    pub fn resize(&mut self, size: WindowSize) -> Vec<u8> {
        self.size = size;
        let mut out = Vec::new();
        if self.ours[usize::from(WINDOW_SIZE)] {
            self.window_size(&mut out);
        }
        out
    }

    fn data(&mut self, byte: u8, data: &mut Vec<u8>) {
        let padding = self.after_cr && byte == NUL && !self.theirs[usize::from(BINARY)];
        self.after_cr = byte == CR;
        if !padding {
            data.push(byte);
        }
    }

    fn keep(&mut self, byte: u8) {
        if self.sub.len() < MAX_SUBNEGOTIATION {
            self.sub.push(byte);
        }
    }

    fn negotiate(&mut self, verb: u8, option: u8, reply: &mut Vec<u8>) {
        let index = usize::from(option);
        match verb {
            WILL if !self.theirs[index] => {
                if matches!(option, BINARY | ECHO | SUPPRESS_GO_AHEAD) {
                    self.theirs[index] = true;
                    reply.extend_from_slice(&[IAC, DO, option]);
                } else if !self.refused_theirs[index] {
                    self.refused_theirs[index] = true;
                    reply.extend_from_slice(&[IAC, DONT, option]);
                }
            }
            WONT if self.theirs[index] => {
                self.theirs[index] = false;
                reply.extend_from_slice(&[IAC, DONT, option]);
            }
            DO if !self.ours[index] => {
                if matches!(
                    option,
                    BINARY | SUPPRESS_GO_AHEAD | TERMINAL_TYPE | WINDOW_SIZE
                ) {
                    self.ours[index] = true;
                    reply.extend_from_slice(&[IAC, WILL, option]);
                    if option == WINDOW_SIZE {
                        self.window_size(reply);
                    }
                } else if !self.refused_ours[index] {
                    self.refused_ours[index] = true;
                    reply.extend_from_slice(&[IAC, WONT, option]);
                }
            }
            DONT if self.ours[index] => {
                self.ours[index] = false;
                reply.extend_from_slice(&[IAC, WONT, option]);
            }
            // Already in the requested state: answering would start a loop.
            _ => {}
        }
    }

    fn subnegotiation(&mut self, reply: &mut Vec<u8>) {
        if self.sub.as_slice() == [TERMINAL_TYPE, TERMINAL_TYPE_SEND]
            && self.ours[usize::from(TERMINAL_TYPE)]
        {
            reply.extend_from_slice(&[IAC, SB, TERMINAL_TYPE, TERMINAL_TYPE_IS]);
            reply.extend_from_slice(TERMINAL_TYPE_NAME);
            reply.extend_from_slice(&[IAC, SE]);
        }
        self.sub.clear();
    }

    fn window_size(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&[IAC, SB, WINDOW_SIZE]);
        for value in [self.size.columns, self.size.rows] {
            for byte in value.to_be_bytes() {
                // A size byte of 255 is doubled, as any IAC inside a subnegotiation.
                if byte == IAC {
                    out.push(IAC);
                }
                out.push(byte);
            }
        }
        out.extend_from_slice(&[IAC, SE]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIZE: WindowSize = WindowSize {
        columns: 80,
        rows: 24,
    };

    #[test]
    fn plain_text_passes_and_a_doubled_iac_is_one_byte() {
        let mut telnet = Telnet::new(SIZE);
        let received = telnet.receive(b"ab\xff\xffc");
        assert_eq!(received.data, b"ab\xffc");
        assert!(received.reply.is_empty());
    }

    #[test]
    fn a_cr_nul_is_a_cr_and_a_cr_lf_stays() {
        let mut telnet = Telnet::new(SIZE);
        assert_eq!(telnet.receive(b"a\r\0b\r\nc").data, b"a\rb\r\nc");
        // A NUL not after a CR is data.
        assert_eq!(telnet.receive(b"\0").data, b"\0");
    }

    #[test]
    fn the_echo_the_server_offers_is_accepted_once() {
        let mut telnet = Telnet::new(SIZE);
        assert_eq!(telnet.receive(&[IAC, WILL, ECHO]).reply, [IAC, DO, ECHO]);
        assert!(telnet.server_echoes());
        // Already agreed: no answer, or two peers would loop.
        assert!(telnet.receive(&[IAC, WILL, ECHO]).reply.is_empty());
        assert_eq!(telnet.receive(&[IAC, WONT, ECHO]).reply, [IAC, DONT, ECHO]);
        assert!(!telnet.server_echoes());
        assert!(telnet.receive(&[IAC, WONT, ECHO]).reply.is_empty());
    }

    #[test]
    fn unknown_options_are_refused() {
        let mut telnet = Telnet::new(SIZE);
        // NEW-ENVIRON (39), which would send the user's environment.
        assert_eq!(telnet.receive(&[IAC, DO, 39]).reply, [IAC, WONT, 39]);
        assert_eq!(telnet.receive(&[IAC, WILL, 39]).reply, [IAC, DONT, 39]);
        // Asked again, it stays refused without a word: a server that re-asks on every
        // refusal cannot make the two loop.
        assert!(
            telnet
                .receive(&[IAC, DO, 39, IAC, WILL, 39])
                .reply
                .is_empty()
        );
    }

    #[test]
    fn the_window_size_is_sent_when_asked_for_and_on_every_resize() {
        let mut telnet = Telnet::new(SIZE);
        assert!(
            telnet
                .resize(WindowSize {
                    columns: 100,
                    rows: 30
                })
                .is_empty(),
            "not asked for yet"
        );
        assert_eq!(
            telnet.receive(&[IAC, DO, WINDOW_SIZE]).reply,
            [
                IAC,
                WILL,
                WINDOW_SIZE,
                IAC,
                SB,
                WINDOW_SIZE,
                0,
                100,
                0,
                30,
                IAC,
                SE
            ]
        );
        // 255 columns: the byte 255 is doubled.
        assert_eq!(
            telnet.resize(WindowSize {
                columns: 255,
                rows: 1
            }),
            [IAC, SB, WINDOW_SIZE, 0, IAC, IAC, 0, 1, IAC, SE]
        );
        assert_eq!(
            telnet.receive(&[IAC, DONT, WINDOW_SIZE]).reply,
            [IAC, WONT, WINDOW_SIZE]
        );
        assert!(telnet.resize(SIZE).is_empty());
    }

    #[test]
    fn the_terminal_type_is_given_once_agreed() {
        let mut telnet = Telnet::new(SIZE);
        let send = [IAC, SB, TERMINAL_TYPE, TERMINAL_TYPE_SEND, IAC, SE];
        assert!(telnet.receive(&send).reply.is_empty(), "not agreed yet");
        assert_eq!(
            telnet.receive(&[IAC, DO, TERMINAL_TYPE]).reply,
            [IAC, WILL, TERMINAL_TYPE]
        );
        let mut expected = vec![IAC, SB, TERMINAL_TYPE, TERMINAL_TYPE_IS];
        expected.extend_from_slice(TERMINAL_TYPE_NAME);
        expected.extend_from_slice(&[IAC, SE]);
        assert_eq!(telnet.receive(&send).reply, expected);
    }

    #[test]
    fn commands_split_across_chunks_parse_as_whole_ones() {
        let stream: Vec<u8> = [
            b"login: ".as_slice(),
            &[IAC, WILL, ECHO, IAC, DO, WINDOW_SIZE],
            b"x\xff\xffy",
            &[IAC, SB, TERMINAL_TYPE, TERMINAL_TYPE_SEND, IAC, SE],
            &[IAC, DO, TERMINAL_TYPE],
            b"\r\0end",
        ]
        .concat();
        let whole = Telnet::new(SIZE).receive(&stream);
        let mut telnet = Telnet::new(SIZE);
        let mut split = Received::default();
        for byte in &stream {
            let part = telnet.receive(std::slice::from_ref(byte));
            split.data.extend(part.data);
            split.reply.extend(part.reply);
        }
        assert_eq!(split, whole);
        assert_eq!(whole.data, b"login: x\xffy\rend");
    }

    #[test]
    fn a_long_subnegotiation_is_bounded_and_ends_cleanly() {
        let mut telnet = Telnet::new(SIZE);
        let mut stream = vec![IAC, SB];
        stream.extend(std::iter::repeat_n(7, 10_000));
        stream.extend_from_slice(&[IAC, SE]);
        stream.extend_from_slice(b"after");
        let received = telnet.receive(&stream);
        assert_eq!(received.data, b"after");
        assert!(telnet.sub.capacity() <= MAX_SUBNEGOTIATION * 2);
    }

    #[test]
    fn typed_input_escapes_iac_and_pads_a_lone_cr() {
        let mut telnet = Telnet::new(SIZE);
        assert_eq!(telnet.encode_input(b"ls\r"), b"ls\r\0");
        assert_eq!(telnet.encode_input(b"a\r\nb"), b"a\r\nb");
        assert_eq!(telnet.encode_input(b"\xff"), [IAC, IAC]);
        // In binary mode a CR goes as it is; an IAC is still doubled.
        let _ = telnet.receive(&[IAC, DO, BINARY]);
        assert_eq!(telnet.encode_input(b"ls\r\xff"), b"ls\r\xff\xff");
    }
}
