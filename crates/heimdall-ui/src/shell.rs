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

use heimdall_core::settings::AgentPreference;
use std::collections::HashMap;
use std::fmt;
use std::path::PathBuf;

use heimdall_app::files::{
    EntryKind, FilesError, FilesKey, Side, copy_remote, file_operation, list_local, list_remote,
    move_remote, plan_transfer, transfer_events,
};
use heimdall_app::ftp_driver::ftp_events;
use heimdall_app::gateway_draft::{GATEWAY_FIELDS, GatewayDraft};
use heimdall_app::local_driver::{LocalShell, local_events};
use heimdall_app::profile_draft::{
    DraftError, DraftProtocol, ProfileChoice, ProfileDraft, ProfileField, ProfileToggle,
    SavedSecret,
};
use heimdall_app::rdp_driver::rdp_events;
use heimdall_app::telnet_driver::telnet_events;
use heimdall_app::tunnel_driver::tunnel_events;
use heimdall_app::vnc_driver::vnc_events;
use heimdall_app::winrm_driver::winrm_events;
use heimdall_app::{
    Answer, AnswerRegistry, App, AppConfig, AttemptId, BroadcastMessage, CertificateContext,
    ConnectionEvent, DesktopPane, Dialog, Effect, FilesMessage, FilterMessage, FolderMessage,
    FolderNaming, LONG_MASTER_PASSWORD_CHARS, LocalConfirmation, MIN_MASTER_PASSWORD_CHARS,
    MIN_MASTER_PASSWORD_CLASSES, Message as AppMessage, NameAction, Phase, PinDialog, PinFailure,
    PinMessage, PinMode, PostConnectConfirmation, PostConnectProgress, ProfileMenuMessage, Prompt,
    ProviderMessage, Purpose, QuestionId, QuestionKind, Retry, SaveState, SelectionMessage,
    SessionState, SettingsMessage, SpecialKeys, SystemCredentials, Tab, TabGroup, TabId,
    TabMenuMessage, TabProfile, TreeRow, TrustedKeysMessage, TunnelMessage, UiError, VaultDialog,
    VaultJob, VaultMode, VaultProblem, VaultStatus, connection_events, master_password_problem,
    open_vault, server_text,
};
use heimdall_core::folder::FolderError;
use heimdall_core::paths::{self, KNOWN_HOSTS_FILE_NAME, PROFILES_FILE_NAME};
use heimdall_core::pin::{MAX_PIN_DIGITS, MIN_PIN_DIGITS, PinProblem};
use heimdall_core::profile::{ProfileId, SshGateway, display_address};
use heimdall_core::settings::Language;
use heimdall_core::settings::{
    BroadcastScope, ColorScheme, DEFAULT_SESSION_LOG_DIRECTORY, SSH_AUTO_RECONNECT_ATTEMPTS_MAX,
    SSH_AUTO_RECONNECT_ATTEMPTS_MIN,
};
use heimdall_ssh::{AgentSource, Secret};
use heimdall_term::{FindDirection, GridSize};
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
use crate::finder::Finder;
use crate::i18n::fl;
use crate::palette::Palette;
use crate::report;
use crate::search_keys::SearchKeys;
pub use crate::session_settings::SessionField;
use crate::terminal_view::TerminalView;
use crate::terminal_view::keys::{
    WindowShortcut, Zoom, ctrl_letter, is_lock_key, is_search_key, window_shortcut,
};
use crate::texts;
use crate::tree_view::{self, CursorSpot, CursorTracker, TabMenuState, TranscriptEntry, TreeMenu};
use crate::trusted_keys_view::TrustedList;

/// Grid of a tab before its first layout.
const INITIAL_GRID: GridSize = GridSize { cols: 80, rows: 24 };

/// Width of the profile list, in logical pixels.
const SIDEBAR_WIDTH: f32 = 260.0;
/// Narrowest the sidebar is dragged to, as the C# column's minimum.
const SIDEBAR_MIN_WIDTH: f32 = 180.0;
/// Widest it is dragged to.
const SIDEBAR_MAX_WIDTH: f32 = 600.0;
/// Width of the handle between the sidebar and the sessions, dragged to resize it.
const SPLITTER_WIDTH: f32 = 4.0;
/// Letters typed in the tree further apart than this start a new search, as a Windows
/// tree's type-ahead.
const TYPE_AHEAD_RESET: std::time::Duration = std::time::Duration::from_secs(1);

/// Gap between stacked elements, in logical pixels.
const SPACING: f32 = 8.0;

/// Padding inside panels, in logical pixels.
const PADDING: f32 = 12.0;

/// Space between the terminal and the panels around it, in logical pixels.
const TERMINAL_MARGIN: f32 = 6.0;

/// Width of a question or dialog card, in logical pixels.
const CARD_WIDTH: f32 = 520.0;

/// Widest a dialog holding a table grows.
const WIDE_CARD_WIDTH: f32 = 1000.0;

/// The filter button's mark, a funnel as the C# one's icon.
const FILTER_GLYPH: &str = "\u{25BD}";

/// Size of headings, in logical pixels.
const HEADING_SIZE: f32 = 20.0;

/// Size of secondary text, in logical pixels.
const SMALL_SIZE: f32 = 12.0;

/// Size of a form section's title.
const BODY_SIZE: f32 = 16.0;

/// Width of the port column beside the server field, as in the C# dialog.
const PORT_FIELD_WIDTH: f32 = 150.0;

/// Where the SOCKS proxy listens: this computer's loopback address, as in the C# Heimdall.
const LOOPBACK: &str = "127.0.0.1";

/// The port that opens none, the placeholder of the forwarded ports' fields.
const PORT_OFF: u16 = 0;

/// Tallest the list of skipped profiles grows before it scrolls, in logical pixels.
const SKIPPED_LIST_HEIGHT: f32 = 200.0;

/// Height the command of a local profile scrolls within, however long it is.
const LOCAL_COMMAND_HEIGHT: f32 = 240.0;

/// How often a waiting session's countdown is drawn anew.
const COUNTDOWN_TICK: std::time::Duration = std::time::Duration::from_secs(1);

/// Smallest and largest terminal text a zoom reaches, as the C# terminal's.
const MIN_FONT_SIZE: f32 = 8.0;
const MAX_FONT_SIZE: f32 = 28.0;
/// Width of the font size field, as the C# one's.
const FONT_SIZE_FIELD_WIDTH: f32 = 80.0;

/// Room above Quick Connect.
const PALETTE_TOP: f32 = 80.0;

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
/// The tree shortcut `key` with `modifiers` is, as the C# Heimdall's: Ctrl+E, Ctrl+N.
fn tree_shortcut(
    key: &keyboard::Key,
    physical: keyboard::key::Physical,
    modifiers: keyboard::Modifiers,
) -> Option<TreeShortcut> {
    match ctrl_letter(key, physical, modifiers)? {
        'e' => Some(TreeShortcut::Edit),
        'n' => Some(TreeShortcut::New),
        'k' => Some(TreeShortcut::QuickConnect),
        'z' => Some(TreeShortcut::Undo),
        'b' => Some(TreeShortcut::ToggleSidebar),
        _ => None,
    }
}

/// A character typed that no widget took, for the tree's type-ahead: printable, without
/// Ctrl, Alt or the logo key.
fn type_ahead(text: Option<&str>, modifiers: keyboard::Modifiers) -> Option<Message> {
    if modifiers.control() || modifiers.alt() || modifiers.logo() {
        return None;
    }
    let text = text?;
    (!text.is_empty() && text.chars().all(|c| !c.is_control()))
        .then(|| Message::TypeAhead(text.to_owned()))
}

/// While the sidebar's handle is dragged: where the pointer is, and its release.
#[expect(
    clippy::needless_pass_by_value,
    reason = "the signature `event::listen_with` takes"
)]
fn sidebar_drag_event(
    event: iced::Event,
    _status: event::Status,
    _window: window::Id,
) -> Option<Message> {
    match event {
        iced::Event::Mouse(iced::mouse::Event::CursorMoved { position }) => {
            Some(Message::SidebarDragged(position.x))
        }
        iced::Event::Mouse(iced::mouse::Event::ButtonReleased(iced::mouse::Button::Left)) => {
            Some(Message::SidebarDragEnd)
        }
        _ => None,
    }
}

fn window_event(event: iced::Event, status: event::Status, _window: window::Id) -> Option<Message> {
    match event {
        iced::Event::Window(window::Event::CloseRequested) => {
            Some(Message::App(AppMessage::WindowCloseRequested))
        }
        iced::Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) => {
            Some(Message::Modifiers(modifiers))
        }
        iced::Event::Window(window::Event::Rescaled(scale)) => Some(Message::Rescaled(scale)),
        // Whatever took it: a press on a Files tab's entry, which its button keeps.
        iced::Event::Mouse(iced::mouse::Event::ButtonPressed(iced::mouse::Button::Left)) => {
            Some(Message::PointerPressed)
        }
        iced::Event::Window(window::Event::Resized(size)) => Some(Message::WindowResized(size)),
        iced::Event::Window(window::Event::Focused) => {
            Some(Message::App(AppMessage::WindowFocus(true)))
        }
        // Files dragged from Explorer over the window, and dropped on it.
        iced::Event::Window(window::Event::FileHovered(_)) => Some(Message::FilesHovered(true)),
        iced::Event::Window(window::Event::FilesHoveredLeft) => Some(Message::FilesHovered(false)),
        iced::Event::Window(window::Event::FileDropped(path)) => Some(Message::FileDropped(path)),
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
        // lose its focus, and the dialog would need a second one. Taken by none, it may
        // leave full screen; a terminal's or a desktop's is its session's.
        iced::Event::Keyboard(keyboard::Event::KeyPressed {
            key: keyboard::Key::Named(Named::Escape),
            repeat: false,
            ..
        }) => Some(if status == event::Status::Ignored {
            Message::EscapeUntaken
        } else {
            Message::DialogKey { confirm: false }
        }),
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
            text,
            ..
        }) if status == event::Status::Ignored => {
            if is_search_key(&key, physical_key, modifiers) {
                return Some(Message::FocusSearch);
            }
            if let Some(shortcut) = tree_shortcut(&key, physical_key, modifiers) {
                return Some(Message::TreeShortcut(shortcut));
            }
            // F1 left by every widget: a terminal or a desktop keeps it for its programs.
            if physical_key == keyboard::key::Physical::Code(keyboard::key::Code::F1)
                && modifiers.is_empty()
                && !repeat
            {
                return Some(Message::Shortcut(WindowShortcut::Help));
            }
            // Shift+F10 or the menu key: the menu of what the tree has selected.
            let menu_key = matches!(key, keyboard::Key::Named(Named::ContextMenu))
                || (physical_key == keyboard::key::Physical::Code(keyboard::key::Code::F10)
                    && modifiers == keyboard::Modifiers::SHIFT);
            if menu_key {
                return Some(Message::MenuKey);
            }
            match window_shortcut(&key, physical_key, modifiers) {
                Some(WindowShortcut::CloseTab) if repeat => None,
                Some(shortcut) => Some(Message::Shortcut(shortcut)),
                // A terminal or a field took its keys first.
                None => files_view::files_key(&key, physical_key, modifiers)
                    .map(Message::FilesKey)
                    .or_else(|| type_ahead(text.as_deref(), modifiers)),
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
    /// A message of a Files tab's integrated editor, whose text the window holds.
    Editor(crate::integrated_editor::EditorMessage),
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
    /// Escape, uncaptured by any widget: leaves full screen when nothing else answers it,
    /// else as [`Message::DialogKey`].
    EscapeUntaken,
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
    /// A search typed over a list of trusted keys on the Settings page.
    TrustedSearch(TrustedList, String),
    /// A language chosen on the Settings page.
    LanguageChosen(Language),
    /// F11: the window full screen, showing the session only, or back.
    ToggleFullscreen,
    /// The window opened: its screen's density is asked for.
    WindowOpened(window::Id),
    /// The window's screen draws this many physical pixels per logical one.
    Rescaled(f32),
    /// How a tab's remote desktop is shown: fitted to the tab, or matching it.
    DesktopFit {
        /// Tab.
        tab: TabId,
        /// Fit to window rather than match it.
        fit: bool,
    },
    /// The tree's search changed.
    Search(String),
    /// The search emptied and every filter off, as the C# "Reset all filters".
    ResetTreeFilters,
    /// Ctrl+F: move to the tree's search.
    FocusSearch,
    /// Enter in the tree's search: the one profile it finds is opened.
    SearchSubmit,
    /// Down in the tree's search: the first profile it finds is selected.
    SearchDown,
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
    /// Hand over what the PIN dialog holds: the PIN at start, or the new one.
    SubmitPin,
    /// Remove the PIN, the current one typed.
    RemovePin,
    /// The external credential provider's unlock secret typed.
    ProviderUnlock(String),
    /// Save the unlock secret typed.
    SaveProviderUnlock,
    /// The password field of the profile form changed.
    ProfilePassword(String),
    /// The sudo password typed in its question.
    SudoPasswordEdited(String),
    /// Give the sudo password typed to the question.
    SudoPasswordConfirm,
    /// The key passphrase field of the profile form changed.
    ProfilePassphrase(String),
    /// Save the profile form, with the password typed into it.
    SaveProfileForm,
    /// Show this tab of the Settings page.
    SettingsTab(SettingsTab),
    /// Pick the SSH key of the profile form in the system's open dialog.
    BrowseKeyFile,
    /// The password field of the gateway dialog changed.
    GatewayPassword(String),
    /// The key passphrase field of the gateway dialog changed.
    GatewayPassphrase(String),
    /// Save the gateway dialog, with the password typed into it.
    SaveGatewayForm,
    /// "Test route" in the gateway dialog: its typed secrets copied, not taken.
    TestRouteForm,
    /// "Copy diagnostic report" in the gateway dialog.
    CopyRouteReport,
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
    /// Shift, Ctrl, Alt or the logo key pressed or released.
    Modifiers(keyboard::Modifiers),
    /// A click on a profile of the tree: it alone selected, or, with Ctrl, added or taken,
    /// or, with Shift, all from the last one clicked.
    TreeClick(ProfileId),
    /// A click in the session shown: the keyboard goes back to it from the tree.
    ContentFocus,
    /// Ctrl+E, Ctrl+N or Ctrl+K, uncaptured by any widget.
    TreeShortcut(TreeShortcut),
    /// The gateway picked, in the Gateways tab, for the references to a missing one.
    GatewayReassignPicked {
        /// The missing gateway's identifier.
        missing: ProfileId,
        /// The gateway picked.
        to: ProfileId,
    },
    /// A click beside the folders of a Files pane's breadcrumb: its path, to type in.
    EditPath {
        /// Tab.
        tab: TabId,
        /// Pane.
        side: heimdall_app::files::Side,
    },
    /// The pointer came over a place in a Files tab's panes.
    FilesHover(crate::files_drag::Spot),
    /// The pointer left it.
    FilesHoverLeft(crate::files_drag::Spot),
    /// The left button went down, wherever: a press on a Files tab's entry starts a drag.
    PointerPressed,
    /// The pointer moved, a press on an entry held.
    FilesDragMoved(Point),
    /// That press is let go.
    FilesDragEnd,
    /// The pointer is over a row of the tree, where a drag would drop.
    TreeHover(heimdall_app::DropTarget),
    /// The pointer left that row.
    TreeHoverLeft(heimdall_app::DropTarget),
    /// The pointer moved, a press in the tree held.
    TreeDragMoved(Point),
    /// The press in the tree is let go.
    TreeDragEnd,
    /// Shift+F10 or the menu key, uncaptured by any widget.
    MenuKey,
    /// A character typed that no widget took: the tree's type-ahead, while it has the
    /// keyboard.
    TypeAhead(String),
    /// The window was resized to this size.
    WindowResized(iced::Size),
    /// The handle between the sidebar and the sessions is pressed.
    SidebarDragStart,
    /// The pointer moved to this x while the handle is held.
    SidebarDragged(f32),
    /// The handle is let go.
    SidebarDragEnd,
    /// Quick Connect's search changed.
    PaletteQuery(String),
    /// Open Quick Connect's result at this place.
    PaletteChoose(usize),
    /// Close Quick Connect.
    PaletteClose,
    /// The terminal search bar's text changed.
    FinderQuery(String),
    /// Look for the search bar's text, that way.
    FinderFind(FindDirection),
    /// Close the terminal's search bar.
    FinderClose,
    /// Files are dragged over the window, or no longer.
    FilesHovered(bool),
    /// A file or folder dropped on the window.
    FileDropped(std::path::PathBuf),
    /// An action in the Settings page's box of resolution presets.
    PresetsEdited(iced::widget::text_editor::Action),
    /// Open a folder, or a web address, with the system, as the About page's buttons do.
    OpenWithSystem(std::path::PathBuf),
    /// Write a note about the session `id` from `template`, then open it in the editor set.
    NewNote {
        /// The session.
        id: ProfileId,
        /// What it starts as.
        template: heimdall_app::notes::NoteTemplate,
    },
    /// Show a page of the window's navigation.
    Navigate(Destination),
    /// The external editor typed in the Settings page.
    EditorEdited(String),
    /// Apply the external editor typed.
    EditorApply,
    /// The transcripts' folder typed in the Settings page.
    LogDirectoryEdited(String),
    /// Apply the folder typed.
    LogDirectoryApply,
    /// The terminals' font size typed in the Settings page.
    FontSizeEdited(String),
    /// Apply the font size typed.
    FontSizeApply,
    /// A number of the session card typed in the Settings page.
    SessionFieldEdited(SessionField, String),
    /// Apply the number typed in a field of the session card.
    SessionFieldApply(SessionField),
}

/// The tree's shortcuts that hold Ctrl, as the C# Heimdall's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreeShortcut {
    /// Ctrl+E: edit the profile selected, when the tree has the keyboard.
    Edit,
    /// Ctrl+N: a new session.
    New,
    /// Ctrl+K: Quick Connect.
    QuickConnect,
    /// Ctrl+Z: the last move made by a drop in the tree undone.
    Undo,
    /// Ctrl+B: the sidebar shown or hidden, as the C# one.
    ToggleSidebar,
}

impl fmt::Debug for Message {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // A field holds what the user types into a question: a password, a passphrase.
        match self {
            Self::App(message) => write!(f, "App({message:?})"),
            Self::Editor(message) => write!(f, "Editor({message:?})"),
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
            Self::EscapeUntaken => f.write_str("EscapeUntaken"),
            Self::FilesKey(key) => write!(f, "FilesKey({key:?})"),
            Self::TabKey { backward } => write!(f, "TabKey({backward})"),
            Self::LockKey => f.write_str("LockKey"),
            Self::ShowSettings => f.write_str("ShowSettings"),
            Self::Navigate(destination) => write!(f, "Navigate({destination:?})"),
            Self::TrustedSearch(list, _) => write!(f, "TrustedSearch({list:?}, ..)"),
            Self::LanguageChosen(language) => write!(f, "LanguageChosen({language:?})"),
            Self::ToggleFullscreen => f.write_str("ToggleFullscreen"),
            Self::WindowOpened(_) => f.write_str("WindowOpened"),
            Self::Rescaled(scale) => write!(f, "Rescaled({scale})"),
            Self::DesktopFit { tab, fit } => write!(f, "DesktopFit({}, {fit})", tab.value()),
            Self::Search(_) => f.write_str("Search(..)"),
            Self::ResetTreeFilters => f.write_str("ResetTreeFilters"),
            Self::FocusSearch => f.write_str("FocusSearch"),
            Self::SearchSubmit => f.write_str("SearchSubmit"),
            Self::SearchDown => f.write_str("SearchDown"),
            Self::VaultField { index, .. } => write!(f, "VaultField({index}, ..)"),
            Self::FocusVaultField(index) => write!(f, "FocusVaultField({index})"),
            Self::SubmitVault => f.write_str("SubmitVault"),
            Self::SubmitPin => f.write_str("SubmitPin"),
            Self::RemovePin => f.write_str("RemovePin"),
            Self::ProviderUnlock(_) => f.write_str("ProviderUnlock(..)"),
            Self::SaveProviderUnlock => f.write_str("SaveProviderUnlock"),
            Self::ProfilePassword(_) => f.write_str("ProfilePassword(..)"),
            Self::SudoPasswordEdited(_) => f.write_str("SudoPasswordEdited(..)"),
            Self::SudoPasswordConfirm => f.write_str("SudoPasswordConfirm"),
            Self::ProfilePassphrase(_) => f.write_str("ProfilePassphrase(..)"),
            Self::SaveProfileForm => f.write_str("SaveProfileForm"),
            Self::SettingsTab(tab) => write!(f, "SettingsTab({tab:?})"),
            Self::BrowseKeyFile => f.write_str("BrowseKeyFile"),
            Self::GatewayPassword(_) => f.write_str("GatewayPassword(..)"),
            Self::GatewayPassphrase(_) => f.write_str("GatewayPassphrase(..)"),
            Self::SaveGatewayForm => f.write_str("SaveGatewayForm"),
            Self::TestRouteForm => f.write_str("TestRouteForm"),
            Self::CopyRouteReport => f.write_str("CopyRouteReport"),
            Self::OpenTreeMenu(menu) => write!(f, "OpenTreeMenu({menu:?})"),
            Self::CloseTreeMenu => f.write_str("CloseTreeMenu"),
            Self::MenuChoice(message) => write!(f, "MenuChoice({message:?})"),
            Self::MenuFullscreen(tab) => write!(f, "MenuFullscreen({})", tab.value()),
            Self::CopyError(tab) => write!(f, "CopyError({})", tab.value()),
            Self::Tick => f.write_str("Tick"),
            Self::Modifiers(modifiers) => write!(f, "Modifiers({modifiers:?})"),
            Self::TreeClick(id) => write!(f, "TreeClick({id})"),
            Self::ContentFocus => f.write_str("ContentFocus"),
            Self::TreeShortcut(shortcut) => write!(f, "TreeShortcut({shortcut:?})"),
            Self::GatewayReassignPicked { missing, to } => {
                write!(f, "GatewayReassignPicked({missing}, {to})")
            }
            Self::EditPath { tab, side } => write!(f, "EditPath({}, {side:?})", tab.value()),
            Self::FilesHover(spot) => write!(f, "FilesHover({spot:?})"),
            Self::FilesHoverLeft(spot) => write!(f, "FilesHoverLeft({spot:?})"),
            Self::PointerPressed => f.write_str("PointerPressed"),
            Self::FilesDragMoved(_) => f.write_str("FilesDragMoved"),
            Self::FilesDragEnd => f.write_str("FilesDragEnd"),
            Self::TreeHover(target) => write!(f, "TreeHover({target:?})"),
            Self::TreeHoverLeft(target) => write!(f, "TreeHoverLeft({target:?})"),
            Self::TreeDragMoved(_) => f.write_str("TreeDragMoved"),
            Self::TreeDragEnd => f.write_str("TreeDragEnd"),
            Self::MenuKey => f.write_str("MenuKey"),
            Self::TypeAhead(_) => f.write_str("TypeAhead(..)"),
            Self::SidebarDragStart => f.write_str("SidebarDragStart"),
            Self::WindowResized(size) => write!(f, "WindowResized({size:?})"),
            Self::SidebarDragged(x) => write!(f, "SidebarDragged({x})"),
            Self::SidebarDragEnd => f.write_str("SidebarDragEnd"),
            Self::PaletteQuery(_) => f.write_str("PaletteQuery(..)"),
            Self::PaletteChoose(index) => write!(f, "PaletteChoose({index})"),
            Self::PaletteClose => f.write_str("PaletteClose"),
            Self::FinderQuery(_) => f.write_str("FinderQuery(..)"),
            Self::FinderFind(direction) => write!(f, "FinderFind({direction:?})"),
            Self::FinderClose => f.write_str("FinderClose"),
            Self::LogDirectoryEdited(_) => f.write_str("LogDirectoryEdited(..)"),
            Self::EditorEdited(_) => f.write_str("EditorEdited(..)"),
            Self::PresetsEdited(_) => f.write_str("PresetsEdited(..)"),
            Self::OpenWithSystem(_) => f.write_str("OpenWithSystem(..)"),
            Self::NewNote { id, template } => write!(f, "NewNote({id}, {template:?})"),
            Self::EditorApply => f.write_str("EditorApply"),
            Self::FilesHovered(over) => write!(f, "FilesHovered({over})"),
            Self::FileDropped(_) => f.write_str("FileDropped(..)"),
            Self::LogDirectoryApply => f.write_str("LogDirectoryApply"),
            Self::FontSizeEdited(typed) => write!(f, "FontSizeEdited({typed:?})"),
            Self::FontSizeApply => f.write_str("FontSizeApply"),
            Self::SessionFieldEdited(field, typed) => {
                write!(f, "SessionFieldEdited({field:?}, {typed:?})")
            }
            Self::SessionFieldApply(field) => write!(f, "SessionFieldApply({field:?})"),
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
        environment: Vec::new(),
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
        agent: AgentSource::Auto(AgentPreference::default()),
        initial_grid: INITIAL_GRID,
        files_start: paths::home_dir().unwrap_or_else(|| PathBuf::from(".")),
        system_credentials: SystemCredentials::keyring(CREDENTIAL_SERVICE),
    }
}

/// The window's state.
/// A tab of the Settings page, as the C# Settings tabs group them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SettingsTab {
    /// The language.
    #[default]
    General,
    /// The terminals' text and colours, and session logging.
    Terminal,
    /// SSH auto-reconnect and the trusted host keys.
    Ssh,
    /// The trusted RDP certificates.
    Rdp,
    /// The SSH gateways and what goes through each.
    Gateways,
    /// The PIN, the master password and the external credential provider.
    Security,
}

impl SettingsTab {
    /// Every tab, in the C# order.
    pub const ALL: [Self; 6] = [
        Self::General,
        Self::Terminal,
        Self::Ssh,
        Self::Rdp,
        Self::Gateways,
        Self::Security,
    ];

    fn label(self) -> String {
        match self {
            Self::General => fl!("ui-settings-tab-general"),
            Self::Terminal => fl!("ui-settings-tab-terminal"),
            Self::Ssh => fl!("ui-settings-tab-ssh"),
            Self::Rdp => fl!("ui-settings-tab-rdp"),
            Self::Gateways => fl!("ui-settings-tab-gateways"),
            Self::Security => fl!("ui-settings-tab-security"),
        }
    }
}

#[expect(
    clippy::struct_excessive_bools,
    reason = "the window's independent states: focus, panels, drags"
)]
pub struct Shell {
    app: App,
    /// The texts of the Files tabs' integrated editors.
    editors: crate::integrated_editor::Editors,
    registry: AnswerRegistry,
    /// Running connection attempts; dropping a handle aborts its task.
    connections: HashMap<TabId, Handle>,
    /// What is typed into each open question, zeroed when dropped. iced keeps its own
    /// transient copies of a field's text, which this cannot reach.
    drafts: HashMap<QuestionId, Vec<Zeroizing<String>>>,
    /// The terminals' text sizes changed by a zoom, by tab; for this run only, as the C#.
    font_sizes: HashMap<TabId, f32>,
    /// The question whose first field was last given focus.
    focused: Option<QuestionId>,
    /// Which field of the open dialog was last given focus, so it is given once.
    dialog_focus: Option<DialogFocus>,
    /// What is typed into the vault dialog or the PIN dialog, in the order it shows its
    /// fields.
    vault_fields: [Zeroizing<String>; 3],
    /// What is typed into the password field of the profile form.
    profile_password: Zeroizing<String>,
    /// The password typed in sudo's question, until given.
    sudo_password: Zeroizing<String>,
    /// What is typed into the key passphrase field of the profile form.
    profile_passphrase: Zeroizing<String>,
    /// The credential provider's unlock secret typed, not saved yet.
    provider_unlock: Zeroizing<String>,
    /// What is typed into the password field of the gateway dialog.
    gateway_password: Zeroizing<String>,
    /// What is typed into the key passphrase field of the gateway dialog.
    gateway_passphrase: Zeroizing<String>,
    /// Where the pointer is, for a menu to open there.
    cursor: CursorSpot,
    /// The menu open in the profile tree, and where.
    menu: Option<(TreeMenu, Point)>,
    /// What the content area shows.
    page: Page,
    /// Full screen: the window shows the session only.
    fullscreen: bool,
    /// Physical pixels per logical one on the window's screen: remote desktops are drawn
    /// one of their pixels per physical one.
    density: f32,
    /// The keyboard's modifiers, for a click in the tree.
    modifiers: keyboard::Modifiers,
    /// The tree has the keyboard: a click in it took it from the session shown.
    tree_focused: bool,
    /// The moving end of a Shift+arrow range in the tree; where it started is the profile
    /// selected.
    tree_focus: Option<ProfileId>,
    /// The language the texts are in, as last chosen in the settings.
    language_shown: Option<heimdall_core::settings::Language>,
    /// The gateway picked for the references to each missing gateway, by its identifier.
    gateway_reassign: std::collections::BTreeMap<ProfileId, ProfileId>,
    /// The Files pane whose path bar is typed in, rather than showing its breadcrumb.
    path_editing: Option<(TabId, heimdall_app::files::Side)>,
    /// Where the pointer is in a Files tab's panes.
    files_hover: Option<crate::files_drag::Spot>,
    /// A press on a Files tab's entry, held: a drag once the pointer moves.
    files_drag: Option<crate::files_drag::FilesDrag>,
    /// A press in the tree, held: a drag once the pointer moves.
    tree_drag: Option<crate::tree_drag::TreeDrag>,
    /// The sidebar is hidden, Ctrl+B having hidden it.
    sidebar_hidden: bool,
    /// Where the window's state is kept, and how it was left; none in tests.
    window_memory: Option<(PathBuf, heimdall_core::window_state::WindowState)>,
    /// The window's size, as last resized out of full screen.
    window_size: Option<iced::Size>,
    /// The sidebar's width, as dragged.
    sidebar_width: f32,
    /// The handle between the sidebar and the sessions is held.
    sidebar_drag: bool,
    /// The letters typed in the tree so far, and when the last one was.
    type_ahead: (String, Option<std::time::Instant>),
    /// Quick Connect, while open.
    palette: Option<Palette>,
    /// The terminal's search bar, while open.
    finder: Option<Finder>,
    /// The transcripts' folder as typed in the Settings page, until applied.
    log_directory: Option<String>,
    /// The external editor typed in the Settings page, until applied.
    editor_typed: Option<String>,
    /// The Settings page's box of resolution presets.
    presets: crate::presets_editor::PresetsEditor,
    /// The terminals' font size as typed in the Settings page, until applied.
    font_size_typed: Option<String>,
    /// The numbers of the session card as typed in the Settings page, until applied, by
    /// [`SessionField::index`].
    session_typed: [Option<String>; SessionField::COUNT],
    /// The search typed over the trusted SSH host keys.
    host_key_search: String,
    /// The Settings tab shown, kept while the application runs.
    settings_tab: SettingsTab,
    /// The search typed over the trusted RDP certificates.
    certificate_search: String,
    /// Files are dragged over the window.
    files_hovered: bool,
    /// A field that gets the keyboard once this update is drawn: Quick Connect's or the
    /// search bar's, just opened.
    focus_next: Option<iced::widget::Id>,
    /// Desktops shown otherwise than their protocol's default: fitted or matched.
    desktop_fit: HashMap<TabId, (bool, Option<(u16, u16)>)>,
    /// What the tree's search holds: the profiles it finds are shown.
    search: String,
}

