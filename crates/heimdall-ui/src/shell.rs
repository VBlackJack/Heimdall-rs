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

//! The window: profiles, tabs, questions and dialogs, and the runtime that turns the
//! application core's effects into iced tasks.
//!
//! Every decision stays in [`heimdall_app::App`]; this layer only draws its state, holds
//! what the user is typing into a question, and runs effects.

use std::collections::HashMap;
use std::fmt;
use std::path::PathBuf;

use heimdall_app::files::{
    Direction, FilesKey, file_operation, list_local, list_remote, transfer_events,
};
use heimdall_app::{
    Answer, AnswerRegistry, App, AppConfig, Dialog, Effect, FilesMessage, Message as AppMessage,
    NameAction, Phase, Prompt, QuestionId, QuestionKind, Tab, TabId, UiError, connection_events,
    server_text,
};
use heimdall_core::paths::{self, KNOWN_HOSTS_FILE_NAME, PROFILES_FILE_NAME};
use heimdall_core::profile::{SshProfile, display_address};
use heimdall_ssh::{AgentSource, Secret};
use heimdall_term::GridSize;
use iced::futures::{StreamExt as _, stream};
use iced::keyboard::key::Named;
use iced::task::Handle;
use iced::widget::scrollable::RelativeOffset;
use iced::widget::{
    Column, button, center, column, container, opaque, operation, row, scrollable, stack, text,
    text_input,
};
use iced::{Color, Element, Length, Subscription, Task, Theme, event, keyboard, window};
use zeroize::Zeroizing;

use crate::files_view;
use crate::i18n::fl;
use crate::terminal_view::TerminalView;
use crate::terminal_view::keys::{WindowShortcut, window_shortcut};
use crate::texts;

/// Grid of a tab before its first layout.
const INITIAL_GRID: GridSize = GridSize { cols: 80, rows: 24 };

/// Width of the profile list, in logical pixels.
const SIDEBAR_WIDTH: f32 = 260.0;

/// Gap between stacked elements, in logical pixels.
const SPACING: f32 = 8.0;

/// Padding inside panels, in logical pixels.
const PADDING: f32 = 12.0;

/// Space between the terminal and the panels around it, in logical pixels.
const TERMINAL_MARGIN: f32 = 6.0;

/// Width of a question or dialog card, in logical pixels.
const CARD_WIDTH: f32 = 520.0;

/// Size of headings, in logical pixels.
const HEADING_SIZE: f32 = 20.0;

/// Size of secondary text, in logical pixels.
const SMALL_SIZE: f32 = 12.0;

/// Tallest the list of skipped profiles grows before it scrolls, in logical pixels.
const SKIPPED_LIST_HEIGHT: f32 = 200.0;

/// Longest tab title shown, in characters.
const MAX_TAB_TITLE_CHARS: usize = 32;

/// Marks a cut title; three ASCII dots, as everywhere in the project.
const ELLIPSIS: &str = "...";

/// Opacity of the veil behind a dialog.
const VEIL_ALPHA: f32 = 0.6;

/// Window events and the window's shortcuts.
fn window_event(event: iced::Event, status: event::Status, _window: window::Id) -> Option<Message> {
    match event {
        iced::Event::Window(window::Event::CloseRequested) => {
            Some(Message::App(AppMessage::WindowCloseRequested))
        }
        iced::Event::Window(window::Event::Focused) => {
            Some(Message::App(AppMessage::WindowFocus(true)))
        }
        iced::Event::Window(window::Event::Unfocused) => {
            Some(Message::App(AppMessage::WindowFocus(false)))
        }
        iced::Event::Keyboard(keyboard::Event::KeyPressed {
            key: keyboard::Key::Named(named @ (Named::Enter | Named::Escape)),
            repeat: false,
            ..
        }) if status == event::Status::Ignored => Some(Message::DialogKey {
            confirm: named == Named::Enter,
        }),
        iced::Event::Keyboard(keyboard::Event::KeyPressed {
            key,
            physical_key,
            modifiers,
            repeat,
            ..
        }) if status == event::Status::Ignored => {
            match window_shortcut(&key, physical_key, modifiers) {
                Some(WindowShortcut::CloseTab) if repeat => None,
                Some(shortcut) => Some(Message::Shortcut(shortcut)),
                None => files_view::files_key(&key, modifiers).map(Message::FilesKey),
            }
        }
        _ => None,
    }
}

