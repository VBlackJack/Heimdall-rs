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
//! Versions 3.3, 3.7 and 3.8; security None and VNC Authentication, directly or inside
//! Tight or `VeNCrypt`, and the X509 subtypes of `VeNCrypt`, whose TLS the caller starts when
//! told to; encodings Tight, ZRLE, `CopyRect` and Raw, with the
//! `DesktopSize` and `LastRect` pseudo-encodings and the Tight compression and JPEG quality
//! levels, and the Extended Clipboard pseudo-encoding for text in UTF-8. A message is read
//! once it is whole; whatever the server announces (a name, a
//! clipboard, a rectangle, a list of security types) is bounded before anything is
//! allocated for it.

use zeroize::Zeroizing;

use super::auth::{self, CHALLENGE_LENGTH, TooLong};
use super::clipboard::{
    self, ACTION_NOTIFY, ACTION_PROVIDE, ACTION_REQUEST, FORMAT_TEXT, Incoming,
    MAX_EXTENDED_CUT_TEXT, PSEUDO_EXTENDED_CLIPBOARD, ServerCaps,
};
use super::screen::{MAX_SIDE, PIXEL_BYTES, Rect, Screen};
use super::security::{
    self, Authentication, MAX_TIGHT_AUTH_TYPES, MAX_TIGHT_INIT_CAPABILITIES, MAX_TIGHT_TUNNELS,
    MAX_VENCRYPT_SUBTYPES, PREFERENCE, SECURITY_NONE, SECURITY_TIGHT, SECURITY_VENCRYPT,
    SECURITY_VNC_AUTH, Security, SecurityWrapper, TIGHT_AUTH_NONE, TIGHT_CAPABILITY_BYTES,
    TIGHT_NO_TUNNEL, VENCRYPT_ACCEPTED, VENCRYPT_SUBTYPE_BYTES, VENCRYPT_TLS_ACCEPTED,
    VENCRYPT_VERSION,
};
use super::tight::Tight;
use super::zrle::Zrle;

/// Length of the version message.
const VERSION_LENGTH: usize = 12;

/// Encodings.
const ENCODING_RAW: i32 = 0;
const ENCODING_COPY_RECT: i32 = 1;
const ENCODING_TIGHT: i32 = 7;
const ENCODING_ZRLE: i32 = 16;
const PSEUDO_DESKTOP_SIZE: i32 = -223;
const PSEUDO_LAST_RECT: i32 = -224;
/// The server says its screens, and takes a size asked of it: noVNC's "remote resizing".
const PSEUDO_EXTENDED_DESKTOP_SIZE: i32 = -308;
/// The server says its desktop's new name, as noVNC asks it: the name live, not only the
/// one given at the start.
const PSEUDO_DESKTOP_NAME: i32 = -307;

/// Tight compression level N is asked as this plus N, 0 to 9.
const PSEUDO_COMPRESS_LEVEL_0: i32 = -256;
/// Tight JPEG quality level N is asked as this plus N, 0 to 9; without one, a Tight server
/// sends no JPEG.
const PSEUDO_QUALITY_LEVEL_0: i32 = -32;

/// Encodings asked for, preferred first; the levels of the quality chosen follow them.
const ENCODINGS: [i32; 9] = [
    ENCODING_TIGHT,
    ENCODING_ZRLE,
    ENCODING_COPY_RECT,
    ENCODING_RAW,
    PSEUDO_DESKTOP_SIZE,
    PSEUDO_LAST_RECT,
    PSEUDO_EXTENDED_DESKTOP_SIZE,
    PSEUDO_DESKTOP_NAME,
    PSEUDO_EXTENDED_CLIPBOARD,
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

/// The levels of each quality: compression, then JPEG quality.
const BEST_COMPRESSION: u8 = 0;
const BALANCED_COMPRESSION: u8 = 3;
const BALANCED_JPEG: u8 = 7;
const PERFORMANCE_COMPRESSION: u8 = 6;
const PERFORMANCE_JPEG: u8 = 6;
const LOW_BANDWIDTH_COMPRESSION: u8 = 9;
const LOW_BANDWIDTH_JPEG: u8 = 3;

/// How the server is asked to trade the picture for bandwidth, as the C# Heimdall's
/// "Quality" menu. Each choice sets both Tight levels, where the C# set compression only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Quality {
    /// Compression 0 and no JPEG quality level: a Tight server then never sends JPEG, and
    /// the picture is lossless.
    Best,
    /// Compression 3, JPEG quality 7.
    Balanced,
    /// Compression 6, JPEG quality 6: noVNC's levels, and the C# default.
    #[default]
    Performance,
    /// Compression 9, JPEG quality 3.
    LowBandwidth,
}

