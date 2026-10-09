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

//! The JSON formatter, as the C# `JsonFormatterView` and `JsonFormatterViewModel`: JSON
//! prettified or minified, what is not JSON said with where; a large document formatted
//! away from the window, "Processing..." meanwhile; the output copied.

use heimdall_app::TabId;
use heimdall_core::tools::json_codec::{self, ASYNC_THRESHOLD_BYTES, JsonError, JsonFormat};
use iced::keyboard::{Key, key::Named};
use iced::widget::text_editor::{Action, Binding, Content, KeyPress, Status};
use iced::widget::{column, container, row};
use iced::{Element, Length, Task};

use super::{CopySlot, ToolMessage, ToolPane};
use crate::i18n::fl;
use crate::shell::Message;
use crate::tokens::spacing;

/// Room between the parts of the page, as the C# `MarginLabelBottom` and the buttons' row.
const STACK_GAP: f32 = 8.0;

/// What the status line says, as the C# `JsonStatusKind`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum JsonStatus {
    /// Nothing.
    #[default]
    None,
    /// A large document is being formatted.
    Processing,
    /// Prettified: the output's length in UTF-16 units.
    Prettified(usize),
    /// Minified: the output's length in UTF-16 units.
    Minified(usize),
    /// Not JSON: why, and where when known.
    Invalid(JsonError),
    /// Past the size limit.
    TooLarge,
}

impl JsonStatus {
    /// What it says, and whether it is an error, drawn in the error colour.
    fn said(&self) -> Option<(String, bool)> {
        let said = match self {
            Self::None => return None,
            Self::Processing => (fl!("ui-tool-json-processing"), false),
            Self::Prettified(count) => {
                let count = *count;
                (fl!("ui-tool-json-status-prettified", count = count), false)
            }
            Self::Minified(count) => {
                let count = *count;
                (fl!("ui-tool-json-status-minified", count = count), false)
            }
            Self::Invalid(JsonError {
                message,
                line: Some(line),
                column: Some(column),
            }) => {
                let (line, column) = (*line, *column);
                (
                    fl!(
                        "ui-tool-json-status-error-at",
                        line = line,
                        column = column,
                        error = message.as_str()
                    ),
                    true,
                )
            }
            Self::Invalid(error) => (
                fl!("ui-tool-json-status-error", error = error.message.as_str()),
                true,
            ),
            Self::TooLarge => (fl!("ui-tool-json-too-large"), true),
        };
        Some(said)
    }
}

/// What the JSON formatter is asked.
#[derive(Debug, Clone)]
pub enum JsonMessage {
    /// Something done in the input box.
    Input(Action),
    /// Something done in the output box, which does not change it.
    Output(Action),
    /// Prettify, as the C# button and Ctrl+Enter.
    Prettify,
    /// Minify, as the C# button and Ctrl+Shift+Enter.
    Minify,
    /// Copy the output.
    Copy,
    /// A large document formatted away from the window: the run it answers, whether it
    /// was indented, and what it gave; `None` when the run was lost.
    Formatted(u64, bool, Option<JsonFormat>),
}

/// What an update asks of its tab.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Nothing more.
    Done,
    /// This text copied by the output's copy button.
    Copy(String),
    /// This document formatted away from the window, indented or not, as run `.0`.
    Format(u64, bool, String),
}

impl Outcome {
    /// What tab `tab` runs for it; a copy is its tab's.
    pub fn task(self, tab: TabId) -> Task<Message> {
        match self {
            Self::Done | Self::Copy(_) => Task::none(),
            Self::Format(run, indented, input) => Task::perform(
                async move {
                    tokio::task::spawn_blocking(move || {
                        json_codec::format_input(&input, indented, super::NEW_LINE)
                    })
                    .await
                    .ok()
                },
                move |formatted| {
                    Message::Tool(
                        tab,
                        ToolMessage::Json(JsonMessage::Formatted(run, indented, formatted)),
                    )
                },
            ),
        }
    }
}

/// The JSON formatter's state, as the C# view model's.
#[derive(Debug, Default)]
pub struct JsonPane {
    input: Content,
    output: Content,
    output_text: String,
    status: JsonStatus,
    /// The output is shown, rather than the empty state.
    results: bool,
    /// The run of the document formatted away from the window, while it is.
    processing: Option<u64>,
    /// Runs started, the last one's number.
    runs: u64,
}