/// What the window reacts to.
#[derive(Clone)]
pub enum Message {
    /// A message for the application core.
    App(AppMessage),
    /// The user edited field `index` of a question.
    Field {
        /// Question.
        question: QuestionId,
        /// Field.
        index: usize,
        /// New content.
        value: String,
    },
    /// Move to field `index` of a question.
    FocusField {
        /// Question.
        question: QuestionId,
        /// Field.
        index: usize,
    },
    /// Answer the question shown in `tab`.
    Submit(TabId),
    /// Decline the question shown in `tab`.
    Decline(TabId),
    /// A window shortcut.
    Shortcut(WindowShortcut),
    /// Enter (`confirm`) or Escape, uncaptured by any widget: answers the open dialog.
    DialogKey {
        /// Enter rather than Escape.
        confirm: bool,
    },
    /// A key for the Files tab shown, uncaptured by any widget.
    FilesKey(FilesKey),
}

impl fmt::Debug for Message {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // A field holds what the user types into a question: a password, a passphrase.
        match self {
            Self::App(message) => write!(f, "App({message:?})"),
            Self::Field {
                question, index, ..
            } => write!(f, "Field({}, {index}, ..)", question.value()),
            Self::FocusField { question, index } => {
                write!(f, "FocusField({}, {index})", question.value())
            }
            Self::Submit(tab) => write!(f, "Submit({})", tab.value()),
            Self::Decline(tab) => write!(f, "Decline({})", tab.value()),
            Self::Shortcut(shortcut) => write!(f, "Shortcut({shortcut:?})"),
            Self::DialogKey { confirm } => write!(f, "DialogKey({confirm})"),
            Self::FilesKey(key) => write!(f, "FilesKey({key:?})"),
        }
    }
}

/// Number of fields a question shows.
fn field_count(kind: &QuestionKind) -> usize {
    match kind {
        QuestionKind::KeyboardInteractive(question) => question.prompts.len(),
        QuestionKind::Username(_) | QuestionKind::Password(_) | QuestionKind::Passphrase(_) => 1,
    }
}

/// The answer to `kind` made of what was typed; a missing field is empty.
fn answer(kind: &QuestionKind, mut typed: Vec<Zeroizing<String>>) -> Answer {
    typed.resize_with(field_count(kind), Zeroizing::default);
    let secret = |text: &mut Zeroizing<String>| Secret::new(std::mem::take(&mut **text));
    match kind {
        QuestionKind::Username(_) => Answer::Text(std::mem::take(&mut *typed[0])),
        QuestionKind::Password(_) | QuestionKind::Passphrase(_) => {
            Answer::Secret(secret(&mut typed[0]))
        }
        QuestionKind::KeyboardInteractive(_) => {
            Answer::Secrets(typed.iter_mut().map(secret).collect())
        }
    }
}

/// Widget identifier of the name field of a dialog.
fn name_field_id() -> iced::widget::Id {
    iced::widget::Id::new("dialog-name")
}

/// Widget identifier of a question field.
fn field_id(question: QuestionId, index: usize) -> iced::widget::Id {
    iced::widget::Id::from(format!("question-{}-{index}", question.value()))
}

/// `user@host:port`, or `host:port` without a user.
fn target(host: &str, port: u16, user: Option<&str>) -> String {
    let address = display_address(host, port);
    match user {
        Some(user) if !user.is_empty() => format!("{user}@{address}"),
        _ => address,
    }
}

/// A tab title cut to [`MAX_TAB_TITLE_CHARS`], so one long server title cannot take the
/// whole tab bar.
fn tab_label(title: &str) -> String {
    if title.chars().count() <= MAX_TAB_TITLE_CHARS {
        return title.to_owned();
    }
    let kept: String = title
        .chars()
        .take(MAX_TAB_TITLE_CHARS - ELLIPSIS.len())
        .collect();
    format!("{kept}{ELLIPSIS}")
}

/// Where the application keeps its files; the working directory when the platform has
/// no home.
fn config() -> AppConfig {
    AppConfig {
        profiles_file: paths::profiles_file().unwrap_or_else(|| PathBuf::from(PROFILES_FILE_NAME)),
        known_hosts: paths::known_hosts_file()
            .unwrap_or_else(|| PathBuf::from(KNOWN_HOSTS_FILE_NAME)),
        legacy_dir: paths::legacy_data_dir(),
        agent: AgentSource::Auto,
        initial_grid: INITIAL_GRID,
        files_start: paths::home_dir().unwrap_or_else(|| PathBuf::from(".")),
    }
}

/// The window's state.
pub struct Shell {
    app: App,
    registry: AnswerRegistry,
    /// Running connection attempts; dropping a handle aborts its task.
    connections: HashMap<TabId, Handle>,
    /// What is typed into each open question, zeroed when dropped. iced keeps its own
    /// transient copies of a field's text, which this cannot reach.
    drafts: HashMap<QuestionId, Vec<Zeroizing<String>>>,
    /// The question whose first field was last given focus.
    focused: Option<QuestionId>,
    /// Whether the name field of the open dialog was given focus.
    name_focused: bool,
}