impl Quality {
    /// Every choice, in the C# menu's order.
    pub const ALL: [Self; 4] = [
        Self::Best,
        Self::Balanced,
        Self::Performance,
        Self::LowBandwidth,
    ];

    /// The Tight compression level asked, 0 to 9.
    #[must_use]
    pub fn compression_level(self) -> u8 {
        match self {
            Self::Best => BEST_COMPRESSION,
            Self::Balanced => BALANCED_COMPRESSION,
            Self::Performance => PERFORMANCE_COMPRESSION,
            Self::LowBandwidth => LOW_BANDWIDTH_COMPRESSION,
        }
    }

    /// The JPEG quality level asked, 0 to 9; `None` asks for no JPEG at all.
    #[must_use]
    pub fn jpeg_quality(self) -> Option<u8> {
        match self {
            Self::Best => None,
            Self::Balanced => Some(BALANCED_JPEG),
            Self::Performance => Some(PERFORMANCE_JPEG),
            Self::LowBandwidth => Some(LOW_BANDWIDTH_JPEG),
        }
    }

    /// The encodings asked at this quality, preferred first, then its levels.
    fn encodings(self) -> Vec<i32> {
        let compression = PSEUDO_COMPRESS_LEVEL_0 + i32::from(self.compression_level());
        let jpeg = self
            .jpeg_quality()
            .map(|level| PSEUDO_QUALITY_LEVEL_0 + i32::from(level));
        ENCODINGS
            .into_iter()
            .chain(std::iter::once(compression))
            .chain(jpeg)
            .collect()
    }
}

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
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SecurityPolicy {
    /// Whether a server asking for no password at all is accepted. Off by default: a
    /// profile that expects a password must not be answered by anyone pretending to be
    /// the server with no password.
    pub allow_no_authentication: bool,
    /// Whether only an X509 subtype of `VeNCrypt` is accepted: a server offering none is
    /// refused, never answered in clear. Set once a certificate is trusted for the server,
    /// so that whoever stands in its way cannot make the client fall back to clear.
    pub require_tls: bool,
    /// Whether `VeNCrypt` is left out of the choice: set for the one connection made again
    /// after a `VeNCrypt` server offered nothing accepted inside it, though it offered
    /// another type. Never with `require_tls`, which keeps only `VeNCrypt`.
    pub exclude_vencrypt: bool,
    /// The user name of `VeNCrypt` Plain, sent inside TLS only: `X509Plain` is taken only with
    /// one.
    pub username: Option<String>,
}

