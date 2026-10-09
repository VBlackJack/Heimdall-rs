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

//! The Password Audit, as the C# `PasswordAuditView` (`PasswordAuditView.xaml.cs`): a
//! password typed hidden or shown and a policy; as it is typed, offline, its score out of a
//! hundred on a bar coloured by its word, and a line per criterion with its tick or its cross
//! and what it says.
//!
//! The password is a secret: held in memory wiped when retyped or when the tab closes, never
//! logged nor kept.

use std::fmt;

use heimdall_app::TabId;
use heimdall_core::tools::password_audit::{
    self, AuditPolicy, Criterion, Detail, PasswordAnalysis, Pattern, ScoreLabel,
};
use iced::widget::{column, container, pick_list, progress_bar, row, text, text_input, tooltip};
use iced::{Alignment, Color, Element, Length, Theme};
use zeroize::Zeroizing;

use super::crypto_parts;
use super::key_parts;
use super::{ToolMessage, ToolPane};
use crate::i18n::fl;
use crate::icons::{self, Icon};
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, spacing};

/// Height of the score's bar, as the C# `Height="8"`.
const BAR_HEIGHT: f32 = 8.0;

/// The score's range.
const SCORE_MAX: f32 = 100.0;

/// Width of a criterion's mark, as the C# `Width="20"`.
const MARK_WIDTH: f32 = 20.0;

/// Width of the policy box, as the C# `Width="180"`.
const POLICY_WIDTH: f32 = 180.0;

/// The marks of a criterion passed and failed, as the C#'s Segoe MDL2 check and cross.
const PASS_MARK: &str = "\u{2713}";
const FAIL_MARK: &str = "\u{2717}";

/// What separates the patterns found, as the C#'s `", "`.
const PATTERN_SEPARATOR: &str = ", ";

/// A policy in the box.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PolicyChoice(pub AuditPolicy);

impl fmt::Display for PolicyChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&match self.0 {
            AuditPolicy::Nist => fl!("ui-tool-pwdaudit-policy-nist"),
            AuditPolicy::Anssi => fl!("ui-tool-pwdaudit-policy-anssi"),
            AuditPolicy::Custom => fl!("ui-tool-pwdaudit-policy-custom"),
        })
    }
}

/// What the Password Audit is asked.
#[derive(Clone)]
pub enum PwdAuditMessage {
    /// The password typed.
    Password(String),
    /// Show or hide it.
    ToggleVisibility,
    /// The policy chosen.
    Policy(PolicyChoice),
}

impl fmt::Debug for PwdAuditMessage {
    /// The password is never written out.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Password(_) => f.write_str("Password(..)"),
            Self::ToggleVisibility => f.write_str("ToggleVisibility"),
            Self::Policy(choice) => write!(f, "Policy({:?})", choice.0),
        }
    }
}

/// The Password Audit's state, as the C# view's.
#[derive(Default)]
pub struct PwdAuditPane {
    password: Zeroizing<String>,
    visible: bool,
    policy: AuditPolicy,
    /// The analysis of the password typed; `None` while it is empty.
    analysis: Option<PasswordAnalysis>,
}

impl fmt::Debug for PwdAuditPane {
    /// The password is never written out.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PwdAuditPane")
            .field("visible", &self.visible)
            .field("policy", &self.policy)
            .field(
                "score",
                &self.analysis.as_ref().map(|analysis| analysis.score),
            )
            .finish_non_exhaustive()
    }
}

/// `value` with one decimal at most and the language's decimal sign, as .NET formats a
/// rounded double: `37.6`, `40`, `37,6` in French and Spanish.
fn decimal(value: f64) -> String {
    decimal_in(value, crate::i18n::current().code())
}

/// `value` as [`decimal`] writes it in the language of code `language`.
fn decimal_in(value: f64, language: &str) -> String {
    let text = format!("{value:.1}");
    let text = text.strip_suffix(".0").map_or(text.clone(), str::to_owned);
    match language {
        "fr" | "es" => text.replace('.', ","),
        _ => text,
    }
}

impl PwdAuditPane {
    /// Applies `message`, the analysis run again, as the C# `RunAnalysis`.
    pub fn update(&mut self, message: PwdAuditMessage) {
        match message {
            PwdAuditMessage::Password(typed) => self.password = Zeroizing::new(typed),
            PwdAuditMessage::ToggleVisibility => self.visible = !self.visible,
            PwdAuditMessage::Policy(choice) => self.policy = choice.0,
        }
        self.analysis = (!self.password.is_empty())
            .then(|| password_audit::analyze(&self.password, self.policy));
    }

