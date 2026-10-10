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

//! The RFB protocol against byte sequences written by hand from RFC 6143, never produced by
//! the code under test.

use heimdall_remote::vnc::{
    Authentication, MAX_CUT_TEXT, MAX_EXTENDED_CUT_TEXT, MAX_PLAIN_PASSWORD, MAX_PLAIN_USERNAME,
    Quality, Rect, Rfb, RfbError, RfbEvent, Security, SecurityPolicy, SecurityWrapper, TooLong,
    Version,
};

/// The Extended Clipboard pseudo-encoding, 0xC0A1E5CE as noVNC's `encodings.js` has it.
const EXTENDED_CLIPBOARD: i32 = 0xC0A1_E5CE_u32.cast_signed();

const VERSION_3_8: &[u8] = b"RFB 003.008\n";

/// The challenge of the VNC Authentication test vector, and its response for "Secret12",
/// computed with openssl.
const CHALLENGE: [u8; 16] = [
    0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff,
];
const RESPONSE: [u8; 16] = [
    0xee, 0xe9, 0x81, 0xe2, 0x74, 0x19, 0x66, 0x45, 0xd7, 0x10, 0xe9, 0xd9, 0x1f, 0xf5, 0xf5, 0xe8,
];

const NO_AUTHENTICATION: SecurityPolicy = SecurityPolicy {
    allow_no_authentication: true,
    require_tls: false,
    exclude_vencrypt: false,
    username: None,
};

/// A profile with a certificate trusted for its server: TLS or nothing.
const TLS_REQUIRED: SecurityPolicy = SecurityPolicy {
    allow_no_authentication: false,
    require_tls: true,
    exclude_vencrypt: false,
    username: None,
};

/// A profile with a user name, for Plain inside TLS.
fn with_username(username: &str) -> SecurityPolicy {
    SecurityPolicy {
        username: Some(username.to_owned()),
        ..SecurityPolicy::default()
    }
}

/// `ServerInit` for a `width` by `height` desktop named `name`, the server's own pixel format.
fn server_init(width: u16, height: u16, name: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&width.to_be_bytes());
    bytes.extend_from_slice(&height.to_be_bytes());
    bytes.extend_from_slice(&[32, 24, 0, 1, 0, 255, 0, 255, 0, 255, 16, 8, 0, 0, 0, 0]);
    bytes.extend_from_slice(&u32::try_from(name.len()).expect("short").to_be_bytes());
    bytes.extend_from_slice(name);
    bytes
}

/// The encodings asked with these quality `levels`, in noVNC's order (`rfb.js`
/// `_sendEncodings`): `CopyRect`, Tight, `TightPNG` (-260), ZRLE, Hextile (5), RRE (2), Raw; the
/// levels, JPEG quality first; then `DesktopSize`, `LastRect`, QEMU's extended key event
/// (-258), the extended desktop size, the desktop name, the Extended Clipboard and the Cursor
/// (-239).
fn asked(levels: &[i32]) -> Vec<i32> {
    let mut encodings = vec![1, 7, -260, 16, 5, 2, 0];
    encodings.extend_from_slice(levels);
    encodings.extend_from_slice(&[-223, -224, -258, -308, -307, EXTENDED_CLIPBOARD, -239]);
    encodings
}

/// What the client sends once the session opens on a `width` by `height` desktop.
fn opening_requests(width: u16, height: u16) -> Vec<u8> {
    let mut bytes = vec![
        // SetPixelFormat: 32 bits, depth 24, little-endian, true colour, red lowest.
        0, 0, 0, 0, 32, 24, 0, 1, 0, 255, 0, 255, 0, 255, 0, 8, 16, 0, 0, 0,
    ];
    // JPEG quality 6 (-32 + 6) and compression level 6 (-256 + 6), noVNC's, the C# default
    // "Performance".
    bytes.extend(set_encodings(&asked(&[-26, -250])));
    bytes.extend_from_slice(&full_request(false, width, height));
    bytes
}

/// `SetEncodings` of `encodings`.
fn set_encodings(encodings: &[i32]) -> Vec<u8> {
    let mut bytes = vec![2, 0];
    bytes.extend_from_slice(&u16::try_from(encodings.len()).expect("few").to_be_bytes());
    for encoding in encodings {
        bytes.extend_from_slice(&encoding.to_be_bytes());
    }
    bytes
}

fn full_request(incremental: bool, width: u16, height: u16) -> Vec<u8> {
    let mut bytes = vec![3, u8::from(incremental), 0, 0, 0, 0];
    bytes.extend_from_slice(&width.to_be_bytes());
    bytes.extend_from_slice(&height.to_be_bytes());
    bytes
}

/// A session opened on a `width` by `height` desktop, without authentication.
fn opened(width: u16, height: u16) -> Rfb {
    let mut rfb = Rfb::new(NO_AUTHENTICATION);
    rfb.receive(VERSION_3_8).expect("version");
    rfb.receive(&[1, 1]).expect("types");
    rfb.receive(&[0, 0, 0, 0]).expect("result");
    rfb.receive(&server_init(width, height, b"desk"))
        .expect("init");
    let _ = rfb.take_output();
    rfb
}

/// A framebuffer update header for `count` rectangles.
fn update(count: u16) -> Vec<u8> {
    let mut bytes = vec![0, 0];
    bytes.extend_from_slice(&count.to_be_bytes());
    bytes
}

fn rect_header(x: u16, y: u16, width: u16, height: u16, encoding: i32) -> Vec<u8> {
    let mut bytes = Vec::new();
    for value in [x, y, width, height] {
        bytes.extend_from_slice(&value.to_be_bytes());
    }
    bytes.extend_from_slice(&encoding.to_be_bytes());
    bytes
}

fn pixel(screen: &heimdall_remote::vnc::Screen, x: usize, y: usize) -> [u8; 4] {
    let at = (y * usize::from(screen.width()) + x) * 4;
    screen.pixels()[at..at + 4].try_into().expect("pixel")
}

#[test]
fn a_vnc_authentication_handshake_answers_the_challenge_and_opens_the_session() {
    let mut rfb = Rfb::new(SecurityPolicy::default());
    assert!(rfb.receive(VERSION_3_8).expect("version").is_empty());
    assert_eq!(rfb.take_output(), VERSION_3_8);
    // Tight and VNC Authentication offered: the second is chosen.
    rfb.receive(&[2, 16, 2]).expect("types");
    assert_eq!(rfb.take_output(), [2]);
    assert_eq!(
        rfb.receive(&CHALLENGE).expect("challenge"),
        [RfbEvent::PasswordRequired]
    );
    assert!(rfb.take_output().is_empty(), "nothing before the password");
    rfb.answer_password(b"Secret12").expect("answered");
    assert_eq!(rfb.take_output(), RESPONSE);
    rfb.receive(&[0, 0, 0, 0]).expect("result");
    assert_eq!(rfb.take_output(), [1], "ClientInit, shared");
    let events = rfb
        .receive(&server_init(1280, 800, b"lab:1"))
        .expect("init");
    assert_eq!(
        events,
        [RfbEvent::Connected {
            width: 1280,
            height: 800,
            name: "lab:1".to_owned()
        }]
    );
    assert_eq!(rfb.take_output(), opening_requests(1280, 800));
    assert_eq!(rfb.version(), Version::V3_8);
}

#[test]
fn feeding_byte_by_byte_changes_nothing() {
    let mut server = Vec::new();
    server.extend_from_slice(VERSION_3_8);
    server.extend_from_slice(&[1, 2]);
    server.extend_from_slice(&CHALLENGE);
    let mut whole = Rfb::new(SecurityPolicy::default());
    let events = whole.receive(&server).expect("whole");
    let mut split = Rfb::new(SecurityPolicy::default());
    let mut split_events = Vec::new();
    for byte in &server {
        split_events.extend(split.receive(std::slice::from_ref(byte)).expect("byte"));
    }
    assert_eq!(split_events, events);
    assert_eq!(split.take_output(), whole.take_output());
}

#[test]
fn a_server_without_authentication_is_refused_unless_allowed() {
    let mut rfb = Rfb::new(SecurityPolicy::default());
    rfb.receive(VERSION_3_8).expect("version");
    assert_eq!(
        rfb.receive(&[1, 1]),
        Err(RfbError::NoAcceptableSecurity(vec![1]))
    );
    assert!(rfb.receive(&[0]).is_err(), "the connection stays failed");

    let mut rfb = Rfb::new(NO_AUTHENTICATION);
    rfb.receive(VERSION_3_8).expect("version");
    rfb.receive(&[1, 1]).expect("types");
    assert_eq!(
        rfb.take_output(),
        [b"RFB 003.008\n".as_slice(), &[1]].concat()
    );
    // 3.8 confirms even no security.
    rfb.receive(&[0, 0, 0, 0]).expect("result");
    assert_eq!(rfb.take_output(), [1]);
}