/// What the protocol reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RfbEvent {
    /// The server asks for a password: answer with [`Rfb::answer_password`].
    PasswordRequired,
    /// The server starts TLS: wrap the connection in it, the server's certificate checked,
    /// then call [`Rfb::tls_started`]. Nothing more goes in clear either way.
    StartTls,
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
    /// The server's clipboard: Latin-1 decoded, or UTF-8 through the Extended Clipboard.
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
    /// The server offers nothing the client accepts inside the wrapping type it chose.
    #[error("the server offers no accepted security inside {wrapper} (offered: {offered:?})")]
    NoAcceptableInnerSecurity {
        /// The wrapping type.
        wrapper: SecurityWrapper,
        /// The codes offered inside it.
        offered: Vec<u32>,
    },
    /// The profile requires TLS and the server offers no X509 subtype of `VeNCrypt`; the codes
    /// offered, `VeNCrypt`'s subtypes when it got that far.
    #[error("the server offers no TLS, which this profile requires (offered: {0:?})")]
    TlsRequired(Vec<u32>),
    /// `VeNCrypt` offers nothing accepted inside it, TLS is not required, and the server
    /// offered another type the client accepts: connect again with
    /// [`SecurityPolicy::exclude_vencrypt`]. The `VeNCrypt` subtypes offered. Nothing was sent
    /// for them.
    #[error(
        "the server offers nothing accepted inside VeNCrypt (offered: {0:?}), but another type: connect again without VeNCrypt"
    )]
    RetryWithoutVencrypt(Vec<u32>),
    /// A Plain credential is longer than the client sends; nothing was sent.
    #[error("the {0} is too long to be sent")]
    CredentialTooLong(TooLong),
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
    /// Tight: the tunnels offered.
    TightTunnels,
    /// Tight: the authentication types offered.
    TightAuthentication,
    /// `VeNCrypt`: the server's version.
    VencryptVersion,
    /// `VeNCrypt`: whether the server takes the client's version.
    VencryptAccepted,
    /// `VeNCrypt`: the subtypes offered.
    VencryptSubtypes,
    /// `VeNCrypt`: whether the server starts the TLS of the X509 subtype taken.
    VencryptTlsAck,
    /// Waiting for the caller to start TLS: nothing is read in clear.
    StartingTls,
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
    /// The wrapping security type chosen, if one was.
    wrapper: Option<SecurityWrapper>,
    /// The authentication chosen, once it is.
    authentication: Option<Authentication>,
    /// Whether it goes inside TLS: an X509 subtype was taken.
    tls: bool,
    /// Whether `VeNCrypt` was chosen while the server offered another type the client
    /// accepts: the way out when nothing inside `VeNCrypt` will do.
    other_type_offered: bool,
    screen: Screen,
    zrle: Zrle,
    tight: Tight,
    /// The quality asked of the server.
    quality: Quality,
    /// The first screen the server said, by its identifier and flags, once it says its
    /// screens: a size can then be asked of it.
    layout: Option<(u32, u32)>,
    /// The server's Extended Clipboard capabilities, once it announced them: the clipboard
    /// then goes both ways in UTF-8.
    clipboard_caps: Option<ServerCaps>,
    /// The text notified to the server, provided when it asks for it.
    clipboard: Option<Zeroizing<String>>,
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

/// A count from the server as a `usize`, refused past `limit` before anything is read or
/// kept for it.
fn bounded(value: u32, limit: usize, what: &str) -> Result<usize, RfbError> {
    usize::try_from(value)
        .ok()
        .filter(|count| *count <= limit)
        .ok_or_else(|| RfbError::Protocol(format!("{value} {what}, more than {limit}")))
}