/// What the content area shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Page {
    /// The tab shown.
    Tab,
    /// Every tunnel, as the C# Tunnels page.
    Tunnels,
    /// About Heimdall, as the C# About page.
    About,
    /// The settings, over the tab shown when they were opened: showing another tab leaves
    /// them.
    Settings {
        /// That tab.
        over: Option<TabId>,
    },
}

/// A page of the window's navigation, as the C# toolbar's tabs. The C# Scheduled and Tools
/// pages come with what they hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Destination {
    /// The sessions: the tree beside them.
    Sessions,
    /// Every tunnel.
    Tunnels,
    /// The settings.
    Settings,
    /// About Heimdall.
    About,
}

impl Destination {
    /// Every page, in the C# order.
    pub const ALL: [Self; 4] = [Self::Sessions, Self::Tunnels, Self::Settings, Self::About];

    fn label(self) -> String {
        match self {
            Self::Sessions => fl!("ui-nav-sessions"),
            Self::Tunnels => fl!("ui-nav-tunnels"),
            Self::Settings => fl!("ui-nav-settings"),
            Self::About => fl!("ui-nav-about"),
        }
    }
}

/// The button hiding the sidebar: an arrow toward where it goes.
const HIDE_SIDEBAR_GLYPH: &str = "\u{2190}";

/// The button showing the sidebar again: an arrow toward where it comes from.
const SHOW_SIDEBAR_GLYPH: &str = "\u{2192}";

/// The application's name, at the head of the navigation: a name, not translated.
const APP_NAME: &str = "Heimdall";

/// Size of the application's name in the navigation.
const NAV_TITLE_SIZE: f32 = 18.0;

/// Room after the application's name.
const NAV_TITLE_PADDING: [f32; 2] = [0.0, 16.0];

/// Height of the line under the page shown.
const NAV_UNDERLINE: f32 = 2.0;