#[test]
fn a_refused_password_carries_the_reason_in_3_8_only() {
    let mut rfb = Rfb::new(SecurityPolicy::default());
    rfb.receive(VERSION_3_8).expect("version");
    rfb.receive(&[1, 2]).expect("types");
    rfb.receive(&CHALLENGE).expect("challenge");
    rfb.answer_password(b"wrong").expect("answered");
    let mut failure = vec![0, 0, 0, 1, 0, 0, 0, 7];
    failure.extend_from_slice(b"no luck");
    assert_eq!(
        rfb.receive(&failure),
        Err(RfbError::AuthenticationFailed(Some("no luck".to_owned())))
    );

    let mut rfb = Rfb::new(SecurityPolicy::default());
    rfb.receive(b"RFB 003.007\n").expect("version");
    assert_eq!(rfb.take_output(), b"RFB 003.007\n");
    rfb.receive(&[1, 2]).expect("types");
    rfb.receive(&CHALLENGE).expect("challenge");
    rfb.answer_password(b"wrong").expect("answered");
    assert_eq!(
        rfb.receive(&[0, 0, 0, 1]),
        Err(RfbError::AuthenticationFailed(None))
    );
}

#[test]
fn version_3_3_lets_the_server_pick_and_other_majors_are_refused() {
    let mut rfb = Rfb::new(SecurityPolicy::default());
    rfb.receive(b"RFB 003.003\n").expect("version");
    assert_eq!(rfb.take_output(), b"RFB 003.003\n");
    // The server names VNC Authentication as a 32-bit value; the client sends nothing back.
    assert_eq!(
        rfb.receive(&[0, 0, 0, 2]).expect("type"),
        [] as [RfbEvent; 0]
    );
    assert_eq!(
        rfb.receive(&CHALLENGE).expect("challenge"),
        [RfbEvent::PasswordRequired]
    );
    assert!(rfb.take_output().is_empty());

    // Apple's 3.889 is a 3.8.
    let mut rfb = Rfb::new(SecurityPolicy::default());
    rfb.receive(b"RFB 003.889\n").expect("version");
    assert_eq!(rfb.version(), Version::V3_8);

    let mut rfb = Rfb::new(SecurityPolicy::default());
    assert!(matches!(
        rfb.receive(b"RFB 004.000\n"),
        Err(RfbError::UnsupportedVersion(_))
    ));
}

#[test]
fn a_refusal_before_security_carries_its_reason() {
    let mut rfb = Rfb::new(SecurityPolicy::default());
    rfb.receive(VERSION_3_8).expect("version");
    let mut refusal = vec![0, 0, 0, 0, 9];
    refusal.extend_from_slice(b"too many");
    // The reason's length says 9 but 8 bytes came: waiting for the last.
    assert!(rfb.receive(&refusal).expect("partial").is_empty());
    assert_eq!(
        rfb.receive(b"!"),
        Err(RfbError::Refused("too many!".to_owned()))
    );
}

#[test]
fn a_raw_rectangle_paints_opaque_pixels_and_the_next_update_is_asked_for() {
    let mut rfb = opened(3, 2);
    let mut bytes = update(1);
    bytes.extend(rect_header(1, 1, 2, 1, 0));
    // Two pixels, red then blue, in the format asked for: R G B and an unused byte.
    bytes.extend_from_slice(&[255, 0, 0, 9, 0, 0, 255, 9]);
    let events = rfb.receive(&bytes).expect("update");
    let area = Rect {
        x: 1,
        y: 1,
        width: 2,
        height: 1,
    };
    assert_eq!(events, [RfbEvent::Updated(area)]);
    assert_eq!(pixel(rfb.screen(), 1, 1), [255, 0, 0, 255]);
    assert_eq!(pixel(rfb.screen(), 2, 1), [0, 0, 255, 255]);
    assert_eq!(pixel(rfb.screen(), 0, 0), [0, 0, 0, 255], "untouched");
    assert_eq!(rfb.take_output(), full_request(true, 3, 2));
}

#[test]
fn a_copy_moves_pixels_and_a_rectangle_outside_the_desktop_is_refused() {
    let mut rfb = opened(2, 1);
    let mut bytes = update(2);
    bytes.extend(rect_header(0, 0, 1, 1, 0));
    bytes.extend_from_slice(&[0, 255, 0, 0]);
    // CopyRect: the pixel at 0,0 to 1,0.
    bytes.extend(rect_header(1, 0, 1, 1, 1));
    bytes.extend_from_slice(&[0, 0, 0, 0]);
    rfb.receive(&bytes).expect("update");
    assert_eq!(pixel(rfb.screen(), 1, 0), [0, 255, 0, 255]);

    let mut rfb = opened(2, 1);
    let mut bytes = update(1);
    bytes.extend(rect_header(1, 0, 2, 1, 0));
    assert!(matches!(rfb.receive(&bytes), Err(RfbError::Protocol(_))));

    // A copy from outside the desktop.
    let mut rfb = opened(2, 1);
    let mut bytes = update(1);
    bytes.extend(rect_header(0, 0, 1, 1, 1));
    bytes.extend_from_slice(&[0, 5, 0, 0]);
    assert!(matches!(rfb.receive(&bytes), Err(RfbError::Protocol(_))));
}

/// An `ExtendedDesktopSize` rectangle: why, whether a size asked was taken, the size, then
/// one screen, identifier 7 and flags 0, from the protocol's community specification.
fn extended_size(reason: u16, status: u16, width: u16, height: u16) -> Vec<u8> {
    let mut bytes = rect_header(reason, status, width, height, -308);
    // One screen, three bytes of padding.
    bytes.extend_from_slice(&[1, 0, 0, 0]);
    bytes.extend_from_slice(&7_u32.to_be_bytes());
    bytes.extend_from_slice(&[0, 0, 0, 0]);
    bytes.extend_from_slice(&width.to_be_bytes());
    bytes.extend_from_slice(&height.to_be_bytes());
    bytes.extend_from_slice(&0_u32.to_be_bytes());
    bytes
}

#[test]
fn a_server_saying_its_screens_takes_a_size_asked_of_it() {
    let mut rfb = opened(800, 600);
    assert!(!rfb.can_resize(), "not before it says its screens");
    rfb.request_size(1024, 768);
    assert!(rfb.take_output().is_empty());

    // Its screens said, at the size it has: nothing changes but what it takes.
    let mut bytes = update(1);
    bytes.extend(extended_size(0, 0, 800, 600));
    assert!(rfb.receive(&bytes).expect("update").is_empty());
    assert!(rfb.can_resize());
    let _ = rfb.take_output();

    // SetDesktopSize, its one screen at the origin with the identifier the server gave.
    rfb.request_size(1024, 768);
    let mut asked = vec![251, 0, 4, 0, 3, 0, 1, 0];
    asked.extend_from_slice(&7_u32.to_be_bytes());
    asked.extend_from_slice(&[0, 0, 0, 0, 4, 0, 3, 0]);
    asked.extend_from_slice(&0_u32.to_be_bytes());
    assert_eq!(rfb.take_output(), asked);
    // The size it has already: nothing asked.
    rfb.request_size(800, 600);
    assert!(rfb.take_output().is_empty());

    // Taken, by the client's asking (1): the desktop is that size.
    let mut bytes = update(1);
    bytes.extend(extended_size(1, 0, 1024, 768));
    assert_eq!(
        rfb.receive(&bytes).expect("update"),
        [RfbEvent::Resized {
            width: 1024,
            height: 768
        }]
    );
    assert_eq!((rfb.screen().width(), rfb.screen().height()), (1024, 768));
    let _ = rfb.take_output();

    // Refused (status 3, out of resources): the desktop stays as it is.
    let mut bytes = update(1);
    bytes.extend(extended_size(1, 3, 4000, 3000));
    assert!(rfb.receive(&bytes).expect("update").is_empty());
    assert_eq!((rfb.screen().width(), rfb.screen().height()), (1024, 768));
}

#[test]
fn a_new_desktop_size_and_an_open_ended_update_closed_by_a_last_rect() {
    let mut rfb = opened(2, 2);
    let mut bytes = update(u16::MAX);
    bytes.extend(rect_header(0, 0, 640, 480, -223));
    bytes.extend(rect_header(0, 0, 0, 0, -224));
    let events = rfb.receive(&bytes).expect("update");
    assert_eq!(
        events,
        [RfbEvent::Resized {
            width: 640,
            height: 480
        }]
    );
    assert_eq!((rfb.screen().width(), rfb.screen().height()), (640, 480));
    assert_eq!(rfb.take_output(), full_request(true, 640, 480));

    let mut rfb = opened(2, 2);
    let mut bytes = update(1);
    bytes.extend(rect_header(0, 0, 9000, 10, -223));
    assert!(matches!(rfb.receive(&bytes), Err(RfbError::Protocol(_))));
}

#[test]
fn the_clipboard_is_latin_1_and_an_oversized_one_is_dropped() {
    let mut rfb = opened(2, 2);
    let mut bytes = vec![3, 0, 0, 0, 0, 0, 0, 4];
    bytes.extend_from_slice(b"caf\xe9");
    bytes.push(2); // then a bell
    assert_eq!(
        rfb.receive(&bytes).expect("text"),
        [
            RfbEvent::ServerCutText("caf\u{e9}".to_owned()),
            RfbEvent::Bell
        ]
    );

    let mut rfb = opened(2, 2);
    let size = u32::try_from(MAX_CUT_TEXT + 1).expect("fits");
    let mut bytes = vec![3, 0, 0, 0];
    bytes.extend_from_slice(&size.to_be_bytes());
    assert!(rfb.receive(&bytes).expect("header").is_empty());
    // Read and dropped in pieces, then the next message is read.
    let chunk = vec![b'x'; 1 << 16];
    let mut left = MAX_CUT_TEXT + 1;
    while left > 0 {
        let piece = left.min(chunk.len());
        assert!(rfb.receive(&chunk[..piece]).expect("dropped").is_empty());
        left -= piece;
    }
    assert_eq!(rfb.receive(&[2]).expect("bell"), [RfbEvent::Bell]);
}

