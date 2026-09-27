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
use heimdall_app::gateway_draft::{GATEWAY_FIELDS, GatewayDraft};
use heimdall_app::local_driver::{LocalShell, local_events};
use heimdall_app::profile_draft::{
    DraftError, DraftProtocol, ProfileDraft, ProfileField, ProfileToggle,
};
use heimdall_app::rdp_driver::rdp_events;
use heimdall_app::telnet_driver::telnet_events;
use heimdall_app::vnc_driver::vnc_events;
use heimdall_app::{
    Answer, AnswerRegistry, App, AppConfig, AttemptId, ConnectionEvent, DesktopPane, Dialog,
    Effect, FilesMessage, LONG_MASTER_PASSWORD_CHARS, LocalConfirmation, MIN_MASTER_PASSWORD_CHARS,
    MIN_MASTER_PASSWORD_CLASSES, Message as AppMessage, NameAction, Phase, Prompt, Purpose,
    QuestionId, QuestionKind, Retry, SpecialKeys, SystemCredentials, Tab, TabGroup, TabId,
    TabMenuMessage, TreeRow, UiError, VaultDialog, VaultJob, VaultMode, VaultProblem, VaultStatus,
    connection_events, master_password_problem, open_vault, server_text,
};
use heimdall_core::paths::{self, KNOWN_HOSTS_FILE_NAME, PROFILES_FILE_NAME};
use heimdall_core::profile::{ProfileId, SshGateway, display_address};
use heimdall_ssh::{AgentSource, Secret};
use heimdall_term::GridSize;
use iced::futures::{Stream, StreamExt as _, stream};
use iced::keyboard::key::Named;
use iced::task::Handle;
use iced::widget::scrollable::RelativeOffset;
use iced::widget::{
    Column, button, center, checkbox, column, container, mouse_area, opaque, operation, pick_list,
    pin, responsive, row, scrollable, stack, text, text_input, tooltip,
};
use iced::{Color, Element, Length, Point, Subscription, Task, Theme, event, keyboard, window};
use zeroize::Zeroizing;

use crate::desktop_view::DesktopView;
use crate::files_view;
use crate::i18n::fl;
use crate::report;
use crate::terminal_view::TerminalView;
use crate::terminal_view::keys::{WindowShortcut, is_lock_key, is_search_key, window_shortcut};
use crate::texts;
use crate::tree_view::{self, CursorSpot, CursorTracker, TabMenuState, TreeMenu};

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

/// Size of a form section's title.
const BODY_SIZE: f32 = 16.0;

/// Width of the port column beside the server field, as in the C# dialog.
const PORT_FIELD_WIDTH: f32 = 150.0;

/// Tallest the list of skipped profiles grows before it scrolls, in logical pixels.
const SKIPPED_LIST_HEIGHT: f32 = 200.0;

/// Height the command of a local profile scrolls within, however long it is.
const LOCAL_COMMAND_HEIGHT: f32 = 240.0;

/// How often a waiting session's countdown is drawn anew.
const COUNTDOWN_TICK: std::time::Duration = std::time::Duration::from_secs(1);

/// Longest tab title shown, in characters.
const MAX_TAB_TITLE_CHARS: usize = 32;

/// Marks a cut title; three ASCII dots, as everywhere in the project.
const ELLIPSIS: &str = "...";

/// Opacity of the veil behind a dialog.
const VEIL_ALPHA: f32 = 0.6;

/// Widest the settings' cards grow.
const SETTINGS_WIDTH: f32 = 720.0;

/// Height of the window a dialog's scrolling fields leave to the rest: the card's padding
/// (24), the buttons (31), the error line (21) with the spacing around them (16), and a
/// margin of a spacing and a half above and below the card.
const DIALOG_RESERVED_HEIGHT: f32 = 112.0;

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
            key: keyboard::Key::Named(Named::Enter),
            repeat: false,
            ..
        }) if status == event::Status::Ignored => Some(Message::DialogKey { confirm: true }),
        // Tab whether or not a field took it: under a dialog it moves between fields.
        iced::Event::Keyboard(keyboard::Event::KeyPressed {
            key: keyboard::Key::Named(Named::Tab),
            modifiers,
            ..
        }) if !(modifiers.control() || modifiers.alt() || modifiers.logo()) => {
            Some(Message::TabKey {
                backward: modifiers.shift(),
            })
        }
        // Escape even when a widget took it: a field in a dialog takes the first Escape to
        // lose its focus, and the dialog would need a second one.
        iced::Event::Keyboard(keyboard::Event::KeyPressed {
            key: keyboard::Key::Named(Named::Escape),
            repeat: false,
            ..
        }) => Some(Message::DialogKey { confirm: false }),
        // F11 whatever took it: the window's full screen, as in the C# Heimdall. A desktop
        // keeps it from its server.
        iced::Event::Keyboard(keyboard::Event::KeyPressed {
            physical_key: keyboard::key::Physical::Code(keyboard::key::Code::F11),
            modifiers,
            repeat: false,
            ..
        }) if modifiers.is_empty() => Some(Message::ToggleFullscreen),
        // Ctrl+L even when a terminal took it: the session gets it too, as a shell's clear.
        iced::Event::Keyboard(keyboard::Event::KeyPressed {
            key,
            physical_key,
            modifiers,
            repeat: false,
            ..
        }) if is_lock_key(&key, physical_key, modifiers) => Some(Message::LockKey),
        iced::Event::Keyboard(keyboard::Event::KeyPressed {
            key,
            physical_key,
            modifiers,
            repeat,
            ..
        }) if status == event::Status::Ignored => {
            if is_search_key(&key, physical_key, modifiers) {
                return Some(Message::FocusSearch);
            }
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
    /// Tab: the next field of a dialog, or the other pane of a Files tab.
    TabKey {
        /// With Shift: the previous field.
        backward: bool,
    },
    /// Ctrl+L: lock the workspace, when a master password is set.
    LockKey,
    /// Show the settings.
    ShowSettings,
    /// F11: the window full screen, showing the session only, or back.
    ToggleFullscreen,
    /// How a tab's remote desktop is shown: fitted to the tab, or matching it.
    DesktopFit {
        /// Tab.
        tab: TabId,
        /// Fit to window rather than match it.
        fit: bool,
    },
    /// The tree's search changed.
    Search(String),
    /// Ctrl+F: move to the tree's search.
    FocusSearch,
    /// A field of the vault dialog changed.
    VaultField {
        /// Field, in the order the dialog shows them.
        index: usize,
        /// New content.
        value: String,
    },
    /// Move to field `index` of the vault dialog.
    FocusVaultField(usize),
    /// Try the master password typed.
    SubmitVault,
    /// The password field of the profile form changed.
    ProfilePassword(String),
    /// Save the profile form, with the password typed into it.
    SaveProfileForm,
    /// The password field of the gateway dialog changed.
    GatewayPassword(String),
    /// Save the gateway dialog, with the password typed into it.
    SaveGatewayForm,
    /// Open a menu of the profile tree at the pointer.
    OpenTreeMenu(TreeMenu),
    /// Close the open menu.
    CloseTreeMenu,
    /// An entry of the open menu was chosen: the menu closes, the core gets the message.
    MenuChoice(AppMessage),
    /// A tab menu's Fullscreen: the menu closes, the tab is shown, full screen.
    MenuFullscreen(TabId),
    /// Copy the report of a tab's failure, as the C# card's "Copy error".
    CopyError(TabId),
    /// A second passed while a tab waits to open again: its countdown is drawn anew.
    Tick,
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
            Self::TabKey { backward } => write!(f, "TabKey({backward})"),
            Self::LockKey => f.write_str("LockKey"),
            Self::ShowSettings => f.write_str("ShowSettings"),
            Self::ToggleFullscreen => f.write_str("ToggleFullscreen"),
            Self::DesktopFit { tab, fit } => write!(f, "DesktopFit({}, {fit})", tab.value()),
            Self::Search(_) => f.write_str("Search(..)"),
            Self::FocusSearch => f.write_str("FocusSearch"),
            Self::VaultField { index, .. } => write!(f, "VaultField({index}, ..)"),
            Self::FocusVaultField(index) => write!(f, "FocusVaultField({index})"),
            Self::SubmitVault => f.write_str("SubmitVault"),
            Self::ProfilePassword(_) => f.write_str("ProfilePassword(..)"),
            Self::SaveProfileForm => f.write_str("SaveProfileForm"),
            Self::GatewayPassword(_) => f.write_str("GatewayPassword(..)"),
            Self::SaveGatewayForm => f.write_str("SaveGatewayForm"),
            Self::OpenTreeMenu(menu) => write!(f, "OpenTreeMenu({menu:?})"),
            Self::CloseTreeMenu => f.write_str("CloseTreeMenu"),
            Self::MenuChoice(message) => write!(f, "MenuChoice({message:?})"),
            Self::MenuFullscreen(tab) => write!(f, "MenuFullscreen({})", tab.value()),
            Self::CopyError(tab) => write!(f, "CopyError({})", tab.value()),
            Self::Tick => f.write_str("Tick"),
        }
    }
}

/// Number of fields a question shows.
fn field_count(kind: &QuestionKind) -> usize {
    match kind {
        QuestionKind::KeyboardInteractive(question) => question.prompts.len(),
        QuestionKind::Username(_)
        | QuestionKind::Password(_)
        | QuestionKind::Passphrase(_)
        | QuestionKind::ServerPassword(_) => 1,
    }
}

/// The answer to `kind` made of what was typed; a missing field is empty.
fn answer(kind: &QuestionKind, mut typed: Vec<Zeroizing<String>>) -> Answer {
    typed.resize_with(field_count(kind), Zeroizing::default);
    let secret = |text: &mut Zeroizing<String>| Secret::new(std::mem::take(&mut **text));
    match kind {
        QuestionKind::Username(_) => Answer::Text(std::mem::take(&mut *typed[0])),
        QuestionKind::Password(_)
        | QuestionKind::Passphrase(_)
        | QuestionKind::ServerPassword(_) => Answer::Secret(secret(&mut typed[0])),
        QuestionKind::KeyboardInteractive(_) => {
            Answer::Secrets(typed.iter_mut().map(secret).collect())
        }
    }
}

/// Widget identifier of the name field of a dialog.
fn name_field_id() -> iced::widget::Id {
    iced::widget::Id::new("dialog-name")
}

/// Widget identifier of a field of the profile form.
fn profile_field_id(field: ProfileField) -> iced::widget::Id {
    iced::widget::Id::from(format!("profile-{field:?}"))
}

/// Widget identifier of a question field.
fn field_id(question: QuestionId, index: usize) -> iced::widget::Id {
    iced::widget::Id::from(format!("question-{}-{index}", question.value()))
}

