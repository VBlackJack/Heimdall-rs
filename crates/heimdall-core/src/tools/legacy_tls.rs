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

//! The certificates of a server that speaks only TLS 1.0, 1.1 or 1.2 with suites the TLS
//! client refuses, read from the clear part of its handshake (owner decision of 2026-10-17:
//! the certificate inspector shows them, as the C# `SslStream` does where the system still
//! allows those versions).
//!
//! Up to TLS 1.2 the server's `Certificate` message travels in clear, right after its
//! `ServerHello` (RFC 5246 7.3, 7.4.2): a client hello offering 1.0 to 1.2 and a broad list
//! of suites, legacy ones included, is answered with the chain before any key is agreed.
//! Nothing is computed and the handshake is never finished: the reader stops at the
//! certificates, and the caller closes the connection. So nothing proves the server holds
//! the key of the certificate read: what it reads is shown, never trusted nor pinned.
//!
//! Sans I/O, and bounded at every step against a hostile server: a record, a handshake
//! message, the chain and the whole answer have caps, every length is checked against
//! what is there, and a server's bytes never panic the reader.

use super::tls_audit::{KnownCipherSuite, TlsProtocol};

/// Bytes of a record's header: its type, its version, its length.
const RECORD_HEADER_LEN: usize = 5;
/// Bytes of a handshake message's header: its type and its 24-bit length.
const HANDSHAKE_HEADER_LEN: usize = 4;
/// The longest record taken: RFC 5246 6.2.3's bound on a record's fragment
/// (2^14 + 2048). A clear handshake record is at most 2^14; the slack only tolerates a
/// lax server, never more.
pub const MAX_RECORD_LEN: usize = 16_384 + 2_048;
/// The longest handshake message taken, the certificate chain's: far above a real chain
/// (a few kilobytes), far below what a hostile server could make the reader hold.
pub const MAX_HANDSHAKE_MESSAGE_LEN: usize = 128 * 1024;
/// The most bytes of a server's answer read before its certificates: the cap on the
/// whole read, whatever it is made of.
pub const MAX_ANSWER_BYTES: usize = 256 * 1024;
/// The most certificates of a chain kept.
pub const MAX_CHAIN_CERTIFICATES: usize = 16;

/// The record type of an alert.
const CONTENT_ALERT: u8 = 21;
/// The record type of a handshake message.
const CONTENT_HANDSHAKE: u8 = 22;
/// The major version byte of every record from SSL 3.0 to TLS 1.3.
const RECORD_MAJOR_VERSION: u8 = 3;
/// An alert's level byte when it is a warning.
const ALERT_WARNING: u8 = 1;
/// The `close_notify` alert.
const ALERT_CLOSE_NOTIFY: u8 = 0;
/// The handshake type of a `HelloRequest`, which a server may send at any time and a
/// client in a handshake ignores (RFC 5246 7.4.1.1).
const HELLO_REQUEST: u8 = 0;
/// The handshake type of a `ServerHello`.
const SERVER_HELLO: u8 = 2;
/// The handshake type of a `Certificate`.
const CERTIFICATE: u8 = 11;
/// Bytes of a hello's random.
const RANDOM_LEN: usize = 32;
/// The longest session identifier (RFC 5246 7.4.1.2).
const MAX_SESSION_ID_LEN: usize = 32;
/// The only compression offered: none.
const NULL_COMPRESSION: u8 = 0;
/// The signalling suite of secure renegotiation (RFC 5746): offered, never a choice.
const RENEGOTIATION_INFO_SCSV: u16 = 0x00FF;
/// The handshake type of a `ClientHello`.
const CLIENT_HELLO: u8 = 1;
/// The record version of a client hello, TLS 1.0, which every server of 1.0 to 1.2 reads.
const HELLO_RECORD_VERSION: u16 = 0x0301;
/// The random of the hello: any bytes, since no key is ever made from it.
const CLIENT_RANDOM: [u8; RANDOM_LEN] = [0x48; RANDOM_LEN];
/// The extension carrying the server's name (RFC 6066 3), and its name type `host_name`.
const EXT_SERVER_NAME: u16 = 0x0000;
const SERVER_NAME_HOST: u8 = 0;
/// The extension of the groups an ECDHE suite may use (RFC 8422 5.1.1): X25519, P-256,
/// P-384, P-521.
const EXT_SUPPORTED_GROUPS: u16 = 0x000A;
const SUPPORTED_GROUPS: [u16; 4] = [0x001D, 0x0017, 0x0018, 0x0019];
/// The extension of the point formats (RFC 8422 5.1.2): uncompressed alone.
const EXT_POINT_FORMATS: u16 = 0x000B;
const POINT_UNCOMPRESSED: u8 = 0;
/// The extension of the signature schemes a TLS 1.2 server may sign with (RFC 5246
/// 7.4.1.4.1), SHA-1 and DSA ones included: an old server signing with them still answers.
const EXT_SIGNATURE_ALGORITHMS: u16 = 0x000D;
const SIGNATURE_SCHEMES: [u16; 13] = [
    0x0401, 0x0501, 0x0601, 0x0403, 0x0503, 0x0603, 0x0804, 0x0805, 0x0806, 0x0201, 0x0203, 0x0202,
    0x0402,
];
/// The padding extension (RFC 7685), and the hello lengths it avoids: some old load
/// balancers hang on a client hello of 256 to 511 bytes, which is padded to 512 instead.
const EXT_PADDING: u16 = 0x0015;
const HELLO_HANG_FROM: usize = 256;
const HELLO_PADDED_LEN: usize = 512;
/// Bytes of an extension's header: its type and its length.
const EXTENSION_HEADER_LEN: usize = 4;
/// The last 8 bytes of the random of a TLS 1.3 server that settles on TLS 1.2 (RFC 8446
/// 4.1.3): such a server would have spoken TLS 1.3 with rustls.
const TLS13_DOWNGRADE_SENTINEL: [u8; 8] = [0x44, 0x4F, 0x57, 0x4E, 0x47, 0x52, 0x44, 0x01];
/// The TLS 1.2 suites rustls speaks with its ring provider: a server choosing one in TLS
/// 1.2 is no legacy server, whatever made the verified handshake fail.
const RUSTLS_TLS12_SUITES: [u16; 6] = [0xC02B, 0xC02F, 0xC02C, 0xC030, 0xCCA9, 0xCCA8];

