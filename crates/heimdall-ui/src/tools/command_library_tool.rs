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

//! The command library: the user's own commands, made, changed and deleted here, then the
//! seeded catalog filtered by platform and by what is typed. Each command shows its
//! description, the values it asks for and its command with them put in, copied by its own
//! button or sent to a terminal tab, once the user agrees to the question that follows.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use heimdall_app::{SendTarget, TabId};
use heimdall_core::tools::command_library::{
    self, BUILTIN_SNIPPETS, CommandSnippet, ParamLabel, SnippetCategory, SnippetId, SnippetPlatform,
};
use heimdall_core::tools::user_commands::{
    self, Criticality, Draft, DraftError, GenerateError, LIBRARY_FILE, LibraryError,
    TemplatePlatform, UserCommand, UserLibrary, UserPlatform, ValueError,
};
use iced::widget::{Column, Row, button, column, container, row, scrollable, text, text_input};
use iced::{Element, Length};

use super::{CopySlot, ToolMessage, ToolPane};
use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, spacing};

/// Width of a value's label, as the result grids' label column.
const PARAM_LABEL_WIDTH: f32 = 160.0;

/// Width of the editor's field labels.
const EDITOR_LABEL_WIDTH: f32 = 140.0;

/// A command of the library: one of the seeded catalog or one of the user's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandRef {
    /// A command of the catalog.
    Catalog(SnippetId),
    /// A command of the user, by identity.
    User(String),
}

/// What the command library is asked.
#[derive(Debug, Clone)]
pub enum CommandLibraryMessage {
    /// The platform shown; `None` for every one.
    Platform(Option<SnippetPlatform>),
    /// The search typed.
    Search(String),
    /// Value `.1` of snippet `.0` typed.
    Param(SnippetId, &'static str, String),
    /// Value `.1` of the user's command `.0` typed.
    UserParam(String, String, String),
    /// The template of the user's command `.0` shown.
    UserTemplate(String, TemplatePlatform),
    /// Copy the command.
    Copy(CommandRef),
    /// Ask to send the command to the terminal chosen.
    Send(CommandRef),
    /// The terminal tab the commands are sent to.
    Target(TabId),
    /// Start a new command of the user.
    New,
    /// Edit the user's command of this identity.
    Edit(String),
    /// A field of the editor changed.
    Field(Field, String),
    /// The platforms of the command edited.
    EditPlatform(UserPlatform),
    /// The level of the command edited.
    EditLevel(Criticality),
    /// Keep the command edited.
    Save,
    /// Leave the editor.
    CancelEdit,
    /// Ask whether to delete the user's command of this identity.
    AskDelete(String),
    /// Delete it.
    ConfirmDelete(String),
    /// Keep it.
    CancelDelete,
}

/// A text field of the editor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    /// The title.
    Title,
    /// The description.
    Description,
    /// The category.
    Category,
    /// The tags.
    Tags,
    /// The PowerShell pattern.
    WindowsPattern,
    /// The POSIX shell pattern.
    LinuxPattern,
}

/// What an update asks of the window.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Nothing outside the pane.
    None,
    /// Copy `.1` by the button of `.0`.
    Copy(CopySlot, String),
    /// Ask whether to write `command` into terminal `target`.
    Send {
        /// The terminal tab.
        target: TabId,
        /// The command.
        command: String,
        /// Marked dangerous.
        dangerous: bool,
    },
}

/// The editor: the draft and why it was not kept.
#[derive(Debug, Clone, Default)]
struct Editor {
    draft: Draft,
    problem: Option<DraftError>,
}

/// What went wrong with the user's file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Failure {
    /// It could not be read: it is left as it is and nothing is changed.
    Unreadable,
    /// The change could not be saved.
    Unsaved,
}

/// The command library's state.
#[derive(Debug, Default)]
pub struct CommandLibraryPane {
    platform: Option<SnippetPlatform>,
    search: String,
    values: HashMap<(SnippetId, &'static str), String>,
    /// Where the user's commands are kept; `None` when the platform has no folder.
    file: Option<PathBuf>,
    library: UserLibrary,
    failure: Option<Failure>,
    user_values: HashMap<(String, String), String>,
    user_templates: HashMap<String, TemplatePlatform>,
    editor: Option<Editor>,
    deleting: Option<String>,
    targets: Vec<SendTarget>,
    target: Option<TabId>,
}

impl CommandLibraryPane {
    /// A pane whose user commands are kept beside `profiles_file`.
    #[must_use]
    pub fn new(profiles_file: Option<&Path>) -> Self {
        let file = profiles_file
            .and_then(Path::parent)
            .map(|folder| folder.join(LIBRARY_FILE));
        let mut pane = Self {
            file,
            ..Self::default()
        };
        if let Some(path) = &pane.file {
            match UserLibrary::load(path) {
                Ok(library) => pane.library = library,
                Err(LibraryError::Io(_) | LibraryError::TooLarge | LibraryError::Format) => {
                    pane.failure = Some(Failure::Unreadable);
                }
            }
        }
        pane
    }

    /// The terminals commands can be sent to, as the application offers them now.
    pub fn set_targets(&mut self, targets: Vec<SendTarget>) {
        self.targets = targets;
    }

    /// The terminal a command goes to: the one chosen while it is still there, else the
    /// first.
    fn effective_target(&self) -> Option<&SendTarget> {
        self.target
            .and_then(|chosen| self.targets.iter().find(|each| each.tab == chosen))
            .or_else(|| self.targets.first())
    }

    /// Applies `message`.
    pub fn update(&mut self, message: CommandLibraryMessage) -> Outcome {
        match message {
            CommandLibraryMessage::Platform(platform) => self.platform = platform,
            CommandLibraryMessage::Search(typed) => self.search = typed,
            CommandLibraryMessage::Param(id, key, value) => {
                self.values.insert((id, key), value);
            }
            CommandLibraryMessage::UserParam(id, name, value) => {
                self.user_values.insert((id, name), value);
            }
            CommandLibraryMessage::UserTemplate(id, platform) => {
                self.user_templates.insert(id, platform);
            }
            CommandLibraryMessage::Copy(command) => {
                return match self.command_text(&command) {
                    Some(text) => Outcome::Copy(self.copy_slot(&command), text),
                    None => Outcome::None,
                };
            }
            CommandLibraryMessage::Send(command) => return self.send(&command),
            CommandLibraryMessage::Target(tab) => self.target = Some(tab),
            other => self.edit(other),
        }
        Outcome::None
    }

