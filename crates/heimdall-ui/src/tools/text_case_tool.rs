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

//! The text case converter, as the C# `TextCaseConverterView` and
//! `TextCaseConverterViewModel`: a button per case style; once one is pressed, the output
//! follows the input as it is typed, in that style; Ctrl+Enter converts to title case; the
//! output copied.

use heimdall_app::TabId;
use heimdall_core::tools::text_case_codec::{self, TextCaseStyle};
use iced::keyboard::{Key, key::Named};
use iced::widget::text_editor::{Action, Binding, Content, KeyPress, Status};
use iced::widget::{Row, column, container, row};
use iced::{Element, Theme};

use super::{CopySlot, ToolMessage, ToolPane};
use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::tokens::spacing;

/// Least height of the input and the output, as the C# `MinHeight`.
const BOX_MIN_HEIGHT: f32 = 100.0;

/// Most height of the input and the output before they scroll.
const BOX_MAX_HEIGHT: f32 = 240.0;

/// Room above the conversions' label, as the C# `Margin="0,16,0,8"`.
const CONVERSIONS_TOP: f32 = 16.0;

/// Room between the conversion buttons, as the C# `MarginButtonGroup`.
const BUTTON_GAP: f32 = 8.0;

/// Room under the buttons, as the C# `MarginSectionSeparator`.
const SECTION_GAP: f32 = 16.0;

/// What the text case converter is asked.
#[derive(Debug, Clone)]
pub enum TextCaseMessage {
    /// Something done in the input box.
    Input(Action),
    /// Something done in the output box, which does not change it.
    Output(Action),
    /// A style's button, or Ctrl+Enter for title case.
    Convert(TextCaseStyle),
    /// Copy the output.
    Copy,
}

/// The text case converter's state, as the C# view model's.
#[derive(Debug, Default)]
pub struct TextCasePane {
    input: Content,
    output: Content,
    output_text: String,
    /// The style chosen last, which the output follows.
    selected: Option<TextCaseStyle>,
    /// A conversion was made: the output is shown, rather than the empty state.
    results: bool,
}

impl TextCasePane {
    /// Applies `message`; what the copy button copies, when it is pressed.
    pub fn update(&mut self, message: TextCaseMessage) -> Option<(CopySlot, String)> {
        match message {
            TextCaseMessage::Input(action) => {
                let edit = action.is_edit();
                self.input.perform(action);
                // As the C# `OnInputTextChanged`: the output follows in the chosen style.
                if edit && let Some(style) = self.selected {
                    self.show(style);
                }
            }
            TextCaseMessage::Output(action) => super::read_only(&mut self.output, action),
            TextCaseMessage::Convert(style) => {
                // As the C# `Convert` command.
                self.selected = Some(style);
                self.show(style);
                self.results = true;
            }
            TextCaseMessage::Copy => {
                return Some((CopySlot::TextCaseOutput, self.output_text.clone()));
            }
        }
        None
    }

    /// The output: the input in `style`.
    fn show(&mut self, style: TextCaseStyle) {
        let output = text_case_codec::convert(&super::box_text(&self.input), style);
        self.output = Content::with_text(&output);
        self.output_text = output;
    }

    /// The tool's page, as the C# `TextCaseConverterView.xaml`: the input, the conversions,
    /// then the output with its copy button, or the empty state.
    pub fn view<'a>(&'a self, tab: TabId, state: &ToolPane) -> Element<'a, Message> {
        let send = move |message| Message::Tool(tab, ToolMessage::TextCase(message));
        let input = super::text_box(&self.input, Some(fl!("ui-tool-textcase-placeholder")))
            .min_height(BOX_MIN_HEIGHT)
            .max_height(BOX_MAX_HEIGHT)
            .on_action(move |action| send(TextCaseMessage::Input(action)))
            .key_binding(move |press| binding(press, send));
        let buttons = Row::with_children(TextCaseStyle::ALL.into_iter().map(|style| {
            super::action_button(
                style_label(style),
                false,
                Some(send(TextCaseMessage::Convert(style))),
            )
        }))
        .spacing(BUTTON_GAP)
        .wrap()
        .vertical_spacing(BUTTON_GAP);
        let output: Element<'a, Message> = if self.results {
            row![
                super::text_box(&self.output, None)
                    .min_height(BOX_MIN_HEIGHT)
                    .max_height(BOX_MAX_HEIGHT)
                    .style(accent_box)
                    .on_action(move |action| send(TextCaseMessage::Output(action))),
                super::copy_button(
                    fl!("ui-tool-textcase-copy"),
                    state.copied(CopySlot::TextCaseOutput),
                    send(TextCaseMessage::Copy),
                    super::COPY_PADDING,
                ),
            ]
            .spacing(spacing::SM)
            .into()
        } else {
            super::empty_state(fl!("ui-tool-textcase-empty"))
        };
        super::content_column(column![
            super::field_label(fl!("ui-tool-textcase-input")),
            container(input).padding(iced::Padding {
                top: spacing::XS,
                ..iced::Padding::ZERO
            }),
            container(super::field_label(fl!("ui-tool-textcase-conversions"))).padding(
                iced::Padding {
                    top: CONVERSIONS_TOP,
                    bottom: spacing::SM,
                    ..iced::Padding::ZERO
                }
            ),
            container(buttons).padding(iced::Padding {
                bottom: SECTION_GAP,
                ..iced::Padding::ZERO
            }),
            super::field_label(fl!("ui-tool-textcase-output")),
            container(output).padding(iced::Padding {
                top: spacing::XS,
                ..iced::Padding::ZERO
            }),
        ])
    }
}