/// The shell the sidebar button opens: the user's own, in the home folder.
fn default_local_shell() -> LocalShell {
    LocalShell {
        name: fl!("ui-local-shell-name"),
        program: None,
        arguments: heimdall_term::local::LocalArguments::default(),
        working_directory: None,
    }
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
/// Heimdall-rs's name in the system's credential store, apart from the C# Heimdall's.
const CREDENTIAL_SERVICE: &str = "Heimdall-rs";

fn config() -> AppConfig {
    AppConfig {
        profiles_file: paths::profiles_file().unwrap_or_else(|| PathBuf::from(PROFILES_FILE_NAME)),
        known_hosts: paths::known_hosts_file()
            .unwrap_or_else(|| PathBuf::from(KNOWN_HOSTS_FILE_NAME)),
        legacy_dir: paths::legacy_data_dir(),
        agent: AgentSource::Auto,
        initial_grid: INITIAL_GRID,
        files_start: paths::home_dir().unwrap_or_else(|| PathBuf::from(".")),
        system_credentials: SystemCredentials::keyring(CREDENTIAL_SERVICE),
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
    /// Which field of the open dialog was last given focus, so it is given once.
    dialog_focus: Option<DialogFocus>,
    /// What is typed into the vault dialog, in the order it shows its fields.
    vault_fields: [Zeroizing<String>; 3],
    /// What is typed into the password field of the profile form.
    profile_password: Zeroizing<String>,
    /// What is typed into the password field of the gateway dialog.
    gateway_password: Zeroizing<String>,
    /// Where the pointer is, for a menu to open there.
    cursor: CursorSpot,
    /// The menu open in the profile tree, and where.
    menu: Option<(TreeMenu, Point)>,
    /// What the content area shows.
    page: Page,
    /// Full screen: the window shows the session only.
    fullscreen: bool,
    /// Desktops shown otherwise than their protocol's default: fitted or matched.
    desktop_fit: HashMap<TabId, bool>,
    /// What the tree's search holds: the profiles it finds are shown.
    search: String,
}

/// What the content area shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Page {
    /// The tab shown.
    Tab,
    /// The settings, over the tab shown when they were opened: showing another tab leaves
    /// them.
    Settings {
        /// That tab.
        over: Option<TabId>,
    },
}

/// A field given focus in a dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DialogFocus {
    /// The name of a new folder or renamed entry.
    Name,
    /// A form, opened or being typed into.
    Form(DialogForm),
    /// A form refused for this reason: the field to fix.
    FormError(DialogForm, DraftError),
    /// The vault's master password.
    Vault,
}