#[test]
fn keys_pointer_and_clipboard_are_encoded_as_the_rfc_says() {
    let mut rfb = opened(2, 2);
    rfb.key(0xff0d, true);
    rfb.pointer(0b101, 300, 2);
    rfb.cut_text("\u{e9}\u{263a}");
    assert_eq!(
        rfb.take_output(),
        [
            4, 1, 0, 0, 0, 0, 0xff, 0x0d, // Return pressed
            5, 0b101, 1, 44, 0, 2, // buttons 1 and 3 at 300,2
            6, 0, 0, 0, 0, 0, 0, 2, 0xe9, b'?', // Latin-1, the rest as '?'
        ]
    );
}

#[test]
fn a_server_naming_its_desktop_anew_is_heard() {
    let mut rfb = opened(4, 4);
    let mut bytes = update(1);
    bytes.extend(rect_header(0, 0, 0, 0, -307));
    let name = "build box \u{e9}";
    bytes.extend_from_slice(&u32::try_from(name.len()).expect("short").to_be_bytes());
    bytes.extend_from_slice(name.as_bytes());
    assert_eq!(
        rfb.receive(&bytes).expect("update"),
        [RfbEvent::Renamed(name.to_owned())],
        "UTF-8, as noVNC reads it"
    );
}

#[test]
fn a_quality_asks_its_levels_again_then_the_whole_desktop() {
    let mut rfb = opened(4, 2);
    // Best: compression 0 (-256) and no JPEG quality at all.
    rfb.set_quality(Quality::Best);
    let mut expected = set_encodings(&asked(&[-256]));
    expected.extend(full_request(false, 4, 2));
    assert_eq!(rfb.take_output(), expected);
    // The quality asked already: nothing.
    rfb.set_quality(Quality::Best);
    assert!(rfb.take_output().is_empty());
    // Low bandwidth: JPEG quality 3 (-29), compression 9 (-247).
    rfb.set_quality(Quality::LowBandwidth);
    let mut expected = set_encodings(&asked(&[-29, -247]));
    expected.extend(full_request(false, 4, 2));
    assert_eq!(rfb.take_output(), expected);
    assert_eq!(rfb.quality(), Quality::LowBandwidth);
}

#[test]
fn a_quality_chosen_before_the_session_opens_is_asked_first() {
    let mut rfb = Rfb::new(NO_AUTHENTICATION);
    rfb.set_quality(Quality::Balanced);
    assert!(rfb.take_output().is_empty(), "nothing before the session");
    rfb.receive(VERSION_3_8).expect("version");
    rfb.receive(&[1, 1]).expect("types");
    rfb.receive(&[0, 0, 0, 0]).expect("result");
    let _ = rfb.take_output();
    rfb.receive(&server_init(4, 2, b"desk")).expect("init");
    let output = rfb.take_output();
    // Balanced: JPEG quality 7 (-25), compression 3 (-253), after the pixel format.
    assert_eq!(
        output[20..output.len() - 10],
        set_encodings(&asked(&[-25, -253]))
    );
}

#[test]
fn tight_rectangles_are_drawn_whole_even_fed_byte_by_byte() {
    let mut rfb = opened(3, 1);
    let mut bytes = update(2);
    // Tight fill: control 0x80, then one green TPIXEL, red green blue.
    bytes.extend(rect_header(0, 0, 3, 1, 7));
    bytes.extend_from_slice(&[0x80, 0, 255, 0]);
    // Tight basic, copy filter implied, 3 bytes: under 12, as is, a blue pixel at 2,0.
    bytes.extend(rect_header(2, 0, 1, 1, 7));
    bytes.extend_from_slice(&[0x00, 0, 0, 255]);
    let mut events = Vec::new();
    for byte in &bytes {
        events.extend(rfb.receive(std::slice::from_ref(byte)).expect("fed"));
    }
    let whole = Rect {
        x: 0,
        y: 0,
        width: 3,
        height: 1,
    };
    let last = Rect {
        x: 2,
        width: 1,
        ..whole
    };
    assert_eq!(events, [RfbEvent::Updated(whole), RfbEvent::Updated(last)]);
    assert_eq!(pixel(rfb.screen(), 0, 0), [0, 255, 0, 255]);
    assert_eq!(pixel(rfb.screen(), 1, 0), [0, 255, 0, 255]);
    assert_eq!(pixel(rfb.screen(), 2, 0), [0, 0, 255, 255]);
    assert_eq!(rfb.take_output(), full_request(true, 3, 1));

    // A PNG image in plain Tight, not TightPNG: refused, as a rectangle outside the desktop
    // is.
    let mut rfb = opened(3, 1);
    let mut bytes = update(1);
    bytes.extend(rect_header(0, 0, 1, 1, 7));
    bytes.extend_from_slice(&[0xa0, 1, 0]);
    assert!(matches!(rfb.receive(&bytes), Err(RfbError::Protocol(_))));
    let mut rfb = opened(3, 1);
    let mut bytes = update(1);
    bytes.extend(rect_header(1, 0, 3, 1, 7));
    bytes.extend_from_slice(&[0x80, 0, 255, 0]);
    assert!(matches!(rfb.receive(&bytes), Err(RfbError::Protocol(_))));
}

/// A Tight capability: a code, a vendor and a signature.
fn capability(code: u32, vendor: [u8; 4], signature: [u8; 8]) -> Vec<u8> {
    let mut bytes = code.to_be_bytes().to_vec();
    bytes.extend_from_slice(&vendor);
    bytes.extend_from_slice(&signature);
    bytes
}

/// A Tight list: its count, then its capabilities.
fn tight_list(capabilities: &[Vec<u8>]) -> Vec<u8> {
    let mut bytes = u32::try_from(capabilities.len())
        .expect("few")
        .to_be_bytes()
        .to_vec();
    bytes.extend(capabilities.concat());
    bytes
}

/// `VeNCrypt` subtypes: their count in a byte, then each in 32 bits.
fn vencrypt_subtypes(subtypes: &[u32]) -> Vec<u8> {
    let mut bytes = vec![u8::try_from(subtypes.len()).expect("few")];
    for subtype in subtypes {
        bytes.extend_from_slice(&subtype.to_be_bytes());
    }
    bytes
}

/// Tight's capability lists after `ServerInit`: 1 server message, 1 client message and 2
/// encodings, as `TightVNC` sends them.
fn tight_init_capabilities() -> Vec<u8> {
    let mut bytes = vec![0, 1, 0, 1, 0, 2, 0, 0];
    bytes.extend(capability(150, *b"TGHT", *b"FTS_LSDT"));
    bytes.extend(capability(132, *b"TGHT", *b"FTC_LSRQ"));
    bytes.extend(capability(7, *b"TGHT", *b"TIGHT___"));
    bytes.extend(capability(0xffff_ff20, *b"TGHT", *b"LASTRECT"));
    bytes
}

/// A session at the start of its security, version 3.8, with `policy`.
fn at_security(policy: SecurityPolicy) -> Rfb {
    let mut rfb = Rfb::new(policy);
    rfb.receive(VERSION_3_8).expect("version");
    let _ = rfb.take_output();
    rfb
}

#[test]
fn tight_takes_no_tunnel_then_vnc_authentication_and_reads_past_its_capabilities() {
    let mut rfb = at_security(SecurityPolicy::default());
    // No authentication is offered too, but this profile requires a password.
    rfb.receive(&[2, 1, 16]).expect("types");
    assert_eq!(rfb.take_output(), [16]);
    rfb.receive(&tight_list(&[
        capability(7, *b"VEND", *b"SOMETUNL"),
        capability(0, *b"TGHT", *b"NOTUNNEL"),
    ]))
    .expect("tunnels");
    assert_eq!(rfb.take_output(), [0, 0, 0, 0], "no tunnel");
    // VNC Authentication is taken though no authentication is offered first.
    rfb.receive(&tight_list(&[
        capability(1, *b"STDV", *b"NOAUTH__"),
        capability(2, *b"STDV", *b"VNCAUTH_"),
    ]))
    .expect("authentication types");
    assert_eq!(rfb.take_output(), [0, 0, 0, 2]);
    assert_eq!(
        rfb.receive(&CHALLENGE).expect("challenge"),
        [RfbEvent::PasswordRequired]
    );
    rfb.answer_password(b"Secret12").expect("answered");
    assert_eq!(rfb.take_output(), RESPONSE);
    rfb.receive(&[0, 0, 0, 0]).expect("result");
    assert_eq!(rfb.take_output(), [1], "ClientInit, shared");

    let mut init = server_init(2, 1, b"tight");
    init.extend(tight_init_capabilities());
    let (head, last) = init.split_at(init.len() - 1);
    assert!(
        rfb.receive(head).expect("most of it").is_empty(),
        "the capability lists are waited for"
    );
    assert_eq!(
        rfb.receive(last).expect("init"),
        [RfbEvent::Connected {
            width: 2,
            height: 1,
            name: "tight".to_owned()
        }]
    );
    assert_eq!(rfb.take_output(), opening_requests(2, 1));
    assert_eq!(
        rfb.security(),
        Some(Security {
            wrapper: Some(SecurityWrapper::Tight),
            authentication: Authentication::VncAuth,
            tls: false,
        })
    );

    // The next message is read where the lists end.
    let mut bytes = update(1);
    bytes.extend(rect_header(1, 0, 1, 1, 0));
    bytes.extend_from_slice(&[0, 255, 0, 0]);
    let area = Rect {
        x: 1,
        y: 0,
        width: 1,
        height: 1,
    };
    assert_eq!(
        rfb.receive(&bytes).expect("update"),
        [RfbEvent::Updated(area)]
    );
    assert_eq!(pixel(rfb.screen(), 1, 0), [0, 255, 0, 255]);
}