/// Tight's capability lists after `ServerInit`, read past as noVNC does: the counts of
/// server messages, client messages and encodings, padding, then their entries. `false`
/// until all of it is there.
fn tight_init_capabilities(reader: &mut Reader<'_>) -> Result<bool, RfbError> {
    let (Some(server_messages), Some(client_messages), Some(encodings), Some(_padding)) =
        (reader.u16(), reader.u16(), reader.u16(), reader.take(2))
    else {
        return Ok(false);
    };
    let mut entries = 0;
    for (count, what) in [
        (server_messages, "Tight server message capabilities"),
        (client_messages, "Tight client message capabilities"),
        (encodings, "Tight encoding capabilities"),
    ] {
        entries += bounded(u32::from(count), MAX_TIGHT_INIT_CAPABILITIES, what)?;
    }
    Ok(reader.take(entries * TIGHT_CAPABILITY_BYTES).is_some())
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
            wrapper: None,
            authentication: None,
            tls: false,
            other_type_offered: false,
            screen: Screen::new(0, 0),
            zrle: Zrle::new(),
            tight: Tight::new(),
            quality: Quality::default(),
            layout: None,
            clipboard_caps: None,
            clipboard: None,
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

    /// Asks the server for pictures at `quality`, as noVNC does when its levels change: the
    /// encodings again, then the whole desktop anew so the picture changes at once. Before
    /// the session opens, the quality is only kept, to be asked first.
    pub fn set_quality(&mut self, quality: Quality) {
        if quality == self.quality {
            return;
        }
        self.quality = quality;
        if matches!(
            self.state,
            State::Messages | State::Rectangles(_) | State::DroppingCutText(_)
        ) {
            self.send_encodings();
            self.request_update(false);
        }
    }

    /// The quality asked of the server.
    #[must_use]
    pub fn quality(&self) -> Quality {
        self.quality
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

    /// The security agreed on, once the authentication is chosen.
    #[must_use]
    pub fn security(&self) -> Option<Security> {
        self.authentication.map(|authentication| Security {
            wrapper: self.wrapper,
            authentication,
            tls: self.tls,
        })
    }

    /// Bytes to send to the server, taken.
    pub fn take_output(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.output)
    }

    /// Answers [`RfbEvent::PasswordRequired`]. For VNC Authentication only the first 8 bytes
    /// of `password` count; Plain sends it whole, with the policy's user name.
    ///
    /// # Errors
    ///
    /// [`RfbError::Protocol`] when no password was asked for;
    /// [`RfbError::CredentialTooLong`] for a Plain user name or password past its bound,
    /// and then nothing is sent.
    pub fn answer_password(&mut self, password: &[u8]) -> Result<(), RfbError> {
        if self.state != State::AwaitingPassword {
            return Err(RfbError::Protocol("no password was asked for".to_owned()));
        }
        if self.authentication == Some(Authentication::Plain) {
            let username = self.policy.username.as_deref().unwrap_or_default();
            let credentials =
                auth::plain(username.as_bytes(), password).map_err(RfbError::CredentialTooLong)?;
            self.output.extend_from_slice(&credentials);
        } else {
            let response = Zeroizing::new(auth::response(password, &self.challenge));
            self.output.extend_from_slice(&*response);
        }
        self.state = State::SecurityResult;
        Ok(())
    }

    /// Goes on inside the TLS started after [`RfbEvent::StartTls`]: the authentication of
    /// the X509 subtype taken, [`RfbEvent::PasswordRequired`] at once for Plain.
    ///
    /// # Errors
    ///
    /// [`RfbError::Protocol`] when no TLS was to start.
    pub fn tls_started(&mut self) -> Result<Vec<RfbEvent>, RfbError> {
        let (State::StartingTls, Some(authentication)) = (self.state, self.authentication) else {
            return Err(RfbError::Protocol("no TLS was to start".to_owned()));
        };
        self.authenticate(authentication);
        Ok(if self.state == State::AwaitingPassword {
            vec![RfbEvent::PasswordRequired]
        } else {
            Vec::new()
        })
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

    /// Sends the clipboard. Through the Extended Clipboard, when the server announced it, in
    /// UTF-8 as noVNC: notified, then provided when the server asks for it; or provided at
    /// once when the server takes no notify and the text is within the size it takes unasked.
    /// Otherwise the protocol carries Latin-1: other characters become `?`.
    pub fn cut_text(&mut self, text: &str) {
        if let Some(caps) = self.clipboard_caps
            && let Some(text_max) = caps.text_max
        {
            if caps.takes(ACTION_NOTIFY) {
                self.clipboard = Some(Zeroizing::new(text.to_owned()));
                self.extended_cut_text(&clipboard::notify(FORMAT_TEXT));
                return;
            }
            let wire = Zeroizing::new(clipboard::wire_text(text));
            if caps.takes(ACTION_PROVIDE)
                && u32::try_from(wire.len()).is_ok_and(|size| size <= text_max)
            {
                if let Some(message) = clipboard::provide_message(&wire) {
                    self.extended_cut_text(&message);
                }
                return;
            }
        }
        let bytes: Vec<u8> = text
            .chars()
            .map(|character| u8::try_from(u32::from(character)).unwrap_or(b'?'))
            .collect();
        self.output.extend_from_slice(&[CLIENT_CUT_TEXT, 0, 0, 0]);
        self.output
            .extend_from_slice(&u32::try_from(bytes.len()).unwrap_or(u32::MAX).to_be_bytes());
        self.output.extend_from_slice(&bytes);
    }

    /// A `ClientCutText` carrying an extended clipboard `message`: its length negative.
    fn extended_cut_text(&mut self, message: &[u8]) {
        let Some(size) = i32::try_from(message.len()).ok().and_then(i32::checked_neg) else {
            return;
        };
        self.output.extend_from_slice(&[CLIENT_CUT_TEXT, 0, 0, 0]);
        self.output.extend_from_slice(&size.to_be_bytes());
        self.output.extend_from_slice(message);
    }

    /// Answers an extended clipboard message from the server, as noVNC's
    /// `_handleServerCutText` does.
    fn extended_clipboard(&mut self, incoming: Incoming, events: &mut Vec<RfbEvent>) {
        match incoming {
            Incoming::Caps(caps) => {
                self.clipboard_caps = Some(caps);
                self.extended_cut_text(&clipboard::client_caps());
            }
            Incoming::Request(formats) => {
                let caps = self.clipboard_caps;
                if formats & FORMAT_TEXT != 0
                    && caps.is_some_and(|caps| caps.takes(ACTION_PROVIDE))
                    && let Some(text) = self.clipboard.as_deref()
                {
                    let wire = Zeroizing::new(clipboard::wire_text(text));
                    if let Some(message) = clipboard::provide_message(&wire) {
                        self.extended_cut_text(&message);
                    }
                }
            }
            Incoming::Peek => {
                if self
                    .clipboard_caps
                    .is_some_and(|caps| caps.takes(ACTION_NOTIFY))
                {
                    let formats = if self.clipboard.is_some() {
                        FORMAT_TEXT
                    } else {
                        0
                    };
                    self.extended_cut_text(&clipboard::notify(formats));
                }
            }
            Incoming::Notify(formats) => {
                if formats & FORMAT_TEXT != 0
                    && self
                        .clipboard_caps
                        .is_some_and(|caps| caps.takes(ACTION_REQUEST))
                {
                    self.extended_cut_text(&clipboard::request(FORMAT_TEXT));
                }
            }
            Incoming::Provide(text) => {
                // The server's clipboard replaces what this side had offered.
                self.clipboard = None;
                if let Some(text) = text {
                    events.push(RfbEvent::ServerCutText(text));
                }
            }
        }
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
            State::TightTunnels => self.tight_tunnels(&mut reader),
            State::TightAuthentication => self.tight_authentication(&mut reader),
            State::VencryptVersion => self.vencrypt_version(&mut reader),
            State::VencryptAccepted => self.vencrypt_accepted(&mut reader),
            State::VencryptSubtypes => self.vencrypt_subtypes(&mut reader),
            State::VencryptTlsAck => {
                let Some(status) = reader.u8() else {
                    return Ok(Step::More);
                };
                if status != VENCRYPT_TLS_ACCEPTED {
                    return Err(RfbError::Protocol(format!(
                        "the server did not start TLS (status {status})"
                    )));
                }
                self.state = State::StartingTls;
                events.push(RfbEvent::StartTls);
                done(&reader)
            }
            // TLS speaks first from the client: whatever the server sends before is refused,
            // not read in clear.
            State::StartingTls if data.is_empty() => Ok(Step::More),
            State::StartingTls => Err(RfbError::Protocol(
                "the server sent data in clear where TLS was to start".to_owned(),
            )),
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

    /// The type taken from those offered, in the order of [`PREFERENCE`]; no security only
    /// when the policy allows it, only `VeNCrypt` when it requires TLS, and never `VeNCrypt`
    /// when it leaves it out.
    fn choose(&mut self, offered: &[u8]) -> Result<u8, RfbError> {
        if self.policy.require_tls {
            return if offered.contains(&SECURITY_VENCRYPT) {
                Ok(SECURITY_VENCRYPT)
            } else {
                Err(RfbError::TlsRequired(
                    offered.iter().copied().map(u32::from).collect(),
                ))
            };
        }
        let acceptable = |skip_vencrypt: bool| {
            PREFERENCE
                .into_iter()
                .filter(|kind| *kind != SECURITY_NONE || self.policy.allow_no_authentication)
                .filter(|kind| *kind != SECURITY_VENCRYPT || !skip_vencrypt)
                .find(|kind| offered.contains(kind))
        };
        let chosen = acceptable(self.policy.exclude_vencrypt)
            .ok_or_else(|| RfbError::NoAcceptableSecurity(offered.to_vec()))?;
        self.other_type_offered = chosen == SECURITY_VENCRYPT && acceptable(true).is_some();
        Ok(chosen)
    }

    fn after_choice(&mut self, chosen: u8) {
        match chosen {
            SECURITY_TIGHT => {
                self.wrapper = Some(SecurityWrapper::Tight);
                self.state = State::TightTunnels;
            }
            SECURITY_VENCRYPT => {
                self.wrapper = Some(SecurityWrapper::VeNCrypt);
                self.state = State::VencryptVersion;
            }
            SECURITY_VNC_AUTH => self.authenticate(Authentication::VncAuth),
            _ => self.authenticate(Authentication::NoAuthentication),
        }
    }

    /// Goes on with `authentication`, directly or inside the wrapper chosen.
    fn authenticate(&mut self, authentication: Authentication) {
        self.authentication = Some(authentication);
        self.state = match authentication {
            Authentication::VncAuth => State::Challenge,
            // 3.8 confirms even no security, and Tight always does, as noVNC reads it; 3.3
            // and 3.7 otherwise go straight to initialisation.
            Authentication::NoAuthentication
                if self.version == Version::V3_8
                    || self.wrapper == Some(SecurityWrapper::Tight) =>
            {
                State::SecurityResult
            }
            Authentication::NoAuthentication => {
                self.output.push(1);
                State::ServerInit
            }
            // Asked for by the caller, inside TLS only.
            Authentication::Plain => State::AwaitingPassword,
        };
    }

    /// Tight: the tunnels offered, then no tunnel taken when there are some.
    fn tight_tunnels(&mut self, reader: &mut Reader<'_>) -> Result<Step, RfbError> {
        let Some(count) = reader.u32() else {
            return Ok(Step::More);
        };
        let count = bounded(count, MAX_TIGHT_TUNNELS, "Tight tunnel types")?;
        let Some(tunnels) = reader.take(count * TIGHT_CAPABILITY_BYTES) else {
            return Ok(Step::More);
        };
        if count > 0 {
            if !security::tight_takes_no_tunnel(tunnels) {
                return Err(RfbError::NoAcceptableInnerSecurity {
                    wrapper: SecurityWrapper::Tight,
                    offered: security::tight_codes(tunnels),
                });
            }
            self.output
                .extend_from_slice(&TIGHT_NO_TUNNEL.to_be_bytes());
        }
        self.state = State::TightAuthentication;
        Ok(Step::Done(reader.at))
    }

    /// Tight: the authentication types offered, then the one taken. None offered means no
    /// authentication, which the policy must allow.
    fn tight_authentication(&mut self, reader: &mut Reader<'_>) -> Result<Step, RfbError> {
        let Some(count) = reader.u32() else {
            return Ok(Step::More);
        };
        let count = bounded(count, MAX_TIGHT_AUTH_TYPES, "Tight authentication types")?;
        let Some(types) = reader.take(count * TIGHT_CAPABILITY_BYTES) else {
            return Ok(Step::More);
        };
        let allow_none = self.policy.allow_no_authentication;
        let refused = |offered| RfbError::NoAcceptableInnerSecurity {
            wrapper: SecurityWrapper::Tight,
            offered,
        };
        let authentication = if count == 0 {
            if !allow_none {
                return Err(refused(vec![TIGHT_AUTH_NONE]));
            }
            Authentication::NoAuthentication
        } else {
            let authentication = security::tight_authentication(types, allow_none)
                .ok_or_else(|| refused(security::tight_codes(types)))?;
            self.output
                .extend_from_slice(&security::tight_code(authentication).to_be_bytes());
            authentication
        };
        self.authenticate(authentication);
        Ok(Step::Done(reader.at))
    }

    /// `VeNCrypt`: the server's version, which must be 0.2; the client answers the same.
    fn vencrypt_version(&mut self, reader: &mut Reader<'_>) -> Result<Step, RfbError> {
        let Some(version) = reader.take(VENCRYPT_VERSION.len()) else {
            return Ok(Step::More);
        };
        if version != VENCRYPT_VERSION {
            return Err(RfbError::UnsupportedVersion(format!(
                "VeNCrypt {}.{}",
                version[0], version[1]
            )));
        }
        self.output.extend_from_slice(&VENCRYPT_VERSION);
        self.state = State::VencryptAccepted;
        Ok(Step::Done(reader.at))
    }

    /// `VeNCrypt`: whether the server takes the client's version.
    fn vencrypt_accepted(&mut self, reader: &mut Reader<'_>) -> Result<Step, RfbError> {
        let Some(status) = reader.u8() else {
            return Ok(Step::More);
        };
        if status != VENCRYPT_ACCEPTED {
            return Err(RfbError::Protocol(format!(
                "the server refused VeNCrypt 0.2 (status {status})"
            )));
        }
        self.state = State::VencryptSubtypes;
        Ok(Step::Done(reader.at))
    }

    /// `VeNCrypt`: the subtypes offered, then the one taken. A standard one goes on at once;
    /// an X509 one waits for the server to start TLS.
    fn vencrypt_subtypes(&mut self, reader: &mut Reader<'_>) -> Result<Step, RfbError> {
        let Some(count) = reader.u8() else {
            return Ok(Step::More);
        };
        let count = bounded(u32::from(count), MAX_VENCRYPT_SUBTYPES, "VeNCrypt subtypes")?;
        let Some(subtypes) = reader.take(count * VENCRYPT_SUBTYPE_BYTES) else {
            return Ok(Step::More);
        };
        let offered: Vec<u32> = subtypes
            .as_chunks::<VENCRYPT_SUBTYPE_BYTES>()
            .0
            .iter()
            .map(|subtype| u32::from_be_bytes(*subtype))
            .collect();
        let Some(code) = security::vencrypt_subtype(&offered, &self.policy) else {
            return Err(if self.policy.require_tls {
                RfbError::TlsRequired(offered)
            } else if self.other_type_offered {
                RfbError::RetryWithoutVencrypt(offered)
            } else {
                RfbError::NoAcceptableInnerSecurity {
                    wrapper: SecurityWrapper::VeNCrypt,
                    offered,
                }
            });
        };
        self.output.extend_from_slice(&code.to_be_bytes());
        let (authentication, tls) = security::vencrypt_meaning(code);
        if tls {
            self.authentication = Some(authentication);
            self.tls = true;
            self.state = State::VencryptTlsAck;
        } else {
            self.authenticate(authentication);
        }
        Ok(Step::Done(reader.at))
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
        if self.wrapper == Some(SecurityWrapper::Tight) && !tight_init_capabilities(reader)? {
            return Ok(Step::More);
        }
        check_size(width, height)?;
        self.screen = Screen::new(width, height);
        self.output.push(SET_PIXEL_FORMAT);
        self.output.extend_from_slice(&[0, 0, 0]);
        self.output.extend_from_slice(&PIXEL_FORMAT);
        self.send_encodings();
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
                let size = size.cast_signed();
                if size < 0 {
                    // The Extended Clipboard: its flags and payload.
                    let size = length(size.unsigned_abs(), usize::MAX, "a clipboard")?;
                    if size > MAX_EXTENDED_CUT_TEXT {
                        self.state = State::DroppingCutText(size);
                        return Ok(Step::Done(reader.at));
                    }
                    let Some(message) = reader.take(size) else {
                        return Ok(Step::More);
                    };
                    let incoming = clipboard::parse(message).map_err(RfbError::Protocol)?;
                    self.extended_clipboard(incoming, events);
                    return Ok(Step::Done(reader.at));
                }
                let size = length(size.unsigned_abs(), usize::MAX, "a clipboard")?;
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
            ENCODING_TIGHT => {
                if !self.tight_rect(reader, rect)? {
                    return Ok(Step::More);
                }
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

    /// A Tight rectangle, drawn once all of it is there; `false` until then.
    fn tight_rect(&mut self, reader: &mut Reader<'_>, rect: Rect) -> Result<bool, RfbError> {
        self.check_inside(rect)?;
        let unread = &reader.data[reader.at..];
        let Some(taken) = self
            .tight
            .decode(unread, rect, &mut self.screen)
            .map_err(RfbError::Protocol)?
        else {
            return Ok(false);
        };
        Ok(reader.take(taken).is_some())
    }

    /// `SetEncodings`: those asked at the quality chosen.
    fn send_encodings(&mut self) {
        let encodings = self.quality.encodings();
        self.output.extend_from_slice(&[SET_ENCODINGS, 0]);
        self.output
            .extend_from_slice(&u16::try_from(encodings.len()).unwrap_or(0).to_be_bytes());
        for encoding in encodings {
            self.output.extend_from_slice(&encoding.to_be_bytes());
        }
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