impl JsonPane {
    /// Applies `message`; what is then to be done.
    pub fn update(&mut self, message: JsonMessage) -> Outcome {
        match message {
            JsonMessage::Input(action) => {
                let edit = action.is_edit();
                self.input.perform(action);
                // As the C# `OnInputTextChanged`: the output and the status cleared.
                if edit {
                    self.clear();
                }
            }
            JsonMessage::Output(action) => super::read_only(&mut self.output, action),
            JsonMessage::Prettify => return self.format(true),
            JsonMessage::Minify => return self.format(false),
            JsonMessage::Copy => return Outcome::Copy(self.output_text.clone()),
            JsonMessage::Formatted(run, indented, formatted) => {
                if self.processing == Some(run) {
                    self.processing = None;
                    match formatted {
                        Some(formatted) => self.apply(formatted, indented),
                        None => self.status = JsonStatus::None,
                    }
                }
            }
        }
        Outcome::Done
    }

    /// The input's text.
    #[must_use]
    pub fn input_text(&self) -> String {
        super::box_text(&self.input)
    }

    /// Whether a document is being formatted away from the window: the buttons wait.
    #[must_use]
    pub fn processing(&self) -> bool {
        self.processing.is_some()
    }

    /// The output and the status cleared, the empty state shown.
    fn clear(&mut self) {
        self.output_text.clear();
        self.output = Content::new();
        self.results = false;
        self.status = JsonStatus::None;
    }

    /// The input formatted, as the C# `FormatAsync` (`JsonFormatterViewModel.cs:134-164`):
    /// blank input clears; a document past [`ASYNC_THRESHOLD_BYTES`] is formatted away
    /// from the window, the rest at once.
    fn format(&mut self, indented: bool) -> Outcome {
        if self.processing.is_some() {
            return Outcome::Done;
        }
        let input = self.input_text();
        if input.trim().is_empty() {
            self.clear();
            return Outcome::Done;
        }
        if input.len() > ASYNC_THRESHOLD_BYTES {
            self.runs += 1;
            self.processing = Some(self.runs);
            self.status = JsonStatus::Processing;
            return Outcome::Format(self.runs, indented, input);
        }
        let formatted = json_codec::format_input(&input, indented, super::NEW_LINE);
        self.apply(formatted, indented);
        Outcome::Done
    }

    /// What formatting gave, shown, as the C# `ApplyResult`
    /// (`JsonFormatterViewModel.cs:166-203`).
    fn apply(&mut self, formatted: JsonFormat, indented: bool) {
        match formatted {
            JsonFormat::Success(output) => {
                let count = json_count(&output);
                self.output = Content::with_text(&output);
                self.output_text = output;
                self.results = true;
                self.status = if indented {
                    JsonStatus::Prettified(count)
                } else {
                    JsonStatus::Minified(count)
                };
            }
            JsonFormat::Empty => self.clear(),
            JsonFormat::InputTooLarge => {
                self.clear();
                self.status = JsonStatus::TooLarge;
            }
            JsonFormat::ParseError(error) => {
                self.clear();
                self.status = JsonStatus::Invalid(error);
            }
        }
    }

    /// The tool's page, as the C# `JsonFormatterView.xaml`: the input, Prettify and Minify,
    /// the output or its empty state, the status and the copy button.
    pub fn view<'a>(&'a self, tab: TabId, state: &ToolPane) -> Element<'a, Message> {
        let send = move |message| Message::Tool(tab, ToolMessage::Json(message));
        let idle = !self.processing();
        let input = super::text_box(&self.input, Some(fl!("ui-tool-json-placeholder")))
            .height(Length::Fill)
            .on_action(move |action| send(JsonMessage::Input(action)))
            .key_binding(move |press| binding(press, idle, send));
        let actions = row![
            super::action_button(
                fl!("ui-tool-json-prettify"),
                true,
                idle.then(|| send(JsonMessage::Prettify))
            ),
            super::action_button(
                fl!("ui-tool-json-minify"),
                false,
                idle.then(|| send(JsonMessage::Minify))
            ),
        ]
        .spacing(spacing::SM);
        let output: Element<'a, Message> = if self.results {
            super::text_box(&self.output, None)
                .height(Length::Fill)
                .on_action(move |action| send(JsonMessage::Output(action)))
                .into()
        } else {
            super::empty_state(fl!("ui-tool-json-empty"))
        };
        let footer = row![
            container(super::status_line(self.status.said())).width(Length::Fill),
            super::copy_button(
                fl!("ui-tool-json-copy"),
                state.copied(CopySlot::JsonOutput),
                send(JsonMessage::Copy),
                super::COPY_PADDING,
            ),
        ]
        .spacing(spacing::SM)
        .align_y(iced::Alignment::Center);
        super::tool_body(
            column![
                super::field_label(fl!("ui-tool-json-input")),
                input,
                container(actions).center_x(Length::Fill),
                super::field_label(fl!("ui-tool-json-output")),
                output,
                footer,
            ]
            .spacing(STACK_GAP),
        )
    }
}