/// A dialog made of fields, focused on its name when it opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DialogForm {
    /// A session's.
    Profile,
    /// An SSH gateway's.
    Gateway,
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
            dialog_focus: None,
            vault_fields: Default::default(),
            profile_password: Zeroizing::default(),
            gateway_password: Zeroizing::default(),
            cursor: CursorSpot::default(),
            menu: None,
            page: Page::Tab,
            fullscreen: false,
            desktop_fit: HashMap::new(),
            search: String::new(),
        }
    }

    /// Whether the settings are shown.
    #[must_use]
    pub fn settings_shown(&self) -> bool {
        self.page
            == Page::Settings {
                over: self.app.active,
            }
    }

    /// The application core.
    #[must_use]
    pub fn app(&self) -> &App {
        &self.app
    }

    /// The application core, the window dropped.
    #[must_use]
    pub fn into_app(self) -> App {
        self.app
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
            Some(tab) => fl!("ui-window-title-tab", tab = tab.display_title()),
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
        let events = event::listen_with(window_event);
        if self.app.tabs.iter().any(|tab| tab.retry.is_some()) {
            Subscription::batch([
                events,
                iced::time::every(COUNTDOWN_TICK).map(|_| Message::Tick),
            ])
        } else {
            events
        }
    }

    /// Applies a message.
    pub fn update(&mut self, message: Message) -> Task<Message> {
        // Behind the lock screen, the window's keys do nothing; its sessions go on. Nothing
        // else of the window is drawn to be clicked.
        if self.app.is_locked() && matches!(message, Message::Shortcut(_) | Message::FilesKey(_)) {
            return Task::none();
        }
        if matches!(message, Message::App(AppMessage::SelectTab(_))) {
            self.page = Page::Tab;
        }
        let reveal = matches!(
            message,
            Message::FilesKey(_) | Message::DialogKey { .. } | Message::TabKey { .. }
        );
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
            Message::TabKey { backward } => {
                if self.app.dialog.is_some() {
                    return if backward {
                        operation::focus_previous()
                    } else {
                        operation::focus_next()
                    };
                }
                self.files_key(FilesKey::SwitchPane)
            }
            Message::VaultField { index, value } => {
                if let Some(field) = self.vault_fields.get_mut(index) {
                    *field = Zeroizing::new(value);
                }
                return Task::none();
            }
            Message::FocusVaultField(index) => return operation::focus(vault_field_id(index)),
            Message::LockKey => {
                self.menu = None;
                self.app.update(AppMessage::LockVault)
            }
            message @ (Message::DesktopFit { .. }
            | Message::ToggleFullscreen
            | Message::ShowSettings) => return self.view_message(&message),
            Message::Search(term) => {
                self.search = term;
                return Task::none();
            }
            // Under a dialog, the tree is not there to search.
            Message::FocusSearch if self.app.dialog.is_some() => return Task::none(),
            Message::FocusSearch => {
                return operation::focus(search_field_id())
                    .chain(operation::select_all(search_field_id()));
            }
            Message::SubmitVault => self.submit_vault(),
            Message::ProfilePassword(value) => {
                self.profile_password = Zeroizing::new(value);
                return Task::none();
            }
            Message::SaveProfileForm => self.save_profile_form(),
            Message::GatewayPassword(value) => {
                self.gateway_password = Zeroizing::new(value);
                return Task::none();
            }
            Message::SaveGatewayForm => self.save_gateway_form(),
            Message::OpenTreeMenu(menu) => {
                self.open_tree_menu(menu);
                return Task::none();
            }
            Message::CloseTreeMenu => {
                self.menu = None;
                return Task::none();
            }
            Message::MenuChoice(message) => {
                self.menu = None;
                self.app.update(message)
            }
            Message::MenuFullscreen(tab) => return self.menu_fullscreen(tab),
            Message::CopyError(tab) => return self.copy_error(tab),
            Message::Tick => return Task::none(),
        };
        let mut tasks: Vec<Task<Message>> =
            effects.into_iter().map(|effect| self.run(effect)).collect();
        self.forget_finished();
        tasks.push(self.focus_question());
        tasks.push(self.focus_dialog());
        if reveal {
            tasks.push(self.reveal_selection());
        }
        Task::batch(tasks)
    }

    /// Applies a message about what the window shows: the settings, full screen, how a
    /// desktop is drawn.
    fn view_message(&mut self, message: &Message) -> Task<Message> {
        match message {
            Message::DesktopFit { tab, fit } => {
                self.desktop_fit.insert(*tab, *fit);
                Task::none()
            }
            Message::ToggleFullscreen => {
                self.fullscreen = !self.fullscreen;
                let mode = if self.fullscreen {
                    window::Mode::Fullscreen
                } else {
                    window::Mode::Windowed
                };
                window::latest().and_then(move |id| window::set_mode(id, mode))
            }
            Message::ShowSettings => {
                self.menu = None;
                self.page = Page::Settings {
                    over: self.app.active,
                };
                Task::none()
            }
            _ => Task::none(),
        }
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

    /// Hands the gateway dialog to the core with the password typed, which leaves the window.
    fn save_gateway_form(&mut self) -> Vec<Effect> {
        let typed = std::mem::take(&mut *self.gateway_password);
        let password = (!typed.is_empty()).then(|| Secret::new(typed));
        self.app.update(AppMessage::SaveGateway { password })
    }

    /// Hands the profile form to the core with the password typed, which leaves the window.
    fn save_profile_form(&mut self) -> Vec<Effect> {
        let typed = std::mem::take(&mut *self.profile_password);
        let password = (!typed.is_empty()).then(|| Secret::new(typed));
        self.app.update(AppMessage::SaveProfile { password })
    }

    /// What the dialogs show that the window holds: typed secrets, and where passwords go.
    /// The dialogs' inputs, for a window `height` high.
    fn forms(&self, height: f32) -> Forms<'_> {
        Forms {
            fields_height: (height - DIALOG_RESERVED_HEIGHT).max(0.0),
            vault: &self.vault_fields,
            profile_password: &self.profile_password,
            gateway_password: &self.gateway_password,
            gateways: self.app.gateways(),
            passwords: if self.app.can_save_passwords() {
                PasswordStore::Ready
            } else if self.app.vault_status() == VaultStatus::Locked {
                PasswordStore::VaultLocked
            } else {
                PasswordStore::None
            },
        }
    }

    /// Hands the master password typed to the core; the fields are emptied either way.
    fn submit_vault(&mut self) -> Vec<Effect> {
        let Some(Dialog::Vault(dialog)) = &self.app.dialog else {
            return Vec::new();
        };
        let mode = dialog.mode;
        let [first, second, third] = std::mem::take(&mut self.vault_fields);
        let secret = |mut text: Zeroizing<String>| Secret::new(std::mem::take(&mut *text));
        let (new, confirm) = match mode {
            VaultMode::Create => (None, Some(secret(second))),
            VaultMode::Change => (Some(secret(second)), Some(secret(third))),
            VaultMode::Unlock | VaultMode::Locked | VaultMode::Disable => (None, None),
        };
        self.app.update(AppMessage::SubmitVault {
            password: secret(first),
            new,
            confirm,
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

    /// Opens `menu` at the pointer, or, for a sub-menu, where its menu was.
    fn open_tree_menu(&mut self, menu: TreeMenu) {
        let at = match (&menu, &self.menu) {
            (TreeMenu::ConnectAs(_), Some((_, at))) => *at,
            _ => self.cursor.get(),
        };
        // As in the C# tree: a right click selects the row it is on.
        if let TreeMenu::Profile(id) = &menu {
            let _ = self.app.update(AppMessage::SelectProfile(id.clone()));
        }
        self.menu = Some((menu, at));
    }

    /// Enter confirms the open dialog, Escape dismisses it. Without a dialog, Enter opens
    /// the selection of a Files tab, and the core ignores the rest.
    fn dialog_key(&mut self, confirm: bool) -> Vec<Effect> {
        if !confirm && self.menu.take().is_some() {
            // Escape closes the open menu first.
            return Vec::new();
        }
        if self.app.dialog.is_none() {
            // Escape reaches here even when a terminal sent it to its session.
            return if confirm {
                self.files_key(FilesKey::Open)
            } else {
                Vec::new()
            };
        }
        let enter_confirms = self
            .app
            .dialog
            .as_ref()
            .is_some_and(Dialog::confirms_on_enter);
        match (confirm, enter_confirms) {
            // The form's password is here, not in the core.
            (true, true) if matches!(self.app.dialog, Some(Dialog::EditProfile { .. })) => {
                self.save_profile_form()
            }
            (true, _) if matches!(self.app.dialog, Some(Dialog::EditGateway { .. })) => {
                self.save_gateway_form()
            }
            (true, true) => self.app.update(AppMessage::ConfirmDialog),
            // Only a click agrees to this one.
            (true, false) => Vec::new(),
            (false, _) => self.app.update(AppMessage::DismissDialog),
        }
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
        if !matches!(app.dialog, Some(Dialog::Vault(_))) {
            self.vault_fields = Default::default();
        }
        // A session's form waiting under the gateway dialog keeps what was typed into it.
        let form_open = match &app.dialog {
            Some(Dialog::EditProfile { .. }) => true,
            Some(Dialog::EditGateway { back, .. }) => {
                matches!(back.as_deref(), Some(Dialog::EditProfile { .. }))
            }
            _ => false,
        };
        if !form_open {
            self.profile_password = Zeroizing::default();
        }
        if !matches!(app.dialog, Some(Dialog::EditGateway { .. })) {
            self.gateway_password = Zeroizing::default();
        }
        self.connections.retain(|tab, _| app.tab(*tab).is_some());
        self.desktop_fit.retain(|tab, _| app.tab(*tab).is_some());
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

    /// Gives focus to a dialog's field when the dialog opens, and to the field to fix when
    /// a form is refused; never again while the user types.
    fn focus_dialog(&mut self) -> Task<Message> {
        let (next, field) = match &self.app.dialog {
            Some(Dialog::AskName { .. } | Dialog::RenameTab { .. }) => {
                (Some(DialogFocus::Name), name_field_id())
            }
            Some(Dialog::Vault(_)) => (Some(DialogFocus::Vault), vault_field_id(0)),
            Some(Dialog::EditProfile { error, .. }) => {
                match self.form_focus(DialogForm::Profile, *error, profile_field_id) {
                    Some(focus) => focus,
                    None => return Task::none(),
                }
            }
            Some(Dialog::EditGateway { error, .. }) => {
                match self.form_focus(DialogForm::Gateway, *error, gateway_field_id) {
                    Some(focus) => focus,
                    None => return Task::none(),
                }
            }
            _ => (None, name_field_id()),
        };
        if next == self.dialog_focus {
            return Task::none();
        }
        self.dialog_focus = next;
        if next.is_none() {
            return Task::none();
        }
        // Selected, so typing replaces what is there: a renamed entry's name, a wrong value.
        operation::focus(field.clone()).chain(operation::select_all(field))
    }

    /// Where a form's focus goes: its name when it opens, the field to fix when refused,
    /// and nowhere once the user types, which clears the error.
    fn form_focus(
        &mut self,
        form: DialogForm,
        error: Option<DraftError>,
        field_id: fn(ProfileField) -> iced::widget::Id,
    ) -> Option<(Option<DialogFocus>, iced::widget::Id)> {
        if let Some(error) = error {
            return Some((
                Some(DialogFocus::FormError(form, error)),
                field_id(error.field()),
            ));
        }
        if matches!(
            self.dialog_focus,
            Some(DialogFocus::Form(shown) | DialogFocus::FormError(shown, _)) if shown == form
        ) {
            // Typing cleared the error: the focus stays where the user put it.
            self.dialog_focus = Some(DialogFocus::Form(form));
            return None;
        }
        Some((Some(DialogFocus::Form(form)), field_id(ProfileField::Name)))
    }

    /// Feeds the events of a connection attempt back as messages, until the tab closes: the
    /// task is aborted with its tab.
    fn connection_task(
        &mut self,
        tab: TabId,
        attempt: AttemptId,
        events: impl Stream<Item = ConnectionEvent> + Send + 'static,
    ) -> Task<Message> {
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
                self.connection_task(tab, attempt, events)
            }
            Effect::ConnectRdp {
                tab,
                attempt,
                request,
            } => {
                let registry = self.registry.clone();
                let events = stream::once(async move { rdp_events(*request, registry) }).flatten();
                self.connection_task(tab, attempt, events)
            }
            Effect::ConnectTelnet {
                tab,
                attempt,
                request,
            } => {
                let events = stream::once(async move { telnet_events(*request) }).flatten();
                self.connection_task(tab, attempt, events)
            }
            Effect::ConnectVnc {
                tab,
                attempt,
                request,
            } => {
                let registry = self.registry.clone();
                let events = stream::once(async move { vnc_events(*request, registry) }).flatten();
                self.connection_task(tab, attempt, events)
            }
            Effect::ConnectLocal {
                tab,
                attempt,
                request,
            } => {
                let events = stream::once(async move { local_events(*request) }).flatten();
                self.connection_task(tab, attempt, events)
            }
            Effect::Answer { question, answer } => {
                if !self.registry.answer(question, answer) {
                    log::debug!("question {} was no longer waiting", question.value());
                }
                Task::none()
            }
            effect @ (Effect::ListRemote { .. }
            | Effect::ListLocal { .. }
            | Effect::Transfer { .. }
            | Effect::FileOperation { .. }) => files_task(effect),
            Effect::WriteClipboard(content) => iced::clipboard::write(content),
            Effect::ReadClipboard { tab } => iced::clipboard::read()
                .map(move |text| Message::App(AppMessage::ClipboardText { tab, text })),
            Effect::RetryAt {
                tab,
                attempt,
                deadline,
            } => Task::perform(
                tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)),
                move |()| Message::App(AppMessage::AutoReconnect { tab, attempt }),
            ),
            Effect::WakeAt {
                tab,
                generation,
                deadline,
            } => Task::perform(
                tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)),
                move |()| Message::App(AppMessage::SyncDeadline { tab, generation }),
            ),
            Effect::OpenVault {
                path,
                password,
                job,
            } => open_vault_task(path, password, job),
            Effect::Exit => iced::exit(),
        }
    }

    /// Draws the window.
    #[must_use]
    pub fn view(&self) -> Element<'_, Message> {
        let locked = self.app.is_locked();
        // Locked, the window is not drawn: nothing of it shows, and no hidden field takes
        // what is typed. Its sessions go on.
        let body: Element<'_, Message> = if locked {
            iced::widget::space().into()
        } else if self.fullscreen {
            // Full screen is the session's: no tree, no tabs.
            self.content()
        } else {
            row![
                self.sidebar(),
                column![self.tab_bar(), self.content()]
                    .width(Length::Fill)
                    .height(Length::Fill)
            ]
            .into()
        };
        // Always a stack with the window first: a tree of one shape keeps the state of the
        // widgets under a dialog, such as how far a list is scrolled.
        let mut layers = stack![body];
        if let Some(dialog) = &self.app.dialog {
            // Built for the window's height: a long form scrolls above its buttons.
            layers = layers.push(opaque(
                container(responsive(move |size| {
                    center(card(dialog_view(dialog, &self.forms(size.height)))).into()
                }))
                .style(move |theme: &Theme| container::Style {
                    background: Some(if locked {
                        theme.palette().background.into()
                    } else {
                        Color {
                            a: VEIL_ALPHA,
                            ..Color::BLACK
                        }
                        .into()
                    }),
                    ..container::Style::default()
                }),
            ));
        }
        let open_menu = self
            .menu
            .as_ref()
            .filter(|_| !locked)
            .and_then(|(menu, at)| {
                let entries = if let TreeMenu::Tab(tab) = menu {
                    tree_view::tab_menu_entries(&self.tab_menu_state(*tab)?)
                } else {
                    let profile = match menu {
                        TreeMenu::Profile(id) | TreeMenu::ConnectAs(id) => {
                            self.app.profile_summary(id)
                        }
                        TreeMenu::Add | TreeMenu::More | TreeMenu::Tab(_) => None,
                    };
                    let editable = profile.as_ref().is_some_and(|p| self.app.can_edit(&p.id));
                    let connect_as = profile
                        .as_ref()
                        .map(|p| self.app.connect_as_choices(&p.id))
                        .unwrap_or_default();
                    tree_view::menu_entries(
                        menu,
                        profile.as_ref(),
                        &connect_as,
                        editable,
                        self.app.can_import(),
                    )
                };
                Some((entries, *at))
            });
        if let Some((entries, at)) = open_menu {
            // Opaque: what is under the menu is neither hovered nor clicked.
            layers = layers.push(opaque(
                mouse_area(
                    pin(entries)
                        .x(at.x)
                        .y(at.y)
                        .width(Length::Fill)
                        .height(Length::Fill),
                )
                .on_press(Message::CloseTreeMenu)
                .on_right_press(Message::CloseTreeMenu),
            ));
        }
        CursorTracker::new(layers, self.cursor.clone()).into()
    }

    fn sidebar(&self) -> Element<'_, Message> {
        // As in the C# Heimdall: "+" adds, "..." holds the rest; the vault and the local shell
        // keep their buttons until they find their C# place.
        let tool = |label: &'static str, tip: String, menu: TreeMenu| {
            tooltip(
                button(text(label))
                    .style(button::secondary)
                    .on_press(Message::OpenTreeMenu(menu)),
                text(tip).size(SMALL_SIZE),
                tooltip::Position::Bottom,
            )
            .style(container::rounded_box)
        };
        let header = row![
            text(fl!("ui-sidebar-title")).size(HEADING_SIZE),
            iced::widget::space::horizontal(),
            tool("+", fl!("ui-tree-add-tooltip"), TreeMenu::Add),
            tool("...", fl!("ui-tree-more-tooltip"), TreeMenu::More),
        ]
        .spacing(SPACING / 2.0)
        .align_y(iced::Alignment::Center);
        let mut actions = row![
            button(text(fl!("ui-sidebar-local-shell-button")))
                .on_press(Message::App(AppMessage::OpenLocal(default_local_shell())))
                .style(button::secondary),
            button(text(fl!("ui-sidebar-settings-button")))
                .on_press(Message::ShowSettings)
                .style(if self.settings_shown() {
                    button::primary
                } else {
                    button::secondary
                }),
        ]
        .spacing(SPACING / 2.0);
        // As the C# toolbar's lock: there only while a master password is set.
        if self.app.vault_status() == VaultStatus::Open {
            actions = actions.push(
                tooltip(
                    button(text(fl!("ui-sidebar-lock-button")))
                        .on_press(Message::App(AppMessage::LockVault))
                        .style(button::secondary),
                    text(fl!("ui-sidebar-lock-tooltip")).size(SMALL_SIZE),
                    tooltip::Position::Bottom,
                )
                .style(container::rounded_box),
            );
        }
        let actions = actions.wrap();
        let mut list = Column::new().spacing(2.0);
        if self.app.profile_summaries().is_empty() {
            list = list.push(text(fl!("ui-sidebar-empty")));
        }
        let rows = self.app.tree_rows(&self.search);
        if rows.is_empty() && !self.search.trim().is_empty() {
            list = list
                .push(text(fl!("ui-tree-search-no-results")).size(SMALL_SIZE))
                .push(
                    button(text(fl!("ui-tree-search-clear")).size(SMALL_SIZE))
                        .style(button::secondary)
                        .on_press(Message::Search(String::new())),
                );
        }
        // As the C# tree: folders nested and folded, sub-folders first, "(No Folder)" last.
        list = list.extend(rows.into_iter().map(|row| match row {
            TreeRow::Folder {
                path,
                name,
                depth,
                open,
            } => tree_view::folder_row(path, name, depth, open),
            TreeRow::Profile { profile, depth } => {
                let selected = self.app.selected_profile.as_ref() == Some(&profile.id);
                tree_view::indented(tree_view::owned_row(&profile, selected), depth)
            }
        }));
        // A right click beside the rows is the tree's own menu.
        let tree = mouse_area(
            container(scrollable(list))
                .width(Length::Fill)
                .height(Length::Fill),
        )
        .on_right_press(Message::OpenTreeMenu(TreeMenu::Add));
        container(
            column![header, actions, self.search_box(), tree]
                .spacing(SPACING)
                .padding(PADDING),
        )
        .width(SIDEBAR_WIDTH)
        .height(Length::Fill)
        .style(container::rounded_box)
        .into()
    }

    /// The tree's search, as the C# sidebar's: typing filters the profiles, Ctrl+F comes
    /// here, and the clear button empties it.
    fn search_box(&self) -> Element<'_, Message> {
        let mut search = row![
            tooltip(
                text_input(&fl!("ui-tree-search-placeholder"), &self.search)
                    .id(search_field_id())
                    .on_input(Message::Search),
                text(fl!("ui-tree-search-tooltip")).size(SMALL_SIZE),
                tooltip::Position::Bottom,
            )
            .style(container::rounded_box)
        ]
        .spacing(SPACING / 2.0)
        .align_y(iced::Alignment::Center);
        if !self.search.is_empty() {
            search = search.push(
                tooltip(
                    button(text(fl!("ui-tree-search-clear-button")))
                        .style(button::secondary)
                        .on_press(Message::Search(String::new())),
                    text(fl!("ui-tree-search-clear")).size(SMALL_SIZE),
                    tooltip::Position::Bottom,
                )
                .style(container::rounded_box),
            );
        }
        search.into()
    }

    /// The settings, as the C# Settings tab's Security page: the master password card.
    fn settings_page(&self) -> Element<'_, Message> {
        let enabled = self.app.vault_status() != VaultStatus::Missing;
        let mut actions = row![
            text(if enabled {
                fl!("ui-settings-vault-enabled")
            } else {
                fl!("ui-settings-vault-disabled")
            }),
            iced::widget::space::horizontal(),
        ]
        .spacing(SPACING)
        .align_y(iced::Alignment::Center);
        if enabled {
            actions = actions
                .push(
                    button(text(fl!("ui-settings-vault-change")))
                        .style(button::secondary)
                        .on_press(Message::App(AppMessage::ChangeMasterPassword)),
                )
                .push(
                    button(text(fl!("ui-settings-vault-disable")))
                        .style(button::secondary)
                        .on_press(Message::App(AppMessage::DisableMasterPassword)),
                );
        } else {
            actions = actions.push(
                button(text(fl!("ui-settings-vault-enable")))
                    .on_press(Message::App(AppMessage::ShowVault)),
            );
        }
        let vault_card = container(
            column![
                text(fl!("ui-settings-vault-title")).size(BODY_SIZE),
                text(fl!("ui-settings-vault-explanation")).size(SMALL_SIZE),
                actions,
            ]
            .spacing(SPACING),
        )
        .padding(PADDING)
        .max_width(SETTINGS_WIDTH)
        .style(container::bordered_box);
        scrollable(
            column![
                text(fl!("ui-settings-title")).size(HEADING_SIZE),
                text(fl!("ui-settings-security")).size(BODY_SIZE),
                vault_card,
            ]
            .spacing(SPACING)
            .padding(PADDING),
        )
        .into()
    }

    /// A tab menu's Fullscreen: the menu closes, the tab is shown, and the window goes full
    /// screen as F11 takes it.
    fn menu_fullscreen(&mut self, tab: TabId) -> Task<Message> {
        self.menu = None;
        self.page = Page::Tab;
        let effects = self.app.update(AppMessage::SelectTab(tab));
        let mut tasks: Vec<Task<Message>> =
            effects.into_iter().map(|effect| self.run(effect)).collect();
        tasks.push(self.view_message(&Message::ToggleFullscreen));
        Task::batch(tasks)
    }

    /// Copies the report of the failure of tab `id`.
    fn copy_error(&self, id: TabId) -> Task<Message> {
        self.failure_report(id, std::time::SystemTime::now())
            .map_or_else(Task::none, iced::clipboard::write)
    }

    /// The report "Copy error" copies for tab `id` at `now`: when, which server, which
    /// version, and the error as its card says it; `None` unless its session failed.
    #[must_use]
    pub fn failure_report(&self, id: TabId, now: std::time::SystemTime) -> Option<String> {
        let tab = self.app.tab(id)?;
        let Phase::Failed(error) = &tab.phase else {
            return None;
        };
        let server = tab
            .profile
            .endpoint()
            .map(|(host, port)| format!("{} ({host}:{port})", tab.profile.name()));
        Some(report::error_report(
            self.app.tab_kind(tab).label(),
            server.as_deref(),
            &texts::error(error),
            now,
        ))
    }

    /// What the menu of tab `id` offers; `None` once the tab is gone.
    fn tab_menu_state(&self, id: TabId) -> Option<TabMenuState> {
        let tab = self.app.tab(id)?;
        let profile = self.app.tab_profile(tab);
        Some(TabMenuState {
            tab: id,
            renamed: tab.custom_title.is_some(),
            can_restart: self.app.can_restart(tab),
            can_reopen: self.app.can_reopen(tab),
            editable: profile.as_ref().is_some_and(|p| self.app.can_edit(&p.id)),
            profile,
            others: !self.app.tab_group(id, TabGroup::Others).is_empty(),
            right: !self.app.tab_group(id, TabGroup::Right).is_empty(),
        })
    }

    fn tab_bar(&self) -> Element<'_, Message> {
        let mut tabs = row![].spacing(SPACING).padding(PADDING);
        for tab in &self.app.tabs {
            let active = self.app.active == Some(tab.id);
            let title = if tab.files.is_some() && tab.custom_title.is_none() {
                fl!("ui-tab-files-title", name = tab_label(tab.display_title()))
            } else {
                tab_label(tab.display_title())
            };
            // The protocol before the name, as the C# tab's icon; in the button's own colour,
            // which a secondary one would lose on both the active and the other tabs.
            let mut label = row![
                text(self.app.tab_kind(tab).label()).size(SMALL_SIZE),
                text(title),
            ]
            .spacing(SPACING / 2.0)
            .align_y(iced::Alignment::Center);
            if tab.bell && !active {
                label = label.push(text(fl!("ui-tab-bell-badge")).size(SMALL_SIZE));
            }
            tabs = tabs.push(
                row![
                    mouse_area(
                        button(label)
                            .style(if active {
                                button::primary
                            } else {
                                button::secondary
                            })
                            .on_press(Message::App(AppMessage::SelectTab(tab.id)))
                    )
                    .on_right_press(Message::OpenTreeMenu(TreeMenu::Tab(tab.id))),
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
        if self.settings_shown() {
            return self.settings_page();
        }
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
        match &tab.phase {
            Phase::Connecting => center(card(
                column![
                    text(match (tab.retry, tab.profile.endpoint()) {
                        (Some(retry), _) => reconnecting(retry),
                        (None, Some((host, port))) => fl!(
                            "ui-connect-progress",
                            target = target(host, port, tab.profile.username())
                        ),
                        (None, None) => fl!("ui-local-starting", name = tab.profile.name()),
                    }),
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
            } => host_key_card(
                tab.id,
                host,
                *port,
                fingerprint,
                tab.asks_about_certificate().then(|| tab.profile.name()),
            ),
            Phase::Connected => match (tab.files.as_deref(), tab.desktop.as_deref()) {
                (Some(pane), _) => crate::files_view::view(tab.id, pane),
                (_, Some(pane)) => self.desktop(tab, pane),
                _ => terminal(tab, self.app.dialog.is_none()),
            },
            // A remote desktop that ended leaves nothing to look at.
            Phase::Closed { .. } if matches!(tab.purpose, Purpose::Rdp | Purpose::Vnc) => {
                let mut ended = column![text(fl!("ui-session-closed"))].spacing(SPACING);
                if let Some(reason) = &tab.end_reason {
                    ended = ended.push(text(fl!(
                        "ui-session-closed-reason",
                        reason = reason.as_str()
                    )));
                }
                center(card(ended.push(self.session_actions(tab)))).into()
            }
            Phase::Closed { exit_status } => {
                let status = exit_status.map_or_else(
                    || fl!("ui-session-closed"),
                    |status| fl!("ui-session-closed-status", status = status.to_string()),
                );
                column![
                    terminal(tab, self.app.dialog.is_none()),
                    row![text(status), self.session_actions(tab)]
                        .spacing(SPACING)
                        .padding(PADDING)
                        .align_y(iced::Alignment::Center),
                ]
                .into()
            }
            Phase::Failed(UiError::Cancelled) => center(card(
                column![text(fl!("ui-session-cancelled")), self.session_actions(tab)]
                    .spacing(SPACING),
            ))
            .into(),
            Phase::Failed(_) if let Some(retry) = tab.retry => countdown_card(tab.id, retry),
            Phase::Failed(error) => self.failure_card(tab, error),
        }
    }

    /// A failed session's card: the error, and its ways out.
    fn failure_card<'a>(&'a self, tab: &'a Tab, error: &'a UiError) -> Element<'a, Message> {
        let mut actions = self.session_actions(tab);
        // A changed key's way out is deliberate, never part of the connection: the old key
        // is forgotten, and the new one asked about as on a first contact. An RDP
        // certificate is routine to change (Windows renews its own every six months); an SSH
        // key, as the C# Heimdall warns, may be an interception.
        let forget = match error {
            UiError::HostKeyChanged { target: None, .. } if tab.purpose == Purpose::Rdp => {
                Some(fl!("ui-session-forget-server-button"))
            }
            UiError::HostKeyChanged {
                target: Some(_), ..
            } => Some(fl!("ui-session-accept-new-key-button")),
            _ => None,
        };
        if let Some(label) = forget {
            actions = actions.push(
                button(text(label))
                    .style(button::danger)
                    .on_press(Message::App(AppMessage::ForgetServer(tab.id))),
            );
        }
        center(card(
            column![
                text(fl!("ui-session-failed-title")).size(HEADING_SIZE),
                text(texts::error(error)),
                actions,
            ]
            .spacing(SPACING),
        ))
        .into()
    }

    /// What an ended or failed session offers, as the C# Heimdall's card: Reconnect when it
    /// can open again, Copy error for a failure, Edit profile, the way out of a failure
    /// that would only repeat, then Close.
    fn session_actions(&self, tab: &Tab) -> iced::widget::Row<'_, Message> {
        let mut actions = row![].spacing(SPACING).align_y(iced::Alignment::Center);
        if self.app.can_reconnect(tab) {
            actions = actions.push(
                button(text(fl!("ui-session-reconnect-button")))
                    .on_press(Message::App(AppMessage::ReconnectTab(tab.id))),
            );
        }
        if matches!(&tab.phase, Phase::Failed(error) if *error != UiError::Cancelled) {
            actions = actions.push(
                button(text(fl!("ui-session-copy-error-button")))
                    .style(button::secondary)
                    .on_press(Message::CopyError(tab.id)),
            );
        }
        if let Some(profile) = self
            .app
            .tab_profile(tab)
            .filter(|profile| self.app.can_edit(&profile.id))
        {
            actions = actions.push(
                button(text(fl!("ui-session-edit-profile-button")))
                    .style(button::secondary)
                    .on_press(Message::App(AppMessage::EditProfile(profile.id))),
            );
        }
        actions.push(
            button(text(fl!("ui-session-close-button")))
                .style(button::secondary)
                .on_press(Message::App(AppMessage::RequestCloseTab(tab.id))),
        )
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

    /// The remote desktop of a connected tab.
    /// Whether `tab`'s desktop is fitted to the tab: as chosen, else as its protocol needs.
    /// An RDP server matches the tab's size; a VNC server keeps its own, and is fitted.
    fn fits(&self, tab: &Tab) -> bool {
        self.desktop_fit
            .get(&tab.id)
            .copied()
            .unwrap_or_else(|| fits_by_default(tab.purpose))
    }

    /// A remote desktop under its bar, as the C# session's: the keys this computer keeps for
    /// itself, sent from a menu, how the desktop is shown, and full screen.
    fn desktop<'a>(&self, tab: &Tab, pane: &'a DesktopPane) -> Element<'a, Message> {
        let fit = self.fits(tab);
        let view = DesktopView::new(pane, tab.id, Message::App)
            .interactive(self.app.dialog.is_none())
            .fit(fit);
        let tab_id = tab.id;
        let mode = pick_list(
            [DesktopMode::Match, DesktopMode::Fit],
            Some(if fit {
                DesktopMode::Fit
            } else {
                DesktopMode::Match
            }),
            move |mode| Message::DesktopFit {
                tab: tab_id,
                fit: mode == DesktopMode::Fit,
            },
        )
        .text_size(SMALL_SIZE);
        let send_keys = pick_list(
            SpecialKeys::ALL.map(KeysChoice).to_vec(),
            None::<KeysChoice>,
            move |KeysChoice(keys)| Message::App(AppMessage::SendKeys { tab: tab_id, keys }),
        )
        .placeholder(fl!("ui-desktop-send-keys"))
        .text_size(SMALL_SIZE);
        let fullscreen = button(
            text(if self.fullscreen {
                fl!("ui-desktop-exit-fullscreen")
            } else {
                fl!("ui-desktop-fullscreen")
            })
            .size(SMALL_SIZE),
        )
        .style(button::secondary)
        .on_press(Message::ToggleFullscreen);
        let mut bar = row![
            // Beside it: below, it would cover the menu's first entry.
            tooltip(
                send_keys,
                text(fl!("ui-desktop-send-keys-tooltip")).size(SMALL_SIZE),
                tooltip::Position::Right,
            )
            .style(container::rounded_box),
            mode,
            fullscreen,
        ]
        .spacing(SPACING)
        .align_y(iced::Alignment::Center);
        // VNC carries the clipboard in clear: sent on a click only, as the C# Heimdall's
        // noVNC "sync" does. RDP shares it by itself when its profile says so.
        if tab.purpose == Purpose::Vnc && pane.accepts_clipboard() {
            bar = bar.push(
                tooltip(
                    button(text(fl!("ui-desktop-send-clipboard")).size(SMALL_SIZE))
                        .style(button::secondary)
                        .on_press(Message::App(AppMessage::SendClipboard(tab_id))),
                    text(fl!("ui-desktop-send-clipboard-tooltip")).size(SMALL_SIZE),
                    tooltip::Position::Right,
                )
                .style(container::rounded_box),
            );
        }
        if tab.purpose == Purpose::Vnc {
            // Always in sight: nothing on a VNC connection is encrypted.
            bar = bar.push(
                text(fl!("ui-session-vnc-unencrypted"))
                    .size(SMALL_SIZE)
                    .style(text::danger),
            );
        }
        column![bar, view].spacing(SPACING / 2.0).into()
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
            QuestionKind::ServerPassword(asked) => {
                form = form
                    .push(text(fl!(
                        "ui-prompt-server-password-title",
                        target = target(&asked.host, asked.port, None)
                    )))
                    .push(self.field(tab.id, id, 0, true, true));
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
                    host = profile.endpoint().map_or("", |(host, _)| host)
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
/// The question about an unknown key, as the C# Heimdall asks it: an SSH host's, or, when
/// `certificate` names the profile, an RDP server's own certificate. Either can be trusted
/// for this run only, never recorded.
fn host_key_card<'a>(
    tab: TabId,
    host: &'a str,
    port: u16,
    fingerprint: &'a str,
    certificate: Option<&'a str>,
) -> Element<'a, Message> {
    let port = port.to_string();
    let (heading, body, fingerprint, [reject, once, accept]) = match certificate {
        Some(name) => (
            fl!("ui-certificate-title"),
            column![
                text(fl!(
                    "ui-certificate-body",
                    name = name,
                    host = host,
                    port = port.as_str()
                )),
                text(fl!("ui-certificate-caution")),
            ]
            .spacing(SPACING),
            fl!("ui-certificate-fingerprint", fingerprint = fingerprint),
            [
                fl!("ui-certificate-refuse-button"),
                fl!("ui-certificate-trust-once-button"),
                fl!("ui-certificate-trust-button"),
            ],
        ),
        None => (
            fl!("ui-hostkey-title"),
            column![text(fl!(
                "ui-hostkey-body",
                host = host,
                port = port.as_str()
            ))],
            fl!("ui-hostkey-fingerprint", fingerprint = fingerprint),
            [
                fl!("ui-hostkey-reject-button"),
                fl!("ui-hostkey-trust-once-button"),
                fl!("ui-hostkey-accept-button"),
            ],
        ),
    };
    center(card(
        column![
            text(heading).size(HEADING_SIZE),
            body,
            text(fingerprint).font(iced::Font::MONOSPACE),
            row![
                button(text(reject))
                    .style(button::secondary)
                    .on_press(Message::App(AppMessage::HostKeyDecision {
                        tab,
                        accept: false
                    })),
                button(text(once))
                    .style(button::secondary)
                    .on_press(Message::App(AppMessage::HostKeyTrustOnce(tab))),
                button(text(accept)).on_press(Message::App(AppMessage::HostKeyDecision {
                    tab,
                    accept: true
                })),
            ]
            .spacing(SPACING)
            .wrap(),
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

/// What the dialogs show that the window holds.
struct Forms<'a> {
    /// The vault dialog's fields.
    vault: &'a [Zeroizing<String>; 3],
    /// The profile form's password.
    profile_password: &'a str,
    /// The gateway dialog's password.
    gateway_password: &'a str,
    /// Saved SSH gateways, for the lists to choose from.
    gateways: &'a [SshGateway],
    /// Whether a password typed now can be saved.
    passwords: PasswordStore,
    /// The most a dialog's scrolling fields may take, so its buttons stay in the window.
    fields_height: f32,
}

/// Whether a password typed now can be saved.
#[derive(Clone, Copy, PartialEq, Eq)]
enum PasswordStore {
    /// It can.
    Ready,
    /// A master password is set and the vault is locked.
    VaultLocked,
    /// No master password, and no store on this system.
    None,
}

/// The password of a profile, as the C# editor shows it: an empty field whatever is saved,
/// "Password saved" and a button to clear it when one is.
fn password_field<'a>(draft: &ProfileDraft, forms: &Forms<'a>) -> Element<'a, Message> {
    let mut input = text_input("", forms.profile_password)
        .id(password_field_id())
        .secure(true);
    if forms.passwords == PasswordStore::Ready {
        input = input
            .on_input(Message::ProfilePassword)
            .on_submit(Message::SaveProfileForm);
    }
    let mut field = column![
        text(fl!("ui-profile-field-password")).size(SMALL_SIZE),
        input
    ]
    .spacing(SPACING / 2.0);
    match forms.passwords {
        PasswordStore::Ready if draft.password_saved => {
            field = field.push(
                row![
                    text(fl!("ui-profile-password-saved")).size(SMALL_SIZE),
                    tooltip(
                        button(text(fl!("ui-profile-password-clear")).size(SMALL_SIZE))
                            .style(button::text)
                            .on_press(Message::App(AppMessage::ClearPassword)),
                        text(fl!("ui-profile-password-clear-tooltip")).size(SMALL_SIZE),
                        tooltip::Position::Top,
                    )
                    .style(container::rounded_box),
                ]
                .spacing(SPACING)
                .align_y(iced::Alignment::Center),
            );
        }
        PasswordStore::Ready => {}
        PasswordStore::VaultLocked => {
            field = field.push(text(fl!("ui-profile-password-locked")).size(SMALL_SIZE));
        }
        PasswordStore::None => {
            field = field.push(text(fl!("ui-profile-password-no-store")).size(SMALL_SIZE));
        }
    }
    field.into()
}

fn password_field_id() -> iced::widget::Id {
    iced::widget::Id::from("profile-password")
}

/// A protocol as the picker and the chip name it: the C# Heimdall's names.
fn protocol_name(protocol: DraftProtocol) -> String {
    match protocol {
        DraftProtocol::Rdp => fl!("ui-profile-protocol-rdp-name"),
        DraftProtocol::Ssh => fl!("ui-profile-protocol-ssh-name"),
        DraftProtocol::WinRm => fl!("ui-profile-protocol-winrm-name"),
        DraftProtocol::Vnc => fl!("ui-profile-protocol-vnc-name"),
        DraftProtocol::Telnet => fl!("ui-profile-protocol-telnet-name"),
    }
}

fn protocol_description(protocol: DraftProtocol) -> String {
    match protocol {
        DraftProtocol::Rdp => fl!("ui-profile-protocol-rdp-desc"),
        DraftProtocol::Ssh => fl!("ui-profile-protocol-ssh-desc"),
        DraftProtocol::WinRm => fl!("ui-profile-protocol-winrm-desc"),
        DraftProtocol::Vnc => fl!("ui-profile-protocol-vnc-desc"),
        DraftProtocol::Telnet => fl!("ui-profile-protocol-telnet-desc"),
    }
}

/// The first step of a new session, as in the C# dialog: a card per protocol.
fn protocol_picker<'a>() -> Element<'a, Message> {
    let mut cards = Column::new().spacing(SPACING / 2.0);
    for protocol in DraftProtocol::ALL {
        cards = cards.push(
            button(column![
                text(protocol_name(protocol)),
                text(protocol_description(protocol)).size(SMALL_SIZE),
            ])
            .width(Length::Fill)
            .style(button::secondary)
            .on_press(Message::App(AppMessage::ChooseProtocol(protocol))),
        );
    }
    column![
        text(fl!("ui-profile-new-title")).size(HEADING_SIZE),
        text(fl!("ui-profile-protocol-picker-title")),
        text(fl!("ui-profile-protocol-picker-desc")).size(SMALL_SIZE),
        cards,
        row![
            button(text(fl!("ui-dialog-cancel-button")))
                .style(button::secondary)
                .on_press(Message::App(AppMessage::DismissDialog)),
        ],
    ]
    .spacing(SPACING)
    .into()
}

/// A section of the form: its title, and its description when it has one.
fn section<'a>(title: String, description: Option<String>) -> Element<'a, Message> {
    let mut heading = column![text(title).size(BODY_SIZE)].spacing(2.0);
    if let Some(description) = description {
        heading = heading.push(text(description).size(SMALL_SIZE));
    }
    heading.into()
}

/// A text field of the form: its label, then the box. Enter saves.
fn form_field(draft: &ProfileDraft, field: ProfileField) -> Element<'_, Message> {
    let (label, placeholder) = match field {
        ProfileField::Name => (fl!("ui-profile-field-name"), String::new()),
        ProfileField::Group => (
            fl!("ui-profile-field-group"),
            fl!("ui-profile-folder-placeholder"),
        ),
        ProfileField::Host => (
            fl!("ui-profile-field-host"),
            fl!("ui-profile-host-placeholder"),
        ),
        ProfileField::Port => (
            match draft.protocol {
                DraftProtocol::Rdp => fl!("ui-profile-port-rdp"),
                DraftProtocol::Ssh => fl!("ui-profile-port-ssh"),
                DraftProtocol::WinRm => fl!("ui-profile-port-winrm"),
                DraftProtocol::Vnc => fl!("ui-profile-port-vnc"),
                DraftProtocol::Telnet => fl!("ui-profile-port-telnet"),
            },
            draft.default_port().to_string(),
        ),
        ProfileField::Username => (
            fl!("ui-profile-field-username"),
            if draft.protocol == DraftProtocol::Rdp {
                fl!("ui-profile-username-rdp-placeholder")
            } else {
                fl!("ui-profile-optional")
            },
        ),
        ProfileField::Domain => (
            fl!("ui-profile-field-domain"),
            fl!("ui-profile-domain-placeholder"),
        ),
        ProfileField::KeyPath => (fl!("ui-profile-field-key"), fl!("ui-profile-optional")),
    };
    column![
        text(label).size(SMALL_SIZE),
        text_input(&placeholder, draft.value(field))
            .id(profile_field_id(field))
            .on_input(move |value| Message::App(AppMessage::ProfileField { field, value }))
            .on_submit(Message::SaveProfileForm),
    ]
    .spacing(SPACING / 2.0)
    .into()
}

