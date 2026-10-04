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

//! The Settings box of the RDP resolution presets, as the C# one: one `WIDTHxHEIGHT` per
//! line, read as a whole. A text that reads as presets is applied as it is typed; one that
//! does not stays typed, its bad lines said under it, and nothing is dropped without a word.

use heimdall_app::{Message as AppMessage, SettingsMessage};
use heimdall_core::profile::{
    FIXED_HEIGHT_MAX, FIXED_SIDE_MIN, FIXED_WIDTH_MAX, RESOLUTION_PRESETS,
    parse_resolution_presets, resolution_text,
};
use iced::widget::text_editor::{Action, Content};
use iced::widget::{button, column, row, text, text_editor};
use iced::{Element, Length};

use crate::i18n::fl;
use crate::shell::Message;

const SPACING: f32 = 8.0;
/// Size of the hint and of the error under the box.
const CAPTION_SIZE: f32 = 12.0;
/// Height of the box: the ten built-in presets in sight.
const BOX_HEIGHT: f32 = 200.0;
/// Between the bad lines the error quotes, as the C# `ResolutionPresetsErrorSeparator`.
const INVALID_SEPARATOR: &str = ", ";

/// The box and what it last read.
pub struct PresetsEditor {
    content: Content,
    /// The presets the box stands for, as the settings last had them or as last applied.
    seen: Vec<(u16, u16)>,
    /// The lines typed that are not a preset.
    invalid: Vec<String>,
}

impl PresetsEditor {
    /// The box showing `presets`, one per line.
    #[must_use]
    pub fn new(presets: &[(u16, u16)]) -> Self {
        let lines: Vec<String> = presets.iter().copied().map(resolution_text).collect();
        Self {
            content: Content::with_text(&lines.join("\n")),
            seen: presets.to_vec(),
            invalid: Vec::new(),
        }
    }

    /// Shows `presets` again when the settings changed them from elsewhere, a reset or a
    /// change that could not be saved; what is typed otherwise stays.
    pub fn sync(&mut self, presets: &[(u16, u16)]) {
        if self.seen != presets {
            *self = Self::new(presets);
        }
    }

    /// An action in the box: the presets to apply when the edit leaves a text that reads as
    /// presets other than the ones shown; `None` otherwise.
    pub fn perform(&mut self, action: Action) -> Option<Vec<(u16, u16)>> {
        let edit = action.is_edit();
        self.content.perform(action);
        if !edit {
            return None;
        }
        match parse_resolution_presets(&self.content.text()) {
            Ok(presets) => {
                self.invalid.clear();
                (presets != self.seen).then(|| {
                    self.seen.clone_from(&presets);
                    presets
                })
            }
            Err(invalid) => {
                self.invalid = invalid;
                None
            }
        }
    }

    /// The lines typed that are not a preset.
    #[must_use]
    pub fn invalid(&self) -> &[String] {
        &self.invalid
    }

    /// The text typed.
    #[must_use]
    pub fn text(&self) -> String {
        self.content.text()
    }

    /// The box, its hint, the bad lines when there are any, and "Reset to defaults".
    pub fn view(&self) -> Element<'_, Message> {
        let mut card = column![
            text(fl!("ui-settings-rdp-resolution-presets")),
            text_editor(&self.content)
                .on_action(Message::PresetsEdited)
                .height(Length::Fixed(BOX_HEIGHT)),
            text(fl!("ui-settings-rdp-resolution-presets-hint")).size(CAPTION_SIZE),
        ]
        .spacing(SPACING);
        if !self.invalid.is_empty() {
            card = card.push(
                text(fl!(
                    "ui-settings-rdp-resolution-presets-invalid",
                    lines = self.invalid.join(INVALID_SEPARATOR),
                    min = FIXED_SIDE_MIN,
                    width = FIXED_WIDTH_MAX,
                    height = FIXED_HEIGHT_MAX
                ))
                .size(CAPTION_SIZE)
                .style(text::danger),
            );
        }
        card.push(row![
            button(text(fl!("ui-settings-rdp-resolution-presets-reset")))
                .style(button::secondary)
                .on_press(Message::App(AppMessage::Settings(
                    SettingsMessage::RdpResolutionPresets(RESOLUTION_PRESETS.to_vec()),
                ))),
        ])
        .into()
    }
}

#[cfg(test)]
mod tests {
    use iced::widget::text_editor::{Edit, Motion};

    use super::*;

    fn typed(editor: &mut PresetsEditor, text: &str) -> Option<Vec<(u16, u16)>> {
        let mut applied = None;
        for character in text.chars() {
            let edit = if character == '\n' {
                Edit::Enter
            } else {
                Edit::Insert(character)
            };
            if let Some(presets) = editor.perform(Action::Edit(edit)) {
                applied = Some(presets);
            }
        }
        applied
    }

    #[test]
    fn a_line_being_typed_is_kept_and_said_until_it_reads_as_a_preset() {
        let mut editor = PresetsEditor::new(&[(1920, 1080)]);
        editor.perform(Action::Move(Motion::DocumentEnd));
        assert_eq!(typed(&mut editor, "\n1280x"), None, "not a preset yet");
        assert_eq!(editor.invalid(), ["1280x"], "said, not dropped");
        assert!(editor.text().contains("1280x"), "{:?}", editor.text());
        assert_eq!(
            typed(&mut editor, "720"),
            Some(vec![(1920, 1080), (1280, 720)])
        );
        assert!(editor.invalid().is_empty());
        // Out of the limits, as the C# refuses "99999x1".
        assert_eq!(typed(&mut editor, "\n9999x1"), None);
        assert_eq!(editor.invalid(), ["9999x1"]);
    }

    #[test]
    fn a_change_from_elsewhere_shows_again_and_one_applied_here_does_not() {
        let mut editor = PresetsEditor::new(&[(1920, 1080)]);
        editor.perform(Action::Move(Motion::DocumentEnd));
        let applied = typed(&mut editor, "\n1024x768").expect("applied");
        editor.sync(&applied);
        assert!(editor.text().contains("1024x768"), "what is typed stays");
        editor.sync(&RESOLUTION_PRESETS);
        assert_eq!(editor.text().lines().count(), RESOLUTION_PRESETS.len());
        assert!(editor.invalid().is_empty());
    }
}