    /// The messages about the user's commands.
    fn edit(&mut self, message: CommandLibraryMessage) {
        match message {
            CommandLibraryMessage::New if self.can_edit() => {
                self.editor = Some(Editor::default());
            }
            CommandLibraryMessage::Edit(id) if self.can_edit() => {
                if let Some(command) = self.library.get(&id) {
                    self.editor = Some(Editor {
                        draft: Draft::of(command),
                        problem: None,
                    });
                }
            }
            CommandLibraryMessage::Field(field, value) => {
                if let Some(editor) = &mut self.editor {
                    let draft = &mut editor.draft;
                    match field {
                        Field::Title => draft.title = value,
                        Field::Description => draft.description = value,
                        Field::Category => draft.category = value,
                        Field::Tags => draft.tags = value,
                        Field::WindowsPattern => draft.windows_pattern = value,
                        Field::LinuxPattern => draft.linux_pattern = value,
                    }
                }
            }
            CommandLibraryMessage::EditPlatform(platform) => {
                if let Some(editor) = &mut self.editor {
                    editor.draft.platform = platform;
                }
            }
            CommandLibraryMessage::EditLevel(level) => {
                if let Some(editor) = &mut self.editor {
                    editor.draft.level = level;
                }
            }
            CommandLibraryMessage::Save => self.save_editor(),
            CommandLibraryMessage::CancelEdit => self.editor = None,
            CommandLibraryMessage::AskDelete(id) if self.can_edit() => self.deleting = Some(id),
            CommandLibraryMessage::ConfirmDelete(id) if self.can_edit() => {
                self.deleting = None;
                let before = self.library.clone();
                if self.library.delete(&id) {
                    self.keep(&before);
                }
            }
            CommandLibraryMessage::CancelDelete => self.deleting = None,
            _ => {}
        }
    }

    /// Whether the user's commands can be changed: their file was read, and has a place.
    fn can_edit(&self) -> bool {
        self.failure != Some(Failure::Unreadable) && self.file.is_some()
    }

    /// Keeps the editor's draft: saved to the file, or said why not.
    fn save_editor(&mut self) {
        let Some(editor) = self.editor.as_mut() else {
            return;
        };
        let before = self.library.clone();
        match self.library.upsert(&editor.draft) {
            Ok(_) => {
                if self.keep(&before) {
                    self.editor = None;
                }
            }
            Err(problem) => editor.problem = Some(problem),
        }
    }

    /// Writes the library to its file; when that fails the library goes back to `before`
    /// and the failure is said. Whether it was written.
    fn keep(&mut self, before: &UserLibrary) -> bool {
        let Some(path) = &self.file else {
            return false;
        };
        match self.library.save(path) {
            Ok(()) => {
                self.failure = None;
                true
            }
            Err(_) => {
                self.library = before.clone();
                self.failure = Some(Failure::Unsaved);
                false
            }
        }
    }

    /// The button that copies `command`.
    fn copy_slot(&self, command: &CommandRef) -> CopySlot {
        match command {
            CommandRef::Catalog(id) => CopySlot::CmdLibCommand(*id),
            CommandRef::User(id) => CopySlot::CmdLibUserCommand(
                self.library
                    .commands()
                    .iter()
                    .position(|each| &each.id == id)
                    .unwrap_or_default(),
            ),
        }
    }

    /// The command as it is copied or sent; `None` when a user's command cannot be made
    /// from the values typed.
    fn command_text(&self, command: &CommandRef) -> Option<String> {
        match command {
            CommandRef::Catalog(id) => command_library::snippet(*id).map(|each| self.render(each)),
            CommandRef::User(id) => self
                .library
                .get(id)
                .and_then(|each| self.user_render(each).ok()),
        }
    }

    /// Asks the application to question the user about sending `command`.
    fn send(&self, command: &CommandRef) -> Outcome {
        let (Some(target), Some(text)) = (self.effective_target(), self.command_text(command))
        else {
            return Outcome::None;
        };
        let dangerous = match command {
            CommandRef::Catalog(id) => command_library::is_dangerous(*id),
            CommandRef::User(id) => self
                .library
                .get(id)
                .is_some_and(|each| each.level == Criticality::Dangerous),
        };
        Outcome::Send {
            target: target.tab,
            command: text,
            dangerous,
        }
    }

    /// The template of `command` shown: the one chosen, else this system's.
    fn template_shown(&self, command: &UserCommand) -> Option<TemplatePlatform> {
        let chosen = self
            .user_templates
            .get(&command.id)
            .copied()
            .filter(|platform| command.template_platforms().contains(platform));
        chosen.or_else(|| command.preferred_platform(cfg!(windows)))
    }

    /// The command of the user with the values typed.
    fn user_render(&self, command: &UserCommand) -> Result<String, GenerateError> {
        let platform = self
            .template_shown(command)
            .ok_or(GenerateError::EmptyPattern)?;
        let template = command
            .template(platform)
            .ok_or(GenerateError::EmptyPattern)?;
        user_commands::generate(template, platform, |name| {
            self.user_values
                .get(&(command.id.clone(), name.to_owned()))
                .map(String::as_str)
        })
    }

    /// The command of `snippet` with the values typed.
    fn render(&self, snippet: &CommandSnippet) -> String {
        snippet.render(|key| self.values.get(&(snippet.id, key)).map(String::as_str))
    }

    /// The snippets the platform and the search keep.
    fn shown(&self) -> Vec<&'static CommandSnippet> {
        BUILTIN_SNIPPETS
            .iter()
            .filter(|snippet| command_library::on_platform(snippet, self.platform))
            .filter(|snippet| {
                let (title, description) = snippet_texts(snippet.id);
                let category = category_name(snippet.category);
                command_library::matches_query(
                    snippet,
                    &[&title, &description, &category],
                    &self.search,
                )
            })
            .collect()
    }