/// A row of the tree the keyboard is on.
#[derive(Debug, Clone, PartialEq, Eq)]
enum TreeCursor {
    /// A profile.
    Profile(ProfileId),
    /// A folder, by its path.
    Folder(String),
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
    /// The PIN's first field.
    Pin,
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
        let config = config();
        let memory = heimdall_core::window_state::state_path(&config.profiles_file);
        let mut shell = Self::with_config(config);
        // The sidebar as the window was left.
        let left = heimdall_core::window_state::load(&memory);
        if let Some(width) = left.sidebar_width.filter(|width| width.is_finite()) {
            shell.sidebar_width = width.clamp(SIDEBAR_MIN_WIDTH, SIDEBAR_MAX_WIDTH);
        }
        shell.sidebar_hidden = left.sidebar_hidden;
        // The tree as it was left.
        shell
            .app
            .restore_tree(&left.folded, left.selected.as_deref().map(ProfileId::new));
        shell.window_memory = Some((memory, left));
        // The language chosen, once the settings are read; else the desktop's, set at start.
        if let Some(language) = shell.app.settings().language {
            crate::i18n::apply(Some(language));
        }
        shell
    }

    /// The window over `config`.
    #[must_use]
    pub fn with_config(config: AppConfig) -> Self {
        Self::with_app(App::new(config))
    }

    /// The window over an application core already in some state.
    #[must_use]
    pub fn with_app(mut app: App) -> Self {
        app.set_transcript_lines(crate::transcript_lines::lines());
        crate::logging::set_enabled(app.settings().diagnostics_log);
        let presets =
            crate::presets_editor::PresetsEditor::new(&app.settings().rdp_resolution_presets);
        let language_shown = app.settings().language;
        Self {
            app,
            editors: crate::integrated_editor::Editors::default(),
            registry: AnswerRegistry::default(),
            connections: HashMap::new(),
            drafts: HashMap::new(),
            font_sizes: HashMap::new(),
            focused: None,
            dialog_focus: None,
            vault_fields: Default::default(),
            profile_password: Zeroizing::default(),
            sudo_password: Zeroizing::default(),
            profile_passphrase: Zeroizing::default(),
            provider_unlock: Zeroizing::default(),
            gateway_password: Zeroizing::default(),
            gateway_passphrase: Zeroizing::default(),
            cursor: CursorSpot::default(),
            menu: None,
            page: Page::Tab,
            fullscreen: false,
            density: 1.0,
            modifiers: keyboard::Modifiers::empty(),
            tree_focused: false,
            tree_focus: None,
            language_shown,
            gateway_reassign: std::collections::BTreeMap::new(),
            path_editing: None,
            files_hover: None,
            files_drag: None,
            tree_drag: None,
            sidebar_hidden: false,
            window_memory: None,
            window_size: None,
            sidebar_width: SIDEBAR_WIDTH,
            sidebar_drag: false,
            type_ahead: (String::new(), None),
            palette: None,
            finder: None,
            focus_next: None,
            log_directory: None,
            editor_typed: None,
            presets,
            font_size_typed: None,
            session_typed: Default::default(),
            host_key_search: String::new(),
            settings_tab: SettingsTab::default(),
            certificate_search: String::new(),
            files_hovered: false,
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
        let events = Subscription::batch([
            event::listen_with(window_event),
            // The screen's density is known once the window is open.
            window::open_events().map(Message::WindowOpened),
        ]);
        // A countdown shown: a tab's next attempt, or the minutes before a master password
        // or a PIN is taken again.
        let locked_out = matches!(
            &self.app.dialog,
            Some(
                Dialog::Vault(VaultDialog {
                    problem: Some(VaultProblem::LockedOut { .. }),
                    ..
                }) | Dialog::Pin(PinDialog {
                    problem: Some(PinFailure::LockedOut { .. }),
                    ..
                })
            )
        );
        let mut subscriptions = vec![events];
        if self.files_drag.is_some() {
            subscriptions.push(event::listen_with(crate::files_drag::drag_event));
        }
        if self.tree_drag.is_some() {
            subscriptions.push(event::listen_with(crate::tree_drag::drag_event));
        }
        if self.sidebar_drag {
            subscriptions.push(event::listen_with(sidebar_drag_event));
        }
        if locked_out || self.app.tabs.iter().any(|tab| tab.retry.is_some()) {
            subscriptions.push(iced::time::every(COUNTDOWN_TICK).map(|_| Message::Tick));
        }
        if let Some(interval) = self.app.tmout_reset_interval() {
            subscriptions.push(
                iced::time::every(interval).map(|_| Message::App(AppMessage::TmoutResetTick)),
            );
        }
        // Every server checked in the background, as the C# session health monitor.
        if let Some(interval) = self.app.reachability_interval() {
            subscriptions.push(
                iced::time::every(interval).map(|_| Message::App(AppMessage::ReachabilityTick)),
            );
        }
        // The servers whose health panel is shown, asked as the C# asks them.
        if self.app.polls_health() {
            subscriptions.push(
                iced::time::every(heimdall_app::server_health::HEALTH_INTERVAL)
                    .map(|_| Message::App(AppMessage::HealthTick)),
            );
        }
        if let Some(interval) = self.app.anti_idle_interval() {
            subscriptions
                .push(iced::time::every(interval).map(|_| Message::App(AppMessage::AntiIdleTick)));
        }
        // Saves of files edited in an external editor, looked at while there are some.
        if self.app.has_edits() {
            subscriptions.push(
                iced::time::every(heimdall_app::external_edit::EDIT_LOOK)
                    .map(|_| Message::App(AppMessage::Files(FilesMessage::EditTick))),
            );
        }
        Subscription::batch(subscriptions)
    }

    /// Applies a message.
    #[expect(clippy::too_many_lines, reason = "one arm per family of messages")]
    pub fn update(&mut self, message: Message) -> Task<Message> {
        // Behind the lock screen, the window's keys do nothing; its sessions go on. Nothing
        // else of the window is drawn to be clicked.
        if self.gated() && matches!(message, Message::Shortcut(_) | Message::FilesKey(_)) {
            return Task::none();
        }
        // Escape no widget took leaves full screen when there is nothing else to close, as
        // the C# Heimdall's.
        if matches!(message, Message::EscapeUntaken) {
            return self.update(if self.escape_leaves_fullscreen() {
                Message::ToggleFullscreen
            } else {
                Message::DialogKey { confirm: false }
            });
        }
        let message = self.files_click(message);
        self.note_focus(&message);
        // A gesture in a Files pane, the path gone to among them, gives the breadcrumb back.
        if let Message::App(AppMessage::Files(files)) = &message
            && files.gesture().is_some()
        {
            self.path_editing = None;
        }
        // A press on a folder: the folder selected, as the C# tree's click, and the start of
        // a drag of it.
        if let Message::App(AppMessage::ToggleFolder(path)) = &message {
            self.tree_focus = None;
            let _ = self.app.update(AppMessage::SelectFolder(path.clone()));
        }
        if let Message::App(AppMessage::ToggleFolder(path)) = &message
            && path != heimdall_app::NO_FOLDER
        {
            self.tree_drag = Some(crate::tree_drag::TreeDrag::pressed(
                crate::tree_drag::DragSource::Folder(path.clone()),
                self.cursor.get(),
            ));
        }
        let reveal = matches!(
            message,
            Message::FilesKey(_) | Message::DialogKey { .. } | Message::TabKey { .. }
        );
        let effects = match message {
            Message::App(message) => self.app.update(message),
            Message::Editor(message) => self.editors.update(message, &mut self.app),
            message @ (Message::Field { .. }
            | Message::FocusField { .. }
            | Message::VaultField { .. }
            | Message::FocusVaultField(_)
            | Message::ProviderUnlock(_)
            | Message::Search(_)
            | Message::SettingsTab(_)
            | Message::ProfilePassword(_)
            | Message::SudoPasswordEdited(_)
            | Message::SudoPasswordConfirm
            | Message::ProfilePassphrase(_)
            | Message::GatewayPassword(_)
            | Message::GatewayPassphrase(_)
            | Message::CopyRouteReport) => return self.input_message(message),
            Message::Submit(tab) => self.reply(tab, true),
            Message::Decline(tab) => self.reply(tab, false),
            Message::Shortcut(WindowShortcut::Screenshot) => return self.screenshot(),
            Message::Shortcut(shortcut) => self.shortcut(shortcut),
            Message::DialogKey { confirm } => self.dialog_key(confirm),
            // Answered above, before the rest.
            Message::EscapeUntaken => Vec::new(),
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
            Message::LockKey => self.closing_menu(AppMessage::LockVault),
            message @ (Message::DesktopFit { .. }
            | Message::ToggleFullscreen
            | Message::WindowOpened(_)
            | Message::Rescaled(_)
            | Message::ShowSettings
            | Message::Navigate(_)
            | Message::TrustedSearch(..)
            | Message::LanguageChosen(_)
            | Message::Modifiers(_)
            | Message::Tick) => return self.view_message(&message),
            // Under a dialog, the tree is not there to search.
            Message::FocusSearch if self.app.dialog.is_some() => return Task::none(),
            Message::FocusSearch => {
                // Ctrl+F in a Files tab's lists is its filter's, as the C# file browser's.
                let field = match self.shown_files_side() {
                    Some(side) => files_view::field_id(side, files_view::PaneField::Filter),
                    None => search_field_id(),
                };
                return operation::focus(field.clone()).chain(operation::select_all(field));
            }
            message @ (Message::SearchSubmit | Message::SearchDown) => {
                return self.search_key(&message);
            }
            Message::SubmitVault => self.submit_vault(),
            Message::SubmitPin => self.submit_pin(),
            Message::RemovePin => self.remove_pin(),
            Message::SaveProviderUnlock => self.save_provider_unlock(),
            Message::SaveProfileForm => self.save_profile_form(),
            Message::SaveGatewayForm => self.save_gateway_form(),
            Message::TestRouteForm => self.test_route_form(),
            message @ (Message::OpenTreeMenu(_)
            | Message::CloseTreeMenu
            | Message::ResetTreeFilters) => return self.tree_menu_message(message),
            Message::MenuChoice(message) => self.closing_menu(message),
            Message::MenuFullscreen(tab) => return self.menu_fullscreen(tab),
            Message::BrowseKeyFile => return pick_key_file(),
            Message::CopyError(tab) => return self.copy_error(tab),
            Message::EditPath { tab, side } => {
                self.edit_path(tab, side);
                Vec::new()
            }
            message @ (Message::TreeClick(_)
            | Message::ContentFocus
            | Message::TreeShortcut(_)
            | Message::TreeHover(_)
            | Message::TreeHoverLeft(_)
            | Message::TreeDragMoved(_)
            | Message::TreeDragEnd
            | Message::MenuKey
            | Message::TypeAhead(_)
            | Message::SidebarDragStart
            | Message::WindowResized(_)
            | Message::SidebarDragged(_)
            | Message::SidebarDragEnd) => self.tree_input(message),
            Message::GatewayReassignPicked { missing, to } => {
                self.gateway_reassign.insert(missing, to);
                Vec::new()
            }
            message @ (Message::FilesHover(_)
            | Message::FilesHoverLeft(_)
            | Message::PointerPressed
            | Message::FilesDragMoved(_)
            | Message::FilesDragEnd) => self.files_drag_message(&message),
            message @ (Message::PaletteQuery(_)
            | Message::PaletteChoose(_)
            | Message::PaletteClose) => self.palette_message(message),
            message @ (Message::FinderQuery(_) | Message::FinderFind(_) | Message::FinderClose) => {
                self.finder_message(message)
            }
            message @ (Message::LogDirectoryEdited(_)
            | Message::LogDirectoryApply
            | Message::EditorEdited(_)
            | Message::PresetsEdited(_)
            | Message::EditorApply
            | Message::FontSizeEdited(_)
            | Message::FontSizeApply
            | Message::SessionFieldEdited(..)
            | Message::SessionFieldApply(_)) => self.settings_field_message(message),
            message @ (Message::FilesHovered(_) | Message::FileDropped(_)) => {
                self.drop_message(message)
            }
            Message::OpenWithSystem(target) => return open_with_system(target),
            Message::NewNote { id, template } => {
                self.menu = None;
                return self.new_note(&id, template);
            }
        };
        let mut tasks: Vec<Task<Message>> =
            effects.into_iter().map(|effect| self.run(effect)).collect();
        // sudo's question gone, answered or not: what was typed for it goes too.
        if !matches!(self.app.dialog, Some(Dialog::SudoPassword { .. }))
            && !self.sudo_password.is_empty()
        {
            self.sudo_password = Zeroizing::default();
        }
        self.forget_finished();
        // The previous run's sessions, offered once nothing else is asked and the window is
        // open to the user.
        if !self.gated() {
            self.app.offer_restore();
            // The servers' first background check, once their tree can be seen.
            for effect in self.app.start_reachability() {
                tasks.push(self.run(effect));
            }
        }
        // The texts of editors closed, with their tab or not, go.
        self.editors.prune(&self.app);
        // What the detail panel says of the session selected, read again once it changed.
        self.app.refresh_detail();
        // The diagnostics log as the settings say now.
        crate::logging::set_enabled(self.app.settings().diagnostics_log);
        // The language the settings name now: an import may have changed it.
        let language = self.app.settings().language;
        if language != self.language_shown {
            if language.is_some() {
                crate::i18n::apply(language);
            }
            self.language_shown = language;
        }
        // A reset, or presets that could not be saved, shown again in their box.
        self.presets
            .sync(&self.app.settings().rdp_resolution_presets);
        tasks.push(self.focus_question());
        tasks.push(self.focus_dialog());
        if let Some(field) = self.focus_next.take() {
            tasks.push(operation::focus(field));
        }
        if reveal {
            tasks.push(self.reveal_selection());
        }
        Task::batch(tasks)
    }

    /// Something typed into a field: a question's, the vault's, a password, the search; or
    /// the keyboard moved to one.
    fn input_message(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Field {
                question,
                index,
                value,
            } => self.edit(question, index, value),
            Message::FocusField { question, index } => {
                return operation::focus(field_id(question, index));
            }
            Message::VaultField { index, value } => {
                if let Some(field) = self.vault_fields.get_mut(index) {
                    *field = Zeroizing::new(value);
                }
            }
            Message::FocusVaultField(index) => return operation::focus(vault_field_id(index)),
            Message::Search(term) => self.search = term,
            Message::SettingsTab(tab) => self.settings_tab = tab,
            Message::ProfilePassword(value) => self.profile_password = Zeroizing::new(value),
            Message::SudoPasswordEdited(value) => self.sudo_password = Zeroizing::new(value),
            Message::SudoPasswordConfirm => {
                let typed = std::mem::take(&mut self.sudo_password);
                let Some(Dialog::SudoPassword { tab, .. }) = self.app.dialog else {
                    return Task::none();
                };
                let password = heimdall_app::sudo_edit::SudoPassword::new(&typed);
                let effects = self
                    .app
                    .update(AppMessage::Files(FilesMessage::SudoPasswordGiven {
                        tab,
                        password,
                    }));
                return Task::batch(effects.into_iter().map(|effect| self.run(effect)));
            }
            Message::ProviderUnlock(value) => self.provider_unlock = Zeroizing::new(value),
            Message::GatewayPassword(value) => {
                self.gateway_password = Zeroizing::new(value);
                // What the route test found was signed in with another password.
                let _ = self.app.update(AppMessage::ForgetRouteTest);
            }
            Message::ProfilePassphrase(value) => self.profile_passphrase = Zeroizing::new(value),
            Message::GatewayPassphrase(value) => {
                self.gateway_passphrase = Zeroizing::new(value);
                let _ = self.app.update(AppMessage::ForgetRouteTest);
            }
            Message::CopyRouteReport => {
                if let Some(Dialog::EditGateway { draft, .. }) = &self.app.dialog
                    && let Some(report) = crate::route_test_view::finished_report(draft)
                {
                    return iced::clipboard::write(report);
                }
            }
            _ => {}
        }
        Task::none()
    }

    /// Applies a message about what the window shows: the settings, full screen, how a
    /// desktop is drawn.
    fn view_message(&mut self, message: &Message) -> Task<Message> {
        match message {
            Message::DesktopFit { tab, fit } => {
                // Kept for the size the desktop has now: another chosen later makes it stale.
                let fixed = self
                    .app
                    .tab(*tab)
                    .and_then(|found| found.desktop.as_deref())
                    .and_then(DesktopPane::fixed_size);
                self.desktop_fit.insert(*tab, (*fit, fixed));
                Task::none()
            }
            Message::WindowOpened(id) => window::scale_factor(*id).map(Message::Rescaled),
            Message::Rescaled(scale) => {
                self.density = *scale;
                // RDP desktops opened from now on ask for this scale.
                let _ = self.app.update(AppMessage::DisplayScale(*scale));
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
            Message::Modifiers(modifiers) => {
                self.modifiers = *modifiers;
                Task::none()
            }
            Message::Navigate(destination) => {
                self.menu = None;
                match destination {
                    Destination::Sessions => self.page = Page::Tab,
                    Destination::Tunnels => self.page = Page::Tunnels,
                    Destination::About => self.page = Page::About,
                    Destination::Settings => return self.view_message(&Message::ShowSettings),
                }
                Task::none()
            }
            Message::ShowSettings => {
                self.menu = None;
                self.page = Page::Settings {
                    over: self.app.active,
                };
                // The trusted keys as they are now: another program may have changed them.
                let _ = self
                    .app
                    .update(AppMessage::Settings(SettingsMessage::TrustedKeys(
                        TrustedKeysMessage::Refresh,
                    )));
                Task::none()
            }
            Message::LanguageChosen(language) => {
                crate::i18n::apply(Some(*language));
                let _ = self
                    .app
                    .update(AppMessage::Settings(SettingsMessage::Language(*language)));
                Task::none()
            }
            Message::TrustedSearch(list, typed) => {
                match list {
                    TrustedList::HostKeys => typed.clone_into(&mut self.host_key_search),
                    TrustedList::Certificates => typed.clone_into(&mut self.certificate_search),
                }
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

    /// Closes the open menu, then hands `message` to the core.
    fn closing_menu(&mut self, message: AppMessage) -> Vec<Effect> {
        self.menu = None;
        // Revealed in the tree: the tree takes the keyboard, its search cleared so the
        // profile shows.
        if matches!(
            message,
            AppMessage::TabMenu(heimdall_app::TabMenuMessage::RevealInTree(_))
        ) {
            self.search.clear();
            self.tree_focused = true;
        }
        self.app.update(message)
    }

    /// Hands the gateway dialog to the core with the password and passphrase typed, which
    /// leave the window.
    /// Hands "Test route" to the core with copies of the secrets typed in the gateway
    /// dialog: they stay in its fields, to be saved or tested again.
    fn test_route_form(&mut self) -> Vec<Effect> {
        let copy = |field: &str| (!field.is_empty()).then(|| Secret::new(field.to_owned()));
        let password = copy(&self.gateway_password);
        let passphrase = copy(&self.gateway_passphrase);
        self.app.update(AppMessage::TestRoute {
            password,
            passphrase,
        })
    }

    fn save_gateway_form(&mut self) -> Vec<Effect> {
        let password = typed_secret(&mut self.gateway_password);
        let passphrase = typed_secret(&mut self.gateway_passphrase);
        self.app.update(AppMessage::SaveGateway {
            password,
            passphrase,
        })
    }

    /// Hands the profile form to the core with the password and passphrase typed, which
    /// leave the window.
    fn save_profile_form(&mut self) -> Vec<Effect> {
        let password = typed_secret(&mut self.profile_password);
        let passphrase = typed_secret(&mut self.profile_passphrase);
        self.app.update(AppMessage::SaveProfile {
            password,
            passphrase,
        })
    }

    /// What the dialogs show that the window holds: typed secrets, and where passwords go.
    /// The dialogs' inputs, for a window `height` high.
    fn forms(&self, height: f32) -> Forms<'_> {
        Forms {
            fields_height: (height - DIALOG_RESERVED_HEIGHT).max(0.0),
            vault: &self.vault_fields,
            profile_password: &self.profile_password,
            sudo_password: &self.sudo_password,
            profile_passphrase: &self.profile_passphrase,
            gateway_password: &self.gateway_password,
            gateway_passphrase: &self.gateway_passphrase,
            gateways: self.app.gateways(),
            tunnel_problem: self.app.tunnel_problem(),
            agent_chip: self.app.agent_chip(),
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

    /// Hands what the PIN dialog holds to the core; the fields are emptied either way, as
    /// the C# dialog empties them after each try.
    fn submit_pin(&mut self) -> Vec<Effect> {
        let Some(Dialog::Pin(dialog)) = &self.app.dialog else {
            return Vec::new();
        };
        let mode = dialog.mode.clone();
        let [first, second, third] = std::mem::take(&mut self.vault_fields);
        let secret = |mut text: Zeroizing<String>| Secret::new(std::mem::take(&mut *text));
        let message = match mode {
            PinMode::Start { .. } => PinMessage::Submit(secret(first)),
            PinMode::Setup { current: true } => PinMessage::Save {
                current: secret(first),
                new: secret(second),
                confirm: secret(third),
            },
            PinMode::Setup { current: false } => PinMessage::Save {
                current: Secret::new(String::new()),
                new: secret(first),
                confirm: secret(second),
            },
        };
        self.app.update(AppMessage::Pin(message))
    }

    /// Hands the current PIN typed to remove it; only its field is emptied, as in C#.
    fn remove_pin(&mut self) -> Vec<Effect> {
        let mut current = std::mem::take(&mut self.vault_fields[0]);
        let current = Secret::new(std::mem::take(&mut *current));
        self.app
            .update(AppMessage::Pin(PinMessage::Remove(current)))
    }

    /// Hands the credential provider's unlock secret typed to the core; the field empties.
    fn save_provider_unlock(&mut self) -> Vec<Effect> {
        let mut typed = std::mem::take(&mut self.provider_unlock);
        let secret = Secret::new(std::mem::take(&mut *typed));
        self.app.update(AppMessage::CredentialProvider(
            ProviderMessage::SaveUnlockSecret(secret),
        ))
    }

    /// Whether the window is behind a gate: the lock screen, or the PIN asked at start.
    /// Nothing of it is drawn, and its keys do nothing.
    fn gated(&self) -> bool {
        self.app.is_locked() || self.app.pin_asked()
    }

    /// Ctrl+Shift+S: the session shown copied to the clipboard as an image, as the C#
    /// Heimdall does, and said; nothing without a session shown or over a dialog.
    fn screenshot(&self) -> Task<Message> {
        if self.app.active.is_none() || self.app.dialog.is_some() || self.settings_shown() {
            return Task::none();
        }
        crate::screenshot::copy_session()
            .map(|copied| Message::App(AppMessage::ScreenshotTaken { copied }))
    }

    fn shortcut(&mut self, shortcut: WindowShortcut) -> Vec<Effect> {
        if shortcut == WindowShortcut::Help {
            self.menu = None;
            return self.app.update(AppMessage::ShowShortcuts);
        }
        if shortcut == WindowShortcut::Settings {
            // Ctrl+, as the C#, with or without a tab; not over a dialog, which has the keyboard.
            if self.app.dialog.is_none() {
                let _ = self.view_message(&Message::ShowSettings);
            }
            return Vec::new();
        }
        let Some(active) = self.app.active else {
            return Vec::new();
        };
        let count = self.app.tabs.len();
        let index = self.app.tabs.iter().position(|tab| tab.id == active);
        let message = match (shortcut, index) {
            (WindowShortcut::Zoom(zoom), _) => {
                self.zoom(active, zoom);
                return Vec::new();
            }
            (WindowShortcut::Broadcast, _) => AppMessage::Broadcast(BroadcastMessage::Toggle),
            (WindowShortcut::Find, _) => {
                self.toggle_finder(active);
                return Vec::new();
            }
            (WindowShortcut::CloseTab, _) => AppMessage::RequestCloseTab(active),
            (WindowShortcut::NextTab, Some(index)) => {
                AppMessage::SelectTab(self.app.tabs[(index + 1) % count].id)
            }
            (WindowShortcut::PreviousTab, Some(index)) => {
                AppMessage::SelectTab(self.app.tabs[(index + count - 1) % count].id)
            }
            // Settings and the help: shown above, tab or no tab; a screenshot is taken by
            // the window.
            (WindowShortcut::Settings | WindowShortcut::Help | WindowShortcut::Screenshot, _)
            | (_, None) => {
                return Vec::new();
            }
        };
        self.app.update(message)
    }

    /// Opens the search bar over `tab`'s terminal, or closes it when open; only a tab
    /// showing a terminal has one.
    fn toggle_finder(&mut self, tab: TabId) {
        if self.finder.take().is_some() {
            return;
        }
        if self.app.tab(tab).is_some_and(shows_terminal) {
            self.finder = Some(Finder::new(tab));
            self.focus_next = Some(crate::finder::field_id());
        }
    }

    /// A change in the terminal's search bar: its text, a search, closed.
    fn finder_message(&mut self, message: Message) -> Vec<Effect> {
        let Some(finder) = self.finder.as_mut() else {
            return Vec::new();
        };
        match message {
            Message::FinderQuery(query) => finder.query = query,
            Message::FinderFind(direction) => {
                finder.searched = Some(finder.query.clone());
                let (tab, query) = (finder.tab, finder.query.clone());
                return self.app.update(AppMessage::FindInTerminal {
                    tab,
                    query,
                    direction,
                });
            }
            _ => self.finder = None,
        }
        Vec::new()
    }

    /// The search bar over the terminal of `tab`, when open there.
    fn finder_of(&self, tab: &Tab) -> Option<&Finder> {
        self.finder.as_ref().filter(|finder| finder.tab == tab.id)
    }

    /// `tab`'s terminal, with its search bar over it when open. Under the bar the terminal
    /// takes no keys: Escape and what is typed are the bar's.
    /// A shell tab's page: its terminal, and its server health panel beside it when shown.
    fn shell_page<'a>(&'a self, tab: &'a Tab) -> Element<'a, Message> {
        let terminal =
            self.searchable_terminal(tab, self.app.dialog.is_none() && !self.tree_focused);
        if tab.health.shown {
            row![
                container(terminal).width(Length::Fill),
                crate::health_view::view(&tab.health)
            ]
            .into()
        } else {
            terminal
        }
    }

    fn searchable_terminal<'a>(&'a self, tab: &'a Tab, interactive: bool) -> Element<'a, Message> {
        let finder = self.finder_of(tab);
        let shown = terminal(tab, interactive && finder.is_none(), self.font_size(tab.id));
        match finder {
            Some(finder) => stack![
                shown,
                crate::finder::view(finder, tab.find_found, self.modifiers.shift())
            ]
            .into(),
            None => shown,
        }
    }

    /// The text size of `tab`'s terminal: zoomed, or the one the settings give, drawn within
    /// the bounds the C# terminal draws at.
    #[must_use]
    pub fn font_size(&self, tab: TabId) -> f32 {
        self.font_sizes
            .get(&tab)
            .copied()
            .unwrap_or_else(|| f32::from(self.app.settings().terminal_font_size))
            .clamp(MIN_FONT_SIZE, MAX_FONT_SIZE)
    }

    /// Makes `tab`'s terminal text a point larger or smaller, within the C# bounds, or
    /// back to its size.
    fn zoom(&mut self, tab: TabId, zoom: Zoom) {
        // Tabs closed since leave their sizes behind no longer.
        self.font_sizes.retain(|id, _| self.app.tab(*id).is_some());
        let size = match zoom {
            Zoom::Reset => {
                self.font_sizes.remove(&tab);
                return;
            }
            Zoom::In => self.font_size(tab) + 1.0,
            Zoom::Out => self.font_size(tab) - 1.0,
        };
        self.font_sizes
            .insert(tab, size.clamp(MIN_FONT_SIZE, MAX_FONT_SIZE));
    }

    /// Opens `menu` at the pointer, or, for a sub-menu, where its menu was.
    fn open_tree_menu(&mut self, menu: TreeMenu) {
        let at = match (&menu, &self.menu) {
            (
                TreeMenu::ConnectAs(_)
                | TreeMenu::MoveFolder(_)
                | TreeMenu::FolderColor(_)
                | TreeMenu::MoveProfile(_)
                | TreeMenu::MoveSelection
                | TreeMenu::EditSelection
                | TreeMenu::GatewaySelection,
                Some((_, at)),
            ) => *at,
            _ => self.cursor.get(),
        };
        // A right click on one of the profiles selected together is theirs, as in C#.
        let menu = match menu {
            TreeMenu::Profile(id) if self.app.selected_profiles().contains(&id) => {
                TreeMenu::Selection
            }
            menu => menu,
        };
        // As in the C# tree: a right click selects the row it is on.
        if let TreeMenu::Profile(id) = &menu {
            let _ = self.app.update(AppMessage::SelectProfile(id.clone()));
        }
        // And in the C# Files tab; a folder already selected stays closed.
        if let TreeMenu::FilesEntry {
            tab,
            side,
            index: Some(index),
        } = menu
        {
            let selected = self
                .app
                .tab(tab)
                .and_then(|found| found.files.as_deref())
                .and_then(|files| match side {
                    Side::Remote => files.remote.selected,
                    Side::Local => files.local.selected,
                });
            if selected != Some(index) {
                let _ =
                    self.app
                        .update(AppMessage::Files(FilesMessage::Select { tab, side, index }));
            }
        }
        self.menu = Some((menu, at));
    }

    /// Whether Escape leaves full screen: in it, with no dialog, menu, Quick Connect, search
    /// bar or path bar for Escape to close first.
    fn escape_leaves_fullscreen(&self) -> bool {
        self.fullscreen
            && self.app.dialog.is_none()
            && self.palette.is_none()
            && self.finder.is_none()
            && self.menu.is_none()
            && self.path_editing.is_none()
    }

    /// Whether the page shown is a remote desktop under its bar, whose bar has its own way
    /// out of full screen.
    fn desktop_bar_shown(&self) -> bool {
        !self.settings_shown()
            && self.app.active_tab().is_some_and(|tab| {
                tab.prompts.is_empty()
                    && matches!(tab.phase, Phase::Connected)
                    && tab.files.is_none()
                    && tab.desktop.is_some()
            })
    }

    /// Enter confirms the open dialog, Escape dismisses it. Without a dialog, Enter opens
    /// the selection of a Files tab, and the core ignores the rest.
    fn dialog_key(&mut self, confirm: bool) -> Vec<Effect> {
        if !confirm && self.palette.take().is_some() {
            // Escape closes Quick Connect first.
            return Vec::new();
        }
        if !confirm && self.finder.take().is_some() {
            // Then the terminal's search bar.
            return Vec::new();
        }
        if !confirm && self.menu.take().is_some() {
            // Escape closes the open menu first.
            return Vec::new();
        }
        if !confirm
            && self.app.dialog.is_none()
            && let Some((tab, side)) = self.path_editing.take()
        {
            // Then a path bar typed in, back to the folder shown, as the C# one.
            return self
                .app
                .update(AppMessage::Files(FilesMessage::PathCancelled { tab, side }));
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
    /// The pane that has the keyboard in the Files tab shown, its lists in sight: none for
    /// another tab, the settings, a dialog or the integrated editor.
    fn shown_files_side(&self) -> Option<heimdall_app::files::Side> {
        if self.settings_shown() || self.app.dialog.is_some() || self.tree_focused {
            return None;
        }
        let files = self.app.active_tab()?.files.as_deref()?;
        files.editor.is_none().then_some(files.focus)
    }

    /// The path bar of `side` in `tab` given the keyboard, its path shown to type in.
    fn edit_path(&mut self, tab: TabId, side: heimdall_app::files::Side) {
        self.path_editing = Some((tab, side));
        self.focus_next = Some(files_view::field_id(side, files_view::PaneField::Path));
    }

    fn files_key(&mut self, key: FilesKey) -> Vec<Effect> {
        if key == FilesKey::FocusPath {
            if let (Some(side), Some(tab)) = (
                self.shown_files_side(),
                self.app.active_tab().map(|tab| tab.id),
            ) {
                self.edit_path(tab, side);
            }
            return Vec::new();
        }
        // Quick Connect's first, while open.
        if let Some(effects) = self.palette_key(key) {
            return effects;
        }
        // The tree's first, while it has the keyboard.
        if let Some(effects) = self.tree_key(key) {
            return effects;
        }
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
        if !matches!(app.dialog, Some(Dialog::Vault(_) | Dialog::Pin(_))) {
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
            self.profile_passphrase = Zeroizing::default();
        }
        if !matches!(app.dialog, Some(Dialog::EditGateway { .. })) {
            self.gateway_password = Zeroizing::default();
            self.gateway_passphrase = Zeroizing::default();
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
            Some(
                Dialog::AskName { .. }
                | Dialog::SaveMacro { .. }
                | Dialog::RenameTab { .. }
                | Dialog::CustomResolution { .. }
                | Dialog::FolderName { .. }
                | Dialog::RenameProfile { .. }
                | Dialog::BulkEdit { .. },
            ) => (Some(DialogFocus::Name), name_field_id()),
            Some(Dialog::Vault(_)) => (Some(DialogFocus::Vault), vault_field_id(0)),
            Some(Dialog::Pin(_)) => (Some(DialogFocus::Pin), vault_field_id(0)),
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

    /// Starts the attempt a connection effect asks for; its events come back as messages.
    fn start_attempt(&mut self, effect: Effect) -> Task<Message> {
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
            Effect::ConnectFtp {
                tab,
                attempt,
                request,
            } => {
                let registry = self.registry.clone();
                let events = stream::once(async move { ftp_events(*request, registry) }).flatten();
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
            Effect::ConnectWinRm {
                tab,
                attempt,
                request,
            } => {
                let registry = self.registry.clone();
                let events =
                    stream::once(async move { winrm_events(*request, registry) }).flatten();
                self.connection_task(tab, attempt, events)
            }
            _ => Task::none(),
        }
    }

    /// Turns an effect into a task.
    #[expect(clippy::too_many_lines, reason = "one arm per effect")]
    fn run(&mut self, effect: Effect) -> Task<Message> {
        match effect {
            effect @ (Effect::Connect { .. }
            | Effect::ConnectRdp { .. }
            | Effect::ConnectTelnet { .. }
            | Effect::ConnectVnc { .. }
            | Effect::ConnectFtp { .. }
            | Effect::ConnectLocal { .. }
            | Effect::ConnectWinRm { .. }) => self.start_attempt(effect),
            Effect::TestRoute { run, request } => route_test_task(run, *request),
            Effect::PlayMacro { tab, run } => Task::perform(run, move |outcome| {
                Message::App(AppMessage::Macro(heimdall_app::MacroMessage::Finished {
                    tab,
                    outcome,
                }))
            }),
            Effect::CheckReachability {
                probes,
                timeout,
                at_once,
            } => {
                let checks = stream::iter(probes)
                    .map(move |probe| async move {
                        let verdict =
                            heimdall_app::reachability::check(probe.host, probe.port, timeout)
                                .await;
                        Message::App(AppMessage::ReachabilityChecked {
                            id: probe.id,
                            verdict,
                        })
                    })
                    .buffer_unordered(at_once);
                Task::stream(checks)
            }
            effect @ (Effect::TestReachability { .. }
            | Effect::WakeOnLan(_)
            | Effect::TestAddress { .. }
            | Effect::SurveyAgents(_)) => probe_task(effect),
            Effect::OpenTunnel { id, request } => {
                // Its end is the tunnel's own: closing it cancels the attempt, which ends
                // the stream.
                let registry = self.registry.clone();
                // Started once the task runs, on the runtime, as a connection's.
                let events =
                    stream::once(async move { tunnel_events(*request, registry) }).flatten();
                Task::stream(events).map(move |event| {
                    Message::App(AppMessage::Tunnel(TunnelMessage::Event { id, event }))
                })
            }
            Effect::Answer { question, answer } => {
                if !self.registry.answer(question, answer) {
                    log::debug!("question {} was no longer waiting", question.value());
                }
                Task::none()
            }
            effect @ (Effect::ListRemote { .. }
            | Effect::ListLocal { .. }
            | Effect::PlanTransfer { .. }
            | Effect::Transfer { .. }
            | Effect::FileOperation { .. }
            | Effect::FileBatchStep { .. }
            | Effect::MoveRemote { .. }
            | Effect::CopyRemote { .. }
            | Effect::CopyAcross { .. }
            | Effect::StartEdit { .. }
            | Effect::LaunchEditor { .. }
            | Effect::CheckEdits { .. }
            | Effect::SudoOpen { .. }
            | Effect::SudoSave { .. }
            | Effect::SendEditAnyway { .. }
            | Effect::OpenFolder { .. }) => files_task(effect),
            effect @ (Effect::OpenEditor { .. } | Effect::SaveEditor { .. }) => {
                crate::integrated_editor::task(effect)
            }
            Effect::ReadHealth { tab, connection } => Task::perform(
                heimdall_app::server_health::collect(connection),
                move |health| {
                    Message::App(AppMessage::HealthRead {
                        tab,
                        health: Box::new(health),
                    })
                },
            ),
            Effect::WriteClipboard(content) => iced::clipboard::write(content),
            Effect::OpenUrl(url) => Task::future(async move {
                let opened =
                    tokio::task::spawn_blocking(move || heimdall_app::external_url::open_url(&url))
                        .await;
                if let Ok(Err(error)) = opened {
                    log::warn!("the browser did not start: {error}");
                }
            })
            .discard(),
            Effect::WriteClipboardImage(image) => Task::future(async move {
                let _ = tokio::task::spawn_blocking(move || write_clipboard_image(&image)).await;
            })
            .discard(),
            Effect::SaveExport { document, count } => save_export(document, count),
            Effect::PickOpenSshConfig => pick_openssh(),
            Effect::PickRdpFiles => pick_rdp(),
            Effect::PickUploads { tab } => pick_uploads(tab),
            Effect::ReadExplorerFiles { tab } => read_explorer_files(tab),
            Effect::ReadDesktopClipboard { tab } => read_desktop_clipboard(tab),
            Effect::PickSaveFolder { tab } => pick_save_folder(tab),
            Effect::PickSessionsFile => pick_sessions_file(),
            Effect::SaveSettingsFile { document } => crate::settings_file::save(document),
            Effect::PickSettingsFile => crate::settings_file::pick(),
            Effect::PickKnownHosts => pick_known_hosts(),
            // The registry or the files, read off the window's thread.
            Effect::ReadPuttySessions => Task::perform(
                async {
                    tokio::task::spawn_blocking(heimdall_app::putty_store::read)
                        .await
                        .unwrap_or_else(|error| Err(error.to_string()))
                },
                |read| {
                    Message::App(AppMessage::Sessions(
                        heimdall_app::SessionsMessage::PuttyRead(read),
                    ))
                },
            ),
            Effect::ReadRdpFiles(paths) => {
                Task::perform(crate::rdp_view::read_all(paths), rdp_read)
            }
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
            Effect::AskCredentialProvider(request) => Task::perform(request.run(), |answer| {
                Message::App(AppMessage::CredentialProvided(Box::new(answer)))
            }),
            Effect::TestCredentialProvider { settings, unlock } => Task::perform(
                heimdall_app::credential_provider::test(settings, unlock),
                |outcome| {
                    Message::App(AppMessage::CredentialProvider(ProviderMessage::Tested(
                        outcome,
                    )))
                },
            ),
            Effect::Exit => self.exit(),
        }
    }

    /// Keeps how the window is left, then closes it. Maximized, minimized or full screen,
    /// its place and size are the ones it had before: the ones kept when it was last shown
    /// as a window.
    fn exit(&self) -> Task<Message> {
        let Some((path, left)) = self.window_memory.clone() else {
            return iced::exit();
        };
        let leaving = heimdall_core::window_state::WindowState {
            sidebar_width: Some(self.sidebar_width),
            sidebar_hidden: self.sidebar_hidden,
            folded: self.app.folded_folders(),
            selected: self
                .app
                .selected_profile
                .as_ref()
                .map(|id| id.as_str().to_owned()),
            ..left
        };
        let (size, fullscreen) = (self.window_size, self.fullscreen);
        let keep = move |maximized: bool, place: Option<((i32, i32), f32)>| {
            let mut state = leaving.clone();
            state.maximized = maximized;
            if let (false, Some(size)) = (maximized, size) {
                state.width = Some(size.width);
                state.height = Some(size.height);
            }
            if let Some(((x, y), scale)) = place {
                (state.x, state.y, state.scale) = (Some(x), Some(y), Some(scale));
            }
            if let Err(error) = heimdall_core::window_state::save(&path, &state) {
                log::warn!("the window's state could not be kept: {error}");
            }
            iced::exit()
        };
        window::latest().then(move |id| {
            let Some(id) = id else {
                return keep(false, None);
            };
            let keep = keep.clone();
            window::is_maximized(id).then(move |maximized| {
                let keep = keep.clone();
                if maximized || fullscreen {
                    return keep(maximized, None);
                }
                // Minimized, the system reports a place off every screen.
                window::is_minimized(id).then(move |minimized| {
                    let keep = keep.clone();
                    if minimized == Some(true) {
                        return keep(false, None);
                    }
                    window::position(id).then(move |position| {
                        let keep = keep.clone();
                        window::scale_factor(id).then(move |scale| {
                            keep(
                                false,
                                position.map(|position| {
                                    (crate::screens::physical(position, scale), scale)
                                }),
                            )
                        })
                    })
                })
            })
        })
    }

    /// Draws the window.
    #[must_use]
    pub fn view(&self) -> Element<'_, Message> {
        let locked = self.gated();
        // Locked, the window is not drawn: nothing of it shows, and no hidden field takes
        // what is typed. Its sessions go on.
        let body: Element<'_, Message> = if locked {
            // The whole window: the stack takes its size from it, and the dialog its own.
            iced::widget::space()
                .width(Length::Fill)
                .height(Length::Fill)
                .into()
        } else if self.fullscreen {
            // Full screen is the session's: no tree, no tabs.
            self.content()
        } else {
            column![self.navigation(), self.shown_page(), self.status_bar()].into()
        };
        // Always a stack with the window first: a tree of one shape keeps the state of the
        // widgets under a dialog, such as how far a list is scrolled.
        let mut layers = stack![body];
        // Full screen, a way out the mouse finds, as the C# floating button; a desktop's bar
        // has its own.
        if self.fullscreen && !locked && !self.desktop_bar_shown() {
            layers = layers.push(
                container(
                    tooltip(
                        button(text(fl!("ui-fullscreen-exit")).size(SMALL_SIZE))
                            .style(button::secondary)
                            .on_press(Message::ToggleFullscreen),
                        text(fl!("ui-fullscreen-exit-tooltip")).size(SMALL_SIZE),
                        tooltip::Position::Left,
                    )
                    .style(container::rounded_box),
                )
                .padding(PADDING)
                .width(Length::Fill)
                .align_x(iced::alignment::Horizontal::Right),
            );
        }
        if let Some(dialog) = &self.app.dialog {
            // Built for the window's height: a long form scrolls above its buttons.
            layers = layers.push(opaque(
                container(responsive(move |size| {
                    let content = dialog_view(dialog, &self.forms(size.height));
                    // The OpenSSH preview is a table: wider than a form, as the C# one.
                    let card = if matches!(
                        dialog,
                        Dialog::SessionsPreview(_)
                            | Dialog::RdpPreview(_)
                            | Dialog::HostKeysPreview(_)
                            | Dialog::FileConflicts { .. }
                    ) {
                        wide_card(content)
                    } else {
                        card(content)
                    };
                    center(card).into()
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
            .and_then(|(menu, at)| Some((self.open_menu_entries(menu)?, *at)));
        if let Some(palette) = self.palette.as_ref().filter(|_| !locked) {
            let results = self.app.quick_results(&palette.query);
            // At the top, as the C# palette; a click beside it closes it.
            layers = layers.push(opaque(
                mouse_area(
                    container(crate::palette::view(palette, &results))
                        .center_x(Length::Fill)
                        .padding(PALETTE_TOP)
                        .height(Length::Fill),
                )
                .on_press(Message::PaletteClose),
            ));
        }
        if let Some(overlay) = self.drop_overlay() {
            layers = layers.push(overlay);
        }
        if let Some((entries, at)) = open_menu {
            // A tunnel row is at the window's foot: its menu opens above the cursor.
            let y = if matches!(self.menu, Some((TreeMenu::Tunnel(_), _))) {
                (at.y - tree_view::TUNNEL_MENU_HEIGHT).max(0.0)
            } else {
                at.y
            };
            // Opaque: what is under the menu is neither hovered nor clicked.
            layers = layers.push(opaque(
                mouse_area(
                    pin(entries)
                        .x(at.x)
                        .y(y)
                        .width(Length::Fill)
                        .height(Length::Fill),
                )
                .on_press(Message::CloseTreeMenu)
                .on_right_press(Message::CloseTreeMenu),
            ));
        }
        CursorTracker::new(layers, self.cursor.clone()).into()
    }

    /// A terminal tab's Macros menu; `None` once the tab takes none.
    fn macros_menu(&self, tab: TabId) -> Option<Element<'_, Message>> {
        let tab = self.app.tab(tab)?;
        Some(tree_view::macro_entries(tab.id, &self.app.macro_menu(tab)?))
    }

    /// A note about the session `id`, written from `template` now, then opened in the editor
    /// set; the day's note, written already, opened again.
    fn new_note(
        &self,
        id: &ProfileId,
        template: heimdall_app::notes::NoteTemplate,
    ) -> Task<Message> {
        let Some(context) = self.app.profile_summary_note(id) else {
            return Task::none();
        };
        let draft = heimdall_app::notes::draft(
            template,
            &context,
            heimdall_app::notes::LocalTime::now(),
            &note_labels(),
        );
        Task::perform(
            heimdall_app::notes::open(
                self.app.notes_dir(),
                draft,
                self.app.settings().external_editor.clone(),
            ),
            |opened| Message::App(AppMessage::NoteOpened(opened)),
        )
    }

    /// The entries of `menu`, the open one; `None` once what it is for is gone.
    #[expect(clippy::too_many_lines, reason = "one arm per menu")]
    fn open_menu_entries(&self, menu: &TreeMenu) -> Option<Element<'_, Message>> {
        let entries = if let TreeMenu::Tab(tab) = menu {
            tree_view::tab_menu_entries(&self.tab_menu_state(*tab)?)
        } else if let TreeMenu::FilesBookmarks(tab) | TreeMenu::FilesBookmarksRemove(tab) = *menu {
            let files = self.app.tab(tab)?.files.as_deref()?;
            let shown: Vec<String> = files
                .bookmarks
                .iter()
                .map(|path| heimdall_app::server_text(&path.display()))
                .collect();
            if matches!(menu, TreeMenu::FilesBookmarksRemove(_)) {
                tree_view::files_bookmarks_remove_menu(tab, &shown)
            } else {
                tree_view::files_bookmarks_menu(tab, &shown)
            }
        } else if let TreeMenu::Tunnel(id) = *menu {
            // Only while the tunnel is listed.
            tree_view::tunnel_menu_entries(id, self.app.tunnel(id)?.interrupted)
        } else if let TreeMenu::Macros(tab) = *menu {
            self.macros_menu(tab)?
        } else if let TreeMenu::Notes(id) = menu {
            // Only while the session is saved.
            self.app.profile_summary(id)?;
            tree_view::notes_entries(id)
        } else if let TreeMenu::Resolution(tab) = *menu {
            // Only while its desktop is shown.
            tree_view::resolution_entries(
                &self.resolution_state(self.app.tab(tab)?)?,
                self.app.settings().resolution_presets(),
            )
        } else if let TreeMenu::FilesEntry { tab, side, index } = *menu {
            let files = self.app.tab(tab)?.files.as_deref()?;
            let (kinds, chosen): (Vec<EntryKind>, usize) = match side {
                Side::Remote => (
                    files
                        .remote
                        .entries
                        .iter()
                        .map(|entry| entry.kind)
                        .collect(),
                    files.remote.chosen().len(),
                ),
                Side::Local => (
                    files.local.entries.iter().map(|entry| entry.kind).collect(),
                    files.local.chosen().len(),
                ),
            };
            let facts = match index {
                // Only while the entry is still listed.
                Some(index) => {
                    let kind = *kinds.get(index)?;
                    Some(tree_view::FilesEntryFacts {
                        index,
                        single: chosen <= 1,
                        one_file: chosen <= 1 && kind == EntryKind::File,
                        link: kind == EntryKind::Link,
                    })
                }
                None => None,
            };
            tree_view::files_entry_menu(
                (tab, side),
                facts,
                self.app.can_paste(tab),
                self.app.can_copy(tab),
            )
        } else if let TreeMenu::Folder(path) = menu {
            tree_view::folder_menu_entries(path, self.app.folder_connectable(path))
        } else if let TreeMenu::MoveFolder(path) = menu {
            tree_view::move_folder_entries(path, &self.app.folder_targets(path))
        } else if let TreeMenu::FolderColor(path) = menu {
            tree_view::folder_color_entries(path, self.app.own_folder_color(path))
        } else if let TreeMenu::MoveProfile(id) = menu {
            tree_view::move_profile_entries(id, &self.app.profile_move_targets(id))
        } else if let TreeMenu::Filter = menu {
            tree_view::filter_entries(self.app.tree_filter())
        } else if let TreeMenu::Selection = menu {
            let selected = self.app.selected_profiles();
            let connectable = selected
                .iter()
                .filter(|id| self.app.connects_in_bulk(id))
                .count();
            tree_view::selection_menu_entries(
                selected.len(),
                connectable,
                self.app.all_favorites(&selected),
            )
        } else if let TreeMenu::MoveSelection = menu {
            tree_view::move_selection_entries(&self.app.folder_paths())
        } else if let TreeMenu::EditSelection = menu {
            let selected = self.app.selected_profiles();
            tree_view::edit_selection_entries(
                self.app
                    .bulk_targets(&selected, heimdall_app::BulkField::Username),
                self.app.gateway_targets(&selected),
            )
        } else if let TreeMenu::GatewaySelection = menu {
            tree_view::gateway_selection_entries(self.app.gateways())
        } else {
            let profile = match menu {
                TreeMenu::Profile(id) | TreeMenu::ConnectAs(id) => self.app.profile_summary(id),
                TreeMenu::Add
                | TreeMenu::More
                | TreeMenu::Filter
                | TreeMenu::Tab(_)
                | TreeMenu::Folder(_)
                | TreeMenu::MoveFolder(_)
                | TreeMenu::FolderColor(_)
                | TreeMenu::MoveProfile(_)
                | TreeMenu::Selection
                | TreeMenu::MoveSelection
                | TreeMenu::EditSelection
                | TreeMenu::GatewaySelection
                | TreeMenu::FilesEntry { .. }
                | TreeMenu::FilesBookmarks(_)
                | TreeMenu::FilesBookmarksRemove(_)
                | TreeMenu::Resolution(_)
                | TreeMenu::Macros(_)
                | TreeMenu::Notes(_)
                | TreeMenu::Tunnel(_) => None,
            };
            let editable = profile.as_ref().is_some_and(|p| self.app.can_edit(&p.id));
            let connect_as = profile
                .as_ref()
                .map(|p| self.app.connect_as_choices(&p.id))
                .unwrap_or_default();
            tree_view::menu_entries(menu, profile.as_ref(), &connect_as, editable)
        };
        Some(entries)
    }

    /// No session shown, as the C# window: with no session saved, a welcome and the ways
    /// to add one; otherwise, how to open one.
    fn home(&self) -> Element<'_, Message> {
        if !self.app.profile_summaries().is_empty() {
            // The session selected, as the C# detail panel shows it.
            let selected = self
                .app
                .selected_profile
                .as_ref()
                .and_then(|id| self.app.profile_summary(id));
            return match selected {
                Some(profile) => center(crate::detail_view::view(
                    &profile,
                    self.app.selected_credentials(),
                    self.app.can_edit(&profile.id),
                ))
                .into(),
                None => center(text(fl!("ui-home-select"))).into(),
            };
        }
        center(
            column![
                text(fl!("ui-home-welcome")).size(HEADING_SIZE),
                text(fl!("ui-home-subtitle")),
                row![
                    button(text(fl!("ui-home-add-button")))
                        .on_press(Message::App(AppMessage::NewProfile)),
                    button(text(fl!("ui-home-import-button")))
                        .style(button::secondary)
                        .on_press_maybe(
                            self.app
                                .can_import()
                                .then_some(Message::App(AppMessage::ImportLegacy)),
                        ),
                ]
                .spacing(SPACING),
                text(fl!("ui-home-shortcuts")).size(SMALL_SIZE),
            ]
            .spacing(SPACING)
            .align_x(iced::Alignment::Center),
        )
        .into()
    }

    /// The page of the window's navigation shown.
    fn shown_page(&self) -> Element<'_, Message> {
        // The Sessions page, the C#'s: the tree beside the sessions; the others the
        // window's whole width, as the C# pages.
        match self.page {
            Page::Tab => row![
                (!self.sidebar_hidden).then(|| self.sidebar()),
                (!self.sidebar_hidden).then(splitter),
                column![self.tab_bar(), self.focusable_content()]
                    .push(
                        self.app
                            .tunnels_panel
                            .then(|| crate::tunnels_view::panel(&self.app.tunnels)),
                    )
                    .width(Length::Fill)
                    .height(Length::Fill)
            ]
            .height(Length::Fill)
            .into(),
            Page::Tunnels => crate::tunnels_view::page(&self.app.tunnels),
            Page::About => scrollable(
                container(crate::about_view::view(&self.app))
                    .padding(PADDING)
                    .width(Length::Fill),
            )
            .height(Length::Fill)
            .into(),
            Page::Settings { .. } => container(self.content())
                .width(Length::Fill)
                .height(Length::Fill)
                .into(),
        }
    }

    /// The window's navigation, as the C# toolbar's: the application's name, then its pages.
    fn navigation(&self) -> Element<'_, Message> {
        let shown = match self.page {
            Page::Tab => Destination::Sessions,
            Page::Tunnels => Destination::Tunnels,
            Page::About => Destination::About,
            Page::Settings { .. } => Destination::Settings,
        };
        let mut bar = row![
            container(
                text(APP_NAME)
                    .size(NAV_TITLE_SIZE)
                    .style(|theme: &Theme| text::Style {
                        color: Some(theme.extended_palette().primary.base.color),
                    })
            )
            .padding(NAV_TITLE_PADDING),
        ]
        .spacing(SPACING / 2.0)
        .align_y(iced::Alignment::Center);
        for destination in Destination::ALL {
            let active = destination == shown;
            // The page shown in the accent colour, a line under it, as the C# tabs.
            let entry = button(text(destination.label()))
                .style(move |theme: &Theme, status| {
                    let mut style = button::text(theme, status);
                    if active {
                        style.text_color = theme.extended_palette().primary.base.color;
                    }
                    style
                })
                .on_press(Message::Navigate(destination));
            let underline = container(iced::widget::space())
                .height(NAV_UNDERLINE)
                .width(Length::Fill)
                .style(move |theme: &Theme| container::Style {
                    background: active.then(|| theme.extended_palette().primary.base.color.into()),
                    ..container::Style::default()
                });
            bar = bar.push(column![entry, underline].width(Length::Shrink));
        }
        // Quick Connect at the end, as the C# toolbar's button.
        let bar = bar.push(iced::widget::space::horizontal()).push(
            tooltip(
                button(text(fl!("ui-nav-quick-connect")))
                    .style(button::secondary)
                    .on_press(Message::TreeShortcut(TreeShortcut::QuickConnect)),
                text(fl!("ui-nav-quick-connect-tooltip")).size(SMALL_SIZE),
                tooltip::Position::Bottom,
            )
            .style(container::rounded_box),
        );
        container(bar)
            .padding([SPACING / 2.0, PADDING])
            .width(Length::Fill)
            .style(container::bordered_box)
            .into()
    }

    /// The status bar: the session shown, or what was just done; the sessions counted.
    fn status_bar(&self) -> Element<'_, Message> {
        let summaries = self.app.profile_summaries();
        let shown = summaries
            .iter()
            .filter(|profile| profile.matches(&self.search))
            .count();
        let targets = self.app.broadcast_target_count();
        crate::status_bar::view(
            crate::status_bar::status_text(&self.app.session_status(), self.app.notice(), targets),
            crate::status_bar::count_text(shown, summaries.len(), !self.search.trim().is_empty()),
            row![
                crate::shortcuts_view::hint(SMALL_SIZE),
                self.tunnels_toggle(),
                self.broadcast_controls(targets)
            ]
            .align_y(iced::Alignment::Center)
            .into(),
        )
    }

    /// The tunnels panel's button, with how many tunnels are open, as the C# bar's.
    fn tunnels_toggle(&self) -> Element<'_, Message> {
        tooltip(
            button(text(fl!("ui-tunnels-count", count = self.app.live_tunnels())).size(SMALL_SIZE))
                .style(if self.app.tunnels_panel {
                    button::primary
                } else {
                    button::text
                })
                .on_press(Message::App(AppMessage::Tunnel(TunnelMessage::TogglePanel))),
            text(fl!("ui-tunnels-toggle-tooltip")).size(SMALL_SIZE),
            tooltip::Position::Top,
        )
        .style(container::rounded_box)
        .into()
    }

    /// Broadcast input's toggle and scope, as the C# bar's: lit while on.
    fn broadcast_controls(&self, targets: usize) -> Element<'_, Message> {
        let on = self.app.broadcasting();
        let scope = crate::status_bar::scope_label(self.app.settings().broadcast_scope, targets);
        let broadcast = |message| Message::App(AppMessage::Broadcast(message));
        row![
            tooltip(
                button(text(fl!("ui-broadcast-button")).size(SMALL_SIZE))
                    .style(if on { button::primary } else { button::text })
                    .on_press(broadcast(BroadcastMessage::Toggle)),
                text(if on {
                    fl!("ui-broadcast-on", scope = scope.as_str())
                } else {
                    fl!("ui-broadcast-toggle-tooltip")
                })
                .size(SMALL_SIZE),
                tooltip::Position::Top,
            )
            .style(container::rounded_box),
            tooltip(
                button(text(scope.clone()).size(SMALL_SIZE))
                    .style(button::text)
                    .on_press(broadcast(BroadcastMessage::Scope)),
                text(fl!("ui-broadcast-scope-tooltip")).size(SMALL_SIZE),
                tooltip::Position::Top,
            )
            .style(container::rounded_box),
        ]
        .align_y(iced::Alignment::Center)
        .into()
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
            tooltip(
                button(text(HIDE_SIDEBAR_GLYPH))
                    .style(button::secondary)
                    .on_press(Message::TreeShortcut(TreeShortcut::ToggleSidebar)),
                text(fl!("ui-sidebar-hide-tooltip")).size(SMALL_SIZE),
                tooltip::Position::Bottom,
            )
            .style(container::rounded_box),
        ]
        .spacing(SPACING / 2.0)
        .align_y(iced::Alignment::Center);
        let mut actions = row![
            button(text(fl!("ui-sidebar-local-shell-button")))
                .on_press(Message::App(AppMessage::OpenLocal(default_local_shell())))
                .style(button::secondary),
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
        let list = self.tree_list();
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
        .width(self.sidebar_width)
        .height(Length::Fill)
        .style(container::rounded_box)
        .into()
    }

    /// The tree's rows, searched and filtered, and what it says when none passes.
    fn tree_list(&self) -> Column<'_, Message> {
        let mut list = Column::new().spacing(2.0);
        if self.app.profile_summaries().is_empty() {
            list = list.push(text(fl!("ui-sidebar-empty")));
        }
        let rows = self.app.tree_rows(&self.search);
        let filter = self.app.tree_filter();
        let searching = !self.search.trim().is_empty() || filter.is_active();
        if rows.is_empty() && searching {
            // As the C# tree: the way back is emptying the search, or every filter with it.
            let (said, way_back, message) = if filter.is_active() {
                (
                    fl!("ui-tree-filter-no-results"),
                    fl!("ui-tree-filter-reset"),
                    Message::ResetTreeFilters,
                )
            } else {
                (
                    fl!("ui-tree-search-no-results"),
                    fl!("ui-tree-search-clear"),
                    Message::Search(String::new()),
                )
            };
            list = list.push(text(said).size(SMALL_SIZE)).push(
                button(text(way_back).size(SMALL_SIZE))
                    .style(button::secondary)
                    .on_press(message),
            );
        }
        let badge = filter.shows_gateway_badge();
        // As the C# tree: folders nested and folded, sub-folders first, "(No Folder)" last.
        list = list.extend(rows.into_iter().map(|row| match row {
            TreeRow::Folder {
                path,
                name,
                depth,
                open,
                count,
            } => {
                let color = self.app.folder_color(&path);
                let target = heimdall_app::DropTarget::Folder(path.clone());
                let selected = self.app.selected_folder.as_deref() == Some(path.as_str());
                crate::tree_drag::drop_zone(
                    tree_view::folder_row(path, name, depth, open, count, color, selected),
                    target,
                    self.tree_drag.as_ref(),
                )
            }
            TreeRow::Profile { mut profile, depth } => {
                if !badge {
                    profile.gateway = None;
                }
                let selected = self.app.is_selected(&profile.id);
                let state = self.app.profile_state(&profile.id);
                let context = searching
                    .then(|| tree_view::search_context(&profile))
                    .flatten();
                let target = heimdall_app::DropTarget::Profile(profile.id.clone());
                let reach = self.app.reachability(&profile.id).cloned();
                crate::tree_drag::drop_zone(
                    tree_view::indented(
                        tree_view::owned_row(&profile, selected, (state, reach), context),
                        depth,
                    ),
                    target,
                    self.tree_drag.as_ref(),
                )
            }
        }));
        list
    }

    /// Opens or closes the tree's menus; "Reset all filters" empties the search too, as the
    /// C# one does.
    fn tree_menu_message(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::OpenTreeMenu(menu) => self.open_tree_menu(menu),
            Message::CloseTreeMenu => self.menu = None,
            Message::ResetTreeFilters => {
                self.search.clear();
                let effects = self.app.update(AppMessage::Filter(FilterMessage::Reset));
                return Task::batch(effects.into_iter().map(|effect| self.run(effect)));
            }
            _ => {}
        }
        Task::none()
    }

    /// The tree's search, as the C# sidebar's: typing filters the profiles, Ctrl+F comes
    /// here, and the clear button empties it; Escape, Down and Enter as the C# filter box
    /// ([`SearchKeys`], [`Shell::search_key`]).
    fn search_box(&self) -> Element<'_, Message> {
        let mut search = row![
            tooltip(
                SearchKeys::new(
                    text_input(&fl!("ui-tree-search-placeholder"), &self.search)
                        .id(search_field_id())
                        .on_input(Message::Search)
                        .on_submit(Message::SearchSubmit),
                    (!self.search.is_empty()).then(|| Message::Search(String::new())),
                    Message::SearchDown,
                ),
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
        // The filters, as the C# button beside the search: lit while one leaves profiles out.
        let active = self.app.tree_filter().is_active();
        search = search.push(
            tooltip(
                button(text(FILTER_GLYPH))
                    .style(if active {
                        button::primary
                    } else {
                        button::secondary
                    })
                    .on_press(Message::OpenTreeMenu(TreeMenu::Filter)),
                text(fl!("ui-tree-filter-tooltip")).size(SMALL_SIZE),
                tooltip::Position::Bottom,
            )
            .style(container::rounded_box),
        );
        search.into()
    }

    /// The Gateways tab: the gateways, what goes through each, and the references to one
    /// that is not configured.
    fn gateways_settings(&self) -> Column<'_, Message> {
        crate::gateways_view::view(
            self.app.gateway_overview(),
            self.app.gateways(),
            &self.gateway_reassign,
        )
    }

    /// The Terminal tab: the terminals' look, the transcripts, the macros.
    fn terminal_tab(&self) -> Column<'_, Message> {
        column![
            text(fl!("ui-settings-terminal")).size(BODY_SIZE),
            self.terminal_settings(),
            text(fl!("ui-settings-session-logging")).size(BODY_SIZE),
            self.session_log_settings(),
            text(fl!("ui-macros-menu")).size(BODY_SIZE),
            crate::macros_view::card(self.app.macros()),
        ]
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
        let pin_card = container(
            column![
                text(fl!("ui-settings-pin-title")).size(BODY_SIZE),
                row![
                    text(if self.app.settings().pin.is_some() {
                        fl!("ui-settings-pin-enabled")
                    } else {
                        fl!("ui-settings-pin-disabled")
                    }),
                    iced::widget::space::horizontal(),
                    button(text(fl!("ui-settings-pin-configure")))
                        .style(button::secondary)
                        .on_press(Message::App(AppMessage::Pin(PinMessage::Configure))),
                ]
                .spacing(SPACING)
                .align_y(iced::Alignment::Center),
            ]
            .spacing(SPACING),
        )
        .padding(PADDING)
        .max_width(SETTINGS_WIDTH)
        .style(container::bordered_box);
        let body: Column<'_, Message> = match self.settings_tab {
            SettingsTab::General => self.general_settings(),
            SettingsTab::Terminal => self.terminal_tab(),
            SettingsTab::Ssh => column![
                text(fl!("ui-settings-ssh-auto-reconnect")).size(BODY_SIZE),
                self.ssh_reconnect_settings(),
                text(fl!("ui-settings-ssh-session")).size(BODY_SIZE),
                self.ssh_session_settings(),
                text(fl!("ui-settings-external-editor")).size(BODY_SIZE),
                self.editor_settings(),
                self.trusted_keys_settings(TrustedList::HostKeys),
            ],
            SettingsTab::Rdp => self.rdp_settings(),
            SettingsTab::Gateways => self.gateways_settings(),
            SettingsTab::Security => column![
                pin_card,
                vault_card,
                // Last: a long card, which would push the everyday settings down.
                container(crate::provider_view::card(&self.app, &self.provider_unlock))
                    .max_width(SETTINGS_WIDTH),
            ],
        };
        scrollable(
            column![
                text(fl!("ui-settings-title")).size(HEADING_SIZE),
                settings_tabs(self.settings_tab),
                body.spacing(SPACING),
            ]
            .spacing(SPACING)
            .padding(PADDING),
        )
        .into()
    }

    /// The application's appearance, as the C# General tab's card: its language, applied at
    /// once.
    fn appearance_settings(&self) -> Element<'_, Message> {
        let shown = self
            .app
            .settings()
            .language
            .unwrap_or_else(crate::i18n::current);
        let card = column![
            row![
                text(fl!("ui-settings-language")),
                iced::widget::space::horizontal(),
                pick_list(
                    Language::ALL.map(LanguageChoice).to_vec(),
                    Some(LanguageChoice(shown)),
                    |LanguageChoice(language)| Message::LanguageChosen(language),
                ),
            ]
            .spacing(SPACING)
            .align_y(iced::Alignment::Center),
        ]
        .spacing(SPACING);
        container(card)
            .padding(PADDING)
            .max_width(SETTINGS_WIDTH)
            .style(container::bordered_box)
            .into()
    }

    /// The C# General tab: the appearance, then the behaviour.
    fn general_settings(&self) -> Column<'_, Message> {
        column![
            text(fl!("ui-settings-appearance")).size(BODY_SIZE),
            self.appearance_settings(),
            text(fl!("ui-settings-behavior")).size(BODY_SIZE),
            self.behavior_settings(),
            text(fl!("ui-settings-reachability")).size(BODY_SIZE),
            self.reachability_settings(),
        ]
    }

    /// The C# General tab's Behavior section: whether the tunnels panel starts collapsed.
    fn behavior_settings(&self) -> Element<'_, Message> {
        container(
            column![
                checkbox(self.app.settings().collapse_tunnels_panel)
                    .label(fl!("ui-settings-collapse-tunnels-panel"))
                    .on_toggle(|collapse| {
                        Message::App(AppMessage::Settings(SettingsMessage::CollapseTunnelsPanel(
                            collapse,
                        )))
                    }),
                text(fl!("ui-settings-collapse-tunnels-panel-hint")).size(SMALL_SIZE),
            ]
            .spacing(SPACING),
        )
        .padding(PADDING)
        .max_width(SETTINGS_WIDTH)
        .style(container::bordered_box)
        .into()
    }

    /// The keys trusted for servers of `list`, as the C# Host keys and Certificates pages
    /// list them, with what could not be read.
    fn trusted_keys_settings(&self, list: TrustedList) -> Element<'_, Message> {
        let keys = self.app.trusted_keys();
        let mut lists = column![match list {
            TrustedList::HostKeys =>
                crate::trusted_keys_view::host_keys(keys, &self.host_key_search),
            TrustedList::Certificates => {
                crate::trusted_keys_view::certificates(keys, &self.certificate_search)
            }
        }]
        .spacing(SPACING)
        .max_width(SETTINGS_WIDTH);
        if let Some(unreadable) = crate::trusted_keys_view::unreadable(keys) {
            lists = lists.push(unreadable);
        }
        lists.into()
    }

    /// Session logging, as the C# Settings page offers it: on or off, with what a transcript
    /// keeps said, and the folder the transcripts go to, applied with Enter.
    fn session_log_settings(&self) -> Element<'_, Message> {
        let settings = self.app.settings();
        let typed = self
            .log_directory
            .as_deref()
            .unwrap_or(&settings.session_log_directory);
        container(
            column![
                checkbox(settings.session_logging)
                    .label(fl!("ui-settings-session-logging-record"))
                    .on_toggle(|on| {
                        Message::App(AppMessage::Settings(SettingsMessage::SessionLogging(on)))
                    }),
                text(fl!("ui-settings-session-logging-warning")).size(SMALL_SIZE),
                row![
                    text(fl!("ui-settings-session-log-directory")),
                    text_input(DEFAULT_SESSION_LOG_DIRECTORY, typed)
                        .on_input(Message::LogDirectoryEdited)
                        .on_submit(Message::LogDirectoryApply),
                ]
                .spacing(SPACING)
                .align_y(iced::Alignment::Center),
                text(fl!("ui-settings-session-log-directory-hint")).size(SMALL_SIZE),
            ]
            .spacing(SPACING),
        )
        .padding(PADDING)
        .max_width(SETTINGS_WIDTH)
        .style(container::bordered_box)
        .into()
    }

    /// The program a server's file is edited with, as the C# Settings page's "External
    /// editor": applied with Enter; empty takes the system's own.
    fn editor_settings(&self) -> Element<'_, Message> {
        let typed = self
            .editor_typed
            .as_deref()
            .unwrap_or(&self.app.settings().external_editor);
        container(
            column![
                row![
                    text(fl!("ui-settings-external-editor-path")),
                    text_input("", typed)
                        .on_input(Message::EditorEdited)
                        .on_submit(Message::EditorApply),
                ]
                .spacing(SPACING)
                .align_y(iced::Alignment::Center),
                text(fl!("ui-settings-external-editor-hint")).size(SMALL_SIZE),
            ]
            .spacing(SPACING),
        )
        .padding(PADDING)
        .max_width(SETTINGS_WIDTH)
        .style(container::bordered_box)
        .into()
    }

    /// The RDP tab: the application's RDP options, then the certificates trusted.
    fn rdp_settings(&self) -> Column<'_, Message> {
        column![
            text(fl!("ui-settings-rdp-defaults")).size(BODY_SIZE),
            container(crate::rdp_options::defaults(
                self.app.settings().rdp_defaults
            ))
            .padding(PADDING)
            .max_width(SETTINGS_WIDTH)
            .style(container::bordered_box),
            self.rdp_session_settings(),
            container(self.presets.view())
                .padding(PADDING)
                .max_width(SETTINGS_WIDTH)
                .style(container::bordered_box),
            tooltip(
                button(text(fl!("ui-settings-rdp-reset-defaults")))
                    .style(button::secondary)
                    .on_press(Message::App(AppMessage::Settings(
                        SettingsMessage::ResetRdpDefaults
                    ))),
                text(fl!("ui-settings-rdp-reset-defaults-tooltip")).size(SMALL_SIZE),
                tooltip::Position::Bottom,
            )
            .style(container::rounded_box),
            self.trusted_keys_settings(TrustedList::Certificates),
        ]
    }

    /// How many times a dropped desktop is opened again by itself, as the C#
    /// `RdpAutoReconnectMaxAttempts`.
    fn rdp_session_settings(&self) -> Element<'_, Message> {
        let attempts: Vec<u32> = (heimdall_core::settings::RDP_AUTO_RECONNECT_ATTEMPTS_MIN
            ..=heimdall_core::settings::RDP_AUTO_RECONNECT_ATTEMPTS_MAX)
            .collect();
        container(
            row![
                text(fl!("ui-settings-rdp-auto-reconnect-attempts")),
                iced::widget::space::horizontal(),
                pick_list(
                    attempts,
                    Some(self.app.settings().rdp_auto_reconnect_attempts),
                    |attempts| {
                        Message::App(AppMessage::Settings(
                            SettingsMessage::RdpAutoReconnectAttempts(attempts),
                        ))
                    },
                ),
            ]
            .spacing(SPACING)
            .align_y(iced::Alignment::Center),
        )
        .padding(PADDING)
        .max_width(SETTINGS_WIDTH)
        .style(container::bordered_box)
        .into()
    }

    /// The C# SSH Connection card: auto-reconnect, on or off, and how many attempts before
    /// the reconnect is left to the user; which SSH agent's keys are offered first.
    fn ssh_reconnect_settings(&self) -> Element<'_, Message> {
        let settings = self.app.settings();
        let attempts: Vec<u32> =
            (SSH_AUTO_RECONNECT_ATTEMPTS_MIN..=SSH_AUTO_RECONNECT_ATTEMPTS_MAX).collect();
        container(
            column![
                text(fl!("ui-settings-ssh-auto-reconnect-description")).size(SMALL_SIZE),
                checkbox(settings.ssh_auto_reconnect)
                    .label(fl!("ui-settings-ssh-auto-reconnect-enable"))
                    .on_toggle(|on| {
                        Message::App(AppMessage::Settings(SettingsMessage::SshAutoReconnect(on)))
                    }),
                row![
                    text(fl!("ui-settings-ssh-auto-reconnect-attempts")),
                    iced::widget::space::horizontal(),
                    pick_list(
                        attempts,
                        Some(settings.ssh_auto_reconnect_attempts),
                        |attempts| {
                            Message::App(AppMessage::Settings(
                                SettingsMessage::SshAutoReconnectAttempts(attempts),
                            ))
                        },
                    ),
                ]
                .spacing(SPACING)
                .align_y(iced::Alignment::Center),
                row![
                    text(fl!("ui-settings-ssh-agent-preference")),
                    iced::widget::space::horizontal(),
                    pick_list(
                        AgentPreference::ALL.map(AgentChoice).to_vec(),
                        Some(AgentChoice(settings.ssh_agent_preference)),
                        |AgentChoice(preference)| Message::App(AppMessage::Settings(
                            SettingsMessage::SshAgentPreference(preference)
                        )),
                    ),
                ]
                .spacing(SPACING)
                .align_y(iced::Alignment::Center),
                text(fl!("ui-settings-ssh-agent-preference-hint")).size(SMALL_SIZE),
            ]
            .spacing(SPACING),
        )
        .padding(PADDING)
        .max_width(SETTINGS_WIDTH)
        .style(container::bordered_box)
        .into()
    }

    /// The Files tab shown with its session open, the one files dropped on the window go to.
    fn drop_target(&self) -> Option<TabId> {
        self.app
            .active_tab()
            .filter(|tab| !self.settings_shown() && tab.phase == Phase::Connected)
            .filter(|tab| {
                tab.files
                    .as_ref()
                    .is_some_and(|files| files.client.is_some())
            })
            .map(|tab| tab.id)
    }

    /// Files dragged over the window, or dropped: sent to the server's folder of the Files
    /// tab shown, as the C# tab takes what Explorer drops on it.
    fn drop_message(&mut self, message: Message) -> Vec<Effect> {
        match message {
            Message::FilesHovered(over) => {
                self.files_hovered = over;
                Vec::new()
            }
            Message::FileDropped(path) => {
                self.files_hovered = false;
                match self.drop_target() {
                    Some(tab) => self
                        .app
                        .update(AppMessage::Files(FilesMessage::Dropped { tab, path })),
                    // Where no Files tab takes it, a .rdp file is imported, as the C# window
                    // takes one dropped anywhere.
                    None => self
                        .app
                        .update(AppMessage::Rdp(heimdall_app::RdpMessage::Dropped(vec![
                            path,
                        ]))),
                }
            }
            _ => Vec::new(),
        }
    }

    /// "Drop files to upload" over the Files tab shown while files are dragged over it.
    fn drop_overlay(&self) -> Option<Element<'_, Message>> {
        self.drop_target().filter(|_| self.files_hovered)?;
        Some(opaque(
            container(
                container(text(fl!("ui-files-drop-overlay")).size(HEADING_SIZE))
                    .padding(PADDING)
                    .style(container::bordered_box),
            )
            .center(Length::Fill),
        ))
    }

    /// The transcripts' folder typed, or applied.
    fn settings_field_message(&mut self, message: Message) -> Vec<Effect> {
        match message {
            Message::LogDirectoryEdited(typed) => {
                self.log_directory = Some(typed);
                Vec::new()
            }
            Message::EditorEdited(typed) => {
                self.editor_typed = Some(typed);
                Vec::new()
            }
            Message::PresetsEdited(action) => {
                match self.presets.perform(action) {
                    Some(presets) => self.app.update(AppMessage::Settings(
                        SettingsMessage::RdpResolutionPresets(presets),
                    )),
                    None => Vec::new(),
                }
            }
            Message::EditorApply => match self.editor_typed.take() {
                Some(typed) => self
                    .app
                    .update(AppMessage::Settings(SettingsMessage::ExternalEditor(typed))),
                None => Vec::new(),
            },
            Message::FontSizeEdited(typed) => {
                self.font_size_typed = Some(typed);
                Vec::new()
            }
            Message::FontSizeApply => {
                // A size out of the range stays typed, the C# message under it.
                let Some(size) = self
                    .typed_font_size()
                    .filter(|size| heimdall_core::settings::terminal_font_size_accepted(*size))
                else {
                    return Vec::new();
                };
                self.font_size_typed = None;
                self.app
                    .update(AppMessage::Settings(SettingsMessage::TerminalFontSize(
                        size,
                    )))
            }
            Message::SessionFieldEdited(field, typed) => {
                self.session_typed[field.index()] = Some(typed);
                Vec::new()
            }
            Message::SessionFieldApply(field) => {
                // Out of the range, it stays typed, the C# message under it.
                let Some(seconds) = self
                    .typed_session(field)
                    .filter(|seconds| field.accepted(*seconds))
                else {
                    return Vec::new();
                };
                self.session_typed[field.index()] = None;
                self.app
                    .update(AppMessage::Settings(field.applied(seconds)))
            }
            _ => match self.log_directory.take() {
                Some(typed) => {
                    self.app
                        .update(AppMessage::Settings(SettingsMessage::SessionLogDirectory(
                            typed,
                        )))
                }
                None => Vec::new(),
            },
        }
    }

    /// The font size typed, as a number; `None` when nothing is typed or it is not one.
    fn typed_font_size(&self) -> Option<u16> {
        self.font_size_typed.as_deref()?.trim().parse().ok()
    }

    /// `field`'s value typed, as a number; `None` when nothing is typed or it is not one.
    fn typed_session(&self, field: SessionField) -> Option<u32> {
        self.session_typed[field.index()]
            .as_deref()?
            .trim()
            .parse()
            .ok()
    }

    /// The session settings, as the C# SSH/SFTP Session tab: the SSH keep-alive interval,
    /// the `TMOUT` reset of idle SSH shells, and the anti-idle interval RDP sessions asking
    /// for anti-idle keys follow; each applied with Enter.
    fn ssh_session_settings(&self) -> Element<'_, Message> {
        container(self.number_fields(Column::new().spacing(SPACING), &SessionField::SESSION))
            .padding(PADDING)
            .max_width(SETTINGS_WIDTH)
            .style(container::bordered_box)
            .into()
    }

    /// `fields` under `card`, each typed and applied with Enter, its rule said under it
    /// while what is typed is out of its range.
    fn number_fields<'a>(
        &'a self,
        mut card: Column<'a, Message>,
        fields: &[SessionField],
    ) -> Column<'a, Message> {
        let settings = self.app.settings();
        for &field in fields {
            let shown = field.value(settings).to_string();
            let typed = self.session_typed[field.index()].clone().unwrap_or(shown);
            let refused = self.session_typed[field.index()].is_some()
                && !self
                    .typed_session(field)
                    .is_some_and(|value| field.accepted(value));
            let mut line = row![
                text(field.label()),
                iced::widget::space::horizontal(),
                text_input("", &typed)
                    .width(FONT_SIZE_FIELD_WIDTH)
                    .on_input(move |typed| Message::SessionFieldEdited(field, typed))
                    .on_submit(Message::SessionFieldApply(field)),
            ]
            .spacing(SPACING)
            .align_y(iced::Alignment::Center);
            if let Some(unit) = field.unit() {
                line = line.push(text(unit));
            }
            card = card.push(line);
            if let Some(hint) = field.hint() {
                card = card.push(text(hint).size(SMALL_SIZE));
            }
            if refused {
                card = card.push(text(field.refusal()).size(SMALL_SIZE).style(text::danger));
            }
        }
        card
    }

    /// The C# session health monitor's settings: whether every server is checked in the
    /// background, how often, how long each has to answer and how many at once.
    fn reachability_settings(&self) -> Element<'_, Message> {
        let card = column![
            checkbox(self.app.settings().reachability.enabled)
                .label(fl!("ui-settings-reachability-enabled"))
                .on_toggle(
                    |on| Message::App(AppMessage::Settings(SettingsMessage::Reachability(on)))
                ),
            text(fl!("ui-settings-reachability-hint")).size(SMALL_SIZE),
        ]
        .spacing(SPACING);
        container(self.number_fields(card, &SessionField::REACHABILITY))
            .padding(PADDING)
            .max_width(SETTINGS_WIDTH)
            .style(container::bordered_box)
            .into()
    }

    /// The terminal's appearance, as the C# Settings page offers it: its font size, applied
    /// with Enter, and its colour scheme.
    fn terminal_settings(&self) -> Element<'_, Message> {
        let settings = self.app.settings();
        let shown = settings.terminal_font_size.to_string();
        let typed = self.font_size_typed.as_deref().unwrap_or(&shown);
        let refused = self.font_size_typed.is_some()
            && !self
                .typed_font_size()
                .is_some_and(heimdall_core::settings::terminal_font_size_accepted);
        let mut card = column![
            row![
                text(fl!("ui-settings-font-size")),
                iced::widget::space::horizontal(),
                text_input("", typed)
                    .width(FONT_SIZE_FIELD_WIDTH)
                    .on_input(Message::FontSizeEdited)
                    .on_submit(Message::FontSizeApply),
                text(fl!("ui-settings-font-size-unit")),
            ]
            .spacing(SPACING)
            .align_y(iced::Alignment::Center),
        ]
        .spacing(SPACING);
        if refused {
            card = card.push(
                text(fl!(
                    "ui-settings-font-size-refused",
                    min = heimdall_core::settings::TERMINAL_FONT_SIZE_MIN,
                    max = heimdall_core::settings::TERMINAL_FONT_SIZE_MAX
                ))
                .size(SMALL_SIZE)
                .style(text::danger),
            );
        }
        card = card.push(
            row![
                text(fl!("ui-settings-color-scheme")),
                iced::widget::space::horizontal(),
                pick_list(
                    ColorScheme::ALL.map(SchemeChoice).to_vec(),
                    Some(SchemeChoice(settings.color_scheme)),
                    |SchemeChoice(scheme)| Message::App(AppMessage::Settings(
                        SettingsMessage::ColorScheme(scheme)
                    )),
                ),
            ]
            .spacing(SPACING)
            .align_y(iced::Alignment::Center),
        );
        container(card)
            .padding(PADDING)
            .max_width(SETTINGS_WIDTH)
            .style(container::bordered_box)
            .into()
    }

    /// Where the keyboard goes after `message`: to the tree after a click in it, back to the
    /// session after a tab is chosen; and the page a chosen tab shows.
    fn note_focus(&mut self, message: &Message) {
        match message {
            Message::App(AppMessage::SelectTab(_)) => {
                self.page = Page::Tab;
                self.tree_focused = false;
            }
            // A tab's menu is the tab bar's, an entry's the Files tab's: the keyboard stays
            // where it was.
            Message::OpenTreeMenu(
                TreeMenu::Tab(_)
                | TreeMenu::FilesEntry { .. }
                | TreeMenu::FilesBookmarks(_)
                | TreeMenu::FilesBookmarksRemove(_),
            ) => {}
            Message::App(AppMessage::ToggleFolder(_))
            | Message::OpenTreeMenu(_)
            | Message::TreeClick(_) => self.tree_focused = true,
            Message::ContentFocus => self.tree_focused = false,
            _ => {}
        }
    }

    /// A change in Quick Connect: its search, a result chosen, closed.
    fn palette_message(&mut self, message: Message) -> Vec<Effect> {
        match message {
            Message::PaletteQuery(query) => {
                if let Some(palette) = self.palette.as_mut() {
                    palette.query = query;
                    palette.chosen = 0;
                }
                Vec::new()
            }
            Message::PaletteChoose(index) => {
                let Some(palette) = self.palette.take() else {
                    return Vec::new();
                };
                if let Some(result) = self
                    .app
                    .quick_results(&palette.query)
                    .into_iter()
                    .nth(index)
                {
                    self.app.update(AppMessage::QuickConnect(result))
                } else {
                    // Nothing there: the palette stays for another search.
                    self.palette = Some(palette);
                    Vec::new()
                }
            }
            _ => {
                self.palette = None;
                Vec::new()
            }
        }
    }

    /// A key while Quick Connect is open: the arrows move its choice within its results,
    /// Enter opens the one chosen, the others are nobody's; `None` while it is closed.
    fn palette_key(&mut self, key: FilesKey) -> Option<Vec<Effect>> {
        let palette = self.palette.as_mut()?;
        let count = self.app.quick_results(&palette.query).len();
        match key {
            FilesKey::Previous => palette.chosen = palette.chosen.saturating_sub(1),
            FilesKey::Next if palette.chosen + 1 < count => palette.chosen += 1,
            FilesKey::Open => {
                let chosen = palette.chosen;
                return Some(self.palette_message(Message::PaletteChoose(chosen)));
            }
            _ => {}
        }
        Some(Vec::new())
    }

    /// The pointer over a Files tab's panes, a press on an entry, its drag and its drop.
    fn files_drag_message(&mut self, message: &Message) -> Vec<Effect> {
        match *message {
            Message::FilesHover(spot) => {
                self.files_hover = Some(spot);
                if let Some(drag) = self.files_drag.as_mut() {
                    drag.over = Some(spot);
                }
            }
            Message::FilesHoverLeft(spot) => {
                // Off an entry is still in its pane; off a pane, nowhere.
                let back = spot.index.map(|_| crate::files_drag::Spot {
                    index: None,
                    ..spot
                });
                if self.files_hover == Some(spot) {
                    self.files_hover = back;
                }
                if let Some(drag) = self
                    .files_drag
                    .as_mut()
                    .filter(|drag| drag.over == Some(spot))
                {
                    drag.over = back;
                }
            }
            Message::PointerPressed => {
                self.files_drag = self
                    .files_hover
                    .filter(|spot| spot.index.is_some())
                    .map(|spot| crate::files_drag::FilesDrag::pressed(spot, self.cursor.get()));
            }
            Message::FilesDragMoved(at) => {
                let started = self.files_drag.as_mut().is_some_and(|drag| drag.moved(at));
                // The entry pressed, not among those chosen: it alone is dragged, as the C#
                // selects it first.
                if started && let Some(from) = self.files_drag.as_ref().map(|drag| drag.from) {
                    let chosen = self
                        .app
                        .tab(from.tab)
                        .and_then(|tab| tab.files.as_deref())
                        .map(|files| match from.side {
                            heimdall_app::files::Side::Remote => files.remote.chosen(),
                            heimdall_app::files::Side::Local => files.local.chosen(),
                        })
                        .unwrap_or_default();
                    if let Some(index) = from.index.filter(|index| !chosen.contains(index)) {
                        return self.app.update(AppMessage::Files(FilesMessage::Select {
                            tab: from.tab,
                            side: from.side,
                            index,
                        }));
                    }
                }
            }
            Message::FilesDragEnd => {
                if let Some(message) = self
                    .files_drag
                    .take()
                    .and_then(crate::files_drag::FilesDrag::drop_message)
                {
                    return self.app.update(message);
                }
            }
            _ => {}
        }
        Vec::new()
    }

    /// A click on a profile of the tree, or one of the tree's shortcuts holding Ctrl.
    fn tree_input(&mut self, message: Message) -> Vec<Effect> {
        match message {
            Message::TreeClick(id) => {
                let effects = self.tree_click(id.clone());
                self.press_profile(id);
                effects
            }
            Message::TreeShortcut(shortcut) => self.tree_shortcut(shortcut),
            Message::TreeHover(target) => {
                if let Some(drag) = self.tree_drag.as_mut() {
                    drag.over = Some(target);
                }
                Vec::new()
            }
            Message::TreeHoverLeft(target) => {
                if let Some(drag) = self
                    .tree_drag
                    .as_mut()
                    .filter(|drag| drag.over.as_ref() == Some(&target))
                {
                    drag.over = None;
                }
                Vec::new()
            }
            Message::TreeDragMoved(at) => {
                let started = self.tree_drag.as_mut().is_some_and(|drag| drag.moved(at));
                // A folder pressed opened or closed: dragged, it is put back as it was.
                match self.tree_drag.as_ref().map(|drag| &drag.source) {
                    Some(crate::tree_drag::DragSource::Folder(path)) if started => {
                        self.app.update(AppMessage::ToggleFolder(path.clone()))
                    }
                    _ => Vec::new(),
                }
            }
            Message::TreeDragEnd => {
                match self
                    .tree_drag
                    .take()
                    .and_then(crate::tree_drag::TreeDrag::drop_message)
                {
                    Some(message) => self.app.update(message),
                    None => Vec::new(),
                }
            }
            Message::MenuKey => {
                if self.tree_focused
                    && self.app.dialog.is_none()
                    && let Some(id) = self.app.selected_profile.clone()
                {
                    self.open_tree_menu(TreeMenu::Profile(id));
                }
                Vec::new()
            }
            Message::TypeAhead(typed) => self.type_ahead(&typed),
            Message::SidebarDragStart => {
                self.sidebar_drag = true;
                Vec::new()
            }
            // Full screen is not the window's own size.
            Message::WindowResized(size) => {
                if !self.fullscreen {
                    self.window_size = Some(size);
                }
                Vec::new()
            }
            Message::SidebarDragged(x) if self.sidebar_drag => {
                self.sidebar_width = x.clamp(SIDEBAR_MIN_WIDTH, SIDEBAR_MAX_WIDTH);
                Vec::new()
            }
            Message::SidebarDragEnd => {
                self.sidebar_drag = false;
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    /// A press on session `id`: the start of a drag of it, or of the sessions selected with
    /// it.
    fn press_profile(&mut self, id: ProfileId) {
        let selected = self.app.selected_profiles();
        let source = if selected.contains(&id) {
            selected
        } else {
            vec![id]
        };
        self.tree_drag = Some(crate::tree_drag::TreeDrag::pressed(
            crate::tree_drag::DragSource::Profiles(source),
            self.cursor.get(),
        ));
    }

    /// A character typed while the tree has the keyboard: the next profile whose name starts
    /// with what was typed within [`TYPE_AHEAD_RESET`] is selected, as a Windows tree's
    /// type-ahead; the same letter again goes on to the next one.
    fn type_ahead(&mut self, typed: &str) -> Vec<Effect> {
        if !self.tree_focused || self.app.dialog.is_some() || self.gated() {
            return Vec::new();
        }
        let now = std::time::Instant::now();
        let (buffer, last) = &mut self.type_ahead;
        if last.is_none_or(|at| now.duration_since(at) > TYPE_AHEAD_RESET) {
            buffer.clear();
        }
        *last = Some(now);
        buffer.push_str(&typed.to_lowercase());
        let wanted = buffer.clone();
        let order: Vec<(ProfileId, String)> = self
            .app
            .tree_rows(&self.search)
            .into_iter()
            .filter_map(|row| match row {
                TreeRow::Profile { profile, .. } => Some((profile.id, profile.name.to_lowercase())),
                TreeRow::Folder { .. } => None,
            })
            .collect();
        // A letter repeated looks for the next one starting with it.
        let mut letters = wanted.chars();
        let repeated = letters
            .next()
            .is_some_and(|first| letters.all(|c| c == first));
        let prefix = if repeated {
            wanted.chars().take(1).collect::<String>()
        } else {
            wanted
        };
        let at = self
            .app
            .selected_profile
            .as_ref()
            .and_then(|id| order.iter().position(|(found, _)| found == id));
        let start = match at {
            Some(at) if repeated => at + 1,
            Some(at) => at,
            None => 0,
        };
        let found = (0..order.len())
            .map(|step| &order[(start + step) % order.len()])
            .find(|(_, name)| name.starts_with(&prefix));
        match found {
            Some((id, _)) => self.app.update(AppMessage::SelectProfile(id.clone())),
            None => Vec::new(),
        }
    }

    /// The session shown; while the tree has the keyboard, a click in it takes it back.
    fn focusable_content(&self) -> Element<'_, Message> {
        if self.tree_focused {
            mouse_area(self.content())
                .on_press(Message::ContentFocus)
                .into()
        } else {
            self.content()
        }
    }

    /// A tree shortcut holding Ctrl: Ctrl+E edits the profile selected when the tree has
    /// the keyboard; Ctrl+N opens a new session's form.
    fn tree_shortcut(&mut self, shortcut: TreeShortcut) -> Vec<Effect> {
        if self.app.dialog.is_some() || self.gated() {
            return Vec::new();
        }
        match shortcut {
            TreeShortcut::New => self.app.update(AppMessage::NewProfile),
            TreeShortcut::Undo => self.app.update(AppMessage::UndoMove),
            TreeShortcut::ToggleSidebar => {
                self.sidebar_hidden = !self.sidebar_hidden;
                if self.sidebar_hidden {
                    self.tree_focused = false;
                }
                Vec::new()
            }
            TreeShortcut::QuickConnect => {
                self.menu = None;
                self.palette = Some(Palette::default());
                self.focus_next = Some(crate::palette::field_id());
                Vec::new()
            }
            TreeShortcut::Edit => match self.app.selected_profile.clone() {
                Some(id) if self.tree_focused && self.app.can_edit(&id) => {
                    self.app.update(AppMessage::EditProfile(id))
                }
                _ => Vec::new(),
            },
        }
    }

    /// A key of the Files tab's set pressed while the tree has the keyboard, as the C# tree
    /// takes them: the arrows move through folders and profiles, Shift extending the
    /// selection; Left and Right fold and unfold a folder, or go to its parent and its first
    /// row; Enter connects or folds; F2 renames, Delete deletes once asked; Ctrl+Space adds a
    /// profile to the selection or takes it out. `None` when the tree does not have it.
    fn tree_key(&mut self, key: FilesKey) -> Option<Vec<Effect>> {
        if !self.tree_focused || self.app.dialog.is_some() {
            return None;
        }
        let several = !self.app.selected_profiles().is_empty();
        let cursor = self.tree_cursor();
        Some(match key {
            FilesKey::ExtendPrevious | FilesKey::ExtendNext => {
                self.extend_tree_selection(key == FilesKey::ExtendPrevious)
            }
            FilesKey::Previous | FilesKey::Next => {
                let rows = self.tree_cursors();
                let at = cursor
                    .as_ref()
                    .and_then(|cursor| rows.iter().position(|row| row == cursor));
                let next = match (key, at) {
                    (FilesKey::Previous, Some(at)) => at.checked_sub(1),
                    (_, Some(at)) => Some(at + 1),
                    (_, None) => Some(0),
                };
                match next.and_then(|index| rows.get(index)) {
                    Some(row) => self.select_tree_row(row.clone()),
                    None => Vec::new(),
                }
            }
            FilesKey::First | FilesKey::Last => {
                let rows = self.tree_cursors();
                let row = if key == FilesKey::First {
                    rows.first()
                } else {
                    rows.last()
                };
                match row {
                    Some(row) => self.select_tree_row(row.clone()),
                    None => Vec::new(),
                }
            }
            FilesKey::SelectAll => {
                let order = self.tree_order();
                let (Some(first), Some(last)) = (order.first().cloned(), order.last().cloned())
                else {
                    return Some(Vec::new());
                };
                // Every profile shown, as the C# tree's Ctrl+A.
                let mut effects = self.app.update(AppMessage::SelectProfile(first));
                effects.extend(
                    self.app
                        .update(AppMessage::Selection(SelectionMessage::Range {
                            to: last,
                            order,
                        })),
                );
                effects
            }
            FilesKey::Focus(side) => self.tree_fold(cursor, side == Side::Remote),
            FilesKey::Open => match cursor {
                Some(TreeCursor::Folder(path)) => {
                    let effects = self.app.update(AppMessage::ToggleFolder(path.clone()));
                    let _ = self.app.update(AppMessage::SelectFolder(path));
                    effects
                }
                _ if several => self
                    .app
                    .update(AppMessage::Selection(SelectionMessage::Connect)),
                Some(TreeCursor::Profile(id)) => self.app.update(AppMessage::ConnectProfile(id)),
                None => Vec::new(),
            },
            FilesKey::Delete => match cursor {
                Some(TreeCursor::Folder(path)) if path != heimdall_app::NO_FOLDER => self
                    .app
                    .update(AppMessage::Folder(FolderMessage::RequestDelete(path))),
                _ if several => self
                    .app
                    .update(AppMessage::Selection(SelectionMessage::RequestDelete)),
                Some(TreeCursor::Profile(id)) => {
                    self.app.update(AppMessage::RequestDeleteProfile(id))
                }
                _ => Vec::new(),
            },
            FilesKey::Rename => match cursor {
                Some(TreeCursor::Folder(path)) if path != heimdall_app::NO_FOLDER => self
                    .app
                    .update(AppMessage::Folder(FolderMessage::Rename(path))),
                Some(TreeCursor::Profile(id)) if self.app.can_edit(&id) => self
                    .app
                    .update(AppMessage::ProfileMenu(ProfileMenuMessage::Rename(id))),
                _ => Vec::new(),
            },
            // The row the keyboard is on: where a Shift range ended, else the one selected.
            FilesKey::ToggleMark => match (self.tree_focus.take(), cursor) {
                (Some(id), _) | (None, Some(TreeCursor::Profile(id))) => self
                    .app
                    .update(AppMessage::Selection(SelectionMessage::Toggle(id))),
                _ => Vec::new(),
            },
            _ => Vec::new(),
        })
    }

    /// Left (`unfold` false): a folder open closes; else up to the folder holding the row.
    /// Right: a folder closed opens; open, down to its first row.
    fn tree_fold(&mut self, cursor: Option<TreeCursor>, unfold: bool) -> Vec<Effect> {
        match (cursor, unfold) {
            (Some(TreeCursor::Folder(path)), false) if self.folder_open(&path) => {
                self.app.update(AppMessage::ToggleFolder(path))
            }
            (Some(row), false) => match self.parent_row(&row) {
                Some(parent) => self.select_tree_row(parent),
                None => Vec::new(),
            },
            (Some(TreeCursor::Folder(path)), true) if !self.folder_open(&path) => {
                let effects = self.app.update(AppMessage::ToggleFolder(path.clone()));
                // Opening it keeps it the one selected.
                let _ = self.app.update(AppMessage::SelectFolder(path));
                effects
            }
            (Some(folder @ TreeCursor::Folder(_)), true) => {
                let rows = self.tree_cursors();
                let below = rows
                    .iter()
                    .position(|row| *row == folder)
                    .and_then(|at| rows.get(at + 1))
                    .cloned();
                match below {
                    Some(row) if self.parent_row(&row).as_ref() == Some(&folder) => {
                        self.select_tree_row(row)
                    }
                    _ => Vec::new(),
                }
            }
            _ => Vec::new(),
        }
    }

    /// The row the tree's keyboard is on: the folder selected, or the profile.
    fn tree_cursor(&self) -> Option<TreeCursor> {
        self.app
            .selected_folder
            .clone()
            .map(TreeCursor::Folder)
            .or_else(|| self.app.selected_profile.clone().map(TreeCursor::Profile))
    }

    /// Every row of the tree, folders and profiles, as it shows them.
    fn tree_cursors(&self) -> Vec<TreeCursor> {
        self.app
            .tree_rows(&self.search)
            .into_iter()
            .map(|row| match row {
                TreeRow::Profile { profile, .. } => TreeCursor::Profile(profile.id),
                TreeRow::Folder { path, .. } => TreeCursor::Folder(path),
            })
            .collect()
    }

    /// Whether the folder at `path` shows its content.
    fn folder_open(&self, path: &str) -> bool {
        self.app.tree_rows(&self.search).into_iter().any(
            |row| matches!(row, TreeRow::Folder { path: found, open: true, .. } if found == path),
        )
    }

    /// The folder row holding `row`, when the tree shows one: the closest row above it, less
    /// deep.
    fn parent_row(&self, row: &TreeCursor) -> Option<TreeCursor> {
        let rows = self.app.tree_rows(&self.search);
        let depth_of = |row: &TreeRow| match row {
            TreeRow::Profile { depth, .. } | TreeRow::Folder { depth, .. } => *depth,
        };
        let at = rows.iter().position(|found| match (found, row) {
            (TreeRow::Profile { profile, .. }, TreeCursor::Profile(id)) => profile.id == *id,
            (TreeRow::Folder { path, .. }, TreeCursor::Folder(wanted)) => path == wanted,
            _ => false,
        })?;
        let depth = depth_of(&rows[at]);
        rows[..at].iter().rev().find_map(|above| match above {
            TreeRow::Folder { path, depth: d, .. } if *d < depth => {
                Some(TreeCursor::Folder(path.clone()))
            }
            _ => None,
        })
    }

    /// Selects `row`, a folder or a profile.
    fn select_tree_row(&mut self, row: TreeCursor) -> Vec<Effect> {
        // A range's moving end starts again from here.
        self.tree_focus = None;
        match row {
            TreeCursor::Profile(id) => self.app.update(AppMessage::SelectProfile(id)),
            TreeCursor::Folder(path) => self.app.update(AppMessage::SelectFolder(path)),
        }
    }

    /// Shift+Up or Shift+Down: the selection grows to the profile above or below, from the
    /// one it started at, as a Windows tree's.
    fn extend_tree_selection(&mut self, up: bool) -> Vec<Effect> {
        let order = self.tree_order();
        let at = self
            .tree_focus
            .clone()
            .or_else(|| self.app.selected_profile.clone())
            .and_then(|id| order.iter().position(|found| *found == id));
        let next = match (up, at) {
            (true, Some(at)) => at.checked_sub(1),
            (false, Some(at)) => Some(at + 1),
            (_, None) => Some(0),
        };
        let Some(to) = next.and_then(|index| order.get(index)).cloned() else {
            return Vec::new();
        };
        let mut effects = Vec::new();
        if self.app.selected_profile.is_none() {
            effects.extend(self.app.update(AppMessage::SelectProfile(to.clone())));
        }
        self.tree_focus = Some(to.clone());
        effects.extend(
            self.app
                .update(AppMessage::Selection(SelectionMessage::Range { to, order })),
        );
        effects
    }

    /// The profiles as the tree shows them, folders left out.
    fn tree_order(&self) -> Vec<ProfileId> {
        self.app
            .tree_rows(&self.search)
            .into_iter()
            .filter_map(|row| match row {
                TreeRow::Profile { profile, .. } => Some(profile.id),
                TreeRow::Folder { .. } => None,
            })
            .collect()
    }

    /// Enter or Down in the tree's search, as the C# filter box: Enter opens the profile
    /// found when it is the only one and does nothing rather than guess among several; Down
    /// gives the tree the keyboard on the first profile found.
    fn search_key(&mut self, message: &Message) -> Task<Message> {
        let found = self.search_found();
        let effects = match (message, found.as_slice()) {
            (Message::SearchSubmit, [only]) if !self.search.trim().is_empty() => {
                self.app.update(AppMessage::ConnectProfile(only.clone()))
            }
            (Message::SearchDown, [first, ..]) => {
                self.tree_focused = true;
                let effects = self.app.update(AppMessage::SelectProfile(first.clone()));
                let tasks: Vec<Task<Message>> =
                    effects.into_iter().map(|effect| self.run(effect)).collect();
                return Task::batch(tasks).chain(iced::advanced::widget::operate(
                    iced::advanced::widget::operation::focusable::unfocus(),
                ));
            }
            _ => Vec::new(),
        };
        Task::batch(effects.into_iter().map(|effect| self.run(effect)))
    }

    /// The profiles the tree's search finds, in the order shown: while a search is typed
    /// every folder is open, so these are all of them.
    fn search_found(&self) -> Vec<ProfileId> {
        self.app
            .tree_rows(&self.search)
            .into_iter()
            .filter_map(|row| match row {
                TreeRow::Profile { profile, .. } => Some(profile.id),
                TreeRow::Folder { .. } => None,
            })
            .collect()
    }

    /// A click on profile `id` in the tree, as the C# tree takes it: alone, with Ctrl added or
    /// taken, with Shift all from the last one clicked in the order shown.
    /// A click on an entry of a Files tab holding Ctrl selects it with the others, holding
    /// Shift every entry up to it, as in the C# tab.
    fn files_click(&self, message: Message) -> Message {
        let Message::App(AppMessage::Files(FilesMessage::Select { tab, side, index })) = message
        else {
            return message;
        };
        let files = if self.modifiers.command() {
            FilesMessage::Toggle { tab, side, index }
        } else if self.modifiers.shift() {
            FilesMessage::Range { tab, side, index }
        } else {
            FilesMessage::Select { tab, side, index }
        };
        Message::App(AppMessage::Files(files))
    }

    fn tree_click(&mut self, id: ProfileId) -> Vec<Effect> {
        let message = if self.modifiers.command() {
            SelectionMessage::Toggle(id)
        } else if self.modifiers.shift() {
            let order = self
                .app
                .tree_rows(&self.search)
                .into_iter()
                .filter_map(|row| match row {
                    TreeRow::Profile { profile, .. } => Some(profile.id),
                    TreeRow::Folder { .. } => None,
                })
                .collect();
            SelectionMessage::Range { to: id, order }
        } else {
            return self.app.update(AppMessage::SelectProfile(id));
        };
        self.app.update(AppMessage::Selection(message))
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
            resolution: matches!(tab.profile, TabProfile::Rdp(_)) && tab.desktop.is_some(),
            health: (tab.health.shown || tab.health.available()).then_some(tab.health.shown),
            pinned: tab.pinned,
            vnc_resize: tab
                .desktop
                .as_ref()
                .and_then(|pane| pane.vnc_remote_resize()),
            saveable: self.app.can_save_as_profile(tab),
            macros: self.app.macro_menu(tab).is_some(),
            transcript: if tab.transcript.is_some() {
                TranscriptEntry::Stop
            } else if shows_terminal(tab) {
                TranscriptEntry::Start(self.app.can_start_transcript(tab))
            } else {
                TranscriptEntry::Absent
            },
        })
    }

    fn tab_bar(&self) -> Element<'_, Message> {
        let mut tabs = row![].spacing(SPACING).padding(PADDING);
        // The sidebar hidden, a way to show it again, as the C# button where it was.
        if self.sidebar_hidden {
            tabs = tabs.push(
                tooltip(
                    button(text(SHOW_SIDEBAR_GLYPH))
                        .style(button::secondary)
                        .on_press(Message::TreeShortcut(TreeShortcut::ToggleSidebar)),
                    text(fl!("ui-sidebar-show-tooltip")).size(SMALL_SIZE),
                    tooltip::Position::Bottom,
                )
                .style(container::rounded_box),
            );
        }
        for tab in &self.app.tabs {
            let active = self.app.active == Some(tab.id);
            let title = if tab.files.is_some() && tab.custom_title.is_none() {
                fl!("ui-tab-files-title", name = tab_label(tab.display_title()))
            } else {
                tab_label(tab.display_title())
            };
            // The session's state and protocol before the name, as the C# tab's dot and icon;
            // the protocol in the button's own colour, which a secondary one would lose on
            // both the active and the other tabs.
            let mut label = row![
                tree_view::state_dot(Some(SessionState::of(tab))),
                text(self.app.tab_kind(tab).label()).size(SMALL_SIZE),
                text(title),
            ]
            .spacing(SPACING / 2.0)
            .align_y(iced::Alignment::Center);
            if tab.pinned {
                label = label.push(text(fl!("ui-tab-pinned-badge")).size(SMALL_SIZE));
            }
            // A macro recorded from it, or typed into it.
            if tab.macro_recording.is_some() {
                label = label.push(
                    text(fl!("ui-tab-recording-badge"))
                        .size(SMALL_SIZE)
                        .style(text::danger),
                );
            } else if tab.macro_playing.is_some() {
                label = label.push(text(fl!("ui-tab-macro-badge")).size(SMALL_SIZE));
            }
            if tab.bell && !active {
                label = label.push(text(fl!("ui-tab-bell-badge")).size(SMALL_SIZE));
            }
            let marked = self.app.broadcasting()
                && self.app.settings().broadcast_scope == BroadcastScope::SelectedTabs
                && shows_terminal(tab);
            if marked {
                // A target of broadcast input, or not, as the C# tab's marker.
                let target = self.app.is_broadcast_target(tab.id);
                tabs = tabs.push(
                    tooltip(
                        button(
                            text(if target {
                                fl!("ui-broadcast-target-on")
                            } else {
                                fl!("ui-broadcast-target-off")
                            })
                            .size(SMALL_SIZE),
                        )
                        .style(button::text)
                        .on_press(Message::App(AppMessage::Broadcast(
                            BroadcastMessage::Target(tab.id),
                        ))),
                        text(fl!("ui-broadcast-target-tooltip")).size(SMALL_SIZE),
                        tooltip::Position::Bottom,
                    )
                    .style(container::rounded_box),
                );
            }
            if let Some(progress) = &tab.post_connect {
                tabs = tabs.push(post_connect_badge(tab.id, progress));
            }
            if tab.transcript.is_some() {
                label = label.push(
                    tooltip(
                        text(fl!("ui-tab-recording")).size(SMALL_SIZE),
                        text(fl!("ui-tab-recording-tooltip")).size(SMALL_SIZE),
                        tooltip::Position::Bottom,
                    )
                    .style(container::rounded_box),
                );
            }
            // The close button inside the tab, at its right, as the C# tab's: it takes the
            // press, which selecting the tab then does not see.
            label = label.push(
                button(text(fl!("ui-tab-close-button")).size(SMALL_SIZE))
                    .style(button::text)
                    .padding([0.0, 2.0])
                    .on_press(Message::App(AppMessage::RequestCloseTab(tab.id))),
            );
            tabs = tabs.push(
                mouse_area(
                    button(label)
                        .style(if active {
                            button::primary
                        } else {
                            button::secondary
                        })
                        .on_press(Message::App(AppMessage::SelectTab(tab.id))),
                )
                .on_right_press(Message::OpenTreeMenu(TreeMenu::Tab(tab.id))),
            );
        }
        tabs.wrap().into()
    }

    /// A Files tab's page: its integrated editor when a file is open in it, else its lists.
    fn files_page<'a>(
        &'a self,
        tab: TabId,
        pane: &'a heimdall_app::files::FilesPane,
        live: bool,
    ) -> Element<'a, Message> {
        match &pane.editor {
            Some(edit) => crate::integrated_editor::view(
                tab,
                edit,
                self.editors.get(edit.id),
                pane.client.is_some(),
            ),
            None => crate::files_view::view(
                tab,
                pane,
                live,
                self.path_editing
                    .filter(|(editing, _)| *editing == tab)
                    .map(|(_, side)| side),
                self.files_drag
                    .as_ref()
                    .filter(|drag| drag.active)
                    .and_then(|drag| drag.over),
            ),
        }
    }

    /// What the window shows beside the tree, in the area a screenshot takes.
    fn content(&self) -> Element<'_, Message> {
        container(self.page())
            .id(crate::screenshot::area_id())
            .into()
    }

    fn page(&self) -> Element<'_, Message> {
        if self.settings_shown() {
            return self.settings_page();
        }
        let Some(tab) = self.app.active_tab() else {
            return self.home();
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
                tab.asks_about_certificate()
                    .then(|| (tab.profile.name(), tab.certificate_context.as_ref())),
            ),
            Phase::Connected => match (tab.files.as_deref(), tab.desktop.as_deref()) {
                (Some(pane), _) => self.files_page(tab.id, pane, tab.is_live()),
                (_, Some(pane)) => self.desktop(tab, pane),
                _ => self.shell_page(tab),
            },
            // An editor's text outlives its session: kept in sight, saved once connected
            // again.
            Phase::Closed { .. }
                if let Some(edit) = tab.files.as_deref().and_then(|pane| pane.editor.as_ref()) =>
            {
                column![
                    crate::integrated_editor::view(tab.id, edit, self.editors.get(edit.id), false),
                    self.session_actions(tab),
                ]
                .spacing(SPACING)
                .into()
            }
            // A remote desktop that ended leaves nothing to look at.
            Phase::Closed { .. } if matches!(tab.purpose, Purpose::Rdp | Purpose::Vnc) => {
                // As the C# says each: the Remote Desktop session by its name.
                let said = if tab.purpose == Purpose::Rdp {
                    fl!("ui-rdp-session-closed")
                } else {
                    fl!("ui-session-closed")
                };
                let mut ended = column![text(said)].spacing(SPACING);
                if let Some(reason) = tab.end_reason.as_ref().and_then(texts::rdp_ending) {
                    ended = ended.push(text(reason));
                }
                center(card(ended.push(self.session_actions(tab).wrap()))).into()
            }
            Phase::Closed { exit_status } => {
                let status = exit_status.map_or_else(
                    || fl!("ui-session-closed"),
                    |status| fl!("ui-session-closed-status", status = status.to_string()),
                );
                let mut ended = column![
                    self.searchable_terminal(tab, self.app.dialog.is_none()),
                    row![text(status), self.session_actions(tab)]
                        .spacing(SPACING)
                        .padding(PADDING)
                        .align_y(iced::Alignment::Center),
                ];
                // What PowerShell's error meant, as the C# says it under the session.
                if let Some(found) = tab.winrm_diagnostic {
                    ended = ended.push(
                        container(text(texts::winrm_diagnostic(found)).style(text::danger))
                            .padding(PADDING),
                    );
                }
                ended.into()
            }
            // Stopped by the user: said, without an error to report.
            Phase::Failed(error @ (UiError::Cancelled | UiError::CertificateRefused)) => {
                let said = if *error == UiError::Cancelled {
                    fl!("ui-session-cancelled")
                } else {
                    texts::error(error)
                };
                center(card(
                    column![text(said), self.session_actions(tab).wrap()].spacing(SPACING),
                ))
                .into()
            }
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
                // Buttons go to the next line whole when a translation is long, never cut.
                actions.wrap().vertical_spacing(SPACING),
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
                button(action_label(fl!("ui-session-reconnect-button")))
                    .on_press(Message::App(AppMessage::ReconnectTab(tab.id))),
            );
        }
        if matches!(
            &tab.phase,
            Phase::Failed(error) if !matches!(error, UiError::Cancelled | UiError::CertificateRefused)
        ) {
            actions = actions.push(
                button(action_label(fl!("ui-session-copy-error-button")))
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
                button(action_label(fl!("ui-session-edit-profile-button")))
                    .style(button::secondary)
                    .on_press(Message::App(AppMessage::EditProfile(profile.id))),
            );
        }
        actions.push(
            button(action_label(fl!("ui-session-close-button")))
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

    /// What `tab`'s "Resolution" menu and its button on the session bar show, while its
    /// RDP desktop is.
    fn resolution_state(&self, tab: &Tab) -> Option<tree_view::ResolutionMenuState> {
        let pane = tab.desktop.as_deref()?;
        Some(tree_view::ResolutionMenuState {
            tab: tab.id,
            fixed: pane.fixed_size(),
            saved: self.app.tab_profile(tab).is_some(),
            mode: tab.resolution_mode()?,
            shown: pane.tab_size(),
            aspect: pane.aspect,
        })
    }

    /// The remote desktop of a connected tab.
    /// Whether `tab`'s desktop is fitted to the tab: as chosen, else as its profile asks.
    fn fits(&self, tab: &Tab) -> bool {
        let pane = tab.desktop.as_deref();
        let fixed = pane.and_then(DesktopPane::fixed_size);
        match self.desktop_fit.get(&tab.id) {
            Some((fit, made_for)) if *made_for == fixed => *fit,
            // A size chosen larger than the tab is scaled to fit, as the C# turns smart
            // sizing on then only.
            _ => match (fixed, pane.and_then(DesktopPane::tab_size)) {
                (Some((width, height)), Some((shown_width, shown_height))) => {
                    width > shown_width || height > shown_height
                }
                _ => fits_by_default(&tab.profile),
            },
        }
    }

    /// A remote desktop under its bar, as the C# session's: the keys this computer keeps for
    /// itself, sent from a menu, how the desktop is shown, and full screen.
    fn desktop<'a>(&self, tab: &Tab, pane: &'a DesktopPane) -> Element<'a, Message> {
        let fit = self.fits(tab);
        let view = DesktopView::new(pane, tab.id, Message::App)
            .interactive(self.app.dialog.is_none() && !self.tree_focused)
            .fit(fit)
            .density(self.density);
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
        let disconnect = tooltip(
            button(text(fl!("ui-desktop-disconnect")).size(SMALL_SIZE))
                .style(button::secondary)
                .on_press(Message::App(AppMessage::DisconnectDesktop(tab_id))),
            text(fl!("ui-desktop-disconnect-tooltip")).size(SMALL_SIZE),
            tooltip::Position::Bottom,
        )
        .style(container::rounded_box);
        let bar = row![
            // Beside it: below, it would cover the menu's first entry.
            tooltip(
                send_keys,
                text(fl!("ui-desktop-send-keys-tooltip")).size(SMALL_SIZE),
                tooltip::Position::Right,
            )
            .style(container::rounded_box),
            mode,
            fullscreen,
            disconnect,
        ]
        .spacing(SPACING)
        .align_y(iced::Alignment::Center);
        let bar = self.desktop_bar_end(tab, pane, bar);
        column![bar, view].spacing(SPACING / 2.0).into()
    }

    /// The end of a desktop's session bar: the resolution menu, the anti-idle badge, saving
    /// the server's files, VNC's clipboard, the desktop's name and VNC's warning.
    fn desktop_bar_end<'a>(
        &self,
        tab: &Tab,
        pane: &'a DesktopPane,
        mut bar: iced::widget::Row<'a, Message>,
    ) -> iced::widget::Row<'a, Message> {
        let tab_id = tab.id;
        // The C# session bar's resolution button: the tab's menu, its tip naming the mode,
        // in the accent colour while a size of its own is kept.
        if let Some(state) = self.resolution_state(tab) {
            let style = if state.fixed.is_some() {
                button::primary
            } else {
                button::secondary
            };
            bar = bar.push(
                tooltip(
                    button(text(fl!("ui-resolution-menu")).size(SMALL_SIZE))
                        .style(style)
                        .on_press(Message::OpenTreeMenu(TreeMenu::Resolution(tab_id))),
                    text(state.tooltip()).size(SMALL_SIZE),
                    tooltip::Position::Bottom,
                )
                .style(container::rounded_box),
            );
        }
        // Shown while the session gets anti-idle keys; a click stops them, as the C# badge.
        if self.app.anti_idle_on(tab_id) {
            bar = bar.push(
                tooltip(
                    button(text(fl!("ui-desktop-anti-idle")).size(SMALL_SIZE))
                        .style(button::text)
                        .on_press(Message::App(AppMessage::StopAntiIdle(tab_id))),
                    text(fl!("ui-desktop-anti-idle-tooltip")).size(SMALL_SIZE),
                    tooltip::Position::Bottom,
                )
                .style(container::rounded_box),
            );
        }
        if let Some(control) = save_files_control(pane, tab_id) {
            bar = bar.push(control);
        }
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
        if let Some(name) = &pane.desktop_name {
            bar = bar.push(text(name.as_str()).size(SMALL_SIZE).style(text::secondary));
        }
        if tab.purpose == Purpose::Vnc {
            // Always in sight: nothing on a VNC connection is encrypted.
            bar = bar.push(
                text(fl!("ui-session-vnc-unencrypted"))
                    .size(SMALL_SIZE)
                    .style(text::danger),
            );
        }
        bar
    }

    fn question<'a>(&'a self, tab: &'a Tab, prompt: &'a Prompt) -> Element<'a, Message> {
        let id = prompt.question;
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
                // The server asking, which is a gateway's while the route is walked, not the
                // profile's own host.
                form = form.push(text(fl!(
                    "ui-prompt-interactive-title",
                    user = asked.username.as_str(),
                    host = asked.host.as_str()
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

/// Whether `tab` shows a terminal: a session of text, connected or ended.
fn shows_terminal(tab: &Tab) -> bool {
    tab.files.is_none()
        && tab.desktop.is_none()
        && !matches!(tab.purpose, Purpose::Files | Purpose::Rdp | Purpose::Vnc)
        && matches!(tab.phase, Phase::Connected | Phase::Closed { .. })
}

fn terminal(tab: &Tab, interactive: bool, font_size: f32) -> Element<'_, Message> {
    container(
        TerminalView::new(&tab.terminal, tab.id, Message::App)
            .interactive(interactive)
            .font_size(font_size)
            .on_zoom(|zoom| Message::Shortcut(WindowShortcut::Zoom(zoom))),
    )
    .padding(TERMINAL_MARGIN)
    .into()
}

/// The question about an unknown key, as the C# Heimdall asks it: an SSH host's, or, when
/// `certificate` names the profile, an RDP or FTPS server's own certificate, with the other
/// certificates already trusted for the name and the gateways on the way. Either can be
/// trusted for this run only, never recorded, and its fingerprint copied.
fn host_key_card<'a>(
    tab: TabId,
    host: &'a str,
    port: u16,
    fingerprint: &'a str,
    certificate: Option<(&'a str, Option<&'a CertificateContext>)>,
) -> Element<'a, Message> {
    let port = port.to_string();
    let (heading, body, label, [reject, once, accept]) = match certificate {
        Some((name, context)) => (
            fl!("ui-certificate-title"),
            certificate_body(name, host, &port, context),
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
            row![
                text(label).font(iced::Font::MONOSPACE).width(Length::Fill),
                button(text(fl!("ui-hostkey-copy-fingerprint-button")).size(SMALL_SIZE))
                    .style(button::secondary)
                    .on_press(Message::App(AppMessage::CopyHostKeyFingerprint(tab))),
            ]
            .spacing(SPACING)
            .align_y(iced::alignment::Vertical::Center),
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

/// What the certificate question says above the fingerprint: who answered, the caution, the
/// certificates already trusted for the name, and the route.
fn certificate_body<'a>(
    name: &str,
    host: &str,
    port: &str,
    context: Option<&CertificateContext>,
) -> iced::widget::Column<'a, Message> {
    let mut body = column![
        text(fl!(
            "ui-certificate-body",
            name = name,
            host = host,
            port = port
        )),
        text(fl!("ui-certificate-caution")),
    ]
    .spacing(SPACING);
    let Some(context) = context else {
        return body;
    };
    if context.others > 0 {
        body = body.push(text(fl!(
            "ui-certificate-already-trusted",
            count = context.others
        )));
    }
    if !context.route.is_empty() {
        let route = context.route.join(&fl!("ui-route-test-separator"));
        body = body.push(text(fl!("ui-certificate-route", route = route)));
    }
    body
}

fn card<'a>(content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    container(content)
        .padding(PADDING)
        .max_width(CARD_WIDTH)
        .style(container::bordered_box)
        .into()
}