#[test]
fn tight_without_authentication_types_is_no_authentication_and_is_confirmed() {
    // 3.7: Tight confirms even no authentication, as noVNC reads it.
    let mut rfb = Rfb::new(NO_AUTHENTICATION);
    rfb.receive(b"RFB 003.007\n").expect("version");
    let _ = rfb.take_output();
    rfb.receive(&[1, 16]).expect("types");
    assert_eq!(rfb.take_output(), [16]);
    // No tunnel offered, none answered; no authentication type offered, none answered.
    rfb.receive(&[0, 0, 0, 0]).expect("tunnels");
    rfb.receive(&[0, 0, 0, 0]).expect("authentication types");
    assert!(rfb.take_output().is_empty());
    rfb.receive(&[0, 0, 0, 0]).expect("result");
    assert_eq!(rfb.take_output(), [1], "ClientInit");
    assert_eq!(
        rfb.security(),
        Some(Security {
            wrapper: Some(SecurityWrapper::Tight),
            authentication: Authentication::NoAuthentication,
            tls: false,
        })
    );
    let mut init = server_init(1, 1, b"open");
    init.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 0]);
    assert!(matches!(
        rfb.receive(&init).expect("init")[..],
        [RfbEvent::Connected { .. }]
    ));
}

#[test]
fn a_siemens_server_takes_no_tunnel_though_it_does_not_say_so() {
    let mut rfb = at_security(SecurityPolicy::default());
    rfb.receive(&[1, 16]).expect("types");
    let _ = rfb.take_output();
    rfb.receive(&tight_list(&[capability(1, *b"SICR", *b"SCHANNEL")]))
        .expect("tunnels");
    assert_eq!(rfb.take_output(), [0, 0, 0, 0]);
}

#[test]
fn vencrypt_0_2_takes_vnc_authentication_and_sends_no_more_than_the_subtype() {
    let mut rfb = at_security(SecurityPolicy::default());
    // No authentication is not allowed: VeNCrypt is taken.
    rfb.receive(&[2, 1, 19]).expect("types");
    assert_eq!(rfb.take_output(), [19]);
    rfb.receive(&[0, 2]).expect("version");
    assert_eq!(rfb.take_output(), [0, 2]);
    rfb.receive(&[0]).expect("accepted");
    // Plain and the TLS ones are not spoken; VNC Authentication is taken, though not first.
    rfb.receive(&vencrypt_subtypes(&[256, 259, 2, 1]))
        .expect("subtypes");
    assert_eq!(rfb.take_output(), [0, 0, 0, 2]);
    assert_eq!(
        rfb.receive(&CHALLENGE).expect("challenge"),
        [RfbEvent::PasswordRequired]
    );
    rfb.answer_password(b"Secret12").expect("answered");
    assert_eq!(rfb.take_output(), RESPONSE);
    rfb.receive(&[0, 0, 0, 0]).expect("result");
    assert_eq!(rfb.take_output(), [1]);
    rfb.receive(&server_init(2, 2, b"vencrypt")).expect("init");
    assert_eq!(rfb.take_output(), opening_requests(2, 2));
    assert_eq!(
        rfb.security(),
        Some(Security {
            wrapper: Some(SecurityWrapper::VeNCrypt),
            authentication: Authentication::VncAuth,
            tls: false,
        })
    );
}

#[test]
fn vencrypt_takes_no_authentication_when_allowed_confirmed_in_3_8_only() {
    let mut rfb = at_security(NO_AUTHENTICATION);
    rfb.receive(&[1, 19]).expect("types");
    rfb.receive(&[0, 2, 0]).expect("version and accepted");
    rfb.receive(&vencrypt_subtypes(&[257, 1]))
        .expect("subtypes");
    assert_eq!(rfb.take_output(), [19, 0, 2, 0, 0, 0, 1]);
    rfb.receive(&[0, 0, 0, 0]).expect("result");
    assert_eq!(rfb.take_output(), [1]);
    assert_eq!(
        rfb.security(),
        Some(Security {
            wrapper: Some(SecurityWrapper::VeNCrypt),
            authentication: Authentication::NoAuthentication,
            tls: false,
        })
    );

    // 3.7: straight to initialisation, as with no security offered directly.
    let mut rfb = Rfb::new(NO_AUTHENTICATION);
    rfb.receive(b"RFB 003.007\n").expect("version");
    let _ = rfb.take_output();
    rfb.receive(&[1, 19]).expect("types");
    rfb.receive(&[0, 2, 0]).expect("version and accepted");
    rfb.receive(&vencrypt_subtypes(&[1])).expect("subtypes");
    assert_eq!(rfb.take_output(), [19, 0, 2, 0, 0, 0, 1, 1]);
}

#[test]
fn a_vencrypt_version_other_than_0_2_or_refused_ends_the_connection() {
    let mut rfb = at_security(SecurityPolicy::default());
    rfb.receive(&[1, 19]).expect("types");
    let _ = rfb.take_output();
    assert_eq!(
        rfb.receive(&[0, 1]),
        Err(RfbError::UnsupportedVersion("VeNCrypt 0.1".to_owned()))
    );
    assert!(rfb.take_output().is_empty(), "no version answered");

    let mut rfb = at_security(SecurityPolicy::default());
    rfb.receive(&[1, 19]).expect("types");
    assert!(matches!(
        rfb.receive(&[0, 2, 1]),
        Err(RfbError::Protocol(_))
    ));
}

#[test]
fn counts_past_their_bounds_are_refused_before_their_lists_come() {
    // 65 Tight tunnels.
    let mut rfb = at_security(SecurityPolicy::default());
    rfb.receive(&[1, 16]).expect("types");
    assert!(matches!(
        rfb.receive(&[0, 0, 0, 65]),
        Err(RfbError::Protocol(_))
    ));
    // 64 are waited for.
    let mut rfb = at_security(SecurityPolicy::default());
    rfb.receive(&[1, 16]).expect("types");
    assert!(rfb.receive(&[0, 0, 0, 64]).expect("waiting").is_empty());

    // 4 billion Tight authentication types.
    let mut rfb = at_security(SecurityPolicy::default());
    rfb.receive(&[1, 16]).expect("types");
    assert!(matches!(
        rfb.receive(&[0, 0, 0, 0, 0xff, 0xff, 0xff, 0xff]),
        Err(RfbError::Protocol(_))
    ));

    // 65 VeNCrypt subtypes.
    let mut rfb = at_security(SecurityPolicy::default());
    rfb.receive(&[1, 19]).expect("types");
    assert!(matches!(
        rfb.receive(&[0, 2, 0, 65]),
        Err(RfbError::Protocol(_))
    ));

    // 257 encodings in Tight's capability lists after ServerInit.
    let mut rfb = at_security(NO_AUTHENTICATION);
    rfb.receive(&[1, 16]).expect("types");
    rfb.receive(&[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0])
        .expect("no tunnel, no authentication, accepted");
    let mut init = server_init(1, 1, b"many");
    init.extend_from_slice(&[0, 0, 0, 0, 1, 1, 0, 0]);
    assert!(matches!(rfb.receive(&init), Err(RfbError::Protocol(_))));
}

#[test]
fn only_types_not_spoken_are_refused_with_what_was_offered() {
    // TLS and Apple's.
    let mut rfb = at_security(NO_AUTHENTICATION);
    assert_eq!(
        rfb.receive(&[2, 18, 30]),
        Err(RfbError::NoAcceptableSecurity(vec![18, 30]))
    );

    // VeNCrypt's Plain and TLS subtypes.
    let mut rfb = at_security(NO_AUTHENTICATION);
    rfb.receive(&[1, 19]).expect("types");
    rfb.receive(&[0, 2, 0]).expect("version and accepted");
    assert_eq!(
        rfb.receive(&vencrypt_subtypes(&[256, 257, 262])),
        Err(RfbError::NoAcceptableInnerSecurity {
            wrapper: SecurityWrapper::VeNCrypt,
            offered: vec![256, 257, 262],
        })
    );

    // No VeNCrypt subtype at all.
    let mut rfb = at_security(NO_AUTHENTICATION);
    rfb.receive(&[1, 19]).expect("types");
    assert_eq!(
        rfb.receive(&[0, 2, 0, 0]),
        Err(RfbError::NoAcceptableInnerSecurity {
            wrapper: SecurityWrapper::VeNCrypt,
            offered: Vec::new(),
        })
    );

    // Tight tunnels without the no-tunnel one, or with it under another vendor.
    for tunnel in [
        capability(5, *b"TGHT", *b"NOTUNNEL"),
        capability(0, *b"VEND", *b"NOTUNNEL"),
    ] {
        let mut rfb = at_security(NO_AUTHENTICATION);
        rfb.receive(&[1, 16]).expect("types");
        assert!(matches!(
            rfb.receive(&tight_list(&[tunnel])),
            Err(RfbError::NoAcceptableInnerSecurity {
                wrapper: SecurityWrapper::Tight,
                ..
            })
        ));
    }

    // Tight's Unix login only.
    let mut rfb = at_security(NO_AUTHENTICATION);
    rfb.receive(&[1, 16]).expect("types");
    rfb.receive(&[0, 0, 0, 0]).expect("tunnels");
    assert_eq!(
        rfb.receive(&tight_list(&[capability(129, *b"TGHT", *b"ULGNAUTH")])),
        Err(RfbError::NoAcceptableInnerSecurity {
            wrapper: SecurityWrapper::Tight,
            offered: vec![129],
        })
    );
}

