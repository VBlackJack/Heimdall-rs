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

//! The Diagram Editor tool view: architecture and topology diagram editor
//! supporting Mermaid, Graphviz DOT, and ASCII network topologies.

use heimdall_app::TabId;
use iced::widget::{button, column, container, row, text, text_editor};
use iced::{Alignment, Element, Length, Task, Theme};

use heimdall_core::tools::diagram_editor::{DIAGRAM_TEMPLATES, DiagramDocument};

use super::{CopySlot, ToolPane};
use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, spacing};

/// Messages emitted by the Diagram Editor view.
#[derive(Debug, Clone)]
pub enum DiagramMessage {
    /// Select a diagram template by index.
    SelectTemplate(usize),
    /// Content editor action.
    ContentAction(text_editor::Action),
    /// Copy diagram source to clipboard.
    CopySource,
    /// Reset to selected template.
    Reset,
    /// Clear editor.
    Clear,
}

/// The Diagram Editor tool pane state.
#[derive(Debug)]
pub struct DiagramPane {
    document: DiagramDocument,
    content_editor: text_editor::Content,
    template_index: usize,
}

impl Default for DiagramPane {
    fn default() -> Self {
        let template_index = 0;
        let document = DiagramDocument::from_template(&DIAGRAM_TEMPLATES[template_index]);
        let content_editor = text_editor::Content::with_text(&document.content);

        Self {
            document,
            content_editor,
            template_index,
        }
    }
}

/// Outcome of updating the Diagram Editor tool pane.
#[derive(Debug)]
#[allow(dead_code)]
pub enum Outcome {
    None,
    Task(Task<Message>),
    Copy(CopySlot, String),
}

impl DiagramPane {
    /// Handles tool messages.
    pub fn update(&mut self, _tab: TabId, message: DiagramMessage) -> Outcome {
        match message {
            DiagramMessage::SelectTemplate(index) => {
                let idx = index.min(DIAGRAM_TEMPLATES.len().saturating_sub(1));
                self.template_index = idx;
                self.document = DiagramDocument::from_template(&DIAGRAM_TEMPLATES[idx]);
                self.content_editor = text_editor::Content::with_text(&self.document.content);
                Outcome::None
            }
            DiagramMessage::ContentAction(action) => {
                self.content_editor.perform(action);
                let current_text = self.content_editor.text();
                self.document.update_source(current_text);
                Outcome::None
            }
            DiagramMessage::CopySource => {
                let text = self.content_editor.text();
                Outcome::Copy(CopySlot::DiagramSource, text)
            }
            DiagramMessage::Reset => {
                let idx = self
                    .template_index
                    .min(DIAGRAM_TEMPLATES.len().saturating_sub(1));
                self.document = DiagramDocument::from_template(&DIAGRAM_TEMPLATES[idx]);
                self.content_editor = text_editor::Content::with_text(&self.document.content);
                Outcome::None
            }
            DiagramMessage::Clear => {
                self.document.clear();
                self.content_editor = text_editor::Content::new();
                Outcome::None
            }
        }
    }

    /// Renders the Diagram Editor tool view.
    pub fn view<'a>(
        &'a self,
        tab: TabId,
        state: &ToolPane,
        _theme: &Theme,
    ) -> Element<'a, Message> {
        let send = move |msg| Message::Tool(tab, super::ToolMessage::Diagram(msg));

        let btn_dmz =
            button(text(fl!("ui-tool-diagram-template-perimeter")).size(font_size::CAPTION))
                .padding([spacing::XS, spacing::SM])
                .style(if self.template_index == 0 {
                    styles::primary
                } else {
                    styles::secondary
                })
                .on_press(send(DiagramMessage::SelectTemplate(0)));

        let btn_cloud =
            button(text(fl!("ui-tool-diagram-template-cloud")).size(font_size::CAPTION))
                .padding([spacing::XS, spacing::SM])
                .style(if self.template_index == 1 {
                    styles::primary
                } else {
                    styles::secondary
                })
                .on_press(send(DiagramMessage::SelectTemplate(1)));

        let btn_ascii =
            button(text(fl!("ui-tool-diagram-template-ascii")).size(font_size::CAPTION))
                .padding([spacing::XS, spacing::SM])
                .style(if self.template_index == 2 {
                    styles::primary
                } else {
                    styles::secondary
                })
                .on_press(send(DiagramMessage::SelectTemplate(2)));

        let btn_ad = button(text(fl!("ui-tool-diagram-template-ad")).size(font_size::CAPTION))
            .padding([spacing::XS, spacing::SM])
            .style(if self.template_index == 3 {
                styles::primary
            } else {
                styles::secondary
            })
            .on_press(send(DiagramMessage::SelectTemplate(3)));

        let copy_btn = super::copy_button(
            fl!("ui-tool-diagram-btn-copy-source"),
            state.copied(CopySlot::DiagramSource),
            send(DiagramMessage::CopySource),
            super::COPY_PADDING,
        );

        let btn_reset = button(text(fl!("ui-tool-diagram-btn-clear")).size(font_size::CAPTION))
            .padding([spacing::XS, spacing::SM])
            .style(styles::secondary)
            .on_press(send(DiagramMessage::Reset));

        let dirty_indicator = if self.document.dirty {
            text(format!("• {}", fl!("ui-tool-diagram-status-dirty")))
                .size(font_size::SMALL_CAPTION)
                .style(text::secondary)
        } else {
            text(fl!("ui-tool-diagram-status-saved"))
                .size(font_size::SMALL_CAPTION)
                .style(text::secondary)
        };

        let format_badge = text(self.document.format.label())
            .size(font_size::SMALL_CAPTION)
            .style(text::secondary);

        let top_row = row![
            text(fl!("ui-tool-diagram-template-select")).size(font_size::CAPTION),
            btn_dmz,
            btn_cloud,
            btn_ascii,
            btn_ad,
            copy_btn,
            btn_reset,
            dirty_indicator,
            format_badge,
        ]
        .spacing(spacing::SM)
        .align_y(Alignment::Center);

        let editor = text_editor(&self.content_editor)
            .on_action(move |action| send(DiagramMessage::ContentAction(action)))
            .height(Length::Fill);

        let editor_container = container(
            column![
                text(fl!("ui-tool-diagram-editor-header"))
                    .size(font_size::CAPTION)
                    .style(text::secondary),
                editor,
            ]
            .spacing(spacing::XS)
            .height(Length::Fill),
        )
        .padding(spacing::SM)
        .style(styles::card)
        .height(Length::Fill);

        column![top_row, editor_container]
            .spacing(spacing::MD)
            .padding(spacing::MD)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }
}