/// A session action's label, kept on one line: a card moves the whole button instead.
fn action_label<'a>(label: String) -> iced::widget::Text<'a> {
    text(label).wrapping(text::Wrapping::None)
}

/// A card for a table, wider than a form's.
fn wide_card<'a>(content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    container(content)
        .padding(PADDING)
        .max_width(WIDE_CARD_WIDTH)
        .style(container::bordered_box)
        .into()
}

/// The report of an import: counts, and the profiles left out with their reason.
/// The files read for the `.rdp` import, with the words it writes into names.
fn rdp_read(files: Vec<(std::path::PathBuf, Result<String, String>)>) -> Message {
    Message::App(AppMessage::Rdp(heimdall_app::RdpMessage::Read {
        files,
        names: crate::rdp_view::names(),
    }))
}

/// The handle between the sidebar and the sessions, dragged to resize the sidebar, as the
/// C# `GridSplitter`.
fn splitter<'a>() -> Element<'a, Message> {
    mouse_area(
        container(iced::widget::space().width(SPLITTER_WIDTH))
            .height(Length::Fill)
            .style(|theme: &Theme| container::Style {
                background: Some(theme.extended_palette().background.strong.color.into()),
                ..container::Style::default()
            }),
    )
    .on_press(Message::SidebarDragStart)
    .interaction(iced::mouse::Interaction::ResizingHorizontally)
    .into()
}

