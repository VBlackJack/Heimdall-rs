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

//! The TOTP generator, as the C# `TotpGeneratorView` and `TotpGeneratorViewModel`: a Base32
//! secret, read on Start or Enter; the current six-digit code by HMAC-SHA1 over 30-second
//! steps, refreshed every second with the seconds left on a bar; the code copied.
//!
//! The secret is never logged nor kept: it lives in the tab, typed and decoded, wiped from
//! memory when the tab closes or the secret is retyped.

use std::fmt;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use heimdall_app::TabId;
use heimdall_core::tools::totp_generator::{
    self, DEFAULT_ALGORITHM, DEFAULT_DIGITS, DEFAULT_TIME_STEP_SECONDS,
};
use iced::widget::{column, container, progress_bar, row, text, text_input};
use iced::{Alignment, Element, Length, Task};
use zeroize::Zeroizing;

use super::crypto_parts::{self, Tone};
use super::{CopySlot, ToolMessage, ToolPane};
use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, spacing};

/// What the code shows before one is made, as the C# `PlaceholderCode`.
pub const PLACEHOLDER_CODE: &str = "------";

/// How often the code is made again, as the C# `DispatcherTimer`'s second.
const TICK: Duration = Duration::from_secs(1);

/// Size of the code, as the C# `IconSizeXLarge`.
const CODE_SIZE: f32 = 36.0;

/// Padding of the code's card, as the C# `Padding="24,16"`.
const CODE_PADDING: [f32; 2] = [16.0, 24.0];

/// Width of the seconds' bar, as the C# `Width="300"`.
const BAR_WIDTH: f32 = 300.0;

/// Height of the seconds' bar, as the C# `Height="6"`.
const BAR_HEIGHT: f32 = 6.0;

/// Room between the code and its copy button, as the C# `Margin="12,0,0,0"`.
const CODE_GAP: f32 = 12.0;

/// Room between the sections, as the C# `MarginSectionSeparator`.
const SECTION_GAP: f32 = 16.0;

/// What the TOTP generator is asked.
#[derive(Clone)]
pub enum TotpMessage {
    /// The secret typed.
    Secret(String),
    /// Read the secret and show its code, as the C# Start button and Enter.
    Start,
    /// A second of timer `.0` has passed.
    Tick(u64),
    /// Copy the code.
    Copy,
}

impl fmt::Debug for TotpMessage {
    /// The secret typed is never written out.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Secret(_) => f.write_str("Secret(..)"),
            Self::Start => f.write_str("Start"),
            Self::Tick(timer) => write!(f, "Tick({timer})"),
            Self::Copy => f.write_str("Copy"),
        }
    }
}

/// What an update asks of its tab.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Nothing more.
    Done,
    /// This code copied by the copy button.
    Copy(String),
    /// The next second of timer `.0` waited for.
    Tick(u64),
}

impl Outcome {
    /// What tab `tab` runs for it; a copy is its tab's.
    pub fn task(self, tab: TabId) -> Task<Message> {
        match self {
            Self::Done | Self::Copy(_) => Task::none(),
            // The timer made when it is first awaited: in the runtime that runs it.
            Self::Tick(timer) => {
                Task::perform(async { tokio::time::sleep(TICK).await }, move |()| {
                    Message::Tool(tab, ToolMessage::Totp(TotpMessage::Tick(timer)))
                })
            }
        }
    }
}

/// Why a secret was refused, as the C#'s two error texts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SecretError {
    /// Nothing typed.
    Required,
    /// Not Base32.
    InvalidBase32,
}

/// The TOTP generator's state, as the C# view model's.
pub struct TotpPane {
    secret: Zeroizing<String>,
    /// The secret's bytes, once read.
    key: Option<Zeroizing<Vec<u8>>>,
    code: String,
    remaining: i64,
    error: Option<SecretError>,
    code_shown: bool,
    /// The timer running: its ticks of another are let go.
    timer: u64,
}

impl fmt::Debug for TotpPane {
    /// The secret and the code are never written out.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TotpPane")
            .field("read", &self.key.is_some())
            .field("remaining", &self.remaining)
            .field("error", &self.error)
            .field("code_shown", &self.code_shown)
            .finish_non_exhaustive()
    }
}

impl Default for TotpPane {
    /// A new tab's state, as the C# `Initialize`: no code, a whole step left.
    fn default() -> Self {
        Self {
            secret: Zeroizing::new(String::new()),
            key: None,
            code: PLACEHOLDER_CODE.to_owned(),
            remaining: DEFAULT_TIME_STEP_SECONDS,
            error: None,
            code_shown: false,
            timer: 0,
        }
    }
}