/// The suites offered, the strongest first: what a TLS 1.0 to 1.2 server may speak,
/// legacy ones included, so that any such server picks one. Offering them reads a
/// certificate and agrees on nothing: no key is ever made with them.
pub const OFFERED_CIPHER_SUITES: [KnownCipherSuite; 46] = [
    suite(0xC02B, "TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256"),
    suite(0xC02F, "TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256"),
    suite(0xC02C, "TLS_ECDHE_ECDSA_WITH_AES_256_GCM_SHA384"),
    suite(0xC030, "TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384"),
    suite(0xCCA9, "TLS_ECDHE_ECDSA_WITH_CHACHA20_POLY1305_SHA256"),
    suite(0xCCA8, "TLS_ECDHE_RSA_WITH_CHACHA20_POLY1305_SHA256"),
    suite(0x009E, "TLS_DHE_RSA_WITH_AES_128_GCM_SHA256"),
    suite(0x009F, "TLS_DHE_RSA_WITH_AES_256_GCM_SHA384"),
    suite(0xC023, "TLS_ECDHE_ECDSA_WITH_AES_128_CBC_SHA256"),
    suite(0xC027, "TLS_ECDHE_RSA_WITH_AES_128_CBC_SHA256"),
    suite(0xC024, "TLS_ECDHE_ECDSA_WITH_AES_256_CBC_SHA384"),
    suite(0xC028, "TLS_ECDHE_RSA_WITH_AES_256_CBC_SHA384"),
    suite(0xC009, "TLS_ECDHE_ECDSA_WITH_AES_128_CBC_SHA"),
    suite(0xC013, "TLS_ECDHE_RSA_WITH_AES_128_CBC_SHA"),
    suite(0xC00A, "TLS_ECDHE_ECDSA_WITH_AES_256_CBC_SHA"),
    suite(0xC014, "TLS_ECDHE_RSA_WITH_AES_256_CBC_SHA"),
    suite(0x0067, "TLS_DHE_RSA_WITH_AES_128_CBC_SHA256"),
    suite(0x006B, "TLS_DHE_RSA_WITH_AES_256_CBC_SHA256"),
    suite(0x0033, "TLS_DHE_RSA_WITH_AES_128_CBC_SHA"),
    suite(0x0039, "TLS_DHE_RSA_WITH_AES_256_CBC_SHA"),
    suite(0x0040, "TLS_DHE_DSS_WITH_AES_128_CBC_SHA256"),
    suite(0x006A, "TLS_DHE_DSS_WITH_AES_256_CBC_SHA256"),
    suite(0x0032, "TLS_DHE_DSS_WITH_AES_128_CBC_SHA"),
    suite(0x0038, "TLS_DHE_DSS_WITH_AES_256_CBC_SHA"),
    suite(0xC004, "TLS_ECDH_ECDSA_WITH_AES_128_CBC_SHA"),
    suite(0xC005, "TLS_ECDH_ECDSA_WITH_AES_256_CBC_SHA"),
    suite(0xC00E, "TLS_ECDH_RSA_WITH_AES_128_CBC_SHA"),
    suite(0xC00F, "TLS_ECDH_RSA_WITH_AES_256_CBC_SHA"),
    suite(0x009C, "TLS_RSA_WITH_AES_128_GCM_SHA256"),
    suite(0x009D, "TLS_RSA_WITH_AES_256_GCM_SHA384"),
    suite(0x003C, "TLS_RSA_WITH_AES_128_CBC_SHA256"),
    suite(0x003D, "TLS_RSA_WITH_AES_256_CBC_SHA256"),
    suite(0x002F, "TLS_RSA_WITH_AES_128_CBC_SHA"),
    suite(0x0035, "TLS_RSA_WITH_AES_256_CBC_SHA"),
    suite(0xC012, "TLS_ECDHE_RSA_WITH_3DES_EDE_CBC_SHA"),
    suite(0xC008, "TLS_ECDHE_ECDSA_WITH_3DES_EDE_CBC_SHA"),
    suite(0x0016, "TLS_DHE_RSA_WITH_3DES_EDE_CBC_SHA"),
    suite(0x0013, "TLS_DHE_DSS_WITH_3DES_EDE_CBC_SHA"),
    suite(0x000A, "TLS_RSA_WITH_3DES_EDE_CBC_SHA"),
    suite(0xC011, "TLS_ECDHE_RSA_WITH_RC4_128_SHA"),
    suite(0xC007, "TLS_ECDHE_ECDSA_WITH_RC4_128_SHA"),
    suite(0x0005, "TLS_RSA_WITH_RC4_128_SHA"),
    suite(0x0004, "TLS_RSA_WITH_RC4_128_MD5"),
    suite(0x0041, "TLS_RSA_WITH_CAMELLIA_128_CBC_SHA"),
    suite(0x0084, "TLS_RSA_WITH_CAMELLIA_256_CBC_SHA"),
    suite(0x0009, "TLS_RSA_WITH_DES_CBC_SHA"),
];

/// A suite of the list above.
const fn suite(id: u16, name: &'static str) -> KnownCipherSuite {
    KnownCipherSuite { id, name }
}

/// What the server answered, up to its certificates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyHandshake {
    /// The version its `ServerHello` chose: TLS 1.0, 1.1 or 1.2.
    pub version: TlsProtocol,
    /// The suite it chose, one of [`OFFERED_CIPHER_SUITES`].
    pub cipher_suite: u16,
    /// The certificates in the order it sent them, its own first (DER), never empty.
    pub certificates: Vec<Vec<u8>>,
    /// The random of its `ServerHello`.
    pub server_random: [u8; RANDOM_LEN],
}

impl LegacyHandshake {
    /// The IANA name of the suite chosen.
    #[must_use]
    pub fn cipher_suite_name(&self) -> String {
        cipher_suite_name(self.cipher_suite)
    }