/// Opens `target`, a folder or a web address, with the system; one it cannot open is
/// logged, nothing else depends on it.
fn open_with_system(target: std::path::PathBuf) -> Task<Message> {
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || heimdall_app::external_edit::open_folder(&target))
                .await
                .map_err(std::io::Error::other)
                .and_then(|opened| opened)
        },
        |result| {
            if let Err(error) = result {
                log::warn!("could not open with the system: {error}");
            }
            Message::Tick
        },
    )
}

/// The tabs of the Settings page, the one shown marked, as the C# `TabControl`.
fn settings_tabs<'a>(shown: SettingsTab) -> Element<'a, Message> {
    SettingsTab::ALL
        .into_iter()
        .fold(row![].spacing(SPACING / 2.0), |tabs, tab| {
            tabs.push(
                button(text(tab.label()))
                    .style(if tab == shown {
                        button::primary
                    } else {
                        button::secondary
                    })
                    .on_press(Message::SettingsTab(tab)),
            )
        })
        .into()
}

/// The open dialog of "Import Sessions", held by the window, then the file read; a `.rdp`
/// file goes to the `.rdp` import, as the C# sends it. Nothing when no file is picked.
fn pick_sessions_file() -> Task<Message> {
    window::latest().then(|id| {
        let pick = match id {
            Some(id) => window::run(id, |window| crate::file_import_view::pick(Some(window))),
            None => Task::done(crate::file_import_view::pick(None)),
        };
        pick.then(|pick| {
            Task::future(pick).then(|picked| {
                let Some(path) = picked.map(|file| file.path().to_owned()) else {
                    return Task::none();
                };
                if crate::file_import_view::is_rdp(&path) {
                    Task::perform(crate::rdp_view::read_all(vec![path]), rdp_read)
                } else {
                    Task::perform(crate::file_import_view::read(path), |read| {
                        Message::App(AppMessage::Sessions(
                            heimdall_app::SessionsMessage::FileRead(read),
                        ))
                    })
                }
            })
        })
    })
}