    /// The commands of the user the platform and the search keep.
    fn shown_user(&self) -> Vec<&UserCommand> {
        let query = self.search.trim().to_lowercase();
        self.library
            .commands()
            .iter()
            .filter(|command| {
                self.platform.is_none_or(|platform| match platform {
                    SnippetPlatform::Linux => command.platform != UserPlatform::Windows,
                    SnippetPlatform::Windows => command.platform != UserPlatform::Linux,
                    SnippetPlatform::CrossPlatform => true,
                })
            })
            .filter(|command| {
                query.is_empty()
                    || [&command.title, &command.description, &command.category]
                        .iter()
                        .any(|field| field.to_lowercase().contains(&query))
                    || command
                        .tags
                        .iter()
                        .any(|tag| tag.to_lowercase().contains(&query))
                    || [
                        &command.windows_command_template,
                        &command.linux_command_template,
                    ]
                    .into_iter()
                    .flatten()
                    .any(|t| t.command_pattern.to_lowercase().contains(&query))
            })
            .collect()
    }

    /// The tool's page: the platforms and the search, the terminal commands go to, the
    /// editor when open, then the user's commands and the catalog.
    pub fn view<'a>(&'a self, tab: TabId, state: &ToolPane) -> Element<'a, Message> {
        let send = move |message| Message::Tool(tab, ToolMessage::CmdLib(message));
        let platform = |label: String, choice: Option<SnippetPlatform>| {
            button(text(label).size(font_size::BODY))
                .padding(super::COPY_PADDING)
                .style(if self.platform == choice {
                    styles::primary
                } else {
                    styles::secondary
                })
                .on_press(send(CommandLibraryMessage::Platform(choice)))
                .into()
        };
        let platforms = Row::with_children(
            std::iter::once(platform(fl!("ui-tool-cmdlib-platform-all"), None)).chain(
                SnippetPlatform::ALL
                    .into_iter()
                    .map(|choice| platform(platform_name(choice), Some(choice))),
            ),
        )
        .spacing(spacing::SM);
        let search = text_input(&fl!("ui-tool-cmdlib-search-placeholder"), &self.search)
            .size(font_size::BODY)
            .padding(super::INPUT_PADDING)
            .style(styles::text_input)
            .on_input(move |typed| send(CommandLibraryMessage::Search(typed)));
        let new = button(text(fl!("ui-tool-cmdlib-new")).size(font_size::BODY))
            .padding(super::COPY_PADDING)
            .style(styles::primary)
            .on_press_maybe(
                (self.can_edit() && self.editor.is_none())
                    .then(|| send(CommandLibraryMessage::New)),
            );
        let shown = self.shown();
        let mine = self.shown_user();
        let count = shown.len() + mine.len();
        let mut page = column![
            row![platforms, search, new]
                .spacing(spacing::MD)
                .align_y(iced::Alignment::Center),
            text(fl!("ui-tool-cmdlib-count", count = count))
                .size(font_size::CAPTION)
                .style(text::secondary),
        ]
        .spacing(spacing::SM);
        if let Some(failure) = self.failure {
            page = page.push(
                text(match failure {
                    Failure::Unreadable => fl!("ui-tool-cmdlib-library-unreadable"),
                    Failure::Unsaved => fl!("ui-tool-cmdlib-library-unsaved"),
                })
                .size(font_size::BODY)
                .style(text::danger),
            );
        }
        page = page.push(self.target_row(tab));
        let mut cards: Column<'a, Message> = Column::new().spacing(spacing::MD);
        if let Some(editor) = &self.editor {
            cards = cards.push(editor_view(tab, editor));
        }
        if count == 0 && self.editor.is_none() {
            return super::tool_body(page.push(super::empty_state(fl!("ui-tool-cmdlib-empty"))));
        }
        if !mine.is_empty() {
            cards = cards.push(
                text(fl!("ui-tool-cmdlib-section-mine"))
                    .size(font_size::BODY_LARGE)
                    .font(styles::SEMIBOLD),
            );
            for command in mine {
                cards = cards.push(self.user_card(tab, state, command));
            }
            if !shown.is_empty() {
                cards = cards.push(
                    text(fl!("ui-tool-cmdlib-section-catalog"))
                        .size(font_size::BODY_LARGE)
                        .font(styles::SEMIBOLD),
                );
            }
        }
        for snippet in shown {
            cards = cards.push(self.card(tab, state, snippet));
        }
        super::tool_body(page.push(scrollable(cards).height(Length::Fill)))
    }

    /// Where commands are sent: the terminals offered, the one chosen marked; a hint when
    /// there is none.
    fn target_row<'a>(&'a self, tab: TabId) -> Element<'a, Message> {
        let send = move |message| Message::Tool(tab, ToolMessage::CmdLib(message));
        let Some(chosen) = self.effective_target() else {
            return text(fl!("ui-tool-cmdlib-send-none"))
                .size(font_size::CAPTION)
                .style(text::secondary)
                .into();
        };
        let mut targets = Row::new()
            .spacing(spacing::SM)
            .align_y(iced::Alignment::Center)
            .push(
                text(fl!("ui-tool-cmdlib-send-to"))
                    .size(font_size::CAPTION)
                    .style(text::secondary),
            );
        for target in &self.targets {
            targets = targets.push(
                button(text(target.name.clone()).size(font_size::BODY))
                    .padding(super::COPY_PADDING)
                    .style(if target.tab == chosen.tab {
                        styles::primary
                    } else {
                        styles::secondary
                    })
                    .on_press(send(CommandLibraryMessage::Target(target.tab))),
            );
        }
        targets.into()
    }

    /// The copy and send buttons of `command`, with `rendered` its text or `None` when it
    /// cannot be made yet.
    fn actions<'a>(
        &self,
        tab: TabId,
        state: &ToolPane,
        command: &CommandRef,
        rendered: bool,
    ) -> Row<'a, Message> {
        let send = move |message| Message::Tool(tab, ToolMessage::CmdLib(message));
        let can_send = rendered && self.effective_target().is_some();
        row![
            super::copy_button(
                fl!("ui-tool-cmdlib-copy"),
                state.copied(self.copy_slot(command)),
                send(CommandLibraryMessage::Copy(command.clone())),
                super::COPY_PADDING,
            ),
            button(text(fl!("ui-tool-cmdlib-send")).size(font_size::BODY))
                .padding(super::COPY_PADDING)
                .style(styles::secondary)
                .on_press_maybe(
                    can_send.then(|| send(CommandLibraryMessage::Send(command.clone())))
                ),
        ]
        .spacing(spacing::SM)
    }

    /// One snippet: its title, platform and category, its description, its values and its
    /// command with its buttons.
    fn card<'a>(
        &'a self,
        tab: TabId,
        state: &ToolPane,
        snippet: &'static CommandSnippet,
    ) -> Element<'a, Message> {
        let send = move |message| Message::Tool(tab, ToolMessage::CmdLib(message));
        let (title, description) = snippet_texts(snippet.id);
        let header = row![
            text(title)
                .size(font_size::BODY_LARGE)
                .font(styles::SEMIBOLD),
            text(platform_name(snippet.platform))
                .size(font_size::CAPTION)
                .style(text::secondary),
            text(category_name(snippet.category))
                .size(font_size::CAPTION)
                .style(text::secondary),
        ]
        .spacing(spacing::SM)
        .align_y(iced::Alignment::Center);
        let params = Column::with_children(snippet.parameters.iter().map(|parameter| {
            let id = snippet.id;
            let key = parameter.key;
            let value = self.values.get(&(id, key)).map_or("", String::as_str);
            row![
                text(param_label(parameter.label))
                    .size(font_size::BODY)
                    .width(PARAM_LABEL_WIDTH),
                text_input(parameter.default_value, value)
                    .size(font_size::BODY)
                    .padding(super::INPUT_PADDING)
                    .style(styles::text_input)
                    .on_input(move |typed| send(CommandLibraryMessage::Param(id, key, typed))),
            ]
            .spacing(spacing::SM)
            .align_y(iced::Alignment::Center)
            .into()
        }))
        .spacing(spacing::XS);
        let command = command_box(
            self.render(snippet),
            self.actions(tab, state, &CommandRef::Catalog(snippet.id), true),
        );
        container(
            column![
                header,
                text(description)
                    .size(font_size::BODY)
                    .style(text::secondary),
                params,
                command
            ]
            .spacing(spacing::SM),
        )
        .padding(spacing::MD)
        .width(Length::Fill)
        .style(styles::card)
        .into()
    }

    /// One command of the user: as a snippet's card, with the template to use when it has
    /// two, why its command cannot be made yet, and its edit and delete buttons.
    fn user_card<'a>(
        &'a self,
        tab: TabId,
        state: &ToolPane,
        command: &'a UserCommand,
    ) -> Element<'a, Message> {
        let send = move |message| Message::Tool(tab, ToolMessage::CmdLib(message));
        let shown = self.template_shown(command);
        let header = row![
            text(command.title.as_str())
                .size(font_size::BODY_LARGE)
                .font(styles::SEMIBOLD),
            text(user_platform_name(command.platform))
                .size(font_size::CAPTION)
                .style(text::secondary),
            text(command.category.as_str())
                .size(font_size::CAPTION)
                .style(text::secondary),
            text(level_name(command.level))
                .size(font_size::CAPTION)
                .style(if command.level == Criticality::Dangerous {
                    text::danger
                } else {
                    text::secondary
                }),
        ]
        .spacing(spacing::SM)
        .align_y(iced::Alignment::Center);
        let mut body = column![header].spacing(spacing::SM);
        if !command.description.is_empty() {
            body = body.push(
                text(command.description.as_str())
                    .size(font_size::BODY)
                    .style(text::secondary),
            );
        }
        let platforms = command.template_platforms();
        if platforms.len() > 1 {
            body = body.push(Row::with_children(platforms.into_iter().map(|choice| {
                button(text(template_name(choice)).size(font_size::BODY))
                    .padding(super::COPY_PADDING)
                    .style(if shown == Some(choice) {
                        styles::primary
                    } else {
                        styles::secondary
                    })
                    .on_press(send(CommandLibraryMessage::UserTemplate(
                        command.id.clone(),
                        choice,
                    )))
                    .into()
            })));
        }
        if let Some(template) = shown.and_then(|platform| command.template(platform)) {
            body = body.push(
                Column::with_children(template.parameters.iter().map(|parameter| {
                    let id = command.id.clone();
                    let name = parameter.name.clone();
                    let value = self
                        .user_values
                        .get(&(id.clone(), name.clone()))
                        .map_or("", String::as_str);
                    row![
                        text(if parameter.label.is_empty() {
                            parameter.name.as_str()
                        } else {
                            parameter.label.as_str()
                        })
                        .size(font_size::BODY)
                        .width(PARAM_LABEL_WIDTH),
                        text_input(parameter.default_value.as_deref().unwrap_or(""), value)
                            .size(font_size::BODY)
                            .padding(super::INPUT_PADDING)
                            .style(styles::text_input)
                            .on_input(move |typed| {
                                send(CommandLibraryMessage::UserParam(
                                    id.clone(),
                                    name.clone(),
                                    typed,
                                ))
                            }),
                    ]
                    .spacing(spacing::SM)
                    .align_y(iced::Alignment::Center)
                    .into()
                }))
                .spacing(spacing::XS),
            );
        }
        let reference = CommandRef::User(command.id.clone());
        match self.user_render(command) {
            Ok(rendered) => {
                body = body.push(command_box(
                    rendered,
                    self.actions(tab, state, &reference, true),
                ));
            }
            Err(error) => {
                body = body.push(
                    text(generate_error(&error))
                        .size(font_size::BODY)
                        .style(text::danger),
                );
            }
        }
        let manage: Element<'a, Message> = if self.deleting.as_deref() == Some(&command.id) {
            row![
                text(fl!(
                    "ui-tool-cmdlib-delete-question",
                    title = command.title.as_str()
                ))
                .size(font_size::BODY),
                button(text(fl!("ui-tool-cmdlib-delete")).size(font_size::BODY))
                    .padding(super::COPY_PADDING)
                    .style(styles::danger)
                    .on_press(send(CommandLibraryMessage::ConfirmDelete(
                        command.id.clone()
                    ))),
                button(text(fl!("ui-tool-cmdlib-delete-keep")).size(font_size::BODY))
                    .padding(super::COPY_PADDING)
                    .style(styles::secondary)
                    .on_press(send(CommandLibraryMessage::CancelDelete)),
            ]
            .spacing(spacing::SM)
            .align_y(iced::Alignment::Center)
            .into()
        } else {
            row![
                button(text(fl!("ui-tool-cmdlib-edit")).size(font_size::BODY))
                    .padding(super::COPY_PADDING)
                    .style(styles::secondary)
                    .on_press_maybe(
                        (self.editor.is_none())
                            .then(|| send(CommandLibraryMessage::Edit(command.id.clone())))
                    ),
                button(text(fl!("ui-tool-cmdlib-delete")).size(font_size::BODY))
                    .padding(super::COPY_PADDING)
                    .style(styles::secondary)
                    .on_press(send(CommandLibraryMessage::AskDelete(command.id.clone()))),
            ]
            .spacing(spacing::SM)
            .into()
        };
        container(body.push(manage))
            .padding(spacing::MD)
            .width(Length::Fill)
            .style(styles::card)
            .into()
    }
}