    /// Whether the server speaks what rustls speaks after all: TLS 1.2 with one of its
    /// suites, or TLS 1.3 by the downgrade sentinel of its random. Its verified handshake
    /// then failed for another reason (a client certificate asked, a signature scheme), and
    /// its certificate is not to be shown as that of a legacy server.
    #[must_use]
    pub fn speaks_modern_tls(&self) -> bool {
        let modern_suite =
            self.version == TlsProtocol::Tls12 && RUSTLS_TLS12_SUITES.contains(&self.cipher_suite);
        modern_suite || self.server_random.ends_with(&TLS13_DOWNGRADE_SENTINEL)
    }
}

/// Why no certificate was read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LegacyTlsError {
    /// The server refused the hello with a fatal alert, of this description.
    Alert(u8),
    /// What came is no TLS: a record type or version no TLS server sends.
    NotTls,
    /// The server chose a version this reader does not show: SSL 3.0 or older, or one it
    /// was never offered.
    Version(u16),
    /// A record, a message, the chain or the whole answer past its cap.
    TooLarge,
    /// A length that does not add up, a message out of its place, a suite or a compression
    /// that was never offered.
    Malformed,
    /// The server went on past its `ServerHello` without a certificate, or sent an empty
    /// chain.
    NoCertificate,
    /// The connection closed before the certificates.
    Closed,
}

/// What a client hello is made of.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HelloShape {
    /// Version TLS 1.2 (with no `supported_versions`, so a server picks the highest of
    /// 1.0 to 1.2 it speaks), the server's name, the groups, the point formats and the
    /// signature schemes: what a server of TLS 1.0 to 1.2 expects.
    Full,
    /// Version TLS 1.0 and no extension, for an old server that closes on a later version
    /// or on extensions it does not know (RFC 5246 E.1).
    Minimal,
}

/// `value`, at most 16 bits by construction here, appended big-endian to `out`.
fn push_u16(out: &mut Vec<u8>, value: usize) {
    out.extend(u16::try_from(value).unwrap_or(u16::MAX).to_be_bytes());
}

/// An extension of type `kind` and body `body`, appended to `out`.
fn push_extension(out: &mut Vec<u8>, kind: u16, body: &[u8]) {
    out.extend(kind.to_be_bytes());
    push_u16(out, body.len());
    out.extend_from_slice(body);
}

/// The extensions of a [`HelloShape::Full`] hello to `name`: its name when it is no
/// address, then the groups, point formats and signature schemes.
fn full_extensions(name: &str) -> Vec<u8> {
    let mut extensions = Vec::new();
    if !name.is_empty() && name.parse::<std::net::IpAddr>().is_err() {
        let mut entry = vec![SERVER_NAME_HOST];
        push_u16(&mut entry, name.len());
        entry.extend_from_slice(name.as_bytes());
        let mut list = Vec::new();
        push_u16(&mut list, entry.len());
        list.extend(entry);
        push_extension(&mut extensions, EXT_SERVER_NAME, &list);
    }
    let mut groups = Vec::new();
    push_u16(&mut groups, SUPPORTED_GROUPS.len() * 2);
    for group in SUPPORTED_GROUPS {
        groups.extend(group.to_be_bytes());
    }
    push_extension(&mut extensions, EXT_SUPPORTED_GROUPS, &groups);
    push_extension(&mut extensions, EXT_POINT_FORMATS, &[1, POINT_UNCOMPRESSED]);
    let mut schemes = Vec::new();
    push_u16(&mut schemes, SIGNATURE_SCHEMES.len() * 2);
    for scheme in SIGNATURE_SCHEMES {
        schemes.extend(scheme.to_be_bytes());
    }
    push_extension(&mut extensions, EXT_SIGNATURE_ALGORITHMS, &schemes);
    extensions
}

/// The client hello sent to `host` in `shape`: every suite of [`OFFERED_CIPHER_SUITES`]
/// then the renegotiation signal, no compression, and for a full one its extensions, the
/// name of `host` without its final dot (RFC 6066 3), padded out of the lengths some old
/// load balancers hang on.
#[must_use]
pub fn client_hello(host: &str, shape: HelloShape) -> Vec<u8> {
    let name = host.strip_suffix('.').unwrap_or(host);
    let version = match shape {
        HelloShape::Full => TlsProtocol::Tls12,
        HelloShape::Minimal => TlsProtocol::Tls10,
    };
    let mut body = version.version_code().to_be_bytes().to_vec();
    body.extend(CLIENT_RANDOM);
    // No session to resume.
    body.push(0);
    push_u16(&mut body, (OFFERED_CIPHER_SUITES.len() + 1) * 2);
    for suite in OFFERED_CIPHER_SUITES {
        body.extend(suite.id.to_be_bytes());
    }
    body.extend(RENEGOTIATION_INFO_SCSV.to_be_bytes());
    body.extend([1, NULL_COMPRESSION]);
    if shape == HelloShape::Full {
        let mut extensions = full_extensions(name);
        // The message as it would be: its header, the body, the extensions' length.
        let unpadded = HANDSHAKE_HEADER_LEN + body.len() + 2 + extensions.len();
        if (HELLO_HANG_FROM..HELLO_PADDED_LEN).contains(&unpadded) {
            // As OpenSSL pads: an extension of at least one byte, the message 512 or just past.
            let padding = (HELLO_PADDED_LEN - unpadded)
                .saturating_sub(EXTENSION_HEADER_LEN)
                .max(1);
            push_extension(&mut extensions, EXT_PADDING, &vec![0; padding]);
        }
        push_u16(&mut body, extensions.len());
        body.extend(extensions);
    }
    let mut message = vec![CLIENT_HELLO, 0];
    push_u16(&mut message, body.len());
    message.extend(body);
    let mut record = vec![CONTENT_HANDSHAKE];
    record.extend(HELLO_RECORD_VERSION.to_be_bytes());
    push_u16(&mut record, message.len());
    record.extend(message);
    record
}

/// The IANA name of suite `id`, or its number when it is none of the offered ones.
#[must_use]
pub fn cipher_suite_name(id: u16) -> String {
    OFFERED_CIPHER_SUITES
        .iter()
        .find(|suite| suite.id == id)
        .map_or_else(|| format!("0x{id:04X}"), |suite| suite.name.to_owned())
}