/// A box to tick, sending `toggle`.
fn toggle_box<'a>(
    draft: &ProfileDraft,
    toggle: ProfileToggle,
    label: String,
) -> Element<'a, Message> {
    checkbox(draft.is_on(toggle))
        .label(label)
        .on_toggle(move |on| Message::App(AppMessage::ProfileToggle { toggle, on }))
        .into()
}

fn toggle_label(toggle: ProfileToggle) -> String {
    match toggle {
        ProfileToggle::RedirectClipboard => fl!("ui-profile-toggle-clipboard"),
        ProfileToggle::RedirectDrives => fl!("ui-profile-toggle-drives"),
        ProfileToggle::Nla => fl!("ui-profile-toggle-nla"),
        ProfileToggle::StoredCredential => fl!("ui-profile-winrm-identity-stored"),
        ProfileToggle::UseSsl => fl!("ui-profile-toggle-use-ssl"),
        ProfileToggle::SkipCertificateCheck => fl!("ui-profile-toggle-skip-cert"),
        ProfileToggle::ViewOnly => fl!("ui-profile-toggle-view-only"),
        ProfileToggle::AllowNoPassword => fl!("ui-profile-toggle-no-password"),
        ProfileToggle::DirectConnection => fl!("ui-profile-direct-connect"),
    }
}