/// The open dialog of the `.rdp` import, held by the window, then the files read; nothing
/// when none is picked.
fn pick_rdp() -> Task<Message> {
    let (title, filter) = (fl!("ui-rdp-title"), fl!("ui-rdp-filter"));
    window::latest().then(move |id| {
        let (title, filter) = (title.clone(), filter.clone());
        let pick = match id {
            Some(id) => window::run(id, move |window| {
                crate::rdp_view::pick(title, filter, Some(window))
            }),
            None => Task::done(crate::rdp_view::pick(title, filter, None)),
        };
        pick.then(|pick| {
            Task::future(crate::rdp_view::read_picked(pick)).then(|read| match read {
                Some(files) => Task::done(rdp_read(files)),
                None => Task::none(),
            })
        })
    })
}

/// The files copied in Explorer read off the window's thread, for `tab`.
fn read_explorer_files(tab: TabId) -> Task<Message> {
    Task::perform(
        async {
            tokio::task::spawn_blocking(explorer_files)
                .await
                .unwrap_or_default()
        },
        move |paths| {
            Message::App(AppMessage::Files(FilesMessage::ExplorerFilesRead {
                tab,
                paths,
            }))
        },
    )
}

/// The desktop bar's control for the server's copied files: a button to save them while
/// the server's clipboard holds some, then how far saving them is, with a way to stop.
fn save_files_control(pane: &DesktopPane, tab_id: TabId) -> Option<Element<'_, Message>> {
    if pane.can_save_files() {
        return Some(
            tooltip(
                button(text(fl!("ui-desktop-save-files")).size(SMALL_SIZE))
                    .style(button::secondary)
                    .on_press(Message::App(AppMessage::SaveRemoteFiles(tab_id))),
                text(fl!("ui-desktop-save-files-tooltip")).size(SMALL_SIZE),
                tooltip::Position::Bottom,
            )
            .style(container::rounded_box)
            .into(),
        );
    }
    let Some(SaveState::Running { saved, total }) = pane.save_state() else {
        return None;
    };
    Some(
        row![
            text(fl!("ui-desktop-saving-files", saved = saved, total = total)).size(SMALL_SIZE),
            button(text(fl!("ui-desktop-save-files-cancel")).size(SMALL_SIZE))
                .style(button::secondary)
                .on_press(Message::App(AppMessage::CancelSave(tab_id))),
        ]
        .spacing(SPACING / 2.0)
        .align_y(iced::Alignment::Center)
        .into(),
    )
}

/// The folder dialog of "Save copied files...", held by the window: the folder picked
/// goes to the session of `tab`.
fn pick_save_folder(tab: TabId) -> Task<Message> {
    type PickFolder =
        std::pin::Pin<Box<dyn std::future::Future<Output = Option<rfd::FileHandle>> + Send>>;
    let title = fl!("ui-desktop-save-files");
    window::latest().then(move |id| {
        let title = title.clone();
        let pick = match id {
            Some(id) => window::run(id, move |window| {
                Box::pin(
                    rfd::AsyncFileDialog::new()
                        .set_title(title)
                        .set_parent(&window)
                        .pick_folder(),
                ) as PickFolder
            }),
            None => Task::done(
                Box::pin(rfd::AsyncFileDialog::new().set_title(title).pick_folder()) as PickFolder,
            ),
        };
        pick.then(move |pick| {
            Task::future(pick).map(move |picked| {
                Message::App(AppMessage::SaveFolderPicked {
                    tab,
                    folder: picked.map(|folder| folder.path().to_owned()),
                })
            })
        })
    })
}

/// This side's clipboard for the desktop of `tab`, read off the window's thread: the files
/// copied in Explorer, or else its text.
fn read_desktop_clipboard(tab: TabId) -> Task<Message> {
    Task::perform(
        async {
            tokio::task::spawn_blocking(explorer_files)
                .await
                .unwrap_or_default()
        },
        std::convert::identity,
    )
    .then(move |paths| {
        if paths.is_empty() {
            iced::clipboard::read().then(move |text| match text.filter(|text| !text.is_empty()) {
                // No text: an image, when the clipboard holds one.
                None => Task::perform(
                    async {
                        tokio::task::spawn_blocking(clipboard_image)
                            .await
                            .ok()
                            .flatten()
                    },
                    move |image| match image {
                        Some(image) => Message::App(AppMessage::ClipboardImage { tab, image }),
                        None => Message::App(AppMessage::ClipboardText { tab, text: None }),
                    },
                ),
                text => Task::done(Message::App(AppMessage::ClipboardText { tab, text })),
            })
        } else {
            Task::done(Message::App(AppMessage::ClipboardFiles { tab, paths }))
        }
    })
}

/// The clipboard's image as a device-independent bitmap, when it holds one no larger than
/// an RDP server is offered; none elsewhere than on Windows.
fn clipboard_image() -> Option<Vec<u8>> {
    #[cfg(windows)]
    {
        use clipboard_win::{Clipboard, formats, raw};
        let _open = Clipboard::new_attempts(CLIPBOARD_ATTEMPTS).ok()?;
        let size = raw::size(formats::CF_DIB)?.get();
        if size > heimdall_rdp::MAX_IMAGE_BYTES {
            return None;
        }
        let mut image = Vec::with_capacity(size);
        raw::get_vec(formats::CF_DIB, &mut image).ok()?;
        Some(image)
    }
    #[cfg(not(windows))]
    {
        None
    }
}

/// Puts `image`, a device-independent bitmap an RDP server copied, on the clipboard in
/// place of what it held. Windows only: elsewhere it goes nowhere.
fn write_clipboard_image(image: &[u8]) {
    #[cfg(windows)]
    {
        use clipboard_win::{Clipboard, formats, raw};
        // Another program may hold the clipboard a moment: asked again a little later.
        for _ in 0..CLIPBOARD_ATTEMPTS {
            if let Ok(_open) = Clipboard::new() {
                if let Err(error) = raw::set(formats::CF_DIB, image) {
                    log::warn!("the server's image did not reach the clipboard: {error}");
                }
                return;
            }
            std::thread::sleep(CLIPBOARD_RETRY);
        }
        log::warn!("the server's image did not reach the clipboard: it stayed held");
    }
    #[cfg(not(windows))]
    {
        let _ = image;
    }
}

/// How many times the clipboard is asked for, another program holding it.
#[cfg(windows)]
const CLIPBOARD_ATTEMPTS: usize = 10;

/// How long before the clipboard is asked for again.
#[cfg(windows)]
const CLIPBOARD_RETRY: std::time::Duration = std::time::Duration::from_millis(20);

/// The files copied in Explorer, as the clipboard lists them; none elsewhere than on
/// Windows, or when it holds none.
fn explorer_files() -> Vec<std::path::PathBuf> {
    #[cfg(windows)]
    {
        clipboard_win::get_clipboard::<Vec<String>, _>(clipboard_win::formats::FileList)
            .unwrap_or_default()
            .into_iter()
            .map(std::path::PathBuf::from)
            .collect()
    }
    #[cfg(not(windows))]
    {
        Vec::new()
    }
}

/// The open dialog of "Upload here...", held by the window: the files picked go to the
/// tab; nothing when none is.
fn pick_uploads(tab: TabId) -> Task<Message> {
    let title = fl!("ui-files-menu-upload-here");
    window::latest().then(move |id| {
        let title = title.clone();
        let pick = match id {
            Some(id) => window::run(id, move |window| {
                Box::pin(
                    rfd::AsyncFileDialog::new()
                        .set_title(title)
                        .set_parent(&window)
                        .pick_files(),
                ) as crate::rdp_view::Pick
            }),
            None => Task::done(
                Box::pin(rfd::AsyncFileDialog::new().set_title(title).pick_files())
                    as crate::rdp_view::Pick,
            ),
        };
        pick.then(move |pick| {
            Task::future(pick).then(move |picked| match picked {
                Some(files) => Task::done(Message::App(AppMessage::Files(
                    FilesMessage::UploadPicked {
                        tab,
                        paths: files.iter().map(|file| file.path().to_owned()).collect(),
                    },
                ))),
                None => Task::none(),
            })
        })
    })
}

/// The open dialog of the `known_hosts` import, held by the window, then the file read;
/// nothing when no file is picked.
fn pick_known_hosts() -> Task<Message> {
    window::latest().then(|id| {
        let pick = match id {
            Some(id) => window::run(id, |window| crate::hostkeys_view::pick(Some(window))),
            None => Task::done(crate::hostkeys_view::pick(None)),
        };
        pick.then(|pick| {
            Task::future(crate::hostkeys_view::read(pick)).then(|read| match read {
                Some(read) => Task::done(crate::hostkeys_view::app(
                    heimdall_app::HostKeysMessage::Read(read),
                )),
                None => Task::none(),
            })
        })
    })
}

/// The open dialog of the OpenSSH import, held by the window, then the file read; nothing
/// when no file is picked.
fn pick_openssh() -> Task<Message> {
    let title = fl!("ui-openssh-title");
    window::latest().then(move |id| {
        let title = title.clone();
        let pick = match id {
            Some(id) => window::run(id, move |window| {
                crate::sessions_view::pick(title, Some(window))
            }),
            None => Task::done(crate::sessions_view::pick(title, None)),
        };
        pick.then(|pick| {
            Task::future(crate::sessions_view::read(pick)).then(|read| match read {
                Some(read) => Task::done(Message::App(AppMessage::Sessions(
                    heimdall_app::SessionsMessage::Read(read),
                ))),
                None => Task::none(),
            })
        })
    })
}

/// The save dialog of an export, held by the window, then the file written.
fn save_export(document: String, count: usize) -> Task<Message> {
    let (title, filter) = (fl!("ui-dialog-export-title"), fl!("ui-export-filter-json"));
    window::latest().then(move |id| {
        let (title, filter) = (title.clone(), filter.clone());
        let pick = match id {
            Some(id) => window::run(id, move |window| {
                crate::export_file::dialog(title, filter, Some(window))
            }),
            None => Task::done(crate::export_file::dialog(title, filter, None)),
        };
        let document = document.clone();
        pick.then(move |pick| {
            Task::perform(
                crate::export_file::save(pick, document.clone(), count),
                |outcome| Message::App(AppMessage::ExportFinished(outcome)),
            )
        })
    })
}

/// A report under its title, closed by OK: how the export went, as the C# message box
/// says it, or why the import or a password save failed, with the technical detail.
fn report<'a>(dialog: &Dialog, ok: iced::widget::Button<'a, Message>) -> Element<'a, Message> {
    let detail = |detail: &str| text(fl!("ui-dialog-detail", detail = detail)).size(SMALL_SIZE);
    let (title, lines) = match dialog {
        Dialog::ExportDone { count } => (
            fl!("ui-dialog-export-title"),
            vec![
                text(fl!("ui-dialog-export-done", count = count.to_owned())),
                text(fl!("ui-dialog-export-credentials")),
            ],
        ),
        Dialog::ExportFailed { detail } => (
            fl!("ui-dialog-export-title"),
            vec![text(fl!(
                "ui-dialog-export-failed",
                detail = detail.as_str()
            ))],
        ),
        Dialog::ImportFailed { detail: technical } => (
            fl!("ui-dialog-import-failed-title"),
            vec![detail(technical)],
        ),
        Dialog::PasswordSaveFailed { detail: technical } => {
            (fl!("ui-vault-save-failed-title"), vec![detail(technical)])
        }
        Dialog::ImportNothing { skipped, warnings } => (
            fl!("ui-import-file-title"),
            std::iter::once(fl!("ui-import-file-nothing"))
                .chain(crate::file_import_view::warning_lines(warnings))
                .chain(skipped.iter().map(|(name, reason)| {
                    fl!(
                        "ui-dialog-import-skipped-item",
                        name = server_text(name),
                        reason = texts::skip_reason(reason)
                    )
                }))
                .map(text)
                .collect(),
        ),
        Dialog::StoreError { detail: technical } => (
            fl!("ui-dialog-store-title"),
            vec![text(fl!("ui-dialog-store-body")), detail(technical)],
        ),
        other => {
            let (title, lines) = crate::rdp_view::report_lines(other)
                .map(|lines| (fl!("ui-rdp-title"), lines))
                .or_else(|| crate::sessions_view::report_lines(other))
                .or_else(|| crate::hostkeys_view::report_lines(other))
                .unwrap_or_default();
            (title, lines.into_iter().map(text).collect())
        }
    };
    column![text(title).size(HEADING_SIZE)]
        .extend(lines.into_iter().map(Element::from))
        .push(ok)
        .spacing(SPACING)
        .into()
}

/// The preview of an import, a table of what the file gives to choose from.
fn import_preview(dialog: &Dialog) -> Element<'_, Message> {
    match dialog {
        Dialog::SessionsPreview(preview) => crate::sessions_view::preview(preview),
        Dialog::RdpPreview(preview) => crate::rdp_view::preview(preview),
        Dialog::HostKeysPreview(preview) => crate::hostkeys_view::preview(preview),
        Dialog::ConfirmImportFile(pending) => {
            let (title, body, action) = crate::file_import_view::question(pending);
            column![
                text(title).size(HEADING_SIZE),
                text(body),
                row![
                    button(text(fl!("ui-dialog-cancel-button")))
                        .style(button::secondary)
                        .on_press(Message::App(AppMessage::DismissDialog)),
                    button(text(action)).on_press(Message::App(AppMessage::ConfirmDialog)),
                ]
                .spacing(SPACING),
            ]
            .spacing(SPACING)
            .into()
        }
        _ => column![].into(),
    }
}

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
    if !summary.dropped.is_empty() {
        let dropped = summary.dropped.iter().fold(
            Column::new().spacing(SPACING / 2.0),
            |list, (name, settings)| {
                let settings: Vec<String> = settings
                    .iter()
                    .map(|setting| texts::dropped_setting(*setting))
                    .collect();
                list.push(
                    text(fl!(
                        "ui-dialog-import-dropped-item",
                        name = name.as_str(),
                        settings = settings.join(&fl!("ui-dialog-import-dropped-separator"))
                    ))
                    .size(SMALL_SIZE),
                )
            },
        );
        content = content
            .push(text(fl!("ui-dialog-import-dropped")))
            .push(container(scrollable(dropped)).max_height(SKIPPED_LIST_HEIGHT));
    }
    match &summary.host_keys {
        Some(Ok(carried)) if carried.keys + carried.pins > 0 => {
            content = content.push(
                text(fl!(
                    "ui-dialog-import-host-keys",
                    keys = carried.keys,
                    pins = carried.pins
                ))
                .size(SMALL_SIZE),
            );
        }
        Some(Err(detail)) => {
            content = content.push(
                text(fl!(
                    "ui-dialog-import-host-keys-failed",
                    detail = detail.as_str()
                ))
                .size(SMALL_SIZE)
                .style(text::danger),
            );
        }
        _ => {}
    }
    // What the file said, and for `MobaXterm` that its passwords must be entered again.
    for line in crate::file_import_view::warning_lines(&summary.warnings)
        .into_iter()
        .chain(crate::file_import_view::password_notice(
            summary.stored_credentials,
        ))
    {
        content = content.push(text(line).size(SMALL_SIZE));
    }
    content.push(ok).into()
}

/// What the dialogs show that the window holds.
struct Forms<'a> {
    /// The vault dialog's fields.
    vault: &'a [Zeroizing<String>; 3],
    /// The profile form's password.
    profile_password: &'a str,
    /// The password typed in sudo's question.
    sudo_password: &'a str,
    /// The profile form's key passphrase.
    profile_passphrase: &'a str,
    /// The gateway dialog's password.
    gateway_password: &'a str,
    /// The gateway dialog's key passphrase.
    gateway_passphrase: &'a str,
    /// Saved SSH gateways, for the lists to choose from.
    gateways: &'a [SshGateway],
    /// What stops the "New tunnel" dialog's tunnel from opening, if anything.
    tunnel_problem: Option<heimdall_app::tunnel::TunnelProblem>,
    /// What the SSH agent chip knows.
    agent_chip: &'a heimdall_app::AgentChip,
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
        DraftProtocol::Sftp => fl!("ui-profile-protocol-sftp-name"),
        DraftProtocol::WinRm => fl!("ui-profile-protocol-winrm-name"),
        DraftProtocol::Vnc => fl!("ui-profile-protocol-vnc-name"),
        DraftProtocol::Telnet => fl!("ui-profile-protocol-telnet-name"),
        DraftProtocol::Ftp => fl!("ui-profile-protocol-ftp-name"),
        DraftProtocol::Local => fl!("ui-profile-protocol-local-name"),
    }
}

