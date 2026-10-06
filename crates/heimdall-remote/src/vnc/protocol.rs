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

//! The RFB protocol (RFC 6143) without input or output: the server's bytes in, events and
//! bytes to send out.
//!
//! Versions 3.3, 3.7 and 3.8; security None and VNC Authentication; encodings Raw,
//! `CopyRect` and ZRLE, with the `DesktopSize` and `LastRect` pseudo-encodings. A message is read once it
//! is whole; whatever the server announces (a name, a clipboard, a rectangle) is bounded
//! before anything is allocated for it.

use zeroize::Zeroizing;

use super::auth::{self, CHALLENGE_LENGTH};
use super::screen::{MAX_SIDE, PIXEL_BYTES, Rect, Screen};
use super::zrle::Zrle;

/// Length of the version message.
const VERSION_LENGTH: usize = 12;

/// Security types.
const SECURITY_NONE: u8 = 1;
const SECURITY_VNC_AUTH: u8 = 2;

/// Encodings.
const ENCODING_RAW: i32 = 0;
const ENCODING_COPY_RECT: i32 = 1;
const ENCODING_ZRLE: i32 = 16;
const PSEUDO_DESKTOP_SIZE: i32 = -223;
const PSEUDO_LAST_RECT: i32 = -224;
/// The server says its screens, and takes a size asked of it: noVNC's "remote resizing".
const PSEUDO_EXTENDED_DESKTOP_SIZE: i32 = -308;
/// The server says its desktop's new name, as noVNC asks it: the name live, not only the
/// one given at the start.
const PSEUDO_DESKTOP_NAME: i32 = -307;

/// Encodings asked for, preferred first.
const ENCODINGS: [i32; 7] = [
    ENCODING_ZRLE,
    ENCODING_COPY_RECT,
    ENCODING_RAW,
    PSEUDO_DESKTOP_SIZE,
    PSEUDO_LAST_RECT,
    PSEUDO_EXTENDED_DESKTOP_SIZE,
    PSEUDO_DESKTOP_NAME,
];

/// Bytes of one screen of an extended desktop size: identifier, place, size and flags.
const SCREEN_BYTES: usize = 16;

/// Server messages.
const FRAMEBUFFER_UPDATE: u8 = 0;
const SET_COLOUR_MAP_ENTRIES: u8 = 1;
const BELL: u8 = 2;
const SERVER_CUT_TEXT: u8 = 3;

/// Client messages.
const SET_PIXEL_FORMAT: u8 = 0;
const SET_ENCODINGS: u8 = 2;
const FRAMEBUFFER_UPDATE_REQUEST: u8 = 3;
const KEY_EVENT: u8 = 4;
const POINTER_EVENT: u8 = 5;
const CLIENT_CUT_TEXT: u8 = 6;
const SET_DESKTOP_SIZE: u8 = 251;

/// Longest server text kept: a failure reason or the desktop's name.
const MAX_TEXT: usize = 4096;

/// Longest clipboard kept from the server; a longer one is read and dropped.
pub const MAX_CUT_TEXT: usize = 1 << 20;

/// Longest compressed rectangle accepted.
const MAX_COMPRESSED_RECT: usize = 64 << 20;

/// The pixel format asked for: 32 bits, depth 24, true colour, little-endian, red in the
/// lowest byte. In memory a pixel is then red, green, blue and one unused byte.
const PIXEL_FORMAT: [u8; 16] = [32, 24, 0, 1, 0, 255, 0, 255, 0, 255, 0, 8, 16, 0, 0, 0];

/// A version the server and the client agreed on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Version {
    /// 3.3: the server picks the security type.
    V3_3,
    /// 3.7: the client picks it.
    V3_7,
    /// 3.8: as 3.7, and a failure carries its reason.
    V3_8,
}

/// Which security the client accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SecurityPolicy {
    /// Whether a server asking for no password at all is accepted. Off by default: a
    /// profile that expects a password must not be answered by anyone pretending to be
    /// the server with no password.
    pub allow_no_authentication: bool,
}