impl Shell {
    /// The window, with the profiles on disk.
    #[must_use]
    pub fn new() -> Self {
        Self::with_config(config())
    }

    /// The window over `config`.
    #[must_use]
    pub fn with_config(config: AppConfig) -> Self {
        Self::with_app(App::new(config))
    }

    /// The window over an application core already in some state.
    #[must_use]
    pub fn with_app(app: App) -> Self {
        Self {
            app,
            registry: AnswerRegistry::default(),
            connections: HashMap::new(),
            drafts: HashMap::new(),
            focused: None,
            name_focused: false,
        }
    }

    /// The application core.
    #[must_use]
    pub fn app(&self) -> &App {
        &self.app
    }

    /// Whether something typed into `question` is held.
    #[must_use]
    pub fn holds_draft(&self, question: QuestionId) -> bool {
        self.drafts.contains_key(&question)
    }

    /// Window title.
    #[must_use]
    pub fn title(&self) -> String {
        match self.app.active_tab() {
            Some(tab) => fl!("ui-window-title-tab", tab = tab.title.as_str()),
            None => fl!("ui-window-title"),
        }
    }

    /// Theme: the terminal palette is Dracula, so is the window.
    #[must_use]
    pub fn theme(&self) -> Theme {
        Theme::Dracula
    }

    /// Window events and shortcuts.
    pub fn subscription(&self) -> Subscription<Message> {
        event::listen_with(window_event)
    }

    /// Applies a message.
    pub fn update(&mut self, message: Message) -> Task<Message> {
        let reveal = matches!(message, Message::FilesKey(_) | Message::DialogKey { .. });
        let effects = match message {
            Message::App(message) => self.app.update(message),
            Message::Field {
                question,
                index,
                value,
            } => {
                self.edit(question, index, value);
                return Task::none();
            }
            Message::FocusField { question, index } => {
                return operation::focus(field_id(question, index));
            }
            Message::Submit(tab) => self.reply(tab, true),
            Message::Decline(tab) => self.reply(tab, false),
            Message::Shortcut(shortcut) => self.shortcut(shortcut),
            Message::DialogKey { confirm } => self.dialog_key(confirm),
            Message::FilesKey(key) => self.files_key(key),
        };
        let mut tasks: Vec<Task<Message>> =
            effects.into_iter().map(|effect| self.run(effect)).collect();
        self.forget_finished();
        tasks.push(self.focus_question());
        tasks.push(self.focus_name());
        if reveal {
            tasks.push(self.reveal_selection());
        }
        Task::batch(tasks)
    }

    fn prompt(&self, question: QuestionId) -> Option<&Prompt> {
        self.app
            .tabs
            .iter()
            .flat_map(|tab| tab.prompts.iter())
            .find(|prompt| prompt.question == question)
    }

    fn edit(&mut self, question: QuestionId, index: usize, value: String) {
        let Some(count) = self
            .prompt(question)
            .map(|prompt| field_count(&prompt.kind))
        else {
            return;
        };
        if index >= count {
            return;
        }
        let fields = self.drafts.entry(question).or_default();
        fields.resize_with(count, Zeroizing::default);
        fields[index] = Zeroizing::new(value);
    }

    fn reply(&mut self, tab: TabId, accept: bool) -> Vec<Effect> {
        let Some(prompt) = self.app.tab(tab).and_then(|found| found.prompts.front()) else {
            return Vec::new();
        };
        let question = prompt.question;
        let typed = self.drafts.remove(&question).unwrap_or_default();
        let answer = accept.then(|| answer(&prompt.kind, typed));
        self.app.update(AppMessage::Answer {
            tab,
            question,
            answer,
        })
    }

    fn shortcut(&mut self, shortcut: WindowShortcut) -> Vec<Effect> {
        let Some(active) = self.app.active else {
            return Vec::new();
        };
        let count = self.app.tabs.len();
        let index = self.app.tabs.iter().position(|tab| tab.id == active);
        let message = match (shortcut, index) {
            (WindowShortcut::CloseTab, _) => AppMessage::RequestCloseTab(active),
            (WindowShortcut::NextTab, Some(index)) => {
                AppMessage::SelectTab(self.app.tabs[(index + 1) % count].id)
            }
            (WindowShortcut::PreviousTab, Some(index)) => {
                AppMessage::SelectTab(self.app.tabs[(index + count - 1) % count].id)
            }
            (_, None) => return Vec::new(),
        };
        self.app.update(message)
    }

