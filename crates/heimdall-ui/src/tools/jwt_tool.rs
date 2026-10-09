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

//! The JWT parser, as the C# `JwtParserView` and `JwtParserViewModel`: a token pasted is
//! read as it is typed; its header and payload shown indented, its signature in
//! hexadecimal, each with its copy button; its `exp` against the clock; an HS256, HS384 or
//! HS512 signature checked against a secret typed, an RSA or ECDSA one said not checked.
//!
//! The secret is never logged nor kept: it lives in the tab, wiped from memory when the tab
//! closes or the secret is retyped.

use std::fmt;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use heimdall_app::TabId;
use heimdall_core::tools::jwt_parser::{
    self, Expiration, HmacVerification, JwtAlgorithm, JwtDecodeError, JwtDecoded,
};
use iced::widget::text_editor::{Action, Content};
use iced::widget::{column, container, row, text, text_input};
use iced::{Alignment, Color, Element, Length, Theme};
use zeroize::Zeroizing;

use super::crypto_parts::{self, Tone};
use super::{CopySlot, ToolMessage, ToolPane};
use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::themes::colors_of;
use crate::tokens::{font_size, spacing};

/// Height of the token's box, as the C# `Height="80"`.
const INPUT_HEIGHT: f32 = 80.0;

/// Padding of the expiry's box, as the C# `Padding="10,6"`.
const EXPIRY_PADDING: [f32; 2] = [6.0, 10.0];

/// Padding of a part's box, as the C# `SpacingSm`.
const PART_PADDING: f32 = spacing::SM;

/// Padding of the verification's card, as the C# `SpacingMd`.
const VERIFY_PADDING: f32 = spacing::MD;

/// What a signature found valid is said with, as the C# heavy check mark.
const VALID_MARK: &str = "\u{2714}";

/// What a signature found invalid is said with, as the C# heavy cross.
const INVALID_MARK: &str = "\u{2716}";

/// What the JWT parser is asked.
#[derive(Clone)]
pub enum JwtMessage {
    /// Something done in the token's box.
    Input(Action),
    /// Something done in the header's box, which does not change it.
    Header(Action),
    /// In the payload's.
    Payload(Action),
    /// In the signature's.
    Signature(Action),
    /// The secret typed.
    Secret(String),
    /// Check the signature against the secret.
    Verify,
    /// Copy the header.
    CopyHeader,
    /// Copy the payload.
    CopyPayload,
    /// Copy the signature.
    CopySignature,
}

impl fmt::Debug for JwtMessage {
    /// The secret typed is never written out.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Input(_) => "Input(..)",
            Self::Header(_) => "Header(..)",
            Self::Payload(_) => "Payload(..)",
            Self::Signature(_) => "Signature(..)",
            Self::Secret(_) => "Secret(..)",
            Self::Verify => "Verify",
            Self::CopyHeader => "CopyHeader",
            Self::CopyPayload => "CopyPayload",
            Self::CopySignature => "CopySignature",
        })
    }
}

/// A part of the token shown, its text and its box.
#[derive(Default)]
struct Part {
    text: String,
    content: Content,
}

impl Part {
    fn set(&mut self, text: String) {
        self.content = Content::with_text(&text);
        self.text = text;
    }
}

/// The JWT parser's state, as the C# view model's.
#[derive(Default)]
pub struct JwtPane {
    input: Content,
    decoded: Option<JwtDecoded>,
    algorithm: JwtAlgorithm,
    header: Part,
    payload: Part,
    signature: Part,
    error: Option<JwtDecodeError>,
    /// What the `exp` claim says; `None` when hidden, as for a claim not read.
    expiration: Option<Expiration>,
    secret: Zeroizing<String>,
    /// Whether the secret signs the token, once checked.
    verified: Option<bool>,
    /// Nothing pasted: the empty state is shown.
    blank: bool,
}

impl fmt::Debug for JwtPane {
    /// The secret is never written out.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("JwtPane")
            .field("decoded", &self.decoded.is_some())
            .field("algorithm", &self.algorithm)
            .field("error", &self.error)
            .field("expiration", &self.expiration)
            .field("verified", &self.verified)
            .finish_non_exhaustive()
    }
}

impl JwtPane {
    /// A new tab's state: nothing pasted.
    #[must_use]
    pub fn new() -> Self {
        Self {
            blank: true,
            ..Self::default()
        }
    }