/// The length the status says of `output`: in UTF-16 units, as .NET's `string.Length`.
fn json_count(output: &str) -> usize {
    output.chars().map(char::len_utf16).sum()
}

/// The input's keys, as the C# `OnKeyDown`: Ctrl+Enter prettifies, and Ctrl+Shift+Enter
/// minifies, as the C# help says; while `idle`. The rest as any text box.
fn binding(
    press: KeyPress,
    idle: bool,
    send: impl Fn(JsonMessage) -> Message,
) -> Option<Binding<Message>> {
    if idle
        && matches!(press.status, Status::Focused { .. })
        && press.key == Key::Named(Named::Enter)
        && press.modifiers.control()
        && !press.modifiers.alt()
    {
        return Some(Binding::Custom(send(if press.modifiers.shift() {
            JsonMessage::Minify
        } else {
            JsonMessage::Prettify
        })));
    }
    Binding::from_key_press(press)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use iced::widget::text_editor::Edit;

    use super::*;

    fn typed(pane: &mut JsonPane, typed: &str) {
        let _ = pane.update(JsonMessage::Input(Action::Edit(Edit::Paste(Arc::new(
            typed.to_owned(),
        )))));
    }

    #[test]
    fn prettify_and_minify_show_the_output_and_its_length() {
        let mut pane = JsonPane::default();
        typed(&mut pane, "{\"a\":1}");
        assert_eq!(pane.update(JsonMessage::Prettify), Outcome::Done);
        let pretty = format!("{{{0}  \"a\": 1{0}}}", super::super::NEW_LINE);
        assert_eq!(pane.output_text, pretty);
        assert_eq!(&pane.status, &JsonStatus::Prettified(json_count(&pretty)));
        assert!(pane.results);
        let _ = pane.update(JsonMessage::Minify);
        assert_eq!(pane.output_text, "{\"a\":1}");
        assert_eq!(&pane.status, &JsonStatus::Minified(7));
        assert_eq!(
            pane.update(JsonMessage::Copy),
            Outcome::Copy("{\"a\":1}".to_owned())
        );
    }

    #[test]
    fn an_edit_clears_the_output_and_an_error_says_where() {
        let mut pane = JsonPane::default();
        typed(&mut pane, "[1]");
        let _ = pane.update(JsonMessage::Minify);
        typed(&mut pane, ",");
        assert!(!pane.results && pane.output_text.is_empty());
        assert_eq!(&pane.status, &JsonStatus::None);
        let _ = pane.update(JsonMessage::Prettify);
        let JsonStatus::Invalid(error) = &pane.status else {
            panic!("refused: {:?}", pane.status);
        };
        assert_eq!(error.line, Some(1));
        let (said, is_error) = pane.status.said().expect("said");
        assert!(is_error && said.contains("line 1"), "{said}");
    }

    #[test]
    fn blank_input_clears_and_a_large_one_is_formatted_away() {
        let mut pane = JsonPane::default();
        typed(&mut pane, "  ");
        assert_eq!(pane.update(JsonMessage::Prettify), Outcome::Done);
        assert_eq!(&pane.status, &JsonStatus::None);
        let large = format!("[\"{}\"]", "a".repeat(ASYNC_THRESHOLD_BYTES));
        let mut pane = JsonPane::default();
        typed(&mut pane, &large);
        let Outcome::Format(run, true, sent) = pane.update(JsonMessage::Prettify) else {
            panic!("formatted away");
        };
        assert_eq!(sent, large);
        assert!(pane.processing());
        assert_eq!(&pane.status, &JsonStatus::Processing);
        assert_eq!(
            pane.update(JsonMessage::Minify),
            Outcome::Done,
            "the buttons wait"
        );
        let formatted = json_codec::format_input(&large, true, super::super::NEW_LINE);
        // A run that is not the last is not shown.
        let _ = pane.update(JsonMessage::Formatted(
            run + 1,
            true,
            Some(formatted.clone()),
        ));
        assert!(pane.processing());
        let _ = pane.update(JsonMessage::Formatted(run, true, Some(formatted)));
        assert!(!pane.processing() && pane.results);
        assert!(matches!(&pane.status, JsonStatus::Prettified(_)));
    }
}