    /// Enter confirms the open dialog, Escape dismisses it. Without a dialog, Enter opens
    /// the selection of a Files tab, and the core ignores the rest.
    fn dialog_key(&mut self, confirm: bool) -> Vec<Effect> {
        if confirm && self.app.dialog.is_none() {
            return self.files_key(FilesKey::Open);
        }
        self.app.update(if confirm {
            AppMessage::ConfirmDialog
        } else {
            AppMessage::DismissDialog
        })
    }

    /// Sends `key` to the tab shown; the core ignores it unless that is a Files tab.
    fn files_key(&mut self, key: FilesKey) -> Vec<Effect> {
        let Some(tab) = self.app.active else {
            return Vec::new();
        };
        self.app
            .update(AppMessage::Files(FilesMessage::Key { tab, key }))
    }

    /// Scrolls the focused list of the Files tab shown so its selection is in view. The
    /// list is snapped to the selection's share of its length, which keeps a row of equal
    /// height inside the viewport wherever it is.
    fn reveal_selection(&self) -> Task<Message> {
        let Some(files) = self.app.active_tab().and_then(|tab| tab.files.as_deref()) else {
            return Task::none();
        };
        let (Some(index), count) = files.focused() else {
            return Task::none();
        };
        let Some(last) = count.checked_sub(1).filter(|last| *last > 0) else {
            return Task::none();
        };
        #[allow(
            clippy::cast_precision_loss,
            reason = "a share of a listing, far below the 2^24 entries where f32 loses units"
        )]
        let share = index as f32 / last as f32;
        operation::snap_to(
            files_view::list_id(files.focus),
            RelativeOffset {
                x: None,
                y: Some(share),
            },
        )
    }

    /// Drops the tasks of closed tabs and the drafts of questions no longer asked.
    fn forget_finished(&mut self) {
        let app = &self.app;
        self.connections.retain(|tab, _| app.tab(*tab).is_some());
        self.drafts.retain(|question, _| {
            app.tabs.iter().any(|tab| {
                tab.prompts
                    .iter()
                    .any(|prompt| prompt.question == *question)
            })
        });
    }

    /// Gives focus to the first field of the question shown, once per question.
    fn focus_question(&mut self) -> Task<Message> {
        let shown = self
            .app
            .active_tab()
            .and_then(|tab| tab.prompts.front())
            .map(|prompt| prompt.question);
        if shown == self.focused {
            return Task::none();
        }
        self.focused = shown;
        shown.map_or_else(Task::none, |question| {
            operation::focus(field_id(question, 0))
        })
    }

    /// Gives focus to the name field when a dialog asking for a name opens.
    fn focus_name(&mut self) -> Task<Message> {
        let asking = matches!(self.app.dialog, Some(Dialog::AskName { .. }));
        let opened = asking && !self.name_focused;
        self.name_focused = asking;
        if opened {
            operation::focus(name_field_id())
        } else {
            Task::none()
        }
    }

    /// Turns an effect into a task.
    fn run(&mut self, effect: Effect) -> Task<Message> {
        match effect {
            Effect::Connect {
                tab,
                attempt,
                request,
            } => {
                let registry = self.registry.clone();
                // Started inside the task: spawning needs the runtime, which `update` is not in.
                let events =
                    stream::once(async move { connection_events(*request, registry) }).flatten();
                let (task, handle) = Task::stream(events)
                    .map(move |event| {
                        Message::App(AppMessage::Connection {
                            tab,
                            attempt,
                            event,
                        })
                    })
                    .abortable();
                self.connections.insert(tab, handle.abort_on_drop());
                task
            }
            Effect::Answer { question, answer } => {
                if !self.registry.answer(question, answer) {
                    log::debug!("question {} was no longer waiting", question.value());
                }
                Task::none()
            }
            Effect::ListRemote { tab, client, path } => {
                Task::perform(list_remote(client, path), move |result| {
                    Message::App(AppMessage::Files(FilesMessage::RemoteListed {
                        tab,
                        result,
                    }))
                })
            }
            Effect::ListLocal { tab, path } => Task::perform(list_local(path), move |result| {
                Message::App(AppMessage::Files(FilesMessage::LocalListed { tab, result }))
            }),
            Effect::Transfer { tab, id, request } => {
                // Started inside the task, like a connection: spawning needs the runtime.
                let events = stream::once(async move { transfer_events(*request) }).flatten();
                Task::stream(events).map(move |event| {
                    Message::App(AppMessage::Files(FilesMessage::TransferEvent {
                        tab,
                        id,
                        event,
                    }))
                })
            }
            Effect::FileOperation {
                tab,
                side,
                operation,
            } => Task::perform(file_operation(*operation), move |result| {
                Message::App(AppMessage::Files(FilesMessage::OperationDone {
                    tab,
                    side,
                    result,
                }))
            }),
            Effect::WriteClipboard(content) => iced::clipboard::write(content),
            Effect::ReadClipboard { tab } => iced::clipboard::read()
                .map(move |text| Message::App(AppMessage::ClipboardText { tab, text })),
            Effect::WakeAt {
                tab,
                generation,
                deadline,
            } => Task::perform(
                tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)),
                move |()| Message::App(AppMessage::SyncDeadline { tab, generation }),
            ),
            Effect::Exit => iced::exit(),
        }
    }

    /// Draws the window.
    #[must_use]
    pub fn view(&self) -> Element<'_, Message> {
        let body = row![
            self.sidebar(),
            column![self.tab_bar(), self.content()]
                .width(Length::Fill)
                .height(Length::Fill)
        ];
        match &self.app.dialog {
            Some(dialog) => stack![
                body,
                opaque(center(card(dialog_view(dialog))).style(|_theme: &Theme| {
                    container::Style {
                        background: Some(
                            Color {
                                a: VEIL_ALPHA,
                                ..Color::BLACK
                            }
                            .into(),
                        ),
                        ..container::Style::default()
                    }
                }))
            ]
            .into(),
            None => body.into(),
        }
    }

    fn sidebar(&self) -> Element<'_, Message> {
        let mut list = Column::new()
            .spacing(SPACING)
            .padding(PADDING)
            .push(text(fl!("ui-sidebar-title")).size(HEADING_SIZE));
        if self.app.can_import() {
            list = list.push(
                button(text(fl!("ui-sidebar-import-button")))
                    .on_press(Message::App(AppMessage::ImportLegacy))
                    .style(button::secondary),
            );
        }
        let mut profiles: Vec<&SshProfile> = self.app.profiles().iter().collect();
        if profiles.is_empty() {
            list = list.push(text(fl!("ui-sidebar-empty")));
        }
        // Named groups first, alphabetically; profiles without a group last.
        profiles.sort_by(|a, b| {
            (a.group.is_none(), a.group.as_deref(), &a.name).cmp(&(
                b.group.is_none(),
                b.group.as_deref(),
                &b.name,
            ))
        });
        let mut group: Option<Option<&str>> = None;
        for profile in profiles {
            let current = profile.group.as_deref();
            if group != Some(current) {
                group = Some(current);
                let label = current.map_or_else(|| fl!("ui-sidebar-group-none"), str::to_owned);
                list = list.push(text(label).size(SMALL_SIZE));
            }
            list = list.push(
                row![
                    button(column![
                        text(profile.name.as_str()),
                        text(target(
                            &profile.host,
                            profile.port,
                            profile.username.as_deref()
                        ))
                        .size(SMALL_SIZE)
                    ])
                    .width(Length::Fill)
                    .style(button::text)
                    .on_press(Message::App(AppMessage::OpenProfile(profile.id.clone()))),
                    button(text(fl!("ui-sidebar-files-button")).size(SMALL_SIZE))
                        .style(button::secondary)
                        .on_press(Message::App(AppMessage::OpenFiles(profile.id.clone()))),
                ]
                .spacing(SPACING / 2.0)
                .align_y(iced::Alignment::Center),
            );
        }
        container(scrollable(list))
            .width(SIDEBAR_WIDTH)
            .height(Length::Fill)
            .style(container::rounded_box)
            .into()
    }

    fn tab_bar(&self) -> Element<'_, Message> {
        let mut tabs = row![].spacing(SPACING).padding(PADDING);
        for tab in &self.app.tabs {
            let active = self.app.active == Some(tab.id);
            let title = if tab.files.is_some() {
                fl!("ui-tab-files-title", name = tab_label(&tab.title))
            } else {
                tab_label(&tab.title)
            };
            let mut label = row![text(title)].spacing(SPACING);
            if tab.bell && !active {
                label = label.push(text(fl!("ui-tab-bell-badge")).size(SMALL_SIZE));
            }
            tabs = tabs.push(
                row![
                    button(label)
                        .style(if active {
                            button::primary
                        } else {
                            button::secondary
                        })
                        .on_press(Message::App(AppMessage::SelectTab(tab.id))),
                    button(text(fl!("ui-tab-close-button")).size(SMALL_SIZE))
                        .style(button::text)
                        .on_press(Message::App(AppMessage::RequestCloseTab(tab.id))),
                ]
                .align_y(iced::Alignment::Center),
            );
        }
        tabs.wrap().into()
    }

    fn content(&self) -> Element<'_, Message> {
        let Some(tab) = self.app.active_tab() else {
            return center(
                column![
                    text(fl!("ui-home-welcome")).size(HEADING_SIZE),
                    text(fl!("ui-home-hint")),
                ]
                .spacing(SPACING),
            )
            .into();
        };
        if let Some(prompt) = tab.prompts.front() {
            return center(card(self.question(tab, prompt))).into();
        }
        let close = || {
            button(text(fl!("ui-session-close-button")))
                .on_press(Message::App(AppMessage::RequestCloseTab(tab.id)))
        };
        match &tab.phase {
            Phase::Connecting => center(card(
                column![
                    text(fl!(
                        "ui-connect-progress",
                        target = target(
                            &tab.profile.host,
                            tab.profile.port,
                            tab.profile.username.as_deref()
                        )
                    )),
                    button(text(fl!("ui-connect-cancel-button")))
                        .style(button::secondary)
                        .on_press(Message::App(AppMessage::RequestCloseTab(tab.id))),
                ]
                .spacing(SPACING),
            ))
            .into(),
            Phase::HostKey {
                host,
                port,
                fingerprint,
            } => host_key_card(tab.id, host, *port, fingerprint),
            Phase::Connected => match tab.files.as_deref() {
                Some(pane) => crate::files_view::view(tab.id, pane),
                None => terminal(tab, self.app.dialog.is_none()),
            },
            Phase::Closed { exit_status } => {
                let status = exit_status.map_or_else(
                    || fl!("ui-session-closed"),
                    |status| fl!("ui-session-closed-status", status = status.to_string()),
                );
                column![
                    terminal(tab, self.app.dialog.is_none()),
                    row![text(status), close()]
                        .spacing(SPACING)
                        .padding(PADDING)
                        .align_y(iced::Alignment::Center),
                ]
                .into()
            }
            Phase::Failed(UiError::Cancelled) => center(card(
                column![text(fl!("ui-session-cancelled")), close()].spacing(SPACING),
            ))
            .into(),
            Phase::Failed(error) => center(card(
                column![
                    text(fl!("ui-session-failed-title")).size(HEADING_SIZE),
                    text(texts::error(error)),
                    close(),
                ]
                .spacing(SPACING),
            ))
            .into(),
        }
    }

    fn field(
        &self,
        tab: TabId,
        question: QuestionId,
        index: usize,
        last: bool,
        secure: bool,
    ) -> Element<'_, Message> {
        let value = self
            .drafts
            .get(&question)
            .and_then(|fields| fields.get(index))
            .map_or("", |typed| typed.as_str());
        text_input("", value)
            .id(field_id(question, index))
            .secure(secure)
            .on_input(move |value| Message::Field {
                question,
                index,
                value,
            })
            .on_submit(if last {
                Message::Submit(tab)
            } else {
                Message::FocusField {
                    question,
                    index: index + 1,
                }
            })
            .into()
    }

    fn question<'a>(&'a self, tab: &'a Tab, prompt: &'a Prompt) -> Element<'a, Message> {
        let id = prompt.question;
        let profile = &tab.profile;
        let mut form = Column::new().spacing(SPACING);
        match &prompt.kind {
            QuestionKind::Username(asked) => {
                form = form
                    .push(text(fl!(
                        "ui-prompt-username-title",
                        target = target(&asked.host, asked.port, None)
                    )))
                    .push(self.field(tab.id, id, 0, true, false));
            }
            QuestionKind::Password(asked) => {
                form = form.push(text(fl!(
                    "ui-prompt-password-title",
                    user = asked.username.as_str(),
                    target = target(&asked.host, asked.port, None)
                )));
                if asked.attempt > 1 {
                    form = form.push(text(fl!("ui-prompt-password-retry")));
                }
                form = form.push(self.field(tab.id, id, 0, true, true));
            }
            QuestionKind::Passphrase(asked) => {
                form = form.push(text(fl!(
                    "ui-prompt-passphrase-title",
                    path = asked.key_path.display().to_string()
                )));
                if asked.attempt > 1 {
                    form = form.push(text(fl!("ui-prompt-passphrase-retry")));
                }
                form = form.push(self.field(tab.id, id, 0, true, true));
            }
            QuestionKind::KeyboardInteractive(asked) => {
                form = form.push(text(fl!(
                    "ui-prompt-interactive-title",
                    user = asked.username.as_str(),
                    host = profile.host.as_str()
                )));
                // Server words are labelled as such, so they cannot pass for Heimdall's.
                for said in [&asked.name, &asked.instructions] {
                    if !said.is_empty() {
                        form = form.push(
                            text(fl!("ui-prompt-server-text", text = said.as_str()))
                                .size(SMALL_SIZE),
                        );
                    }
                }
                let count = asked.prompts.len();
                for (index, line) in asked.prompts.iter().enumerate() {
                    form = form.push(text(line.text.as_str())).push(self.field(
                        tab.id,
                        id,
                        index,
                        index + 1 == count,
                        !line.echo,
                    ));
                }
            }
        }
        form.push(
            row![
                button(text(fl!("ui-prompt-cancel-button")))
                    .style(button::secondary)
                    .on_press(Message::Decline(tab.id)),
                button(text(fl!("ui-prompt-submit-button"))).on_press(Message::Submit(tab.id)),
            ]
            .spacing(SPACING),
        )
        .into()
    }
}