/// What the protocol reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RfbEvent {
    /// The server asks for a password: answer with [`Rfb::answer_password`].
    PasswordRequired,
    /// The session is open.
    Connected {
        /// Desktop width.
        width: u16,
        /// Desktop height.
        height: u16,
        /// The desktop's name, as the server gives it: untrusted.
        name: String,
    },
    /// Pixels changed in this rectangle.
    Updated(Rect),
    /// The desktop changed size; its pixels are black until the next update.
    Resized {
        /// Width.
        width: u16,
        /// Height.
        height: u16,
    },
    /// The server rang the bell.
    Bell,
    /// The server's clipboard, as Latin-1 decoded text.
    ServerCutText(String),
    /// The desktop's new name, as the server gives it: untrusted.
    Renamed(String),
}

/// Why the session cannot go on.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RfbError {
    /// The server speaks a version this client does not.
    #[error("the server speaks an unsupported protocol version: {0}")]
    UnsupportedVersion(String),
    /// The server refused the connection before any security; its reason, untrusted.
    #[error("the server refused the connection: {0}")]
    Refused(String),
    /// The server offers no security the client accepts; the types offered.
    #[error("the server offers no accepted security type (offered: {0:?})")]
    NoAcceptableSecurity(Vec<u8>),
    /// The password was refused; the server's reason when it gives one, untrusted.
    #[error("the server refused the password")]
    AuthenticationFailed(Option<String>),
    /// The server broke the protocol.
    #[error("protocol error: {0}")]
    Protocol(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Version,
    SecurityTypes,
    /// Version 3.3: the server names the type.
    SecurityType,
    /// A failure reason before any security.
    RefusalReason,
    Challenge,
    AwaitingPassword,
    SecurityResult,
    FailureReason,
    ServerInit,
    Messages,
    /// Inside a framebuffer update: rectangles left, `None` until a `LastRect`.
    Rectangles(Option<u16>),
    /// Dropping the rest of an oversized clipboard.
    DroppingCutText(usize),
    Failed,
}

/// One RFB connection's protocol state.
pub struct Rfb {
    policy: SecurityPolicy,
    state: State,
    version: Version,
    buffer: Vec<u8>,
    output: Vec<u8>,
    challenge: [u8; CHALLENGE_LENGTH],
    screen: Screen,
    zrle: Zrle,
    /// The first screen the server said, by its identifier and flags, once it says its
    /// screens: a size can then be asked of it.
    layout: Option<(u32, u32)>,
}

impl std::fmt::Debug for Rfb {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Rfb")
            .field("state", &self.state)
            .field("version", &self.version)
            .finish_non_exhaustive()
    }
}