/// This computer's clock, in Unix seconds.
fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| {
            i64::try_from(since.as_secs()).unwrap_or(i64::MAX)
        })
}

impl TotpPane {
    /// Applies `message`; what is then to be done.
    pub fn update(&mut self, message: TotpMessage) -> Outcome {
        match message {
            TotpMessage::Secret(typed) => self.secret = Zeroizing::new(typed),
            TotpMessage::Start => return self.start(now()),
            TotpMessage::Tick(timer) => {
                if timer != self.timer {
                    return Outcome::Done;
                }
                self.refresh(now());
                return Outcome::Tick(timer);
            }
            TotpMessage::Copy => {
                if self.can_copy() {
                    return Outcome::Copy(self.code.clone());
                }
            }
        }
        Outcome::Done
    }

    /// The secret read and its code shown at `unix_seconds`, as the C# `Start`
    /// (`TotpGeneratorViewModel.cs:65-90`): spaces taken out and upper-cased; the timer
    /// started when the code first shows, as the C# view starts it.
    fn start(&mut self, unix_seconds: i64) -> Outcome {
        self.error = None;
        let sanitized = Zeroizing::new(self.secret.trim().replace(' ', "").to_uppercase());
        if sanitized.is_empty() {
            self.error = Some(SecretError::Required);
            return Outcome::Done;
        }
        let Ok(bytes) = totp_generator::decode_base32(&sanitized) else {
            self.key = None;
            self.error = Some(SecretError::InvalidBase32);
            return Outcome::Done;
        };
        self.key = Some(Zeroizing::new(bytes));
        let first = !self.code_shown;
        self.code_shown = true;
        self.refresh(unix_seconds);
        if first {
            self.timer += 1;
            return Outcome::Tick(self.timer);
        }
        Outcome::Done
    }

    /// The code at `unix_seconds` and the seconds left, as the C# `RefreshCode`
    /// (`TotpGeneratorViewModel.cs:92-115`); nothing without a secret read.
    pub fn refresh(&mut self, unix_seconds: i64) {
        let Some(key) = &self.key else {
            return;
        };
        match totp_generator::generate(
            key,
            unix_seconds,
            DEFAULT_ALGORITHM,
            DEFAULT_DIGITS,
            DEFAULT_TIME_STEP_SECONDS,
        ) {
            Ok(code) => {
                self.code = code;
                if let Ok(remaining) =
                    totp_generator::remaining_in_step(unix_seconds, DEFAULT_TIME_STEP_SECONDS)
                {
                    self.remaining = remaining;
                }
            }
            Err(_) => PLACEHOLDER_CODE.clone_into(&mut self.code),
        }
    }

    /// Whether there is a code to copy, as the C# `CanCopy`.
    #[must_use]
    pub fn can_copy(&self) -> bool {
        !self.code.is_empty() && self.code != PLACEHOLDER_CODE
    }