impl Default for Shell {
    fn default() -> Self {
        Self::new()
    }
}

fn terminal(tab: &Tab, interactive: bool) -> Element<'_, Message> {
    container(TerminalView::new(&tab.terminal, tab.id, Message::App).interactive(interactive))
        .padding(TERMINAL_MARGIN)
        .into()
}

/// The question about an unknown server key.
fn host_key_card<'a>(
    tab: TabId,
    host: &'a str,
    port: u16,
    fingerprint: &'a str,
) -> Element<'a, Message> {
    center(card(
        column![
            text(fl!("ui-hostkey-title")).size(HEADING_SIZE),
            text(fl!("ui-hostkey-body", host = host, port = port.to_string())),
            text(fl!("ui-hostkey-fingerprint", fingerprint = fingerprint))
                .font(iced::Font::MONOSPACE),
            row![
                button(text(fl!("ui-hostkey-reject-button")))
                    .style(button::secondary)
                    .on_press(Message::App(AppMessage::HostKeyDecision {
                        tab,
                        accept: false
                    })),
                button(text(fl!("ui-hostkey-accept-button"))).on_press(Message::App(
                    AppMessage::HostKeyDecision { tab, accept: true }
                )),
            ]
            .spacing(SPACING),
        ]
        .spacing(SPACING),
    ))
    .into()
}