/// The version of wire code `code` this reader shows: TLS 1.0, 1.1 or 1.2.
fn shown_version(code: u16) -> Option<TlsProtocol> {
    [TlsProtocol::Tls10, TlsProtocol::Tls11, TlsProtocol::Tls12]
        .into_iter()
        .find(|version| version.version_code() == code)
}

/// The server's answer read as it comes: whole records out of the bytes, whole handshake
/// messages out of the records, until its `Certificate`.
#[derive(Debug, Default)]
pub struct LegacyReader {
    /// Bytes of a record not whole yet.
    pending: Vec<u8>,
    /// Handshake bytes of whole records, of a message not whole yet.
    handshake: Vec<u8>,
    /// Bytes fed so far.
    fed: usize,
    /// What its `ServerHello` chose, and its random, once read.
    hello: Option<(TlsProtocol, u16, [u8; RANDOM_LEN])>,
}

impl LegacyReader {
    /// A reader at the start of the answer.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// `bytes` of the answer read: the handshake once its certificates are whole, `None`
    /// while more is needed.
    ///
    /// # Errors
    ///
    /// Why the answer holds no certificate this reader shows; once an error is returned the
    /// reader is done with, the connection to be closed.
    pub fn feed(&mut self, bytes: &[u8]) -> Result<Option<LegacyHandshake>, LegacyTlsError> {
        self.fed = self.fed.saturating_add(bytes.len());
        if self.fed > MAX_ANSWER_BYTES {
            return Err(LegacyTlsError::TooLarge);
        }
        self.pending.extend_from_slice(bytes);
        // The records read are let go once, at the end: a flood of small records costs
        // no move of what is left after each.
        let mut consumed = 0;
        let outcome = self.records(&mut consumed);
        self.pending.drain(..consumed);
        outcome
    }

    /// The whole records from `consumed` on, `consumed` moved past each one read.
    fn records(&mut self, consumed: &mut usize) -> Result<Option<LegacyHandshake>, LegacyTlsError> {
        loop {
            let start = *consumed;
            let Some(header) = self.pending.get(start..start + RECORD_HEADER_LEN) else {
                return Ok(None);
            };
            let content_type = header[0];
            if header[1] != RECORD_MAJOR_VERSION {
                return Err(LegacyTlsError::NotTls);
            }
            let length = usize::from(u16::from_be_bytes([header[3], header[4]]));
            if length > MAX_RECORD_LEN {
                return Err(LegacyTlsError::TooLarge);
            }
            let body = start + RECORD_HEADER_LEN;
            let Some(fragment) = self.pending.get(body..body + length) else {
                return Ok(None);
            };
            match content_type {
                CONTENT_ALERT => Self::alert(fragment)?,
                // RFC 5246 6.2.1: an empty handshake fragment is never sent.
                CONTENT_HANDSHAKE if fragment.is_empty() => return Err(LegacyTlsError::Malformed),
                CONTENT_HANDSHAKE => self.handshake.extend_from_slice(fragment),
                // A cipher change or data before the certificates: no TLS answer to a hello.
                _ => return Err(LegacyTlsError::NotTls),
            }
            *consumed = body + length;
            if content_type == CONTENT_HANDSHAKE
                && let Some(read) = self.messages()?
            {
                return Ok(Some(read));
            }
        }
    }

    /// An alert record: a warning skipped, as a client does with the `unrecognized_name`
    /// some servers send first; a `close_notify` or a fatal one ends the read.
    fn alert(fragment: &[u8]) -> Result<(), LegacyTlsError> {
        match *fragment {
            [ALERT_WARNING, ALERT_CLOSE_NOTIFY] => Err(LegacyTlsError::Closed),
            [ALERT_WARNING, _] => Ok(()),
            [_, description] => Err(LegacyTlsError::Alert(description)),
            _ => Err(LegacyTlsError::Malformed),
        }
    }

    /// The whole handshake messages read so far: the chain once its message is whole.
    /// Those read are let go once, at the end.
    fn messages(&mut self) -> Result<Option<LegacyHandshake>, LegacyTlsError> {
        let mut consumed = 0;
        let outcome = self.whole_messages(&mut consumed);
        self.handshake.drain(..consumed);
        outcome
    }

    /// The whole handshake messages from `consumed` on, `consumed` moved past each one.
    fn whole_messages(
        &mut self,
        consumed: &mut usize,
    ) -> Result<Option<LegacyHandshake>, LegacyTlsError> {
        loop {
            let start = *consumed;
            let Some(header) = self.handshake.get(start..start + HANDSHAKE_HEADER_LEN) else {
                return Ok(None);
            };
            let kind = header[0];
            let length = u24(&header[1..]);
            if length > MAX_HANDSHAKE_MESSAGE_LEN {
                return Err(LegacyTlsError::TooLarge);
            }
            let body_start = start + HANDSHAKE_HEADER_LEN;
            let Some(body) = self.handshake.get(body_start..body_start + length) else {
                return Ok(None);
            };
            match (kind, self.hello) {
                (HELLO_REQUEST, _) if body.is_empty() => {}
                (SERVER_HELLO, None) => self.hello = Some(server_hello(body)?),
                (CERTIFICATE, Some((version, cipher_suite, server_random))) => {
                    return Ok(Some(LegacyHandshake {
                        version,
                        cipher_suite,
                        certificates: certificates(body)?,
                        server_random,
                    }));
                }
                // Its key exchange or its hello done with no certificate before: a suite
                // without one, or no answer of the kind asked.
                (_, Some(_)) => return Err(LegacyTlsError::NoCertificate),
                (_, None) => return Err(LegacyTlsError::Malformed),
            }
            *consumed = body_start + length;
        }
    }
}

/// The 24-bit big-endian length of the three bytes `bytes`.
fn u24(bytes: &[u8]) -> usize {
    bytes
        .iter()
        .take(3)
        .fold(0, |length, byte| (length << 8) | usize::from(*byte))
}