/// A command in its own box with its buttons at the right.
fn command_box<'a>(command: String, buttons: Row<'a, Message>) -> Element<'a, Message> {
    container(
        row![
            text(command)
                .font(super::BOX_FONT)
                .size(font_size::BODY)
                .width(Length::Fill),
            buttons,
        ]
        .spacing(spacing::SM)
        .align_y(iced::Alignment::Center),
    )
    .padding(spacing::SM)
    .width(Length::Fill)
    .style(styles::field_box)
    .into()
}

/// A labelled row of choice buttons in the editor.
fn choice_row<'a>(label: String, buttons: Vec<Element<'a, Message>>) -> Row<'a, Message> {
    row![
        text(label).size(font_size::BODY).width(EDITOR_LABEL_WIDTH),
        Row::with_children(buttons).spacing(spacing::SM),
    ]
    .spacing(spacing::SM)
    .align_y(iced::Alignment::Center)
}

/// The editor of a command of the user.
fn editor_view(tab: TabId, editor: &Editor) -> Element<'_, Message> {
    let send = move |message| Message::Tool(tab, ToolMessage::CmdLib(message));
    let draft = &editor.draft;
    let field = |label: String, value: &str, which: Field| {
        row![
            text(label).size(font_size::BODY).width(EDITOR_LABEL_WIDTH),
            text_input("", value)
                .size(font_size::BODY)
                .padding(super::INPUT_PADDING)
                .style(styles::text_input)
                .on_input(move |typed| send(CommandLibraryMessage::Field(which, typed))),
        ]
        .spacing(spacing::SM)
        .align_y(iced::Alignment::Center)
    };
    let choice = |label: String, on: bool, message: CommandLibraryMessage| {
        button(text(label).size(font_size::BODY))
            .padding(super::COPY_PADDING)
            .style(if on {
                styles::primary
            } else {
                styles::secondary
            })
            .on_press(send(message))
            .into()
    };
    let mut form = column![
        text(if draft.id.is_some() {
            fl!("ui-tool-cmdlib-editor-edit")
        } else {
            fl!("ui-tool-cmdlib-editor-new")
        })
        .size(font_size::BODY_LARGE)
        .font(styles::SEMIBOLD),
        field(
            fl!("ui-tool-cmdlib-field-title"),
            &draft.title,
            Field::Title
        ),
        field(
            fl!("ui-tool-cmdlib-field-description"),
            &draft.description,
            Field::Description
        ),
        field(
            fl!("ui-tool-cmdlib-field-category"),
            &draft.category,
            Field::Category
        ),
        field(fl!("ui-tool-cmdlib-field-tags"), &draft.tags, Field::Tags),
        choice_row(
            fl!("ui-tool-cmdlib-field-platform"),
            UserPlatform::ALL
                .into_iter()
                .map(|platform| choice(
                    user_platform_name(platform),
                    draft.platform == platform,
                    CommandLibraryMessage::EditPlatform(platform)
                ))
                .collect()
        ),
        choice_row(
            fl!("ui-tool-cmdlib-field-level"),
            Criticality::ALL
                .into_iter()
                .map(|level| choice(
                    level_name(level),
                    draft.level == level,
                    CommandLibraryMessage::EditLevel(level)
                ))
                .collect()
        ),
    ]
    .spacing(spacing::SM);
    if draft.platform != UserPlatform::Linux {
        form = form.push(field(
            fl!("ui-tool-cmdlib-field-windows-pattern"),
            &draft.windows_pattern,
            Field::WindowsPattern,
        ));
    }
    if draft.platform != UserPlatform::Windows {
        form = form.push(field(
            fl!("ui-tool-cmdlib-field-linux-pattern"),
            &draft.linux_pattern,
            Field::LinuxPattern,
        ));
    }
    form = form.push(
        text(fl!("ui-tool-cmdlib-field-pattern-hint"))
            .size(font_size::CAPTION)
            .style(text::secondary),
    );
    if let Some(problem) = editor.problem {
        form = form.push(
            text(draft_error(problem))
                .size(font_size::BODY)
                .style(text::danger),
        );
    }
    form = form.push(
        row![
            button(text(fl!("ui-tool-cmdlib-save")).size(font_size::BODY))
                .padding(super::PRIMARY_PADDING)
                .style(styles::primary)
                .on_press(send(CommandLibraryMessage::Save)),
            button(text(fl!("ui-tool-cmdlib-cancel")).size(font_size::BODY))
                .padding(super::PRIMARY_PADDING)
                .style(styles::secondary)
                .on_press(send(CommandLibraryMessage::CancelEdit)),
        ]
        .spacing(spacing::SM),
    );
    container(form)
        .padding(spacing::MD)
        .width(Length::Fill)
        .style(styles::card)
        .into()
}