fn card<'a>(content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    container(content)
        .padding(PADDING)
        .max_width(CARD_WIDTH)
        .style(container::bordered_box)
        .into()
}

/// The report of an import: counts, and the profiles left out with their reason.
fn import_report<'a>(
    summary: &'a heimdall_app::ImportSummary,
    ok: iced::widget::Button<'a, Message>,
) -> Element<'a, Message> {
    let mut content = column![
        text(fl!("ui-dialog-import-title")).size(HEADING_SIZE),
        text(fl!(
            "ui-dialog-import-counts",
            added = summary.merged.added,
            updated = summary.merged.updated,
            unchanged = summary.merged.unchanged
        )),
    ]
    .spacing(SPACING);
    if !summary.skipped.is_empty() {
        let skipped = summary.skipped.iter().fold(
            Column::new().spacing(SPACING / 2.0),
            |list, (name, reason)| {
                list.push(
                    text(fl!(
                        "ui-dialog-import-skipped-item",
                        name = server_text(name),
                        reason = texts::skip_reason(reason)
                    ))
                    .size(SMALL_SIZE),
                )
            },
        );
        content = content
            .push(text(fl!("ui-dialog-import-skipped")))
            .push(container(scrollable(skipped)).max_height(SKIPPED_LIST_HEIGHT));
    }
    content.push(ok).into()
}