/// Bytes read in order, each read checked against what is left.
struct Cursor<'a>(&'a [u8]);

impl<'a> Cursor<'a> {
    /// The next `count` bytes.
    fn take(&mut self, count: usize) -> Result<&'a [u8], LegacyTlsError> {
        if count > self.0.len() {
            return Err(LegacyTlsError::Malformed);
        }
        let (taken, rest) = self.0.split_at(count);
        self.0 = rest;
        Ok(taken)
    }

    /// The next byte.
    fn byte(&mut self) -> Result<u8, LegacyTlsError> {
        self.take(1).map(|taken| taken[0])
    }

    /// The next two bytes, big-endian.
    fn u16(&mut self) -> Result<u16, LegacyTlsError> {
        self.take(2)
            .map(|taken| u16::from_be_bytes([taken[0], taken[1]]))
    }

    /// The next three bytes, big-endian.
    fn u24(&mut self) -> Result<usize, LegacyTlsError> {
        self.take(3).map(u24)
    }
}

/// The version and suite a `ServerHello` of body `body` chose (RFC 5246 7.4.1.3), each
/// checked against what was offered, and its random; its extensions are not read.
fn server_hello(body: &[u8]) -> Result<(TlsProtocol, u16, [u8; RANDOM_LEN]), LegacyTlsError> {
    let mut cursor = Cursor(body);
    let code = cursor.u16()?;
    let mut random = [0; RANDOM_LEN];
    random.copy_from_slice(cursor.take(RANDOM_LEN)?);
    let session_id_len = usize::from(cursor.byte()?);
    if session_id_len > MAX_SESSION_ID_LEN {
        return Err(LegacyTlsError::Malformed);
    }
    cursor.take(session_id_len)?;
    let cipher_suite = cursor.u16()?;
    let compression = cursor.byte()?;
    let version = shown_version(code).ok_or(LegacyTlsError::Version(code))?;
    let offered = OFFERED_CIPHER_SUITES
        .iter()
        .any(|suite| suite.id == cipher_suite);
    if !offered || compression != NULL_COMPRESSION {
        return Err(LegacyTlsError::Malformed);
    }
    Ok((version, cipher_suite, random))
}

/// The certificates of a `Certificate` message of body `body` (RFC 5246 7.4.2): a 24-bit
/// list length that is the rest of the body, then each certificate after its own.
fn certificates(body: &[u8]) -> Result<Vec<Vec<u8>>, LegacyTlsError> {
    let mut cursor = Cursor(body);
    let list_len = cursor.u24()?;
    if list_len != cursor.0.len() {
        return Err(LegacyTlsError::Malformed);
    }
    let mut chain = Vec::new();
    while !cursor.0.is_empty() {
        if chain.len() == MAX_CHAIN_CERTIFICATES {
            return Err(LegacyTlsError::TooLarge);
        }
        let length = cursor.u24()?;
        if length == 0 {
            return Err(LegacyTlsError::Malformed);
        }
        chain.push(cursor.take(length)?.to_vec());
    }
    if chain.is_empty() {
        return Err(LegacyTlsError::NoCertificate);
    }
    Ok(chain)
}

/// What a server's answer is made of, for the tests: a TLS server speaking `version` and
/// choosing `cipher_suite`, then presenting `chain`.
#[cfg(test)]
mod answer {
    use super::{CERTIFICATE, CONTENT_HANDSHAKE, MAX_RECORD_LEN, RANDOM_LEN, SERVER_HELLO};

    /// The handshake type of a `ServerHelloDone`.
    const SERVER_HELLO_DONE: u8 = 14;
    /// A server's random, any bytes.
    const SERVER_RANDOM: u8 = 0x5A;

    /// A handshake message of type `kind` and body `body`.
    #[must_use]
    pub fn message(kind: u8, body: &[u8]) -> Vec<u8> {
        let length = u32::try_from(body.len()).unwrap_or(u32::MAX).to_be_bytes();
        let mut message = vec![kind, length[1], length[2], length[3]];
        message.extend_from_slice(body);
        message
    }

    /// The body of a `ServerHello` choosing `version` and `cipher_suite`, no session, no
    /// compression, no extension.
    #[must_use]
    pub fn server_hello_body(version: u16, cipher_suite: u16) -> Vec<u8> {
        let mut body = version.to_be_bytes().to_vec();
        body.extend([SERVER_RANDOM; RANDOM_LEN]);
        body.push(0);
        body.extend(cipher_suite.to_be_bytes());
        body.push(0);
        body
    }

    /// The body of a `Certificate` message presenting `chain`.
    #[must_use]
    pub fn certificate_body(chain: &[Vec<u8>]) -> Vec<u8> {
        let mut list = Vec::new();
        for der in chain {
            let length = u32::try_from(der.len()).unwrap_or(u32::MAX).to_be_bytes();
            list.extend_from_slice(&length[1..]);
            list.extend_from_slice(der);
        }
        let length = u32::try_from(list.len()).unwrap_or(u32::MAX).to_be_bytes();
        let mut body = length[1..].to_vec();
        body.extend(list);
        body
    }

    /// `handshake` cut into handshake records of `version`, each at most `fragment` bytes
    /// (and at most [`MAX_RECORD_LEN`]).
    #[must_use]
    pub fn records(version: u16, handshake: &[u8], fragment: usize) -> Vec<u8> {
        let fragment = fragment.clamp(1, MAX_RECORD_LEN);
        let mut wire = Vec::new();
        for chunk in handshake.chunks(fragment) {
            wire.push(CONTENT_HANDSHAKE);
            wire.extend(version.to_be_bytes());
            wire.extend(u16::try_from(chunk.len()).unwrap_or(u16::MAX).to_be_bytes());
            wire.extend_from_slice(chunk);
        }
        wire
    }

    /// The whole answer of a server speaking `version`, choosing `cipher_suite` and
    /// presenting `chain`: its hello, its certificates and its hello done, in one record.
    #[must_use]
    pub fn server_answer(version: u16, cipher_suite: u16, chain: &[Vec<u8>]) -> Vec<u8> {
        let mut handshake = message(SERVER_HELLO, &server_hello_body(version, cipher_suite));
        handshake.extend(message(CERTIFICATE, &certificate_body(chain)));
        handshake.extend(message(SERVER_HELLO_DONE, &[]));
        records(version, &handshake, MAX_RECORD_LEN)
    }
}