#[test]
fn a_profile_requiring_a_password_refuses_no_authentication_inside_a_wrapper() {
    // Tight offering no authentication by name.
    let mut rfb = at_security(SecurityPolicy::default());
    rfb.receive(&[1, 16]).expect("types");
    rfb.receive(&[0, 0, 0, 0]).expect("tunnels");
    assert_eq!(
        rfb.receive(&tight_list(&[capability(1, *b"STDV", *b"NOAUTH__")])),
        Err(RfbError::NoAcceptableInnerSecurity {
            wrapper: SecurityWrapper::Tight,
            offered: vec![1],
        })
    );
    assert_eq!(rfb.security(), None);

    // Tight offering no authentication type at all.
    let mut rfb = at_security(SecurityPolicy::default());
    rfb.receive(&[1, 16]).expect("types");
    assert_eq!(
        rfb.receive(&[0, 0, 0, 0, 0, 0, 0, 0]),
        Err(RfbError::NoAcceptableInnerSecurity {
            wrapper: SecurityWrapper::Tight,
            offered: vec![1],
        })
    );

    // VeNCrypt offering no authentication only.
    let mut rfb = at_security(SecurityPolicy::default());
    rfb.receive(&[1, 19]).expect("types");
    rfb.receive(&[0, 2, 0]).expect("version and accepted");
    assert_eq!(
        rfb.receive(&vencrypt_subtypes(&[1])),
        Err(RfbError::NoAcceptableInnerSecurity {
            wrapper: SecurityWrapper::VeNCrypt,
            offered: vec![1],
        })
    );
}

#[test]
fn the_type_taken_is_vencrypt_then_vnc_authentication_then_none_if_allowed_then_tight() {
    for (policy, offered, taken) in [
        (SecurityPolicy::default(), vec![19, 16, 1, 2], 19),
        (SecurityPolicy::default(), vec![16, 1, 2], 2),
        (NO_AUTHENTICATION, vec![16, 1], 1),
        (SecurityPolicy::default(), vec![16, 1], 16),
        (NO_AUTHENTICATION, vec![16], 16),
        (SecurityPolicy::default(), vec![1, 19], 19),
        (TLS_REQUIRED, vec![2, 19], 19),
    ] {
        let mut rfb = at_security(policy);
        let mut types = vec![u8::try_from(offered.len()).expect("few")];
        types.extend(&offered);
        rfb.receive(&types).expect("types");
        assert_eq!(rfb.take_output(), [taken], "offered {offered:?}");
    }
}

/// A session that took `VeNCrypt` 0.2 with `policy`, at its subtypes.
fn at_vencrypt_subtypes(policy: SecurityPolicy) -> Rfb {
    let mut rfb = at_security(policy);
    rfb.receive(&[1, 19]).expect("types");
    rfb.receive(&[0, 2, 0]).expect("version and accepted");
    let _ = rfb.take_output();
    rfb
}

/// The subtype a server offering `offered` gets from a client with `policy`.
fn subtype_taken(policy: SecurityPolicy, offered: &[u32]) -> Result<u32, RfbError> {
    let mut rfb = at_vencrypt_subtypes(policy);
    rfb.receive(&vencrypt_subtypes(offered))?;
    let output = rfb.take_output();
    Ok(u32::from_be_bytes(
        output[..].try_into().expect("one subtype"),
    ))
}

#[test]
fn x509_subtypes_go_first_plain_with_a_user_name_and_none_only_when_allowed() {
    let every = [1, 2, 256, 257, 258, 259, 260, 261, 262];
    // X509Vnc before all.
    assert_eq!(subtype_taken(SecurityPolicy::default(), &every), Ok(261));
    assert_eq!(subtype_taken(with_username("admin"), &every), Ok(261));
    // X509Plain next, only with a user name; never Plain (256) in clear.
    let no_vnc = [1, 2, 256, 260, 262];
    assert_eq!(subtype_taken(with_username("admin"), &no_vnc), Ok(262));
    assert_eq!(subtype_taken(SecurityPolicy::default(), &no_vnc), Ok(2));
    // X509None before anything in clear, only when no password is allowed.
    assert_eq!(subtype_taken(NO_AUTHENTICATION, &no_vnc), Ok(260));
    assert_eq!(
        subtype_taken(SecurityPolicy::default(), &[1, 260]),
        Err(RfbError::NoAcceptableInnerSecurity {
            wrapper: SecurityWrapper::VeNCrypt,
            offered: vec![1, 260],
        })
    );
    // The anonymous TLS ones are never taken.
    assert_eq!(subtype_taken(NO_AUTHENTICATION, &[257, 258, 259, 1]), Ok(1));
}

#[test]
fn x509_vnc_starts_tls_then_answers_the_challenge_inside_it() {
    let mut rfb = at_vencrypt_subtypes(SecurityPolicy::default());
    assert!(
        rfb.receive(&vencrypt_subtypes(&[2, 261]))
            .expect("subtypes")
            .is_empty()
    );
    assert_eq!(rfb.take_output(), [0, 0, 1, 5], "X509Vnc");
    // The server starts TLS: nothing more is read in clear.
    assert_eq!(rfb.receive(&[1]).expect("ack"), [RfbEvent::StartTls]);
    assert!(rfb.take_output().is_empty());
    assert!(rfb.tls_started().expect("started").is_empty());
    assert_eq!(
        rfb.receive(&CHALLENGE).expect("challenge"),
        [RfbEvent::PasswordRequired]
    );
    rfb.answer_password(b"Secret12").expect("answered");
    assert_eq!(rfb.take_output(), RESPONSE);
    rfb.receive(&[0, 0, 0, 0]).expect("result");
    assert_eq!(rfb.take_output(), [1], "ClientInit");
    assert_eq!(
        rfb.security(),
        Some(Security {
            wrapper: Some(SecurityWrapper::VeNCrypt),
            authentication: Authentication::VncAuth,
            tls: true,
        })
    );
    assert!(rfb.tls_started().is_err(), "TLS starts once");
}

#[test]
fn a_server_refusing_tls_or_talking_in_clear_where_it_starts_ends_the_connection() {
    let mut rfb = at_vencrypt_subtypes(SecurityPolicy::default());
    rfb.receive(&vencrypt_subtypes(&[261])).expect("subtypes");
    assert!(matches!(rfb.receive(&[0]), Err(RfbError::Protocol(_))));

    // The challenge right after the ack, in clear: refused, not answered.
    let mut rfb = at_vencrypt_subtypes(SecurityPolicy::default());
    rfb.receive(&vencrypt_subtypes(&[261])).expect("subtypes");
    let _ = rfb.take_output();
    let mut clear = vec![1];
    clear.extend_from_slice(&CHALLENGE);
    assert!(matches!(rfb.receive(&clear), Err(RfbError::Protocol(_))));
    assert!(rfb.take_output().is_empty());

    // TLS cannot start before the server says so.
    let mut rfb = at_vencrypt_subtypes(SecurityPolicy::default());
    rfb.receive(&vencrypt_subtypes(&[261])).expect("subtypes");
    assert!(rfb.tls_started().is_err());
}

#[test]
fn x509_plain_asks_the_password_once_tls_is_up_and_sends_both_with_their_lengths() {
    let mut rfb = at_vencrypt_subtypes(with_username("admin"));
    rfb.receive(&vencrypt_subtypes(&[262, 2]))
        .expect("subtypes");
    assert_eq!(rfb.take_output(), [0, 0, 1, 6], "X509Plain");
    assert_eq!(rfb.receive(&[1]).expect("ack"), [RfbEvent::StartTls]);
    assert!(
        rfb.answer_password(b"pass").is_err(),
        "no password before TLS is up"
    );
    assert_eq!(
        rfb.tls_started().expect("started"),
        [RfbEvent::PasswordRequired]
    );
    rfb.answer_password(b"pass").expect("answered");
    let mut expected = vec![0, 0, 0, 5, 0, 0, 0, 4];
    expected.extend_from_slice(b"adminpass");
    assert_eq!(rfb.take_output(), expected);
    rfb.receive(&[0, 0, 0, 0]).expect("result");
    assert_eq!(rfb.take_output(), [1]);
    assert_eq!(
        rfb.security(),
        Some(Security {
            wrapper: Some(SecurityWrapper::VeNCrypt),
            authentication: Authentication::Plain,
            tls: true,
        })
    );
}