/// A gateway in a list: "Name (host:port)", as the C# combo shows one.
#[derive(Debug, Clone, PartialEq, Eq)]
struct GatewayChoice {
    id: ProfileId,
    label: String,
}

impl std::fmt::Display for GatewayChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label)
    }
}

fn gateway_choice(gateway: &SshGateway) -> GatewayChoice {
    GatewayChoice {
        id: gateway.id.clone(),
        label: format!(
            "{} ({})",
            server_text(&gateway.name),
            display_address(&server_text(&gateway.host), gateway.port)
        ),
    }
}

/// A session's gateway routing, as the C# Network tab: connect directly, or through a
/// gateway chosen, added or edited here.
fn network_section<'a>(draft: &ProfileDraft, gateways: &'a [SshGateway]) -> Element<'a, Message> {
    let direct = draft.is_on(ProfileToggle::DirectConnection);
    let mut section_column = column![
        section(
            fl!("ui-profile-gateway-routing"),
            Some(fl!("ui-profile-gateway-routing-desc"))
        ),
        toggle_box(
            draft,
            ProfileToggle::DirectConnection,
            fl!("ui-profile-direct-connect")
        ),
    ]
    .spacing(SPACING);
    if gateways.is_empty() {
        section_column = section_column
            .push(text(fl!("ui-gateway-list-empty")).size(SMALL_SIZE))
            .push(text(fl!("ui-gateway-empty-hint")).size(SMALL_SIZE));
    }
    let routed = draft.routed_gateway();
    if direct {
        section_column =
            section_column.push(text(fl!("ui-profile-gateway-direct-hint")).size(SMALL_SIZE));
    } else {
        let choices: Vec<GatewayChoice> = gateways.iter().map(gateway_choice).collect();
        let selected = draft
            .gateway
            .as_ref()
            .and_then(|id| choices.iter().find(|choice| choice.id == *id).cloned());
        section_column = section_column.push(
            row![
                pick_list(choices, selected, |choice: GatewayChoice| {
                    Message::App(AppMessage::ChooseGateway(choice.id))
                })
                .width(Length::Fill),
                button(text(fl!("ui-gateway-add")))
                    .style(button::secondary)
                    .on_press(Message::App(AppMessage::NewGateway)),
            ]
            .spacing(SPACING),
        );
        section_column = section_column.push(
            text(if routed.is_some() {
                fl!("ui-profile-gateway-explain-tunnel")
            } else {
                fl!("ui-profile-gateway-explain-direct")
            })
            .size(SMALL_SIZE),
        );
    }
    if let Some(id) = routed.filter(|id| gateways.iter().any(|known| known.id == *id)) {
        section_column = section_column.push(
            button(text(fl!("ui-profile-edit-gateway")).size(SMALL_SIZE))
                .style(button::secondary)
                .on_press(Message::App(AppMessage::EditGateway(id))),
        );
    }
    section_column.into()
}