fn protocol_description(protocol: DraftProtocol) -> String {
    match protocol {
        DraftProtocol::Rdp => fl!("ui-profile-protocol-rdp-desc"),
        DraftProtocol::Ssh => fl!("ui-profile-protocol-ssh-desc"),
        DraftProtocol::Sftp => fl!("ui-profile-protocol-sftp-desc"),
        DraftProtocol::WinRm => fl!("ui-profile-protocol-winrm-desc"),
        DraftProtocol::Vnc => fl!("ui-profile-protocol-vnc-desc"),
        DraftProtocol::Telnet => fl!("ui-profile-protocol-telnet-desc"),
        DraftProtocol::Ftp => fl!("ui-profile-protocol-ftp-desc"),
        DraftProtocol::Local => fl!("ui-profile-protocol-local-desc"),
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
                DraftProtocol::Ssh | DraftProtocol::Sftp => fl!("ui-profile-port-ssh"),
                DraftProtocol::WinRm => fl!("ui-profile-port-winrm"),
                DraftProtocol::Vnc => fl!("ui-profile-port-vnc"),
                // Not shown: a local shell has no port.
                DraftProtocol::Ftp => fl!("ui-profile-port-ftp"),
                DraftProtocol::Telnet | DraftProtocol::Local => fl!("ui-profile-port-telnet"),
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
        ProfileField::FixedWidth => (fl!("ui-profile-resolution-width"), String::new()),
        ProfileField::FixedHeight => (fl!("ui-profile-resolution-height"), String::new()),
        ProfileField::VaultEntry => (
            fl!("ui-profile-field-vault-entry"),
            fl!("ui-profile-vault-entry-placeholder"),
        ),
        ProfileField::SocksPort => (fl!("ui-profile-socks-port"), PORT_OFF.to_string()),
        ProfileField::RemoteBindPort => (fl!("ui-profile-remote-bind-port"), PORT_OFF.to_string()),
        ProfileField::RemoteLocalPort => {
            (fl!("ui-profile-remote-local-port"), PORT_OFF.to_string())
        }
        ProfileField::LocalProgram => (
            fl!("ui-profile-local-executable"),
            fl!("ui-profile-local-default-shell"),
        ),
        ProfileField::LocalArguments => (fl!("ui-profile-local-arguments"), String::new()),
        ProfileField::WorkingDirectory => (
            fl!("ui-profile-local-working-directory"),
            fl!("ui-profile-optional"),
        ),
        ProfileField::Tags => (
            fl!("ui-profile-field-tags"),
            fl!("ui-profile-tags-placeholder"),
        ),
        ProfileField::MacAddress => (
            fl!("ui-profile-field-mac-address"),
            fl!("ui-profile-mac-address-placeholder"),
        ),
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

/// The SSH key field, with the C# "Browse..." button beside it.
fn key_field(draft: &ProfileDraft) -> Element<'_, Message> {
    let field = ProfileField::KeyPath;
    column![
        text(fl!("ui-profile-field-key")).size(SMALL_SIZE),
        row![
            text_input(&fl!("ui-profile-optional"), draft.value(field))
                .id(profile_field_id(field))
                .on_input(move |value| Message::App(AppMessage::ProfileField { field, value }))
                .on_submit(Message::SaveProfileForm),
            button(text(fl!("ui-profile-browse-button")))
                .style(button::secondary)
                .on_press(Message::BrowseKeyFile),
        ]
        .spacing(SPACING)
        .align_y(iced::Alignment::Center),
    ]
    .spacing(SPACING / 2.0)
    .into()
}

/// The open dialog of the profile form's SSH key, held by the window, in `~/.ssh`: every
/// file first, since an OpenSSH key has no extension, then the C# `.ppk` and `.pem`. The
/// path picked fills the field; nothing when none is picked.
fn pick_key_file() -> Task<Message> {
    let dialog = || {
        let mut dialog = rfd::AsyncFileDialog::new()
            .set_title(fl!("ui-profile-browse-key-title"))
            .add_filter(fl!("ui-profile-browse-key-all"), &["*"])
            .add_filter(fl!("ui-profile-browse-key-ppk"), &["ppk"])
            .add_filter(fl!("ui-profile-browse-key-pem"), &["pem"]);
        if let Some(folder) = std::env::home_dir()
            .map(|home| home.join(crate::sessions_view::SSH_FOLDER))
            .filter(|folder| folder.is_dir())
        {
            dialog = dialog.set_directory(folder);
        }
        dialog
    };
    window::latest().then(move |id| {
        let pick = match id {
            Some(id) => window::run(id, move |window| {
                let pick: crate::sessions_view::Pick =
                    Box::pin(dialog().set_parent(&window).pick_file());
                pick
            }),
            None => Task::done(Box::pin(dialog().pick_file()) as crate::sessions_view::Pick),
        };
        pick.then(|pick| {
            Task::future(pick).then(|picked| match picked {
                Some(file) => Task::done(Message::App(AppMessage::ProfileField {
                    field: ProfileField::KeyPath,
                    value: file.path().display().to_string(),
                })),
                None => Task::none(),
            })
        })
    })
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

/// The C# hint under `toggle`, for the boxes the C# dialog explains.
fn toggle_hint(toggle: ProfileToggle) -> Option<String> {
    match toggle {
        ProfileToggle::UseSsl => Some(fl!("ui-profile-use-ssl-hint")),
        ProfileToggle::SkipCertificateCheck => Some(fl!("ui-profile-skip-cert-hint")),
        _ => None,
    }
}

fn toggle_label(toggle: ProfileToggle) -> String {
    match toggle {
        ProfileToggle::RedirectClipboard => fl!("ui-profile-toggle-clipboard"),
        ProfileToggle::FollowDefaults => fl!("ui-profile-rdp-follow-defaults"),
        ProfileToggle::SeveralServers => fl!("ui-profile-toggle-several-servers"),
        ProfileToggle::StrictServerAuthentication => fl!("ui-profile-toggle-strict-server-auth"),
        ProfileToggle::AntiIdle => fl!("ui-profile-toggle-anti-idle"),
        ProfileToggle::AutoReconnect => fl!("ui-profile-toggle-auto-reconnect"),
        ProfileToggle::RedirectDrives => fl!("ui-profile-toggle-drives"),
        ProfileToggle::Nla => fl!("ui-profile-toggle-nla"),
        ProfileToggle::StoredCredential => fl!("ui-profile-winrm-identity-stored"),
        ProfileToggle::UseSsl => fl!("ui-profile-toggle-use-ssl"),
        ProfileToggle::SkipCertificateCheck => fl!("ui-profile-toggle-skip-cert"),
        ProfileToggle::ViewOnly => fl!("ui-profile-toggle-view-only"),
        ProfileToggle::AllowNoPassword => fl!("ui-profile-toggle-no-password"),
        ProfileToggle::DirectConnection => fl!("ui-profile-direct-connect"),
        ProfileToggle::AdminSession => fl!("ui-profile-toggle-admin"),
        ProfileToggle::ForwardAgent => fl!("ui-profile-toggle-forward-agent"),
        ProfileToggle::Compression => fl!("ui-profile-toggle-compression"),
        ProfileToggle::LegacyAlgorithms => fl!("ui-profile-toggle-legacy-algorithms"),
        ProfileToggle::Passive => fl!("ui-profile-toggle-passive"),
        ProfileToggle::Tls => fl!("ui-profile-toggle-ftps"),
        ProfileToggle::Favorite => fl!("ui-profile-toggle-favorite"),
    }
}

/// The task running a gateway route test: each step, then its end.
fn route_test_task(run: u64, request: heimdall_app::route_test::RouteTestRequest) -> Task<Message> {
    // Started once the task runs, on the runtime.
    let events =
        stream::once(async move { heimdall_app::route_test::route_test_events(request) }).flatten();
    Task::stream(events).map(move |step| {
        Message::App(match step {
            Some(step) => AppMessage::RouteStep { run, step },
            None => AppMessage::RouteTestDone { run },
        })
    })
}

/// The words of the note templates, in the language shown.
fn note_labels() -> heimdall_app::notes::NoteLabels {
    heimdall_app::notes::NoteLabels {
        working_note: fl!("ui-notes-tpl-working-note"),
        notes: fl!("ui-notes-tpl-notes"),
        commands: fl!("ui-notes-tpl-commands"),
        next: fl!("ui-notes-tpl-next"),
        daily_note: fl!("ui-notes-tpl-daily-note"),
        focus: fl!("ui-notes-tpl-focus"),
        journal: fl!("ui-notes-tpl-journal"),
        follow_up: fl!("ui-notes-tpl-follow-up"),
        incident: fl!("ui-notes-tpl-incident"),
        incident_report: fl!("ui-notes-tpl-incident-report"),
        summary: fl!("ui-notes-tpl-summary"),
        impact: fl!("ui-notes-tpl-impact"),
        timeline: fl!("ui-notes-tpl-timeline"),
        incident_started: fl!("ui-notes-tpl-incident-started"),
        investigation: fl!("ui-notes-tpl-investigation"),
        actions: fl!("ui-notes-tpl-actions"),
        resolution: fl!("ui-notes-tpl-resolution"),
        procedure: fl!("ui-notes-tpl-procedure"),
        purpose: fl!("ui-notes-tpl-purpose"),
        scope: fl!("ui-notes-tpl-scope"),
        preconditions: fl!("ui-notes-tpl-preconditions"),
        steps: fl!("ui-notes-tpl-steps"),
        validation: fl!("ui-notes-tpl-validation"),
        rollback: fl!("ui-notes-tpl-rollback"),
        references: fl!("ui-notes-tpl-references"),
    }
}

/// The task asking what answers: an address, from the tree or the profile form, or the SSH
/// agents, for the form's chip.
fn probe_task(effect: Effect) -> Task<Message> {
    match effect {
        Effect::SurveyAgents(source) => Task::perform(
            async move { heimdall_ssh::survey_agents(&source).await },
            |found| Message::App(AppMessage::AgentsSurveyed(found)),
        ),
        Effect::WakeOnLan(mac) => Task::perform(heimdall_app::wake_on_lan::send(mac), |sent| {
            Message::App(AppMessage::ProfileMenu(ProfileMenuMessage::WakeOnLanSent(
                sent.map_err(|error| error.to_string()),
            )))
        }),
        Effect::TestReachability { host, port } => Task::perform(
            heimdall_app::reachability::test_from_tree(host.clone(), port),
            move |result| {
                Message::App(AppMessage::ProfileMenu(ProfileMenuMessage::Tested {
                    host: host.clone(),
                    port,
                    result,
                }))
            },
        ),
        Effect::TestAddress {
            test,
            host,
            port,
            ssh,
            cancel,
        } => Task::perform(
            heimdall_app::reachability::test(host, port, ssh, cancel),
            move |result| Message::App(AppMessage::AddressTested { test, result }),
        ),
        _ => Task::none(),
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
fn network_section<'a>(
    draft: &'a ProfileDraft,
    gateways: &'a [SshGateway],
) -> Element<'a, Message> {
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
    // Said where the SSL box was, as the C# dialog says it; HTTPS asked for, then a gateway
    // chosen, is said to be off.
    if draft.protocol == DraftProtocol::WinRm && draft.routed_gateway().is_some() {
        section_column =
            section_column.push(text(fl!("ui-profile-winrm-gateway-http")).size(SMALL_SIZE));
        if draft.is_on(ProfileToggle::UseSsl) {
            section_column = section_column.push(
                text(fl!("ui-profile-winrm-https-off-by-gateway"))
                    .size(SMALL_SIZE)
                    .style(text::warning),
            );
        }
    }
    if draft.shows(ProfileField::SocksPort) {
        section_column = forward_cards(draft, section_column);
    }
    section_column.into()
}

/// A port typed in a forward's field, when it opens one.
fn typed_port(typed: &str) -> Option<u16> {
    typed.trim().parse::<u16>().ok().filter(|port| *port != 0)
}

/// The C# SOCKS5 and remote forwarding cards, shown only through a gateway: each port's
/// field and, under it, what it opens.
fn forward_cards<'a>(draft: &'a ProfileDraft, column: Column<'a, Message>) -> Column<'a, Message> {
    let port_field =
        |field: ProfileField| container(form_field(draft, field)).width(PORT_FIELD_WIDTH);
    let listening = typed_port(&draft.socks_port).map_or_else(
        || fl!("ui-profile-socks-off"),
        |port| display_address(LOOPBACK, port),
    );
    let route = typed_port(&draft.remote_bind_port).map_or_else(
        || fl!("ui-profile-socks-off"),
        |remote| {
            let local = typed_port(&draft.remote_local_port).unwrap_or(remote);
            fl!(
                "ui-profile-remote-route",
                remote = remote.to_string(),
                local = local.to_string()
            )
        },
    );
    column
        .push(section(
            fl!("ui-profile-socks-title"),
            Some(fl!("ui-profile-socks-desc")),
        ))
        .push(port_field(ProfileField::SocksPort))
        .push(text(listening).size(SMALL_SIZE))
        .push(section(
            fl!("ui-profile-remote-title"),
            Some(fl!("ui-profile-remote-desc")),
        ))
        .push(
            row![
                port_field(ProfileField::RemoteBindPort),
                port_field(ProfileField::RemoteLocalPort),
            ]
            .spacing(SPACING),
        )
        .push(text(fl!("ui-profile-remote-local-hint")).size(SMALL_SIZE))
        .push(text(route).size(SMALL_SIZE))
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
        .push(passphrase_field(
            &Passphrase {
                saved: draft.passphrase == SavedSecret::Saved,
                typed: forms.gateway_passphrase,
                on_input: Message::GatewayPassphrase,
                submit: Message::SaveGatewayForm,
                clear: AppMessage::ClearGatewayPassphrase,
                id: "gateway-passphrase",
            },
            forms,
        ))
        .push(parent_gateway(draft, forms))
        .push(crate::route_test_view::card(draft, forms.gateways));
    if let Some(error) = error {
        form = form.push(text(texts::draft_error(error)).style(text::danger));
    }
    // What is tested is what is saved: not while the test runs.
    let testing = matches!(
        draft.route_test,
        heimdall_app::gateway_draft::RouteTest::Running(_)
    );
    form.push(
        row![
            iced::widget::space::horizontal(),
            button(text(fl!("ui-dialog-cancel-button")))
                .style(button::secondary)
                .on_press(Message::App(AppMessage::DismissDialog)),
            button(text(fl!("ui-profile-save-button")))
                .on_press_maybe((!testing).then_some(Message::SaveGatewayForm)),
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

/// A key passphrase field, of the profile form or of the gateway dialog.
struct Passphrase<'a> {
    /// A passphrase is saved.
    saved: bool,
    /// What is typed.
    typed: &'a str,
    /// What typing sends.
    on_input: fn(String) -> Message,
    /// What Enter sends.
    submit: Message,
    /// What clearing the saved one sends.
    clear: AppMessage,
    /// The field's widget identifier.
    id: &'static str,
}

/// The passphrase of the key file, as the C# dialog shows it: an empty field whatever is
/// saved, "Passphrase saved" and a button to remove it when one is, then the C# hint.
fn passphrase_field<'a>(passphrase: &Passphrase<'a>, forms: &Forms<'a>) -> Column<'a, Message> {
    let mut input = text_input("", passphrase.typed)
        .id(iced::widget::Id::new(passphrase.id))
        .secure(true);
    if forms.passwords == PasswordStore::Ready {
        input = input
            .on_input(passphrase.on_input)
            .on_submit(passphrase.submit.clone());
    }
    let mut field = column![
        text(fl!("ui-profile-field-passphrase")).size(SMALL_SIZE),
        input
    ]
    .spacing(SPACING / 2.0);
    match forms.passwords {
        PasswordStore::Ready if passphrase.saved => {
            field = field.push(
                row![
                    text(fl!("ui-profile-passphrase-saved")).size(SMALL_SIZE),
                    tooltip(
                        button(text(fl!("ui-profile-password-clear")).size(SMALL_SIZE))
                            .style(button::text)
                            .on_press(Message::App(passphrase.clear.clone())),
                        text(fl!("ui-profile-passphrase-clear-tooltip")).size(SMALL_SIZE),
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
    field.push(text(fl!("ui-profile-passphrase-hint")).size(SMALL_SIZE))
}

/// What is typed into a secret field, taken out of the window; `None` when nothing is.
fn typed_secret(field: &mut Zeroizing<String>) -> Option<Secret> {
    let typed = std::mem::take(&mut **field);
    (!typed.is_empty()).then(|| Secret::new(typed))
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
        DraftProtocol::Ssh | DraftProtocol::Sftp => Some((
            fl!("ui-profile-credentials-ssh"),
            Some(fl!("ui-profile-credentials-ssh-desc")),
        )),
        DraftProtocol::WinRm => Some((
            fl!("ui-profile-credentials-winrm"),
            Some(fl!("ui-profile-credentials-winrm-desc")),
        )),
        DraftProtocol::Vnc => Some((fl!("ui-profile-credentials-vnc"), None)),
        DraftProtocol::Ftp => Some((
            fl!("ui-profile-credentials-ftp"),
            Some(fl!("ui-profile-credentials-ftp-desc")),
        )),
        DraftProtocol::Telnet | DraftProtocol::Local => None,
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
                // How the identity is proven, and what HTTP outside a domain needs, as the
                // C# dialog's hints.
                text(fl!("ui-profile-winrm-identity-hint")).size(SMALL_SIZE),
                text(fl!("ui-profile-winrm-trusted-hosts-hint")).size(SMALL_SIZE),
            ]
            .spacing(SPACING / 2.0),
        );
    }
    for field in [ProfileField::Username, ProfileField::Domain] {
        if draft.shows(field) {
            form = form.push(form_field(draft, field));
        }
    }
    if draft.shows(ProfileField::KeyPath) {
        form = form.push(key_field(draft));
        if matches!(draft.protocol, DraftProtocol::Ssh | DraftProtocol::Sftp)
            && let Some(chip) = crate::agent_chip_view::view(forms.agent_chip)
        {
            form = form.push(chip);
        }
        if draft.protocol.has_key_file() {
            form = form.push(passphrase_field(
                &Passphrase {
                    saved: draft.passphrase == SavedSecret::Saved,
                    typed: forms.profile_passphrase,
                    on_input: Message::ProfilePassphrase,
                    submit: Message::SaveProfileForm,
                    clear: AppMessage::ClearPassphrase,
                    id: "profile-passphrase",
                },
                forms,
            ));
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
        DraftProtocol::Ftp => Some(fl!("ui-profile-options-ftp")),
        DraftProtocol::Ssh | DraftProtocol::Sftp => Some(fl!("ui-profile-options-ssh")),
        DraftProtocol::WinRm | DraftProtocol::Local => None,
    };
    if let Some(options) = options {
        form = form.push(section(options, None));
    }
    if draft.protocol == DraftProtocol::Rdp {
        // As the C# card: the choice first; the profile's own options stay shown, said not to
        // be the ones in effect.
        form = form.push(toggle_box(
            draft,
            ProfileToggle::FollowDefaults,
            toggle_label(ProfileToggle::FollowDefaults),
        ));
        if draft.is_on(ProfileToggle::FollowDefaults) {
            form = form
                .push(text(fl!("ui-profile-rdp-defaults-banner")).size(SMALL_SIZE))
                .push(text(fl!("ui-profile-rdp-defaults-not-in-effect")).size(SMALL_SIZE));
        }
        form = form.push(crate::rdp_options::view(draft.rdp_options)).push(
            crate::rdp_options::resolution(draft, |field| form_field(draft, field)),
        );
    }
    for toggle in ProfileToggle::of(draft.protocol) {
        if *toggle != ProfileToggle::StoredCredential && draft.shows_toggle(*toggle) {
            form = form.push(toggle_box(draft, *toggle, toggle_label(*toggle)));
            // What the box does, under it, as the C# dialog's hint.
            if let Some(hint) = toggle_hint(*toggle) {
                form = form.push(text(hint).size(SMALL_SIZE));
            }
        }
    }
    if draft.shows_session_logging() {
        form = form.push(session_logging_choice(draft));
    }
    // TLS to the plaintext port: said, not corrected, as the C# schema check reports it.
    if draft.protocol == DraftProtocol::WinRm
        && draft.uses_ssl()
        && draft.port.trim() == heimdall_core::profile::DEFAULT_WINRM_HTTP_PORT.to_string()
    {
        form = form.push(
            text(fl!(
                "ui-profile-winrm-tls-on-http-port",
                http = heimdall_core::profile::DEFAULT_WINRM_HTTP_PORT,
                https = heimdall_core::profile::DEFAULT_WINRM_HTTPS_PORT
            ))
            .size(SMALL_SIZE)
            .style(text::danger),
        );
    }
    if matches!(draft.protocol, DraftProtocol::Ssh | DraftProtocol::Sftp)
        && draft.is_on(ProfileToggle::LegacyAlgorithms)
    {
        form = form.push(
            text(fl!("ui-profile-legacy-algorithms-hint"))
                .size(SMALL_SIZE)
                .style(text::danger),
        );
    }
    if draft.protocol == DraftProtocol::Rdp && !draft.is_on(ProfileToggle::Nla) {
        form = form.push(
            text(fl!("ui-profile-nla-off-hint"))
                .size(SMALL_SIZE)
                .style(text::danger),
        );
    }
    if draft.protocol == DraftProtocol::Rdp {
        form = form.push(crate::rdp_options::experience(draft.rdp_options));
    }
    if draft.protocol == DraftProtocol::Ssh {
        form = form.push(crate::post_connect_form::view(&draft.post_connect));
    }
    if draft.protocol == DraftProtocol::Local {
        form = form.push(crate::local_form::view(draft, |field| {
            form_field(draft, field)
        }));
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
            Some(if draft.protocol == DraftProtocol::Local {
                fl!("ui-profile-section-basics-local-desc")
            } else {
                fl!("ui-profile-section-basics-desc")
            })
        ),
        form_field(draft, ProfileField::Name),
    ]
    .spacing(SPACING);
    // A local shell has no server.
    if draft.shows(ProfileField::Host) {
        form = form
            .push(
                row![
                    container(form_field(draft, ProfileField::Host)).width(Length::Fill),
                    container(form_field(draft, ProfileField::Port)).width(PORT_FIELD_WIDTH),
                ]
                .spacing(SPACING),
            )
            .push(crate::address_test_view::view(draft, forms.gateways));
    }

    form = form
        .push(credentials_section(draft, forms))
        .push(options_section(draft));

    if draft.protocol.routes_through_gateway() {
        form = form.push(network_section(draft, forms.gateways));
    }

    // Organization.
    form = form
        .push(section(fl!("ui-profile-section-organization"), None))
        .push(form_field(draft, ProfileField::Group))
        // As the C#: the separator is taught by the example and by a sentence that stays.
        .push(text(fl!("ui-profile-folder-hint")).size(SMALL_SIZE));
    form = form.push(metadata_fields(draft));
    // With the C# metadata: the password manager's entry, for the protocols it serves.
    if draft.shows(ProfileField::VaultEntry) {
        form = form
            .push(form_field(draft, ProfileField::VaultEntry))
            .push(text(fl!("ui-profile-vault-entry-help")).size(SMALL_SIZE));
    }
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

/// Asks for a name, or for permission bits in octal: Enter in the field confirms, like the
/// button.
fn name_dialog(action: NameAction, value: &str) -> Element<'_, Message> {
    let (title, confirm) = match action {
        NameAction::Permissions => (
            fl!("ui-dialog-permissions-title"),
            fl!("ui-dialog-permissions-confirm"),
        ),
        NameAction::NewFolder => (
            fl!("ui-dialog-new-folder-title"),
            fl!("ui-dialog-new-folder-confirm"),
        ),
        NameAction::Rename => (
            fl!("ui-dialog-rename-title"),
            fl!("ui-dialog-rename-confirm"),
        ),
    };
    let (label, placeholder) = if action == NameAction::Permissions {
        (
            Some(text(fl!("ui-dialog-permissions-label"))),
            fl!("ui-dialog-permissions-placeholder"),
        )
    } else {
        (None, fl!("ui-dialog-name-placeholder"))
    };
    column![
        text(title).size(HEADING_SIZE),
        column![].push(label),
        text_input(&placeholder, value)
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

/// A new name for a profile, its present one written in.
/// One value for `count` profiles, as the C# bulk edit dialog asks it: what and for how
/// many, the field, empty and saying so when they differed, and why a value was refused.
fn bulk_edit_dialog(
    field: heimdall_app::BulkField,
    count: usize,
    value: &str,
    mixed: bool,
    refused: Option<heimdall_app::BulkRefusal>,
) -> Element<'_, Message> {
    use heimdall_app::{BulkField, BulkRefusal};

    let (header, label, mixed_hint) = match field {
        BulkField::Port => (
            fl!("ui-bulk-port-header", count = count),
            fl!("ui-bulk-port-label"),
            fl!("ui-bulk-port-mixed"),
        ),
        BulkField::Username => (
            fl!("ui-bulk-username-header", count = count),
            fl!("ui-bulk-username-label"),
            fl!("ui-bulk-username-mixed"),
        ),
    };
    let placeholder = if mixed { mixed_hint } else { String::new() };
    let mut body = column![
        text(header).size(HEADING_SIZE),
        text(label),
        text_input(&placeholder, value)
            .id(name_field_id())
            .on_input(|value| {
                Message::App(AppMessage::Selection(SelectionMessage::BulkEdited(value)))
            })
            .on_submit(Message::App(AppMessage::ConfirmDialog)),
    ]
    .spacing(SPACING);
    if let Some(refused) = refused {
        body = body.push(
            text(match refused {
                BulkRefusal::Port => fl!("ui-bulk-port-invalid"),
                BulkRefusal::Username => fl!("ui-bulk-username-invalid"),
            })
            .style(text::danger),
        );
    }
    body.push(
        row![
            button(text(fl!("ui-dialog-cancel-button")))
                .style(button::secondary)
                .on_press(Message::App(AppMessage::DismissDialog)),
            button(text(fl!("ui-dialog-ok-button")))
                .on_press(Message::App(AppMessage::ConfirmDialog)),
        ]
        .spacing(SPACING),
    )
    .into()
}

fn rename_profile_dialog(value: &str) -> Element<'_, Message> {
    column![
        text(fl!("ui-tree-rename-title")).size(HEADING_SIZE),
        text_input(&fl!("ui-dialog-name-placeholder"), value)
            .id(name_field_id())
            .on_input(|value| {
                Message::App(AppMessage::ProfileMenu(ProfileMenuMessage::NameEdited(
                    value,
                )))
            })
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

/// The dialogs about the tree: a folder's name, its deletion, connecting all it holds, a
/// profile's name.
fn folder_dialog(dialog: &Dialog) -> Element<'_, Message> {
    let buttons = |action: String| {
        row![
            button(text(fl!("ui-dialog-cancel-button")))
                .style(button::secondary)
                .on_press(Message::App(AppMessage::DismissDialog)),
            button(text(action)).on_press(Message::App(AppMessage::ConfirmDialog)),
        ]
        .spacing(SPACING)
    };
    let (title, body, action) = match dialog {
        Dialog::FolderName {
            naming,
            value,
            error,
        } => {
            let (title, action) = match naming {
                FolderNaming::New(_) => (
                    fl!("ui-folder-new-title"),
                    fl!("ui-dialog-new-folder-confirm"),
                ),
                FolderNaming::Rename(_) => (
                    fl!("ui-folder-rename-title"),
                    fl!("ui-dialog-rename-confirm"),
                ),
            };
            let mut content = column![
                text(title).size(HEADING_SIZE),
                text(fl!("ui-folder-name-field")),
                text_input(&fl!("ui-dialog-name-placeholder"), value)
                    .id(name_field_id())
                    .on_input(
                        |value| Message::App(AppMessage::Folder(FolderMessage::NameEdited(value)))
                    )
                    .on_submit(Message::App(AppMessage::ConfirmDialog)),
            ]
            .spacing(SPACING);
            if let Some(error) = error {
                content = content.push(
                    text(match error {
                        FolderError::Collision => fl!("ui-folder-error-collision"),
                        _ => fl!("ui-folder-error-invalid"),
                    })
                    .style(text::danger),
                );
            }
            return content.push(buttons(action)).into();
        }
        Dialog::ConfirmDeleteFolder { name, count, .. } => (
            fl!("ui-folder-delete"),
            fl!(
                "ui-folder-delete-body",
                name = name.as_str(),
                count = (*count)
            ),
            fl!("ui-folder-delete"),
        ),
        Dialog::RenameProfile { value, .. } => return rename_profile_dialog(value),
        Dialog::BulkEdit {
            field,
            ids,
            value,
            mixed,
            refused,
        } => return bulk_edit_dialog(*field, ids.len(), value, *mixed, *refused),
        Dialog::ConfirmDeleteProfiles { ids, names } => (
            fl!("ui-dialog-delete-selection-title"),
            std::iter::once(fl!("ui-dialog-delete-selection-body", count = ids.len()))
                .chain(names.iter().map(|name| format!("- {name}")))
                .collect::<Vec<_>>()
                .join("\n"),
            fl!("ui-dialog-delete-profile-confirm"),
        ),
        Dialog::ConfirmConnectFolder { count, .. } => (
            fl!("ui-folder-connect-all-title"),
            fl!("ui-folder-connect-all-body", count = (*count)),
            fl!("ui-folder-connect-all-confirm"),
        ),
        _ => return column![].into(),
    };
    column![text(title).size(HEADING_SIZE), text(body), buttons(action)]
        .spacing(SPACING)
        .into()
}

/// The dialogs about a tab: closing it or others, naming it, pasting several lines in it.
fn tab_dialog(dialog: &Dialog) -> Element<'_, Message> {
    let (title, body, action) = match dialog {
        Dialog::RenameTab { value, .. } => return rename_tab_dialog(value),
        Dialog::SaveMacro { name, entries } => return save_macro_dialog(name, entries.len()),
        Dialog::CustomResolution { value, .. } => return custom_resolution_dialog(value),
        Dialog::ConfirmPaste {
            command: Some(command),
            ..
        } => (
            fl!("ui-dialog-paste-dangerous-title"),
            fl!(
                "ui-dialog-paste-dangerous-body",
                command = command.to_string()
            ),
            fl!("ui-dialog-paste-dangerous-confirm"),
        ),
        Dialog::ConfirmPaste { lines, .. } => (
            fl!("ui-dialog-paste-title"),
            fl!("ui-dialog-paste-body", count = (*lines)),
            fl!("ui-dialog-paste-confirm"),
        ),
        Dialog::ConfirmDisconnectDesktop { name, .. } => (
            fl!("ui-desktop-disconnect-title"),
            fl!("ui-desktop-disconnect-body", name = name.as_str()),
            fl!("ui-desktop-disconnect"),
        ),
        Dialog::ConfirmCloseTransfers { name, .. } => (
            fl!("ui-dialog-close-transfers-title"),
            fl!("ui-dialog-close-transfers-body", name = name.as_str()),
            fl!("ui-dialog-close-tab-confirm"),
        ),
        Dialog::ConfirmCloseEdits { name, .. } => (
            fl!("ui-dialog-close-tab-title"),
            fl!("ui-dialog-close-edits-body", name = name.as_str()),
            fl!("ui-dialog-close-tab-confirm"),
        ),
        Dialog::ConfirmCloseTabs {
            tabs,
            live,
            unsaved,
        } => (
            fl!("ui-dialog-close-tabs-title"),
            with_unsaved(
                (*live > 0).then(|| {
                    fl!(
                        "ui-dialog-close-tabs-body",
                        count = tabs.len(),
                        live = (*live)
                    )
                }),
                *unsaved,
            ),
            fl!("ui-dialog-close-tab-confirm"),
        ),
        Dialog::ConfirmCloseEditor { name, .. } => (
            fl!("ui-dialog-close-tab-title"),
            fl!("ui-dialog-close-editor-body", name = name.as_str()),
            fl!("ui-dialog-close-tab-confirm"),
        ),
        Dialog::ConfirmDiscardEditor { .. } => (
            fl!("ui-dialog-discard-editor-title"),
            fl!("ui-dialog-discard-editor-body"),
            fl!("ui-editor-close"),
        ),
        Dialog::ConfirmDownloadBinary { name, .. } => (
            fl!("ui-dialog-binary-title"),
            fl!("ui-dialog-binary-body", name = name.as_str()),
            fl!("ui-dialog-binary-confirm"),
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
/// The C# "Custom resolution" of an RDP tab: the size typed as `WIDTHxHEIGHT`.
/// sudo's question: its password, typed hidden, kept for the tab once sudo takes it.
fn sudo_password_dialog<'a>(name: &str, typed: &'a str) -> Element<'a, Message> {
    column![
        text(fl!("ui-dialog-sudo-title")).size(HEADING_SIZE),
        text(fl!("ui-dialog-sudo-body", name = name)),
        text_input("", typed)
            .id(name_field_id())
            .secure(true)
            .on_input(Message::SudoPasswordEdited)
            .on_submit(Message::SudoPasswordConfirm),
        row![
            button(text(fl!("ui-dialog-cancel-button")))
                .style(button::secondary)
                .on_press(Message::App(AppMessage::DismissDialog)),
            button(text(fl!("ui-dialog-ok-button"))).on_press(Message::SudoPasswordConfirm),
        ]
        .spacing(SPACING),
    ]
    .spacing(SPACING)
    .into()
}

fn custom_resolution_dialog(value: &str) -> Element<'_, Message> {
    column![
        text(fl!("ui-resolution-custom-title")).size(HEADING_SIZE),
        text(fl!("ui-resolution-custom-prompt")),
        text_input("", value)
            .id(name_field_id())
            .on_input(|value| {
                Message::App(AppMessage::TabMenu(TabMenuMessage::ResolutionEdited(value)))
            })
            .on_submit(Message::App(AppMessage::ConfirmDialog)),
        row![
            button(text(fl!("ui-dialog-cancel-button")))
                .style(button::secondary)
                .on_press(Message::App(AppMessage::DismissDialog)),
            button(text(fl!("ui-dialog-ok-button")))
                .on_press(Message::App(AppMessage::ConfirmDialog)),
        ]
        .spacing(SPACING),
    ]
    .spacing(SPACING)
    .into()
}

/// The name of the macro just recorded, of `count` inputs, asked before it is kept.
fn save_macro_dialog(value: &str, count: usize) -> Element<'_, Message> {
    column![
        text(fl!("ui-dialog-save-macro-title")).size(HEADING_SIZE),
        text(fl!("ui-dialog-save-macro-prompt", count = count)),
        text(fl!("ui-dialog-save-macro-warning")).size(SMALL_SIZE),
        text_input(&fl!("ui-dialog-name-placeholder"), value)
            .id(name_field_id())
            .on_input(|value| Message::App(AppMessage::Macro(
                heimdall_app::MacroMessage::NameEdited(value)
            )))
            .on_submit(Message::App(AppMessage::ConfirmDialog)),
        row![
            button(text(fl!("ui-dialog-cancel-button")))
                .style(button::secondary)
                .on_press(Message::App(AppMessage::DismissDialog)),
            button(text(fl!("ui-dialog-save-macro-confirm")))
                .on_press(Message::App(AppMessage::ConfirmDialog)),
        ]
        .spacing(SPACING),
    ]
    .spacing(SPACING)
    .into()
}

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

/// The C# question about an imported profile's post-connect commands, with the commands
/// shown: Yes types them and remembers the choice, No opens the shell without them.
fn post_connect_dialog(confirmation: &PostConnectConfirmation) -> Element<'_, Message> {
    let count = confirmation.commands.len();
    column![
        text(fl!("ui-dialog-post-connect-title")).size(HEADING_SIZE),
        text(fl!(
            "ui-dialog-post-connect-body",
            name = confirmation.name.as_str(),
            count = count
        )),
        container(
            scrollable(
                text(confirmation.commands.join("\n"))
                    .font(iced::Font::MONOSPACE)
                    .wrapping(text::Wrapping::Glyph)
            )
            .height(Length::Shrink)
        )
        .max_height(LOCAL_COMMAND_HEIGHT)
        .padding(PADDING)
        .style(container::rounded_box),
        row![
            button(text(fl!("ui-dialog-post-connect-skip")))
                .style(button::secondary)
                .on_press(Message::App(AppMessage::SkipPostConnect)),
            button(text(fl!("ui-dialog-post-connect-run")))
                .style(button::danger)
                .on_press(Message::App(AppMessage::ConfirmDialog)),
        ]
        .spacing(SPACING),
    ]
    .spacing(SPACING)
    .into()
}

/// The count of a tab's running post-connect steps, as the C# tab shows it; its tooltip says
/// which step and what became of it, a click stops the rest.
fn post_connect_badge(tab: TabId, progress: &PostConnectProgress) -> Element<'_, Message> {
    let count = format!("{}/{}", progress.step, progress.total);
    let status = texts::step_status(progress.status);
    tooltip(
        button(text(count.clone()).size(SMALL_SIZE))
            .style(button::text)
            .on_press(Message::App(AppMessage::StopPostConnect(tab))),
        text(fl!(
            "ui-post-connect-tooltip",
            progress = count,
            status = status,
            command = progress.command.as_str()
        ))
        .size(SMALL_SIZE),
        tooltip::Position::Bottom,
    )
    .style(container::rounded_box)
    .into()
}

/// The work of a file edited with sudo: opening it, saving it.
fn sudo_task(effect: Effect) -> Task<Message> {
    match effect {
        Effect::SudoOpen {
            tab,
            shell,
            remote,
            editor,
            folders,
            password,
        } => Task::perform(
            {
                let remote = remote.clone();
                async move {
                    heimdall_app::sudo_edit::start_sudo_edit(
                        shell, remote, editor, folders, password,
                    )
                    .await
                }
            },
            move |result| {
                Message::App(AppMessage::Files(FilesMessage::SudoOpened {
                    tab,
                    remote: remote.clone(),
                    result: result.map(Box::new),
                }))
            },
        ),
        Effect::SudoSave {
            tab,
            shell,
            edit,
            password,
        } => Task::perform(
            async move {
                let check =
                    heimdall_app::sudo_edit::save_with_sudo(&shell, &edit, password.as_ref()).await;
                (edit.local, check)
            },
            move |(local, check)| {
                Message::App(AppMessage::Files(FilesMessage::SudoSaved {
                    tab,
                    local,
                    check,
                }))
            },
        ),
        _ => Task::none(),
    }
}

/// The work of a file edited with the external editor: opening it, starting the editor
/// again, looking at its saves.
fn edit_task(effect: Effect) -> Task<Message> {
    match effect {
        effect @ (Effect::SudoOpen { .. } | Effect::SudoSave { .. }) => sudo_task(effect),
        Effect::SendEditAnyway { tab, client, edit } => Task::perform(
            async move {
                let check = heimdall_app::external_edit::send_anyway(&client, &edit).await;
                (edit.local, check)
            },
            move |(local, check)| {
                Message::App(AppMessage::Files(FilesMessage::EditSentAnyway {
                    tab,
                    local,
                    check,
                }))
            },
        ),
        Effect::OpenFolder { tab, folder } => Task::perform(
            async move {
                tokio::task::spawn_blocking(move || {
                    heimdall_app::external_edit::open_folder(&folder)
                })
                .await
                .map_err(std::io::Error::other)
                .and_then(|opened| opened)
                .map_err(|error| FilesError::EditorFailed {
                    detail: error.to_string(),
                })
            },
            move |result| {
                Message::App(AppMessage::Files(FilesMessage::EditorLaunched {
                    tab,
                    result,
                }))
            },
        ),
        Effect::StartEdit {
            tab,
            client,
            remote,
            editor,
            base,
            keep,
            cancel,
        } => Task::perform(
            heimdall_app::external_edit::start_edit(client, remote, editor, (base, keep), cancel),
            move |result| {
                Message::App(AppMessage::Files(FilesMessage::EditStarted {
                    tab,
                    result: result.map(Box::new),
                }))
            },
        ),
        Effect::LaunchEditor { tab, editor, file } => Task::perform(
            async move {
                tokio::task::spawn_blocking(move || {
                    heimdall_app::external_edit::launch(&editor, &file)
                })
                .await
                .map_err(std::io::Error::other)
                .and_then(|launched| launched)
                .map_err(|error| FilesError::EditorFailed {
                    detail: error.to_string(),
                })
            },
            move |result| {
                Message::App(AppMessage::Files(FilesMessage::EditorLaunched {
                    tab,
                    result,
                }))
            },
        ),
        Effect::CheckEdits {
            tab,
            client,
            shell,
            password,
            edits,
        } => Task::perform(
            async move {
                let mut results = Vec::with_capacity(edits.len());
                for edit in &edits {
                    let sudo = shell.as_ref().map(|shell| (shell, password.as_ref()));
                    let check = heimdall_app::external_edit::check_edit(&client, edit, sudo).await;
                    results.push((edit.local.clone(), check));
                }
                results
            },
            move |results| {
                Message::App(AppMessage::Files(FilesMessage::EditsChecked {
                    tab,
                    results,
                }))
            },
        ),
        _ => Task::none(),
    }
}

/// The work of a Files tab: listing, transferring, changing entries.
#[expect(clippy::too_many_lines, reason = "one arm per effect")]
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
        Effect::PlanTransfer { tab, request } => {
            let planned = (*request).clone();
            Task::perform(plan_transfer(planned), move |result| {
                Message::App(AppMessage::Files(FilesMessage::Planned {
                    tab,
                    request: request.clone(),
                    result: result.map(Box::new),
                }))
            })
        }
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
        Effect::FileBatchStep { tab, operation, .. } => {
            Task::perform(file_operation(*operation), move |result| {
                Message::App(AppMessage::Files(FilesMessage::BatchStepDone {
                    tab,
                    result,
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
        Effect::MoveRemote { tab, client, moves } => {
            Task::perform(move_remote(client, moves), move |results| {
                Message::App(AppMessage::Files(FilesMessage::Moved { tab, results }))
            })
        }
        effect @ (Effect::StartEdit { .. }
        | Effect::LaunchEditor { .. }
        | Effect::CheckEdits { .. }
        | Effect::SudoOpen { .. }
        | Effect::SudoSave { .. }
        | Effect::SendEditAnyway { .. }
        | Effect::OpenFolder { .. }) => edit_task(effect),
        Effect::CopyAcross {
            tab,
            from,
            to,
            sources,
            folder,
            staging,
            cancel,
        } => Task::perform(
            heimdall_app::files::copy_across(from, to, sources, folder, staging, cancel),
            move |results| {
                Message::App(AppMessage::Files(FilesMessage::Copied {
                    tab,
                    results,
                    duplicate: false,
                }))
            },
        ),
        Effect::CopyRemote {
            tab,
            client,
            shell,
            sources,
            folder,
            cancel,
            duplicate,
        } => Task::perform(
            copy_remote(client, Some(shell), sources, folder, cancel),
            move |results| {
                Message::App(AppMessage::Files(FilesMessage::Copied {
                    tab,
                    results,
                    duplicate,
                }))
            },
        ),
        _ => Task::none(),
    }
}

/// Opens the vault away from the window's thread: the key derivation takes a moment.
fn open_vault_task(path: PathBuf, password: Secret, job: VaultJob) -> Task<Message> {
    Task::perform(open_vault(path, password, job), |result| {
        Message::App(AppMessage::VaultOpened(result))
    })
}

/// Whether a desktop is fitted to its tab unless the user chose otherwise: a VNC server
/// keeps its own size; an RDP server is asked for the tab's, unless its profile keeps a
/// size of its own and scales it, or asks the tab's size only once.
fn fits_by_default(profile: &TabProfile) -> bool {
    match profile {
        TabProfile::Vnc(_) => true,
        TabProfile::Rdp(rdp) => rdp.options.scaled(),
        TabProfile::Ssh(_)
        | TabProfile::Telnet(_)
        | TabProfile::Local(_)
        | TabProfile::Ftp(_)
        | TabProfile::WinRm(_) => false,
    }
}

/// A language as the list names it: in its own name, as the C# list does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LanguageChoice(Language);

impl std::fmt::Display for LanguageChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&match self.0 {
            Language::English => fl!("ui-settings-language-en"),
            Language::French => fl!("ui-settings-language-fr"),
            Language::Spanish => fl!("ui-settings-language-es"),
        })
    }
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

/// An environment in the form's list, as the C# names it; "(None)" for none.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct EnvironmentChoice(Option<heimdall_core::metadata::Environment>);

impl fmt::Display for EnvironmentChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&texts::environment_name(self.0))
    }
}

/// The C# Metadata section's fields: the environment, the tags and the MAC address
/// Wake-on-LAN wakes the server with.
fn metadata_fields(draft: &ProfileDraft) -> Element<'_, Message> {
    let choices: Vec<EnvironmentChoice> = std::iter::once(None)
        .chain(heimdall_core::metadata::Environment::ALL.map(Some))
        .map(EnvironmentChoice)
        .collect();
    column![
        row![
            text(fl!("ui-profile-field-environment")),
            iced::widget::space::horizontal(),
            pick_list(
                choices,
                Some(EnvironmentChoice(draft.environment)),
                |EnvironmentChoice(environment)| Message::App(AppMessage::ProfileChoice(
                    ProfileChoice::Environment(environment)
                )),
            ),
        ]
        .spacing(SPACING)
        .align_y(iced::Alignment::Center),
        form_field(draft, ProfileField::Tags),
        form_field(draft, ProfileField::MacAddress),
    ]
    .spacing(SPACING)
    .into()
}

/// A profile's session logging in its form's list, as the C# "Inherit", "On" and "Off".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LoggingChoice(Option<bool>);

impl LoggingChoice {
    /// The choices, in the C# order.
    const ALL: [Self; 3] = [Self(None), Self(Some(true)), Self(Some(false))];
}

impl fmt::Display for LoggingChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&match self.0 {
            None => fl!("ui-profile-session-logging-inherit"),
            Some(true) => fl!("ui-profile-session-logging-on"),
            Some(false) => fl!("ui-profile-session-logging-off"),
        })
    }
}