#[test]
fn plain_credentials_past_their_bounds_are_refused_and_nothing_is_sent() {
    for (username, password, which) in [
        (
            "u".repeat(MAX_PLAIN_USERNAME + 1),
            b"pass".to_vec(),
            TooLong::Username,
        ),
        (
            "admin".to_owned(),
            vec![b'p'; MAX_PLAIN_PASSWORD + 1],
            TooLong::Password,
        ),
    ] {
        let mut rfb = at_vencrypt_subtypes(with_username(&username));
        rfb.receive(&vencrypt_subtypes(&[262])).expect("subtypes");
        rfb.receive(&[1]).expect("ack");
        rfb.tls_started().expect("started");
        let _ = rfb.take_output();
        assert_eq!(
            rfb.answer_password(&password),
            Err(RfbError::CredentialTooLong(which))
        );
        assert!(rfb.take_output().is_empty(), "nothing sent");
    }
}

#[test]
fn x509_none_needs_no_password_allowed_and_is_confirmed_inside_tls() {
    let mut rfb = at_vencrypt_subtypes(NO_AUTHENTICATION);
    rfb.receive(&vencrypt_subtypes(&[260])).expect("subtypes");
    rfb.receive(&[1]).expect("ack");
    assert!(rfb.tls_started().expect("started").is_empty());
    assert_eq!(rfb.take_output(), [0, 0, 1, 4], "X509None, nothing else");
    rfb.receive(&[0, 0, 0, 0]).expect("result");
    assert_eq!(rfb.take_output(), [1]);
}

#[test]
fn a_profile_requiring_tls_refuses_any_server_offering_none_never_falling_back_to_clear() {
    // No VeNCrypt at all: refused before anything is answered.
    let mut rfb = at_security(TLS_REQUIRED);
    assert_eq!(
        rfb.receive(&[3, 2, 1, 16]),
        Err(RfbError::TlsRequired(vec![2, 1, 16]))
    );
    assert!(rfb.take_output().is_empty());
    // Version 3.3: the server naming VNC Authentication.
    let mut rfb = Rfb::new(TLS_REQUIRED);
    rfb.receive(b"RFB 003.003\n").expect("version");
    assert_eq!(
        rfb.receive(&[0, 0, 0, 2]),
        Err(RfbError::TlsRequired(vec![2]))
    );
    // VeNCrypt without an X509 subtype, VNC Authentication and none offered in clear.
    let mut rfb = at_vencrypt_subtypes(SecurityPolicy {
        allow_no_authentication: true,
        ..TLS_REQUIRED
    });
    assert_eq!(
        rfb.receive(&vencrypt_subtypes(&[2, 1, 258])),
        Err(RfbError::TlsRequired(vec![2, 1, 258]))
    );
    assert!(rfb.take_output().is_empty(), "no subtype answered");
    // With one, it is taken.
    assert_eq!(subtype_taken(TLS_REQUIRED, &[2, 261]), Ok(261));
}

#[test]
fn nothing_accepted_inside_vencrypt_asks_to_connect_again_without_it_only_when_that_helps() {
    // VeNCrypt and VNC Authentication offered, only TLSVnc inside VeNCrypt.
    let mut rfb = at_security(SecurityPolicy::default());
    rfb.receive(&[2, 19, 2]).expect("types");
    rfb.receive(&[0, 2, 0]).expect("version and accepted");
    let _ = rfb.take_output();
    assert_eq!(
        rfb.receive(&vencrypt_subtypes(&[258])),
        Err(RfbError::RetryWithoutVencrypt(vec![258]))
    );
    assert!(rfb.take_output().is_empty(), "no subtype answered");
    // Again, VeNCrypt left out: VNC Authentication.
    let mut rfb = at_security(SecurityPolicy {
        exclude_vencrypt: true,
        ..SecurityPolicy::default()
    });
    rfb.receive(&[2, 19, 2]).expect("types");
    assert_eq!(rfb.take_output(), [2]);
    assert_eq!(
        rfb.receive(&CHALLENGE).expect("challenge"),
        [RfbEvent::PasswordRequired]
    );

    // TLS required: refused, never asked to go on in clear.
    let mut rfb = at_security(TLS_REQUIRED);
    rfb.receive(&[2, 19, 2]).expect("types");
    rfb.receive(&[0, 2, 0]).expect("version and accepted");
    assert_eq!(
        rfb.receive(&vencrypt_subtypes(&[258])),
        Err(RfbError::TlsRequired(vec![258]))
    );

    // VeNCrypt alone, or beside a type the policy refuses: nothing else to try.
    for types in [vec![1, 19], vec![2, 19, 1]] {
        let mut rfb = at_security(SecurityPolicy::default());
        rfb.receive(&types).expect("types");
        rfb.receive(&[0, 2, 0]).expect("version and accepted");
        assert!(matches!(
            rfb.receive(&vencrypt_subtypes(&[258])),
            Err(RfbError::NoAcceptableInnerSecurity { .. })
        ));
    }
}

/// The Adler-32 of `data`, RFC 1950.
fn adler32(data: &[u8]) -> u32 {
    const MODULO: u32 = 65_521;
    let (mut a, mut b) = (1_u32, 0_u32);
    for byte in data {
        a = (a + u32::from(*byte)) % MODULO;
        b = (b + a) % MODULO;
    }
    (b << 16) | a
}

/// A zlib stream of `data` in one stored block, written by hand from RFC 1950 and 1951:
/// header, final stored block, its length and complement, the data, the Adler-32.
fn zlib_stored(data: &[u8]) -> Vec<u8> {
    let length = u16::try_from(data.len()).expect("one block");
    let mut bytes = vec![0x78, 0x01, 0x01];
    bytes.extend_from_slice(&length.to_le_bytes());
    bytes.extend_from_slice(&(!length).to_le_bytes());
    bytes.extend_from_slice(data);
    bytes.extend_from_slice(&adler32(data).to_be_bytes());
    bytes
}

/// A cut text message of type `kind` carrying the extended clipboard `message`: its length
/// negative.
fn extended(kind: u8, message: &[u8]) -> Vec<u8> {
    let size = -i32::try_from(message.len()).expect("short");
    let mut bytes = vec![kind, 0, 0, 0];
    bytes.extend_from_slice(&size.to_be_bytes());
    bytes.extend_from_slice(message);
    bytes
}

/// From the server.
fn server_extended(message: &[u8]) -> Vec<u8> {
    extended(3, message)
}

/// From the client.
fn client_extended(message: &[u8]) -> Vec<u8> {
    extended(6, message)
}

/// Server caps flags: every action, and the text format.
const SERVER_CAPS_ALL: [u8; 4] = [0x1F, 0, 0, 0x01];

/// Server caps of `flags`, the text's longest unasked size `text_max`.
fn server_caps(flags: [u8; 4], text_max: u32) -> Vec<u8> {
    let mut message = flags.to_vec();
    message.extend_from_slice(&text_max.to_be_bytes());
    server_extended(&message)
}

/// The client's caps: caps, request, peek, notify and provide, the text format, and the
/// longest text it takes, `MAX_CUT_TEXT` (1 MiB).
const CLIENT_CAPS: [u8; 8] = [0x1F, 0, 0, 0x01, 0x00, 0x10, 0x00, 0x00];

/// A session whose server announced the Extended Clipboard with `flags`, its caps answered.
fn extended_session(flags: [u8; 4], text_max: u32) -> Rfb {
    let mut rfb = opened(2, 2);
    assert!(
        rfb.receive(&server_caps(flags, text_max))
            .expect("caps")
            .is_empty()
    );
    assert_eq!(rfb.take_output(), client_extended(&CLIENT_CAPS));
    rfb
}

/// A provide of text from the server: its length, then it, in a stored zlib stream.
fn server_provide(text: &[u8]) -> Vec<u8> {
    let mut inflated = u32::try_from(text.len())
        .expect("short")
        .to_be_bytes()
        .to_vec();
    inflated.extend_from_slice(text);
    let mut message = vec![0x10, 0, 0, 0x01];
    message.extend(zlib_stored(&inflated));
    server_extended(&message)
}

/// Inflates a client provide's payload with flate2 directly.
fn inflate(compressed: &[u8]) -> Vec<u8> {
    use std::io::Read as _;
    let mut inflated = Vec::new();
    flate2::read::ZlibDecoder::new(compressed)
        .read_to_end(&mut inflated)
        .expect("a zlib stream");
    inflated
}

#[test]
fn the_extended_clipboard_is_asked_for_and_its_caps_are_answered_as_novnc() {
    // Asked after the desktop name, as noVNC's `_sendEncodings`.
    assert!(
        opening_requests(2, 2)
            .windows(4)
            .any(|word| word == [0xC0, 0xA1, 0xE5, 0xCE])
    );
    let mut rfb = extended_session(SERVER_CAPS_ALL, 10 << 20);
    // Caps again are answered again.
    rfb.receive(&server_caps(SERVER_CAPS_ALL, 0)).expect("caps");
    assert_eq!(rfb.take_output(), client_extended(&CLIENT_CAPS));
}