/// The C# gateway dialog: name, host, port, username, key, password, parent gateway.
fn gateway_dialog<'a>(
    draft: &'a GatewayDraft,
    error: Option<DraftError>,
    forms: &Forms<'a>,
) -> Element<'a, Message> {
    let title = if draft.editing.is_some() {
        fl!("ui-gateway-edit-title")
    } else {
        fl!("ui-gateway-add-title")
    };
    let mut form = column![text(title).size(HEADING_SIZE)].spacing(SPACING);
    for field in GATEWAY_FIELDS {
        let label = match field {
            ProfileField::Name => fl!("ui-gateway-field-name"),
            ProfileField::Host => fl!("ui-gateway-field-host"),
            ProfileField::Port => fl!("ui-gateway-field-port"),
            ProfileField::Username => fl!("ui-gateway-field-username"),
            _ => fl!("ui-gateway-field-key"),
        };
        form = form.push(
            column![
                text(label).size(SMALL_SIZE),
                text_input("", draft.value(field))
                    .id(gateway_field_id(field))
                    .on_input(move |value| Message::App(AppMessage::GatewayField { field, value }))
                    .on_submit(Message::SaveGatewayForm),
            ]
            .spacing(SPACING / 2.0),
        );
    }
    form = form
        .push(gateway_password(draft, forms))
        .push(parent_gateway(draft, forms));
    if let Some(error) = error {
        form = form.push(text(texts::draft_error(error)).style(text::danger));
    }
    form.push(
        row![
            iced::widget::space::horizontal(),
            button(text(fl!("ui-dialog-cancel-button")))
                .style(button::secondary)
                .on_press(Message::App(AppMessage::DismissDialog)),
            button(text(fl!("ui-profile-save-button"))).on_press(Message::SaveGatewayForm),
        ]
        .spacing(SPACING),
    )
    .into()
}

/// The gateway's password, as the session form's: an empty field, "Password saved" and
/// Clear, then the C# hint.
fn gateway_password<'a>(draft: &GatewayDraft, forms: &Forms<'a>) -> Column<'a, Message> {
    let mut form = Column::new().spacing(SPACING);
    let mut password = text_input("", forms.gateway_password)
        .id(gateway_field_id(ProfileField::Domain))
        .secure(true);
    if forms.passwords == PasswordStore::Ready {
        password = password
            .on_input(Message::GatewayPassword)
            .on_submit(Message::SaveGatewayForm);
    }
    form = form.push(
        column![
            text(fl!("ui-gateway-field-password")).size(SMALL_SIZE),
            password
        ]
        .spacing(SPACING / 2.0),
    );
    match forms.passwords {
        PasswordStore::Ready if draft.password_saved => {
            form = form.push(
                row![
                    text(fl!("ui-profile-password-saved")).size(SMALL_SIZE),
                    button(text(fl!("ui-profile-password-clear")).size(SMALL_SIZE))
                        .style(button::text)
                        .on_press(Message::App(AppMessage::ClearGatewayPassword)),
                ]
                .spacing(SPACING)
                .align_y(iced::Alignment::Center),
            );
        }
        PasswordStore::Ready => {}
        PasswordStore::VaultLocked => {
            form = form.push(text(fl!("ui-profile-password-locked")).size(SMALL_SIZE));
        }
        PasswordStore::None => {
            form = form.push(text(fl!("ui-profile-password-no-store")).size(SMALL_SIZE));
        }
    }
    form = form.push(text(fl!("ui-gateway-password-hint")).size(SMALL_SIZE));
    form
}

/// The gateway this one is reached through: none, or another saved gateway.
fn parent_gateway<'a>(draft: &GatewayDraft, forms: &Forms<'a>) -> Element<'a, Message> {
    let mut form = Column::new();
    let mut parents = vec![ParentChoice::None];
    parents.extend(
        forms
            .gateways
            .iter()
            .filter(|gateway| Some(&gateway.id) != draft.editing.as_ref())
            .map(|gateway| ParentChoice::Gateway(gateway_choice(gateway))),
    );
    let selected = parents
        .iter()
        .find(|choice| match (choice, &draft.parent) {
            (ParentChoice::None, None) => true,
            (ParentChoice::Gateway(gateway), Some(parent)) => gateway.id == *parent,
            _ => false,
        })
        .cloned();
    form = form.push(
        column![
            text(fl!("ui-gateway-field-parent")).size(SMALL_SIZE),
            pick_list(parents, selected, |choice| {
                Message::App(AppMessage::ChooseParentGateway(match choice {
                    ParentChoice::None => None,
                    ParentChoice::Gateway(gateway) => Some(gateway.id),
                }))
            })
            .width(Length::Fill),
        ]
        .spacing(SPACING / 2.0),
    );
    form.into()
}

fn gateway_field_id(field: ProfileField) -> iced::widget::Id {
    iced::widget::Id::from(format!("gateway-{field:?}"))
}

/// A choice of parent gateway: none, which the C# list lacks, or a gateway.
#[derive(Debug, Clone, PartialEq, Eq)]
enum ParentChoice {
    None,
    Gateway(GatewayChoice),
}

impl std::fmt::Display for ParentChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::None => f.write_str(&fl!("ui-gateway-parent-none")),
            Self::Gateway(gateway) => gateway.fmt(f),
        }
    }
}

/// The `WinRM` identity, as the C# Identity list names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WinRmIdentity {
    /// The Windows account running Heimdall.
    Current,
    /// An account named in the profile.
    Stored,
}

impl std::fmt::Display for WinRmIdentity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&match self {
            Self::Current => fl!("ui-profile-winrm-identity-current"),
            Self::Stored => fl!("ui-profile-winrm-identity-stored"),
        })
    }
}

/// The protocol's credentials, as its C# card: title, account fields, password.
fn credentials_section<'a>(draft: &'a ProfileDraft, forms: &Forms<'a>) -> Column<'a, Message> {
    let mut form = Column::new().spacing(SPACING);
    let credentials = match draft.protocol {
        DraftProtocol::Rdp => Some((
            fl!("ui-profile-credentials-rdp"),
            Some(fl!("ui-profile-credentials-rdp-desc")),
        )),
        DraftProtocol::Ssh => Some((
            fl!("ui-profile-credentials-ssh"),
            Some(fl!("ui-profile-credentials-ssh-desc")),
        )),
        DraftProtocol::WinRm => Some((
            fl!("ui-profile-credentials-winrm"),
            Some(fl!("ui-profile-credentials-winrm-desc")),
        )),
        DraftProtocol::Vnc => Some((fl!("ui-profile-credentials-vnc"), None)),
        DraftProtocol::Telnet => None,
    };
    if let Some((title, description)) = credentials {
        form = form.push(section(title, description));
    }
    if draft.protocol == DraftProtocol::WinRm {
        // As in C#: an Identity list, the current Windows identity by default.
        let selected = if draft.is_on(ProfileToggle::StoredCredential) {
            WinRmIdentity::Stored
        } else {
            WinRmIdentity::Current
        };
        form = form.push(
            column![
                text(fl!("ui-profile-winrm-identity")).size(SMALL_SIZE),
                pick_list(
                    [WinRmIdentity::Current, WinRmIdentity::Stored],
                    Some(selected),
                    |identity| Message::App(AppMessage::ProfileToggle {
                        toggle: ProfileToggle::StoredCredential,
                        on: identity == WinRmIdentity::Stored,
                    }),
                )
                .width(Length::Fill),
            ]
            .spacing(SPACING / 2.0),
        );
    }
    for field in [
        ProfileField::Username,
        ProfileField::Domain,
        ProfileField::KeyPath,
    ] {
        if draft.shows(field) {
            form = form.push(form_field(draft, field));
        }
    }
    if draft.protocol == DraftProtocol::Rdp {
        form = form.push(text(fl!("ui-profile-domain-hint")).size(SMALL_SIZE));
    }
    if draft.protocol.saves_password() {
        form = form.push(password_field(draft, forms));
    }

    form
}