/// Whether the profile's sessions keep a transcript, as the C# server dialog's choice.
fn session_logging_choice(draft: &ProfileDraft) -> Element<'_, Message> {
    column![
        row![
            text(fl!("ui-profile-session-logging")),
            iced::widget::space::horizontal(),
            pick_list(
                LoggingChoice::ALL.to_vec(),
                Some(LoggingChoice(draft.session_logging)),
                |LoggingChoice(logging)| Message::App(AppMessage::ProfileChoice(
                    ProfileChoice::SessionLogging(logging)
                )),
            ),
        ]
        .spacing(SPACING)
        .align_y(iced::Alignment::Center),
        text(fl!("ui-profile-session-logging-hint")).size(SMALL_SIZE),
    ]
    .spacing(SPACING / 2.0)
    .into()
}

/// An SSH agent preference in the Settings page's list, named as the C# Heimdall names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AgentChoice(AgentPreference);

impl fmt::Display for AgentChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&match self.0 {
            AgentPreference::OpenSshFirst => fl!("ui-settings-ssh-agent-openssh-first"),
            AgentPreference::PageantFirst => fl!("ui-settings-ssh-agent-pageant-first"),
            AgentPreference::OpenSshOnly => fl!("ui-settings-ssh-agent-openssh-only"),
            AgentPreference::PageantOnly => fl!("ui-settings-ssh-agent-pageant-only"),
        })
    }
}

/// A colour scheme in the Settings page's list, named as the C# Heimdall names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SchemeChoice(ColorScheme);

impl fmt::Display for SchemeChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&match self.0 {
            ColorScheme::Standard => fl!("ui-scheme-default"),
            ColorScheme::Dracula => fl!("ui-scheme-dracula"),
            ColorScheme::SolarizedDark => fl!("ui-scheme-solarized-dark"),
            ColorScheme::Monokai => fl!("ui-scheme-monokai"),
            ColorScheme::Nord => fl!("ui-scheme-nord"),
        })
    }
}

impl fmt::Display for KeysChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&match self.0 {
            SpecialKeys::CtrlAltDel => fl!("ui-desktop-keys-ctrl-alt-del"),
            SpecialKeys::Windows => fl!("ui-desktop-keys-windows"),
            SpecialKeys::AltTab => fl!("ui-desktop-keys-alt-tab"),
            SpecialKeys::CtrlEsc => fl!("ui-desktop-keys-ctrl-esc"),
            SpecialKeys::Escape => fl!("ui-desktop-keys-escape"),
            SpecialKeys::PrintScreen => fl!("ui-desktop-keys-print-screen"),
            SpecialKeys::F11 => fl!("ui-desktop-keys-f11"),
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
    // Locked out, no try is taken until the minutes said are over.
    let locked_out = matches!(
        dialog.problem,
        Some(VaultProblem::LockedOut { until }) if until > std::time::SystemTime::now()
    );
    buttons = buttons
        .push(button(text(action)).on_press_maybe(
            (!dialog.busy && ready && !locked_out).then_some(Message::SubmitVault),
        ));
    form.push(buttons).into()
}

/// The PIN's dialogs, as the C# Heimdall's: the PIN asked at start, and the one setting,
/// changing or removing it.
fn pin_dialog<'a>(
    dialog: &'a PinDialog,
    fields: &'a [Zeroizing<String>; 3],
) -> Element<'a, Message> {
    let (title, labels, action) = match dialog.mode {
        PinMode::Start { .. } => (
            fl!("ui-pin-enter-title"),
            vec![fl!("ui-pin-field-pin")],
            fl!("ui-pin-unlock-button"),
        ),
        PinMode::Setup { current } => {
            let mut labels = Vec::new();
            if current {
                labels.push(fl!("ui-pin-field-current"));
            }
            labels.push(fl!("ui-pin-field-new"));
            labels.push(fl!("ui-pin-field-confirm"));
            (fl!("ui-pin-setup-title"), labels, fl!("ui-pin-save-button"))
        }
    };
    let count = labels.len();
    let mut form = column![text(title).size(HEADING_SIZE)].spacing(SPACING);
    for (index, label) in labels.into_iter().enumerate() {
        let input = text_input("", fields[index].as_str())
            .id(vault_field_id(index))
            .secure(true)
            .on_input(move |value| Message::VaultField { index, value })
            .on_submit(if index + 1 < count {
                Message::FocusVaultField(index + 1)
            } else {
                Message::SubmitPin
            });
        form = form.push(column![text(label).size(SMALL_SIZE), input].spacing(SPACING / 2.0));
    }
    if let Some(problem) = &dialog.problem {
        form = form.push(text(pin_problem_text(problem)).style(text::danger));
    }
    // Locked out, no try is taken until the minutes said are over.
    let open = !matches!(
        dialog.problem,
        Some(PinFailure::LockedOut { until }) if until > std::time::SystemTime::now()
    );
    let mut buttons = row![iced::widget::space::horizontal()].spacing(SPACING);
    if dialog.mode == (PinMode::Setup { current: true }) {
        buttons = buttons.push(
            button(text(fl!("ui-pin-remove-button")))
                .style(button::secondary)
                .on_press_maybe(open.then_some(Message::RemovePin)),
        );
    }
    buttons = buttons
        .push(
            button(text(fl!("ui-dialog-cancel-button")))
                .style(button::secondary)
                .on_press(Message::App(AppMessage::DismissDialog)),
        )
        .push(button(text(action)).on_press_maybe(open.then_some(Message::SubmitPin)));
    form.push(buttons).into()
}

fn pin_problem_text(problem: &PinFailure) -> String {
    match problem {
        PinFailure::Wrong { remaining } => fl!("ui-pin-problem-wrong", remaining = remaining),
        PinFailure::LockedOut { until } => fl!(
            "ui-pin-problem-locked-out",
            minutes = heimdall_core::lockout::minutes_left(*until, std::time::SystemTime::now())
        ),
        PinFailure::WrongCurrent => fl!("ui-pin-problem-wrong-current"),
        PinFailure::Refused(PinProblem::TooShort) => {
            fl!("ui-pin-problem-too-short", min = MIN_PIN_DIGITS)
        }
        PinFailure::Refused(PinProblem::TooLong) => {
            fl!("ui-pin-problem-too-long", max = MAX_PIN_DIGITS)
        }
        PinFailure::Refused(PinProblem::NotDigits) => fl!("ui-pin-problem-not-digits"),
        PinFailure::Mismatch => fl!("ui-pin-problem-mismatch"),
        PinFailure::System { detail } => fl!("ui-pin-problem-system", detail = detail.as_str()),
    }
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
        VaultProblem::LockedOut { until } => fl!(
            "ui-vault-problem-locked-out",
            minutes = heimdall_core::lockout::minutes_left(*until, std::time::SystemTime::now())
        ),
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

/// What deleting `count` entries of a Files tab asks, the first named `name`; `folder`
/// when a folder is among them.
fn delete_question(name: &str, folder: bool, count: usize) -> String {
    if count > 1 {
        fl!("ui-dialog-delete-many-body", count = count)
    } else if folder {
        fl!("ui-dialog-delete-folder-body", name = name)
    } else {
        fl!("ui-dialog-delete-file-body", name = name)
    }
}

/// A question's text, then what it would lose of integrated editors' text not saved.
fn with_unsaved(body: Option<String>, unsaved: usize) -> String {
    let lost = (unsaved > 0).then(|| fl!("ui-dialog-unsaved-editors", count = unsaved));
    match (body, lost) {
        (Some(body), Some(lost)) => format!("{body}\n\n{lost}"),
        (Some(text), None) | (None, Some(text)) => text,
        (None, None) => String::new(),
    }
}

/// The title, text and action of a plain question: leaving the window with sessions live,
/// broadcasting input to every tab, recording every session, resetting the RDP settings,
/// deleting profiles or folders.
fn plain_question(dialog: &Dialog) -> (String, String, String) {
    match dialog {
        Dialog::ConfirmDeleteProfile { name, .. } => (
            fl!("ui-dialog-delete-profile-title"),
            fl!("ui-dialog-delete-profile-body", name = name.as_str()),
            fl!("ui-dialog-delete-profile-confirm"),
        ),
        Dialog::ConfirmDelete {
            name,
            folder,
            count,
            ..
        } => (
            fl!("ui-dialog-delete-title"),
            delete_question(name, *folder, *count),
            fl!("ui-dialog-delete-confirm"),
        ),
        Dialog::ConfirmExit { live, unsaved } => (
            fl!("ui-dialog-exit-title"),
            with_unsaved(
                (*live > 0).then(|| fl!("ui-dialog-exit-body", count = (*live))),
                *unsaved,
            ),
            fl!("ui-dialog-exit-confirm"),
        ),
        Dialog::ConfirmSessionLogging => (
            fl!("ui-dialog-session-logging-title"),
            fl!("ui-dialog-session-logging-body"),
            fl!("ui-dialog-session-logging-confirm"),
        ),
        Dialog::ConfirmResetRdpDefaults => (
            fl!("ui-dialog-reset-rdp-title"),
            fl!("ui-dialog-reset-rdp-body"),
            fl!("ui-settings-rdp-reset-defaults"),
        ),
        Dialog::ConfirmDeleteMacro(name) => (
            fl!("ui-macro-editor-delete-macro"),
            fl!("ui-dialog-delete-macro-body", name = server_text(name)),
            fl!("ui-macros-delete"),
        ),
        Dialog::ConfirmDeleteGateway {
            name,
            servers,
            gateways,
            ..
        } => (
            fl!("ui-dialog-delete-gateway-title"),
            fl!(
                "ui-dialog-delete-gateway-body",
                name = server_text(name),
                servers = (*servers),
                gateways = (*gateways)
            ),
            fl!("ui-gateways-delete"),
        ),
        _ => (
            fl!("ui-dialog-broadcast-title"),
            fl!("ui-dialog-broadcast-body"),
            fl!("ui-dialog-broadcast-confirm"),
        ),
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
    match dialog {
        Dialog::SudoPassword { name, .. } => sudo_password_dialog(name, forms.sudo_password),
        Dialog::ConfirmCloseTab(_)
        | Dialog::ConfirmDisconnectDesktop { .. }
        | Dialog::ConfirmCloseTransfers { .. }
        | Dialog::ConfirmCloseEdits { .. }
        | Dialog::ConfirmCloseEditor { .. }
        | Dialog::ConfirmDiscardEditor { .. }
        | Dialog::ConfirmDownloadBinary { .. }
        | Dialog::ConfirmCloseTabs { .. }
        | Dialog::RenameTab { .. }
        | Dialog::SaveMacro { .. }
        | Dialog::CustomResolution { .. }
        | Dialog::ConfirmPaste { .. } => tab_dialog(dialog),
        Dialog::FolderName { .. }
        | Dialog::ConfirmDeleteFolder { .. }
        | Dialog::ConfirmConnectFolder { .. }
        | Dialog::RenameProfile { .. }
        | Dialog::BulkEdit { .. }
        | Dialog::ConfirmDeleteProfiles { .. } => folder_dialog(dialog),
        Dialog::ConfirmBroadcast
        | Dialog::ConfirmExit { .. }
        | Dialog::ConfirmSessionLogging
        | Dialog::ConfirmResetRdpDefaults
        | Dialog::ConfirmDeleteMacro(_)
        | Dialog::ConfirmDeleteGateway { .. }
        | Dialog::ConfirmDeleteProfile { .. }
        | Dialog::ConfirmDelete { .. } => {
            let (title, body, action) = plain_question(dialog);
            question(title, body, action).into()
        }
        Dialog::ConfirmSettingsExportPaths { count } => {
            crate::settings_file::export_question(*count)
        }
        Dialog::ConfirmSettingsImport(read) => crate::settings_file::import_question(read),
        Dialog::FileConflicts { rows, .. } => crate::conflicts_view::view(rows),
        Dialog::EditMacro(edited) => crate::macros_view::editor(edited),
        Dialog::NewTunnel(form) => {
            crate::tunnels_view::new_tunnel(form, forms.gateways, forms.tunnel_problem)
        }
        Dialog::TunnelHostKey {
            host,
            port,
            fingerprint,
        } => crate::tunnels_view::host_key(host, *port, fingerprint),
        Dialog::AskName { action, value, .. } => name_dialog(*action, value),
        Dialog::EditProfile { draft, error } => profile_form(draft, *error, forms),
        Dialog::ConfirmLocalCommand(confirmation) => local_command_dialog(confirmation),
        Dialog::ConfirmPostConnect(confirmation) => post_connect_dialog(confirmation),
        Dialog::ForgetTrustedKey(key) => crate::trusted_keys_view::forget_question(key),
        Dialog::ImportDone(summary) => import_report(summary, ok()),
        Dialog::RestoreSessions(dialog) => crate::restore_view::view(dialog),
        Dialog::Shortcuts => crate::shortcuts_view::view(ok()),
        Dialog::FileProperties(properties) => crate::files_view::properties(properties, ok()),
        Dialog::ExportDone { .. }
        | Dialog::ExportFailed { .. }
        | Dialog::ImportFailed { .. }
        | Dialog::SessionsUnreadable { .. }
        | Dialog::SessionsEmpty { .. }
        | Dialog::SessionsDone { .. }
        | Dialog::RdpNothing { .. }
        | Dialog::RdpDone(_)
        | Dialog::HostKeysUnreadable { .. }
        | Dialog::HostKeysEmpty
        | Dialog::HostKeysDone { .. }
        | Dialog::ImportNothing { .. }
        | Dialog::PasswordSaveFailed { .. }
        | Dialog::StoreError { .. } => report(dialog, ok()),
        Dialog::SessionsPreview(_)
        | Dialog::RdpPreview(_)
        | Dialog::HostKeysPreview(_)
        | Dialog::ConfirmImportFile(_) => import_preview(dialog),
        Dialog::Vault(vault) => vault_dialog(vault, forms.vault),
        Dialog::Pin(pin) => pin_dialog(pin, forms.vault),
        Dialog::EditGateway { draft, error, .. } => gateway_dialog(draft, *error, forms),
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
    fn f1_left_by_every_widget_shows_the_shortcuts_and_a_session_keeps_its_own() {
        let f1 = |modifiers, status| {
            window_event(
                iced::Event::Keyboard(keyboard::Event::KeyPressed {
                    key: Key::Named(Named::F1),
                    modified_key: Key::Named(Named::F1),
                    physical_key: Physical::Code(keyboard::key::Code::F1),
                    location: Location::Standard,
                    modifiers,
                    text: None,
                    repeat: false,
                }),
                status,
                window::Id::unique(),
            )
        };
        assert!(matches!(
            f1(Modifiers::empty(), event::Status::Ignored),
            Some(Message::Shortcut(WindowShortcut::Help))
        ));
        assert!(
            f1(Modifiers::empty(), event::Status::Captured).is_none(),
            "a terminal's program gets it"
        );
        assert!(!matches!(
            f1(Modifiers::SHIFT, event::Status::Ignored),
            Some(Message::Shortcut(WindowShortcut::Help))
        ));
    }

    #[test]
    fn ctrl_comma_left_by_every_widget_is_the_settings_shortcut() {
        let comma = |status| {
            window_event(
                iced::Event::Keyboard(keyboard::Event::KeyPressed {
                    key: Key::Character(",".into()),
                    modified_key: Key::Character(",".into()),
                    physical_key: Physical::Unidentified(NativeCode::Unidentified),
                    location: Location::Standard,
                    modifiers: Modifiers::CTRL,
                    text: None,
                    repeat: false,
                }),
                status,
                window::Id::unique(),
            )
        };
        assert!(matches!(
            comma(event::Status::Ignored),
            Some(Message::Shortcut(WindowShortcut::Settings))
        ));
        assert!(
            comma(event::Status::Captured).is_none(),
            "a field that took it keeps it"
        );
    }

    #[test]
    fn the_schemes_are_listed_by_their_csharp_names() {
        assert_eq!(
            ColorScheme::ALL.map(|scheme| SchemeChoice(scheme).to_string()),
            ["Default", "Dracula", "Solarized Dark", "Monokai", "Nord"]
        );
    }

    #[test]
    fn a_vnc_desktop_is_fitted_and_an_rdp_one_as_its_profile_asks_unless_chosen() {
        use heimdall_core::profile::{ProfileId, RdpOptions, RdpProfile, Resolution, VncProfile};

        let vnc = VncProfile {
            id: ProfileId::new("v"),
            name: "v".to_owned(),
            group: None,
            host: "h".to_owned(),
            port: 5900,
            view_only: false,
            allow_no_password: false,
            vault_entry: None,
        };
        assert!(fits_by_default(&TabProfile::Vnc(vnc)));
        let rdp = |options| {
            TabProfile::Rdp(RdpProfile {
                extras: heimdall_core::profile::RdpExtras::default(),
                id: ProfileId::new("r"),
                name: "r".to_owned(),
                group: None,
                host: "h".to_owned(),
                port: 3389,
                username: None,
                domain: None,
                allow_tls_only: false,
                gateway: None,
                redirect_clipboard: true,
                redirect_drives: false,
                options,
                vault_entry: None,
                forwards: heimdall_core::profile::Forwards::default(),
                follow_defaults: false,
                several_servers: false,
                anti_idle: false,
                auto_reconnect: true,
            })
        };
        let fixed = RdpOptions {
            resolution: Resolution::Fixed,
            ..RdpOptions::default()
        };
        for (options, fitted) in [
            (RdpOptions::default(), false),
            (
                RdpOptions {
                    dynamic_resolution: false,
                    ..RdpOptions::default()
                },
                true,
            ),
            (fixed, true),
            (
                RdpOptions {
                    scale_fixed: false,
                    ..fixed
                },
                false,
            ),
        ] {
            assert_eq!(fits_by_default(&rdp(options)), fitted, "{options:?}");
        }
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
    fn ctrl_e_n_and_k_reach_the_tree_only_when_no_widget_took_them() {
        let ctrl = |letter: &str, modifiers: Modifiers| {
            let key = Key::Character(letter.into());
            iced::Event::Keyboard(keyboard::Event::KeyPressed {
                key: key.clone(),
                modified_key: key,
                physical_key: Physical::Unidentified(NativeCode::Unidentified),
                location: Location::Standard,
                modifiers,
                text: None,
                repeat: false,
            })
        };
        let routed = |letter: &str, modifiers: Modifiers, status: event::Status| {
            window_event(ctrl(letter, modifiers), status, window::Id::unique())
        };
        assert!(matches!(
            routed("e", Modifiers::CTRL, event::Status::Ignored),
            Some(Message::TreeShortcut(TreeShortcut::Edit))
        ));
        assert!(matches!(
            routed("n", Modifiers::CTRL, event::Status::Ignored),
            Some(Message::TreeShortcut(TreeShortcut::New))
        ));
        assert!(matches!(
            routed("k", Modifiers::CTRL, event::Status::Ignored),
            Some(Message::TreeShortcut(TreeShortcut::QuickConnect))
        ));
        assert!(
            routed("e", Modifiers::CTRL, event::Status::Captured).is_none(),
            "a shell's Ctrl+E, end of line, stays its own"
        );
        assert!(
            !matches!(
                routed(
                    "e",
                    Modifiers::CTRL | Modifiers::SHIFT,
                    event::Status::Ignored
                ),
                Some(Message::TreeShortcut(_))
            ),
            "Ctrl alone"
        );
        assert!(!matches!(
            routed("x", Modifiers::CTRL, event::Status::Ignored),
            Some(Message::TreeShortcut(_))
        ));
    }

    #[test]
    fn ctrl_b_the_menu_key_and_letters_reach_the_tree_only_when_no_widget_took_them() {
        let pressed = |key: Key, physical: Physical, modifiers: Modifiers, text: Option<&str>| {
            iced::Event::Keyboard(keyboard::Event::KeyPressed {
                key: key.clone(),
                modified_key: key,
                physical_key: physical,
                location: Location::Standard,
                modifiers,
                text: text.map(Into::into),
                repeat: false,
            })
        };
        let routed = |event, status| window_event(event, status, window::Id::unique());
        let unknown = Physical::Unidentified(NativeCode::Unidentified);
        let ctrl_b = || pressed(Key::Character("b".into()), unknown, Modifiers::CTRL, None);
        assert!(matches!(
            routed(ctrl_b(), event::Status::Ignored),
            Some(Message::TreeShortcut(TreeShortcut::ToggleSidebar))
        ));
        assert!(
            routed(ctrl_b(), event::Status::Captured).is_none(),
            "tmux's prefix in a terminal stays its own"
        );
        let shift_f10 = pressed(
            Key::Named(Named::F10),
            Physical::Code(keyboard::key::Code::F10),
            Modifiers::SHIFT,
            None,
        );
        assert!(matches!(
            routed(shift_f10, event::Status::Ignored),
            Some(Message::MenuKey)
        ));
        let menu = pressed(
            Key::Named(Named::ContextMenu),
            unknown,
            Modifiers::empty(),
            None,
        );
        assert!(matches!(
            routed(menu, event::Status::Ignored),
            Some(Message::MenuKey)
        ));
        let letter = |modifiers| pressed(Key::Character("w".into()), unknown, modifiers, Some("w"));
        assert!(matches!(
            routed(letter(Modifiers::empty()), event::Status::Ignored),
            Some(Message::TypeAhead(typed)) if typed == "w"
        ));
        assert!(routed(letter(Modifiers::empty()), event::Status::Captured).is_none());
        assert!(!matches!(
            routed(letter(Modifiers::ALT), event::Status::Ignored),
            Some(Message::TypeAhead(_))
        ));
    }

    #[test]
    fn files_dragged_from_explorer_reach_the_window() {
        let path = std::path::PathBuf::from("report.pdf");
        let routed = |event| {
            window_event(
                iced::Event::Window(event),
                event::Status::Ignored,
                window::Id::unique(),
            )
        };
        assert!(matches!(
            routed(window::Event::FileHovered(path.clone())),
            Some(Message::FilesHovered(true))
        ));
        assert!(matches!(
            routed(window::Event::FilesHoveredLeft),
            Some(Message::FilesHovered(false))
        ));
        assert!(matches!(
            routed(window::Event::FileDropped(path.clone())),
            Some(Message::FileDropped(dropped)) if dropped == path
        ));
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
        assert!(
            matches!(
                message(Named::Escape, Modifiers::empty(), event::Status::Ignored),
                Some(Message::EscapeUntaken)
            ),
            "taken by none, Escape may leave full screen"
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
