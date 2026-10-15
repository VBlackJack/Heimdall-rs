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

//! The Notes Scratchpad tool view: Markdown notes editor, tag organization,
//! and Markdown / HTML export.

use heimdall_app::TabId;
use iced::widget::{button, column, container, row, scrollable, text, text_editor, text_input};
use iced::{Alignment, Element, Length, Task, Theme};

use heimdall_core::tools::notes_storage::NotesStorage;

use super::{CopySlot, ToolPane};
use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, spacing};

/// Messages emitted by the Notes view.
#[derive(Debug, Clone)]
pub enum NotesMessage {
    /// Search filter text changed.
    SearchChanged(String),
    /// Select a note by ID.
    SelectNote(String),
    /// Create a new note.
    NewNote,
    /// Delete selected note.
    DeleteNote(String),
    /// Active note title changed.
    TitleChanged(String),
    /// Active note tags changed.
    TagsChanged(String),
    /// Content editor action.
    ContentAction(text_editor::Action),
    /// Copy markdown to clipboard.
    CopyMarkdown,
    /// Copy rendered HTML to clipboard.
    CopyHtml,
}

/// The Notes tool pane state.
#[derive(Debug)]
pub struct NotesPane {
    storage: NotesStorage,
    selected_id: Option<String>,
    search_query: String,
    active_title: String,
    active_tags: String,
    content_editor: text_editor::Content,
}

impl Default for NotesPane {
    fn default() -> Self {
        let mut storage = NotesStorage::new();
        let initial_note = storage.create_note(
            "Welcome to Notes",
            "# Welcome to Heimdall Notes\n\nA lightweight scratchpad with **Markdown** support and *tagging*.\n\n- Quick scratchpad for credentials, logs, and command outputs\n- Export to Markdown or HTML\n- Filter notes by tags or content",
            &["welcome", "scratchpad"],
        );
        let active_title = initial_note.title.clone();
        let active_tags = initial_note.tags.join(", ");
        let content_editor = text_editor::Content::with_text(&initial_note.content);
        let selected_id = Some(initial_note.id);

        Self {
            storage,
            selected_id,
            search_query: String::new(),
            active_title,
            active_tags,
            content_editor,
        }
    }
}

/// Outcome of updating the Notes tool pane.
#[derive(Debug)]
#[allow(dead_code)]
pub enum Outcome {
    None,
    Task(Task<Message>),
    Copy(CopySlot, String),
}

impl NotesPane {
    /// Handles tool messages.
    pub fn update(&mut self, _tab: TabId, message: NotesMessage) -> Outcome {
        match message {
            NotesMessage::SearchChanged(query) => {
                self.search_query = query;
                Outcome::None
            }
            NotesMessage::SelectNote(id) => {
                self.sync_active_note();
                if let Some(note) = self.storage.get_note(&id) {
                    self.active_title = note.title.clone();
                    self.active_tags = note.tags.join(", ");
                    self.content_editor = text_editor::Content::with_text(&note.content);
                    self.selected_id = Some(id);
                }
                Outcome::None
            }
            NotesMessage::NewNote => {
                self.sync_active_note();
                let note = self.storage.create_note("New Note", "", &[]);
                self.active_title = note.title.clone();
                self.active_tags = String::new();
                self.content_editor = text_editor::Content::new();
                self.selected_id = Some(note.id);
                Outcome::None
            }
            NotesMessage::DeleteNote(id) => {
                self.storage.delete_note(&id);
                if self.selected_id.as_deref() == Some(&id) {
                    if let Some(first) = self.storage.notes().first() {
                        self.active_title = first.title.clone();
                        self.active_tags = first.tags.join(", ");
                        self.content_editor = text_editor::Content::with_text(&first.content);
                        self.selected_id = Some(first.id.clone());
                    } else {
                        self.selected_id = None;
                        self.active_title.clear();
                        self.active_tags.clear();
                        self.content_editor = text_editor::Content::new();
                    }
                }
                Outcome::None
            }
            NotesMessage::TitleChanged(title) => {
                self.active_title = title;
                self.sync_active_note();
                Outcome::None
            }
            NotesMessage::TagsChanged(tags) => {
                self.active_tags = tags;
                self.sync_active_note();
                Outcome::None
            }
            NotesMessage::ContentAction(action) => {
                self.content_editor.perform(action);
                self.sync_active_note();
                Outcome::None
            }
            NotesMessage::CopyMarkdown => {
                let text = self.content_editor.text();
                Outcome::Copy(CopySlot::NotesExport, text)
            }
            NotesMessage::CopyHtml => {
                if let Some(id) = &self.selected_id {
                    if let Some(note) = self.storage.get_note(id) {
                        let html = note.to_html();
                        return Outcome::Copy(CopySlot::NotesExport, html);
                    }
                }
                Outcome::None
            }
        }
    }