    /// The label of a criterion, as the C# `ToolPwdAudit*` label keys.
    fn label(criterion: Criterion) -> String {
        match criterion {
            Criterion::Length => fl!("ui-tool-pwdaudit-length"),
            Criterion::Uppercase => fl!("ui-tool-pwdaudit-uppercase"),
            Criterion::Lowercase => fl!("ui-tool-pwdaudit-lowercase"),
            Criterion::Digits => fl!("ui-tool-pwdaudit-digits"),
            Criterion::Symbols => fl!("ui-tool-pwdaudit-symbols"),
            Criterion::Entropy => fl!("ui-tool-pwdaudit-entropy"),
            Criterion::Common => fl!("ui-tool-pwdaudit-common"),
            Criterion::Patterns => fl!("ui-tool-pwdaudit-patterns"),
        }
    }

    /// What a criterion's line says on its right, as the C# detail keys.
    fn detail(detail: &Detail) -> String {
        match detail {
            Detail::Length { length, minimum } => fl!(
                "ui-tool-pwdaudit-length-detail",
                length = length,
                minimum = minimum
            ),
            Detail::Pass => fl!("ui-tool-pwdaudit-pass"),
            Detail::Fail => fl!("ui-tool-pwdaudit-fail"),
            Detail::Warn => fl!("ui-tool-pwdaudit-warn"),
            Detail::EntropyBits(bits) => {
                fl!("ui-tool-pwdaudit-entropy-bits", bits = decimal(*bits))
            }
            Detail::InCommonList => fl!("ui-tool-pwdaudit-in-common-list"),
            Detail::NotInCommonList => fl!("ui-tool-pwdaudit-not-in-common-list"),
            Detail::Patterns(patterns) => patterns
                .iter()
                .map(|pattern| match pattern {
                    Pattern::Keyboard => fl!("ui-tool-pwdaudit-pattern-keyboard"),
                    Pattern::Sequence => fl!("ui-tool-pwdaudit-pattern-sequence"),
                    Pattern::Repeat => fl!("ui-tool-pwdaudit-pattern-repeat"),
                })
                .collect::<Vec<_>>()
                .join(PATTERN_SEPARATOR),
        }
    }

    /// The colour of a score's word, as the C# `UpdateStrengthBar`: the error, the warning,
    /// the warning's text and the success brushes.
    fn score_color(label: ScoreLabel, theme: &Theme) -> Color {
        let palette = theme.extended_palette();
        match label {
            ScoreLabel::Weak => palette.danger.base.color,
            ScoreLabel::Fair => palette.warning.strong.color,
            ScoreLabel::Good => palette.warning.base.color,
            ScoreLabel::Strong => palette.success.base.color,
        }
    }