/// Asks for a name: Enter in the field confirms, like the button.
fn name_dialog(action: NameAction, value: &str) -> Element<'_, Message> {
    let (title, confirm) = match action {
        NameAction::NewFolder => (
            fl!("ui-dialog-new-folder-title"),
            fl!("ui-dialog-new-folder-confirm"),
        ),
        NameAction::Rename => (
            fl!("ui-dialog-rename-title"),
            fl!("ui-dialog-rename-confirm"),
        ),
    };
    column![
        text(title).size(HEADING_SIZE),
        text_input(&fl!("ui-dialog-name-placeholder"), value)
            .id(name_field_id())
            .on_input(|value| Message::App(AppMessage::Files(FilesMessage::NameEdited(value))))
            .on_submit(Message::App(AppMessage::ConfirmDialog)),
        row![
            button(text(fl!("ui-dialog-cancel-button")))
                .style(button::secondary)
                .on_press(Message::App(AppMessage::DismissDialog)),
            button(text(confirm)).on_press(Message::App(AppMessage::ConfirmDialog)),
        ]
        .spacing(SPACING),
    ]
    .spacing(SPACING)
    .into()
}

fn dialog_view(dialog: &Dialog) -> Element<'_, Message> {
    let confirm = |label: String| {
        button(text(label))
            .style(button::danger)
            .on_press(Message::App(AppMessage::ConfirmDialog))
    };
    let dismiss = |label: String| {
        button(text(label))
            .style(button::secondary)
            .on_press(Message::App(AppMessage::DismissDialog))
    };
    let ok = || {
        button(text(fl!("ui-dialog-ok-button"))).on_press(Message::App(AppMessage::DismissDialog))
    };
    let heading = |label: String| text(label).size(HEADING_SIZE);
    let question = |title: String, body: String, action: String| {
        column![
            heading(title),
            text(body),
            row![dismiss(fl!("ui-dialog-cancel-button")), confirm(action)].spacing(SPACING),
        ]
        .spacing(SPACING)
    };
    let detail = |detail: &str| text(fl!("ui-dialog-detail", detail = detail)).size(SMALL_SIZE);
    match dialog {
        Dialog::ConfirmCloseTab(_) => question(
            fl!("ui-dialog-close-tab-title"),
            fl!("ui-dialog-close-tab-body"),
            fl!("ui-dialog-close-tab-confirm"),
        )
        .into(),
        Dialog::ConfirmExit { live } => question(
            fl!("ui-dialog-exit-title"),
            fl!("ui-dialog-exit-body", count = (*live)),
            fl!("ui-dialog-exit-confirm"),
        )
        .into(),
        Dialog::ConfirmOverwrite {
            direction, name, ..
        } => question(
            fl!("ui-dialog-overwrite-title"),
            match direction {
                Direction::Download => {
                    fl!("ui-dialog-overwrite-local-body", name = name.as_str())
                }
                Direction::Upload => fl!("ui-dialog-overwrite-remote-body", name = name.as_str()),
            },
            fl!("ui-dialog-overwrite-confirm"),
        )
        .into(),
        Dialog::AskName { action, value, .. } => name_dialog(*action, value),
        Dialog::ConfirmDelete { name, folder, .. } => question(
            fl!("ui-dialog-delete-title"),
            if *folder {
                fl!("ui-dialog-delete-folder-body", name = name.as_str())
            } else {
                fl!("ui-dialog-delete-file-body", name = name.as_str())
            },
            fl!("ui-dialog-delete-confirm"),
        )
        .into(),
        Dialog::ConfirmPaste { lines, .. } => question(
            fl!("ui-dialog-paste-title"),
            fl!("ui-dialog-paste-body", count = (*lines)),
            fl!("ui-dialog-paste-confirm"),
        )
        .into(),
        Dialog::ImportDone(summary) => import_report(summary, ok()),
        Dialog::ImportFailed { detail: technical } => column![
            heading(fl!("ui-dialog-import-failed-title")),
            detail(technical),
            ok(),
        ]
        .spacing(SPACING)
        .into(),
        Dialog::StoreError { detail: technical } => column![
            heading(fl!("ui-dialog-store-title")),
            text(fl!("ui-dialog-store-body")),
            detail(technical),
            ok(),
        ]
        .spacing(SPACING)
        .into(),
    }
}