    /// Applies `message`; what a copy button copies, when one is pressed.
    pub fn update(&mut self, message: JwtMessage) -> Option<(CopySlot, String)> {
        match message {
            JwtMessage::Input(action) => {
                let edit = action.is_edit();
                self.input.perform(action);
                if edit {
                    self.parse(now());
                }
            }
            JwtMessage::Header(action) => super::read_only(&mut self.header.content, action),
            JwtMessage::Payload(action) => super::read_only(&mut self.payload.content, action),
            JwtMessage::Signature(action) => {
                super::read_only(&mut self.signature.content, action);
            }
            JwtMessage::Secret(typed) => self.secret = Zeroizing::new(typed),
            JwtMessage::Verify => self.verify(),
            JwtMessage::CopyHeader => return copied(CopySlot::JwtHeader, &self.header.text),
            JwtMessage::CopyPayload => return copied(CopySlot::JwtPayload, &self.payload.text),
            JwtMessage::CopySignature => {
                return copied(CopySlot::JwtSignature, &self.signature.text);
            }
        }
        None
    }

    /// The token read at Unix second `now`, as the C# `Parse` (`JwtParserViewModel.cs:99-136`):
    /// nothing pasted clears everything; a token not read says why; one read shows its
    /// parts, its expiry and how its signature is checked, the last check forgotten.
    fn parse(&mut self, now: i64) {
        let typed = super::box_text(&self.input);
        let input = typed.trim();
        self.clear();
        if input.is_empty() {
            self.blank = true;
            return;
        }
        match jwt_parser::decode(input) {
            Err(error) => self.error = Some(error),
            Ok(decoded) => {
                self.header.set(decoded.pretty_header(super::NEW_LINE));
                self.payload.set(decoded.pretty_payload(super::NEW_LINE));
                self.signature.set(decoded.signature_hex());
                // As the C#: the expiry read from the payload as shown.
                self.expiration = Some(jwt_parser::evaluate_expiration(&self.payload.text, now))
                    .filter(|expiration| *expiration != Expiration::InvalidClaim);
                self.algorithm = jwt_parser::classify_algorithm(
                    jwt_parser::extract_algorithm(&decoded.header_json).as_deref(),
                );
                self.decoded = Some(decoded);
            }
        }
    }

    /// Everything shown forgotten, as the C# `ClearOutput`, the secret kept.
    fn clear(&mut self) {
        self.decoded = None;
        self.algorithm = JwtAlgorithm::Unknown;
        self.header = Part::default();
        self.payload = Part::default();
        self.signature = Part::default();
        self.error = None;
        self.expiration = None;
        self.verified = None;
        self.blank = false;
    }

    /// Whether the signature can be checked, as the C# `CanVerify`: a token read, an HMAC
    /// algorithm, a secret not blank.
    #[must_use]
    pub fn can_verify(&self) -> bool {
        self.decoded.is_some() && self.algorithm.is_hmac() && !self.secret.trim().is_empty()
    }

    /// The signature checked against the secret, as the C# `Verify`
    /// (`JwtParserViewModel.cs:138-163`).
    fn verify(&mut self) {
        let Some(decoded) = self.decoded.as_ref().filter(|_| self.can_verify()) else {
            return;
        };
        self.verified = match jwt_parser::verify_hmac(decoded, self.algorithm, &self.secret) {
            HmacVerification::Valid => Some(true),
            HmacVerification::Invalid => Some(false),
            HmacVerification::AlgorithmNotHmac | HmacVerification::MalformedInput => None,
        };
    }