/// The title and description of snippet `id`, in the window's language.
fn snippet_texts(id: SnippetId) -> (String, String) {
    match id {
        SnippetId::LinuxListeningPorts => (
            fl!("ui-tool-cmdlib-snippet-listening-ports"),
            fl!("ui-tool-cmdlib-snippet-listening-ports-description"),
        ),
        SnippetId::LinuxKillPort => (
            fl!("ui-tool-cmdlib-snippet-kill-port"),
            fl!("ui-tool-cmdlib-snippet-kill-port-description"),
        ),
        SnippetId::LinuxJournalFollow => (
            fl!("ui-tool-cmdlib-snippet-journal-follow"),
            fl!("ui-tool-cmdlib-snippet-journal-follow-description"),
        ),
        SnippetId::LinuxCertExpiry => (
            fl!("ui-tool-cmdlib-snippet-cert-expiry"),
            fl!("ui-tool-cmdlib-snippet-cert-expiry-description"),
        ),
        SnippetId::WindowsTestConnection => (
            fl!("ui-tool-cmdlib-snippet-test-connection"),
            fl!("ui-tool-cmdlib-snippet-test-connection-description"),
        ),
        SnippetId::WindowsPortOwner => (
            fl!("ui-tool-cmdlib-snippet-port-owner"),
            fl!("ui-tool-cmdlib-snippet-port-owner-description"),
        ),
        SnippetId::WindowsRestartService => (
            fl!("ui-tool-cmdlib-snippet-restart-service"),
            fl!("ui-tool-cmdlib-snippet-restart-service-description"),
        ),
        SnippetId::WindowsFlushDns => (
            fl!("ui-tool-cmdlib-snippet-flush-dns"),
            fl!("ui-tool-cmdlib-snippet-flush-dns-description"),
        ),
        SnippetId::MtrProbe => (
            fl!("ui-tool-cmdlib-snippet-mtr"),
            fl!("ui-tool-cmdlib-snippet-mtr-description"),
        ),
        SnippetId::NmapServiceScan => (
            fl!("ui-tool-cmdlib-snippet-nmap"),
            fl!("ui-tool-cmdlib-snippet-nmap-description"),
        ),
    }
}