#[cfg(test)]
mod tests {
    use super::answer::{certificate_body, message, records, server_answer, server_hello_body};
    use super::*;

    const TLS10: u16 = 0x0301;
    const TLS12: u16 = 0x0303;
    const RSA_AES_128_CBC_SHA: u16 = 0x002F;
    /// The handshake type of a `ServerKeyExchange`.
    const SERVER_KEY_EXCHANGE: u8 = 12;

    fn chain() -> Vec<Vec<u8>> {
        vec![vec![0x30, 0x03, 1, 2, 3], vec![0x30, 0x01, 9]]
    }

    /// What a reader makes of `wire` fed whole.
    fn read(wire: &[u8]) -> Result<Option<LegacyHandshake>, LegacyTlsError> {
        LegacyReader::new().feed(wire)
    }

    #[test]
    fn a_tls_1_0_answer_gives_its_version_suite_and_chain() {
        let tls10 = read(&server_answer(TLS10, RSA_AES_128_CBC_SHA, &chain()))
            .expect("read")
            .expect("whole");
        assert_eq!(tls10.version, TlsProtocol::Tls10);
        assert_eq!(tls10.cipher_suite, RSA_AES_128_CBC_SHA);
        assert_eq!(tls10.cipher_suite_name(), "TLS_RSA_WITH_AES_128_CBC_SHA");
        assert_eq!(tls10.certificates, chain());
        let tls11 = read(&server_answer(0x0302, 0x0005, &chain()))
            .expect("read")
            .expect("whole");
        assert_eq!(tls11.version, TlsProtocol::Tls11);
        let tls12 = read(&server_answer(TLS12, 0xC013, &chain()))
            .expect("read")
            .expect("whole");
        assert_eq!(tls12.version, TlsProtocol::Tls12);
    }

    #[test]
    fn an_answer_fed_a_byte_at_a_time_or_in_small_records_reads_the_same() {
        let whole = server_answer(TLS10, RSA_AES_128_CBC_SHA, &chain());
        let mut reader = LegacyReader::new();
        let mut done = None;
        for byte in &whole {
            assert!(done.is_none(), "nothing past the certificates is needed");
            done = reader.feed(&[*byte]).expect("read");
        }
        assert_eq!(done.expect("whole").certificates, chain());
        // The messages cut across records of 3 bytes each.
        let mut handshake = message(SERVER_HELLO, &server_hello_body(TLS10, RSA_AES_128_CBC_SHA));
        handshake.extend(message(CERTIFICATE, &certificate_body(&chain())));
        let read = read(&records(TLS10, &handshake, 3))
            .expect("read")
            .expect("whole");
        assert_eq!(read.certificates, chain());
    }

    #[test]
    fn a_warning_and_a_hello_request_are_skipped_a_fatal_alert_said() {
        let mut wire = vec![CONTENT_ALERT, 3, 1, 0, 2, ALERT_WARNING, 112];
        wire.extend(records(TLS10, &message(HELLO_REQUEST, &[]), MAX_RECORD_LEN));
        wire.extend(server_answer(TLS10, RSA_AES_128_CBC_SHA, &chain()));
        assert!(read(&wire).expect("read").is_some());
        let handshake_failure = [CONTENT_ALERT, 3, 1, 0, 2, 2, 40];
        assert_eq!(read(&handshake_failure), Err(LegacyTlsError::Alert(40)));
        let close_notify = [CONTENT_ALERT, 3, 1, 0, 2, ALERT_WARNING, 0];
        assert_eq!(read(&close_notify), Err(LegacyTlsError::Closed));
        assert_eq!(
            read(&[CONTENT_ALERT, 3, 1, 0, 1, 2]),
            Err(LegacyTlsError::Malformed)
        );
    }

    #[test]
    fn what_is_no_tls_or_an_old_version_is_refused() {
        assert_eq!(
            read(b"HTTP/1.1 400 Bad Request\r\n"),
            Err(LegacyTlsError::NotTls)
        );
        assert_eq!(
            read(b"SSH-2.0-OpenSSH_9.6\r\n"),
            Err(LegacyTlsError::NotTls)
        );
        // Application data before any hello.
        assert_eq!(read(&[23, 3, 1, 0, 1, 0]), Err(LegacyTlsError::NotTls));
        assert_eq!(
            read(&server_answer(0x0300, RSA_AES_128_CBC_SHA, &chain())),
            Err(LegacyTlsError::Version(0x0300))
        );
        assert_eq!(
            read(&server_answer(0x0304, RSA_AES_128_CBC_SHA, &chain())),
            Err(LegacyTlsError::Version(0x0304))
        );
    }

    #[test]
    fn a_suite_or_a_compression_never_offered_is_refused() {
        assert_eq!(
            read(&server_answer(TLS10, 0x1301, &chain())),
            Err(LegacyTlsError::Malformed)
        );
        assert_eq!(
            read(&server_answer(TLS10, RENEGOTIATION_INFO_SCSV, &chain())),
            Err(LegacyTlsError::Malformed)
        );
        let mut hello = server_hello_body(TLS10, RSA_AES_128_CBC_SHA);
        *hello.last_mut().expect("compression") = 1;
        let wire = records(TLS10, &message(SERVER_HELLO, &hello), MAX_RECORD_LEN);
        assert_eq!(read(&wire), Err(LegacyTlsError::Malformed));
    }

    #[test]
    fn no_certificate_after_the_hello_is_said_so() {
        let mut handshake = message(SERVER_HELLO, &server_hello_body(TLS10, RSA_AES_128_CBC_SHA));
        handshake.extend(message(SERVER_KEY_EXCHANGE, &[1, 2, 3]));
        assert_eq!(
            read(&records(TLS10, &handshake, MAX_RECORD_LEN)),
            Err(LegacyTlsError::NoCertificate)
        );
        assert_eq!(
            read(&server_answer(TLS10, RSA_AES_128_CBC_SHA, &[])),
            Err(LegacyTlsError::NoCertificate)
        );
        // A certificate before any hello is out of its place.
        let early = message(CERTIFICATE, &certificate_body(&chain()));
        assert_eq!(
            read(&records(TLS10, &early, MAX_RECORD_LEN)),
            Err(LegacyTlsError::Malformed)
        );
    }

