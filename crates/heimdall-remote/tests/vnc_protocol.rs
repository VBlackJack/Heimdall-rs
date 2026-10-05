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

use heimdall_remote::vnc::{MAX_CUT_TEXT, Rect, Rfb, RfbError, RfbEvent, SecurityPolicy, Version};

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
};

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

/// What the client sends once the session opens on a `width` by `height` desktop.
fn opening_requests(width: u16, height: u16) -> Vec<u8> {
    let mut bytes = vec![
        // SetPixelFormat: 32 bits, depth 24, little-endian, true colour, red lowest.
        0, 0, 0, 0, 32, 24, 0, 1, 0, 255, 0, 255, 0, 255, 0, 8, 16, 0, 0, 0,
        // SetEncodings: 6 of them.
        2, 0, 0, 6,
    ];
    for encoding in [16_i32, 1, 0, -223, -224, -308] {
        bytes.extend_from_slice(&encoding.to_be_bytes());
    }
    bytes.extend_from_slice(&full_request(false, width, height));
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
    // VeNCrypt and VNC Authentication offered: the second is chosen.
    rfb.receive(&[2, 19, 2]).expect("types");
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