    /// The tool's page, as the C# `JwtParserView.xaml`: the token, its expiry or what is
    /// wrong with it, the empty state, the three parts in their colours, and the signature's
    /// check.
    pub fn view<'a>(&'a self, tab: TabId, state: &ToolPane) -> Element<'a, Message> {
        let send = move |message| Message::Tool(tab, ToolMessage::Jwt(message));
        let input = super::text_box(&self.input, Some(fl!("ui-tool-jwt-placeholder")))
            .size(font_size::BODY)
            .height(INPUT_HEIGHT)
            .on_action(move |action| send(JwtMessage::Input(action)));
        let expiry = self.expiration.map(|expiration| {
            let (said, color): (String, fn(&Theme) -> Color) = match expiration {
                Expiration::Expired(at) => (fl!("ui-tool-jwt-expired", date = date(at)), |theme| {
                    theme.extended_palette().danger.base.color
                }),
                Expiration::Valid(at) => (fl!("ui-tool-jwt-valid", date = date(at)), |theme| {
                    theme.extended_palette().success.base.color
                }),
                Expiration::NoExpiry | Expiration::InvalidClaim => {
                    (fl!("ui-tool-jwt-no-expiry"), |theme| {
                        theme.extended_palette().secondary.base.color
                    })
                }
            };
            container(text(said).size(font_size::BODY).font(styles::SEMIBOLD))
                .padding(EXPIRY_PADDING)
                .width(Length::Fill)
                .style(crypto_parts::tinted(color))
        });
        let error = self.error.map(|error| {
            let said = match error {
                JwtDecodeError::InvalidFormat => fl!("ui-tool-jwt-error-format"),
                JwtDecodeError::DecodeFailed => fl!("ui-tool-jwt-error-decode"),
            };
            crypto_parts::said(said, Tone::Error, font_size::BODY, false)
        });
        let parts = [
            (
                fl!("ui-tool-jwt-header"),
                &self.header,
                (|theme| colors_of(theme).blue) as fn(&Theme) -> Color,
                CopySlot::JwtHeader,
                JwtMessage::CopyHeader,
                JwtMessage::Header as fn(Action) -> JwtMessage,
            ),
            (
                fl!("ui-tool-jwt-payload"),
                &self.payload,
                |theme| colors_of(theme).green,
                CopySlot::JwtPayload,
                JwtMessage::CopyPayload,
                JwtMessage::Payload,
            ),
            (
                fl!("ui-tool-jwt-signature"),
                &self.signature,
                |theme| colors_of(theme).orange,
                CopySlot::JwtSignature,
                JwtMessage::CopySignature,
                JwtMessage::Signature,
            ),
        ];
        let mut page = column![super::field_label(fl!("ui-tool-jwt-input")), input]
            .push(expiry)
            .push(error)
            .push(
                self.blank
                    .then(|| crypto_parts::empty_state(fl!("ui-tool-jwt-empty"))),
            )
            .spacing(spacing::SM);
        for (label, part, color, slot, copy, action) in parts {
            page = page.push(
                row![
                    text(label)
                        .size(font_size::BODY)
                        .font(styles::SEMIBOLD)
                        .style(move |theme: &Theme| text::Style {
                            color: Some(color(theme)),
                        })
                        .width(Length::Fill),
                    super::copy_button(
                        fl!("ui-tool-jwt-copy"),
                        state.copied(slot),
                        send(copy),
                        super::COPY_PADDING,
                    ),
                ]
                .align_y(Alignment::Center),
            );
            page = page.push(
                container(
                    crypto_parts::bare_box(&part.content, color)
                        .size(font_size::BODY)
                        .on_action(move |done| send(action(done))),
                )
                .padding(PART_PADDING)
                .style(crypto_parts::edged(color)),
            );
        }
        super::content_column(page.push(self.verify_card(send)))
    }

    /// The signature's check, as the C# verification card: for an HMAC, the secret, Verify
    /// and what it found; for RSA or ECDSA, that it is not checked; nothing otherwise.
    fn verify_card<'a>(
        &'a self,
        send: impl Fn(JwtMessage) -> Message + Copy + 'a,
    ) -> Option<Element<'a, Message>> {
        if !self.algorithm.is_hmac() && !self.algorithm.is_asymmetric() {
            return None;
        }
        let mut card = column![
            text(fl!("ui-tool-jwt-verify-title"))
                .size(font_size::BODY)
                .font(styles::SEMIBOLD)
        ]
        .spacing(spacing::SM);
        if self.algorithm.is_asymmetric() {
            card = card.push(crypto_parts::said(
                fl!("ui-tool-jwt-unsupported"),
                Tone::Quiet,
                font_size::BODY,
                false,
            ));
        } else {
            card = card
                .push(super::field_label(fl!("ui-tool-jwt-secret")))
                .push(
                    row![
                        text_input("", &self.secret)
                            .font(super::BOX_FONT)
                            .size(font_size::BODY)
                            .padding(super::INPUT_PADDING)
                            .style(styles::text_input)
                            .on_input(move |typed| send(JwtMessage::Secret(typed))),
                        super::action_button(
                            fl!("ui-tool-jwt-verify"),
                            true,
                            self.can_verify().then(|| send(JwtMessage::Verify)),
                        ),
                    ]
                    .spacing(spacing::XS)
                    .align_y(Alignment::Center),
                )
                .push(self.verified.map(|valid| {
                    let (said, tone) = if valid {
                        (
                            format!("{VALID_MARK} {}", fl!("ui-tool-jwt-signature-valid")),
                            Tone::Success,
                        )
                    } else {
                        (
                            format!("{INVALID_MARK} {}", fl!("ui-tool-jwt-signature-invalid")),
                            Tone::Error,
                        )
                    };
                    crypto_parts::said(said, tone, font_size::BODY, true)
                }));
        }
        Some(
            container(card)
                .padding(VERIFY_PADDING)
                .width(Length::Fill)
                .style(styles::card)
                .into(),
        )
    }
}