/// The name of `platform`.
fn platform_name(platform: SnippetPlatform) -> String {
    match platform {
        SnippetPlatform::Linux => fl!("ui-tool-cmdlib-platform-linux"),
        SnippetPlatform::Windows => fl!("ui-tool-cmdlib-platform-windows"),
        SnippetPlatform::CrossPlatform => fl!("ui-tool-cmdlib-platform-cross"),
    }
}

/// The name of the platforms of a command of the user.
fn user_platform_name(platform: UserPlatform) -> String {
    match platform {
        UserPlatform::Windows => fl!("ui-tool-cmdlib-platform-windows"),
        UserPlatform::Linux => fl!("ui-tool-cmdlib-platform-linux"),
        UserPlatform::Both => fl!("ui-tool-cmdlib-platform-both"),
    }
}

/// The name of the template of `platform`: the shell it is written for.
fn template_name(platform: TemplatePlatform) -> String {
    match platform {
        TemplatePlatform::Windows => fl!("ui-tool-cmdlib-template-windows"),
        TemplatePlatform::Linux => fl!("ui-tool-cmdlib-template-linux"),
    }
}

/// The name of `level`.
fn level_name(level: Criticality) -> String {
    match level {
        Criticality::Info => fl!("ui-tool-cmdlib-level-info"),
        Criticality::Run => fl!("ui-tool-cmdlib-level-run"),
        Criticality::Dangerous => fl!("ui-tool-cmdlib-level-dangerous"),
    }
}

/// The name of `category`.
fn category_name(category: SnippetCategory) -> String {
    match category {
        SnippetCategory::Network => fl!("ui-tool-cmdlib-category-network"),
        SnippetCategory::System => fl!("ui-tool-cmdlib-category-system"),
        SnippetCategory::Services => fl!("ui-tool-cmdlib-category-services"),
        SnippetCategory::Security => fl!("ui-tool-cmdlib-category-security"),
        SnippetCategory::Diagnostics => fl!("ui-tool-cmdlib-category-diagnostics"),
    }
}

/// The label of a value of kind `label`.
fn param_label(label: ParamLabel) -> String {
    match label {
        ParamLabel::Host => fl!("ui-tool-cmdlib-param-host"),
        ParamLabel::Port => fl!("ui-tool-cmdlib-param-port"),
        ParamLabel::LocalPort => fl!("ui-tool-cmdlib-param-local-port"),
        ParamLabel::PortRange => fl!("ui-tool-cmdlib-param-port-range"),
        ParamLabel::Service => fl!("ui-tool-cmdlib-param-service"),
    }
}

/// Why a draft was not kept, in the window's language.
fn draft_error(problem: DraftError) -> String {
    match problem {
        DraftError::TitleMissing => fl!("ui-tool-cmdlib-error-title-missing"),
        DraftError::TitleTooLong => fl!("ui-tool-cmdlib-error-title-long"),
        DraftError::CategoryMissing => fl!("ui-tool-cmdlib-error-category-missing"),
        DraftError::CategoryTooLong => fl!("ui-tool-cmdlib-error-category-long"),
        DraftError::DescriptionTooLong => fl!("ui-tool-cmdlib-error-description-long"),
        DraftError::PatternMissing => fl!("ui-tool-cmdlib-error-pattern-missing"),
        DraftError::PatternTooLong => fl!("ui-tool-cmdlib-error-pattern-long"),
        DraftError::PatternMultiline => fl!("ui-tool-cmdlib-error-pattern-multiline"),
        DraftError::TooManyParameters => fl!("ui-tool-cmdlib-error-too-many-parameters"),
        DraftError::TooManyTags => fl!("ui-tool-cmdlib-error-too-many-tags"),
        DraftError::Gone => fl!("ui-tool-cmdlib-error-gone"),
    }
}