#[cfg(test)]
mod tests {
    use iced::keyboard::key::{NativeCode, Physical};
    use iced::keyboard::{Key, Location, Modifiers};

    use super::*;

    fn pressed(key: Named, modifiers: Modifiers) -> iced::Event {
        iced::Event::Keyboard(keyboard::Event::KeyPressed {
            key: Key::Named(key),
            modified_key: Key::Named(key),
            physical_key: Physical::Unidentified(NativeCode::Unidentified),
            location: Location::Standard,
            modifiers,
            text: None,
            repeat: false,
        })
    }

    fn message(key: Named, modifiers: Modifiers, status: event::Status) -> Option<Message> {
        window_event(pressed(key, modifiers), status, window::Id::unique())
    }

    #[test]
    fn a_free_key_goes_to_the_files_tab_and_a_shortcut_stays_a_shortcut() {
        assert!(matches!(
            message(Named::ArrowDown, Modifiers::empty(), event::Status::Ignored),
            Some(Message::FilesKey(FilesKey::Next))
        ));
        assert!(matches!(
            message(Named::Tab, Modifiers::CTRL, event::Status::Ignored),
            Some(Message::Shortcut(WindowShortcut::NextTab))
        ));
        assert!(
            message(
                Named::ArrowDown,
                Modifiers::empty(),
                event::Status::Captured
            )
            .is_none(),
            "a key a widget took is not taken twice"
        );
        assert!(matches!(
            message(Named::Enter, Modifiers::empty(), event::Status::Ignored),
            Some(Message::DialogKey { confirm: true })
        ));
    }
}