    /// The tool's page, as the C# `PasswordAuditView.xaml`.
    pub fn view<'a>(&'a self, tab: TabId, _state: &ToolPane) -> Element<'a, Message> {
        let send = move |message| Message::Tool(tab, ToolMessage::PwdAudit(message));
        let eye = if self.visible {
            Icon::EyeHidden
        } else {
            Icon::Eye
        };
        let eye_tip = if self.visible {
            fl!("ui-tool-pwdaudit-hide")
        } else {
            fl!("ui-tool-pwdaudit-show")
        };
        let input = row![
            text_input(&fl!("ui-tool-pwdaudit-placeholder"), &self.password)
                .secure(!self.visible)
                .font(super::BOX_FONT)
                .size(font_size::BODY_LARGE)
                .padding(super::INPUT_PADDING)
                .style(styles::text_input)
                .on_input(move |typed| send(PwdAuditMessage::Password(typed))),
            tooltip(
                icons::button(eye)
                    .style(styles::secondary)
                    .on_press(send(PwdAuditMessage::ToggleVisibility)),
                text(eye_tip).size(font_size::CAPTION),
                tooltip::Position::Bottom,
            )
            .style(container::rounded_box),
        ]
        .spacing(spacing::XS)
        .align_y(Alignment::Center);
        let policy = row![
            super::field_label(fl!("ui-tool-pwdaudit-policy")),
            pick_list(
                AuditPolicy::ALL.map(PolicyChoice),
                Some(PolicyChoice(self.policy)),
                move |choice| send(PwdAuditMessage::Policy(choice)),
            )
            .width(POLICY_WIDTH)
            .style(styles::pick_list)
            .menu_style(styles::menu)
            .text_size(font_size::BODY),
        ]
        .spacing(spacing::SM)
        .align_y(Alignment::Center);
        let head = key_parts::card(key_parts::stack([
            super::field_label(fl!("ui-tool-pwdaudit-input")),
            input.into(),
            policy.into(),
        ]));
        let body: Element<'a, Message> = match &self.analysis {
            None => crypto_parts::empty_state(fl!("ui-tool-pwdaudit-empty")),
            Some(analysis) => Self::analysis_view(analysis),
        };
        super::content_column(column![head, body].spacing(spacing::MD))
    }

    /// The score and the criteria, as the C# `PanelStrength` and `PanelCriteria`.
    fn analysis_view(analysis: &PasswordAnalysis) -> Element<'_, Message> {
        let label = analysis.label;
        let word = match label {
            ScoreLabel::Weak => fl!("ui-tool-pwdaudit-score-weak"),
            ScoreLabel::Fair => fl!("ui-tool-pwdaudit-score-fair"),
            ScoreLabel::Good => fl!("ui-tool-pwdaudit-score-good"),
            ScoreLabel::Strong => fl!("ui-tool-pwdaudit-score-strong"),
        };
        let score = row![
            text(fl!("ui-tool-pwdaudit-strength"))
                .size(font_size::BODY)
                .font(styles::SEMIBOLD)
                .width(Length::Fill),
            text(format!("{}/100 - {word}", analysis.score))
                .size(font_size::BODY)
                .font(styles::SEMIBOLD)
                .style(move |theme: &Theme| text::Style {
                    color: Some(Self::score_color(label, theme)),
                }),
        ];
        let bar = progress_bar(0.0..=SCORE_MAX, f32::from(analysis.score))
            .girth(BAR_HEIGHT)
            .style(move |theme: &Theme| {
                let default = progress_bar::primary(theme);
                progress_bar::Style {
                    bar: iced::Background::Color(Self::score_color(label, theme)),
                    ..default
                }
            });
        let criteria = analysis.criteria.iter().map(|line| {
            let passed = line.passed;
            row![
                text(if passed { PASS_MARK } else { FAIL_MARK })
                    .size(font_size::BODY)
                    .width(MARK_WIDTH)
                    .style(move |theme: &Theme| {
                        let palette = theme.extended_palette();
                        text::Style {
                            color: Some(if passed {
                                palette.success.base.color
                            } else {
                                palette.danger.base.color
                            }),
                        }
                    }),
                text(Self::label(line.criterion))
                    .size(font_size::BODY)
                    .width(Length::Fill),
                text(Self::detail(&line.detail))
                    .size(font_size::CAPTION)
                    .style(text::secondary),
            ]
            .spacing(spacing::XS)
            .align_y(Alignment::Center)
            .into()
        });
        column![
            key_parts::card(key_parts::stack([score.into(), bar.into()])),
            key_parts::card(key_parts::stack(criteria)),
        ]
        .spacing(spacing::MD)
        .into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_analysis_follows_the_password_and_the_policy() {
        let mut pane = PwdAuditPane::default();
        assert!(pane.analysis.is_none());
        pane.update(PwdAuditMessage::Password("password".to_owned()));
        let analysis = pane.analysis.clone().expect("analysed");
        assert!(analysis.is_common);
        pane.update(PwdAuditMessage::Policy(PolicyChoice(AuditPolicy::Anssi)));
        assert_eq!(
            pane.analysis.as_ref().expect("analysed").criteria[1].detail,
            Detail::Fail
        );
        pane.update(PwdAuditMessage::Password(String::new()));
        assert!(pane.analysis.is_none(), "empty, the empty state");
    }

    #[test]
    fn the_password_is_never_written_out() {
        let mut pane = PwdAuditPane::default();
        pane.update(PwdAuditMessage::Password("hunter2".to_owned()));
        let shown = format!(
            "{pane:?} {:?}",
            PwdAuditMessage::Password("hunter2".to_owned())
        );
        assert!(!shown.contains("hunter2"), "{shown}");
    }

    #[test]
    fn entropy_is_said_with_one_decimal_at_most() {
        assert_eq!(decimal_in(37.6, "en"), "37.6");
        assert_eq!(decimal_in(40.0, "en"), "40");
        assert_eq!(decimal_in(37.6, "fr"), "37,6");
    }
}