/// The protocol's options, as its C# card.
fn options_section(draft: &ProfileDraft) -> Column<'_, Message> {
    let mut form = Column::new().spacing(SPACING);
    let options = match draft.protocol {
        DraftProtocol::Rdp => Some(fl!("ui-profile-options-rdp")),
        DraftProtocol::Vnc => Some(fl!("ui-profile-options-vnc")),
        DraftProtocol::Telnet => Some(fl!("ui-profile-options-telnet")),
        DraftProtocol::Ssh | DraftProtocol::WinRm => None,
    };
    if let Some(options) = options {
        form = form.push(section(options, None));
    }
    for toggle in ProfileToggle::of(draft.protocol) {
        if *toggle != ProfileToggle::StoredCredential && draft.shows_toggle(*toggle) {
            form = form.push(toggle_box(draft, *toggle, toggle_label(*toggle)));
        }
    }
    if draft.protocol == DraftProtocol::Rdp && !draft.is_on(ProfileToggle::Nla) {
        form = form.push(
            text(fl!("ui-profile-nla-off-hint"))
                .size(SMALL_SIZE)
                .style(text::danger),
        );
    }
    if draft.protocol == DraftProtocol::Telnet {
        form = form.push(
            text(fl!("ui-profile-telnet-warning"))
                .size(SMALL_SIZE)
                .style(text::danger),
        );
    }

    form
}

/// The profile form, in the C# session dialog's order: the protocol, the connection basics,
/// the protocol's credentials, its options, then the folder. Enter in a field saves.
fn profile_form<'a>(
    draft: &'a ProfileDraft,
    error: Option<DraftError>,
    forms: &Forms<'a>,
) -> Element<'a, Message> {
    if !draft.protocol_chosen {
        return protocol_picker();
    }
    let adding = draft.editing.is_none();
    let title = if adding {
        fl!("ui-profile-new-title")
    } else {
        fl!("ui-profile-edit-title")
    };
    // The protocol chip: in a new session it goes back to the picker, as in C#.
    let chip = button(text(protocol_name(draft.protocol)).size(SMALL_SIZE))
        .style(button::secondary)
        .on_press_maybe(adding.then_some(Message::App(AppMessage::NewProfile)));
    let mut form = column![
        text(title).size(HEADING_SIZE),
        row![
            text(fl!("ui-profile-protocol-badge")).size(SMALL_SIZE),
            chip
        ]
        .spacing(SPACING / 2.0)
        .align_y(iced::Alignment::Center),
        section(
            fl!("ui-profile-section-basics"),
            Some(fl!("ui-profile-section-basics-desc"))
        ),
        form_field(draft, ProfileField::Name),
        row![
            container(form_field(draft, ProfileField::Host)).width(Length::Fill),
            container(form_field(draft, ProfileField::Port)).width(PORT_FIELD_WIDTH),
        ]
        .spacing(SPACING),
    ]
    .spacing(SPACING);

    form = form
        .push(credentials_section(draft, forms))
        .push(options_section(draft));

    if draft.protocol.routes_through_gateway() {
        form = form.push(network_section(draft, forms.gateways));
    }

    // Organization.
    form = form
        .push(section(fl!("ui-profile-section-organization"), None))
        .push(form_field(draft, ProfileField::Group));
    // The fields scroll; the error and the buttons stay in view under them, as the C#
    // dialog's footer does.
    let mut footer = Column::new().spacing(SPACING);
    if let Some(error) = error {
        footer = footer.push(text(texts::draft_error(error)).style(text::danger));
    }
    // As in C#: Cancel, then Save; a profile is deleted from its menu.
    footer = footer.push(
        row![
            iced::widget::space::horizontal(),
            button(text(fl!("ui-dialog-cancel-button")))
                .style(button::secondary)
                .on_press(Message::App(AppMessage::DismissDialog)),
            button(text(fl!("ui-profile-save-button"))).on_press(Message::SaveProfileForm),
        ]
        .spacing(SPACING),
    );
    column![
        container(
            scrollable(form.padding(iced::Padding::ZERO.right(PADDING))).height(Length::Shrink)
        )
        .max_height(forms.fields_height),
        footer,
    ]
    .spacing(SPACING)
    .into()
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

/// The dialogs about closing or naming tabs.
fn tab_dialog(dialog: &Dialog) -> Element<'_, Message> {
    let (title, body, action) = match dialog {
        Dialog::RenameTab { value, .. } => return rename_tab_dialog(value),
        Dialog::ConfirmCloseTabs { tabs, live } => (
            fl!("ui-dialog-close-tabs-title"),
            fl!(
                "ui-dialog-close-tabs-body",
                count = tabs.len(),
                live = (*live)
            ),
            fl!("ui-dialog-close-tab-confirm"),
        ),
        _ => (
            fl!("ui-dialog-close-tab-title"),
            fl!("ui-dialog-close-tab-body"),
            fl!("ui-dialog-close-tab-confirm"),
        ),
    };
    column![
        text(title).size(HEADING_SIZE),
        text(body),
        row![
            button(text(fl!("ui-dialog-cancel-button")))
                .style(button::secondary)
                .on_press(Message::App(AppMessage::DismissDialog)),
            button(text(action))
                .style(button::danger)
                .on_press(Message::App(AppMessage::ConfirmDialog)),
        ]
        .spacing(SPACING),
    ]
    .spacing(SPACING)
    .into()
}

/// "Reconnecting (attempt 2/20)...", as the C# countdown says it.
fn reconnecting(retry: Retry) -> String {
    fl!(
        "ui-session-reconnecting",
        attempt = retry.attempt,
        max = retry.max
    )
}

/// A session waiting to open again by itself: which attempt, in how long, and Cancel.
fn countdown_card<'a>(tab: TabId, retry: Retry) -> Element<'a, Message> {
    let left = retry
        .due
        .saturating_duration_since(std::time::Instant::now());
    // Rounded up: "in 0s" would show while the wait still runs.
    let seconds = left.as_secs() + u64::from(left.subsec_nanos() > 0);
    center(card(
        column![
            text(reconnecting(retry)).size(HEADING_SIZE),
            text(fl!("ui-session-reconnecting-in", seconds = seconds)),
            button(text(fl!("ui-session-reconnecting-cancel")))
                .style(button::secondary)
                .on_press(Message::App(AppMessage::CancelAutoReconnect(tab))),
        ]
        .spacing(SPACING),
    ))
    .into()
}

/// A name for a tab, as the C# "Rename Tab" asks it: the present one written in, an empty
/// one giving the tab its own title back.
fn rename_tab_dialog(value: &str) -> Element<'_, Message> {
    column![
        text(fl!("ui-dialog-rename-tab-title")).size(HEADING_SIZE),
        text(fl!("ui-dialog-rename-tab-prompt")),
        text_input(&fl!("ui-dialog-name-placeholder"), value)
            .id(name_field_id())
            .on_input(|value| Message::App(AppMessage::TabMenu(TabMenuMessage::NameEdited(value))))
            .on_submit(Message::App(AppMessage::ConfirmDialog)),
        row![
            button(text(fl!("ui-dialog-cancel-button")))
                .style(button::secondary)
                .on_press(Message::App(AppMessage::DismissDialog)),
            button(text(fl!("ui-dialog-rename-confirm")))
                .on_press(Message::App(AppMessage::ConfirmDialog)),
        ]
        .spacing(SPACING),
    ]
    .spacing(SPACING)
    .into()
}

/// The command a local profile would run, shown whole before it does: nothing cut, every
/// invisible character written out, and a warning when the program reads its line again.
fn local_command_dialog(confirmation: &LocalConfirmation) -> Element<'_, Message> {
    let mut body = column![
        text(fl!("ui-dialog-local-title")).size(HEADING_SIZE),
        text(fl!(
            "ui-dialog-local-body",
            name = confirmation.name.as_str()
        )),
        container(
            scrollable(
                text(confirmation.command.as_str())
                    .font(iced::Font::MONOSPACE)
                    .wrapping(text::Wrapping::Glyph)
            )
            .height(Length::Shrink)
        )
        .max_height(LOCAL_COMMAND_HEIGHT)
        .padding(PADDING)
        .style(container::rounded_box),
    ]
    .spacing(SPACING);
    if let Some(folder) = &confirmation.folder {
        body = body.push(text(fl!(
            "ui-dialog-local-folder",
            folder = folder.as_str()
        )));
    }
    if confirmation.rereads {
        body = body.push(text(fl!("ui-dialog-local-rereads")).style(text::danger));
    }
    body.push(
        row![
            button(text(fl!("ui-dialog-cancel-button")))
                .style(button::secondary)
                .on_press(Message::App(AppMessage::DismissDialog)),
            button(text(fl!("ui-dialog-local-confirm")))
                .style(button::danger)
                .on_press(Message::App(AppMessage::ConfirmDialog)),
        ]
        .spacing(SPACING),
    )
    .into()
}

/// The work of a Files tab: listing, transferring, changing entries.
fn files_task(effect: Effect) -> Task<Message> {
    match effect {
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
        _ => Task::none(),
    }
}

/// Opens the vault away from the window's thread: the key derivation takes a moment.
fn open_vault_task(path: PathBuf, password: Secret, job: VaultJob) -> Task<Message> {
    Task::perform(open_vault(path, password, job), |result| {
        Message::App(AppMessage::VaultOpened(result))
    })
}

/// Whether a desktop of purpose is fitted to its tab unless the user chose otherwise: a
/// VNC server keeps its own size, an RDP server is asked for the tab's.
fn fits_by_default(purpose: Purpose) -> bool {
    purpose == Purpose::Vnc
}

/// How a remote desktop is shown, as the C# Heimdall's resolution menu names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DesktopMode {
    /// The server asked for the tab's size, drawn pixel for pixel.
    Match,
    /// The whole desktop scaled into the tab.
    Fit,
}

impl fmt::Display for DesktopMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&match self {
            Self::Match => fl!("ui-desktop-match-window"),
            Self::Fit => fl!("ui-desktop-fit-window"),
        })
    }
}

/// A key combination in the desktop's menu, by the C# Heimdall's name for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct KeysChoice(SpecialKeys);

impl fmt::Display for KeysChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&match self.0 {
            SpecialKeys::CtrlAltDel => fl!("ui-desktop-keys-ctrl-alt-del"),
            SpecialKeys::Windows => fl!("ui-desktop-keys-windows"),
            SpecialKeys::AltTab => fl!("ui-desktop-keys-alt-tab"),
            SpecialKeys::CtrlEsc => fl!("ui-desktop-keys-ctrl-esc"),
            SpecialKeys::Escape => fl!("ui-desktop-keys-escape"),
            SpecialKeys::PrintScreen => fl!("ui-desktop-keys-print-screen"),
            SpecialKeys::WinL => fl!("ui-desktop-keys-win-l"),
            SpecialKeys::WinD => fl!("ui-desktop-keys-win-d"),
            SpecialKeys::WinE => fl!("ui-desktop-keys-win-e"),
        })
    }
}

fn search_field_id() -> iced::widget::Id {
    iced::widget::Id::new("tree-search")
}

fn vault_field_id(index: usize) -> iced::widget::Id {
    iced::widget::Id::from(format!("vault-field-{index}"))
}