/// Why a command of the user cannot be made from the values typed.
fn generate_error(error: &GenerateError) -> String {
    match error {
        GenerateError::EmptyPattern => fl!("ui-tool-cmdlib-error-empty-pattern"),
        GenerateError::TooManyParameters => fl!("ui-tool-cmdlib-error-too-many-parameters"),
        GenerateError::TooLong => fl!("ui-tool-cmdlib-error-command-long"),
        GenerateError::Values(values) => {
            values.iter().map(value_error).collect::<Vec<_>>().join(" ")
        }
    }
}

/// Why a value is refused, naming its parameter.
fn value_error(error: &ValueError) -> String {
    match error {
        ValueError::Required(label) => fl!("ui-tool-cmdlib-value-required", label = label.as_str()),
        ValueError::NotOffered(label) => {
            fl!("ui-tool-cmdlib-value-not-offered", label = label.as_str())
        }
        ValueError::NotInteger(label) => {
            fl!("ui-tool-cmdlib-value-not-integer", label = label.as_str())
        }
        ValueError::NotNumber(label) => {
            fl!("ui-tool-cmdlib-value-not-number", label = label.as_str())
        }
        ValueError::Hostname(label) => fl!("ui-tool-cmdlib-value-hostname", label = label.as_str()),
        ValueError::IpAddress(label) => fl!("ui-tool-cmdlib-value-ip", label = label.as_str()),
        ValueError::DriveLetter(label) => fl!("ui-tool-cmdlib-value-drive", label = label.as_str()),
        ValueError::Path(label) => fl!("ui-tool-cmdlib-value-path", label = label.as_str()),
        ValueError::TooLong(label) => fl!(
            "ui-tool-cmdlib-value-long",
            label = label.as_str(),
            max = user_commands::MAX_PARAMETER_LENGTH
        ),
        ValueError::Dangerous(label) => {
            fl!("ui-tool-cmdlib-value-dangerous", label = label.as_str())
        }
        ValueError::Control(label) => fl!("ui-tool-cmdlib-value-control", label = label.as_str()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Three tab identifiers, made by opening three tools in an application.
    fn tab_ids() -> [TabId; 3] {
        use heimdall_app::tools::{ToolContext, ToolId};
        use heimdall_app::{App, AppConfig, Message as AppMessage, ToolsMessage};
        let folder = tempfile::tempdir().expect("folder");
        let mut app = App::new(AppConfig {
            profiles_file: folder.path().join("profiles.toml"),
            known_hosts: folder.path().join("known_hosts"),
            legacy_dir: None,
            agent: heimdall_ssh::AgentSource::Disabled,
            initial_grid: heimdall_term::GridSize { cols: 80, rows: 24 },
            files_start: folder.path().to_owned(),
            system_credentials: heimdall_app::SystemCredentials::memory(),
        });
        for word in ["a", "b", "c"] {
            let _ = app.update(AppMessage::Tools(ToolsMessage::Open {
                tool: ToolId::Hash,
                title: word.to_owned(),
                context: ToolContext::with_argument(word),
            }));
        }
        [app.tabs[0].id, app.tabs[1].id, app.tabs[2].id]
    }

    fn target(tab: TabId, name: &str) -> SendTarget {
        SendTarget {
            tab,
            name: name.to_owned(),
        }
    }

    /// A pane keeping its commands in a folder of its own.
    fn pane(folder: &Path) -> CommandLibraryPane {
        CommandLibraryPane::new(Some(&folder.join("profiles.toml")))
    }

    fn type_into(pane: &mut CommandLibraryPane, field: Field, value: &str) {
        pane.update(CommandLibraryMessage::Field(field, value.to_owned()));
    }

    /// A user command "Ping" made through the editor; its identity.
    fn make_ping(pane: &mut CommandLibraryPane) -> String {
        pane.update(CommandLibraryMessage::New);
        pane.update(CommandLibraryMessage::EditPlatform(UserPlatform::Linux));
        type_into(pane, Field::Title, "Ping");
        type_into(pane, Field::Category, "Network");
        type_into(pane, Field::LinuxPattern, "ping -c 3 {host}");
        pane.update(CommandLibraryMessage::Save);
        assert!(pane.editor.is_none(), "kept");
        pane.library.commands()[0].id.clone()
    }

    #[test]
    fn a_snippet_copies_its_command_with_the_values_typed_by_its_own_button() {
        let mut pane = CommandLibraryPane::default();
        pane.update(CommandLibraryMessage::Param(
            SnippetId::MtrProbe,
            "host",
            "10.0.0.1".to_owned(),
        ));
        assert_eq!(
            pane.update(CommandLibraryMessage::Copy(CommandRef::Catalog(
                SnippetId::MtrProbe
            ))),
            Outcome::Copy(
                CopySlot::CmdLibCommand(SnippetId::MtrProbe),
                "mtr -rw -c 50 10.0.0.1".to_owned()
            )
        );
        assert_eq!(
            pane.update(CommandLibraryMessage::Copy(CommandRef::Catalog(
                SnippetId::WindowsFlushDns
            ))),
            Outcome::Copy(
                CopySlot::CmdLibCommand(SnippetId::WindowsFlushDns),
                "Clear-DnsClientCache".to_owned()
            )
        );
    }

    #[test]
    fn the_platform_and_the_search_filter_the_catalog() {
        let mut pane = CommandLibraryPane::default();
        assert_eq!(pane.shown().len(), BUILTIN_SNIPPETS.len());
        pane.update(CommandLibraryMessage::Platform(Some(
            SnippetPlatform::Windows,
        )));
        assert!(
            pane.shown()
                .iter()
                .all(|snippet| { snippet.platform != SnippetPlatform::Linux })
        );
        pane.update(CommandLibraryMessage::Search("dnsclient".to_owned()));
        let shown: Vec<SnippetId> = pane.shown().iter().map(|snippet| snippet.id).collect();
        assert_eq!(shown, [SnippetId::WindowsFlushDns]);
    }

    #[test]
    fn a_send_asks_for_the_command_with_its_values_and_never_without_a_terminal() {
        let mut pane = CommandLibraryPane::default();
        pane.update(CommandLibraryMessage::Param(
            SnippetId::MtrProbe,
            "host",
            "10.0.0.1".to_owned(),
        ));
        let mtr = CommandRef::Catalog(SnippetId::MtrProbe);
        assert_eq!(
            pane.update(CommandLibraryMessage::Send(mtr.clone())),
            Outcome::None,
            "no terminal: nothing asked"
        );
        let [first, second, _] = tab_ids();
        pane.set_targets(vec![target(first, "A"), target(second, "B")]);
        let Outcome::Send {
            target, command, ..
        } = pane.update(CommandLibraryMessage::Send(mtr.clone()))
        else {
            panic!("asked");
        };
        assert_eq!(
            (target, command.as_str()),
            (first, "mtr -rw -c 50 10.0.0.1"),
            "the first terminal until one is chosen"
        );
        pane.update(CommandLibraryMessage::Target(second));
        assert!(matches!(
            pane.update(CommandLibraryMessage::Send(mtr.clone())),
            Outcome::Send { target, .. } if target == second
        ));
        // The terminal chosen is closed: the first again.
        pane.set_targets(vec![target_of(first)]);
        assert!(matches!(
            pane.update(CommandLibraryMessage::Send(mtr)),
            Outcome::Send { target, .. } if target == first
        ));
    }

    fn target_of(tab: TabId) -> SendTarget {
        target(tab, "A")
    }

    #[test]
    fn a_dangerous_command_is_marked_so_to_the_question() {
        let mut pane = CommandLibraryPane::default();
        pane.set_targets(vec![target(tab_ids()[0], "A")]);
        assert!(matches!(
            pane.update(CommandLibraryMessage::Send(CommandRef::Catalog(
                SnippetId::LinuxKillPort
            ))),
            Outcome::Send {
                dangerous: true,
                ..
            }
        ));
        assert!(matches!(
            pane.update(CommandLibraryMessage::Send(CommandRef::Catalog(
                SnippetId::MtrProbe
            ))),
            Outcome::Send {
                dangerous: false,
                ..
            }
        ));
    }

    #[test]
    fn the_user_makes_changes_and_deletes_commands_kept_in_their_own_file() {
        let folder = tempfile::tempdir().expect("folder");
        let mut pane = pane(folder.path());
        let id = make_ping(&mut pane);
        assert!(folder.path().join(LIBRARY_FILE).is_file());

        // Another pane over the same folder sees it: kept, apart from the catalog.
        let again = self::pane(folder.path());
        assert_eq!(again.library.commands().len(), 1);
        assert_eq!(again.shown().len(), BUILTIN_SNIPPETS.len());

        pane.update(CommandLibraryMessage::Edit(id.clone()));
        type_into(&mut pane, Field::Title, "Ping three times");
        pane.update(CommandLibraryMessage::Save);
        assert_eq!(pane.library.commands()[0].title, "Ping three times");
        assert_eq!(
            self::pane(folder.path()).library.commands()[0].title,
            "Ping three times"
        );

        pane.update(CommandLibraryMessage::AskDelete(id.clone()));
        assert_eq!(pane.library.commands().len(), 1, "asked first");
        pane.update(CommandLibraryMessage::CancelDelete);
        assert_eq!(pane.library.commands().len(), 1, "kept");
        pane.update(CommandLibraryMessage::AskDelete(id.clone()));
        pane.update(CommandLibraryMessage::ConfirmDelete(id));
        assert!(pane.library.commands().is_empty());
        assert!(self::pane(folder.path()).library.commands().is_empty());
    }

    #[test]
    fn a_draft_not_kept_stays_in_the_editor_with_its_reason() {
        let folder = tempfile::tempdir().expect("folder");
        let mut pane = pane(folder.path());
        pane.update(CommandLibraryMessage::New);
        pane.update(CommandLibraryMessage::Save);
        assert_eq!(
            pane.editor.as_ref().and_then(|editor| editor.problem),
            Some(DraftError::TitleMissing)
        );
        pane.update(CommandLibraryMessage::CancelEdit);
        assert!(pane.editor.is_none());
        assert!(pane.library.commands().is_empty());
    }

    #[test]
    fn a_user_command_is_copied_and_sent_with_its_values_quoted() {
        let folder = tempfile::tempdir().expect("folder");
        let mut pane = pane(folder.path());
        let id = make_ping(&mut pane);
        let ping = CommandRef::User(id.clone());
        pane.update(CommandLibraryMessage::UserParam(
            id.clone(),
            "host".to_owned(),
            "web.lab".to_owned(),
        ));
        assert_eq!(
            pane.update(CommandLibraryMessage::Copy(ping.clone())),
            Outcome::Copy(
                CopySlot::CmdLibUserCommand(0),
                "ping -c 3 'web.lab'".to_owned()
            )
        );
        let terminal = tab_ids()[0];
        pane.set_targets(vec![target(terminal, "A")]);
        assert_eq!(
            pane.update(CommandLibraryMessage::Send(ping.clone())),
            Outcome::Send {
                target: terminal,
                command: "ping -c 3 'web.lab'".to_owned(),
                dangerous: false,
            }
        );
        // A value that would run more is refused: nothing to copy, nothing to send.
        pane.update(CommandLibraryMessage::UserParam(
            id,
            "host".to_owned(),
            "web.lab\nrm -rf x".to_owned(),
        ));
        assert_eq!(
            pane.update(CommandLibraryMessage::Copy(ping.clone())),
            Outcome::None
        );
        assert_eq!(
            pane.update(CommandLibraryMessage::Send(ping)),
            Outcome::None
        );
    }

    #[test]
    fn a_file_that_cannot_be_read_is_left_alone_and_nothing_is_changed() {
        let folder = tempfile::tempdir().expect("folder");
        let path = folder.path().join(LIBRARY_FILE);
        std::fs::write(&path, "not json").expect("written");
        let mut pane = pane(folder.path());
        assert_eq!(pane.failure, Some(Failure::Unreadable));
        pane.update(CommandLibraryMessage::New);
        assert!(pane.editor.is_none(), "no editor over a file not read");
        assert_eq!(std::fs::read_to_string(&path).expect("read"), "not json");
    }
}