    #[test]
    fn records_messages_and_chains_past_their_caps_are_refused() {
        // A record announcing more than the cap, before its bytes come.
        let too_long = u16::try_from(MAX_RECORD_LEN + 1)
            .expect("u16")
            .to_be_bytes();
        assert_eq!(
            read(&[CONTENT_HANDSHAKE, 3, 1, too_long[0], too_long[1]]),
            Err(LegacyTlsError::TooLarge)
        );
        // A message announcing 16 MiB.
        let huge = records(TLS10, &[SERVER_HELLO, 0xFF, 0xFF, 0xFF], MAX_RECORD_LEN);
        assert_eq!(read(&huge), Err(LegacyTlsError::TooLarge));
        // More certificates than kept.
        let many = vec![vec![0x30, 0]; MAX_CHAIN_CERTIFICATES + 1];
        assert_eq!(
            read(&server_answer(TLS10, RSA_AES_128_CBC_SHA, &many)),
            Err(LegacyTlsError::TooLarge)
        );
        // An answer that never ends: warnings past the cap on the whole read.
        let warning = [CONTENT_ALERT, 3, 1, 0, 2, ALERT_WARNING, 112];
        let mut reader = LegacyReader::new();
        let mut outcome = Ok(None);
        for _ in 0..=MAX_ANSWER_BYTES / warning.len() {
            outcome = reader.feed(&warning);
            if outcome.is_err() {
                break;
            }
        }
        assert_eq!(outcome, Err(LegacyTlsError::TooLarge));
    }

    #[test]
    fn lengths_that_do_not_add_up_are_refused() {
        let hello = message(SERVER_HELLO, &server_hello_body(TLS10, RSA_AES_128_CBC_SHA));
        // A certificate list longer or shorter than its message.
        for list in [[0, 0, 9, 0, 0, 1, 7], [0, 0, 3, 0, 0, 9, 7]] {
            let mut handshake = hello.clone();
            handshake.extend(message(CERTIFICATE, &list));
            assert_eq!(
                read(&records(TLS10, &handshake, MAX_RECORD_LEN)),
                Err(LegacyTlsError::Malformed),
                "{list:?}"
            );
        }
        // An empty certificate in the list.
        let mut handshake = hello.clone();
        handshake.extend(message(CERTIFICATE, &[0, 0, 3, 0, 0, 0]));
        assert_eq!(
            read(&records(TLS10, &handshake, MAX_RECORD_LEN)),
            Err(LegacyTlsError::Malformed)
        );
        // A hello cut short, or with a session identifier past 32 bytes.
        let short = message(SERVER_HELLO, &[3, 1, 0]);
        assert_eq!(
            read(&records(TLS10, &short, MAX_RECORD_LEN)),
            Err(LegacyTlsError::Malformed)
        );
        let mut long_session = server_hello_body(TLS10, RSA_AES_128_CBC_SHA);
        long_session[2 + RANDOM_LEN] = 33;
        let wire = records(TLS10, &message(SERVER_HELLO, &long_session), MAX_RECORD_LEN);
        assert_eq!(read(&wire), Err(LegacyTlsError::Malformed));
        // An empty handshake record.
        assert_eq!(
            read(&[CONTENT_HANDSHAKE, 3, 1, 0, 0]),
            Err(LegacyTlsError::Malformed)
        );
    }

    #[test]
    fn every_cut_of_an_answer_and_every_flipped_byte_reads_without_panic() {
        let whole = server_answer(TLS12, 0xC02F, &chain());
        for end in 0..whole.len() {
            // A cut answer waits for more, never ends with a chain.
            assert_eq!(read(&whole[..end]), Ok(None), "cut at {end}");
        }
        for at in 0..whole.len() {
            for flip in [0x01, 0x80, 0xFF] {
                let mut garbled = whole.clone();
                garbled[at] ^= flip;
                let _ = read(&garbled);
            }
        }
        // Bytes of a simple generator, fed in uneven pieces.
        let mut state: u32 = 0x1234_5678;
        for _ in 0..200 {
            let mut reader = LegacyReader::new();
            for _ in 0..20 {
                state = state.wrapping_mul(1_103_515_245).wrapping_add(12_345);
                let len = usize::try_from(state >> 24).expect("small");
                let piece: Vec<u8> = (0..len)
                    .map(|i| {
                        let shifted = state.rotate_left(u32::try_from(i % 32).expect("small"));
                        shifted.to_be_bytes()[0]
                    })
                    .collect();
                if reader.feed(&piece).is_err() {
                    break;
                }
            }
        }
    }

    /// Hello `hello` read back as a server reads it, every length checked: its version,
    /// the length of its message with its header, its suites and its extensions by type.
    fn hello_parts(hello: &[u8]) -> (u16, usize, Vec<u16>, Vec<(u16, Vec<u8>)>) {
        assert_eq!(hello[0], CONTENT_HANDSHAKE);
        assert_eq!(&hello[1..3], &HELLO_RECORD_VERSION.to_be_bytes());
        let record_len = usize::from(u16::from_be_bytes([hello[3], hello[4]]));
        assert_eq!(record_len, hello.len() - RECORD_HEADER_LEN);
        assert_eq!(hello[5], CLIENT_HELLO);
        let message_len = u24(&hello[6..9]);
        assert_eq!(
            message_len,
            hello.len() - RECORD_HEADER_LEN - HANDSHAKE_HEADER_LEN
        );
        let mut cursor = Cursor(&hello[RECORD_HEADER_LEN + HANDSHAKE_HEADER_LEN..]);
        let version = cursor.u16().expect("version");
        cursor.take(RANDOM_LEN).expect("random");
        let session = cursor.byte().expect("session");
        assert_eq!(session, 0);
        let suites_len = usize::from(cursor.u16().expect("suites"));
        let suites = cursor
            .take(suites_len)
            .expect("suites")
            .chunks(2)
            .map(|pair| u16::from_be_bytes([pair[0], pair[1]]))
            .collect();
        assert_eq!(cursor.take(2).expect("compression"), [1, NULL_COMPRESSION]);
        let mut extensions = Vec::new();
        if !cursor.0.is_empty() {
            let total = usize::from(cursor.u16().expect("extensions"));
            assert_eq!(total, cursor.0.len());
            while !cursor.0.is_empty() {
                let kind = cursor.u16().expect("type");
                let length = usize::from(cursor.u16().expect("length"));
                extensions.push((kind, cursor.take(length).expect("body").to_vec()));
            }
        }
        (
            version,
            message_len + HANDSHAKE_HEADER_LEN,
            suites,
            extensions,
        )
    }