#[test]
fn the_clipboard_is_notified_then_provided_in_utf_8_with_cr_lf_and_a_nul() {
    let mut rfb = extended_session(SERVER_CAPS_ALL, 10 << 20);
    rfb.cut_text("\u{e9}\n\u{263a}");
    assert_eq!(
        rfb.take_output(),
        client_extended(&[0x08, 0, 0, 0x01]),
        "notify, text"
    );
    // The server asks for the text.
    assert!(
        rfb.receive(&server_extended(&[0x02, 0, 0, 0x01]))
            .expect("request")
            .is_empty()
    );
    let output = rfb.take_output();
    assert_eq!(output[..4], [6, 0, 0, 0]);
    let size = i32::from_be_bytes([output[4], output[5], output[6], output[7]]);
    assert_eq!(usize::try_from(-size).expect("negative"), output.len() - 8);
    assert_eq!(output[8..12], [0x10, 0, 0, 0x01], "provide, text");
    assert_eq!(output[12], 0x78, "a zlib stream");
    assert_eq!(
        inflate(&output[12..]),
        [
            0, 0, 0, 8, // the length, NUL included
            0xC3, 0xA9, // e acute
            b'\r', b'\n', // the line ending made CR LF
            0xE2, 0x98, 0xBA, // the smiling face
            0,    // the NUL
        ]
    );
    // Asked again, the same text is provided again; a peek is told the text is there.
    rfb.receive(&server_extended(&[0x02, 0, 0, 0x01]))
        .expect("request");
    assert_eq!(rfb.take_output(), output);
    rfb.receive(&server_extended(&[0x04, 0, 0, 0]))
        .expect("peek");
    assert_eq!(rfb.take_output(), client_extended(&[0x08, 0, 0, 0x01]));
    // Once the server provides its own, nothing is left to provide.
    rfb.receive(&server_provide(b"x\0")).expect("provide");
    rfb.receive(&server_extended(&[0x04, 0, 0, 0]))
        .expect("peek");
    assert_eq!(rfb.take_output(), client_extended(&[0x08, 0, 0, 0]));
    rfb.receive(&server_extended(&[0x02, 0, 0, 0x01]))
        .expect("request");
    assert!(rfb.take_output().is_empty());
}

#[test]
fn a_server_notify_is_answered_by_a_request_and_its_provide_is_utf_8() {
    let mut rfb = extended_session(SERVER_CAPS_ALL, 10 << 20);
    assert!(
        rfb.receive(&server_extended(&[0x08, 0, 0, 0x01]))
            .expect("notify")
            .is_empty()
    );
    assert_eq!(rfb.take_output(), client_extended(&[0x02, 0, 0, 0x01]));
    // A notify without text asks for nothing.
    rfb.receive(&server_extended(&[0x08, 0, 0, 0]))
        .expect("notify");
    assert!(rfb.take_output().is_empty());

    let text = "caf\u{e9} \u{6f22}\u{5b57} \u{1f389}\r\nfin\0";
    let mut bytes = server_provide(text.as_bytes());
    bytes.push(2); // then a bell
    // Fed byte by byte, it is read once whole.
    let mut events = Vec::new();
    for byte in bytes {
        events.extend(rfb.receive(&[byte]).expect("provide"));
    }
    assert_eq!(
        events,
        [
            RfbEvent::ServerCutText("caf\u{e9} \u{6f22}\u{5b57} \u{1f389}\nfin".to_owned()),
            RfbEvent::Bell
        ],
        "CR LF made LF, the NUL taken off, as noVNC"
    );
}

#[test]
fn without_the_extended_clipboard_or_its_text_format_the_clipboard_is_latin_1() {
    let latin1 = [6, 0, 0, 0, 0, 0, 0, 2, 0xe9, b'?'];
    // Not announced.
    let mut rfb = opened(2, 2);
    rfb.cut_text("\u{e9}\u{263a}");
    assert_eq!(rfb.take_output(), latin1);
    // Announced without the text format.
    let mut rfb = opened(2, 2);
    rfb.receive(&server_extended(&[0x1F, 0, 0, 0]))
        .expect("caps");
    assert_eq!(rfb.take_output(), client_extended(&CLIENT_CAPS));
    rfb.cut_text("\u{e9}\u{263a}");
    assert_eq!(rfb.take_output(), latin1);
}

#[test]
fn a_server_taking_no_notify_is_provided_at_once_within_the_size_it_takes_unasked() {
    // Caps and provide only; text up to 6 bytes, NUL included.
    let mut rfb = extended_session([0x11, 0, 0, 0x01], 6);
    rfb.cut_text("\u{e9}t\u{e9}");
    let output = rfb.take_output();
    assert_eq!(output[8..12], [0x10, 0, 0, 0x01], "provide, text, unasked");
    assert_eq!(
        inflate(&output[12..]),
        [0, 0, 0, 6, 0xC3, 0xA9, b't', 0xC3, 0xA9, 0]
    );
    // Past the size: Latin-1.
    rfb.cut_text("\u{e9}t\u{e9}s");
    assert_eq!(
        rfb.take_output(),
        [6, 0, 0, 0, 0, 0, 0, 4, 0xe9, b't', 0xe9, b's']
    );
}

#[test]
fn an_oversized_or_bombing_extended_clipboard_is_dropped_and_the_session_goes_on() {
    use std::io::Write as _;

    let mut rfb = extended_session(SERVER_CAPS_ALL, 10 << 20);
    // A text announced past the bound is not inflated.
    let size = u32::try_from(MAX_CUT_TEXT + 1).expect("fits");
    let mut message = vec![0x10, 0, 0, 0x01];
    message.extend(zlib_stored(&size.to_be_bytes()));
    let mut bytes = server_extended(&message);
    bytes.push(2);
    assert_eq!(rfb.receive(&bytes).expect("dropped"), [RfbEvent::Bell]);

    // A zip bomb: 8 MiB of zeros announced as 1 GiB, in a few kilobytes.
    let mut deflater = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::best());
    deflater
        .write_all(&(1_u32 << 30).to_be_bytes())
        .expect("size");
    let zeros = vec![0; 1 << 20];
    for _ in 0..8 {
        deflater.write_all(&zeros).expect("zeros");
    }
    let mut message = vec![0x10, 0, 0, 0x01];
    message.extend(deflater.finish().expect("deflated"));
    assert!(message.len() < MAX_EXTENDED_CUT_TEXT);
    let mut bytes = server_extended(&message);
    bytes.push(2);
    assert_eq!(rfb.receive(&bytes).expect("dropped"), [RfbEvent::Bell]);

    // A message past its bound is read and dropped in pieces.
    let size = -i32::try_from(MAX_EXTENDED_CUT_TEXT + 1).expect("fits");
    let mut bytes = vec![3, 0, 0, 0];
    bytes.extend_from_slice(&size.to_be_bytes());
    assert!(rfb.receive(&bytes).expect("header").is_empty());
    let chunk = vec![0; 1 << 16];
    let mut left = MAX_EXTENDED_CUT_TEXT + 1;
    while left > 0 {
        let piece = left.min(chunk.len());
        assert!(rfb.receive(&chunk[..piece]).expect("dropped").is_empty());
        left -= piece;
    }
    assert_eq!(rfb.receive(&[2]).expect("bell"), [RfbEvent::Bell]);
}

#[test]
fn a_malformed_extended_clipboard_is_a_protocol_error() {
    let garbage = server_extended(&[0x10, 0, 0, 0x01, 0xDE, 0xAD, 0xBE, 0xEF]);
    let truncated = {
        let mut message = vec![0x10, 0, 0, 0x01];
        message.extend(zlib_stored(&[0, 0, 0, 10, b'a', b'b', b'c']));
        server_extended(&message)
    };
    let not_utf8 = server_provide(b"caf\xe9\0");
    let cases: [(&str, Vec<u8>); 6] = [
        ("shorter than its flags", server_extended(&[0x10, 0])),
        (
            "caps missing a size",
            server_extended(&[0x1F, 0, 0, 0x03, 0, 0, 0, 1]),
        ),
        ("no action", server_extended(&[0, 0, 0, 0x01])),
        ("garbage for zlib", garbage),
        ("a text shorter than announced", truncated),
        ("a text not UTF-8", not_utf8),
    ];
    for (what, bytes) in cases {
        let mut rfb = extended_session(SERVER_CAPS_ALL, 10 << 20);
        assert!(
            matches!(rfb.receive(&bytes), Err(RfbError::Protocol(_))),
            "{what}"
        );
    }
}

/// Feeds `bytes` one at a time, as a server cutting them anywhere would: the events of all.
fn fed_byte_by_byte(rfb: &mut Rfb, bytes: &[u8]) -> Vec<RfbEvent> {
    let mut events = Vec::new();
    for byte in bytes {
        events.extend(rfb.receive(std::slice::from_ref(byte)).expect("fed"));
    }
    events
}