/// The label of `style`'s button, as the C# `ToolTextCase*` keys.
#[must_use]
pub fn style_label(style: TextCaseStyle) -> String {
    match style {
        TextCaseStyle::Camel => fl!("ui-tool-textcase-camel"),
        TextCaseStyle::Pascal => fl!("ui-tool-textcase-pascal"),
        TextCaseStyle::Snake => fl!("ui-tool-textcase-snake"),
        TextCaseStyle::Kebab => fl!("ui-tool-textcase-kebab"),
        TextCaseStyle::Upper => fl!("ui-tool-textcase-upper"),
        TextCaseStyle::Lower => fl!("ui-tool-textcase-lower"),
        TextCaseStyle::Title => fl!("ui-tool-textcase-title-case"),
        TextCaseStyle::Constant => fl!("ui-tool-textcase-constant"),
    }
}

/// The output box, as the C#'s: its text in the accent.
fn accent_box(
    theme: &Theme,
    status: iced::widget::text_editor::Status,
) -> iced::widget::text_editor::Style {
    iced::widget::text_editor::Style {
        value: theme.palette().primary,
        ..styles::text_box(theme, status)
    }
}

/// The input's keys, as the C# `OnInputPreviewKeyDown`: Ctrl+Enter converts to title case;
/// the rest as any text box.
fn binding(press: KeyPress, send: impl Fn(TextCaseMessage) -> Message) -> Option<Binding<Message>> {
    if matches!(press.status, Status::Focused { .. })
        && press.key == Key::Named(Named::Enter)
        && press.modifiers.control()
        && !press.modifiers.shift()
        && !press.modifiers.alt()
    {
        return Some(Binding::Custom(send(TextCaseMessage::Convert(
            TextCaseStyle::Title,
        ))));
    }
    Binding::from_key_press(press)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use iced::widget::text_editor::Edit;

    use super::*;

    fn typed(pane: &mut TextCasePane, typed: &str) {
        let _ = pane.update(TextCaseMessage::Input(Action::Edit(Edit::Paste(Arc::new(
            typed.to_owned(),
        )))));
    }

    #[test]
    fn a_style_converts_and_the_output_then_follows_the_input() {
        let mut pane = TextCasePane::default();
        typed(&mut pane, "hello world");
        assert!(!pane.results, "the empty state until a style is chosen");
        assert!(pane.output_text.is_empty());
        let _ = pane.update(TextCaseMessage::Convert(TextCaseStyle::Camel));
        assert_eq!(pane.output_text, "helloWorld");
        assert!(pane.results);
        typed(&mut pane, " again");
        assert_eq!(pane.output_text, "helloWorldAgain");
        let _ = pane.update(TextCaseMessage::Convert(TextCaseStyle::Constant));
        assert_eq!(pane.output_text, "HELLO_WORLD_AGAIN");
        assert_eq!(
            pane.update(TextCaseMessage::Copy),
            Some((CopySlot::TextCaseOutput, "HELLO_WORLD_AGAIN".to_owned()))
        );
    }

    #[test]
    fn every_style_has_its_label() {
        for style in TextCaseStyle::ALL {
            assert!(!style_label(style).is_empty());
        }
        assert_eq!(style_label(TextCaseStyle::Upper), "UPPER CASE");
    }
}