    #[test]
    fn the_full_hello_offers_tls_1_2_down_and_names_the_host() {
        let (version, length, suites, extensions) =
            hello_parts(&client_hello("old.lab.", HelloShape::Full));
        assert_eq!(version, TLS12, "up to TLS 1.2");
        assert!(length < HELLO_HANG_FROM);
        let mut offered: Vec<u16> = OFFERED_CIPHER_SUITES.iter().map(|suite| suite.id).collect();
        offered.push(RENEGOTIATION_INFO_SCSV);
        assert_eq!(suites, offered);
        let kinds: Vec<u16> = extensions.iter().map(|(kind, _)| *kind).collect();
        assert_eq!(
            kinds,
            [
                EXT_SERVER_NAME,
                EXT_SUPPORTED_GROUPS,
                EXT_POINT_FORMATS,
                EXT_SIGNATURE_ALGORITHMS
            ],
            "no supported_versions: TLS 1.3 is never offered"
        );
        // The name without its final dot: list length, host_name, name length, name.
        assert_eq!(
            extensions[0].1,
            [&[0, 10, 0, 0, 7][..], b"old.lab"].concat()
        );
        for address in ["10.0.0.5", "2001:db8::1"] {
            let (_, _, _, extensions) = hello_parts(&client_hello(address, HelloShape::Full));
            assert!(
                extensions.iter().all(|(kind, _)| *kind != EXT_SERVER_NAME),
                "no name for {address}"
            );
        }
        assert_eq!(cipher_suite_name(0x1234), "0x1234");
    }

    #[test]
    fn no_hello_has_a_length_old_load_balancers_hang_on() {
        for name_len in 1..=253 {
            let (_, length, _, extensions) =
                hello_parts(&client_hello(&"a".repeat(name_len), HelloShape::Full));
            assert!(
                !(HELLO_HANG_FROM..HELLO_PADDED_LEN).contains(&length),
                "{name_len}: {length}"
            );
            let padded = extensions.iter().any(|(kind, _)| *kind == EXT_PADDING);
            assert_eq!(padded, length >= HELLO_PADDED_LEN, "{name_len}");
        }
    }

    #[test]
    fn the_minimal_hello_says_tls_1_0_with_no_extension() {
        let (version, _, suites, extensions) =
            hello_parts(&client_hello("old.lab", HelloShape::Minimal));
        assert_eq!(version, TLS10);
        assert_eq!(suites.len(), OFFERED_CIPHER_SUITES.len() + 1);
        assert!(extensions.is_empty());
    }

    #[test]
    fn a_server_speaking_what_rustls_speaks_is_told_apart() {
        let handshake = |version, cipher_suite, server_random| LegacyHandshake {
            version,
            cipher_suite,
            certificates: chain(),
            server_random,
        };
        let plain = [0x5A; RANDOM_LEN];
        assert!(handshake(TlsProtocol::Tls12, 0xC02F, plain).speaks_modern_tls());
        assert!(handshake(TlsProtocol::Tls12, 0xCCA8, plain).speaks_modern_tls());
        assert!(!handshake(TlsProtocol::Tls12, RSA_AES_128_CBC_SHA, plain).speaks_modern_tls());
        assert!(!handshake(TlsProtocol::Tls10, 0xC013, plain).speaks_modern_tls());
        let mut tls13 = plain;
        tls13[RANDOM_LEN - 8..].copy_from_slice(&TLS13_DOWNGRADE_SENTINEL);
        assert!(handshake(TlsProtocol::Tls12, RSA_AES_128_CBC_SHA, tls13).speaks_modern_tls());
        // A TLS 1.2 server settling on 1.1 or older marks its random with 00: no TLS 1.3.
        let mut tls12 = tls13;
        tls12[RANDOM_LEN - 1] = 0;
        assert!(!handshake(TlsProtocol::Tls10, RSA_AES_128_CBC_SHA, tls12).speaks_modern_tls());
    }

    #[test]
    fn well_framed_records_of_random_messages_read_without_panic() {
        let hello = message(SERVER_HELLO, &server_hello_body(TLS12, 0xC013));
        let mut state: u32 = 0x9E37_79B9;
        let mut next = move || {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            state.to_be_bytes()[0]
        };
        for round in 0..2_000 {
            let mut handshake = if round % 2 == 0 {
                hello.clone()
            } else {
                Vec::new()
            };
            for _ in 0..usize::from(next() % 4) + 1 {
                // A known or random type, a length that may lie, random bytes.
                let kind =
                    [SERVER_HELLO, CERTIFICATE, HELLO_REQUEST, next()][usize::from(next() % 4)];
                let told = usize::from(next());
                let real = usize::from(next());
                let length = u32::try_from(told).expect("small").to_be_bytes();
                handshake.extend([kind, length[1], length[2], length[3]]);
                handshake.extend((0..real).map(|_| next()));
            }
            let fragment = usize::from(next() % 64) + 1;
            let mut wire = records(TLS12, &handshake, fragment);
            if next() % 8 == 0 {
                wire.extend([CONTENT_ALERT, 3, 3, 0, 2, next() % 3, next()]);
            }
            let mut reader = LegacyReader::new();
            for piece in wire.chunks(usize::from(next()) + 1) {
                if reader.feed(piece).is_err() {
                    break;
                }
            }
        }
    }
}