    fn sync_active_note(&mut self) {
        if let Some(id) = &self.selected_id {
            let tags: Vec<String> = self
                .active_tags
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            let tags_refs: Vec<&str> = tags.iter().map(|s| s.as_str()).collect();
            let content = self.content_editor.text();
            self.storage
                .update_note(id, &self.active_title, &content, &tags_refs);
        }
    }

    /// Renders the Notes Scratchpad tool view.
    pub fn view<'a>(
        &'a self,
        tab: TabId,
        state: &ToolPane,
        _theme: &Theme,
    ) -> Element<'a, Message> {
        let send = move |msg| Message::Tool(tab, super::ToolMessage::Notes(msg));

        let search_input = text_input(&fl!("ui-tool-notes-search-placeholder"), &self.search_query)
            .on_input(move |s| send(NotesMessage::SearchChanged(s)))
            .width(Length::Fill);

        let btn_new = button(text(fl!("ui-tool-notes-btn-new")).size(font_size::CAPTION))
            .padding([spacing::XS, spacing::SM])
            .on_press(send(NotesMessage::NewNote));

        let search_row = row![search_input, btn_new]
            .spacing(spacing::SM)
            .align_y(Alignment::Center);

        let matching_notes = self.storage.search(&self.search_query);

        let mut notes_list_col = column![].spacing(spacing::XS);

        for note in matching_notes {
            let is_selected = self.selected_id.as_deref() == Some(&note.id);
            let note_id = note.id.clone();
            let delete_id = note.id.clone();

            let title_text = text(&note.title).size(font_size::CAPTION);
            let date_text = text(&note.updated_at)
                .size(font_size::SMALL_CAPTION)
                .style(text::secondary);

            let note_item = button(
                row![
                    column![title_text, date_text]
                        .spacing(spacing::XS)
                        .width(Length::Fill),
                    button(text("×").size(font_size::CAPTION))
                        .padding([0.0, spacing::XS])
                        .style(styles::danger)
                        .on_press(send(NotesMessage::DeleteNote(delete_id))),
                ]
                .spacing(spacing::SM)
                .align_y(Alignment::Center),
            )
            .padding(spacing::XS)
            .style(if is_selected {
                styles::primary
            } else {
                styles::secondary
            })
            .width(Length::Fill)
            .on_press(send(NotesMessage::SelectNote(note_id)));

            notes_list_col = notes_list_col.push(note_item);
        }

        let left_sidebar = container(
            column![search_row, scrollable(notes_list_col).height(Length::Fill),]
                .spacing(spacing::SM),
        )
        .width(Length::Fixed(260.0))
        .height(Length::Fill)
        .padding(spacing::SM)
        .style(styles::card);

        let right_main: Element<'a, Message> = if self.selected_id.is_some() {
            let title_input =
                text_input(&fl!("ui-tool-notes-title-placeholder"), &self.active_title)
                    .on_input(move |s| send(NotesMessage::TitleChanged(s)))
                    .size(font_size::SUBTITLE)
                    .width(Length::Fill);

            let tags_input = text_input(&fl!("ui-tool-notes-tags-placeholder"), &self.active_tags)
                .on_input(move |s| send(NotesMessage::TagsChanged(s)))
                .size(font_size::CAPTION)
                .width(Length::Fill);

            let copy_md_btn = super::copy_button(
                fl!("ui-tool-notes-btn-copy-md"),
                state.copied(CopySlot::NotesExport),
                send(NotesMessage::CopyMarkdown),
                super::COPY_PADDING,
            );

            let copy_html_btn = super::copy_button(
                fl!("ui-tool-notes-btn-copy-html"),
                state.copied(CopySlot::NotesExport),
                send(NotesMessage::CopyHtml),
                super::COPY_PADDING,
            );

            let toolbar = row![copy_md_btn, copy_html_btn]
                .spacing(spacing::SM)
                .align_y(Alignment::Center);

            let editor = text_editor(&self.content_editor)
                .on_action(move |action| send(NotesMessage::ContentAction(action)))
                .height(Length::Fill);

            container(
                column![title_input, tags_input, toolbar, editor]
                    .spacing(spacing::SM)
                    .height(Length::Fill),
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(spacing::SM)
            .into()
        } else {
            container(
                text(fl!("ui-tool-notes-no-selection"))
                    .size(font_size::CAPTION)
                    .style(text::secondary),
            )
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
        };

        row![left_sidebar, right_main]
            .spacing(spacing::MD)
            .padding(spacing::MD)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }
}