    /// The tool's page, as the C# `TotpGeneratorView.xaml`: the secret and Start, what is
    /// wrong with it, the code's card with its copy button and the seconds left, the note.
    pub fn view<'a>(&'a self, tab: TabId, state: &ToolPane) -> Element<'a, Message> {
        let send = move |message| Message::Tool(tab, ToolMessage::Totp(message));
        let secret = row![
            text_input(&fl!("ui-tool-totp-secret-placeholder"), &self.secret)
                .font(super::BOX_FONT)
                .size(font_size::BODY_LARGE)
                .padding(super::INPUT_PADDING)
                .style(styles::text_input)
                .on_input(move |typed| send(TotpMessage::Secret(typed)))
                .on_submit(send(TotpMessage::Start)),
            super::action_button(
                fl!("ui-tool-totp-start"),
                true,
                Some(send(TotpMessage::Start))
            ),
        ]
        .spacing(spacing::XS)
        .align_y(Alignment::Center);
        let error = self.error.map(|error| {
            let said = match error {
                SecretError::Required => fl!("ui-tool-totp-error-required"),
                SecretError::InvalidBase32 => fl!("ui-tool-totp-error-base32"),
            };
            crypto_parts::said(said, Tone::Error, font_size::BODY, false)
        });
        let card = self.code_shown.then(|| {
            let step = f32::from(u8::try_from(DEFAULT_TIME_STEP_SECONDS).unwrap_or(u8::MAX));
            let left = f32::from(u8::try_from(self.remaining).unwrap_or_default());
            container(
                column![
                    crypto_parts::field_label(fl!("ui-tool-totp-code")),
                    row![
                        text(&self.code)
                            .size(CODE_SIZE)
                            .font(iced::Font {
                                weight: iced::font::Weight::Bold,
                                ..super::BOX_FONT
                            })
                            .style(|theme: &iced::Theme| text::Style {
                                color: Some(crypto_parts::accent(theme)),
                            }),
                        super::copy_button(
                            fl!("ui-tool-totp-copy"),
                            state.copied(CopySlot::TotpCode),
                            send(TotpMessage::Copy),
                            super::PRIMARY_PADDING,
                        ),
                    ]
                    .spacing(CODE_GAP)
                    .align_y(Alignment::Center),
                    crypto_parts::field_label(fl!(
                        "ui-tool-totp-remaining",
                        seconds = self.remaining
                    )),
                    progress_bar(0.0..=step, left)
                        .length(BAR_WIDTH)
                        .girth(BAR_HEIGHT),
                ]
                .spacing(spacing::SM)
                .align_x(Alignment::Center),
            )
            .padding(CODE_PADDING)
            .center_x(Length::Fill)
            .style(styles::card)
        });
        super::content_column(
            column![
                crypto_parts::field_label(fl!("ui-tool-totp-secret")),
                secret,
            ]
            .push(error)
            .push(iced::widget::space().height(SECTION_GAP - spacing::SM))
            .push(card)
            .push(crypto_parts::said(
                fl!("ui-tool-totp-info"),
                Tone::Quiet,
                font_size::CAPTION,
                false,
            ))
            .spacing(spacing::SM),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The RFC 6238 SHA-1 seed, `12345678901234567890`, in Base32, spaced and in lower case.
    const RFC_SECRET: &str = " gezd gnbv gy3t qojq gezd gnbv gy3t qojq ";

    #[test]
    fn a_secret_missing_or_not_base32_is_refused_as_the_csharp_says() {
        let mut pane = TotpPane::default();
        assert_eq!(pane.remaining, 30);
        assert_eq!(pane.start(59), Outcome::Done);
        assert_eq!(pane.error, Some(SecretError::Required));
        assert!(!pane.code_shown);
        pane.update(TotpMessage::Secret("ABC1".to_owned()));
        assert_eq!(pane.start(59), Outcome::Done);
        assert_eq!(pane.error, Some(SecretError::InvalidBase32));
        assert!(!pane.can_copy());
    }

    #[test]
    fn a_secret_read_shows_its_code_and_starts_the_timer_once() {
        let mut pane = TotpPane::default();
        pane.update(TotpMessage::Secret(RFC_SECRET.to_owned()));
        assert_eq!(pane.start(59), Outcome::Tick(1));
        assert!(pane.code_shown && pane.error.is_none());
        assert_eq!(pane.code, "287082", "RFC 6238 at 59 seconds, six digits");
        assert_eq!(pane.remaining, 1);
        assert_eq!(
            pane.start(1_111_111_109),
            Outcome::Done,
            "the timer runs already"
        );
        assert_eq!(pane.code, "081804");
        assert_eq!(
            pane.update(TotpMessage::Copy),
            Outcome::Copy("081804".to_owned())
        );
        // A tick of a timer let go is not followed.
        assert_eq!(pane.update(TotpMessage::Tick(0)), Outcome::Done);
        assert_eq!(pane.update(TotpMessage::Tick(1)), Outcome::Tick(1));
    }

    #[test]
    fn without_a_secret_read_the_code_stays_the_placeholder() {
        let mut pane = TotpPane::default();
        pane.refresh(59);
        assert_eq!(pane.code, PLACEHOLDER_CODE);
        assert!(!pane.can_copy());
        assert_eq!(pane.update(TotpMessage::Copy), Outcome::Done);
    }

    #[test]
    fn the_secret_is_never_written_out() {
        let mut pane = TotpPane::default();
        pane.update(TotpMessage::Secret(RFC_SECRET.to_owned()));
        let _ = pane.start(59);
        let shown = format!("{pane:?}");
        assert!(
            !shown.contains("gezd") && !shown.contains("287082"),
            "{shown}"
        );
        assert!(!format!("{:?}", TotpMessage::Secret("gezd".to_owned())).contains("gezd"));
    }
}