#[test]
fn hextile_rre_and_tight_png_rectangles_are_drawn_whole_even_fed_byte_by_byte() {
    let mut rfb = opened(4, 3);
    let mut bytes = update(3);
    // Hextile, 4 by 2: one tile, a blue background, a red foreground subrectangle at 1,1 of
    // 2 by 1.
    bytes.extend(rect_header(0, 0, 4, 2, 5));
    bytes.extend_from_slice(&[
        0x02 | 0x04 | 0x08,
        0,
        0,
        255,
        0,
        255,
        0,
        0,
        0,
        1,
        0x11,
        0x10,
    ]);
    // RRE, 2 by 1 at 2,2: a green background, a white subrectangle at 1,0.
    bytes.extend(rect_header(2, 2, 2, 1, 2));
    bytes.extend_from_slice(&[
        0, 0, 0, 1, 0, 255, 0, 0, 255, 255, 255, 0, 0, 1, 0, 0, 0, 1, 0, 1,
    ]);
    // TightPNG, 1 by 1 at 0,2: a grey PNG.
    let image = GREY_77_PNG;
    bytes.extend(rect_header(0, 2, 1, 1, -260));
    bytes.push(0xa0);
    bytes.push(u8::try_from(image.len()).expect("a short image"));
    bytes.extend_from_slice(&image);
    let events = fed_byte_by_byte(&mut rfb, &bytes);
    let rect = |x, y, width, height| Rect {
        x,
        y,
        width,
        height,
    };
    assert_eq!(
        events,
        [
            RfbEvent::Updated(rect(0, 0, 4, 2)),
            RfbEvent::Updated(rect(2, 2, 2, 1)),
            RfbEvent::Updated(rect(0, 2, 1, 1)),
        ]
    );
    let screen = rfb.screen();
    assert_eq!(pixel(screen, 0, 0), [0, 0, 255, 255]);
    assert_eq!(pixel(screen, 1, 1), [255, 0, 0, 255]);
    assert_eq!(pixel(screen, 2, 1), [255, 0, 0, 255]);
    assert_eq!(pixel(screen, 3, 1), [0, 0, 255, 255]);
    assert_eq!(pixel(screen, 2, 2), [0, 255, 0, 255]);
    assert_eq!(pixel(screen, 3, 2), [255, 255, 255, 255]);
    assert_eq!(pixel(screen, 0, 2), [77, 77, 77, 255]);
    assert_eq!(rfb.take_output(), full_request(true, 4, 3));
}

/// A 1 by 1 PNG of grey 77, 8 bits, unfiltered: written once with Python's zlib and
/// `zlib.crc32`, IHDR, IDAT and IEND, as a server's encoder would.
const GREY_77_PNG: [u8; 67] = [
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x00, 0x00, 0x00, 0x00, 0x3a, 0x7e, 0x9b,
    0x55, 0x00, 0x00, 0x00, 0x0a, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0xf0, 0x05, 0x00, 0x00,
    0x4f, 0x00, 0x4e, 0x69, 0x8b, 0x01, 0x6c, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae,
    0x42, 0x60, 0x82,
];

/// A malformed rectangle: what is wrong, its encoding, its size and its bytes.
type Malformed = (&'static str, i32, (u16, u16), Vec<u8>);

#[test]
fn malformed_hextile_rre_tight_png_and_cursor_rectangles_are_protocol_errors() {
    let cases: [Malformed; 7] = [
        ("Hextile flags past 30", 5, (2, 2), vec![31]),
        (
            "a Hextile subrectangle outside its tile",
            5,
            (2, 2),
            vec![0x08, 1, 0x11, 0x10],
        ),
        (
            "an RRE subrectangle outside its rectangle",
            2,
            (2, 2),
            vec![0, 0, 0, 1, 0, 0, 0, 0, 1, 1, 1, 0, 0, 1, 0, 0, 0, 2, 0, 1],
        ),
        (
            "more RRE subrectangles than pixels",
            2,
            (2, 2),
            vec![0, 0, 0, 5, 0, 0, 0, 0],
        ),
        (
            "basic compression in TightPNG",
            -260,
            (1, 1),
            vec![0x00, 1, 2, 3],
        ),
        ("not a PNG", -260, (1, 1), vec![0xa0, 3, 1, 2, 3]),
        ("a cursor past its bound", -239, (257, 1), Vec::new()),
    ];
    for (what, encoding, (width, height), body) in cases {
        let mut rfb = opened(600, 4);
        let mut bytes = update(1);
        bytes.extend(rect_header(0, 0, width, height, encoding));
        bytes.extend(body);
        assert!(
            matches!(rfb.receive(&bytes), Err(RfbError::Protocol(_))),
            "{what}"
        );
    }
    // A Hextile or RRE rectangle outside the desktop, refused before its bytes.
    for encoding in [5, 2] {
        let mut rfb = opened(2, 2);
        let mut bytes = update(1);
        bytes.extend(rect_header(1, 1, 2, 2, encoding));
        assert!(matches!(rfb.receive(&bytes), Err(RfbError::Protocol(_))));
    }
}

#[test]
fn the_cursor_is_decoded_with_its_mask_and_an_empty_one_hides_it() {
    let mut rfb = opened(4, 4);
    let mut bytes = update(2);
    // 3 by 2, pointing at 2,1: rows "red shown, green masked, blue shown" and "all masked".
    bytes.extend(rect_header(2, 1, 3, 2, -239));
    bytes.extend_from_slice(&[255, 0, 0, 0, 0, 255, 0, 0, 0, 0, 255, 0]);
    bytes.extend_from_slice(&[1; 12]);
    bytes.extend_from_slice(&[0b1010_0000, 0]);
    // An empty one: hidden.
    bytes.extend(rect_header(0, 0, 0, 0, -239));
    let events = fed_byte_by_byte(&mut rfb, &bytes);
    let [RfbEvent::Cursor(shape), RfbEvent::Cursor(empty)] = events.as_slice() else {
        panic!("{events:?}");
    };
    assert_eq!(
        (shape.width(), shape.height(), shape.hotspot()),
        (3, 2, (2, 1))
    );
    assert_eq!(
        shape.rgba()[..12],
        [255, 0, 0, 255, 0, 255, 0, 0, 0, 0, 255, 255]
    );
    assert!(shape.rgba()[12..].chunks(4).all(|pixel| pixel[3] == 0));
    assert!(!shape.is_invisible());
    assert!(empty.is_invisible());
    assert_eq!((empty.width(), empty.height()), (0, 0));
    // The desktop is untouched, and the update ends as any other.
    assert_eq!(pixel(rfb.screen(), 2, 1), [0, 0, 0, 255]);
    assert_eq!(rfb.take_output(), full_request(true, 4, 4));
}

/// A key given, by keysym and scancode, down or up, and the bytes it sends.
type KeySent = (Option<u32>, Option<u16>, bool, Vec<u8>);

/// QEMU's extended key event: message 255, subtype 0, down, the keysym and the keycode.
fn qemu_key(down: bool, keysym: u32, keycode: u32) -> Vec<u8> {
    let mut bytes = vec![255, 0, 0, u8::from(down)];
    bytes.extend_from_slice(&keysym.to_be_bytes());
    bytes.extend_from_slice(&keycode.to_be_bytes());
    bytes
}

#[test]
fn keys_go_with_their_scancode_once_the_server_takes_them_and_plainly_before() {
    let mut rfb = opened(2, 2);
    // Before the server says it takes scancodes: the keysym alone, nothing without one.
    rfb.key_with_scancode(Some(0x61), Some(0x1e), true);
    rfb.key_with_scancode(None, Some(0x1e), true);
    assert_eq!(rfb.take_output(), [4, 1, 0, 0, 0, 0, 0, 0x61]);
    assert!(!rfb.takes_scancodes());

    // The server echoes QEMU's pseudo-encoding as an empty rectangle.
    let mut bytes = update(1);
    bytes.extend(rect_header(0, 0, 0, 0, -258));
    assert!(rfb.receive(&bytes).expect("update").is_empty());
    assert!(rfb.takes_scancodes());
    let _ = rfb.take_output();

    let keys: [KeySent; 7] = [
        // A letter: 'a' at KeyA.
        (Some(0x61), Some(0x1e), true, qemu_key(true, 0x61, 0x1e)),
        // AltGr: ISO_Level3_Shift at the extended 0x38, sent as 0xB8.
        (
            Some(0xfe03),
            Some(0xe038),
            true,
            qemu_key(true, 0xfe03, 0xb8),
        ),
        // Numpad 7 with Num Lock: KP_7 at 0x47.
        (
            Some(0xffb7),
            Some(0x47),
            false,
            qemu_key(false, 0xffb7, 0x47),
        ),
        // Numpad Enter: the extended 0x1C, sent as 0x9C.
        (
            Some(0xff8d),
            Some(0xe01c),
            true,
            qemu_key(true, 0xff8d, 0x9c),
        ),
        // F1 and F17, the latter extended.
        (Some(0xffbe), Some(0x3b), true, qemu_key(true, 0xffbe, 0x3b)),
        (
            Some(0xffce),
            Some(0xe003),
            true,
            qemu_key(true, 0xffce, 0x83),
        ),
        // A key nothing types: the scancode alone, keysym 0.
        (None, Some(0x56), true, qemu_key(true, 0, 0x56)),
    ];
    for (keysym, scancode, down, expected) in keys {
        rfb.key_with_scancode(keysym, scancode, down);
        assert_eq!(rfb.take_output(), expected, "{keysym:?} {scancode:?}");
    }
    // No scancode known: a plain key event still; neither: nothing.
    rfb.key_with_scancode(Some(0x20ac), None, true);
    assert_eq!(rfb.take_output(), [4, 1, 0, 0, 0, 0, 0x20, 0xac]);
    rfb.key_with_scancode(None, None, true);
    assert!(rfb.take_output().is_empty());
}