/// The vault's dialogs, as the C# Heimdall's: the master password asked at start, the lock
/// screen, and the master password enabled, changed or disabled.
fn vault_dialog<'a>(
    dialog: &'a VaultDialog,
    fields: &'a [Zeroizing<String>; 3],
) -> Element<'a, Message> {
    let master = || fl!("ui-vault-field-master");
    let new = || fl!("ui-vault-field-new");
    let confirm = || fl!("ui-vault-field-confirm");
    let (title, body, labels, action, busy) = match dialog.mode {
        VaultMode::Unlock => (
            fl!("ui-vault-unlock-title"),
            None,
            vec![master()],
            fl!("ui-vault-unlock-button"),
            fl!("ui-vault-unlock-busy"),
        ),
        VaultMode::Locked => (
            fl!("ui-vault-locked-title"),
            Some(fl!("ui-vault-locked-body")),
            vec![master()],
            fl!("ui-vault-unlock-button"),
            fl!("ui-vault-unlock-busy"),
        ),
        VaultMode::Create => (
            fl!("ui-vault-enable-title"),
            Some(fl!("ui-vault-enable-body")),
            vec![new(), confirm()],
            fl!("ui-vault-enable-button"),
            fl!("ui-vault-enable-busy"),
        ),
        VaultMode::Change => (
            fl!("ui-vault-change-title"),
            None,
            vec![fl!("ui-vault-field-current"), new(), confirm()],
            fl!("ui-vault-change-button"),
            fl!("ui-vault-change-busy"),
        ),
        VaultMode::Disable => (
            fl!("ui-vault-disable-title"),
            Some(fl!("ui-vault-disable-warning")),
            vec![master()],
            fl!("ui-vault-disable-button"),
            fl!("ui-vault-disable-busy"),
        ),
    };
    // The new master password's field: its strength is said as it is typed, and the
    // confirmation follows it.
    let new_field = match dialog.mode {
        VaultMode::Create => Some(0),
        VaultMode::Change => Some(1),
        VaultMode::Unlock | VaultMode::Locked | VaultMode::Disable => None,
    };
    let count = labels.len();
    let mut form = column![text(title).size(HEADING_SIZE)].spacing(SPACING);
    if let Some(body) = body {
        form = form.push(text(body));
    }
    for (index, label) in labels.into_iter().enumerate() {
        let mut input = text_input("", fields[index].as_str())
            .id(vault_field_id(index))
            .secure(true);
        if !dialog.busy {
            input = input
                .on_input(move |value| Message::VaultField { index, value })
                .on_submit(if index + 1 < count {
                    Message::FocusVaultField(index + 1)
                } else {
                    Message::SubmitVault
                });
        }
        form = form.push(column![text(label).size(SMALL_SIZE), input].spacing(SPACING / 2.0));
        if new_field == Some(index) {
            form = form.push(text(policy_line(fields[index].as_str())).size(SMALL_SIZE));
        }
    }
    if let Some(problem) = &dialog.problem {
        form = form.push(text(vault_problem(problem)).style(text::danger));
    }
    if dialog.busy {
        form = form.push(text(busy).size(SMALL_SIZE));
    }
    // As in C#: a new password is taken once it follows the rules and is typed twice alike.
    let ready = new_field.is_none_or(|index| {
        master_password_problem(fields[index].as_str()).is_none()
            && fields[index].as_str() == fields[index + 1].as_str()
    });
    let mut buttons = row![iced::widget::space::horizontal()].spacing(SPACING);
    // The lock screen has no Cancel; the one asked at start quits.
    if dialog.mode != VaultMode::Locked {
        buttons = buttons.push(
            button(text(fl!("ui-dialog-cancel-button")))
                .style(button::secondary)
                .on_press(Message::App(AppMessage::DismissDialog)),
        );
    }
    buttons = buttons.push(
        button(text(action))
            .on_press_maybe((!dialog.busy && ready).then_some(Message::SubmitVault)),
    );
    form.push(buttons).into()
}

/// What the dialog says of a new master password as it is typed, by the core's own rule.
fn policy_line(password: &str) -> String {
    if password.is_empty() {
        return fl!("ui-vault-policy-hint", min = MIN_MASTER_PASSWORD_CHARS);
    }
    match master_password_problem(password) {
        Some(VaultProblem::TooShort) => {
            fl!("ui-vault-policy-too-short", min = MIN_MASTER_PASSWORD_CHARS)
        }
        Some(_) => fl!(
            "ui-vault-policy-complexity",
            classes = MIN_MASTER_PASSWORD_CLASSES,
            long = LONG_MASTER_PASSWORD_CHARS
        ),
        None => fl!("ui-vault-policy-ok"),
    }
}

fn vault_problem(problem: &VaultProblem) -> String {
    match problem {
        VaultProblem::Unreadable => fl!("ui-vault-problem-unreadable"),
        VaultProblem::Mismatch => fl!("ui-vault-problem-mismatch"),
        VaultProblem::TooShort => {
            fl!("ui-vault-policy-too-short", min = MIN_MASTER_PASSWORD_CHARS)
        }
        VaultProblem::TooSimple => fl!(
            "ui-vault-policy-complexity",
            classes = MIN_MASTER_PASSWORD_CLASSES,
            long = LONG_MASTER_PASSWORD_CHARS
        ),
        VaultProblem::NoSystemStore => fl!("ui-vault-problem-no-system-store"),
        VaultProblem::AlreadyExists => fl!("ui-vault-problem-exists"),
        VaultProblem::System { detail } => {
            fl!("ui-vault-problem-system", detail = detail.as_str())
        }
    }
}

fn dialog_view<'a>(dialog: &'a Dialog, forms: &Forms<'a>) -> Element<'a, Message> {
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
        Dialog::ConfirmCloseTab(_) | Dialog::ConfirmCloseTabs { .. } | Dialog::RenameTab { .. } => {
            tab_dialog(dialog)
        }
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
        Dialog::EditProfile { draft, error } => profile_form(draft, *error, forms),
        Dialog::ConfirmDeleteProfile { name, .. } => question(
            fl!("ui-dialog-delete-profile-title"),
            fl!("ui-dialog-delete-profile-body", name = name.as_str()),
            fl!("ui-dialog-delete-profile-confirm"),
        )
        .into(),
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
        Dialog::ConfirmLocalCommand(confirmation) => local_command_dialog(confirmation),
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
        Dialog::Vault(vault) => vault_dialog(vault, forms.vault),
        Dialog::EditGateway { draft, error, .. } => gateway_dialog(draft, *error, forms),
        Dialog::PasswordSaveFailed { detail: technical } => column![
            heading(fl!("ui-vault-save-failed-title")),
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
    fn a_vnc_desktop_is_fitted_and_an_rdp_one_matched_unless_chosen() {
        assert!(fits_by_default(Purpose::Vnc));
        assert!(!fits_by_default(Purpose::Rdp));
    }

    #[test]
    fn f11_is_the_windows_full_screen_whatever_took_it() {
        let f11 = |modifiers: Modifiers, repeat: bool| {
            iced::Event::Keyboard(keyboard::Event::KeyPressed {
                key: Key::Named(Named::F11),
                modified_key: Key::Named(Named::F11),
                physical_key: Physical::Code(iced::keyboard::key::Code::F11),
                location: Location::Standard,
                modifiers,
                text: None,
                repeat,
            })
        };
        for status in [event::Status::Captured, event::Status::Ignored] {
            assert!(matches!(
                window_event(f11(Modifiers::empty(), false), status, window::Id::unique()),
                Some(Message::ToggleFullscreen)
            ));
        }
        assert!(
            !matches!(
                window_event(
                    f11(Modifiers::empty(), true),
                    event::Status::Ignored,
                    window::Id::unique()
                ),
                Some(Message::ToggleFullscreen)
            ),
            "held down, it switches once"
        );
        assert!(
            !matches!(
                window_event(
                    f11(Modifiers::CTRL, false),
                    event::Status::Ignored,
                    window::Id::unique()
                ),
                Some(Message::ToggleFullscreen)
            ),
            "Ctrl+F11 is the session's"
        );
    }

    #[test]
    fn ctrl_f_goes_to_the_search_only_when_no_widget_took_it() {
        let ctrl_f = || {
            let key = Key::Character("f".into());
            iced::Event::Keyboard(keyboard::Event::KeyPressed {
                key: key.clone(),
                modified_key: key,
                physical_key: Physical::Unidentified(NativeCode::Unidentified),
                location: Location::Standard,
                modifiers: Modifiers::CTRL,
                text: None,
                repeat: false,
            })
        };
        assert!(matches!(
            window_event(ctrl_f(), event::Status::Ignored, window::Id::unique()),
            Some(Message::FocusSearch)
        ));
        assert!(
            window_event(ctrl_f(), event::Status::Captured, window::Id::unique()).is_none(),
            "a terminal's Ctrl+F stays its own"
        );
    }

    #[test]
    fn ctrl_l_reaches_the_window_even_when_a_terminal_took_it() {
        let letter = |modifiers: Modifiers, repeat: bool| {
            let key = Key::Character("l".into());
            iced::Event::Keyboard(keyboard::Event::KeyPressed {
                key: key.clone(),
                modified_key: key,
                physical_key: Physical::Unidentified(NativeCode::Unidentified),
                location: Location::Standard,
                modifiers,
                text: None,
                repeat,
            })
        };
        for status in [event::Status::Captured, event::Status::Ignored] {
            assert!(matches!(
                window_event(letter(Modifiers::CTRL, false), status, window::Id::unique()),
                Some(Message::LockKey)
            ));
        }
        assert!(
            window_event(
                letter(Modifiers::CTRL, true),
                event::Status::Captured,
                window::Id::unique()
            )
            .is_none(),
            "held down, it locks once"
        );
        assert!(
            window_event(
                letter(Modifiers::empty(), false),
                event::Status::Ignored,
                window::Id::unique()
            )
            .is_none()
        );
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
        assert!(matches!(
            message(Named::Tab, Modifiers::empty(), event::Status::Captured),
            Some(Message::TabKey { backward: false })
        ));
        assert!(matches!(
            message(Named::Tab, Modifiers::SHIFT, event::Status::Ignored),
            Some(Message::TabKey { backward: true })
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
        assert!(
            message(Named::Enter, Modifiers::empty(), event::Status::Captured).is_none(),
            "a field's Enter submits the field"
        );
        assert!(
            matches!(
                message(Named::Escape, Modifiers::empty(), event::Status::Captured),
                Some(Message::DialogKey { confirm: false })
            ),
            "a field taking Escape does not keep its dialog open"
        );
    }

    #[test]
    fn the_gateway_dialog_opens_on_its_name_and_a_refused_one_on_the_field_to_fix() {
        let dir = tempfile::tempdir().expect("dir");
        let mut shell = Shell::with_app(App::new(AppConfig {
            profiles_file: dir.path().join("profiles.toml"),
            known_hosts: dir.path().join("known_hosts"),
            legacy_dir: None,
            agent: AgentSource::Disabled,
            initial_grid: INITIAL_GRID,
            files_start: dir.path().to_owned(),
            system_credentials: heimdall_app::SystemCredentials::memory(),
        }));
        let _ = shell.update(Message::App(AppMessage::NewProfile));
        let _ = shell.update(Message::App(AppMessage::ChooseProtocol(DraftProtocol::Ssh)));
        assert_eq!(
            shell.dialog_focus,
            Some(DialogFocus::Form(DialogForm::Profile))
        );
        // Added from the session's form: the gateway's name, not the form's field.
        let _ = shell.update(Message::App(AppMessage::NewGateway));
        assert_eq!(
            shell.dialog_focus,
            Some(DialogFocus::Form(DialogForm::Gateway))
        );
        let _ = shell.update(Message::SaveGatewayForm);
        let Some(Dialog::EditGateway {
            error: Some(error), ..
        }) = &shell.app.dialog
        else {
            panic!("an empty gateway is refused: {:?}", shell.app.dialog);
        };
        let error = *error;
        assert_eq!(
            shell.dialog_focus,
            Some(DialogFocus::FormError(DialogForm::Gateway, error))
        );
        // Typing clears the error; the focus stays where the user put it.
        let _ = shell.update(Message::App(AppMessage::GatewayField {
            field: ProfileField::Name,
            value: "b".to_owned(),
        }));
        assert_eq!(
            shell.dialog_focus,
            Some(DialogFocus::Form(DialogForm::Gateway))
        );
    }
}