/// `text` copied by the button of `slot`; nothing when it is empty.
fn copied(slot: CopySlot, text: &str) -> Option<(CopySlot, String)> {
    (!text.is_empty()).then(|| (slot, text.to_owned()))
}

/// This computer's clock, in Unix seconds.
fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| {
            i64::try_from(since.as_secs()).unwrap_or(i64::MAX)
        })
}

/// Unix second `seconds` in this computer's time, as the window shows a date.
fn date(seconds: i64) -> String {
    let moment = if seconds >= 0 {
        UNIX_EPOCH.checked_add(Duration::from_secs(seconds.unsigned_abs()))
    } else {
        UNIX_EPOCH.checked_sub(Duration::from_secs(seconds.unsigned_abs()))
    };
    moment
        .map(heimdall_app::local_date_time)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use iced::widget::text_editor::Edit;

    use super::*;

    /// The token jwt.io shows, signed by its default secret.
    const TOKEN: &str = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.\
        eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIiwiaWF0IjoxNTE2MjM5MDIyfQ.\
        SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c";

    fn pasted(pane: &mut JwtPane, token: &str) {
        pane.input = Content::new();
        pane.update(JwtMessage::Input(Action::Edit(Edit::Paste(Arc::new(
            token.to_owned(),
        )))));
    }

    #[test]
    fn a_token_pasted_shows_its_parts_its_expiry_and_its_hmac_check() {
        let mut pane = JwtPane::new();
        assert!(pane.blank);
        pasted(&mut pane, TOKEN);
        assert!(!pane.blank && pane.error.is_none());
        assert!(pane.header.text.contains("\"alg\": \"HS256\""));
        assert!(pane.payload.text.contains("\"name\": \"John Doe\""));
        assert_eq!(pane.signature.text.len(), 64);
        assert_eq!(pane.expiration, Some(Expiration::NoExpiry));
        assert_eq!(pane.algorithm, JwtAlgorithm::Hmac256);
        assert!(!pane.can_verify(), "no secret yet");
        pane.update(JwtMessage::Secret("your-256-bit-secret".to_owned()));
        pane.update(JwtMessage::Verify);
        assert_eq!(pane.verified, Some(true));
        pane.update(JwtMessage::Secret("other".to_owned()));
        assert_eq!(pane.verified, Some(true), "kept until checked again");
        pane.update(JwtMessage::Verify);
        assert_eq!(pane.verified, Some(false));
        assert_eq!(
            pane.update(JwtMessage::CopySignature).map(|(slot, _)| slot),
            Some(CopySlot::JwtSignature)
        );
        // A token read again forgets the check.
        pasted(&mut pane, TOKEN);
        assert_eq!(pane.verified, None);
    }

    #[test]
    fn a_token_not_read_says_why_and_a_blank_one_clears_everything() {
        let mut pane = JwtPane::new();
        pasted(&mut pane, "a.b");
        assert_eq!(pane.error, Some(JwtDecodeError::InvalidFormat));
        assert!(!pane.blank);
        pasted(&mut pane, "%%%%.e30.");
        assert_eq!(pane.error, Some(JwtDecodeError::DecodeFailed));
        assert!(pane.verify_card(|_| Message::PointerPressed).is_none());
        pasted(&mut pane, "   ");
        assert!(pane.blank && pane.error.is_none() && pane.header.text.is_empty());
        assert_eq!(pane.update(JwtMessage::CopyHeader), None);
    }

    #[test]
    fn an_expiry_is_read_at_the_clock_and_a_bad_one_hidden() {
        let segment =
            |json: &str| heimdall_core::tools::base64_codec::encode(json.as_bytes(), true);
        let header = segment("{\"alg\":\"RS256\"}");
        let mut pane = JwtPane::new();
        pane.input = Content::with_text(&format!("{header}.{}.", segment("{\"exp\":100}")));
        pane.parse(200);
        assert_eq!(pane.expiration, Some(Expiration::Expired(100)));
        assert_eq!(pane.algorithm, JwtAlgorithm::Rsa);
        assert!(pane.verify_card(|_| Message::PointerPressed).is_some());
        pane.input = Content::with_text(&format!("{header}.{}.", segment("{\"exp\":\"x\"}")));
        pane.parse(200);
        assert_eq!(
            pane.expiration, None,
            "an exp not read is hidden, as the C#"
        );
    }

    #[test]
    fn the_secret_is_never_written_out() {
        let mut pane = JwtPane::new();
        pane.update(JwtMessage::Secret("hunter2".to_owned()));
        assert!(!format!("{pane:?}").contains("hunter2"));
        assert!(!format!("{:?}", JwtMessage::Secret("hunter2".to_owned())).contains("hunter2"));
    }
}