/// Reads whole fields from the buffer; `None` when more bytes are needed.
struct Reader<'a> {
    data: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, count: usize) -> Option<&'a [u8]> {
        let end = self.at.checked_add(count)?;
        let taken = self.data.get(self.at..end)?;
        self.at = end;
        Some(taken)
    }

    fn u8(&mut self) -> Option<u8> {
        Some(self.take(1)?[0])
    }

    fn u16(&mut self) -> Option<u16> {
        let bytes = self.take(2)?;
        Some(u16::from_be_bytes([bytes[0], bytes[1]]))
    }

    fn u32(&mut self) -> Option<u32> {
        let bytes = self.take(4)?;
        Some(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    fn i32(&mut self) -> Option<i32> {
        let bytes = self.take(4)?;
        Some(i32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    fn rect(&mut self) -> Option<Rect> {
        Some(Rect {
            x: self.u16()?,
            y: self.u16()?,
            width: self.u16()?,
            height: self.u16()?,
        })
    }
}

/// What parsing one step decided.
enum Step {
    /// Needs more bytes; nothing was consumed.
    More,
    /// Consumed this many bytes.
    Done(usize),
}

/// A `u32` length from the server as a `usize`, refused past `limit`.
fn length(value: u32, limit: usize, what: &str) -> Result<usize, RfbError> {
    usize::try_from(value)
        .ok()
        .filter(|length| *length <= limit)
        .ok_or_else(|| RfbError::Protocol(format!("{what} of {value} bytes")))
}

/// A desktop's new name: a length, then the name in UTF-8. `false` until all of it is
/// there.
fn desktop_name(reader: &mut Reader<'_>, events: &mut Vec<RfbEvent>) -> Result<bool, RfbError> {
    let Some(size) = reader.u32() else {
        return Ok(false);
    };
    let size = length(size, MAX_TEXT, "a desktop name")?;
    let Some(name) = reader.take(size) else {
        return Ok(false);
    };
    events.push(RfbEvent::Renamed(
        String::from_utf8_lossy(name).into_owned(),
    ));
    Ok(true)
}

/// Server text, Latin-1 as the protocol has it, cut to [`MAX_TEXT`] characters.
fn latin1(bytes: &[u8]) -> String {
    bytes
        .iter()
        .take(MAX_TEXT)
        .map(|byte| char::from(*byte))
        .collect()
}

impl Rfb {
    /// A new connection, accepting security by `policy`.
    #[must_use]
    pub fn new(policy: SecurityPolicy) -> Self {
        Self {
            policy,
            state: State::Version,
            version: Version::V3_8,
            buffer: Vec::new(),
            output: Vec::new(),
            challenge: [0; CHALLENGE_LENGTH],
            screen: Screen::new(0, 0),
            zrle: Zrle::new(),
            layout: None,
        }
    }

    /// Whether the server takes a size asked of it: it said its screens.
    #[must_use]
    pub fn can_resize(&self) -> bool {
        self.layout.is_some()
    }

    /// Asks the server to make its desktop `width` by `height`, one screen, as noVNC's
    /// remote resizing does; nothing when it does not take a size asked of it, or the size
    /// is the desktop's already.
    pub fn request_size(&mut self, width: u16, height: u16) {
        let Some((id, flags)) = self.layout else {
            return;
        };
        if (width, height) == (self.screen.width(), self.screen.height())
            || check_size(width, height).is_err()
        {
            return;
        }
        self.output.extend_from_slice(&[SET_DESKTOP_SIZE, 0]);
        self.output.extend_from_slice(&width.to_be_bytes());
        self.output.extend_from_slice(&height.to_be_bytes());
        // One screen, then padding.
        self.output.extend_from_slice(&[1, 0]);
        self.output.extend_from_slice(&id.to_be_bytes());
        // At the origin.
        self.output.extend_from_slice(&[0, 0, 0, 0]);
        self.output.extend_from_slice(&width.to_be_bytes());
        self.output.extend_from_slice(&height.to_be_bytes());
        self.output.extend_from_slice(&flags.to_be_bytes());
    }

    /// The desktop.
    #[must_use]
    pub fn screen(&self) -> &Screen {
        &self.screen
    }

    /// The version agreed on.
    #[must_use]
    pub fn version(&self) -> Version {
        self.version
    }

    /// Bytes to send to the server, taken.
    pub fn take_output(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.output)
    }

    /// Answers [`RfbEvent::PasswordRequired`]. Only the first 8 bytes of `password` count.
    ///
    /// # Errors
    ///
    /// [`RfbError::Protocol`] when no password was asked for.
    pub fn answer_password(&mut self, password: &[u8]) -> Result<(), RfbError> {
        if self.state != State::AwaitingPassword {
            return Err(RfbError::Protocol("no password was asked for".to_owned()));
        }
        let response = Zeroizing::new(auth::response(password, &self.challenge));
        self.output.extend_from_slice(&*response);
        self.state = State::SecurityResult;
        Ok(())
    }

    /// Takes the next bytes from the server.
    ///
    /// # Errors
    ///
    /// [`RfbError`]; the connection cannot go on after one.
    pub fn receive(&mut self, bytes: &[u8]) -> Result<Vec<RfbEvent>, RfbError> {
        if self.state == State::Failed {
            return Err(RfbError::Protocol(
                "the connection already failed".to_owned(),
            ));
        }
        // Out of `self` while it is parsed, so the parser may change the rest of `self`.
        let mut buffer = std::mem::take(&mut self.buffer);
        buffer.extend_from_slice(bytes);
        let mut events = Vec::new();
        let mut at = 0;
        let outcome = loop {
            match self.step(&buffer[at..], &mut events) {
                Ok(Step::More) => break Ok(()),
                Ok(Step::Done(consumed)) => at += consumed,
                Err(error) => break Err(error),
            }
        };
        buffer.drain(..at);
        self.buffer = buffer;
        if let Err(error) = outcome {
            self.state = State::Failed;
            return Err(error);
        }
        Ok(events)
    }

    /// Reports a key: `keysym` is an X11 keysym.
    pub fn key(&mut self, keysym: u32, down: bool) {
        self.output
            .extend_from_slice(&[KEY_EVENT, u8::from(down), 0, 0]);
        self.output.extend_from_slice(&keysym.to_be_bytes());
    }

    /// Reports the pointer: buttons 1 to 8 as bits 0 to 7 of `buttons`.
    pub fn pointer(&mut self, buttons: u8, x: u16, y: u16) {
        self.output.push(POINTER_EVENT);
        self.output.push(buttons);
        self.output.extend_from_slice(&x.to_be_bytes());
        self.output.extend_from_slice(&y.to_be_bytes());
    }

    /// Sends the clipboard. The protocol carries Latin-1: other characters become `?`.
    pub fn cut_text(&mut self, text: &str) {
        let bytes: Vec<u8> = text
            .chars()
            .map(|character| u8::try_from(u32::from(character)).unwrap_or(b'?'))
            .collect();
        self.output.extend_from_slice(&[CLIENT_CUT_TEXT, 0, 0, 0]);
        self.output
            .extend_from_slice(&u32::try_from(bytes.len()).unwrap_or(u32::MAX).to_be_bytes());
        self.output.extend_from_slice(&bytes);
    }

    fn step(&mut self, data: &[u8], events: &mut Vec<RfbEvent>) -> Result<Step, RfbError> {
        let mut reader = Reader { data, at: 0 };
        let done = |reader: &Reader<'_>| Ok(Step::Done(reader.at));
        match self.state {
            State::Version => {
                let Some(version) = reader.take(VERSION_LENGTH) else {
                    return Ok(Step::More);
                };
                self.version = parse_version(version)?;
                self.output.extend_from_slice(match self.version {
                    Version::V3_3 => b"RFB 003.003\n",
                    Version::V3_7 => b"RFB 003.007\n",
                    Version::V3_8 => b"RFB 003.008\n",
                });
                self.state = if self.version == Version::V3_3 {
                    State::SecurityType
                } else {
                    State::SecurityTypes
                };
                done(&reader)
            }
            State::SecurityTypes => self.security_types(&mut reader),
            State::SecurityType => self.security_type(&mut reader),
            State::RefusalReason => {
                let Some(text) = read_text(&mut reader)? else {
                    return Ok(Step::More);
                };
                Err(RfbError::Refused(text))
            }
            State::Challenge => {
                let Some(challenge) = reader.take(CHALLENGE_LENGTH) else {
                    return Ok(Step::More);
                };
                self.challenge.copy_from_slice(challenge);
                self.state = State::AwaitingPassword;
                events.push(RfbEvent::PasswordRequired);
                done(&reader)
            }
            // Nothing is read until the password is given.
            State::AwaitingPassword | State::Failed => Ok(Step::More),
            State::SecurityResult => {
                let Some(result) = reader.u32() else {
                    return Ok(Step::More);
                };
                if result == 0 {
                    self.output.push(1); // ClientInit: share the desktop with other viewers.
                    self.state = State::ServerInit;
                } else if self.version == Version::V3_8 {
                    self.state = State::FailureReason;
                } else {
                    return Err(RfbError::AuthenticationFailed(None));
                }
                done(&reader)
            }
            State::FailureReason => {
                let Some(text) = read_text(&mut reader)? else {
                    return Ok(Step::More);
                };
                Err(RfbError::AuthenticationFailed(Some(text)))
            }
            State::ServerInit => self.server_init(&mut reader, events),
            State::Messages => self.message(&mut reader, events),
            State::Rectangles(left) => self.rectangle(&mut reader, left, events),
            State::DroppingCutText(left) => {
                let dropped = left.min(data.len());
                if dropped == 0 {
                    return Ok(Step::More);
                }
                self.state = if dropped == left {
                    State::Messages
                } else {
                    State::DroppingCutText(left - dropped)
                };
                Ok(Step::Done(dropped))
            }
        }
    }

    /// Versions 3.7 and 3.8: the server lists its types, the client picks one.
    fn security_types(&mut self, reader: &mut Reader<'_>) -> Result<Step, RfbError> {
        let Some(count) = reader.u8() else {
            return Ok(Step::More);
        };
        if count == 0 {
            self.state = State::RefusalReason;
            return Ok(Step::Done(reader.at));
        }
        let Some(offered) = reader.take(usize::from(count)) else {
            return Ok(Step::More);
        };
        let chosen = self.choose(offered)?;
        self.output.push(chosen);
        self.after_choice(chosen);
        Ok(Step::Done(reader.at))
    }

    /// Version 3.3: the server names the type.
    fn security_type(&mut self, reader: &mut Reader<'_>) -> Result<Step, RfbError> {
        let Some(chosen) = reader.u32() else {
            return Ok(Step::More);
        };
        match u8::try_from(chosen) {
            Ok(0) => self.state = State::RefusalReason,
            Ok(chosen) => {
                let chosen = self.choose(&[chosen])?;
                self.after_choice(chosen);
            }
            Err(_) => return Err(RfbError::NoAcceptableSecurity(Vec::new())),
        }
        Ok(Step::Done(reader.at))
    }

    fn choose(&self, offered: &[u8]) -> Result<u8, RfbError> {
        if offered.contains(&SECURITY_VNC_AUTH) {
            Ok(SECURITY_VNC_AUTH)
        } else if offered.contains(&SECURITY_NONE) && self.policy.allow_no_authentication {
            Ok(SECURITY_NONE)
        } else {
            Err(RfbError::NoAcceptableSecurity(offered.to_vec()))
        }
    }

    fn after_choice(&mut self, chosen: u8) {
        self.state = if chosen == SECURITY_VNC_AUTH {
            State::Challenge
        } else if self.version == Version::V3_8 {
            // 3.8 confirms even no security; 3.3 and 3.7 go straight to initialisation.
            State::SecurityResult
        } else {
            self.output.push(1);
            State::ServerInit
        };
    }

    fn server_init(
        &mut self,
        reader: &mut Reader<'_>,
        events: &mut Vec<RfbEvent>,
    ) -> Result<Step, RfbError> {
        let (Some(width), Some(height), Some(_format), Some(name_length)) =
            (reader.u16(), reader.u16(), reader.take(16), reader.u32())
        else {
            return Ok(Step::More);
        };
        let name_length = length(name_length, MAX_TEXT, "a desktop name")?;
        let Some(name) = reader.take(name_length) else {
            return Ok(Step::More);
        };
        check_size(width, height)?;
        self.screen = Screen::new(width, height);
        self.output.push(SET_PIXEL_FORMAT);
        self.output.extend_from_slice(&[0, 0, 0]);
        self.output.extend_from_slice(&PIXEL_FORMAT);
        self.output.extend_from_slice(&[SET_ENCODINGS, 0]);
        self.output
            .extend_from_slice(&u16::try_from(ENCODINGS.len()).unwrap_or(0).to_be_bytes());
        for encoding in ENCODINGS {
            self.output.extend_from_slice(&encoding.to_be_bytes());
        }
        self.request_update(false);
        self.state = State::Messages;
        events.push(RfbEvent::Connected {
            width,
            height,
            name: latin1(name),
        });
        Ok(Step::Done(reader.at))
    }

    fn message(
        &mut self,
        reader: &mut Reader<'_>,
        events: &mut Vec<RfbEvent>,
    ) -> Result<Step, RfbError> {
        let Some(kind) = reader.u8() else {
            return Ok(Step::More);
        };
        match kind {
            FRAMEBUFFER_UPDATE => {
                let (Some(_padding), Some(count)) = (reader.u8(), reader.u16()) else {
                    return Ok(Step::More);
                };
                // 0xFFFF: as many as come until a LastRect.
                self.state = State::Rectangles((count != u16::MAX).then_some(count));
                if count == 0 {
                    self.end_update();
                }
            }
            SET_COLOUR_MAP_ENTRIES => {
                // Not used with true colour; read past it.
                let (Some(_padding), Some(_first), Some(count)) =
                    (reader.u8(), reader.u16(), reader.u16())
                else {
                    return Ok(Step::More);
                };
                if reader.take(usize::from(count) * 6).is_none() {
                    return Ok(Step::More);
                }
            }
            BELL => events.push(RfbEvent::Bell),
            SERVER_CUT_TEXT => {
                let (Some(_padding), Some(size)) = (reader.take(3), reader.u32()) else {
                    return Ok(Step::More);
                };
                let size = length(size, usize::MAX, "a clipboard")?;
                if size > MAX_CUT_TEXT {
                    self.state = State::DroppingCutText(size);
                    return Ok(Step::Done(reader.at));
                }
                let Some(text) = reader.take(size) else {
                    return Ok(Step::More);
                };
                events.push(RfbEvent::ServerCutText(
                    text.iter().map(|byte| char::from(*byte)).collect(),
                ));
            }
            other => {
                return Err(RfbError::Protocol(format!(
                    "unknown server message {other}"
                )));
            }
        }
        Ok(Step::Done(reader.at))
    }

    fn rectangle(
        &mut self,
        reader: &mut Reader<'_>,
        left: Option<u16>,
        events: &mut Vec<RfbEvent>,
    ) -> Result<Step, RfbError> {
        let (Some(rect), Some(encoding)) = (reader.rect(), reader.i32()) else {
            return Ok(Step::More);
        };
        let mut last = false;
        match encoding {
            ENCODING_RAW => {
                self.check_inside(rect)?;
                let Some(pixels) = reader.take(rect.area() * PIXEL_BYTES) else {
                    return Ok(Step::More);
                };
                let mut pixels = pixels.as_chunks::<PIXEL_BYTES>().0.iter();
                for y in rect.y..rect.y + rect.height {
                    for x in rect.x..rect.x + rect.width {
                        if let Some(pixel) = pixels.next() {
                            self.screen.set(x, y, [pixel[0], pixel[1], pixel[2]]);
                        }
                    }
                }
                events.push(RfbEvent::Updated(rect));
            }
            ENCODING_COPY_RECT => {
                self.check_inside(rect)?;
                let (Some(from_x), Some(from_y)) = (reader.u16(), reader.u16()) else {
                    return Ok(Step::More);
                };
                self.check_inside(Rect {
                    x: from_x,
                    y: from_y,
                    ..rect
                })?;
                self.screen.copy((from_x, from_y), rect);
                events.push(RfbEvent::Updated(rect));
            }
            ENCODING_ZRLE => {
                self.check_inside(rect)?;
                let Some(size) = reader.u32() else {
                    return Ok(Step::More);
                };
                let size = length(size, MAX_COMPRESSED_RECT, "a ZRLE rectangle")?;
                let Some(compressed) = reader.take(size) else {
                    return Ok(Step::More);
                };
                self.zrle
                    .decode(compressed, rect, &mut self.screen)
                    .map_err(RfbError::Protocol)?;
                events.push(RfbEvent::Updated(rect));
            }
            PSEUDO_DESKTOP_SIZE => {
                check_size(rect.width, rect.height)?;
                self.screen = Screen::new(rect.width, rect.height);
                events.push(RfbEvent::Resized {
                    width: rect.width,
                    height: rect.height,
                });
            }
            PSEUDO_LAST_RECT => last = true,
            PSEUDO_DESKTOP_NAME => {
                if !desktop_name(reader, events)? {
                    return Ok(Step::More);
                }
            }
            PSEUDO_EXTENDED_DESKTOP_SIZE => {
                if !self.extended_desktop_size(reader, rect, events)? {
                    return Ok(Step::More);
                }
            }
            other => {
                return Err(RfbError::Protocol(format!(
                    "a rectangle in encoding {other}, which was not asked for"
                )));
            }
        }
        let left = left.map(|left| left.saturating_sub(1));
        if last || left == Some(0) {
            self.end_update();
        } else {
            self.state = State::Rectangles(left);
        }
        Ok(Step::Done(reader.at))
    }

    /// An extended desktop size: the x is why it came, the y whether a size asked was taken
    /// (0), the size the desktop's; then its screens. `false` until all of it is there.
    fn extended_desktop_size(
        &mut self,
        reader: &mut Reader<'_>,
        rect: Rect,
        events: &mut Vec<RfbEvent>,
    ) -> Result<bool, RfbError> {
        let (Some(count), Some(_padding)) = (reader.u8(), reader.take(3)) else {
            return Ok(false);
        };
        let Some(screens) = reader.take(usize::from(count) * SCREEN_BYTES) else {
            return Ok(false);
        };
        if let Some(first) = screens.get(..SCREEN_BYTES) {
            let word = |at: usize| {
                u32::from_be_bytes([first[at], first[at + 1], first[at + 2], first[at + 3]])
            };
            self.layout = Some((word(0), word(12)));
        }
        let taken = rect.y == 0;
        if taken && (rect.width, rect.height) != (self.screen.width(), self.screen.height()) {
            check_size(rect.width, rect.height)?;
            self.screen = Screen::new(rect.width, rect.height);
            events.push(RfbEvent::Resized {
                width: rect.width,
                height: rect.height,
            });
        }
        Ok(true)
    }

    fn end_update(&mut self) {
        self.state = State::Messages;
        self.request_update(true);
    }

    fn request_update(&mut self, incremental: bool) {
        self.output.extend_from_slice(&[
            FRAMEBUFFER_UPDATE_REQUEST,
            u8::from(incremental),
            0,
            0,
            0,
            0,
        ]);
        self.output
            .extend_from_slice(&self.screen.width().to_be_bytes());
        self.output
            .extend_from_slice(&self.screen.height().to_be_bytes());
    }

    fn check_inside(&self, rect: Rect) -> Result<(), RfbError> {
        if self.screen.contains(rect) {
            Ok(())
        } else {
            Err(RfbError::Protocol(format!(
                "a rectangle {rect:?} outside the {}x{} desktop",
                self.screen.width(),
                self.screen.height()
            )))
        }
    }
}

fn parse_version(bytes: &[u8]) -> Result<Version, RfbError> {
    let text = latin1(bytes);
    let unsupported = || RfbError::UnsupportedVersion(text.trim_end().to_owned());
    let (Some(major), Some(minor)) = (
        text.strip_prefix("RFB ").and_then(|rest| rest.get(..3)),
        text.get(8..11),
    ) else {
        return Err(unsupported());
    };
    let (Ok(major), Ok(minor)) = (major.parse::<u16>(), minor.parse::<u16>()) else {
        return Err(unsupported());
    };
    match (major, minor) {
        // 3.889 is Apple's 3.8.
        (3, 8..) => Ok(Version::V3_8),
        (3, 7) => Ok(Version::V3_7),
        // Anything else of 3 is read as 3.3, as the specification says.
        (3, _) => Ok(Version::V3_3),
        _ => Err(unsupported()),
    }
}

/// A `u32`-prefixed Latin-1 text; `None` when more bytes are needed.
fn read_text(reader: &mut Reader<'_>) -> Result<Option<String>, RfbError> {
    let Some(size) = reader.u32() else {
        return Ok(None);
    };
    // A reason longer than this is not a reason; refuse it rather than wait for it.
    let size = length(size, MAX_TEXT * 16, "a reason")?;
    Ok(reader.take(size).map(latin1))
}

fn check_size(width: u16, height: u16) -> Result<(), RfbError> {
    if width == 0 || height == 0 || width > MAX_SIDE || height > MAX_SIDE {
        return Err(RfbError::Protocol(format!(
            "a {width}x{height} desktop, outside 1 to {MAX_SIDE} pixels a side"
        )));
    }
    Ok(())
}
